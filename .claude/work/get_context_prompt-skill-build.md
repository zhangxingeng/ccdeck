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
    "stack/claude-code/skill_protocol",
    "craft/prompt_engineering/authoring_protocol",
    "craft/prompt_engineering/layering_protocol"
  ],
  "optional": [
    "craft/docs/ai_first_suite_unfolding_protocol",
    "craft/code/python_patterns_reference"
  ],
  "trajectory_correction": [
    "Docs-first: this skill changes/extends the Prompt Library's ingestion path (markdown -> JSON snippet). project_docs/prompts-design.md is the contract for the snippet schema — if the skill introduces any new conventions (e.g. how translated snippets get a 'source' provenance tag, or where clarifying-question answers get recorded), amend prompts-design.md/prompts-ux.md rather than letting the skill's own instructions silently become the only record of the behavior.",
    "Read-before-write: before hand-authoring the JSON schema validator, re-read the actual Rust structs/TS types that serialize the on-disk snippet JSON (likely under src-tauri/src/... and src/lib/...) right before writing the validator — don't validate against your recollection of prompts-design.md's schema description, since the code is the ground truth and may have drifted.",
    "Verify-after: the plan as stated ends at 'ship the skill' — add an explicit verify step: run the new validator script against at least one hand-crafted valid snippet and one deliberately-invalid one (missing required field, bad variable grammar) to confirm it actually catches non-compliance, plus the project's check_cmd if any Rust/TS surface is touched.",
    "Pattern-match existing skills: project_docs/skills/cut-release and project_docs/skills/skill-sync are this project's actual skill sources (synced into .claude/skills/ via the skill-sync skill) — author the new skill under project_docs/skills/<name>/ in the same layout/frontmatter shape, then run skill-sync to symlink it, rather than inventing a new skill-folder convention.",
    "Commit discipline: this is non-trivial new infra (skill + bundled validator script) — commit as a cohesive chunk once the skill authoring + validator + a smoke test of the validator all pass, and don't push unless asked."
  ]
}
```

**Role:** manager — the brief names an outcome (a new skill that translates markdown prompts into Prompt Library JSON, with a bundled validator) but the concrete steps (schema discovery, skill layout, script language) are still open and require research before dispatch/build.

**Corrections summary:** amend the Prompt Library design docs if the skill invents new conventions; re-read the actual Rust/TS schema types (not just the design doc prose) right before writing the validator; add an explicit verify step exercising the validator on good/bad fixtures; pattern-match the two existing project skills (cut-release, skill-sync) for folder layout and run skill-sync after adding the new one; commit as one cohesive chunk, don't push unprompted.


## doc previews

| tier | stem | path | type | description |
|-|-|-|-|-|
| required | `stack/claude-code/skill_protocol` | ai-first-docs/stack/claude-code/skill_protocol.mdx | protocol | Read when deciding whether a capability should be an MCP server, a skill, or a plain doc, or when adding or placing a skill — the cost-model sorting rubric, the harness-vs-project source split (real .claude/skills dir vs symlink into a tree you own), the loader discovery contract (id from dir name, trigger from SKILL.md description), the thin-vs-symlinked SKILL.md tradeoff and its JSX-leak gotcha, and the docs-generator collision when skills live in a content tree. |
| required | `craft/prompt_engineering/authoring_protocol` | ai-first-docs/craft/prompt_engineering/authoring_protocol.mdx | protocol | Read before authoring or reducing an LLM-facing task prompt where the author holds domain knowledge the model lacks — covers the author/reader gap that inverts the peer-prompt rules, the direction-vs-depth agency split, the domain-content-vs-scaffolding test for what survives a reduction pass, and placeholder-syntax over example deletion. |
| required | `craft/prompt_engineering/layering_protocol` | ai-first-docs/craft/prompt_engineering/layering_protocol.mdx | protocol | Read before authoring or modifying any agentic prompt caller — covers the three-layer prompt build (system instructions / user message / loop control), which layer owns what, the single-composer convergence seam, and the content-rule vs tool-shape split that keeps the layers coherent. |
| optional | `craft/docs/ai_first_suite_unfolding_protocol` | ai-first-docs/craft/docs/ai_first_suite_unfolding_protocol.mdx | protocol | Read when adding a new artifact to the AI-first suite — a doc, agent stub, MCP tool, script, config, or skill — or deciding how it unfolds into a project. Covers the six unfoldable components, one stable unfold mechanism per kind, the self-documenting-script default and its doc-governed exception, and the docs-are-docs / scripts-are-trunk / configs-are-docs thesis. |
| optional | `craft/code/python_patterns_reference` | ai-first-docs/craft/code/python_patterns_reference.mdx | reference | Read when a lint rule fires, when removing files, or when tempted to reach for `# noqa` — covers the .old_code/ soft-delete convention and reading lint rules as design hints |
| role | `craft/workflow/research_and_plan_procedure` | ai-first-docs/craft/workflow/research_and_plan_procedure.mdx | procedure | Read before planning any feature — alignment phase, assumption verification, and research-first protocol to avoid implementation failures |
| role | `orchestration/worker_usage_principles` | ai-first-docs/orchestration/worker_usage_principles.mdx | principles | Read when delegating work to a `` `worker` `` — covers the trunk-and-branches shape now that subagents nest natively, when to delegate vs inline, model choice, and the context-window calculus that determines whether dispatch saves context or relocates it |
| role | `orchestration/multi_agent_principles` | ai-first-docs/orchestration/multi_agent_principles.mdx | principles | Read before dispatching, forking, or coordinating any subagent — the casting question that decides fork vs steerable teammate, the society-of-minds frame where a running agent's output is signal, the essence of cold / teammate / fork agents, why nesting is native but a fork can't nest and depth caps uniformly, why a worker is a pure function so open-ended waits belong to the trunk, identity discipline against message spoofing, and the both-chairs teammate ethos |
| role | `orchestration/agent_coordination_protocol` | ai-first-docs/orchestration/agent_coordination_protocol.mdx | protocol | Read before coordinating more than one agent — casting (cold/fork/teammate/named-subagent, worktree isolation), the cooperation grammar (event families, push/pull/blackboard initiative, topology, triage, subscription tuning), live-channel delivery/lifecycle, who owns a long wait (blocking command or open-ended event) and the mandatory pre-halt status report with its one-clean-done-signal recipe, reusable patterns, and worktree/merge discipline |
| role | `craft/prompt_engineering/agent_prompt_protocol` | ai-first-docs/craft/prompt_engineering/agent_prompt_protocol.mdx | protocol | Read before writing any prompt, brief, handoff, plan, or principle aimed at another agent — voice, anti-patterns, the author-receiver contract, decision-scope rule, and pointer discipline |
