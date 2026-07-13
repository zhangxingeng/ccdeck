/**
 * Toast store — the *whole* notification model, not half of one (contract
 * project_docs/prompts-ux.md §Conventions). Toasts are transient by design:
 * every one auto-dismisses after 5 seconds and nothing durable hides in one, so
 * missing a toast costs the user nothing.
 *
 * That is a constraint on callers, not just a description: only ever toast a
 * confirmation of something the user just did (copied, saved). If you find
 * yourself wanting a toast for something the user must act on later, the toast
 * is the wrong surface — the durable-notice tier it would need was cut in 0.13
 * along with the schema-repair events that were its only producer, and a
 * "you'll want to know this" message that self-destructs in 5 seconds is worse
 * than no message.
 *
 * Function-based factory (stack/svelte/design_protocol) so the reactive array
 * survives the export boundary; timers live in a closure and are cleared on
 * dismiss so a manually-dismissed toast never re-fires.
 */

export interface Toast {
  id: number;
  text: string;
}

/** Contract §Conventions: "Toasts are transient — 5 seconds, or click to dismiss." */
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
