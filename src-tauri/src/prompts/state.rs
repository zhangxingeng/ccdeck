//! Managed state, hybrid fusion, and the Tauri commands for the Prompt Library.
//!
//! The command surface (0.13 contract), all async, `Result<T, String>`,
//! snake_case: `list_projects` / `add_project` / `remove_project` /
//! `set_active_project` / `list_snippets` / `save_snippet` / `delete_snippet` /
//! `match_snippets` / `touch_snippet`.
//!
//! Embedding has **no command surface**. The model downloads and indexes itself
//! in the background on first launch and never asks: lexical match is
//! unconditional and instant, so a download that is slow, failed, or impossible
//! on this platform degrades to lexical-only with nothing for the user to see,
//! decide, or retry. Callers never know which engine ran.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use fastembed::TextEmbedding;
use serde::Serialize;

use super::appstate::{self, Project, ProjectList};
use super::embed;
use super::lexical;
use super::store::{self, Snippet};

/// If one query embedding takes longer than this, the machine is too slow for
/// per-keystroke inference (the UI debounce budget) — degrade to lexical-only
/// for the rest of the session instead of blocking the panel.
const INFERENCE_BUDGET_MS: u128 = 250;

/// How many stale snippets one match call will embed before answering. Keeps a
/// large library from freezing a single keystroke — the cache warms over a few
/// queries; the bulk pass runs in the background at launch anyway.
const EMBED_TOPUP_PER_QUERY: usize = 32;

/// Lexical weight in the normalized-score blend (semantic gets the rest).
/// Lexical leads: on a curated corpus the user's own words beat inferred
/// similarity more often than not; semantic exists to catch the phrasings the
/// name and the content missed.
const LEX_BLEND: f32 = 0.6;

/// A semantic-only candidate below this cosine is noise, not a hit — without a
/// floor, low-similarity vectors pad the panel with head-scratchers.
const SEM_MIN_COSINE: f32 = 0.35;

/// One match result. A snippet's `name` is its identity, so that is all a hit
/// needs to carry.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MatchHit {
    pub name: String,
    pub score: f32,
}

/// Runtime-only embedding state — nothing here is persisted, because there is
/// nothing left for the user to configure.
#[derive(Default)]
pub struct PromptsInner {
    embedder: Mutex<Option<TextEmbedding>>,
    /// Set while the background download runs: semantic match sits it out rather
    /// than racing it.
    downloading: AtomicBool,
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

impl Default for PromptsState {
    fn default() -> Self {
        Self::new()
    }
}

fn root() -> Result<PathBuf, String> {
    crate::datadir::data_root()
}

// ---------------------------------------------------------------------------
// Projects
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_projects() -> Result<ProjectList, String> {
    appstate::list_projects(&root()?)
}

#[tauri::command]
pub async fn add_project(name: String, path: String) -> Result<Project, String> {
    appstate::add_project(&root()?, &name, Path::new(&path))
}

/// Forget a project. **Never deletes files** — the user's prompts are their own.
#[tauri::command]
pub async fn remove_project(path: String) -> Result<(), String> {
    appstate::remove_project(&root()?, Path::new(&path))
}

#[tauri::command]
pub async fn set_active_project(path: String) -> Result<(), String> {
    appstate::set_active_project(&root()?, Path::new(&path))
}

// ---------------------------------------------------------------------------
// Snippets
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_snippets(project: String) -> Result<Vec<Snippet>, String> {
    store::scan_snippets(Path::new(&project))
}

#[tauri::command]
pub async fn save_snippet(
    project: String,
    name: String,
    content: String,
) -> Result<Snippet, String> {
    store::save_snippet(Path::new(&project), &name, &content)
}

#[tauri::command]
pub async fn delete_snippet(project: String, name: String) -> Result<(), String> {
    let project = PathBuf::from(project);
    store::delete_snippet(&project, &name)?;
    // The file is gone, so its usage entry is dead weight. Best-effort: a failure
    // here leaves a stale key in app state — never a resurrected snippet.
    if let Err(e) = appstate::forget_snippet(&root()?, &project, &name) {
        eprintln!("[prompts] could not forget usage for {name}: {e}");
    }
    Ok(())
}

/// Record that a snippet was used — this is what orders the at-rest list. It
/// writes to app-local state, never into the project folder, which is git-tracked.
#[tauri::command]
pub async fn touch_snippet(project: String, name: String) -> Result<(), String> {
    appstate::touch_snippet(&root()?, Path::new(&project), &name)
}

// ---------------------------------------------------------------------------
// Matching
// ---------------------------------------------------------------------------

/// Normalized-score fusion. The one hard constraint is enforced structurally: a
/// hit flagged `exact` sorts above every non-exact hit no matter what either
/// engine scored, so an exact name match can never be buried.
fn fuse(lex: Vec<(String, f32, bool)>, sem: Vec<(String, f32)>, limit: usize) -> Vec<MatchHit> {
    let lex_max = lex.iter().map(|(_, s, _)| *s).fold(0.0f32, f32::max).max(f32::EPSILON);
    let mut hits: Vec<(MatchHit, bool)> = Vec::new();
    for (name, score, exact) in &lex {
        let sem_score =
            sem.iter().find(|(n, _)| n == name).map(|(_, c)| c.clamp(0.0, 1.0)).unwrap_or(0.0);
        let fused = LEX_BLEND * (score / lex_max) + (1.0 - LEX_BLEND) * sem_score;
        hits.push((MatchHit { name: name.clone(), score: fused }, *exact));
    }
    for (name, cosine) in &sem {
        if lex.iter().any(|(n, _, _)| n == name) || *cosine < SEM_MIN_COSINE {
            continue;
        }
        hits.push((
            MatchHit { name: name.clone(), score: (1.0 - LEX_BLEND) * cosine.clamp(0.0, 1.0) },
            false,
        ));
    }
    hits.sort_by(|(a, a_exact), (b, b_exact)| {
        b_exact
            .cmp(a_exact)
            .then(b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal))
    });
    hits.into_iter().map(|(h, _)| h).take(limit).collect()
}

/// The at-rest order, for an empty query: most recently used first, then the
/// never-used ones alphabetically.
///
/// An empty query used to return nothing — the user had to type to make their own
/// library appear, which is backwards. The list filters *down*, not up. No toggle
/// is needed to pick between "recent" and "relevant": with no query there is no
/// score to rank by, so recency is the only meaningful order; with a query, the
/// score is. The question answers itself.
fn at_rest_order(snippets: Vec<Snippet>, usage: &BTreeMap<String, u64>) -> Vec<MatchHit> {
    let mut ordered: Vec<(Option<u64>, String)> =
        snippets.into_iter().map(|s| (usage.get(&s.name).copied(), s.name)).collect();
    ordered.sort_by(|(a_used, a_name), (b_used, b_name)| {
        match (a_used, b_used) {
            (Some(a), Some(b)) => b.cmp(a), // most recently used first
            (Some(_), None) => std::cmp::Ordering::Less, // used beats never-used
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
        .then(a_name.cmp(b_name)) // alphabetical among the never-used, and a stable tiebreak
    });
    ordered.into_iter().map(|(_, name)| MatchHit { name, score: 0.0 }).collect()
}

/// The semantic side of one match call: lazy-load the embedder, top up this
/// project's cache, embed the query (budget-guarded), cosine-scan. Any `Err`
/// degrades the call to lexical-only.
fn semantic_scores(
    inner: &PromptsInner,
    root: &Path,
    project: &str,
    query: &str,
    pool: &[Snippet],
) -> Result<Vec<(String, f32)>, String> {
    let mut guard = inner.embedder.lock().map_err(|e| e.to_string())?;
    if guard.is_none() {
        *guard = Some(embed::load_embedder(root)?);
    }
    let embedder = guard.as_mut().expect("just loaded");

    let conn = embed::open_cache(root)?;
    embed::ensure_embeddings(&conn, embedder, project, pool, EMBED_TOPUP_PER_QUERY)?;

    let started = Instant::now();
    let query_vec = embedder
        .embed(vec![query.to_string()], None)
        .map_err(|e| e.to_string())?
        .into_iter()
        .next()
        .ok_or("empty embedding")?;
    if started.elapsed().as_millis() > INFERENCE_BUDGET_MS {
        inner.slow.store(true, Ordering::SeqCst);
        eprintln!(
            "[prompts] query embedding took {}ms (budget {INFERENCE_BUDGET_MS}ms); staying lexical-only on this machine",
            started.elapsed().as_millis()
        );
    }

    let live: std::collections::HashSet<&str> = pool.iter().map(|s| s.name.as_str()).collect();
    Ok(embed::cached_vectors(&conn, project)?
        .into_iter()
        .filter(|(name, _)| live.contains(name.as_str()))
        .map(|(name, v)| (name, embed::cosine(&query_vec, &v)))
        .collect())
}

#[tauri::command]
pub async fn match_snippets(
    state: tauri::State<'_, PromptsState>,
    project: String,
    query: String,
    limit: usize,
) -> Result<Vec<MatchHit>, String> {
    let inner = state.inner.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let root = root()?;
        let project_path = PathBuf::from(&project);
        let pool = store::scan_snippets(&project_path)?;

        if query.trim().is_empty() {
            let usage = appstate::usage_for(&root, &project_path)?;
            return Ok(at_rest_order(pool, &usage).into_iter().take(limit).collect());
        }

        let lex: Vec<(String, f32, bool)> = pool
            .iter()
            .filter_map(|s| {
                lexical::score_snippet(&query, s).map(|r| (s.name.clone(), r.score, r.exact))
            })
            .collect();

        let semantic_on = embed::platform_supported()
            && embed::artifacts_present(&root)
            && !inner.slow.load(Ordering::SeqCst)
            && !inner.downloading.load(Ordering::SeqCst);
        let sem = if semantic_on {
            match semantic_scores(&inner, &root, &project, &query, &pool) {
                Ok(s) => s,
                Err(e) => {
                    // Degrade gracefully, but never silently: logged, not swallowed
                    // into "no results". The user gets a working lexical panel; the
                    // reason lands in the log.
                    eprintln!("[prompts] semantic match unavailable: {e}");
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
// Background embedding — no user-facing surface at all
// ---------------------------------------------------------------------------

/// Download the model and index the active project in the background, silently.
///
/// Two conditions define this, and they are the whole design:
/// - **it never blocks startup or any user action** — it runs on a blocking task
///   nothing waits for, and lexical match works unconditionally throughout;
/// - **it fails silently to lexical** — no toast, no notice, no retry nagging. A
///   failure is logged and the app carries on fully working, because semantic
///   match improves ranking and is never a prerequisite for it.
pub fn spawn_background_index(state: &PromptsState) {
    let inner = state.inner.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if !embed::platform_supported() {
            return; // no ONNX Runtime build here; lexical-only, and that is fine
        }
        inner.downloading.store(true, Ordering::SeqCst);
        let outcome = background_index(&inner);
        // Reset whatever happened, including a panic unwinding past here — a
        // stuck `downloading` flag would wedge semantic match off until restart.
        inner.downloading.store(false, Ordering::SeqCst);
        if let Err(e) = outcome {
            eprintln!(
                "[prompts] background semantic index unavailable ({e}); matching stays lexical"
            );
        }
    });
}

fn background_index(inner: &PromptsInner) -> Result<(), String> {
    let root = root()?;
    if !embed::artifacts_present(&root) {
        // No progress channel: nobody is watching, by design.
        embed::download_artifacts(&root, &|_| {})?;
    }
    let Some(project) = appstate::active_project(&root)? else {
        return Ok(()); // no project registered yet — nothing to index
    };
    let snippets = store::scan_snippets(&project)?;
    let mut embedder = embed::load_embedder(&root)?;
    let conn = embed::open_cache(&root)?;
    let key = project.display().to_string();
    // Loop until nothing is stale: each `ensure_embeddings` pass is capped, so a
    // huge library cannot monopolize a single call.
    while embed::ensure_embeddings(&conn, &mut embedder, &key, &snippets, EMBED_TOPUP_PER_QUERY)? > 0
    {}
    if let Ok(mut guard) = inner.embedder.lock() {
        *guard = Some(embedder);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snippet(name: &str) -> Snippet {
        Snippet { name: name.into(), content: String::new() }
    }

    // --- the at-rest order (empty query) ---

    #[test]
    fn empty_query_returns_everything_most_recently_used_first() {
        // The defect this fixes: an empty query returned nothing, so the user had
        // to type to make their own library appear.
        let snippets = vec![snippet("old"), snippet("fresh"), snippet("never")];
        let usage = BTreeMap::from([("old".to_string(), 100), ("fresh".to_string(), 200)]);

        let names: Vec<String> =
            at_rest_order(snippets, &usage).into_iter().map(|h| h.name).collect();
        assert_eq!(names, ["fresh", "old", "never"], "recent first, never-used last");
    }

    #[test]
    fn never_used_snippets_sort_alphabetically_after_the_used_ones() {
        let snippets = vec![snippet("zebra"), snippet("apple"), snippet("used")];
        let usage = BTreeMap::from([("used".to_string(), 1)]);
        let names: Vec<String> =
            at_rest_order(snippets, &usage).into_iter().map(|h| h.name).collect();
        assert_eq!(names, ["used", "apple", "zebra"]);
    }

    #[test]
    fn at_rest_order_is_total_even_with_no_usage_at_all() {
        // A fresh install: nothing has ever been used, and the list must still be
        // everything, in a stable order — not empty.
        let names: Vec<String> = at_rest_order(vec![snippet("b"), snippet("a")], &BTreeMap::new())
            .into_iter()
            .map(|h| h.name)
            .collect();
        assert_eq!(names, ["a", "b"]);
    }

    // --- fusion invariants ---

    #[test]
    fn an_exact_hit_is_never_buried_by_a_semantic_score() {
        // The one hard fusion constraint: a middling exact hit must outrank even a
        // perfect-cosine semantic hit.
        let lex = vec![("exact".to_string(), 0.4, true), ("fuzzy".to_string(), 5.0, false)];
        let sem = vec![("semantic".to_string(), 1.0)];
        assert_eq!(fuse(lex, sem, 10)[0].name, "exact");
    }

    #[test]
    fn low_cosine_semantic_only_candidates_are_dropped() {
        let hits = fuse(vec![], vec![("noise".to_string(), 0.2), ("real".to_string(), 0.8)], 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "real");
    }

    #[test]
    fn lexical_only_ranking_is_score_ordered_and_limited() {
        let lex = vec![
            ("low".to_string(), 0.5, false),
            ("high".to_string(), 3.0, false),
            ("mid".to_string(), 1.0, false),
        ];
        let names: Vec<String> = fuse(lex, vec![], 2).into_iter().map(|h| h.name).collect();
        assert_eq!(names, ["high", "mid"]);
    }

    #[test]
    fn semantic_contribution_reorders_equal_lexical_scores() {
        let lex = vec![("plain".to_string(), 1.0, false), ("boosted".to_string(), 1.0, false)];
        let sem = vec![("boosted".to_string(), 0.9)];
        assert_eq!(fuse(lex, sem, 10)[0].name, "boosted");
    }
}
