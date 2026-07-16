# CC Deck — Roadmap

Status: actively shipping on `main` — each phase below records what landed, its decisions, and its
verification. This doc replaces the open-ended "Search Phase 2" tail in
[`project_docs/search-design.md`](search-design.md) (that doc's own Phase 2 section is now marked
done and points here).

## Why the pivot

The app started as "Claude Code Studio" — a history browser/editor. It's repositioning as **Deck**,
a **Claude Code Control Center**: the founding mission is to make Claude Code approachable to
non-technical people, so the terminal and raw JSON config are never the wall that stops someone
from coding. Product principle throughout: **simple by default, advanced on demand.**

Naming note: "Claude Code Studio" led with Anthropic's trademark as the product's own brand, which
is the riskier pattern under US nominative-fair-use doctrine (using a mark *as your product name*
vs. using it *descriptively* to say what you're compatible with). Renamed to **Deck** — a fully
distinctive brand name — with "Claude Code Control Center" / "for Claude Code" used only as a
descriptor, plus an explicit unaffiliated disclaimer in the README. Ecosystem precedent (e.g. the
popular Claude Code GUI "Claudia" renaming to "Opcode") points the same direction.

## Phase 1 — Rebrand to Deck (DONE)

- [`tauri.conf.json`](../src-tauri/tauri.conf.json): `productName` → "Deck", window title → "Deck — Claude Code Control Center",
  bundle descriptions rewritten, updater endpoint repointed at the renamed repo. `identifier`
  (`com.zhangxingeng.ccstudio`) and the `.ccstudio-*` on-disk paths **deliberately left unchanged**
  — renaming those would strand existing installs from updates and orphan every user's search
  cache/backups/drafts. Cargo crate name (`ccstudio`/`ccstudio_lib`) also left as-is — internal
  only, zero user-facing value, kept the diff small.
- [`package.json`](../package.json) description rewritten; npm package name (`claude-code-studio`) left alone
  (cosmetic, not user-facing).
- Every other user-visible string updated: [`src/routes/+page.svelte`](../src/routes/+page.svelte) (h1, footer), [`src/app.html`](../src/app.html)
  title, [`src/app.css`](../src/app.css) header comment, [`ARCHITECTURE.md`](../ARCHITECTURE.md) title, [`.github/workflows/release.yml`](../.github/workflows/release.yml)
  release name.
- **README.md fully rewritten** around the mission (hero, mission callout, personas, three feature
  pillars, privacy, FAQ, trademark disclaimer).
- **GitHub repo renamed** `zhangxingeng/claude-code-studio` → `zhangxingeng/deck` (`gh repo rename`,
  confirmed with the founder first since it's outward-facing). GitHub keeps a redirect from the old
  path. Local `origin` remote updated to match.

## Phase 2 — Schema-driven settings editor (DONE)

The founder's real pain: Claude Code config is spread across three files and you can't tell what's
set where.

- **Backend** ([`src-tauri/src/settings.rs`](../src-tauri/src/settings.rs)): `read_claude_settings(project_cwd)` reads whichever of
  `~/.claude/settings.json` (user), `<project>/.claude/settings.json` (project), and
  `<project>/.claude/settings.local.json` (local) exist; returns each tier's path/exists/raw/parsed,
  a top-level merged `effective` view, and a `conflicts` list (key set to *differing* values in ≥2
  tiers, with the winning tier). `write_claude_settings(tier, project_cwd, value)` writes exactly
  one tier, pretty-printed, never merging. 6 unit tests (precedence, merge, conflict detection,
  user-only scope, write-creates-dir, malformed-JSON-doesn't-crash).
- **Schema**: Claude Code's published settings JSON Schema vendored at
  [`src/lib/schema/claude-code-settings.json`](../src/lib/schema/claude-code-settings.json) (~190 KB, 125 top-level properties) so the app stays
  100% offline — no runtime fetch. **To refresh**: re-download
  `https://json.schemastore.org/claude-code-settings.json` and overwrite that file; nothing else
  needs to change unless key names change.
- **Frontend** (`src/lib/components/SettingsView.svelte`): tier tabs (Local/Project/User), a
  conflict banner up top with jump-to-field, and a schema-driven form — each field rendered with
  its schema `description`, typed by JSON-Schema `type`/`enum` (checkbox / select / text / number /
  comma-separated array / JSON-textarea fallback for objects and complex arrays). A curated
  "Simple" grouping (Model, Permissions, Environment, Git, Hooks, MCP, Interface — about 20 keys)
  is shown by default; a "Show advanced settings" toggle reveals the remaining ~100 keys
  alphabetically. Entry points: a "⚙ Settings" header button (global/user scope) and a per-project
  gear next to each project's group header in [`BrowseView.svelte`](../src/lib/components/BrowseView.svelte) (resolves that project's real
  `cwd` from its sessions' `meta.cwd`).
- Validation is render-time type coercion only (no `ajv`/JSON-Schema validator) — deferred per the
  original plan as out of scope for v1.

## Phase 3 — Configurable terminal launcher (DONE)

- **[`src-tauri/src/appconfig.rs`](../src-tauri/src/appconfig.rs)**: Deck's own tiny preference file at
  `~/.claude/.ccstudio-config.json` (`{ terminal, terminalArgs }`), with `get_app_config` /
  `set_app_config` commands. Falls back to defaults (auto-detect, no extra args) on any read error.
- **`resume_in_terminal`** ([`src-tauri/src/lib.rs`](../src-tauri/src/lib.rs)) generalized to load this preference: "auto"
  (default, empty string) reproduces the exact original per-OS auto-detect behavior; a custom
  preference supplies a terminal command template (first token = program, rest = args preceding the
  `claude` invocation) plus optional extra CLI args appended after `--resume <id>` (e.g.
  `--dangerously-skip-permissions`).
- **UI**: a "Terminal" section inside `SettingsView.svelte` (global scope only — terminal choice
  isn't a per-project concept). Default radio = "Automatic (recommended)"; "Custom" reveals a
  terminal-command field; extra arguments live behind an "Advanced" disclosure with a caution note.

## Phase 4 — Search Phase 2 cleanup (DONE)

Closes out the two items [`project_docs/search-design.md`](search-design.md) left "NOT STARTED":

- **Keyboard navigation**: `SearchView.svelte` tracks a `focusedIdx` over the flattened,
  collapse-aware hit list (`visibleHits`, derived from non-collapsed groups only). ↓/↑ on the
  search input move focus and scroll the focused hit into view; Enter jumps to it. Resets on every
  new query.
- **Tool-name filter**: `SearchFilters.toolName` (Rust `tool_name: Option<String>`) restricts to
  `tool_use` blocks whose text is exactly the tool name or starts with `"{name}\n"` (matching the
  extraction format in [`search/extract.rs`](../src-tauri/src/search/extract.rs)) — implemented as an escaped `LIKE ... ESCAPE '\'`
  prefix match. **Overrides `sources`** rather than ANDing with it (an AND would always be empty,
  since `sources` rarely includes `tool_use` alongside a tool-name filter). Same override logic
  applied to the cold-path scanner (`passes_source_and_date`) so warm/cold tiers agree.
- **Current-session filter**: `SearchFilters.sessionPath` (Rust `session_path: Option<String>`)
  restricts to one session file — a `WHERE b.session_path = ?` clause in the warm-path SQL, and a
  file-skip in the cold-path directory loop ([`state.rs`](../src-tauri/src/search/state.rs)). Wired through [`search.svelte.ts`](../src/lib/search.svelte.ts)
  (`sessionOnly` toggle + `currentSessionPath`, set via `initSearch(currentSessionPath)`) and a
  "This session only" checkbox in `SearchView.svelte`, shown when a `currentSessionPath` prop is
  passed. **Gap closed in Phase 6**: [`InlineSearchPanel.svelte`](../src/lib/components/InlineSearchPanel.svelte) now calls `initSearch(sessionPath)`
  and sets `sessionOnly = true`, mounted from [`SessionEditor.svelte`](../src/lib/components/SessionEditor.svelte) (in-chat search / Ctrl+F) —
  the "search within this session" filter has a live entry point.
- Model/git-branch filtering remains explicitly out of scope (needs new indexed columns + a
  reindex, not requested).
- 4 new Rust tests (tool-name match, tool-name overrides sources, session-path restriction) on top
  of the existing 27, all passing (30 total, `search::query` module). `pnpm check` / `pnpm build`
  clean throughout.

## Phase 5 — Rename/resume compatibility fix (DONE, 2026-07-04)

Founder report: renaming a session via Deck's Browse-list rename, then trying `claude --resume` /
`/resume` on it from the real CLI, silently failed to find it under the new name. The only
workaround was resuming the session via Deck itself (path-based, doesn't need the title), then
re-running the real `/rename` slash command from inside it.

**Root cause**: `renameSession` wrote the new title into a fabricated `message.content` field on
the session's `ai-title` line — a field Deck itself invented and was the only reader of. The real
CLI's `--resume`/`/resume` picker reads a distinct `custom-title` entry
(`{"type":"custom-title","customTitle":...}`), which is exactly what the real `/rename` slash
command writes (confirmed by inspecting a session where the founder used it as a workaround — it
also writes a matching `agent-name` entry alongside it). Deck's rename never touched that field, so
the CLI never saw the new name. A second, compounding bug: Deck's Browse list only reads the first
50 lines of a session file (`SessionMeta.preview`) for display, so even a *correct* rename landing
later in a long conversation — which is exactly where the CLI's own `/rename` writes it, wherever
the conversation currently is — wouldn't have shown up in Deck's own list either.

**Fix**:
- [`src/lib/sessionOps.ts`](../src/lib/sessionOps.ts) — rename now appends real `custom-title` + `agent-name` entries
  (mirroring exactly what the CLI's own `/rename` writes) instead of mutating the fabricated field.
  Always appends, never edits an earlier line, matching how the CLI re-asserts the current title.
- [`src-tauri/src/lib.rs`](../src-tauri/src/lib.rs) — `list_sessions`'s existing one-pass whole-file scan (already used for
  `user_count`/`models`/`cwd` etc.) now also tracks the last-seen `customTitle`, exposed as a new
  `SessionMeta.custom_title` field, so a rename is found regardless of where in the file it lands,
  not just the 50-line preview.
- [`src/lib/components/BrowseView.svelte`](../src/lib/components/BrowseView.svelte) prefers `custom_title` over the preview-derived guess.
- [`src/lib/parser.ts`](../src/lib/parser.ts)'s `extractMeta` no longer reads the fabricated field.
- Two real sessions in `juror_fullstack` had the old broken rename: `central-config` (already fixed
  by the founder's manual `/rename` workaround) and `steam-stuff` — patched directly with the
  corrected write path during this session.
- Rewrote [`tests/title_smoke.mjs`](../tests/title_smoke.mjs) for the new append-based behavior (14/14 assertions).
  `cargo test --lib` 30/30. `pnpm check` / `pnpm build` clean.
- **Needs an app rebuild** (`cargo tauri build`) to ship — this touches Rust (`SessionMeta`), so a
  plain frontend redeploy isn't enough.

## Phase 6 — Chat viewer & Browse polish (DONE)

Ideas raised by the founder while using the app day-to-day (2026-07-04), all shipped same day:

1. **Back-to-top button.** [`SessionEditor.svelte`](../src/lib/components/SessionEditor.svelte) now shows a floating bottom-right button once
   scrolled past ~600px, smooth-scrolling back to the top. Placed bottom-right (not the vertically-
   centered right edge `SaveRail` uses) so the two floating controls never overlap.
2. **Sticky app header.** Root cause was `html, body { height: 100% }` in [`app.css`](../src/app.css) capping
   `.app-header`'s sticky containing block at one viewport. Fixed: only `html` gets `height: 100%`;
   `body` keeps its own `min-height: 100vh` and grows with content.
3. **Browse's search box replaced by the global Search engine.** [`BrowseView.svelte`](../src/lib/components/BrowseView.svelte) and
   `SearchView.svelte` are fully merged into one view: an always-visible advanced search bar
   (case/whole-word/regex, source/date/tool-name/project filters) sits above the project-grouped
   session list. Empty query shows the browse list unchanged; a typed query nests results
   project → chat (by resolved title) → matching lines, reusing the same [`search.svelte.ts`](../src/lib/search.svelte.ts) engine.
   `SearchView.svelte` deleted; [`+page.svelte`](../src/routes/+page.svelte)'s `'search'` view state removed.
4. **Search within a single open chat.** New [`InlineSearchPanel.svelte`](../src/lib/components/InlineSearchPanel.svelte) — "find in this chat" —
   reuses [`search.svelte.ts`](../src/lib/search.svelte.ts) with `sessionOnly` forced on, a trimmed filter set (query +
   case/whole-word/regex + tool-name), and a flat hit list. Toggled via a button above the message
   list or Ctrl/Cmd+F (intercepted so the browser's own find bar never opens). Clicking a hit or
   pressing Enter scrolls straight to that message.
5. **Chat title + inline rename inside the viewer.** New `extractCustomTitle()` ([`parser.ts`](../src/lib/parser.ts)) scans
   the full raw file for the last real `custom-title` entry — needed because `Session.meta.title` is
   derived fresh from the first user message and has no concept of a prior rename. Shown as a
   heading above `SessionMetaCard`, with a Rename button reusing [`sessionOps.ts`](../src/lib/sessionOps.ts)'s `renameSession`.
   The Rename button is disabled while the session has unsaved edits, since the rename writes
   straight to disk independent of the in-memory edit draft — renaming mid-edit could otherwise let
   a later Save silently overwrite the rename, or a post-rename reload silently discard the edits.
6. **"Resume" action on Browse-list cards.** Every session card (browse mode) and search-result
   header (search mode) now has a hover-revealed Resume button, reusing the same
   `resumeInTerminal`/clipboard-copy sequence the open viewer's header Resume button already used —
   no need to open the session first.

## Phase 7 — Second rebrand: Deck → CC Deck (apt name collision) (DONE, 2026-07-05)

Founder report: Ubuntu already has an unrelated `deck` apt package, and this app's own `.deb`
(package name derived from `productName`) collided with it — installing/updating one could remove
the other. Renamed the product-facing name from **Deck** to **CC Deck** to get a distinctive package
name (`cc-deck`-shaped on Linux) that won't collide.

- [`tauri.conf.json`](../src-tauri/tauri.conf.json): `productName` → "CC Deck", window title → "CC Deck — Claude Code Control
  Center", updater endpoint repointed at the renamed repo, bundle `longDescription` reworded.
  `identifier` (`com.zhangxingeng.ccstudio`) and the `.ccstudio-*` on-disk paths **left unchanged
  again**, same reasoning as Phase 1 — renaming those would strand existing installs' updates and
  orphan search cache/backups/drafts. Cargo crate name (`ccstudio`/`ccstudio_lib`) and npm package
  name (`claude-code-studio`) also left alone — internal only, same call as Phase 1.
- Every user-visible string updated: [`src/routes/+page.svelte`](../src/routes/+page.svelte) (h1, footer + link), [`src/app.html`](../src/app.html)
  title, [`src/app.css`](../src/app.css) header comment, [`ARCHITECTURE.md`](../ARCHITECTURE.md) title, [`README.md`](../README.md), [`CONTRIBUTING.md`](../CONTRIBUTING.md),
  issue templates, [`.github/workflows/release.yml`](../.github/workflows/release.yml) release name, [`e2e/browse.spec.ts`](../e2e/browse.spec.ts)'s heading
  assertion, plus a handful of source comments referencing the product by name.
- **GitHub repo renamed** `zhangxingeng/deck` → `zhangxingeng/ccdeck` (`gh repo rename`, confirmed
  with the founder first). GitHub keeps a redirect from the old path. Local `origin` remote updated
  to match.

## Phase 8 — get-context docs-routing infra + backend/repo cleanup campaign (DONE, 2026-07-07)

A code-review sweep (svelte-check / cargo check / cargo test / Playwright e2e all green, no TODO
markers — so findings had to come from an actual read, not tooling) surfaced four groups of real
issues, filed as **#8-11** and built the same session. Before building them, the **get-context**
docs-routing subagent was migrated into this repo and wired to actually function.

### get-context infra (flat-docs mode)

- [`project_profile.yaml`](../project_profile.yaml) added at the repo root (previously missing) — `docs_regen_cmd`,
  `check_cmd`, `github_repos`. Points at a **flat-mode** doc catalog: the corpus at `ai-first-docs/`
  (a separate git repo, gitignored from ccdeck) has no Astro site wrapper, so the catalog generator
  (`ai-first-docs/.setup/site/mcp_servers/docs_catalog.py`) and the enrichment util
  (`ai-first-docs/scripts/get_context_enrich.py`) were made **content-root-aware** (root-cause fix,
  not a ccdeck-only fork) so both flat and the kit's default nested-Astro layout work off one code
  path. [`.claude/agents/get-context.md`](../.claude/agents/get-context.md) was migrated/adapted to this project (juror_fullstack's
  paths remapped to the flat layout) — **a Claude Code session restart is required** to register a
  project-level agent; `/reload-plugins` alone does not.
- Verified end-to-end by actually dispatching `get-context` on this campaign's own build work — it
  correctly inferred the manager/worker roles, returned real doc picks, and caught a genuine
  scheduling bug in the initial plan (see below) — stronger proof than the script-level catalog
  checks alone.
- The doc's own now-outdated "blocking dispatch" guidance (claimed no lever forces a synchronous
  subagent wait, so treat `background: false`/`TaskOutput` as tested-broken workarounds) was
  corrected in `ai-first-docs/craft/docs/get_context_usage_protocol.mdx`: the real mechanism is
  simpler — dispatch, **end your turn**, and the subagent's completion fires an inbound event that
  wakes the caller in a later turn. No trick, no per-call lever needed.

### The cleanup campaign — sequencing

`get-context`'s own routing caught that the naive "4 parallel workers" plan violated the
file-disjoint parallelism rule (`orchestration/fix_campaign_manager_protocol`): #10 and #11 both
touch [`src-tauri/src/lib.rs`](../src-tauri/src/lib.rs). Regrouped into two file-disjoint streams, run sequentially (not
worktree-isolated, to respect the pre-commit-stash trap) rather than concurrently:

- **`fix(#8, 360e2b9)`** — stale `roadmap.md` note (Phase 4's "no live entry point" gap was closed by
  Phase 6 — see above), a dead `toggleSessionOnly()` export in [`search.svelte.ts`](../src/lib/search.svelte.ts), and a transitive
  `cookie@0.6.0` advisory. **Decision:** the fix moved from [`package.json`](../package.json)'s `"pnpm"` key (as
  originally suggested) to [`pnpm-workspace.yaml`](../pnpm-workspace.yaml)'s `overrides`, because pnpm 11.9 dropped support
  for the old location — caught live rather than silently targeting stale syntax.
- **`test(#9, 1b6ee27)`** — the `tests/*.mjs` smoke suite (~200 assertions, [`parser.ts`](../src/lib/parser.ts)/[`builder.ts`](../src/lib/builder.ts)/
  [`sessionOps.ts`](../src/lib/sessionOps.ts)/`diff.ts`/[`displayModel.ts`](../src/lib/displayModel.ts)) previously only ran via manual `npx tsx`, invisible to
  CI. Added a `pnpm test:smoke` script (+ pinned `tsx` devDependency), a CI step, and a
  [`CONTRIBUTING.md`](../CONTRIBUTING.md) checklist line.
- **`fix(#10, 2e35c44)`** — `list_sessions`'s hand-rolled JSONL scanner (`json_str_after`) mangled
  `\uXXXX`/`\/` escapes and wasn't scoped to the top level (a nested `"type":"..."` substring could
  misclassify a line). **Decision: replaced with `serde_json`** (already a direct dependency) parsing
  each line once into a small `#[serde(default)]` struct — fixes both bugs in one idiomatic move.
  The write-side `json_replace_str_value` was deliberately left as a surgical string edit (round-
  tripping through serde would reformat lines the real Claude Code CLI also reads). While building
  this, `get-context` routing surfaced a corpus gap — no Rust/Tauri coding protocol exists under
  `ai-first-docs/stack/` (unlike `svelte`/`sveltekit`), so this fell back to generic
  `craft/code/coding_principles` only. Filed as
  [`ai-first-docs#16`](https://github.com/zhangxingeng/ai-first-docs/issues/16) rather than dropped.
- **`fix(#11, 8f5105f)`** — two unenforced behavioral contracts: `write_claude_settings` had no
  read-modify-write guard (a concurrent external write got silently clobbered); `write_session`'s
  "caller MUST snapshot first" doc comment was never checked. **Decision: fix both.** Added an
  optimistic RMW guard (base-version = the tier's last-read raw text; a mismatch returns a
  `CONFLICT:`-prefixed error, surfaced in `SettingsView.svelte` as a dismissible reload banner) and
  `ensure_snapshotted` (a smart no-op-if-already-backed-up guard inside `write_session`).

### Backup simplification (founder request, same session, `refactor`, `ab69b17`)

The founder asked to simplify session-file backups from an unbounded version history (`vNNN-*.jsonl`
per edit, forever) to **exactly one overwritten backup slot per session**, refreshed **only** on an
explicit Save-confirm in [`SessionEditor.svelte`](../src/lib/components/SessionEditor.svelte) — not automatically from any other write path. This
**reverted part of #11**: `ensure_snapshotted`'s automatic call inside `write_session` had
unintentionally given [`sessionOps.ts`](../src/lib/sessionOps.ts)'s `renameSession` a backup it deliberately never wanted
("low-stakes", per its own comment). `ensure_snapshotted` was removed entirely; `snapshot_at` now
clears the session's backup directory before writing the single new file, so `list_backups`
naturally returns 0 or 1 entries with **no type/API change** (the frontend's existing `{#each}` loop
handles that generically). The settings RMW guard from #11 is unrelated and was left untouched.

**This also corrected issue #6** (chat-viewer trim), which had specced *removing* the backup
mechanism entirely — now updated to say keep the single-slot backup (already implemented, not part
of that future teardown), with one open call flagged for whoever picks up #6: whether
[`SessionEditor.svelte`](../src/lib/components/SessionEditor.svelte)'s restore-UI (`showHistoryModal`) survives as a user-facing affordance, or the
backup becomes a purely silent safety net.

### Standing preference (recorded in [`.claude/memory/MEMORY.md`](../.claude/memory/MEMORY.md))

Always target the latest toolchain/dependency versions — fix forward (`pnpm upgrade --latest`,
`cargo update`, `rustup update`), don't code around an older one. A broad dependency bump is still
its own reviewable change, confirmed before running unprompted mid-task.

### Verification (Phase 8)

`cargo test --lib`: 39/39 (net of removing 4 now-obsolete tests, adding 6 new ones across the
campaign). `cargo check` clean. `pnpm check`: 0 errors/warnings. `pnpm exec playwright test`: 7/7.
`pnpm test:smoke`: 200/200 assertions. Every commit's file scope was independently re-verified
(re-running the tests myself, not just trusting each build agent's own report) before being counted
done.

## Phase 9 — Chat viewer trim: lean read + plain-edit, no version control (DONE, 2026-07-06)

Closes **#6**. Built directly (founder: "we dont want complex feature... removing is the right call"),
across 3 sequential subagent dispatches (sequential, not parallel, because the phases share hot files —
[`displayModel.ts`](../src/lib/displayModel.ts), [`MessageCell.svelte`](../src/lib/components/MessageCell.svelte), [`SessionEditor.svelte`](../src/lib/components/SessionEditor.svelte) — so concurrent writers would've hit the
pre-commit-stash trap). Each phase independently re-verified by me before the next was dispatched.

- **Phase A (`5b341bd`)** — removed `thinking` and `tool_use`/`tool_result` rendering entirely: deleted
  [`ToolGroup.svelte`](../src/lib/components/ToolGroup.svelte), the corresponding [`Block.svelte`](../src/lib/components/Block.svelte) branches (including the subagent "Open →"
  affordance — intentionally dropped, not carved out, since there's no tool-call block left to click
  through from), the `DisplayToolGroup` half of [`displayModel.ts`](../src/lib/displayModel.ts), and the matching parse/build/type
  surface ([`parser.ts`](../src/lib/parser.ts)/[`builder.ts`](../src/lib/builder.ts)/[`types.ts`](../src/lib/types.ts)). Display model is now user/assistant text only.
- **Phase B (`313d52c`)** — replaced the per-row version-stack edit model ([`editDraft.ts`](../src/lib/editDraft.ts)'s
  `versions[]`/`active`, `diff.ts`, `DiffView.svelte`, the [`MessageCell.svelte`](../src/lib/components/MessageCell.svelte) diff/history toolbar,
  the crash-safe draft autosave loop and Rust `read_edit_draft`/`write_edit_draft`/`delete_edit_draft`/
  `edit_draft_path`) with plain edit-in-place → save. **The single-slot backup mechanism
  (`snapshot`/`list_backups`/`restore_backup`/`BackupVersion`) was explicitly kept, untouched** — that's
  the Phase 8 backup-simplification decision, not part of this teardown. Added a minimal one-button
  "restore backup" affordance (no version picker — there's only ever one file) resolving the open call
  issue #6 had flagged.
- **Phase C (`c6b9bed`)** — confirmed nav demotion needed no changes (`browse` was already the default
  view, viewer already reached only via `openSession`, no hero styling favored it) — then found and
  removed an entire dead subsystem Phase A's "Open →" removal had orphaned but not cleaned up: the
  subagent-transcript drilldown (`readSubagents`/`linkSubagents`/`buildSubagentSessions`/`SubagentFile`
  type/Rust `read_subagents` command, its mock fixtures, and two now-fully-dead e2e specs —
  `subagent-stack.spec.ts`, `tool-input-popover.spec.ts`). Left `subagent_count` (the BrowseView "N
  subagents" badge) untouched — separate, still-used metadata, not part of the drilldown feature.

### Standing preference this prompted (recorded in [`.claude/memory/MEMORY.md`](../.claude/memory/MEMORY.md) and
`ai-first-docs/craft/workflow/user_preferences_reference.mdx`)

Cut features people don't actually use, even ones that took real effort to build — a smaller surface
that's all genuinely used beats a larger one padded with idle machinery. This issue is the worked
example: editing a chat message was already rare, and editing tool-call/thinking content was
structurally impossible, so the fine-grained version-control/diff/draft machinery around chat edits had
no usage to justify its maintenance cost.

### Verification (Phase 9)

Independently re-run by me after each phase (not just trusting each build agent's report): `pnpm check`
0 errors across all 3 phases; `cargo check` clean; `cargo test --lib` 39/39 throughout; `pnpm test:smoke`
98/98 assertions after the final phase; `pnpm test:e2e` 4/4 (down from 7 — the 3 remaining specs are
`browse`/`session-viewer`/`inline-search`, the 2 removed were dead since Phase A). Confirmed via grep
that the dead subsystem left zero references and `subagent_count` was not over-deleted.

## Phase 10 — Fuzzy/intent search engine: tantivy replaces regex/SQLite matcher (DONE, 2026-07-07)

Closes **#5**. Full design rationale lives in [`project_docs/search-design.md`](search-design.md)'s "v2 — Fuzzy/intent
search redesign" section — this entry is the build record, not a design restatement.

- **Backend (`40b0d38`)** — replaced the `regex`-crate substring/whole-word/regex scan and the
  `blocks` SQLite table with an embedded tantivy index: BM25 ranking, a boosted exact+fuzzy union
  query per token (exact/near-exact still ranks above loosely-fuzzy), and an `ENGINE_VERSION` marker
  forcing one full rebuild for existing users' stale v1 cache so nobody upgrades into an empty index.
- **Frontend (`deccec6`)** — removed the case/whole-word/regex toggle UI entirely (no geek-mode gate)
  from [`BrowseView.svelte`](../src/lib/components/BrowseView.svelte) and [`InlineSearchPanel.svelte`](../src/lib/components/InlineSearchPanel.svelte); `SearchOpts` deleted from [`types.ts`](../src/lib/types.ts);
  `SearchHit` gained a `score` field for the engine's relevance ranking.
- **Gate-2 audit + fixes (`66c1fc0`)** — three independent audits (contract consistency, migration
  data-safety, relevance correctness) ran before calling the feature done, and found real bugs a
  green test suite hadn't caught: a **CRITICAL** (tantivy's default tokenizer silently dropped any
  token 40+ chars at index time — a git SHA or long hash, routine in this app's domain, was
  unsearchable with zero error), two **HIGH** (the cold-tier fallback required every query token
  present while the warm tier is OR-across-tokens by design — same query, a stricter result set
  purely from index staleness; unscaled `FUZZY_DISTANCE=1` flooded short/common-token queries with
  near-random noise), and several MEDIUM/LOW items (fuzzy-only hits had no highlighted snippet
  range; the "indexing N/M…" indicator never appeared during the first-launch build; `ENGINE_VERSION`
  wasn't tied to the schema actually changing shape; stale comments). All fixed with regression tests
  in the same pass.

### Verification (Phase 10)

`cargo test --lib`: 36/36 passing (33 pre-audit, +3 regression tests for the audit findings).
`pnpm check`: 0 errors/warnings. `pnpm test:smoke`: 98/98 assertions. `pnpm build`: clean production
build. Three independent audit passes (contract/migration/relevance, each reading the system fresh
without being told what changed) ran between the feature commits and the fix commit; findings
triaged as a union and fixed together, then re-verified green before commit.

### Follow-up filed, not fixed here

**Issue #12** — a narrower, self-healing cross-process race in the engine-version migration path
(only triggers if two app instances launch simultaneously right after an upgrade, and the corrupted
intermediate state self-heals on the next sweep/restart — not permanent data loss). The real fix
(single-instance enforcement via `tauri-plugin-single-instance`, or a cross-process file lock) is an
app-wide decision beyond search's own scope, so it's tracked separately rather than bundled in.
**Closed in Phase 11** (below).

## Phase 11 — Two correctness fixes: engine-migration race + chat-edit corruption (DONE, 2026-07-07)

Two unrelated bugs, fixed and committed separately.

### #12 — cross-process migration race (`5b69b7b`)

`ensure_engine_version` ([`search/db.rs`](../src-tauri/src/search/db.rs)) does the destructive wipe-and-reset described in Phase 10's
follow-up note with no protection across processes. **Decision: a cross-process file lock, not
`tauri-plugin-single-instance`** — the plugin route was considered but rejected because
`SearchState::new()` (which calls `ensure_engine_version`) runs before the Tauri builder/plugin
system even starts, so a plugin-based single-instance check couldn't preempt the race without a
larger startup-order restructure. Instead, the whole check-and-reset now runs under an exclusive
lock on a dotfile next to (not inside) the tantivy directory — surviving the `remove_dir_all` wipe.
**Used `std::fs::File::lock`, stabilized in Rust 1.89, instead of adding a crate** — `fs4` was tried
first, then dropped once the compiler pointed at std's own now-stable lock methods; net dependency
change was zero. New regression test spawns two threads racing for the lock and asserts the waiter
never acquires before the holder releases.

### #13 — chat message editing could silently corrupt session JSONL (`dcb4442`)

Founder report: editing a chat message produced a file Claude Code (the real CLI) no longer
recognized — the session read back blank. **Root-caused with tests, not inspection** — a new
adversarial round-trip suite ([`tests/edit_roundtrip_smoke.mjs`](../tests/edit_roundtrip_smoke.mjs)) fuzzing [`editDraft.ts`](../src/lib/editDraft.ts) against
deliberately hostile fixtures (duplicate uuids, integers past 2^53, unpaired UTF-16 surrogates,
numeric-looking object keys) found two independent, real corruption bugs on the first run:

1. **Duplicate-uuid row collision.** `buildDraft` keyed rows by uuid with no collision handling —
   two lines sharing a uuid silently collapsed into one row, dropping the earlier line's content on
   save. Fixed: fall back to `idx:<n>` whenever a uuid was already claimed by an earlier line in the
   same file, so two rows can never merge.
2. **Big-integer precision loss on every edit.** Every mutator round-tripped the *entire* line
   through native `JSON.parse` → mutate → `JSON.stringify`, which coerces all numbers through a
   float64 and silently rounds any integer past 2^53 — corrupting sibling numeric fields on any
   edited line, not just the field actually touched. Confirmed: a sibling field of `9223372036854775807`
   became `9223372036854776000` after an unrelated text edit. **Fixed by adopting the
   [`lossless-json`](https://www.npmjs.com/package/lossless-json) library** (new dependency) in place
   of every `JSON.parse`/`JSON.stringify` call in [`editDraft.ts`](../src/lib/editDraft.ts) — it preserves untouched numbers
   byte-for-byte through the round trip. Side effect (intentional): its parser rejects duplicate JSON
   keys within one line instead of native JSON's silent last-key-wins — stricter, not more permissive.

[`tests/edit_roundtrip_smoke.mjs`](../tests/edit_roundtrip_smoke.mjs) is now part of `pnpm test:smoke` permanently, checking two
properties against both the real fixture and the adversarial ones: **identity round trip** (build →
serialize with zero edits must reproduce the raw file byte-for-byte) and **edit fidelity** (editing
one field must leave every sibling field byte/value-identical).

**Not done:** no attempt was made to recover any session file already corrupted by the #13 bug
before this fix landed — that's a separate, deliberate recovery task if the founder needs it.

### Verification (Phase 11)

`cargo test --lib`: 37/37 (36 + 1 new lock regression test). `pnpm check`: 0 errors/warnings.
`pnpm test:smoke`: 105/105 assertions (98 + 7 new round-trip assertions). `pnpm build`: clean
production build. `cargo build --lib`: zero warnings. **Not run:** `pnpm exec playwright test` — the
sandbox this was built in hit `ENOSPC` (system file-watcher limit) starting Vite's dev server for
Playwright, unrelated to this change; founder should run the e2e suite before shipping.

## Phase 12 — Remove settings editor; App Config page (env-var launch command + update toggle) (DONE)

Closes **#18** and **#19**, built together in one pass (per #19's own note: "coordinate with #18 ...
either land together or keep a stopgap entry point" — landing together avoided ever leaving Resume
without a working launch mechanism, and avoided merge friction since both issues touch the same
files: the header entry-point button, [`appconfig.rs`](../src-tauri/src/appconfig.rs), [`lib.rs`](../src-tauri/src/lib.rs)'s `resume_in_terminal`).

- **Removed (#18)** — the schema-driven Claude Code `settings.json` editor, entirely, no
  replacement UI: `SettingsView.svelte` (501 lines), [`src-tauri/src/settings.rs`](../src-tauri/src/settings.rs) (544 lines) and its
  `mod settings`/command registrations, the vendored ~190KB [`src/lib/schema/claude-code-settings.json`](../src/lib/schema/claude-code-settings.json),
  and the matching [`api.ts`](../src/lib/api.ts)/[`types.ts`](../src/lib/types.ts) surface (`readClaudeSettings`/`writeClaudeSettings`/
  `isSettingsConflict`/the dev-mode settings shims, `SettingsTier`/`SettingsTierData`/
  `SettingsConflictValue`/`SettingsConflict`/`ClaudeSettings`). Users hand-edit `settings.json`
  themselves now — same "lean beats impressive-and-idle" call already made for the chat-viewer diff
  machinery in Phase 9.
- **Added (#19)** — [`AppConfigView.svelte`](../src/lib/components/AppConfigView.svelte), a single global-scope page (launch command / terminal /
  update toggle are app-level preferences, not per-project) replacing the removed "⚙ Settings" header
  button with "⚙ App Config". Holds: the existing terminal-launcher preference (unchanged semantics),
  a new fully-custom `launch_command` (multi-line-capable, defaults to
  `claude --resume "$CCDECK_SESSION_ID"`, with two starter presets — plain and a `tmux new-session`
  example), and a new `update_check_on_launch` toggle (defaults `true`, gating only the silent
  launch-time check in [`+layout.svelte`](../src/routes/+layout.svelte); the footer's manual "Check for updates" stays ungated).
- **Env-var launch mechanism** — `resume_in_terminal` now exports `CCDECK_SESSION_ID`,
  `CCDECK_SESSION_TITLE`, `CCDECK_CWD` into the launched command's environment instead of splicing
  a session id into a hardcoded `claude --resume <id> <extra-args>` string. `terminal_args` is
  retired (folded into the free-text `launch_command` — keeping both would be two mechanisms doing
  the same job); dropping it needed no migration since `AppConfig` has no `deny_unknown_fields`, so a
  stale on-disk `terminalArgs` key is simply ignored on next load (covered by a round-trip unit test
  deserializing a JSON blob containing the stale key, per this repo's parse/serialize testing
  convention). The three previously-duplicated per-OS command-string builders were replaced with one
  shared pure function (`appconfig::build_resume_script`, plus a Windows `.bat` counterpart) that
  every terminal-emulator candidate now invokes via `sh <script-path>` (or the OS-native equivalent)
  instead of re-deriving the command per platform — the only way a multi-line custom command works
  uniformly across `open -a`, `gnome-terminal --`, `konsole -e`, `wt`, etc. `resume_in_terminal`
  gained a `session_title` parameter, threaded through from all three call sites
  (`+page.svelte::resumeSession`, `BrowseView.svelte::doResume`, `SessionEditor.svelte::doResumeFrom`)
  reusing each component's existing display-title value.
- Per-project settings gear icon in [`BrowseView.svelte`](../src/lib/components/BrowseView.svelte) (two near-identical blocks) was deleted, not
  relocated — App Config has no per-project analog.

### Verification (Phase 12)

`cargo test --lib`: 36/36 passing (includes new `appconfig` module tests: stale-key round trip,
default-`true` update toggle, script-builder unit tests for both POSIX and Windows shapes).
`pnpm check`: 0 errors/warnings across 221 files. `pnpm test:smoke`: 105/105 assertions still green —
no smoke test touched the removed/changed surface. `pnpm build`: clean production build.
**Not performed:** live GUI verification (open App Config, toggle the update check, save, reload) —
the Chrome browser-automation extension wasn't connected in the sandbox this was built in, same gap
noted for Phase 6/7's live-verification steps. Founder should do a visual pass before shipping.

## Phase 13 — Partial reversal of #18: schema-driven settings backend restored + search-and-popover frontend (DONE, resolves #20, amends #18)

Phase 12 removed the schema-driven `settings.json` editor entirely with no replacement UI, on the
call that "schema-driven editing is useless." The founder revisited that immediately after shipping:
the real complaint was never the read/merge/conflict/write mechanism — it was the always-visible
~125-field form (`SettingsView.svelte`) that nobody wanted to navigate. This phase restores the
backend/schema/types verbatim and replaces the deleted form with a much smaller frontend: a fuzzy
search box over the schema's top-level properties plus a popover that edits exactly one field at a
time. It **resolves #20 in its expanded shape and amends #18** — the backend removal from #18 stands
corrected; the frontend removal stands as originally shipped (a *different*, smaller frontend
replaces it, not the old one).

- **Restored unchanged (verbatim from `940d66f~1`, before #18 deleted it):**
  [`src-tauri/src/settings.rs`](../src-tauri/src/settings.rs) (tier read/merge/conflict-detection/optimistic-write-guard, 12 unit
  tests), [`src/lib/schema/claude-code-settings.json`](../src/lib/schema/claude-code-settings.json) (vendored ~190KB Claude Code settings schema),
  `mod settings` + its two command registrations in [`lib.rs`](../src-tauri/src/lib.rs), and the matching [`types.ts`](../src/lib/types.ts)
  (`SettingsTier`/`SettingsTierData`/`SettingsConflictValue`/`SettingsConflict`/`ClaudeSettings`) /
  [`api.ts`](../src/lib/api.ts) (`readClaudeSettings`/`writeClaudeSettings`/`isSettingsConflict` + dev-mode mock shims)
  surface. None of this changed shape from before #18 — the read/merge/conflict/write mechanism was
  never the problem.
- **New frontend — [`SettingsSearchView.svelte`](../src/lib/components/SettingsSearchView.svelte):** header (title, scope, Close — same
  `.appconfig-head`-style pattern as [`AppConfigView.svelte`](../src/lib/components/AppConfigView.svelte)) → a single search input that fuzzy-
  filters the schema's ~125 top-level properties client-side on every keystroke (substring-on-key
  ranks highest, substring-on-description next, no match excluded; capped at 30 results — no search
  index, no tantivy, this is 125 short strings) → a candidate list (key, truncated description, a dot
  if set in any tier) → clicking a row opens a popover for that key alone. The popover shows the
  key + full schema description, a Local/Workspace/User tier radio (labelled "Workspace" in the UI;
  the underlying `SettingsTier` type/API keeps `'project'` — no backend rename), the same
  `widgetKind` schema-driven widget mapping `SettingsView` used (boolean/enum/string/number/
  comma-separated array/JSON-textarea fallback), an optional one-line "Also set in `<tier>`: `<value>`"
  hint reusing the backend's existing `conflicts` array, and Save / Clear / Cancel. Save and Clear
  both write via `writeClaudeSettings` with that tier's `raw` as `baseVersion` (the existing
  optimistic-concurrency guard); a refused write surfaces `isSettingsConflict` as a dismissible
  "reload" message, same as before. Never renders more than one field at once — the all-125-visible
  form stays deleted.
- **Two entry points, both restored/added per the founder's explicit call:** the per-project gear
  icon in [`BrowseView.svelte`](../src/lib/components/BrowseView.svelte) (`onOpenSettings` prop + both project-group gear-icon blocks, restored
  from `940d66f~1` without touching the unrelated App Config changes made since — `doResume`'s
  `title` param, etc. — that landed in the same file) plus its `.project-group__settings` CSS in
  [`app.css`](../src/app.css); and a new global "⚙ Settings" header button in [`+page.svelte`](../src/routes/+page.svelte) alongside the existing
  "⚙ App Config" one. `view` gained a fourth state (`'settings'`), and `settingsProjectCwd`/
  `settingsProjectLabel` (removed alongside `SettingsView`) are back for scoping. With no project
  context (the global entry point), the popover shows only "User" — Local/Workspace need a project
  cwd that doesn't exist in that scope.
- [`AppConfigView.svelte`](../src/lib/components/AppConfigView.svelte)'s header comment (which said the schema-driven editor "was removed... users
  hand-edit settings.json themselves now") was updated — that's no longer accurate now that
  `SettingsSearchView` exists as a separate view/entry point. The two views stay distinct: App Config
  is CC Deck's own prefs, Settings is Claude Code's `settings.json` — consistent with how #18/#19
  already drew that line.
- **Independent audit found and fixed two issues in [`SettingsSearchView.svelte`](../src/lib/components/SettingsSearchView.svelte):** (1) MEDIUM — the
  popover never read a tier's `parseError`, so editing a key in a tier whose on-disk JSON was
  currently invalid would spread `{...null}` and silently overwrite the rest of that file on Save;
  fixed by surfacing `parseError` as a warning and disabling the field/Save/Clear (via a `<fieldset
  disabled>`) for that tier. (2) LOW — the popover wasn't focused on open, so Escape only worked
  once a child input already had focus; fixed with a focus-on-open `$effect`. Both re-verified clean
  (`pnpm check` 0/0, `cargo test --lib` 47/47).

### Verification (Phase 13)

`cargo test --lib`: 47/47 passing (12 restored `settings` module tests + the 35 already covering
search/appconfig/session-scan/resume, unchanged). `pnpm check`: 0 errors, 0 warnings across 223
files. `pnpm build`: clean production build. **Not performed:** live GUI verification (search for a
key, confirm the popover, round-trip an edit across Local/Workspace/User on a real project) — the
Chrome browser-automation extension wasn't connected in this sandbox, the same gap noted for Phase
6/7/12. Founder should do a visual pass before shipping.

## Phase 14 — Editor simplification + toast/resume fixes, built by concurrent event-based teammates (DONE, resolves #16 #15 #22)

Three code-fix issues cleared in one concurrent burst — the first live test of the **iterative
teammate protocol** (now a generic doc in the ai-first-docs corpus:
`ai-first-docs/orchestration/iterative_teammate_protocol.mdx`): steerable, worktree-isolated
teammates the manager holds a multi-turn dialogue with (investigate → gate → implement + commit →
issue-update), as opposed to one-shot passive workers.

- **#16 — editor simplification.** Removed the version-history/restore-backup UI (button + modal +
  handlers) while keeping the silent single-slot `snapshot`-on-save mechanism and its server-side
  restore command as the recovery path; moved the dirty indicator + Save / Save-as-copy / Discard
  out of the floating `SaveRail` (deleted) into the top nav via a `$bindable` save-state surface;
  dropped "Edit raw JSON" (`RawJsonModal` deleted) and the user/assistant role-change `<select>`;
  added `cleanup_empty_sessions` — auto-cleans zero-turn, untitled, **stale** session files on the
  browse scan, gated by a pure `is_cleanup_eligible` guard with a 15-min recency window (and
  clock-skew-safe `saturating_sub`) so a freshly-created *live* Claude Code session is never deleted
  out from under a running CLI. The only irreversible action in the batch; unit-tested on all edges.
- **#15 — stuck toasts/popovers.** The real bugs were not the suspected error-path (that was already
  covered) but missing `onDestroy` timer cleanup on five surfaces and the update-banner's transient
  statuses (`checking`/`uptodate`/`error`) never auto-dismissing at all. Fixed via a centralized
  `setStatus()` dismiss timer in [`updater.svelte.ts`](../src/lib/updater.svelte.ts) plus `onDestroy(clearTimeout)` everywhere.
- **#22 — clipboard fallback stale since #19.** `resumeCommand()` now mirrors the backend
  `build_resume_script` (env exports + `cd` + the configured `launch_command` verbatim) instead of a
  hardcoded `claude --resume <id>`, so the paste-fallback matches what actually launches for custom
  commands; the Resume tooltip became a generic label rather than a possibly-wrong command preview.

Process notes worth keeping: all three teammates hit the harness's worktree-reclamation-on-idle gap
(an uncommitted isolation worktree is reclaimed as "unchanged" at the idle boundary) and recovered
by re-establishing a worktree and re-doing still-uncommitted work — validating the protocol's
commit-early durability rule, now sharpened in the doc. The manager owned the 3-way merge (only two
trivial import-line conflicts, since lanes were pre-declared inside the shared hub files).

### Verification (Phase 14)

`pnpm check`: 0 errors / 0 warnings across 221 files. `cargo test --lib`: 51/51 passing (the 47 from
Phase 13 + 4 new `cleanup_tests` covering stale-eligible, boundary, recent-spared, future-mtime-
spared, and any-real-content-spared). `pnpm build`: clean production build (this caught a stale
pre-merge `.svelte-kit`/`.vite` cache 404'ing on the deleted `SaveRail`/`RawJsonModal` during a dev
run — a cache artifact, not a real dangling reference; the production build resolves cleanly).
**Live GUI verification: performed by the founder** (top-nav save controls, toast auto-dismiss,
auto-clean) before the v0.10.0 release.

## Phase 15 — Harness parity + the generic migration protocol (DONE 2026-07-09, merged to main)

Two coupled tracks, run lead-gated with sonnet teammates through checkpoint dialogues:

- **Corpus track** (`ai-first-docs` branch `migration-protocol`, 8 commits): the new
  **agentic-system migration protocol** (`craft/docs/agentic_system_migration_protocol.mdx` —
  audit → gated plan → md-first execution → upstream merge → verify-with-loads-check, plus the
  destination hints for misplaced artifacts), its pasteable entry point `.setup/migrate.md`
  (existing-system sibling of `setup.md`), and `craft/docs/md_to_mdx_lift_procedure.mdx` (the
  optional post-migration Starlight lift the md-first rule promises). Cold-audited, then amended
  eight times from its own dogfood run before review.
- **Dogfood track** (this repo, branch `harness-parity`, 16 commits): ccdeck migrated to full
  harness parity BY the new protocol — three-layer always-on (root `CLAUDE.md`, `MEMORY.md` router,
  autoloaded `system_prompt_append.md`), four agents, five skills with skills-as-docs symlinks,
  three fail-open hooks (51 tests), playwright MCP (disabled-by-default), smoke tests in
  `check_cmd`, ~110 bare paths → real links, scratch retired. The resulting bindings are documented
  in [`agentic_harness.md`](agentic_harness.md).
- **Discoveries worth remembering:** repo memory injection had been silently broken
  (`autoMemoryDirectory` in tracked settings.json is ignored — fixed via gitignored
  `settings.local.json`); Claude Code **autoloads** `.claude/system_prompt_append.md` (no launcher —
  four docs corrected); the release flow's `gh release create` step was wrong (CI's tauri-action
  creates/updates the release on tag push and overwrites manual title/body — now encoded in the
  `cut-release` skill).
- **Deferred with issues filed:** flat-mode check-links `/generic_docs/` prefix resolution
  (ai-first-docs#17), upstream-merge of two generic memory rails (ai-first-docs#18),
  retire-nudge hook port (#23).

## Phase 16 — Prompt Library ships: snippets, projects, keyboard-first compose (DONE 2026-07-10, v0.12.0, closes #24)

Closes the Prompt Library epic's **Core build** (#24, epic #7) — save any prompt or fragment as a
**snippet**, group snippets into colored **projects**, and compose without leaving the keyboard:
type to search, arrow into the match panel, Enter inserts over the query line. Two living
contracts govern the feature and are meant to be kept current, not restated here:
[`project_docs/prompts-design.md`](prompts-design.md) (storage, schema, command surface, variable
grammar, match engine) and [`project_docs/prompts-ux.md`](prompts-ux.md) (every interaction, key
by key).

- **Rename mid-round: "pieces" → "snippets"** (`5b4a989`) — the founder's call, free to make
  because the feature had never shipped. Complete across Rust/TS/Svelte/CSS/docs/UI copy, but
  **stops at the storage boundary on purpose**: on-disk snippet JSON field names and the
  embeddings cache's `piece_id` column keep the old name, so existing `~/.ccdeck/prompts/` data
  and caches keep loading with no schema migration.
- **Projects are first-class**: colored (a fixed palette-key set, never a raw hex, so dark-mode
  contrast stays retunable in one theme file), pinnable as tabs, with an unpinned-but-active
  project surfacing as a temporary trailing tab so scope is never invisible. Deleting a project
  rescopes its snippets to Global rather than destroying them.
- **Variable grammar v2**: single-brace f-string style (`{name}` / `{name:default}`), implemented
  independently in Rust and TypeScript against a shared test-vector table — the seam a prior audit
  round had caught diverging.
- **Per-variable copy** replaces one global toggle: each variable defaults to hoisted (rendered as
  `<prompt_var>` and listed once in a trailing `<prompt_vars>` block) because hoisting never
  breaks while substituting an unexpectedly-long value inline can bloat the prompt — an
  asymmetric-failure-mode default, not a smarter heuristic. `AppConfig.prompts_as_variable` was
  removed (the choice is per-variable, per-session, never persisted); `AppConfig.hotkeys` was
  added in its place.
- **Keyboard-first compose**: newline-as-query, `↓` steps into the match panel only when the caret
  sits at the very end of the text (the one spot `↓` is natively inert in a textarea), `Enter`
  inserts over the query line rather than after it (the query was scaffolding for the search, not
  text worth keeping). An always-present, selection-aware `Save as…` closes the "can't save the
  whole box" gap. The snippet modal defaults to the edited span text (not the stale stored body)
  with a read-only **Original** preview and an **Update vs. Save-as-new** choice. Toasts
  auto-dismiss at 5s; a data-touching event (an auto-repair) additionally leaves a durable trace in
  a **Notices** list, so a transient surface is never the only record of something that changed a
  user's file. The config popover moved out of the library panel (where it rendered behind the
  compose box) to the tab row, portalled to escape ancestor clipping. Command hotkeys are
  Mod-normalized (Ctrl on Windows/Linux, Cmd on macOS) and rebindable; a chord must carry Ctrl/Cmd
  or it's rejected as a plain keystroke a user might type into the box.
- **Store robustness**: a JSON parse failure triggers in-memory repair — never rewrites the user's
  file; the repaired form only persists on the user's next explicit save — surfaced via toast plus
  a Notices badge rather than a silent swallow or a permanent banner.

### Verification (Phase 16)

Per the merge commit (`601f228`): svelte-check 248/0/0, 9 smoke suites, 134 cargo tests, clippy at
baseline, clean build. A cold audit fuzzed the Rust/TS grammar seam over 800k inputs (zero
divergences) and the span model over 1,049,159 operations (zero invariant violations), and found
two hotkey-dispatch defects (window-level `Mod+C`/chord dispatch reasoning about the wrong
selection), both fixed and proven in a browser before merge. Tagged and released as **v0.12.0**
(minor, not patch — a new feature surface and a new data root; nothing existing changes shape, so
no install needs a migration).

### Deferred, not dropped

- **Issue #25** — M4 organization layer (browse-by-tag panel, bulk tag/category management).
- **Compose-surface Playwright e2e** — held until this round's interactions settled; now
  actionable.
- Presets, RAG auto-assembly, sharing/export — issue #7's original deferred list.
- `path`-driven auto-scoping of projects — the field exists on the project record; no behavior
  hangs on it yet.
- Three README screenshots need recapturing (cosmetic).
- Chord validation requires Ctrl/Cmd specifically rather than any modifier — a deliberate
  narrowing, one line to loosen if it proves too strict.

*(Superseded by Phase 17 — #25 was closed as **cut**, not built; the project `path` field became the
project's identity; hotkey chords stopped being configurable at all.)*

## Phase 17 — Prompt Library subtraction: a snippet is a Markdown file (built on `prompt-simplify`, targets v0.13.0, closes #31)

The Prompt Library shipped in v0.12.0 and became the half of ccdeck the founder actually used daily
— and it was over-featured to the point where **its own designer had forgotten how to use it**: nine
UI surfaces and ~15 affordances, several unguessable without having read the UX contract. That was
the defect. Not a missing feature — an excess of them. **This round is a subtraction**, and its
measure of success is that the founder can open the app after two weeks away and still know how
everything works without reading anything.

The two living contracts were rewritten from the round's plan and the merged code, and remain the
place to look: [`project_docs/prompts-design.md`](prompts-design.md) (storage, command surface,
grammar, match engine, the compose model) and [`project_docs/prompts-ux.md`](prompts-ux.md) (every
interaction, plus a table of what was cut and why it stays cut).

- **A snippet is a Markdown file whose filename is its name.** No uuid, no JSON, no
  `title`/`body`/`keywords`/`tags`/`category`/`scope`/`placeholders`/`versions` — the file *is* the
  prompt. Once the metadata was cut the schema had exactly one structured field left, and a JSON file
  wrapping a single string field is a worse text file: it escapes a prompt's quotes and newlines into
  one unreadable line, which destroys the git diff the library exists to serve. Subfolders became the
  organization system (`rust/code_review.md`), replacing tags/categories — grouping is now `mkdir`.
- **A project is a name and a folder**, and the folder *is* the project: every `*.md` under it,
  recursively, is one of its snippets. No roster ids, no `scope` field, **no Global scope at all**
  (`active: null` means "no project configured yet"). Nothing can drift out of sync with the
  filesystem, because the filesystem is the only source of truth. Two invariants guard the user's
  repo: **app state never gets written into a project folder** (it is git-tracked — a `last_used`
  write per insert would dirty the tree on every use, so the roster/usage/active live in
  `~/.ccdeck/prompts-state.json`), and **`remove_project` forgets a path, never deletes files**.
- **The grammar is a Python format string** — `{name}` everywhere, `{{`/`}}` to escape, `{name:default}`
  removed (every variable is a string, and all share one implicit default: the sentinel *"variable not
  set, ask user for it"*, so a forgotten variable still yields a working prompt). The earlier
  markdown-aware design carved code fences and backticks out of the grammar; that was **reverted on
  purpose**, because *"variables work everywhere except inside backticks and fences"* is a rule you
  must be **told**, while *"it's a Python format string"* is one the user — and every LLM reading the
  output — already knows. It had also failed on contact: the first realistic fixture put its
  placeholder in backticks and the carve-out silently ate it. **One implementation, in TypeScript**;
  `grammar.rs` was deleted rather than simplified, which makes two-language divergence structurally
  impossible instead of test-guarded.
- **Chips replace span-tiling, and a chip is never editable in place.** An inserted snippet renders as
  its name plus its variable names, is `contenteditable=false`, and opens the popup on click — always.
  The fix was not a guard: the old model's invariant `sum(span.length) === text.length` asserted that
  what a node *renders* equals what it *contributes*, and a chip renders a label while contributing a
  whole body. That invariant is *why* inline editing was possible at all, so it had to die rather than
  be defended. The `linked-modified` provenance state went with it. `Use once` (apply an edit to this
  chip in this prompt, write nothing to the library) is what keeps the rule a simplification rather
  than a cage; the popup's Delete removes the file but leaves the words as plain text.
- **Search filters down.** An empty query used to return nothing in *both* layers, so the user had to
  type to make their own library appear. It now returns the whole library, most-recently-used first;
  typing narrows it. No "recent or relevant?" toggle, because the question answers itself: with no
  query there is no score to rank by.
- **Cut entirely** (not deprecated, no flags left behind): the embeddings UI — the engine stays and now
  downloads/indexes silently in the background, degrading to lexical with nothing for the user to see
  or retry; hotkey rebinding (~410 lines defending a capability nobody used); Notices and the
  auto-repair machinery (it existed to defend a JSON schema that no longer exists — a `.md` file cannot
  fail to parse); version history (`git` does it better, and "save under a new name" is more obvious);
  project colors and pinning; the config gear popover; the `Original` preview and the `Update` vs
  `Save as new` split (the name field decides it).
- **No migration ships.** v0.13 does not read `~/.ccdeck/prompts/<uuid>.json` — no import, no legacy
  path. A deliberate, founder-approved break, justified by the fact that he was the only user of the
  prompt half in practice: shipping a migration for a population of one and carrying its code forever
  is exactly the impressive-and-idle upkeep this round existed to remove. His own library was converted
  once by hand, as a throwaway script.

### Status and verification (Phase 17)

Built on branch `prompt-simplify` across three worktree-isolated lanes (store / compose / shell) plus
a concurrent docs lane. **Not yet merged or tagged** — the version files still read 0.12.0. The
authoritative gate is the lead running the full `check_cmd` on the merged branch and then *driving the
app*, because static green passes a merge the app cannot run. Rust-side coverage lives with the code:
`store.rs` asserts byte-exact save round-trips and that no name can escape the project folder;
`appstate.rs` asserts that using the app writes nothing into the project folder and that
`remove_project` leaves every `.md` byte-for-byte intact; `state.rs` asserts the at-rest recency order
and that an exact name hit is never buried by a semantic score.

### Deferred, not dropped

- **Splitting the Prompt Library into its own repo** — issue **#32**. The founder's judgment: *"a
  desktop app is actually fine."* Set aside deliberately, because splitting a product whose
  interaction model is still moving means maintaining two moving things.
- **Issue #25** (the organization layer) was **closed as cut** — subfolders replaced it.
- **Compose-surface Playwright e2e** — still held, now against a much smaller surface.

The **`prompt-import` skill was retired** (deleted, with its `.claude/skills/` symlink) rather than
ported: it existed to translate prose into the old JSON schema, and importing a Markdown prompt into
a folder of Markdown prompts is `cp`.

## Phase 18 — Core refocus: cut the unused surfaces; Resume becomes copyable facts + PDF export (targets v0.14.0, closes #34 #36)

CC Deck refocuses on its two core jobs — **search** and **edit/share**. Three whole features nobody
used were deleted (not deprecated — every unused surface is upkeep with no return), and the terminal
launcher was replaced with the most flexible thing possible: show the user the facts and let them act
in their own terminal.

**Cut whole (no flags left behind):**

- **Claude Code settings editor (#34, was Phases 2/12/13, #18/#20)** — `SettingsSearchView.svelte`,
  the vendored `claude-code-settings.json` schema, `src-tauri/src/settings.rs` + its two commands, the
  `settings` view state / header button / mount in `+page.svelte`, the read/write/conflict fns + types,
  and both project-group ⚙ gear entry points. Claude Code's own `settings.json` is untouched — users
  hand-edit it themselves. It only ever wrote the user's *own* files, so nothing ccdeck-owned is
  orphaned by the cut.
- **Provider profiles (#34, was Phase 11 / #21)** — `src-tauri/src/providers.rs` + its 6 commands, the
  `keyring` crate dependency, `ProviderResumeMenu.svelte`, the providers section of `AppConfigView`,
  the api.ts fns, and `ProviderProfile`/`KeyBackend` types. Stranded on-disk state (the profiles file
  + OS-keychain entries + any `~/.claude/.ccstudio-providers-plaintext.json`) is harmless post-cut —
  nothing reads it — so no migration ships; release notes should tell users they may delete the
  plaintext key file.
- **Terminal launcher (#34, was Phase 3/12 / #18)** — `resume_in_terminal` + the per-OS spawn
  branches + `write_resume_script`/`shell_quote` in `lib.rs`, and the script builders
  (`build_resume_script*`, `windows_escape`, `parse_args`, `is_auto`, `effective_launch_command`) +
  the `terminal`/`launch_command` fields in `appconfig.rs`. **`AppConfig` load/save plumbing and
  `update_check_on_launch` stay** — App Config shrinks to a one-toggle prefs page. No
  `deny_unknown_fields`, so a released install's config carrying the now-stale `terminal` /
  `launchCommand` keys still loads cleanly (round-trip test in `appconfig.rs`).

**Replacement build:**

- **Resume/fork → copyable facts.** Browse cards, the viewer header, and the per-message ⑂ fork all
  open a small popover (`ResumeMenu.svelte`, house style from the deleted provider menu) presenting
  three one-click-copy facts: the ready-to-paste `cd '<cwd>' && claude --resume '<id>'` command, the
  project path, and the session id. Left-click opens it; right-click opens the same menu. Fork keeps
  its on-disk mechanism (`fork_session`) and then shows the *new* session's facts the same way.
  `resume.ts` shrank accordingly (no launch command, no provider, no env exports).
- **PDF export (#36).** An "Export PDF" button beside Export HTML opens the webview print dialog on a
  clean read-only `SessionView` render, revealed via an `@media print` stylesheet (app.css) that hides
  the app chrome, so the OS "Save as PDF" produces a clean document. No bundled renderer. `window.print()`
  can be a no-op on some Linux webviews (wry / webkit2gtk); the HTML export remains the guaranteed
  fallback there.

### Deferred, not dropped

- **Live GUI verification of the new popover / fork-then-copy / PDF print dialog** — built and checked
  (svelte-check, cargo, smoke, production build all green) but not driven in a real window; per project
  convention the founder does the visual/print pass on their own machine, especially the OS Save-as-PDF.

## Phase 19 — Progressive browse loading: stat-only first paint + streamed enrichment (v0.14 core-refocus, issue #37)

The founder's chat history grew large enough that the app **suffocated at startup**. Ground truth:
`list_sessions` read *every* session file in full and JSON-parsed every line to compute per-session
stats, and `cleanup_empty_sessions` — which the browse view called *immediately before*
`list_sessions` — independently repeated the identical full-corpus walk + read. Two synchronous
O(total-bytes) passes blocked first paint. The product goal: **the user always sees something**.

- **`list_sessions` is now a stat-only tier** (`SessionStub`: id, path, project_raw, mtime, size) —
  one directory walk + one `stat` per file, **no content reads** — so the browse list paints
  instantly in mtime (recency) order. See [`ARCHITECTURE.md`](../ARCHITECTURE.md) for the full contract.
- **A new streaming `enrich_sessions(enrichId, Channel<SessionEnrichment>)`** delivers the
  content-derived fields (turn counts, models, timestamps, `custom_title`, preview) lazily, walking
  files **newest-first** so visible cards fill in first. Same Tauri-Channel + generation-guarded
  `spawn_blocking` pattern as `search`. The frontend paints stubs, then patches each card in place
  (coalesced one reassignment per frame). Browse is windowed at 100 cards ("Load more").
- **The duplicated walker is unified and cleanup folded in.** `cleanup_empty_sessions` is **removed**;
  its purge now rides the enrichment walk (one read per file does both), and a `cleaned` payload tells
  the frontend to drop the stub. `is_cleanup_eligible` and its **15-minute recency guard are preserved
  verbatim** (they keep a live CLI's freshly-opened session from being deleted).
- **Behavior change worth naming (shipped users).** Old cleanup was a synchronous full purge on every
  launch — guaranteed complete *because it blocked*. It is now a background full-purge per browse
  *mount*: it completes unless the app is closed mid-walk (navigating away does **not** cancel it).
  This is strictly better than the freeze it replaces, but the completeness guarantee is now "per
  session, best-effort" rather than "per launch, synchronous." Accepted deliberately for this round.

## Verification performed (historical — Phase 7 snapshot, 2026-07-05)

Counts and file names below reflect the tree as of the CC Deck rebrand (Phase 7); later phases
changed both (e.g. `SettingsView.svelte` was removed in Phase 12). Each later phase records its
own verification inline; this block is kept only as the rebrand's audit trail.

- `cargo test --lib` (src-tauri): 30/30 passing.
- `pnpm check` (svelte-check): 0 errors, 0 warnings across 230 files.
- `pnpm build`: clean production build (client + server/prerender), including the vendored schema
  JSON bundling correctly.
- Full-repo grep confirms no remaining "Claude Code Studio" / "claude-code-studio" strings except
  the intentionally-left npm package name ([`package.json`](../package.json)'s `"name"` field — cosmetic, not
  user-facing) — the same call already made for the Cargo crate name.
- **Not performed**: live GUI/browser verification of `SettingsView.svelte`, the merged Browse+Search
  UI, or any of the six Phase 6 items above — the Chrome browser-automation extension wasn't
  connected on the day any of this was built. Founder should do a visual pass before shipping, per
  the standing project convention that GUI verification happens on the founder's machine.

## Open follow-ups (not done here)

- New app icon/logo art for "CC Deck" (needs founder-supplied art).
- Guided onboarding / install-Claude-Code flow, for reaching truly non-technical users who don't
  yet have Claude Code installed.

## Future ideas (exploratory — not planned, not scoped, no timeline)

Raised by the founder while reviewing the 0.6.0 release. Recorded here so they aren't lost, but
none of these are committed work — each needs its own design pass before becoming a real phase.

- **"Ask Claude" about a setting — moot (2026-07-08).** Inline AI help inside `SettingsView` so a
  user could ask what a specific field does / whether a value is sane, instead of just reading the
  schema description. Phase 12 removed `SettingsView` and the whole schema-driven settings editor
  entirely (issue #18: users hand-edit `settings.json` themselves now), so the surface this idea
  was scoped to no longer exists. Recorded here rather than deleted so the underlying theme —
  embedding AI help into a CC Deck surface — isn't lost if it resurfaces scoped to something else.
- **Selective / smarter chat compaction — dropped (2026-07-07).** A finer-grained alternative to
  Claude Code's raw `/compact`, letting a user selectively condense parts of a long conversation.
  Explored further (a design sketch lived briefly at `project_docs/future/conversation-compactor.md`)
  but rejected: it means CC Deck writing its own compaction entries into a session file the real
  Claude Code CLI later reads and resumes — reaching into Claude Code's own session format is out of
  CC Deck's lane and not worth the complexity/risk it invites. Not being pursued.
- This theme — **embedding real AI assistance into the control center itself**, not just viewing/
  configuring Claude Code from the outside — is still open via the "Ask Claude" idea above; worth
  revisiting once that one gets a concrete design.
- **Embedding / semantic chat search — rejected (2026-07-16).** Adding vector/semantic ranking to the
  chat search engine (beyond the current tantivy BM25 + fuzzy keyword match). The founder explicitly
  ruled it out: keyword search is good enough for this corpus, and the cost/complexity of a local
  embedding model over ~1GB of chat history isn't worth it. **Do not build it.** (Separate from the
  prompt-library embedding work in issue #7, which is its own small-volume engine.) Recorded during
  the v0.14 search-narrowing round (#35) so it isn't re-proposed.

## Release history

- **v0.5.0 (2026-07-04)** — Deck pivot (Phases 1–4 above): rebrand, settings editor, terminal
  launcher, search cleanup. Founder tested locally (AppImage + `.deb`, after removing the
  superseded `claude-code-studio` package) — confirmed good.
- **v0.6.0** — Version bump after founder sign-off on the 0.5.0 Deck pivot testing pass; no
  functional changes beyond the version bump itself. README rebrand (marketing pass, non-technical
  section on top / technical below) and repo contribution setup ([`CONTRIBUTING.md`](../CONTRIBUTING.md), issue/PR
  templates) landed alongside it — see below.
- **CI/CD hardening (post-v0.6.0 tag).** The v0.6.0 release run flagged Node 20 deprecation
  warnings — `actions/checkout@v4`, `actions/setup-node@v4`, and `pnpm/action-setup@v4` were being
  silently forced onto Node 24 by GitHub. Bumped all three (`@v7`/`@v6`/`@v6`) plus
  `tauri-apps/tauri-action@v0` → `@v1` (checked the v1 changelog — none of its breaking changes
  touch the inputs this repo uses). Added [`.github/workflows/ci.yml`](../.github/workflows/ci.yml): a real CI check
  (`pnpm check` + `pnpm build` + `cargo test --lib`) on every push/PR to `main` — previously the
  only workflow was the release build itself, so a broken PR could only be caught by a human.
  Added [`.github/dependabot.yml`](../.github/dependabot.yml) covering the three dependency surfaces (pnpm/npm, Cargo, and the
  GitHub Actions versions in our own workflows), weekly, grouped by minor/patch to keep the PR
  volume sane.
- **v0.10.0 (2026-07-08)** — Phase 14: editor simplification (#16), stuck-toast/popover fixes (#15),
  and the config-aware resume-clipboard fix (#22), built concurrently by three event-based teammates
  as the first live run of the iterative teammate protocol. Founder did the visual verification pass
  before release. (Release-history entries for v0.7.0–v0.9.0 were not recorded at the time; noted
  here for the next reader rather than backfilled.)
- **v0.10.1** — Delete-only editing (#14): turn/block-level soft delete, bulk multi-select, and
  tool_use/tool_result cascade, alongside read-only thinking/tool-call rendering restored for
  browsing (removed entirely in Phase 9; revived here read-only, not editable). Not recorded in a
  phase entry at the time; noted here for the next reader rather than backfilled.
- **v0.11.0** — Provider profiles (#21): keychain-backed storage and a management UI for
  AI-provider credentials (e.g. DeepSeek), with `ANTHROPIC_*` env injection and a masked-clipboard
  fallback for Resume / Fork-and-resume. Not recorded in a phase entry at the time; noted here for
  the next reader rather than backfilled.
- **v0.12.0 (2026-07-10)** — Phase 16: the Prompt Library ships (#24, epic #7's Core build). See
  Phase 16 above for the full record.
