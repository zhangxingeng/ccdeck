<script lang="ts">
  /**
   * The project tab row. A project is a name and a folder, so every project in
   * the roster is simply a tab — there is no pin (nothing to promote) and no
   * color (nothing to decorate).
   *
   * There is no "Global" tab, and its absence is the point: a snippet lives in
   * the folder it sits in, so a scope belonging to no folder cannot exist. An
   * empty roster is not a scope either — it renders as the add-a-folder prompt
   * in the panel below, not as a tab you could compose against.
   *
   * Plain Tab-stop buttons, not a roving tablist: the roving version was one of
   * the affordances nobody could guess without having read the UX contract, and
   * a handful of tabs does not need its own navigation model.
   */
  import { prompts, setActiveProject } from '$lib/prompts.svelte';

  interface Props {
    onOpenManager: () => void;
  }

  let { onOpenManager }: Props = $props();
</script>

<div class="project-tabs" role="tablist" aria-label="Prompt projects">
  {#each prompts.projects as p (p.path)}
    <button
      type="button"
      role="tab"
      aria-selected={prompts.activeProjectPath === p.path}
      class="project-tabs__tab"
      class:project-tabs__tab--active={prompts.activeProjectPath === p.path}
      title={p.path}
      onclick={() => setActiveProject(p.path)}
    >
      {p.name}
    </button>
  {/each}

  <button
    type="button"
    class="project-tabs__manage"
    onclick={onOpenManager}
    title="Manage prompt folders"
    aria-label="Manage prompt folders"
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
    font-family: inherit;
    font-size: 0.76rem;
    padding: 0.3rem 0.75rem;
    border: 1px solid transparent;
    border-radius: 1rem;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    transition:
      background 0.12s,
      color 0.12s,
      border-color 0.12s;
  }
  .project-tabs__tab:hover {
    background: var(--bg-subtle);
    color: var(--text);
  }
  .project-tabs__tab--active {
    background: color-mix(in srgb, var(--text-muted) 12%, transparent);
    border-color: color-mix(in srgb, var(--text-muted) 25%, transparent);
    color: var(--text);
    font-weight: 600;
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
