# Prompt Library — 0.13 Simplification

**Status:** contract of record for this round. Build teammates build against *this file*.
`project_docs/prompts-design.md` and `project_docs/prompts-ux.md` are amended *from* it, concurrently.

**Target:** v0.13.0, on branch `prompt-simplify` off `main`.

---

## Why this round exists

The Prompt Library shipped in v0.12.0 (issue #24) and is the half of ccdeck the founder actually
uses daily. It is also over-featured to the point where **its own designer has forgotten how to use
it** — nine distinct UI surfaces and ~15 affordances, several of which (the `Original` preview
toggle, `Update` vs `Save as new`, per-variable as-var toggles, selection-aware `Ctrl+C`, roving
project tabs) are unguessable without having read the UX contract.

That is the defect. Not a missing feature — an excess of them.

**The goal of this round is subtraction.** Every decision below is biased toward removing a moving
part. Where a capability survives, it survives because it earns its keep in daily use, not because
it was expensive to build. The measure of success is that the founder can open the app after two
weeks away and still know how everything works without reading anything.

Two consequences worth naming up front, because they justify choices that would otherwise look
reckless:

- **The Rust core is not the problem.** It is 2,975 lines across 8 files with clean seams. The
  frontend is 4,347 lines across 20 files — and the bloat there is *surface count*, not complexity
  per surface. So this round deletes surfaces, and only rewrites backend where the storage model
  itself is wrong.
- **Migration is explicitly out of scope.** See [§ Migration](#migration-there-is-none). This is what
  lets the round move fast, and it is a deliberate, founder-approved break — not an oversight.

---

## The two model changes

Everything else in this document falls out of these two.

### 1. A snippet is a Markdown file. The filename is its name.

Today: `~/.ccdeck/prompts/<uuid>.json`, an opaque filename holding a JSON object with a uuid `id`,
`title`, `body`, `keywords[]`, `tags[]`, `category`, `scope`, `placeholders[]`, `created_at`,
`updated_at`, and an append-only `versions[]` array.

Tomorrow: `<name>.md`, whose **entire content is the prompt**. Nothing else. No frontmatter, no
metadata, no id.

```
rust/code_review.md          → snippet named "rust/code_review"
planning/spec_writer.md      → snippet named "planning/spec_writer"
```

Why Markdown and not "nicer JSON": once keywords, tags, category, versions, and defaults are cut
(all confirmed cut below), the schema has exactly **one** structured field left — the body. A JSON
file wrapping a single string field is a worse text file. Prompts are dense with quotes and
newlines, so JSON escapes the body into one unreadable line, and a GitHub diff of a prompt edit
becomes noise — which defeats the entire reason for wanting the library in git. A `.md` file *is*
the prompt: GitHub renders it, diffs are line-by-line, any editor edits it, and the "is this file
schema-compliant?" check disappears because there is no schema.

**Consequences that must be honored:**

- **The filename is the identity.** There is no uuid. Renaming a file renames the snippet. This is
  what makes the folder hand-manageable, and it is the point.
- **Subfolders are organization, for free.** The name is the path relative to the project root, minus
  `.md`. This is what replaces the tags/category system being cut — the founder gets grouping by
  making a folder, which he can do in Finder or `mkdir`, with no UI at all.
- **"Save as new" collapses into "Save".** Change the name in the popup and save → new file. Keep the
  name and save → update. One button, no ambiguity. This is a direct win from filename-as-identity,
  and it is the answer to the founder's "edit here or save there?" complaint at its sharpest point.
- **Never write app state into the project folder.** The folder is git-tracked. Usage timestamps,
  last-opened project, the project roster — none of it may touch a `.md` file or add a sidecar,
  because a `last_used` write on every insert would produce a dirty git tree every time the app is
  used. All of it lives in app-local state (below).

### 2. A project is a name and a folder. Nothing else.

Today: projects are a roster in `~/.ccdeck/projects.json`, each with an id, a color key, and a
pinned flag; snippets carry a `scope: {kind: "project", project_id}` field pointing back.

Tomorrow:

```rust
struct Project { name: String, path: PathBuf }
```

The project **is** the folder. Every `*.md` under it, recursively, is one of its snippets. There is
no `scope` field, no `project_id`, no cross-referencing — a snippet belongs to the project whose
folder it sits in, and that is a fact about the filesystem, not about metadata that can drift out of
sync with it.

Removing a project forgets the path. **It must never delete files.**

---

## App-local state (the only non-Markdown persistence)

Lives outside every project folder, at `~/.ccdeck/prompts-state.json`. Never in git, never in the
user's prompt repo.

```jsonc
{
  "projects": [{ "name": "juror", "path": "/home/shane/workspace/juror_fullstack/.prompt_snippets" }],
  "active": "/home/shane/workspace/juror_fullstack/.prompt_snippets",
  "usage": { "/abs/path/.prompt_snippets::rust/code_review": 1720000000 }  // last-used epoch
}
```

`usage` is keyed by `<project path>::<snippet name>` and is the **only** input to the at-rest sort
order. Keeping it here rather than in the snippet file is what keeps the git tree clean.

---

## Variables

**Grammar simplifies to `{name}`. That is the whole grammar.**

**One implementation, in TypeScript only.** `src-tauri/src/prompts/grammar.rs` is **deleted**, not
simplified. After the cut, nothing in Rust parses variables — `placeholders[]` was its only consumer.
Keeping a second implementation of a subtle rule with zero product callers is a liability that buys
nothing, and deleting it makes the two-language divergence *structurally impossible* rather than
test-guarded. `content` is an opaque string to the backend. The chip derives its variable list in the
frontend. There is no shared cross-language vector table because there is nothing to share.

- `name` is `[A-Za-z0-9_-]+`.
- `{{` and `}}` remain the literal-brace escapes.
- **The `{name:default}` form is removed.** Every variable is a string — an LLM only consumes
  strings, so a type system here was ceremony. And every variable has the *same* implicit default:
  the literal text **`variable not set, ask user for it`**. So a prompt with a forgotten variable
  still works — the model asks instead of silently receiving a blank or a stray `{placeholder}`.
  This deletes both the default-declaration syntax and the "remember to set a default" step.

**All variables are global by name, across the whole composed prompt.** Two chips that both contain
`{language}` share one value. This is not a compromise — it is correct: the model cannot tell two
identically-named variables apart anyway, so pretending they are distinct would be a fiction the UI
maintains and the output discards.

**Two edit surfaces, both onto the same global value, propagating both ways:**

| Surface | Shows |
|---|---|
| The chip's popup | Only the variables *that chip's body* uses |
| The fill list under the compose box | *Every* variable in the whole composed prompt |

Editing in either updates the same value and the other reflects it immediately. This does **not**
reintroduce the two-places-to-edit confusion the round exists to kill — that confusion was about
snippet **bodies**, where two surfaces meant two divergent sources of truth. A variable's value is a
single global cell; showing one cell in two places is convenience, not ambiguity.

**The per-variable as-variable toggle survives** (founder call, against my recommendation to cut it).
It stays as shipped: per-variable, session-only, never persisted, default ON, controlling whether the
value is substituted in place or emitted once as an XML block at the top of the copy output.

### ⚠ Known hazard: brace collision inside code

A prompt body containing a code sample — `if (x) { return foo }`, a Rust `format!("{name}")`, a JS
object literal — will have its braces parsed as variables. **This hazard exists today** and is not
introduced by this round, but it gets more likely in Markdown, where fenced code blocks are the
norm in a dev-prompt library.

**Decision (flag if wrong): variables are not parsed inside fenced code blocks (```` ``` ````) or
inline code spans (`` ` ``).** This is natural in Markdown, needs no configuration, and eliminates
the entire false-positive class. The cost is that a variable *inside* a code example becomes
impossible — judged rare enough to accept, and `{{`/`}}` remains available as the manual escape
everywhere else.

**A code fence is fully verbatim — escapes included.** `{{` does **not** unescape inside a fence.
Otherwise a Rust `format!("{{}}")` in a code sample is silently rewritten on copy, which presents to
the user as "the app mangled my prompt" with no way to guess why. Verbatim means verbatim.

---

## The compose surface

### Chips

An inserted snippet renders in the compose box as a **chip**: a small button showing its **name** and
the **variable names it contains**. Not its body. Not a 50-character preview. Just the name.

The founder's reasoning, which is the load-bearing rationale here: *"I actually rarely read it. If I
really want to read it, it means I want to edit it. And if I want to edit it, I would click into
it."* Body text in the box is visual clutter that serves no reader.

**A chip is never inline-editable.** Clicking it opens the popup. Always. No exceptions. This is the
single rule the whole redesign is built on, and its value is that it makes the interaction
*predictable*: the user never has to ask "do I edit this in the box or click something?" — the
answer is always "click the chip."

**This deletes the `linked-modified` provenance state entirely.** A chip that cannot be edited in
place cannot be modified in place. Spans collapse to `typed` (free text) and `linked` (a chip), and
`src/lib/compose/doc.ts` shrinks accordingly.

Free-typed text in the box remains freely editable, exactly as today. The box is still a text box.

### The popup — the one and only edit surface

Opened by clicking a chip, or by the save-as-snippet action on typed text.

Fields: **name**, **content**, and the fill inputs for the variables that content uses.

Three actions, each unambiguous:

| Action | Effect |
|---|---|
| **Save** | Writes `<name>.md` to the project. Same name → updates. New name → new snippet. |
| **Use once** | Applies the edited content to *this chip in this prompt only*. Nothing is written to the library. This is the escape hatch that replaces inline editing — the founder's "temporary / customized ones". |
| **Delete** | Removes the file from the project, and **converts the chip to plain typed text**. Deleting a library file must not gut the prompt the user is halfway through writing — the link goes, the words stay. |

`Use once` is what makes the never-edit-inline rule tolerable: the user *can* tweak a prompt without
polluting the library, they just do it in the one predictable place.

---

## Search — filter down, not up

Today the empty query returns nothing, in **both** layers independently: `lexical.rs:26` bails
before scoring, and `prompts.svelte.ts:272` does not even call the backend. The user is shown an
empty list and must type to make anything appear. This is backwards.

Tomorrow:

- **Empty query → every snippet in the active project**, sorted by **most recently used** (from
  app-local `usage`), with never-used snippets after them, alphabetical.
- **Typing → the list shrinks**, ranked by the hybrid match score (lexical + semantic).

No toggle, and none is needed: with no query there is no score to sort by, so recency is the only
meaningful order; with a query, the score is. The "recent or relevant?" question answers itself
based on whether a query exists.

---

## Cuts

Everything in this table is **removed**, not deprecated. Do not leave a flag, a config key, or a
dead code path behind — a shortcut left in place reads as sanctioned to the next agent.

| Cut | Why |
|---|---|
| **Semantic-embedding UI** — `EmbeddingsSection.svelte`, the enable toggle, both progress bars, `set_embed_enabled`, `embed_status`/`embed_download` as user-facing commands | The *engine stays* — it is genuinely helpful. But it becomes automatic: download and index in the background on first launch, silently, no opt-in. Lexical match works immediately and unconditionally, so a failed or in-progress download degrades to lexical with no user-visible ceremony. |
| **Hotkey rebinding** — `ShortcutsSection.svelte`, `prompts/hotkeys.ts`, chord capture/conflict detection (~410 lines) | Fixed hotkeys. Nobody rebound them. |
| **Version history** — the `versions[]` array, `backups/` | Founder: *"if user want to keep the old one, save under a new name — that is their choice, more obvious."* And if the folder is a git repo, git already does this incomparably better. |
| **`keywords[]`, `tags[]`, `category`** | Never used, never surfaced in a browse UI. Search matches name + content. **Subfolders replace them.** This also formally kills issue **#25** (the unbuilt "organization layer") — close it rather than carry it as debt. |
| **The `Original` preview toggle** in the snippet modal | Obsolete under popup-only editing. |
| **`Update snippet` vs `Save as new`** as two buttons | Collapses into one `Save`, disambiguated by the name field. |
| **Project colors, pin/unpin, `palette.ts`** | Pure decoration on a thing that is now just a name and a path. |
| **`scope` / `project_id` / uuid `id`** | The filesystem is the source of truth. |
| **`repair.rs`, duplicate-id detection, scope normalization, the load-error/Notices system** | All of it exists to defend a JSON schema that no longer exists. A `.md` file cannot fail to parse. |
| **All legacy-store support** | See below. |

---

## Migration: there is none

**No migration code ships in the product.** v0.13 does not read `~/.ccdeck/prompts/<uuid>.json`. It
does not offer an import. It has no legacy path at all.

This is a deliberate, founder-approved break, justified by a fact: *he is the only user of the prompt
half in practice.* Shipping a migration path for a population of one — and carrying its code forever
— is exactly the "impressive and idle" upkeep this round exists to remove.

Instead: **a one-off migration of the founder's own library, run by me, as a throwaway script.**
Target: `/home/shane/workspace/juror_fullstack/.prompt_snippets` (a git-tracked folder in the juror
repo). Each `<uuid>.json` → `<slug>.md`, slug derived from `title`, body written verbatim,
`{var:default}` rewritten to `{var}`, everything else dropped.

**Do this last**, after the app is working, so the migration targets the real final format rather
than a moving one.

---

## New command surface

Down from 11 commands to 7. All prompt-related.

| Command | Notes |
|---|---|
| `list_projects() -> {projects: Project[], active: string \| null}` | The `active` path is the launch-restore reader. **`active: null` is not a "global" scope — it means no project is configured yet** (first launch), and renders as an empty state prompting for a folder. Under folder-as-project there *is* no global scope: a snippet lives in the folder it sits in. **The Global tab dies.** Keeping it would be the old design wearing new labels. |
| `add_project(name, path) -> Project` | |
| `remove_project(path)` | **Forgets the path. Never deletes files.** |
| `set_active_project(path)` | Persisted; restored on launch. |
| `list_snippets(project) -> Snippet[]` | Recursive `*.md` scan. `Snippet { name, content }`. |
| `save_snippet(project, name, content)` | Creates parent dirs for a slashed name. |
| `delete_snippet(project, name)` | |
| `match_snippets(project, query, limit) -> Hit[]` | **Empty query returns everything, recency-sorted.** |
| `touch_snippet(project, name)` | Records usage in app-local state. |

Embedding download/index become internal background work with no command surface.

---

## Lanes

Three worktree-isolated teammates under the iterative teammate protocol
(`ai-first-docs/orchestration/iterative_teammate_protocol.mdx`), gated
investigate → implement+commit → update-issue. **Lead owns integration; teammates never merge.**

| Lane | Owns | Surface |
|---|---|---|
| **A — Store** | The two model changes, backend, **and the Rust↔TS seam** | `src-tauri/src/prompts/*`, `datadir.rs`, **plus `src/lib/api.ts` and `src/lib/prompts/types.ts`**. Rewrite `store.rs` + `projects.rs` for Markdown/folder; **delete `grammar.rs`** and `repair.rs`; strip embed commands to background; new command surface in `state.rs` + `lib.rs`. *Lane A crosses into TS deliberately: `api.ts`/`types.ts` are the contract seam, and a mismatch there is invisible to both `pnpm check` and `cargo test`. One author writes both sides so the bug class stops existing. B and C consume `types.ts`; neither edits it.* |
| **B — Compose** | Chips, popup, variables | `ComposeBox.svelte`, `SnippetModal.svelte`, `VariableFillList.svelte`, `compose/doc.ts`, `compose/variables.ts`. |
| **C — Shell & cuts** | Projects, search, deletions | `PromptsView.svelte`, `ProjectTabs.svelte`, `ProjectManagerPopover.svelte`, `MatchPanel.svelte`, `ConfigPopover.svelte`. Deletes `EmbeddingsSection`, `ShortcutsSection`, `hotkeys.ts`, `NoticesSection`, `notices.ts`, `palette.ts`. |
| **D — Docs** | The two contracts | `project_docs/prompts-design.md`, `project_docs/prompts-ux.md`, amended from this file. Runs concurrently. |

**Pre-declared lanes inside the shared hub file.** `src/lib/prompts.svelte.ts` (473 lines) is touched
by both B and C. Split it explicitly:

- **B owns**: compose ops, `doc` state, `fills`, `asVars`, `saveSnippet`/`deleteSnippet` callers, `copyOutput`.
- **C owns**: `projects` state, active-project persistence, `runMatch`/debounce, `hits`, embed lifecycle removal, hotkey-constant inlining.

A teammate that finds itself needing to edit the other's region **surfaces it to the lead rather than
guessing**.

**Worktree base quirk — every brief must carry this.** Agent-tool worktrees fork from `main`, not
from the checked-out feature branch. This has bitten the project twice. Each teammate verifies its
base commit first and resets with `git checkout -B <lane> prompt-simplify` if it is wrong.

**There is no whole-project `pnpm check` gate per lane — it is unsatisfiable, and asking for it was a
planning error.** The moment `Snippet` becomes `{name, content}`, each frontend lane's worktree fails
to compile on the *other* lane's untouched code. Neither B nor C can pass a whole-project check
alone, and demanding it would only teach them to paper over the other's region.

**Per-lane signal** (fast, local, not the gate):

```
cargo test --lib --manifest-path src-tauri/Cargo.toml && pnpm run test:smoke
```
plus `pnpm check` clean **in the files that lane owns**, plus that lane's own new tests. Residual
`pnpm check` errors confined to another lane's region are expected, must be **reported explicitly**,
and are the lead's to reconcile.

**The authoritative gate** is the lead running the full `check_cmd` on the *merged* branch — only the
integrated tree proves the lanes compose — and then **driving the actual app**, because static green
passes a merge the app cannot run.

**Merge order:** A first. It owns the seam (`api.ts`, `types.ts`), so it is the foundation B and C
rebase onto.

---

## Deliberately deferred

- **Splitting the Prompt Library into its own repository** — filed as its own issue, not built. The
  founder's judgment: *"a desktop app is actually fine"* — the VS Code extension idea was raised and
  set aside. Do not act on it this round; the UX must settle first, because splitting a product whose
  interaction model is still moving just means maintaining two moving things.
- **The `prompt-import` skill** (`project_docs/skills/prompt-import/`) emits the old JSON schema and
  will be wrong the moment lane A lands. Out of scope for the build lanes; fix it after the format
  settles, or retire it — a Markdown library barely needs an importer.
