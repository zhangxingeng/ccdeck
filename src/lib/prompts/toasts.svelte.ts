/**
 * Toast store — the transient half of the notification model (contract
 * project_docs/prompts-ux.md §S13). Every toast auto-dismisses after 5 seconds:
 * confirmations (copy succeeded, snippet saved) simply vanish, and data events
 * (a JSON auto-repair) flash here too but ALSO leave a durable Notice (see
 * notices.ts) so the transient surface is never their only record.
 *
 * Function-based factory (stack/svelte/design_protocol) so the reactive array
 * survives the export boundary; timers live in a closure and are cleared on
 * dismiss so a manually-dismissed toast never re-fires.
 */

export interface Toast {
  id: number;
  text: string;
}

/** Contract §S13: "Every toast auto-dismisses after 5 seconds." */
const TOAST_TTL_MS = 5000;

function createToasts() {
  let items = $state<Toast[]>([]);
  let seq = 0;
  const timers = new Map<number, ReturnType<typeof setTimeout>>();

  function dismiss(id: number): void {
    const timer = timers.get(id);
    if (timer !== undefined) {
      clearTimeout(timer);
      timers.delete(id);
    }
    items = items.filter((t) => t.id !== id);
  }

  function push(text: string): number {
    const id = ++seq;
    items = [...items, { id, text }];
    timers.set(id, setTimeout(() => dismiss(id), TOAST_TTL_MS));
    return id;
  }

  return {
    get items(): Toast[] {
      return items;
    },
    push,
    dismiss,
  };
}

export const toasts = createToasts();
