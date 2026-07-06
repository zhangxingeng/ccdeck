//! CC Deck's own small preference store — the one file the app owns for itself
//! (as opposed to Claude Code's `settings.json`, which we only read/write on the
//! user's behalf in `settings.rs`).
//!
//! Lives at `~/.claude/.ccstudio-config.json`, keeping the established
//! `.ccstudio-*` on-disk naming (the same reason we don't rename those dirs: not
//! worth orphaning existing state). Persisted as a file rather than localStorage
//! because the Rust side needs these values at terminal-launch time, before any
//! webview is involved.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// User preferences for how CC Deck launches Claude Code. All fields are optional /
/// default to "just works" — customization is a hidden advanced affordance.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    /// Terminal launcher preference. Empty or "auto" ⇒ auto-detect (the default
    /// that "just works"). Otherwise a terminal command *prefix* that precedes
    /// the `claude` invocation — e.g. `gnome-terminal --`, `wezterm start --`,
    /// `konsole -e` on Linux; an app name like `iTerm` on macOS; `wt` on Windows.
    pub terminal: String,
    /// Extra arguments appended to the `claude` invocation, space-separated —
    /// e.g. `--dangerously-skip-permissions`. Power-user only; empty by default.
    pub terminal_args: String,
}

/// `~/.claude/.ccstudio-config.json`.
fn config_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    Ok(home.join(".claude").join(".ccstudio-config.json"))
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

/// Split a free-text argument string into tokens on whitespace. No shell-style
/// quote handling — sufficient for flag-shaped args like
/// `--dangerously-skip-permissions`; documented as a limitation in the UI.
pub fn parse_args(s: &str) -> Vec<String> {
    s.split_whitespace().map(|t| t.to_string()).collect()
}

/// Is this terminal preference the "auto-detect" default?
pub fn is_auto(terminal: &str) -> bool {
    let t = terminal.trim();
    t.is_empty() || t.eq_ignore_ascii_case("auto")
}

/// Return the current app config for the UI.
#[tauri::command]
pub fn get_app_config() -> AppConfig {
    load()
}

/// Persist the app config (pretty-printed), creating `~/.claude/` if needed.
#[tauri::command]
pub fn set_app_config(config: AppConfig) -> Result<(), String> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut pretty = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    pretty.push('\n');
    std::fs::write(&path, pretty).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_splits_on_whitespace() {
        assert_eq!(parse_args(""), Vec::<String>::new());
        assert_eq!(parse_args("   "), Vec::<String>::new());
        assert_eq!(
            parse_args("--dangerously-skip-permissions"),
            vec!["--dangerously-skip-permissions"]
        );
        assert_eq!(
            parse_args("  --foo   --bar baz "),
            vec!["--foo", "--bar", "baz"]
        );
    }

    #[test]
    fn is_auto_recognizes_empty_and_auto() {
        assert!(is_auto(""));
        assert!(is_auto("  "));
        assert!(is_auto("auto"));
        assert!(is_auto("AUTO"));
        assert!(!is_auto("gnome-terminal --"));
        assert!(!is_auto("iTerm"));
    }

    #[test]
    fn config_defaults_are_auto_and_no_args() {
        let c = AppConfig::default();
        assert!(is_auto(&c.terminal));
        assert!(c.terminal_args.is_empty());
    }
}
