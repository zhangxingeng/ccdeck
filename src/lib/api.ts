/**
 * Bridge to the native Rust commands via Tauri `invoke`, with a browser-dev
 * fallback so the full UI can be exercised in a plain browser (Vite dev) using
 * bundled mock fixtures. No data ever leaves the machine in either mode.
 */
import type { SessionMeta, SubagentFile, BackupVersion } from './types';

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
