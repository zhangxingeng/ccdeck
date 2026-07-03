//! Search: a SQLite-backed cache of the *extracted* text from every session
//! JSONL, so we can substring/regex-scan it (VS Code style) without re-parsing
//! JSON on every keystroke. Not FTS5 — plain tables scanned by the `regex` crate.
//!
//! Built up over milestones:
//!   1. `db`      — open/create the DB, schema, round-trip.
//!   2. `extract` — port `parser.ts`'s block extraction to Rust.
//!   (indexer, matcher, streaming follow in later milestones.)

mod db;
mod extract;
mod index;
mod query;

// Re-exports so the rest of the crate (and later milestones) can use these
// without reaching into submodules. `#[allow(unused)]` until wired up.
#[allow(unused_imports)]
pub use db::open_db;
#[allow(unused_imports)]
pub use extract::{extract_blocks, ExtractedBlock};
#[allow(unused_imports)]
pub use index::{
    build_index, build_index_parallel, index_file, remove_from_index, session_files, sweep_index,
    IndexStats, SweepStats,
};
#[allow(unused_imports)]
pub use query::{
    build_regex, search, search_streaming, SearchFilters, SearchHit, SearchOpts, SearchSummary,
};

// Public so lib.rs can register `state::search` / `state::refresh_index` /
// `state::index_status` as Tauri commands by their real paths.
pub mod state;
