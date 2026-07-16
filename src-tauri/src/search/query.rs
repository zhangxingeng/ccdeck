//! The matcher: turn a query string into a tantivy `BooleanQuery` — an
//! exact-term clause boosted above a same-token `FuzzyTermQuery` clause,
//! unioned across query tokens — combined with the caller's filters, ranked
//! by BM25. Replaces the old regex-crate substring/whole-word/regex scan
//! (issue #5: fuzzy/intent search over precision-editing tooling).
//!
//! Also carries a small substring-based cold-path matcher for session files
//! not yet reflected in the tantivy index (see `state.rs`'s cold tier) — a
//! deliberately simpler fallback than building a throwaway tantivy index per
//! uncached file.

use std::ops::Bound;

use serde::{Deserialize, Serialize};
use tantivy::collector::{Count, TopDocs};
use tantivy::query::{BooleanQuery, BoostQuery, FuzzyTermQuery, Occur, Query, RangeQuery, TermQuery};
use tantivy::schema::{document::Value, IndexRecordOption};
use tantivy::snippet::SnippetGenerator;
use tantivy::{Index, Searcher, TantivyDocument, Term};

use super::index::{SearchSchema, TEXT_TOKENIZER};

/// Boost applied to an exact-term clause over its sibling fuzzy clause, so an
/// exact/near-exact match ranks above a loosely-fuzzy one (tantivy's fuzzy hits
/// otherwise score on par with exact hits — the risk flagged in issue #5).
const EXACT_BOOST: f32 = 3.0;
/// Levenshtein distance tolerated by the fuzzy clause (typo tolerance).
const FUZZY_DISTANCE: u8 = 1;
/// Minimum token length (in chars) before a fuzzy sibling clause is added.
/// Below this, edit-distance-1 matches too large a fraction of any real
/// vocabulary to be useful — empirically, a 2-char query fuzzy-matched nearly
/// every other short word in a small test corpus (found in the issue #5
/// Gate-2 audit). Tokens under the floor get exact-only matching. Mirrors
/// Elasticsearch's `fuzziness: "AUTO"` floor (0 edits below length 3, 1 edit
/// at length 3+).
const MIN_FUZZY_TOKEN_LEN: usize = 3;
const SNIPPET_MAX_CHARS: usize = 240;

/// Query-time filters. Empty `projects` means "no restriction". Narrowed to
/// date + project (+ the `session_path` scope) when search became
/// messages-only (#35) — the old `sources`/`tool_name` filters are gone.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchFilters {
    /// Inclusive epoch-ms lower bound on the block timestamp.
    pub from: Option<i64>,
    /// Inclusive epoch-ms upper bound.
    pub to: Option<i64>,
    /// Home-relative project labels to include.
    #[serde(default)]
    pub projects: Vec<String>,
    /// Restrict results to this one session file (the "current session only" filter).
    #[serde(default)]
    pub session_path: Option<String>,
}

/// Returned when a search finishes (or is cancelled / truncated by limit).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchSummary {
    /// Number of hits emitted.
    pub hits: usize,
    /// Total number of matching blocks found (before the `limit` truncation).
    pub scanned: usize,
    /// True if a newer search superseded this one before it finished.
    pub cancelled: bool,
    /// True if the result set stopped early because it hit the optional `limit`.
    pub truncated: bool,
}

/// One search result: a matched block plus a windowed snippet, the match
/// offsets (in *chars*, so the JS side can slice with `Array.from`), and its
/// relevance score (BM25 + the exact/fuzzy boost — highest first).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub session_path: String,
    pub project: String,
    pub ts: Option<i64>,
    pub line_no: i64,
    pub block_no: i64,
    pub uuid: String,
    pub source: String,
    pub snippet: String,
    pub match_ranges: Vec<(u32, u32)>,
    pub score: f32,
}

/// Tokenize a query string with the same named analyzer that indexed the
/// `text` field (see `index::TEXT_TOKENIZER`), so query tokens line up with
/// the terms actually stored in the index.
pub(crate) fn tokenize(index: &Index, text: &str) -> Result<Vec<String>, String> {
    let mut analyzer = index
        .tokenizers()
        .get(TEXT_TOKENIZER)
        .ok_or("search tokenizer not registered")?;
    let mut stream = analyzer.token_stream(text);
    let mut tokens = Vec::new();
    while stream.advance() {
        tokens.push(stream.token().text.clone());
    }
    Ok(tokens)
}

fn term_query(field: tantivy::schema::Field, value: &str) -> Box<dyn Query> {
    Box::new(TermQuery::new(
        Term::from_field_text(field, value),
        IndexRecordOption::Basic,
    ))
}

fn should_group(clauses: Vec<Box<dyn Query>>) -> Box<dyn Query> {
    Box::new(BooleanQuery::new(
        clauses.into_iter().map(|q| (Occur::Should, q)).collect(),
    ))
}

/// Build the full query: an exact-boosted/fuzzy union per query token, ANDed
/// with the caller's filters. `Ok(None)` for an empty/whitespace-only query —
/// callers treat that as "no results" without running a search.
pub fn build_query(
    index: &Index,
    schema: &SearchSchema,
    query_str: &str,
    filters: &SearchFilters,
) -> Result<Option<BooleanQuery>, String> {
    let tokens = tokenize(index, query_str)?;
    if tokens.is_empty() {
        return Ok(None);
    }

    let token_clauses: Vec<Box<dyn Query>> = tokens
        .iter()
        .map(|tok| {
            let term = Term::from_field_text(schema.text, tok);
            let exact: Box<dyn Query> = Box::new(BoostQuery::new(
                Box::new(TermQuery::new(term.clone(), IndexRecordOption::WithFreqsAndPositions)),
                EXACT_BOOST,
            ));
            // Below the floor, a fuzzy sibling clause matches too much of any
            // real vocabulary to be useful — exact-only for short tokens.
            if tok.chars().count() < MIN_FUZZY_TOKEN_LEN {
                return exact;
            }
            let fuzzy: Box<dyn Query> =
                Box::new(FuzzyTermQuery::new(term, FUZZY_DISTANCE, true));
            should_group(vec![exact, fuzzy])
        })
        .collect();
    let text_query = should_group(token_clauses);

    let mut must: Vec<(Occur, Box<dyn Query>)> = vec![(Occur::Must, text_query)];

    if !filters.projects.is_empty() {
        let group = should_group(
            filters
                .projects
                .iter()
                .map(|p| term_query(schema.project, p))
                .collect(),
        );
        must.push((Occur::Must, group));
    }

    if filters.from.is_some() || filters.to.is_some() {
        let lower = filters
            .from
            .map(|v| Bound::Included(Term::from_field_i64(schema.ts, v)))
            .unwrap_or(Bound::Unbounded);
        let upper = filters
            .to
            .map(|v| Bound::Included(Term::from_field_i64(schema.ts, v)))
            .unwrap_or(Bound::Unbounded);
        must.push((Occur::Must, Box::new(RangeQuery::new(lower, upper))));
    }

    if let Some(session_path) = &filters.session_path {
        must.push((Occur::Must, term_query(schema.session_path, session_path)));
    }

    Ok(Some(BooleanQuery::new(must)))
}

fn get_str(doc: &TantivyDocument, field: tantivy::schema::Field) -> String {
    doc.get_first(field)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn get_i64(doc: &TantivyDocument, field: tantivy::schema::Field) -> i64 {
    doc.get_first(field).and_then(|v| v.as_i64()).unwrap_or(0)
}

/// Run the built query against the warm tantivy index: collect the top
/// `limit` hits by relevance plus the total match count, and build a
/// highlighted snippet for each. Hits come back already sorted by score desc
/// (tantivy's `TopDocs` collector sorts internally) — no separate buffering
/// step needed for relevance ordering. `tokens` are the same query tokens
/// `build_query` was built from — needed for the fuzzy-only highlight
/// fallback below, since the compiled `query` object alone doesn't expose
/// them cheaply.
pub fn search_warm(
    searcher: &Searcher,
    schema: &SearchSchema,
    query: &dyn Query,
    tokens: &[String],
    limit: usize,
) -> Result<(usize, Vec<SearchHit>), String> {
    let top = searcher
        .search(query, &TopDocs::with_limit(limit).order_by_score())
        .map_err(|e| e.to_string())?;
    let total = searcher.search(query, &Count).map_err(|e| e.to_string())?;

    let mut snippet_gen =
        SnippetGenerator::create(searcher, query, schema.text).map_err(|e| e.to_string())?;
    snippet_gen.set_max_num_chars(SNIPPET_MAX_CHARS);

    let mut hits = Vec::with_capacity(top.len());
    for (score, addr) in top {
        let doc: TantivyDocument = searcher.doc(addr).map_err(|e| e.to_string())?;
        let text = get_str(&doc, schema.text);
        let snippet = snippet_gen.snippet_from_doc(&doc);
        let fragment = snippet.fragment();
        let mut match_ranges: Vec<(u32, u32)> = snippet
            .highlighted()
            .iter()
            .map(|r| {
                let cs = fragment[..r.start].chars().count() as u32;
                let ce = fragment[..r.end].chars().count() as u32;
                (cs, ce)
            })
            .collect();
        // A fuzzy-only match may not literally contain the query term, so the
        // snippet generator can return an empty fragment — fall back to a
        // plain leading excerpt of the stored text so the hit still has
        // something to show.
        let snippet_text = if fragment.is_empty() {
            text.chars().take(SNIPPET_MAX_CHARS).collect()
        } else {
            fragment.to_string()
        };
        // The snippet generator only highlights a literal substring, so a
        // fuzzy-only hit (the term the user typed isn't literally in the
        // text) leaves match_ranges empty — the user gets zero visual
        // explanation of why a fuzzy result surfaced, which undercuts the
        // point of a fuzzy engine being legible. Best-effort fallback: scan
        // for a word within edit distance of a query token and highlight that
        // instead.
        if match_ranges.is_empty() {
            match_ranges = fuzzy_highlight(&snippet_text, tokens);
        }

        let ts_val = get_i64(&doc, schema.ts);
        hits.push(SearchHit {
            session_path: get_str(&doc, schema.session_path),
            project: get_str(&doc, schema.project),
            ts: if ts_val == 0 { None } else { Some(ts_val) },
            line_no: get_i64(&doc, schema.line_no),
            block_no: get_i64(&doc, schema.block_no),
            uuid: get_str(&doc, schema.uuid),
            source: get_str(&doc, schema.source),
            snippet: snippet_text,
            match_ranges,
            score,
        });
    }

    Ok((total, hits))
}

/// Cheap bounded edit-distance check — a full Levenshtein DP is fine at this
/// scale (single words, `max_dist` is always small), and the length-diff
/// early-exit skips the DP entirely for the common non-match case.
fn within_edit_distance(a: &str, b: &str, max_dist: usize) -> bool {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.len().abs_diff(b.len()) > max_dist {
        return false;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![0usize; b.len() + 1];
        cur[0] = i;
        for j in 1..=b.len() {
            cur[j] = if a[i - 1] == b[j - 1] {
                prev[j - 1]
            } else {
                1 + prev[j - 1].min(prev[j]).min(cur[j - 1])
            };
        }
        prev = cur;
    }
    prev[b.len()] <= max_dist
}

/// Word-level fallback highlight for a fuzzy-only hit: scan `text` for any
/// word within [`FUZZY_DISTANCE`] edits of one of the query `tokens` (only
/// tokens at or above [`MIN_FUZZY_TOKEN_LEN`] — shorter tokens never went
/// through a fuzzy clause, so a "close" word for one would be a false
/// explanation) and return its char range. Best-effort only — used solely
/// when the literal snippet path found nothing to highlight.
fn fuzzy_highlight(text: &str, tokens: &[String]) -> Vec<(u32, u32)> {
    fn is_match(word: &str, tokens: &[String]) -> bool {
        let lower = word.to_lowercase();
        tokens
            .iter()
            .filter(|t| t.chars().count() >= MIN_FUZZY_TOKEN_LEN)
            .any(|t| within_edit_distance(&lower, t, FUZZY_DISTANCE as usize))
    }

    let mut ranges = Vec::new();
    let mut char_idx: u32 = 0;
    let mut word_start: Option<u32> = None;
    let mut word = String::new();

    for ch in text.chars() {
        if ch.is_alphanumeric() {
            if word_start.is_none() {
                word_start = Some(char_idx);
            }
            word.push(ch);
        } else if let Some(start) = word_start.take() {
            if is_match(&word, tokens) {
                ranges.push((start, char_idx));
            }
            word.clear();
        }
        char_idx += 1;
    }
    if let Some(start) = word_start {
        if is_match(&word, tokens) {
            ranges.push((start, char_idx));
        }
    }
    ranges
}

fn floor_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn ceil_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

const COLD_WINDOW_BEFORE: usize = 60;
const COLD_WINDOW_AFTER: usize = 180;

/// Cold-path match: at least one query token appears as a case-insensitive
/// substring — an OR/partial-credit match (a simpler, non-fuzzy stand-in used
/// only for session files not yet reflected in the tantivy index — see
/// `state.rs`'s cold tier). Previously required ALL tokens (an AND), which
/// gave the exact same query a stricter, different result set purely because
/// a file hadn't been swept into the tantivy index yet — the warm tier is
/// OR-across-tokens by design (see `build_query`), so the cold tier now
/// matches that recall shape (found in the issue #5 Gate-2 audit). Returns a
/// windowed snippet around the earliest match, char-offset ranges for every
/// matched token's occurrences inside that window, and the count of distinct
/// tokens matched — a coarse relevance proxy so more-tokens-matched still
/// ranks higher, mirroring (without a full BM25 recompute) the warm tier's
/// "more matched tokens accumulate more score" behavior.
pub fn cold_match(text: &str, tokens: &[String]) -> Option<(String, Vec<(u32, u32)>, usize)> {
    if tokens.is_empty() {
        return None;
    }
    let lower = text.to_lowercase();
    let mut first = usize::MAX;
    let mut matched = 0usize;
    for tok in tokens {
        if let Some(pos) = lower.find(tok.as_str()) {
            matched += 1;
            first = first.min(pos);
        }
    }
    if matched == 0 {
        return None;
    }

    let win_start = floor_boundary(&lower, first.saturating_sub(COLD_WINDOW_BEFORE));
    let win_end = ceil_boundary(
        &lower,
        (first + COLD_WINDOW_AFTER).min(lower.len()),
    );

    let mut snippet = String::new();
    let lead = win_start > 0;
    if lead {
        snippet.push('…');
    }
    snippet.push_str(&text[win_start..win_end]);
    if win_end < text.len() {
        snippet.push('…');
    }

    let base_chars = if lead { 1u32 } else { 0 };
    let mut ranges = Vec::new();
    for tok in tokens {
        let mut search_from = win_start;
        while let Some(rel) = lower[search_from..win_end.max(search_from)].find(tok.as_str()) {
            let s = search_from + rel;
            let e = s + tok.len();
            if s < win_start || e > win_end {
                break;
            }
            let cs = base_chars + text[win_start..s].chars().count() as u32;
            let ce = base_chars + text[win_start..e].chars().count() as u32;
            ranges.push((cs, ce));
            search_from = e;
        }
    }
    ranges.sort_unstable();

    Some((snippet, ranges, matched))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::index::{open_index, SearchSchema};
    use std::fs;

    fn tmp_index(tag: &str) -> (Index, SearchSchema) {
        let dir = std::env::temp_dir().join(format!("ccstudio_query_test_{tag}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let schema = SearchSchema::build();
        let index = open_index(&dir, &schema.schema).unwrap();
        (index, schema)
    }

    fn add_block(
        writer: &tantivy::IndexWriter,
        schema: &SearchSchema,
        session_path: &str,
        project: &str,
        source: &str,
        text: &str,
    ) {
        let mut doc = TantivyDocument::default();
        doc.add_text(schema.session_path, session_path);
        doc.add_text(schema.project, project);
        doc.add_i64(schema.line_no, 0);
        doc.add_i64(schema.block_no, 0);
        doc.add_text(schema.uuid, "u1");
        doc.add_text(schema.source, source);
        doc.add_text(schema.text, text);
        writer.add_document(doc).unwrap();
    }

    #[test]
    fn fuzzy_query_finds_typo_and_ranks_exact_above_it() {
        let (index, schema) = tmp_index("fuzzy_rank");
        let mut writer = index.writer(15_000_000).unwrap();
        add_block(&writer, &schema, "/a.jsonl", "~/app", "user", "please fix the parser bug today");
        add_block(&writer, &schema, "/b.jsonl", "~/app", "assistant", "the parsr looked fine to me");
        writer.commit().unwrap();

        let reader = index.reader().unwrap();
        let searcher = reader.searcher();

        let tokens = tokenize(&index, "parser").unwrap();
        let query = build_query(&index, &schema, "parser", &SearchFilters::default())
            .unwrap()
            .expect("non-empty query");
        let (total, hits) = search_warm(&searcher, &schema, &query, &tokens, 10).unwrap();

        assert_eq!(total, 2, "exact + typo variant both match");
        assert_eq!(hits.len(), 2);
        // Exact match ("parser") must outrank the fuzzy typo match ("parsr").
        assert!(hits[0].snippet.contains("parser"));
        assert!(hits[0].score > hits[1].score);
    }

    #[test]
    fn fuzzy_only_hit_gets_a_best_effort_highlight() {
        let (index, schema) = tmp_index("fuzzy_highlight");
        let mut writer = index.writer(15_000_000).unwrap();
        add_block(&writer, &schema, "/a.jsonl", "~/app", "assistant", "the parsr looked fine to me");
        writer.commit().unwrap();

        let reader = index.reader().unwrap();
        let searcher = reader.searcher();

        let tokens = tokenize(&index, "parser").unwrap();
        let query = build_query(&index, &schema, "parser", &SearchFilters::default())
            .unwrap()
            .expect("non-empty query");
        let (_, hits) = search_warm(&searcher, &schema, &query, &tokens, 10).unwrap();

        assert_eq!(hits.len(), 1);
        assert!(
            !hits[0].match_ranges.is_empty(),
            "a fuzzy-only hit should still get a best-effort highlight"
        );
        let (s, e) = hits[0].match_ranges[0];
        let highlighted: String = hits[0]
            .snippet
            .chars()
            .skip(s as usize)
            .take((e - s) as usize)
            .collect();
        assert_eq!(highlighted, "parsr");
    }

    #[test]
    fn multi_token_query_ranks_both_tokens_matched_above_one_token_matched() {
        let (index, schema) = tmp_index("multi_token");
        let mut writer = index.writer(15_000_000).unwrap();
        add_block(&writer, &schema, "/both.jsonl", "~/app", "user", "the parser bug is fixed");
        add_block(&writer, &schema, "/one.jsonl", "~/app", "user", "parser was never the problem");
        writer.commit().unwrap();

        let reader = index.reader().unwrap();
        let searcher = reader.searcher();

        let tokens = tokenize(&index, "parser fixed").unwrap();
        let query = build_query(&index, &schema, "parser fixed", &SearchFilters::default())
            .unwrap()
            .expect("non-empty query");
        let (total, hits) = search_warm(&searcher, &schema, &query, &tokens, 10).unwrap();

        assert_eq!(
            total, 2,
            "OR-across-tokens: a doc matching only one of two tokens still counts"
        );
        assert_eq!(
            hits[0].session_path, "/both.jsonl",
            "matching both query tokens ranks first"
        );
        assert!(hits[0].score > hits[1].score);
    }

    #[test]
    fn project_filter_narrows_results() {
        // Source is no longer a filter (search is messages-only now, #35), so
        // only the project filter is exercised here. `source` still rides along
        // on each hit for the You/Claude badge — asserted below.
        let (index, schema) = tmp_index("filters");
        let mut writer = index.writer(15_000_000).unwrap();
        add_block(&writer, &schema, "/a.jsonl", "~/app", "user", "investigate the bug report");
        add_block(&writer, &schema, "/a.jsonl", "~/app", "assistant", "found the bug cause");
        add_block(&writer, &schema, "/b.jsonl", "~/lib", "user", "bug in the lib too");
        writer.commit().unwrap();

        let reader = index.reader().unwrap();
        let searcher = reader.searcher();

        let tokens = tokenize(&index, "bug").unwrap();

        // No filter: all three blocks match "bug".
        let query = build_query(&index, &schema, "bug", &SearchFilters::default())
            .unwrap()
            .unwrap();
        let (total, _) = search_warm(&searcher, &schema, &query, &tokens, 10).unwrap();
        assert_eq!(total, 3);

        // Project filter narrows to the ~/lib session only.
        let filters = SearchFilters {
            projects: vec!["~/lib".into()],
            ..Default::default()
        };
        let query = build_query(&index, &schema, "bug", &filters).unwrap().unwrap();
        let (total, hits) = search_warm(&searcher, &schema, &query, &tokens, 10).unwrap();
        assert_eq!(total, 1);
        assert_eq!(hits[0].project, "~/lib");
        assert_eq!(hits[0].source, "user", "source is still returned for the badge");
    }

    #[test]
    fn long_tokens_survive_tokenization_and_are_findable() {
        // tantivy's built-in "default" tokenizer keeps a token only if
        // `token.len() < limit`, so `RemoveLongFilter::limit(40)` drops any
        // token AT OR OVER 40 chars — exactly a git SHA-1's length, routine
        // content in this app's domain. Locks in the fix: our own
        // TEXT_TOKENIZER raises that ceiling, both at index time and here at
        // query time (found in the issue #5 Gate-2 audit).
        let sha = "9fceb02d0ae598e95dc970b74767f19372d61af8"; // a real 40-char SHA-1
        assert_eq!(sha.len(), 40);

        let (index, schema) = tmp_index("long_token");
        let tokens = tokenize(&index, sha).unwrap();
        assert_eq!(tokens, vec![sha.to_string()], "a 40-char token must survive tokenization");

        let mut writer = index.writer(15_000_000).unwrap();
        add_block(&writer, &schema, "/a.jsonl", "~/app", "user", &format!("fixed in commit {sha}"));
        writer.commit().unwrap();

        let reader = index.reader().unwrap();
        let searcher = reader.searcher();
        let query = build_query(&index, &schema, sha, &SearchFilters::default())
            .unwrap()
            .expect("non-empty query");
        let (total, _hits) = search_warm(&searcher, &schema, &query, &tokens, 10).unwrap();
        assert_eq!(total, 1, "the long token must be findable, not silently dropped");
    }

    #[test]
    fn empty_query_yields_no_query() {
        let (index, schema) = tmp_index("empty");
        assert!(build_query(&index, &schema, "   ", &SearchFilters::default())
            .unwrap()
            .is_none());
    }

    #[test]
    fn cold_match_matches_any_token_and_returns_snippet() {
        let tokens = vec!["fix".to_string(), "parser".to_string()];
        let (snippet, ranges, matched) =
            cold_match("please fix the parser bug today", &tokens).unwrap();
        assert!(snippet.contains("fix"));
        assert!(snippet.contains("parser"));
        assert_eq!(ranges.len(), 2);
        assert_eq!(matched, 2);

        // Partial match (only one of the two tokens present) still counts —
        // OR, not AND, consistent with the warm tier's relevance-not-boolean-
        // gate philosophy (previously this returned None).
        let (_, partial_ranges, partial_matched) =
            cold_match("please fix this today", &tokens).unwrap();
        assert_eq!(partial_matched, 1);
        assert_eq!(partial_ranges.len(), 1);

        assert!(cold_match("nothing relevant here", &tokens).is_none());
    }
}
