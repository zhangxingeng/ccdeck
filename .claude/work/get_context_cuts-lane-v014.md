# get-context — cuts lane (v0.14 core-refocus: issues #34, #36)

## role: worker

Scoped, well-specified slice — three named deletions plus a small, concretely-described
replacement build, no sub-dispatch implied. Treated as **worker**.

## trajectory_correction

- **Docs-first, not just code-first.** The plan deletes three whole subsystems and changes
  AppConfig's parsing behavior (drop `deny_unknown_fields` tolerance) but names no update to
  the docs that describe those systems — `ARCHITECTURE.md` documents the Rust↔JS command
  contract and data model these cuts reshape, and `project_docs/roadmap.md` tracks what
  shipped vs. was deliberately deferred. `contract_first_procedure`'s protocol-doc-first
  sequence applies here: a behavioral-contract change (stale-key tolerance) that types can't
  express should get its doc correction alongside the code, not after.
- **"Shipped users" cuts beyond the one case named.** The brief already covers the AppConfig
  stale-key migration concern explicitly (round-trip test, no `deny_unknown_fields`). It does
  *not* address the other two deletions the same way: existing installs may have on-disk state
  from the settings editor (vendored schema JSON references, any settings overrides) and from
  provider profiles (stored `ProviderProfile`/`KeyBackend` data) that this cut orphans. Confirm
  those leftover files are harmlessly ignored post-cut rather than assuming it — CLAUDE.md's
  "this app has shipped users" mandate treats behavior changes as migrations project-wide, not
  per-file.
- **Known worktree-base quirk, not yet fixed at root.** Project memory records (2026-07-10,
  recurred twice) that Agent-tool worktrees have forked from `main` instead of the intended
  feature branch, leaving teammates missing their branch's code/briefs until they reset
  (`git checkout -B <lane> <feature-branch>`). This dispatch runs inside a worktree
  (`agent-afbe4607955903c5d`) — verify the base commit actually carries the v0.14 branch state
  before trusting the inventory read, and reset if it doesn't.
- **Read-before-write across a shared file.** `resume.ts` and the resume/fork surfaces are
  touched by both the provider-cut (removing the provider branch) and the replacement build
  (copyable resume command). Re-read each file immediately before editing it — parallel lanes
  or IDE edits may have moved it since the inventory pass.
- **Verify-after isn't named.** The plan ends at "report a plan to the lead before
  implementing" with no stated verify step for the build itself. Before calling it done: run
  the project's full check suite (profile `check_cmd`: `pnpm check && cargo test --lib
  --manifest-path src-tauri/Cargo.toml && pnpm run test:smoke`), and drive the actual new
  surfaces (right-click context menu copy, fork-then-copy, PDF print dialog) rather than
  trusting types and unit tests alone — the `/verify` skill's "exercise the flow, don't just
  check types" framing applies directly to a UI-surface change like this.
- **Commit discipline not named.** The plan doesn't state a commit cadence. Given three
  independent deletions plus one replacement feature, commit in cohesive chunks (e.g. one per
  cut, one for the replacement) once each passes checks, per `git_protocol`'s cohesive-commit
  discipline — don't push unless asked.

## doc picks

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
    "craft/code/typescript_coding_protocol",
    "craft/code/coding_principles",
    "craft/workflow/redesign_discipline_insight",
    "craft/workflow/contract_first_procedure",
    "craft/workflow/user_preferences_reference",
    "craft/code/testing_principles",
    "craft/workflow/git_protocol"
  ],
  "optional": [
    "craft/code/frontend_test_principles",
    "craft/code/frontend_test_protocol",
    "stack/shadcn/reference"
  ],
  "trajectory_correction": [
    "Docs-first: the cuts reshape the Rust<->JS command contract (ARCHITECTURE.md) and the shipped/deferred ledger (project_docs/roadmap.md), plus a behavioral-contract change to AppConfig parsing that types can't express (contract_first_procedure) - none named as doc updates in the plan.",
    "Shipped-users migration concern extends beyond the one case named: settings-editor and provider-profile on-disk state need confirming as harmlessly ignored post-cut, not just AppConfig's stale JSON keys.",
    "This dispatch runs in a worktree; project memory records a recurring bug where Agent-tool worktrees fork from main instead of the target feature branch - verify the base commit before trusting the inventory read.",
    "resume.ts and the resume/fork surfaces are touched by both a cut (provider branch removal) and the replacement build (copyable resume command) - re-read immediately before editing, not from the earlier inventory pass.",
    "No verify step named after 'report a plan to the lead' - run check_cmd (pnpm check && cargo test --lib --manifest-path src-tauri/Cargo.toml && pnpm run test:smoke) and actually drive the new context-menu/PDF-print surfaces before calling the build done.",
    "No commit cadence named - commit in cohesive per-cut/per-feature chunks once each passes checks; don't push unless asked."
  ]
}
```

Note: this project's Rust/Tauri backend has no dedicated stack doc in the generic corpus —
`coding_principles` is the cross-language fallback for the Rust side (settings.rs,
providers.rs, appconfig.rs, lib.rs). The systems being cut (settings editor, provider
profiles, terminal launch) are documented only in this project's own `ARCHITECTURE.md` /
`project_docs/`, not in the ai-first-docs catalog this router indexes — pull those directly,
not through this tool.

You have context this router doesn't (the actual inventory, the issue bodies) — drop any pick
that doesn't apply, and re-dispatch with a tighter simulation if the set feels off-target.


## doc previews

| tier | stem | path | type | description |
|-|-|-|-|-|
| required | `stack/svelte/design_protocol` | ai-first-docs/stack/svelte/design_protocol.mdx | protocol | Read before writing Svelte 5 components or stores — covers $state/$derived/$effect rules, class vs factory stores, Props conventions, and the Tailwind-first-vs-raw/scoped-CSS exception ladder for when hardcoded CSS is justified over utility classes |
| required | `stack/svelte/insight` | ai-first-docs/stack/svelte/insight.mdx | insight | Read when hitting Svelte 5 or SvelteKit surprises — covers rune gotchas, deprecated APIs ($app/stores, lucide-svelte), event handling migration, and advanced rune features |
| required | `craft/code/typescript_coding_protocol` | ai-first-docs/craft/code/typescript_coding_protocol.mdx | protocol | Read when writing TypeScript — `any`/`unknown` discipline, satisfies vs as, type-only imports, branded types, exhaustive switch, OpenAPI types, eslint rule names |
| required | `craft/code/coding_principles` | ai-first-docs/craft/code/coding_principles.mdx | principles | Read before writing or reviewing code in any language — generic wisdom for type safety, trust boundaries, casts, exhaustive variants, immutability, identifiers, doc comments, helpers, soft-delete, coherent architecture, lint discipline, formatting |
| required | `craft/workflow/redesign_discipline_insight` | ai-first-docs/craft/workflow/redesign_discipline_insight.mdx | insight | Read when the user says 'redesign' / 'no back-compat' / 'clean-sheet' — covers the anchoring trap that ships the old design with new labels, the per-module derivation discipline that prevents it, and the self-check before reporting results |
| required | `craft/workflow/contract_first_procedure` | ai-first-docs/craft/workflow/contract_first_procedure.mdx | procedure | Read when building a new API feature end-to-end OR fixing a behavioral/timing-contract bug — covers the parallel frontend/backend workflow, API contract, type generation, mock data, coordination points, and the protocol-doc-first sequence for behavioral contracts types can't express (author the owning doc before the code) |
| required | `craft/workflow/user_preferences_reference` | ai-first-docs/craft/workflow/user_preferences_reference.mdx | reference | Read when about to run a workspace command, scope a feature, or estimating context runway — covers habit verify/lint/test commands with the why behind each, the test-log location convention, the feature-scope/cut-unused-complexity preference, and the context-budget heuristic |
| required | `craft/code/testing_principles` | ai-first-docs/craft/code/testing_principles.mdx | principles | Read first before writing any tests — establishes the project's testing philosophy of broad assertions, stability-seam focus, and low-maintenance coverage over exhaustive edge cases |
| required | `craft/workflow/git_protocol` | ai-first-docs/craft/workflow/git_protocol.mdx | protocol | Read before committing, stashing, branching, or any git operation — covers cohesive-commit discipline and Conventional Commits format, the partial-commit trap against the pre-commit index, why not to hand-stash a slice pre-commit already isolates, destructive-op consent and safety, polyrepo root-confirmation, branch and push policy, and verifying tree state after a commit |
| optional | `craft/code/frontend_test_principles` | ai-first-docs/craft/code/frontend_test_principles.mdx | principles | Read when deciding which test layer (unit, view-model, store, component, MSW integration, e2e) a new frontend test belongs in — covers the testing pyramid, the simplest-tool-per-layer rule, and MSW vs module-mock trade-offs |
| optional | `craft/code/frontend_test_protocol` | ai-first-docs/craft/code/frontend_test_protocol.mdx | protocol | Read before running or writing frontend tests — covers the four test layers (vitest unit, MSW integration, reactive chain, Playwright E2E), commands, log locations, and infrastructure requirements per mode |
| optional | `stack/shadcn/reference` | ai-first-docs/stack/shadcn/reference.mdx | reference | Read when choosing a UI primitive — the shadcn-svelte component catalog (Button, Dialog, Table, Sheet, etc.) with one-line purposes to avoid redundant installs, plus the third-party-code rule that scopes type-suppression comments to the generated component directory. |
| role | `craft/workflow/worker_execution_protocol` | ai-first-docs/craft/workflow/worker_execution_protocol.mdx | protocol | Read the moment you're dispatched as a worker to run an already-scoped, already-approved brief — covers the self-orient-then-verify execution loop, the phase-gate convention some briefs impose, the escalate-vs-proceed frame synthesized for the executing worker, and the report shape you hand back. |
| role | `craft/workflow/feature_build_principles` | ai-first-docs/craft/workflow/feature_build_principles.mdx | principles | Read before picking up any non-trivial feature, refactor, bug, or multi-file change — covers the context-load → propose → build shape, failure modes each phase prevents, and how to proportion ceremony to task size |
| role | `craft/workflow/team_operating_principles` | ai-first-docs/craft/workflow/team_operating_principles.mdx | principles | Read at orientation — the engineering beliefs, decision-ownership model, and anti-patterns every agent inherits when working in this codebase; the substance behind 'why' calls when the work loop and protocols go silent |
| role | `craft/workflow/issue_driven/issue_driven_development_protocol` | ai-first-docs/craft/workflow/issue_driven/issue_driven_development_protocol.mdx | protocol | Read when you discover a problem you won't fix immediately, or need to orient in the issue lifecycle — the map for the issue_driven neighborhood covering the ledger model (discovery, diagnosis, and resolution decoupled), the three dispositions with the bug-auto-file vs feature-escalate rule, tracker eligibility, the three lifecycle stages and their docs, the atomic single-owner claim, and the issues-vs-digest reporting split |
| role | `craft/prompt_engineering/agent_prompt_protocol` | ai-first-docs/craft/prompt_engineering/agent_prompt_protocol.mdx | protocol | Read before writing any prompt, brief, handoff, plan, or principle aimed at another agent — voice, anti-patterns, the author-receiver contract, decision-scope rule, and pointer discipline |
