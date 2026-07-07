/**
 * Build a Session from parsed Entry objects.
 * Groups by requestId into Turns.
 *
 * Pure TypeScript — no DOM, no Tauri, no Svelte.
 */

import { parseJsonl } from './parser.js';
import type { Entry, Session, SubagentFile, Turn } from './types.js';

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

// ---------------------------------------------------------------------------
// buildSubagentSessions
// ---------------------------------------------------------------------------

/**
 * Parse subagent files into a stem → Session map ("agent-foo.jsonl" +
 * "agent-foo.meta.json" → stem "agent-foo").
 *
 * subagentFiles comes from the Rust read_subagents() command. Non-meta files
 * (is_meta=false) contain JSONL; meta files (is_meta=true) contain JSON
 * metadata. Still used by SessionEditor's subagent stacked-navigation view
 * (parses entries line-by-line, so it resolves subagent transcripts itself).
 */
export function buildSubagentSessions(subagentFiles: SubagentFile[]): Map<string, Session> {
  const byName = new Map<string, { jsonl?: string; meta?: string }>();
  for (const f of subagentFiles) {
    // Derive stem: "agent-foo.jsonl" → "agent-foo"
    const stem = f.name.replace(/\.(jsonl|meta\.json)$/, '').replace(/\.meta$/, '');
    if (!byName.has(stem)) byName.set(stem, {});
    const entry = byName.get(stem)!;
    if (f.is_meta) {
      entry.meta = f.content;
    } else {
      entry.jsonl = f.content;
    }
  }

  const subagentSessions = new Map<string, Session>();
  for (const [stem, { jsonl, meta }] of byName) {
    if (!jsonl) continue;
    const entries = parseJsonl(jsonl);
    if (!entries.length) continue;

    const subSession = buildSession(entries, { sourcePath: stem });

    if (meta) {
      try {
        const m = JSON.parse(meta) as Record<string, unknown>;
        subSession.meta.model = (m['model'] as string) || subSession.meta.model;
        subSession.meta.project = (m['description'] as string) || subSession.meta.project;
      } catch {
        // Ignore malformed meta
      }
    }

    subagentSessions.set(stem, subSession);
  }
  return subagentSessions;
}

/**
 * No-op: subagent linking existed only to attach a Session onto the
 * (now-removed) tool_use blocks' subagent "Open →" affordance. ContentBlock
 * no longer carries `agentId`/`subagent` fields, so there is nothing left to
 * attach. Kept as an exported no-op because `+page.svelte` still calls it —
 * removing that call site is out of scope for this change.
 */
export function linkSubagents(_session: Session, _subagentFiles: SubagentFile[]): void {
  // Intentionally empty — see doc comment above.
}
