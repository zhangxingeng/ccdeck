# Brief: author the agentic-system migration docs (corpus track)

## Why this work exists

The ai-first-docs corpus fully covers *greenfield* adoption (`.setup/setup.md` + stages) and covers
two *pieces* of migration — splitting entangled docs (`craft/docs/trunk_leaf_migration_procedure.mdx`)
and deciding what kind of artifact something should be (`craft/docs/ai_first_suite_unfolding_protocol.mdx`
role sort + `stack/claude-code/skill_protocol.mdx` cost model). But nothing covers migrating a
pre-existing NON-STANDARD agentic project's own setup — its accumulated `.claude` skills, agents, MCP
servers, hooks, CLAUDE.md, and memory — onto the standard form. The adoption flow is explicitly
greenfield (Stage 1 refuses a non-empty target; the only pre-existing-content rule anywhere is Stage 4's
".mcp.json merge, don't clobber"). You are writing the missing whole-project migration protocol, its
`.setup/` entry point, and one more missing piece it depends on (the md→mdx lift). These docs will
govern every future project the user standardizes, so quality dominates speed.

## Repo and git discipline

- Everything you write lives in the **ai-first-docs repo**: `/home/shane/workspace/ccdeck/ai-first-docs`
  — a separate git repo nested inside the ccdeck checkout. The parent ccdeck repo shows it as clean,
  so a status/diff run from the wrong root reads falsely clean. Run every git command as
  `git -C /home/shane/workspace/ccdeck/ai-first-docs …` and never commit to the ccdeck parent.
- Branch `migration-protocol` is already checked out there. Commit to it; **do not push** — the lead
  pushes after the audit gate.
- Conventional Commits, cohesive commits: one commit per doc (plus follow-up commits for gate
  revisions — corrections are new commits, never amend/reset, so nothing reported is ever rewritten).
- Commit before every checkpoint report, so your work survives any interruption.

## Orientation (before writing anything)

Dispatch the project's `get-context` agent with a simulation of this task ("authoring three new corpus
docs: a whole-project migration protocol in craft/docs, a .setup entry-point brief, an md→mdx lift
procedure — need the doc-authoring conventions"), then read what it floors. At minimum you need, and
must follow, these — they are the conventions your docs will be audited against:

| Doc | What it floors |
| - | - |
| `craft/docs/protocol_writing_protocol.mdx` | the spine/sections/voice a `_protocol` doc must have; procedure-vs-protocol boundary |
| `craft/docs/high_quality_docs_protocol.mdx` | quality rules: refer-don't-repeat, capture wisdom, lean density |
| `craft/docs/doc_taxonomy_protocol.mdx` | folder routing, filename type vocabulary, frontmatter description rules |
| `craft/docs/mdx_doc_protocol.mdx` | MDX frontmatter schema, DocMeta/DocumentScope/DocLink components, heading rules |
| `craft/prompt_engineering/agent_prompt_protocol.mdx` | voice for the `.setup/migrate.md` prompt body (§7 two-voice template rules) |
| `craft/docs/doc_lifecycle_protocol.mdx` | a shipped generic methodology doc graduates straight into the corpus |

And read the four docs yours must LINK TO, NOT RESTATE (duplicating any of them is the failure mode
that gets your draft rejected): `trunk_leaf_migration_procedure.mdx`, `ai_first_suite_unfolding_protocol.mdx`,
`stack/claude-code/skill_protocol.mdx`, `template_adoption_overview.mdx`. Also read `.setup/setup.md`
end-to-end — your `.setup/migrate.md` is its sibling and should feel like one — and skim
`.setup/adopt/stage_4_wire_ai.md` for the merge-don't-clobber rule your protocol generalizes.

## The three deliverables

### 1. `craft/docs/agentic_system_migration_protocol.mdx` — the foundation; write it first

The judgment loop for taking ONE existing project whose agentic system is non-standard — any mix of:
skills that blend generic methodology with project-specific steps, project knowledge living inside
skill bodies, MCP servers that should be skills or scripts, actionable docs nobody reaches for, a
mega-CLAUDE.md holding everything, fragmented or missing memory, agents/hooks accreted ad hoc — and
re-homing every artifact onto the standard form. Required content, shaped how the protocol-writing
rules say, not how this list is ordered:

- **The flow it controls** (abstract — it governs the flow, never the format of project artifacts):
  1. **Audit**: one dispatched agent inventories every artifact of the existing system and classifies
     each against the existing rubrics (cite the unfolding protocol's role sort and skill_protocol's
     cost model — the classification vocabulary already exists; your protocol contributes the
     *migration* application of it).
  2. **Migration plan doc, before any migration**: the plan is written and approved first. One file or
     several depending on migration size — the protocol names what a plan must ANSWER (inventory,
     verdict per artifact, destination, ordering, risks), never a rigid template.
  3. **Execute md-first** (below).
  4. **Merge upstream**: generic knowledge found in the project's docs/skills flows to the shared
     corpus — that half is owned by trunk_leaf_migration_procedure; link it.
  5. **Verify**: the standard gates, plus "the migrated system actually loads" (skills discovered,
     agents dispatchable, MCP servers start).
  6. **Optional lift**: md→mdx Starlight, via deliverable 3 — a separate later step, never part of
     the migration itself.
- **The md-first rule**: migration always converges on pure-markdown project docs (relative-path
  links, no JSX, no site scaffold) regardless of the project's final destination — because md is the
  universal intermediate both setup formats share; it's lossless to lift later, and it keeps the
  migration identical for a flat-docs project and a future Starlight one (the two formats differ only
  in site setup). Carry that why.
- **Generic migration hints** — the heart of the doc: "when you find X in the existing system, its
  standard home is Y, because Z" guidance. Cover at least: the artifact-shape mismatches (skill that
  only informs, util-shaped MCP, actionable doc nobody reaches for — skill_protocol's costume
  vocabulary is citable), entangled generic+project content (route to trunk_leaf), mega/monolithic
  CLAUDE.md → three-layer split (cite `craft/memory/agent_memory_protocol.mdx`), fragmented memory
  files → single MEMORY.md, pre-existing .mcp.json (merge-don't-clobber, generalized), scratch/session
  artifacts (retire, don't migrate), stale docs (verify against ground truth before moving — migration
  is the freshness gate). **Format per hint by what conveys best**: recognition rules with the because
  where reasoning matters, the costume metaphor where it fits, a compact table where scanning wins.
  The user explicitly rejected one-template dogma — mixing forms deliberately is correct here.
- **Boundary section**: what this protocol is NOT — not the greenfield adoption (setup.md), not the
  sibling-repo graft (`orchestration/agentic_system_sync_protocol.mdx` — name it, it's the doc people
  will confuse this with), not the docs-split mechanics (trunk_leaf owns those).

### 2. `.setup/migrate.md` — thin entry point, sibling of setup.md

The pasteable brief for a fresh session that becomes the MIGRATION MANAGER for an existing project —
the "existing system" counterpart to setup.md's greenfield manager. Follow agent_prompt_protocol §7:
wrapper prose (when to use this vs setup.md) + fenced prompt body. The body orients the manager:
confirm the project, dispatch the audit, gate the migration plan with the user, dispatch execution,
verify, offer the lift. It POINTS at deliverable 1 for all judgment content — if the fenced body
exceeds roughly a screen and a half, you've restated protocol content that should be a pointer.
Plain `.md` deliberately (it's read in-repo like setup.md, not rendered).

### 3. `craft/docs/md_to_mdx_lift_procedure.mdx`

The missing second step the md-first rule promises: lifting a pure-md project docs folder into the
owned Astro/Starlight site form. Ordered steps with their why (it's a `_procedure`): scaffold via the
existing adoption stages (link, don't restate), then the conversion mechanics — frontmatter conformance,
relative links → `<DocLink>`, where JSX/imports may now enter and the SKILL.md JSX-leak caution
(skill_protocol owns it — link), catalog re-wiring from flat mode to site mode (`--content-root`
flip), and the verify gates. Include the reverse assurance: what makes md→mdx lossless (why the
md-first rule is safe to trust).

## Decisions you do NOT make silently

- Renaming/rescoping any EXISTING corpus doc — flag at a checkpoint instead.
- Deviating from the deliverable list above (adding a fourth doc, merging two into one) — propose at
  CP1 with reasoning, don't just do it.
- If the taxonomy/lifecycle docs contradict anything in this brief (e.g. a different folder for a
  deliverable), the corpus docs WIN — surface the conflict in your checkpoint report.

## Checkpoints (report, then wait for the lead's reply before proceeding)

- **CP1 — outline**: per-doc skeleton (headings one level deep) + the complete hints list (one line
  per hint: the finding → the verdict → chosen presentation form). This is where framing gets steered;
  cheap to change here, expensive after.
- **CP2 — deliverable 1 drafted and committed.**
- **CP3 — deliverables 2 and 3 drafted and committed.**
- **CP4 — gate fixes**: the lead runs a cold audit + catalog gates and returns findings; you fix and
  commit.

## Verification you run yourself (before CP2 and CP3 reports)

From the ai-first-docs repo root, each with `timeout` ~120000ms:

```
python3 .setup/site/mcp_servers/docs_catalog.py regen --content-root .
python3 .setup/site/mcp_servers/docs_catalog.py verify --content-root .
python3 .setup/site/mcp_servers/docs_catalog.py check-links --content-root .
```

All links must resolve INSIDE the ai-first-docs repo boundary — never reference the ccdeck parent tree.

## Checkpoint report format (≤400 words each)

- What landed (files + commit SHAs on `migration-protocol`)
- Gate results (the three catalog commands: pass/fail)
- **Compromises bubbled up** — every uncertain call, shortcut, or brief-deviation ("None — clean" only
  if truly clean)
- Open questions with options + your recommendation
