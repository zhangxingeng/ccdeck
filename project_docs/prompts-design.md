# Prompt Library — Engineering Contract

Status: **CONTRACT — Revision round in build 2026-07-10** (founder's post-feel-check design
revision on top of Core M1+M2+M3, issue #24; on branch `prompt-library`, unmerged, no shipped
release contains this feature). The product design and its vision live in
[issue #7's pinned design comment](https://github.com/zhangxingeng/ccdeck/issues/7). This doc is
the *engineering* contract that design maps onto: storage, schema, the Rust↔JS command surface,
the variable grammar, the match-engine architecture, and the compose-surface behavior model. It
ages with the code, like [search-design.md](search-design.md) does for chat search.

Founder directives folded in 2026-07-10: **projects are first-class (creatable, colored,
pinnable as tabs); variables are single-brace f-string style with defaults; the compose surface
is the only typing surface (situational affordances, no persistent buttons); keywords/tags are
demoted to metadata; embeddings collapse into a config popover; the store survives hand-edit
JSON corruption. Apple vibe, not geek vibe: simple per page, split meaningfully.**

## Storage layout — `~/.ccdeck/`

ccdeck's own data root (env `CCDECK_DATA_DIR` overrides for tests, else `<home>/.ccdeck` via
`dirs`). Rationale and the `~/.claude` de-contamination invariant are unchanged (§ Legacy-state
migration).

```
~/.ccdeck/
  prompts/            # the piece library — one JSON file per piece, hand-editable, git-able
    <uuid>.json
  projects.json       # the project roster — one small file (records are tiny and few;
                      # one-file-per-record earns nothing here, unlike pieces)
  backups/            # session edit backups
  models/             # opt-in embedding model files
  cache/
    embeddings.sqlite # piece-embedding cache (piece_id, model_id, body_hash, vector blob)
```

`prompts/` holds **only** hand-editable piece JSON. One piece per file (LLM-ingestable whole,
per-piece diffs). `id` is canonical; the loader trusts content over filename; saves write
`<id>.json`.

## Project model (new this round)

A project is a named, colored grouping for pieces — the unit the tabs, the compose-box tint,
and piece-span hues all key off.

```json
{
  "projects": [
    {
      "id": "uuid-v4",
      "name": "ccdeck",
      "color": "blue",
      "pinned": true,
      "path": null,
      "created_at": 1770000000
    }
  ]
}
```

- **`color` is a palette key, never a hex value** — one of the fixed preset keys
  `red | orange | yellow | green | teal | blue | purple | pink | graphite`. Each key maps to a
  `--project-<key>` CSS token defined in `app.css` for light AND dark (color-token protocol:
  stored data carries intent, the theme file owns the hue — dark-mode contrast stays retunable
  in one file, and a user can never pick an unreadable arbitrary hex).
- `pinned: true` renders the project as a tab atop the Prompts view. Global is always the first
  tab (white/neutral — it is not a project record).
- `path` is optional metadata (absolute project dir) for future auto-scoping; no behavior hangs
  on it in this round.
- **Delete semantics: deleting a project rescopes its pieces to global.** Nothing a user wrote
  ever vanishes as a side effect; the pieces surface again under Global.

Piece `scope` (v2) references projects by id:

```json
"scope": { "kind": "global" }
"scope": { "kind": "project", "project_id": "uuid-v4" }
```

Legacy/unknown scope shapes (the pre-revision path-keyed form, or a `project_id` that matches no
roster entry) load as **global** plus an entry in `piece_load_errors` — visible, non-fatal, file
untouched. (Migration reality: the feature never shipped in a release; only founder feel-check
data exists. No dual-schema machinery — the honest notice is the whole path.)

## Piece schema (canonical)

```json
{
  "id": "3f2a…-uuid-v4",
  "title": "senior-reviewer",
  "body": "You are a senior reviewer. Review the PR for {ticket:ABC-123}…",
  "keywords": ["review", "role"],
  "tags": [],
  "category": null,
  "scope": { "kind": "global" },
  "placeholders": [{ "name": "ticket", "default": "ABC-123" }],
  "created_at": 1770000000,
  "updated_at": 1770000000,
  "versions": [ { "body": "…prior body…", "saved_at": 1769990000 } ]
}
```

- `versions` is newest-first, **append-only on body change** — a save never destroys the
  previous body. Metadata-only saves don't version. (Unchanged from Core.)
- `placeholders` is derived from the variable grammar (below) at save time; each entry carries
  `name` and optional `default`. The body is the single source of truth.
- Unknown extra fields in hand-edited files are preserved on round-trip (serde flatten).
- `keywords`/`tags`/`category` are **metadata** — no top-level UI prominence; they live in the
  piece modal's Metadata tab. Fuzzy match still searches them.
- `list_pieces` may additionally mark a piece with transient `"recovered": true` (never written
  to disk) — see § Store robustness.

## Variable grammar (shared spec — Rust and TS MUST implement identically)

Single-brace, python-f-string flavored. This section is the seam contract: last round's audit
caught the two sides diverging on exactly this class of rule, so both implementations test
against the shared vectors below.

Scan left-to-right:

1. `{{` emits literal `{`; `}}` emits literal `}` (escapes consume first).
2. `{name}` or `{name:default}` is a **variable** when `name` matches `[A-Za-z0-9_-]+`
   (case-sensitive). The first `:` separates name from default; the default is everything up to
   the closing `}` and may not contain braces. Equivalent token regex after escape handling:
   `\{([A-Za-z0-9_-]+)(?::([^{}]*))?\}`.
3. Any other braced run (invalid name, spaces, quotes, nesting) is left **verbatim** — JSON
   examples inside prompt bodies never parse as variables.
4. The same name is the **same variable everywhere** in a composed document — one fill value
   serves every occurrence across every inserted piece and typed text (this is the point:
   standardized names like `{task}` fill once).

Shared test vectors (both sides assert all of these):

| input | result |
|-|-|
| `{task}` | variable `task`, no default |
| `{task:write tests}` | variable `task`, default `write tests` |
| `{task:}` | variable `task`, default `""` (fills as empty when unfilled) |
| `{x:a:b}` | variable `x`, default `a:b` (first colon splits) |
| `{{task}}` | literal `{task}` — no variable |
| `{"a": 1}` | literal — invalid name |
| `{my var}` | literal — space |
| `{x-1_Y}` | variable `x-1_Y` |
| `{:x}` | literal — empty name |
| `{{{task}}}` | literal `{` + variable `task` + literal `}` |

## Copy output — the "as variable" toggle

A compose-box toggle, default **ON**, persisted in app config (`prompts_as_variable`). Copy
always resolves escapes (`{{`→`{`).

- **ON** (dedup mode — a long value is stated once, never repeated inline): every variable
  occurrence becomes `<prompt_var name="x"/>`, and a block is appended after the body listing
  each distinct variable in first-appearance order:

  ```
  Review the PR for <prompt_var name="ticket"/> and summarize.

  <prompt_vars>
  <prompt_var name="ticket">ABC-123</prompt_var>
  </prompt_vars>
  ```

  Value resolution: user-filled value, else default, else empty element (an empty
  `<prompt_var name="x"></prompt_var>` is an honest "fill me" signal to the reading LLM).
  The wrapper form `<prompt_var name="x">` is used (not `<x>`) because variable names may start
  with digits or hyphens, which are invalid XML element names.
- **OFF** (substitute in place): each occurrence is replaced by user value, else default, else
  the literal `{x}` stays — visible, so an unfilled variable is never silently blanked.

## Rust ↔ JS command contract

All async, `Result<T, String>`, snake_case, registered in `invoke_handler`. Module:
`src-tauri/src/prompts/`.

```
list_pieces() -> Piece[]                       // may carry transient recovered: true
save_piece(piece: PieceInput) -> Piece         // create (no id) / update; versioning per schema
delete_piece(id: string) -> null
piece_load_errors() -> { file: string, error: string }[]
    // Skipped/degraded piece files: broken JSON that repair could not recover, shadowed
    // duplicate ids, legacy/unknown scope fallbacks. Fresh scan; files stay intact on disk.

list_projects() -> Project[]
save_project(project: ProjectInput) -> Project // create (no id) / update (rename, color, pin)
delete_project(id: string) -> null             // rescopes the project's pieces to global

match_pieces(query: string, project_id: string | null, limit: number) -> MatchHit[]
    // MatchHit { id, score, source: "lexical" | "semantic" | "hybrid" }
    // Pool: global pieces + pieces scoped to project_id (null = global only).

embed_status() -> EmbedStatus                  // unchanged from Core (state, model_id,
                                               // model_size_mb, runtime_size_mb, error?)
embed_download(channel) -> null
    // Progress events: { stage: "runtime" | "model" | "index", done: number, total: number }
    // — bytes for the two download stages, piece counts for "index" (embedding the existing
    // library, which now runs as part of the same one-click flow). Terminal signal is the
    // command Result + an embed_status re-fetch, as in Core.
set_embed_enabled(enabled: bool) -> null
```

## Match engine — hybrid, lexical default, embeddings opt-in

Unchanged from Core (fzf-style weighted lexical always on; fastembed-rs
`Qdrant/bge-small-en-v1.5-onnx-Q` opt-in; ort `load-dynamic` with pinned sha256-verified
artifacts; linear cosine KNN over `cache/embeddings.sqlite`; fusion where an exact title hit is
never buried). The only surface change: the download flow now ends with the **index** stage
(embed every existing piece) so the popover's promise — "Download & index" — is literally what
the one click does.

## Compose surface — behavioral contract (revised)

The compose box holds **raw literal text** — including `{var}` tokens, which are substituted
only at copy time. Spans track provenance as before: **typed**, **linked** (from a piece,
unchanged), **linked-modified** (edited inline; never touches the stored piece).

- **Tabs**: Global plus every pinned project, atop the view. The active tab is the scope — it
  sets the match pool (`match_pieces` project_id), the save target for new pieces, and the
  visual tint. Unpinned projects are reachable through the project manager popover.
- **Situational affordances — no persistent buttons**:
  - *Copy Prompt* appears bottom-right only while the box has content.
  - *Save as piece* floats next to an active text selection; it opens the piece modal prefilled
    with the selection, scoped to the active tab. (This is also how new pieces are born — there
    is no "+ New piece" button; everything is typed in the compose box first.)
  - The *variable fill list* auto-appears beneath the box whenever parsing finds variables: one
    row per distinct name (first-appearance order) showing the name, its default as the input's
    placeholder text, and a fill input. The fill-at-insert popover from Core is retired —
    inserting a piece with variables just merges its names into this unified list.
- **Color language** (all values are `app.css` tokens or `color-mix` over tokens — no hex in
  components; light + dark both defined):
  - Compose-box background: a *faint hint* of the active project's `--project-<key>` (via
    `color-mix`, low single-digit percent); plain neutral/white on Global. The tint is contained
    to the compose box, never the whole app.
  - Piece spans: translucent fills — greyish for global pieces, a darker-hue translucent mix of
    the piece's project color for project pieces.
  - Text selection inside the box: `--highlight` (highlighter yellow — light but bright, defined
    for both themes) via `::selection` scoped to the compose surface.
  - The active project's color reaches components through one CSS custom property
    (`--project-color`) set at the view wrapper from the palette token — components never branch
    on color keys.
- **Piece modal**: two tabs — **Content** (title, body, and a read-only variable preview:
  parsed names + defaults) and **Metadata** (keywords, tags, category). Editing reached from the
  match panel or a linked span, as in Core.
- **Embeddings/config popover**: replaces the inline embeddings panel. One "Download & index"
  action with the requirements note (sizes, CPU-only), then two progress bars — Download
  (runtime + model stages aggregated) and Index (the "index" stage) — plus the enable toggle.

## Store robustness — hand-edit corruption (new this round)

- On JSON parse failure the loader attempts an **in-memory jsonrepair-style recovery** (vetted
  mature crate if one exists — builder verifies — else a bounded port: unquoted keys, trailing
  commas, comments, single quotes, truncation). A recovered piece loads flagged
  `recovered: true` (transient) and the UI shows it needs attention.
- **The user's file on disk is never rewritten by the loader.** The repaired form persists only
  on the user's next explicit save of that piece — which appends a version like any body change.
- Unrecoverable files stay in `piece_load_errors` exactly as in Core: visible, intact on disk.

## Legacy-state migration — de-contaminate `~/.claude`

Unchanged from Core: `.ccstudio-backups` → `~/.ccdeck/backups`, `.ccstudio-config.json` →
`~/.ccdeck/config.json`, `.ccstudio-index` → `~/.ccdeck/index`; non-fatal, collision rules as
implemented; invariant: **nothing ccdeck-owned lives under `~/.claude`**.

## Deliberately out of this round (filed, not dropped)

- **M4 Organization** (browse-by-tag panel, bulk metadata) — issue #25.
- **Presets, RAG auto-assembly, sharing/export** — issue #7's deferred list.
- **Playwright e2e for the compose surface** — after this round's interactions settle.
- **`path`-driven auto-scoping of projects** — the field exists, no behavior yet.
