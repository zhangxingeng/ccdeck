//! The indexer: walk session JSONLs, extract blocks, and populate the tantivy
//! full-text index (BM25 + fuzzy matching — see `query.rs`). The
//! `session_files` SQLite table (mtime/size fingerprints) still lives
//! alongside it for cheap invalidation, unchanged in spirit from before
//! issue #5 — only the searchable-content store moved from a SQLite `blocks`
//! table to a tantivy index.
//!
//! Each file is indexed as a delete-then-add against the tantivy writer, keyed
//! by its `session_path` term, with a `(mtime, size)` fingerprint recorded in
//! `session_files` for later invalidation.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use tantivy::directory::MmapDirectory;
use tantivy::schema::{
    Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, FAST, INDEXED, STORED, STRING,
};
use tantivy::tokenizer::{LowerCaser, RemoveLongFilter, SimpleTokenizer, TextAnalyzer};
use tantivy::{doc, Index, IndexWriter, TantivyDocument, Term};

use super::extract::{extract_blocks, ExtractedBlock};

/// Heap budget handed to a tantivy `IndexWriter`. Generous enough for this
/// app's ~1GB-max corpus without tuning; tantivy spills to disk as needed.
pub const WRITER_HEAP_BYTES: usize = 50_000_000;

/// Name under which we register our own copy of tantivy's built-in "default"
/// tokenizer pipeline (`SimpleTokenizer` -> `RemoveLongFilter` -> `LowerCaser`),
/// with a higher length ceiling — see `MAX_TOKEN_LEN`. Query-side tokenizing
/// (`query.rs::tokenize`) must look up this same name, or query tokens won't
/// line up with the terms the index actually stored.
pub const TEXT_TOKENIZER: &str = "ccstudio_text";

/// tantivy's built-in "default" tokenizer applies `RemoveLongFilter::limit(40)`,
/// silently dropping — at INDEX time, so no query-time fuzziness can recover it
/// — any token 40+ characters long. That's routine content for this app's
/// domain (a git SHA, a long hash, a long identifier pasted into chat), so the
/// default ceiling made that content permanently unsearchable with no error
/// surfaced anywhere (caught in the issue #5 Gate-2 audit). 200 comfortably
/// covers a SHA-256 hex digest (64 chars) or a UUID (36) while still bounding
/// a pathological single "token" (e.g. a minified blob with no whitespace)
/// from bloating the index.
const MAX_TOKEN_LEN: usize = 200;

/// Register [`TEXT_TOKENIZER`] on `index` — every place that opens or creates
/// an `Index` must do this, since tokenizer *implementations* (unlike the
/// schema itself) aren't persisted to disk; only the tokenizer *name* is
/// stored per-field, and the actual pipeline has to be re-registered in
/// memory each time the process starts. Centralized in `open_index` below so
/// every caller (production and tests) gets it for free.
fn register_tokenizer(index: &Index) {
    let analyzer = TextAnalyzer::builder(SimpleTokenizer::default())
        .filter(RemoveLongFilter::limit(MAX_TOKEN_LEN))
        .filter(LowerCaser)
        .build();
    index.tokenizers().register(TEXT_TOKENIZER, analyzer);
}

/// A structural fingerprint of the current [`SearchSchema`] — every field's
/// name, type, and indexing options (including the tokenizer *name*, so
/// renaming `TEXT_TOKENIZER` or changing a field's shape both register here)
/// serialize deterministically, so hashing that catches most schema changes
/// automatically rather than relying on a human remembering to bump
/// `db::ENGINE_VERSION` by hand (the actual gap an issue #5 Gate-2 audit
/// found: nothing tied the version bump to the schema actually changing
/// shape). This does NOT see a tokenizer *parameter* tweak (e.g. changing
/// `MAX_TOKEN_LEN`'s value without renaming `TEXT_TOKENIZER`) — the tokenizer
/// pipeline itself isn't part of the serialized `Schema`, only its name is —
/// so `db::ENGINE_VERSION` still needs a manual bump for that narrower class
/// of change.
pub fn schema_fingerprint() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let schema = SearchSchema::build().schema;
    let json = serde_json::to_string(&schema).unwrap_or_default();
    let mut hasher = DefaultHasher::new();
    json.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// Result of an index pass.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct IndexStats {
    pub sessions: usize,
    pub blocks: usize,
}

/// The tantivy schema plus its field handles, so callers don't restring field
/// names. One instance is built once at startup and shared by writer + reader.
#[derive(Clone)]
pub struct SearchSchema {
    pub schema: Schema,
    pub session_path: Field,
    pub project: Field,
    pub ts: Field,
    pub line_no: Field,
    pub block_no: Field,
    pub uuid: Field,
    pub source: Field,
    /// First line of tool_use text (the tool's name), or "" for non-tool_use
    /// blocks. A dedicated field so the `tool_name` filter is an exact-term
    /// match instead of emulating a `LIKE 'name\n%'` prefix scan.
    pub tool_name: Field,
    pub text: Field,
}

impl SearchSchema {
    pub fn build() -> Self {
        let mut b = Schema::builder();
        let session_path = b.add_text_field("session_path", STRING | STORED);
        let project = b.add_text_field("project", STRING | STORED);
        let ts = b.add_i64_field("ts", INDEXED | STORED | FAST);
        let line_no = b.add_i64_field("line_no", STORED);
        let block_no = b.add_i64_field("block_no", STORED);
        let uuid = b.add_text_field("uuid", STRING | STORED);
        let source = b.add_text_field("source", STRING | STORED);
        let tool_name = b.add_text_field("tool_name", STRING | STORED);
        // Same shape as the `TEXT` constant (tokenized, `WithFreqsAndPositions`,
        // stored) but naming our own tokenizer instead of "default" — see
        // `TEXT_TOKENIZER`/`MAX_TOKEN_LEN` above.
        let text_indexing = TextFieldIndexing::default()
            .set_tokenizer(TEXT_TOKENIZER)
            .set_index_option(IndexRecordOption::WithFreqsAndPositions);
        let text_options = TextOptions::default()
            .set_indexing_options(text_indexing)
            .set_stored();
        let text = b.add_text_field("text", text_options);
        let schema = b.build();
        Self {
            schema,
            session_path,
            project,
            ts,
            line_no,
            block_no,
            uuid,
            source,
            tool_name,
            text,
        }
    }
}

/// Open (creating if needed) the tantivy index at `dir` with the current
/// [`SearchSchema`]. `dir` must already exist (see `db::tantivy_index_dir`).
pub fn open_index(dir: &Path, schema: &Schema) -> Result<Index, String> {
    let directory = MmapDirectory::open(dir).map_err(|e| e.to_string())?;
    let index = Index::open_or_create(directory, schema.clone()).map_err(|e| e.to_string())?;
    register_tokenizer(&index);
    Ok(index)
}

fn unix_secs(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// First-seen `"cwd"` value via a cheap substring scan (paths don't contain
/// quotes, so we don't need full JSON parsing here). Mirrors `list_sessions`.
fn first_cwd(content: &str) -> Option<String> {
    for line in content.lines() {
        if let Some(idx) = line.find("\"cwd\":\"") {
            let rest = &line[idx + 7..];
            if let Some(end) = rest.find('"') {
                let c = &rest[..end];
                if !c.is_empty() {
                    return Some(c.to_string());
                }
            }
        }
    }
    None
}

/// Home-relative project label from the real cwd (`~/workspace/app`), falling
/// back to the encoded project dir name when no cwd is recorded.
fn project_label(cwd: Option<&str>, dir_name: &str, home: Option<&Path>) -> String {
    if let Some(cwd) = cwd {
        if let Some(home) = home {
            let home_s = home.to_string_lossy();
            if cwd == home_s {
                return "~".to_string();
            }
            if let Some(rest) = cwd.strip_prefix(&format!("{home_s}/")) {
                return format!("~/{rest}");
            }
        }
        return cwd.to_string();
    }
    dir_name.to_string()
}

/// Discover indexable session files: every `*.jsonl` directly under a project
/// dir, excluding `agent-*.jsonl` and the `subagents`/`tool-results` dirs.
pub fn session_files(projects_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(top) = fs::read_dir(projects_dir) else {
        return out;
    };
    for top in top.flatten() {
        let p = top.path();
        if !p.is_dir() {
            continue;
        }
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name == "subagents" || name == "tool-results" {
            continue;
        }
        let Ok(inner) = fs::read_dir(&p) else {
            continue;
        };
        for e in inner.flatten() {
            let fp = e.path();
            let fname = fp.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if fname.ends_with(".jsonl") && !fname.starts_with("agent-") {
                out.push(fp);
            }
        }
    }
    out
}

/// Read + extract one file for the cold search path, returning its
/// `(project_label, blocks)` without touching the index.
pub fn extract_file(path: &Path, home: Option<&Path>) -> Option<(String, Vec<ExtractedBlock>)> {
    parse_file(path, home).map(|p| (p.project, p.blocks))
}

/// Everything one worker extracts from one file, handed to the writer thread.
struct FilePayload {
    session_path: String,
    project: String,
    mtime: i64,
    size: i64,
    blocks: Vec<ExtractedBlock>,
}

/// Read + parse + extract one file into a [`FilePayload`]. Pure CPU/IO work with
/// no DB/index access, so it's safe to run on many threads at once. `None` if
/// the file can't be read.
fn parse_file(path: &Path, home: Option<&Path>) -> Option<FilePayload> {
    let meta = fs::metadata(path).ok()?;
    let mtime = meta.modified().map(unix_secs).unwrap_or(0);
    let size = meta.len() as i64;
    let content = fs::read_to_string(path).ok()?;
    let dir_name = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("");
    let project = project_label(first_cwd(&content).as_deref(), dir_name, home);
    let blocks = extract_blocks(&content);
    Some(FilePayload {
        session_path: path.to_string_lossy().to_string(),
        project,
        mtime,
        size,
        blocks,
    })
}

/// First line of a tool_use block's text is the tool name (see `extract.rs`'s
/// `"{name}\n{flattened input}"` shape); everything else has no tool name.
fn tool_name_of<'a>(source: &str, text: &'a str) -> &'a str {
    if source == "tool_use" {
        text.split('\n').next().unwrap_or("")
    } else {
        ""
    }
}

/// Stage one file's blocks into the tantivy writer (delete-then-add) and
/// upsert its fingerprint in `session_files`. Does NOT commit either side —
/// callers batch many files into one commit for throughput; see
/// `build_index_parallel`/`sweep_index`. `index_file` (single-file path) does
/// commit, since it's called standalone.
fn stage_payload(
    conn: &Connection,
    writer: &IndexWriter,
    schema: &SearchSchema,
    p: &FilePayload,
) -> Result<(), String> {
    writer
        .delete_term(Term::from_field_text(schema.session_path, &p.session_path));
    for b in &p.blocks {
        let tool_name = tool_name_of(&b.source, &b.text);
        let mut document = TantivyDocument::default();
        document.add_text(schema.session_path, &p.session_path);
        document.add_text(schema.project, &p.project);
        if let Some(ts) = b.ts {
            document.add_i64(schema.ts, ts);
        }
        document.add_i64(schema.line_no, b.line_no);
        document.add_i64(schema.block_no, b.block_no);
        document.add_text(schema.uuid, &b.uuid);
        document.add_text(schema.source, &b.source);
        document.add_text(schema.tool_name, tool_name);
        document.add_text(schema.text, &b.text);
        writer
            .add_document(document)
            .map_err(|e| e.to_string())?;
    }
    conn.execute(
        "INSERT INTO session_files (session_path, project, mtime, size, indexed_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(session_path)
         DO UPDATE SET project = ?2, mtime = ?3, size = ?4, indexed_at = ?5",
        rusqlite::params![
            p.session_path,
            p.project,
            p.mtime,
            p.size,
            unix_secs(SystemTime::now())
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Index a single session file: delete its old docs, extract fresh blocks,
/// add them, and upsert its fingerprint — committing both sides immediately
/// (this is the eager after-Save path, not a batch). Returns the number of
/// blocks written.
pub fn index_file(
    conn: &Connection,
    writer: &mut IndexWriter,
    schema: &SearchSchema,
    path: &Path,
    home: Option<&Path>,
) -> Result<usize, String> {
    let payload =
        parse_file(path, home).ok_or_else(|| format!("could not read {}", path.display()))?;
    let n = payload.blocks.len();
    stage_payload(conn, writer, schema, &payload)?;
    writer.commit().map_err(|e| e.to_string())?;
    Ok(n)
}

/// Parallel full index: **parse in parallel, write serialized.**
///
/// A rayon pool fans the files across CPU cores (each worker parses one file and
/// sends a [`FilePayload`] down an `mpsc` channel); a single writer thread — the
/// only one allowed to touch the tantivy writer / sqlite connection — drains the
/// channel and stages every payload, committing once at the end. Same shape as
/// before issue #5, just writing into a tantivy index instead of SQLite rows.
pub fn build_index_parallel(
    conn: &mut Connection,
    writer: &mut IndexWriter,
    schema: &SearchSchema,
    projects_dir: &Path,
    home: Option<&Path>,
) -> Result<IndexStats, String> {
    use rayon::prelude::*;
    use std::sync::mpsc;

    let files = session_files(projects_dir);
    let (sender, receiver) = mpsc::channel::<FilePayload>();
    let mut stats = IndexStats::default();

    std::thread::scope(|scope| -> Result<(), String> {
        scope.spawn(move || {
            files.par_iter().for_each_with(sender, |sender, path| {
                if let Some(payload) = parse_file(path, home) {
                    let _ = sender.send(payload);
                }
            });
        });

        let tx = conn.transaction().map_err(|e| e.to_string())?;
        for payload in &receiver {
            stage_payload(&tx, writer, schema, &payload)?;
            stats.sessions += 1;
            stats.blocks += payload.blocks.len();
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    })?;

    writer.commit().map_err(|e| e.to_string())?;
    Ok(stats)
}

/// What an invalidation sweep changed.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct SweepStats {
    /// Files that were new or whose fingerprint changed → reindexed.
    pub reindexed: usize,
    /// Files gone from disk → their rows removed.
    pub deleted: usize,
    /// Files whose `(mtime, size)` matched the cache → left alone.
    pub unchanged: usize,
}

/// Drop every indexed doc + fingerprint for a session path. Used both on
/// deletion cleanup and as the eager "this file changed" hook after our own
/// Save/Restore. Commits immediately (standalone call, not part of a batch).
pub fn remove_from_index(
    conn: &Connection,
    writer: &mut IndexWriter,
    schema: &SearchSchema,
    session_path: &str,
) -> Result<(), String> {
    writer.delete_term(Term::from_field_text(schema.session_path, session_path));
    writer.commit().map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM session_files WHERE session_path = ?1",
        [session_path],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Read every cached fingerprint into memory for cheap comparison.
fn db_fingerprints(conn: &Connection) -> Result<HashMap<String, (i64, i64)>, String> {
    let mut stmt = conn
        .prepare("SELECT session_path, mtime, size FROM session_files")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?))
        })
        .map_err(|e| e.to_string())?;
    let mut out = HashMap::new();
    for row in rows {
        let (p, m, s) = row.map_err(|e| e.to_string())?;
        out.insert(p, (m, s));
    }
    Ok(out)
}

/// Incremental refresh: reindex new/changed files (by `(mtime, size)`), remove
/// docs for files that no longer exist, leave unchanged files untouched. This
/// is the lazy background sweep and catches external changes (the CLI appending
/// to a session, edits made outside the app), plus the one-time full rebuild
/// forced by `db::ensure_engine_version` after an engine upgrade (every file
/// looks "new" once `session_files` has been cleared).
pub fn sweep_index(
    conn: &mut Connection,
    writer: &mut IndexWriter,
    schema: &SearchSchema,
    projects_dir: &Path,
    home: Option<&Path>,
) -> Result<SweepStats, String> {
    let db_fp = db_fingerprints(conn)?;
    let mut stats = SweepStats::default();
    let mut seen: HashSet<String> = HashSet::new();
    let mut wrote_any = false;

    {
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        for path in session_files(projects_dir) {
            let sp = path.to_string_lossy().to_string();
            seen.insert(sp.clone());

            let Ok(meta) = fs::metadata(&path) else {
                continue;
            };
            let mtime = meta.modified().map(unix_secs).unwrap_or(0);
            let size = meta.len() as i64;

            match db_fp.get(&sp) {
                Some(&(m, s)) if m == mtime && s == size => stats.unchanged += 1,
                _ => {
                    if let Some(payload) = parse_file(&path, home) {
                        stage_payload(&tx, writer, schema, &payload)?;
                        wrote_any = true;
                    }
                    stats.reindexed += 1;
                }
            }
        }

        // Anything in the cache but no longer on disk is stale — drop it.
        let stale: Vec<String> = db_fp
            .keys()
            .filter(|p| !seen.contains(*p))
            .cloned()
            .collect();
        for p in &stale {
            writer.delete_term(Term::from_field_text(schema.session_path, p));
            tx.execute("DELETE FROM session_files WHERE session_path = ?1", [p])
                .map_err(|e| e.to_string())?;
            wrote_any = true;
        }
        stats.deleted = stale.len();

        tx.commit().map_err(|e| e.to_string())?;
    }

    if wrote_any {
        writer.commit().map_err(|e| e.to_string())?;
    }

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = concat!(
        r#"{"type":"user","uuid":"u1","timestamp":"2026-07-02T10:00:00.000Z","cwd":"/home/user/app","message":{"content":"hello indexer"}}"#,
        "\n",
        r#"{"type":"assistant","uuid":"a1","message":{"content":[{"type":"text","text":"hi there"},{"type":"tool_use","name":"Read","input":{"file_path":"/x.rs"}}]}}"#,
        "\n",
    );

    /// Build a throwaway projects dir with one session file.
    fn tmp_projects(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("ccstudio_idx_{tag}"));
        let _ = fs::remove_dir_all(&base);
        let proj = base.join("-home-user-app");
        fs::create_dir_all(&proj).unwrap();
        fs::write(proj.join("sess1.jsonl"), FIXTURE).unwrap();
        base
    }

    /// A throwaway tantivy index + writer for one test, plus the sqlite conn.
    fn tmp_engine(tag: &str) -> (Connection, Index, IndexWriter, SearchSchema) {
        let dir = std::env::temp_dir().join(format!("ccstudio_idx_tantivy_{tag}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let schema = SearchSchema::build();
        let index = open_index(&dir, &schema.schema).unwrap();
        let writer = index.writer(WRITER_HEAP_BYTES).unwrap();
        let conn = Connection::open_in_memory().unwrap();
        super::super::db::init_schema(&conn).unwrap();
        (conn, index, writer, schema)
    }

    fn doc_count(index: &Index) -> usize {
        let reader = index.reader().unwrap();
        reader.searcher().num_docs() as usize
    }

    #[test]
    fn indexes_a_dir_and_counts() {
        let base = tmp_projects("count");
        let home = Path::new("/home/user");
        let (mut conn, index, mut writer, schema) = tmp_engine("count");

        let stats = build_index_parallel(&mut conn, &mut writer, &schema, &base, Some(home)).unwrap();
        assert_eq!(stats.sessions, 1);
        assert_eq!(stats.blocks, 3); // user text + assistant text + tool_use
        assert_eq!(doc_count(&index), 3);

        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM session_files", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn sweep_detects_change_add_and_deletion() {
        let base = tmp_projects("sweep");
        let proj = base.join("-home-user-app");
        let (mut conn, _index, mut writer, schema) = tmp_engine("sweep");
        build_index_parallel(&mut conn, &mut writer, &schema, &base, None).unwrap();

        // No changes → nothing reindexed.
        let s = sweep_index(&mut conn, &mut writer, &schema, &base, None).unwrap();
        assert_eq!((s.reindexed, s.deleted, s.unchanged), (0, 0, 1));

        // Grow the file (size changes) → reindexed.
        let bigger = format!(
            "{FIXTURE}{}\n",
            r#"{"type":"user","uuid":"u2","message":{"content":"another line"}}"#
        );
        fs::write(proj.join("sess1.jsonl"), &bigger).unwrap();
        let s = sweep_index(&mut conn, &mut writer, &schema, &base, None).unwrap();
        assert_eq!(s.reindexed, 1);

        // Add a second session → the new one is reindexed, the old is unchanged.
        fs::write(proj.join("sess2.jsonl"), FIXTURE).unwrap();
        let s = sweep_index(&mut conn, &mut writer, &schema, &base, None).unwrap();
        assert_eq!((s.reindexed, s.unchanged), (1, 1));

        // Delete it → its docs are removed.
        fs::remove_file(proj.join("sess2.jsonl")).unwrap();
        let s = sweep_index(&mut conn, &mut writer, &schema, &base, None).unwrap();
        assert_eq!(s.deleted, 1);

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn reindex_is_idempotent() {
        let base = tmp_projects("idem");
        let (mut conn, index, mut writer, schema) = tmp_engine("idem");

        build_index_parallel(&mut conn, &mut writer, &schema, &base, None).unwrap();
        build_index_parallel(&mut conn, &mut writer, &schema, &base, None).unwrap(); // second pass must not duplicate

        assert_eq!(doc_count(&index), 3, "delete-then-add must not double docs");

        let _ = fs::remove_dir_all(&base);
    }
}
