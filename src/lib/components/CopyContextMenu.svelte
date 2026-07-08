<script lang="ts">
  /**
   * CopyContextMenu.svelte — app-wide custom right-click menu.
   *
   * Tauri's native webview context menu only offers browser chrome (Reload,
   * Back, Inspect...), which isn't useful in a packaged desktop app. This
   * replaces it everywhere with a single-purpose HTML menu whose only action
   * is Copy. Mounted once, globally, in +layout.svelte.
   *
   * Inputs/textareas/contenteditable are left alone so the native menu (with
   * cut/paste/undo) still works there — this only takes over on read-only
   * display content.
   *
   * What gets copied, in priority order:
   *   1. The current text selection, if any (matches normal browser behavior).
   *   2. The nearest ancestor's `data-copy-text` value — explicit opt-in used
   *      on values that may be visually truncated (long paths, chips, titles).
   *   3. Nothing — if neither applies, no menu is shown (native menu stays
   *      suppressed either way, so right-click never leaks a useless one).
   */
  import { onDestroy } from 'svelte';
  import { copyToClipboard } from '$lib/copy';

  let visible = $state(false);
  let x = $state(0);
  let y = $state(0);
  let copyText = $state('');
  let copied = $state(false);
  let closeTimer: ReturnType<typeof setTimeout> | null = null;

  function isEditable(el: Element | null): boolean {
    if (!el) return false;
    if (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA') return true;
    return (el as HTMLElement).isContentEditable === true;
  }

  function findCopyText(target: EventTarget | null): string {
    const sel = window.getSelection()?.toString();
    if (sel) return sel;
    if (target instanceof Element) {
      const withAttr = target.closest<HTMLElement>('[data-copy-text]');
      if (withAttr) return withAttr.dataset.copyText ?? '';
    }
    return '';
  }

  function handleContextMenu(e: MouseEvent) {
    const target = e.target instanceof Element ? e.target : null;
    if (isEditable(target)) return; // native menu keeps cut/paste/undo working

    e.preventDefault();
    const text = findCopyText(e.target);
    if (!text) {
      visible = false;
      return;
    }
    copyText = text;
    copied = false;
    // Clamp so the menu never renders off-screen.
    x = Math.min(e.clientX, window.innerWidth - 140);
    y = Math.min(e.clientY, window.innerHeight - 48);
    visible = true;
  }

  function close() {
    visible = false;
  }

  async function doCopy() {
    copied = await copyToClipboard(copyText);
    if (closeTimer) clearTimeout(closeTimer);
    closeTimer = setTimeout(() => { close(); closeTimer = null; }, 500);
  }

  // Clear the pending close timer on unmount so it can't fire against a
  // torn-down component or leak.
  onDestroy(() => { if (closeTimer) clearTimeout(closeTimer); });

  function handleWindowClick() {
    if (visible) close();
  }
  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') close();
  }
</script>

<svelte:window
  oncontextmenu={handleContextMenu}
  onclick={handleWindowClick}
  onkeydown={handleKeydown}
  onscroll={close}
  onblur={close}
/>

{#if visible}
  <div class="copy-menu" style="left:{x}px; top:{y}px;" role="menu">
    <button
      type="button"
      class="copy-menu__item"
      role="menuitem"
      onclick={(e) => { e.stopPropagation(); doCopy(); }}
    >
      {copied ? 'Copied ✓' : 'Copy'}
    </button>
  </div>
{/if}

<style>
  .copy-menu {
    position: fixed;
    z-index: 1000;
    min-width: 8rem;
    background: var(--bg-card);
    border: 1px solid var(--border-strong);
    border-radius: 0.4rem;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25);
    padding: 0.25rem;
    font-size: 0.8rem;
  }
  .copy-menu__item {
    width: 100%;
    text-align: left;
    padding: 0.4rem 0.6rem;
    border-radius: 0.3rem;
    border: 0;
    background: none;
    color: var(--text);
    cursor: pointer;
    font-family: inherit;
  }
  .copy-menu__item:hover {
    background: var(--bg-subtle);
  }
</style>
