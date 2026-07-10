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

/** One durable notice. `kind` distinguishes an in-memory repair (fixable by a
 *  re-save) from an unreadable file (the user must fix the JSON by hand). */
export interface Notice {
  kind: 'repaired' | 'unreadable';
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

/**
 * Derive the durable notices from the two data-event sources. Repairs come
 * first (they are the recoverable, self-inflicted-by-hand-edit case with a
 * one-click fix); unreadable files follow (they need manual repair). The order
 * is stable so the badge count and the list never reshuffle under the user.
 */
export function deriveNotices(
  recovered: readonly RecoveredSnippet[],
  loadErrors: readonly LoadErrorEntry[]
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
  return notices;
}

/** The gear badge count — the number of unresolved data events. Zero hides the
 *  badge; the badge clears when the underlying condition clears (a repaired
 *  snippet is re-saved, an unreadable file is fixed and reloaded). */
export function noticeBadgeCount(notices: readonly Notice[]): number {
  return notices.length;
}
