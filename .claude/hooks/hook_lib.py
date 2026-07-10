# hook_lib.py — shared helpers for Claude Code PreToolUse / PostToolUse hooks.
#
# One home for the contract every hook re-implements: the fail-open stdin parse,
# the three verdict wire shapes (block / nudge / allow), the shlex-first shell
# segmentation a Bash hook uses to find a command's effective executable, and the
# fire-once-per-agent dedup. The authoring contract these encode is documented in
# ai-first-docs/stack/claude-code/hook_protocol.mdx.
#
# Imported (never run) by the sibling hook scripts, so it carries no PEP 723
# header and pulls no dependencies — pure stdlib, running under whatever
# interpreter the importing `uv run --script` hook provides (python >=3.14).

import json
import os
import shlex
import sys
import time
from dataclasses import dataclass
from pathlib import Path

# --- Payload ---------------------------------------------------------------

# agent_id is absent for the top-level agent; this stands in as its dedup key.
_MAIN_AGENT = "main-agent"


@dataclass(frozen=True, slots=True)
class HookPayload:
    """One parsed PreToolUse / PostToolUse stdin payload.

    tool_input holds unknown-until-read values (its shape varies by tool — a
    `command` for Bash, a `file_path` for Edit/Read), so callers pull a key and
    isinstance-check it: the boundary is validated here, each field at its use.
    agent_id is "" when the hook fired from the main agent; agent_key() maps that
    to the sentinel so a dedup key is never empty.
    """

    event: str
    tool_name: str
    tool_input: dict[str, object]
    tool_response: object
    session_id: str
    agent_id: str

    def agent_key(self) -> str:
        return self.agent_id or _MAIN_AGENT


def read_payload() -> HookPayload | None:
    """Parse stdin into a HookPayload, or None on any unexpected shape (fail open).

    The stdin JSON is untyped external input crossing a trust boundary — validate
    once here and return None on anything malformed, so every caller's fail-open
    path is a single `is None` check instead of a repeated defensive dance.
    """
    try:
        raw = json.load(sys.stdin)
    except (json.JSONDecodeError, ValueError):
        return None
    if not isinstance(raw, dict):
        return None
    tool_input = raw.get("tool_input")
    if not isinstance(tool_input, dict):
        tool_input = {}
    return HookPayload(
        event=_as_str(raw.get("hook_event_name")),
        tool_name=_as_str(raw.get("tool_name")),
        tool_input=tool_input,
        tool_response=raw.get("tool_response"),
        session_id=_as_str(raw.get("session_id")),
        agent_id=_as_str(raw.get("agent_id")),
    )


def _as_str(value: object) -> str:
    return value if isinstance(value, str) else ""


# --- Shell parsing ---------------------------------------------------------

# Command-separator operator tokens (matched as WHOLE shlex tokens, so a `|`/`;`
# inside a quoted argument is never a separator). The default set omits lone `&`;
# a hook that must split on backgrounding passes its own set.
DEFAULT_OPERATORS = frozenset({"&&", "||", "|", ";"})


def shell_segments(
    command: str, operators: frozenset[str] = DEFAULT_OPERATORS
) -> list[list[str]]:
    """Split a compound shell command into per-segment token lists.

    shlex-tokenize the WHOLE command first so a quoted argument stays one token —
    a multi-line commit message or heredoc body is never split on an operator or
    newline it merely contains (the newline-in-a-commit-message false positive).
    Then split that token stream on the operator tokens. Returns [] on anything
    shlex can't parse (unbalanced quotes) so the caller fails open. Stripping each
    segment down to its effective executable is the caller's concern, not this.
    """
    try:
        tokens = shlex.split(command)
    except ValueError:
        return []
    segments: list[list[str]] = []
    current: list[str] = []
    for tok in tokens:
        if tok in operators:
            if current:
                segments.append(current)
                current = []
        else:
            current.append(tok)
    if current:
        segments.append(current)
    return segments


def basename(token: str) -> str:
    """`.venv/bin/pytest` -> `pytest`; `pytest` -> `pytest`."""
    return token.rsplit("/", 1)[-1]


# --- Verdict emit ----------------------------------------------------------


def emit_block(reason: str) -> int:
    """BLOCK: write the reason to stderr; return 2 for the caller to exit with.

    The tool call is refused and stderr is fed back to the model as the reason.
    """
    sys.stderr.write(reason + "\n")
    return 2


def emit_nudge(*messages: str, event_name: str = "PreToolUse") -> None:
    """NUDGE: inject additionalContext into the model; the tool call still runs.

    Non-empty messages are joined into one context block. Emits nothing when every
    message is empty, so a caller can pass conditionally-built strings unguarded.

    `hookEventName` must name the firing event: additionalContext is honored on
    PreToolUse/PostToolUse and on Stop (turn-end) — a Stop hook passes
    event_name="Stop" so the envelope matches the event it fired on. Kept as a
    keyword-only arg so every existing positional-message caller is untouched. On
    Stop the injection is non-blocking precisely because no `decision` field is
    emitted; adding one would force another turn, which is a BLOCK, not a nudge.
    """
    context = "\n\n".join(m for m in messages if m)
    if not context:
        return
    json.dump(
        {
            "hookSpecificOutput": {
                "hookEventName": event_name,
                "additionalContext": context,
            }
        },
        sys.stdout,
    )


def emit_replacement(text: str) -> None:
    """PostToolUse: replace the tool output the model sees (updatedToolOutput)."""
    json.dump(
        {
            "hookSpecificOutput": {
                "hookEventName": "PostToolUse",
                "updatedToolOutput": text,
            }
        },
        sys.stdout,
    )


# --- Fire-once-per-agent dedup ---------------------------------------------

# The hook protocol prescribes a gitignored `.state/` beside the hooks for dedup
# markers (per-session runtime state, never a versioned artifact).
_STATE_DIR = Path(__file__).resolve().parent / ".state"
_PRUNE_AGE_SECONDS = 24 * 60 * 60


def fire_once(payload: HookPayload, nudge_id: str) -> bool:
    """True exactly once per (session, agent, nudge_id); False on every repeat.

    Marks via an atomic O_EXCL file create: the create succeeds on the first fire
    and raises FileExistsError on every repeat, so a nudge fires once per agent and
    then stays silent. The create must be atomic because the main agent can issue
    parallel tool calls in one turn — a check-then-write would race itself into
    firing twice. Keyed on session_id (a new session nudges afresh) and the
    per-agent key (each subagent fires independently of its siblings).

    Fails OPEN toward firing: if the state dir can't be written, return True — a
    duplicate nudge is a lesser harm than a silently-dropped one.
    """
    marker = _STATE_DIR / _safe_name(
        f"{payload.session_id}__{payload.agent_key()}__{nudge_id}"
    )
    try:
        _STATE_DIR.mkdir(parents=True, exist_ok=True)
        _prune()
        fd = os.open(marker, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        os.close(fd)
        return True
    except FileExistsError:
        return False
    except OSError:
        return True  # can't persist state -> fire rather than swallow the nudge


def _safe_name(key: str) -> str:
    """A filesystem-safe marker name — keep word chars, fold everything else."""
    return "".join(c if (c.isalnum() or c in "._-") else "_" for c in key)


def _prune() -> None:
    """Best-effort removal of markers older than a day; never raises.

    Markers are ~0-byte files, so accumulation is cheap — but pruning here keeps a
    long-lived checkout from collecting one per (agent, nudge) forever with no
    separate cleanup hook to run.
    """
    cutoff = time.time() - _PRUNE_AGE_SECONDS
    try:
        entries = list(_STATE_DIR.iterdir())
    except OSError:
        return
    for entry in entries:
        try:
            if entry.is_file() and entry.stat().st_mtime < cutoff:
                entry.unlink()
        except OSError:
            continue
