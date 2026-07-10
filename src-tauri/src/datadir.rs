//! ccdeck's own data root (`~/.ccdeck/`) and the one-time startup migration
//! that moves legacy ccdeck-owned state out of `~/.claude/`.
//!
//! Rationale (prompts-design contract): `~/.claude` belongs to Claude Code —
//! users audit it and other tools parse it. Everything ccdeck owns lives under
//! its own root, and the three artifacts we historically parked in `~/.claude`
//! (`.ccstudio-backups/`, `.ccstudio-config.json`, `.ccstudio-index/`) migrate
//! out on startup. Invariant from here on: nothing ccdeck-owned lives under
//! `~/.claude`.
//!
//! Migration is NON-FATAL by design: backups/config are conveniences and the
//! search index is a rebuildable cache, not core data — the app must boot even
//! if a rename fails (read paths fall back, or the cache rebuilds).

use std::fs;
use std::path::{Path, PathBuf};

/// Resolve the ccdeck data root: `CCDECK_DATA_DIR` env override (tests use
/// this — same pattern as `CLAUDE_CONFIG_DIR`), else `~/.ccdeck`. Unlike the
/// projects-dir lookup this does NOT require the directory to exist yet —
/// writers create what they need under it.
pub fn data_root() -> Result<PathBuf, String> {
    if let Ok(dir) = std::env::var("CCDECK_DATA_DIR") {
        if !dir.trim().is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }
    dirs::home_dir()
        .map(|h| h.join(".ccdeck"))
        .ok_or_else(|| "Cannot determine home directory".to_string())
}

/// The legacy backups dir this app shipped with: `~/.claude/.ccstudio-backups`.
pub fn legacy_backups_dir(home: &Path) -> PathBuf {
    home.join(".claude").join(".ccstudio-backups")
}

/// The legacy app-config file: `~/.claude/.ccstudio-config.json`.
pub fn legacy_config_path(home: &Path) -> PathBuf {
    home.join(".claude").join(".ccstudio-config.json")
}

/// The legacy search-cache dir: `~/.claude/.ccstudio-index` (search.db +
/// tantivy index + migration lock).
pub fn legacy_index_dir(home: &Path) -> PathBuf {
    home.join(".claude").join(".ccstudio-index")
}

/// Run the whole legacy migration once at startup. Never fails the boot:
/// each step logs on error and the app continues (readers fall back to the
/// legacy locations).
pub fn migrate_legacy_state() {
    let Some(home) = dirs::home_dir() else {
        return;
    };
    let Ok(root) = data_root() else {
        return;
    };
    if let Err(e) = migrate_backups_at(&legacy_backups_dir(&home), &root.join("backups")) {
        eprintln!("[datadir] backups migration incomplete ({e}); falling back to legacy reads");
    }
    if let Err(e) = migrate_config_at(&legacy_config_path(&home), &root.join("config.json")) {
        eprintln!("[datadir] config migration incomplete ({e}); falling back to legacy reads");
    }
    if let Err(e) = migrate_index_at(&legacy_index_dir(&home), &root.join("index")) {
        eprintln!("[datadir] search-cache migration incomplete ({e}); the index will rebuild");
    }
}

/// Move the legacy backups tree to `new`, per the contract's rules:
/// - legacy absent → nothing to do;
/// - `new` absent → plain rename (same filesystem: both live under home);
/// - both exist (half migration / downgrade-then-upgrade) → merge legacy
///   session dirs that don't collide, prefer the new location on collision
///   (the colliding legacy dir is left in place — deleting a backup a user
///   might still want is worse than leaving residue), and remove the legacy
///   dir only when emptied.
fn migrate_backups_at(legacy: &Path, new: &Path) -> Result<(), String> {
    if !legacy.is_dir() {
        return Ok(());
    }
    if !new.exists() {
        if let Some(parent) = new.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        return fs::rename(legacy, new).map_err(|e| e.to_string());
    }
    // Both exist: merge per-session dirs.
    let mut first_err: Option<String> = None;
    for entry in fs::read_dir(legacy).map_err(|e| e.to_string())?.flatten() {
        let target = new.join(entry.file_name());
        if target.exists() {
            continue; // collision: prefer the new location, leave legacy copy
        }
        if let Err(e) = fs::rename(entry.path(), &target) {
            first_err.get_or_insert(e.to_string());
        }
    }
    // Only remove the legacy dir once nothing is left inside it.
    let emptied = fs::read_dir(legacy)
        .map(|mut d| d.next().is_none())
        .unwrap_or(false);
    if emptied {
        if let Err(e) = fs::remove_dir(legacy) {
            first_err.get_or_insert(e.to_string());
        }
    }
    match first_err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// Move the legacy config file to `new`. If both exist, the new location wins
/// (it was written by a newer install) and the superseded legacy file is
/// removed — leaving it would keep ccdeck-owned state under `~/.claude`
/// forever, violating the invariant this migration exists to establish.
fn migrate_config_at(legacy: &Path, new: &Path) -> Result<(), String> {
    if !legacy.is_file() {
        return Ok(());
    }
    if new.exists() {
        return fs::remove_file(legacy).map_err(|e| e.to_string());
    }
    if let Some(parent) = new.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::rename(legacy, new).map_err(|e| e.to_string())
}

/// Move the legacy search cache to `new`. Simpler collision rule than
/// backups/config because this artifact is REBUILDABLE (a wiped index just
/// re-indexes on next launch): if both exist, delete the legacy one — merging
/// caches is pointless and leaving it defeats the nothing-under-`~/.claude`
/// invariant.
fn migrate_index_at(legacy: &Path, new: &Path) -> Result<(), String> {
    if !legacy.is_dir() {
        return Ok(());
    }
    if new.exists() {
        return fs::remove_dir_all(legacy).map_err(|e| e.to_string());
    }
    if let Some(parent) = new.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::rename(legacy, new).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ccdeck-datadir-test-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    // --- backups migration ---

    #[test]
    fn backups_fresh_install_is_noop() {
        let root = tmp_dir("fresh");
        let legacy = root.join("legacy");
        let new = root.join("new");
        assert!(migrate_backups_at(&legacy, &new).is_ok());
        assert!(!new.exists(), "no legacy dir must create nothing");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn backups_legacy_only_is_renamed_whole() {
        let root = tmp_dir("legacy-only");
        let legacy = root.join("legacy");
        let new = root.join("deep").join("new"); // parent doesn't exist yet
        fs::create_dir_all(legacy.join("session_a")).unwrap();
        fs::write(legacy.join("session_a").join("v001-1.jsonl"), "x").unwrap();

        migrate_backups_at(&legacy, &new).unwrap();

        assert!(!legacy.exists(), "legacy dir must be gone");
        assert_eq!(fs::read_to_string(new.join("session_a").join("v001-1.jsonl")).unwrap(), "x");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn backups_both_exist_merges_and_prefers_new_on_collision() {
        let root = tmp_dir("collision");
        let legacy = root.join("legacy");
        let new = root.join("new");
        // legacy: colliding session + unique session
        fs::create_dir_all(legacy.join("shared")).unwrap();
        fs::write(legacy.join("shared").join("old.jsonl"), "legacy").unwrap();
        fs::create_dir_all(legacy.join("only_legacy")).unwrap();
        fs::write(legacy.join("only_legacy").join("v001-1.jsonl"), "moved").unwrap();
        // new: the colliding session already migrated
        fs::create_dir_all(new.join("shared")).unwrap();
        fs::write(new.join("shared").join("new.jsonl"), "new").unwrap();

        migrate_backups_at(&legacy, &new).unwrap();

        // Unique legacy session moved over.
        assert_eq!(fs::read_to_string(new.join("only_legacy").join("v001-1.jsonl")).unwrap(), "moved");
        // Collision: new location untouched, legacy copy left in place.
        assert_eq!(fs::read_to_string(new.join("shared").join("new.jsonl")).unwrap(), "new");
        assert!(!new.join("shared").join("old.jsonl").exists());
        assert!(legacy.join("shared").join("old.jsonl").exists(), "colliding legacy data must not be destroyed");
        // Legacy dir NOT removed — it isn't empty (the colliding dir remains).
        assert!(legacy.exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn backups_both_exist_removes_legacy_when_fully_merged() {
        let root = tmp_dir("emptied");
        let legacy = root.join("legacy");
        let new = root.join("new");
        fs::create_dir_all(legacy.join("only_legacy")).unwrap();
        fs::create_dir_all(&new).unwrap();

        migrate_backups_at(&legacy, &new).unwrap();

        assert!(new.join("only_legacy").exists());
        assert!(!legacy.exists(), "emptied legacy dir must be removed");
        fs::remove_dir_all(&root).unwrap();
    }

    // --- config migration ---

    #[test]
    fn config_fresh_install_is_noop() {
        let root = tmp_dir("cfg-fresh");
        assert!(migrate_config_at(&root.join("none.json"), &root.join("new.json")).is_ok());
        assert!(!root.join("new.json").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn config_legacy_only_moves_to_new_path() {
        let root = tmp_dir("cfg-move");
        let legacy = root.join(".ccstudio-config.json");
        let new = root.join("ccdeck").join("config.json");
        fs::write(&legacy, "{\"terminal\":\"konsole -e\"}").unwrap();

        migrate_config_at(&legacy, &new).unwrap();

        assert!(!legacy.exists());
        assert_eq!(fs::read_to_string(&new).unwrap(), "{\"terminal\":\"konsole -e\"}");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn config_both_exist_prefers_new_and_removes_superseded_legacy() {
        let root = tmp_dir("cfg-both");
        let legacy = root.join("legacy.json");
        let new = root.join("new.json");
        fs::write(&legacy, "old").unwrap();
        fs::write(&new, "new").unwrap();

        migrate_config_at(&legacy, &new).unwrap();

        assert_eq!(fs::read_to_string(&new).unwrap(), "new", "newer config must win");
        assert!(!legacy.exists(), "superseded legacy config must not linger under ~/.claude");
        fs::remove_dir_all(&root).unwrap();
    }

    // --- search-cache migration (audit M1) ---

    #[test]
    fn index_fresh_install_is_noop() {
        let root = tmp_dir("idx-fresh");
        assert!(migrate_index_at(&root.join("legacy"), &root.join("new")).is_ok());
        assert!(!root.join("new").exists(), "no legacy dir must create nothing");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn index_legacy_only_is_renamed_whole() {
        let root = tmp_dir("idx-legacy");
        let legacy = root.join("legacy");
        let new = root.join("deep").join("new"); // parent doesn't exist yet
        fs::create_dir_all(legacy.join("tantivy")).unwrap();
        fs::write(legacy.join("search.db"), "db").unwrap();

        migrate_index_at(&legacy, &new).unwrap();

        assert!(!legacy.exists(), "legacy cache dir must be gone");
        assert_eq!(fs::read_to_string(new.join("search.db")).unwrap(), "db");
        assert!(new.join("tantivy").is_dir());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn index_both_exist_deletes_legacy_cache() {
        // Unlike backups (irreplaceable), a cache rebuilds — so the collision
        // rule is deletion, not merge: keeping the legacy copy would defeat
        // the nothing-under-~/.claude invariant for zero benefit.
        let root = tmp_dir("idx-both");
        let legacy = root.join("legacy");
        let new = root.join("new");
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("search.db"), "stale").unwrap();
        fs::create_dir_all(&new).unwrap();
        fs::write(new.join("search.db"), "current").unwrap();

        migrate_index_at(&legacy, &new).unwrap();

        assert!(!legacy.exists(), "legacy cache must be deleted on collision");
        assert_eq!(fs::read_to_string(new.join("search.db")).unwrap(), "current", "the live cache is untouched");
        fs::remove_dir_all(&root).unwrap();
    }
}
