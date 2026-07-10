# ccdeck Harness-Parity Migration Plan

Work artifact (post-ship exhaust — retire at close-out). Governing protocol:
[agentic_system_migration_protocol](../../../ai-first-docs/craft/docs/agentic_system_migration_protocol.mdx)
(on the corpus's `migration-protocol` branch). Reference implementation: `juror_fullstack`
(read-only; corpus wins on any disagreement). Locked decisions from the user are recorded below as
**pre-approved verdicts** — they are not reopened here.

## 1. Inventory + verdict per artifact

Classification vocabulary: the unfolding protocol's role sort + skill_protocol's cost model.
"Is → should-be" delta is the migration item; matching answers = already home.

### Existing artifacts

| Artifact | Is | Should be (verdict) | Destination | CP |
|-|-|-|-|-|
| `.claude/agents/get-context.md` | Flat-adapted agent stub, in use | **Already home** — it is the in-repo example the new roster adaptations follow | no move | — |
| `.claude/memory/MEMORY.md` | Single always-on file doing all three layers' jobs (override banner + editing rules + 5 standing feedback rules) | **Re-sort per tier model** (pre-approved): content survives, file becomes the router shape (sections per memory protocol); universal disposition moves to new CLAUDE.md; duplicated-in-corpus items become pointers | same path, rewritten | CP2 |
| — MEMORY rule: expensive-plans/cheap-builds | Tier-1 project rail (dispatch tiering) | Keep inline, caveman-compressed | MEMORY.md §rails | CP2 |
| — MEMORY rule: kit flat-mode / fix-at-root-cause | Tier-1 project rail | Keep inline, compressed | MEMORY.md §rails | CP2 |
| — MEMORY rule: latest-toolchain | User preference, generic-leaning | Keep inline compressed; **flag upstream candidate** (user_preferences_reference) | MEMORY.md | CP2 |
| — MEMORY rule: round-trip-test for corruption bugs | Generic testing methodology | Keep compressed; **flag upstream candidate** (corpus testing doc) — merge-upstream is out of scope this run | MEMORY.md | CP2 |
| — MEMORY rule: cut-unused-features | Duplicate — full writeup already in corpus `user_preferences_reference` | Tier-2 pointer, duplicate prose deleted; **fix dead path** (`craft/team/` → `craft/workflow/`, verified dead) | MEMORY.md pointer | CP2 |
| Root `CLAUDE.md` | **Missing** — the stable always-on layer doesn't exist | Create (pre-approved): durable disposition adapted from juror's, trimmed to ccdeck; three-layer orientation + append-only instruction-candidate inbox | `/CLAUDE.md` | CP2 |
| `.claude/system_prompt_append.md` | Missing | Create (pre-approved): harness-integrity read-only guard per juror shape | `.claude/system_prompt_append.md` | CP2 |
| `.claude/settings.json` | Tracked; `autoMemoryDirectory` set here — **silently ignored** by the harness (verified: memory currently loads from the global `~/.claude/projects/<slug>/memory/` default, NOT the repo file — the repo MEMORY.md banner's claim is dead); `enabledPlugins` block mis-indented | Rewrite (pre-approved): permissions deny rules (secrets), `enabledMcpjsonServers: ["docs"]`, keep `enabledPlugins`; hooks block lands with the hook scripts (CP3), `disabledMcpjsonServers: ["playwright"]` lands with the playwright entry (CP4) — merge-don't-clobber applied to my own sequencing | same path | CP2–CP4 |
| `autoMemoryDirectory` | In tracked settings.json (ignored) | Move to gitignored `.claude/settings.local.json` (pre-approved; the memory protocol's gotcha). `.gitignore` already covers `settings.local.json` — verified. File is per-machine: create locally, don't commit; document the fresh-clone setup step in CLAUDE.md | `.claude/settings.local.json` | CP2 |
| `.mcp.json` | Untracked, new (docs MCP wiring, pre-existing working-tree change) | Commit as-is at CP2 (settings references the `docs` server); playwright entry added CP4 (pre-approved) | same path | CP2, CP4 |
| `project_profile.yaml` | `check_cmd` lacks smoke tests | `check_cmd` gains `&& pnpm run test:smoke` (pre-approved; NOT test:e2e). `test:smoke` verified present in package.json | same path | CP4 |
| `ARCHITECTURE.md` | Root doc, bare inline paths | **Already home** (pre-approved: public-OSS convention, README links it); normalize bare paths → markdown links only | same path | CP4 |
| `project_docs/{roadmap,search-design,multi_agent_bindings}.md` + `hero.png` | Docs home, pure-md, flat-mode standard | **Already home** (pre-approved); normalize bare inline paths → relative markdown links; fix verified-dead `craft/team/user_preferences_reference` path in roadmap.md; content edits otherwise out of scope | same paths | CP4 |
| `.claude/work/plans/{appconfig_relaunch,settings_search_edit}_handoff.md` + 8 `prompt_report/` files (2 closed slices) | Tracked scratch from shipped sessions | Retire (pre-approved): all 10 **verified in git history** → `git rm` + commit | deleted | CP4 |
| `.claude/work/prompt_report/harness_parity_prompt.md` (+ my `get_context_harness_parity.md`) | THIS brief's open pair | Keep untracked until slice closes (pre-approved) | — | — |
| `.claude/work/prompt_report/migration_docs_prompt.md` + `get_context_ccdeck_migration_dogfood.md` | **Track A's / the lead's** open-slice files, untracked, not in history | Not mine to retire or commit — leave; lead owns their close-out. Named here so CP4's "clean except this brief's pair" gate is honest about them | — | open Q1 |
| `.claude/worktrees/agent-abc90ab0d85332c53/` (one stray `resume.ts`) | Stale teammate-worktree residue, git-excluded via `.git/info/exclude` | Retire: confirm not in `git worktree list`, then delete dir. Another agent's leftover → deletion needs lead's go-ahead (destructive-op consent) | deleted | CP4, open Q2 |

### New artifacts (locked roster)

| Artifact | Source | Adaptation | CP |
|-|-|-|-|
| Agents `doc-maintainer`, `memory-organizer`, `quality-fix` | juror `.claude/agents/*.md` | Adapt to ccdeck's FLAT layout the way `get-context.md` was (paths, no doc_site, project_docs home) | CP2 |
| Skills infra | — | `project_docs/skills/<name>/SKILL.md`, relative symlinks from `.claude/skills/<name>` (`../../project_docs/skills/<name>`) | CP3 |
| Skill `skill-sync` | juror `doc_site/docs/skills/skill-sync/` (`skill_sync.py` + SKILL.md) | Port; re-point source root at `project_docs/skills/`; SessionStart hook wiring | CP3 |
| Skill `cut-release` | ccdeck git history (v0.11.0 commits) + CI | Recovered flow: bump 3 version files (package.json, src-tauri/tauri.conf.json, src-tauri/Cargo.toml) → `cargo update -p ccstudio` (lockfile) → commit → tag `vX.Y.Z` → push main + tag → `gh release create`. Note: v0.10/v0.11 tags absent locally — verify tag/release mechanics against `gh release list` at CP3 | CP3 |
| Harness trio `caveman`, `claude-settings`, `claude-workspace` | juror `.claude/skills/` | Copy as REAL dirs (project-agnostic by design); strip any juror residue found | CP3 |
| Hooks `hook_lib.py`, `pre-edit-reminder.py`, `mask-secrets.py` + `test_*.py` | juror `.claude/hooks/` | Port, keep uv-run-script + fail-open + unit-tested. pre-edit-reminder RULES minimal+real: one nudge on JSONL parser/builder edits → run `pnpm run test:smoke` (edit_roundtrip guards corruption); nothing invented. mask-secrets targets: `~/.claude/.ccstudio-providers-plaintext.json` + juror's generic `.env*` patterns. Add `.claude/hooks/.state/` to .gitignore (fire-once markers). **NOT ported: cmd-enforce** (pre-approved — no command conventions; empty enforcement = costume anti-pattern) | CP3 |
| Playwright MCP | juror `.mcp.json` shape | `npx @playwright/mcp --headless --isolated --output-dir=.claude/tmp/playwright`, NO --storage-state; listed in `disabledMcpjsonServers` (pre-approved) | CP4 |
| Project harness doc (**proposed, not locked** — see Q4) | new | Lean `project_docs/agentic_harness.md`: resulting three-layer split, skills-as-docs infra, hooks inventory, settings tiers — the docs-first mandate for a harness behavior change | CP3/CP4 |

## 2. Ordering (dependencies)

1. **CP2** — settings split first (`.mcp.json` commit + settings.json rewrite + settings.local.json), then CLAUDE.md → MEMORY.md reconcile → system_prompt_append.md, then agent roster. MEMORY.md router points only at targets that already exist; no pointers to CP3 artifacts needed (skills self-surface via triggers).
2. **CP3** — skills infra before skill-sync (sync needs the source tree); hooks + tests; settings.json gains hooks block + SessionStart **in the same commit as the scripts** (never wire a hook whose script doesn't exist).
3. **CP4** — playwright + `disabledMcpjsonServers`, check_cmd, link normalization, scratch retirement, verification battery.
4. Re-read CLAUDE.md/MEMORY.md/settings.json/.mcp.json/project_profile.yaml immediately before each edit (repeat-touched files; Track A runs concurrently).

Commit discipline: Conventional Commits, cohesive commit per slice, commit before every checkpoint report, corrections are new commits (never amend). No push (lead pushes).

## 3. Risks

- **Memory injection is currently broken** (autoMemoryDirectory ignored in tracked settings.json) — the migration fixes it, but the fix (settings.local.json) is per-machine and gitignored: a fresh clone silently loses memory injection unless the setup step is followed. Mitigation: CLAUDE.md carries the one-time step (CLAUDE.md itself IS auto-injected regardless).
- **Orphaned global memory**: `~/.claude/projects/-home-shane-workspace-ccdeck/memory/` (MEMORY.md + opus-plans-sonnet-builds.md) is what sessions load today; after the local override lands it goes dark but stays on disk. Outside the repo → **escalate to lead** (Q3), not a repo commit.
- **system_prompt_append.md has no injection path in ccdeck** — juror injects it via its `ai` launcher; ccdeck has no launcher. The file lands per the locked decision, but until a launcher (or `--append-system-prompt` wiring) exists it protects nothing by itself; CLAUDE.md carries the same rule as disposition (the sanctioned duplication). Flagged, not silently absorbed (Q5).
- **Settings changes don't affect the running session** — hooks/MCP toggles load at session start; loads-check at CP4 can verify parsing/launch but not in-session firing. Fail-open exercise covers the hook side.
- **retire-nudge.py** (juror Stop-hook nudging work-artifact retirement) is in juror's hook set but NOT in the locked port list — treated as out of scope; flagged as a follow-up candidate (Q6).
- Staleness I can't verify cheaply: roadmap.md is 49K of shipped-phase narrative; only its dead paths get fixed (freshness gate applied to links, not a content audit).

## 4. Protocol-dogfood findings (so far)

1. **The always-on-layer hint only covers over-accretion** (mega-CLAUDE.md, fragmented memory). ccdeck is the inverse: *missing* layers (no CLAUDE.md, no append-file) with one MEMORY.md doing every job. The tier-sort still re-homes it, but the protocol never names the missing-layer case — worth a sentence in the hints.
2. **The loads-check omits memory injection.** Skills/agents/MCP/symlinks are checked, but the one always-on artifact whose discovery failure is silent (autoMemoryDirectory silently ignored → memory loads from the wrong place) isn't in the verify gate. ccdeck's audit found exactly this failure live.
3. **The append-file's unfold is unspecified for launcher-less projects.** The memory protocol says it "reaches only `ai`-launched sessions"; the migration protocol re-homes content into it without saying what a project with no launcher does.
4. **Audit-as-dispatched-agent vs teammate-does-it**: the protocol prescribes "one dispatched agent" for the audit; in this run the gated teammate (me) did audit+plan as CP1, which preserved the one-reader-sees-all property anyway. The protocol could say the property (one reader holds the whole inventory), not the mechanism (a dispatch).

## 5. Open questions (recommendation first)

- **Q1** — Track A's open files in ccdeck's `.claude/work/` (migration_docs_prompt.md, dogfood get-context file): leave untracked for the lead's close-out (recommended), or commit alongside my pairs?
- **Q2** — Stale `.claude/worktrees/agent-*/` residue: OK to delete at CP4 after `git worktree list` confirms it's orphaned? (Recommended: yes — it's excluded, empty of value, one stray file.)
- **Q3** — Orphaned global memory dir on this machine: want me to fold its content into the repo MEMORY.md at CP2 and leave the global files for you to delete (recommended), or ignore entirely? (Its "Opus plans, sonnet builds" line duplicates the expensive-plans rail that's staying.)
- **Q4** — Proposed lean `project_docs/agentic_harness.md` (docs-first mandate: the harness behavior changes should land with a governing doc). Recommended: yes, one lean doc at CP3/CP4. Not in the brief — needs your verdict.
- **Q5** — system_prompt_append.md wiring: file lands regardless; do you want a repo-local launcher script (out of brief scope) or is user-side wiring your own follow-up? (Recommended: your follow-up; I note the gap in CLAUDE.md.)
- **Q6** — retire-nudge.py: skip this run (recommended, not locked-in), or add to the CP3 port list?
