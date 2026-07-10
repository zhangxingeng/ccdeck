#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.14"
# dependencies = []
# ///
# Regression lock for mask-secrets.py — the Read/Bash secret-masking + hard-block
# hook. A load-bearing security surface: pin both the masking (aggressive
# secret-file mode incl. the providers-fallback JSON shape, content-net mode, URL
# creds) and the PreToolUse hard-block (fires on a real secret-file read, never on
# a mere mention).
#
# Stdlib only (unittest), run via uv (or pytest over the hooks dir):
#     uv run --script .claude/hooks/test_mask_secrets.py

import importlib.util
import json
import os
import subprocess
import sys
import unittest
from pathlib import Path

_HOOK_PATH = Path(__file__).resolve().parent / "mask-secrets.py"
_SETTINGS_PATH = _HOOK_PATH.parent.parent / "settings.json"
_PROJECT_DIR = _HOOK_PATH.parent.parent.parent


def _load_hook():
    spec = importlib.util.spec_from_file_location("mask_secrets", _HOOK_PATH)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


hook = _load_hook()


class SecretFileDetection(unittest.TestCase):
    """ccdeck's secret-file globs: providers-fallback JSON + dotenvs, example excluded."""

    def test_providers_plaintext_fallback(self):
        self.assertTrue(hook.SECRET_FILE_RE.search(
            "/home/u/.claude/.ccstudio-providers-plaintext.json"))

    def test_bare_env_and_variants(self):
        self.assertTrue(hook.SECRET_FILE_RE.search("cat .env"))
        self.assertTrue(hook.SECRET_FILE_RE.search("src/.env.local"))
        self.assertTrue(hook.SECRET_FILE_RE.search(".env.production"))

    def test_example_and_lookalikes_excluded(self):
        self.assertFalse(hook.SECRET_FILE_RE.search(".env.example"))
        self.assertFalse(hook.SECRET_FILE_RE.search(".envrc"))
        self.assertFalse(hook.SECRET_FILE_RE.search("providers.env"))
        self.assertFalse(hook.SECRET_FILE_RE.search("my.envelope"))


class AggressiveMasking(unittest.TestCase):
    """Secret-file mode masks every assignment value, keeping keys/structure."""

    def test_masks_every_dotenv_value(self):
        text = "API_TOKEN=abc123def\nDB_HOST=localhost\n# a comment\n"
        masked, count = hook.mask_text(text, aggressive=True)
        self.assertIn("API_TOKEN=***", masked)
        self.assertIn("DB_HOST=***", masked)
        self.assertIn("# a comment", masked)  # comment untouched
        self.assertEqual(count, 2)

    def test_masks_providers_json_lines(self):
        # The ccstudio fallback is JSON — quoted keys, `: "value",` shape.
        text = '{\n  "anthropic_api_key": "sk-ant-verylongkey123",\n  "label": "work"\n}\n'
        masked, count = hook.mask_text(text, aggressive=True)
        self.assertIn('"anthropic_api_key": ***', masked)
        self.assertNotIn("sk-ant-verylongkey123", masked)
        self.assertEqual(count, 2)  # aggressive masks label too — every value is suspect

    def test_yaml_list_and_mapping_shapes(self):
        masked, _ = hook.mask_text("  - PW=hunter2secret\n  DSN: postgres-here\n", aggressive=True)
        self.assertIn("- PW=***", masked)
        self.assertIn("DSN: ***", masked)


class ContentNetMasking(unittest.TestCase):
    """Non-secret-file output masks only secret-named keys with literal values."""

    def test_masks_secret_named_literal(self):
        masked, count = hook.mask_text("MY_API_KEY=supersecretvalue\n", aggressive=False)
        self.assertIn("MY_API_KEY=***", masked)
        self.assertEqual(count, 1)

    def test_masks_json_secret_line_with_trailing_comma(self):
        masked, count = hook.mask_text('  "session_token": "abcdef123456",\n', aggressive=False)
        self.assertIn('"session_token": ***', masked)
        self.assertEqual(count, 1)

    def test_leaves_source_code_alone(self):
        src = 'API_KEY = os.getenv("X")\n'
        masked, count = hook.mask_text(src, aggressive=False)
        self.assertEqual(masked, src)
        self.assertEqual(count, 0)

    def test_leaves_non_secret_key_alone(self):
        _, count = hook.mask_text("DB_HOST=some-hostname\n", aggressive=False)
        self.assertEqual(count, 0)

    def test_url_credential_masked_without_assignment(self):
        masked, count = hook.mask_text("redis://:hunter2pw@cache:6379\n", aggressive=False)
        self.assertIn(":***@cache", masked)
        self.assertEqual(count, 1)


class CommandReadsSecretFile(unittest.TestCase):
    """The executable-aware check: a read-verb + secret-file arg, not a mention."""

    def test_cat_env(self):
        self.assertTrue(hook._command_reads_secret_file("cat .env"))

    def test_jq_providers_fallback(self):
        self.assertTrue(hook._command_reads_secret_file(
            "jq . ~/.claude/.ccstudio-providers-plaintext.json"))

    def test_nested_bash_c(self):
        self.assertTrue(hook._command_reads_secret_file('bash -c "cat .env.local"'))

    def test_mention_in_commit_message_is_not_a_read(self):
        self.assertFalse(
            hook._command_reads_secret_file('git commit -m "rotate .env.local values"')
        )

    def test_grep_pattern_not_a_read(self):
        # git is not a read-verb; the path is a search arg, not a file it dumps.
        self.assertFalse(hook._command_reads_secret_file("git log --grep .env"))

    def test_example_excluded(self):
        self.assertFalse(hook._command_reads_secret_file("cat .env.example"))


class PreToolUseBlock(unittest.TestCase):
    """End-to-end: a secret-file read blocks (exit 2); anything else allows (0)."""

    def _run(self, payload) -> int:
        proc = subprocess.run(
            [sys.executable, str(_HOOK_PATH)],
            input=json.dumps(payload),
            capture_output=True,
            text=True,
            check=False,
        )
        return proc.returncode

    def test_read_providers_fallback_blocks(self):
        self.assertEqual(
            self._run({
                "hook_event_name": "PreToolUse", "tool_name": "Read",
                "tool_input": {"file_path": "/home/u/.claude/.ccstudio-providers-plaintext.json"},
            }), 2,
        )

    def test_bash_cat_env_blocks(self):
        self.assertEqual(
            self._run({
                "hook_event_name": "PreToolUse", "tool_name": "Bash",
                "tool_input": {"command": "cat .env"},
            }), 2,
        )

    def test_read_env_example_allows(self):
        self.assertEqual(
            self._run({
                "hook_event_name": "PreToolUse", "tool_name": "Read",
                "tool_input": {"file_path": ".env.example"},
            }), 0,
        )

    def test_mention_in_bash_allows(self):
        self.assertEqual(
            self._run({
                "hook_event_name": "PreToolUse", "tool_name": "Bash",
                "tool_input": {"command": 'git commit -m "touch .env.local"'},
            }), 0,
        )

    def test_garbage_stdin_fails_open(self):
        proc = subprocess.run(
            [sys.executable, str(_HOOK_PATH)],
            input="not json{", capture_output=True, text=True, check=False,
        )
        self.assertEqual(proc.returncode, 0)


class DispatchRegistration(unittest.TestCase):
    """Proves Claude Code is STILL wired to invoke the PreToolUse hard-block, not
    just that the script's own logic is right. `PreToolUseBlock` above runs the
    script directly, bypassing settings.json — so it can't see the two real
    regression classes: the settings entry silently dropped/re-matchered, or the
    registered command string drifting (wrong path, wrong interpreter) while the
    script on disk stays fine. Nothing errors when either happens; the hook just
    goes dark. So this class reads the LIVE settings.json, extracts the literal
    registered command for the PreToolUse Read|Bash entry, and runs THAT string
    with a faithful stdin payload — as close to the harness's own dispatch as a
    free, deterministic test gets. (A `claude -p --include-hook-events` run is the
    stronger alternative, but costs an API call and is non-deterministic on
    whether the model attempts the read at all.) Asserts both directions so a
    block-everything hook can't read green.
    """

    def _registered_command(self) -> str:
        settings = json.loads(_SETTINGS_PATH.read_text())
        for entry in settings.get("hooks", {}).get("PreToolUse", []):
            matcher = entry.get("matcher", "")
            if "Read" not in matcher or "Bash" not in matcher:
                continue
            for h in entry.get("hooks", []):
                command = h.get("command", "")
                if "mask-secrets.py" in command:
                    return command
        self.fail(
            "No PreToolUse hook is registered for mask-secrets.py under a "
            "Read|Bash matcher in .claude/settings.json — the hard-block "
            "registration has been dropped, re-matchered, or renamed."
        )

    def _dispatch(self, payload: dict) -> subprocess.CompletedProcess:
        command = self._registered_command()
        env = {**os.environ, "CLAUDE_PROJECT_DIR": str(_PROJECT_DIR)}
        return subprocess.run(
            command, shell=True, input=json.dumps(payload),
            capture_output=True, text=True, check=False, env=env,
        )

    def test_registered_command_blocks_secret_file_read(self):
        proc = self._dispatch({
            "hook_event_name": "PreToolUse", "tool_name": "Read",
            "tool_input": {"file_path": "/home/u/.claude/.ccstudio-providers-plaintext.json"},
        })
        self.assertEqual(
            proc.returncode, 2,
            msg=(
                "the LIVE settings.json registration did not block a "
                f"secret-file read (exit={proc.returncode}, "
                f"stderr={proc.stderr!r}) — the PreToolUse hard-block has "
                "regressed; nothing else errors when this silently stops working."
            ),
        )
        self.assertIn("BLOCKED by mask-secrets.py", proc.stderr)

    def test_registered_command_allows_normal_read(self):
        # Two-sided: a check that only ever fires "blocked" can't tell a
        # working gate from one that blocks everything.
        proc = self._dispatch({
            "hook_event_name": "PreToolUse", "tool_name": "Read",
            "tool_input": {"file_path": "src/lib/parser.ts"},
        })
        self.assertEqual(
            proc.returncode, 0,
            msg=(
                "the LIVE settings.json registration blocked a normal, "
                f"non-secret file read (stderr={proc.stderr!r}) — "
                "over-blocking regression."
            ),
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)
