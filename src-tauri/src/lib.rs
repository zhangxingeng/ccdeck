use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::Manager;

// Search: SQLite-backed extracted-text cache (built up over milestones 1–12).
mod search;

// CC Deck's own preference store (terminal launcher choice + extra args).
mod appconfig;

// Schema-driven Claude Code settings: read/merge/conflict across the three tiers.
mod settings;

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

#[derive(Serialize)]
pub struct SubagentFile {
    pub name: String,
    pub content: String,
    pub is_meta: bool,
}

#[derive(Serialize)]
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

/// Return the quoted string value immediately following `key_token` in `line`, if present.
/// Handles simple `\"` escape sequences; good enough for these well-known key tokens.
fn json_str_after(line: &str, key_token: &str) -> Option<String> {
    let start = line.find(key_token)?;
    let rest = &line[start + key_token.len()..];
    let mut result = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        if c == '"' {
            return Some(result);
        } else if c == '\\' {
            if let Some(escaped) = chars.next() {
                match escaped {
                    '"' => result.push('"'),
                    '\\' => result.push('\\'),
                    'n' => result.push('\n'),
                    'r' => result.push('\r'),
                    't' => result.push('\t'),
                    _ => {
                        result.push('\\');
                        result.push(escaped);
                    }
                }
            }
        } else {
            result.push(c);
        }
    }
    None // no closing quote found
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

/// Single-quote `s` for embedding in a POSIX shell script (used only for the
/// macOS Terminal.app launch script).
#[cfg(target_os = "macos")]
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Resolve the path to the edit draft file for a given session path.
fn edit_draft_path(session_path: &str) -> Result<PathBuf, String> {
    let file_path = Path::new(session_path);

    let session_id = if let Some(projects) = projects_dir_inner() {
        if let Ok(rel) = file_path.strip_prefix(&projects) {
            sanitize_id(&rel.to_string_lossy())
        } else {
            sanitize_id(session_path)
        }
    } else {
        sanitize_id(session_path)
    };

    let home = dirs::home_dir()
        .ok_or_else(|| "Cannot determine home directory".to_string())?;
    Ok(home
        .join(".claude")
        .join(".ccstudio-edits")
        .join(format!("{}.json", session_id)))
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
            let mut line_count: u64 = 0;
            let mut user_count: u64 = 0;
            let mut assistant_count: u64 = 0;
            let mut models: Vec<String> = Vec::new();
            let mut first_ts: String = String::new();
            let mut last_ts: String = String::new();
            let mut cwd: String = String::new();
            let mut custom_title: String = String::new();

            for line in content.lines() {
                if line.is_empty() {
                    continue;
                }
                line_count += 1;

                if let Some(ty) = json_str_after(line, "\"type\":\"") {
                    match ty.as_str() {
                        "user" => user_count += 1,
                        "assistant" => assistant_count += 1,
                        _ => {}
                    }
                }

                if let Some(model) = json_str_after(line, "\"model\":\"") {
                    if !model.is_empty() && !models.contains(&model) {
                        models.push(model);
                    }
                }

                if let Some(ts) = json_str_after(line, "\"timestamp\":\"") {
                    if !ts.is_empty() {
                        if first_ts.is_empty() {
                            first_ts = ts.clone();
                        }
                        last_ts = ts;
                    }
                }

                // The encoded project dir name is lossy ('/' -> '-'), so prefer the
                // real cwd recorded on the JSONL lines themselves (first-seen).
                if cwd.is_empty() {
                    if let Some(c) = json_str_after(line, "\"cwd\":\"") {
                        if !c.is_empty() {
                            cwd = c;
                        }
                    }
                }

                // Renames can land anywhere in the file (Claude Code's own /rename
                // appends near wherever the conversation currently is), so this must
                // scan the whole file, not just the 50-line preview — and last-wins,
                // since a later rename supersedes an earlier one.
                if let Some(ct) = json_str_after(line, "\"customTitle\":\"") {
                    if !ct.is_empty() {
                        custom_title = ct;
                    }
                }
            }

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

/// Return raw UTF-8 contents of a session .jsonl file.
#[tauri::command]
fn read_session(path: String) -> Result<String, String> {
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

/// Look in <dir-of-session>/subagents/ for agent-*.jsonl and agent-*.meta.json.
#[tauri::command]
fn read_subagents(session_path: String) -> Result<Vec<SubagentFile>, String> {
    let session_file = Path::new(&session_path);
    let parent = session_file
        .parent()
        .ok_or_else(|| "Cannot determine parent directory".to_string())?;
    let subagents_dir = parent.join("subagents");

    if !subagents_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut files: Vec<SubagentFile> = Vec::new();
    let entries = fs::read_dir(&subagents_dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let file_path = entry.path();
        let fname = match file_path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let is_meta: bool;
        if fname.starts_with("agent-") && fname.ends_with(".meta.json") {
            is_meta = true;
        } else if fname.starts_with("agent-") && fname.ends_with(".jsonl") {
            is_meta = false;
        } else {
            continue;
        }
        let content = fs::read_to_string(&file_path).map_err(|e| e.to_string())?;
        files.push(SubagentFile {
            name: fname,
            content,
            is_meta,
        });
    }

    Ok(files)
}

/// Overwrite the original .jsonl.  Caller MUST call snapshot(path) first.
#[tauri::command]
fn write_session(
    state: tauri::State<'_, search::state::SearchState>,
    path: String,
    content: String,
) -> Result<(), String> {
    fs::write(&path, content).map_err(|e| e.to_string())?;
    // Eager reindex so search reflects the edit immediately (the lazy sweep
    // would catch it eventually, but this keeps results in step with Save).
    state.indexer().reindex_one(&path);
    Ok(())
}

/// Copy the current on-disk file into the backup store before an override.
///
/// Backup location:
///   ~/.claude/.ccstudio-backups/<sanitized_session_id>/vNNN-<unixsecs>.jsonl
///
/// NNN is 1-based and grows by counting existing *.jsonl files in the dir.
#[tauri::command]
fn snapshot(path: String) -> Result<BackupVersion, String> {
    let projects = projects_dir_inner()
        .ok_or_else(|| "Projects directory not found".to_string())?;

    let file_path = Path::new(&path);

    let rel = file_path
        .strip_prefix(&projects)
        .map_err(|_| "Session file is not under the projects directory".to_string())?;

    let session_id = sanitize_id(&rel.to_string_lossy());

    let home = dirs::home_dir()
        .ok_or_else(|| "Cannot determine home directory".to_string())?;
    let backup_root = home
        .join(".claude")
        .join(".ccstudio-backups")
        .join(&session_id);

    fs::create_dir_all(&backup_root).map_err(|e| e.to_string())?;

    // Count existing *.jsonl snapshots to derive the next version number.
    let existing_count = fs::read_dir(&backup_root)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| x == "jsonl")
                .unwrap_or(false)
        })
        .count();

    let version = (existing_count as u32) + 1;
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

/// List all snapshots for a session, newest first.
#[tauri::command]
fn list_backups(session_path: String) -> Result<Vec<BackupVersion>, String> {
    let projects = projects_dir_inner()
        .ok_or_else(|| "Projects directory not found".to_string())?;

    let file_path = Path::new(&session_path);
    let rel = file_path
        .strip_prefix(&projects)
        .map_err(|_| "Session file is not under the projects directory".to_string())?;
    let session_id = sanitize_id(&rel.to_string_lossy());

    let home = dirs::home_dir()
        .ok_or_else(|| "Cannot determine home directory".to_string())?;
    let backup_root = home
        .join(".claude")
        .join(".ccstudio-backups")
        .join(&session_id);

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

/// Return raw contents of a backup file (caller decides what to do with it).
#[tauri::command]
fn restore_backup(backup_path: String) -> Result<String, String> {
    fs::read_to_string(&backup_path).map_err(|e| e.to_string())
}

/// Return the raw JSON string of a saved edit draft for this session, or None if absent.
#[tauri::command]
fn read_edit_draft(session_path: String) -> Result<Option<String>, String> {
    let draft_path = edit_draft_path(&session_path)?;
    if !draft_path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&draft_path).map_err(|e| e.to_string())?;
    Ok(Some(content))
}

/// Persist an edit draft for this session (creates parent dirs as needed).
#[tauri::command]
fn write_edit_draft(session_path: String, content: String) -> Result<(), String> {
    let draft_path = edit_draft_path(&session_path)?;
    if let Some(parent) = draft_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&draft_path, content).map_err(|e| e.to_string())
}

/// Delete the edit draft for this session; ok if already gone.
#[tauri::command]
fn delete_edit_draft(session_path: String) -> Result<(), String> {
    let draft_path = edit_draft_path(&session_path)?;
    if draft_path.exists() {
        fs::remove_file(&draft_path).map_err(|e| e.to_string())?;
    }
    Ok(())
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

/// Best-effort: open a terminal in `cwd` running `claude --resume <session_id>`,
/// honouring the user's persisted terminal preference + extra args (Settings →
/// Terminal). Platform-specific and inherently unreliable (depends on what's
/// installed); callers should always pair this with a copy-to-clipboard
/// fallback. Default (no preference saved, or "auto") reproduces the original
/// auto-detect behavior exactly.
#[tauri::command]
fn resume_in_terminal(cwd: String, session_id: String) -> Result<(), String> {
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

    let config = appconfig::load();
    let extra_args = appconfig::parse_args(&config.terminal_args);
    let auto = appconfig::is_auto(&config.terminal);

    #[cfg(target_os = "macos")]
    {
        let mut claude_cmd = format!("claude --resume {}", session_id);
        for a in &extra_args {
            claude_cmd.push(' ');
            claude_cmd.push_str(a);
        }
        let script = format!(
            "#!/bin/sh\ncd {} && exec {}\n",
            shell_quote(&cwd),
            claude_cmd
        );
        let tmp = std::env::temp_dir().join(format!("ccstudio-resume-{}.command", session_id));
        fs::write(&tmp, script).map_err(|e| e.to_string())?;
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&tmp).map_err(|e| e.to_string())?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&tmp, perms).map_err(|e| e.to_string())?;
        }
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
                cmd.args(*prefix)
                    .arg("claude")
                    .arg("--resume")
                    .arg(&session_id)
                    .args(&extra_args)
                    .current_dir(&cwd);
                if cmd.spawn().is_ok() {
                    return Ok(());
                }
            }
            return Err("No supported terminal emulator found".to_string());
        }

        // Advanced: the preference is a terminal command template, e.g.
        // "gnome-terminal --" or "konsole -e" — the first token is the program,
        // the rest are args that precede the `claude` invocation.
        let tokens = appconfig::parse_args(&config.terminal);
        let Some((program, prefix_args)) = tokens.split_first() else {
            return Err("No terminal command configured".to_string());
        };
        let mut cmd = std::process::Command::new(program);
        cmd.args(prefix_args)
            .arg("claude")
            .arg("--resume")
            .arg(&session_id)
            .args(&extra_args)
            .current_dir(&cwd);
        return cmd
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Could not launch '{program}': {e}"));
    }

    #[cfg(target_os = "windows")]
    {
        if auto {
            std::process::Command::new("cmd")
                .args([
                    "/C",
                    "start",
                    "Claude Resume",
                    "cmd",
                    "/K",
                    "claude",
                    "--resume",
                    &session_id,
                ])
                .args(&extra_args)
                .current_dir(&cwd)
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
        cmd.args(prefix_args)
            .arg("claude")
            .arg("--resume")
            .arg(&session_id)
            .args(&extra_args)
            .current_dir(&cwd);
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

// ---------------------------------------------------------------------------
// App entry point
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Build the search state up front (opens the SQLite cache). If it fails
    // (e.g. no home dir), the app still runs — search is just unavailable.
    let search_state = search::state::SearchState::new(projects_dir_inner(), dirs::home_dir());

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            find_projects_dir,
            home_dir,
            list_sessions,
            read_session,
            read_subagents,
            write_session,
            snapshot,
            list_backups,
            restore_backup,
            read_edit_draft,
            write_edit_draft,
            delete_edit_draft,
            fork_session,
            resume_in_terminal,
            search::state::search,
            search::state::refresh_index,
            search::state::index_status,
            settings::read_claude_settings,
            settings::write_claude_settings,
            appconfig::get_app_config,
            appconfig::set_app_config,
        ]);

    match search_state {
        Ok(state) => {
            builder = builder.manage(state).setup(|app| {
                // Build/refresh the index in the background so launch isn't blocked.
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    let state = handle.state::<search::state::SearchState>();
                    state.indexer().run_index();
                });
                Ok(())
            });
        }
        Err(e) => eprintln!("[search] disabled ({e}); browse/edit still work"),
    }

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
