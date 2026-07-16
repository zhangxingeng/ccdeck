//! CC Deck's own small preference store — the one file the app owns for itself
//! (never Claude Code's own `settings.json`, which this app never touches now
//! that the schema-driven settings editor — issue #34 — has been removed;
//! users hand-edit `settings.json` themselves).
//!
//! Lives at `~/.ccdeck/config.json` (issue #24 de-contamination: nothing
//! ccdeck-owned lives under `~/.claude` anymore; the legacy
//! `~/.claude/.ccstudio-config.json` is moved here on startup by the datadir
//! migration). Persisted as a file rather than localStorage because the Rust
//! side reads it at launch (the update-on-launch toggle) before the webview
//! settles.
//!
//! v0.14 (issue #34) shrank this to a single preference: the terminal launcher
//! (`terminal`, `launch_command`) is gone — Resume now surfaces the session's
//! facts as copyable text and the user acts in their own terminal. The struct
//! deliberately sets **no** `deny_unknown_fields`, so a config written by an
//! older install — carrying the now-removed `terminal` / `launchCommand`
//! keys (and older stale keys before them) — still loads cleanly, the stale
//! keys simply ignored. Proven by the round-trip tests below, not reasoned
//! about; a released user's config must never become a load failure on upgrade.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

/// CC Deck's own preferences. The single field defaults to "just works" so a
/// user who never opens App Config keeps the always-checked update behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    /// Whether CC Deck checks for app updates automatically on launch.
    /// Defaults to `true` — preserves the always-checked behavior for anyone
    /// who never opens App Config. A manual "Check for updates" click always
    /// runs regardless of this toggle.
    #[serde(default = "default_true")]
    pub update_check_on_launch: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            update_check_on_launch: true,
        }
    }
}

/// `~/.ccdeck/config.json` (under the datadir root, so `CCDECK_DATA_DIR`
/// redirects it in tests).
fn config_path() -> Result<PathBuf, String> {
    Ok(crate::datadir::data_root()?.join("config.json"))
}

/// Load the config, falling back to defaults on any error (missing file, bad
/// JSON). CC Deck must always launch even if this file is absent or corrupt.
pub fn load() -> AppConfig {
    config_path()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Persist the config (pretty-printed), creating the data root if needed.
pub fn save(config: &AppConfig) -> Result<(), String> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut pretty = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    pretty.push('\n');
    std::fs::write(&path, pretty).map_err(|e| e.to_string())
}

/// Return the current app config for the UI.
#[tauri::command]
pub fn get_app_config() -> AppConfig {
    load()
}

/// Persist the app config from the UI.
#[tauri::command]
pub fn set_app_config(config: AppConfig) -> Result<(), String> {
    save(&config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_has_update_check_on() {
        assert!(AppConfig::default().update_check_on_launch);
    }

    #[test]
    fn deserialize_missing_update_check_on_launch_defaults_true() {
        // An empty object (or any object omitting the field) loads with the
        // default preserved.
        let config: AppConfig = serde_json::from_str("{}").unwrap();
        assert!(config.update_check_on_launch);
    }

    #[test]
    fn deserialize_explicit_false_is_respected() {
        let json = r#"{"updateCheckOnLaunch":false}"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert!(!config.update_check_on_launch);
    }

    #[test]
    fn deserialize_ignores_removed_terminal_launcher_keys() {
        // Round-trip test (per project convention: verify parse/serialize
        // behavior with an adversarial fixture, not by reasoning about serde).
        //
        // v0.14 (issue #34) removed the terminal launcher — the `terminal` and
        // `launchCommand` fields are gone. The Prompt Library and this launcher
        // have shipped users (v0.12+), so a config already on someone's disk
        // still carries those keys (plus older stale ones: `terminalArgs`,
        // `promptsAsVariable`, `hotkeys`, `embedEnabled`). Because AppConfig
        // sets no `deny_unknown_fields`, every one of them is simply ignored and
        // the config loads cleanly — no migration code, and the surviving
        // `updateCheckOnLaunch` is honored. Removing a struct field must never
        // turn a released user's config into a load failure.
        let stale_json = r#"{
            "terminal": "konsole -e",
            "launchCommand": "tmux new-session -A -s x \"claude --resume $CCDECK_SESSION_ID\"",
            "terminalArgs": "--dangerously-skip-permissions",
            "promptsAsVariable": false,
            "hotkeys": {"copyPrompt": "Mod+C"},
            "embedEnabled": true,
            "updateCheckOnLaunch": false
        }"#;
        let config: AppConfig = serde_json::from_str(stale_json).unwrap();
        assert!(
            !config.update_check_on_launch,
            "the one surviving field is still honored across all the stale keys"
        );

        // And the serialized form no longer emits any of the removed fields.
        let json = serde_json::to_string(&AppConfig::default()).unwrap();
        assert!(!json.contains("terminal"), "terminal field is gone: {json}");
        assert!(
            !json.contains("launchCommand"),
            "launchCommand field is gone: {json}"
        );
    }
}
