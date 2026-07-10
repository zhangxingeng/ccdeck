#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.14"
# dependencies = []
# ///
# Hook for Read/Bash — keeps raw secret VALUES out of the model's view. Rewrites
# `KEY=value` -> `KEY=***` (keeping the KEY and structure visible) and, on the
# PreToolUse side, hard-blocks a Read/Bash call that reads a known plaintext secret
# file. Layer 1 of the secrets story; the settings.json permissions.deny rules are
# layer 2 (same globs — deliberately redundant, deny globs have had reliability bugs).
#
# Wired into BOTH PreToolUse and PostToolUse (matcher "Read|Bash"). The two sides
# do different jobs, because they must:
#   - PostToolUse output-masking is BEST-EFFORT only. updatedToolOutput is an
#     unreliable rewrite for built-in tools on the current Claude Code version (a
#     confirmed upstream regression, anthropics/claude-code#68951) — it silently
#     no-ops, so a broken mask reads as "nothing to mask." Do not lean on it.
#   - PreToolUse is the hard boundary: it hard-blocks (exit 2) any Read/Bash call
#     targeting a plaintext secret file, using the same precise executable-aware
#     check as the aggressive mask so a mere mention of the path in unrelated text
#     (a commit message, a grep pattern) never trips it. This closes the gap
#     permissions.deny leaves — a `cat`/`sed` read isn't a `Read` tool call.
#
# ccdeck's secret files: ~/.claude/.ccstudio-providers-plaintext.json (the
# provider-key plaintext fallback this app writes when no OS keychain is
# available — real API keys, JSON shape) and generic .env / .env.* files
# (.env.example excluded — it holds placeholders, and blocking it would cost
# agents their one legitimate config-shape reference).
#
# Two masking modes:
#   1. SECRET-FILE (aggressive) — the tool read a plaintext secret file, so every
#      assignment line (`KEY=value`, `KEY: value`, `"key": value`) is a secret;
#      mask them all.
#   2. CONTENT-NET (safety net) — for other output, mask only secret-named keys
#      with literal-looking values (so `env | grep`, `printenv` leak nothing, but
#      real source like `API_KEY = os.getenv("X")` is left alone).
#
# Fail-OPEN on the masking path (parse error -> original output stands) — an
# availability-over-secrecy tradeoff; the PreToolUse block is a real refuse, not
# best-effort. Known bypass classes: a value encoded (base64/xxd) or laundered
# through a non-read-verb wrapper sidesteps the aggressive mask and the block —
# content-net still masks secret-named literals in the output, but not every
# shape. Verdict wire shape and the fail-open parse live in hook_lib — see
# ai-first-docs/stack/claude-code/hook_protocol.mdx.

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import hook_lib

# --- Detection -------------------------------------------------------------

# A plaintext secret file: the ccstudio provider-key fallback JSON, or a dotenv —
# bare `.env` / `.env.<anything>` but NOT `.env.example` (placeholders, not
# secrets). The `(?<!\w)` lookbehind (not `^`/`/`) is deliberate: a real read is
# preceded by whitespace/quote/`=`/`/` (`cat .env`, `~/.claude/...json`), while a
# preceding word char means the dot is part of a longer name (`providers.env`,
# `.envrc` — correctly excluded via the `(?![\w])` after `.env`).
SECRET_FILE_RE = re.compile(
    r"\.ccstudio-providers-plaintext\.json(?![.\w])"
    r"|(?<!\w)\.env(?![\w])(?!\.example(?![.\w]))(?:\.[\w.-]+)?"
)

# Executables that can dump a file's content to stdout/stderr. Deliberately narrow:
# a command whose executable ISN'T here never triggers on a secret-file path merely
# appearing in its arguments. Shell-launchers are included so a nested
# `bash -c "cat .env"` is caught (the quoted inner command is one token). `jq` is
# here because the providers fallback is JSON — jq is its natural read verb.
_READ_VERBS = frozenset(
    {
        "cat", "less", "more", "head", "tail", "sed", "awk", "grep", "egrep",
        "fgrep", "rg", "strings", "xxd", "hexdump", "od", "base64", "python",
        "python3", "node", "perl", "ruby", "source", "cp", "tee", "diff",
        "printenv", "env", "bash", "sh", "zsh", "eval", "jq",
    }
)

# mask's segmentation splits on backgrounding `&` too (unlike the default set).
_SHELL_SEP_TOKENS = frozenset({";", "&&", "||", "|", "&"})


def _command_reads_secret_file(command: str) -> bool:
    """True only if some pipeline segment's executable is a read-verb AND one of
    its own arguments matches SECRET_FILE_RE — i.e. the command actually reads a
    secret file, not merely mentions the path in unrelated text.
    """
    for segment in hook_lib.shell_segments(command, _SHELL_SEP_TOKENS):
        exe = hook_lib.basename(segment[0])
        if exe in _READ_VERBS and any(
            SECRET_FILE_RE.search(tok) for tok in segment[1:]
        ):
            return True
    return False


# An assignment line: leading indent, optional YAML list-item dash (`- `), optional
# `export `, KEY (optionally double-quoted — JSON), then `=` or `:` (YAML/JSON
# mapping), then value. KEY starts with a letter/underscore (so `#` comment lines
# never match). The dash/`:`/quoted forms catch YAML blocks and the providers
# fallback's JSON lines (`"anthropic_api_key": "sk-..."`), not just bare
# `KEY=value`.
ASSIGN_RE = re.compile(
    r"^(\s*(?:-\s+)?(?:export\s+)?\"?[A-Za-z_][A-Za-z0-9_.]*\"?\s*[:=]\s*)(.*)$"
)

# Content-net: a KEY name that signals a credential.
SECRET_KEY_RE = re.compile(
    r"(SECRET|TOKEN|PASSWORD|PASSWD|PWD|APIKEY|API_KEY|ACCESS_KEY|PRIVATE_KEY"
    r"|CREDENTIAL|AUTH|SESSION_KEY|MASTER_?KEY|ENCRYPT_?KEY|SIGNING|_SALT|SALT_"
    r"|CONNECTION_STRING|_DSN|DSN_|CLIENT_SECRET)",
    re.IGNORECASE,
)

# Characters that mean a value is code/shell, not a bare literal secret.
_CODE_CHARS = set("(){}<>$;`\"' \t")

# URL-embedded credentials: `scheme://[user]:PASSWORD@host` -> `...:***@host`. Runs
# on EVERY line so a bare connection string (Postgres DSN, `redis://:pw@host`,
# `https://user:token@github.com`) is masked even with no assignment to key off.
URL_CRED_RE = re.compile(r"(://[^:@/\s]*:)([^@/\s]+)(@)")

MASK = "***"

# Shorter values are too likely to be a flag/enum/boolean to mask on key-name alone.
_MIN_LITERAL_LEN = 6


def _value_looks_literal(value: str) -> bool:
    """True if `value` looks like a bare secret literal, not a code expression.

    The trailing-comma strip is for JSON lines (`"key": "sk-abc",`) — without it
    the comma keeps the closing quote in the string and the quote chars read as
    code punctuation, silently exempting every non-final JSON entry.
    """
    v = value.strip().rstrip(",").strip("\"'")
    if len(v) < _MIN_LITERAL_LEN:
        return False
    return not any(c in _CODE_CHARS for c in v)


def _mask_line(line: str, aggressive: bool) -> tuple[str, bool]:
    """Mask one line's value; return (line, changed?).

    aggressive=True (secret-file mode): mask every non-empty assignment value.
    aggressive=False (content-net): mask only secret-named keys with literal values.
    """
    m = ASSIGN_RE.match(line)
    if not m:
        return line, False
    key_part, value = m.group(1), m.group(2)
    if not value.strip() or value.strip() == MASK:
        return line, False  # nothing there, or already masked
    if aggressive:
        return key_part + MASK, True
    if SECRET_KEY_RE.search(key_part) and _value_looks_literal(value):
        return key_part + MASK, True
    return line, False


def _mask_url_creds(line: str) -> tuple[str, bool]:
    """Mask any `scheme://user:password@host` credential; return (line, changed?)."""
    new = URL_CRED_RE.sub(r"\1***\3", line)
    return new, new != line


def mask_text(text: str, aggressive: bool) -> tuple[str, int]:
    """Mask every applicable assignment line + any URL-embedded credential.

    Each line goes through two passes: the `KEY=value` assignment mask, then a
    URL-credential mask that catches connection strings with no assignment shape.
    Returns (text, count_masked).
    """
    if not text:
        return text, 0
    out: list[str] = []
    count = 0
    for line in text.split("\n"):
        new, changed = _mask_line(line, aggressive)
        new, url_changed = _mask_url_creds(new)
        out.append(new)
        count += 1 if (changed or url_changed) else 0
    return "\n".join(out), count


# --- Hook I/O --------------------------------------------------------------


def _handle_read(tool_input: dict[str, object], tool_response: object) -> None:
    """Read tool: tool_response.file.content holds the file text."""
    if not isinstance(tool_response, dict):
        return
    file_obj = tool_response.get("file")
    if not isinstance(file_obj, dict):
        return
    content = file_obj.get("content")
    if not isinstance(content, str):
        return
    path = tool_input.get("file_path", "")
    aggressive = bool(SECRET_FILE_RE.search(path)) if isinstance(path, str) else False
    masked, count = mask_text(content, aggressive)
    if count:
        hook_lib.emit_replacement(masked)


def _handle_bash(tool_input: dict[str, object], tool_response: object) -> None:
    """Bash tool: mask stdout+stderr; aggressive if the command actually reads a
    secret file (not merely mentions its path in unrelated text).
    """
    if not isinstance(tool_response, dict):
        return
    command = tool_input.get("command", "")
    aggressive = (
        _command_reads_secret_file(command) if isinstance(command, str) else False
    )
    stdout = tool_response.get("stdout")
    stderr = tool_response.get("stderr")
    stdout_s = stdout if isinstance(stdout, str) else ""
    stderr_s = stderr if isinstance(stderr, str) else ""
    masked_out, c1 = mask_text(stdout_s, aggressive)
    masked_err, c2 = mask_text(stderr_s, aggressive)
    if c1 + c2 == 0:
        return
    # Reconstruct the text the model sees: stdout, then stderr if present.
    combined = masked_out
    if masked_err.strip():
        combined = (combined + "\n" + masked_err) if combined else masked_err
    hook_lib.emit_replacement(combined)


def _pretooluse_block_reason(tool_name: str, tool_input: dict[str, object]) -> str | None:
    """PreToolUse guard: hard-block (not just mask-after) a Read/Bash call that
    actually reads a known plaintext secret file. Returns a stderr reason, or None
    to allow through unchanged.

    Exists because PostToolUse's rewrite can't be relied on (see the module
    docstring's #68951 note), so the aggressive case gets a real refusal here. Uses
    the same precise executable-aware check as `_handle_bash`, NOT a raw substring
    search — a hard block is far more disruptive on a false positive than
    over-masking is, so a mere mention of a secret-file path must not trip it.
    """
    if tool_name == "Read":
        path = tool_input.get("file_path", "")
        hit = isinstance(path, str) and bool(SECRET_FILE_RE.search(path))
    elif tool_name == "Bash":
        command = tool_input.get("command", "")
        hit = isinstance(command, str) and _command_reads_secret_file(command)
    else:
        hit = False
    if not hit:
        return None
    return (
        "BLOCKED by mask-secrets.py (PreToolUse): this path/command targets a "
        "plaintext secret file (the ccstudio provider-key fallback or a dotenv). "
        "Direct agent reads of live secret values are not permitted — the app "
        "manages provider keys (keychain-first), and config shape questions are "
        "answered by .env.example or the docs, not the live file. "
        "(PostToolUse output-masking can't be relied on for this call on the "
        "current Claude Code version: anthropics/claude-code#68951.)"
    )


def main() -> int:
    payload = hook_lib.read_payload()
    if payload is None:
        return 0  # fail open — original output stands

    if payload.event == "PreToolUse":
        reason = _pretooluse_block_reason(payload.tool_name, payload.tool_input)
        if reason is None:
            return 0
        return hook_lib.emit_block(reason)

    if payload.tool_name == "Read":
        _handle_read(payload.tool_input, payload.tool_response)
    elif payload.tool_name == "Bash":
        _handle_bash(payload.tool_input, payload.tool_response)
    return 0


if __name__ == "__main__":
    sys.exit(main())
