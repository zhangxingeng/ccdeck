//! The SQLite cache: schema, connection opening, on-disk location.
//!
//! As of issue #5 (tantivy fuzzy/intent search), the extracted-text content
//! itself lives in a tantivy full-text index (see `index.rs`), not here. This
//! DB now holds only `session_files` (the mtime/size fingerprint table used
//! for invalidation) and `search_meta` (the engine-version marker used to
//! force a one-time full rebuild when the on-disk engine changes shape).

use std::fs::File;
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

/// Where the search cache lives: `~/.ccdeck/index/search.db`.
/// Creates the parent directory if it doesn't exist yet.
fn db_path() -> Result<PathBuf, String> {
    Ok(index_root()?.join("search.db"))
}

/// Where the tantivy full-text index lives: `~/.ccdeck/index/tantivy/`.
/// Creates the directory if it doesn't exist yet.
pub fn tantivy_index_dir() -> Result<PathBuf, String> {
    let dir = index_root()?.join("tantivy");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// `<data root>/index` — moved out of `~/.claude/.ccstudio-index` (issue #24
/// de-contamination invariant; the datadir startup migration relocates an
/// existing cache, and a missed migration just re-indexes here from scratch).
fn index_root() -> Result<PathBuf, String> {
    let dir = crate::datadir::data_root()?.join("index");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Where the cross-process migration lock lives: a dotfile next to
/// `tantivy_dir` (its *parent*, not inside it — `tantivy_dir` itself gets
/// `remove_dir_all`'d during a reset, and deleting a file out from under an
/// open handle mid-lock is asking for trouble, especially on Windows).
/// Named after `tantivy_dir`'s own basename so tests using distinct tagged
/// temp dirs (see `tmp_dir` below) don't contend with each other.
fn migration_lock_path(tantivy_dir: &Path) -> PathBuf {
    let name = tantivy_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let parent = tantivy_dir.parent().unwrap_or(tantivy_dir);
    parent.join(format!(".{name}.migration.lock"))
}

/// Block until we hold the exclusive cross-process migration lock. Released
/// automatically when the returned `File` is dropped (i.e. at the end of
/// [`ensure_engine_version`]'s call frame).
fn acquire_migration_lock(tantivy_dir: &Path) -> Result<File, String> {
    let path = migration_lock_path(tantivy_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    // `File::lock` (std, stabilized 1.89) blocks until it holds an exclusive
    // advisory lock (flock on Unix, LockFileEx on Windows); released when
    // `file` is dropped.
    file.lock().map_err(|e| e.to_string())?;
    Ok(file)
}

/// Reset `session_files` and wipe the tantivy index directory when the
/// on-disk engine version doesn't match `effective_version` (see
/// [`ENGINE_VERSION`] for how the caller composes it) — forcing exactly one
/// full rebuild into the current engine. A no-op once the version matches, so
/// this is safe to call on every startup.
///
/// Guarded by a cross-process file lock (issue #12): two app instances
/// launching at once (most likely right after an upgrade, when every
/// instance sees a stale version) used to race the version check against
/// each other — one instance's fingerprint clear could land after another had
/// already started repopulating them, and the two `remove_dir_all`/
/// `create_dir_all` pairs against the same tantivy directory could interleave.
/// Now the whole check-and-reset runs under an exclusive lock; a second
/// instance blocks until the first finishes, then re-checks the version
/// (already current) and takes the no-op path instead of racing the wipe.
pub fn ensure_engine_version(
    conn: &Connection,
    tantivy_dir: &Path,
    effective_version: &str,
) -> Result<(), String> {
    let _lock = acquire_migration_lock(tantivy_dir)?;

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

    /// Regression test for issue #12: two "processes" (here, two independently
    /// opened `File` handles on the same lock path, which is exactly what two
    /// separate OS processes would each hold) racing for the migration lock
    /// must serialize, not interleave. The waiter must not acquire the lock
    /// until the holder has released it.
    #[test]
    fn migration_lock_serializes_concurrent_processes() {
        let dir = tmp_dir("lock");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let order = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        let holder_dir = dir.clone();
        let holder_order = order.clone();
        let holder = std::thread::spawn(move || {
            let _lock = acquire_migration_lock(&holder_dir).expect("holder acquires lock");
            holder_order.lock().unwrap().push("holder-acquired");
            std::thread::sleep(std::time::Duration::from_millis(150));
            holder_order.lock().unwrap().push("holder-released");
            // `_lock` drops here, releasing the flock.
        });

        // Give the holder a head start so it wins the race for the lock.
        std::thread::sleep(std::time::Duration::from_millis(30));

        let waiter_dir = dir.clone();
        let waiter_order = order.clone();
        let waiter = std::thread::spawn(move || {
            let _lock = acquire_migration_lock(&waiter_dir).expect("waiter acquires lock");
            waiter_order.lock().unwrap().push("waiter-acquired");
        });

        holder.join().unwrap();
        waiter.join().unwrap();

        let order = order.lock().unwrap();
        let released_idx = order.iter().position(|e| *e == "holder-released").unwrap();
        let waiter_idx = order.iter().position(|e| *e == "waiter-acquired").unwrap();
        assert!(
            waiter_idx > released_idx,
            "waiter must not acquire the lock until the holder releases it: {:?}",
            *order
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
