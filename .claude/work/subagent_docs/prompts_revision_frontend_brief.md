# Brief — Prompt Library revision round, FRONTEND lane (Svelte 5 / TypeScript)

You are an opus teammate on ccdeck's `prompt-library` branch, working in a git worktree. You own
the entire frontend of the founder's post-feel-check design revision (issue #24 continuation).
You are a smart agent: the contract fixes the WHAT at the seams; the HOW inside your lane —
component boundaries, the span-model changes, interaction feel — is yours. Decisions that change
the seam contract or the founder's stated design come to the lead at a gate, never improvised.

## The contract — read first, it is the spec

`project_docs/prompts-design.md` (in your worktree, freshly amended — especially § Variable
grammar, § Copy output, § Compose surface, § Project model). The founder's design intent in one
line: **Apple vibe, not geek vibe — a daily prompt builder that stays simple, not a powerful
chore.** When a micro-decision is open, pick the calmer, less-chromed option.

## Your lane (exclusive) — `src/**` and `tests/prompts_smoke.mjs`

A backend teammate works `src-tauri/**` in parallel; the commands and shapes they build are
exactly the contract's. You own `src/lib/api.ts` and `src/lib/prompts/types.ts` — mirror the
contract, not the Rust source. Contract wrong or ambiguous → STOP and raise at your gate (last
round's audit caught a cross-side grammar divergence born from unilateral seam "fixes").

## The work

1. **Tabs + projects**: Global tab (always first, neutral) + one tab per pinned project atop the
   Prompts view. A project-manager popover creates/renames/recolors/pins/deletes projects
   (palette keys only — swatches rendered from the `--project-<key>` tokens). Active tab drives
   the match pool, the save scope, and the tint.
2. **Compose surface redesign** (this is the heart — reread the contract section):
   - Raw literal text incl. `{var}` tokens; spans keep typed/linked/linked-modified provenance
     (`src/lib/compose/doc.ts` evolves — variables are no longer filled at insert).
   - **Situational affordances, no persistent buttons**: Copy Prompt bottom-right only when
     non-empty; Save-as-piece floating next to an active selection (opens the piece modal
     prefilled, scoped to the active tab — this is also the ONLY way pieces are born; the
     "+ New piece" and persistent "Save selection"/"Copy" buttons are removed).
   - **Variable fill list** auto-appears under the box when parsing finds variables: distinct
     names in first-appearance order, defaults as placeholder text, one fill input each. Names
     unify across the whole document. `PlaceholderPopover.svelte` is retired — delete it and its
     machinery, per the deprecation discipline (replacement in the same change, nothing dangling).
   - **TS grammar implementation** must match the contract exactly; encode ALL shared test
     vectors verbatim in the smoke tests.
   - **Copy output** per § Copy output: as-variable toggle (default ON, persisted via app
     config `prompts_as_variable`), XML mode / substitute-in-place mode.
3. **Color language — tokens only** (mandatory reading below; repo is plain CSS, no Tailwind,
   tokens are hand-rolled CSS custom properties in `src/app.css`): add `--project-<key>` for the
   nine palette keys and `--highlight` (highlighter yellow), each defined for light AND dark.
   Components never see a key name or hex — the active project's color flows through ONE
   `--project-color` custom property set at the view wrapper; every fill is `var()` or
   `color-mix(...)` over vars. Compose-box background = faint hint of project color (contained
   to the box); global piece spans greyish translucent; project piece spans darker translucent
   project hue; selection = `--highlight` via `::selection` scoped to the compose surface.
4. **Piece modal**: Content | Metadata tabs (keywords/tags/category demoted to Metadata);
   Content shows a read-only variable preview (parsed names + defaults).
5. **Embeddings popover** replaces the inline panel: one "Download & index" CTA with the
   requirements note, two progress bars (Download = runtime+model stages aggregated; Index =
   the new index stage), the enable toggle. New event shape: `{stage, done, total}`.
6. **api.ts + dev mocks**: new project commands, `recovered` flag, new event shape,
   `match_pieces(query, project_id, limit)` — the browser mock layer must exercise everything
   (`pnpm dev` is how the founder feel-checks; a fake embed download + fake projects must work).

## Quality mandate (founder's words: this is the tweaking/optimization pass)

The skeleton exists; enforce the standards on everything you touch — refactor touched code up to
standard. Mandatory reading (the docs corpus is gitignored and NOT in your worktree — absolute
paths into the main checkout):

- /home/shane/workspace/ccdeck/ai-first-docs/stack/svelte/design_protocol.mdx
- /home/shane/workspace/ccdeck/ai-first-docs/craft/code/typescript_coding_protocol.mdx
- /home/shane/workspace/ccdeck/ai-first-docs/craft/code/coding_principles.mdx
- /home/shane/workspace/ccdeck/ai-first-docs/stack/design-tokens/color_token_protocol.mdx
- /home/shane/workspace/ccdeck/ai-first-docs/stack/design-tokens/design_token_protocol.mdx
- /home/shane/workspace/ccdeck/ai-first-docs/craft/workflow/git_protocol.mdx
- /home/shane/workspace/ccdeck/ai-first-docs/craft/workflow/teammate_execution_protocol.mdx

Token-protocol translation for this repo: no Tailwind/shadcn layers exist — apply the intent
rules directly (every color a `var(--token)` or `color-mix` over tokens; no hex/literal color in
components or `<style>` blocks; light+dark first-class). Svelte 5 runes discipline per the
design protocol ($derived before handlers before $effect; factory stores; Props interfaces;
snippets; callback props). Plus in-worktree: `ARCHITECTURE.md`, `CONTRIBUTING.md`.

## Pipeline (gated — do not skip gates)

1. **INVESTIGATE**: read the contract, the docs, and the existing components
   (`src/lib/components/PromptsView.svelte`, `prompts/*`, `src/lib/compose/doc.ts`,
   `src/lib/prompts.svelte.ts`, `src/lib/api.ts`). Produce a short plan: component tree after
   the redesign, what dies, what doc.ts keeps/loses, risks, any contract ambiguity.
   **STOP and report — wait for lead approval.**
2. **IMPLEMENT + COMMIT** on approval: conventional commits, cohesive chunks. Commit before
   every report; corrections are NEW commits — never amend/rebase/force-push. Verify in your
   worktree: `pnpm install`, then `pnpm check` clean, `pnpm run test:smoke` green, `pnpm build`
   clean. **STOP and report** what you built, verified, and judged.
3. The lead integrates and runs a visual pass; expect correction rounds — each lands as new
   commits.
