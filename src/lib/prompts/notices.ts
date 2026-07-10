/**
 * Data-event notices — the durable half of the toast/notice split (contract
 * project_docs/prompts-ux.md §S13, project_docs/prompts-design.md §Store
 * robustness). A *data event* touched the user's files: a snippet JSON file was
 * auto-repaired in memory, or a file could not be read at all. It flashes a 5s
 * toast like any notification, but — unlike a confirmation — it must also leave
 * a durable trace the user can return to, because a transient surface must
 * never be the only record of something that changed their data.
 *
 * This module is the pure derivation: store state (recovered snippets +
 * unreadable files) → the Notices list the config popover renders and the gear
 * badge counts. No DOM, no Svelte — unit-tested in tests/notices_smoke.mjs.
 */

/** One durable notice. `kind` distinguishes an in-memory snippet repair
 *  (fixable by a re-save), an unreadable file (the user must fix the JSON by
 *  hand), and a config event — a hand-edited hotkey that fell back to its
 *  default (re-set it in Shortcuts to keep a custom key). */
export interface Notice {
  kind: 'repaired' | 'unreadable' | 'config';
  /** Stable key for keyed rendering — the snippet id or the file path. */
  id: string;
  /** What the user sees as the headline: the snippet title or the file name. */
  title: string;
  /** The one-line explanation + the action that clears it. */
  detail: string;
}

/** The minimal shape of a recovered snippet this module reads. */
export interface RecoveredSnippet {
  id: string;
  title: string;
}

/** The minimal shape of a load-error entry this module reads. */
export interface LoadErrorEntry {
  file: string;
  error: string;
}

/** A hotkey override that failed validation on load and fell back to its
 *  default — the config equivalent of a JSON auto-repair (the store keeps
 *  running, but the user's typed value did not take, so it must not vanish
 *  without a trace). `label` is the human command name; `reason` is why. */
export interface InvalidHotkeyEntry {
  command: string;
  label: string;
  chord: string;
  reason: string;
}

/**
 * Derive the durable notices from the data-event sources. Snippet repairs come
 * first (recoverable by a one-click re-save), then unreadable files (manual JSON
 * fix), then config events (a hotkey reset — re-bind in Shortcuts). The order is
 * stable so the badge count and the list never reshuffle under the user.
 */
export function deriveNotices(
  recovered: readonly RecoveredSnippet[],
  loadErrors: readonly LoadErrorEntry[],
  invalidHotkeys: readonly InvalidHotkeyEntry[] = []
): Notice[] {
  const notices: Notice[] = [];
  for (const snippet of recovered) {
    notices.push({
      kind: 'repaired',
      id: snippet.id,
      title: snippet.title,
      detail: 'Auto-repaired from invalid JSON in memory (the file on disk is untouched). Open and re-save it to keep the repair.',
    });
  }
  for (const entry of loadErrors) {
    notices.push({
      kind: 'unreadable',
      id: entry.file,
      title: entry.file,
      detail: entry.error,
    });
  }
  for (const hk of invalidHotkeys) {
    notices.push({
      kind: 'config',
      id: `hotkey:${hk.command}`,
      title: `Shortcut “${hk.label}” reset to default`,
      detail: `The saved binding ${hk.chord} isn't usable (${hk.reason}) — the default is in effect. Set a new one in Shortcuts to keep a custom key.`,
    });
  }
  return notices;
}

/** The gear badge count — the number of unresolved data events. Zero hides the
 *  badge; the badge clears when the underlying condition clears (a repaired
 *  snippet is re-saved, an unreadable file is fixed and reloaded). */
export function noticeBadgeCount(notices: readonly Notice[]): number {
  return notices.length;
}
