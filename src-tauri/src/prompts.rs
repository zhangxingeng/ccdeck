//! Prompt Library (issue #24): the JSON piece store, the hybrid match engine,
//! and their Tauri commands. Engineering contract: the prompts design doc in
//! project_docs — storage layout, piece schema, command surface, engine rules.
//!
//! Modules:
//!   `store`   — one-JSON-per-piece store under `<data root>/prompts/`,
//!               hand-editable, unknown fields preserved, append-only body
//!               versioning.
//!   `projects` — the project roster (`<data root>/projects.json`): named,
//!               colored, pinnable groupings pieces reference by id.
//!   `grammar` — the shared single-brace variable grammar (seam contract:
//!               the TS lane implements the identical spec, both assert the
//!               contract's shared vectors).
//!   `lexical` — the always-on fzf-style weighted subsequence scorer.
//!   `embed`   — the opt-in semantic path: pinned model + ONNX Runtime
//!               download, sqlite embedding cache, linear cosine KNN.
//!   `state`   — managed state, hybrid fusion, and the Tauri commands.

mod embed;
mod grammar;
mod lexical;
mod projects;
mod repair;
mod store;

// Public so lib.rs can register the commands by their real paths.
pub mod state;
