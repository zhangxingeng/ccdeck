# Brief — Prompt Library Core: frontend (Svelte 5 / TS)

You are a **steerable teammate** on the Prompt Library Core build (issue #24), running the gated
pipeline below. You own the whole frontend lane — the new Prompts view — and the *how* within it.
The compose surface is the judgment-dense heart of this feature: you decide the implementation
technique; the contract fixes only the observable behavior. You are a smart agent: where the code
you read contradicts this brief, surface it at a gate rather than silently complying or diverging.

## Your worktree lacks the docs corpus

You run in a git worktree; the docs corpus `ai-first-docs/` is a gitignored nested repo, so it is
NOT in your tree. Read docs from the main checkout by absolute path:
`/home/shane/workspace/ccdeck/ai-first-docs/...`. Do not edit anything under that path.

## Mandatory reading, in order

1. `project_docs/prompts-design.md` (in your worktree) — **the contract.** Command surface you
   call, piece schema, the compose-surface provenance state machine, engine states. Binding.
2. Issue #24 (`gh issue view 24`) — scope, preserved constraints, acceptance. The product
   narrative (happy path, F1–F8) is in issue #7's pinned design comment — read §F1–F5 closely;
   the modal's two-modes-by-background-color signal is deliberate and load-bearing.
3. Existing patterns to match (verified pointers):
   - View switching: `src/routes/+page.svelte:35` (`view` state union), nav header `:304-368`,
     view dispatch `:371-403`, `goAppConfig()`-style setters `:156`. Prompts becomes a new union
     member + nav button + branch. Ctrl/Cmd+K focus pattern `:79-88` if you add a shortcut.
   - API seam: `src/lib/api.ts:23-27` (`isTauri()` + `call<T>`), Channel streaming `:366-385`
     (copy for embed download progress), module-level in-memory dev mocks `:32-34`.
   - Store idiom: `src/lib/search.svelte.ts` (exported `$state` object + setter fns, debounce
     `:19,64`, monotonic supersession `:75-92`) — your `src/lib/prompts.svelte.ts` follows it.
   - Modal: inline markup + shared classes `.modal-backdrop`/`.modal` (`src/app.css:290-295`;
     usage `src/lib/components/SessionEditor.svelte:789-812`). No native confirm dialogs.
   - Keyboard nav: `src/lib/components/BrowseView.svelte:348-377`.
   - Colors: CSS-variable tokens `src/app.css:25-32` (`--accent-*`), tints via
     `color-mix(in srgb, var(--accent-*) N%, transparent)`. No Tailwind in this repo.
   - Project labels: `search.svelte.ts:169-184` (`availableProjects`) / `projectLabel` in
     `src/lib/parser.ts` — reuse for the project picker; add the "Global only" option.
4. Corpus protocols (absolute paths): `/home/shane/workspace/ccdeck/ai-first-docs/stack/svelte/design_protocol.mdx`,
   `.../craft/code/typescript_coding_protocol.mdx`, `.../stack/design-tokens/color_token_protocol.mdx`,
   `.../stack/svelte/insight.mdx` (rune gotchas), and
   `.../craft/workflow/teammate_execution_protocol.mdx` — your side of the gated pipeline.

## Your lane (pre-declared — hard boundary)

`src/**`, `tests/prompts_smoke.mjs`, and the one-line `test:smoke` chain addition in
`package.json`. You do NOT touch `src-tauri/**` or `Cargo.toml`. The backend teammate builds the
real commands in parallel against the same contract; **you build entirely against your api.ts
browser-dev mocks** (in-memory piece store seeded with a few sample pieces, fake embed-status
states) — `pnpm dev` must exercise the whole view with no native shell, per CONTRIBUTING.md.
Contract friction = gate conversation, not a unilateral edit.

## What you build

1. **Prompts view** (`src/lib/components/PromptsView.svelte` + subcomponents as you see fit),
   wired as a new top-level view: side-by-side layout — collapsible library/match panel left,
   compose box primary (settled decision #2 on #7).
2. **Compose surface** honoring the contract's provenance state machine: typed / linked /
   linked-modified spans, insert-at-cursor, inline edits transition linked→linked-modified
   without touching the store, Copy Prompt flattens to exact visible plain text.
   Contenteditable vs overlay vs other — your call; report the choice + tradeoffs at Gate 1.
   Provenance tints: new `--accent-*`-pattern tokens themed light+dark.
3. **Live match panel**: debounced `match_pieces` on typing, weighted-hit display, click-to-
   insert (keyboard nav nice-to-have), scope pool = global + active project; project picker with
   "Global only".
4. **Piece modal** (M2): instance mode vs template mode, background-color mode signal, edit
   metadata, save destination (This project / Global), "use current text as template body",
   save-as-new-piece fork, save-selection-as-piece entry (opens in template mode).
5. **Placeholders** (M3): `{{token}}` mark/unmark in template mode, fill-in popover on insert,
   re-fill in instance mode, filled spans remember template + values.
6. **Embeddings UI**, simple-by-default: a small advanced affordance showing engine status, the
   Download button with the requirements note (size, CPU-only), progress from the Channel, and
   the enable toggle. Never in the common path's way.
7. **`tests/prompts_smoke.mjs`** covering the pure logic you extract (span model transitions,
   copy flattening, placeholder substitution — design for testability: keep that logic in plain
   TS modules, not component internals) + `pnpm check` green.

## The gated pipeline (STOP means: report, then wait for the lead)

1. **INVESTIGATE** → STOP. Report: component/store decomposition, your compose-surface technique
   choice with tradeoffs, mock-layer plan, any contract friction. A bad plan caught here costs
   one message.
2. **IMPLEMENT + COMMIT** → STOP. Report: diff summary, commit refs, your branch name, scoped
   check results (`pnpm check` tail + smoke run). Corrections land as NEW commits — never
   amend/rebase/force-push.
3. **UPDATE ISSUE** → comment on #24 what shipped in your lane (don't close it).

**Durability contract:** commit in your worktree BEFORE every report and before going idle, even
WIP — an uncommitted worktree can be reclaimed as "unchanged" while you wait at a gate. Commit is
how you deliver. Never merge to any shared branch; never push. If you find yourself on a trunk
checkout without a worktree, re-create your own worktree/branch before editing.

**Verify scope:** `pnpm check` and `node --import tsx tests/prompts_smoke.mjs` (match how
package.json runs the smoke suite) in your worktree. The full suite runs on the lead's
integrated tree.
