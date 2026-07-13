# get-context — Lane A (Store), prompt-simplify, issue #31

Caller: Lane A, one of three teammates on branch `prompt-simplify`. Owns the entire
Rust side of the Prompt Library storage-model rewrite (JSON+uuid store → Markdown
files + folder scanning, grammar simplification, command-surface cut). Single clear
deliverable, plan already drafted step-by-step → **role: worker**, cast as a live
teammate in a gated multi-lane build (not a one-shot solo worker).

**Gap in the generic corpus:** there is no `stack/rust/` or `stack/tauri/` doc in
`ai-first-docs` — this project has never authored one. The Rust↔JS command
contract, repo layout, and Tauri command/state conventions this project actually
uses live in **project-local docs, not the catalog**: `ARCHITECTURE.md` (repo
layout + command contract + data model) and `CONTRIBUTING.md` (verify commands,
PR flow). Same for filesystem/datadir persistence conventions and Rust
test/smoke-suite conventions — no generic doc covers them; go straight to
`src-tauri/src/prompts/datadir.rs` and `CONTRIBUTING.md`. The two files you
explicitly asked about, `project_docs/prompts-design.md` (engineering contract)
and `project_docs/prompts-ux.md` (interaction contract), are also outside the
`ai-first-docs` catalog — they're this project's leaf docs, routed via
`CLAUDE.md`'s Project router table, not this catalog. Both exist (confirmed via
`ls project_docs/`) and are the primary governing docs for this slice; read them
directly, per project memory the Docs lane is amending them concurrently.

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
    "orchestration/iterative_teammate_protocol",
    "craft/workflow/teammate_execution_protocol",
    "craft/workflow/git_protocol",
    "craft/workflow/contract_first_procedure",
    "craft/code/coding_principles"
  ],
  "optional": [
    "craft/docs/doc_taxonomy_protocol",
    "craft/workflow/user_preferences_reference"
  ],
  "trajectory_correction": [
    "Verify your worktree's base commit is actually on `prompt-simplify`, not `main`, before writing any code — a recurring harness quirk (Agent-tool worktrees have forked from `main` twice this project) has stranded teammates on a base missing their branch's code; reset with `git checkout -B <lane> prompt-simplify` if wrong.",
    "Docs-first: the grammar rewrite introduces a genuinely new rule (no variable parsing inside fenced code blocks or inline code spans) and deletes a form (`{name:default}`) — that's a contract change, not just an implementation detail. Land it in `project_docs/prompts-design.md`'s shared test-vector table (coordinating with the concurrent Docs lane) rather than letting the Rust implementation get ahead of the doc both you and Lane B (TS) are keying off.",
    "Grammar rewrite is a parse/mutate class bug magnet (escaping `{{`/`}}`, code-fence/code-span exclusion, deleted syntax form) — per project memory's round-trip-test rail, write the hostile-fixture test table first (nested fences, escaped braces adjacent to real placeholders, inline code spans, unterminated fences, unicode in names) and let failures point at the bug, rather than reasoning `grammar.rs` correct from a read.",
    "The recursive `*.md` folder scan reads hand-edited, git-committed files — exactly the untrusted-input case. Don't let a malformed file (bad frontmatter, non-UTF8, broken symlink) blanket-catch into a silently-empty snippet list; fail loud or skip-and-report per-file.",
    "Plan doesn't name a re-read-before-edit step for `store.rs` / `projects.rs` / `state.rs` / `lib.rs` — three lanes are editing concurrently in worktrees; re-read each file immediately before editing it, don't trust what you read at planning time.",
    "Plan names the merge gate (`pnpm check && cargo test --lib ... && pnpm run test:smoke`) but not commit discipline — run it before each cohesive commit (not only once at the end), especially after the `state.rs` command cut and `lib.rs` `generate_handler!` registration edit, where a miscount is easy and silent; commit in cohesive chunks, don't push unless asked."
  ]
}
```

## role

worker — cast as a live teammate (Lane A) inside a gated three-lane build, not a
solo one-shot dispatch. `orchestration/iterative_teammate_protocol` and
`craft/workflow/teammate_execution_protocol` cover the lane-boundary and
gated-lifecycle mechanics your plan already gestures at (worktree isolation,
approval gates, append-only commit durability) — read both before the first commit.

## trajectory_correction (rendered)

1. Confirm worktree base is `prompt-simplify`, not `main` — known recurring harness bug.
2. Docs-first: the fenced-code/inline-code exclusion rule and the `{name:default}` deletion are contract changes — land them in `prompts-design.md`'s shared test-vector table, coordinated with Docs lane, not after the fact.
3. Grammar rewrite → hostile-fixture round-trip tests first, not a code-read.
4. Folder scan of hand-edited Markdown → no blanket catch-to-empty on malformed files.
5. Re-read `store.rs`/`projects.rs`/`state.rs`/`lib.rs` immediately before editing — concurrent lanes.
6. Run the merge gate before each cohesive commit, not just once at the end; conventional commits; don't push unless asked.


## doc previews

| tier | stem | path | type | description |
|-|-|-|-|-|
| required | `orchestration/iterative_teammate_protocol` | ai-first-docs/orchestration/iterative_teammate_protocol.mdx | protocol | Read when running steerable teammates through a gated build lifecycle — the lead-side operational recipe complementing the event-based paradigm, covering the investigate-approve-implement-commit-update-issue pipeline, worktree-per-teammate isolation, the append-only commit durability contract that survives a lost teammate, gate-as-dialogue steering, and lead-owned integration onto trunk |
| required | `craft/workflow/teammate_execution_protocol` | ai-first-docs/craft/workflow/teammate_execution_protocol.mdx | protocol | Read the moment you're cast as a teammate in a live team — covers standing as a persistent addressable specialist rather than a one-shot callee, why plain output never crosses without an explicit SendMessage, surfacing lane boundaries live as a duty rather than guessing, and holding an approval gate as dialogue rather than a single terminal report. |
| required | `craft/workflow/git_protocol` | ai-first-docs/craft/workflow/git_protocol.mdx | protocol | Read before committing, stashing, branching, or any git operation — covers cohesive-commit discipline and Conventional Commits format, the partial-commit trap against the pre-commit index, why not to hand-stash a slice pre-commit already isolates, destructive-op consent and safety, polyrepo root-confirmation, branch and push policy, and verifying tree state after a commit |
| required | `craft/workflow/contract_first_procedure` | ai-first-docs/craft/workflow/contract_first_procedure.mdx | procedure | Read when building a new API feature end-to-end OR fixing a behavioral/timing-contract bug — covers the parallel frontend/backend workflow, API contract, type generation, mock data, coordination points, and the protocol-doc-first sequence for behavioral contracts types can't express (author the owning doc before the code) |
| required | `craft/code/coding_principles` | ai-first-docs/craft/code/coding_principles.mdx | principles | Read before writing or reviewing code in any language — generic wisdom for type safety, trust boundaries, casts, exhaustive variants, immutability, identifiers, doc comments, helpers, soft-delete, coherent architecture, lint discipline, formatting |
| optional | `craft/docs/doc_taxonomy_protocol` | ai-first-docs/craft/docs/doc_taxonomy_protocol.mdx | protocol | Read when adding or moving a doc — covers the trunk-vs-leaf genericity test (asked first, picks the repo), folder routing, filename type vocabulary, plan-folder names, reference-direction invariant, and frontmatter description rules |
| optional | `craft/workflow/user_preferences_reference` | ai-first-docs/craft/workflow/user_preferences_reference.mdx | reference | Read when about to run a workspace command, scope a feature, or estimating context runway — covers habit verify/lint/test commands with the why behind each, the test-log location convention, the feature-scope/cut-unused-complexity preference, and the context-budget heuristic |
| role | `craft/workflow/worker_execution_protocol` | ai-first-docs/craft/workflow/worker_execution_protocol.mdx | protocol | Read the moment you're dispatched as a worker to run an already-scoped, already-approved brief — covers the self-orient-then-verify execution loop, the phase-gate convention some briefs impose, the escalate-vs-proceed frame synthesized for the executing worker, and the report shape you hand back. |
| role | `craft/workflow/feature_build_principles` | ai-first-docs/craft/workflow/feature_build_principles.mdx | principles | Read before picking up any non-trivial feature, refactor, bug, or multi-file change — covers the context-load → propose → build shape, failure modes each phase prevents, and how to proportion ceremony to task size |
| role | `craft/workflow/team_operating_principles` | ai-first-docs/craft/workflow/team_operating_principles.mdx | principles | Read at orientation — the engineering beliefs, decision-ownership model, and anti-patterns every agent inherits when working in this codebase; the substance behind 'why' calls when the work loop and protocols go silent |
| role | `craft/workflow/issue_driven/issue_driven_development_protocol` | ai-first-docs/craft/workflow/issue_driven/issue_driven_development_protocol.mdx | protocol | Read when you discover a problem you won't fix immediately, or need to orient in the issue lifecycle — the map for the issue_driven neighborhood covering the ledger model (discovery, diagnosis, and resolution decoupled), the three dispositions with the bug-auto-file vs feature-escalate rule, tracker eligibility, the three lifecycle stages and their docs, the atomic single-owner claim, and the issues-vs-digest reporting split |
| role | `craft/prompt_engineering/agent_prompt_protocol` | ai-first-docs/craft/prompt_engineering/agent_prompt_protocol.mdx | protocol | Read before writing any prompt, brief, handoff, plan, or principle aimed at another agent — voice, anti-patterns, the author-receiver contract, decision-scope rule, and pointer discipline |
