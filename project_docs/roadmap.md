# CC Deck — Roadmap

Status: **Phases 1–7 shipped through v0.7.2**, committed on `main`. This doc replaces the
open-ended "Search Phase 2" tail in `project_docs/search-design.md` (that doc's own Phase 2 section is now
marked done and points here).

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

- `tauri.conf.json`: `productName` → "Deck", window title → "Deck — Claude Code Control Center",
  bundle descriptions rewritten, updater endpoint repointed at the renamed repo. `identifier`
  (`com.zhangxingeng.ccstudio`) and the `.ccstudio-*` on-disk paths **deliberately left unchanged**
  — renaming those would strand existing installs from updates and orphan every user's search
  cache/backups/drafts. Cargo crate name (`ccstudio`/`ccstudio_lib`) also left as-is — internal
  only, zero user-facing value, kept the diff small.
- `package.json` description rewritten; npm package name (`claude-code-studio`) left alone
  (cosmetic, not user-facing).
- Every other user-visible string updated: `src/routes/+page.svelte` (h1, footer), `src/app.html`
  title, `src/app.css` header comment, `ARCHITECTURE.md` title, `.github/workflows/release.yml`
  release name.
- **README.md fully rewritten** around the mission (hero, mission callout, personas, three feature
  pillars, privacy, FAQ, trademark disclaimer).
- **GitHub repo renamed** `zhangxingeng/claude-code-studio` → `zhangxingeng/deck` (`gh repo rename`,
  confirmed with the founder first since it's outward-facing). GitHub keeps a redirect from the old
  path. Local `origin` remote updated to match.

## Phase 2 — Schema-driven settings editor (DONE)

The founder's real pain: Claude Code config is spread across three files and you can't tell what's
set where.

- **Backend** (`src-tauri/src/settings.rs`): `read_claude_settings(project_cwd)` reads whichever of
  `~/.claude/settings.json` (user), `<project>/.claude/settings.json` (project), and
  `<project>/.claude/settings.local.json` (local) exist; returns each tier's path/exists/raw/parsed,
  a top-level merged `effective` view, and a `conflicts` list (key set to *differing* values in ≥2
  tiers, with the winning tier). `write_claude_settings(tier, project_cwd, value)` writes exactly
  one tier, pretty-printed, never merging. 6 unit tests (precedence, merge, conflict detection,
  user-only scope, write-creates-dir, malformed-JSON-doesn't-crash).
- **Schema**: Claude Code's published settings JSON Schema vendored at
  `src/lib/schema/claude-code-settings.json` (~190 KB, 125 top-level properties) so the app stays
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
  gear next to each project's group header in `BrowseView.svelte` (resolves that project's real
  `cwd` from its sessions' `meta.cwd`).
- Validation is render-time type coercion only (no `ajv`/JSON-Schema validator) — deferred per the
  original plan as out of scope for v1.

## Phase 3 — Configurable terminal launcher (DONE)

- **`src-tauri/src/appconfig.rs`**: Deck's own tiny preference file at
  `~/.claude/.ccstudio-config.json` (`{ terminal, terminalArgs }`), with `get_app_config` /
  `set_app_config` commands. Falls back to defaults (auto-detect, no extra args) on any read error.
- **`resume_in_terminal`** (`src-tauri/src/lib.rs`) generalized to load this preference: "auto"
  (default, empty string) reproduces the exact original per-OS auto-detect behavior; a custom
  preference supplies a terminal command template (first token = program, rest = args preceding the
  `claude` invocation) plus optional extra CLI args appended after `--resume <id>` (e.g.
  `--dangerously-skip-permissions`).
- **UI**: a "Terminal" section inside `SettingsView.svelte` (global scope only — terminal choice
  isn't a per-project concept). Default radio = "Automatic (recommended)"; "Custom" reveals a
  terminal-command field; extra arguments live behind an "Advanced" disclosure with a caution note.

## Phase 4 — Search Phase 2 cleanup (DONE)

Closes out the two items `project_docs/search-design.md` left "NOT STARTED":

- **Keyboard navigation**: `SearchView.svelte` tracks a `focusedIdx` over the flattened,
  collapse-aware hit list (`visibleHits`, derived from non-collapsed groups only). ↓/↑ on the
  search input move focus and scroll the focused hit into view; Enter jumps to it. Resets on every
  new query.
- **Tool-name filter**: `SearchFilters.toolName` (Rust `tool_name: Option<String>`) restricts to
  `tool_use` blocks whose text is exactly the tool name or starts with `"{name}\n"` (matching the
  extraction format in `search/extract.rs`) — implemented as an escaped `LIKE ... ESCAPE '\'`
  prefix match. **Overrides `sources`** rather than ANDing with it (an AND would always be empty,
  since `sources` rarely includes `tool_use` alongside a tool-name filter). Same override logic
  applied to the cold-path scanner (`passes_source_and_date`) so warm/cold tiers agree.
- **Current-session filter**: `SearchFilters.sessionPath` (Rust `session_path: Option<String>`)
  restricts to one session file — a `WHERE b.session_path = ?` clause in the warm-path SQL, and a
  file-skip in the cold-path directory loop (`state.rs`). Wired through `search.svelte.ts`
  (`sessionOnly` toggle + `currentSessionPath`, set via `initSearch(currentSessionPath)`) and a
  "This session only" checkbox in `SearchView.svelte`, shown when a `currentSessionPath` prop is
  passed. **Gap closed in Phase 6**: `InlineSearchPanel.svelte` now calls `initSearch(sessionPath)`
  and sets `sessionOnly = true`, mounted from `SessionEditor.svelte` (in-chat search / Ctrl+F) —
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
- `src/lib/sessionOps.ts` — rename now appends real `custom-title` + `agent-name` entries
  (mirroring exactly what the CLI's own `/rename` writes) instead of mutating the fabricated field.
  Always appends, never edits an earlier line, matching how the CLI re-asserts the current title.
- `src-tauri/src/lib.rs` — `list_sessions`'s existing one-pass whole-file scan (already used for
  `user_count`/`models`/`cwd` etc.) now also tracks the last-seen `customTitle`, exposed as a new
  `SessionMeta.custom_title` field, so a rename is found regardless of where in the file it lands,
  not just the 50-line preview.
- `src/lib/components/BrowseView.svelte` prefers `custom_title` over the preview-derived guess.
- `src/lib/parser.ts`'s `extractMeta` no longer reads the fabricated field.
- Two real sessions in `juror_fullstack` had the old broken rename: `central-config` (already fixed
  by the founder's manual `/rename` workaround) and `steam-stuff` — patched directly with the
  corrected write path during this session.
- Rewrote `tests/title_smoke.mjs` for the new append-based behavior (14/14 assertions).
  `cargo test --lib` 30/30. `pnpm check` / `pnpm build` clean.
- **Needs an app rebuild** (`cargo tauri build`) to ship — this touches Rust (`SessionMeta`), so a
  plain frontend redeploy isn't enough.

## Phase 6 — Chat viewer & Browse polish (DONE)

Ideas raised by the founder while using the app day-to-day (2026-07-04), all shipped same day:

1. **Back-to-top button.** `SessionEditor.svelte` now shows a floating bottom-right button once
   scrolled past ~600px, smooth-scrolling back to the top. Placed bottom-right (not the vertically-
   centered right edge `SaveRail` uses) so the two floating controls never overlap.
2. **Sticky app header.** Root cause was `html, body { height: 100% }` in `app.css` capping
   `.app-header`'s sticky containing block at one viewport. Fixed: only `html` gets `height: 100%`;
   `body` keeps its own `min-height: 100vh` and grows with content.
3. **Browse's search box replaced by the global Search engine.** `BrowseView.svelte` and
   `SearchView.svelte` are fully merged into one view: an always-visible advanced search bar
   (case/whole-word/regex, source/date/tool-name/project filters) sits above the project-grouped
   session list. Empty query shows the browse list unchanged; a typed query nests results
   project → chat (by resolved title) → matching lines, reusing the same `search.svelte.ts` engine.
   `SearchView.svelte` deleted; `+page.svelte`'s `'search'` view state removed.
4. **Search within a single open chat.** New `InlineSearchPanel.svelte` — "find in this chat" —
   reuses `search.svelte.ts` with `sessionOnly` forced on, a trimmed filter set (query +
   case/whole-word/regex + tool-name), and a flat hit list. Toggled via a button above the message
   list or Ctrl/Cmd+F (intercepted so the browser's own find bar never opens). Clicking a hit or
   pressing Enter scrolls straight to that message.
5. **Chat title + inline rename inside the viewer.** New `extractCustomTitle()` (`parser.ts`) scans
   the full raw file for the last real `custom-title` entry — needed because `Session.meta.title` is
   derived fresh from the first user message and has no concept of a prior rename. Shown as a
   heading above `SessionMetaCard`, with a Rename button reusing `sessionOps.ts`'s `renameSession`.
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

- `tauri.conf.json`: `productName` → "CC Deck", window title → "CC Deck — Claude Code Control
  Center", updater endpoint repointed at the renamed repo, bundle `longDescription` reworded.
  `identifier` (`com.zhangxingeng.ccstudio`) and the `.ccstudio-*` on-disk paths **left unchanged
  again**, same reasoning as Phase 1 — renaming those would strand existing installs' updates and
  orphan search cache/backups/drafts. Cargo crate name (`ccstudio`/`ccstudio_lib`) and npm package
  name (`claude-code-studio`) also left alone — internal only, same call as Phase 1.
- Every user-visible string updated: `src/routes/+page.svelte` (h1, footer + link), `src/app.html`
  title, `src/app.css` header comment, `ARCHITECTURE.md` title, `README.md`, `CONTRIBUTING.md`,
  issue templates, `.github/workflows/release.yml` release name, `e2e/browse.spec.ts`'s heading
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

- `project_profile.yaml` added at the repo root (previously missing) — `docs_regen_cmd`,
  `check_cmd`, `github_repos`. Points at a **flat-mode** doc catalog: the corpus at `ai-first-docs/`
  (a separate git repo, gitignored from ccdeck) has no Astro site wrapper, so the catalog generator
  (`ai-first-docs/.setup/site/mcp_servers/docs_catalog.py`) and the enrichment util
  (`ai-first-docs/scripts/get_context_enrich.py`) were made **content-root-aware** (root-cause fix,
  not a ccdeck-only fork) so both flat and the kit's default nested-Astro layout work off one code
  path. `.claude/agents/get-context.md` was migrated/adapted to this project (juror_fullstack's
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
touch `src-tauri/src/lib.rs`. Regrouped into two file-disjoint streams, run sequentially (not
worktree-isolated, to respect the pre-commit-stash trap) rather than concurrently:

- **`fix(#8, 360e2b9)`** — stale `roadmap.md` note (Phase 4's "no live entry point" gap was closed by
  Phase 6 — see above), a dead `toggleSessionOnly()` export in `search.svelte.ts`, and a transitive
  `cookie@0.6.0` advisory. **Decision:** the fix moved from `package.json`'s `"pnpm"` key (as
  originally suggested) to `pnpm-workspace.yaml`'s `overrides`, because pnpm 11.9 dropped support
  for the old location — caught live rather than silently targeting stale syntax.
- **`test(#9, 1b6ee27)`** — the `tests/*.mjs` smoke suite (~200 assertions, `parser.ts`/`builder.ts`/
  `sessionOps.ts`/`diff.ts`/`displayModel.ts`) previously only ran via manual `npx tsx`, invisible to
  CI. Added a `pnpm test:smoke` script (+ pinned `tsx` devDependency), a CI step, and a
  `CONTRIBUTING.md` checklist line.
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
explicit Save-confirm in `SessionEditor.svelte` — not automatically from any other write path. This
**reverted part of #11**: `ensure_snapshotted`'s automatic call inside `write_session` had
unintentionally given `sessionOps.ts`'s `renameSession` a backup it deliberately never wanted
("low-stakes", per its own comment). `ensure_snapshotted` was removed entirely; `snapshot_at` now
clears the session's backup directory before writing the single new file, so `list_backups`
naturally returns 0 or 1 entries with **no type/API change** (the frontend's existing `{#each}` loop
handles that generically). The settings RMW guard from #11 is unrelated and was left untouched.

**This also corrected issue #6** (chat-viewer trim), which had specced *removing* the backup
mechanism entirely — now updated to say keep the single-slot backup (already implemented, not part
of that future teardown), with one open call flagged for whoever picks up #6: whether
`SessionEditor.svelte`'s restore-UI (`showHistoryModal`) survives as a user-facing affordance, or the
backup becomes a purely silent safety net.

### Standing preference (recorded in `.claude/memory/MEMORY.md`)

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
`displayModel.ts`, `MessageCell.svelte`, `SessionEditor.svelte` — so concurrent writers would've hit the
pre-commit-stash trap). Each phase independently re-verified by me before the next was dispatched.

- **Phase A (`5b341bd`)** — removed `thinking` and `tool_use`/`tool_result` rendering entirely: deleted
  `ToolGroup.svelte`, the corresponding `Block.svelte` branches (including the subagent "Open →"
  affordance — intentionally dropped, not carved out, since there's no tool-call block left to click
  through from), the `DisplayToolGroup` half of `displayModel.ts`, and the matching parse/build/type
  surface (`parser.ts`/`builder.ts`/`types.ts`). Display model is now user/assistant text only.
- **Phase B (`313d52c`)** — replaced the per-row version-stack edit model (`editDraft.ts`'s
  `versions[]`/`active`, `diff.ts`, `DiffView.svelte`, the `MessageCell.svelte` diff/history toolbar,
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

### Standing preference this prompted (recorded in `.claude/memory/MEMORY.md` and
`ai-first-docs/craft/team/user_preferences_reference.mdx`)

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

Closes **#5**. Full design rationale lives in `project_docs/search-design.md`'s "v2 — Fuzzy/intent
search redesign" section — this entry is the build record, not a design restatement.

- **Backend (`40b0d38`)** — replaced the `regex`-crate substring/whole-word/regex scan and the
  `blocks` SQLite table with an embedded tantivy index: BM25 ranking, a boosted exact+fuzzy union
  query per token (exact/near-exact still ranks above loosely-fuzzy), and an `ENGINE_VERSION` marker
  forcing one full rebuild for existing users' stale v1 cache so nobody upgrades into an empty index.
- **Frontend (`deccec6`)** — removed the case/whole-word/regex toggle UI entirely (no geek-mode gate)
  from `BrowseView.svelte` and `InlineSearchPanel.svelte`; `SearchOpts` deleted from `types.ts`;
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

`ensure_engine_version` (`search/db.rs`) does the destructive wipe-and-reset described in Phase 10's
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
adversarial round-trip suite (`tests/edit_roundtrip_smoke.mjs`) fuzzing `editDraft.ts` against
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
   of every `JSON.parse`/`JSON.stringify` call in `editDraft.ts` — it preserves untouched numbers
   byte-for-byte through the round trip. Side effect (intentional): its parser rejects duplicate JSON
   keys within one line instead of native JSON's silent last-key-wins — stricter, not more permissive.

`tests/edit_roundtrip_smoke.mjs` is now part of `pnpm test:smoke` permanently, checking two
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
files: the header entry-point button, `appconfig.rs`, `lib.rs`'s `resume_in_terminal`).

- **Removed (#18)** — the schema-driven Claude Code `settings.json` editor, entirely, no
  replacement UI: `SettingsView.svelte` (501 lines), `src-tauri/src/settings.rs` (544 lines) and its
  `mod settings`/command registrations, the vendored ~190KB `src/lib/schema/claude-code-settings.json`,
  and the matching `api.ts`/`types.ts` surface (`readClaudeSettings`/`writeClaudeSettings`/
  `isSettingsConflict`/the dev-mode settings shims, `SettingsTier`/`SettingsTierData`/
  `SettingsConflictValue`/`SettingsConflict`/`ClaudeSettings`). Users hand-edit `settings.json`
  themselves now — same "lean beats impressive-and-idle" call already made for the chat-viewer diff
  machinery in Phase 9.
- **Added (#19)** — `AppConfigView.svelte`, a single global-scope page (launch command / terminal /
  update toggle are app-level preferences, not per-project) replacing the removed "⚙ Settings" header
  button with "⚙ App Config". Holds: the existing terminal-launcher preference (unchanged semantics),
  a new fully-custom `launch_command` (multi-line-capable, defaults to
  `claude --resume "$CCDECK_SESSION_ID"`, with two starter presets — plain and a `tmux new-session`
  example), and a new `update_check_on_launch` toggle (defaults `true`, gating only the silent
  launch-time check in `+layout.svelte`; the footer's manual "Check for updates" stays ungated).
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
- Per-project settings gear icon in `BrowseView.svelte` (two near-identical blocks) was deleted, not
  relocated — App Config has no per-project analog.

### Verification (Phase 12)

`cargo test --lib`: 36/36 passing (includes new `appconfig` module tests: stale-key round trip,
default-`true` update toggle, script-builder unit tests for both POSIX and Windows shapes).
`pnpm check`: 0 errors/warnings across 221 files. `pnpm test:smoke`: 105/105 assertions still green —
no smoke test touched the removed/changed surface. `pnpm build`: clean production build.
**Not performed:** live GUI verification (open App Config, toggle the update check, save, reload) —
the Chrome browser-automation extension wasn't connected in the sandbox this was built in, same gap
noted for Phase 6/7's live-verification steps. Founder should do a visual pass before shipping.

## Verification performed

- `cargo test --lib` (src-tauri): 30/30 passing.
- `pnpm check` (svelte-check): 0 errors, 0 warnings across 230 files.
- `pnpm build`: clean production build (client + server/prerender), including the vendored schema
  JSON bundling correctly.
- Full-repo grep confirms no remaining "Claude Code Studio" / "claude-code-studio" strings except
  the intentionally-left npm package name (`package.json`'s `"name"` field — cosmetic, not
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

## Release history

- **v0.5.0 (2026-07-04)** — Deck pivot (Phases 1–4 above): rebrand, settings editor, terminal
  launcher, search cleanup. Founder tested locally (AppImage + `.deb`, after removing the
  superseded `claude-code-studio` package) — confirmed good.
- **v0.6.0** — Version bump after founder sign-off on the 0.5.0 Deck pivot testing pass; no
  functional changes beyond the version bump itself. README rebrand (marketing pass, non-technical
  section on top / technical below) and repo contribution setup (`CONTRIBUTING.md`, issue/PR
  templates) landed alongside it — see below.
- **CI/CD hardening (post-v0.6.0 tag).** The v0.6.0 release run flagged Node 20 deprecation
  warnings — `actions/checkout@v4`, `actions/setup-node@v4`, and `pnpm/action-setup@v4` were being
  silently forced onto Node 24 by GitHub. Bumped all three (`@v7`/`@v6`/`@v6`) plus
  `tauri-apps/tauri-action@v0` → `@v1` (checked the v1 changelog — none of its breaking changes
  touch the inputs this repo uses). Added `.github/workflows/ci.yml`: a real CI check
  (`pnpm check` + `pnpm build` + `cargo test --lib`) on every push/PR to `main` — previously the
  only workflow was the release build itself, so a broken PR could only be caught by a human.
  Added `.github/dependabot.yml` covering the three dependency surfaces (pnpm/npm, Cargo, and the
  GitHub Actions versions in our own workflows), weekly, grouped by minor/patch to keep the PR
  volume sane.
