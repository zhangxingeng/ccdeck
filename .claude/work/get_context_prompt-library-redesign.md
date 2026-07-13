# get-context — Prompt Library redesign (manager brief)

**role:** manager — the caller is closing a named outcome (ship the simplification as the next minor
version) whose steps are still open: research the two living contracts + the Rust/Svelte surfaces,
write a plan doc, file one deferred issue, then brief and dispatch teammates in worktrees and review
what comes back. Multiple coordinated pieces under one lead, not several parallel managers — manager,
not director.

```json
{
  "role": "manager",
  "role_docs": [
    "craft/workflow/research_and_plan_procedure",
    "orchestration/worker_usage_principles",
    "orchestration/multi_agent_principles",
    "orchestration/agent_coordination_protocol",
    "craft/prompt_engineering/agent_prompt_protocol"
  ],
  "role_docs_in_portal": [],
  "required": [
    "orchestration/team_management_protocol",
    "orchestration/iterative_teammate_protocol",
    "orchestration/feature_build_manager_protocol",
    "craft/docs/doc_lifecycle_protocol",
    "craft/workflow/issue_driven/issue_planning_protocol",
    "craft/workflow/issue_driven/issue_creation_protocol",
    "craft/workflow/contract_first_procedure",
    "craft/workflow/user_preferences_reference",
    "craft/workflow/git_protocol"
  ],
  "optional": [
    "craft/workflow/handoff_procedure",
    "orchestration/agent_hierarchy_overview",
    "orchestration/parallel_agent_manager_protocol",
    "craft/workflow/feature_build_principles"
  ],
  "trajectory_correction": [
    "Docs-first, timing: your plan reads prompts-design.md and prompts-ux.md and flags them for amendment, but doesn't sequence WHEN — contract_first_procedure's protocol-doc-first rule means the storage-layout and variable-grammar contract amendments belong before or atomically with the teammates' build dispatch, not as trailing cleanup after code ships.",
    "Dispatch hygiene — worktree base: MEMORY.md records a recurring harness quirk — Agent-tool worktrees fork from `main`, not your current feature branch. Every worktree brief for this build must carry 'verify base commit; reset with `git checkout -B <lane> <feature-branch>` if wrong' or a teammate silently starts from the wrong base (it has bitten this project twice already).",
    "Dispatch hygiene — don't pre-feed: your own research pass (Rust command surface, Svelte composer/chip components) is right for YOUR planning, but don't fold that research into teammate briefs. worker_usage_principles' dispatch mandate is to brief with goals + reasons and let each teammate run its own get-context, not hand it a pre-chewed doc dump.",
    "Issue disposition: the repo-split issue is plan-spawned, not defect-discovered — issue_planning_protocol's full-design-intent rule (sibling to, not the same as, issue_creation_protocol's solution-free discovery body) applies. Write the full design intent for the eventual split, since it comes from your own plan doc, not a bug someone tripped over.",
    "Verify-after: the plan ends at 'review what comes back' with no named gate. State the check_cmd run (`pnpm check && cargo test --lib --manifest-path src-tauri/Cargo.toml && pnpm run test:smoke`, profile: check_cmd) as the merge gate for each teammate's lane before it lands on the integration branch.",
    "Plan-doc placement is an open question you named yourself — resolve it via doc_lifecycle_protocol's vision-to-plans-to-shipped pipeline rather than defaulting to `.claude/work/` by habit. Precedent on disk: `.claude/work/prompt_library_frontend_plan.md` from the last Prompt Library round, if you want continuity."
  ]
}
```

## Not in the ai-first-docs catalog — read these directly

`get-context` only indexes the `ai-first-docs/` corpus; this project's own `project_docs/` tree is
outside it, but per the MEMORY.md project router these are the Prompt Library's living contracts —
both need amendment for this redesign, both are load-bearing, required-tier by direct pointer, not
catalog match:

- `project_docs/prompts-design.md` — engineering seams: storage layout under `~/.ccdeck/prompts`, the
  snippet/piece JSON schema, the Rust↔JS command surface, the hybrid match engine, compose provenance.
  Items 1 (storage-by-name), 2 (project = {name, filepath}, recursive scan), and 5 (chip/popup edit
  model) all rewrite sections of this doc.
- `project_docs/prompts-ux.md` — interaction, scenario by scenario. Items 3 (project switcher, last-
  opened), 4 (filter-down search), 5 (chip/popup edit model), 6 (drop the preview toggle) all rewrite
  scenarios here.
- `project_docs/roadmap.md` — where the shipped v0.12.0 entry and the next-minor-version entry both
  live (§Phase 16 precedent for how a Prompt Library round gets recorded).
- `project_docs/multi_agent_bindings.md` — this project's own delegation posture (lead-assigned
  teammates, no backlog-pulling) — read alongside the generic team_management_protocol /
  iterative_teammate_protocol picks above; the generic docs set the mechanics, this sets the local
  rule.

## Reminder

You hold context this router doesn't — drop any pick that doesn't apply, and re-dispatch get-context
with a tighter simulation if this set feels off-target.


## doc previews

| tier | stem | path | type | description |
|-|-|-|-|-|
| required | `orchestration/team_management_protocol` | ai-first-docs/orchestration/team_management_protocol.mdx | protocol | Read when casting a specialist teammate for a multi-turn build — covers the event-based coordination paradigm (dialogue, not one-shot dispatch), why the approval gate emerges from a live channel, expertise-oriented casting of teammates, and the lead-orchestrates-a-workflow pattern, contrasted against one-shot worker delegation. |
| required | `orchestration/iterative_teammate_protocol` | ai-first-docs/orchestration/iterative_teammate_protocol.mdx | protocol | Read when running steerable teammates through a gated build lifecycle — the lead-side operational recipe complementing the event-based paradigm, covering the investigate-approve-implement-commit-update-issue pipeline, worktree-per-teammate isolation, the append-only commit durability contract that survives a lost teammate, gate-as-dialogue steering, and lead-owned integration onto trunk |
| required | `orchestration/feature_build_manager_protocol` | ai-first-docs/orchestration/feature_build_manager_protocol.mdx | protocol | Read when starting a non-trivial feature build — covers the manager's eight-phase trajectory (orient, build, refactor, audit, test, verify, doc-sync, cleanup), per-phase dispatch rules, skip triggers, and completion gates |
| required | `craft/docs/doc_lifecycle_protocol` | ai-first-docs/craft/docs/doc_lifecycle_protocol.mdx | protocol | Read before creating a new doc or moving a doc between folders — the vision-to-plans-to-shipped pipeline that governs which folder a doc belongs in, and the graduation rules that keep the tree from accreting dead weight |
| required | `craft/workflow/issue_driven/issue_planning_protocol` | ai-first-docs/craft/workflow/issue_driven/issue_planning_protocol.mdx | protocol | Read when deciding whether a unit of work is a ready-to-file issue or still a speculative plan doc, or when spawning issues from a matured plan — the readiness-to-build boundary, the full-design-intent rule for a plan-spawned issue (sibling to, not override of, the solution-free discovery body), stand-alone execution with absolute cross-repo links for sandboxed readers, the body-size cap as a decomposition forcing-function, and the cost-aware body-edit tradeoff |
| required | `craft/workflow/issue_driven/issue_creation_protocol` | ai-first-docs/craft/workflow/issue_driven/issue_creation_protocol.mdx | protocol | Read when you file an issue on discovering a problem — Stage 1 of the lifecycle, covering the dense scannable title standard, the solution-free problem body a finder can drop-and-leave, how to size one issue by shared context, and the handoff that triggers diagnosis |
| required | `craft/workflow/contract_first_procedure` | ai-first-docs/craft/workflow/contract_first_procedure.mdx | procedure | Read when building a new API feature end-to-end OR fixing a behavioral/timing-contract bug — covers the parallel frontend/backend workflow, API contract, type generation, mock data, coordination points, and the protocol-doc-first sequence for behavioral contracts types can't express (author the owning doc before the code) |
| required | `craft/workflow/user_preferences_reference` | ai-first-docs/craft/workflow/user_preferences_reference.mdx | reference | Read when about to run a workspace command, scope a feature, or estimating context runway — covers habit verify/lint/test commands with the why behind each, the test-log location convention, the feature-scope/cut-unused-complexity preference, and the context-budget heuristic |
| required | `craft/workflow/git_protocol` | ai-first-docs/craft/workflow/git_protocol.mdx | protocol | Read before committing, stashing, branching, or any git operation — covers cohesive-commit discipline and Conventional Commits format, the partial-commit trap against the pre-commit index, why not to hand-stash a slice pre-commit already isolates, destructive-op consent and safety, polyrepo root-confirmation, branch and push policy, and verifying tree state after a commit |
| optional | `craft/workflow/handoff_procedure` | ai-first-docs/craft/workflow/handoff_procedure.mdx | procedure | Read when handing off work to a fresh agent — frames the handoff as a memory-recovery protocol (the dementia analogy), opens with the manager operating stance (memory-first, delegate, get-context-by-judgment), writes about the recipient in the third person, and gives the five-stage structure plus the plan-mode procedure |
| optional | `orchestration/agent_hierarchy_overview` | ai-first-docs/orchestration/agent_hierarchy_overview.mdx | overview | Read first when a task needs more than one agent — the map of the agent tree (director, manager, worker as scopes of ownership), how each node is cast as a cold subagent, teammate, or fork (and why a fork being built to drive rather than fan out pins it to the leaves), the three dispatch structures, and the ownership matrix showing who owns decompose, brief, workspace, dispatch, merge, and retry as the structure deepens while each role's duty stays fixed |
| optional | `orchestration/parallel_agent_manager_protocol` | ai-first-docs/orchestration/parallel_agent_manager_protocol.mdx | protocol | Read when fanning out N independent workers of similar shape — the map-reduce trajectory under live coordination, the casting call that defaults scoped-slice sweeps to cold subagents and reserves forks for items needing the full inherited context, converging the brief live instead of freezing a task-spec file, the scope-group-cast-coordinate-aggregate cycle, manager capacity (concurrency cap vs session throughput), and the fire-and-forget scaffolding this trajectory retires |
| optional | `craft/workflow/feature_build_principles` | ai-first-docs/craft/workflow/feature_build_principles.mdx | principles | Read before picking up any non-trivial feature, refactor, bug, or multi-file change — covers the context-load → propose → build shape, failure modes each phase prevents, and how to proportion ceremony to task size |
| role | `craft/workflow/research_and_plan_procedure` | ai-first-docs/craft/workflow/research_and_plan_procedure.mdx | procedure | Read before planning any feature — alignment phase, assumption verification, and research-first protocol to avoid implementation failures |
| role | `orchestration/worker_usage_principles` | ai-first-docs/orchestration/worker_usage_principles.mdx | principles | Read when delegating work to a `` `worker` `` — covers the trunk-and-branches shape now that subagents nest natively, when to delegate vs inline, model choice, and the context-window calculus that determines whether dispatch saves context or relocates it |
| role | `orchestration/multi_agent_principles` | ai-first-docs/orchestration/multi_agent_principles.mdx | principles | Read before dispatching, forking, or coordinating any subagent — the casting question that decides fork vs steerable teammate, the society-of-minds frame where a running agent's output is signal, the essence of cold / teammate / fork agents, why nesting is native but a fork can't nest and depth caps uniformly, why a worker is a pure function so open-ended waits belong to the trunk, identity discipline against message spoofing, and the both-chairs teammate ethos |
| role | `orchestration/agent_coordination_protocol` | ai-first-docs/orchestration/agent_coordination_protocol.mdx | protocol | Read before coordinating more than one agent — casting (cold/fork/teammate/named-subagent, worktree isolation), the cooperation grammar (event families, push/pull/blackboard initiative, topology, triage, subscription tuning), live-channel delivery/lifecycle, who owns a long wait (blocking command or open-ended event) and the mandatory pre-halt status report with its one-clean-done-signal recipe, reusable patterns, and worktree/merge discipline |
| role | `craft/prompt_engineering/agent_prompt_protocol` | ai-first-docs/craft/prompt_engineering/agent_prompt_protocol.mdx | protocol | Read before writing any prompt, brief, handoff, plan, or principle aimed at another agent — voice, anti-patterns, the author-receiver contract, decision-scope rule, and pointer discipline |
