/**
 * Auto-update — reactive state driving the in-app update UI.
 *
 * Checks GitHub Releases for a newer signed build and installs it in-app
 * (see UpdateBanner.svelte + +layout.svelte). The update check is the only
 * network request the app ever makes; everything else is fully offline. Any
 * failure (offline, GitHub unreachable, no release yet) is swallowed so it can
 * never block the app from starting.
 *
 * The `@tauri-apps/*` imports are static but import-safe: they only touch the
 * Tauri IPC bridge when their functions are actually called, and every caller
 * guards on `__TAURI_INTERNALS__` first.
 */
import { check, type Update, type DownloadEvent } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

type UpdateStatus =
  | 'idle'
  | 'checking'
  | 'available'
  | 'downloading'
  | 'uptodate'
  | 'error';

export const update = $state<{
  status: UpdateStatus;
  newVersion: string;
  progress: number;
  error: string;
}>({ status: 'idle', newVersion: '', progress: 0, error: '' });

// The update returned by the last successful check(), held until the user
// chooses to install it.
let pending: Update | null = null;

/**
 * @param silent when true (the launch-time call) stay quiet unless an update
 *   is actually available; when false (a manual "check for updates") also
 *   surface "checking", "you're up to date", and errors.
 */
export async function checkForUpdates(silent = true): Promise<void> {
  if (!silent) {
    update.status = 'checking';
    update.error = '';
  }
  try {
    const found = await check();
    if (!found) {
      pending = null;
      update.status = silent ? 'idle' : 'uptodate';
      return;
    }
    pending = found;
    update.newVersion = found.version;
    update.status = 'available';
  } catch (err) {
    // Never let an update check break startup.
    console.error('[updater]', err);
    if (silent) {
      update.status = 'idle';
    } else {
      update.error = err instanceof Error ? err.message : String(err);
      update.status = 'error';
    }
  }
}

/** Download the pending update with progress, then relaunch into the new build. */
export async function installUpdate(): Promise<void> {
  if (!pending) return;
  update.status = 'downloading';
  update.progress = 0;
  update.error = '';

  let total = 0;
  let downloaded = 0;
  try {
    await pending.downloadAndInstall((event: DownloadEvent) => {
      switch (event.event) {
        case 'Started':
          total = event.data.contentLength ?? 0;
          break;
        case 'Progress':
          downloaded += event.data.chunkLength;
          if (total > 0) {
            update.progress = Math.round((downloaded / total) * 100);
          }
          break;
        case 'Finished':
          update.progress = 100;
          break;
      }
    });
    await relaunch();
  } catch (err) {
    console.error('[updater]', err);
    update.error = err instanceof Error ? err.message : String(err);
    update.status = 'error';
  }
}

/** Dismiss the current banner/toast (the "Later" button). */
export function dismiss(): void {
  update.status = 'idle';
}
