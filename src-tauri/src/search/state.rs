//! App state + Tauri commands for search: the shared tantivy writer/reader,
//! the sqlite fingerprint connection, the index-status snapshot, the
//! per-keystroke cancellation counter, and the `search` / `refresh_index` /
//! `index_status` commands.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use serde::Serialize;
use tantivy::{Index, IndexReader, IndexWriter};
use tauri::ipc::Channel;
use tauri::State;

use super::db;
use super::index::{self, SearchSchema, WRITER_HEAP_BYTES};
use super::query::{self, SearchFilters, SearchHit, SearchSummary};

/// Check the cancellation flag every this many emitted hits during the cold
/// tier (cheap atomic load; no need to hit it on literally every block).
const CANCEL_CHECK_EVERY: usize = 64;

/// Result-set cap used when the caller passes no explicit `limit`. Unlike the
/// old regex scan (which could stream every match unbounded), tantivy's
/// `TopDocs` collector needs a concrete top-k — this app's UI paginates in
/// pages of 100 anyway, so an unbounded scan was never actually surfaced.
const DEFAULT_LIMIT: usize = 500;

/// A cheap snapshot for a subtle "indexing N/M…" indicator.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    pub total_sessions: usize,
    pub indexed_sessions: usize,
    pub building: bool,
}

/// Owns the tantivy writer/reader, the sqlite fingerprint connection, and
/// index status. Held behind an `Arc` so the blocking index work can be moved
/// onto a worker thread.
pub struct Indexer {
    projects_dir: Option<PathBuf>,
    home: Option<PathBuf>,
    /// SQLite permits one writer at a time — this Mutex enforces exactly that
    /// for the `session_files` fingerprint table.
    conn: Mutex<Connection>,
    tantivy_index: Index,
    /// tantivy also permits exactly one writer at a time.
    tantivy_writer: Mutex<IndexWriter>,
    tantivy_reader: IndexReader,
    schema: SearchSchema,
    status: Mutex<IndexStatus>,
}

impl Indexer {
    fn set_building(&self, building: bool) {
        if let Ok(mut s) = self.status.lock() {
            s.building = building;
        }
    }

    /// Recompute total (on disk) vs indexed (in cache) counts. Must be called
    /// while neither lock below is held for long (it briefly takes the sqlite
    /// lock).
    fn update_counts(&self) {
        let total = self
            .projects_dir
            .as_ref()
            .map(|p| index::session_files(p).len())
            .unwrap_or(0);
        let indexed = self
            .conn
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

    /// Full build on an empty cache, incremental sweep otherwise. Also the
    /// path a fresh engine-version rebuild takes (see `db::ensure_engine_version`
    /// clearing `session_files` so every file looks new).
    pub fn run_index(&self) {
        let Some(projects) = self.projects_dir.clone() else {
            return;
        };
        self.set_building(true);
        // Compute counts up front too, not just after: `total_sessions`
        // otherwise stayed at its zero default for the whole build, and
        // combined with the frontend's `total_sessions > 0` gate, the
        // "indexing N/M…" indicator never appeared during exactly the
        // first-launch window it matters most (found in the issue #5 Gate-2
        // audit).
        self.update_counts();
        {
            let (Ok(mut conn), Ok(mut writer)) = (self.conn.lock(), self.tantivy_writer.lock())
            else {
                self.set_building(false);
                return;
            };
            let has_rows: i64 = conn
                .query_row("SELECT COUNT(*) FROM session_files", [], |r| r.get(0))
                .unwrap_or(0);
            let result: Result<(), String> = if has_rows == 0 {
                index::build_index_parallel(&mut conn, &mut writer, &self.schema, &projects, self.home.as_deref())
                    .map(|_| ())
            } else {
                index::sweep_index(&mut conn, &mut writer, &self.schema, &projects, self.home.as_deref())
                    .map(|_| ())
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
        if let (Ok(conn), Ok(mut writer)) = (self.conn.lock(), self.tantivy_writer.lock()) {
            let _ = index::index_file(&conn, &mut writer, &self.schema, Path::new(session_path), self.home.as_deref());
        }
        self.update_counts();
    }

    /// Drop one file's docs (e.g. after our own delete). No caller yet — the
    /// app has no delete-session command — but kept as the deletion
    /// counterpart to `reindex_one`.
    #[allow(dead_code)]
    pub fn remove_one(&self, session_path: &str) {
        if let (Ok(conn), Ok(mut writer)) = (self.conn.lock(), self.tantivy_writer.lock()) {
            let _ = index::remove_from_index(&conn, &mut writer, &self.schema, session_path);
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
    /// Open the sqlite fingerprint DB + the tantivy index and resolve dirs.
    /// Runs the engine-version check up front, which does a one-time full
    /// wipe-and-rebuild trigger if the on-disk engine doesn't match (see
    /// `db::ensure_engine_version`). Fails only if the DB/index can't be
    /// opened (e.g. no home dir) — the caller decides whether that's fatal.
    pub fn new(projects_dir: Option<PathBuf>, home: Option<PathBuf>) -> Result<Self, String> {
        let conn = db::open_db()?;
        let tantivy_dir = db::tantivy_index_dir()?;
        let effective_version = format!("{}-{}", db::ENGINE_VERSION, index::schema_fingerprint());
        db::ensure_engine_version(&conn, &tantivy_dir, &effective_version)?;

        let schema = SearchSchema::build();
        let tantivy_index = index::open_index(&tantivy_dir, &schema.schema)?;
        let tantivy_writer = tantivy_index
            .writer(WRITER_HEAP_BYTES)
            .map_err(|e| e.to_string())?;
        let tantivy_reader = tantivy_index.reader().map_err(|e| e.to_string())?;

        Ok(Self {
            indexer: Arc::new(Indexer {
                projects_dir,
                home,
                conn: Mutex::new(conn),
                tantivy_index,
                tantivy_writer: Mutex::new(tantivy_writer),
                tantivy_reader,
                schema,
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

/// Does a cold-path (not-yet-indexed) block pass the source/date/tool-name
/// filters? (Project and session path are filtered per-file, not here — see
/// the cold-tier loop below.) Mirrors the warm-tier filter semantics built
/// into the tantivy query itself.
fn passes_cold_filters(source: &str, text: &str, ts: Option<i64>, filters: &SearchFilters) -> bool {
    if let Some(tool_name) = &filters.tool_name {
        if source != "tool_use" {
            return false;
        }
        if text != tool_name.as_str() && !text.starts_with(&format!("{tool_name}\n")) {
            return false;
        }
    } else if !filters.sources.is_empty() && !filters.sources.iter().any(|s| s == source) {
        return false;
    }
    if let Some(from) = filters.from {
        if !matches!(ts, Some(t) if t >= from) {
            return false;
        }
    }
    if let Some(to) = filters.to {
        if !matches!(ts, Some(t) if t <= to) {
            return false;
        }
    }
    true
}

/// Search. Pushes each [`SearchHit`] onto `on_hit` as it's found (warm hits
/// first, already relevance-sorted by tantivy's `TopDocs`, then any cold-tier
/// hits appended) and returns a summary. A newer `search_id` supersedes this
/// call mid-scan.
///
/// Two tiers (correctness never waits on the index):
///  - **warm**: query the tantivy index (fast, fuzzy/relevance ranked);
///  - **cold**: for sessions not yet indexed (e.g. during the first-launch
///    build, or right after an engine-version rebuild clears the cache),
///    a simpler substring match directly over freshly-parsed JSONL so results
///    are still complete. The background indexer fills the index, so cold
///    work shrinks to nothing once it's warm.
#[tauri::command]
pub async fn search(
    state: State<'_, SearchState>,
    query: String,
    filters: SearchFilters,
    search_id: u64,
    limit: Option<usize>,
    on_hit: Channel<SearchHit>,
) -> Result<SearchSummary, String> {
    // Register this as the current search before doing any work.
    let generation = state.generation.clone();
    generation.store(search_id, Ordering::SeqCst);
    let indexer = state.indexer();
    let limit = limit.unwrap_or(DEFAULT_LIMIT);

    tauri::async_runtime::spawn_blocking(move || {
        let mut summary = SearchSummary::default();
        let cancelled = || generation.load(Ordering::SeqCst) != search_id;

        let tokens = query::tokenize(&indexer.tantivy_index, &query)?;
        if tokens.is_empty() {
            return Ok(summary);
        }
        let Some(built_query) = query::build_query(&indexer.tantivy_index, &indexer.schema, &query, &filters)?
        else {
            return Ok(summary);
        };

        // --- Warm tier: query the tantivy index. ---
        let searcher = indexer.tantivy_reader.searcher();
        let (total, hits) = query::search_warm(&searcher, &indexer.schema, &built_query, &tokens, limit)?;
        summary.scanned = total;
        for hit in &hits {
            if cancelled() {
                summary.cancelled = true;
                break;
            }
            let _ = on_hit.send(hit.clone());
            summary.hits += 1;
        }
        summary.truncated = !summary.cancelled && total > summary.hits;
        if summary.cancelled || summary.truncated || summary.hits >= limit {
            return Ok(summary);
        }

        // --- Cold tier: sessions not yet in the index. ---
        let status = indexer.status_snapshot();
        let fully_warm = !status.building
            && status.total_sessions > 0
            && status.indexed_sessions >= status.total_sessions;
        if fully_warm {
            return Ok(summary);
        }

        let read_conn = db::open_read()?;
        let cached = cached_session_paths(&read_conn)?;
        for path in indexer.disk_session_files() {
            if summary.hits % CANCEL_CHECK_EVERY == 0 && cancelled() {
                summary.cancelled = true;
                break;
            }
            let remaining = limit.saturating_sub(summary.hits);
            if remaining == 0 {
                summary.truncated = true;
                break;
            }
            let sp = path.to_string_lossy().to_string();
            if cached.contains(&sp) {
                continue; // already covered by the warm tier
            }
            if let Some(session_path) = &filters.session_path {
                if &sp != session_path {
                    continue;
                }
            }
            let Some((project, blocks)) = index::extract_file(&path, indexer.home()) else {
                continue;
            };
            if !filters.projects.is_empty() && !filters.projects.iter().any(|p| *p == project) {
                continue;
            }
            for b in &blocks {
                if summary.hits >= limit {
                    summary.truncated = true;
                    break;
                }
                if !passes_cold_filters(&b.source, &b.text, b.ts, &filters) {
                    continue;
                }
                if let Some((snippet, match_ranges, matched)) = query::cold_match(&b.text, &tokens) {
                    let _ = on_hit.send(SearchHit {
                        session_path: sp.clone(),
                        project: project.clone(),
                        ts: b.ts,
                        line_no: b.line_no,
                        block_no: b.block_no,
                        uuid: b.uuid.clone(),
                        source: b.source.clone(),
                        snippet,
                        match_ranges,
                        // Count of matched tokens, not a real BM25 score —
                        // see `cold_match`'s doc comment. A coarse but
                        // meaningful proxy, rather than a flat 0.0 that would
                        // make every cold hit indistinguishable.
                        score: matched as f32,
                    });
                    summary.hits += 1;
                    summary.scanned += 1;
                }
            }
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
