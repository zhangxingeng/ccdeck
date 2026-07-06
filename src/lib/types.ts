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

/** VS Code-style search toggles. */
export interface SearchOpts {
  caseSensitive: boolean;
  wholeWord: boolean;
  regex: boolean;
}

/** Query-time filters. Empty sources/projects mean "no restriction". */
export interface SearchFilters {
  sources: string[];             // low-level: user|assistant|thinking|tool_use|tool_result
  from: number | null;           // inclusive epoch-ms lower bound
  to: number | null;             // inclusive epoch-ms upper bound
  projects: string[];            // home-relative project labels
  toolName: string | null;       // restrict to tool_use blocks for this exact tool name; overrides `sources`
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

// ---------------------------------------------------------------------------
// Claude Code settings (mirrors src-tauri/src/settings.rs)
// ---------------------------------------------------------------------------

/** Precedence high -> low: local wins over project wins over user. */
export type SettingsTier = 'local' | 'project' | 'user';

/** One settings file tier as read from disk. */
export interface SettingsTierData {
  tier: SettingsTier;
  path: string;
  exists: boolean;
  raw: string;
  parsed: Record<string, unknown> | null;
  parseError: string | null;
}

/** One tier's value for a key set in more than one tier. */
export interface SettingsConflictValue {
  tier: SettingsTier;
  value: unknown;
}

/** A key set with differing values across tiers — the "what's set where" signal. */
export interface SettingsConflict {
  key: string;
  tierValues: SettingsConflictValue[];
  winner: SettingsTier;
}

/** Full picture returned by read_claude_settings: every tier + merge + conflicts. */
export interface ClaudeSettings {
  tiers: SettingsTierData[];
  effective: Record<string, unknown>;
  conflicts: SettingsConflict[];
  projectCwd: string | null;
}

// ---------------------------------------------------------------------------
// CC Deck's own app preferences (mirrors src-tauri/src/appconfig.rs)
// ---------------------------------------------------------------------------

/** Terminal launcher preference, persisted at ~/.claude/.ccstudio-config.json. */
export interface AppConfig {
  /** "" or "auto" = auto-detect (default). Otherwise a terminal command template. */
  terminal: string;
  /** Extra args appended to the `claude` invocation, e.g. "--dangerously-skip-permissions". */
  terminalArgs: string;
}
