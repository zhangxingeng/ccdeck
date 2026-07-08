/**
 * toolIntent.ts — one-line "what is this tool call doing" formatter.
 *
 * tool_use blocks are shown as a read-only INTENT BRIEF, never as raw JSON
 * (issue #14 locked decision). Each known tool maps to its single most
 * relevant input field; anything else falls back to the first meaningful
 * string field on the input, truncated to stay a one-liner.
 */

import type { ContentBlock } from './types.js';

/** Tool name → the one input field that best summarizes "what is this doing". */
const FIELD_BY_TOOL: Record<string, string> = {
  Bash: 'command',
  BashOutput: 'bash_id',
  Read: 'file_path',
  Edit: 'file_path',
  Write: 'file_path',
  NotebookEdit: 'notebook_path',
  Grep: 'pattern',
  Glob: 'pattern',
  Task: 'description',
  WebFetch: 'url',
  WebSearch: 'query',
  TodoWrite: 'todos',
};

const MAX_LEN = 140;

function truncate(s: string, max = MAX_LEN): string {
  const oneLine = s.replace(/\s+/g, ' ').trim();
  return oneLine.length > max ? oneLine.slice(0, max - 1) + '…' : oneLine;
}

/** Best-effort single-line stringification of a non-string field value
 *  (e.g. TodoWrite's `todos` array) — still never raw/pretty JSON, just a
 *  short summary. */
function stringifyBriefly(value: unknown): string | null {
  if (typeof value === 'string') return value.trim() || null;
  if (Array.isArray(value)) return `${value.length} item${value.length === 1 ? '' : 's'}`;
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  return null;
}

/** First meaningful (non-empty) string field on the input, for tools with no
 *  explicit mapping above. */
function firstMeaningfulField(input: Record<string, unknown>): string | null {
  for (const v of Object.values(input)) {
    if (typeof v === 'string' && v.trim()) return v;
  }
  return null;
}

/** One-line human-readable intent for a tool_use block. NEVER raw JSON. */
export function toolIntent(block: ContentBlock): string {
  const name = block.toolName || 'unknown';
  const input = block.toolInput ?? {};
  const field = FIELD_BY_TOOL[name];
  const raw = field !== undefined ? stringifyBriefly(input[field]) : null;
  const value = raw ?? firstMeaningfulField(input);
  return value ? `${name}: ${truncate(value)}` : name;
}

/** Short brief for a tool_result: ok/error + a rough size, never the full
 *  output (that stays hidden — this is a brief, not a viewer). */
export function toolResultBrief(block: ContentBlock): string {
  const text = block.toolOutput ?? block.text ?? '';
  const chars = text.length;
  const sizeLabel = chars < 1000 ? `${chars} char${chars === 1 ? '' : 's'}` : `${(chars / 1000).toFixed(1)}k chars`;
  return `${block.isError ? 'Error' : 'OK'} · ${sizeLabel}`;
}
