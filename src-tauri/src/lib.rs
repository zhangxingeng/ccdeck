use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

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
fn write_session(path: String, content: String) -> Result<(), String> {
    fs::write(&path, content).map_err(|e| e.to_string())
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

// ---------------------------------------------------------------------------
// App entry point
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
