#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.14"
# dependencies = []
# ///
# Regression lock for hook_lib.py — the shared PreToolUse / PostToolUse primitive.
#
# Pins the two contracts every hook leans on: fire_once's once-per-agent semantics
# (first fire fires, repeat stays silent, a distinct agent fires, a new session
# fires — the property that keeps a nudge from bombarding) and the fail-open stdin
# parse. A regression here silently breaks every hook that imports the lib.
#
# Stdlib only (unittest), run via uv (pins python 3.14):
#     uv run --script .claude/hooks/test_hook_lib.py

import importlib.util
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path

_LIB_PATH = Path(__file__).resolve().parent / "hook_lib.py"


def _load_lib():
    spec = importlib.util.spec_from_file_location("hook_lib", _LIB_PATH)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


hook_lib = _load_lib()


def _payload(session_id: str = "s1", agent_id: str = "") -> object:
    return hook_lib.HookPayload(
        event="PreToolUse",
        tool_name="Bash",
        tool_input={"command": "gh issue view 1"},
        tool_response=None,
        session_id=session_id,
        agent_id=agent_id,
    )


class FireOnce(unittest.TestCase):
    """Once per (session, agent, nudge_id); repeats stay silent."""

    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self._orig = hook_lib._STATE_DIR
        hook_lib._STATE_DIR = Path(self._tmp.name) / ".state"

    def tearDown(self):
        hook_lib._STATE_DIR = self._orig
        self._tmp.cleanup()

    def test_first_fire_then_silent(self):
        p = _payload()
        self.assertTrue(hook_lib.fire_once(p, "issue-claim"))
        self.assertFalse(hook_lib.fire_once(p, "issue-claim"))
        self.assertFalse(hook_lib.fire_once(p, "issue-claim"))

    def test_distinct_nudge_id_fires_independently(self):
        p = _payload()
        self.assertTrue(hook_lib.fire_once(p, "issue-claim"))
        self.assertTrue(hook_lib.fire_once(p, "lint"))  # different id -> its own first fire

    def test_distinct_agent_fires_independently(self):
        self.assertTrue(hook_lib.fire_once(_payload(agent_id="a1"), "issue-claim"))
        self.assertTrue(hook_lib.fire_once(_payload(agent_id="a2"), "issue-claim"))
        self.assertFalse(hook_lib.fire_once(_payload(agent_id="a1"), "issue-claim"))

    def test_main_agent_absent_id_keys_on_sentinel(self):
        # agent_id "" (main agent) still dedups — it must not collapse to "fire always".
        self.assertTrue(hook_lib.fire_once(_payload(agent_id=""), "issue-claim"))
        self.assertFalse(hook_lib.fire_once(_payload(agent_id=""), "issue-claim"))

    def test_new_session_fires_afresh(self):
        self.assertTrue(hook_lib.fire_once(_payload(session_id="s1"), "issue-claim"))
        self.assertTrue(hook_lib.fire_once(_payload(session_id="s2"), "issue-claim"))

    def test_fires_open_when_state_dir_unwritable(self):
        # Point the state dir at a path under a *file* so mkdir fails -> fire (not swallow).
        blocker = Path(self._tmp.name) / "blocker"
        blocker.write_text("x")
        hook_lib._STATE_DIR = blocker / "cannot" / ".state"
        self.assertTrue(hook_lib.fire_once(_payload(), "issue-claim"))


class ReadPayload(unittest.TestCase):
    """Fail-open parse — any unexpected shape returns None, valid input parses."""

    def _read(self, stdin_text: str) -> object:
        orig = sys.stdin
        sys.stdin = io.StringIO(stdin_text)
        try:
            return hook_lib.read_payload()
        finally:
            sys.stdin = orig

    def test_valid_payload_parses(self):
        p = self._read(json.dumps({
            "hook_event_name": "PreToolUse",
            "tool_name": "Bash",
            "tool_input": {"command": "ls"},
            "session_id": "s1",
            "agent_id": "a1",
        }))
        self.assertEqual(p.tool_name, "Bash")
        self.assertEqual(p.tool_input["command"], "ls")
        self.assertEqual(p.agent_key(), "a1")

    def test_absent_agent_id_defaults_to_sentinel(self):
        p = self._read(json.dumps({"tool_name": "Bash", "tool_input": {}}))
        self.assertEqual(p.agent_id, "")
        self.assertEqual(p.agent_key(), "main-agent")

    def test_non_dict_tool_input_becomes_empty(self):
        p = self._read(json.dumps({"tool_name": "Bash", "tool_input": "oops"}))
        self.assertEqual(p.tool_input, {})

    def test_garbage_returns_none(self):
        self.assertIsNone(self._read("not json{"))

    def test_json_array_returns_none(self):
        self.assertIsNone(self._read("[1, 2, 3]"))


class ShellSegments(unittest.TestCase):
    """Quoted args stay one token; operators split; unparsable fails open to []."""

    def test_operator_split(self):
        self.assertEqual(
            hook_lib.shell_segments("cd x && gh issue view 1"),
            [["cd", "x"], ["gh", "issue", "view", "1"]],
        )

    def test_quoted_operator_not_split(self):
        # a `&&`/newline inside a quoted commit body is one token, never a separator.
        segs = hook_lib.shell_segments('git commit -m "a && b\nc"')
        self.assertEqual(len(segs), 1)

    def test_unbalanced_quote_fails_open(self):
        self.assertEqual(hook_lib.shell_segments('git commit -m "oops'), [])

    def test_basename(self):
        self.assertEqual(hook_lib.basename(".venv/bin/pytest"), "pytest")
        self.assertEqual(hook_lib.basename("gh"), "gh")


class EmitShapes(unittest.TestCase):
    """The wire shapes: nudge envelope on stdout, block reason on stderr + rc 2."""

    def _capture_stdout(self, fn) -> str:
        orig = sys.stdout
        sys.stdout = io.StringIO()
        try:
            fn()
            return sys.stdout.getvalue()
        finally:
            sys.stdout = orig

    def test_nudge_envelope(self):
        out = self._capture_stdout(lambda: hook_lib.emit_nudge("hello"))
        env = json.loads(out)
        self.assertEqual(env["hookSpecificOutput"]["hookEventName"], "PreToolUse")
        self.assertEqual(env["hookSpecificOutput"]["additionalContext"], "hello")

    def test_nudge_joins_and_skips_empty(self):
        out = self._capture_stdout(lambda: hook_lib.emit_nudge("a", "", "b"))
        self.assertEqual(json.loads(out)["hookSpecificOutput"]["additionalContext"], "a\n\nb")

    def test_nudge_event_name_overrides_default(self):
        # A Stop hook must stamp hookEventName="Stop" so the envelope names the
        # event it fired on; additionalContext is honored on Stop without a
        # `decision` field (non-blocking turn-end context injection).
        out = self._capture_stdout(lambda: hook_lib.emit_nudge("hi", event_name="Stop"))
        env = json.loads(out)
        self.assertEqual(env["hookSpecificOutput"]["hookEventName"], "Stop")
        self.assertEqual(env["hookSpecificOutput"]["additionalContext"], "hi")

    def test_nudge_all_empty_emits_nothing(self):
        self.assertEqual(self._capture_stdout(lambda: hook_lib.emit_nudge("", "")), "")

    def test_replacement_envelope(self):
        out = self._capture_stdout(lambda: hook_lib.emit_replacement("masked"))
        env = json.loads(out)
        self.assertEqual(env["hookSpecificOutput"]["hookEventName"], "PostToolUse")
        self.assertEqual(env["hookSpecificOutput"]["updatedToolOutput"], "masked")

    def test_block_returns_2(self):
        orig = sys.stderr
        sys.stderr = io.StringIO()
        try:
            self.assertEqual(hook_lib.emit_block("nope"), 2)
        finally:
            sys.stderr = orig


if __name__ == "__main__":
    unittest.main(verbosity=2)
