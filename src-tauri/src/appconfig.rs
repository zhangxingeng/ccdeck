//! CC Deck's own small preference store — the one file the app owns for itself
//! (never Claude Code's own `settings.json`, which this app never touches now
//! that the schema-driven settings editor — issue #18 — has been removed;
//! users hand-edit `settings.json` themselves).
//!
//! Lives at `~/.claude/.ccstudio-config.json`, keeping the established
//! `.ccstudio-*` on-disk naming (the same reason we don't rename those dirs: not
//! worth orphaning existing state). Persisted as a file rather than localStorage
//! because the Rust side needs these values at terminal-launch time, before any
//! webview is involved.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Zero-config default: reproduces the exact `claude --resume <id>` behavior
/// CC Deck always had, now expressed as a free-text command that reads the
/// session id from the injected `CCDECK_SESSION_ID` env var instead of a
/// hardcoded CLI arg.
pub const DEFAULT_LAUNCH_COMMAND: &str = r#"claude --resume "$CCDECK_SESSION_ID""#;

fn default_true() -> bool {
    true
}

/// User preferences for how CC Deck launches Claude Code. All fields are optional /
/// default to "just works" — customization is a hidden advanced affordance.
///
/// `terminal_args` (issue #18-era "extra CLI args appended after `--resume
/// <id>`") is gone — superseded by `launch_command`, which is fully free-text
/// and already lets a user append any flag they want. No `deny_unknown_fields`
/// is set, so an old on-disk config with a stale `terminalArgs` key simply has
/// that key ignored on next load (covered by
/// `deserialize_ignores_stale_terminal_args_key` below) — no migration needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    /// Terminal launcher preference. Empty or "auto" ⇒ auto-detect (the default
    /// that "just works"). Otherwise a terminal command *prefix* that precedes
    /// the launch script invocation — e.g. `gnome-terminal --`, `wezterm start --`,
    /// `konsole -e` on Linux; an app name like `iTerm` on macOS; `wt` on Windows.
    pub terminal: String,
    /// Fully custom resume-launch command, run as a shell-script body (may be
    /// multi-line — a small script, not just a one-liner). Empty ⇒
    /// [`DEFAULT_LAUNCH_COMMAND`]. Three env vars are exported before it runs:
    /// `CCDECK_SESSION_ID`, `CCDECK_SESSION_TITLE`, `CCDECK_CWD`.
    pub launch_command: String,
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
            terminal: String::new(),
            launch_command: String::new(),
            update_check_on_launch: true,
        }
    }
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
/// Still used to split the `terminal` field's command-prefix template (e.g.
/// `"gnome-terminal --"`) into a program + leading args.
pub fn parse_args(s: &str) -> Vec<String> {
    s.split_whitespace().map(|t| t.to_string()).collect()
}

/// Is this terminal preference the "auto-detect" default?
pub fn is_auto(terminal: &str) -> bool {
    let t = terminal.trim();
    t.is_empty() || t.eq_ignore_ascii_case("auto")
}

/// Resolve the effective launch-command text: the user's custom command, or
/// [`DEFAULT_LAUNCH_COMMAND`] when it's empty/whitespace-only.
pub fn effective_launch_command(launch_command: &str) -> String {
    if launch_command.trim().is_empty() {
        DEFAULT_LAUNCH_COMMAND.to_string()
    } else {
        launch_command.to_string()
    }
}

/// The one shared script-generation function every terminal-emulator/OS
/// candidate uses (replacing three separately hand-built
/// `claude --resume <id> <extra_args>` strings) — a pure function, fully
/// unit-testable without spawning any process. Returns a POSIX shell-script
/// body: env exports for the three `CCDECK_*` vars (shell-quoted via
/// [`crate::shell_quote`]), then `cd <cwd> &&`, then the raw `launch_command`
/// text verbatim (which may itself be multi-line).
pub fn build_resume_script(
    cwd: &str,
    session_id: &str,
    session_title: &str,
    launch_command: &str,
) -> String {
    let command = effective_launch_command(launch_command);
    format!(
        "#!/bin/sh\nexport CCDECK_SESSION_ID={}\nexport CCDECK_SESSION_TITLE={}\nexport CCDECK_CWD={}\ncd {} &&\n{}\n",
        crate::shell_quote(session_id),
        crate::shell_quote(session_title),
        crate::shell_quote(cwd),
        crate::shell_quote(cwd),
        command,
    )
}

/// `cmd.exe` has no reliable escape for a literal `"` inside a `set "VAR=value"`
/// value (unlike POSIX single-quoting, which `shell_quote` uses), so a
/// session title containing `"` could otherwise close the quoted value early
/// and let a trailing `&`/`|` chain an arbitrary command. Rather than attempt
/// imperfect `cmd.exe` escaping, strip what would break out: embedded quotes
/// (replaced with a single quote, which carries no special meaning to
/// `cmd.exe`) and CR/LF (which would otherwise inject new script lines).
/// Also double `%` to `%%`, since batch-file parsing expands `%...%` inside a
/// value regardless of quoting — a title like `%PATH%` would otherwise leak
/// unrelated environment content into the exported var.
fn windows_escape(s: &str) -> String {
    s.chars()
        .filter(|c| *c != '\r' && *c != '\n')
        .flat_map(|c| match c {
            '"' => vec!['\''],
            '%' => vec!['%', '%'],
            other => vec![other],
        })
        .collect()
}

/// Windows equivalent of [`build_resume_script`]: a `.bat`/`.cmd` script body.
/// Best-effort — this repo has no Windows CI/dev machine, so this gets unit
/// coverage on the pure string-builder here rather than exhaustive manual
/// testing. `cwd`/`session_id`/`session_title` are escaped via
/// [`windows_escape`] before being spliced into `set "VAR=value"`; the
/// `launch_command` text itself is spliced raw, same as the POSIX path — it's
/// the user's own authored command, not attacker-influenced input.
///
/// Only called from `lib.rs`'s `#[cfg(target_os = "windows")]` branch, so
/// non-Windows builds see it as dead code — kept unconditionally compiled
/// (rather than `#[cfg(target_os = "windows")]`-gated) so it's still
/// unit-tested on every platform's `cargo test`.
#[allow(dead_code)]
pub fn build_resume_script_windows(
    cwd: &str,
    session_id: &str,
    session_title: &str,
    launch_command: &str,
) -> String {
    let command = effective_launch_command(launch_command);
    let cwd_esc = windows_escape(cwd);
    let id_esc = windows_escape(session_id);
    let title_esc = windows_escape(session_title);
    format!(
        "@echo off\r\nset \"CCDECK_SESSION_ID={id_esc}\"\r\nset \"CCDECK_SESSION_TITLE={title_esc}\"\r\nset \"CCDECK_CWD={cwd_esc}\"\r\ncd /D \"{cwd_esc}\"\r\n{command}\r\n"
    )
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
    fn config_defaults_are_auto_no_command_and_update_check_on() {
        let c = AppConfig::default();
        assert!(is_auto(&c.terminal));
        assert!(c.launch_command.is_empty());
        assert!(c.update_check_on_launch);
    }

    #[test]
    fn deserialize_ignores_stale_terminal_args_key() {
        // Round-trip test (per project convention: verify parse/serialize
        // behavior with an adversarial fixture, not just by reading the code).
        // An old on-disk config written before `terminal_args` was dropped
        // must still load cleanly — the stale key is simply ignored, no
        // migration code needed, because AppConfig has no
        // `deny_unknown_fields`.
        let stale_json = r#"{"terminal":"konsole -e","terminalArgs":"--dangerously-skip-permissions"}"#;
        let config: AppConfig = serde_json::from_str(stale_json).unwrap();
        assert_eq!(config.terminal, "konsole -e");
        assert_eq!(config.launch_command, "");
        assert!(config.update_check_on_launch, "missing key must default to true");
    }

    #[test]
    fn deserialize_missing_update_check_on_launch_defaults_true() {
        let json = r#"{"terminal":"","launchCommand":""}"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert!(config.update_check_on_launch);
    }

    #[test]
    fn deserialize_explicit_false_is_respected() {
        let json = r#"{"updateCheckOnLaunch":false}"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert!(!config.update_check_on_launch);
    }

    #[test]
    fn effective_launch_command_falls_back_to_default_when_empty() {
        assert_eq!(effective_launch_command(""), DEFAULT_LAUNCH_COMMAND);
        assert_eq!(effective_launch_command("   \n  "), DEFAULT_LAUNCH_COMMAND);
        assert_eq!(effective_launch_command("echo hi"), "echo hi");
    }

    #[test]
    fn build_resume_script_exports_env_vars_and_uses_default_command() {
        let script = build_resume_script("/home/user/proj", "abc-123", "My Session", "");
        assert!(script.starts_with("#!/bin/sh\n"));
        assert!(script.contains("export CCDECK_SESSION_ID='abc-123'"));
        assert!(script.contains("export CCDECK_SESSION_TITLE='My Session'"));
        assert!(script.contains("export CCDECK_CWD='/home/user/proj'"));
        assert!(script.contains("cd '/home/user/proj' &&"));
        assert!(script.contains(DEFAULT_LAUNCH_COMMAND));
    }

    #[test]
    fn build_resume_script_preserves_multiline_custom_command() {
        let custom = "tmux new-session -A -s \"$CCDECK_SESSION_TITLE\" \"claude --resume $CCDECK_SESSION_ID\"";
        let script = build_resume_script("/tmp/proj", "id-1", "Title", custom);
        assert!(script.contains(custom));
    }

    #[test]
    fn build_resume_script_shell_quotes_titles_with_special_characters() {
        // Adversarial title: embedded single quote + spaces — must not break
        // out of the exported env var's shell-quoting.
        let script = build_resume_script("/tmp/proj", "id-1", "It's a \"test\" session", "");
        assert!(script.contains(r#"export CCDECK_SESSION_TITLE='It'\''s a "test" session'"#));
    }

    #[test]
    fn build_resume_script_windows_contains_env_sets_and_command() {
        let script = build_resume_script_windows("C:\\proj", "abc-123", "My Session", "");
        assert!(script.starts_with("@echo off\r\n"));
        assert!(script.contains("set \"CCDECK_SESSION_ID=abc-123\""));
        assert!(script.contains("set \"CCDECK_SESSION_TITLE=My Session\""));
        assert!(script.contains("set \"CCDECK_CWD=C:\\proj\""));
        assert!(script.contains("cd /D \"C:\\proj\""));
        assert!(script.contains(DEFAULT_LAUNCH_COMMAND));
    }

    #[test]
    fn build_resume_script_windows_escapes_quote_breakout_attempt() {
        // Adversarial title: an embedded `"` followed by `&` would otherwise
        // close the `set "VAR=..."` value early and chain an arbitrary
        // command on real cmd.exe. The escaped output must not contain a
        // bare `" & calc.exe & "` sequence, and must not contain any `"`
        // characters at all inside the exported values (each embedded quote
        // is replaced with a single quote).
        let evil_title = "x\" & calc.exe & \"";
        let script = build_resume_script_windows("C:\\proj", "id-1", evil_title, "");
        assert!(!script.contains("\" & calc.exe & \""));
        assert!(script.contains("set \"CCDECK_SESSION_TITLE=x' & calc.exe & '\""));
    }

    #[test]
    fn build_resume_script_windows_strips_newlines_and_escapes_percent() {
        let script = build_resume_script_windows("C:\\proj", "id-1", "line1\r\nline2 %PATH%", "");
        assert!(script.contains("set \"CCDECK_SESSION_TITLE=line1line2 %%PATH%%\""));
    }
}
