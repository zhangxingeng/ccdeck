/**
 * Focus-trap attachment — the a11y contract for every modal and popover
 * (project_docs/prompts-ux.md §"Popovers, focus trap, and Escape").
 * While the host element is mounted: focus moves into it, `Tab`/`Shift+Tab`
 * cycle *within* it and never escape to the page behind, and the element that
 * held focus before it opened is re-focused on close. A mouse-off product where
 * `Tab` walks focus behind an open modal is not operable without a mouse — the
 * trap is load-bearing, not polish.
 *
 * Written as a Svelte 5 attachment (`{@attach focusTrap}`) — the framework's
 * preferred directive-like form (stack/svelte/design_protocol). The host must
 * carry `tabindex="-1"` so it can hold focus when it contains no focusable
 * child yet.
 */

const FOCUSABLE_SELECTOR = [
  'a[href]',
  'button:not([disabled])',
  'textarea:not([disabled])',
  'input:not([disabled])',
  'select:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(', ');

export function focusTrap(node: HTMLElement): () => void {
  const previouslyFocused =
    document.activeElement instanceof HTMLElement ? document.activeElement : null;

  function focusables(): HTMLElement[] {
    // Visible focusables only — a hidden control (e.g. a collapsed section)
    // must not become a Tab dead-end. offsetParent is null for display:none.
    return [...node.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)].filter(
      (el) => el.offsetParent !== null || el === document.activeElement
    );
  }

  // Move focus in unless it is already inside (opening from a child keeps it).
  // Deferred a frame on purpose: a portalled host is moved to <body> by its
  // portal attachment during the same mount flush, and moving a focused node
  // blurs it. Focusing on the next frame lands AFTER that move, so focus sticks
  // inside the panel (and the panel's keyboard handlers then actually fire).
  let rafId = 0;
  if (!node.contains(document.activeElement)) {
    rafId = requestAnimationFrame(() => {
      (focusables()[0] ?? node).focus();
    });
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key !== 'Tab') return;
    const items = focusables();
    if (items.length === 0) {
      // Nothing to land on — keep focus on the container rather than letting
      // Tab reach the page behind.
      e.preventDefault();
      node.focus();
      return;
    }
    const first = items[0];
    const last = items[items.length - 1];
    const active = document.activeElement;
    if (e.shiftKey) {
      if (active === first || !node.contains(active)) {
        e.preventDefault();
        last.focus();
      }
    } else if (active === last || !node.contains(active)) {
      e.preventDefault();
      first.focus();
    }
  }

  node.addEventListener('keydown', onKeydown);
  return () => {
    cancelAnimationFrame(rafId);
    node.removeEventListener('keydown', onKeydown);
    // Restore focus to wherever attention was before this opened (the trigger).
    previouslyFocused?.focus();
  };
}
