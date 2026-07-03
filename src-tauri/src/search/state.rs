//! App state + Tauri commands for search: the shared writer connection, the
//! index-status snapshot, the per-keystroke cancellation counter, and the
//! `search` / `refresh_index` / `index_status` commands.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;

use super::db;
use super::index;
use super::query::{self, SearchFilters, SearchHit, SearchOpts, SearchSummary};

/// A cheap snapshot for a subtle "indexing N/M…" indicator.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    pub total_sessions: usize,
    pub indexed_sessions: usize,
    pub building: bool,
}

/// Owns the single writer connection + index status. Held behind an `Arc` so the
/// blocking index work can be moved onto a worker thread.
pub struct Indexer {
    projects_dir: Option<PathBuf>,
    home: Option<PathBuf>,
    /// SQLite permits one writer at a time — this Mutex enforces exactly that.
    writer: Mutex<Connection>,
    status: Mutex<IndexStatus>,
}

impl Indexer {
    fn set_building(&self, building: bool) {
        if let Ok(mut s) = self.status.lock() {
            s.building = building;
        }
    }

    /// Recompute total (on disk) vs indexed (in cache) counts. Must be called
    /// while the writer lock is NOT held (it briefly takes it).
    fn update_counts(&self) {
        let total = self
            .projects_dir
            .as_ref()
            .map(|p| index::session_files(p).len())
            .unwrap_or(0);
        let indexed = self
            .writer
            .lock()
            .ok()
            .and_then(|c| {
                c.query_row("SELECT COUNT(*) FROM session_files", [], |r| r.get::<_, i64>(0))
                    .ok()
            })
            .unwrap_or(0) as usize;
        if let Ok(mut s) = self.status.lock() {
            s.total_sessions = total;
            s.indexed_sessions = indexed;
        }
    }

    /// Full build on an empty cache, incremental sweep otherwise.
    pub fn run_index(&self) {
        let Some(projects) = self.projects_dir.clone() else {
            return;
        };
        self.set_building(true);
        {
            let Ok(mut conn) = self.writer.lock() else {
                self.set_building(false);
                return;
            };
            let has_rows: i64 = conn
                .query_row("SELECT COUNT(*) FROM session_files", [], |r| r.get(0))
                .unwrap_or(0);
            let result: Result<(), String> = if has_rows == 0 {
                index::build_index_parallel(&mut conn, &projects, self.home.as_deref()).map(|_| ())
            } else {
                index::sweep_index(&mut conn, &projects, self.home.as_deref()).map(|_| ())
            };
            if let Err(e) = result {
                eprintln!("[search] index error: {e}");
            }
        }
        self.update_counts();
        self.set_building(false);
    }

    /// Eager reindex of one file after our own Save/Restore changes it.
    pub fn reindex_one(&self, session_path: &str) {
        if let Ok(mut conn) = self.writer.lock() {
            let _ = index::index_file(&mut conn, Path::new(session_path), self.home.as_deref());
        }
        self.update_counts();
    }

    /// Drop one file's rows (e.g. after our own delete). No caller yet — the app
    /// has no delete-session command — but kept as the deletion counterpart to
    /// `reindex_one`.
    #[allow(dead_code)]
    pub fn remove_one(&self, session_path: &str) {
        if let Ok(conn) = self.writer.lock() {
            let _ = index::remove_from_index(&conn, session_path);
        }
        self.update_counts();
    }

    pub fn status_snapshot(&self) -> IndexStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn home(&self) -> Option<&Path> {
        self.home.as_deref()
    }

    /// Every session file currently on disk (used to find un-cached sessions).
    pub fn disk_session_files(&self) -> Vec<PathBuf> {
        self.projects_dir
            .as_ref()
            .map(|p| index::session_files(p))
            .unwrap_or_default()
    }
}

/// Tauri-managed search state.
pub struct SearchState {
    indexer: Arc<Indexer>,
    /// Latest search id; a running scan stops when this no longer equals its own.
    generation: Arc<AtomicU64>,
}

impl SearchState {
    /// Open the writer connection and resolve dirs. Fails only if the DB can't
    /// be opened (e.g. no home dir) — the caller decides whether that's fatal.
    pub fn new(projects_dir: Option<PathBuf>, home: Option<PathBuf>) -> Result<Self, String> {
        let writer = db::open_db()?;
        Ok(Self {
            indexer: Arc::new(Indexer {
                projects_dir,
                home,
                writer: Mutex::new(writer),
                status: Mutex::new(IndexStatus::default()),
            }),
            generation: Arc::new(AtomicU64::new(0)),
        })
    }

    pub fn indexer(&self) -> Arc<Indexer> {
        self.indexer.clone()
    }
}

/// Read the set of session paths currently present in the cache.
fn cached_session_paths(conn: &Connection) -> Result<HashSet<String>, String> {
    let mut stmt = conn
        .prepare("SELECT session_path FROM session_files")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let mut set = HashSet::new();
    for r in rows {
        set.insert(r.map_err(|e| e.to_string())?);
    }
    Ok(set)
}

/// Streaming search. Pushes each [`SearchHit`] onto `on_hit` as it's found and
/// returns a summary. A newer `search_id` supersedes this call mid-scan.
///
/// Two tiers (correctness never waits on the index):
///  - **warm**: scan the SQLite cache (fast, no JSON parse);
///  - **cold**: for sessions not yet cached (e.g. during the first-launch build),
///    parse their JSONL directly so results are still complete. The background
///    indexer fills the cache, so cold work shrinks to nothing once it's warm.
#[tauri::command]
pub async fn search(
    state: State<'_, SearchState>,
    query: String,
    opts: SearchOpts,
    filters: SearchFilters,
    search_id: u64,
    on_hit: Channel<SearchHit>,
) -> Result<SearchSummary, String> {
    // Register this as the current search before doing any work.
    let generation = state.generation.clone();
    generation.store(search_id, Ordering::SeqCst);
    let indexer = state.indexer();

    // The scan is blocking (SQLite + regex), so run it off the async runtime.
    tauri::async_runtime::spawn_blocking(move || {
        let mut summary = SearchSummary::default();
        if query.is_empty() {
            return Ok(summary);
        }
        // Compile once; a bad regex surfaces here as an error hint.
        let re = query::build_regex(&query, &opts)?;
        let cancelled = || generation.load(Ordering::SeqCst) != search_id;

        // Fresh read connection: WAL lets us read while the indexer is writing,
        // and this avoids the write lock that schema-creation would take.
        let conn = db::open_read()?;

        // --- Warm tier: scan the cache. ---
        let warm = query::search_streaming(
            &conn,
            &re,
            &filters,
            |hit| {
                let _ = on_hit.send(hit);
            },
            &cancelled,
        )?;
        summary.hits += warm.hits;
        summary.scanned += warm.scanned;
        summary.cancelled = warm.cancelled;
        if summary.cancelled {
            return Ok(summary);
        }

        // --- Cold tier: sessions not yet in the cache. ---
        // Skip the directory scan entirely once the cache is fully warm.
        let status = indexer.status_snapshot();
        let fully_warm = !status.building
            && status.total_sessions > 0
            && status.indexed_sessions >= status.total_sessions;
        if fully_warm {
            return Ok(summary);
        }

        let cached = cached_session_paths(&conn)?;
        for path in indexer.disk_session_files() {
            if cancelled() {
                summary.cancelled = true;
                break;
            }
            let sp = path.to_string_lossy().to_string();
            if cached.contains(&sp) {
                continue; // already covered by the warm tier
            }
            let Some((project, blocks)) = index::extract_file(&path, indexer.home()) else {
                continue;
            };
            if !filters.projects.is_empty() && !filters.projects.iter().any(|p| *p == project) {
                continue;
            }
            summary.hits += query::scan_blocks(&sp, &project, &blocks, &re, &filters, &mut |hit| {
                let _ = on_hit.send(hit);
            });
        }

        Ok(summary)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Kick a (re)index sweep. Safe to call often — a sweep no-ops on fresh files.
#[tauri::command]
pub async fn refresh_index(state: State<'_, SearchState>) -> Result<IndexStatus, String> {
    let indexer = state.indexer();
    tauri::async_runtime::spawn_blocking(move || {
        indexer.run_index();
        indexer.status_snapshot()
    })
    .await
    .map_err(|e| e.to_string())
}

/// Cheap current index status for the UI indicator.
#[tauri::command]
pub fn index_status(state: State<'_, SearchState>) -> IndexStatus {
    state.indexer().status_snapshot()
}
