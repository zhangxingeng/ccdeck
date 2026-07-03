//! The matcher: turn a query + toggles into a `Regex`, prefilter candidate
//! blocks with SQL, scan them, and build snippet hits.
//!
//! All three query modes (plain / whole-word / regex) unify through the `regex`
//! crate — plain is just `regex::escape`d, whole-word wraps in `\b…\b`. The
//! crate's literal prefiltering keeps even a full-column scan fast.

use rusqlite::types::Value;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use regex::{Regex, RegexBuilder};

/// Check the cancellation flag every this many candidate rows (cheap atomic
/// load; no need to hit it on literally every row).
const CANCEL_CHECK_EVERY: usize = 64;

/// How much text around the first match to include in a snippet (bytes, snapped
/// to char boundaries).
const WINDOW_BEFORE: usize = 60;
const WINDOW_AFTER: usize = 180;

/// VS Code-style search toggles.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchOpts {
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub whole_word: bool,
    #[serde(default)]
    pub regex: bool,
}

/// Query-time filters. Empty `sources`/`projects` mean "no restriction".
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchFilters {
    /// Low-level block sources: user, assistant, thinking, tool_use, tool_result.
    #[serde(default)]
    pub sources: Vec<String>,
    /// Inclusive epoch-ms lower bound on the block timestamp.
    pub from: Option<i64>,
    /// Inclusive epoch-ms upper bound.
    pub to: Option<i64>,
    /// Home-relative project labels to include.
    #[serde(default)]
    pub projects: Vec<String>,
}

/// Returned when a search finishes (or is cancelled).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchSummary {
    /// Number of hits emitted.
    pub hits: usize,
    /// Number of candidate blocks scanned (post SQL-prefilter).
    pub scanned: usize,
    /// True if a newer search superseded this one before it finished.
    pub cancelled: bool,
}

/// One search result: a matched block plus a windowed snippet and the match
/// offsets (in *chars*, so the JS side can slice with `Array.from`).
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
}

/// Compile the query into a `Regex`, honouring the toggles. A bad regex (in
/// regex mode) surfaces as an `Err(String)` the UI shows as a gentle hint.
pub fn build_regex(query: &str, opts: &SearchOpts) -> Result<Regex, String> {
    let base = if opts.regex {
        query.to_string()
    } else {
        regex::escape(query)
    };
    let pattern = if opts.whole_word {
        format!(r"\b{base}\b")
    } else {
        base
    };
    RegexBuilder::new(&pattern)
        .case_insensitive(!opts.case_sensitive)
        .build()
        .map_err(|e| e.to_string())
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

/// Build a windowed snippet around the first match, with char-offset ranges for
/// every match that falls inside the window. Returns `None` if nothing matches.
pub fn build_snippet(text: &str, re: &Regex) -> Option<(String, Vec<(u32, u32)>)> {
    let matches: Vec<(usize, usize)> = re.find_iter(text).map(|m| (m.start(), m.end())).collect();
    let (first_start, first_end) = *matches.first()?;

    let win_start = floor_boundary(text, first_start.saturating_sub(WINDOW_BEFORE));
    let win_end = ceil_boundary(text, first_end.saturating_add(WINDOW_AFTER).min(text.len()));

    let mut snippet = String::new();
    let lead = win_start > 0;
    if lead {
        snippet.push('…');
    }
    snippet.push_str(&text[win_start..win_end]);
    if win_end < text.len() {
        snippet.push('…');
    }

    // Char offset of the window start within the snippet (1 if a leading '…').
    let base_chars = if lead { 1u32 } else { 0 };
    let ranges = matches
        .iter()
        .filter(|(s, e)| *s >= win_start && *e <= win_end)
        .map(|(s, e)| {
            let cs = base_chars + text[win_start..*s].chars().count() as u32;
            let ce = base_chars + text[win_start..*e].chars().count() as u32;
            (cs, ce)
        })
        .collect();

    Some((snippet, ranges))
}

/// Push a `?,?,…` placeholder group for an IN clause and its bound values.
fn push_in_clause(
    where_parts: &mut Vec<String>,
    params: &mut Vec<Value>,
    column: &str,
    items: &[String],
) {
    if items.is_empty() {
        return;
    }
    let placeholders = vec!["?"; items.len()].join(",");
    where_parts.push(format!("{column} IN ({placeholders})"));
    for it in items {
        params.push(Value::Text(it.clone()));
    }
}

/// Build the candidate-selection SQL + bound params from the filters. Candidates
/// come back recency-first (session mtime desc, then file order) so streamed
/// results are stable/append-only.
fn candidate_sql(filters: &SearchFilters) -> (String, Vec<Value>) {
    let mut where_parts: Vec<String> = Vec::new();
    let mut params: Vec<Value> = Vec::new();

    push_in_clause(&mut where_parts, &mut params, "b.source", &filters.sources);
    push_in_clause(&mut where_parts, &mut params, "b.project", &filters.projects);
    if let Some(from) = filters.from {
        where_parts.push("b.ts >= ?".to_string());
        params.push(Value::Integer(from));
    }
    if let Some(to) = filters.to {
        where_parts.push("b.ts <= ?".to_string());
        params.push(Value::Integer(to));
    }

    let where_sql = if where_parts.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_parts.join(" AND "))
    };
    let sql = format!(
        "SELECT b.session_path, b.project, b.ts, b.line_no, b.block_no, b.uuid, b.source, b.text
         FROM blocks b
         JOIN session_files sf ON b.session_path = sf.session_path
         {where_sql}
         ORDER BY sf.mtime DESC, b.rowid ASC"
    );
    (sql, params)
}

/// Does a block pass the source + date filters? (Project is filtered per-file /
/// in SQL, not here.)
fn passes_source_and_date(source: &str, ts: Option<i64>, filters: &SearchFilters) -> bool {
    if !filters.sources.is_empty() && !filters.sources.iter().any(|s| s == source) {
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

/// Cold-path scan: apply the same filters + regex to freshly-extracted blocks
/// from a session not yet in the cache. Emits a hit per match. Returns the count.
pub fn scan_blocks<E: FnMut(SearchHit)>(
    session_path: &str,
    project: &str,
    blocks: &[super::extract::ExtractedBlock],
    re: &Regex,
    filters: &SearchFilters,
    emit: &mut E,
) -> usize {
    let mut n = 0;
    for b in blocks {
        if !passes_source_and_date(&b.source, b.ts, filters) {
            continue;
        }
        if let Some((snippet, match_ranges)) = build_snippet(&b.text, re) {
            emit(SearchHit {
                session_path: session_path.to_string(),
                project: project.to_string(),
                ts: b.ts,
                line_no: b.line_no,
                block_no: b.block_no,
                uuid: b.uuid.clone(),
                source: b.source.clone(),
                snippet,
                match_ranges,
            });
            n += 1;
        }
    }
    n
}

/// Warm-path streaming search. SQL-prefilters candidate blocks, scans each with
/// the (prebuilt) regex, and calls `emit` for every hit as it's found.
/// `cancelled` is polled periodically; when it returns true the scan stops
/// promptly (a newer search has superseded this one).
pub fn search_streaming<E, C>(
    conn: &Connection,
    re: &Regex,
    filters: &SearchFilters,
    mut emit: E,
    cancelled: C,
) -> Result<SearchSummary, String>
where
    E: FnMut(SearchHit),
    C: Fn() -> bool,
{
    let mut summary = SearchSummary::default();
    let (sql, params) = candidate_sql(filters);

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let mut rows = stmt
        .query(rusqlite::params_from_iter(params))
        .map_err(|e| e.to_string())?;

    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        summary.scanned += 1;
        if summary.scanned % CANCEL_CHECK_EVERY == 0 && cancelled() {
            summary.cancelled = true;
            break;
        }

        let text: String = row.get(7).map_err(|e| e.to_string())?;
        let Some((snippet, match_ranges)) = build_snippet(&text, &re) else {
            continue;
        };
        emit(SearchHit {
            session_path: row.get(0).map_err(|e| e.to_string())?,
            project: row.get(1).map_err(|e| e.to_string())?,
            ts: row.get(2).map_err(|e| e.to_string())?,
            line_no: row.get(3).map_err(|e| e.to_string())?,
            block_no: row.get(4).map_err(|e| e.to_string())?,
            uuid: row.get(5).map_err(|e| e.to_string())?,
            source: row.get(6).map_err(|e| e.to_string())?,
            snippet,
            match_ranges,
        });
        summary.hits += 1;
    }

    Ok(summary)
}

/// Non-streaming convenience wrapper (collect all hits into a Vec). Used by
/// tests and any caller that wants the whole result set at once.
#[allow(dead_code)]
pub fn search(
    conn: &Connection,
    query: &str,
    opts: &SearchOpts,
    filters: &SearchFilters,
) -> Result<Vec<SearchHit>, String> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let re = build_regex(query, opts)?;
    let mut hits = Vec::new();
    search_streaming(conn, &re, filters, |h| hits.push(h), || false)?;
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(conn: &Connection) {
        super::super::db::init_schema(conn).unwrap();
        conn.execute(
            "INSERT INTO session_files (session_path, project, mtime, size, indexed_at)
             VALUES ('/a.jsonl','~/app',100,1,0),('/b.jsonl','~/lib',200,1,0)",
            [],
        )
        .unwrap();
        let rows = [
            ("/a.jsonl", "~/app", "user", "please fix the parser Bug today"),
            ("/a.jsonl", "~/app", "assistant", "the bug is an off-by-one"),
            ("/a.jsonl", "~/app", "thinking", "no relevant text here"),
            ("/b.jsonl", "~/lib", "tool_use", "Read\nfile_path: /src/bug.rs"),
        ];
        for (i, (sp, proj, src, text)) in rows.iter().enumerate() {
            conn.execute(
                "INSERT INTO blocks (session_path, project, ts, line_no, block_no, uuid, source, text)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                rusqlite::params![sp, proj, 1000i64 + i as i64, i as i64, 0i64, "u", src, text],
            )
            .unwrap();
        }
    }

    #[test]
    fn plain_is_case_insensitive_by_default() {
        let conn = Connection::open_in_memory().unwrap();
        seed(&conn);
        let hits = search(&conn, "bug", &SearchOpts::default(), &SearchFilters::default()).unwrap();
        // "Bug", "bug", "bug.rs" → 3 (thinking row has no "bug").
        assert_eq!(hits.len(), 3);
        // Recency order: /b.jsonl (mtime 200) first.
        assert_eq!(hits[0].session_path, "/b.jsonl");
    }

    #[test]
    fn case_sensitive_toggle() {
        let conn = Connection::open_in_memory().unwrap();
        seed(&conn);
        let opts = SearchOpts { case_sensitive: true, ..Default::default() };
        let hits = search(&conn, "Bug", &opts, &SearchFilters::default()).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].snippet.contains("Bug"), true);
    }

    #[test]
    fn whole_word_toggle() {
        // Whole-word excludes substrings inside larger words, but a boundary
        // like "bug.rs" (`.` is a non-word char) still matches.
        let re = build_regex("bug", &SearchOpts { whole_word: true, ..Default::default() })
            .unwrap();
        assert!(re.is_match("a bug here"));
        assert!(re.is_match("path/bug.rs"));
        assert!(!re.is_match("debugging"));
        assert!(!re.is_match("bugs"));

        // In the seed, "Bug", "bug" and "bug.rs" all match as whole words (3);
        // the thinking row has no "bug".
        let conn = Connection::open_in_memory().unwrap();
        seed(&conn);
        let opts = SearchOpts { whole_word: true, ..Default::default() };
        let hits = search(&conn, "bug", &opts, &SearchFilters::default()).unwrap();
        assert_eq!(hits.len(), 3);
    }

    #[test]
    fn source_and_project_filters() {
        let conn = Connection::open_in_memory().unwrap();
        seed(&conn);
        let filters = SearchFilters {
            sources: vec!["assistant".into()],
            ..Default::default()
        };
        let hits = search(&conn, "bug", &SearchOpts::default(), &filters).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source, "assistant");

        let filters = SearchFilters {
            projects: vec!["~/lib".into()],
            ..Default::default()
        };
        let hits = search(&conn, "bug", &SearchOpts::default(), &filters).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].project, "~/lib");
    }

    #[test]
    fn snippet_ranges_point_at_the_match() {
        let re = build_regex("bug", &SearchOpts::default()).unwrap();
        let (snippet, ranges) = build_snippet("please fix the bug now", &re).unwrap();
        assert_eq!(ranges.len(), 1);
        let (s, e) = ranges[0];
        let chars: Vec<char> = snippet.chars().collect();
        let got: String = chars[s as usize..e as usize].iter().collect();
        assert_eq!(got.to_lowercase(), "bug");
    }

    #[test]
    fn bad_regex_is_an_error_not_a_panic() {
        let opts = SearchOpts { regex: true, ..Default::default() };
        assert!(build_regex("(unclosed", &opts).is_err());
    }

    /// Capstone: write a realistic multi-source session to disk, run the real
    /// parallel indexer, then search it — extract → index → query as one flow.
    #[test]
    fn end_to_end_index_then_search() {
        use std::path::Path;

        let base = std::env::temp_dir().join("ccstudio_e2e_search");
        let _ = std::fs::remove_dir_all(&base);
        let proj = base.join("-home-user-app");
        std::fs::create_dir_all(&proj).unwrap();
        let jsonl = concat!(
            r#"{"type":"user","uuid":"u1","timestamp":"2026-07-02T10:00:00.000Z","cwd":"/home/user/app","message":{"content":"please fix the parser bug"}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"a1","timestamp":"2026-07-02T10:00:05.000Z","message":{"content":[{"type":"thinking","thinking":"analyze the parser structure"},{"type":"text","text":"reading parser"},{"type":"tool_use","name":"Read","input":{"file_path":"/home/user/app/src/parser.rs"}}]}}"#,
            "\n",
            r#"{"type":"user","uuid":"u2","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"off-by-one bug at line 42"}]}]}}"#,
            "\n",
        );
        std::fs::write(proj.join("s.jsonl"), jsonl).unwrap();

        let mut conn = Connection::open_in_memory().unwrap();
        super::super::db::init_schema(&conn).unwrap();
        super::super::index::build_index_parallel(&mut conn, &base, Some(Path::new("/home/user")))
            .unwrap();

        // "bug" (case-insensitive) → the user message + the tool_result.
        let hits = search(&conn, "bug", &SearchOpts::default(), &SearchFilters::default()).unwrap();
        assert_eq!(hits.len(), 2);
        let sources: std::collections::HashSet<_> =
            hits.iter().map(|h| h.source.as_str()).collect();
        assert!(sources.contains("user") && sources.contains("tool_result"));
        assert!(hits.iter().all(|h| !h.snippet.is_empty() && !h.match_ranges.is_empty()));

        // "parser" restricted to thinking → the thinking block only, uuid preserved.
        let f = SearchFilters { sources: vec!["thinking".into()], ..Default::default() };
        let think = search(&conn, "parser", &SearchOpts::default(), &f).unwrap();
        assert_eq!(think.len(), 1);
        assert_eq!(think[0].source, "thinking");
        assert_eq!(think[0].uuid, "a1");

        // A file path inside a tool_use is searchable; project came from cwd.
        let path_hits =
            search(&conn, "parser.rs", &SearchOpts::default(), &SearchFilters::default()).unwrap();
        assert_eq!(path_hits.len(), 1);
        assert_eq!(path_hits[0].source, "tool_use");
        assert_eq!(path_hits[0].project, "~/app");

        let _ = std::fs::remove_dir_all(&base);
    }
}
