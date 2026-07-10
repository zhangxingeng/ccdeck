<script lang="ts">
  /**
   * Project manager popover: create, rename, recolor, pin/unpin, delete —
   * and reach unpinned projects (clicking a row activates it as the scope).
   * Colors are palette keys only; swatches render from the --project-<key>
   * tokens via the single palette helper. Deleting rescopes the project's
   * pieces to Global (backend semantics) — said in the confirm label so the
   * consequence is read before the click, not discovered after.
   */
  import { PALETTE_KEYS, type PaletteKey, type Project } from '$lib/prompts/types';
  import { prompts, saveProject, deleteProject, setActiveProject } from '$lib/prompts.svelte';
  import { projectColorVar } from '$lib/prompts/palette';

  interface Props {
    onClose: () => void;
  }

  let { onClose }: Props = $props();

  /** Which project's swatch strip is expanded ('new' = the create row). */
  let colorPickFor = $state<string | null>(null);
  let confirmingDeleteId = $state<string | null>(null);
  let newName = $state('');
  let newColor = $state<PaletteKey>('blue');
  let error = $state<string | null>(null);

  async function run(action: () => Promise<unknown>): Promise<void> {
    error = null;
    try {
      await action();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  /** The input is uncontrolled (value= + onchange), so a rejected save
   *  would otherwise leave typed-but-unpersisted text on screen lying about
   *  the roster — on failure (or a no-op edit) the input is reset to the
   *  stored name, the truth. */
  async function rename(p: Project, input: HTMLInputElement): Promise<void> {
    const trimmed = input.value.trim();
    if (!trimmed || trimmed === p.name) {
      input.value = p.name;
      return;
    }
    error = null;
    try {
      await saveProject({ ...p, name: trimmed });
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      input.value = p.name;
    }
  }

  function recolor(p: Project, color: PaletteKey): void {
    colorPickFor = null;
    if (color !== p.color) void run(() => saveProject({ ...p, color }));
  }

  function togglePin(p: Project): void {
    void run(() => saveProject({ ...p, pinned: !p.pinned }));
  }

  function handleDelete(p: Project): void {
    if (confirmingDeleteId !== p.id) {
      confirmingDeleteId = p.id;
      return;
    }
    confirmingDeleteId = null;
    void run(() => deleteProject(p.id));
  }

  async function create(): Promise<void> {
    const name = newName.trim();
    if (!name) return;
    await run(async () => {
      const saved = await saveProject({ name, color: newColor, pinned: true, path: null });
      setActiveProject(saved.id); // a fresh project is what you came to work in
      newName = '';
      colorPickFor = null;
    });
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }
</script>

<!-- Transparent click-away layer; the popover itself stops propagation. -->
<div class="proj-mgr__backdrop" onclick={onClose} aria-hidden="true"></div>

<div class="proj-mgr" role="dialog" aria-label="Manage projects" tabindex="-1" onkeydown={handleKeydown}>
  {#if prompts.projects.length === 0}
    <p class="proj-mgr__empty">No projects yet — create one below to get a colored tab.</p>
  {/if}

  {#each prompts.projects as p (p.id)}
    <div class="proj-mgr__row" style="--swatch-color: {projectColorVar(p.color)}">
      <button
        type="button"
        class="proj-mgr__dot"
        title="Change color"
        aria-label="Change color of {p.name}"
        onclick={() => (colorPickFor = colorPickFor === p.id ? null : p.id)}
      ></button>
      <input
        type="text"
        class="proj-mgr__name"
        value={p.name}
        aria-label="Project name"
        onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()}
        onchange={(e) => rename(p, e.currentTarget)}
      />
      <button
        type="button"
        class="proj-mgr__action"
        class:proj-mgr__action--on={p.pinned}
        title={p.pinned ? 'Unpin — remove the tab (project keeps its pieces)' : 'Pin as a tab'}
        onclick={() => togglePin(p)}
      >
        {p.pinned ? 'Pinned' : 'Pin'}
      </button>
      <button
        type="button"
        class="proj-mgr__action"
        title="Work in this project"
        onclick={() => {
          setActiveProject(p.id);
          onClose();
        }}
      >
        Open
      </button>
      <button
        type="button"
        class="proj-mgr__action proj-mgr__action--danger"
        title="Delete the project — its pieces move to Global, nothing is lost"
        onclick={() => handleDelete(p)}
      >
        {confirmingDeleteId === p.id ? 'Pieces move to Global — sure?' : 'Delete'}
      </button>
    </div>
    {#if colorPickFor === p.id}
      <div class="proj-mgr__swatches" role="radiogroup" aria-label="Project color">
        {#each PALETTE_KEYS as key (key)}
          <button
            type="button"
            role="radio"
            aria-checked={p.color === key}
            aria-label={key}
            class="proj-mgr__swatch"
            class:proj-mgr__swatch--current={p.color === key}
            style="--swatch-color: {projectColorVar(key)}"
            onclick={() => recolor(p, key)}
          ></button>
        {/each}
      </div>
    {/if}
  {/each}

  <div class="proj-mgr__row proj-mgr__row--new" style="--swatch-color: {projectColorVar(newColor)}">
    <button
      type="button"
      class="proj-mgr__dot"
      title="Pick a color for the new project"
      aria-label="Pick a color for the new project"
      onclick={() => (colorPickFor = colorPickFor === 'new' ? null : 'new')}
    ></button>
    <input
      type="text"
      class="proj-mgr__name"
      bind:value={newName}
      placeholder="New project…"
      autocomplete="off"
      spellcheck="false"
      onkeydown={(e) => e.key === 'Enter' && create()}
    />
    <button type="button" class="proj-mgr__action" disabled={!newName.trim()} onclick={create}>
      Create
    </button>
  </div>
  {#if colorPickFor === 'new'}
    <div class="proj-mgr__swatches" role="radiogroup" aria-label="New project color">
      {#each PALETTE_KEYS as key (key)}
        <button
          type="button"
          role="radio"
          aria-checked={newColor === key}
          aria-label={key}
          class="proj-mgr__swatch"
          class:proj-mgr__swatch--current={newColor === key}
          style="--swatch-color: {projectColorVar(key)}"
          onclick={() => {
            newColor = key;
            colorPickFor = null;
          }}
        ></button>
      {/each}
    </div>
  {/if}

  {#if error}
    <p class="proj-mgr__error">{error}</p>
  {/if}
</div>

<style>
  .proj-mgr__backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
    background: transparent;
  }
  .proj-mgr {
    position: absolute;
    top: 2.2rem;
    left: 0;
    z-index: 41;
    width: min(24rem, 92vw);
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    padding: 0.7rem 0.8rem;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    box-shadow: 0 10px 32px rgba(0, 0, 0, 0.18);
  }
  .proj-mgr__empty {
    font-size: 0.72rem;
    color: var(--text-muted);
    margin: 0;
  }
  .proj-mgr__row {
    display: flex;
    align-items: center;
    gap: 0.45rem;
  }
  .proj-mgr__row--new {
    margin-top: 0.2rem;
    padding-top: 0.5rem;
    border-top: 1px solid var(--border);
  }
  .proj-mgr__dot {
    width: 0.9rem;
    height: 0.9rem;
    border-radius: 50%;
    border: 1px solid color-mix(in srgb, var(--swatch-color) 55%, var(--border));
    background: var(--swatch-color);
    cursor: pointer;
    flex-shrink: 0;
    padding: 0;
  }
  .proj-mgr__name {
    flex: 1;
    min-width: 0;
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0.25rem 0.45rem;
    border: 1px solid transparent;
    border-radius: 0.3rem;
    background: transparent;
    color: var(--text);
  }
  .proj-mgr__name:hover {
    border-color: var(--border);
  }
  .proj-mgr__name:focus {
    outline: none;
    border-color: var(--border-strong);
    background: var(--bg);
  }
  .proj-mgr__action {
    font-family: inherit;
    font-size: 0.66rem;
    padding: 0.2rem 0.5rem;
    border: 1px solid transparent;
    border-radius: 0.3rem;
    background: transparent;
    color: var(--text-faint);
    cursor: pointer;
    white-space: nowrap;
    flex-shrink: 0;
  }
  .proj-mgr__action:hover {
    color: var(--text);
    background: var(--bg-subtle);
  }
  .proj-mgr__action:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .proj-mgr__action--on {
    color: var(--text-muted);
    border-color: var(--border);
  }
  .proj-mgr__action--danger:hover {
    color: var(--accent-result-err);
    background: color-mix(in srgb, var(--accent-result-err) 8%, transparent);
  }
  .proj-mgr__swatches {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.15rem 0 0.25rem 1.35rem;
  }
  .proj-mgr__swatch {
    width: 1.05rem;
    height: 1.05rem;
    border-radius: 50%;
    border: 1px solid color-mix(in srgb, var(--swatch-color) 55%, var(--border));
    background: var(--swatch-color);
    cursor: pointer;
    padding: 0;
  }
  .proj-mgr__swatch:hover {
    transform: scale(1.12);
  }
  .proj-mgr__swatch--current {
    outline: 2px solid var(--text);
    outline-offset: 1px;
  }
  .proj-mgr__error {
    font-size: 0.7rem;
    color: var(--accent-result-err);
    margin: 0.2rem 0 0;
  }
</style>
