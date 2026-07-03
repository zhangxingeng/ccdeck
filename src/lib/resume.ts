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

/** The command a user would paste into their own terminal to resume this session. */
export function resumeCommand(cwd: string, sessionId: string): string {
  return cwd ? `cd ${shellQuote(cwd)} && claude --resume ${sessionId}` : `claude --resume ${sessionId}`;
}
