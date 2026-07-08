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
 *
 * `provider` (issue #21) is the optional selected provider profile. When set,
 * the provider's `ANTHROPIC_*` exports are emitted right after the `CCDECK_*`
 * ones and before `cd` — mirroring the backend script order. The API key is
 * NEVER available to the frontend, so `ANTHROPIC_AUTH_TOKEN` is emitted as a
 * MASKED placeholder (`'<paste your {name} key>'`) rather than a real secret:
 * copying a live key to the clipboard would contradict the write-only posture.
 * The no-provider path stays byte-identical to before.
 */
export interface ResumeProviderInfo {
  /** Profile name — used only to render the masked-key placeholder hint. */
  name: string;
  /** Anthropic-compatible base URL, exported as ANTHROPIC_BASE_URL. */
  baseUrl: string;
  /** Optional default model, exported as ANTHROPIC_MODEL when set. */
  defaultModel?: string;
}

export function resumeCommand(
  cwd: string,
  sessionId: string,
  sessionTitle: string,
  launchCommand: string,
  provider?: ResumeProviderInfo,
): string {
  const command = launchCommand.trim() === '' ? DEFAULT_LAUNCH_COMMAND : launchCommand;
  const lines = [
    `export CCDECK_SESSION_ID=${shellQuote(sessionId)}`,
    `export CCDECK_SESSION_TITLE=${shellQuote(sessionTitle)}`,
    `export CCDECK_CWD=${shellQuote(cwd)}`,
  ];
  if (provider) {
    lines.push(`export ANTHROPIC_BASE_URL=${shellQuote(provider.baseUrl)}`);
    // Masked, never the real key — the frontend has no access to it.
    lines.push(`export ANTHROPIC_AUTH_TOKEN=${shellQuote(`<paste your ${provider.name} key>`)}`);
    if (provider.defaultModel && provider.defaultModel.trim() !== '') {
      lines.push(`export ANTHROPIC_MODEL=${shellQuote(provider.defaultModel)}`);
    }
  }
  lines.push(`cd ${shellQuote(cwd)} &&`, command);
  return lines.join('\n');
}
