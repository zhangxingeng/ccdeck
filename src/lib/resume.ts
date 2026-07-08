/**
 * "Resume from Claude Code" helpers — pure string logic, no DOM/Tauri.
 * The real Claude session id is the session file's basename (uuid.jsonl),
 * NOT the app's own SessionMeta.id (which is a projects-dir-relative path).
 */

export function sessionIdFromPath(path: string): string {
  const fname = path.split('/').pop() ?? path;
  return fname.replace(/\.jsonl$/, '');
}

function shellQuote(s: string): string {
  return `'${s.replace(/'/g, `'\\''`)}'`;
}

/**
 * Zero-config default launch command — must stay in lock-step with
 * `DEFAULT_LAUNCH_COMMAND` in `src-tauri/src/appconfig.rs`. Used when App Config's
 * `launchCommand` is empty/whitespace-only, mirroring the backend's
 * `effective_launch_command`.
 */
export const DEFAULT_LAUNCH_COMMAND = `claude --resume "$CCDECK_SESSION_ID"`;

/**
 * The command a user would paste into their own terminal to resume this session.
 *
 * Faithfully mirrors the script the backend actually runs on Resume
 * (`build_resume_script` in `src-tauri/src/appconfig.rs`): it exports the three
 * `CCDECK_*` env vars and then runs the configured `launchCommand` verbatim,
 * rather than splicing values into a hardcoded `claude --resume <id>` shape.
 * This keeps the clipboard fallback accurate for custom / multi-line launch
 * commands (a tmux wrapper, a script, etc.), not just the default.
 *
 * `launchCommand` is passed in by the caller (already fetched from App Config)
 * so this helper stays pure — no Tauri dependency. Empty/whitespace-only ⇒
 * [`DEFAULT_LAUNCH_COMMAND`], matching the backend.
 */
export function resumeCommand(
  cwd: string,
  sessionId: string,
  sessionTitle: string,
  launchCommand: string,
): string {
  const command = launchCommand.trim() === '' ? DEFAULT_LAUNCH_COMMAND : launchCommand;
  return [
    `export CCDECK_SESSION_ID=${shellQuote(sessionId)}`,
    `export CCDECK_SESSION_TITLE=${shellQuote(sessionTitle)}`,
    `export CCDECK_CWD=${shellQuote(cwd)}`,
    `cd ${shellQuote(cwd)} &&`,
    command,
  ].join('\n');
}
