/**
 * Build a Session from parsed Entry objects.
 * Groups by requestId into Turns, matches tool results to tool_use blocks via
 * a GLOBAL registry (results may arrive in a different turn than the call),
 * and links subagent sessions.
 *
 * Pure TypeScript — no DOM, no Tauri, no Svelte.
 */

import { parseJsonl } from './parser.js';
import type { ContentBlock, Entry, Session, SubagentFile, Turn } from './types.js';

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

interface ToolResultInfo {
  toolOutput: string;
  isError: boolean;
  isAsync: boolean;
}

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

/** Extract all tool_result blocks from a user Entry's rawContent. */
function extractToolResults(entry: Entry): { toolId: string; info: ToolResultInfo }[] {
  const results: { toolId: string; info: ToolResultInfo }[] = [];
  const raw = entry.rawContent;
  if (!Array.isArray(raw)) return results;

  for (const b of raw as Array<Record<string, unknown>>) {
    if (b['type'] !== 'tool_result') continue;
    const resultContent = b['content'];
    const textParts: string[] = [];
    if (Array.isArray(resultContent)) {
      for (const item of resultContent as Array<Record<string, unknown>>) {
        if (item['type'] === 'text' && typeof item['text'] === 'string') {
          textParts.push(item['text']);
        }
      }
    }
    results.push({
      toolId: (b['tool_use_id'] as string) || '',
      info: {
        toolOutput: textParts.join('\n'),
        isError: !!(b['is_error'] as boolean),
        isAsync: false,
      },
    });
  }
  return results;
}

/** Attach global tool results to matching tool_use blocks in a turn. */
function attachResults(
  turn: Turn,
  registry: Map<string, ToolResultInfo>,
  agentIdByToolId: Map<string, string>
): void {
  for (const block of turn.blocks) {
    if (block.blockType === 'tool_use' && block.toolId) {
      const result = registry.get(block.toolId);
      if (result) {
        block.toolOutput = result.toolOutput;
        block.isError = result.isError;
        block.isAsync = result.isAsync;
      }
      // Set agentId for Agent tool_use blocks
      const agentId = agentIdByToolId.get(block.toolId);
      if (agentId) {
        block.agentId = agentId;
      }
    }
  }
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
 *
 * Key correctness guarantee: tool results are matched to tool_use blocks via a
 * GLOBAL registry keyed by tool_use id. Results are never assumed to live in
 * the same turn as the call — they can appear in any subsequent user entry.
 */
export function buildSession(entries: Entry[], opts: BuildSessionOptions = {}): Session {
  const turns: Turn[] = [];

  // Global tool result registry: tool_use_id → result info
  const toolResultRegistry = new Map<string, ToolResultInfo>();
  // Maps tool_use_id → agentId for async Agent launches
  const agentIdByToolId = new Map<string, string>();

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
      // 1. Collect tool results from this entry into the global registry
      const trList = extractToolResults(entry);
      for (const { toolId, info } of trList) {
        if (toolId) toolResultRegistry.set(toolId, info);
      }

      // 2. Check for async subagent launch metadata
      const tur = entry.toolUseResult;
      if (tur && tur['status'] === 'async_launched' && tur['agentId']) {
        const agentId = tur['agentId'] as string;
        // Map all tool_result tool_use_ids in this entry to this agentId
        for (const { toolId } of trList) {
          if (toolId) agentIdByToolId.set(toolId, agentId);
        }
        // Also set on current assistant turn for quick lookup
        if (currentTurn) {
          currentTurn.subagentAgentId = agentId;
        }
      }

      // 3. If this is only tool results (no user text), don't create a user turn
      if (!hasUserText(entry)) continue;

      // 4. task-notification entries — skip (they're subagent result deliveries)
      if (entry.taskNotification) continue;

      // 5. Interruption marker — mark the last assistant turn
      if (entry.isInterruption) {
        if (turns.length > 0) {
          turns[turns.length - 1].isInterrupted = true;
        } else if (currentTurn) {
          currentTurn.isInterrupted = true;
        }
        // Don't create a user turn for this
        continue;
      }

      // 6. Regular user message — flush assistant turn, create user turn
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

  // GLOBAL pass: attach tool results to tool_use blocks across ALL turns
  for (const turn of turns) {
    attachResults(turn, toolResultRegistry, agentIdByToolId);
  }

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
// linkSubagents
// ---------------------------------------------------------------------------

/**
 * Parse subagent files, build their sessions, and attach them to Agent
 * tool_use blocks in the parent session by agentId.
 *
 * subagentFiles comes from the Rust read_subagents() command.
 * Non-meta files (is_meta=false) contain JSONL; meta files (is_meta=true)
 * contain JSON metadata.
 */
/**
 * Parse subagent files into a stem → Session map ("agent-foo.jsonl" +
 * "agent-foo.meta.json" → stem "agent-foo"). Split out from linkSubagents so
 * callers that don't build a full turn-grouped Session (e.g. SessionEditor,
 * which parses entries line-by-line) can still resolve subagent transcripts.
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

/** Resolve an agentId to its subagent Session, tolerating a fragment match
 *  (agentId may be a prefix/substring of the file stem or vice versa). */
export function resolveSubagentSession(
  subagentSessions: Map<string, Session>,
  agentId: string
): Session | undefined {
  const direct = subagentSessions.get(agentId);
  if (direct) return direct;
  for (const [stem, subSess] of subagentSessions) {
    if (stem.includes(agentId) || agentId.includes(stem)) return subSess;
  }
  return undefined;
}

/**
 * Map tool_use ids to the agentId of the subagent they spawned, by scanning
 * user entries for the async_launched toolUseResult marker. Extracted out of
 * buildSession's internal turn-assembly loop so callers that parse entries
 * without turn-grouping (SessionEditor) can compute the same mapping.
 */
export function mapAgentIdsByToolId(entries: Entry[]): Map<string, string> {
  const agentIdByToolId = new Map<string, string>();
  for (const entry of entries) {
    if (entry.type !== 'user') continue;
    const tur = entry.toolUseResult;
    if (tur && tur['status'] === 'async_launched' && tur['agentId']) {
      const agentId = tur['agentId'] as string;
      for (const { toolId } of extractToolResults(entry)) {
        if (toolId) agentIdByToolId.set(toolId, agentId);
      }
    }
  }
  return agentIdByToolId;
}

export function linkSubagents(session: Session, subagentFiles: SubagentFile[]): void {
  const subagentSessions = buildSubagentSessions(subagentFiles);

  // Walk ALL turns and blocks in the parent session; attach subagent by agentId
  for (const turn of session.turns) {
    for (const block of turn.blocks) {
      if (block.blockType === 'tool_use' && block.agentId) {
        const sub = resolveSubagentSession(subagentSessions, block.agentId);
        if (sub) block.subagent = sub;
      }
    }
  }
}
