# get-context — Lane B (Compose), Prompt Library simplification round (v0.13.0)

Simulation: rebuild the prompt-compose surface (`src/lib/compose/doc.ts`, `ComposeBox.svelte`,
`SnippetModal.svelte`, `VariableFillList.svelte`, the compose slice of `src/lib/prompts.svelte.ts`)
around chip-only rendering (never inline-editable), a single popup edit surface (Save / Use once /
Delete), a simplified `{name}`-only variable grammar with Markdown-code-aware tokenizing (skip
fenced/inline code), and a `linked-modified` provenance-state removal — implemented in TS here and
duplicated in Rust (`src-tauri/src/prompts/grammar.rs`, Lane A's half) against a shared test-vector
table. Runs on branch `lane-b-compose` off `prompt-simplify`, one lane of a three-teammate build
under `.claude/work/prompt_library_simplification_plan.md` (contract of record, already read).
Verify gate: `pnpm check && cargo test --lib --manifest-path src-tauri/Cargo.toml && pnpm run test:smoke`.

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
    "craft/code/typescript_coding_protocol",
    "craft/code/coding_principles",
    "craft/workflow/contract_first_procedure",
    "craft/workflow/git_protocol",
    "craft/code/frontend_test_protocol",
    "craft/workflow/redesign_discipline_insight",
    "orchestration/iterative_teammate_protocol",
    "craft/workflow/teammate_execution_protocol"
  ],
  "optional": [
    "stack/svelte/insight",
    "craft/code/frontend_test_principles",
    "craft/code/mock_fidelity_protocol",
    "orchestration/agent_coordination_protocol"
  ],
  "trajectory_correction": [
    "Shipped-users migration gap: your plan deletes the `linked-modified` provenance state, removes the `{name:default}` grammar form, and (via Lane A) collapses the stored snippet schema (uuid/title/body/tags/keywords/scope -> {name, content}) — but names no migration or back-compat path for existing `~/.ccdeck/prompts` data written by the shipped v0.12.0 release. CLAUDE.md's shipped-users mandate treats this as a migration, not a free rewrite, and project memory flags storage layout + variable grammar as the two costliest fields to change. Confirm the simplification-plan contract already specifies the migration/read-path for old-format data before you build against 'old format no longer exists.'",
    "Docs-first: the shared grammar (TS + Rust, one test-vector table) and the chip/provenance model are exactly contract_first_procedure's 'behavioral contract types can't express' case — your plan never names updating the owning contract docs (project_docs/prompts-design.md, project_docs/prompts-ux.md, both outside this catalog but load-bearing here) alongside the code, and doesn't say where the shared TS/Rust test-vector table itself is documented or how you'll confirm Lane A stays in sync with it.",
    "No-error-suppression / round-trip-first: the grammar rewrite is a lossless parse-then-serialize transform (doc spans <-> final prompt text via copyOutput) — exactly the shape project memory's lossless/idempotent-transform rail names. Build the round-trip test against hostile fixtures (fenced code containing `{brace}` text, an unterminated code fence, `{{`/`}}` escapes sitting at a fence boundary, a variable name abutting an inline-code backtick) before reasoning from a code read, not after — this is the class of bug a code read rationalizes past.",
    "Issue disposition: your plan names Lane A's and Lane C's owned regions as off-limits but states no fix-now/file/escalate decision for a defect you spot there while rebuilding the shared seams (e.g. a Rust grammar.rs edge case, or a `hits`/`runMatch` interaction with a chip). Undeclared disposition on an out-of-lane defect is the gap issue_driven_development_protocol exists to close.",
    "Commit discipline: no cohesive-commit boundary named beyond 'verify gate at the end.' Given this is a three-teammate concurrent round on one grammar contract, land the doc.ts/variables.ts collapse, the ComposeBox/SnippetModal chip rewrite, and any grammar-vector-table sync as separable, reviewable commits rather than one large diff — easier for the lead to bisect if Lane A's Rust half and your TS half drift."
  ]
}
```

## Project docs — outside the ai-first-docs catalog, resolved directly (per MEMORY.md's project router)

Not indexed by the get-context catalog (`docs_regen_cmd` only covers `ai-first-docs/`) but load-bearing
for this exact slice — read before the corpus docs above:

| Path | Why it's load-bearing here |
|---|---|
| `project_docs/prompts-design.md` | Prompt Library engineering contract — the pre-simplification shape of the storage schema, variable grammar, and the Rust<->JS command contract this round is subtracting from. Confirm what it says about `{name:default}`, provenance states, and the snippet schema before assuming your plan's target shape is already reflected here — this doc is the thing your round should update. |
| `project_docs/prompts-ux.md` | Prompt Library interaction contract — the pre-round spec for how a linked/modified snippet renders and how editing works today; the doc your new chip-only, popup-only interaction model needs to supersede in the same round. |
| `ARCHITECTURE.md` (repo root) | Repo layout, the Rust<->JS command contract shape, and `src/lib/api.ts` as the Tauri-invoke seam — read this for "how api.ts invokes Tauri commands and any browser-dev fallback," which nothing in the generic catalog covers (Tauri-specific, project-only). |
| `CONTRIBUTING.md` (repo root) | Dev setup, the verify-command set (matches `check_cmd` above), and the "simple by default, advanced on demand" design rule relevant to how the chip popup surfaces its three actions. |
| `.claude/work/prompt_library_simplification_plan.md` | The contract of record for this whole round (already read per your brief) — re-confirm your compose-surface understanding against it, not memory, immediately before editing, since it's shared across three concurrent lanes. |

## Notes

- No doc in the generic catalog covers "contenteditable / chip-style rich compose surface" as a
  pattern — this looks like new-to-this-codebase UI, not an existing convention. Treat it as a
  redesign surface (see `redesign_discipline_insight`) rather than searching for a doc that isn't
  there; if a real pattern emerges, it's a docs-first candidate once your round lands.
  `stack/svelte/design_protocol` is the closest floor (contenteditable-adjacent state/DOM handling
  under Svelte 5 runes), not a chip-pattern doc.
- The shared TS/Rust grammar test-vector table's actual location is a code-search question, not a
  docs one — get-context doesn't explore code; grep the repo (likely near `src/lib/compose/` or
  `src-tauri/src/prompts/`) rather than re-dispatching this router for it.
- `stack/type-generation/protocol` and `stack/openapi-fetch/principles` were considered (frontend/
  backend command-contract keywords) and **excluded** — both describe a generated-OpenAPI-types
  litestar/pydantic stack this project doesn't run; ccdeck's actual Rust<->JS contract is
  Tauri-command-based and lives only in `ARCHITECTURE.md` (see Project docs above). Including the
  generic docs would have been a framework mismatch.
- You have context the router doesn't — drop any pick above that doesn't apply, and re-dispatch
  get-context with a tighter simulation if the set feels off-target.


## doc previews

| tier | stem | path | type | description |
|-|-|-|-|-|
| required | `stack/svelte/design_protocol` | ai-first-docs/stack/svelte/design_protocol.mdx | protocol | Read before writing Svelte 5 components or stores — covers $state/$derived/$effect rules, class vs factory stores, Props conventions, and the Tailwind-first-vs-raw/scoped-CSS exception ladder for when hardcoded CSS is justified over utility classes |
| required | `craft/code/typescript_coding_protocol` | ai-first-docs/craft/code/typescript_coding_protocol.mdx | protocol | Read when writing TypeScript — `any`/`unknown` discipline, satisfies vs as, type-only imports, branded types, exhaustive switch, OpenAPI types, eslint rule names |
| required | `craft/code/coding_principles` | ai-first-docs/craft/code/coding_principles.mdx | principles | Read before writing or reviewing code in any language — generic wisdom for type safety, trust boundaries, casts, exhaustive variants, immutability, identifiers, doc comments, helpers, soft-delete, coherent architecture, lint discipline, formatting |
| required | `craft/workflow/contract_first_procedure` | ai-first-docs/craft/workflow/contract_first_procedure.mdx | procedure | Read when building a new API feature end-to-end OR fixing a behavioral/timing-contract bug — covers the parallel frontend/backend workflow, API contract, type generation, mock data, coordination points, and the protocol-doc-first sequence for behavioral contracts types can't express (author the owning doc before the code) |
| required | `craft/workflow/git_protocol` | ai-first-docs/craft/workflow/git_protocol.mdx | protocol | Read before committing, stashing, branching, or any git operation — covers cohesive-commit discipline and Conventional Commits format, the partial-commit trap against the pre-commit index, why not to hand-stash a slice pre-commit already isolates, destructive-op consent and safety, polyrepo root-confirmation, branch and push policy, and verifying tree state after a commit |
| required | `craft/code/frontend_test_protocol` | ai-first-docs/craft/code/frontend_test_protocol.mdx | protocol | Read before running or writing frontend tests — covers the four test layers (vitest unit, MSW integration, reactive chain, Playwright E2E), commands, log locations, and infrastructure requirements per mode |
| required | `craft/workflow/redesign_discipline_insight` | ai-first-docs/craft/workflow/redesign_discipline_insight.mdx | insight | Read when the user says 'redesign' / 'no back-compat' / 'clean-sheet' — covers the anchoring trap that ships the old design with new labels, the per-module derivation discipline that prevents it, and the self-check before reporting results |
| required | `orchestration/iterative_teammate_protocol` | ai-first-docs/orchestration/iterative_teammate_protocol.mdx | protocol | Read when running steerable teammates through a gated build lifecycle — the lead-side operational recipe complementing the event-based paradigm, covering the investigate-approve-implement-commit-update-issue pipeline, worktree-per-teammate isolation, the append-only commit durability contract that survives a lost teammate, gate-as-dialogue steering, and lead-owned integration onto trunk |
| required | `craft/workflow/teammate_execution_protocol` | ai-first-docs/craft/workflow/teammate_execution_protocol.mdx | protocol | Read the moment you're cast as a teammate in a live team — covers standing as a persistent addressable specialist rather than a one-shot callee, why plain output never crosses without an explicit SendMessage, surfacing lane boundaries live as a duty rather than guessing, and holding an approval gate as dialogue rather than a single terminal report. |
| optional | `stack/svelte/insight` | ai-first-docs/stack/svelte/insight.mdx | insight | Read when hitting Svelte 5 or SvelteKit surprises — covers rune gotchas, deprecated APIs ($app/stores, lucide-svelte), event handling migration, and advanced rune features |
| optional | `craft/code/frontend_test_principles` | ai-first-docs/craft/code/frontend_test_principles.mdx | principles | Read when deciding which test layer (unit, view-model, store, component, MSW integration, e2e) a new frontend test belongs in — covers the testing pyramid, the simplest-tool-per-layer rule, and MSW vs module-mock trade-offs |
| optional | `craft/code/mock_fidelity_protocol` | ai-first-docs/craft/code/mock_fidelity_protocol.mdx | protocol | Read when authoring or trimming mock handlers and fixtures for browser-mode dev or integration tests — covers the one-rich-rest-sparse richness ladder, the single shared exemplar source, deriving sibling endpoints so mocks can't contradict each other, determinism for branch-gating values, module-scope write-then-read handler state, and the data-vs-state-machine consumer asymmetry |
| optional | `orchestration/agent_coordination_protocol` | ai-first-docs/orchestration/agent_coordination_protocol.mdx | protocol | Read before coordinating more than one agent — casting (cold/fork/teammate/named-subagent, worktree isolation), the cooperation grammar (event families, push/pull/blackboard initiative, topology, triage, subscription tuning), live-channel delivery/lifecycle, who owns a long wait (blocking command or open-ended event) and the mandatory pre-halt status report with its one-clean-done-signal recipe, reusable patterns, and worktree/merge discipline |
| role | `craft/workflow/worker_execution_protocol` | ai-first-docs/craft/workflow/worker_execution_protocol.mdx | protocol | Read the moment you're dispatched as a worker to run an already-scoped, already-approved brief — covers the self-orient-then-verify execution loop, the phase-gate convention some briefs impose, the escalate-vs-proceed frame synthesized for the executing worker, and the report shape you hand back. |
| role | `craft/workflow/feature_build_principles` | ai-first-docs/craft/workflow/feature_build_principles.mdx | principles | Read before picking up any non-trivial feature, refactor, bug, or multi-file change — covers the context-load → propose → build shape, failure modes each phase prevents, and how to proportion ceremony to task size |
| role | `craft/workflow/team_operating_principles` | ai-first-docs/craft/workflow/team_operating_principles.mdx | principles | Read at orientation — the engineering beliefs, decision-ownership model, and anti-patterns every agent inherits when working in this codebase; the substance behind 'why' calls when the work loop and protocols go silent |
| role | `craft/workflow/issue_driven/issue_driven_development_protocol` | ai-first-docs/craft/workflow/issue_driven/issue_driven_development_protocol.mdx | protocol | Read when you discover a problem you won't fix immediately, or need to orient in the issue lifecycle — the map for the issue_driven neighborhood covering the ledger model (discovery, diagnosis, and resolution decoupled), the three dispositions with the bug-auto-file vs feature-escalate rule, tracker eligibility, the three lifecycle stages and their docs, the atomic single-owner claim, and the issues-vs-digest reporting split |
| role | `craft/prompt_engineering/agent_prompt_protocol` | ai-first-docs/craft/prompt_engineering/agent_prompt_protocol.mdx | protocol | Read before writing any prompt, brief, handoff, plan, or principle aimed at another agent — voice, anti-patterns, the author-receiver contract, decision-scope rule, and pointer discipline |
