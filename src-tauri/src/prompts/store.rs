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

/// Where a piece applies: everywhere, or one project (the decoded cwd the
/// app's project picker shows — readable in hand-edited JSON).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Scope {
    #[default]
    Global,
    Project { project: String },
}

/// A `{{token}}` occurrence in the body, derived at save time (the body is
/// the single source of truth; this array exists so consumers don't re-parse).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placeholder {
    pub name: String,
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

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Derive `placeholders` from `{{token}}` occurrences: trimmed, non-empty,
/// no braces/newlines inside, deduped in first-seen order.
pub fn derive_placeholders(body: &str) -> Vec<Placeholder> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut rest = body;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            break;
        };
        let name = after[..end].trim();
        if !name.is_empty()
            && !name.contains('{')
            && !name.contains('}')
            && !name.contains('\n')
            && seen.insert(name.to_string())
        {
            out.push(Placeholder { name: name.to_string() });
        }
        rest = &after[end + 2..];
    }
    out
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
pub fn scan_pieces(dir: &Path) -> Result<(Vec<Piece>, Vec<LoadError>), String> {
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
            .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
        {
            Ok(p) => p,
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
pub fn load_pieces(dir: &Path) -> Result<Vec<Piece>, String> {
    let (pieces, errors) = scan_pieces(dir)?;
    for e in &errors {
        eprintln!("[prompts] skipping piece file {}: {}", e.file, e.error);
    }
    Ok(pieces)
}

/// Create (no id) or update (id present) a piece. Versioning per the
/// contract: a body change pushes the old body (with its timestamp) onto
/// `versions`, newest-first; metadata-only saves don't version. An id that
/// matches no stored piece is treated as a create with that id (upsert) —
/// erroring would strand an edit made while the file was deleted on disk.
pub fn save_piece_at(dir: &Path, input: PieceInput, now: u64) -> Result<Piece, String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let existing = match &input.id {
        Some(id) => load_pieces(dir)?.into_iter().find(|p| &p.id == id),
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
                placeholders: derive_placeholders(&input.body),
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
            placeholders: derive_placeholders(&input.body),
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

        let (pieces, errors) = scan_pieces(&dir).unwrap();
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
        assert_eq!(load_pieces(&dir).unwrap().len(), 1);
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

        let (pieces, errors) = scan_pieces(&dir).unwrap();
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

        let pieces = load_pieces(&dir).unwrap();
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

        let pieces = load_pieces(&dir).unwrap();
        assert_eq!(pieces[0].id, "real-id", "content id wins over filename");

        let mut inp = input("t", "b");
        inp.id = Some("real-id".to_string());
        save_piece_at(&dir, inp, 2).unwrap();
        assert!(dir.join("real-id.json").is_file(), "save lands at <id>.json");
        assert!(!dir.join("hand-copied.json").exists(), "stale twin cleaned up");
        assert_eq!(load_pieces(&dir).unwrap().len(), 1);
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
        let p = save_piece_at(&dir, input("t", "b {{ticket}} {{ticket}} {{ env }}"), 100).unwrap();
        assert!(uuid::Uuid::parse_str(&p.id).is_ok());
        assert_eq!(p.created_at, p.updated_at);
        assert!(dir.join(format!("{}.json", p.id)).is_file());
        assert_eq!(
            p.placeholders,
            vec![Placeholder { name: "ticket".into() }, Placeholder { name: "env".into() }],
            "derived, deduped, trimmed"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    // --- placeholder derivation ---

    #[test]
    fn placeholder_edge_cases() {
        assert!(derive_placeholders("no tokens").is_empty());
        assert!(derive_placeholders("empty {{}} token").is_empty());
        assert!(derive_placeholders("unclosed {{token").is_empty());
        assert_eq!(derive_placeholders("{{a}}{{b}}").len(), 2);
        assert!(
            derive_placeholders("{{multi\nline}}").is_empty(),
            "newline inside braces is prose, not a placeholder"
        );
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
        assert!(load_pieces(&dir).unwrap().is_empty(), "no file may resurrect the piece");
        delete_piece_at(&dir, "x").unwrap(); // idempotent
        fs::remove_dir_all(&dir).unwrap();
    }
}
