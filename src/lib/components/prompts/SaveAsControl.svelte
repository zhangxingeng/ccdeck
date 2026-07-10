<script lang="ts">
  /**
   * Save-as affordance (contract §S5/S6/S8): a "Save as…" button plus a scope
   * chip that names the target (the active tab) and, on a chevron click, offers
   * Global or any project in one step — so a snippet typed under one tab can be
   * saved elsewhere without switching tabs first. Both the always-present
   * bottom-left control and the floating selection button render this, so the
   * scope switch is identical on both (the contract requires both to surface it).
   *
   * The main button saves to the active-tab scope; a menu item saves straight to
   * that scope. Either way the parent opens the same snippet modal, pre-scoped.
   */
  import type { SnippetScope } from '$lib/prompts/types';
  import { prompts, activeProject } from '$lib/prompts.svelte';
  import { projectColorVar } from '$lib/prompts/palette';

  interface Props {
    label: string;
    onSave: (scope: SnippetScope) => void;
    /** 'floating' suppresses mousedown so clicking never collapses the
     *  selection the button acts on. */
    variant?: 'persistent' | 'floating';
  }

  let { label, onSave, variant = 'persistent' }: Props = $props();

  let menuOpen = $state(false);

  // The active tab is the default save scope (contract: the tab IS the scope).
  const activeScope = $derived.by((): SnippetScope => {
    const p = activeProject();
    return p ? { kind: 'project', project_id: p.id } : { kind: 'global' };
  });
  const activeLabel = $derived(activeProject()?.name ?? 'Global');
  const activeColorVar = $derived.by(() => {
    const p = activeProject();
    return p ? projectColorVar(p.color) : null;
  });

  function save(scope: SnippetScope): void {
    menuOpen = false;
    onSave(scope);
  }

  // Floating variant: swallow mousedown so focus/selection survive the click.
  function guard(e: MouseEvent): void {
    if (variant === 'floating') e.preventDefault();
  }

  function onMenuKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault();
      menuOpen = false;
    }
  }
</script>

<div class="save-as save-as--{variant}">
  <button
    type="button"
    class="save-as__main"
    onmousedown={guard}
    onclick={() => save(activeScope)}
    title="Save as a reusable library snippet"
  >
    {label}
  </button>
  <button
    type="button"
    class="save-as__scope"
    aria-haspopup="menu"
    aria-expanded={menuOpen}
    onmousedown={guard}
    onclick={() => (menuOpen = !menuOpen)}
    title="Choose where to save — Global or a project"
  >
    <span
      class="save-as__dot"
      class:save-as__dot--global={!activeColorVar}
      style={activeColorVar ? `--dot-color: ${activeColorVar}` : null}
      aria-hidden="true"
    ></span>
    <span class="save-as__scope-name">{activeLabel}</span>
    <span class="save-as__chev" aria-hidden="true">▾</span>
  </button>

  {#if menuOpen}
    <div class="save-as__backdrop" onmousedown={guard} onclick={() => (menuOpen = false)} aria-hidden="true"></div>
    <div class="save-as__menu" role="menu" tabindex="-1" onkeydown={onMenuKeydown}>
      <button type="button" role="menuitem" class="save-as__item" onmousedown={guard} onclick={() => save({ kind: 'global' })}>
        <span class="save-as__dot save-as__dot--global" aria-hidden="true"></span>
        Global
      </button>
      {#each prompts.projects as p (p.id)}
        <button
          type="button"
          role="menuitem"
          class="save-as__item"
          onmousedown={guard}
          onclick={() => save({ kind: 'project', project_id: p.id })}
        >
          <span class="save-as__dot" style="--dot-color: {projectColorVar(p.color)}" aria-hidden="true"></span>
          {p.name}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .save-as {
    position: relative;
    display: inline-flex;
    align-items: stretch;
    border: 1px solid var(--border-strong);
    border-radius: 1rem;
    background: var(--bg-card);
    overflow: visible;
  }
  .save-as--floating {
    box-shadow: 0 3px 10px rgba(0, 0, 0, 0.14);
  }
  .save-as__main,
  .save-as__scope {
    font-family: inherit;
    font-size: 0.68rem;
    border: 0;
    background: transparent;
    color: var(--text);
    cursor: pointer;
  }
  .save-as__main {
    font-weight: 600;
    padding: 0.25rem 0.55rem;
    border-radius: 1rem 0 0 1rem;
  }
  .save-as__scope {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.25rem 0.5rem;
    border-left: 1px solid var(--border);
    border-radius: 0 1rem 1rem 0;
    color: var(--text-muted);
  }
  .save-as__main:hover,
  .save-as__scope:hover {
    background: color-mix(in srgb, var(--project-color, var(--accent-snippet)) 12%, transparent);
  }
  .save-as__dot {
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 50%;
    background: var(--dot-color, var(--text-faint));
    flex-shrink: 0;
  }
  .save-as__dot--global {
    background: var(--text-faint);
  }
  .save-as__scope-name {
    max-width: 8rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .save-as__chev {
    font-size: 0.6rem;
    color: var(--text-faint);
  }
  .save-as__backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
    background: transparent;
  }
  .save-as__menu {
    position: absolute;
    bottom: calc(100% + 0.3rem);
    left: 0;
    z-index: 41;
    min-width: 10rem;
    max-height: 14rem;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    padding: 0.25rem;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    box-shadow: 0 10px 32px rgba(0, 0, 0, 0.18);
  }
  .save-as__item {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    font-family: inherit;
    font-size: 0.72rem;
    text-align: left;
    padding: 0.3rem 0.5rem;
    border: 0;
    border-radius: 0.35rem;
    background: transparent;
    color: var(--text);
    cursor: pointer;
    white-space: nowrap;
  }
  .save-as__item:hover,
  .save-as__item:focus-visible {
    background: var(--bg-subtle);
    outline: none;
  }
</style>
