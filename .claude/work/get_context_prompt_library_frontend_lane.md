# get-context — Prompt Library frontend lane (src/**, Svelte 5 + TS)

Simulation: frontend lane of the ccdeck Prompt Library UX round — Svelte 5 component
decomposition (focus-trap action, portalled popover primitive escaping ancestor
overflow/stacking, pure hotkeys module for chord parsing/normalization/conflict-detection,
a notices store), keyboard interaction (roving tabindex, focus traps, arrow/enter/esc in a
textarea + match panel), toast lifecycle, and the color-token protocol (no Tailwind in this
repo — every color is `var(--token)` or `color-mix` over tokens, light+dark both first-class,
no hex in components).

```json
{
  "role": "worker",
  "role_docs": [
    "craft/workflow/worker_execution_protocol",
    "craft/workflow/feature_build_principles",
    "craft/workflow/team_operating_principles",
    "craft/workflow/issue_driven/issue_driven_development_protocol",
    "craft/prompt_engineering/agent_prompt_protocol"
  ],
  "role_docs_in_portal": [],
  "required": [
    "stack/svelte/design_protocol",
    "stack/svelte/insight",
    "stack/design-tokens/color_token_protocol",
    "stack/design-tokens/design_token_protocol",
    "craft/code/typescript_coding_protocol",
    "craft/code/coding_principles",
    "craft/code/frontend_test_principles",
    "craft/code/frontend_test_protocol"
  ],
  "optional": [
    "craft/code/testing_principles",
    "craft/workflow/contract_first_procedure"
  ],
  "trajectory_correction": [
    "Docs-first: project_docs/prompts-design.md and project_docs/prompts-ux.md ARE the governing contract for the exact surfaces you're building (hotkey map, popover/focus-trap geometry, toast/notice lifecycle) — if your implementation has to deviate from what those docs specify (a chord conflict, a focus-trap edge case, a toast timing detail), correct the doc in the same change, don't let code and contract silently diverge.",
    "Read-before-write: several of these components already exist (ComposeBox.svelte, MatchPanel.svelte, EmbeddingsPopover.svelte, ProjectManagerPopover.svelte, VariableFillList.svelte under src/lib/components/prompts/) — re-read each file immediately before editing, not from earlier exploration; this is a multi-lane round so another agent may have touched them since you last looked.",
    "Verify-after: your plan doesn't name a verify step. Before commit run the project's check_cmd: `pnpm check` (svelte-check, 0 errors/warnings) plus `pnpm run test:smoke`; a focus-trap/roving-tabindex/hotkey-conflict change is exactly the class of behavior that needs a driven check (the `verify` skill), not just a type-check.",
    "Commit discipline: no cohesive-set / branch boundary named. Per project memory the Prompt Library UX round lives on branch `prompt-library` (issue #24) — confirm your worktree is based on that branch, not `main` or `harness-parity`; a recorded harness quirk has Agent-tool worktrees fork from `main` by default, so verify the base commit before you start and `git checkout -B <lane> prompt-library` if it's wrong.",
    "No error suppression: a portal primitive and a focus-trap action are exactly the kind of code that tempts a defensive try/catch-and-continue around DOM edge cases (detached nodes, missing ancestors) — catch only the specific case you expect, never fail-open to a silently-broken trap.",
    "Framework-mismatch note (not a doc gap, just a heads-up): stack/svelte/design_protocol's CSS section is written Tailwind-first-vs-raw/scoped-CSS — this repo has no Tailwind, so read that section as 'raw/scoped CSS is the only path' and lean on the two design-token docs (color_token_protocol, design_token_protocol) as the actual authority for what a hardcoded value is allowed to be."
  ]
}
```

## Project docs — outside the ai-first-docs catalog, resolved directly (per MEMORY.md's project router)

These are NOT indexed by the get-context catalog (project_profile.yaml's `docs_regen_cmd` only
covers `ai-first-docs/`) but are the load-bearing project-specific contract for this exact slice —
read them before the corpus docs above:

| Path | Why it's load-bearing here |
|---|---|
| `project_docs/prompts-design.md` | Prompt Library engineering contract — see §"Hotkeys (new this round)" (chord grammar), §"Store robustness — hand-edit corruption" (notices/store patterns), and the Rust↔JS command contract for anything the frontend lane touches. |
| `project_docs/prompts-ux.md` | Prompt Library interaction contract — see §"Popover geometry, focus trap, and Escape", §"Hotkey map", §"Keyboard operability audit — the mouse-only gaps to close", and §"S13 Toast lifecycle and recovering a repair notice". This is the closest thing to a spec for the focus-trap action, portal primitive, and toast/notices store you're building. |
| `ARCHITECTURE.md` (repo root) | Confirms the stack: Tauri v2 + SvelteKit static SPA, Svelte 5 + TS, `src/lib/` is pure logic (no DOM/Tauri), `src/lib/api.ts` is the Tauri-invoke seam. |
| `CONTRIBUTING.md` (repo root) | Verify commands (`pnpm check`, `pnpm test:smoke`, `cargo test --lib`) and the "simple by default, advanced on demand" design rule that governs where new controls surface. |

## Notes

- No hex-in-components note in the brief matches the color-token protocol's raw-palette lint —
  that protocol is the enforcement mechanism, read it before adding any CSS variable.
- `stack/daisyui/reference` and `stack/shadcn/reference` were matched by the catalog's UI-primitive
  keywords but deliberately **excluded**: `package.json` and `ARCHITECTURE.md` confirm this repo
  ships neither DaisyUI nor shadcn-svelte (no Tailwind) — including them would be a framework
  mismatch, not a floor for this slice.
- You have context the router doesn't — drop any pick above that doesn't apply, and re-dispatch
  get-context with a tighter simulation if the set feels off-target.


## doc previews

| tier | stem | path | type | description |
|-|-|-|-|-|
| required | `stack/svelte/design_protocol` | ai-first-docs/stack/svelte/design_protocol.mdx | protocol | Read before writing Svelte 5 components or stores — covers $state/$derived/$effect rules, class vs factory stores, Props conventions, and the Tailwind-first-vs-raw/scoped-CSS exception ladder for when hardcoded CSS is justified over utility classes |
| required | `stack/svelte/insight` | ai-first-docs/stack/svelte/insight.mdx | insight | Read when hitting Svelte 5 or SvelteKit surprises — covers rune gotchas, deprecated APIs ($app/stores, lucide-svelte), event handling migration, and advanced rune features |
| required | `stack/design-tokens/color_token_protocol` | ai-first-docs/stack/design-tokens/color_token_protocol.mdx | protocol | Read before picking any color or adding a color CSS variable — the three color-token axes (brand identity, ordinal severity grading, categorical state), the decision tree for which axis a need belongs on, the regenerable-baseline file architecture that keeps each axis independently retunable, and the lint enforcement against raw palette names. |
| required | `stack/design-tokens/design_token_protocol` | ai-first-docs/stack/design-tokens/design_token_protocol.mdx | protocol | Read before adding any new design token (radius, spacing, dev-only, future axes) or judging whether a hardcoded value belongs in a token — covers tokens-by-intent naming, the rule of three before a new axis, the soft/strong fill convention generalized from color, dev-only namespacing for tree-shake-eligible surfaces, and the boundary test against the color-token protocol. |
| required | `craft/code/typescript_coding_protocol` | ai-first-docs/craft/code/typescript_coding_protocol.mdx | protocol | Read when writing TypeScript — `any`/`unknown` discipline, satisfies vs as, type-only imports, branded types, exhaustive switch, OpenAPI types, eslint rule names |
| required | `craft/code/coding_principles` | ai-first-docs/craft/code/coding_principles.mdx | principles | Read before writing or reviewing code in any language — generic wisdom for type safety, trust boundaries, casts, exhaustive variants, immutability, identifiers, doc comments, helpers, soft-delete, coherent architecture, lint discipline, formatting |
| required | `craft/code/frontend_test_principles` | ai-first-docs/craft/code/frontend_test_principles.mdx | principles | Read when deciding which test layer (unit, view-model, store, component, MSW integration, e2e) a new frontend test belongs in — covers the testing pyramid, the simplest-tool-per-layer rule, and MSW vs module-mock trade-offs |
| required | `craft/code/frontend_test_protocol` | ai-first-docs/craft/code/frontend_test_protocol.mdx | protocol | Read before running or writing frontend tests — covers the four test layers (vitest unit, MSW integration, reactive chain, Playwright E2E), commands, log locations, and infrastructure requirements per mode |
| optional | `craft/code/testing_principles` | ai-first-docs/craft/code/testing_principles.mdx | principles | Read first before writing any tests — establishes the project's testing philosophy of broad assertions, stability-seam focus, and low-maintenance coverage over exhaustive edge cases |
| optional | `craft/workflow/contract_first_procedure` | ai-first-docs/craft/workflow/contract_first_procedure.mdx | procedure | Read when building a new API feature end-to-end OR fixing a behavioral/timing-contract bug — covers the parallel frontend/backend workflow, API contract, type generation, mock data, coordination points, and the protocol-doc-first sequence for behavioral contracts types can't express (author the owning doc before the code) |
| role | `craft/workflow/worker_execution_protocol` | ai-first-docs/craft/workflow/worker_execution_protocol.mdx | protocol | Read the moment you're dispatched as a worker to run an already-scoped, already-approved brief — covers the self-orient-then-verify execution loop, the phase-gate convention some briefs impose, the escalate-vs-proceed frame synthesized for the executing worker, and the report shape you hand back. |
| role | `craft/workflow/feature_build_principles` | ai-first-docs/craft/workflow/feature_build_principles.mdx | principles | Read before picking up any non-trivial feature, refactor, bug, or multi-file change — covers the context-load → propose → build shape, failure modes each phase prevents, and how to proportion ceremony to task size |
| role | `craft/workflow/team_operating_principles` | ai-first-docs/craft/workflow/team_operating_principles.mdx | principles | Read at orientation — the engineering beliefs, decision-ownership model, and anti-patterns every agent inherits when working in this codebase; the substance behind 'why' calls when the work loop and protocols go silent |
| role | `craft/workflow/issue_driven/issue_driven_development_protocol` | ai-first-docs/craft/workflow/issue_driven/issue_driven_development_protocol.mdx | protocol | Read when you discover a problem you won't fix immediately, or need to orient in the issue lifecycle — the map for the issue_driven neighborhood covering the ledger model (discovery, diagnosis, and resolution decoupled), the three dispositions with the bug-auto-file vs feature-escalate rule, tracker eligibility, the three lifecycle stages and their docs, the atomic single-owner claim, and the issues-vs-digest reporting split |
| role | `craft/prompt_engineering/agent_prompt_protocol` | ai-first-docs/craft/prompt_engineering/agent_prompt_protocol.mdx | protocol | Read before writing any prompt, brief, handoff, plan, or principle aimed at another agent — voice, anti-patterns, the author-receiver contract, decision-scope rule, and pointer discipline |
