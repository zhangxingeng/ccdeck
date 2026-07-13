//! The snippet store: a snippet **is** a Markdown file. Its path relative to
//! the project folder, minus the `.md`, is its name; the file's entire content
//! is the prompt. There is no schema, no uuid, no metadata — the filesystem is
//! the source of truth.
//!
//! That is the whole point of the 0.13 design: the library is hand-editable and
//! git-committable, so a user can keep their prompts in a GitHub repo and read
//! the diffs. Two invariants fall out of it, and every writer here honors them:
//!
//! - **Never write app state into a project folder.** It is git-tracked. Usage
//!   timestamps, the project roster and the active project all live in
//!   [`super::appstate`] instead — a `last_used` write on every insert would
//!   dirty the user's git tree every time they used the app.
//! - **The app is a viewer onto a folder it does not own.** It creates and
//!   deletes the `.md` files the user asks it to, and nothing else: it never
//!   prunes a directory, never rewrites a file it did not understand, and never
//!   touches anything that is not a snippet.
//!
//! `content` is an **opaque string** to the backend. The variable grammar lives
//! on the frontend now, in one implementation; nothing here parses a body.

use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;

/// One snippet — the entire model. `name` is the identity (renaming the file
/// renames the snippet); `content` is the prompt, verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Snippet {
    pub name: String,
    pub content: String,
}

/// Reject a name that cannot be a safe relative path inside the project folder.
///
/// A trust boundary, not a formality: [`save_snippet`] creates parent
/// directories for a slashed name, so a name of `../../.ssh/authorized_keys`
/// would write clean outside the project. Names come from the frontend and are
/// untrusted input.
///
/// Each rule and its reason:
/// - non-empty, and no empty segment (`a//b`) — not a path a user can have meant;
/// - no `.` / `..` segment and no absolute path — these are the escapes;
/// - no backslash — a name is always `/`-separated, and on Windows a backslash
///   would silently act as a second separator that none of these rules checked;
/// - no `:` — the usage map keys on `<project path>::<name>`, so a colon in a
///   name makes that key ambiguous (and is illegal in a Windows filename anyway);
/// - no NUL — it truncates the path at the syscall boundary.
fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("snippet name cannot be empty".to_string());
    }
    if name.starts_with('/') {
        return Err(format!("snippet name must be relative, not absolute: {name}"));
    }
    if name.contains('\\') || name.contains(':') || name.contains('\0') {
        return Err(format!("snippet name may not contain '\\', ':' or NUL: {name}"));
    }
    for segment in name.split('/') {
        if segment.is_empty() {
            return Err(format!("snippet name has an empty path segment: {name}"));
        }
        if segment == "." || segment == ".." {
            return Err(format!("snippet name may not contain '.' or '..' segments: {name}"));
        }
    }
    Ok(())
}

/// The file a name maps to: `<project>/<name>.md`.
///
/// Belt and braces on purpose. [`validate_name`] already rejects every escape we
/// know of; this then re-checks that the *resolved* path really does sit under
/// the project root. The check is lexical because the file need not exist yet,
/// so `canonicalize` is unavailable — and if a future edit ever weakens the name
/// rules, this still holds the line.
fn snippet_path(project: &Path, name: &str) -> Result<PathBuf, String> {
    validate_name(name)?;
    let path = project.join(format!("{name}.md"));
    let escapes = path
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)))
        || !path.starts_with(project);
    if escapes {
        return Err(format!("snippet name escapes the project folder: {name}"));
    }
    Ok(path)
}

/// The name a `.md` file carries: its path relative to the project root, minus
/// the extension, always `/`-separated — so a name means the same thing on every
/// platform, and reads back into [`save_snippet`] unchanged.
fn name_of(project: &Path, file: &Path) -> Option<String> {
    let rel = file.strip_prefix(project).ok()?;
    let parts: Vec<String> =
        rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    parts.join("/").strip_suffix(".md").map(str::to_string)
}

/// Every `*.md` under `project`, recursively.
///
/// A missing or non-directory project path is a loud error, never an empty list:
/// a folder that was deleted, renamed, or sits on an unmounted drive is a real
/// failure, and `[]` would present it to the user as "you have no prompts" —
/// breakage that reads as emptiness.
///
/// A single unreadable file (permissions, non-UTF-8) is skipped and logged, not
/// fatal: one bad file must not hide every other snippet. Symlinks are not
/// followed — a symlinked directory can escape the folder or cycle — and
/// dot-directories are skipped, because the project folder is typically a git
/// repo and `.git` is not content.
pub fn scan_snippets(project: &Path) -> Result<Vec<Snippet>, String> {
    if !project.exists() {
        return Err(format!("project folder not found: {}", project.display()));
    }
    if !project.is_dir() {
        return Err(format!("project path is not a folder: {}", project.display()));
    }
    let mut out = Vec::new();
    collect(project, project, &mut out)?;
    // Deterministic; the caller re-sorts by recency or by score.
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn collect(project: &Path, dir: &Path, out: &mut Vec<Snippet>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?.flatten() {
        let path = entry.path();
        let Some(fname) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if fname.starts_with('.') {
            continue; // .git, .DS_Store, editor swap files — never content
        }
        // `file_type()` does not follow symlinks, which is exactly what we want:
        // a symlinked directory is neither recursed into nor read as a file.
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            collect(project, &path, out)?;
        } else if file_type.is_file()
            && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("md"))
        {
            let Some(name) = name_of(project, &path) else {
                continue;
            };
            match fs::read_to_string(&path) {
                Ok(content) => out.push(Snippet { name, content }),
                // Skip and report — never swallow. The file stays on disk and
                // the reason reaches the log, instead of the snippet silently
                // vanishing from the library.
                Err(e) => eprintln!("[prompts] skipping {}: {e}", path.display()),
            }
        }
    }
    Ok(())
}

/// Write `<project>/<name>.md`, creating parent directories for a slashed name.
/// Same name → the file is updated; a new name → a new snippet exists. That is
/// the whole of "Save as new": the filename is the identity.
///
/// Content is written **verbatim** — no trailing newline added, no normalization
/// applied. A save must be a byte-exact round-trip of what the user typed, or
/// the git diff this library exists to serve would show edits they never made.
/// Atomic (temp sibling + rename), so a crash cannot leave a half-written prompt.
pub fn save_snippet(project: &Path, name: &str, content: &str) -> Result<Snippet, String> {
    let path = snippet_path(project, name)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("snippet.md");
    let tmp = path.with_file_name(format!(".tmp-{fname}"));
    fs::write(&tmp, content).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(Snippet { name: name.to_string(), content: content.to_string() })
}

/// Delete the snippet's file. Idempotent: deleting one that is already gone is
/// success, not an error.
///
/// It removes that file and nothing else — in particular it does not prune a
/// directory the deletion left empty. The folder is the user's; removing a
/// directory they made is a side effect they did not ask for.
pub fn delete_snippet(project: &Path, name: &str) -> Result<(), String> {
    let path = snippet_path(project, name)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir()
            .join(format!("ccdeck-prompts-test-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn write(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    // --- the model: the filename is the name, the content is the prompt ---

    #[test]
    fn a_markdown_file_is_a_snippet_named_by_its_path() {
        let dir = tmp_dir("model");
        write(&dir, "code_review.md", "review this");
        write(&dir, "rust/borrow_checker.md", "explain lifetimes");

        let snippets = scan_snippets(&dir).unwrap();
        assert_eq!(
            snippets,
            vec![
                Snippet { name: "code_review".into(), content: "review this".into() },
                Snippet { name: "rust/borrow_checker".into(), content: "explain lifetimes".into() },
            ],
            "a subfolder is organization for free — the name is the relative path"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn save_round_trips_content_byte_exact() {
        // The library exists to produce a readable git diff. A save that added
        // or stripped a trailing newline would show edits the user never made.
        let dir = tmp_dir("verbatim");
        for content in ["no trailing newline", "trailing newline\n", "", "a\n\nb\n", "{{braces}}"] {
            save_snippet(&dir, "p", content).unwrap();
            let back = scan_snippets(&dir).unwrap();
            assert_eq!(back[0].content, content, "content must round-trip byte-exact");
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn same_name_updates_and_a_new_name_creates() {
        // "Update vs Save as new" collapses into this: the filename is identity.
        let dir = tmp_dir("identity");
        save_snippet(&dir, "p", "v1").unwrap();
        save_snippet(&dir, "p", "v2").unwrap();
        let after = scan_snippets(&dir).unwrap();
        assert_eq!(after.len(), 1, "same name updates in place");
        assert_eq!(after[0].content, "v2");

        save_snippet(&dir, "p2", "v2").unwrap();
        assert_eq!(scan_snippets(&dir).unwrap().len(), 2, "a new name is a new snippet");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn save_creates_parent_dirs_for_a_slashed_name() {
        let dir = tmp_dir("mkdir");
        save_snippet(&dir, "planning/deep/spec", "body").unwrap();
        assert!(dir.join("planning/deep/spec.md").is_file());
        assert_eq!(scan_snippets(&dir).unwrap()[0].name, "planning/deep/spec");
        fs::remove_dir_all(&dir).unwrap();
    }

    // --- the trust boundary ---

    #[test]
    fn a_name_can_never_escape_the_project_folder() {
        let root = tmp_dir("escape");
        let outside = root.join("outside");
        let project = root.join("project");
        fs::create_dir_all(&outside).unwrap();
        fs::create_dir_all(&project).unwrap();

        for hostile in [
            "../outside/pwned",
            "../../.ssh/authorized_keys",
            "a/../../outside/pwned",
            "/etc/passwd",
            "/absolute",
            "a//b",
            ".",
            "..",
            "",
            "has:colon",
            "back\\slash",
        ] {
            assert!(
                save_snippet(&project, hostile, "pwned").is_err(),
                "save must refuse the hostile name {hostile:?}"
            );
            assert!(
                delete_snippet(&project, hostile).is_err(),
                "delete must refuse the hostile name {hostile:?}"
            );
        }
        assert!(!outside.join("pwned.md").exists(), "nothing may be written outside the project");
        assert!(scan_snippets(&project).unwrap().is_empty());
        fs::remove_dir_all(&root).unwrap();
    }

    // --- failures are loud, never empty ---

    #[test]
    fn a_missing_project_folder_errors_rather_than_reading_as_empty() {
        // Fail-open-to-empty here would tell the user "you have no prompts" when
        // the truth is "your folder is gone".
        let dir = tmp_dir("missing");
        let err = scan_snippets(&dir.join("never-existed")).unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_markdown_files_are_snippets() {
        let dir = tmp_dir("md-only");
        write(&dir, "notes.txt", "not a snippet");
        write(&dir, "config.json", "{}");
        write(&dir, "real.md", "a snippet");
        let snippets = scan_snippets(&dir).unwrap();
        assert_eq!(snippets.len(), 1);
        assert_eq!(snippets[0].name, "real");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn dot_entries_are_skipped() {
        // The project folder is typically a git repo; `.git` is not content.
        let dir = tmp_dir("dotfiles");
        write(&dir, ".git/COMMIT_EDITMSG.md", "not a snippet");
        write(&dir, ".hidden.md", "not a snippet");
        write(&dir, "real.md", "a snippet");
        let snippets = scan_snippets(&dir).unwrap();
        assert_eq!(snippets.len(), 1);
        assert_eq!(snippets[0].name, "real");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn delete_removes_the_file_idempotently_and_keeps_the_directory() {
        let dir = tmp_dir("delete");
        save_snippet(&dir, "rust/thing", "body").unwrap();
        delete_snippet(&dir, "rust/thing").unwrap();
        assert!(scan_snippets(&dir).unwrap().is_empty());
        // A snippet delete is not a licence to remove a folder the user made.
        assert!(dir.join("rust").is_dir());
        delete_snippet(&dir, "rust/thing").expect("deleting an absent snippet is success");
        fs::remove_dir_all(&dir).unwrap();
    }
}
