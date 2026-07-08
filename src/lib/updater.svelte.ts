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

// Auto-dismiss for the *transient* statuses. Unlike SessionEditor/BrowseView
// toasts, these are driven off shared module state (no component + onDestroy to
// clear a timer), so 'checking' / 'uptodate' / 'error' would otherwise sit on
// screen forever. `setStatus` routes every transition through here so the timer
// is always cleared/replaced: transient statuses arm a fresh dismiss timer, and
// the persistent ones ('available', 'downloading') just clear it.
const TRANSIENT_DISMISS_MS = 4000;
let dismissTimer: ReturnType<typeof setTimeout> | null = null;

function setStatus(status: UpdateStatus): void {
  update.status = status;
  if (dismissTimer) {
    clearTimeout(dismissTimer);
    dismissTimer = null;
  }
  if (status === 'checking' || status === 'uptodate' || status === 'error') {
    dismissTimer = setTimeout(() => {
      dismissTimer = null;
      update.status = 'idle';
    }, TRANSIENT_DISMISS_MS);
  }
}

/**
 * @param silent when true (the launch-time call) stay quiet unless an update
 *   is actually available; when false (a manual "check for updates") also
 *   surface "checking", "you're up to date", and errors.
 */
export async function checkForUpdates(silent = true): Promise<void> {
  // Never let a check stomp an in-flight download or overlap another check —
  // without this, clicking "Check for updates" mid-download resets the UI
  // back to "available" while the original download keeps running, and a
  // second "Update & restart" click races a second downloadAndInstall().
  if (update.status === 'downloading' || update.status === 'checking') return;
  if (!silent) {
    update.error = '';
    setStatus('checking');
  }
  try {
    const found = await check();
    if (!found) {
      pending = null;
      setStatus(silent ? 'idle' : 'uptodate');
      return;
    }
    pending = found;
    update.newVersion = found.version;
    setStatus('available');
  } catch (err) {
    // Never let an update check break startup.
    console.error('[updater]', err);
    if (silent) {
      setStatus('idle');
    } else {
      update.error = err instanceof Error ? err.message : String(err);
      setStatus('error');
    }
  }
}

/** Download the pending update with progress, then relaunch into the new build. */
export async function installUpdate(): Promise<void> {
  // Reentrancy guard: without it, two triggers (e.g. a stale banner click
  // after checkForUpdates() re-ran) start two concurrent
  // downloadAndInstall() calls, each prompting its own install/permission
  // dialog and racing writes to the shared `update.progress`.
  if (!pending || update.status === 'downloading') return;
  update.progress = 0;
  update.error = '';
  setStatus('downloading');

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
    setStatus('error');
  }
}

/** Dismiss the current banner/toast (the "Later" button). */
export function dismiss(): void {
  setStatus('idle');
}
