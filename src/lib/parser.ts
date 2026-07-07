/**
 * Parse Claude Code JSONL files into Entry objects.
 * Pure TypeScript — no DOM, no Tauri, no Svelte.
 */

import type { ContentBlock, Entry } from './types.js';

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/** Entry types that carry no conversational content — skip entirely. */
const META_TYPES = new Set([
  'mode',
  'permission-mode',
  'ai-title',
  'file-history-snapshot',
  'last-prompt',
  'queue-operation',
  'attachment',
  'bridge-session',
  'skill-listing',
  'deferred-tools-delta',
]);

/** Prefixes that identify internal command echo messages — skip them. */
const INTERNAL_ECHO_PREFIXES: string[] = [
  '<command-name>',
  '<local-command-stdout>',
  '<command-message>',
  '<command-args>',
  '<local-command-caveat>',
  '<system-reminder>',
  '<teammate-message',
  '<task-notification>',
];

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

type RawBlock = Record<string, unknown>;
type RawEntry = Record<string, unknown>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function isInternalEcho(content: unknown): boolean {
  if (content == null) return false;
  let text = '';
  if (typeof content === 'string') {
    text = content;
  } else if (Array.isArray(content) && content.length >= 1) {
    const first = content[0] as RawBlock;
    if (first && first['type'] === 'text' && typeof first['text'] === 'string') {
      text = first['text'];
    }
  }
  return INTERNAL_ECHO_PREFIXES.some((p) => text.startsWith(p));
}

function parseTaskNotification(text: string): Record<string, string> {
  const fields: Record<string, string> = {};
  const tags = [
    'task-id', 'tool-use-id', 'output-file', 'status',
    'summary', 'note', 'result',
  ];
  for (const tag of tags) {
    const re = new RegExp(`<${tag}>([\\s\\S]*?)<\\/${tag}>`);
    const m = re.exec(text);
    if (m) fields[tag.replace(/-/g, '_')] = m[1].trim();
  }

  // Usage stats
  const usageParts: string[] = [];
  for (const stat of ['subagent_tokens', 'tool_uses', 'duration_ms']) {
    const re = new RegExp(`<${stat}>(\\d+)<\\/${stat}>`);
    const m = re.exec(text);
    if (m) usageParts.push(`${stat}: ${m[1]}`);
  }
  if (usageParts.length) fields['usage'] = usageParts.join(' | ');

  return fields;
}

function extractContentBlocks(rawBlocks: RawBlock[]): ContentBlock[] {
  const blocks: ContentBlock[] = [];
  for (const b of rawBlocks) {
    if (typeof b !== 'object' || b === null) continue;
    const type = b['type'] as string | undefined;

    if (type === 'text') {
      blocks.push({
        blockType: 'text',
        text: (b['text'] as string) || '',
      });
    }
    // thinking / tool_use / tool_result blocks are intentionally dropped —
    // the display model only ever carries user/assistant text.
  }
  return blocks;
}

// ---------------------------------------------------------------------------
// Title cleanup
// ---------------------------------------------------------------------------

/**
 * Clean a raw session title for display:
 * 1. Collapse all whitespace runs (incl. newlines/tabs) to a single space.
 * 2. Remove remaining non-whitespace control characters (U+0000–U+001F, U+007F).
 * 3. Strip leading markdown/quoting noise: #, >, *, -, backticks, surrounding quotes.
 * 4. Trim. Returns '' for blank input (caller falls back to 'Untitled').
 */
export function cleanTitle(s: string): string {
  // Collapse all whitespace (newlines, tabs, etc.) to a single space
  let t = s.replace(/\s+/g, ' ');
  // Remove any remaining non-whitespace control characters
  t = t.replace(/[\x00-\x1F\x7F]/g, '');
  // Strip leading markdown/quoting noise: #, >, *, -, backticks, straight/smart quotes
  t = t.replace(/^[\s#>*\-`'"'"‘’“”]+/, '');
  // Strip trailing straight/smart quotes
  t = t.replace(/['"'"‘’“”]+$/, '');
  return t.trim();
}

// ---------------------------------------------------------------------------
// Primary export: parseJsonl
// ---------------------------------------------------------------------------

/**
 * Parse raw JSONL text into Entry objects.
 * Filters meta entry types and internal command echoes.
 * Detects interruptions and task-notification entries.
 */
export function parseJsonl(text: string): Entry[] {
  const entries: Entry[] = [];

  for (const line of text.split('\n')) {
    const trimmed = line.trim();
    if (!trimmed) continue;

    let raw: RawEntry;
    try {
      raw = JSON.parse(trimmed) as RawEntry;
    } catch {
      continue;
    }

    const type = (raw['type'] as string) || '';

    // Skip pure meta entry types
    if (META_TYPES.has(type)) continue;

    // Skip system entries (compact_boundary etc. carry no user-facing content)
    if (type === 'system') continue;

    const message = (raw['message'] as Record<string, unknown>) || {};
    const contentRaw: unknown = message['content'];

    // Parse content into blocks
    let blocks: ContentBlock[] = [];
    let isInterruption: boolean | undefined;
    let taskNotification: Record<string, string> | undefined;

    if (type === 'user') {
      if (typeof contentRaw === 'string') {
        if (contentRaw.startsWith('<task-notification>')) {
          // Must be checked BEFORE echo filter — task-notification is a structured
          // subagent result delivery, not a silent echo to drop.
          taskNotification = parseTaskNotification(contentRaw);
          // No content blocks — structured payload only
        } else if (isInternalEcho(contentRaw)) {
          // Drop internal command echoes (system-reminder, command-name, etc.)
          continue;
        } else if (contentRaw.includes('[Request interrupted by user]')) {
          isInterruption = true;
          blocks = [{ blockType: 'text', text: '[Request interrupted by user]' }];
        } else {
          blocks = [{ blockType: 'text', text: contentRaw }];
        }
      } else if (Array.isArray(contentRaw)) {
        // Array content can also start with an echo prefix in its first text block
        if (isInternalEcho(contentRaw)) continue;
        blocks = extractContentBlocks(contentRaw as RawBlock[]);
      }
    } else if (type === 'assistant') {
      if (Array.isArray(contentRaw)) {
        blocks = extractContentBlocks(contentRaw as RawBlock[]);
      }
    }

    entries.push({
      type,
      uuid: (raw['uuid'] as string) || '',
      parentUuid: raw['parentUuid'] as string | undefined,
      requestId: raw['requestId'] as string | undefined,
      timestamp: raw['timestamp'] as string | undefined,
      model: type === 'assistant' ? (message['model'] as string | undefined) : undefined,
      isSidechain: (raw['isSidechain'] as boolean) || false,
      blocks,
      rawContent: contentRaw,
      isInterruption,
      taskNotification,
      toolUseResult: raw['toolUseResult'] as Record<string, unknown> | undefined,
    });
  }

  return entries;
}

// ---------------------------------------------------------------------------
// extractMeta — used by the browse list (works on raw preview lines or Entries)
// ---------------------------------------------------------------------------

/**
 * Extract session metadata (title, date, model) from either raw JSONL preview
 * lines (string[]) or already-parsed Entry[]).
 *
 * This only ever sees the first ~50 lines (SessionMeta.preview), so it can't
 * see a rename that landed further into the file — BrowseView.svelte prefers
 * SessionMeta.custom_title (scanned server-side across the whole file) over
 * this function's title whenever one is set; this is just the pre-rename
 * fallback (first real user message).
 */
export function extractMeta(
  preview: string[] | Entry[]
): { title: string; date: string; model: string } {
  let title = '';
  let date = '';
  let model = '';

  if (preview.length === 0) return { title: 'Untitled', date, model };

  if (typeof preview[0] === 'string') {
    for (const line of preview as string[]) {
      const trimmed = line.trim();
      if (!trimmed) continue;
      let raw: RawEntry;
      try { raw = JSON.parse(trimmed) as RawEntry; } catch { continue; }

      const type = (raw['type'] as string) || '';
      const message = (raw['message'] as Record<string, unknown>) || {};
      const content = message['content'];

      if (type === 'user' && !date) {
        date = (raw['timestamp'] as string) || '';
        if (!title) {
          if (typeof content === 'string' && !isInternalEcho(content)) {
            title = content;
          } else if (Array.isArray(content) && content.length > 0) {
            const first = content[0] as RawBlock;
            if (first['type'] === 'text' && typeof first['text'] === 'string') {
              title = first['text'];
            }
          }
        }
      }
      if (type === 'assistant' && !model) {
        model = (message['model'] as string) || '';
      }
      if (title && date && model) break;
    }
  } else {
    // Already-parsed Entry[] — note: ai-title was filtered, so use first user turn
    for (const e of preview as Entry[]) {
      if (e.type === 'user' && !date) {
        date = e.timestamp || '';
        if (!title) {
          const tb = e.blocks.find((b) => b.blockType === 'text');
          if (tb?.text && !isInternalEcho(tb.text)) {
            title = tb.text;
          }
        }
      }
      if (e.type === 'assistant' && !model) {
        model = e.model || '';
      }
      if (title && date && model) break;
    }
  }

  // Clean title before truncation
  if (title) title = cleanTitle(title);

  // Truncate long titles
  if (title.length > 80) title = title.slice(0, 80) + '…';

  return { title: title.trim() || 'Untitled', date, model };
}

// ---------------------------------------------------------------------------
// extractCustomTitle — resolve a real rename from full raw JSONL text
// ---------------------------------------------------------------------------

/**
 * Scan full raw JSONL text (not just a preview) for a `custom-title` entry —
 * written by the real CLI's `/rename` and by sessionOps.ts's renameSession().
 * Last-seen wins, matching the "whole-file, last-seen" scan `list_sessions`
 * does server-side for `SessionMeta.custom_title`. Returns '' if none exists,
 * so callers can fall back to extractMeta()'s first-user-message title.
 */
export function extractCustomTitle(rawText: string): string {
  let title = '';
  for (const line of rawText.split('\n')) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    let raw: RawEntry;
    try { raw = JSON.parse(trimmed) as RawEntry; } catch { continue; }
    if (raw['type'] === 'custom-title' && typeof raw['customTitle'] === 'string') {
      title = raw['customTitle'] as string;
    }
  }
  return title;
}

// ---------------------------------------------------------------------------
// decodeProject — human-readable project name from encoded dir name
// ---------------------------------------------------------------------------

/**
 * Convert an encoded Claude project directory name (e.g. "-home-user-myproject")
 * to a human-readable string (e.g. "user/myproject").
 */
export function decodeProject(raw: string): string {
  // Claude encodes paths: [^a-zA-Z0-9] → '-'
  // Strip leading/trailing dashes, split on runs of dashes
  const parts = raw.replace(/^-+|-+$/g, '').split(/-+/).filter(Boolean);
  if (parts.length === 0) return raw;
  if (parts.length >= 2) {
    const last = parts[parts.length - 1];
    const second = parts[parts.length - 2];
    if (last.length > 3 && second.length > 2) {
      return second + '/' + last;
    }
    return last;
  }
  return parts[0];
}

// ---------------------------------------------------------------------------
// homeRelativePath / projectLabel — human-readable project path for a session
// ---------------------------------------------------------------------------

/**
 * Render an absolute path relative to the home directory, e.g.
 * "/home/user/workspace/foo" + "/home/user" -> "~/workspace/foo".
 * Returns `path` unchanged if `homeDir` is unknown or not a prefix of it.
 */
export function homeRelativePath(path: string, homeDir: string | null): string {
  if (!path || !homeDir) return path;
  if (path === homeDir) return '~';
  if (path.startsWith(homeDir + '/')) return '~' + path.slice(homeDir.length);
  return path;
}

/**
 * Best-effort human-readable project label for a session.
 *
 * The encoded project directory name (project_raw) is lossy — Claude Code
 * replaces every '/' with '-', so "-home-user-my-app" is ambiguous between
 * "user/my-app" and "user-my/app". The real `cwd` recorded on the session's
 * JSONL lines is the reliable source of truth; only fall back to decoding
 * the folder name when cwd is genuinely unavailable.
 */
export function projectLabel(cwd: string, projectRaw: string, homeDir: string | null): string {
  if (cwd) return homeRelativePath(cwd, homeDir);
  return decodeProject(projectRaw);
}
