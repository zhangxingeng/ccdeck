# Prompt Library — Engineering Contract

Status: **CONTRACT — UX round in build 2026-07-10** (founder's second feel-check pass on top of
the revision round, issue #24; on branch `prompt-library`, unmerged, no shipped release contains
this feature). The product design and its vision live in
[issue #7's pinned design comment](https://github.com/zhangxingeng/ccdeck/issues/7). This doc is
the *engineering* contract that design maps onto: storage, schema, the Rust↔JS command surface,
the variable grammar, the match-engine architecture, and the compose-surface behavior model. It
ages with the code, like [search-design.md](search-design.md) does for chat search.

Its sibling is [prompts-ux.md](prompts-ux.md), the **interaction contract**: every user scenario
with its exact keys and resulting state. Where this doc says what the system *is*, that one says
what the user *does*. Behavior questions belong there; seam shapes belong here. Neither is
authoritative over the other's half.

Founder directives folded in 2026-07-10: **projects are first-class (creatable, colored,
pinnable as tabs); variables are single-brace f-string style with defaults; the compose surface
is the only typing surface; keywords/tags are demoted to metadata; embeddings collapse into a
config popover; the store survives hand-edit JSON corruption. Apple vibe, not geek vibe: simple
per page, split meaningfully.**

Round-B directives (this round): **the user must be able to work mouse-off; as-variable is a
per-variable choice, not one switch; notifications are transient but data events stay
recoverable; inline edits reconcile at save time (update the snippet, or save a new one); config
is app-level, not a library-panel control; hotkeys exist and rebind.**

## Storage layout — `~/.ccdeck/`

ccdeck's own data root (env `CCDECK_DATA_DIR` overrides for tests, else `<home>/.ccdeck` via
`dirs`). Rationale and the `~/.claude` de-contamination invariant are unchanged (§ Legacy-state
migration).

```
~/.ccdeck/
  prompts/            # the snippet library — one JSON file per snippet, hand-editable, git-able
    <uuid>.json
  projects.json       # the project roster — one small file (records are tiny and few;
                      # one-file-per-record earns nothing here, unlike snippets)
  backups/            # session edit backups
  models/             # opt-in embedding model files
  cache/
    embeddings.sqlite # snippet-embedding cache (piece_id, model_id, body_hash, vector blob)
```

`prompts/` holds **only** hand-editable snippet JSON. One snippet per file (LLM-ingestable whole,
per-snippet diffs). `id` is canonical; the loader trusts content over filename; saves write
`<id>.json`.

## Project model (new this round)

A project is a named, colored grouping for snippets — the unit the tabs, the compose-box tint,
and snippet-span hues all key off.

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
- **Delete semantics: deleting a project rescopes its snippets to global.** Nothing a user wrote
  ever vanishes as a side effect; the snippets surface again under Global.

Snippet `scope` (v2) references projects by id:

```json
"scope": { "kind": "global" }
"scope": { "kind": "project", "project_id": "uuid-v4" }
```

Legacy/unknown scope shapes (the pre-revision path-keyed form, or a `project_id` that matches no
roster entry) load as **global** plus an entry in `snippet_load_errors` — visible, non-fatal, file
untouched. (Migration reality: the feature never shipped in a release; only founder feel-check
data exists. No dual-schema machinery — the honest notice is the whole path.)

## Snippet schema (canonical)

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
  snippet modal's Metadata tab. Fuzzy match still searches them.
- `list_snippets` may additionally mark a snippet with transient `"recovered": true` (never written
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
   serves every occurrence across every inserted snippet and typed text (this is the point:
   standardized names like `{task}` fill once).
5. When the same name appears with differing defaults, **the first occurrence's default wins**
   (consistent with first-appearance ordering everywhere else in this contract).

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
| `{x:a} {x:b}` | one variable `x`, default `a` (first occurrence wins) |
| `{x} {x:b}` | one variable `x`, **no** default (rule 5 reads plainly: the first occurrence wins even when it carries no default — predictability over helpfulness) |
| `{a:{b}}` | literal `{a:` + variable `b` + literal `}` (a failed variable run consumes nothing; scanning resumes within it, same shape as `{{{task}}}`) |

## Copy output — the per-variable "as variable" toggle

**One toggle per variable**, living on that variable's row in the fill list. Every variable
defaults **ON**; the user turns individual ones off. Founder's rule, and the reason it is not a
smarter default: *as-variable never breaks anything, while substituting the wrong data in place
can bloat the prompt.* The failure modes are asymmetric, so the default takes the safe side and
the user opts out per variable when substitution reads better.

A document therefore mixes modes: some variables are referenced and hoisted, others substituted
inline. The appended block lists **only the ON variables**, still in first-appearance order.
There is no global switch and **no `prompts_as_variable` app-config field** (removed this round —
the setting is per-variable, per-session, and never written to the snippet). The frontend copy
builder takes a per-name map:

```ts
copyText(text: string, fills: Record<string, string>, asVars: Record<string, boolean>): string
// A name absent from `asVars` is ON. Copy rendering is frontend-only; Rust never renders.
```

Copy always resolves escapes (`{{`→`{`).

- **ON** (dedup mode — a long value is stated once, never repeated inline): every occurrence of
  that variable becomes `<prompt_var name="x"/>`, and a block is appended after the body listing
  each distinct ON variable in first-appearance order:

  ```
  Review the PR for <prompt_var name="ticket"/> and summarize.

  <prompt_vars>
  <prompt_var name="ticket">ABC-123</prompt_var>
  </prompt_vars>
  ```

  Value resolution: user-filled value, else default, else empty element (an empty
  `<prompt_var name="x"></prompt_var>` is an honest "fill me" signal to the reading LLM).
  The wrapper form `<prompt_var name="x">` is used (not `<x>`) because variable names may start
  with digits or hyphens, which are invalid XML element names. Values interpolated into the
  block are XML-escaped (`&`→`&amp;`, `<`→`&lt;`, `>`→`&gt;`): the wrapper form exists for
  parseability, and an unescaped value containing `</prompt_var>` could inject phantom
  variables into what the reading LLM sees. (Names need no escaping — the grammar's name class
  is attribute-safe by construction.)
- **OFF** (substitute in place): each occurrence of that variable is replaced by user value, else
  default, else the **canonical** literal `{x}` stays (not the occurrence's original spelling —
  relevant when a later occurrence carried a rule-5-ignored default) — visible, so an unfilled
  variable is never silently blanked. Substituted values are plain text and are **never**
  XML-escaped; escaping is a property of the block, not of the prompt.

## Hotkeys (new this round)

Prompts-view-scoped, armed on view enter, never firing while a modal owns the keyboard. The
global `Ctrl/Cmd+K` still wins. Defaults and the full interaction live in
[prompts-ux.md](prompts-ux.md#hotkey-map); the seam here is storage:

```
AppConfig.hotkeys: Record<string, string>   // command id -> chord, e.g. "copyPrompt": "Mod+C"
```

- Persisted through the existing `get_app_config` / `set_app_config` — **no new command surface.**
- A chord is normalized with `Mod` standing for `Ctrl` on Windows/Linux and `Cmd` on macOS, so a
  single stored binding is correct on every platform and a config file is portable.
- An absent command id falls back to its default. A fresh install and a pre-hotkeys config are
  therefore the same case, and neither needs a migration.
- Only *command* hotkeys rebind. `↓` / `Enter` / `Esc` are spatial and contextual keys, not
  commands: rebinding them would break the conventions the rest of the interaction infers from.

## Rust ↔ JS command contract

All async, `Result<T, String>`, snake_case, registered in `invoke_handler`. Module:
`src-tauri/src/prompts/`.

```
list_snippets() -> Snippet[]                       // may carry transient recovered: true
save_snippet(snippet: SnippetInput) -> Snippet         // create (no id) / update; versioning per schema
delete_snippet(id: string) -> null
snippet_load_errors() -> { file: string, error: string }[]
    // Skipped/degraded snippet files: broken JSON that repair could not recover, shadowed
    // duplicate ids, legacy/unknown scope fallbacks. Fresh scan; files stay intact on disk.

list_projects() -> Project[]
save_project(project: ProjectInput) -> Project // create (no id) / update (rename, color, pin)
delete_project(id: string) -> null             // rescopes the project's snippets to global

match_snippets(query: string, project_id: string | null, limit: number) -> MatchHit[]
    // MatchHit { id, score, source: "lexical" | "semantic" | "hybrid" }
    // Pool: global snippets + snippets scoped to project_id (null = global only).

embed_status() -> EmbedStatus                  // unchanged from Core (state, model_id,
                                               // model_size_mb, runtime_size_mb, error?)
embed_download(channel) -> null
    // Progress events: { stage: "runtime" | "model" | "index", done: number, total: number }
    // — bytes for the two download stages, snippet counts for "index" (embedding the existing
    // library, which now runs as part of the same one-click flow). Terminal signal is the
    // command Result + an embed_status re-fetch, as in Core.
set_embed_enabled(enabled: bool) -> null
```

## Match engine — hybrid, lexical default, embeddings opt-in

Unchanged from Core (fzf-style weighted lexical always on; fastembed-rs
`Qdrant/bge-small-en-v1.5-onnx-Q` opt-in; ort `load-dynamic` with pinned sha256-verified
artifacts; linear cosine KNN over `cache/embeddings.sqlite`; fusion where an exact title hit is
never buried). The only surface change: the download flow now ends with the **index** stage
(embed every existing snippet) so the popover's promise — "Download & index" — is literally what
the one click does.

## Compose surface — behavioral contract (revised)

The compose box holds **raw literal text** — including `{var}` tokens, which are substituted
only at copy time. Spans track provenance as before: **typed**, **linked** (from a snippet,
unchanged), **linked-modified** (edited inline; never touches the stored snippet).

Switching tabs **never edits the draft**: composed text survives a tab switch untouched, and
cross-project reference is allowed. Scope is a decision made at save time, never a mode the draft
lives inside — so the app never nags a user to move what they wrote.

- **Tabs**: Global plus every pinned project, atop the view. The active tab is the scope — it
  sets the match pool (`match_snippets` project_id), the *default* save target for new snippets, and
  the visual tint. Unpinned projects are reachable through the project manager popover. The tab
  row is a roving-tabindex widget (`←`/`→` move, `Enter`/`Space` activate).
- **Situational affordances, with exactly one persistent control**:
  - *Copy Prompt* appears bottom-right only while the box has content.
  - *Save as…* is **always present** (bottom-left) and selection-aware: it saves the selection
    when there is one, else the whole box. This is the round-B walk-back on "no persistent
    buttons" — "save what I just wrote" is a first-class intent, and requiring a selection first
    made whole-box save impossible. The floating *Save as…* next to a live selection remains as
    the fast mouse path; both open the same modal and both surface the target scope with a
    one-click switch to Global or another project.
  - The *variable fill list* auto-appears beneath the box whenever parsing finds variables: one
    row per distinct name (first-appearance order) showing the name, its default as the input's
    placeholder text, a fill input, and that variable's **as-variable toggle** (§ Copy output).
    The fill-at-insert popover from Core is retired — inserting a snippet with variables just
    merges its names into this unified list.
- **Insertion is one path with two triggers** (click a hit, or `↓` into the panel then `Enter`).
  Either way the inserted body **replaces the query line** — the text from the current line's
  start to the caret, which is exactly what `caretQuery` matched on. The query was scaffolding
  for finding the snippet; leaving it in front of the inserted body is litter.
  - `↓` steps from the box into the panel **only when the caret is at the very end of the text**,
    the one position where `↓` is natively inert in a textarea. Everywhere else it moves the
    caret, as a user editing mid-document expects.
  - Consequently `Enter` inserts only *after* that explicit step. The two rules are one decision:
    since the whole line is the query, an auto-armed `Enter` would let a stray keypress replace a
    sentence the user meant to keep. Change one and the other must change with it.
- **Color language** (all values are `app.css` tokens or `color-mix` over tokens — no hex in
  components; light + dark both defined):
  - Compose-box background: a *faint hint* of the active project's `--project-<key>` (via
    `color-mix`, low single-digit percent); plain neutral/white on Global. The tint is contained
    to the compose box, never the whole app.
  - Snippet spans: translucent fills — greyish for global snippets, a darker-hue translucent mix of
    the snippet's project color for project snippets.
  - Text selection inside the box: `--highlight` (highlighter yellow — light but bright, defined
    for both themes) via `::selection` scoped to the compose surface.
  - The active project's color reaches components through one CSS custom property
    (`--project-color`) set at the view wrapper from the palette token — components never branch
    on color keys.
- **Snippet modal**: two tabs — **Content** (title, body, and a read-only variable preview:
  parsed names + defaults) and **Metadata** (keywords, tags, category). Editing reached from the
  match panel or a linked span, as in Core. Opened from a span, the body defaults to the span's
  **current edited text**, not the stored body — the user edited it because they meant to, and
  showing them the old text is an answer to a question they did not ask. An **Original** button
  (beside Delete) previews the stored body read-only; it never reverts, because a one-click
  "Original" that discarded the user's typing is exactly the surprise this product avoids.
  Reverting, if offered, is a separate labeled action.
  - Two save actions: **Update snippet** (write the edit back as canonical — appends a version,
    destroys nothing) and **Save as new** (a fresh snippet from the edited text, original
    untouched). After either, the span relinks to the snippet it now reflects.
  - Inline edits **never** mutate a stored snippet on their own. The compose box is a scratch
    surface; reconciliation is always an explicit act.
  - Other spans of the same snippet already in the box do **not** re-sync on Update: spans are
    point-in-time snapshots and the tint marks origin, not liveness. A quiet toast says how many
    copies were left unchanged, so the choice is visible rather than silent.
- **Config popover — app-level, anchored at the right of the tab row.** It is not a library-panel
  control: today's placement inside the panel head is why it renders behind the compose box and
  drags a resizer into view. Fix the containment at root — the popover must not be clipped or
  stacked by any ancestor's `overflow`/stacking context — rather than only moving it, or the same
  bug reappears at the next anchor. It holds semantic-matching config (one "Download & index"
  action with the requirements note, then two progress bars — Download and Index — plus the
  enable toggle), the **Shortcuts** rebinding rows, and **Notices** (§ Store robustness).
- **Popovers and the modal trap focus** (`Tab`/`Shift+Tab` cycle within; the opener is refocused
  on close) and close on `Esc` or click-away. A mouse-off product where `Tab` walks focus behind
  an open modal is not actually operable without a mouse — the trap is load-bearing, not polish.

## Store robustness — hand-edit corruption (new this round)

- On JSON parse failure the loader attempts an **in-memory jsonrepair-style recovery** (vetted
  mature crate if one exists — builder verifies — else a bounded port: unquoted keys, trailing
  commas, comments, single quotes, truncation). A recovered snippet loads flagged
  `recovered: true` (transient).

  **How the UI shows it needs attention** (revised): a repair is a *data event* — it touched the
  user's files. It announces itself in a 5-second toast like every other notification, but unlike
  a confirmation it also leaves a durable trace: a badge on the config gear, and a **Notices**
  section in the config popover listing each repaired snippet (with the "open and re-save to
  persist the repair" nudge) and each unreadable file. The badge clears when the condition does.
  A transient surface must never be the only record of something that changed a user's data —
  and a permanent banner for a routine event is the clutter this design otherwise refuses.
- **The user's file on disk is never rewritten by the loader.** The repaired form persists only
  on the user's next explicit save of that snippet — which appends a version like any body change.
- Unrecoverable files stay in `snippet_load_errors` exactly as in Core: visible, intact on disk.
- **Every write path sees what the loader sees.** Save, delete, twin cleanup, and
  delete-project rescope operate on the loader's repair-aware, canonical-filename-wins view —
  never on a stricter parse. A write/delete decision made from a narrower view than the
  loader's can destroy data the loader would surface (rescope overwriting the canonical body
  with a stale twin) or fail to remove data the loader will resurrect (a deleted snippet
  reappearing from a repairable twin). When neither same-id twin is canonically named, the
  surviving winner is deterministic (lexicographic filename order), never directory-iteration
  order.
- The same in-memory repair applies to `projects.json`. A roster repair that *succeeds*
  surfaces as a `snippet_load_errors` entry naming the file (repair can silently drop truncated
  records, and the roster has no per-record recovered flag — the notice is the user's cue to
  inspect before the next project save rewrites the file); an unrepairable roster is a loud
  `Err`, never a silent-empty roster.

## Legacy-state migration — de-contaminate `~/.claude`

Unchanged from Core: `.ccstudio-backups` → `~/.ccdeck/backups`, `.ccstudio-config.json` →
`~/.ccdeck/config.json`, `.ccstudio-index` → `~/.ccdeck/index`; non-fatal, collision rules as
implemented; invariant: **nothing ccdeck-owned lives under `~/.claude`**.

## Deliberately out of this round (filed, not dropped)

- **M4 Organization** (browse-by-tag panel, bulk metadata) — issue #25.
- **Presets, RAG auto-assembly, sharing/export** — issue #7's deferred list.
- **Playwright e2e for the compose surface** — after this round's interactions settle.
- **`path`-driven auto-scoping of projects** — the field exists, no behavior yet.
