/**
 * displayModel.ts — turn a flat, ordered list of renderable rows into display
 * items for the editor: individual chat bubbles interleaved with collapsed
 * "tool activity" groups.
 *
 * Pure TypeScript — no DOM, no Svelte — so the grouping is unit-testable in
 * isolation.
 *
 * Rule: a row that carries human-readable text (a user/assistant message) is its
 * own bubble. Any maximal run of consecutive rows that carry NO text (tool calls,
 * tool results, standalone thinking — the "geek stuff" between two chat chunks)
 * collapses into a single group the user can expand or delete as a unit (and
 * member-by-member). This is what keeps the default view "just the chat."
 */

export interface RowFlag {
  key: string;
  hasText: boolean;
}

export interface DisplayMessage {
  kind: 'message';
  key: string;
}

export interface DisplayToolGroup {
  kind: 'toolgroup';
  keys: string[];
}

export type DisplayItem = DisplayMessage | DisplayToolGroup;

/**
 * Partition `rows` (already in display order) into messages and contiguous
 * tool-groups. Order is preserved; every input key appears exactly once in the
 * output.
 */
export function groupDisplayItems(rows: RowFlag[]): DisplayItem[] {
  const items: DisplayItem[] = [];
  let pending: string[] = [];

  const flush = () => {
    if (pending.length > 0) {
      items.push({ kind: 'toolgroup', keys: pending });
      pending = [];
    }
  };

  for (const r of rows) {
    if (r.hasText) {
      flush();
      items.push({ kind: 'message', key: r.key });
    } else {
      pending.push(r.key);
    }
  }
  flush();

  return items;
}

// ── Turn spans ──────────────────────────────────────────────────────────────
//
// A "turn" is the delete-as-a-unit span issue #14's turn-level delete operates
// on. It begins at a real user MESSAGE bubble — a row that is `type === 'user'`
// AND carries text — and runs up to (not including) the next such bubble. A
// `tool_result`-only user line (no text) does NOT start a turn (it's the tail
// of the assistant's turn). Rows before the first user message bubble form an
// implicit leading turn (e.g. a session that opens with a summary/assistant
// line). Every input key appears in exactly one span, order preserved.

export interface TurnRow {
  key: string;
  type: string;     // Entry.type ('user' | 'assistant' | …)
  hasText: boolean; // has at least one text block
}

export interface TurnSpan {
  /** Row keys in this turn, in display order. `keys[0]` is the span's start
   *  row — the user message bubble, or (leading turn) whatever comes first. */
  keys: string[];
}

/** True when a row opens a new turn: a user line that actually carries text. */
export function isTurnStart(row: TurnRow): boolean {
  return row.type === 'user' && row.hasText;
}

/** Partition rows (in display order) into turn spans. */
export function deriveTurnSpans(rows: TurnRow[]): TurnSpan[] {
  const spans: TurnSpan[] = [];
  let current: string[] = [];

  for (const r of rows) {
    // A user-with-text row starts a new turn — flush the one in progress
    // first, unless we're still accumulating the implicit leading turn.
    if (isTurnStart(r) && current.length > 0) {
      spans.push({ keys: current });
      current = [];
    }
    current.push(r.key);
  }
  if (current.length > 0) spans.push({ keys: current });

  return spans;
}
