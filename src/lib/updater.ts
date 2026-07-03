/**
 * Auto-update — checks GitHub Releases for a newer signed build and installs it.
 *
 * Runs once at launch (see +layout.svelte). The update check is the only network
 * request the app ever makes; everything else is fully offline. Any failure
 * (offline, GitHub unreachable, no release yet) is swallowed so it can never
 * block the app from starting.
 */
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { ask, message } from '@tauri-apps/plugin-dialog';

/**
 * @param silent when true (the launch-time call) stay quiet unless an update
 *   is actually available; when false (a manual "check for updates") also
 *   report "you're up to date" and surface errors.
 */
export async function checkForUpdates(silent = true): Promise<void> {
  try {
    const update = await check();
    if (!update) {
      if (!silent) {
        await message("You're on the latest version.", { title: 'Claude Code Studio' });
      }
      return;
    }

    const proceed = await ask(
      `Version ${update.version} is available (you have ${update.currentVersion}).\n\nDownload and install it now?`,
      { title: 'Update available', kind: 'info' }
    );
    if (!proceed) return;

    await update.downloadAndInstall();
    await relaunch();
  } catch (err) {
    // Never let an update check break startup.
    console.error('[updater]', err);
    if (!silent) {
      await message(`Update check failed: ${err}`, { title: 'Claude Code Studio', kind: 'error' });
    }
  }
}
