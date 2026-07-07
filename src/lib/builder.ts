/**
 * Build a Session from parsed Entry objects.
 * Groups by requestId into Turns.
 *
 * Pure TypeScript — no DOM, no Tauri, no Svelte.
 */

import type { Entry, Session, Turn } from './types.js';

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/** Check if a user Entry has any actual text (not only tool_result blocks). */
function hasUserText(entry: Entry): boolean {
  const raw = entry.rawContent;
  if (raw == null) return false;
  if (typeof raw === 'string') return raw.trim().length > 0;
  if (Array.isArray(raw)) {
    for (const b of raw as Array<Record<string, unknown>>) {
      if (b['type'] !== 'tool_result') return true;
    }
  }
  return false;
}

// ---------------------------------------------------------------------------
// buildSession
// ---------------------------------------------------------------------------

export interface BuildSessionOptions {
  project?: string;
  sourcePath?: string;
}

/**
 * Build a Session from a flat list of Entries.
 */
export function buildSession(entries: Entry[], opts: BuildSessionOptions = {}): Session {
  const turns: Turn[] = [];

  let currentTurn: Turn | null = null;
  // Track the requestId of the turn currently being assembled (avoids polluting Turn type)
  let currentTurnRequestId = '';

  function flushTurn(): void {
    if (currentTurn) {
      turns.push(currentTurn);
      currentTurn = null;
      currentTurnRequestId = '';
    }
  }

  for (const entry of entries) {
    if (entry.type === 'assistant') {
      const rid = entry.requestId || '';

      // Start a new turn when requestId changes (or on first assistant entry)
      if (!currentTurn || (rid && currentTurnRequestId !== rid)) {
        flushTurn();
        currentTurn = {
          role: 'assistant',
          blocks: [],
          timestamp: entry.timestamp,
          model: entry.model,
        };
        currentTurnRequestId = rid;
      }

      // Track the earliest timestamp and best model
      if (entry.timestamp && !currentTurn.timestamp) {
        currentTurn.timestamp = entry.timestamp;
      }
      if (entry.model && !currentTurn.model) {
        currentTurn.model = entry.model;
      }

      // Accumulate blocks
      for (const block of entry.blocks) {
        currentTurn.blocks.push({ ...block });
      }

    } else if (entry.type === 'user') {
      // 1. If this is only tool results (no user text), don't create a user turn
      if (!hasUserText(entry)) continue;

      // 2. task-notification entries — skip (they're subagent result deliveries)
      if (entry.taskNotification) continue;

      // 3. Interruption marker — mark the last assistant turn
      if (entry.isInterruption) {
        if (turns.length > 0) {
          turns[turns.length - 1].isInterrupted = true;
        } else if (currentTurn) {
          currentTurn.isInterrupted = true;
        }
        // Don't create a user turn for this
        continue;
      }

      // 4. Regular user message — flush assistant turn, create user turn
      flushTurn();

      const userTurn: Turn = {
        role: 'user',
        blocks: entry.blocks.map((b) => ({ ...b })),
        timestamp: entry.timestamp,
      };
      turns.push(userTurn);
      // Reset currentTurn; next assistant entry will create a new one
      currentTurn = null;
    }
  }

  // Flush any remaining assistant turn
  flushTurn();

  // Extract session-level metadata
  const meta = _deriveSessionMeta(entries, opts);

  return { turns, meta };
}

function _deriveSessionMeta(
  entries: Entry[],
  opts: BuildSessionOptions
): Session['meta'] {
  let title = '';
  let date = '';
  let model = '';

  for (const e of entries) {
    if (e.type === 'user' && !date) {
      date = e.timestamp || '';
      if (!title) {
        const tb = e.blocks.find((b) => b.blockType === 'text');
        if (tb?.text) title = tb.text;
      }
    }
    if (e.type === 'assistant' && !model) {
      model = e.model || '';
    }
    if (date && model) break;
  }

  if (title.length > 80) title = title.slice(0, 80) + '…';

  return {
    title: title.trim() || 'Untitled',
    date,
    model,
    project: opts.project || '',
    sourcePath: opts.sourcePath || '',
    cwd: '', // filled in by the caller from the raw text (see +page.svelte::loadSession)
  };
}
