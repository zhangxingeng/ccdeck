/**
 * "Resume from Claude Code" helpers — pure string logic, no DOM/Tauri.
 * The real Claude session id is the session file's basename (uuid.jsonl),
 * NOT the app's own SessionMeta.id (which is a projects-dir-relative path).
 *
 * v0.14 (issue #34) removed the terminal launcher. Resume no longer spawns a
 * terminal or threads a configurable launch command / provider profile; it
 * surfaces the session's facts as copyable text (project path, session id, and
 * a ready-to-paste resume command) and the user runs it in their own terminal.
 */

export function sessionIdFromPath(path: string): string {
  const fname = path.split('/').pop() ?? path;
  return fname.replace(/\.jsonl$/, '');
}

export function shellQuote(s: string): string {
  return `'${s.replace(/'/g, `'\\''`)}'`;
}

/**
 * The single, ready-to-paste line a user runs in their own terminal to resume
 * this session: `cd '<cwd>' && claude --resume '<id>'`. Both values are
 * shell-quoted so a path or id with spaces/quotes can't break the line.
 *
 * `cd` first because `claude --resume` associates a session with its project
 * directory — resuming from the wrong cwd won't find it. When the real cwd is
 * unknown ('' — an old session with no recorded cwd) the `cd` is omitted and
 * just `claude --resume '<id>'` is returned, with the caller free to show the
 * path separately.
 */
export function resumeCommand(cwd: string, sessionId: string): string {
  const resume = `claude --resume ${shellQuote(sessionId)}`;
  return cwd.trim() === '' ? resume : `cd ${shellQuote(cwd)} && ${resume}`;
}
