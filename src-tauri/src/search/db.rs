//! The SQLite cache: schema, connection opening, on-disk location.

use std::path::PathBuf;
use std::time::Duration;

use rusqlite::Connection;

/// The schema. `blocks` is one row per extracted content block; `session_files`
/// is one row per session file, used for mtime/size invalidation.
///
/// `IF NOT EXISTS` makes this idempotent — running it on an already-initialised
/// DB is a harmless no-op, so we can call it on every open.
///
/// `uuid` on `blocks` is the source message's uuid: the frontend re-groups
/// entries into turns and flattens blocks, so a raw line number can't reliably
/// locate a hit — but (uuid, block_no) survives that regrouping, which is what
/// jump-to-hit needs.
const SCHEMA: &str = "\
CREATE TABLE IF NOT EXISTS session_files (
  session_path TEXT PRIMARY KEY,
  project      TEXT NOT NULL,
  mtime        INTEGER NOT NULL,
  size         INTEGER NOT NULL,
  indexed_at   INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS blocks (
  session_path TEXT NOT NULL,
  project      TEXT NOT NULL,
  ts           INTEGER,
  line_no      INTEGER NOT NULL,
  block_no     INTEGER NOT NULL,
  uuid         TEXT NOT NULL DEFAULT '',
  source       TEXT NOT NULL,
  text         TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS blocks_session ON blocks(session_path);
CREATE INDEX IF NOT EXISTS blocks_project ON blocks(project);
CREATE INDEX IF NOT EXISTS blocks_ts      ON blocks(ts);
CREATE INDEX IF NOT EXISTS blocks_source  ON blocks(source);
";

/// Where the search cache lives: `~/.claude/.ccstudio-index/search.db`
/// (same convention as `.ccstudio-backups`).
/// Creates the parent directory if it doesn't exist yet.
fn db_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("could not resolve home directory")?;
    let dir = home.join(".claude").join(".ccstudio-index");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("search.db"))
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

    /// Milestone 1 proof: schema is valid SQL and a row survives a
    /// write→read round-trip. In-memory DB, so it never touches disk.
    #[test]
    fn schema_roundtrip() {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        init_schema(&conn).expect("apply schema");

        conn.execute(
            "INSERT INTO blocks
                 (session_path, project, ts, line_no, block_no, uuid, source, text)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                "/proj/session.jsonl",
                "proj",
                1_720_000_000_000i64,
                0i64,
                0i64,
                "uuid-1",
                "user",
                "hello search"
            ],
        )
        .expect("insert block");

        let text: String = conn
            .query_row(
                "SELECT text FROM blocks WHERE session_path = ?1",
                ["/proj/session.jsonl"],
                |row| row.get(0),
            )
            .expect("read block back");

        assert_eq!(text, "hello search");
    }
}
