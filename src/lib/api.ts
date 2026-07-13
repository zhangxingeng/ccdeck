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
import type { Snippet, MatchHit, Project, ProjectList } from './prompts/types';

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

let devAppConfig: AppConfig = {
  terminal: '',
  launchCommand: '',
  updateCheckOnLaunch: true,
};

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
// Prompt Library — snippets (Markdown files) and projects (folders).
//
// The seam to Rust: this file and prompts/types.ts mirror src-tauri/src/prompts/
// and have one author, because `pnpm check` cannot verify a Rust↔TS command
// signature — a drift here fails at runtime, not at build. All payloads are
// serde-default snake_case; Tauri camelCases the invoke argument keys.
//
// Embedding has no command surface: the model downloads and indexes itself in
// the background, silently, and a failure degrades to lexical match with nothing
// for the user to see or decide.
// ---------------------------------------------------------------------------

/** The project roster plus the active project (persisted by the backend and
 *  restored on launch, so it rides along rather than living in frontend state). */
export async function listProjects(): Promise<ProjectList> {
  if (!isTauri()) return { projects: devProjects.map((p) => ({ ...p })), active: devActive };
  return call<ProjectList>('list_projects');
}

/** Register a folder as a project. Re-adding a path renames it — the path is the
 *  project's identity. The folder must already exist. */
export async function addProject(name: string, path: string): Promise<Project> {
  if (!isTauri()) return devAddProject(name, path);
  return call<Project>('add_project', { name, path });
}

/** Forget a project. **Never deletes files** — the user's prompts are their own,
 *  and the app is a viewer onto a folder it does not own. Re-adding the folder
 *  restores the project intact. */
export async function removeProject(path: string): Promise<void> {
  if (!isTauri()) {
    devRemoveProject(path);
    return;
  }
  await call<null>('remove_project', { path });
}

/** Persisted; restored on launch. */
export async function setActiveProject(path: string): Promise<void> {
  if (!isTauri()) {
    devActive = path;
    return;
  }
  await call<null>('set_active_project', { path });
}

/** Every `*.md` under the project folder, recursively — each one a snippet whose
 *  name is its path minus the extension. */
export async function listSnippets(project: string): Promise<Snippet[]> {
  if (!isTauri()) return (devStore[project] ?? []).map((s) => ({ ...s }));
  return call<Snippet[]>('list_snippets', { project });
}

/** Write `<project>/<name>.md`. Same name updates that file; a new name creates a
 *  new snippet — which is the whole of "Save as new". A slashed name creates its
 *  parent folders. */
export async function saveSnippet(
  project: string,
  name: string,
  content: string
): Promise<Snippet> {
  if (!isTauri()) return devSaveSnippet(project, name, content);
  return call<Snippet>('save_snippet', { project, name, content });
}

export async function deleteSnippet(project: string, name: string): Promise<void> {
  if (!isTauri()) {
    devDeleteSnippet(project, name);
    return;
  }
  await call<null>('delete_snippet', { project, name });
}

/** Rank the project's snippets against `query`.
 *
 *  **An empty query returns everything, most-recently-used first** (then the
 *  never-used, alphabetically) — the list filters *down*, not up. Which engine
 *  ran is the backend's business; callers only see the hit list. */
export async function matchSnippets(
  project: string,
  query: string,
  limit: number
): Promise<MatchHit[]> {
  if (!isTauri()) return devMatchSnippets(project, query, limit);
  return call<MatchHit[]>('match_snippets', { project, query, limit });
}

/** Record that a snippet was used — this is what orders the at-rest list. It
 *  writes to app-local state, never into the project folder, which is git-tracked:
 *  a timestamp written into a `.md` file would dirty the user's git tree on every
 *  insert. */
export async function touchSnippet(project: string, name: string): Promise<void> {
  if (!isTauri()) {
    devUsage[`${project}::${name}`] = Math.floor(Date.now() / 1000);
    return;
  }
  await call<null>('touch_snippet', { project, name });
}

// --- Browser-dev prompt store ------------------------------------------------
// Real folder/file semantics over seeded samples, so `pnpm dev` exercises the
// whole Prompts view with no native shell — this is how the founder feel-checks
// and how the frontend lanes develop.
//
// The seeded library doubles as the README's screenshot set, so the content is
// real prompt text rather than placeholder prose: a reader should learn what the
// feature is *for* from a screenshot alone. Coverage is deliberate — snippets in
// subfolders and at the top level (folders are the whole organization system
// now), with and without variables, a repeated variable across two snippets (they
// share one value), and one containing a fenced code block (which the variable
// grammar must treat as verbatim).

const devProjects: Project[] = [
  { name: 'ccdeck', path: '/dev/mock/ccdeck' },
  { name: 'writing', path: '/dev/mock/writing' },
  { name: 'research', path: '/dev/mock/research' },
];

let devActive: string | null = '/dev/mock/ccdeck';

/** `<project path>::<snippet name>` → last-used epoch seconds. Deliberately kept
 *  out of the snippet objects, mirroring the backend: usage is app state, not
 *  something that belongs in a git-tracked prompt file. */
const devUsage: Record<string, number> = {
  '/dev/mock/ccdeck::review/senior-reviewer': 1751300000,
  '/dev/mock/ccdeck::debug/bug-repro-first': 1751200000,
};

const devStore: Record<string, Snippet[]> = {
  '/dev/mock/ccdeck': [
    {
      name: 'review/senior-reviewer',
      content:
        'You are a senior reviewer. Be rigorous about correctness, but do not nitpick style that a formatter owns. Say plainly when something is fine.',
    },
    {
      name: 'review/pr-checklist',
      content:
        'Review the PR for {ticket}. Focus especially on {concern}. Check error handling, tests, and naming. Flag anything that reads as a silent failure.',
    },
    {
      name: 'debug/bug-repro-first',
      content:
        'Before proposing a fix for {symptom}, write the smallest failing test that reproduces it. If you cannot reproduce it, say so instead of guessing.',
    },
    {
      name: 'testing/test-plan',
      content:
        'Write a test plan for {surface}. Cover the happy path once, then spend the rest of your effort on {risk} — the cases where a bug would be silent.',
    },
    {
      name: 'refactor/refactor-safely',
      content:
        'Refactor {target} without changing behavior. Land the characterization tests first, then move code. If a test is hard to write, that is the design talking.',
    },
    {
      name: 'rust/new-tauri-command',
      content:
        'Add a Rust command `{command_name}` and its TypeScript mirror. Both sides of the seam change together, or the type checker will not catch the drift:\n\n```rust\n#[tauri::command]\npub async fn {command_name}(project: String) -> Result<(), String> {\n    todo!()\n}\n```\n\nA body is a Python-style format string, uniformly: `{command_name}` is substituted everywhere, code fences included — which is exactly what you want here. To emit a literal brace, double it: `{{` and `}}`.',
    },
    {
      name: 'architecture',
      content:
        'ccdeck is a Tauri + Svelte 5 desktop app: Rust owns the filesystem and search index, the frontend owns rendering. Prefer the existing store idioms over new ones.',
    },
    {
      name: 'release-notes-draft',
      content:
        'Draft release notes for {version}. Lead with what a user can now do that they could not before. Migrations and breaking changes go first, not last.',
    },
    { name: 'style/be-terse', content: 'Be terse and concrete. Lead with the answer; skip preamble and hedging.' },
  ],
  '/dev/mock/writing': [
    {
      name: 'tone-notes',
      content: 'Prefer plain words over jargon. Say {audience} when addressing the reader.',
    },
    {
      name: 'headline-rewrite',
      content:
        'Rewrite {draft} three ways: one that states the outcome, one that names the reader, one that asks the question they already have. No clickbait.',
    },
    {
      name: 'cut-it-in-half',
      content:
        'Cut this by half without losing an idea. Delete throat-clearing, restatement, and any sentence that only announces the next one.',
    },
    {
      name: 'explain-like-staff-eng',
      content:
        'Explain {topic} to a strong engineer who has never touched it. Lead with what it is for, then how it works. No analogies to food.',
    },
  ],
  '/dev/mock/research': [
    {
      name: 'literature-scan',
      content:
        'Survey the {n} strongest sources on {question}. For each: the claim, the evidence, and the strongest objection to it. Mark what you could not verify.',
    },
    {
      name: 'steelman-then-rebut',
      content:
        'State the strongest version of {claim} — the one its smartest advocate would recognize. Only then argue against it. A rebuttal of a weak version proves nothing.',
    },
    {
      name: 'weekend-scope-guard',
      content:
        'This is a weekend project. Name the one thing it must do by Sunday, and the things you are deliberately not building.',
    },
  ],
};

function devAddProject(name: string, path: string): Project {
  const existing = devProjects.find((p) => p.path === path);
  if (existing) {
    existing.name = name; // the path is the identity: re-adding is a rename
    return { ...existing };
  }
  const project: Project = { name, path };
  devProjects.push(project);
  devStore[path] ??= [];
  devActive ??= path;
  return { ...project };
}

/** Forgets the path. Never deletes files — `devStore` deliberately keeps the
 *  snippets, so re-adding the folder restores the project intact, exactly as the
 *  real filesystem would. */
function devRemoveProject(path: string): void {
  const i = devProjects.findIndex((p) => p.path === path);
  if (i < 0) return;
  devProjects.splice(i, 1);
  for (const key of Object.keys(devUsage)) {
    if (key.startsWith(`${path}::`)) delete devUsage[key];
  }
  if (devActive === path) devActive = devProjects[0]?.path ?? null;
}

function devSaveSnippet(project: string, name: string, content: string): Snippet {
  const snippets = (devStore[project] ??= []);
  const existing = snippets.find((s) => s.name === name);
  if (existing) {
    existing.content = content; // same name = same file = an update
    return { ...existing };
  }
  const snippet: Snippet = { name, content };
  snippets.push(snippet);
  snippets.sort((a, b) => a.name.localeCompare(b.name));
  return { ...snippet };
}

function devDeleteSnippet(project: string, name: string): void {
  const snippets = devStore[project] ?? [];
  const i = snippets.findIndex((s) => s.name === name);
  if (i >= 0) snippets.splice(i, 1);
  delete devUsage[`${project}::${name}`];
}

// A stand-in weighted fuzzy scorer for browser-dev — deliberately a fixture, not
// shared production logic: the real engine (fzf-style subsequence with field
// weights, plus hybrid fusion) lives in Rust. This one only needs to make the
// match panel behave believably in `pnpm dev`.
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

function devMatchSnippets(project: string, query: string, limit: number): MatchHit[] {
  const pool = devStore[project] ?? [];
  // Empty query returns EVERYTHING, most-recently-used first, then the never-used
  // alphabetically — the same rule the backend applies. The old behavior (empty
  // query → empty list) forced the user to type to see their own library.
  if (!query.trim()) {
    return [...pool]
      .sort((a, b) => {
        const aUsed = devUsage[`${project}::${a.name}`];
        const bUsed = devUsage[`${project}::${b.name}`];
        if (aUsed && bUsed) return bUsed - aUsed || a.name.localeCompare(b.name);
        if (aUsed) return -1;
        if (bUsed) return 1;
        return a.name.localeCompare(b.name);
      })
      .slice(0, limit)
      .map((s) => ({ name: s.name, score: 0 }));
  }
  const hits: MatchHit[] = [];
  for (const s of pool) {
    const score = Math.max(devFuzzyScore(query, s.name) * 3, devFuzzyScore(query, s.content));
    if (score > 0) hits.push({ name: s.name, score });
  }
  hits.sort((a, b) => b.score - a.score);
  return hits.slice(0, limit);
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
