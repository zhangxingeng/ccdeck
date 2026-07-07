/**
 * editDraft.ts — Pure TypeScript edit model for JSONL sessions.
 * No DOM, no Svelte, no Tauri imports.
 *
 * Plain edit-in-place model: each row holds its original line and its current
 * (possibly edited) value. There is no version history, no reorder, and no
 * delete/restore — "edit a message, Save writes to disk" is the whole surface.
 */

export const MESSAGE_ROLES = ['user', 'assistant'];

// ── Types ──────────────────────────────────────────────────────────────────────

export interface DraftRow {
  originalIndex: number; // 0-based position in the original file
  type: string;          // entry `type` ('' if unparseable)
  uuid: string | null;
  original: string;      // exact original line text — never mutated, used for isDirty
  value: string;         // current line text (equals `original` until edited)
}

export interface Draft {
  sessionPath: string;
  order: string[];       // row keys in file order (fixed — no reordering)
  rows: Record<string, DraftRow>;
  createdAt: number;     // unix secs, passed in (do NOT call Date.now in this module)
}

// ── Internal helpers ──────────────────────────────────────────────────────────

function parseLine(text: string): Record<string, unknown> | null {
  try {
    const obj = JSON.parse(text);
    if (typeof obj === 'object' && obj !== null) return obj as Record<string, unknown>;
    return null;
  } catch {
    return null;
  }
}

function deepClone<T>(val: T): T {
  return JSON.parse(JSON.stringify(val)) as T;
}

// ── buildDraft ─────────────────────────────────────────────────────────────────

export function buildDraft(rawText: string, sessionPath: string, createdAt: number): Draft {
  const lines = rawText.split('\n');
  // Drop trailing empty line produced by the final \n
  if (lines.length > 0 && lines[lines.length - 1] === '') {
    lines.pop();
  }

  const rows: Record<string, DraftRow> = {};
  const order: string[] = [];

  for (let originalIndex = 0; originalIndex < lines.length; originalIndex++) {
    const line = lines[originalIndex];
    if (line.trim() === '') continue;

    const obj = parseLine(line);
    const uuid = obj !== null && typeof obj['uuid'] === 'string' ? (obj['uuid'] as string) : null;
    const type = obj !== null && typeof obj['type'] === 'string' ? (obj['type'] as string) : '';
    // Key rule: use uuid if present and parses, else `idx:<originalIndex>`
    const key = uuid !== null ? uuid : `idx:${originalIndex}`;

    rows[key] = { originalIndex, type, uuid, original: line, value: line };
    order.push(key);
  }

  return { sessionPath, order, rows, createdAt };
}

// ── serializeDraft ─────────────────────────────────────────────────────────────

/** Emit the current value of each row in `order`, joined '\n' + trailing '\n'. */
export function serializeDraft(d: Draft): string {
  return d.order.map((key) => d.rows[key].value).join('\n') + '\n';
}

// ── isDirty ────────────────────────────────────────────────────────────────────

export function isDirty(d: Draft): boolean {
  for (const key of d.order) {
    if (d.rows[key].value !== d.rows[key].original) return true;
  }
  return false;
}

// ── applyBlockTextEdit ─────────────────────────────────────────────────────────

/**
 * Edit ONE text block of a message, addressed by its ordinal among text blocks
 * (0 = first text block, 1 = second, …) — a single message bubble may hold
 * several text blocks and each is independently editable. Only the target
 * {type:'text'} block's `.text` is touched, so the surrounding structure (tool
 * calls, thinking, ordering) is preserved.
 *
 * - message.content is a string → only ordinal 0 is valid (the whole string).
 * - message.content is an array → the ordinal-th text block is replaced.
 * No-op if the row is unparseable, has no message, the ordinal is out of
 * range, or the resulting line is unchanged.
 */
export function applyBlockTextEdit(
  d: Draft,
  key: string,
  textOrdinal: number,
  newText: string
): Draft {
  const row = d.rows[key];
  if (!row) return d;

  const obj = parseLine(row.value);
  if (obj === null) return d;

  const msg = obj['message'] as Record<string, unknown> | undefined;
  if (!msg) return d;

  const cloned = deepClone(obj);
  const clonedMsg = cloned['message'] as Record<string, unknown>;
  const content = clonedMsg['content'];

  if (typeof content === 'string') {
    if (textOrdinal !== 0) return d;
    clonedMsg['content'] = newText;
  } else if (Array.isArray(content)) {
    const arr = content as Array<{ type: string; text?: string }>;
    let seen = -1;
    let target = -1;
    for (let i = 0; i < arr.length; i++) {
      if (arr[i] && arr[i].type === 'text') {
        seen++;
        if (seen === textOrdinal) { target = i; break; }
      }
    }
    if (target < 0) return d;
    arr[target] = { ...arr[target], text: newText };
  } else {
    return d;
  }

  const newValue = JSON.stringify(cloned);
  if (newValue === row.value) return d;
  return { ...d, rows: { ...d.rows, [key]: { ...row, value: newValue } } };
}

// ── applyRoleEdit ──────────────────────────────────────────────────────────────

/**
 * Flip a message's speaker. Sets BOTH the top-level `type` and `message.role`
 * (which normally mirror each other) in one write, so the saved file stays
 * internally consistent. No-op if the row is unparseable or unchanged.
 */
export function applyRoleEdit(d: Draft, key: string, role: string): Draft {
  const row = d.rows[key];
  if (!row) return d;

  const obj = parseLine(row.value);
  if (obj === null) return d;

  const cloned = deepClone(obj);
  cloned['type'] = role;
  const msg = cloned['message'] as Record<string, unknown> | undefined;
  if (msg) msg['role'] = role;

  const newValue = JSON.stringify(cloned);
  if (newValue === row.value) return d;
  return { ...d, rows: { ...d.rows, [key]: { ...row, value: newValue, type: role } } };
}

// ── applyRawEdit ─────────────────────────────────────────────────────────────

/**
 * Replace the entire line with user-supplied raw JSON (power-user escape
 * hatch for tool blocks etc.). The input is parsed and re-stringified to
 * canonical single-line JSON, so the saved file is guaranteed to remain valid,
 * parseable JSONL. THROWS if the input is not a valid JSON object/array — the
 * caller must catch and surface the error (we reject rather than save garbage).
 * The row's stable map key is preserved even if the uuid inside it changes.
 */
export function applyRawEdit(d: Draft, key: string, newRawLine: string): Draft {
  const row = d.rows[key];
  if (!row) return d;

  const parsed = JSON.parse(newRawLine); // throws on invalid JSON — caller catches
  if (typeof parsed !== 'object' || parsed === null) {
    throw new Error('Top-level JSON must be an object or array.');
  }

  const normalized = JSON.stringify(parsed);
  const obj = Array.isArray(parsed) ? null : (parsed as Record<string, unknown>);
  const newUuid = obj && typeof obj['uuid'] === 'string' ? (obj['uuid'] as string) : row.uuid;
  const newType = obj && typeof obj['type'] === 'string' ? (obj['type'] as string) : row.type;

  if (normalized === row.value) return d;
  return {
    ...d,
    rows: { ...d.rows, [key]: { ...row, value: normalized, uuid: newUuid, type: newType } },
  };
}

// ── SessionInfo + extractSessionInfo ──────────────────────────────────────────

export interface SessionInfo {
  cwd: string;
  gitBranch: string;
  versions: string[];      // distinct CLI version values, first-seen order
  models: string[];        // distinct message.model values, first-seen order
  permissionMode: string;  // last 'permission-mode' line's permissionMode
  firstTs: string;
  lastTs: string;
  userCount: number;
  assistantCount: number;
  lineCount: number;
}

/** Scan all lines and extract session-level metadata. */
export function extractSessionInfo(rawText: string): SessionInfo {
  const lines = rawText.split('\n').filter(l => l.trim() !== '');

  let cwd = '';
  let gitBranch = '';
  const versions: string[] = [];
  const models: string[] = [];
  let permissionMode = '';
  let firstTs = '';
  let lastTs = '';
  let userCount = 0;
  let assistantCount = 0;
  const lineCount = lines.length;

  for (const line of lines) {
    const obj = parseLine(line);
    if (obj === null) continue;

    const type = typeof obj['type'] === 'string' ? (obj['type'] as string) : '';
    const msg = obj['message'] as Record<string, unknown> | undefined;

    // Extract cwd and gitBranch (first seen)
    if (!cwd && typeof obj['cwd'] === 'string') cwd = obj['cwd'] as string;
    if (!gitBranch && typeof obj['gitBranch'] === 'string') gitBranch = obj['gitBranch'] as string;

    // Extract CLI version (distinct first-seen)
    if (typeof obj['version'] === 'string' && (obj['version'] as string) && !versions.includes(obj['version'] as string)) {
      versions.push(obj['version'] as string);
    }

    // Extract model (distinct first-seen)
    if (msg && typeof msg['model'] === 'string' && (msg['model'] as string) && !models.includes(msg['model'] as string)) {
      models.push(msg['model'] as string);
    }

    // permissionMode: last 'permission-mode' line's permissionMode field
    if (type === 'permission-mode' && typeof obj['permissionMode'] === 'string') {
      permissionMode = obj['permissionMode'] as string;
    }

    // Timestamps: first and last non-empty
    if (typeof obj['timestamp'] === 'string' && (obj['timestamp'] as string)) {
      if (!firstTs) firstTs = obj['timestamp'] as string;
      lastTs = obj['timestamp'] as string;
    }

    // Counts
    if (type === 'user') userCount++;
    if (type === 'assistant') assistantCount++;
  }

  return {
    cwd,
    gitBranch,
    versions,
    models,
    permissionMode,
    firstTs,
    lastTs,
    userCount,
    assistantCount,
    lineCount,
  };
}
