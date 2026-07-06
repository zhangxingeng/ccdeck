/**
 * Session-level operations: rename.
 * Pure helper + async write-back. No snapshot — renaming is low-stakes.
 *
 * Renaming appends the same two entry types Claude Code's own `/rename`
 * slash command writes — `custom-title` and `agent-name` — instead of a
 * field this app invented. An earlier version of this function mutated a
 * fabricated `message.content` field on the `ai-title` line, which only
 * this app ever read back: the real `claude --resume` / `/resume` picker
 * reads `customTitle`, so those renames never showed up outside CC Deck. Always
 * appending (never editing an earlier line) matches how the CLI itself
 * re-asserts the current title wherever the conversation currently is.
 */

import { sessionIdFromPath } from './resume.js';

/**
 * Return a new JSONL string with a rename appended at the end.
 * Existing content is kept byte-for-byte.
 */
export function applyTitleToJsonl(rawText: string, newTitle: string, sessionId: string): string {
  const base = rawText.endsWith('\n') ? rawText : rawText + '\n';
  const customTitleLine = JSON.stringify({ type: 'custom-title', customTitle: newTitle, sessionId });
  const agentNameLine = JSON.stringify({ type: 'agent-name', agentName: newTitle, sessionId });
  return `${base}${customTitleLine}\n${agentNameLine}\n`;
}

/**
 * Read the JSONL at `path`, append the rename, and write it back.
 * No snapshot is taken — renaming is considered low-stakes.
 *
 * api.ts is imported dynamically so that importing THIS module (e.g. in tests)
 * does not trigger api.ts's Vite ?raw top-level imports.
 */
export async function renameSession(path: string, newTitle: string): Promise<void> {
  const { readSession, writeSession } = await import('./api.js');
  const raw = await readSession(path);
  await writeSession(path, applyTitleToJsonl(raw, newTitle, sessionIdFromPath(path)));
}
