//! The project roster: `<data root>/projects.json`, one small file (records
//! are tiny and few — one-file-per-record earns nothing here, unlike pieces).
//! A project is a named, colored grouping for pieces; tabs, the compose-box
//! tint, and piece-span hues all key off it. Pieces reference projects by
//! `id`, so a rename or recolor never touches piece files.
//!
//! Colors are palette KEYS, never hex — the fixed preset set below, each
//! mapping to a `--project-<key>` CSS token the theme file owns (stored data
//! carries intent; dark-mode contrast stays retunable in one place, and a
//! user can never pick an unreadable arbitrary hex).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::store::LoadError;

/// The fixed palette keys (contract § Project model). An enum, not a String:
/// serde rejects an unknown key at every boundary for free — a typo'd color
/// can neither arrive from the frontend nor load from a hand-edited roster
/// as a token the CSS has no variable for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaletteColor {
    Red,
    Orange,
    Yellow,
    Green,
    Teal,
    Blue,
    Purple,
    Pink,
    Graphite,
}

/// One roster entry. `path` is future-auto-scoping metadata — no behavior
/// hangs on it this round (contract's deliberate deferral).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub color: PaletteColor,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub path: Option<String>,
    pub created_at: u64,
    /// Hand-added extra fields survive round-trip, same bet as piece files.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// What `save_project` accepts: the editable fields. `created_at` and
/// unknown extras are backend-owned, merged from the stored record on
/// update — mirroring `PieceInput` so a frontend round-trip can't drop them.
#[derive(Debug, Clone, Deserialize)]
pub struct ProjectInput {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub color: PaletteColor,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub path: Option<String>,
}

/// The on-disk shape: `{"projects": [...]}` — a named wrapper (not a bare
/// array) so the file self-describes and future roster-level fields have a
/// home without a format break.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Roster {
    #[serde(default)]
    projects: Vec<Project>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

fn roster_path(root: &Path) -> PathBuf {
    root.join("projects.json")
}

/// Load the roster, reporting whether repair was needed. A missing file is
/// an empty roster (fresh install); a corrupted file gets an in-memory
/// jsonrepair attempt (same § Store robustness posture as pieces — the file
/// is never rewritten; the repaired roster persists on the next explicit
/// project save); a file that still cannot be parsed is an ERROR naming the
/// file — never a silent empty roster, which would read as every project
/// vanishing (and would cascade every project-scoped piece into
/// global-with-error).
fn read_roster(root: &Path) -> Result<(Roster, bool), String> {
    let path = roster_path(root);
    if !path.is_file() {
        return Ok((Roster::default(), false));
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    match serde_json::from_str(&content) {
        Ok(roster) => Ok((roster, false)),
        Err(strict_err) => super::repair::repair_to_value(&content)
            .and_then(|v| serde_json::from_value::<Roster>(v).ok())
            .map(|roster| (roster, true))
            .ok_or_else(|| {
                format!(
                    "projects.json cannot be parsed even after repair ({strict_err}) — fix or remove the file; the roster is never silently reset"
                )
            }),
    }
}

fn load_roster(root: &Path) -> Result<Roster, String> {
    Ok(read_roster(root)?.0)
}

/// The amber notice for a roster repair that SUCCEEDED (contract § Store
/// robustness): repair can silently drop truncated records, and the roster
/// has no per-record recovered flag — so even a successful repair must
/// surface, as a `piece_load_errors` entry naming the file, cueing the user
/// to inspect before the next project save persists the repaired form.
/// `None` when the roster is absent, clean, or unrepairable (the loud `Err`
/// from `list_projects` owns that last case).
pub fn roster_repair_notice(root: &Path) -> Option<LoadError> {
    match read_roster(root) {
        Ok((_, true)) => Some(LoadError {
            file: "projects.json".to_string(),
            error: "projects.json was repaired in memory (hand-edit corruption); repair can silently drop truncated records — inspect the file before the next project save persists the repaired form".to_string(),
        }),
        _ => None,
    }
}

/// Atomic write (temp sibling + rename), pretty-printed + trailing newline —
/// the same hand-editing-surface conventions as piece files.
fn save_roster(root: &Path, roster: &Roster) -> Result<(), String> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let mut pretty = serde_json::to_string_pretty(roster).map_err(|e| e.to_string())?;
    pretty.push('\n');
    let tmp = root.join(".tmp-projects.json");
    fs::write(&tmp, pretty).map_err(|e| e.to_string())?;
    fs::rename(&tmp, roster_path(root)).map_err(|e| e.to_string())
}

pub fn load_projects(root: &Path) -> Result<Vec<Project>, String> {
    Ok(load_roster(root)?.projects)
}

/// Create (no id) or update (id present) a roster entry. An update keeps
/// `created_at` and hand-added extras; an id that matches no entry is an
/// upsert-create with that id (same rationale as pieces: erroring would
/// strand an edit made while the roster changed underneath).
pub fn save_project_at(root: &Path, input: ProjectInput, now: u64) -> Result<Project, String> {
    let name = input.name.trim();
    if name.is_empty() {
        // A nameless project renders as a blank tab — refuse early with a
        // message the settings popover can show verbatim.
        return Err("project name cannot be empty".to_string());
    }
    let mut roster = load_roster(root)?;
    let existing = input
        .id
        .as_ref()
        .and_then(|id| roster.projects.iter_mut().find(|p| &p.id == id));
    let project = match existing {
        Some(prev) => {
            prev.name = name.to_string();
            prev.color = input.color;
            prev.pinned = input.pinned;
            prev.path = input.path;
            prev.clone()
        }
        None => {
            let project = Project {
                id: input.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                name: name.to_string(),
                color: input.color,
                pinned: input.pinned,
                path: input.path,
                created_at: now,
                extra: Map::new(),
            };
            roster.projects.push(project.clone());
            project
        }
    };
    save_roster(root, &roster)?;
    Ok(project)
}

/// Remove a roster entry. Idempotent (an absent id is Ok, matching the
/// command contract's `null` return). The caller rescopes the project's
/// pieces FIRST — so a crash between the two steps leaves a still-listed
/// project with global pieces (harmless, re-deletable), never pieces
/// pointing at a ghost.
pub fn delete_project_at(root: &Path, id: &str) -> Result<(), String> {
    let mut roster = load_roster(root)?;
    let before = roster.projects.len();
    roster.projects.retain(|p| p.id != id);
    if roster.projects.len() == before {
        return Ok(()); // absent already — nothing to write
    }
    save_roster(root, &roster)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root(name: &str) -> PathBuf {
        let d = std::env::temp_dir()
            .join(format!("ccdeck-projects-test-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn input(name: &str, color: PaletteColor) -> ProjectInput {
        ProjectInput { id: None, name: name.to_string(), color, pinned: false, path: None }
    }

    #[test]
    fn missing_roster_is_empty_not_an_error() {
        let root = tmp_root("fresh");
        assert_eq!(load_projects(&root).unwrap(), vec![]);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn create_update_delete_round_trip() {
        let root = tmp_root("crud");
        let created = save_project_at(&root, input("ccdeck", PaletteColor::Blue), 100).unwrap();
        assert!(uuid::Uuid::parse_str(&created.id).is_ok());
        assert_eq!(created.created_at, 100);

        let mut rename = input("ccdeck-renamed", PaletteColor::Teal);
        rename.id = Some(created.id.clone());
        rename.pinned = true;
        let updated = save_project_at(&root, rename, 200).unwrap();
        assert_eq!(updated.name, "ccdeck-renamed");
        assert_eq!(updated.color, PaletteColor::Teal);
        assert!(updated.pinned);
        assert_eq!(updated.created_at, 100, "created_at never moves on update");
        assert_eq!(load_projects(&root).unwrap().len(), 1, "update is not a duplicate");

        delete_project_at(&root, &created.id).unwrap();
        assert_eq!(load_projects(&root).unwrap(), vec![]);
        delete_project_at(&root, &created.id).unwrap(); // idempotent
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn hand_added_extras_survive_update() {
        let root = tmp_root("extras");
        let raw = r#"{
            "projects": [{
                "id": "p1", "name": "n", "color": "pink", "pinned": true,
                "created_at": 1, "my_note": "hand-added"
            }],
            "roster_note": "also hand-added"
        }"#;
        fs::write(roster_path(&root), raw).unwrap();

        let mut update = input("n2", PaletteColor::Pink);
        update.id = Some("p1".to_string());
        update.pinned = true;
        save_project_at(&root, update, 2).unwrap();

        let reread: Value =
            serde_json::from_str(&fs::read_to_string(roster_path(&root)).unwrap()).unwrap();
        assert_eq!(reread["projects"][0]["my_note"], "hand-added");
        assert_eq!(reread["roster_note"], "also hand-added");
        assert_eq!(reread["projects"][0]["name"], "n2", "the edit itself must land");
        assert_eq!(reread["projects"][0]["created_at"], 1);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn corrupt_roster_recovers_in_memory_and_file_stays_untouched() {
        let root = tmp_root("repair");
        // Trailing comma + comment — the hand-edit shapes § Store robustness
        // names. Must load (never silently reset the roster) with the file
        // byte-identical; the repaired form persists only on the next save.
        let corrupt = r#"{
            // my projects
            "projects": [
                {"id": "p1", "name": "n", "color": "blue", "created_at": 1},
            ]
        }"#;
        fs::write(roster_path(&root), corrupt).unwrap();

        let projects = load_projects(&root).unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].id, "p1");
        assert_eq!(fs::read_to_string(roster_path(&root)).unwrap(), corrupt);

        // The next explicit save persists the repaired roster, well-formed.
        save_project_at(&root, input("second", PaletteColor::Red), 2).unwrap();
        let reread: Value =
            serde_json::from_str(&fs::read_to_string(roster_path(&root)).unwrap()).unwrap();
        assert_eq!(reread["projects"].as_array().unwrap().len(), 2);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn record_dropping_roster_repair_is_surfaced_not_silent() {
        // Audit MED 3: truncation mid-record repairs "successfully" — the
        // structure closes, the tail record is silently gone, and serde
        // happily loads the survivors. The loss mode is invisible in the
        // Ok value, which is exactly why repair-success must surface a
        // notice: it's the user's only cue to inspect before the next
        // project save bakes the loss in.
        let root = tmp_root("drop-notice");
        // Cut at the record boundary: the whole second record is gone, and
        // what remains repairs CLEANLY into a one-record roster. (A cut
        // mid-record repairs into an invalid record and takes the loud-Err
        // path instead — that case is covered below.)
        let truncated = r#"{"projects":[{"id":"p1","name":"one","color":"blue","created_at":1},"#;
        fs::write(roster_path(&root), truncated).unwrap();

        let projects = load_projects(&root).unwrap();
        assert!(
            projects.len() < 2,
            "premise: repair drops the truncated tail record (got {projects:?})"
        );

        let notice = roster_repair_notice(&root).expect("repair-success must produce the notice");
        assert_eq!(notice.file, "projects.json");
        assert!(notice.error.contains("repaired in memory"), "{}", notice.error);
        assert_eq!(
            fs::read_to_string(roster_path(&root)).unwrap(),
            truncated,
            "the notice cues inspection; the loader still never rewrites"
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn clean_absent_and_unrepairable_rosters_produce_no_notice() {
        let root = tmp_root("no-notice");
        assert!(roster_repair_notice(&root).is_none(), "absent roster: no notice");

        save_project_at(&root, input("n", PaletteColor::Blue), 1).unwrap();
        assert!(roster_repair_notice(&root).is_none(), "clean roster: no notice");

        fs::write(roster_path(&root), "\"just a string\"").unwrap();
        assert!(
            roster_repair_notice(&root).is_none(),
            "unrepairable roster: the loud list_projects Err owns it, not the amber notice"
        );
        assert!(load_projects(&root).is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn unknown_color_key_is_rejected_loudly() {
        let root = tmp_root("bad-color");
        fs::write(
            roster_path(&root),
            r##"{"projects":[{"id":"p1","name":"n","color":"#ff0000","created_at":1}]}"##,
        )
        .unwrap();
        let err = load_projects(&root).unwrap_err();
        assert!(err.contains("projects.json"), "error must name the file: {err}");
        // The hex value is exactly the mistake the palette-key design
        // prevents — it must never load as a color the CSS can't resolve.
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn empty_or_whitespace_name_is_refused() {
        let root = tmp_root("no-name");
        assert!(save_project_at(&root, input("", PaletteColor::Red), 1).is_err());
        assert!(save_project_at(&root, input("   ", PaletteColor::Red), 1).is_err());
        assert!(!roster_path(&root).exists(), "a refused save must not create the roster");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn upsert_with_unknown_id_creates_with_that_id() {
        let root = tmp_root("upsert");
        let mut inp = input("n", PaletteColor::Green);
        inp.id = Some("chosen-id".to_string());
        let p = save_project_at(&root, inp, 1).unwrap();
        assert_eq!(p.id, "chosen-id");
        assert_eq!(p.created_at, 1);
        fs::remove_dir_all(&root).unwrap();
    }
}
