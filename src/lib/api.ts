/**
 * Bridge to the native Rust commands via Tauri `invoke`, with a browser-dev
 * fallback so the full UI can be exercised in a plain browser (Vite dev) using
 * bundled mock fixtures. No data ever leaves the machine in either mode.
 */
import type {
  SessionMeta,
  SessionStub,
  SessionEnrichment,
  EnrichSummary,
  BackupVersion,
  SearchFilters,
  SearchHit,
  SearchSummary,
  IndexStatus,
  AppConfig,
} from './types';
import { stubToMeta } from './browseLoad';

// Bundled mock fixtures for browser-dev mode (Vite ?raw import).
import mockSession from '../../tests/mock_data/session.jsonl?raw';

export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(cmd, args);
}

// In-memory edited-content store for browser-dev mode only.
const devContent: Record<string, string> = {};

export async function findProjectsDir(): Promise<string | null> {
  if (!isTauri()) return '/dev/mock/.claude/projects';
  return call<string | null>('find_projects_dir');
}

/** The user's home directory — used to render absolute paths as "~/...". */
export async function homeDir(): Promise<string | null> {
  if (!isTauri()) return '/dev/mock';
  return call<string | null>('home_dir');
}

// ── Browse loading: tier-1 stubs (instant) + tier-2 streamed enrichment ──────
//
// `list_sessions` now returns stat-only stubs so the list paints immediately;
// the content-derived fields stream in afterward via `enrich_sessions`. Both
// tiers share one dev fixture (mock_fidelity: one rich exemplar, siblings
// derived) so the browser-dev mock can't contradict the real backend.

const MOCK_SESSION_PATH = '/dev/mock/session.jsonl';

const mockStub: SessionStub = {
  id: 'demo-project/session.jsonl',
  path: MOCK_SESSION_PATH,
  project_raw: '-home-dev-demo-project',
  mtime: 1751300000,
  size: mockSession.length,
};

/** The enrichment the real backend would stream for `mockStub`, derived from
 *  the same fixture so the two can't drift. */
const mockEnrichment: SessionEnrichment = {
  path: MOCK_SESSION_PATH,
  cleaned: false,
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
};

/** Tier-1: stat-only session rows, painted immediately in recency order. The
 *  content fields are empty until `enrichSessions` streams them in. */
export async function listSessions(): Promise<SessionMeta[]> {
  const stubs = isTauri()
    ? await call<SessionStub[]>('list_sessions')
    : [mockStub];
  return stubs.map(stubToMeta);
}

/**
 * Tier-2: stream content-derived metadata for every session as each file is
 * scanned, so a large history never blocks first paint. Each `SessionEnrichment`
 * is delivered to `onMeta` as it arrives (via a Tauri v2 Channel); a `cleaned`
 * payload means that file was auto-removed as junk and its stub should be
 * dropped. `enrichId` lets a newer call (a remount) supersede an in-flight one.
 * The promise resolves with a summary when the walk finishes or is superseded.
 * In browser-dev mode a small stand-in emits the single mock session.
 */
export async function enrichSessions(
  enrichId: number,
  onMeta: (e: SessionEnrichment) => void
): Promise<EnrichSummary> {
  if (!isTauri()) {
    onMeta(mockEnrichment);
    return { enriched: 1, cleaned: 0, cancelled: false };
  }
  const { invoke, Channel } = await import('@tauri-apps/api/core');
  const channel = new Channel<SessionEnrichment>();
  channel.onmessage = onMeta;
  return invoke<EnrichSummary>('enrich_sessions', { enrichId, onMeta: channel });
}

export async function readSession(path: string): Promise<string> {
  if (!isTauri()) return devContent[path] ?? mockSession;
  return call<string>('read_session', { path });
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
    // Mirrors the Rust side's single-slot backup: each call replaces the
    // previous one instead of growing a list.
    const v: BackupVersion = {
      version: 1,
      timestamp: Math.floor(Date.now() / 1000),
      path: `${path}.v1.bak`,
      size: (devContent[path] ?? mockSession).length,
    };
    return v;
  }
  return call<BackupVersion>('snapshot', { path });
}

// ---------------------------------------------------------------------------
// CC Deck app preferences (App Config: update-check-on-launch toggle)
// ---------------------------------------------------------------------------

let devAppConfig: AppConfig = {
  updateCheckOnLaunch: true,
};

/** CC Deck's own App Config preferences (just the update-check-on-launch toggle
 *  now that the terminal launcher is gone) — never Claude Code's own settings.json. */
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
// Fork ("resume from here")
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
  filters: SearchFilters,
  searchId: number,
  limit: number,
  onHit: (hit: SearchHit) => void
): Promise<SearchSummary> {
  if (!isTauri()) return devSearch(query, filters, limit, onHit);

  const { invoke, Channel } = await import('@tauri-apps/api/core');
  const channel = new Channel<SearchHit>();
  channel.onmessage = onHit;
  return invoke<SearchSummary>('search', {
    query,
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
// Plain case-insensitive substring match — a stand-in for the real fuzzy/intent
// engine (issue #5), not a faithful reimplementation of it. Good enough to
// exercise the UI in `pnpm dev` / Playwright without the native backend.

function buildDevMatcher(query: string): RegExp | null {
  const escaped = query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  try {
    return new RegExp(escaped, 'gi');
  } catch {
    return null;
  }
}

async function devSearch(
  query: string,
  filters: SearchFilters,
  limit: number,
  onHit: (hit: SearchHit) => void
): Promise<SearchSummary> {
  const summary: SearchSummary = { hits: 0, scanned: 0, cancelled: false, truncated: false };
  if (!query) return summary;
  const re = buildDevMatcher(query);
  if (!re) return summary;

  const { parseJsonl } = await import('./parser');
  const entries = parseJsonl(mockSession);
  const project = '~/dev/demo-project';
  const sessionPath = '/dev/mock/session.jsonl';
  if (filters.projects.length && !filters.projects.includes(project)) return summary;
  if (filters.sessionPath && filters.sessionPath !== sessionPath) return summary;

  DEV: for (const [lineNo, entry] of entries.entries()) {
    for (const [blockNo, b] of entry.blocks.entries()) {
      // Messages-only search (#35): only text blocks are indexed; source is
      // just the entry's role (user/assistant). No source/tool-name filter.
      const source = entry.type;
      const text = b.text ?? '';
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
        score: ranges.length,
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
