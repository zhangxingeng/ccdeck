# Brief: migrate ccdeck to full harness parity (Track B — dogfood run)

## Why this work exists

ccdeck runs a deliberately minimal agentic setup: flat `ai-first-docs/` corpus, `project_profile.yaml`,
one `get-context` agent, a single `.claude/memory/MEMORY.md`, no root CLAUDE.md, no skills, no hooks.
The user chose **full harness parity** with the reference implementation (juror_fullstack). This
migration is ALSO the first real run of the just-written migration protocol — you are dogfooding it,
and anything the protocol gets wrong or leaves unclear is a finding to bubble up (the lead feeds those
back to the protocol's branch). Everything already decided by the user is in "Locked decisions" below —
do not re-litigate those; everything else follows the protocol.

## Governing protocol — read first, follow its flow

`ai-first-docs/craft/docs/agentic_system_migration_protocol.mdx` (on the corpus's `migration-protocol`
branch, already checked out). Its flow IS your checkpoint structure: audit → migration plan (gated by
the lead, who holds the user's standing approvals) → execute md-first → merge upstream is OUT OF SCOPE
this run (flag candidates in the plan instead; the lead owns upstream) → verify with the loads-check.

## Repo and git discipline

- All work in the **ccdeck repo** (`/home/shane/workspace/ccdeck`), branch `harness-parity`, already
  checked out. Never commit into `ai-first-docs/` — it is a separate nested git repo on its own branch;
  `git -C /home/shane/workspace/ccdeck …` for every git op if there is any doubt about the root.
- The working tree carries two pre-existing uncommitted changes that are part of this migration:
  `.mcp.json` (docs MCP wiring, new) and `.claude/settings.json` (enabledPlugins addition, note its
  broken indentation — you will rewrite settings.json anyway). Fold them into your appropriate commits.
- Conventional Commits, cohesive commit per slice, commit before every checkpoint report, corrections
  are new commits (never amend/reset — the lead may have already read what you'd be rewriting).
- Do not push; the lead pushes after the final gate.

## Orientation

Dispatch `get-context` with your own simulation of this task, then read what it floors. Beyond the
governing protocol you will certainly need: `craft/memory/agent_memory_protocol.mdx` (three-layer
model, the MEMORY.md section shapes, the autoMemoryDirectory-in-settings.local gotcha),
`stack/claude-code/skill_protocol.mdx` (skills-as-docs, thin SKILL.md, harness-vs-project split),
`stack/claude-code/hook_protocol.mdx` (three-verdict contract, fail-open, hook_lib pattern),
`stack/claude-code/setup_protocol.mdx` (settings tiers, MCP registration, secret-safe reads).

## The reference implementation (adapt, never blind-copy)

juror_fullstack at `/home/shane/workspace/juror_fullstack` — read these as the shapes to adapt:

| Reference | What to take |
| - | - |
| `CLAUDE.md` | the three-layer standing orientation + append-only instruction-candidate inbox shape |
| `.claude/memory/MEMORY.md` | section organization: override banner, spirit/tier rules, orientation ritual, always-on rails, routers, candidates inbox |
| `.claude/system_prompt_append.md` | the harness-integrity read-only guard |
| `.claude/settings.json` | hooks wiring shape, explicit enabled/disabledMcpjsonServers, secrets deny rules |
| `.claude/hooks/hook_lib.py`, `pre-edit-reminder.py`, `mask-secrets.py` + their `test_*.py` | the port sources — keep uv-run-script, fail-open, unit-tested |
| `doc_site/docs/skills/skill-sync/` (`skill_sync.py` + SKILL.md) | the sync mechanism to port |
| `.claude/skills/caveman/`, `claude-settings/`, `claude-workspace/` | the harness trio: copy as REAL dirs (they are project-agnostic by design; strip any juror-specific residue you find) |
| `.claude/agents/doc-maintainer.md`, `memory-organizer.md`, `quality-fix.md` | roster additions — adapt to ccdeck's FLAT layout the same way ccdeck's existing `get-context.md` was adapted (it is your in-repo example of a flat-adapted stub) |
| `.mcp.json` `playwright` entry | the MCP shape (see locked decisions) |

juror is READ-ONLY reference. Where juror and the corpus protocols disagree, the corpus wins; flag the
disagreement.

## Locked decisions (user-approved — the plan records them as verdicts, doesn't reopen them)

- **Skills**: project skills `skill-sync` and `cut-release`, sourced at `project_docs/skills/<name>/SKILL.md`
  with relative symlinks from `.claude/skills/<name>`; harness trio (caveman, claude-settings,
  claude-workspace) as real dirs in `.claude/skills/`. `cut-release` encodes ccdeck's release flow —
  recover it from git history and CONTRIBUTING/CI: bump the THREE version files (package.json,
  src-tauri/tauri.conf.json, src-tauri/Cargo.toml), `cargo update -p ccstudio` for the lockfile, commit,
  tag `vX.Y.Z`, push main + tag, `gh release create` (see the v0.11.0 commits for the exact shape).
- **Hooks**: port `hook_lib`, `pre-edit-reminder`, `mask-secrets` (+ unit tests), and wire skill-sync's
  SessionStart hook. **Do NOT port cmd-enforce** — ccdeck has no command conventions to enforce; an
  empty enforcement hook is the costume anti-pattern. pre-edit-reminder RULES start minimal and real:
  a JSONL-parser/builder edit nudge (tests/edit_roundtrip guards corruption — point at running
  test:smoke) is genuinely useful; invent nothing else without a real trigger. mask-secrets deny/mask
  targets: `~/.claude/.ccstudio-providers-plaintext.json` (the provider-key plaintext fallback) plus
  the generic `.env*` patterns from juror's version.
- **Playwright**: add the `playwright` server to `.mcp.json` — `npx @playwright/mcp --headless
  --isolated --output-dir=.claude/tmp/playwright` (NO --storage-state; ccdeck has no auth). List it in
  `disabledMcpjsonServers` in settings.json (enable-on-demand is the standard; the corpus's
  visual_iteration_protocol says workers use it, not the trunk). `docs` goes in `enabledMcpjsonServers`.
- **check_cmd** in project_profile.yaml gains `&& pnpm run test:smoke` (NOT test:e2e — chromium
  install is too heavy for the agent verify loop; CI covers e2e).
- **ARCHITECTURE.md stays at the repo root** (public-OSS convention, README links it) — the plan
  records it "already home"; only normalize its bare inline paths to markdown links.
- **project_docs/ stays the docs home** (already pure-md, already flat-mode standard).
- **CLAUDE.md three-layer split**: ccdeck's existing MEMORY.md content survives — its five standing
  feedback rules and pointers get re-sorted per the tier model, not rewritten away. The new root
  CLAUDE.md holds only durable disposition (adapt juror's, trim to ccdeck reality); MEMORY.md becomes
  the router shape; `system_prompt_append.md` guards all three.
- **autoMemoryDirectory** moves from tracked settings.json to gitignored `.claude/settings.local.json`
  (the memory protocol's gotcha). Confirm `.gitignore` covers settings.local.json; add if not.
- **Scratch artifacts**: `.claude/work/prompt_report/` and `plans/` residue from past sessions —
  verify each pair is in git history, then `git rm` and commit (commit-before-remove; anything NOT in
  history gets committed first). Do not retire THIS brief's pair — the slice is open.
- **Link normalization**: bare inline paths in project_docs/*.md, MEMORY.md, ARCHITECTURE.md →
  markdown relative links. Content edits beyond links are out of scope EXCEPT dead claims you can
  verify are dead (the protocol's freshness gate) — flag staleness you can't verify in the plan's risks.

## Checkpoints (report, then wait for the lead)

- **CP1 — audit + migration plan** at `.claude/work/plans/migration_plan.md`: full inventory, verdict
  per artifact (locked decisions recorded as pre-approved verdicts), destinations, ordering, risks,
  and any protocol-dogfood findings so far. Commit the plan.
- **CP2 — the always-on layer**: CLAUDE.md, MEMORY.md reconcile, system_prompt_append.md, settings
  split, agents roster. Commit(s).
- **CP3 — skills + hooks**: skills-as-docs infra, the five skills, skill_sync port, hooks + tests,
  SessionStart wiring. Commit(s).
- **CP4 — the rest + full verify**: playwright MCP, check_cmd, link normalization, scratch retirement,
  then the verification battery below. Final report.

## Verification (CP4, each command with an explicit timeout)

1. `pnpm check` (timeout 180000) — 0 errors, 0 warnings.
2. `cargo test --lib --manifest-path src-tauri/Cargo.toml` (timeout 300000) — all pass.
3. `pnpm run test:smoke` (timeout 180000) — all pass.
4. Hook unit tests: `python3 -m pytest .claude/hooks/ -q` or per-file (timeout 120000).
5. **Fail-open exercise** (the standard suites can't cover this): run each hook script with garbage
   stdin and with its dependency unavailable; confirm exit 0 / non-blocking in both. Show the commands
   and outputs in the report.
6. **Loads-check** (protocol's gate): `skill_sync.py --dry-run` reports in-sync; every `.claude/skills/`
   symlink resolves (`find .claude/skills -xtype l` returns nothing); `.mcp.json` parses and the docs
   server launches (`timeout 15 uv run --script ai-first-docs/.setup/site/mcp_servers/load_docs_mcp_server.py` —
   non-instant-crash is the signal); every agent .md has valid frontmatter.
7. `git -C /home/shane/workspace/ccdeck status` clean except this brief's open pair.

## Checkpoint report format (≤400 words)

What landed (files + SHAs) · verification results (CP4) · **protocol-dogfood findings** (anything the
migration protocol got wrong/unclear — the whole point of this run) · compromises bubbled up ·
open questions with recommendation.
