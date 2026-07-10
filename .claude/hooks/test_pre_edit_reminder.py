#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.14"
# dependencies = []
# ///
# Regression lock for pre-edit-reminder.py — the Edit/Write protocol-reminder
# nudge. Pins the verdict matrix: each RULES row fires on its surface, unrelated
# paths stay silent, and garbage stdin fails open (exit 0, no output) — a hook
# that exits non-zero on a wrong path wedges the session.
#
# Stdlib only (unittest), run via uv (or pytest over the hooks dir):
#     uv run --script .claude/hooks/test_pre_edit_reminder.py

import json
import subprocess
import sys
import unittest
from pathlib import Path

_HOOK_PATH = Path(__file__).resolve().parent / "pre-edit-reminder.py"


def _run(stdin_text: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(_HOOK_PATH)],
        input=stdin_text,
        capture_output=True,
        text=True,
        check=False,
    )


def _edit_payload(file_path: str) -> str:
    return json.dumps({
        "hook_event_name": "PreToolUse",
        "tool_name": "Edit",
        "tool_input": {"file_path": file_path},
        "session_id": "s1",
    })


def _context(proc: subprocess.CompletedProcess) -> str:
    env = json.loads(proc.stdout)
    return env["hookSpecificOutput"]["additionalContext"]


class NudgeMatrix(unittest.TestCase):
    def test_memory_md_edit_nudges_memory_protocol(self):
        proc = _run(_edit_payload("/repo/.claude/memory/MEMORY.md"))
        self.assertEqual(proc.returncode, 0)
        self.assertIn("agent_memory_protocol", _context(proc))

    def test_jsonl_surface_edit_nudges_smoke_suite(self):
        for name in ("parser", "builder", "editDraft", "sessionOps"):
            with self.subTest(name=name):
                proc = _run(_edit_payload(f"/repo/src/lib/{name}.ts"))
                self.assertEqual(proc.returncode, 0)
                self.assertIn("test:smoke", _context(proc))

    def test_unrelated_src_lib_file_is_silent(self):
        proc = _run(_edit_payload("/repo/src/lib/theme.ts"))
        self.assertEqual(proc.returncode, 0)
        self.assertEqual(proc.stdout, "")

    def test_unrelated_file_is_silent(self):
        proc = _run(_edit_payload("/repo/README.md"))
        self.assertEqual(proc.returncode, 0)
        self.assertEqual(proc.stdout, "")


class FailOpen(unittest.TestCase):
    def test_garbage_stdin_exits_zero_silent(self):
        proc = _run("not json{")
        self.assertEqual(proc.returncode, 0)
        self.assertEqual(proc.stdout, "")

    def test_missing_file_path_exits_zero_silent(self):
        proc = _run(json.dumps({"tool_name": "Edit", "tool_input": {}}))
        self.assertEqual(proc.returncode, 0)
        self.assertEqual(proc.stdout, "")


if __name__ == "__main__":
    unittest.main(verbosity=2)
