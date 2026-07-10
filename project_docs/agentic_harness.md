# ccdeck's Agentic Harness

What the harness is wired as in THIS repo — the bindings, not the doctrine. Each surface's
governing contract lives in the corpus ([ai-first-docs/](../ai-first-docs/), a separate nested git
repo); this doc carries only what is ccdeck-specific, so it stays true when the doctrine evolves.

## The three always-on layers

| Layer | File | Holds |
|-|-|-|
| Stable disposition | [CLAUDE.md](../CLAUDE.md) | Evergreen principles; the harness-integrity rule; the fresh-clone memory setup step |
| Router + volatile state | [.claude/memory/MEMORY.md](../.claude/memory/MEMORY.md) | Orientation, always-on project rails, doc routers, in-flight plans, candidates inbox |
| Integrity backstop | [.claude/system_prompt_append.md](../.claude/system_prompt_append.md) | Autoloaded by Claude Code from this path (verified live 2026-07-09); duplicates the integrity rule so it survives a session that misreads CLAUDE.md — sanctioned, don't de-duplicate |

Contract: `ai-first-docs/craft/memory/agent_memory_protocol.mdx`. Memory injection depends on a
gitignored `.claude/settings.local.json` (`autoMemoryDirectory`) — the tracked settings.json
silently ignores that key, so a fresh clone must run the one-time step in CLAUDE.md or sessions
load no project memory and nothing errors.

## Settings tiers

- [.claude/settings.json](../.claude/settings.json) (tracked, policy): secret-read deny rules,
  explicit `enabledMcpjsonServers` / `disabledMcpjsonServers`, the hooks block, plugins.
- `.claude/settings.local.json` (gitignored, per-machine): `autoMemoryDirectory` only.
- Placement/precedence judgment: the `claude-settings` skill.

## Skills

Two categories (`ai-first-docs/stack/claude-code/skill_protocol.mdx`):

- **Project skills** — source at `project_docs/skills/<name>/SKILL.md`, exposed by relative
  symlinks in `.claude/skills/`: [skill-sync](skills/skill-sync/SKILL.md) (the sync mechanism
  itself; a SessionStart hook runs it fail-open on every session start),
  [cut-release](skills/cut-release/SKILL.md) (ccdeck's tag-driven release flow).
- **Harness skills** — real dirs in `.claude/skills/`, never touched by sync: `caveman`,
  `claude-settings`, `claude-workspace` (which also carries `work_artifacts.py`, the
  prompt/report-pair retire tool).

Adding a project skill: create `project_docs/skills/<name>/SKILL.md`, run
`uv run --script project_docs/skills/skill-sync/skill_sync.py`, restart the session (the loader
scans at startup).

## Hooks

All in [.claude/hooks/](../.claude/hooks/), single-file `uv run --script`, fail-open, unit-tested
(`uv run --script .claude/hooks/test_<name>.py`). Contract:
`ai-first-docs/stack/claude-code/hook_protocol.mdx`.

| Hook | Events | Job |
|-|-|-|
| `pre-edit-reminder.py` | PreToolUse Edit\|Write | Nudges the memory protocol on MEMORY.md edits and the smoke suite on JSONL parse/build/edit surfaces — never blocks |
| `mask-secrets.py` | PreToolUse + PostToolUse Read\|Bash | Hard-blocks reads of the provider-key plaintext fallback and dotenvs; best-effort masks `KEY=value` in other output |
| `skill_sync.py --hook` | SessionStart (startup) | Self-heals the skill symlinks after a clone/pull |

`hook_lib.py` is the shared primitive (payload parse, verdicts, fire-once dedup under
`.claude/hooks/.state/`, gitignored). There is deliberately no command-enforcement hook — ccdeck
has no command conventions to enforce.

## Agents

Flat-adapted stubs in [.claude/agents/](../.claude/agents/): `get-context` (context router — the
adaptation pattern the others follow), `doc-maintainer`, `memory-organizer`, `quality-fix`. All
`model: sonnet` (profile `audited_providers`).

## MCP servers

Defined in [.mcp.json](../.mcp.json), toggled explicitly in settings.json: `docs` (enabled — the
corpus catalog server), `playwright` (disabled — enable on demand for visual-iteration work; a
disabled server costs zero tokens).

## Verify

The profile's `check_cmd` ([project_profile.yaml](../project_profile.yaml)) is the full suite.
The migrated-harness loads-check: skill-sync `--dry-run` reports in sync, `find .claude/skills
-xtype l` returns nothing, `.mcp.json` parses and the docs server launches, every agent stub has
valid frontmatter.
