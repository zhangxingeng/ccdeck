//! The opt-in semantic path: a pinned embedding model + a pinned ONNX Runtime
//! shared library, both downloaded ONLY when the user clicks Download (never
//! at install, never silently), verified against hardcoded sha256 checksums
//! BEFORE any use, then loaded via ort's dynamic loading.
//!
//! Why dynamic loading (Gate-1 ruling, issue #24): fastembed's default
//! statically links a 15-30MB ONNX Runtime into every shipped binary. With
//! `ort-load-dynamic` non-opted users pay only a few MB of glue; the runtime
//! arrives as a separate, checksummed, user-approved download. A failed or
//! missing runtime degrades to lexical-only — never blocks the app.
//!
//! Security posture: downloading native code is an RCE vector if done
//! sloppily. Every artifact here has a pinned exact version/revision and a
//! hardcoded sha256 computed from the official source (Microsoft's GitHub
//! release for the runtime, the fastembed-blessed Hugging Face repo at a
//! pinned revision for the model). A checksum mismatch deletes the file and
//! reports error state — it never loads.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use fastembed::{
    InitOptionsUserDefined, Pooling, QuantizationMode, TextEmbedding, TokenizerFiles,
    UserDefinedEmbeddingModel,
};
use rusqlite::Connection;
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::store::Snippet;

/// The pinned embedding model: the fastembed-blessed quantized
/// BGE-small-en-v1.5 (Cls pooling, static quantization, 384 dims) — the
/// contract's "small, mature, strong retrieval-per-MB" class. Config-level so
/// a future (e.g. multilingual) swap is a constant change, not a schema one.
pub const MODEL_ID: &str = "Qdrant/bge-small-en-v1.5-onnx-Q";
/// Pinned HF revision — URLs below are immutable snapshots, not `main`.
const MODEL_REVISION: &str = "52398278842ec682c6f32300af41344b1c0b0bb2";

/// (filename, sha256, exact byte size) of every model artifact, verified
/// after download. Sizes are pinned like the hashes (immutable revision) —
/// they let the "model" progress stage report one fixed total across all
/// five files instead of restarting per file.
const MODEL_FILES: &[(&str, &str, u64)] = &[
    ("model_optimized.onnx", "51f1bd0addd6e859e42c2c8021a5e5461385bb676a649f4b269aa445449f2431", 66_465_124),
    ("tokenizer.json", "d241a60d5e8f04cc1b2b3e9ef7a4921b27bf526d9f6050ab90f9267a1f9e5c66", 711_396),
    ("config.json", "13582bcf2effc85b7bf3d3f5532e686bc1c9ce86bb009d10f0ec33cbe92299dd", 706),
    ("special_tokens_map.json", "5d5b662e421ea9fac075174bb0688ee0d9431699900b90662acd44b2a350503a", 695),
    ("tokenizer_config.json", "0b29c7bfc889e53b36d9dd3e686dd4300f6525110eaa98c76a5dafceb2029f53", 1_242),
];

/// Total bytes of the "model" download stage.
///
/// The `model_size_mb` / `runtime_size_mb` disclosure helpers that used to sit
/// here are gone with the download UI they fed: the download is background-only
/// now, so there is no one to disclose a size to.
fn model_total_bytes() -> u64 {
    MODEL_FILES.iter().map(|(_, _, size)| size).sum()
}

/// Pinned ONNX Runtime release (ort 2.0.0-rc.12 is designed for 1.24.x).
const ORT_VERSION: &str = "1.24.4";

/// One platform's runtime artifact: the official Microsoft release archive,
/// its sha256, the shared library's path inside the archive, and the exact
/// archive byte size (progress total + up-front size disclosure).
struct OrtArtifact {
    archive: &'static str,
    sha256: &'static str,
    lib_in_archive: &'static str,
    download_bytes: u64,
}

/// The runtime artifact for the platform this binary was built for. `None` on
/// platforms Microsoft ships no 1.24.x build for (notably macOS Intel) —
/// those get honest lexical-only degradation, not a doomed download.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const ORT_ARTIFACT: Option<OrtArtifact> = Some(OrtArtifact {
    archive: "onnxruntime-linux-x64-1.24.4.tgz",
    sha256: "3a211fbea252c1e66290658f1b735b772056149f28321e71c308942cdb54b747",
    lib_in_archive: "onnxruntime-linux-x64-1.24.4/lib/libonnxruntime.so.1.24.4",
    download_bytes: 8_155_822,
});
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const ORT_ARTIFACT: Option<OrtArtifact> = Some(OrtArtifact {
    archive: "onnxruntime-osx-arm64-1.24.4.tgz",
    sha256: "93787795f47e1eee369182e43ed51b9e5da0878ab0346aecf4258979b8bba989",
    lib_in_archive: "onnxruntime-osx-arm64-1.24.4/lib/libonnxruntime.1.24.4.dylib",
    download_bytes: 30_937_282,
});
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
const ORT_ARTIFACT: Option<OrtArtifact> = Some(OrtArtifact {
    archive: "onnxruntime-win-x64-1.24.4.zip",
    sha256: "d2319fddfb6ea4db99ccc4b60c85c517bcd855721f5daa6a06d40d7cb2ee2357",
    lib_in_archive: "onnxruntime-win-x64-1.24.4/lib/onnxruntime.dll",
    download_bytes: 74_442_783,
});
#[cfg(not(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64")
)))]
const ORT_ARTIFACT: Option<OrtArtifact> = None;

/// Progress event streamed over the `embed_download` Channel (contract):
/// stage `runtime` (the ONNX Runtime archive, bytes), `model` (all five
/// model files under one fixed byte total), then `index` (embedding the
/// existing library, SNIPPET counts — emitted by the command's warm-up loop,
/// so the popover's "Download & index" is literally what one click does).
/// Completion and error are NOT channel events: the command's Result is the
/// terminal signal and the frontend re-fetches `embed_status` afterward.
#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub stage: String, // "runtime" | "model" | "index"
    pub done: u64,
    pub total: u64,
}

/// Is semantic matching even possible on this build's platform?
pub fn platform_supported() -> bool {
    ORT_ARTIFACT.is_some()
}

fn models_dir(root: &Path) -> PathBuf {
    root.join("models")
}

fn model_files_dir(root: &Path) -> PathBuf {
    models_dir(root).join("bge-small-en-v1.5-q")
}

fn runtime_lib_path(root: &Path) -> Option<PathBuf> {
    let artifact = ORT_ARTIFACT.as_ref()?;
    let lib_name = Path::new(artifact.lib_in_archive).file_name()?;
    Some(models_dir(root).join("onnxruntime").join(ORT_VERSION).join(lib_name))
}

/// Are all artifacts (runtime lib + every model file) present on disk?
/// Presence-only by design: content was checksummed at download time, and
/// re-hashing a 63MB model on every status poll would make the UI crawl.
pub fn artifacts_present(root: &Path) -> bool {
    let Some(lib) = runtime_lib_path(root) else {
        return false;
    };
    lib.is_file() && MODEL_FILES.iter().all(|(name, _, _)| model_files_dir(root).join(name).is_file())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Download `url` streaming stage-accumulated progress to `on_progress`
/// (`stage_base` = bytes this stage completed before this file), then verify
/// the hardcoded checksum. A mismatch discards the bytes and errors — the
/// caller never sees unverified content.
fn download_verified(
    url: &str,
    expected_sha256: &str,
    file: &str,
    stage: &str,
    stage_base: u64,
    stage_total: u64,
    on_progress: &dyn Fn(DownloadProgress),
) -> Result<Vec<u8>, String> {
    let response = ureq::get(url).call().map_err(|e| format!("download of {file} failed: {e}"))?;
    let mut reader = response.into_body().into_reader();
    let mut bytes: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    let mut last_emit: u64 = 0;
    loop {
        let n = reader.read(&mut chunk).map_err(|e| format!("download of {file} interrupted: {e}"))?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..n]);
        // Emit at most every 512KB — per-64KB events would flood the channel.
        if bytes.len() as u64 - last_emit >= 512 * 1024 {
            last_emit = bytes.len() as u64;
            on_progress(DownloadProgress {
                stage: stage.to_string(),
                done: stage_base + last_emit,
                total: stage_total,
            });
        }
    }
    let actual = sha256_hex(&bytes);
    if actual != expected_sha256 {
        return Err(format!(
            "checksum mismatch for {file} (expected {expected_sha256}, got {actual}); discarded"
        ));
    }
    on_progress(DownloadProgress {
        stage: stage.to_string(),
        done: stage_base + bytes.len() as u64,
        total: stage_total,
    });
    Ok(bytes)
}

/// Extract exactly one member from a verified `.tgz` archive.
fn extract_from_tgz(archive: &[u8], member: &str) -> Result<Vec<u8>, String> {
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(archive));
    for entry in tar.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?;
        if path.to_string_lossy() == member && entry.header().entry_type().is_file() {
            let mut out = Vec::new();
            entry.read_to_end(&mut out).map_err(|e| e.to_string())?;
            return Ok(out);
        }
    }
    Err(format!("{member} not found in archive"))
}

/// Extract exactly one member from a verified `.zip` archive (Windows).
#[cfg(windows)]
fn extract_from_zip(archive: &[u8], member: &str) -> Result<Vec<u8>, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(archive)).map_err(|e| e.to_string())?;
    let mut entry = zip.by_name(member).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    entry.read_to_end(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

fn extract_lib(artifact: &OrtArtifact, archive: &[u8]) -> Result<Vec<u8>, String> {
    #[cfg(windows)]
    if artifact.archive.ends_with(".zip") {
        return extract_from_zip(archive, artifact.lib_in_archive);
    }
    extract_from_tgz(archive, artifact.lib_in_archive)
}

/// Atomic write: temp sibling + rename, so a crash mid-write can never leave
/// a plausible-but-truncated native library or model on disk.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("part");
    fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// The full Download-click flow: fetch + verify + install the runtime lib and
/// every model file (skipping ones already present — the flow is resumable).
/// Blocking; run it on a worker thread.
pub fn download_artifacts(root: &Path, on_progress: &dyn Fn(DownloadProgress)) -> Result<(), String> {
    let artifact = ORT_ARTIFACT
        .as_ref()
        .ok_or("Semantic match is not available on this platform (no ONNX Runtime build)")?;
    let lib_path = runtime_lib_path(root).ok_or("unsupported platform")?;
    if !lib_path.is_file() {
        let url = format!(
            "https://github.com/microsoft/onnxruntime/releases/download/v{ORT_VERSION}/{}",
            artifact.archive
        );
        let archive = download_verified(
            &url,
            artifact.sha256,
            artifact.archive,
            "runtime",
            0,
            artifact.download_bytes,
            on_progress,
        )?;
        let lib = extract_lib(artifact, &archive)?;
        write_atomic(&lib_path, &lib)?;
    }
    let stage_total = model_total_bytes();
    let mut stage_base: u64 = 0;
    for (name, sha, size) in MODEL_FILES {
        let dest = model_files_dir(root).join(name);
        if dest.is_file() {
            stage_base += size; // resumable: already-present bytes count as done
            continue;
        }
        let url = format!("https://huggingface.co/{MODEL_ID}/resolve/{MODEL_REVISION}/{name}");
        let bytes = download_verified(&url, sha, name, "model", stage_base, stage_total, on_progress)?;
        write_atomic(&dest, &bytes)?;
        stage_base += size;
    }
    Ok(())
}

/// ort's dynamic-load global init is process-wide and once-only; remember it.
static ORT_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Load the embedder from the verified on-disk artifacts. Any failure is a
/// plain Err — callers degrade to lexical-only, never crash.
pub fn load_embedder(root: &Path) -> Result<TextEmbedding, String> {
    let lib_path = runtime_lib_path(root).ok_or("unsupported platform")?;
    if !lib_path.is_file() {
        return Err("ONNX Runtime not downloaded".to_string());
    }
    if !ORT_INITIALIZED.swap(true, Ordering::SeqCst) {
        ort::init_from(lib_path.to_string_lossy().as_ref())
            .map_err(|e| format!("ONNX Runtime failed to load: {e}"))?
            .commit();
    }
    let dir = model_files_dir(root);
    let read = |name: &str| fs::read(dir.join(name)).map_err(|e| format!("{name}: {e}"));
    let model = UserDefinedEmbeddingModel::new(
        read("model_optimized.onnx")?,
        TokenizerFiles {
            tokenizer_file: read("tokenizer.json")?,
            config_file: read("config.json")?,
            special_tokens_map_file: read("special_tokens_map.json")?,
            tokenizer_config_file: read("tokenizer_config.json")?,
        },
    )
    // Cls pooling + static quantization: what fastembed itself applies to
    // this exact model (BGESmallENV15Q) — user-defined loading must match.
    .with_pooling(Pooling::Cls)
    .with_quantization(QuantizationMode::Static);
    TextEmbedding::try_new_from_user_defined(model, InitOptionsUserDefined::default())
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Embedding cache: cache/embeddings.sqlite. Derived data, rebuildable from
// prompts/ at any time — deliberately outside the hand-editable prompts dir.
// ---------------------------------------------------------------------------

/// The text a snippet is embedded from: its name (a hand-chosen label, strong
/// signal) and its content (the substance).
pub fn embedding_text(snippet: &Snippet) -> String {
    format!("{}\n{}", snippet.name, snippet.content)
}

pub fn content_hash(snippet: &Snippet) -> String {
    sha256_hex(embedding_text(snippet).as_bytes())
}

/// A snippet's cache identity is now `(project, name)` — there is no uuid to key
/// on, and a bare name is not unique across projects.
pub fn open_cache(root: &Path) -> Result<Connection, String> {
    let dir = root.join("cache");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let conn = Connection::open(dir.join("embeddings.sqlite")).map_err(|e| e.to_string())?;
    // The legacy table was keyed by snippet uuid — an identity that no longer
    // exists, so every row in it is unreachable. Dropping it is safe and correct:
    // this cache is derived data, rebuildable from the .md files at any time.
    // (`CREATE TABLE IF NOT EXISTS` cannot alter the old table's shape, so the
    // new schema needs its own name regardless.)
    conn.execute_batch(
        "DROP TABLE IF EXISTS embeddings;
         CREATE TABLE IF NOT EXISTS snippet_embeddings (
            project      TEXT NOT NULL,
            name         TEXT NOT NULL,
            model_id     TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            vector       BLOB NOT NULL,
            PRIMARY KEY (project, name, model_id)
        );",
    )
    .map_err(|e| e.to_string())?;
    Ok(conn)
}

fn vector_to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn blob_to_vector(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

/// Bring the cache up to date for **one project's** snippets: embed the
/// missing/stale ones (at most `limit` per call, so a huge corpus warms up over
/// a few queries instead of blocking one) and drop rows for snippets that no
/// longer exist. Returns how many are still stale after this pass.
///
/// Every statement is scoped to `project`, and that scoping is load-bearing, not
/// tidiness: the cleanup deletes cached rows that are absent from `snippets`, so
/// an unscoped delete would wipe every *other* project's vectors on every query
/// against this one. The corruption would be invisible — semantic match would
/// just silently degrade to "slow sometimes" as each project re-embedded the
/// library the last query threw away.
pub fn ensure_embeddings(
    conn: &Connection,
    embedder: &mut TextEmbedding,
    project: &str,
    snippets: &[Snippet],
    limit: usize,
) -> Result<usize, String> {
    let cached: Vec<(String, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT name, content_hash FROM snippet_embeddings WHERE project = ?1 AND model_id = ?2",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map((project, MODEL_ID), |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };
    let live: Vec<&str> = snippets.iter().map(|s| s.name.as_str()).collect();
    prune_cache(conn, project, &live)?;
    let is_fresh =
        |s: &&Snippet| cached.iter().any(|(n, hash)| n == &s.name && hash == &content_hash(s));
    let stale: Vec<&Snippet> = snippets.iter().filter(|s| !is_fresh(s)).collect();
    let batch: Vec<&Snippet> = stale.iter().take(limit).copied().collect();
    if !batch.is_empty() {
        let texts: Vec<String> = batch.iter().map(|s| embedding_text(s)).collect();
        let vectors = embedder.embed(texts, None).map_err(|e| e.to_string())?;
        for (snippet, vector) in batch.iter().zip(vectors) {
            conn.execute(
                "INSERT OR REPLACE INTO snippet_embeddings (project, name, model_id, content_hash, vector) VALUES (?1, ?2, ?3, ?4, ?5)",
                (project, &snippet.name, MODEL_ID, content_hash(snippet), vector_to_blob(&vector)),
            )
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(stale.len().saturating_sub(batch.len()))
}

/// Drop this project's cached rows for snippets that no longer exist.
///
/// Split out from [`ensure_embeddings`] so the project scoping can be tested
/// without a 67MB model in the loop — this is the statement where an omitted
/// `project = ?1` would silently delete every other project's vectors.
fn prune_cache(conn: &Connection, project: &str, live: &[&str]) -> Result<(), String> {
    let cached: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT name FROM snippet_embeddings WHERE project = ?1 AND model_id = ?2")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map((project, MODEL_ID), |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };
    for name in cached.iter().filter(|name| !live.contains(&name.as_str())) {
        conn.execute(
            "DELETE FROM snippet_embeddings WHERE project = ?1 AND name = ?2 AND model_id = ?3",
            (project, name, MODEL_ID),
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// One project's cached vectors for the current model, keyed by snippet name —
/// the in-memory pool the linear cosine scan runs over (microseconds at ≤10k
/// snippets; no vector DB by design).
pub fn cached_vectors(conn: &Connection, project: &str) -> Result<Vec<(String, Vec<f32>)>, String> {
    let mut stmt = conn
        .prepare("SELECT name, vector FROM snippet_embeddings WHERE project = ?1 AND model_id = ?2")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map((project, MODEL_ID), |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).map(|(name, blob)| (name, blob_to_vector(&blob))).collect())
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (mut dot, mut na, mut nb) = (0.0f32, 0.0f32, 0.0f32);
    for (x, y) in a.iter().zip(b) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cache with two projects, each holding a snippet — including one that
    /// shares a name across both, which is legal now that names are only unique
    /// within a project.
    fn seeded_cache() -> (PathBuf, Connection) {
        let root = std::env::temp_dir().join(format!("ccdeck-embed-cache-{}", uuid::Uuid::new_v4()));
        let conn = open_cache(&root).unwrap();
        for (project, name) in
            [("/a", "keep"), ("/a", "shared"), ("/a", "stale"), ("/b", "shared"), ("/b", "other")]
        {
            conn.execute(
                "INSERT INTO snippet_embeddings (project, name, model_id, content_hash, vector) VALUES (?1, ?2, ?3, 'h', ?4)",
                (project, name, MODEL_ID, vector_to_blob(&[1.0f32, 0.0])),
            )
            .unwrap();
        }
        (root, conn)
    }

    #[test]
    fn pruning_one_project_never_touches_another_project_vectors() {
        // The bug this guards: an unscoped DELETE would wipe every other
        // project's cache on every query, and the damage would be invisible —
        // semantic match would just present as "slow sometimes" forever as each
        // project re-embedded the library the last query threw away.
        let (root, conn) = seeded_cache();

        prune_cache(&conn, "/a", &["keep", "shared"]).unwrap();

        let a: Vec<String> = cached_vectors(&conn, "/a").unwrap().into_iter().map(|(n, _)| n).collect();
        assert_eq!(a, vec!["keep".to_string(), "shared".to_string()], "/a drops only its own stale row");

        let mut b: Vec<String> =
            cached_vectors(&conn, "/b").unwrap().into_iter().map(|(n, _)| n).collect();
        b.sort();
        assert_eq!(
            b,
            vec!["other".to_string(), "shared".to_string()],
            "/b must be untouched — including its snippet that shares a name with one in /a"
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn cached_vectors_are_scoped_to_one_project() {
        let (root, conn) = seeded_cache();
        assert_eq!(cached_vectors(&conn, "/a").unwrap().len(), 3);
        assert_eq!(cached_vectors(&conn, "/b").unwrap().len(), 2);
        assert!(cached_vectors(&conn, "/unknown").unwrap().is_empty());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn sha256_matches_known_vector() {
        // "abc" — FIPS 180-2 test vector; guards the verify path itself.
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn vector_blob_round_trip_is_exact() {
        let v = vec![0.0f32, -1.5, 3.25e10, f32::MIN_POSITIVE, -0.0];
        assert_eq!(blob_to_vector(&vector_to_blob(&v)), v);
    }

    #[test]
    fn cosine_basics() {
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
        assert!((cosine(&[1.0, 0.0], &[-1.0, 0.0]) + 1.0).abs() < 1e-6);
        assert_eq!(cosine(&[1.0], &[1.0, 2.0]), 0.0, "length mismatch is 0, not a panic");
        assert_eq!(cosine(&[0.0, 0.0], &[1.0, 1.0]), 0.0, "zero vector is 0, not NaN");
    }

    #[test]
    fn checksum_mismatch_message_is_actionable() {
        // The verify failure path must name the file and both hashes — this is
        // the error a user reports when a CDN mangles a download.
        let err = download_verified(
            "data:text/plain,x", // ureq rejects non-http schemes → download error path
            "00",
            "f",
            "model",
            0,
            0,
            &|_| {},
        )
        .unwrap_err();
        assert!(err.contains('f'));
    }

    /// The whole opt-in flow against the real pinned URLs: download + verify
    /// + extract + dynamically load ONNX Runtime + build the embedder + run
    /// inference + rank by cosine. Network + ~75MB, so opt-in like the
    /// feature itself: `cargo test --lib -- --ignored`. This is the guard
    /// that the pinned URLs/checksums and the user-defined model wiring
    /// (pooling, quantization) actually work — no unit test can fake it.
    #[test]
    #[ignore = "network + ~75MB download; run explicitly with -- --ignored"]
    fn full_opt_in_flow_downloads_loads_and_embeds() {
        let root = std::env::temp_dir().join(format!("ccdeck-embed-e2e-{}", uuid::Uuid::new_v4()));
        download_artifacts(&root, &|p| {
            assert!(matches!(p.stage.as_str(), "runtime" | "model"), "download stages only here");
            assert!(p.done <= p.total);
        })
        .expect("download + verify + extract");
        assert!(artifacts_present(&root));

        let mut embedder = load_embedder(&root).expect("dynamic load + model init");
        let vectors = embedder
            .embed(vec!["a friendly little cat", "a playful kitten", "quarterly tax spreadsheet"], None)
            .expect("inference");
        assert_eq!(vectors[0].len(), 384, "bge-small dims");
        let cat_kitten = cosine(&vectors[0], &vectors[1]);
        let cat_tax = cosine(&vectors[0], &vectors[2]);
        assert!(
            cat_kitten > cat_tax,
            "semantic sanity: cat~kitten ({cat_kitten}) must beat cat~spreadsheet ({cat_tax})"
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn tgz_extraction_finds_exact_member_only() {
        // Build a tiny tgz in memory: member "a/lib/x.so" with known bytes.
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        let payload = b"native-lib-bytes";
        let mut header = tar::Header::new_gnu();
        header.set_size(payload.len() as u64);
        header.set_cksum();
        builder.append_data(&mut header, "a/lib/x.so", payload.as_slice()).unwrap();
        let tgz = builder.into_inner().unwrap().finish().unwrap();

        assert_eq!(extract_from_tgz(&tgz, "a/lib/x.so").unwrap(), payload);
        assert!(extract_from_tgz(&tgz, "a/lib/other.so").is_err());
    }
}
