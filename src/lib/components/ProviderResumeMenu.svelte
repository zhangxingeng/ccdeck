<script lang="ts">
  /**
   * ProviderResumeMenu.svelte — a small right-click menu for choosing which
   * provider a session is resumed / forked-and-resumed against (issue #21).
   *
   * Shared by BrowseView (Resume) and SessionEditor (Fork & resume). Positioned
   * at the click point (fixed, clamped on-screen) and dismissed on any outside
   * click, scroll, blur, Escape, or a fresh right-click — mirroring the app's
   * existing CopyContextMenu pattern.
   *
   * `onSelect(null)` = the default account (no provider override); a profile
   * name selects that provider. Profiles with no key stored yet (`keyBackend
   * === 'none'`) are shown disabled — the backend would error safely, but
   * offering them is a guaranteed dead end, so we grey them out instead.
   */
  import type { ProviderProfile } from '$lib/types';

  let {
    x,
    y,
    profiles,
    verb = 'Resume',
    onSelect,
    onClose,
  }: {
    x: number;
    y: number;
    profiles: ProviderProfile[];
    verb?: string;
    onSelect: (providerName: string | null) => void;
    onClose: () => void;
  } = $props();

  // Clamp so the menu never renders off the right/bottom edge.
  let left = $derived(Math.min(x, (typeof window !== 'undefined' ? window.innerWidth : 9999) - 220));
  let top = $derived(Math.min(y, (typeof window !== 'undefined' ? window.innerHeight : 9999) - (profiles.length + 1) * 34 - 12));

  function badgeIcon(p: ProviderProfile): string {
    if (p.keyBackend === 'keychain') return '🔒';
    if (p.keyBackend === 'plaintext') return '⚠';
    return '○';
  }

  function pick(name: string | null, e: MouseEvent) {
    e.stopPropagation();
    onSelect(name);
    onClose();
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') onClose();
  }
</script>

<svelte:window
  onclick={onClose}
  oncontextmenu={onClose}
  onscroll={onClose}
  onblur={onClose}
  onkeydown={onKeydown}
/>

<div class="provider-menu" style="left:{left}px; top:{top}px;" role="menu">
  <button
    type="button"
    class="provider-menu__item"
    role="menuitem"
    onclick={(e) => pick(null, e)}
  >{verb} (default account)</button>

  {#if profiles.length > 0}
    <div class="provider-menu__sep" role="separator"></div>
    {#each profiles as p (p.name)}
      <button
        type="button"
        class="provider-menu__item"
        role="menuitem"
        disabled={p.keyBackend === 'none'}
        title={p.keyBackend === 'none' ? 'No API key set for this profile' : p.baseUrl}
        onclick={(e) => pick(p.name, e)}
      >
        <span class="provider-menu__icon">{badgeIcon(p)}</span>
        {verb} with {p.name}
        {#if p.keyBackend === 'none'}<span class="provider-menu__note">(no key)</span>{/if}
      </button>
    {/each}
  {/if}
</div>

<style>
  .provider-menu {
    position: fixed;
    z-index: 1000;
    min-width: 12rem;
    max-width: 16rem;
    background: var(--bg-card);
    border: 1px solid var(--border-strong);
    border-radius: 0.4rem;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25);
    padding: 0.25rem;
    font-size: 0.8rem;
  }
  .provider-menu__item {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 0.4rem;
    text-align: left;
    padding: 0.4rem 0.6rem;
    border-radius: 0.3rem;
    border: 0;
    background: none;
    color: var(--text);
    cursor: pointer;
    font-family: inherit;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .provider-menu__item:hover:not(:disabled) { background: var(--bg-subtle); }
  .provider-menu__item:disabled { color: var(--text-faint); cursor: default; }
  .provider-menu__icon { flex-shrink: 0; }
  .provider-menu__note { color: var(--text-faint); font-size: 0.72rem; }
  .provider-menu__sep { height: 1px; background: var(--border); margin: 0.25rem 0.3rem; }
</style>
