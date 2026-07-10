//! The piece store: one hand-editable JSON file per piece under
//! `<data root>/prompts/`. Product bets this file enforces (issue #24):
//! a user can hand any piece file to any LLM and load it back — so unknown
//! fields are never silently dropped — and a save never destroys the previous
//! body (append-only `versions`).
//!
//! The `id` field is canonical, not the filename: the loader trusts content
//! over filename so a hand-copied file with a stale name still loads, and
//! saves always land at `<id>.json` (cleaning up stale-named twins).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::grammar::{self, Placeholder};

/// Where a piece applies: everywhere, or one roster project referenced by id
/// (the roster owns name/color, so a rename or recolor never touches piece
/// files). Legacy/unknown scope shapes — the pre-revision path-keyed form, or
/// an id no roster entry matches — load as Global plus a `piece_load_errors`
/// entry, file untouched (see [`scan_pieces`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Scope {
    #[default]
    Global,
    Project { project_id: String },
}

/// One prior body, pushed when a save changes the body. `saved_at` is when
/// that body was last saved (the piece's `updated_at` at push time).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Version {
    pub body: String,
    pub saved_at: u64,
    /// Hand-edited extra fields on a version entry survive round-trip too.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The canonical piece schema (contract). Field order here is the on-disk
/// order (serde serializes declaration-first, flattened extras last).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Piece {
    pub id: String,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub scope: Scope,
    #[serde(default)]
    pub placeholders: Vec<Placeholder>,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default)]
    pub versions: Vec<Version>,
    /// Unknown fields from hand-edited files, preserved verbatim on
    /// round-trip. serde_json keeps u64/i64 integers exact (the "numbers past
    /// 2^53" hazard is a JavaScript float problem, covered by tests here so a
    /// regression is loud).
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// What `save_piece` accepts from the frontend: the editable fields only.
/// `versions`, timestamps, and unknown extras are owned by the backend —
/// merged from the stored piece on update so a frontend round-trip can never
/// drop a hand-edited field it doesn't know about.
#[derive(Debug, Clone, Deserialize)]
pub struct PieceInput {
    #[serde(default)]
    pub id: Option<String>,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub scope: Scope,
}

/// `<data root>/prompts` — created on first save, not at resolve time.
pub fn prompts_dir() -> Result<PathBuf, String> {
    Ok(crate::datadir::data_root()?.join("prompts"))
}

pub(super) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Parse one piece file's content: strict JSON first, then scope
/// normalization — a legacy/unknown scope (or, when the roster is readable,
/// a `project_id` no roster entry matches) loads as Global and comes back as
/// a load error alongside the piece: visible, non-fatal, file untouched.
/// `known_project_ids: None` means the roster could not be consulted — id
/// validation is suspended rather than falsely degrading every project piece.
fn parse_piece(
    content: &str,
    fname: &str,
    known_project_ids: Option<&HashSet<String>>,
) -> Result<(Piece, Option<LoadError>), String> {
    let mut value: Value = serde_json::from_str(content).map_err(|e| e.to_string())?;
    let scope_error = normalize_scope(&mut value, fname, known_project_ids);
    let piece: Piece = serde_json::from_value(value).map_err(|e| e.to_string())?;
    Ok((piece, scope_error))
}

/// Rewrite an unusable `scope` IN MEMORY to global, returning the honest
/// notice. The no-dual-schema call (contract): the feature never shipped in
/// a release, so this notice — not a migration — is the whole path.
fn normalize_scope(
    value: &mut Value,
    fname: &str,
    known_project_ids: Option<&HashSet<String>>,
) -> Option<LoadError> {
    let global = serde_json::json!({ "kind": "global" });
    let scope = value.get("scope")?; // absent → serde default (Global), no notice
    match serde_json::from_value::<Scope>(scope.clone()) {
        Ok(Scope::Global) => None,
        Ok(Scope::Project { project_id }) => match known_project_ids {
            Some(ids) if !ids.contains(&project_id) => {
                value["scope"] = global;
                Some(LoadError {
                    file: fname.to_string(),
                    error: format!(
                        "scope references unknown project {project_id}; loaded as global (file untouched)"
                    ),
                })
            }
            _ => None,
        },
        Err(_) => {
            let legacy = scope.clone();
            value["scope"] = global;
            Some(LoadError {
                file: fname.to_string(),
                error: format!(
                    "unrecognized scope {legacy} (pre-release shape?); loaded as global (file untouched)"
                ),
            })
        }
    }
}

/// A piece file the loader could not honor: broken JSON, or shadowed by a
/// duplicate id. Surfaced to the UI via the `piece_load_errors` command —
/// the hand-editing user (this feature's core persona) never sees stderr, so
/// without this a broken comma makes a piece silently vanish from the
/// library, which reads as data loss. The file itself always stays intact on
/// disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LoadError {
    pub file: String,
    pub error: String,
}

/// Load every piece in `dir`, collecting per-file errors. A file that fails
/// to parse is reported and skipped — never deleted or rewritten (the user's
/// hand-edit stays intact on disk to fix; failing the whole library for one
/// bad file would hide every other piece). Duplicate ids (hand-copied files):
/// the file actually named `<id>.json` wins, the shadowed ones are reported.
/// Pieces sorted newest-updated first as a sensible default.
pub fn scan_pieces(
    dir: &Path,
    known_project_ids: Option<&HashSet<String>>,
) -> Result<(Vec<Piece>, Vec<LoadError>), String> {
    if !dir.is_dir() {
        return Ok((Vec::new(), Vec::new()));
    }
    // (piece, filename_is_canonical, filename) — filename kept so a
    // duplicate-id error can name the actual shadowed file, whichever scan
    // order the two arrived in.
    let mut pieces: Vec<(Piece, bool, String)> = Vec::new();
    let mut errors: Vec<LoadError> = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        let Some(fname) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !fname.ends_with(".json") || fname.starts_with('.') {
            continue; // dotfiles include our own crash-leftover temp files
        }
        let piece: Piece = match fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|s| parse_piece(&s, fname, known_project_ids))
        {
            Ok((p, scope_notice)) => {
                errors.extend(scope_notice);
                p
            }
            Err(e) => {
                errors.push(LoadError { file: fname.to_string(), error: e });
                continue;
            }
        };
        let canonical = fname == format!("{}.json", piece.id);
        if let Some(existing) = pieces.iter_mut().find(|(p, _, _)| p.id == piece.id) {
            // Winner: the canonically-named file, else first-seen.
            let loser = if canonical && !existing.1 {
                std::mem::replace(existing, (piece, true, fname.to_string())).2
            } else {
                fname.to_string()
            };
            let id = &existing.0.id;
            errors.push(LoadError {
                file: loser.clone(),
                error: format!("duplicate piece id {id} — {} wins; {loser} is ignored", existing.2),
            });
            continue;
        }
        pieces.push((piece, canonical, fname.to_string()));
    }
    let mut out: Vec<Piece> = pieces.into_iter().map(|(p, _, _)| p).collect();
    out.sort_by_key(|p| std::cmp::Reverse(p.updated_at));
    Ok((out, errors))
}

/// [`scan_pieces`] for callers that only need the pieces. Errors still land
/// on stderr so headless contexts keep a trace; the UI-visible surface is the
/// `piece_load_errors` command, which runs its own fresh scan (stateless —
/// it can never serve stale errors from an earlier pass).
pub fn load_pieces(
    dir: &Path,
    known_project_ids: Option<&HashSet<String>>,
) -> Result<Vec<Piece>, String> {
    let (pieces, errors) = scan_pieces(dir, known_project_ids)?;
    for e in &errors {
        eprintln!("[prompts] skipping piece file {}: {}", e.file, e.error);
    }
    Ok(pieces)
}

/// Resolve the stored piece a save to `id` would update — refusing whenever
/// proceeding would overwrite `<id>.json` content we could not read (audit
/// L2: the loader SKIPS an unparseable file, so resolving through it would
/// turn the save into a create and destroy the broken file's versions/extra,
/// violating "a save never destroys a prior body"). Same refusal when the
/// file parses but holds a DIFFERENT piece's id (hand-edited): writing over
/// it would destroy that other piece's data.
fn resolve_existing(dir: &Path, id: &str) -> Result<Option<Piece>, String> {
    let canonical = dir.join(format!("{id}.json"));
    if canonical.is_file() {
        let content = fs::read_to_string(&canonical).map_err(|e| e.to_string())?;
        // parse_piece (not bare serde): a legacy-scope file must stay
        // saveable — the explicit save is exactly the moment its normalized
        // scope is allowed to persist. Scope validation is skipped (None):
        // the save overwrites `scope` from the input anyway.
        let (piece, _scope_notice) = parse_piece(&content, &format!("{id}.json"), None)
            .map_err(|e| {
                format!(
                    "refusing to save piece {id}: {id}.json exists but cannot be parsed ({e}) — fix or remove the file first, so the save cannot destroy its contents"
                )
            })?;
        if piece.id != id {
            return Err(format!(
                "refusing to save piece {id}: {id}.json holds a different piece ({}) — rename or remove that file first",
                piece.id
            ));
        }
        return Ok(Some(piece));
    }
    // No canonical file: the id may live in a hand-copied stale-named file.
    Ok(load_pieces(dir, None)?.into_iter().find(|p| p.id == id))
}

/// Create (no id) or update (id present) a piece. Versioning per the
/// contract: a body change pushes the old body (with its timestamp) onto
/// `versions`, newest-first; metadata-only saves don't version. An id that
/// matches no stored piece is treated as a create with that id (upsert) —
/// erroring would strand an edit made while the file was deleted on disk.
pub fn save_piece_at(dir: &Path, input: PieceInput, now: u64) -> Result<Piece, String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let existing = match &input.id {
        Some(id) => resolve_existing(dir, id)?,
        None => None,
    };
    let piece = match existing {
        Some(mut prev) => {
            if prev.body != input.body {
                prev.versions.insert(
                    0,
                    Version { body: std::mem::take(&mut prev.body), saved_at: prev.updated_at, extra: Map::new() },
                );
            }
            Piece {
                id: prev.id,
                title: input.title,
                body: input.body.clone(),
                keywords: input.keywords,
                tags: input.tags,
                category: input.category,
                scope: input.scope,
                placeholders: grammar::derive_placeholders(&input.body),
                created_at: prev.created_at,
                updated_at: now,
                versions: prev.versions,
                extra: prev.extra,
            }
        }
        None => Piece {
            id: input.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            title: input.title,
            body: input.body.clone(),
            keywords: input.keywords,
            tags: input.tags,
            category: input.category,
            scope: input.scope,
            placeholders: grammar::derive_placeholders(&input.body),
            created_at: now,
            updated_at: now,
            versions: Vec::new(),
            extra: Map::new(),
        },
    };
    write_piece(dir, &piece)?;
    remove_stale_twins(dir, &piece.id);
    Ok(piece)
}

/// Atomically write `<dir>/<id>.json` (temp file + rename, so a crash never
/// leaves a truncated piece). Pretty-printed + trailing newline: these files
/// are a hand-editing surface.
fn write_piece(dir: &Path, piece: &Piece) -> Result<(), String> {
    let mut pretty = serde_json::to_string_pretty(piece).map_err(|e| e.to_string())?;
    pretty.push('\n');
    let tmp = dir.join(format!(".tmp-{}.json", piece.id));
    fs::write(&tmp, pretty).map_err(|e| e.to_string())?;
    fs::rename(&tmp, dir.join(format!("{}.json", piece.id))).map_err(|e| e.to_string())
}

/// After a save lands at `<id>.json`, drop any OTHER file carrying the same
/// id (a hand-copied file with a stale name) — otherwise every such save
/// spawns a duplicate that shadows future loads. Best-effort: a failure here
/// leaves a redundant file, not data loss.
fn remove_stale_twins(dir: &Path, id: &str) {
    let canonical = format!("{id}.json");
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(fname) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if fname == canonical || !fname.ends_with(".json") || fname.starts_with('.') {
            continue;
        }
        let same_id = fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<Piece>(&s).ok())
            .is_some_and(|p| p.id == id);
        if same_id {
            let _ = fs::remove_file(&path);
        }
    }
}

/// Delete every file storing `id` — the canonical `<id>.json` (even if its
/// content no longer parses) plus any stale-named twin. Idempotent: deleting
/// an absent id is Ok, matching the command contract's `null` return.
pub fn delete_piece_at(dir: &Path, id: &str) -> Result<(), String> {
    if !dir.is_dir() {
        return Ok(());
    }
    let canonical = dir.join(format!("{id}.json"));
    if canonical.is_file() {
        fs::remove_file(&canonical).map_err(|e| e.to_string())?;
    }
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        let Some(fname) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !fname.ends_with(".json") || fname.starts_with('.') {
            continue;
        }
        let same_id = fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<Piece>(&s).ok())
            .is_some_and(|p| p.id == id);
        if same_id {
            fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// `save_piece` resolved against the real data root and clock.
pub fn save_piece(input: PieceInput) -> Result<Piece, String> {
    save_piece_at(&prompts_dir()?, input, unix_now())
}

/// Rescope every piece of `project_id` to global — the delete-project
/// semantics (contract: nothing a user wrote ever vanishes as a side effect;
/// the pieces surface again under Global). Metadata-only by design: no
/// version push, `updated_at` untouched — the user changed nothing about the
/// piece itself. Only cleanly-parsed files are rewritten; a broken file is
/// never repaired-and-rewritten as a side effect of deleting a project — it
/// degrades safely later via the unknown-project fallback once the roster
/// entry is gone.
pub fn rescope_project_pieces(dir: &Path, project_id: &str) -> Result<(), String> {
    if !dir.is_dir() {
        return Ok(());
    }
    let target = Scope::Project { project_id: project_id.to_string() };
    // Collect first, write after: rewriting (and twin-cleaning) while
    // read_dir is still iterating makes the listing platform-dependent.
    let mut to_rescope: Vec<Piece> = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        let Some(fname) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !fname.ends_with(".json") || fname.starts_with('.') {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue; // unreadable → handled by the load-error surface
        };
        let Ok(piece) = serde_json::from_str::<Piece>(&content) else {
            continue; // broken/legacy → the dangling-id fallback covers it
        };
        if piece.scope == target && !to_rescope.iter().any(|p| p.id == piece.id) {
            to_rescope.push(piece);
        }
    }
    for mut piece in to_rescope {
        piece.scope = Scope::Global;
        write_piece(dir, &piece)?;
        // A piece that lived only in a stale-named file now has a canonical
        // twin — clean up exactly as a save does.
        remove_stale_twins(dir, &piece.id);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ccdeck-prompts-test-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn input(title: &str, body: &str) -> PieceInput {
        PieceInput {
            id: None,
            title: title.to_string(),
            body: body.to_string(),
            keywords: vec![],
            tags: vec![],
            category: None,
            scope: Scope::Global,
        }
    }

    // --- round-trip against hostile fixtures ---

    #[test]
    fn hostile_unknown_fields_survive_load_save_round_trip() {
        let dir = tmp_dir("hostile");
        // Hand-edited piece: unknown top-level fields including an integer
        // past 2^53 (exact in u64, lossy in a JS float), i64::MIN, a
        // deeply-nested object, and a non-ASCII key.
        let raw = r#"{
            "id": "abc-1",
            "title": "t",
            "body": "b",
            "created_at": 1,
            "updated_at": 1,
            "my_note": "user field",
            "big": 18446744073709551615,
            "neg": -9223372036854775808,
            "nested": {"deep": [1, 2, {"x": 9007199254740993}]},
            "ключ": "значение"
        }"#;
        fs::write(dir.join("abc-1.json"), raw).unwrap();

        // Metadata-only save (same body) — the round-trip that must not drop fields.
        let mut inp = input("t2", "b");
        inp.id = Some("abc-1".to_string());
        save_piece_at(&dir, inp, 2).unwrap();

        let reread: Value = serde_json::from_str(&fs::read_to_string(dir.join("abc-1.json")).unwrap()).unwrap();
        assert_eq!(reread["my_note"], "user field");
        assert_eq!(reread["big"], Value::from(18446744073709551615u64), "u64 past 2^53 must stay exact");
        assert_eq!(reread["neg"], Value::from(-9223372036854775808i64));
        assert_eq!(reread["nested"]["deep"][2]["x"], Value::from(9007199254740993u64));
        assert_eq!(reread["ключ"], "значение");
        assert_eq!(reread["title"], "t2", "the edit itself must land");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unparseable_file_is_skipped_surfaced_and_left_untouched_on_disk() {
        let dir = tmp_dir("surrogate");
        // Unpaired surrogate escape — invalid JSON string content; serde_json
        // refuses it. The loader must skip the file, not corrupt or drop it —
        // and must REPORT it (Gate-2 correction: a desktop user never sees
        // stderr, so an unsurfaced skip reads as silent data loss).
        let bad = r#"{"id":"bad","title":"\ud800","body":"x","created_at":1,"updated_at":1}"#;
        fs::write(dir.join("bad.json"), bad).unwrap();
        fs::write(
            dir.join("good.json"),
            r#"{"id":"good","title":"ok","body":"x","created_at":1,"updated_at":1}"#,
        )
        .unwrap();

        let (pieces, errors) = scan_pieces(&dir, None).unwrap();
        assert_eq!(pieces.len(), 1, "good piece must still load");
        assert_eq!(pieces[0].id, "good");
        assert_eq!(errors.len(), 1, "the broken file must be reported, not silently skipped");
        assert_eq!(errors[0].file, "bad.json");
        assert!(!errors[0].error.is_empty());
        assert_eq!(
            fs::read_to_string(dir.join("bad.json")).unwrap(),
            bad,
            "the bad file must stay byte-identical for the user to fix"
        );
        // The pieces-only wrapper sees the same world minus the errors.
        assert_eq!(load_pieces(&dir, None).unwrap().len(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn shadowed_duplicate_is_reported_naming_the_loser_in_either_scan_order() {
        let dir = tmp_dir("dupe-errors");
        // Same id under a stale name and the canonical name. Whichever order
        // read_dir yields them, the canonical file must win and the error
        // must name the stale file as the ignored one.
        fs::write(
            dir.join("copy.json"),
            r#"{"id":"x","title":"stale copy","body":"b","created_at":1,"updated_at":1}"#,
        )
        .unwrap();
        fs::write(
            dir.join("x.json"),
            r#"{"id":"x","title":"canonical","body":"b","created_at":1,"updated_at":1}"#,
        )
        .unwrap();

        let (pieces, errors) = scan_pieces(&dir, None).unwrap();
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].title, "canonical");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].file, "copy.json", "the SHADOWED file is the one reported");
        assert!(errors[0].error.contains("duplicate piece id x"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn duplicate_ids_canonical_filename_wins() {
        let dir = tmp_dir("dupes");
        fs::write(
            dir.join("copy.json"),
            r#"{"id":"x","title":"stale copy","body":"b","created_at":1,"updated_at":1}"#,
        )
        .unwrap();
        fs::write(
            dir.join("x.json"),
            r#"{"id":"x","title":"canonical","body":"b","created_at":1,"updated_at":1}"#,
        )
        .unwrap();

        let pieces = load_pieces(&dir, None).unwrap();
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].title, "canonical");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn stale_named_file_loads_by_content_id_and_save_renames_it() {
        let dir = tmp_dir("stale-name");
        fs::write(
            dir.join("hand-copied.json"),
            r#"{"id":"real-id","title":"t","body":"b","created_at":1,"updated_at":1}"#,
        )
        .unwrap();

        let pieces = load_pieces(&dir, None).unwrap();
        assert_eq!(pieces[0].id, "real-id", "content id wins over filename");

        let mut inp = input("t", "b");
        inp.id = Some("real-id".to_string());
        save_piece_at(&dir, inp, 2).unwrap();
        assert!(dir.join("real-id.json").is_file(), "save lands at <id>.json");
        assert!(!dir.join("hand-copied.json").exists(), "stale twin cleaned up");
        assert_eq!(load_pieces(&dir, None).unwrap().len(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    // --- versioning invariants ---

    #[test]
    fn body_change_pushes_old_body_newest_first() {
        let dir = tmp_dir("versioning");
        let created = save_piece_at(&dir, input("t", "body v1"), 100).unwrap();
        assert!(created.versions.is_empty());

        let mut second = input("t", "body v2");
        second.id = Some(created.id.clone());
        let v2 = save_piece_at(&dir, second, 200).unwrap();
        assert_eq!(v2.versions.len(), 1);
        assert_eq!(v2.versions[0].body, "body v1");
        assert_eq!(v2.versions[0].saved_at, 100, "prior body carries its own save time");

        let mut third = input("t", "body v3");
        third.id = Some(created.id.clone());
        let v3 = save_piece_at(&dir, third, 300).unwrap();
        assert_eq!(v3.versions.len(), 2);
        assert_eq!(v3.versions[0].body, "body v2", "newest-first");
        assert_eq!(v3.versions[1].body, "body v1");
        assert_eq!(v3.created_at, 100, "created_at never moves");
        assert_eq!(v3.updated_at, 300);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn metadata_only_save_does_not_version() {
        let dir = tmp_dir("meta-only");
        let created = save_piece_at(&dir, input("t", "same body"), 100).unwrap();
        let mut rename = input("renamed", "same body");
        rename.id = Some(created.id.clone());
        rename.keywords = vec!["k".to_string()];
        let saved = save_piece_at(&dir, rename, 200).unwrap();
        assert!(saved.versions.is_empty(), "unchanged body must not version");
        assert_eq!(saved.title, "renamed");
        assert_eq!(saved.updated_at, 200);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn create_assigns_uuid_and_writes_canonical_file() {
        let dir = tmp_dir("create");
        let p = save_piece_at(&dir, input("t", "b {ticket:ABC-123} {ticket} {env}"), 100).unwrap();
        assert!(uuid::Uuid::parse_str(&p.id).is_ok());
        assert_eq!(p.created_at, p.updated_at);
        assert!(dir.join(format!("{}.json", p.id)).is_file());
        assert_eq!(
            p.placeholders,
            vec![
                Placeholder { name: "ticket".into(), default: Some("ABC-123".into()) },
                Placeholder { name: "env".into(), default: None },
            ],
            "derived via the v2 grammar, deduped, first occurrence's default kept"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn placeholder_default_round_trips_through_the_file() {
        // The schema example's exact shape: {"name": "ticket", "default": "ABC-123"}
        // — and a default-less entry omits the key entirely (Option skip).
        let dir = tmp_dir("ph-default");
        let p = save_piece_at(&dir, input("t", "{ticket:ABC-123} {env}"), 100).unwrap();
        let raw: Value =
            serde_json::from_str(&fs::read_to_string(dir.join(format!("{}.json", p.id))).unwrap())
                .unwrap();
        assert_eq!(raw["placeholders"][0]["name"], "ticket");
        assert_eq!(raw["placeholders"][0]["default"], "ABC-123");
        assert_eq!(raw["placeholders"][1]["name"], "env");
        assert!(
            !raw["placeholders"][1].as_object().unwrap().contains_key("default"),
            "no default → no key, per the contract's optional-default schema"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn save_refuses_to_overwrite_unparseable_canonical_file() {
        // Audit L2: the loader skips a broken <id>.json, so resolving through
        // it would silently turn this save into a CREATE that overwrites the
        // broken file — destroying whatever versions/extra it held. The save
        // must refuse instead, and the file must stay byte-identical.
        let dir = tmp_dir("refuse-broken");
        let broken = r#"{"id":"x","title":"t","body":"b","versions":[{"body":"precious"#; // truncated JSON
        fs::write(dir.join("x.json"), broken).unwrap();

        let mut inp = input("t2", "new body");
        inp.id = Some("x".to_string());
        let err = save_piece_at(&dir, inp, 2).unwrap_err();
        assert!(err.contains("x.json"), "error must name the file: {err}");
        assert!(err.contains("cannot be parsed"), "error must say why: {err}");
        assert_eq!(
            fs::read_to_string(dir.join("x.json")).unwrap(),
            broken,
            "the refused save must leave the broken file byte-identical"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn save_refuses_when_canonical_file_holds_another_pieces_id() {
        // Same overwrite hazard, different cause: a hand-edit changed the id
        // INSIDE x.json, so that file now belongs to piece "y". Writing piece
        // "x" to x.json would destroy y's data.
        let dir = tmp_dir("refuse-mismatch");
        fs::write(
            dir.join("x.json"),
            r#"{"id":"y","title":"t","body":"b","created_at":1,"updated_at":1}"#,
        )
        .unwrap();

        let mut inp = input("t", "b");
        inp.id = Some("x".to_string());
        let err = save_piece_at(&dir, inp, 2).unwrap_err();
        assert!(err.contains("different piece"), "{err}");
        assert!(dir.join("x.json").is_file(), "the mismatched file is untouched");
        fs::remove_dir_all(&dir).unwrap();
    }

    // Placeholder-grammar edge cases live in `grammar::tests` (the shared
    // contract vectors, asserted verbatim by both lanes).

    // --- scope v2: legacy/unknown shapes load as global, visibly ---

    #[test]
    fn legacy_scope_loads_as_global_with_notice_and_untouched_file() {
        let dir = tmp_dir("legacy-scope");
        // The pre-revision path-keyed shape (founder feel-check data).
        let raw = r#"{"id":"a","title":"t","body":"b","created_at":1,"updated_at":1,"scope":{"kind":"project","project":"/home/u/proj"}}"#;
        fs::write(dir.join("a.json"), raw).unwrap();

        let (pieces, errors) = scan_pieces(&dir, None).unwrap();
        assert_eq!(pieces.len(), 1, "the piece must LOAD, not be skipped");
        assert_eq!(pieces[0].scope, Scope::Global);
        assert_eq!(errors.len(), 1, "the degradation must be visible");
        assert_eq!(errors[0].file, "a.json");
        assert!(errors[0].error.contains("unrecognized scope"), "{}", errors[0].error);
        assert_eq!(
            fs::read_to_string(dir.join("a.json")).unwrap(),
            raw,
            "the loader must NEVER rewrite the user's file"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unknown_project_id_loads_as_global_when_roster_is_readable() {
        let dir = tmp_dir("dangling-scope");
        let raw = r#"{"id":"a","title":"t","body":"b","created_at":1,"updated_at":1,"scope":{"kind":"project","project_id":"ghost"}}"#;
        fs::write(dir.join("a.json"), raw).unwrap();

        let known: HashSet<String> = ["real".to_string()].into();
        let (pieces, errors) = scan_pieces(&dir, Some(&known)).unwrap();
        assert_eq!(pieces[0].scope, Scope::Global);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].error.contains("unknown project ghost"), "{}", errors[0].error);
        assert_eq!(fs::read_to_string(dir.join("a.json")).unwrap(), raw);

        // Roster unreadable (None): validation suspends, the scope holds.
        let (pieces, errors) = scan_pieces(&dir, None).unwrap();
        assert_eq!(pieces[0].scope, Scope::Project { project_id: "ghost".into() });
        assert!(errors.is_empty());

        // Known id: no degradation, no notice.
        let known: HashSet<String> = ["ghost".to_string()].into();
        let (pieces, errors) = scan_pieces(&dir, Some(&known)).unwrap();
        assert_eq!(pieces[0].scope, Scope::Project { project_id: "ghost".into() });
        assert!(errors.is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn save_over_legacy_scope_file_proceeds_and_persists_clean_scope() {
        // The explicit save is the one moment normalization may persist:
        // loading never rewrites, but a user edit of a legacy-scope piece
        // must not be refused (versions/extras still merge).
        let dir = tmp_dir("legacy-save");
        let raw = r#"{"id":"a","title":"t","body":"old","created_at":1,"updated_at":1,"scope":{"kind":"project","project":"/p"},"my_note":"kept"}"#;
        fs::write(dir.join("a.json"), raw).unwrap();

        let mut inp = input("t", "new");
        inp.id = Some("a".to_string());
        let saved = save_piece_at(&dir, inp, 2).unwrap();
        assert_eq!(saved.scope, Scope::Global, "input scope wins on save");
        assert_eq!(saved.versions.len(), 1, "body change still versions");
        assert_eq!(saved.versions[0].body, "old");
        assert_eq!(saved.extra["my_note"], "kept");

        let reread: Value =
            serde_json::from_str(&fs::read_to_string(dir.join("a.json")).unwrap()).unwrap();
        assert_eq!(reread["scope"]["kind"], "global");
        fs::remove_dir_all(&dir).unwrap();
    }

    // --- delete-project rescope ---

    #[test]
    fn rescope_moves_target_pieces_to_global_without_versioning() {
        let dir = tmp_dir("rescope");
        let mut mine = input("mine", "b");
        mine.scope = Scope::Project { project_id: "target".into() };
        let mine = save_piece_at(&dir, mine, 100).unwrap();
        let mut other = input("other", "b");
        other.scope = Scope::Project { project_id: "different".into() };
        let other = save_piece_at(&dir, other, 100).unwrap();

        rescope_project_pieces(&dir, "target").unwrap();

        let pieces = load_pieces(&dir, None).unwrap();
        let by_id = |id: &str| pieces.iter().find(|p| p.id == id).unwrap();
        assert_eq!(by_id(&mine.id).scope, Scope::Global, "target pieces rescoped");
        assert!(by_id(&mine.id).versions.is_empty(), "rescope is metadata-only: no version");
        assert_eq!(by_id(&mine.id).updated_at, 100, "rescope is metadata-only: updated_at holds");
        assert_eq!(
            by_id(&other.id).scope,
            Scope::Project { project_id: "different".into() },
            "other projects' pieces untouched"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rescope_leaves_broken_files_alone() {
        let dir = tmp_dir("rescope-broken");
        let broken = r#"{"id":"x","scope":{"kind":"project","project_id":"target"},"title":"t""#;
        fs::write(dir.join("x.json"), broken).unwrap();

        rescope_project_pieces(&dir, "target").unwrap();

        assert_eq!(
            fs::read_to_string(dir.join("x.json")).unwrap(),
            broken,
            "a broken file is never repaired-and-rewritten as a delete side effect"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    // --- delete ---

    #[test]
    fn delete_removes_canonical_and_twins_and_is_idempotent() {
        let dir = tmp_dir("delete");
        fs::write(
            dir.join("x.json"),
            r#"{"id":"x","title":"t","body":"b","created_at":1,"updated_at":1}"#,
        )
        .unwrap();
        fs::write(
            dir.join("twin.json"),
            r#"{"id":"x","title":"t","body":"b","created_at":1,"updated_at":1}"#,
        )
        .unwrap();

        delete_piece_at(&dir, "x").unwrap();
        assert!(load_pieces(&dir, None).unwrap().is_empty(), "no file may resurrect the piece");
        delete_piece_at(&dir, "x").unwrap(); // idempotent
        fs::remove_dir_all(&dir).unwrap();
    }
}
