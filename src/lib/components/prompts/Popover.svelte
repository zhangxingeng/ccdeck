<script lang="ts">
  /**
   * Popover primitive — solves containment once (contract §S10). Renders its
   * panel under `<body>` (portal attachment) and positions it `fixed` to the
   * viewport from the anchor's rect, so NO ancestor's `overflow` or stacking
   * context can clip or out-stack it. This is the root fix for the config
   * popover rendering *behind* the compose box; relocating the anchor alone
   * would just move the bug to the next ancestor.
   *
   * Focus is trapped within the panel and restored to the trigger on close
   * (focusTrap attachment). `Esc` and a click on the transparent backdrop are
   * equivalent close gestures.
   */
  import type { Snippet } from 'svelte';
  import { portal } from '$lib/attachments/portal';
  import { focusTrap } from '$lib/attachments/focusTrap';

  interface Props {
    /** The trigger element to position against (bind:this on the caller). */
    anchor: HTMLElement | undefined;
    open: boolean;
    onClose: () => void;
    label: string;
    /** Align the panel's left or right edge to the anchor. */
    align?: 'left' | 'right';
    children: Snippet;
  }

  let { anchor, open, onClose, label, align = 'left', children }: Props = $props();

  const GAP = 6;
  let panelEl = $state<HTMLDivElement | undefined>(undefined);
  let pos = $state<{ top: number; left: number } | null>(null);

  function reposition(): void {
    if (!anchor || !panelEl) {
      pos = null;
      return;
    }
    const r = anchor.getBoundingClientRect();
    const width = panelEl.offsetWidth;
    const rawLeft = align === 'right' ? r.right - width : r.left;
    // Clamp inside the viewport so an edge anchor still shows the whole panel.
    const left = Math.max(8, Math.min(rawLeft, window.innerWidth - width - 8));
    pos = { top: r.bottom + GAP, left };
  }

  // Position after the panel is in the DOM, keep it pinned to the anchor as the
  // page scrolls/resizes, and close on Escape. Escape is handled at the window
  // (not the panel) on purpose: the panel is portalled to <body>, so focus can
  // legitimately sit anywhere inside it, and a panel-scoped handler would miss
  // an Escape that a descendant didn't bubble. A descendant that wants Escape
  // for itself (the rebinding capture) stops propagation, giving innermost-
  // first behaviour for free.
  $effect(() => {
    if (!open) {
      pos = null;
      return;
    }
    void anchor;
    reposition();
    const onMove = (): void => reposition();
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener('resize', onMove);
    // Capture-phase: catch scrolls on any ancestor, not just the window.
    window.addEventListener('scroll', onMove, true);
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('resize', onMove);
      window.removeEventListener('scroll', onMove, true);
      window.removeEventListener('keydown', onKey);
    };
  });
</script>

{#if open}
  <div class="popover-root" {@attach portal}>
    <div class="popover__backdrop" onclick={onClose} aria-hidden="true"></div>
    <div
      bind:this={panelEl}
      class="popover__panel"
      role="dialog"
      aria-label={label}
      tabindex="-1"
      style={pos ? `top: ${pos.top}px; left: ${pos.left}px;` : 'visibility: hidden;'}
      {@attach focusTrap}
    >
      {@render children()}
    </div>
  </div>
{/if}

<style>
  .popover__backdrop {
    position: fixed;
    inset: 0;
    z-index: 200;
    background: transparent;
  }
  .popover__panel {
    position: fixed;
    z-index: 201;
    width: min(23rem, 92vw);
    max-height: min(32rem, 80vh);
    overflow-y: auto;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 0.6rem;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.22);
  }
</style>
