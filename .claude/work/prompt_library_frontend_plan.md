# Prompt Library — frontend plan (Gate 1: INVESTIGATE, awaiting lead approval)

Teammate: frontend lane (Svelte 5 / TS). Contract read from main checkout
`project_docs/prompts-design.md` (not in this worktree — see friction #1). Product design: issue #7
design comment (F1–F8), issue #24 scope.

## Compose-surface technique: OVERLAY model (transparent textarea + highlight layer)

Chosen over contenteditable and over an editor lib (CodeMirror/ProseMirror).

Source of truth = a pure `Doc = { text: string, spans: Span[] }` data structure; the textarea is
the input device; a mirrored `<div>` under it renders provenance tints. Rationale:
- **Testability mandate wins it.** Brief §7 requires span-model transitions / copy-flattening /
  placeholder substitution as pure TS *outside* component internals. Overlay makes the model the
  source of truth (pure, DOM-free, unit-testable). Contenteditable makes the *DOM* the source of
  truth — reverse-engineering intent from DOM mutations is neither clean nor unit-testable.
- **WYSIWYG Copy is free.** `textarea.value` == exact visible plain text (contract's hard promise).
- **No contenteditable cross-browser mutation hell** (Enter→`<div>`, paste HTML, ambiguous span
  boundary attach). Boundary rules (edit at span interior → linked-modified; at trailing edge →
  typed) become *decisions I control* in the pure edit fn, not browser-dependent behavior.

Tradeoffs (honest): overlay needs pixel-matched font/padding/wrap/scroll-sync between textarea and
highlight div (solved pattern, real CSS care); every edit remaps span offsets (the core complexity —
mitigated by `beforeinput` precise target ranges + making it a pure tested fn); hover-name/scope and
click-to-open-modal via caret offset (`selectionStart`) natively — mouse-hover tooltip is a
refinement, caret-based span info is the reliable path.

## Decomposition

Pure logic (DOM-free, unit-tested in prompts_smoke.mjs):
- `src/lib/prompts/types.ts` — Piece, PieceInput, MatchHit, EmbedStatus (snake_case, mirrors serde).
- `src/lib/compose/doc.ts` — Span/SpanState/Doc + `insertPiece`, `applyEdit` (remap + transitions),
  `flatten`, `spanAt`.
- `src/lib/compose/placeholders.ts` — `{{token}}` parse / substitute / mark-unmark.

Store (mirrors search.svelte.ts):
- `src/lib/prompts.svelte.ts` — `export const prompts = $state({...})`, module-private timers/id,
  debounced `match_pieces`, `++matchId` supersession.

API + mocks:
- `src/lib/api.ts` — 7 command wrappers + in-memory dev mock store (provider-profiles CRUD shape),
  seeded pieces, fake embed state machine + Channel progress, dev lexical scorer.

Components:
- `PromptsView.svelte` (layout, project picker, Copy Prompt, embeddings mount)
- `prompts/ComposeBox.svelte` (overlay surface) · `prompts/MatchPanel.svelte` (live match,
  collapsible) · `prompts/PieceModal.svelte` (M2 two-mode, bg-color signal via `.modal--template`) ·
  `prompts/PlaceholderPopover.svelte` (M3) · `prompts/EmbeddingsPanel.svelte` (advanced, collapsed).

Wiring/CSS/tests:
- `+page.svelte` — `'prompts'` in view union + nav button + branch + `goPrompts()`.
- `app.css` — provenance tokens (accent-pattern, `:root` + dark override only if contrast needs it).
- `tests/prompts_smoke.mjs` + one `&& tsx tests/prompts_smoke.mjs` in package.json.

## Contract friction / notes for the lead
1. Contract doc + briefs live on `prompt-library` (e41a910); both worktrees are based on eef310c
   (one commit earlier) so the doc isn't in-tree — read from main checkout by abs path. My src/**
   commits are additive; should merge onto prompt-library cleanly. Keep this base, or rebase onto
   prompt-library? I lean keep-as-is (lead integrates); lead's call.
2. Shared types seam: I assume serde-default snake_case JSON (per contract's schema examples). If
   backend adds rename_all="camelCase" we diverge — flagging so we agree.
3. Project picker carries cwd (identity → match_pieces `project` = absolute path per contract) AND
   label (display via projectLabel). "Global only" = new sentinel → `project: null`. No existing
   global-only sentinel to reuse.
4. node_modules not installed in worktree (pnpm install before phase-2 checks). package.json runs
   smoke via bare `tsx` (brief's verify line said `node --import tsx`; package.json is truth).
