use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::Manager;

// Search: SQLite-backed extracted-text cache (built up over milestones 1–12).
mod search;

// CC Deck's own preference store (terminal launcher choice + resume-launch command).
mod appconfig;

// ccdeck's own data root (~/.ccdeck) + the startup migration that moves
// legacy ccdeck-owned state (backups, config) out of ~/.claude.
mod datadir;

// Prompt Library (issue #24): snippet store + hybrid match engine + commands.
mod prompts;

// Claude Code's own settings.json (schema-driven read/merge/conflict/write across tiers).
mod settings;

// Named provider profiles (issue #21): keychain-backed API keys + ANTHROPIC_*
// env injection for resuming against an alternate Anthropic-compatible provider.
mod providers;

// ---------------------------------------------------------------------------
// Return-type structs (must match the JS contract in ARCHITECTURE.md)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct SessionMeta {
    pub id: String,              // stable id = relative path from projects dir
    pub path: String,            // absolute path to the .jsonl
    pub project_raw: String,     // the encoded project dir name
    pub mtime: u64,              // unix seconds
    pub size: u64,               // bytes
    pub preview: Vec<String>,    // first ≤50 lines of the file
    // Cheap stats computed in one pass over the file content
    pub line_count: u64,         // non-empty lines
    pub user_count: u64,         // lines whose type == "user"
    pub assistant_count: u64,    // lines whose type == "assistant"
    pub subagent_count: u64,     // count of subagents/agent-*.jsonl next to the session file
    pub models: Vec<String>,     // distinct message.model values, first-seen order
    pub first_ts: String,        // first timestamp value seen ("" if none)
    pub last_ts: String,         // last timestamp value seen ("" if none)
    pub cwd: String,             // first-seen "cwd" value ("" if none) — the real project path
    pub custom_title: String,    // last-seen "customTitle" value ("" if none) — a real Claude Code
                                  // rename (ours or the CLI's own /rename), wherever it occurs in
                                  // the file. Last-wins so a later rename supersedes an earlier one.
}

#[derive(Serialize, Clone)]
pub struct BackupVersion {
    pub version: u32,
    pub timestamp: u64,
    pub path: String,
    pub size: u64,
}

#[derive(Serialize)]
pub struct ForkResult {
    pub path: String, // absolute path of the new forked session file
    pub id: String,   // new session uuid (== file stem)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Resolve the Claude projects directory without going through `#[tauri::command]`.
fn projects_dir_inner() -> Option<PathBuf> {
    // 1. Honour CLAUDE_CONFIG_DIR if set
    if let Ok(config_dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        let p = PathBuf::from(config_dir).join("projects");
        if p.is_dir() {
            return Some(p);
        }
    }
    // 2. Fall back to ~/.claude/projects
    let home = dirs::home_dir()?;
    let p = home.join(".claude").join("projects");
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
}

fn unix_secs(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

/// Replace path separators and any non-alphanumeric character with '_'.
fn sanitize_id(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

/// The top-level fields the per-line scan (`scan_session_lines`) needs from a
/// session JSONL line. Parsed once via `serde_json` per line — this replaces
/// the old hand-rolled `json_str_after` substring scanner, which (a) only
/// decoded `\" \\ \n \r \t` and mangled every other JSON escape (`\uXXXX`,
/// `\/`, `\b`, `\f`), and (b) matched its key tokens anywhere in the line,
/// including inside nested string content, so a message whose text happened
/// to contain `"type":"..."` could be misclassified. `#[serde(default)]`
/// means any field simply absent on a given line (most lines don't carry all
/// of these) deserializes to its default instead of erroring.
#[derive(Deserialize, Default)]
struct SessionLineFields {
    #[serde(default, rename = "type")]
    line_type: String,
    #[serde(default)]
    timestamp: String,
    #[serde(default)]
    cwd: String,
    #[serde(default, rename = "customTitle")]
    custom_title: String,
    #[serde(default)]
    message: SessionLineMessage,
}

/// `model` lives nested under `message.model` on assistant lines (confirmed
/// against real `~/.claude/projects/**/*.jsonl` session files) — never at the
/// line's top level.
#[derive(Deserialize, Default)]
struct SessionLineMessage {
    #[serde(default)]
    model: String,
}

/// Cheap one-pass stats `list_sessions` needs per session file, computed by
/// `scan_session_lines`. Split out from the `list_sessions` command so the
/// scan itself — the part with real parsing logic — is testable as a pure
/// function of file content, without touching the filesystem.
#[derive(Debug, Default, PartialEq)]
struct SessionScanStats {
    line_count: u64,
    user_count: u64,
    assistant_count: u64,
    models: Vec<String>,
    first_ts: String,
    last_ts: String,
    cwd: String,
    custom_title: String,
}

/// Scan every non-empty line of a session file's content once, parsing each
/// line as JSON to extract `type`, `timestamp`, `cwd`, `customTitle`, and
/// `message.model`. A line that fails to parse as JSON is silently skipped
/// (still counted in `line_count`, matching the old scanner's
/// silent-skip-on-no-match behavior) rather than failing the whole scan.
fn scan_session_lines(content: &str) -> SessionScanStats {
    let mut stats = SessionScanStats::default();

    for line in content.lines() {
        if line.is_empty() {
            continue;
        }
        stats.line_count += 1;

        let Ok(fields) = serde_json::from_str::<SessionLineFields>(line) else {
            continue;
        };

        match fields.line_type.as_str() {
            "user" => stats.user_count += 1,
            "assistant" => stats.assistant_count += 1,
            _ => {}
        }

        if !fields.message.model.is_empty() && !stats.models.contains(&fields.message.model) {
            stats.models.push(fields.message.model);
        }

        if !fields.timestamp.is_empty() {
            if stats.first_ts.is_empty() {
                stats.first_ts = fields.timestamp.clone();
            }
            stats.last_ts = fields.timestamp;
        }

        // The encoded project dir name is lossy ('/' -> '-'), so prefer the
        // real cwd recorded on the JSONL lines themselves (first-seen).
        if stats.cwd.is_empty() && !fields.cwd.is_empty() {
            stats.cwd = fields.cwd;
        }

        // Renames can land anywhere in the file (Claude Code's own /rename
        // appends near wherever the conversation currently is), so this must
        // scan the whole file, not just the 50-line preview — and last-wins,
        // since a later rename supersedes an earlier one.
        if !fields.custom_title.is_empty() {
            stats.custom_title = fields.custom_title;
        }
    }

    stats
}

/// How long a zero-turn/untitled session file is left alone before it's eligible
/// for auto-cleanup. A freshly-created session that a *live* Claude Code process
/// just opened can legitimately have no logged turns for a short while — this
/// window keeps us from deleting one out from under a running CLI (the only
/// irreversible action in the browse scan). 15 minutes, erring toward keeping.
const CLEANUP_RECENCY_WINDOW_SECS: u64 = 15 * 60;

/// Whether a session file is eligible for auto-cleanup. Three conditions must
/// ALL hold: it has zero conversational turns (no `user` and no `assistant`
/// lines), it was never given a custom title (a manual /rename is a strong
/// signal the user cares about it), AND it is *stale* — untouched for at least
/// `recency_window_secs`. The staleness guard is the safety rail: only truly
/// empty, untitled, and cold files are ever removed; anything with real content
/// or a recent write is always spared. Pure function of its inputs so the guard
/// is unit-testable without touching the filesystem.
fn is_cleanup_eligible(
    stats: &SessionScanStats,
    mtime_secs: u64,
    now_secs: u64,
    recency_window_secs: u64,
) -> bool {
    stats.user_count == 0
        && stats.assistant_count == 0
        && stats.custom_title.is_empty()
        // saturating_sub so a future-dated mtime (clock skew) reads as age 0,
        // i.e. NOT stale — again erring toward keeping the file.
        && now_secs.saturating_sub(mtime_secs) >= recency_window_secs
}

/// Find `key_token` (e.g. `"\"sessionId\":\""`) in `line` and replace the quoted
/// string value that follows it with `new_value`, leaving the rest of the line
/// byte-identical. Assumes the value itself never contains an escaped quote —
/// true for the UUID-shaped values this is used for. No-op if not found.
fn json_replace_str_value(line: &str, key_token: &str, new_value: &str) -> String {
    if let Some(start) = line.find(key_token) {
        let value_start = start + key_token.len();
        if let Some(rel_end) = line[value_start..].find('"') {
            let value_end = value_start + rel_end;
            let mut result = String::with_capacity(line.len());
            result.push_str(&line[..value_start]);
            result.push_str(new_value);
            result.push_str(&line[value_end..]);
            return result;
        }
    }
    line.to_string()
}

/// Single-quote `s` for embedding in a POSIX shell script. Shared by every
/// resume-script call site (macOS, Linux, and the env-var exports both share)
/// — see [`appconfig::build_resume_script`].
pub(crate) fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Write `content` to a fresh temp file named `ccstudio-resume-<session_id>.<ext>`,
/// marking it executable on Unix (`0o755`) — mirrors the temp-script convention
/// the macOS resume path originally used alone, now shared across every OS's
/// resume script.
fn write_resume_script(session_id: &str, ext: &str, content: &str) -> Result<PathBuf, String> {
    let tmp = std::env::temp_dir().join(format!("ccstudio-resume-{session_id}.{ext}"));
    fs::write(&tmp, content).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&tmp).map_err(|e| e.to_string())?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&tmp, perms).map_err(|e| e.to_string())?;
    }
    Ok(tmp)
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Return absolute path of the Claude projects directory, or null if missing.
#[tauri::command]
fn find_projects_dir() -> Option<String> {
    projects_dir_inner().map(|p| p.to_string_lossy().into_owned())
}

/// Return the user's home directory, used by the frontend to render absolute
/// paths (e.g. a session's cwd) home-relative as "~/...".
#[tauri::command]
fn home_dir() -> Option<String> {
    dirs::home_dir().map(|p| p.to_string_lossy().into_owned())
}

/// Walk every immediate sub-directory of the projects dir.  For each *.jsonl
/// that is NOT named agent-*.jsonl, emit one SessionMeta.
/// Skips dirs named "subagents" and "tool-results".
#[tauri::command]
fn list_sessions() -> Result<Vec<SessionMeta>, String> {
    let projects = projects_dir_inner()
        .ok_or_else(|| "Projects directory not found".to_string())?;

    let mut sessions: Vec<SessionMeta> = Vec::new();

    let top_entries = fs::read_dir(&projects).map_err(|e| e.to_string())?;
    for top in top_entries.flatten() {
        let project_path = top.path();
        if !project_path.is_dir() {
            continue;
        }
        let dir_name = match project_path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if dir_name == "subagents" || dir_name == "tool-results" {
            continue;
        }

        let inner = match fs::read_dir(&project_path) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for jentry in inner.flatten() {
            let file_path = jentry.path();
            let fname = match file_path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            if !fname.ends_with(".jsonl") {
                continue;
            }
            if fname.starts_with("agent-") {
                continue;
            }

            let meta = match fs::metadata(&file_path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let mtime = meta.modified().map(unix_secs).unwrap_or(0);
            let size = meta.len();

            let content = fs::read_to_string(&file_path).unwrap_or_default();
            let preview: Vec<String> = content.lines().take(50).map(|l| l.to_string()).collect();

            // --- Cheap one-pass stats ---
            let SessionScanStats {
                line_count,
                user_count,
                assistant_count,
                models,
                first_ts,
                last_ts,
                cwd,
                custom_title,
            } = scan_session_lines(&content);

            // Count subagent *.jsonl files (not .meta.json) in the sibling subagents/ dir.
            let subagent_count: u64 = {
                let parent = file_path.parent().unwrap_or(Path::new(""));
                let subagents_dir = parent.join("subagents");
                if subagents_dir.is_dir() {
                    fs::read_dir(&subagents_dir)
                        .map(|entries| {
                            entries
                                .flatten()
                                .filter(|e| {
                                    let fname = e.file_name();
                                    let s = fname.to_string_lossy();
                                    s.starts_with("agent-") && s.ends_with(".jsonl")
                                })
                                .count() as u64
                        })
                        .unwrap_or(0)
                } else {
                    0
                }
            };

            // Relative path from projects root — this is the stable session id.
            let rel = file_path
                .strip_prefix(&projects)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| fname.clone());

            sessions.push(SessionMeta {
                id: rel,
                path: file_path.to_string_lossy().into_owned(),
                project_raw: dir_name.clone(),
                mtime,
                size,
                preview,
                line_count,
                user_count,
                assistant_count,
                subagent_count,
                models,
                first_ts,
                last_ts,
                cwd,
                custom_title,
            });
        }
    }

    Ok(sessions)
}

/// Auto-clean junk session files: zero-turn, untitled, and stale `*.jsonl`
/// under the projects dir. The frontend calls this once from the browse-list
/// scan (BrowseView's onMount, just before `list_sessions`), so cleanup runs
/// at app start and on every return to the browse view. Uses the exact same
/// walk/skip rules as `list_sessions` (same dir filters, same `agent-*` skip)
/// and the shared `scan_session_lines`, gated by [`is_cleanup_eligible`] so
/// only genuinely-empty, untitled, cold files are removed — never real content.
/// Returns the number of files deleted. Best-effort: a file whose metadata or
/// mtime can't be read, or that fails to delete, is simply skipped.
#[tauri::command]
fn cleanup_empty_sessions() -> Result<u32, String> {
    let projects = projects_dir_inner()
        .ok_or_else(|| "Projects directory not found".to_string())?;
    let now = unix_secs(SystemTime::now());
    let mut removed: u32 = 0;

    let top_entries = fs::read_dir(&projects).map_err(|e| e.to_string())?;
    for top in top_entries.flatten() {
        let project_path = top.path();
        if !project_path.is_dir() {
            continue;
        }
        let dir_name = match project_path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if dir_name == "subagents" || dir_name == "tool-results" {
            continue;
        }

        let inner = match fs::read_dir(&project_path) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for jentry in inner.flatten() {
            let file_path = jentry.path();
            let fname = match file_path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            if !fname.ends_with(".jsonl") {
                continue;
            }
            if fname.starts_with("agent-") {
                continue;
            }

            let meta = match fs::metadata(&file_path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            // Skip (keep) any file whose mtime we can't read — never delete
            // something we can't prove is stale.
            let mtime = match meta.modified().ok().map(unix_secs) {
                Some(m) => m,
                None => continue,
            };

            let content = fs::read_to_string(&file_path).unwrap_or_default();
            let stats = scan_session_lines(&content);

            if is_cleanup_eligible(&stats, mtime, now, CLEANUP_RECENCY_WINDOW_SECS)
                && fs::remove_file(&file_path).is_ok()
            {
                removed += 1;
            }
        }
    }

    Ok(removed)
}

/// Return raw UTF-8 contents of a session .jsonl file.
#[tauri::command]
fn read_session(path: String) -> Result<String, String> {
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

/// Resolve `<backups_root>/<sanitized-session-id>` for `path`, requiring it
/// be under `projects`. Shared by [`snapshot_at`] and [`list_backups_at`].
/// The root is a parameter (callers pass `<data root>/backups`, or the legacy
/// `~/.claude/.ccstudio-backups` on the read-fallback path) — issue #24 moved
/// backups out of `~/.claude`.
fn backup_root_for(projects: &Path, backups_root: &Path, path: &str) -> Result<PathBuf, String> {
    let file_path = Path::new(path);
    let rel = file_path
        .strip_prefix(projects)
        .map_err(|_| "Session file is not under the projects directory".to_string())?;
    let session_id = sanitize_id(&rel.to_string_lossy());
    Ok(backups_root.join(&session_id))
}

/// Copy the current on-disk file into the backup store before an override,
/// resolving paths against `projects`/`backups_root` (parameterized so this
/// is testable without touching `CLAUDE_CONFIG_DIR`/`CCDECK_DATA_DIR`; the
/// `#[tauri::command]` wrapper below resolves the real ones).
///
/// There is exactly one backup slot per session — any existing backup file(s)
/// in the session's backup directory are deleted before the new one is
/// written, so a call to this function never leaves more than one file
/// behind.
///
/// Backup location:
///   ~/.ccdeck/backups/<sanitized_session_id>/v001-<unixsecs>.jsonl
fn snapshot_at(projects: &Path, backups_root: &Path, path: &str) -> Result<BackupVersion, String> {
    let file_path = Path::new(path);
    let backup_root = backup_root_for(projects, backups_root, path)?;

    fs::create_dir_all(&backup_root).map_err(|e| e.to_string())?;

    // Single-slot backup: clear out anything already in the directory before
    // writing the new one, so exactly one file exists afterward.
    for entry in fs::read_dir(&backup_root).map_err(|e| e.to_string())?.flatten() {
        let entry_path = entry.path();
        if entry_path.is_file() {
            fs::remove_file(&entry_path).map_err(|e| e.to_string())?;
        }
    }

    let version = 1;
    let timestamp = unix_secs(SystemTime::now());

    let backup_name = format!("v{:03}-{}.jsonl", version, timestamp);
    let backup_path = backup_root.join(&backup_name);

    fs::copy(file_path, &backup_path).map_err(|e| e.to_string())?;

    let size = fs::metadata(&backup_path)
        .map(|m| m.len())
        .unwrap_or(0);

    Ok(BackupVersion {
        version,
        timestamp,
        path: backup_path.to_string_lossy().into_owned(),
        size,
    })
}

#[tauri::command]
fn snapshot(path: String) -> Result<BackupVersion, String> {
    let projects = projects_dir_inner()
        .ok_or_else(|| "Projects directory not found".to_string())?;
    // Writes always target the new root — even if the startup migration
    // failed, new snapshots must not re-contaminate ~/.claude.
    snapshot_at(&projects, &datadir::data_root()?.join("backups"), &path)
}

/// List the session's backup(s), newest first. Since [`snapshot_at`] keeps
/// only a single backup slot, this returns at most one entry — the `Vec`
/// return shape is kept as-is since the frontend's list rendering already
/// handles it generically. See [`snapshot_at`] for why this is parameterized
/// on `projects`/`backups_root`.
fn list_backups_at(projects: &Path, backups_root: &Path, session_path: &str) -> Result<Vec<BackupVersion>, String> {
    let backup_root = backup_root_for(projects, backups_root, session_path)?;

    if !backup_root.is_dir() {
        return Ok(Vec::new());
    }

    let mut versions: Vec<BackupVersion> = fs::read_dir(&backup_root)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter_map(|entry| {
            let p = entry.path();
            let fname = p.file_name()?.to_str()?.to_string();
            if !fname.ends_with(".jsonl") {
                return None;
            }
            // Parse vNNN-<timestamp>.jsonl
            let stem = fname.strip_suffix(".jsonl")?;
            let mut parts = stem.splitn(2, '-');
            let version_str = parts.next()?;
            let ts_str = parts.next()?;
            let version: u32 = version_str.strip_prefix('v')?.parse().ok()?;
            let timestamp: u64 = ts_str.parse().ok()?;
            let size = fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            Some(BackupVersion {
                version,
                timestamp,
                path: p.to_string_lossy().into_owned(),
                size,
            })
        })
        .collect();

    // Newest first
    versions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    Ok(versions)
}

#[tauri::command]
fn list_backups(session_path: String) -> Result<Vec<BackupVersion>, String> {
    let projects = projects_dir_inner()
        .ok_or_else(|| "Projects directory not found".to_string())?;
    let found = list_backups_at(&projects, &datadir::data_root()?.join("backups"), &session_path)?;
    if !found.is_empty() {
        return Ok(found);
    }
    // Migration-failure fallback (contract: read whichever dir has the file):
    // if the new root has nothing for this session but the legacy dir still
    // exists, an old backup may be stranded there — surface it rather than
    // reporting "no backup" for data that exists.
    if let Some(home) = dirs::home_dir() {
        let legacy = datadir::legacy_backups_dir(&home);
        if legacy.is_dir() {
            return list_backups_at(&projects, &legacy, &session_path);
        }
    }
    Ok(found)
}

/// Overwrite the original .jsonl. By convention, a caller that's overwriting
/// existing content calls `snapshot(path)` immediately before this (see
/// SessionEditor.svelte's confirmSave/exitSave/confirmRestoreBackup) — this
/// function does not take or enforce that backup itself, it's a plain
/// overwrite. Split out from the `#[tauri::command]` wrapper (which also
/// needs `SearchState`) so the write itself is unit-testable, matching
/// [`snapshot_at`]/[`list_backups_at`]'s parameterized-for-testability shape.
fn write_session_at(path: &str, content: &str) -> Result<(), String> {
    fs::write(path, content).map_err(|e| e.to_string())
}

#[tauri::command]
fn write_session(
    state: tauri::State<'_, search::state::SearchState>,
    path: String,
    content: String,
) -> Result<(), String> {
    write_session_at(&path, &content)?;
    // Eager reindex so search reflects the edit immediately (the lazy sweep
    // would catch it eventually, but this keeps results in step with Save).
    state.indexer().reindex_one(&path);
    Ok(())
}

/// Return raw contents of a backup file (caller decides what to do with it).
#[tauri::command]
fn restore_backup(backup_path: String) -> Result<String, String> {
    fs::read_to_string(&backup_path).map_err(|e| e.to_string())
}

/// "Resume from here": copy lines `0..=upto_index` of the session at `path`
/// (same line-splitting rule as the frontend's `buildDraft`: split on '\n',
/// drop a trailing empty element from the final newline) into a NEW file next
/// to the original, under a fresh session uuid. Each kept line's `sessionId`
/// field (if present) is rewritten to the new uuid so the fork reads as its
/// own session; nothing else about the line is touched.
#[tauri::command]
fn fork_session(path: String, upto_index: usize) -> Result<ForkResult, String> {
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut lines: Vec<&str> = content.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    if upto_index >= lines.len() {
        return Err("Line index out of range".to_string());
    }

    let new_id = uuid::Uuid::new_v4().to_string();
    let kept: Vec<String> = lines[..=upto_index]
        .iter()
        .map(|line| {
            if line.trim().is_empty() {
                line.to_string()
            } else {
                json_replace_str_value(line, "\"sessionId\":\"", &new_id)
            }
        })
        .collect();
    let mut new_content = kept.join("\n");
    new_content.push('\n');

    let source_file = Path::new(&path);
    let parent = source_file
        .parent()
        .ok_or_else(|| "Cannot determine parent directory".to_string())?;
    let new_path = parent.join(format!("{}.jsonl", new_id));
    fs::write(&new_path, new_content).map_err(|e| e.to_string())?;

    Ok(ForkResult {
        path: new_path.to_string_lossy().into_owned(),
        id: new_id,
    })
}

/// Best-effort: open a terminal in `cwd` running the user's configured
/// resume-launch command (App Config → Launch command; default reproduces
/// the original `claude --resume <id>` behavior exactly), honouring the
/// user's persisted terminal preference (App Config → Terminal). Platform-
/// specific and inherently unreliable (depends on what's installed); callers
/// should always pair this with a copy-to-clipboard fallback.
///
/// Every OS/terminal candidate shares one script body built by
/// [`appconfig::build_resume_script`] (or its Windows counterpart) — this
/// removes the old per-platform hand-built `claude --resume <id> <args>`
/// strings and is the only way a multi-line custom `launch_command` works
/// uniformly across `open -a`, `gnome-terminal --`, `konsole -e`, `wt`, etc.
#[tauri::command]
fn resume_in_terminal(
    cwd: String,
    session_id: String,
    session_title: String,
    provider_name: Option<String>,
) -> Result<(), String> {
    if session_id.is_empty()
        || !session_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err("Invalid session id".to_string());
    }
    if !Path::new(&cwd).is_dir() {
        return Err(format!("Directory not found: {cwd}"));
    }

    // Resolve the selected provider's ANTHROPIC_* env pairs (issue #21). A
    // missing/empty provider_name ⇒ no override (default account). A named
    // provider that can't be resolved (unknown, or no key stored) errors here
    // rather than silently launching against the default account.
    let provider_env = match provider_name.as_deref() {
        Some(name) if !name.is_empty() => providers::provider_env_for(name)?,
        _ => Vec::new(),
    };

    let config = appconfig::load();
    let auto = appconfig::is_auto(&config.terminal);

    #[cfg(target_os = "macos")]
    {
        let script = appconfig::build_resume_script(&cwd, &session_id, &session_title, &config.launch_command, &provider_env);
        // `.command` extension: what makes Terminal.app treat an `open -a`'d
        // file as a script to run rather than a text file to display.
        let tmp = write_resume_script(&session_id, "command", &script)?;
        // Default terminal app is "Terminal"; an advanced preference can name
        // another (e.g. "iTerm").
        let app = if auto { "Terminal" } else { config.terminal.trim() };
        std::process::Command::new("open")
            .arg("-a")
            .arg(app)
            .arg(&tmp)
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        let script = appconfig::build_resume_script(&cwd, &session_id, &session_title, &config.launch_command, &provider_env);
        let tmp = write_resume_script(&session_id, "sh", &script)?;

        if auto {
            let candidates: &[(&str, &[&str])] = &[
                ("gnome-terminal", &["--"]),
                ("ptyxis", &["--"]),
                ("konsole", &["-e"]),
                ("xfce4-terminal", &["-x"]),
                ("alacritty", &["-e"]),
                ("xterm", &["-e"]),
                ("x-terminal-emulator", &["-e"]),
            ];
            for (term, prefix) in candidates {
                let mut cmd = std::process::Command::new(term);
                cmd.args(*prefix).arg("sh").arg(&tmp);
                if cmd.spawn().is_ok() {
                    return Ok(());
                }
            }
            return Err("No supported terminal emulator found".to_string());
        }

        // Advanced: the preference is a terminal command template, e.g.
        // "gnome-terminal --" or "konsole -e" — the first token is the program,
        // the rest are args that precede the `sh <script>` invocation.
        let tokens = appconfig::parse_args(&config.terminal);
        let Some((program, prefix_args)) = tokens.split_first() else {
            return Err("No terminal command configured".to_string());
        };
        let mut cmd = std::process::Command::new(program);
        cmd.args(prefix_args).arg("sh").arg(&tmp);
        return cmd
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Could not launch '{program}': {e}"));
    }

    #[cfg(target_os = "windows")]
    {
        let script = appconfig::build_resume_script_windows(&cwd, &session_id, &session_title, &config.launch_command, &provider_env);
        let tmp = write_resume_script(&session_id, "bat", &script)?;

        if auto {
            std::process::Command::new("cmd")
                .args(["/C", "start", "Claude Resume", "cmd", "/K", "call"])
                .arg(&tmp)
                .spawn()
                .map_err(|e| e.to_string())?;
            return Ok(());
        }

        // Advanced: the preference is a terminal command template, e.g. "wt".
        let tokens = appconfig::parse_args(&config.terminal);
        let Some((program, prefix_args)) = tokens.split_first() else {
            return Err("No terminal command configured".to_string());
        };
        let mut cmd = std::process::Command::new(program);
        cmd.args(prefix_args).arg("cmd").arg("/K").arg("call").arg(&tmp);
        return cmd
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Could not launch '{program}': {e}"));
    }

    #[allow(unreachable_code)]
    Err("Unsupported platform".to_string())
}

#[cfg(test)]
mod resume_tests {
    use super::*;

    #[test]
    fn json_replace_str_value_swaps_only_the_target_field() {
        let line = r#"{"type":"user","sessionId":"old-id","uuid":"u1","message":{"content":"hi"}}"#;
        let out = json_replace_str_value(line, "\"sessionId\":\"", "new-id");
        assert_eq!(
            out,
            r#"{"type":"user","sessionId":"new-id","uuid":"u1","message":{"content":"hi"}}"#
        );
    }

    #[test]
    fn json_replace_str_value_is_a_noop_when_key_absent() {
        let line = r#"{"type":"user","uuid":"u1"}"#;
        let out = json_replace_str_value(line, "\"sessionId\":\"", "new-id");
        assert_eq!(out, line);
    }

    #[test]
    fn fork_session_truncates_and_rewrites_session_id() {
        let dir = std::env::temp_dir().join(format!(
            "ccstudio-fork-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        let source = dir.join("orig-session.jsonl");
        let content = concat!(
            r#"{"type":"user","sessionId":"orig-session","uuid":"u1","message":{"content":"hi"}}"#, "\n",
            r#"{"type":"assistant","sessionId":"orig-session","uuid":"a1","message":{"content":"hello"}}"#, "\n",
            r#"{"type":"user","sessionId":"orig-session","uuid":"u2","message":{"content":"third line, should be dropped"}}"#, "\n",
        );
        fs::write(&source, content).unwrap();

        let result = fork_session(source.to_string_lossy().into_owned(), 1).unwrap();

        assert_ne!(result.id, "orig-session");
        assert!(result.path.ends_with(&format!("{}.jsonl", result.id)));

        let written = fs::read_to_string(&result.path).unwrap();
        let lines: Vec<&str> = written.lines().collect();
        assert_eq!(lines.len(), 2, "third line must be dropped: {written}");
        assert!(lines[0].contains(&format!("\"sessionId\":\"{}\"", result.id)));
        assert!(lines[1].contains(&format!("\"sessionId\":\"{}\"", result.id)));
        assert!(lines[0].contains("\"uuid\":\"u1\""), "message uuid must be untouched");
        assert!(!written.contains("third line"));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn fork_session_rejects_out_of_range_index() {
        let dir = std::env::temp_dir().join(format!(
            "ccstudio-fork-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        let source = dir.join("orig-session.jsonl");
        fs::write(&source, "{\"type\":\"user\"}\n").unwrap();

        let result = fork_session(source.to_string_lossy().into_owned(), 5);
        assert!(result.is_err());

        fs::remove_dir_all(&dir).unwrap();
    }
}

#[cfg(test)]
mod session_scan_tests {
    use super::*;

    /// `\uXXXX` and `\/` escapes must decode correctly (the old hand-rolled
    /// scanner only handled `\" \\ \n \r \t` and emitted a stray literal
    /// backslash for everything else).
    #[test]
    fn decodes_unicode_and_escaped_slash() {
        let line = concat!(
            r#"{"type":"user","cwd":"/home/user\/project","#,
            r#""customTitle":"Café chat","timestamp":"2026-01-01T00:00:00Z","#,
            r#""message":{"role":"user","content":"hi"}}"#,
        );
        let stats = scan_session_lines(line);
        assert_eq!(stats.cwd, "/home/user/project", "\\/ must decode to a plain slash");
        assert_eq!(stats.custom_title, "Café chat", "\\u00e9 must decode to é");
        assert_eq!(stats.first_ts, "2026-01-01T00:00:00Z");
        assert_eq!(stats.user_count, 1);
    }

    /// A nested string value that itself contains a literal `"type":"user"`
    /// substring must NOT be misread as the line's top-level type — the old
    /// scanner's unscoped `line.find(key_token)` would match this.
    #[test]
    fn nested_type_substring_is_not_misread_as_top_level() {
        let line = r#"{"type":"assistant","message":{"model":"claude-fable-5","content":"Example JSON in my answer: \"type\":\"user\""}}"#;
        let stats = scan_session_lines(line);
        assert_eq!(stats.assistant_count, 1, "must classify by the real top-level type");
        assert_eq!(stats.user_count, 0, "nested \"type\":\"user\" text must not count as a user line");
        assert_eq!(stats.models, vec!["claude-fable-5".to_string()], "message.model must still be read correctly");
    }

    /// A line that isn't valid JSON is silently skipped (still counted in
    /// `line_count`), matching the old scanner's silent-skip-on-no-match
    /// behavior — it must not panic or abort the rest of the scan.
    #[test]
    fn malformed_line_is_skipped_not_fatal() {
        let content = concat!(
            "not json at all\n",
            r#"{"type":"user","timestamp":"2026-01-01T00:00:00Z"}"#, "\n",
        );
        let stats = scan_session_lines(content);
        assert_eq!(stats.line_count, 2, "both non-empty lines are counted");
        assert_eq!(stats.user_count, 1, "only the valid line is classified");
        assert_eq!(stats.first_ts, "2026-01-01T00:00:00Z");
    }

    /// `model` lives at `message.model`, never at the line's top level.
    #[test]
    fn model_is_read_from_nested_message_field() {
        let content = concat!(
            r#"{"type":"assistant","message":{"model":"claude-a"}}"#, "\n",
            r#"{"type":"assistant","message":{"model":"claude-b"}}"#, "\n",
            r#"{"type":"assistant","message":{"model":"claude-a"}}"#, "\n",
        );
        let stats = scan_session_lines(content);
        assert_eq!(stats.models, vec!["claude-a".to_string(), "claude-b".to_string()], "distinct models, first-seen order");
    }
}

#[cfg(test)]
mod write_session_snapshot_tests {
    use super::*;

    /// Returns (base dir to clean up, projects root, backups root), with a
    /// fresh `<projects>/proj1/` directory already created — parameterized so
    /// these tests never touch the real `CLAUDE_CONFIG_DIR`/`CCDECK_DATA_DIR`.
    fn setup() -> (PathBuf, PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!(
            "ccstudio-snapshot-test-{}",
            uuid::Uuid::new_v4()
        ));
        let projects = base.join("home").join(".claude").join("projects");
        let backups_root = base.join("home").join(".ccdeck").join("backups");
        fs::create_dir_all(projects.join("proj1")).unwrap();
        (base, projects, backups_root)
    }

    #[test]
    fn snapshot_at_keeps_exactly_one_backup_file_across_repeated_calls() {
        let (base, projects, backups_root) = setup();
        let session = projects.join("proj1").join("session.jsonl");
        let session_path = session.to_string_lossy().into_owned();

        fs::write(&session, "v1 content\n").unwrap();
        snapshot_at(&projects, &backups_root, &session_path).unwrap();

        fs::write(&session, "v2 content\n").unwrap();
        snapshot_at(&projects, &backups_root, &session_path).unwrap();

        let backup_root = backup_root_for(&projects, &backups_root, &session_path).unwrap();
        let files_on_disk: Vec<_> = fs::read_dir(&backup_root).unwrap().flatten().collect();
        assert_eq!(files_on_disk.len(), 1, "only one backup file must exist on disk after two snapshots");

        let backups = list_backups_at(&projects, &backups_root, &session_path).unwrap();
        assert_eq!(backups.len(), 1, "list_backups_at must report exactly one entry");
        assert_eq!(
            fs::read_to_string(&backups[0].path).unwrap(),
            "v2 content\n",
            "the surviving backup must hold the most recent snapshot's content"
        );

        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn write_session_does_not_auto_snapshot() {
        let (base, projects, backups_root) = setup();
        let session = projects.join("proj1").join("session.jsonl");
        fs::write(&session, "original content\n").unwrap();
        let session_path = session.to_string_lossy().into_owned();

        // Call the real write path with no preceding explicit snapshot() call
        // — write_session must no longer take a backup as a side effect.
        write_session_at(&session_path, "new content\n").unwrap();

        let backups = list_backups_at(&projects, &backups_root, &session_path).unwrap();
        assert!(backups.is_empty(), "write_session must not create a backup as a side effect");
        assert_eq!(fs::read_to_string(&session).unwrap(), "new content\n");

        fs::remove_dir_all(&base).unwrap();
    }
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;

    const NOW: u64 = 1_000_000_000;
    const WINDOW: u64 = CLEANUP_RECENCY_WINDOW_SECS;

    /// A zero-turn, untitled file that's been sitting cold IS eligible.
    #[test]
    fn stale_zero_turn_untitled_is_eligible() {
        let stats = SessionScanStats {
            line_count: 3, // meta/summary lines only, no user/assistant turns
            ..Default::default()
        };
        let mtime = NOW - WINDOW - 60; // older than the window
        assert!(is_cleanup_eligible(&stats, mtime, NOW, WINDOW));
    }

    /// The safety rail: a freshly-created zero-turn session (a live CLI may have
    /// just opened it) is SPARED until it goes stale.
    #[test]
    fn recent_zero_turn_is_spared() {
        let stats = SessionScanStats {
            line_count: 3,
            ..Default::default()
        };
        // Modified 60s ago — well inside the recency window.
        let mtime = NOW - 60;
        assert!(!is_cleanup_eligible(&stats, mtime, NOW, WINDOW));
        // Exactly at the boundary counts as stale (>= window); just under does not.
        assert!(is_cleanup_eligible(&stats, NOW - WINDOW, NOW, WINDOW));
        assert!(!is_cleanup_eligible(&stats, NOW - WINDOW + 1, NOW, WINDOW));
    }

    /// A future-dated mtime (clock skew) must never read as stale.
    #[test]
    fn future_mtime_is_spared() {
        let stats = SessionScanStats {
            line_count: 3,
            ..Default::default()
        };
        assert!(!is_cleanup_eligible(&stats, NOW + 500, NOW, WINDOW));
    }

    /// Real content is never touched, no matter how old: a single user line,
    /// a single assistant line, or a custom title each block cleanup.
    #[test]
    fn any_real_content_is_never_eligible() {
        let ancient = NOW - WINDOW - 100_000; // very stale

        let with_user = SessionScanStats { user_count: 1, ..Default::default() };
        assert!(!is_cleanup_eligible(&with_user, ancient, NOW, WINDOW));

        let with_assistant = SessionScanStats { assistant_count: 1, ..Default::default() };
        assert!(!is_cleanup_eligible(&with_assistant, ancient, NOW, WINDOW));

        let with_title = SessionScanStats {
            custom_title: "My important session".to_string(),
            ..Default::default()
        };
        assert!(!is_cleanup_eligible(&with_title, ancient, NOW, WINDOW));
    }
}

// ---------------------------------------------------------------------------
// App entry point
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Move legacy ccdeck-owned state (backups, config) out of ~/.claude
    // before anything reads either location. Non-fatal by design: the app
    // must boot even if a rename fails (reads fall back to the legacy paths).
    datadir::migrate_legacy_state();

    // Build the search state up front (opens the SQLite cache). If it fails
    // (e.g. no home dir), the app still runs — search is just unavailable.
    let search_state = search::state::SearchState::new(projects_dir_inner(), dirs::home_dir());

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(prompts::state::PromptsState::new())
        .invoke_handler(tauri::generate_handler![
            find_projects_dir,
            home_dir,
            list_sessions,
            cleanup_empty_sessions,
            read_session,
            write_session,
            snapshot,
            list_backups,
            restore_backup,
            fork_session,
            resume_in_terminal,
            search::state::search,
            search::state::refresh_index,
            search::state::index_status,
            settings::read_claude_settings,
            settings::write_claude_settings,
            appconfig::get_app_config,
            appconfig::set_app_config,
            providers::list_provider_profiles,
            providers::save_provider_profile,
            providers::delete_provider_profile,
            providers::set_provider_key,
            providers::provider_key_status,
            providers::provider_probe_keychain,
            prompts::state::list_projects,
            prompts::state::add_project,
            prompts::state::remove_project,
            prompts::state::set_active_project,
            prompts::state::list_snippets,
            prompts::state::save_snippet,
            prompts::state::delete_snippet,
            prompts::state::match_snippets,
            prompts::state::touch_snippet,
        ]);

    let search_enabled = search_state.is_ok();
    match search_state {
        Ok(state) => builder = builder.manage(state),
        Err(e) => eprintln!("[search] disabled ({e}); browse/edit still work"),
    }

    builder = builder.setup(move |app| {
        if search_enabled {
            // Build/refresh the index in the background so launch isn't blocked.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let state = handle.state::<search::state::SearchState>();
                state.indexer().run_index();
            });
        }
        // Prompt Library: fetch the embedding model and index the active project
        // in the background, silently. Semantic match is an improvement to
        // ranking, never a prerequisite — lexical match works instantly and
        // unconditionally, so this blocks nothing and a failure is logged rather
        // than surfaced. There is no toggle and no progress UI by design.
        prompts::state::spawn_background_index(&app.state::<prompts::state::PromptsState>());
        Ok(())
    });

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
