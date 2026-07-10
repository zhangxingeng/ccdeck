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
    /// Transient (§ Store robustness): true when the loader had to repair
    /// this file's JSON in memory. Never persisted — a clean parse forces it
    /// false (so a hand-written `"recovered": true` can't fake the signal;
    /// the key is schema-reserved, not a preserved extra), saves construct
    /// pieces with false, and the skip keeps a false flag off disk and wire.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub recovered: bool,
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

/// Parse one piece file's content: strict JSON first; on failure an
/// in-memory jsonrepair attempt (§ Store robustness) — a recovered piece
/// loads flagged `recovered: true` and the file stays untouched. Then scope
/// normalization: a legacy/unknown scope (or, when the roster is readable, a
/// `project_id` no roster entry matches) loads as Global and comes back as a
/// load error alongside the piece: visible, non-fatal, file untouched.
/// `known_project_ids: None` means the roster could not be consulted — id
/// validation is suspended rather than falsely degrading every project piece.
fn parse_piece(
    content: &str,
    fname: &str,
    known_project_ids: Option<&HashSet<String>>,
) -> Result<(Piece, Option<LoadError>), String> {
    let (mut value, repaired) = match serde_json::from_str::<Value>(content) {
        Ok(v) => (v, false),
        Err(strict_err) => match super::repair::repair_to_value(content) {
            Some(v) => (v, true),
            // Report the STRICT error — it points at the user's actual
            // file, not at what the repairer made of it.
            None => return Err(strict_err.to_string()),
        },
    };
    let scope_error = normalize_scope(&mut value, fname, known_project_ids);
    let mut piece: Piece = serde_json::from_value(value).map_err(|e| e.to_string())?;
    piece.recovered = repaired;
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
/// the file actually named `<id>.json` wins — else the lexicographically
/// first filename (deterministic; write paths key off this view) — and the
/// shadowed ones are reported. Pieces sorted newest-updated first as a
/// sensible default.
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
            // Winner: the canonically-named file; when NEITHER is canonical,
            // the lexicographically-first filename (contract: a deterministic
            // winner, never directory-iteration order — write paths key off
            // this view, so a flaky winner is flaky data destruction).
            let new_wins = !existing.1 && (canonical || *fname < *existing.2);
            let loser = if new_wins {
                std::mem::replace(existing, (piece, canonical, fname.to_string())).2
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
        // parse_piece (not bare serde): a legacy-scope or repairable file
        // must stay saveable — the explicit save is exactly the moment its
        // normalized/repaired form is allowed to persist (versions append,
        // extras merge, like any body change). Scope validation is skipped
        // (None): the save overwrites `scope` from the input anyway.
        let (piece, _scope_notice) = parse_piece(&content, &format!("{id}.json"), None)
            .map_err(|e| {
                format!(
                    "refusing to save piece {id}: {id}.json exists but cannot be parsed even after repair ({e}) — fix or remove the file first, so the save cannot destroy its contents"
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
                recovered: false,
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
            recovered: false,
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

/// Does this file's content belong to piece `id` **in the loader's view**?
/// Repair-aware on purpose (contract: every write path sees what the loader
/// sees) — a strict-only check here lets a repairable twin hide from cleanup
/// or survive a delete and resurrect the piece.
fn file_holds_piece(path: &Path, fname: &str, id: &str) -> bool {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| parse_piece(&s, fname, None).ok())
        .is_some_and(|(p, _)| p.id == id)
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
        if file_holds_piece(&path, fname, id) {
            let _ = fs::remove_file(&path);
        }
    }
}

/// Delete every file storing `id` — the canonical `<id>.json` (even if its
/// content no longer parses) plus any twin **the loader would recognize**,
/// repairable ones included (contract: a destructive action must stick; a
/// strict-only twin check left a repairable twin behind to resurrect the
/// piece on the next load). Idempotent: deleting an absent id is Ok,
/// matching the command contract's `null` return.
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
        if file_holds_piece(&path, fname, id) {
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
/// piece itself.
///
/// Operates on [`scan_pieces`]' view (contract: every write path sees what
/// the loader sees — repair-aware, canonical-filename-wins, deterministic).
/// A parallel stricter parse here once picked a stale twin as "the piece"
/// and overwrote the canonical body with it (audit MED). A winner the loader
/// flags `recovered` is skipped entirely: a delete-project side effect must
/// never bake an unsaved repair — the file stays byte-identical and surfaces
/// under Global via the dangling-id fallback once the roster entry is gone.
pub fn rescope_project_pieces(dir: &Path, project_id: &str) -> Result<(), String> {
    if !dir.is_dir() {
        return Ok(());
    }
    let target = Scope::Project { project_id: project_id.to_string() };
    let (pieces, _notices) = scan_pieces(dir, None)?;
    for mut piece in pieces {
        if piece.scope != target || piece.recovered {
            continue;
        }
        piece.scope = Scope::Global;
        write_piece(dir, &piece)?;
        // The winner now lives canonically; superseded twins are cleaned
        // exactly as a save does.
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
    fn save_refuses_when_repair_cannot_yield_this_piece() {
        // Audit L2, repair-era form: when even repair can't turn <id>.json
        // into a piece, resolving through it would silently turn this save
        // into a CREATE that overwrites the broken file — destroying
        // whatever it held. The save must refuse, file byte-identical.
        let dir = tmp_dir("refuse-broken");
        let broken = "[1, 2,"; // repairs to a JSON array — never a Piece
        fs::write(dir.join("x.json"), broken).unwrap();

        let mut inp = input("t2", "new body");
        inp.id = Some("x".to_string());
        let err = save_piece_at(&dir, inp, 2).unwrap_err();
        assert!(err.contains("x.json"), "error must name the file: {err}");
        assert!(err.contains("even after repair"), "error must say why: {err}");
        assert_eq!(
            fs::read_to_string(dir.join("x.json")).unwrap(),
            broken,
            "the refused save must leave the broken file byte-identical"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    // --- store robustness: in-memory jsonrepair recovery ---

    #[test]
    fn hand_edit_corruption_recovers_in_memory_flagged_and_untouched() {
        let dir = tmp_dir("recover");
        // The contract's bounded corruption classes in one file: comments,
        // unquoted key, single quotes, trailing comma — plus a u64 past 2^53
        // to prove the repair path keeps numbers exact (silent-corruption
        // guard: repair must never "fix" data by rounding it).
        let corrupt = r#"{
            // hand-added comment
            "id": "r1",
            title: 'needs repair',
            "body": "b",
            "created_at": 1,
            "updated_at": 1,
            "big": 18446744073709551615,
        }"#;
        fs::write(dir.join("r1.json"), corrupt).unwrap();

        let (pieces, errors) = scan_pieces(&dir, None).unwrap();
        assert_eq!(pieces.len(), 1, "the piece must recover, not be skipped");
        assert!(pieces[0].recovered, "recovery must be flagged for the UI");
        assert_eq!(pieces[0].title, "needs repair");
        assert_eq!(
            pieces[0].extra["big"],
            Value::from(18446744073709551615u64),
            "repair must keep u64 past 2^53 exact"
        );
        assert!(errors.is_empty(), "recovered is a flag, not a load error");
        assert_eq!(
            fs::read_to_string(dir.join("r1.json")).unwrap(),
            corrupt,
            "the loader must NEVER rewrite the user's file"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn truncated_file_recovers_and_only_explicit_save_persists_repair() {
        let dir = tmp_dir("truncated");
        // Truncation mid-versions — the crash/partial-paste shape.
        let truncated = r#"{"id":"x","title":"t","body":"current","created_at":1,"updated_at":5,"versions":[{"body":"precious","saved_at":1"#;
        fs::write(dir.join("x.json"), truncated).unwrap();

        // Loading recovers in memory; disk stays byte-identical.
        let (pieces, _) = scan_pieces(&dir, None).unwrap();
        assert!(pieces[0].recovered);
        assert_eq!(fs::read_to_string(dir.join("x.json")).unwrap(), truncated);

        // The explicit save is the one moment the repaired form persists —
        // treated as the EXISTING piece: versions append, nothing resets.
        let mut inp = input("t", "new body");
        inp.id = Some("x".to_string());
        let saved = save_piece_at(&dir, inp, 9).unwrap();
        assert_eq!(saved.created_at, 1, "repaired parse is the existing piece, not a create");
        assert_eq!(saved.versions.first().map(|v| v.body.as_str()), Some("current"));
        assert!(
            saved.versions.iter().any(|v| v.body == "precious"),
            "the recovered prior version must survive the save"
        );

        let raw = fs::read_to_string(dir.join("x.json")).unwrap();
        let reread: Value = serde_json::from_str(&raw).unwrap();
        assert!(reread.get("recovered").is_none(), "the transient flag never reaches disk");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn hand_written_recovered_key_cannot_fake_the_transient_signal() {
        let dir = tmp_dir("fake-flag");
        // `recovered` is schema-reserved: on a clean parse it is forced
        // false (and, being a known field, it is not preserved as an extra).
        fs::write(
            dir.join("a.json"),
            r#"{"id":"a","title":"t","body":"b","created_at":1,"updated_at":1,"recovered":true}"#,
        )
        .unwrap();
        let (pieces, errors) = scan_pieces(&dir, None).unwrap();
        assert!(!pieces[0].recovered);
        assert!(errors.is_empty());
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
    fn rescope_operates_on_the_loaders_view_never_a_stale_twin() {
        // Audit MED 1: the loader's winner for id x is the canonical x.json
        // (repairable → recovered); the stale twin parses strictly. A rescope
        // parsing more strictly than the loader sees ONLY the twin and
        // overwrites the canonical body with it — nondeterministic data
        // destruction inside delete-project, whose whole contract is
        // "nothing vanishes". Post-fix: the loader's winner rules; a
        // recovered winner is skipped entirely (a delete-project side effect
        // must never bake an unsaved repair), so NOTHING is written.
        let dir = tmp_dir("rescope-twin");
        let canonical = r#"{"id":"x","title":"t","body":"precious","created_at":1,"updated_at":9,"scope":{"kind":"project","project_id":"target"},}"#; // trailing comma: repair-only
        let twin = r#"{"id":"x","title":"t","body":"stale","created_at":1,"updated_at":1,"scope":{"kind":"project","project_id":"target"}}"#;
        fs::write(dir.join("x.json"), canonical).unwrap();
        fs::write(dir.join("copy.json"), twin).unwrap();

        rescope_project_pieces(&dir, "target").unwrap();

        assert_eq!(
            fs::read_to_string(dir.join("x.json")).unwrap(),
            canonical,
            "the canonical (loader-winner) file must never be overwritten from a twin"
        );
        assert_eq!(
            fs::read_to_string(dir.join("copy.json")).unwrap(),
            twin,
            "skipping the recovered winner means no writes at all for this id"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rescope_canonical_winner_survives_with_both_twins_parseable() {
        // Same rule with both files strictly parseable: the canonical body
        // must survive rescope whichever order read_dir yields the two.
        let dir = tmp_dir("rescope-twin2");
        fs::write(
            dir.join("x.json"),
            r#"{"id":"x","title":"t","body":"canonical-body","created_at":1,"updated_at":9,"scope":{"kind":"project","project_id":"target"}}"#,
        )
        .unwrap();
        fs::write(
            dir.join("copy.json"),
            r#"{"id":"x","title":"t","body":"stale","created_at":1,"updated_at":1,"scope":{"kind":"project","project_id":"target"}}"#,
        )
        .unwrap();

        rescope_project_pieces(&dir, "target").unwrap();

        let pieces = load_pieces(&dir, None).unwrap();
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].body, "canonical-body", "loader-winner content survives");
        assert_eq!(pieces[0].scope, Scope::Global);
        assert!(!dir.join("copy.json").exists(), "the superseded twin is cleaned up");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn delete_removes_repairable_twins_so_the_delete_sticks() {
        // Audit MED 2: the loader recognizes a repairable twin as this piece,
        // so a delete that only strict-parses leaves it behind — and the
        // "deleted" piece resurrects (recovered) on the next load. A
        // destructive action must stick.
        let dir = tmp_dir("delete-repairable-twin");
        fs::write(
            dir.join("x.json"),
            r#"{"id":"x","title":"t","body":"b","created_at":1,"updated_at":1}"#,
        )
        .unwrap();
        // Trailing comma: strict parse fails, the loader's parse yields id x.
        fs::write(
            dir.join("twin.json"),
            r#"{"id":"x","title":"t","body":"b","created_at":1,"updated_at":1,}"#,
        )
        .unwrap();

        delete_piece_at(&dir, "x").unwrap();

        assert!(!dir.join("twin.json").exists(), "the repairable twin must go too");
        assert!(
            load_pieces(&dir, None).unwrap().is_empty(),
            "no file may resurrect the deleted piece"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn no_canonical_twin_tie_resolves_lexicographically() {
        // Audit LOW: when neither same-id file is canonically named, the
        // winner is deterministic — lexicographic filename order — never
        // directory-iteration order.
        let dir = tmp_dir("lex-tie");
        fs::write(
            dir.join("bbb.json"),
            r#"{"id":"x","title":"from-bbb","body":"b","created_at":1,"updated_at":1}"#,
        )
        .unwrap();
        fs::write(
            dir.join("aaa.json"),
            r#"{"id":"x","title":"from-aaa","body":"b","created_at":1,"updated_at":1}"#,
        )
        .unwrap();

        let (pieces, errors) = scan_pieces(&dir, None).unwrap();
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].title, "from-aaa", "lexicographically first filename wins");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].file, "bbb.json", "the loser is the one reported");
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
