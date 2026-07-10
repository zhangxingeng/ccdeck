<script lang="ts">
  /**
   * App-level config popover (contract §S10 / JC-6). The gear anchors at the
   * right end of the scope tab row — it is app-level config, NOT a library-panel
   * control (its old home in the panel head is exactly why it rendered behind
   * the compose box). The popover is portalled and viewport-fixed via the
   * Popover primitive, so it escapes every ancestor's clipping/stacking. It
   * consolidates the three config surfaces: Notices (when there are data events,
   * shown first so it gets initial focus), Semantic matching, and Shortcuts.
   *
   * The `open` flag is exposed via bind so the parent can suppress the
   * view-scoped hotkeys while the popover owns the keyboard.
   */
  import { notices } from '$lib/prompts.svelte';
  import { noticeBadgeCount } from '$lib/prompts/notices';
  import Popover from './Popover.svelte';
  import NoticesSection from './NoticesSection.svelte';
  import EmbeddingsSection from './EmbeddingsSection.svelte';
  import ShortcutsSection from './ShortcutsSection.svelte';

  let { open = $bindable(false) }: { open?: boolean } = $props();

  let gearEl = $state<HTMLButtonElement | undefined>(undefined);

  const noticeList = $derived(notices());
  const badge = $derived(noticeBadgeCount(noticeList));
</script>

<button
  bind:this={gearEl}
  type="button"
  class="config-gear"
  class:config-gear--flagged={badge > 0}
  aria-expanded={open}
  aria-label={badge > 0 ? `Settings — ${badge} notice${badge === 1 ? '' : 's'}` : 'Settings'}
  title="Settings"
  onclick={() => (open = !open)}
>
  ⚙
  {#if badge > 0}<span class="config-gear__badge" aria-hidden="true">{badge}</span>{/if}
</button>

<Popover anchor={gearEl} {open} onClose={() => (open = false)} align="right" label="Settings">
  <div class="config-pop">
    {#if noticeList.length}
      <NoticesSection notices={noticeList} />
      <hr class="config-pop__rule" />
    {/if}
    <EmbeddingsSection />
    <hr class="config-pop__rule" />
    <ShortcutsSection />
  </div>
</Popover>

<style>
  .config-gear {
    position: relative;
    font-family: inherit;
    font-size: 0.9rem;
    line-height: 1;
    padding: 0.25rem 0.4rem;
    border: 0;
    border-radius: 0.35rem;
    background: transparent;
    color: var(--text-faint);
    cursor: pointer;
  }
  .config-gear:hover {
    color: var(--text);
    background: var(--bg-subtle);
  }
  .config-gear--flagged {
    color: var(--accent-template);
  }
  .config-gear__badge {
    position: absolute;
    top: -0.15rem;
    right: -0.15rem;
    min-width: 0.95rem;
    height: 0.95rem;
    padding: 0 0.2rem;
    border-radius: 0.5rem;
    background: var(--accent-template);
    color: var(--bg-card);
    font-size: 0.6rem;
    font-weight: 700;
    line-height: 0.95rem;
    text-align: center;
  }
  .config-pop {
    display: flex;
    flex-direction: column;
    gap: 0.65rem;
    padding: 0.85rem 0.9rem;
  }
  .config-pop__rule {
    width: 100%;
    height: 0;
    border: 0;
    border-top: 1px solid var(--border);
    margin: 0;
  }
</style>
