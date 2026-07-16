/**
 * Data model interfaces for Claude Code chat sessions.
 * Pure TypeScript — no DOM, no Tauri, no Svelte.
 */

export interface ContentBlock {
  // 'unknown' is a structural placeholder for any message.content element the
  // parser doesn't otherwise model (image, redacted_thinking, server_tool_use,
  // MCP result blocks, a non-object element, …). It exists so `Entry.blocks`
  // stays 1:1 (same length AND order) with the raw `message.content` array —
  // block index == content index everywhere. The soft-delete model keys and
  // removes blocks by that index, so any drift would delete the WRONG content
  // element on Save (the exact corruption class issue #13 fought).
  blockType: 'thinking' | 'text' | 'tool_use' | 'tool_result' | 'unknown';
  // text / thinking
  text?: string;
  thinking?: string;
  signature?: string;
  // tool_use fields
  toolName?: string;
  toolId?: string;
  toolInput?: Record<string, unknown>;
  // tool_result fields (matched in from global registry)
  toolOutput?: string;
  isError?: boolean;
  // unknown: the raw content-element `type` string, if any (for the read-only
  // placeholder chip). Undefined for a non-object content element.
  rawType?: string;
}

export interface Entry {
  type: string;                             // user, assistant, system, ...
  uuid: string;
  parentUuid?: string;
  requestId?: string;
  timestamp?: string;
  model?: string;
  isSidechain?: boolean;
  blocks: ContentBlock[];                   // parsed content blocks
  rawContent?: unknown;                     // original message.content
  isInterruption?: boolean;
  taskNotification?: Record<string, string>;
  toolUseResult?: Record<string, unknown>;  // raw toolUseResult from JSONL
}

export interface Turn {
  role: 'user' | 'assistant';
  blocks: ContentBlock[];
  timestamp?: string;
  model?: string;
  isInterrupted?: boolean;
  subagentAgentId?: string;
}

export interface Session {
  turns: Turn[];
  meta: {
    title: string;
    date: string;
    model: string;
    project: string;
    sourcePath: string;
    cwd: string; // real project cwd, for "resume from Claude Code" ('' if unknown)
  };
}

/** Returned by Rust list_sessions; JS side uses preview for extractMeta. */
export interface SessionMeta {
  id: string;
  path: string;
  project_raw: string;
  mtime: number;
  size: number;
  preview: string[];  // first ~50 raw JSONL lines
  // Cheap stats computed server-side in one pass over the file
  line_count: number;       // non-empty lines
  user_count: number;       // lines whose type == "user"
  assistant_count: number;  // lines whose type == "assistant"
  subagent_count: number;   // count of subagents/agent-*.jsonl next to the session file
  models: string[];         // distinct message.model values, first-seen order
  first_ts: string;         // first timestamp value seen ("" if none)
  last_ts: string;          // last timestamp value seen ("" if none)
  cwd: string;              // first-seen "cwd" value ("" if none) — the real project path
  custom_title: string;     // last-seen "customTitle" value ("" if none), scanned across the
                            // whole file — a real Claude Code rename, wherever it occurs
}

// ---------------------------------------------------------------------------
// Search (mirrors the Rust search module's camelCase-serialized structs)
// ---------------------------------------------------------------------------

/** Query-time filters. Empty projects means "no restriction". Narrowed to
 *  date + project (+ sessionPath scope) when search became messages-only (#35). */
export interface SearchFilters {
  from: number | null;           // inclusive epoch-ms lower bound
  to: number | null;             // inclusive epoch-ms upper bound
  projects: string[];            // home-relative project labels
  sessionPath: string | null;    // restrict to this one session file ("current session only")
}

/** One search result: a matched block + a snippet with char-offset match ranges. */
export interface SearchHit {
  sessionPath: string;
  project: string;
  ts: number | null;
  lineNo: number;
  blockNo: number;
  uuid: string;
  source: string;
  snippet: string;
  matchRanges: [number, number][];
  score: number;
}

/** Returned when a search finishes (or is superseded / truncated by limit). */
export interface SearchSummary {
  hits: number;
  scanned: number;
  cancelled: boolean;
  truncated: boolean;
}

/** Cheap index status for the "indexing N/M…" indicator. */
export interface IndexStatus {
  totalSessions: number;
  indexedSessions: number;
  building: boolean;
}

/** Returned by Rust snapshot / list_backups. */
export interface BackupVersion {
  version: number;
  timestamp: number;
  path: string;
  size: number;
}

// ---------------------------------------------------------------------------
// CC Deck's own app preferences (mirrors src-tauri/src/appconfig.rs)
// ---------------------------------------------------------------------------

/** CC Deck's own preferences, persisted at ~/.ccdeck/config.json. Never Claude
 *  Code's own settings.json — the schema-driven editor for that was removed in
 *  v0.14 (issue #34); users hand-edit it themselves. Shrunk to a single pref
 *  now that the terminal launcher (terminal / launchCommand) is gone too. */
export interface AppConfig {
  /** Whether CC Deck checks for app updates automatically on launch. */
  updateCheckOnLaunch: boolean;
}
