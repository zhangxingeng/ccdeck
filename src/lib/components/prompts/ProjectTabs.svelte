<script lang="ts">
  /**
   * The scope tab row atop the Prompts view (contract §Compose surface):
   * Global first (neutral — it is not a project record), then every pinned
   * project. The active tab IS the scope — match pool, save target, tint.
   * An unpinned project activated through the manager shows as a temporary
   * trailing tab while active, so the current scope is never invisible.
   */
  import { prompts, setActiveProject } from '$lib/prompts.svelte';
  import { projectColorVar } from '$lib/prompts/palette';

  interface Props {
    onOpenManager: () => void;
  }

  let { onOpenManager }: Props = $props();

  let rowEl: HTMLDivElement | undefined = $state(undefined);

  const tabs = $derived.by(() => {
    const pinned = prompts.projects.filter((p) => p.pinned);
    const active = prompts.projects.find((p) => p.id === prompts.activeProjectId);
    return active && !active.pinned ? [...pinned, active] : pinned;
  });

  // Roving-tabindex nav (contract §S8): the active tab is the single Tab stop;
  // ←/→ move focus among the scope tabs (wrapping); Enter/Space activate
  // natively (a focused tab button click). The ⋯ manager is not a scope tab and
  // stays out of the roving set.
  function handleKeydown(e: KeyboardEvent): void {
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
    if (!rowEl) return;
    const items = [...rowEl.querySelectorAll<HTMLButtonElement>('.project-tabs__tab')];
    const i = items.indexOf(document.activeElement as HTMLButtonElement);
    if (i < 0) return;
    e.preventDefault();
    const next = e.key === 'ArrowRight' ? (i + 1) % items.length : (i - 1 + items.length) % items.length;
    items[next]?.focus();
  }
</script>

<!-- tabindex="-1": the tablist owns the ←/→ handler (event delegation from the
     roving tab buttons) but is never itself a Tab stop — the active tab is. -->
<div class="project-tabs" role="tablist" aria-label="Prompt scope" tabindex="-1" bind:this={rowEl} onkeydown={handleKeydown}>
  <button
    type="button"
    role="tab"
    aria-selected={prompts.activeProjectId === null}
    tabindex={prompts.activeProjectId === null ? 0 : -1}
    class="project-tabs__tab"
    class:project-tabs__tab--active={prompts.activeProjectId === null}
    onclick={() => setActiveProject(null)}
  >
    Global
  </button>

  {#each tabs as p (p.id)}
    <button
      type="button"
      role="tab"
      aria-selected={prompts.activeProjectId === p.id}
      tabindex={prompts.activeProjectId === p.id ? 0 : -1}
      class="project-tabs__tab"
      class:project-tabs__tab--active={prompts.activeProjectId === p.id}
      style="--tab-color: {projectColorVar(p.color)}"
      onclick={() => setActiveProject(p.id)}
    >
      <span class="project-tabs__dot" aria-hidden="true"></span>
      {p.name}
    </button>
  {/each}

  <button
    type="button"
    class="project-tabs__manage"
    onclick={onOpenManager}
    title="Manage projects"
    aria-label="Manage projects"
  >
    ⋯
  </button>
</div>

<style>
  .project-tabs {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    flex-wrap: wrap;
  }
  .project-tabs__tab {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    font-family: inherit;
    font-size: 0.76rem;
    padding: 0.3rem 0.75rem;
    border: 1px solid transparent;
    border-radius: 1rem;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    transition: background 0.12s, color 0.12s, border-color 0.12s;
  }
  .project-tabs__tab:hover {
    background: var(--bg-subtle);
    color: var(--text);
  }
  /* Active tab: a quiet fill of the tab's own hue — Global falls back to a
     neutral grey base, keeping the first tab colorless by design. */
  .project-tabs__tab--active {
    background: color-mix(in srgb, var(--tab-color, var(--text-muted)) 12%, transparent);
    border-color: color-mix(in srgb, var(--tab-color, var(--text-muted)) 25%, transparent);
    color: var(--text);
    font-weight: 600;
  }
  .project-tabs__dot {
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 50%;
    background: var(--tab-color);
    flex-shrink: 0;
  }
  .project-tabs__manage {
    font-family: inherit;
    font-size: 0.85rem;
    line-height: 1;
    padding: 0.25rem 0.55rem;
    border: 0;
    border-radius: 1rem;
    background: transparent;
    color: var(--text-faint);
    cursor: pointer;
  }
  .project-tabs__manage:hover {
    background: var(--bg-subtle);
    color: var(--text);
  }
</style>
