use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{Manager, State};

// Search: SQLite-backed extracted-text cache (built up over milestones 1–12).
mod search;

// CC Deck's own preference store (terminal launcher choice + resume-launch command).
mod appconfig;

// ccdeck's own data root (~/.ccdeck) + the startup migration that moves
// legacy ccdeck-owned state (backups, config) out of ~/.claude.
mod datadir;

// ---------------------------------------------------------------------------
// Return-type structs (must match the JS contract in ARCHITECTURE.md)
// ---------------------------------------------------------------------------

/// Tier-1 browse metadata: everything obtainable from a directory walk plus a
/// single `stat` per file, with **no file-content read**. `list_sessions`
/// returns these so the browse list can paint immediately in recency order
/// (`mtime`); the expensive content-derived fields arrive afterward, streamed
/// per file by [`enrich_sessions`], so a large history never blocks first paint.
#[derive(Serialize)]
pub struct SessionStub {
    pub id: String,          // stable id = relative path from projects dir
    pub path: String,        // absolute path to the .jsonl
    pub project_raw: String, // the encoded project dir name
    pub mtime: u64,          // unix seconds (file modified time) — the recency sort key
    pub size: u64,           // bytes
}

/// Tier-2 browse metadata: the content-derived fields for one session, streamed
/// to the frontend by [`enrich_sessions`] as each file is scanned. Keyed by
/// `path` — the frontend patches the matching [`SessionStub`] in place. Field
/// names stay snake_case to match what the browse view already consumes.
///
/// When `cleaned` is true the file was an empty/untitled/stale junk session and
/// has just been deleted (see [`is_cleanup_eligible`]); every other field is
/// then meaningless and the frontend drops the stub rather than patching it.
#[derive(Serialize)]
pub struct SessionEnrichment {
    pub path: String,            // key: which stub this patches
    pub cleaned: bool,           // true = file was just deleted as junk; drop the stub
    pub preview: Vec<String>,    // first ≤50 lines — the JS side runs extractMeta on these
    pub line_count: u64,         // non-empty lines
    pub user_count: u64,         // lines whose type == "user"
    pub assistant_count: u64,    // lines whose type == "assistant"
    pub subagent_count: u64,     // count of subagents/agent-*.jsonl next to the session file
    pub models: Vec<String>,     // distinct message.model values, first-seen order
    pub first_ts: String,        // first timestamp value seen ("" if none)
    pub last_ts: String,         // last timestamp value seen ("" if none)
    pub cwd: String,             // first-seen "cwd" value ("" if none) — the real project path
    pub custom_title: String,    // last-seen "customTitle" value ("" if none), scanned across the
                                  // whole file — a real Claude Code rename, wherever it occurs.
}

impl SessionEnrichment {
    /// A "this file was just cleaned up — drop its stub" signal for `path`.
    fn cleaned(path: String) -> Self {
        Self {
            path,
            cleaned: true,
            preview: Vec::new(),
            line_count: 0,
            user_count: 0,
            assistant_count: 0,
            subagent_count: 0,
            models: Vec::new(),
            first_ts: String::new(),
            last_ts: String::new(),
            cwd: String::new(),
            custom_title: String::new(),
        }
    }
}

/// Streaming summary returned when [`enrich_sessions`] finishes (or a newer
/// call supersedes it).
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EnrichSummary {
    pub enriched: u64,
    pub cleaned: u64,
    pub cancelled: bool,
}

/// Per-app browse state: a generation counter so a newer `enrich_sessions`
/// call (a remount) supersedes an in-flight one — mirrors `SearchState`'s
/// cancellation counter. Behind an `Arc` so the blocking walk keeps it after
/// the command future returns.
#[derive(Default)]
pub struct BrowseState {
    generation: Arc<AtomicU64>,
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

/// Walk every immediate sub-directory of the projects dir and collect the path
/// of each session `*.jsonl` that is NOT `agent-*.jsonl`. Skips the `subagents`
/// and `tool-results` dirs. This is the single directory walk shared by
/// [`list_sessions`] (which then just `stat`s each) and [`enrich_sessions`]
/// (which reads + scans each) — one skip/filter rule, not two copies that can
/// drift apart (they did, historically: `list_sessions` and the old
/// `cleanup_empty_sessions` each hand-rolled the same walk).
fn collect_session_files(projects: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(top_entries) = fs::read_dir(projects) else {
        return out;
    };
    for top in top_entries.flatten() {
        let project_path = top.path();
        if !project_path.is_dir() {
            continue;
        }
        let dir_name = match project_path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if dir_name == "subagents" || dir_name == "tool-results" {
            continue;
        }
        let Ok(inner) = fs::read_dir(&project_path) else {
            continue;
        };
        for jentry in inner.flatten() {
            let file_path = jentry.path();
            let Some(fname) = file_path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !fname.ends_with(".jsonl") || fname.starts_with("agent-") {
                continue;
            }
            out.push(file_path);
        }
    }
    out
}

/// Count subagent `agent-*.jsonl` files in the `subagents/` dir sibling to a
/// session file (0 if there is none). Extracted from the old inline
/// `list_sessions` logic — it needs a `read_dir`, so it now belongs to the
/// tier-2 enrichment rather than the cheap first-paint scan.
fn count_subagents(session_path: &Path) -> u64 {
    let parent = session_path.parent().unwrap_or(Path::new(""));
    let subagents_dir = parent.join("subagents");
    if !subagents_dir.is_dir() {
        return 0;
    }
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
}

/// Whether an empty/untitled/stale session file should be auto-deleted during
/// enrichment. Wraps [`is_cleanup_eligible`] with the mtime guard the old
/// `cleanup_empty_sessions` enforced: a file whose mtime couldn't be read
/// (`None`) is **never** deleted — we never remove something we can't prove is
/// stale. Pure, so the guard stays unit-testable without the filesystem.
fn should_cleanup(
    stats: &SessionScanStats,
    mtime: Option<u64>,
    now_secs: u64,
    recency_window_secs: u64,
) -> bool {
    match mtime {
        Some(m) => is_cleanup_eligible(stats, m, now_secs, recency_window_secs),
        None => false,
    }
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

/// Tier-1 browse scan: one directory walk plus a single `stat` per session
/// file, **no content reads**, so the browse list paints immediately. The
/// content-derived fields (turn counts, models, title, timestamps) arrive
/// afterward, streamed per file by [`enrich_sessions`]. Returned in filesystem
/// order; the frontend sorts by `mtime` (recency).
#[tauri::command]
fn list_sessions() -> Result<Vec<SessionStub>, String> {
    let projects =
        projects_dir_inner().ok_or_else(|| "Projects directory not found".to_string())?;

    let mut stubs: Vec<SessionStub> = Vec::new();
    for file_path in collect_session_files(&projects) {
        let Ok(meta) = fs::metadata(&file_path) else {
            continue;
        };
        // The encoded project dir name is the session file's parent dir name.
        let project_raw = file_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        // Relative path from projects root — this is the stable session id.
        let id = file_path
            .strip_prefix(&projects)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| {
                file_path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            });

        stubs.push(SessionStub {
            id,
            path: file_path.to_string_lossy().into_owned(),
            project_raw,
            mtime: meta.modified().map(unix_secs).unwrap_or(0),
            size: meta.len(),
        });
    }

    Ok(stubs)
}

/// Tier-2 browse scan: stream the content-derived metadata for every session as
/// each file is read, and fold the junk-cleanup pass into the *same* walk — one
/// read per file does both jobs, where `list_sessions` + the old
/// `cleanup_empty_sessions` used to walk and full-read the whole corpus twice
/// before the list even loaded. Files are processed newest-first (mtime desc)
/// so the cards the user is looking at fill in first.
///
/// Cleanup: an empty, untitled, stale session (see [`is_cleanup_eligible`], its
/// 15-minute recency guard intact) is deleted and streamed as a `cleaned`
/// signal so the frontend drops its stub; every other file is streamed as a
/// full enrichment.
///
/// A newer `enrich_id` (a remount) supersedes an in-flight walk. Navigating
/// away does NOT cancel it, so a normal app session still completes one full
/// cleanup pass — the deletion is off the first-paint path but not tied to
/// scroll position (see project_docs/roadmap.md for the behavior-change note).
#[tauri::command]
async fn enrich_sessions(
    state: State<'_, BrowseState>,
    enrich_id: u64,
    on_meta: Channel<SessionEnrichment>,
) -> Result<EnrichSummary, String> {
    let generation = state.generation.clone();
    generation.store(enrich_id, Ordering::SeqCst);

    tauri::async_runtime::spawn_blocking(move || {
        let projects =
            projects_dir_inner().ok_or_else(|| "Projects directory not found".to_string())?;
        let now = unix_secs(SystemTime::now());
        let cancelled = || generation.load(Ordering::SeqCst) != enrich_id;
        let mut summary = EnrichSummary::default();

        // Stat once up front so we can process newest-first — the top-of-list
        // (most recent) cards then enrich before the user scrolls past them.
        // A file whose mtime can't be read sorts to the bottom AND is never
        // eligible for cleanup below (see `should_cleanup`).
        let mut files: Vec<(PathBuf, Option<u64>)> = collect_session_files(&projects)
            .into_iter()
            .map(|p| {
                let mtime = fs::metadata(&p)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .map(unix_secs);
                (p, mtime)
            })
            .collect();
        files.sort_by_key(|(_, mtime)| std::cmp::Reverse(mtime.unwrap_or(0)));

        for (file_path, mtime) in files {
            if cancelled() {
                summary.cancelled = true;
                break;
            }

            // A file that exists but can't be read (invalid UTF-8, a transient
            // I/O error) must NOT be treated as empty here: `unwrap_or_default`
            // would hand an empty string to the scan, and a stale + untitled
            // empty scan is cleanup-eligible — so an unreadable but real session
            // file would be silently DELETED below. Skip it instead; it stays an
            // un-enriched stub, which is the safe outcome. A genuinely empty
            // 0-byte file still reads as `Ok("")` and remains eligible.
            let Ok(content) = fs::read_to_string(&file_path) else {
                continue;
            };
            let stats = scan_session_lines(&content);
            let path = file_path.to_string_lossy().into_owned();

            // Cleanup fold-in: delete genuinely-empty, untitled, cold files and
            // tell the frontend to drop the stub, instead of enriching them.
            if should_cleanup(&stats, mtime, now, CLEANUP_RECENCY_WINDOW_SECS) {
                if fs::remove_file(&file_path).is_ok() {
                    summary.cleaned += 1;
                    let _ = on_meta.send(SessionEnrichment::cleaned(path));
                }
                continue;
            }

            let preview: Vec<String> = content.lines().take(50).map(|l| l.to_string()).collect();
            let subagent_count = count_subagents(&file_path);
            let SessionScanStats {
                line_count,
                user_count,
                assistant_count,
                models,
                first_ts,
                last_ts,
                cwd,
                custom_title,
            } = stats;

            let _ = on_meta.send(SessionEnrichment {
                path,
                cleaned: false,
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
            summary.enriched += 1;
        }

        Ok(summary)
    })
    .await
    .map_err(|e| e.to_string())?
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
/// only a single backup slot, this returns at most one entry.
///
/// Test-only since issue #38 retired the in-app restore surface (the
/// `list_backups`/`restore_backup` commands are gone). The pre-save snapshot
/// itself stays — this helper is how [`snapshot_at`]'s tests read a snapshot
/// back to assert the single-slot behaviour. See [`snapshot_at`] for why it is
/// parameterized on `projects`/`backups_root`.
#[cfg(test)]
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
            let (version_str, ts_str) = stem.split_once('-')?;
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
    versions.sort_by_key(|b| std::cmp::Reverse(b.timestamp));

    Ok(versions)
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

    /// The `should_cleanup` wrapper enforces the old `cleanup_empty_sessions`
    /// mtime guard now that cleanup runs inside enrichment: a file whose mtime
    /// couldn't be read (`None`) is NEVER deleted, even if it is otherwise a
    /// stale, empty, untitled junk file. We never remove what we can't prove
    /// is stale.
    #[test]
    fn should_cleanup_never_deletes_when_mtime_unknown() {
        let empty = SessionScanStats { line_count: 3, ..Default::default() };
        // With a known, stale mtime it IS eligible…
        assert!(should_cleanup(&empty, Some(NOW - WINDOW - 60), NOW, WINDOW));
        // …but with an unreadable mtime it must be spared.
        assert!(!should_cleanup(&empty, None, NOW, WINDOW));
        // And real content is spared regardless of the mtime being known.
        let with_user = SessionScanStats { user_count: 1, ..Default::default() };
        assert!(!should_cleanup(&with_user, Some(NOW - WINDOW - 60), NOW, WINDOW));
    }
}

#[cfg(test)]
mod browse_scan_tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "ccdeck-browse-test-{tag}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// The shared walker returns only real session `*.jsonl` files: it skips
    /// `agent-*.jsonl`, non-jsonl files, the `subagents`/`tool-results`
    /// top-level dirs, and never descends into a project's own `subagents/`.
    #[test]
    fn collect_session_files_applies_the_skip_rules() {
        let projects = scratch("collect");

        let proj = projects.join("-home-user-app");
        fs::create_dir_all(proj.join("subagents")).unwrap();
        fs::write(proj.join("sess-1.jsonl"), "{}\n").unwrap(); // kept
        fs::write(proj.join("agent-abc.jsonl"), "{}\n").unwrap(); // skip: agent-*
        fs::write(proj.join("notes.txt"), "x").unwrap(); // skip: not .jsonl
        fs::write(proj.join("subagents").join("agent-x.jsonl"), "{}\n").unwrap(); // not descended

        // A whole top-level dir named `subagents` is skipped wholesale.
        let special = projects.join("subagents");
        fs::create_dir_all(&special).unwrap();
        fs::write(special.join("sess-2.jsonl"), "{}\n").unwrap();

        let names: Vec<String> = collect_session_files(&projects)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["sess-1.jsonl".to_string()]);

        fs::remove_dir_all(&projects).unwrap();
    }

    /// `count_subagents` counts only `agent-*.jsonl` files in the sibling
    /// `subagents/` dir — not `.meta.json` sidecars — and is 0 with no dir.
    #[test]
    fn count_subagents_counts_agent_jsonl_only() {
        let proj = scratch("subagents");
        let sess = proj.join("sess.jsonl");
        fs::write(&sess, "{}").unwrap();

        // No subagents dir yet → 0.
        assert_eq!(count_subagents(&sess), 0);

        let sad = proj.join("subagents");
        fs::create_dir_all(&sad).unwrap();
        fs::write(sad.join("agent-1.jsonl"), "{}").unwrap();
        fs::write(sad.join("agent-2.jsonl"), "{}").unwrap();
        fs::write(sad.join("agent-1.meta.json"), "{}").unwrap(); // not counted
        assert_eq!(count_subagents(&sess), 2);

        fs::remove_dir_all(&proj).unwrap();
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
        .manage(BrowseState::default())
        .invoke_handler(tauri::generate_handler![
            find_projects_dir,
            home_dir,
            list_sessions,
            enrich_sessions,
            read_session,
            write_session,
            snapshot,
            fork_session,
            search::state::search,
            search::state::refresh_index,
            search::state::index_status,
            appconfig::get_app_config,
            appconfig::set_app_config,
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
        Ok(())
    });

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
