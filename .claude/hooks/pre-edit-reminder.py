#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.14"
# dependencies = []
# ///
# PreToolUse hook for Edit/Write — emits a per-slice protocol-loading reminder.
#
# Pattern-matches tool_input.file_path against the file types this repo has
# protocols/guards for and nudges the matching reminders into context. Never
# blocks. The discipline it backstops lives in the memory harness; this is the
# mechanical reminder layer. To extend: add a (pattern, message) row to RULES —
# but only for a REAL trigger with a real target; a speculative rule is noise the
# agent learns to tune out. Verdict wire shape and the fail-open parse live in
# hook_lib — see ai-first-docs/stack/claude-code/hook_protocol.mdx.

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import hook_lib

MASTER_TIP = (
    "Hook reminder, do not respond. Load the docs below before editing if not "
    "already loaded this slice; otherwise ignore."
)

# (regex, message). Every matching pattern fires.
RULES: list[tuple[re.Pattern[str], str]] = [
    (
        re.compile(r"/\.claude/memory/MEMORY\.md$"),
        "MEMORY.md edit — load the agent memory protocol "
        "(ai-first-docs/craft/memory/agent_memory_protocol.mdx): curated sections "
        "are read-only mid-task; new jots go in the candidates inbox",
    ),
    (
        re.compile(r"/src/lib/(parser|builder|editDraft|sessionOps)\.(ts|js)$"),
        "JSONL parse/build/edit surface — this code must round-trip losslessly "
        "(silent corruption reads as fine in a code review; issue #13 shipped two "
        "such bugs). Run `pnpm run test:smoke` (tests/edit_roundtrip_smoke.mjs is "
        "the corruption guard) before committing changes here",
    ),
]


def main() -> int:
    payload = hook_lib.read_payload()
    if payload is None:
        return 0
    file_path = payload.tool_input.get("file_path")
    if not isinstance(file_path, str) or not file_path:
        return 0

    reminders = [msg for pattern, msg in RULES if pattern.search(file_path)]
    if not reminders:
        return 0

    hook_lib.emit_nudge(MASTER_TIP + "\n\n" + "\n".join(f"- {r}" for r in reminders))
    return 0


if __name__ == "__main__":
    sys.exit(main())
