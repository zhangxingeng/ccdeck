//! Managed state, hybrid fusion, and the Tauri commands for the Prompt
//! Library. Command surface per the contract: `list_pieces` / `save_piece` /
//! `delete_piece` / `piece_load_errors` / `list_projects` / `save_project` /
//! `delete_project` / `match_pieces` / `embed_status` / `embed_download` /
//! `set_embed_enabled` — all async, `Result<T, String>`, snake_case.
//!
//! Callers never know which engine ran: `match_pieces` fuses lexical and
//! (when ready + enabled) semantic scores internally and only tags each hit's
//! `source` for observability.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use fastembed::TextEmbedding;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;

use super::embed::{self, DownloadProgress};
use super::lexical;
use super::projects::{self, Project, ProjectInput};
use super::store::{self, Piece, PieceInput, Scope};

/// If one query embedding takes longer than this, the machine is too slow for
/// per-keystroke inference (the UI debounce budget) — degrade to lexical-only
/// for subsequent queries instead of blocking the panel, until the user
/// toggles semantic match off/on (which also reloads the model).
const INFERENCE_BUDGET_MS: u128 = 250;

/// How many stale pieces one match call will embed before answering. Keeps a
/// huge hand-imported corpus from freezing a single keystroke — the cache
/// warms over a few queries; the bulk pass runs at download time anyway.
const EMBED_TOPUP_PER_QUERY: usize = 32;

/// Lexical weight in the normalized-score blend (semantic gets the rest).
/// Lexical leads: on a curated corpus the user's own words beat inferred
/// similarity more often than not; semantic exists to catch phrasings the
/// keywords missed.
const LEX_BLEND: f32 = 0.6;

/// A semantic-only candidate below this cosine is noise, not a hit — without
/// a floor, low-similarity vectors pad the panel with head-scratchers.
const SEM_MIN_COSINE: f32 = 0.35;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MatchHit {
    pub id: String,
    pub score: f32,
    pub source: String, // "lexical" | "semantic" | "hybrid"
}

#[derive(Debug, Clone, Serialize)]
pub struct EmbedStatus {
    pub state: String, // "off" | "not_downloaded" | "downloading" | "ready" | "error"
    pub model_id: String,
    pub model_size_mb: u32,
    /// Disclosed alongside the model size so the pre-download requirements
    /// note covers the TOTAL download (Gate-1 ruling), not just the model.
    pub runtime_size_mb: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Runtime-only embedding state. Everything durable lives elsewhere (files on
/// disk, the appconfig toggle) — so this needs no persistence and commands
/// resolve the data root per call, matching the app's existing style.
#[derive(Default)]
pub struct PromptsInner {
    embedder: Mutex<Option<TextEmbedding>>,
    downloading: AtomicBool,
    /// Last embedder failure, surfaced via embed_status; cleared on toggle.
    embed_error: Mutex<Option<String>>,
    /// Set when inference blew the budget — sticky lexical-only degradation.
    slow: AtomicBool,
}

pub struct PromptsState {
    inner: Arc<PromptsInner>,
}

impl PromptsState {
    pub fn new() -> Self {
        Self { inner: Arc::new(PromptsInner::default()) }
    }
}

fn set_error(inner: &PromptsInner, msg: String) {
    if let Ok(mut e) = inner.embed_error.lock() {
        *e = Some(msg);
    }
}

// ---------------------------------------------------------------------------
// Store commands
// ---------------------------------------------------------------------------

/// The roster's project ids, for dangling-scope validation. `None` when the
/// roster itself can't be read — piece loading then suspends id validation
/// instead of falsely degrading every project piece; the roster failure is
/// surfaced loudly by `list_projects`, not here.
fn roster_ids() -> Option<HashSet<String>> {
    let root = crate::datadir::data_root().ok()?;
    let projects = projects::load_projects(&root).ok()?;
    Some(projects.into_iter().map(|p| p.id).collect())
}

#[tauri::command]
pub async fn list_pieces() -> Result<Vec<Piece>, String> {
    store::load_pieces(&store::prompts_dir()?, roster_ids().as_ref())
}

#[tauri::command]
pub async fn save_piece(piece: PieceInput) -> Result<Piece, String> {
    store::save_piece(piece)
}

#[tauri::command]
pub async fn delete_piece(id: String) -> Result<(), String> {
    store::delete_piece_at(&store::prompts_dir()?, &id)
}

/// Files the loader had to skip or degrade (broken JSON, shadowed duplicate
/// id, legacy scope) — plus the roster-repair notice (contract § Store
/// robustness: a projects.json repair that SUCCEEDS may have dropped
/// truncated records, so it surfaces here as the same amber warning). Shown
/// next to the library so a hand-edit typo never reads as a silently
/// vanished piece. Runs its own fresh scan, so it always reflects the
/// current on-disk state.
#[tauri::command]
pub async fn piece_load_errors() -> Result<Vec<store::LoadError>, String> {
    let (_, mut errors) = store::scan_pieces(&store::prompts_dir()?, roster_ids().as_ref())?;
    errors.extend(projects::roster_repair_notice(&crate::datadir::data_root()?));
    Ok(errors)
}

// ---------------------------------------------------------------------------
// Project roster commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_projects() -> Result<Vec<Project>, String> {
    projects::load_projects(&crate::datadir::data_root()?)
}

#[tauri::command]
pub async fn save_project(project: ProjectInput) -> Result<Project, String> {
    projects::save_project_at(&crate::datadir::data_root()?, project, store::unix_now())
}

/// Delete a roster entry, rescoping its pieces to global FIRST (contract:
/// nothing a user wrote ever vanishes as a side effect). Rescope-then-remove
/// ordering: a crash in between leaves a still-listed project with global
/// pieces — harmless and re-deletable — never pieces pointing at a ghost.
#[tauri::command]
pub async fn delete_project(id: String) -> Result<(), String> {
    store::rescope_project_pieces(&store::prompts_dir()?, &id)?;
    projects::delete_project_at(&crate::datadir::data_root()?, &id)
}

// ---------------------------------------------------------------------------
// Matching
// ---------------------------------------------------------------------------

/// Pool rule (contract): global pieces + pieces scoped to `project_id`
/// (`None` = global only).
fn in_pool(piece: &Piece, project_id: Option<&str>) -> bool {
    match &piece.scope {
        Scope::Global => true,
        Scope::Project { project_id: p } => project_id == Some(p.as_str()),
    }
}

/// Normalized-score fusion. Contract constraint enforced structurally: hits
/// flagged `exact` sort above every non-exact hit no matter what either
/// engine scored — an exact title/keyword hit can never be buried.
fn fuse(
    lex: Vec<(String, f32, bool)>,
    sem: Vec<(String, f32)>,
    limit: usize,
) -> Vec<MatchHit> {
    let lex_max = lex.iter().map(|(_, s, _)| *s).fold(0.0f32, f32::max).max(f32::EPSILON);
    let mut hits: Vec<(MatchHit, bool)> = Vec::new();
    for (id, score, exact) in &lex {
        let sem_score = sem
            .iter()
            .find(|(sid, _)| sid == id)
            .map(|(_, c)| c.clamp(0.0, 1.0))
            .unwrap_or(0.0);
        let source = if sem_score > 0.0 { "hybrid" } else { "lexical" };
        let fused = LEX_BLEND * (score / lex_max) + (1.0 - LEX_BLEND) * sem_score;
        hits.push((MatchHit { id: id.clone(), score: fused, source: source.to_string() }, *exact));
    }
    for (id, cosine) in &sem {
        if lex.iter().any(|(lid, _, _)| lid == id) || *cosine < SEM_MIN_COSINE {
            continue;
        }
        let fused = (1.0 - LEX_BLEND) * cosine.clamp(0.0, 1.0);
        hits.push((MatchHit { id: id.clone(), score: fused, source: "semantic".to_string() }, false));
    }
    hits.sort_by(|(a, a_exact), (b, b_exact)| {
        b_exact
            .cmp(a_exact)
            .then(b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal))
    });
    hits.into_iter().map(|(h, _)| h).take(limit).collect()
}

/// The semantic side of one match call: lazy-load the embedder, top up the
/// cache, embed the query (budget-guarded), cosine-scan. Any Err degrades the
/// call to lexical-only; the error is remembered for embed_status.
fn semantic_scores(
    inner: &PromptsInner,
    root: &std::path::Path,
    query: &str,
    pool: &[Piece],
) -> Result<Vec<(String, f32)>, String> {
    let mut guard = inner.embedder.lock().map_err(|e| e.to_string())?;
    if guard.is_none() {
        *guard = Some(embed::load_embedder(root)?);
    }
    let embedder = guard.as_mut().expect("just loaded");

    let conn = embed::open_cache(root)?;
    embed::ensure_embeddings(&conn, embedder, pool, EMBED_TOPUP_PER_QUERY)?;

    let started = Instant::now();
    let query_vec = embedder
        .embed(vec![query.to_string()], None)
        .map_err(|e| e.to_string())?
        .into_iter()
        .next()
        .ok_or("empty embedding")?;
    if started.elapsed().as_millis() > INFERENCE_BUDGET_MS {
        inner.slow.store(true, Ordering::SeqCst);
        set_error(
            inner,
            format!(
                "query embedding took {}ms (budget {INFERENCE_BUDGET_MS}ms); staying lexical-only on this machine — toggle semantic match off and on to retry",
                started.elapsed().as_millis()
            ),
        );
    }

    let pool_ids: std::collections::HashSet<&str> = pool.iter().map(|p| p.id.as_str()).collect();
    Ok(embed::cached_vectors(&conn)?
        .into_iter()
        .filter(|(id, _)| pool_ids.contains(id.as_str()))
        .map(|(id, v)| (id, embed::cosine(&query_vec, &v)))
        .collect())
}

#[tauri::command]
pub async fn match_pieces(
    state: State<'_, PromptsState>,
    query: String,
    project_id: Option<String>,
    limit: usize,
) -> Result<Vec<MatchHit>, String> {
    let inner = state.inner.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let pieces = store::load_pieces(&store::prompts_dir()?, roster_ids().as_ref())?;
        let pool: Vec<Piece> =
            pieces.into_iter().filter(|p| in_pool(p, project_id.as_deref())).collect();

        let lex: Vec<(String, f32, bool)> = pool
            .iter()
            .filter_map(|p| lexical::score_piece(&query, p).map(|s| (p.id.clone(), s.score, s.exact)))
            .collect();

        let root = crate::datadir::data_root()?;
        let semantic_on = appconfig::load_embed_enabled()
            && embed::platform_supported()
            && embed::artifacts_present(&root)
            && !inner.slow.load(Ordering::SeqCst)
            && !inner.downloading.load(Ordering::SeqCst);
        let sem = if semantic_on && !query.trim().is_empty() {
            match semantic_scores(&inner, &root, &query, &pool) {
                Ok(s) => s,
                Err(e) => {
                    // Graceful degradation, but never silent: remembered for
                    // embed_status and logged, not swallowed into "no results".
                    eprintln!("[prompts] semantic match unavailable: {e}");
                    set_error(&inner, e);
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        Ok(fuse(lex, sem, limit))
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// Embedding lifecycle commands
// ---------------------------------------------------------------------------

use crate::appconfig;

#[tauri::command]
pub async fn embed_status(state: State<'_, PromptsState>) -> Result<EmbedStatus, String> {
    let inner = &state.inner;
    let status = |state: &str, error: Option<String>| EmbedStatus {
        state: state.to_string(),
        model_id: embed::MODEL_ID.to_string(),
        model_size_mb: embed::model_size_mb(),
        runtime_size_mb: embed::runtime_size_mb(),
        error,
    };
    if inner.downloading.load(Ordering::SeqCst) {
        return Ok(status("downloading", None));
    }
    if !embed::platform_supported() {
        return Ok(status(
            "error",
            Some("Semantic match is not available on this platform (no ONNX Runtime build for it)".to_string()),
        ));
    }
    if !appconfig::load_embed_enabled() {
        return Ok(status("off", None));
    }
    // Error outranks not_downloaded: after a failed download the frontend
    // re-fetches this status expecting the failure message, not a silent
    // reset to the download button (contract addendum: the Result + this
    // status are the terminal signal, there is no error channel event).
    if let Some(e) = inner.embed_error.lock().ok().and_then(|e| e.clone()) {
        return Ok(status("error", Some(e)));
    }
    let root = crate::datadir::data_root()?;
    if !embed::artifacts_present(&root) {
        return Ok(status("not_downloaded", None));
    }
    Ok(status("ready", None))
}

#[tauri::command]
pub async fn embed_download(
    state: State<'_, PromptsState>,
    on_progress: Channel<DownloadProgress>,
) -> Result<(), String> {
    let inner = state.inner.clone();
    if inner.downloading.swap(true, Ordering::SeqCst) {
        return Err("A download is already in progress".to_string());
    }
    let worker_inner = inner.clone();
    let joined = tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let inner = worker_inner;
        let root = crate::datadir::data_root()?;
        embed::download_artifacts(&root, &|p| {
            let _ = on_progress.send(p);
        })?;
        // The "index" stage (contract): embed the existing library while the
        // user is already watching the popover, streaming piece-count
        // progress — so "Download & index" is literally what one click does
        // and the first real query is instant. Resumable convention matches
        // the byte stages: already-cached pieces count as done.
        let mut embedder = embed::load_embedder(&root)?;
        // Scope validation is irrelevant for embedding (bodies only) — no
        // roster read needed.
        let pieces = store::load_pieces(&store::prompts_dir()?, None)?;
        let conn = embed::open_cache(&root)?;
        let total = pieces.len() as u64;
        loop {
            let stale =
                embed::ensure_embeddings(&conn, &mut embedder, &pieces, EMBED_TOPUP_PER_QUERY)?;
            let _ = on_progress.send(DownloadProgress {
                stage: "index".to_string(),
                done: total - stale as u64,
                total,
            });
            if stale == 0 {
                break;
            }
        }
        if let Ok(mut guard) = inner.embedder.lock() {
            *guard = Some(embedder);
        }
        if let Ok(mut e) = inner.embed_error.lock() {
            *e = None;
        }
        inner.slow.store(false, Ordering::SeqCst);
        Ok(())
    })
    .await;
    // Reset AFTER the await, whatever happened inside: a panicking closure
    // never reaches an in-closure reset and would wedge embed_status at
    // "downloading" until app restart (audit L1). A JoinError (panic) lands
    // in the same error path as a plain failure.
    inner.downloading.store(false, Ordering::SeqCst);
    let outcome = joined.map_err(|e| e.to_string()).and_then(|r| r);
    if let Err(e) = &outcome {
        set_error(&inner, e.clone());
    }
    outcome
}

#[tauri::command]
pub async fn set_embed_enabled(state: State<'_, PromptsState>, enabled: bool) -> Result<(), String> {
    appconfig::save_embed_enabled(enabled)?;
    // Toggling is also the user's "retry" affordance: clear the sticky
    // slow/error state, and free the model's RAM when switching off.
    let inner = &state.inner;
    inner.slow.store(false, Ordering::SeqCst);
    if let Ok(mut e) = inner.embed_error.lock() {
        *e = None;
    }
    if !enabled {
        if let Ok(mut guard) = inner.embedder.lock() {
            *guard = None;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Map;

    fn piece(id: &str, scope: Scope) -> Piece {
        Piece {
            id: id.into(),
            title: id.into(),
            body: String::new(),
            keywords: vec![],
            tags: vec![],
            category: None,
            scope,
            placeholders: vec![],
            created_at: 0,
            updated_at: 0,
            versions: vec![],
            recovered: false,
            extra: Map::new(),
        }
    }

    // --- pool rule ---

    #[test]
    fn pool_is_global_plus_matching_project() {
        let g = piece("g", Scope::Global);
        let mine = piece("m", Scope::Project { project_id: "proj-uuid".into() });
        let other = piece("o", Scope::Project { project_id: "other-uuid".into() });

        assert!(in_pool(&g, Some("proj-uuid")));
        assert!(in_pool(&mine, Some("proj-uuid")));
        assert!(!in_pool(&other, Some("proj-uuid")));

        assert!(in_pool(&g, None), "null project_id = global only");
        assert!(!in_pool(&mine, None));
    }

    // --- fusion invariants ---

    #[test]
    fn exact_hit_is_never_buried_by_semantic_scores() {
        // The contract's one hard fusion constraint: a middling exact hit
        // must outrank even a perfect-cosine semantic hit.
        let lex = vec![("exact".to_string(), 0.4, true), ("fuzzy".to_string(), 5.0, false)];
        let sem = vec![("semantic".to_string(), 1.0)];
        let hits = fuse(lex, sem, 10);
        assert_eq!(hits[0].id, "exact");
    }

    #[test]
    fn sources_are_tagged_by_contributing_engine() {
        let lex = vec![("both".to_string(), 1.0, false), ("lex-only".to_string(), 0.9, false)];
        let sem = vec![("both".to_string(), 0.9), ("sem-only".to_string(), 0.9)];
        let hits = fuse(lex, sem, 10);
        let source = |id: &str| hits.iter().find(|h| h.id == id).unwrap().source.clone();
        assert_eq!(source("both"), "hybrid");
        assert_eq!(source("lex-only"), "lexical");
        assert_eq!(source("sem-only"), "semantic");
    }

    #[test]
    fn low_cosine_semantic_only_candidates_are_dropped() {
        let hits = fuse(vec![], vec![("noise".to_string(), 0.2), ("real".to_string(), 0.8)], 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "real");
    }

    #[test]
    fn lexical_only_ranking_is_score_ordered_and_limited() {
        let lex = vec![
            ("low".to_string(), 0.5, false),
            ("high".to_string(), 3.0, false),
            ("mid".to_string(), 1.0, false),
        ];
        let hits = fuse(lex, vec![], 2);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].id, "high");
        assert_eq!(hits[1].id, "mid");
        assert!(hits.iter().all(|h| h.source == "lexical"));
    }

    #[test]
    fn semantic_contribution_reorders_equal_lexical_scores() {
        let lex = vec![("plain".to_string(), 1.0, false), ("boosted".to_string(), 1.0, false)];
        let sem = vec![("boosted".to_string(), 0.9)];
        let hits = fuse(lex, sem, 10);
        assert_eq!(hits[0].id, "boosted");
        assert_eq!(hits[0].source, "hybrid");
    }
}
