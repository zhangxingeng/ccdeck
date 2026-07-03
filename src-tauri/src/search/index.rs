//! The indexer: walk session JSONLs, extract blocks, and populate the SQLite
//! cache. Single-threaded for now (M3); parallelised in M5.
//!
//! Each file is indexed as a delete-then-insert inside one transaction, keyed by
//! its absolute path, with a `(mtime, size)` fingerprint recorded in
//! `session_files` for later invalidation (M4).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use super::extract::extract_blocks;

/// Result of an index pass.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct IndexStats {
    pub sessions: usize,
    pub blocks: usize,
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
/// `(project_label, blocks)` without touching the DB.
pub fn extract_file(
    path: &Path,
    home: Option<&Path>,
) -> Option<(String, Vec<super::extract::ExtractedBlock>)> {
    parse_file(path, home).map(|p| (p.project, p.blocks))
}

/// Index a single session file: delete its old rows, extract fresh blocks,
/// bulk-insert them, and upsert its fingerprint — all in one transaction.
/// Returns the number of blocks written. Used by the sweep and the eager hook.
pub fn index_file(conn: &mut Connection, path: &Path, home: Option<&Path>) -> Result<usize, String> {
    let payload =
        parse_file(path, home).ok_or_else(|| format!("could not read {}", path.display()))?;
    let n = payload.blocks.len();
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    write_payload(&tx, &payload)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(n)
}

/// Full single-threaded index of every session under the projects dir.
/// (Kept as the simple reference path + test oracle; the app uses the parallel
/// build.)
#[allow(dead_code)]
pub fn build_index(
    conn: &mut Connection,
    projects_dir: &Path,
    home: Option<&Path>,
) -> Result<IndexStats, String> {
    let mut stats = IndexStats::default();
    for path in session_files(projects_dir) {
        let n = index_file(conn, &path, home)?;
        stats.sessions += 1;
        stats.blocks += n;
    }
    Ok(stats)
}

/// Everything one worker extracts from one file, handed to the writer thread.
struct FilePayload {
    session_path: String,
    project: String,
    mtime: i64,
    size: i64,
    blocks: Vec<super::extract::ExtractedBlock>,
}

/// Read + parse + extract one file into a [`FilePayload`]. Pure CPU/IO work with
/// no DB access, so it's safe to run on many threads at once. `None` if the file
/// can't be read.
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

/// Write one file's payload into the DB inside the given transaction
/// (delete-then-insert + fingerprint upsert). Shared by the serial and parallel
/// paths so the SQL lives in exactly one place.
fn write_payload(tx: &rusqlite::Transaction, p: &FilePayload) -> Result<(), String> {
    tx.execute(
        "DELETE FROM blocks WHERE session_path = ?1",
        [&p.session_path],
    )
    .map_err(|e| e.to_string())?;
    {
        let mut stmt = tx
            .prepare_cached(
                "INSERT INTO blocks
                     (session_path, project, ts, line_no, block_no, uuid, source, text)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )
            .map_err(|e| e.to_string())?;
        for b in &p.blocks {
            stmt.execute(rusqlite::params![
                p.session_path,
                p.project,
                b.ts,
                b.line_no,
                b.block_no,
                b.uuid,
                b.source,
                b.text,
            ])
            .map_err(|e| e.to_string())?;
        }
    }
    tx.execute(
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

/// Parallel full index: **parse in parallel, write serialized.**
///
/// A rayon pool fans the files across CPU cores (each worker parses one file and
/// sends a [`FilePayload`] down an `mpsc` channel); a single writer thread — the
/// only one allowed to touch the SQLite connection — drains the channel and does
/// all inserts inside one big transaction. This is the canonical shape for
/// SQLite, which permits just one writer at a time.
pub fn build_index_parallel(
    conn: &mut Connection,
    projects_dir: &Path,
    home: Option<&Path>,
) -> Result<IndexStats, String> {
    use rayon::prelude::*;
    use std::sync::mpsc;

    let files = session_files(projects_dir);
    let (sender, receiver) = mpsc::channel::<FilePayload>();
    let mut stats = IndexStats::default();

    // A scoped thread lets the producers borrow `files`/`home` without `'static`.
    std::thread::scope(|scope| -> Result<(), String> {
        // Producers: parse every file across the rayon pool, streaming results.
        scope.spawn(move || {
            files.par_iter().for_each_with(sender, |sender, path| {
                if let Some(payload) = parse_file(path, home) {
                    // Writer hung up (shouldn't happen mid-build) → drop quietly.
                    let _ = sender.send(payload);
                }
            });
            // All sender clones drop here, closing the channel.
        });

        // Consumer: the sole DB writer, one transaction for the whole build.
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        for payload in &receiver {
            write_payload(&tx, &payload)?;
            stats.sessions += 1;
            stats.blocks += payload.blocks.len();
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    })?;

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

/// Drop every cached row for a session path. Used both on deletion cleanup and
/// as the eager "this file changed" hook after our own Save/Restore.
pub fn remove_from_index(conn: &Connection, session_path: &str) -> Result<(), String> {
    conn.execute("DELETE FROM blocks WHERE session_path = ?1", [session_path])
        .map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM session_files WHERE session_path = ?1",
        [session_path],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Read every cached fingerprint into memory for cheap comparison.
fn db_fingerprints(conn: &Connection) -> Result<HashMap<String, (i64, i64)>, String> {
    let mut map = HashMap::new();
    let mut stmt = conn
        .prepare("SELECT session_path, mtime, size FROM session_files")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (p, m, s) = row.map_err(|e| e.to_string())?;
        map.insert(p, (m, s));
    }
    Ok(map)
}

/// Incremental refresh: reindex new/changed files (by `(mtime, size)`), remove
/// rows for files that no longer exist, leave unchanged files untouched. This
/// is the lazy background sweep and catches external changes (the CLI appending
/// to a session, edits made outside the app).
pub fn sweep_index(
    conn: &mut Connection,
    projects_dir: &Path,
    home: Option<&Path>,
) -> Result<SweepStats, String> {
    let db_fp = db_fingerprints(conn)?;
    let mut stats = SweepStats::default();
    let mut seen: HashSet<String> = HashSet::new();

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
                index_file(conn, &path, home)?;
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
    for p in stale {
        remove_from_index(conn, &p)?;
        stats.deleted += 1;
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

    #[test]
    fn indexes_a_dir_and_counts() {
        let base = tmp_projects("count");
        let home = Path::new("/home/user");
        let mut conn = Connection::open_in_memory().unwrap();
        super::super::db::init_schema(&conn).unwrap();

        let stats = build_index(&mut conn, &base, Some(home)).unwrap();
        assert_eq!(stats.sessions, 1);
        assert_eq!(stats.blocks, 3); // user text + assistant text + tool_use

        // Fingerprint row exists.
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM session_files", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);

        // Project label came from cwd, home-relativised.
        let project: String = conn
            .query_row("SELECT project FROM blocks LIMIT 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(project, "~/app");

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn sweep_detects_change_add_and_deletion() {
        let base = tmp_projects("sweep");
        let proj = base.join("-home-user-app");
        let mut conn = Connection::open_in_memory().unwrap();
        super::super::db::init_schema(&conn).unwrap();
        build_index(&mut conn, &base, None).unwrap();

        // No changes → nothing reindexed.
        let s = sweep_index(&mut conn, &base, None).unwrap();
        assert_eq!((s.reindexed, s.deleted, s.unchanged), (0, 0, 1));

        // Grow the file (size changes) → reindexed.
        let bigger = format!(
            "{FIXTURE}{}\n",
            r#"{"type":"user","uuid":"u2","message":{"content":"another line"}}"#
        );
        fs::write(proj.join("sess1.jsonl"), &bigger).unwrap();
        let s = sweep_index(&mut conn, &base, None).unwrap();
        assert_eq!(s.reindexed, 1);

        // Add a second session → the new one is reindexed, the old is unchanged.
        fs::write(proj.join("sess2.jsonl"), FIXTURE).unwrap();
        let s = sweep_index(&mut conn, &base, None).unwrap();
        assert_eq!((s.reindexed, s.unchanged), (1, 1));

        // Delete it → its rows are removed.
        fs::remove_file(proj.join("sess2.jsonl")).unwrap();
        let s = sweep_index(&mut conn, &base, None).unwrap();
        assert_eq!(s.deleted, 1);
        let sess2_blocks: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM blocks WHERE session_path LIKE '%sess2.jsonl'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(sess2_blocks, 0);

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn parallel_matches_serial() {
        // Several sessions across two projects.
        let base = std::env::temp_dir().join("ccstudio_idx_par");
        let _ = fs::remove_dir_all(&base);
        for proj in ["-home-user-app", "-home-user-lib"] {
            let dir = base.join(proj);
            fs::create_dir_all(&dir).unwrap();
            for i in 0..3 {
                fs::write(dir.join(format!("s{i}.jsonl")), FIXTURE).unwrap();
            }
        }

        let mut serial = Connection::open_in_memory().unwrap();
        super::super::db::init_schema(&serial).unwrap();
        let s_serial = build_index(&mut serial, &base, None).unwrap();

        let mut par = Connection::open_in_memory().unwrap();
        super::super::db::init_schema(&par).unwrap();
        let s_par = build_index_parallel(&mut par, &base, None).unwrap();

        assert_eq!(s_serial.sessions, 6);
        assert_eq!(s_par.sessions, s_serial.sessions);
        assert_eq!(s_par.blocks, s_serial.blocks);

        // Same number of rows landed in the DB either way.
        let count = |c: &Connection| -> i64 {
            c.query_row("SELECT COUNT(*) FROM blocks", [], |r| r.get(0)).unwrap()
        };
        assert_eq!(count(&par), count(&serial));

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn reindex_is_idempotent() {
        let base = tmp_projects("idem");
        let mut conn = Connection::open_in_memory().unwrap();
        super::super::db::init_schema(&conn).unwrap();

        build_index(&mut conn, &base, None).unwrap();
        build_index(&mut conn, &base, None).unwrap(); // second pass must not duplicate

        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM blocks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 3, "delete-then-insert must not double rows");

        let _ = fs::remove_dir_all(&base);
    }
}
