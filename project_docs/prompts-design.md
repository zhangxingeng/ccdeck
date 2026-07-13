# Prompt Library — Engineering Contract

The *engineering* contract: storage, the Rust↔JS command surface, the variable grammar, the match
engine, and the compose-surface model. It ages with the code, like
[search-design.md](search-design.md) does for chat search.

Its sibling is [prompts-ux.md](prompts-ux.md), the **interaction contract**: every user scenario
and what happens back. Where this doc says what the system *is*, that one says what the user
*does*. Neither is authoritative over the other's half.

**The whole model, in two sentences.** A snippet is a **Markdown file whose filename is its name**;
its entire content is the prompt. A project is a **name and a folder**, and every `*.md` under that
folder, recursively, is one of its snippets. There is no uuid, no schema, no scope field, no
cross-reference — the filesystem is the source of truth. Everything below falls out of that.

## Why the filesystem, and not a schema

The library exists to be **hand-editable and git-committable**: a user keeps their prompts in a
repo and reads the diffs. Every decision here serves that, and the ones that look austere are the
ones that serve it hardest.

Markdown rather than "nicer JSON", because once keywords, tags, category, versions and defaults are
gone the schema has exactly **one** structured field left — the body. A JSON file wrapping a single
string field is a worse text file: prompts are dense with quotes and newlines, so JSON escapes the
body into one unreadable line and a GitHub diff of a prompt edit becomes noise — which defeats the
entire reason for wanting the library in git. A `.md` file *is* the prompt: GitHub renders it,
diffs are line-by-line, any editor edits it, and "is this file schema-compliant?" stops being a
question because there is no schema.

Four invariants follow, and every writer in `src-tauri/src/prompts/` honors them:

- **The filename is the identity.** Renaming a file renames the snippet. This is what makes the
  folder hand-manageable, and it is the point — not a side effect to be papered over with an id.
- **Subfolders are the organization system.** A name is the path relative to the project root minus
  `.md` (`rust/code_review`), so grouping is `mkdir` — doable in Finder, with no UI at all. This is
  what replaced tags/categories rather than a browse panel.
- **Never write app state into a project folder.** It is git-tracked. A `last_used` write on every
  insert would dirty the user's tree every single time they used the app — and reading clean diffs
  is the whole reason the library is Markdown-in-git. Usage timestamps, the roster and the active
  project therefore live in app-local state (below), never in a `.md` file and never in a sidecar.
- **The app is a viewer onto a folder it does not own.** It creates and deletes the files the user
  asks it to, and nothing else: it never prunes a directory a deletion left empty, never rewrites a
  file it did not understand, never touches a non-snippet. `remove_project` **forgets a path and
  never deletes files** — the prompts are the user's, which is the entire reason the folder is
  theirs to choose.

### Save is byte-exact; a missing folder is loud

`store::save_snippet` writes content **verbatim** — no trailing newline added, no normalization —
because a save that "tidied" the text would show the user edits they never made in the very diff
this library exists to serve. Writes are atomic (temp sibling + rename), so a crash cannot leave a
half-written prompt.

`store::scan_snippets` **errors** on a missing or non-directory project path rather than returning
`[]`. A folder that was deleted, renamed, or sits on an unmounted drive is a real failure, and an
empty list would present it to the user as "you have no prompts" — breakage that reads as
emptiness. A single unreadable *file* (permissions, non-UTF-8) is skipped and logged instead: one
bad file must not hide every other snippet. Symlinks are not followed (a symlinked directory can
escape the folder or cycle) and dot-entries are skipped (the folder is typically a git repo; `.git`
is not content).

### Names are a trust boundary, not a formality

`save_snippet` creates parent directories for a slashed name, so `../../.ssh/authorized_keys` would
write cleanly outside the project. Names come from the frontend and are untrusted input.
`store::validate_name` rejects: empty names and empty segments, `.`/`..` segments, absolute paths,
backslashes (a name is always `/`-separated; on Windows a backslash would act as an unchecked second
separator), `:` (the usage map keys on `<project path>::<name>`, so a colon makes that key
ambiguous), and NUL (it truncates the path at the syscall boundary). `snippet_path` then re-checks
that the *resolved* path really sits under the project root — belt and braces, because if a future
edit ever weakens the name rules that check still holds the line.

## App-local state — the only non-Markdown persistence

`<data root>/prompts-state.json` (`src-tauri/src/prompts/appstate.rs`). Outside every project
folder, never in git, never in the user's prompt repo.

```jsonc
{
  "projects": [{ "name": "juror", "path": "/abs/path/.prompt_snippets" }],
  "active": "/abs/path/.prompt_snippets",
  "usage": { "/abs/path/.prompt_snippets::rust/code_review": 1720000000 }
}
```

- `usage` is keyed `<project path>::<snippet name>` → last-used epoch, and is the **only** input to
  the at-rest sort order. It lives here rather than in the snippet file for the git-cleanliness
  reason above.
- A project's **path is its identity**: it is canonicalized on add, so two spellings of one folder
  cannot become two projects with divergent usage keys, and re-adding a registered path is a
  *rename*, not a duplicate.
- The folder must already exist to be added — a project *is* a folder, so registering a path that
  isn't one would create a roster entry whose every future scan errors.
- **A corrupt state file is a loud error, never a silent reset.** Quietly returning an empty roster
  would read to the user as every project having vanished, and the next save would then persist that
  emptiness over the file that still held them. A *missing* file is a fresh install (empty state) —
  a different case, handled differently.
- The first project added becomes active, and removing the active project falls back to another,
  because a roster with no selection is a state the UI has no way to leave.

The data root itself (`~/.ccdeck`, env `CCDECK_DATA_DIR` overrides for tests) and the `~/.claude`
de-contamination invariant — **nothing ccdeck-owned lives under `~/.claude`**, which belongs to
Claude Code — are unchanged; `src-tauri/src/datadir.rs` cites this doc for the rationale. Also under
the root: `models/` (embedding artifacts) and `cache/embeddings.sqlite` (the vector cache). Both are
derived data, rebuildable at any time from the `.md` files.

## Variables — a Python format string, and nothing more

`src/lib/compose/variables.ts` is the **one and only implementation**. The Rust half (`grammar.rs`)
is deleted, not simplified: after the schema cut nothing in the backend parses a body (`content` is
an opaque string to Rust), and keeping a second implementation of a subtle rule with zero product
callers is a liability that buys nothing. Deleting it makes two-language divergence *structurally
impossible* rather than test-guarded — which is why there is no shared cross-language vector table
any more. The vectors live once, in `tests/prompts_smoke.mjs`.

The grammar, whole:

1. `{name}` is a variable, `name` matching `[A-Za-z0-9_-]+`, case-sensitive.
2. `{{` emits a literal `{`, `}}` a literal `}`. A literal `{{` is written `{{{{` — as in Python.
3. Anything else braced is literal, because Python could not read it as a plain field either:
   `{my var}`, `{a.b}`, `{:x}`, `{"json": 1}`, `{ return x }`, and `{task:write tests}` (the removed
   default form) all simply fail rule 1's name test. This is not a list of exceptions — it is rule 1
   seen from the other side.
4. One name is one variable, document-wide, first-appearance order. Two chips containing `{language}`
   share one value: the model cannot tell two identically-named variables apart, so pretending they
   differ would be a fiction the UI maintains and the output discards.
5. An unfilled variable resolves to the literal sentinel `variable not set, ask user for it`
   (`UNSET_VALUE`), in **both** copy modes. A forgotten variable therefore still produces a working
   prompt — the model asks, rather than silently receiving a blank or a stray `{placeholder}`. This
   is what replaced per-variable defaults: every variable is a string (an LLM only consumes strings,
   so a type system here was ceremony) and every variable has the same implicit default, so both the
   default-declaration syntax and the "remember to set a default" step disappear.

### There is no Markdown awareness. Do not add any.

The grammar is **uniform over the whole body**. It does not know what a code fence is, or a
backtick. A `{name}` inside ```-fenced code **is** a variable.

An earlier design excluded fenced blocks and inline code spans, to stop a code sample's braces from
false-positiving. It was cut on purpose, and the reason it was cut is the reason the 0.13 round
existed at all: *"variables work everywhere, except inside backticks, and except inside fences"* is a
rule you have to be **told**. It cannot be guessed. *"It's a Python format string"* is a rule the
user already knows — and so does every LLM reading the output. **Less to remember beats
more-correct-in-a-corner. We do not invent protocols.** The carve-out also failed on contact: the
first realistic dev-prompt fixture anyone wrote put its placeholder in backticks
(`` `{command_name}` ``), which the carve-out silently turned into literal text.

**The accepted cost, stated plainly.** A fenced code sample containing `{name}` does become a
variable — and that is **loud, not silent**: the chip renders the variable names it contains and the
fill list lists them, so a stray `name` appears in the UI and the author escapes it `{{name}}`,
exactly as they would in Python. *The UI surfacing every parsed variable is what makes this safe*,
and it is the load-bearing half of the trade.

**One case is genuinely silent, and is accepted knowingly:** `{{` inside a code sample (a Rust
`format!("{{}}")`) unescapes to `format!("{}")` on copy. That is not a bug — under Python semantics
`{{` *means* a literal brace, so unescaping it is correct, and a user who wants a literal `{{` writes
`{{{{`. Re-introducing a fence carve-out to "protect" this would trade one quiet surprise for an
unguessable rule, which is the worse trade. It is documented in `variables.ts` for the same reason
it is documented here: so the next reader does not "fix" it back.

## Copy output — the per-variable as-variable toggle

```ts
copyText(text: string, fills: Record<string, string>, asVars: Record<string, boolean>): string
```

Copy rendering is **frontend-only**; Rust never renders. As-variable is a **per-variable** choice
keyed by name; a name absent from `asVars` is **ON**. ON is the default because the failure modes
are asymmetric: hoisting a variable never breaks a prompt, while substituting unexpected data in
place can silently bloat it. When one side of a choice can only cost you elegance and the other can
cost you the prompt, the default takes the safe side and the user opts out per variable. The state is
session-only, never persisted to the snippet — with the safe default already chosen for them, a
persisted per-variable hint would earn its complexity only if turning the same variable off, session
after session, turned out to annoy in practice.

A document may mix modes freely.

- **ON** (dedup — a long value is stated once, never repeated inline): every occurrence becomes
  `<prompt_var name="x"/>`, and one `<prompt_vars>` block is appended carrying each distinct ON
  variable's value once, in first-appearance order.

  ```
  Review the PR for <prompt_var name="ticket"/> and summarize.

  <prompt_vars>
  <prompt_var name="ticket">ABC-123</prompt_var>
  </prompt_vars>
  ```

  The wrapper form `<prompt_var name="x">` is used rather than `<x>` because names may start with
  digits or hyphens, which are invalid XML element names. Block values are **XML-escaped** (`&`
  first, or you re-escape the entities you just produced): the wrapper exists to be parseable, and an
  unescaped value containing `</prompt_var>` could inject phantom variables into what the reading LLM
  sees. Names need no escaping — rule 1's name class is attribute-safe by construction.
- **OFF** (substitute in place): every occurrence becomes the value verbatim, as plain text, and is
  **never** XML-escaped — it is prose the model reads, not markup it parses. Escaping is a property
  of the block, not of the prompt.

An empty fill input reads as untouched and resolves to `UNSET_VALUE` exactly as an absent one does.
There is deliberately no way to fill a variable with the empty string: to say nothing, delete the
`{name}`.

## The compose model — why chips are atoms

`src/lib/compose/doc.ts`. A Doc is a **flat list of nodes**, each either free-typed `text` or a
`chip` (an inserted snippet). Pure data and pure transforms — no DOM, no Svelte.

**This replaced a span-tiling model, and the reason is the load-bearing wisdom of the whole round.**
Until v0.13 a Doc was one `text` string plus `spans[]` annotating ranges of it, under the invariant
`sum(span.length) === text.length` — spans *tile* the text. That invariant is precisely what had to
die. A chip **renders** as its name and its variables but **contributes** its whole body to the
copied prompt: rendered ≠ contributed. The tiling invariant asserts they are equal. So no guard
could have grown a chip on top of it — a `<textarea>` can only render the characters it contains,
which is exactly why a snippet's body used to sit in the box as editable text, and exactly why
editing it in place was possible *at all*. Inline editing was not a missing check; it was what the
model made inevitable.

That inline edit was the defect: it silently diverged the composed text from the stored snippet and
flipped the span into a third provenance state, `linked-modified`, which persisted nothing — two
places to edit one thing, with different consequences and no signal about which was which. A chip
that cannot be edited in place cannot be modified in place, so `linked-modified` is gone with the
model that produced it, along with the entire clip-and-demote algebra it needed (the applyEdit
transition table, linkRange, replaceSpan, spanStarts, diffTexts).

A chip is therefore an **atom**. The mechanism is `contenteditable="false"` on the chip element, so
the rule holds *structurally* rather than by guarding: the browser itself refuses to put a caret
inside a chip, treats it as one unit for arrow keys and selection, and deletes it whole on Backspace.
There is no inline edit to intercept because there is no inline edit.

Consequences worth knowing before you touch this file:

- **A chip carries its body** (`content`), rather than reading through to the library by name. This
  is what makes `Use once` possible at all — a chip may legitimately differ from the file of the same
  name because the user tweaked it for one prompt. It also makes a draft durable: the library
  changing, or the snippet being deleted outright, cannot reach in and gut a prompt someone is
  halfway through writing.
- **`cid` identifies the chip instance, not the snippet.** The same snippet can be inserted twice,
  and `Use once` on one copy must not touch the other. `normalize` re-issues a duplicate `cid`,
  which is how a copy/pasted chip becomes its own instance rather than a shared one — without it,
  editing one would silently rewrite the other, the exact class of bug this redesign exists to kill,
  arriving through the clipboard.
- **The DOM is read back wholesale** (`fromRawNodes`), not patched edit-by-edit. Typing, paste, cut,
  drag, undo and IME composition all arrive as "the box now contains this", so there is no
  per-inputType transition table to get wrong. The round-trip `doc → toRenderNodes → (DOM) →
  fromRawNodes → doc` must be the identity; if it is not, a prompt silently corrupts into something
  that still *looks* plausible in the box and copies out wrong.
- **A chip whose `cid` is unknown is dropped, not coerced into text.** Its body lives only in the
  model, so there is nothing faithful to put in its place — rendering its label instead would
  substitute the words "code_review" for the code-review prompt itself.
- **ZWSP is display scaffolding**, padding around chips so the browser always has somewhere to put a
  caret (a chip at the very start or end, or two adjacent chips, otherwise leave nowhere to click).
  It is stripped on the way back in, so it can never reach a copied prompt.
- `flatten(doc)` is the seam where rendered and contributed diverge: it returns typed text plus each
  chip's **body**. Everything downstream — the fill list, Copy Prompt — reads it, never the rendered
  form. A selection that includes a chip resolves the same way (`ComposeBox.selectionText`), or
  saving a selection would store the literal words "rust/code_review" instead of the prompt.

## Rust ↔ JS command contract

All async, `Result<T, String>`, snake_case, registered in `invoke_handler`. Module
`src-tauri/src/prompts/`; the TS mirror is `src/lib/api.ts` + `src/lib/prompts/types.ts`. **Those two
files and the Rust commands have one author on purpose**: `pnpm check` cannot catch a Rust↔TS
mismatch, so a drift here fails at runtime, not at build.

```
list_projects() -> { projects: Project[], active: string | null }
add_project(name, path) -> Project        // folder must exist; re-adding a path renames it
remove_project(path)                      // forgets the path. NEVER deletes files.
set_active_project(path)                  // persisted; restored on launch
list_snippets(project) -> Snippet[]       // recursive *.md scan
save_snippet(project, name, content) -> Snippet   // creates parent dirs for a slashed name
delete_snippet(project, name)             // idempotent; also drops the usage entry
match_snippets(project, query, limit) -> MatchHit[]   // empty query = everything, recency-first
touch_snippet(project, name)              // records usage in app-local state
```

`Project { name, path }`. `Snippet { name, content }`. `MatchHit { name, score }` — a name is an
identity, so that is all a hit needs to carry.

**`active: null` is not a "global" scope — it means no project is configured yet** (first launch),
and renders as the empty state that asks for a folder. Under folder-as-project there *is* no global
scope: a snippet lives in the folder it sits in. Keeping a Global tab would be the old design
wearing new labels.

**Embedding has no command surface at all** — see below.

## Match engine — lexical always, semantic silently

**Empty query returns the whole library, most-recently-used first**, then never-used ones
alphabetically (`state::at_rest_order`). The list **filters down**; it does not build up. The old
behavior bailed on an empty query in both layers, so the user was shown an empty panel and had to
type to make their own library appear — backwards, and the thing the founder hit every day. No
"recent or relevant?" toggle exists because the question answers itself: with no query there is no
score to rank by, so recency is the only meaningful order; with a query, the score is.

**Lexical** (`lexical.rs`) is always on, unconditional, instant: fzf-style weighted scoring over the
only two fields that exist — the name (`W_NAME`, which dominates) and the content (`W_CONTENT`).
The name outweighs the content because it is a deliberate, hand-chosen label, while the content is
prose that merely happens to contain the query. Deliberately not BM25/tantivy: term-frequency
statistics earn their keep on large noisy corpora (chat search); over a few hundred curated snippets,
subsequence match plus field weights is better-fitting and dependency-free. Two field-specific rules
carry real reasoning:

- The **name** gets full fuzzy treatment (substring, else in-order subsequence scored by
  compactness), so `snrev` still finds `senior-reviewer`. A subfolder path is part of the name, which
  is what makes "grouping by `mkdir`" actually searchable.
- The **content** is **substring-only**. Subsequence over long prose scatter-matches almost
  anything, which would turn the content weight into pure noise.
- Multi-token queries are **AND** (every whitespace token must land somewhere), scored as the mean
  of per-token best-field scores so longer queries aren't inflated.

**Semantic** (`embed.rs`) is fastembed-rs `bge-small-en-v1.5-onnx-Q` with `ort` `load-dynamic`,
pinned and sha256-verified artifacts, a linear cosine KNN over `cache/embeddings.sqlite`. Cache
identity is `(project, name, model_id)` — there is no uuid to key on, and a bare name is not unique
across projects.

**It has no user-facing surface.** The model downloads and indexes itself in the background on first
launch and never asks (`state::spawn_background_index`). Two conditions define it, and they are the
whole design: it never blocks startup or any user action, and it **fails silently to lexical** — no
toast, no notice, no retry nagging. That is affordable precisely *because* lexical works
unconditionally: a download that is slow, failed, or impossible on this platform degrades to
lexical-only with nothing for the user to see, decide, or retry. Semantic match improves ranking; it
is never a prerequisite for it. Failures are logged, not swallowed.

Three guards keep it from ever hurting the panel: `INFERENCE_BUDGET_MS` (a query embedding slower
than the UI's debounce budget means this machine is too slow — degrade to lexical-only for the rest
of the session), `EMBED_TOPUP_PER_QUERY` (a capped top-up so a large library cannot freeze one
keystroke; the cache warms over a few queries and the background pass does the bulk), and a
`downloading` flag so semantic match sits out the download rather than racing it.

**Fusion** (`state::fuse`) blends normalized lexical with cosine, weighted by `LEX_BLEND`. Lexical
leads because on a curated corpus the user's own words beat inferred similarity more often than not;
semantic exists to catch the phrasings the name and the content missed. `SEM_MIN_COSINE` floors
semantic-only candidates, without which low-similarity vectors pad the panel with head-scratchers.
The one hard constraint is enforced **structurally**: a hit flagged `exact` (a full-query name match)
sorts above every non-exact hit no matter what either engine scored, so an exact name match can never
be buried.

## Storage — no migration from v0.12

**v0.13 does not read `~/.ccdeck/prompts/<uuid>.json`.** It does not offer an import; there is no
legacy path, no flag, no dead code branch. This was a deliberate, founder-approved break, justified
by a fact: he was the only user of the prompt half in practice, and shipping a migration path for a
population of one — then carrying its code forever — is exactly the impressive-and-idle upkeep the
0.13 round existed to remove. The founder's own library was converted once, by hand, as a throwaway
script.

Do not reintroduce a compatibility read. If a future user needs one, it is a one-off script, not a
product surface.

## Deliberately out (filed, not dropped)

- **Splitting the Prompt Library into its own repository** — issue **#32**. Set aside because the
  interaction model is still moving, and splitting a product whose UX is still settling just means
  maintaining two moving things.
- **Issue #25** (the unbuilt tag/category organization layer) was **closed as cut**, not deferred:
  subfolders replaced it.
- **Compose-surface Playwright e2e** — held until the interactions settle.

The **`prompt-import` skill was retired**, not fixed: importing a hand-written Markdown prompt into a
folder of Markdown prompts is `cp`. The skill existed to translate prose into the JSON schema, and
the schema is gone, so nothing was left for it to do. Do not rebuild one — see *Storage* above: a
migration, if a future user ever needs one, is a throwaway script and not a product surface.
