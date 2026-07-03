/**
 * Reactive search store (Svelte 5 runes). Owns the query, toggles, filters, and
 * the streamed `hits` array. Each query bumps a monotonic `searchId`; hits from
 * a superseded search are ignored, so results only ever append (no flicker).
 */
import type {
  SearchHit,
  SearchOpts,
  SearchFilters,
  SearchSummary,
  IndexStatus,
} from './types';
import { searchSessions, indexStatus, refreshIndex, listSessions, homeDir } from './api';
import { projectLabel } from './parser';

/** Stop appending after this many hits (the backend still counts them all). */
const MAX_DISPLAY_HITS = 1000;

/** Light debounce so we don't launch a scan on every literal keystroke. */
const DEBOUNCE_MS = 110;

export interface ProjectOption {
  label: string;
  count: number;
}

export const search = $state({
  query: '',
  opts: { caseSensitive: false, wholeWord: false, regex: false } as SearchOpts,
  sources: ['user', 'assistant'] as string[], // "Messages" on by default
  from: null as number | null,
  to: null as number | null,
  projects: [] as string[], // selected project labels; empty = all
  hits: [] as SearchHit[],
  truncated: false,
  running: false,
  error: null as string | null,
  summary: null as SearchSummary | null,
  availableProjects: [] as ProjectOption[],
  status: null as IndexStatus | null,
});

let searchId = 0;
let debounceTimer: ReturnType<typeof setTimeout> | null = null;
let statusTimer: ReturnType<typeof setTimeout> | null = null;
let seen = new Set<string>();

function currentFilters(): SearchFilters {
  return {
    sources: search.sources,
    from: search.from,
    to: search.to,
    projects: search.projects,
  };
}

/** Debounced trigger — the normal entry point after any query/filter change. */
export function scheduleSearch(delay = DEBOUNCE_MS): void {
  if (debounceTimer) clearTimeout(debounceTimer);
  debounceTimer = setTimeout(runSearch, delay);
}

/** Run immediately (also used by the debounce timer). */
export async function runSearch(): Promise<void> {
  if (debounceTimer) {
    clearTimeout(debounceTimer);
    debounceTimer = null;
  }
  const id = ++searchId;
  search.hits = [];
  search.truncated = false;
  search.error = null;
  search.summary = null;
  seen = new Set();

  if (!search.query) {
    search.running = false;
    return;
  }
  search.running = true;

  const opts = { ...search.opts };
  const filters = currentFilters();

  try {
    const summary = await searchSessions(search.query, opts, filters, id, (hit) => {
      if (id !== searchId) return; // superseded by a newer search
      if (search.hits.length >= MAX_DISPLAY_HITS) {
        search.truncated = true;
        return;
      }
      const key = `${hit.sessionPath}:${hit.lineNo}:${hit.blockNo}`;
      if (seen.has(key)) return; // warm + cold tiers can overlap briefly
      seen.add(key);
      search.hits.push(hit);
    });
    if (id === searchId) {
      search.summary = summary;
      search.running = false;
    }
  } catch (e) {
    if (id === searchId) {
      search.error = e instanceof Error ? e.message : String(e);
      search.running = false;
    }
  }
}

// ── setters (all schedule a fresh search) ────────────────────────────────────

export function setQuery(q: string): void {
  search.query = q;
  scheduleSearch();
}

export function toggleOpt(k: keyof SearchOpts): void {
  search.opts[k] = !search.opts[k];
  scheduleSearch();
}

export function toggleSource(source: string): void {
  const i = search.sources.indexOf(source);
  if (i >= 0) search.sources.splice(i, 1);
  else search.sources.push(source);
  scheduleSearch();
}

export function toggleProject(label: string): void {
  const i = search.projects.indexOf(label);
  if (i >= 0) search.projects.splice(i, 1);
  else search.projects.push(label);
  scheduleSearch();
}

export function clearProjects(): void {
  search.projects = [];
  scheduleSearch();
}

/** Set the date range from `yyyy-mm-dd` inputs (or '' to clear a bound). */
export function setDateRange(fromISO: string, toISO: string): void {
  search.from = fromISO ? Date.parse(fromISO + 'T00:00:00') : null;
  // `to` is inclusive of the whole day.
  search.to = toISO ? Date.parse(toISO + 'T23:59:59.999') : null;
  scheduleSearch();
}

// ── lifecycle ────────────────────────────────────────────────────────────────

/** Load the project list, kick a sweep, and start polling index status. */
export async function initSearch(): Promise<void> {
  try {
    const [sessions, home] = await Promise.all([listSessions(), homeDir()]);
    const counts = new Map<string, number>();
    for (const s of sessions) {
      const label = projectLabel(s.cwd, s.project_raw, home);
      counts.set(label, (counts.get(label) ?? 0) + 1);
    }
    search.availableProjects = [...counts.entries()]
      .map(([label, count]) => ({ label, count }))
      .sort((a, b) => a.label.localeCompare(b.label));
  } catch {
    // non-fatal; project filter just stays empty
  }

  // Catch external changes (CLI appends, edits outside the app) when opening.
  refreshIndex().catch(() => {});
  pollStatus();
}

async function pollStatus(): Promise<void> {
  search.status = await indexStatus();
  if (search.status?.building) {
    // Results grow as the cache warms — re-run so the view keeps up.
    if (search.query) scheduleSearch(400);
    statusTimer = setTimeout(pollStatus, 1000);
  } else {
    statusTimer = null;
  }
}

/** Stop timers when leaving the search view. */
export function disposeSearch(): void {
  if (debounceTimer) clearTimeout(debounceTimer);
  if (statusTimer) clearTimeout(statusTimer);
  debounceTimer = null;
  statusTimer = null;
  // Bump the id so any in-flight stream is ignored.
  searchId++;
}
