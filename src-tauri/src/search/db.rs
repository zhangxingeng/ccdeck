//! The SQLite cache: schema, connection opening, on-disk location.
//!
//! As of issue #5 (tantivy fuzzy/intent search), the extracted-text content
//! itself lives in a tantivy full-text index (see `index.rs`), not here. This
//! DB now holds only `session_files` (the mtime/size fingerprint table used
//! for invalidation) and `search_meta` (the engine-version marker used to
//! force a one-time full rebuild when the on-disk engine changes shape).

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension};

/// Manual bump for a change `index::schema_fingerprint` can't see on its own —
/// a tokenizer *parameter* tweak (e.g. `index::MAX_TOKEN_LEN`) or a matching-
/// semantics change that doesn't alter the tantivy `Schema`'s field shape.
/// Field/type/tokenizer-*name* changes are caught automatically: the caller
/// (`state.rs`) combines this constant with `index::schema_fingerprint()` (a
/// hash of the schema's serialized field definitions) into the effective
/// version passed to [`ensure_engine_version`] — so most schema changes need
/// no manual bump at all, only this narrower class does. A mismatch on the
/// combined version forces one full rebuild rather than carrying incremental-
/// migration logic for a disposable, always-rebuildable cache — this app
/// already ships to real users with a populated v1 (SQLite `blocks` + regex
/// scan) cache on disk, so silently reusing their stale `session_files`
/// fingerprints would skip re-extraction into the new tantivy index and
/// they'd upgrade into an empty search with nothing to trigger a rebuild.
pub const ENGINE_VERSION: &str = "2";

/// The schema. `session_files` is one row per session file, used for
/// mtime/size invalidation; `search_meta` is a tiny key/value table carrying
/// the engine-version marker.
///
/// `IF NOT EXISTS` makes this idempotent — running it on an already-initialised
/// DB is a harmless no-op, so we can call it on every open.
const SCHEMA: &str = "\
CREATE TABLE IF NOT EXISTS session_files (
  session_path TEXT PRIMARY KEY,
  project      TEXT NOT NULL,
  mtime        INTEGER NOT NULL,
  size         INTEGER NOT NULL,
  indexed_at   INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS search_meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
";

/// Where the search cache lives: `~/.claude/.ccstudio-index/search.db`
/// (same convention as `.ccstudio-backups`).
/// Creates the parent directory if it doesn't exist yet.
fn db_path() -> Result<PathBuf, String> {
    Ok(index_root()?.join("search.db"))
}

/// Where the tantivy full-text index lives: `~/.claude/.ccstudio-index/tantivy/`.
/// Creates the directory if it doesn't exist yet.
pub fn tantivy_index_dir() -> Result<PathBuf, String> {
    let dir = index_root()?.join("tantivy");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn index_root() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("could not resolve home directory")?;
    let dir = home.join(".claude").join(".ccstudio-index");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Reset `session_files` and wipe the tantivy index directory when the
/// on-disk engine version doesn't match `effective_version` (see
/// [`ENGINE_VERSION`] for how the caller composes it) — forcing exactly one
/// full rebuild into the current engine. A no-op once the version matches, so
/// this is safe to call on every startup.
pub fn ensure_engine_version(
    conn: &Connection,
    tantivy_dir: &Path,
    effective_version: &str,
) -> Result<(), String> {
    let current: Option<String> = conn
        .query_row(
            "SELECT value FROM search_meta WHERE key = 'engine_version'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if current.as_deref() == Some(effective_version) {
        return Ok(());
    }

    // Legacy v1 leftovers: the SQLite `blocks` table is no longer used.
    conn.execute_batch("DROP TABLE IF EXISTS blocks;")
        .map_err(|e| e.to_string())?;
    // Stale fingerprints would make the sweep think already-extracted files
    // don't need re-extracting into the (currently empty) tantivy index.
    conn.execute("DELETE FROM session_files", [])
        .map_err(|e| e.to_string())?;
    if tantivy_dir.exists() {
        std::fs::remove_dir_all(tantivy_dir).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(tantivy_dir).map_err(|e| e.to_string())?;

    conn.execute(
        "INSERT INTO search_meta (key, value) VALUES ('engine_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = ?1",
        [effective_version],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Apply the schema to a connection. Separated out so tests can run it against
/// an in-memory DB without touching the real one on disk.
pub fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(SCHEMA).map_err(|e| e.to_string())
}

/// Open (creating if needed) the on-disk search DB with its schema ready.
///
/// The DB is a disposable cache (always rebuildable from source JSONL), so we
/// run non-durable pragmas for speed: WAL journaling + `synchronous=OFF`. A
/// crash can corrupt it, but the fix is simply to rebuild — no user data is lost.
pub fn open_db() -> Result<Connection, String> {
    let conn = Connection::open(db_path()?).map_err(|e| e.to_string())?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=OFF;")
        .map_err(|e| e.to_string())?;
    init_schema(&conn)?;
    Ok(conn)
}

/// Open a read-only-style connection for the search path. Deliberately does NOT
/// run `init_schema` (that needs a write lock and would contend with the
/// indexer); the schema is created up front in [`open_db`] at startup. WAL is a
/// DB-level property, so a plain reader participates automatically. A short
/// busy-timeout absorbs any incidental lock during a checkpoint.
pub fn open_read() -> Result<Connection, String> {
    let conn = Connection::open(db_path()?).map_err(|e| e.to_string())?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Schema is valid SQL and a fingerprint row survives a write→read
    /// round-trip. In-memory DB, so it never touches disk.
    #[test]
    fn schema_roundtrip() {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        init_schema(&conn).expect("apply schema");

        conn.execute(
            "INSERT INTO session_files (session_path, project, mtime, size, indexed_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params!["/proj/session.jsonl", "proj", 100i64, 1i64, 0i64],
        )
        .expect("insert fingerprint");

        let project: String = conn
            .query_row(
                "SELECT project FROM session_files WHERE session_path = ?1",
                ["/proj/session.jsonl"],
                |row| row.get(0),
            )
            .expect("read fingerprint back");

        assert_eq!(project, "proj");
    }

    /// A throwaway tantivy-dir path, tagged to avoid collisions with other
    /// tests — same convention as `index.rs`'s/`query.rs`'s tagged temp dirs.
    fn tmp_dir(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("ccstudio_test_engine_version_{tag}"))
    }

    /// A stale/legacy engine version wipes fingerprints + any old `blocks`
    /// table and re-stamps the current version; a matching version is a no-op
    /// that leaves data alone.
    #[test]
    fn ensure_engine_version_resets_stale_state_but_not_current() {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        init_schema(&conn).expect("apply schema");
        // Simulate a v1 install: a legacy `blocks` table plus a fingerprint row.
        conn.execute_batch("CREATE TABLE blocks (session_path TEXT);")
            .unwrap();
        conn.execute(
            "INSERT INTO session_files (session_path, project, mtime, size, indexed_at)
             VALUES ('/a.jsonl','p',1,1,0)",
            [],
        )
        .unwrap();

        let dir = tmp_dir("reset");
        let _ = std::fs::remove_dir_all(&dir);

        ensure_engine_version(&conn, &dir, "test-version").expect("first call resets stale v1 state");
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM session_files", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "stale fingerprints must be cleared");
        assert!(
            conn.execute_batch("SELECT * FROM blocks").is_err(),
            "legacy blocks table must be dropped"
        );

        // Re-insert a fingerprint, then call again: version now matches, so
        // it must NOT be cleared a second time.
        conn.execute(
            "INSERT INTO session_files (session_path, project, mtime, size, indexed_at)
             VALUES ('/b.jsonl','p',1,1,0)",
            [],
        )
        .unwrap();
        ensure_engine_version(&conn, &dir, "test-version").expect("second call is a no-op");
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM session_files", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "matching version must not reset fingerprints again");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
