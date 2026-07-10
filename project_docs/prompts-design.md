# Prompt Library — Engineering Contract

Status: **CONTRACT — being built** (Core scope = design milestones M1+M2+M3). The product design
and its vision live in [issue #7's pinned design comment](https://github.com/zhangxingeng/ccdeck/issues/7)
— vocabulary, happy-path narrative, F1–F8 specs, settled decisions. This doc is the *engineering*
contract that design maps onto: storage, schema, the Rust↔JS command surface, the match-engine
architecture, and the compose-surface behavior model. It ages with the code, like
[search-design.md](search-design.md) does for chat search.

Decisions here were aligned with the founder 2026-07-09: **hybrid matching, lexical by default,
embeddings opt-in behind a user-clicked model download; JSON one-piece-per-file; storage in
ccdeck's own directory, not `~/.claude`** (and existing ccdeck data contamination in `~/.claude`
migrates out — see § Backups migration).

## Storage layout — `~/.ccdeck/`

ccdeck gets its own data root. Rationale: `~/.claude` belongs to Claude Code; parking our data
there bloats a directory users audit and other tools parse (we already did it once with
`.ccstudio-backups` — that moves out too).

```
~/.ccdeck/
  prompts/            # the piece library — one JSON file per piece, hand-editable, git-able
    <uuid>.json
  backups/            # session edit backups (migrated from ~/.claude/.ccstudio-backups)
    <sanitized-session-id>/v001-<unixsecs>.jsonl
  models/             # opt-in embedding model files (fastembed cache dir)
  cache/
    embeddings.sqlite # piece-embedding cache (piece_id, model_id, body_hash, vector blob)
```

- Root resolution: env `CCDECK_DATA_DIR` if set (tests need this — same pattern as
  `CLAUDE_CONFIG_DIR`), else `<home>/.ccdeck` via the `dirs` crate.
- `prompts/` holds **only** hand-editable piece JSON — no caches, no binaries — so a user can
  `git init` it or feed any file straight to an LLM. Derived data (embeddings) lives in `cache/`
  and is rebuildable from `prompts/` at any time.
- One piece per file: no file ever grows past what an LLM can ingest whole, and hand edits/git
  diffs stay per-piece. The `id` field is canonical; the loader reads every `prompts/*.json` and
  trusts content over filename (a hand-copied file with a stale name still loads); saves always
  write to `<id>.json`.

## Piece schema (canonical)

```json
{
  "id": "3f2a…-uuid-v4",
  "title": "senior-reviewer",
  "body": "You are a senior reviewer. Review the PR for {{ticket}}…",
  "keywords": ["review", "role"],
  "tags": [],
  "category": null,
  "scope": { "kind": "global" },
  "placeholders": [{ "name": "ticket" }],
  "created_at": 1770000000,
  "updated_at": 1770000000,
  "versions": [ { "body": "…prior body…", "saved_at": 1769990000 } ]
}
```

- `scope` is `{"kind":"global"}` or `{"kind":"project","project":"<absolute project path>"}` —
  the decoded cwd the app's project picker shows, readable in hand-edited JSON.
- `versions` is newest-first, **append-only on body change**: saving a piece whose body differs
  from the stored body pushes the old body (with its timestamp) onto `versions`. Product promise
  (issue #7 F7): a save never destroys the previous body. Metadata-only saves don't version.
- `placeholders` is derived from `{{token}}` occurrences in `body` at save time (single source of
  truth is the body; the array exists so consumers don't re-parse). Unknown extra fields in
  hand-edited files are preserved on round-trip (serde `flatten`/passthrough) — hand-editability
  means we never silently drop a user's field.
- `tags`/`category` are stored but have **no management UI in Core** — that's the M4 follow-up
  issue. Fuzzy match already searches them so hand-tagged pieces benefit today.

## Rust ↔ JS command contract

All async, `Result<T, String>` errors, snake_case, registered in `invoke_handler` — the existing
convention. New module: `src-tauri/src/prompts/`.

```
list_pieces() -> Piece[]
    // Every piece in the store (corpus is small; frontend filters by scope/project).
save_piece(piece: PieceInput) -> Piece
    // Create (no id) or update (id present). Handles versioning per the schema
    // rules above. Returns the stored piece.
delete_piece(id: string) -> null
piece_load_errors() -> { file: string, error: string }[]
    // Piece files the loader had to skip (broken JSON from a hand-edit,
    // shadowed duplicate id). Call alongside list_pieces and show a warning —
    // a skipped file must never read as a silently vanished piece (the file
    // itself always stays intact on disk for the user to fix). Runs a fresh
    // scan of the store, so it reflects current on-disk state.
match_pieces(query: string, project: string | null, limit: number) -> MatchHit[]
    // MatchHit { id: string, score: number, source: "lexical" | "semantic" | "hybrid" }
    // Pool: global pieces + pieces scoped to `project` (null = global only).
    // Engine per § Match engine; callers never know which engine ran.
embed_status() -> EmbedStatus
    // { state: "off" | "not_downloaded" | "downloading" | "ready" | "error",
    //   model_id: string, model_size_mb: number, runtime_size_mb: number,
    //   error?: string }
    // runtime_size_mb: the ONNX Runtime download (see § Match engine, dynamic
    // loading) — the UI's requirements note quotes model + runtime, the TOTAL
    // opt-in download. Platforms with no pinned runtime build (macOS Intel:
    // Microsoft ships no 1.24.x artifact) report state "error" with an
    // explanatory message; matching stays lexical-only there.
embed_download(channel) -> null
    // Streams progress events over a tauri Channel (same pattern as `search`):
    // { stage: "runtime" | "model", downloaded_bytes, total_bytes } — one
    // stage per artifact (the ONNX Runtime archive, then the model files
    // under one fixed total). Completion/error are NOT channel events: the
    // command's Result is the terminal signal, and the frontend re-fetches
    // embed_status afterward (which reflects the true post-download state,
    // including a failed download's error message).
set_embed_enabled(enabled: bool) -> null
    // Persisted app-config toggle (appconfig.rs); "ready" + enabled = hybrid
    // on. Toggling is also the retry affordance: it clears sticky
    // error/degradation state, and off unloads the model from RAM.
```

## Match engine — hybrid, lexical default, embeddings opt-in

- **Lexical (always on, zero deps):** fzf-style fuzzy subsequence scoring with field weighting
  (title > keywords/tags > body). NOT tantivy/BM25 — BM25's term-frequency statistics earn their
  keep on large noisy corpora (chat search); over a few hundred curated snippets, subsequence
  match + field weights is both better-fitting and dependency-free.
- **Semantic (opt-in):** fastembed-rs (ONNX, CPU) with a small, mature retrieval model (builder
  picks the concrete model — constraints: fastembed-supported, ≤~130MB download, strong
  MTEB-retrieval-per-MB; e.g. bge-small-en-v1.5 class). Built as: the fastembed-blessed
  quantized `Qdrant/bge-small-en-v1.5-onnx-Q` (~64MB, 384 dims, Cls pooling), pinned to an
  exact HF revision. Model downloads to `~/.ccdeck/models/`
  **only when the user clicks Download** in the Prompts view — never at install, never silently.
  The UI states the requirements before download (~size on disk, CPU-only inference, indicative
  RAM) so weak-machine users can decline informed.
- **ONNX Runtime is dynamically loaded, not statically linked** (Gate-1 ruling): fastembed's
  default would bake a 15–30MB runtime into every shipped binary for a feature most users never
  enable. Instead the build uses ort's `load-dynamic`, and the single Download click fetches
  BOTH the runtime and the model. Security posture (downloading native code is an RCE vector
  if done sloppily): the runtime is the official Microsoft release archive at a pinned exact
  version, the model files a pinned HF revision, and every artifact is verified against a
  hardcoded sha256 BEFORE any use — a mismatch deletes the file and reports error state, never
  loads. A missing/failed runtime degrades to lexical-only; the app never blocks on it.
- **No vector database.** Piece embeddings are cached in `cache/embeddings.sqlite` keyed by
  (piece_id, model_id, body_hash) and KNN is a **linear cosine scan in memory** — microseconds
  at ≤10k pieces. sqlite-vec (named in the original design comment) is deliberately dropped
  until scale demands it; the design comment's own engine reasoning survives, the dependency
  doesn't.
- **Hybrid fusion:** when embeddings are ready+enabled, `match_pieces` runs both engines and
  fuses (reciprocal-rank or normalized-score fusion — builder's call, contract only requires:
  an exact title/keyword hit must never be buried by a middling semantic score). Query embedding
  is computed per debounced keystroke — acceptable because the corpus is small and the model is
  CPU-cheap; if inference latency exceeds the UI debounce budget, degrade to lexical for that
  query rather than blocking the panel.

## Compose surface — provenance model (behavioral contract)

The compose box state machine (issue #7 F1) that the frontend must honor, however it is
implemented (contenteditable, span overlay — builder's call):

- Span states: **typed** (plain), **linked** (from a piece, unchanged — tinted), **linked-modified**
  (linked, then edited inline — tinted + marker). Editing a linked span in place transitions it to
  linked-modified and never touches the stored piece.
- Insertion lands at the cursor as a linked span; a piece with placeholders shows the fill-in
  popover first, and the filled span remembers its template + fill values.
- **Copy prompt** flattens the box to plain text: provenance stripped, placeholder values
  substituted. What you see (text content) is exactly what you get.
- Provenance colors come from CSS variables in `app.css` following the existing `--accent-*`
  token pattern (with `color-mix` tints), themed for light + dark.

## Legacy-state migration — de-contaminate `~/.claude`

Shipped installs park two ccdeck-owned artifacts inside Claude Code's directory:
session-edit backups at `~/.claude/.ccstudio-backups` and the app config at
`~/.claude/.ccstudio-config.json`. Both move out (founder directive: existing contamination
moves too, not just new writes) — backups to `~/.ccdeck/backups/`, config to
`~/.ccdeck/config.json`.

- On startup (in `run()`), if the legacy backups dir exists and `~/.ccdeck/backups` does not:
  rename (same filesystem — it's the same home dir) and leave nothing behind. If both exist (a
  half migration or downgrade-then-upgrade), merge legacy session dirs that don't collide,
  prefer the new location on collision (the colliding legacy dir is left in place — deleting a
  backup a user might still want is worse than leaving residue), and remove the legacy dir only
  when emptied.
- The config file migrates in the same pass: legacy-only → rename; both exist → the new
  location wins (written by a newer install) and the superseded legacy file is removed, so the
  invariant below reads literally true.
- Migration failure is non-fatal: the app must still boot (backups and config are conveniences,
  not core data); log and fall back to reading whichever location has the file. New writes
  always target `~/.ccdeck/` — a failed migration never re-contaminates.
- After this change **nothing ccdeck-owned lives under `~/.claude`** — invariant to keep for
  every future feature.

## Deliberately out of Core (filed, not dropped)

- **M4 Organization** (tag/category management UI, browse-by-tag panel) — follow-up issue.
- **Presets, RAG auto-assembly, sharing/export** — issue #7's deferred list, unchanged.
- **e2e test for the compose surface** — Core ships Rust unit tests (store round-trip with
  hostile fixtures, versioning invariants, matcher ranking) + a `tests/prompts_smoke.mjs`;
  a Playwright spec follows once the founder's hands-on pass settles the interaction details.
