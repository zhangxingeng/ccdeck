/**
 * Bridge to the native Rust commands via Tauri `invoke`, with a browser-dev
 * fallback so the full UI can be exercised in a plain browser (Vite dev) using
 * bundled mock fixtures. No data ever leaves the machine in either mode.
 */
import type {
  SessionMeta,
  SubagentFile,
  BackupVersion,
  SearchOpts,
  SearchFilters,
  SearchHit,
  SearchSummary,
  IndexStatus,
  ClaudeSettings,
  SettingsTier,
  AppConfig,
} from './types';

// Bundled mock fixtures for browser-dev mode (Vite ?raw import).
import mockSession from '../../tests/mock_data/session.jsonl?raw';
import mockAgent from '../../tests/mock_data/subagents/agent-audit-secret.jsonl?raw';
import mockAgentMeta from '../../tests/mock_data/subagents/agent-audit-secret.meta.json?raw';

export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(cmd, args);
}

// In-memory backup store for browser-dev mode only.
const devBackups: Record<string, BackupVersion[]> = {};
const devContent: Record<string, string> = {};
// In-memory edit draft store for browser-dev mode only.
const devDrafts = new Map<string, string>();

export async function findProjectsDir(): Promise<string | null> {
  if (!isTauri()) return '/dev/mock/.claude/projects';
  return call<string | null>('find_projects_dir');
}

/** The user's home directory — used to render absolute paths as "~/...". */
export async function homeDir(): Promise<string | null> {
  if (!isTauri()) return '/dev/mock';
  return call<string | null>('home_dir');
}

/** Open a file with the OS default app (e.g. "View file" on a session's .jsonl). */
export async function openSessionFile(path: string): Promise<void> {
  if (!isTauri()) return; // no-op in browser-dev mode
  const { openPath } = await import('@tauri-apps/plugin-opener');
  await openPath(path);
}

export async function listSessions(): Promise<SessionMeta[]> {
  if (!isTauri()) {
    return [
      {
        id: 'demo-project/session.jsonl',
        path: '/dev/mock/session.jsonl',
        project_raw: '-home-dev-demo-project',
        mtime: 1751300000,
        size: mockSession.length,
        preview: mockSession.split('\n').slice(0, 50),
        line_count: mockSession.split('\n').filter((l) => l.trim().length > 0).length,
        user_count: 3,
        assistant_count: 3,
        subagent_count: 1,
        models: ['claude-sonnet-4-6'],
        first_ts: '2025-06-01T10:00:00.000Z',
        last_ts: '2025-06-01T10:05:00.000Z',
        cwd: '/dev/mock/demo-project',
        custom_title: '',
      },
    ];
  }
  return call<SessionMeta[]>('list_sessions');
}

export async function readSession(path: string): Promise<string> {
  if (!isTauri()) return devContent[path] ?? mockSession;
  return call<string>('read_session', { path });
}

export async function readSubagents(sessionPath: string): Promise<SubagentFile[]> {
  if (!isTauri()) {
    return [
      { name: 'agent-audit-secret.jsonl', content: mockAgent, is_meta: false },
      { name: 'agent-audit-secret.meta.json', content: mockAgentMeta, is_meta: true },
    ];
  }
  return call<SubagentFile[]>('read_subagents', { sessionPath });
}

export async function writeSession(path: string, content: string): Promise<void> {
  if (!isTauri()) {
    devContent[path] = content;
    return;
  }
  await call<null>('write_session', { path, content });
}

export async function snapshot(path: string): Promise<BackupVersion> {
  if (!isTauri()) {
    const list = (devBackups[path] ??= []);
    const v: BackupVersion = {
      version: list.length + 1,
      timestamp: Math.floor(Date.now() / 1000),
      path: `${path}.v${list.length + 1}.bak`,
      size: (devContent[path] ?? mockSession).length,
    };
    list.unshift(v);
    return v;
  }
  return call<BackupVersion>('snapshot', { path });
}

export async function listBackups(sessionPath: string): Promise<BackupVersion[]> {
  if (!isTauri()) return devBackups[sessionPath] ?? [];
  return call<BackupVersion[]>('list_backups', { sessionPath });
}

export async function restoreBackup(backupPath: string): Promise<string> {
  if (!isTauri()) return mockSession;
  return call<string>('restore_backup', { backupPath });
}

export async function readEditDraft(path: string): Promise<string | null> {
  if (!isTauri()) return devDrafts.get(path) ?? null;
  return call<string | null>('read_edit_draft', { sessionPath: path });
}

export async function writeEditDraft(path: string, content: string): Promise<void> {
  if (!isTauri()) {
    devDrafts.set(path, content);
    return;
  }
  await call<null>('write_edit_draft', { sessionPath: path, content });
}

export async function deleteEditDraft(path: string): Promise<void> {
  if (!isTauri()) {
    devDrafts.delete(path);
    return;
  }
  await call<null>('delete_edit_draft', { sessionPath: path });
}

// ---------------------------------------------------------------------------
// Claude Code settings (schema-driven editor)
// ---------------------------------------------------------------------------

/** Browser-dev mock store, keyed by `${tier}:${projectCwd ?? ''}` — seeded with a
 *  deliberate `model` conflict between user and project tiers so the conflict
 *  banner is exercisable offline. */
const devSettingsStore: Record<string, Record<string, unknown>> = {
  'user:': { model: 'claude-opus-4-8', theme: 'dark' },
  'project:/dev/mock/demo-project': { model: 'claude-sonnet-5', outputStyle: 'Explanatory' },
  'local:/dev/mock/demo-project': { includeCoAuthoredBy: false },
};

function devTierPath(tier: SettingsTier, projectCwd: string | null): string {
  if (tier === 'user') return '/dev/mock/.claude/settings.json';
  const file = tier === 'local' ? 'settings.local.json' : 'settings.json';
  return `${projectCwd ?? '/dev/mock/demo-project'}/.claude/${file}`;
}

function devReadClaudeSettings(projectCwd: string | null): ClaudeSettings {
  const tierNames: SettingsTier[] = projectCwd ? ['local', 'project', 'user'] : ['user'];
  const tiers = tierNames.map((tier) => {
    const key = `${tier}:${projectCwd ?? ''}`;
    const parsed = devSettingsStore[key] ?? null;
    return {
      tier,
      path: devTierPath(tier, projectCwd),
      exists: parsed !== null,
      raw: parsed ? JSON.stringify(parsed, null, 2) : '',
      parsed,
      parseError: null,
    };
  });

  const effective: Record<string, unknown> = {};
  const conflicts: ClaudeSettings['conflicts'] = [];
  const keys = new Set<string>();
  for (const t of tiers) if (t.parsed) for (const k of Object.keys(t.parsed)) keys.add(k);
  for (const key of keys) {
    const present = tiers.filter((t) => t.parsed && key in t.parsed);
    if (present.length === 0) continue;
    effective[key] = present[0].parsed![key];
    const distinct = present.some((t) => JSON.stringify(t.parsed![key]) !== JSON.stringify(present[0].parsed![key]));
    if (present.length >= 2 && distinct) {
      conflicts.push({
        key,
        tierValues: present.map((t) => ({ tier: t.tier, value: t.parsed![key] })),
        winner: present[0].tier,
      });
    }
  }

  return { tiers, effective, conflicts, projectCwd };
}

/** Read Claude Code settings across all applicable tiers for an optional project.
 *  With no `projectCwd`, only the user/global tier is read. */
export async function readClaudeSettings(projectCwd: string | null): Promise<ClaudeSettings> {
  if (!isTauri()) return devReadClaudeSettings(projectCwd);
  return call<ClaudeSettings>('read_claude_settings', { projectCwd });
}

/** Write exactly one tier's settings file. Never merges. */
export async function writeClaudeSettings(
  tier: SettingsTier,
  projectCwd: string | null,
  value: Record<string, unknown>
): Promise<void> {
  if (!isTauri()) {
    devSettingsStore[`${tier}:${projectCwd ?? ''}`] = value;
    return;
  }
  await call<null>('write_claude_settings', { tier, projectCwd, value });
}

// ---------------------------------------------------------------------------
// CC Deck app preferences (terminal launcher)
// ---------------------------------------------------------------------------

let devAppConfig: AppConfig = { terminal: '', terminalArgs: '' };

/** CC Deck's own launcher preference (terminal choice + extra args). */
export async function getAppConfig(): Promise<AppConfig> {
  if (!isTauri()) return devAppConfig;
  return call<AppConfig>('get_app_config');
}

export async function setAppConfig(config: AppConfig): Promise<void> {
  if (!isTauri()) {
    devAppConfig = config;
    return;
  }
  await call<null>('set_app_config', { config });
}

// ---------------------------------------------------------------------------
// Resume ("open in claude --resume")
// ---------------------------------------------------------------------------

export interface ForkResult {
  path: string;
  id: string;
}

/** Fork the session at `path`, keeping only lines 0..=uptoIndex, under a fresh session id. */
export async function forkSession(path: string, uptoIndex: number): Promise<ForkResult> {
  if (!isTauri()) return { path: `${path}.fork`, id: 'dev-mock-fork-id' };
  return call<ForkResult>('fork_session', { path, uptoIndex });
}

/** Best-effort: open a terminal in `cwd` running `claude --resume <sessionId>`. */
export async function resumeInTerminal(cwd: string, sessionId: string): Promise<void> {
  if (!isTauri()) throw new Error('Not available outside the desktop app');
  await call<null>('resume_in_terminal', { cwd, sessionId });
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

/**
 * Streaming search. Each hit is delivered to `onHit` as it's found (via a Tauri
 * v2 Channel); the promise resolves with a summary when the scan completes or is
 * superseded. In browser-dev mode, a small JS scan over the mock session stands
 * in so the UI is exercisable without the native backend.
 */
export async function searchSessions(
  query: string,
  opts: SearchOpts,
  filters: SearchFilters,
  searchId: number,
  limit: number,
  onHit: (hit: SearchHit) => void
): Promise<SearchSummary> {
  if (!isTauri()) return devSearch(query, opts, filters, limit, onHit);

  const { invoke, Channel } = await import('@tauri-apps/api/core');
  const channel = new Channel<SearchHit>();
  channel.onmessage = onHit;
  return invoke<SearchSummary>('search', {
    query,
    opts,
    filters,
    searchId,
    limit,
    onHit: channel,
  });
}

/** Kick a background (re)index sweep. */
export async function refreshIndex(): Promise<IndexStatus | null> {
  if (!isTauri()) return null;
  return call<IndexStatus>('refresh_index');
}

/** Cheap index status for the "indexing…" indicator. */
export async function indexStatus(): Promise<IndexStatus | null> {
  if (!isTauri()) return null;
  return call<IndexStatus>('index_status');
}

// --- Browser-dev fallback: a minimal in-JS scan over the mock session. -------

function buildDevRegex(query: string, opts: SearchOpts): RegExp | null {
  let pattern = opts.regex ? query : query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  if (opts.wholeWord) pattern = `\\b${pattern}\\b`;
  try {
    return new RegExp(pattern, opts.caseSensitive ? 'g' : 'gi');
  } catch {
    return null;
  }
}

async function devSearch(
  query: string,
  opts: SearchOpts,
  filters: SearchFilters,
  limit: number,
  onHit: (hit: SearchHit) => void
): Promise<SearchSummary> {
  const summary: SearchSummary = { hits: 0, scanned: 0, cancelled: false, truncated: false };
  if (!query) return summary;
  const re = buildDevRegex(query, opts);
  if (!re) return summary;

  const { parseJsonl } = await import('./parser');
  const entries = parseJsonl(mockSession);
  const project = '~/dev/demo-project';
  const sessionPath = '/dev/mock/session.jsonl';
  if (filters.projects.length && !filters.projects.includes(project)) return summary;
  if (filters.sessionPath && filters.sessionPath !== sessionPath) return summary;

  DEV: for (const [lineNo, entry] of entries.entries()) {
    for (const [blockNo, b] of entry.blocks.entries()) {
      const source =
        b.blockType === 'text' ? entry.type
        : b.blockType === 'thinking' ? 'thinking'
        : b.blockType; // tool_use | tool_result
      const text = b.text ?? b.thinking ?? b.toolOutput ?? b.toolName ?? '';
      if (filters.toolName) {
        if (source !== 'tool_use' || !(text === filters.toolName || text.startsWith(`${filters.toolName}\n`))) continue;
      } else if (filters.sources.length && !filters.sources.includes(source)) {
        continue;
      }
      re.lastIndex = 0;
      summary.scanned++;
      const ranges: [number, number][] = [];
      let m: RegExpExecArray | null;
      while ((m = re.exec(text)) !== null) {
        ranges.push([m.index, m.index + m[0].length]);
        if (m[0].length === 0) re.lastIndex++;
      }
      if (!ranges.length) continue;
      onHit({
        sessionPath,
        project,
        ts: entry.timestamp ? Date.parse(entry.timestamp) : null,
        lineNo,
        blockNo,
        uuid: entry.uuid,
        source,
        snippet: text.slice(0, 240),
        matchRanges: ranges.filter(([s]) => s < 240) as [number, number][],
      });
      summary.hits++;
      if (summary.hits >= limit) {
        summary.truncated = true;
        break DEV;
      }
    }
  }
  return summary;
}
