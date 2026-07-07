//! Read/write Claude Code `settings.json` across its three file tiers and
//! compute what is set where.
//!
//! Claude Code spreads configuration across three files, high precedence first:
//!   1. `<project>/.claude/settings.local.json`  (local — gitignored, per-machine)
//!   2. `<project>/.claude/settings.json`        (project — shared, committed)
//!   3. `~/.claude/settings.json`                (user — global)
//!
//! The founder's real pain is that you can't tell *which* file a given key comes
//! from. [`read_claude_settings`] answers exactly that: it returns each tier's
//! file (path/exists/raw/parsed), a top-level merged `effective` object, and a
//! `conflicts` list naming every key set with differing values in more than one
//! tier (with the winning tier). [`write_claude_settings`] writes exactly one
//! tier, never merging.
//!
//! The merge here is deliberately **top-level only** — Claude Code itself does a
//! richer per-object merge (env deep-merges, permission arrays concatenate), but
//! for the "what's set where" story a top-level key comparison is the honest,
//! legible signal: it shows the user the key and the two files it lives in.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Value};

/// The three settings tiers, ordered high precedence → low (local wins).
const TIER_ORDER: &[&str] = &["local", "project", "user"];

/// One settings file tier: its resolved path, whether it exists, its raw text,
/// and its parsed JSON object (or a parse error).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsTierData {
    /// "user" | "project" | "local".
    pub tier: String,
    /// Absolute path to the tier's settings file.
    pub path: String,
    /// Whether that file currently exists on disk.
    pub exists: bool,
    /// Raw file text ("" when absent).
    pub raw: String,
    /// Parsed top-level object, or null when absent / not an object.
    pub parsed: Option<Value>,
    /// A human-readable parse error when the file exists but isn't valid JSON.
    pub parse_error: Option<String>,
}

/// One tier's value for a conflicting key.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsConflictValue {
    pub tier: String,
    pub value: Value,
}

/// A key set with differing values in ≥2 tiers — the headline "what's set where".
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsConflict {
    /// The top-level settings key (e.g. "model").
    pub key: String,
    /// Each tier that sets the key + its value, in precedence order (winner first).
    pub tier_values: Vec<SettingsConflictValue>,
    /// The tier whose value actually takes effect (highest precedence present).
    pub winner: String,
}

/// The full picture returned to the UI: every tier, the merged effective object,
/// and the conflict list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeSettings {
    /// Tiers in precedence order (local, project, user). When no project cwd is
    /// given, only the user tier is present.
    pub tiers: Vec<SettingsTierData>,
    /// Top-level merged view: for each key, the value from the highest-precedence
    /// tier that sets it.
    pub effective: Value,
    /// Keys set with differing values in more than one tier.
    pub conflicts: Vec<SettingsConflict>,
    /// The project cwd this reading was scoped to (null for user/global only).
    pub project_cwd: Option<String>,
}

/// The user (global) `.claude` directory, honouring `CLAUDE_CONFIG_DIR` exactly
/// like the projects-dir resolver does elsewhere in the app.
fn user_claude_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        let p = PathBuf::from(dir);
        if !p.as_os_str().is_empty() {
            return Some(p);
        }
    }
    dirs::home_dir().map(|h| h.join(".claude"))
}

/// Resolve the settings file path for a tier. `user_dir` is the resolved global
/// `.claude` directory; project/local tiers require a cwd.
fn tier_path(user_dir: &Path, tier: &str, project_cwd: Option<&str>) -> Result<PathBuf, String> {
    match tier {
        "user" => Ok(user_dir.join("settings.json")),
        "project" => {
            let cwd = project_cwd.ok_or("A project is required for project settings")?;
            Ok(Path::new(cwd).join(".claude").join("settings.json"))
        }
        "local" => {
            let cwd = project_cwd.ok_or("A project is required for local settings")?;
            Ok(Path::new(cwd).join(".claude").join("settings.local.json"))
        }
        other => Err(format!("Unknown settings tier: {other}")),
    }
}

/// Read and parse one tier's file into a [`SettingsTierData`].
fn read_tier(tier: &str, path: &Path) -> SettingsTierData {
    let exists = path.is_file();
    let mut raw = String::new();
    let mut parsed: Option<Value> = None;
    let mut parse_error: Option<String> = None;

    if exists {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                raw = text;
                // An empty file is a valid "no settings" state, not an error.
                if raw.trim().is_empty() {
                    parsed = Some(Value::Object(Map::new()));
                } else {
                    match serde_json::from_str::<Value>(&raw) {
                        Ok(v @ Value::Object(_)) => parsed = Some(v),
                        Ok(_) => {
                            parse_error = Some("Settings file is not a JSON object".to_string())
                        }
                        Err(e) => parse_error = Some(e.to_string()),
                    }
                }
            }
            Err(e) => parse_error = Some(e.to_string()),
        }
    }

    SettingsTierData {
        tier: tier.to_string(),
        path: path.to_string_lossy().into_owned(),
        exists,
        raw,
        parsed,
        parse_error,
    }
}

/// Which tiers apply for the given scope: user-only when no cwd, otherwise all
/// three in precedence order.
fn applicable_tiers(project_cwd: Option<&str>) -> &'static [&'static str] {
    if project_cwd.is_some() {
        TIER_ORDER // local, project, user
    } else {
        &["user"]
    }
}

/// Build the merged `effective` object and the `conflicts` list from the tiers.
/// `tiers` must be in precedence order (highest first). Top-level keys only.
fn merge_and_conflicts(tiers: &[SettingsTierData]) -> (Value, Vec<SettingsConflict>) {
    let mut effective = Map::new();
    let mut conflicts: Vec<SettingsConflict> = Vec::new();

    // Collect the set of keys across every parsed tier, first-seen order by
    // precedence (so the conflict list reads winner-first, stable).
    let mut keys: Vec<String> = Vec::new();
    for t in tiers {
        if let Some(Value::Object(obj)) = &t.parsed {
            for k in obj.keys() {
                if !keys.contains(k) {
                    keys.push(k.clone());
                }
            }
        }
    }

    for key in keys {
        // Every tier (precedence order) that sets this key.
        let mut present: Vec<(&str, &Value)> = Vec::new();
        for t in tiers {
            if let Some(Value::Object(obj)) = &t.parsed {
                if let Some(v) = obj.get(&key) {
                    present.push((t.tier.as_str(), v));
                }
            }
        }
        let Some((winner_tier, winner_val)) = present.first().copied() else {
            continue;
        };
        // Effective value = highest-precedence tier that sets it.
        effective.insert(key.clone(), winner_val.clone());

        // A conflict is ≥2 tiers with *differing* values for the same key.
        let distinct_values = present.iter().any(|(_, v)| *v != winner_val);
        if present.len() >= 2 && distinct_values {
            conflicts.push(SettingsConflict {
                key: key.clone(),
                tier_values: present
                    .iter()
                    .map(|(t, v)| SettingsConflictValue {
                        tier: t.to_string(),
                        value: (*v).clone(),
                    })
                    .collect(),
                winner: winner_tier.to_string(),
            });
        }
    }

    (Value::Object(effective), conflicts)
}

/// Core reader, parameterized on the user dir so it's testable without touching
/// the global `CLAUDE_CONFIG_DIR`/`HOME` env.
fn read_settings_at(user_dir: &Path, project_cwd: Option<&str>) -> Result<ClaudeSettings, String> {
    let mut tiers: Vec<SettingsTierData> = Vec::new();
    for tier in applicable_tiers(project_cwd) {
        let path = tier_path(user_dir, tier, project_cwd)?;
        tiers.push(read_tier(tier, &path));
    }
    let (effective, conflicts) = merge_and_conflicts(&tiers);
    Ok(ClaudeSettings {
        tiers,
        effective,
        conflicts,
        project_cwd: project_cwd.map(|s| s.to_string()),
    })
}

/// Read Claude Code settings across all applicable tiers for an optional project.
///
/// With no `project_cwd`, only the user/global tier is read. With one, all three
/// tiers (local, project, user) are read and merged, and conflicts computed.
#[tauri::command]
pub fn read_claude_settings(project_cwd: Option<String>) -> Result<ClaudeSettings, String> {
    let user_dir = user_claude_dir().ok_or("Cannot determine home directory")?;
    read_settings_at(&user_dir, project_cwd.as_deref())
}

/// Read a tier file's current raw text for the optimistic-concurrency check,
/// treating a missing file as `""` — the same convention [`read_tier`] uses
/// for `raw` when `exists` is false, so a base version captured from a prior
/// [`read_claude_settings`] call (whose tier didn't exist yet) compares equal
/// to a still-missing file.
fn current_raw(path: &Path) -> Result<String, String> {
    if path.is_file() {
        std::fs::read_to_string(path).map_err(|e| e.to_string())
    } else {
        Ok(String::new())
    }
}

/// Core writer, parameterized on the user dir so it's testable without touching
/// the global `CLAUDE_CONFIG_DIR`/`HOME` env (mirrors [`read_settings_at`]).
///
/// **Precondition (optimistic read-modify-write guard):** `base_version` must
/// be the exact `raw` text the caller last read for this tier via
/// [`read_settings_at`] / [`read_claude_settings`] (`""` if the tier didn't
/// exist yet). Before writing, this re-reads the file on disk; if its current
/// raw text no longer equals `base_version` — e.g. the `claude` CLI itself
/// wrote this file concurrently — the write is refused with a
/// `"CONFLICT: ..."`-prefixed error instead of silently clobbering the
/// external change. The frontend should surface that error and prompt a
/// reload rather than retry the write as-is.
fn write_settings_at(
    user_dir: &Path,
    tier: &str,
    project_cwd: Option<&str>,
    value: &Value,
    base_version: &str,
) -> Result<(), String> {
    if !value.is_object() {
        return Err("Settings must be a JSON object".to_string());
    }
    let path = tier_path(user_dir, tier, project_cwd)?;

    if current_raw(&path)? != base_version {
        return Err(
            "CONFLICT: settings changed on disk — reload before saving".to_string(),
        );
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut pretty = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    pretty.push('\n'); // trailing newline, like an editor would leave
    std::fs::write(&path, pretty).map_err(|e| e.to_string())
}

/// Write exactly one tier's settings file, pretty-printed. Never merges — the
/// caller edited this tier and this tier alone. Creates the `.claude/` directory
/// if it doesn't exist yet. See [`write_settings_at`] for the read-modify-write
/// guard this now enforces.
#[tauri::command]
pub fn write_claude_settings(
    tier: String,
    project_cwd: Option<String>,
    value: Value,
    base_version: String,
) -> Result<(), String> {
    let user_dir = user_claude_dir().ok_or("Cannot determine home directory")?;
    write_settings_at(&user_dir, &tier, project_cwd.as_deref(), &value, &base_version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Lay down user + project + local fixtures under a fresh temp dir and read
    /// them back scoped to that project. No global env is touched — the user dir
    /// is passed explicitly, so tests are safe to run in parallel.
    fn fixture() -> (PathBuf, ClaudeSettings) {
        let base = std::env::temp_dir().join(format!(
            "ccstudio-settings-test-{}",
            uuid::Uuid::new_v4()
        ));
        let user_dir = base.join("home").join(".claude");
        let proj = base.join("proj");
        std::fs::create_dir_all(&user_dir).unwrap();
        std::fs::create_dir_all(proj.join(".claude")).unwrap();

        // user: model=opus, theme=dark
        std::fs::write(user_dir.join("settings.json"), r#"{"model":"opus","theme":"dark"}"#).unwrap();
        // project: model=sonnet (conflicts with user), outputStyle=Explanatory
        std::fs::write(
            proj.join(".claude").join("settings.json"),
            r#"{"model":"sonnet","outputStyle":"Explanatory"}"#,
        )
        .unwrap();
        // local: model=sonnet (same as project — NOT a conflict vs project),
        // includeCoAuthoredBy=false
        std::fs::write(
            proj.join(".claude").join("settings.local.json"),
            r#"{"model":"sonnet","includeCoAuthoredBy":false}"#,
        )
        .unwrap();

        let settings =
            read_settings_at(&user_dir, Some(proj.to_string_lossy().as_ref())).unwrap();
        (base, settings)
    }

    #[test]
    fn reads_all_three_tiers_in_precedence_order() {
        let (base, s) = fixture();
        assert_eq!(s.tiers.len(), 3);
        assert_eq!(s.tiers[0].tier, "local");
        assert_eq!(s.tiers[1].tier, "project");
        assert_eq!(s.tiers[2].tier, "user");
        assert!(s.tiers.iter().all(|t| t.exists));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn effective_value_is_highest_precedence() {
        let (base, s) = fixture();
        let eff = s.effective.as_object().unwrap();
        // local/project both say sonnet → sonnet effective (local wins).
        assert_eq!(eff.get("model").unwrap(), &json!("sonnet"));
        // only user sets theme.
        assert_eq!(eff.get("theme").unwrap(), &json!("dark"));
        // only project sets outputStyle.
        assert_eq!(eff.get("outputStyle").unwrap(), &json!("Explanatory"));
        // only local sets includeCoAuthoredBy.
        assert_eq!(eff.get("includeCoAuthoredBy").unwrap(), &json!(false));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn conflict_only_when_values_differ() {
        let (base, s) = fixture();
        // `model` differs between user(opus) and project/local(sonnet) → 1 conflict.
        // project and local agree on sonnet, so that pair alone is not a conflict.
        let model_conflicts: Vec<_> =
            s.conflicts.iter().filter(|c| c.key == "model").collect();
        assert_eq!(model_conflicts.len(), 1, "conflicts: {:#?}", s.conflicts);
        let c = model_conflicts[0];
        // Winner is the highest-precedence tier that sets it — local.
        assert_eq!(c.winner, "local");
        // All three tiers set `model`, listed precedence-first.
        assert_eq!(c.tier_values.len(), 3);
        assert_eq!(c.tier_values[0].tier, "local");
        assert_eq!(c.tier_values[2].tier, "user");
        // theme/outputStyle/includeCoAuthoredBy are single-tier → no conflict.
        assert!(!s.conflicts.iter().any(|c| c.key != "model"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn user_only_scope_reads_just_the_user_tier() {
        let base = std::env::temp_dir().join(format!(
            "ccstudio-settings-useronly-{}",
            uuid::Uuid::new_v4()
        ));
        let user_dir = base.join(".claude");
        std::fs::create_dir_all(&user_dir).unwrap();
        std::fs::write(user_dir.join("settings.json"), r#"{"model":"opus"}"#).unwrap();
        let s = read_settings_at(&user_dir, None).unwrap();
        assert_eq!(s.tiers.len(), 1);
        assert_eq!(s.tiers[0].tier, "user");
        assert!(s.conflicts.is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn write_creates_dir_and_pretty_prints_one_tier() {
        let base = std::env::temp_dir().join(format!(
            "ccstudio-settings-write-{}",
            uuid::Uuid::new_v4()
        ));
        let proj = base.join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        // .claude/ does not exist yet — write must create it.
        let user_dir = base.join(".claude");
        let path = tier_path(&user_dir, "project", Some(proj.to_string_lossy().as_ref())).unwrap();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let value = json!({"model": "haiku", "env": {"FOO": "bar"}});
        let mut pretty = serde_json::to_string_pretty(&value).unwrap();
        pretty.push('\n');
        std::fs::write(&path, pretty).unwrap();

        let written =
            std::fs::read_to_string(proj.join(".claude").join("settings.json")).unwrap();
        assert!(written.contains("\"model\": \"haiku\"")); // pretty-printed (space after colon)
        assert!(written.ends_with('\n'));
        let reparsed: Value = serde_json::from_str(&written).unwrap();
        assert_eq!(reparsed["env"]["FOO"], json!("bar"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn invalid_json_tier_surfaces_a_parse_error_without_failing() {
        let base = std::env::temp_dir().join(format!(
            "ccstudio-settings-bad-{}",
            uuid::Uuid::new_v4()
        ));
        let user_dir = base.join(".claude");
        std::fs::create_dir_all(&user_dir).unwrap();
        std::fs::write(user_dir.join("settings.json"), "{not valid json").unwrap();
        let s = read_settings_at(&user_dir, None).unwrap();
        assert!(s.tiers[0].exists);
        assert!(s.tiers[0].parsed.is_none());
        assert!(s.tiers[0].parse_error.is_some());
        let _ = std::fs::remove_dir_all(&base);
    }

    // ── write_settings_at: optimistic read-modify-write guard ──────────────

    #[test]
    fn write_succeeds_when_base_version_matches_current_disk_content() {
        let base = std::env::temp_dir().join(format!(
            "ccstudio-settings-rmw-ok-{}",
            uuid::Uuid::new_v4()
        ));
        let user_dir = base.join(".claude");
        std::fs::create_dir_all(&user_dir).unwrap();
        let settings_path = user_dir.join("settings.json");
        std::fs::write(&settings_path, r#"{"model":"opus"}"#).unwrap();

        // Simulates the frontend's last read: capture the exact raw text on disk.
        let base_version = std::fs::read_to_string(&settings_path).unwrap();

        let result = write_settings_at(
            &user_dir,
            "user",
            None,
            &json!({"model": "sonnet"}),
            &base_version,
        );
        assert!(result.is_ok(), "{result:?}");
        let written = std::fs::read_to_string(&settings_path).unwrap();
        assert!(written.contains("\"model\": \"sonnet\""));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn write_returns_conflict_instead_of_clobbering_an_external_change() {
        let base = std::env::temp_dir().join(format!(
            "ccstudio-settings-rmw-conflict-{}",
            uuid::Uuid::new_v4()
        ));
        let user_dir = base.join(".claude");
        std::fs::create_dir_all(&user_dir).unwrap();
        let settings_path = user_dir.join("settings.json");
        std::fs::write(&settings_path, r#"{"model":"opus"}"#).unwrap();

        // The frontend read this stale content...
        let stale_base_version = std::fs::read_to_string(&settings_path).unwrap();

        // ...then something else (e.g. the real `claude` CLI) wrote the file
        // concurrently, before our write lands.
        std::fs::write(&settings_path, r#"{"model":"opus","theme":"dark"}"#).unwrap();

        let result = write_settings_at(
            &user_dir,
            "user",
            None,
            &json!({"model": "sonnet"}),
            &stale_base_version,
        );
        let err = result.expect_err("a concurrent external change must be detected");
        assert!(err.starts_with("CONFLICT"), "unexpected error: {err}");

        // The external writer's content must survive untouched — no clobber.
        let on_disk = std::fs::read_to_string(&settings_path).unwrap();
        assert_eq!(on_disk, r#"{"model":"opus","theme":"dark"}"#);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn write_succeeds_for_a_brand_new_tier_with_empty_base_version() {
        let base = std::env::temp_dir().join(format!(
            "ccstudio-settings-rmw-new-{}",
            uuid::Uuid::new_v4()
        ));
        let proj = base.join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        let user_dir = base.join(".claude"); // does not exist yet

        // No file has ever existed for this tier — read_tier's `raw` for a
        // missing file is "", so that's the base version a first-ever save
        // would carry.
        let result = write_settings_at(
            &user_dir,
            "project",
            Some(proj.to_string_lossy().as_ref()),
            &json!({"model": "haiku"}),
            "",
        );
        assert!(result.is_ok(), "{result:?}");
        let written =
            std::fs::read_to_string(proj.join(".claude").join("settings.json")).unwrap();
        assert!(written.contains("\"model\": \"haiku\""));
        let _ = std::fs::remove_dir_all(&base);
    }
}
