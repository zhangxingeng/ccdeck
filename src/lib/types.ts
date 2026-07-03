/**
 * Data model interfaces for Claude Code chat sessions.
 * Pure TypeScript — no DOM, no Tauri, no Svelte.
 */

export interface ContentBlock {
  blockType: 'thinking' | 'text' | 'tool_use' | 'tool_result';
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
  isAsync?: boolean;
  // Agent spawn
  agentId?: string;
  subagent?: Session;
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
}

/** Returned by Rust read_subagents. */
export interface SubagentFile {
  name: string;
  content: string;
  is_meta: boolean;
}

/** Returned by Rust snapshot / list_backups. */
export interface BackupVersion {
  version: number;
  timestamp: number;
  path: string;
  size: number;
}
