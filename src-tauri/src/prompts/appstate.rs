//! App-local state: `<data root>/prompts-state.json` — the project roster, the
//! active project, and last-used timestamps.
//!
//! **This file exists so that nothing here ever lands in a project folder.**
//! A project folder is the user's git repo. If a `last_used` timestamp were
//! written into a snippet file (or a sidecar next to it) on every insert, using
//! the app would dirty the user's git tree every single time — and reading the
//! diffs is the entire reason the library is Markdown-in-git. So everything the
//! *app* knows, as opposed to what the *user wrote*, lives out here instead.
//!
//! The roster is the only place a project exists at all: a project is a name and
//! a folder, and its snippets are simply the `*.md` files inside that folder.
//! There is no id, no cross-reference, and therefore nothing that can drift out
//! of sync with the filesystem.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// A project: a name and a folder. Nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub path: PathBuf,
}

/// What `list_projects` answers with. The roster alone is not enough: the active
/// project is persisted and restored on launch, so the frontend has to be able
/// to read it back, and the command surface has no separate getter for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectList {
    pub projects: Vec<Project>,
    pub active: Option<PathBuf>,
}

/// The on-disk shape of `prompts-state.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct State {
    #[serde(default)]
    projects: Vec<Project>,
    #[serde(default)]
    active: Option<PathBuf>,
    /// `<project path>::<snippet name>` → last-used epoch seconds. The only
    /// input to the at-rest sort order. `BTreeMap` for deterministic key order,
    /// so the file has a stable diff if a user ever looks at it.
    #[serde(default)]
    usage: BTreeMap<String, u64>,
}

fn state_path(root: &Path) -> PathBuf {
    root.join("prompts-state.json")
}

pub fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

/// The usage key. `::` is unambiguous because a snippet name may not contain a
/// colon (enforced in `store::validate_name`) and a project path is absolute.
fn usage_key(project: &Path, name: &str) -> String {
    format!("{}::{}", project.display(), name)
}

/// Read the state file. A missing file is a fresh install (empty state), but a
/// file that exists and cannot be parsed is a **loud error**, never a silent
/// reset: quietly returning an empty roster would read to the user as every one
/// of their projects having vanished, and the next save would then persist that
/// emptiness over the top of the file that still held them.
fn load(root: &Path) -> Result<State, String> {
    let path = state_path(root);
    if !path.is_file() {
        return Ok(State::default());
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| {
        format!(
            "prompts-state.json cannot be parsed ({e}) — fix or remove the file; the project roster is never silently reset"
        )
    })
}

/// Atomic write (temp sibling + rename), pretty-printed with a trailing newline.
fn save(root: &Path, state: &State) -> Result<(), String> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let mut pretty = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    pretty.push('\n');
    let tmp = root.join(".tmp-prompts-state.json");
    fs::write(&tmp, pretty).map_err(|e| e.to_string())?;
    fs::rename(&tmp, state_path(root)).map_err(|e| e.to_string())
}

pub fn list_projects(root: &Path) -> Result<ProjectList, String> {
    let state = load(root)?;
    Ok(ProjectList { projects: state.projects, active: state.active })
}

/// The active project's folder, if one is set — the launch-time restore.
pub fn active_project(root: &Path) -> Result<Option<PathBuf>, String> {
    Ok(load(root)?.active)
}

/// Add a project, or rename the one already registered at this path (a path is
/// a project's identity here, so re-adding it is a rename, not a duplicate).
///
/// The folder must already exist: a project *is* a folder, so registering a path
/// that isn't one would create a roster entry whose every future scan errors.
/// The path is canonicalized so that two spellings of the same folder cannot
/// become two projects with divergent usage keys.
pub fn add_project(root: &Path, name: &str, path: &Path) -> Result<Project, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("project name cannot be empty".to_string());
    }
    if !path.is_dir() {
        return Err(format!("project folder does not exist: {}", path.display()));
    }
    let path = path.canonicalize().map_err(|e| format!("{}: {e}", path.display()))?;

    let mut state = load(root)?;
    let project = Project { name: name.to_string(), path: path.clone() };
    match state.projects.iter_mut().find(|p| p.path == path) {
        Some(existing) => existing.name = name.to_string(),
        None => state.projects.push(project.clone()),
    }
    // First project becomes active, or the user would have a roster and no
    // selection — a state the UI has no way to leave.
    if state.active.is_none() {
        state.active = Some(path);
    }
    save(root, &state)?;
    Ok(project)
}

/// Forget a project. **It never deletes files.**
///
/// The user's prompts are their own; the app is a viewer onto a folder it does
/// not own. Removing a project drops the roster entry (and its now-orphaned
/// usage keys, which are app state, not user data) and nothing more — every
/// `.md` file stays exactly where it was, so re-adding the folder restores the
/// project intact.
pub fn remove_project(root: &Path, path: &Path) -> Result<(), String> {
    let mut state = load(root)?;
    let before = state.projects.len();
    state.projects.retain(|p| p.path != path);
    if state.projects.len() == before {
        return Ok(()); // absent already — idempotent
    }
    let prefix = format!("{}::", path.display());
    state.usage.retain(|k, _| !k.starts_with(&prefix));
    if state.active.as_deref() == Some(path) {
        // Fall back to any remaining project rather than leaving the UI with a
        // roster and no selection.
        state.active = state.projects.first().map(|p| p.path.clone());
    }
    save(root, &state)
}

/// Persist the active project; restored on launch. Only a rostered path may be
/// active — anything else would leave the app pointing at a project it cannot
/// list.
pub fn set_active_project(root: &Path, path: &Path) -> Result<(), String> {
    let mut state = load(root)?;
    if !state.projects.iter().any(|p| p.path == path) {
        return Err(format!("not a known project: {}", path.display()));
    }
    state.active = Some(path.to_path_buf());
    save(root, &state)
}

/// Record that a snippet was used. This is the write that must never touch the
/// project folder — see the module header.
pub fn touch_snippet(root: &Path, project: &Path, name: &str) -> Result<(), String> {
    let mut state = load(root)?;
    state.usage.insert(usage_key(project, name), unix_now());
    save(root, &state)
}

/// Drop a snippet's usage entry — called when it is deleted, so the map does not
/// accumulate keys for prompts that no longer exist.
pub fn forget_snippet(root: &Path, project: &Path, name: &str) -> Result<(), String> {
    let mut state = load(root)?;
    if state.usage.remove(&usage_key(project, name)).is_none() {
        return Ok(());
    }
    save(root, &state)
}

/// Last-used timestamps for one project's snippets, keyed by snippet name. The
/// only input to the at-rest (empty-query) sort order.
pub fn usage_for(root: &Path, project: &Path) -> Result<BTreeMap<String, u64>, String> {
    let prefix = format!("{}::", project.display());
    Ok(load(root)?
        .usage
        .into_iter()
        .filter_map(|(k, v)| k.strip_prefix(&prefix).map(|name| (name.to_string(), v)))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir()
            .join(format!("ccdeck-appstate-test-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// A root for app state, plus a folder standing in for the user's git-tracked
    /// prompt repo.
    fn fixture(name: &str) -> (PathBuf, PathBuf) {
        let root = tmp_dir(name);
        let project = root.join("prompt_repo");
        fs::create_dir_all(&project).unwrap();
        (root, project.canonicalize().unwrap())
    }

    #[test]
    fn a_project_is_a_name_and_a_folder_and_the_first_one_becomes_active() {
        let (root, project) = fixture("add");
        let added = add_project(&root, "juror", &project).unwrap();
        assert_eq!(added, Project { name: "juror".into(), path: project.clone() });

        let list = list_projects(&root).unwrap();
        assert_eq!(list.projects, vec![added]);
        assert_eq!(list.active, Some(project), "the first project must become active");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn re_adding_a_path_renames_rather_than_duplicating() {
        // The path is the project's identity; two entries for one folder would
        // give it two divergent sets of usage keys.
        let (root, project) = fixture("rename");
        add_project(&root, "old name", &project).unwrap();
        add_project(&root, "new name", &project).unwrap();
        let list = list_projects(&root).unwrap();
        assert_eq!(list.projects.len(), 1);
        assert_eq!(list.projects[0].name, "new name");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_project_folder_must_exist() {
        let (root, _) = fixture("missing");
        assert!(add_project(&root, "ghost", &root.join("nope")).is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn remove_project_forgets_the_path_and_never_deletes_files() {
        // The whole contract of remove_project, in one assertion: the user's
        // prompts are their own.
        let (root, project) = fixture("remove");
        fs::write(project.join("keep.md"), "my prompt").unwrap();
        add_project(&root, "juror", &project).unwrap();

        remove_project(&root, &project).unwrap();

        assert!(list_projects(&root).unwrap().projects.is_empty(), "the roster entry is gone");
        assert!(project.is_dir(), "the folder must survive");
        assert_eq!(
            fs::read_to_string(project.join("keep.md")).unwrap(),
            "my prompt",
            "every .md file must survive, byte for byte"
        );
        remove_project(&root, &project).expect("removing an absent project is success");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn removing_the_active_project_falls_back_to_another() {
        let (root, first) = fixture("active-fallback");
        let second = first.parent().unwrap().join("other_repo");
        fs::create_dir_all(&second).unwrap();
        let second = second.canonicalize().unwrap();

        add_project(&root, "first", &first).unwrap();
        add_project(&root, "second", &second).unwrap();
        assert_eq!(list_projects(&root).unwrap().active, Some(first.clone()));

        remove_project(&root, &first).unwrap();
        assert_eq!(
            list_projects(&root).unwrap().active,
            Some(second),
            "the UI must never be left with a roster and no selection"
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn active_project_is_persisted_and_only_a_rostered_path_may_be_set() {
        let (root, first) = fixture("active");
        let second = first.parent().unwrap().join("other_repo");
        fs::create_dir_all(&second).unwrap();
        let second = second.canonicalize().unwrap();

        add_project(&root, "first", &first).unwrap();
        add_project(&root, "second", &second).unwrap();
        set_active_project(&root, &second).unwrap();

        // Re-read from disk: this is the launch-time restore.
        assert_eq!(active_project(&root).unwrap(), Some(second));
        assert!(
            set_active_project(&root, &root.join("unknown")).is_err(),
            "activating an unrostered path would point the app at a project it cannot list"
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn usage_is_recorded_outside_the_project_folder() {
        // The invariant this whole module exists for: using the app must not
        // dirty the user's git tree.
        let (root, project) = fixture("usage");
        fs::write(project.join("p.md"), "body").unwrap();
        add_project(&root, "juror", &project).unwrap();

        touch_snippet(&root, &project, "p").unwrap();

        let usage = usage_for(&root, &project).unwrap();
        assert!(usage.contains_key("p"));
        let entries: Vec<_> = fs::read_dir(&project).unwrap().flatten().collect();
        assert_eq!(entries.len(), 1, "no state file may appear inside the project folder");
        assert_eq!(entries[0].file_name(), "p.md");
        assert_eq!(
            fs::read_to_string(project.join("p.md")).unwrap(),
            "body",
            "and the snippet file itself must be untouched"
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn usage_is_scoped_per_project_and_forgotten_with_the_snippet() {
        let (root, project) = fixture("usage-scope");
        let other = project.parent().unwrap().join("other_repo");
        fs::create_dir_all(&other).unwrap();
        let other = other.canonicalize().unwrap();
        add_project(&root, "a", &project).unwrap();
        add_project(&root, "b", &other).unwrap();

        touch_snippet(&root, &project, "shared_name").unwrap();
        assert!(usage_for(&root, &project).unwrap().contains_key("shared_name"));
        assert!(
            !usage_for(&root, &other).unwrap().contains_key("shared_name"),
            "same snippet name in two projects must not share a usage entry"
        );

        forget_snippet(&root, &project, "shared_name").unwrap();
        assert!(usage_for(&root, &project).unwrap().is_empty());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_corrupt_state_file_errors_rather_than_resetting_the_roster() {
        let (root, _) = fixture("corrupt");
        fs::write(state_path(&root), "{ not json").unwrap();
        let err = list_projects(&root).unwrap_err();
        assert!(err.contains("never silently reset"), "got: {err}");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_missing_state_file_is_a_fresh_install() {
        let (root, _) = fixture("fresh");
        let list = list_projects(&root).unwrap();
        assert!(list.projects.is_empty());
        assert_eq!(list.active, None);
        fs::remove_dir_all(&root).unwrap();
    }
}
