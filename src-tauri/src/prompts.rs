//! Prompt Library: a Markdown snippet store, the hybrid match engine, and their
//! Tauri commands. Engineering contract: the prompts design doc in project_docs.
//!
//! The model, in one line: **a snippet is a Markdown file whose filename is its
//! name, and a project is a name and a folder.** Every `*.md` under the folder,
//! recursively, is one of its snippets. There is no uuid and no cross-reference —
//! the filesystem is the source of truth, which is what makes the library
//! hand-editable and git-committable.
//!
//! Modules:
//!   `store`    — the `.md` files: recursive scan, save, delete. Content is an
//!                opaque string here; the variable grammar lives on the frontend.
//!   `appstate` — `<data root>/prompts-state.json`: the project roster, the
//!                active project, and usage timestamps. It exists so that none of
//!                that ever gets written into a user's git-tracked prompt folder.
//!   `lexical`  — the always-on fzf-style scorer, weighted name over content.
//!   `embed`    — the semantic path: pinned model + ONNX Runtime download, sqlite
//!                vector cache, linear cosine KNN. Background-only; no commands.
//!   `state`    — managed state, hybrid fusion, and the Tauri commands.

mod appstate;
mod embed;
mod lexical;
mod store;

// Public so lib.rs can register the commands by their real paths.
pub mod state;
