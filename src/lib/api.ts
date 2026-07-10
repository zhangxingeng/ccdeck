/**
 * Bridge to the native Rust commands via Tauri `invoke`, with a browser-dev
 * fallback so the full UI can be exercised in a plain browser (Vite dev) using
 * bundled mock fixtures. No data ever leaves the machine in either mode.
 */
import type {
  SessionMeta,
  BackupVersion,
  SearchFilters,
  SearchHit,
  SearchSummary,
  IndexStatus,
  ClaudeSettings,
  SettingsTier,
  AppConfig,
  ProviderProfile,
  KeyBackend,
} from './types';
import type {
  Piece,
  PieceInput,
  PieceLoadError,
  MatchHit,
  EmbedStatus,
  EmbedProgress,
} from './prompts/types';

// Bundled mock fixtures for browser-dev mode (Vite ?raw import).
import mockSession from '../../tests/mock_data/session.jsonl?raw';

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

/**
 * Auto-clean zero-turn, untitled, stale session files, returning how many were
 * removed. Called from the browse-list scan just before `listSessions` so junk
 * never accumulates in the list. No-op in browser-dev mode (no real files).
 */
export async function cleanupEmptySessions(): Promise<number> {
  if (!isTauri()) return 0;
  return call<number>('cleanup_empty_sessions');
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
    devBackups[path] = [v];
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

// ---------------------------------------------------------------------------
// Claude Code settings (schema-driven editor)
// ---------------------------------------------------------------------------

/** Browser-dev mock store, keyed by `${tier}:${projectCwd ?? ''}` — seeded with a
 *  deliberate `model` conflict between user and project tiers so the conflict
 *  hint is exercisable offline. */
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

/** Write exactly one tier's settings file. Never merges.
 *
 *  `baseVersion` is the exact `raw` text last read for this tier (a
 *  `SettingsTierData.raw` from `readClaudeSettings`, `''` if the tier didn't
 *  exist yet) — the backend's optimistic read-modify-write guard. If the file
 *  changed on disk since then (e.g. the `claude` CLI wrote it concurrently),
 *  the write is refused with a `CONFLICT: ...`-prefixed error instead of
 *  silently overwriting the external change; callers should catch that and
 *  prompt the user to reload rather than retry as-is. */
export async function writeClaudeSettings(
  tier: SettingsTier,
  projectCwd: string | null,
  value: Record<string, unknown>,
  baseVersion: string
): Promise<void> {
  if (!isTauri()) {
    devSettingsStore[`${tier}:${projectCwd ?? ''}`] = value;
    return;
  }
  await call<null>('write_claude_settings', { tier, projectCwd, value, baseVersion });
}

/** True if an error thrown by `writeClaudeSettings` is the optimistic
 *  read-modify-write conflict (settings changed on disk since last read). */
export function isSettingsConflict(e: unknown): boolean {
  const msg = e instanceof Error ? e.message : String(e);
  return msg.startsWith('CONFLICT');
}

// ---------------------------------------------------------------------------
// CC Deck app preferences (App Config: terminal + launch command + update toggle)
// ---------------------------------------------------------------------------

let devAppConfig: AppConfig = { terminal: '', launchCommand: '', updateCheckOnLaunch: true };

/** CC Deck's own App Config preferences (terminal choice, resume-launch command,
 *  update-check-on-launch toggle) — never Claude Code's own settings.json. */
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

/** Best-effort: open a terminal in `cwd` running the configured resume-launch
 *  command (App Config), with CCDECK_SESSION_ID/CCDECK_SESSION_TITLE/CCDECK_CWD
 *  exported into its environment. When `providerName` names a saved provider
 *  profile (issue #21), its ANTHROPIC_* vars (incl. the keychain-stored key)
 *  are also exported around the command; omit it for the default account. */
export async function resumeInTerminal(
  cwd: string,
  sessionId: string,
  sessionTitle: string,
  providerName?: string
): Promise<void> {
  if (!isTauri()) throw new Error('Not available outside the desktop app');
  await call<null>('resume_in_terminal', { cwd, sessionId, sessionTitle, providerName });
}

// ---------------------------------------------------------------------------
// Provider profiles (issue #21) — alternate Anthropic-compatible providers.
// Keys never cross this boundary on read: the UI only ever writes a new key
// (setProviderKey) or asks whether one is set (providerKeyStatus).
// ---------------------------------------------------------------------------

// Browser-dev store: mirrors the backend's split (profile metadata vs keys) so
// the management UI is exercisable in a plain browser. Dev mode pretends the
// keychain is always available (probe → true), so the plaintext-opt-in branch
// is only reachable in the real desktop app on a keychain-less machine.
const devProviderProfiles: ProviderProfile[] = [];
const devProviderKeys: Record<string, string> = {};

/** List provider profiles (metadata only — never keys). */
export async function listProviderProfiles(): Promise<ProviderProfile[]> {
  if (!isTauri()) return devProviderProfiles.map((p) => ({ ...p }));
  return call<ProviderProfile[]>('list_provider_profiles');
}

/** Upsert a profile's metadata. `name` is the identity and immutable on edit —
 *  an existing profile is matched by name and only baseUrl/defaultModel change. */
export async function saveProviderProfile(profile: ProviderProfile): Promise<void> {
  if (!isTauri()) {
    const existing = devProviderProfiles.find((p) => p.name === profile.name);
    if (existing) {
      existing.baseUrl = profile.baseUrl;
      existing.defaultModel = profile.defaultModel;
    } else {
      devProviderProfiles.push({ ...profile, keyBackend: 'none' });
    }
    return;
  }
  await call<null>('save_provider_profile', { profile });
}

/** Delete a profile and cascade-remove its key from every store. */
export async function deleteProviderProfile(name: string): Promise<void> {
  if (!isTauri()) {
    const i = devProviderProfiles.findIndex((p) => p.name === name);
    if (i >= 0) devProviderProfiles.splice(i, 1);
    delete devProviderKeys[name];
    return;
  }
  await call<null>('delete_provider_profile', { name });
}

/** Write-only key set. Returns the backend actually used ('keychain' when the
 *  keychain probe passes; 'plaintext' only when it fails AND `allowPlaintext`).
 *  Rejects with a message starting `KEYCHAIN_UNAVAILABLE` when the keychain is
 *  unavailable and `allowPlaintext` is false — the UI turns that into the
 *  explicit plaintext opt-in prompt (never auto-retries with plaintext). */
export async function setProviderKey(
  name: string,
  key: string,
  allowPlaintext: boolean
): Promise<KeyBackend> {
  if (!isTauri()) {
    devProviderKeys[name] = key;
    const p = devProviderProfiles.find((pr) => pr.name === name);
    if (p) p.keyBackend = 'keychain';
    return 'keychain';
  }
  return call<KeyBackend>('set_provider_key', { name, key, allowPlaintext });
}

/** Whether a key is currently stored for this profile (in either backend).
 *  Never returns the key itself. */
export async function providerKeyStatus(name: string): Promise<boolean> {
  if (!isTauri()) return name in devProviderKeys;
  return call<boolean>('provider_key_status', { name });
}

/** Runtime-probe whether the OS keychain is usable (write+read+delete a
 *  throwaway value). Drives the plaintext-opt-in gating and mount pre-warning. */
export async function providerProbeKeychain(): Promise<boolean> {
  if (!isTauri()) return true;
  return call<boolean>('provider_probe_keychain');
}

/** True if a `setProviderKey` rejection is the keychain-unavailable signal
 *  (drives the "1% outlier" plaintext opt-in flow). */
export function isKeychainUnavailable(e: unknown): boolean {
  const msg = e instanceof Error ? e.message : String(e);
  return msg.includes('KEYCHAIN_UNAVAILABLE');
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

// ---------------------------------------------------------------------------
// Prompt Library (issue #24) — pieces, matching, opt-in embeddings.
// Contract: project_docs/prompts-design.md. All payloads are serde-default
// snake_case, like SessionMeta.
// ---------------------------------------------------------------------------

/** List every piece in the store (the corpus is small by design — the
 *  frontend filters by scope/project). */
export async function listPieces(): Promise<Piece[]> {
  if (!isTauri()) return devPieces.map((p) => structuredClone(p));
  return call<Piece[]>('list_pieces');
}

/** Create (no id) or update (id present) a piece. The backend owns derived
 *  fields: placeholders re-derived from the body, `versions` gets the prior
 *  body pushed on when the body changed (append-only — a save never destroys
 *  the previous body), timestamps. Returns the stored piece. */
export async function savePiece(piece: PieceInput): Promise<Piece> {
  if (!isTauri()) return devSavePiece(piece);
  return call<Piece>('save_piece', { piece });
}

export async function deletePiece(id: string): Promise<void> {
  if (!isTauri()) {
    const i = devPieces.findIndex((p) => p.id === id);
    if (i >= 0) devPieces.splice(i, 1);
    return;
  }
  await call<null>('delete_piece', { id });
}

/** Rank pieces against `query`. Pool: global pieces + pieces scoped to
 *  `project` (null = global only). Which engine ran (lexical / semantic /
 *  hybrid) is the backend's business — callers only see the hit list. */
export async function matchPieces(
  query: string,
  project: string | null,
  limit: number
): Promise<MatchHit[]> {
  if (!isTauri()) return devMatchPieces(query, project, limit);
  return call<MatchHit[]>('match_pieces', { query, project, limit });
}

/** Piece JSON files that failed to parse on the last load pass — surfaced so
 *  a hand-edit typo never reads as a silently vanished piece. */
export async function pieceLoadErrors(): Promise<PieceLoadError[]> {
  if (!isTauri()) return devPieceLoadErrors.map((e) => ({ ...e }));
  return call<PieceLoadError[]>('piece_load_errors');
}

export async function embedStatus(): Promise<EmbedStatus> {
  if (!isTauri()) return { ...devEmbed };
  return call<EmbedStatus>('embed_status');
}

/** Download the embedding model, streaming progress over a Channel (same
 *  pattern as `search`). Resolves when the download completes. */
export async function embedDownload(onProgress: (p: EmbedProgress) => void): Promise<void> {
  if (!isTauri()) return devEmbedDownload(onProgress);
  const { invoke, Channel } = await import('@tauri-apps/api/core');
  const channel = new Channel<EmbedProgress>();
  channel.onmessage = onProgress;
  await invoke<null>('embed_download', { onProgress: channel });
}

/** Persisted app-config toggle; "ready" + enabled = hybrid matching on. */
export async function setEmbedEnabled(enabled: boolean): Promise<void> {
  if (!isTauri()) {
    if (devEmbed.state === 'ready' && !enabled) devEmbed.state = 'off';
    else if (devEmbed.state === 'off' && enabled) devEmbed.state = 'ready';
    return;
  }
  await call<null>('set_embed_enabled', { enabled });
}

// --- Browser-dev piece store: real save/versioning semantics over seeded ----
// sample pieces, so `pnpm dev` exercises the whole Prompts view (including
// version history and the placeholder flow) with no native shell.

// One seeded broken-file case so the load-errors notice is exercisable in
// browser dev (dismiss it to see the common path).
const devPieceLoadErrors: PieceLoadError[] = [
  {
    file: '~/.ccdeck/prompts/broken-example.json',
    error: 'expected `,` or `}` at line 3 column 14',
  },
];

const devPieces: Piece[] = [
  {
    id: 'dev-piece-reviewer',
    title: 'senior-reviewer',
    body: 'You are a senior reviewer. Be rigorous about correctness, but do not nitpick style that a formatter owns.',
    keywords: ['review', 'role'],
    tags: [],
    category: null,
    scope: { kind: 'global' },
    placeholders: [],
    created_at: 1751000000,
    updated_at: 1751000000,
    versions: [],
  },
  {
    id: 'dev-piece-terse',
    title: 'be-terse',
    body: 'Be terse and concrete. Lead with the answer; skip preamble and hedging.',
    keywords: ['style', 'tone'],
    tags: [],
    category: null,
    scope: { kind: 'global' },
    placeholders: [],
    created_at: 1751000000,
    updated_at: 1751100000,
    versions: [{ body: 'Be terse.', saved_at: 1751000000 }],
  },
  {
    id: 'dev-piece-checklist',
    title: 'pr-review-checklist',
    body: 'Review the PR for {{ticket}}. Focus especially on {{concern}}. Check error handling, tests, and naming.',
    keywords: ['review', 'checklist', 'pr'],
    tags: [],
    category: null,
    scope: { kind: 'global' },
    placeholders: [{ name: 'ticket' }, { name: 'concern' }],
    created_at: 1751000000,
    updated_at: 1751000000,
    versions: [],
  },
  {
    id: 'dev-piece-project',
    title: 'demo-project-context',
    body: 'This project is a Tauri + Svelte desktop app. Prefer the existing store idioms over new abstractions.',
    keywords: ['context'],
    tags: [],
    category: null,
    scope: { kind: 'project', project: '/dev/mock/demo-project' },
    placeholders: [],
    created_at: 1751000000,
    updated_at: 1751000000,
    versions: [],
  },
];

async function devSavePiece(input: PieceInput): Promise<Piece> {
  const { parsePlaceholders } = await import('./compose/placeholders');
  const now = Math.floor(Date.now() / 1000);
  const placeholders = parsePlaceholders(input.body).map((name) => ({ name }));
  const existing = input.id ? devPieces.find((p) => p.id === input.id) : undefined;
  if (existing) {
    if (existing.body !== input.body) {
      existing.versions.unshift({ body: existing.body, saved_at: existing.updated_at });
    }
    existing.title = input.title;
    existing.body = input.body;
    existing.keywords = [...input.keywords];
    existing.tags = [...input.tags];
    existing.category = input.category;
    existing.scope = { ...input.scope };
    existing.placeholders = placeholders;
    existing.updated_at = now;
    return structuredClone(existing);
  }
  const piece: Piece = {
    id: crypto.randomUUID(),
    title: input.title,
    body: input.body,
    keywords: [...input.keywords],
    tags: [...input.tags],
    category: input.category,
    scope: { ...input.scope },
    placeholders,
    created_at: now,
    updated_at: now,
    versions: [],
  };
  devPieces.push(piece);
  return structuredClone(piece);
}

// A stand-in weighted fuzzy scorer for browser-dev — deliberately a fixture,
// not shared production logic: the real engine (fzf-style subsequence with
// field weights, optional hybrid fusion) lives in Rust. This one only needs
// to make the match panel behave believably in `pnpm dev`.
function devFuzzyScore(query: string, target: string): number {
  const q = query.toLowerCase();
  const t = target.toLowerCase();
  if (!q) return 0;
  if (t.includes(q)) return 100 + q.length * 2 - Math.min(20, t.length / 10);
  // Subsequence match: every query char in order, closer together = better.
  let ti = 0;
  let matched = 0;
  let gaps = 0;
  let last = -1;
  for (const ch of q) {
    if (ch === ' ') continue;
    const found = t.indexOf(ch, ti);
    if (found < 0) continue;
    matched++;
    if (last >= 0) gaps += found - last - 1;
    last = found;
    ti = found + 1;
  }
  const qLen = q.replace(/ /g, '').length;
  if (qLen === 0 || matched < qLen * 0.8) return 0;
  return Math.max(0, matched * 8 - gaps);
}

function devMatchPieces(query: string, project: string | null, limit: number): MatchHit[] {
  if (!query.trim()) return [];
  const pool = devPieces.filter(
    (p) => p.scope.kind === 'global' || (project !== null && p.scope.project === project)
  );
  const hits: MatchHit[] = [];
  for (const p of pool) {
    const score = Math.max(
      devFuzzyScore(query, p.title) * 3,
      Math.max(0, ...p.keywords.concat(p.tags).map((k) => devFuzzyScore(query, k))) * 2,
      devFuzzyScore(query, p.body)
    );
    if (score > 0) hits.push({ id: p.id, score, source: 'lexical' });
  }
  hits.sort((a, b) => b.score - a.score);
  return hits.slice(0, limit);
}

// Fake embed engine: not_downloaded → (Download) → downloading with staged
// progress → ready. Lets the whole embeddings UI be exercised offline.
let devEmbed: EmbedStatus = {
  state: 'not_downloaded',
  model_id: 'bge-small-en-v1.5',
  model_size_mb: 85,
};

async function devEmbedDownload(onProgress: (p: EmbedProgress) => void): Promise<void> {
  devEmbed.state = 'downloading';
  // Two stages with per-stage totals — the pinned Channel contract: the ONNX
  // runtime dylib first, then the model itself. Completion is signaled by
  // this promise resolving (callers re-fetch embed_status), never by a
  // channel event.
  const stages: { stage: EmbedProgress['stage']; totalMb: number }[] = [
    { stage: 'runtime', totalMb: 30 },
    { stage: 'model', totalMb: devEmbed.model_size_mb },
  ];
  for (const { stage, totalMb } of stages) {
    const total = totalMb * 1024 * 1024;
    for (let step = 1; step <= 6; step++) {
      await new Promise((r) => setTimeout(r, 150));
      onProgress({ stage, downloaded_bytes: Math.round((total * step) / 6), total_bytes: total });
    }
  }
  devEmbed.state = 'ready';
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
      // Only 'text' blocks exist now (thinking/tool_use/tool_result rendering
      // was removed), so the source is just the entry's role and a toolName
      // restriction can never match.
      const source = entry.type;
      const text = b.text ?? '';
      if (filters.toolName) {
        continue;
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
