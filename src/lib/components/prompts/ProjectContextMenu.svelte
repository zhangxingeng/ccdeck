<script lang="ts">
  /**
   * Right-click menu for one project tab: rename, change color, delete.
   * Round 2 (`prompt_library_ux_round2_plan.md` §4) — replaces
   * `ProjectManagerPopover.svelte`, whose jobs split in two: **add** is now a
   * direct action on the `+` button (`ProjectTabs.svelte`), and everything that
   * acts on an EXISTING project (rename / color / delete) lives here instead,
   * triggered per-tab rather than from a roster list.
   *
   * Deliberately no "change path" — re-pointing a project at a different folder
   * is incoherent since the folder IS the project's identity; to move a
   * library, delete this project and add the new folder (plan §4).
   *
   * **Removing a project never deletes files.** It forgets a path — same
   * two-click "Remove" → "Forget it? (files stay)" confirm the popover used,
   * carried forward so the safety guarantee's UX doesn't regress with the move.
   */
  import type { Project } from '$lib/prompts/types';
  import { renameProject, setProjectColor, removeProject } from '$lib/prompts.svelte';
  import { focusTrap } from '$lib/attachments/focusTrap';

  interface Props {
    project: Project;
    /** Anchor point — the right-click coordinates, clamped by the caller. */
    x: number;
    y: number;
    onClose: () => void;
  }

  let { project, x, y, onClose }: Props = $props();

  /** The fixed swatch (plan §4 clarification: "not a free/native color picker").
   *  Sourced from the app's existing `--accent-*` tokens (app.css) rather than
   *  inventing a new palette — every hue here already has a name and a meaning
   *  elsewhere in the UI. `--accent-result-ok`/`--accent-result-err` are left
   *  out on purpose: those two carry pass/fail meaning across the whole app
   *  (message cells, status text), and reusing green/red decoratively here
   *  would read as "this project is broken/healthy" rather than "this is
   *  project X". The stored value is the resolved hex, not the var name, so a
   *  project's color survives a future token rename. */
  const SWATCHES: readonly string[] = [
    '#5b8def', // --accent-user
    '#64748b', // --accent-assistant
    '#8b5cf6', // --accent-thinking
    '#d4a017', // --accent-tool
    '#f43f5e', // --accent-interrupt
    '#0d9488', // --accent-subagent
    '#0ea5e9', // --accent-snippet
    '#d97706', // --accent-template
  ];

  let error = $state<string | null>(null);
  let confirmingRemove = $state(false);
  let busy = $state(false);

  async function run(action: () => Promise<unknown>): Promise<void> {
    error = null;
    busy = true;
    try {
      await action();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  /** Uncontrolled input (value= + onchange): a rejected rename would otherwise
   *  leave typed-but-unpersisted text on screen lying about the roster — reset
   *  to the stored name on failure or a no-op edit (same contract the old
   *  popover's rename row had). */
  async function rename(input: HTMLInputElement): Promise<void> {
    const trimmed = input.value.trim();
    if (!trimmed || trimmed === project.name) {
      input.value = project.name;
      return;
    }
    error = null;
    try {
      await renameProject(trimmed, project.path);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      input.value = project.name;
    }
  }

  async function pickColor(hex: string): Promise<void> {
    await run(() => setProjectColor(project.path, hex));
    if (!error) onClose();
  }

  async function clearColor(): Promise<void> {
    await run(() => setProjectColor(project.path, null));
    if (!error) onClose();
  }

  function handleRemove(): void {
    if (!confirmingRemove) {
      confirmingRemove = true;
      return;
    }
    void run(async () => {
      await removeProject(project.path);
      onClose();
    });
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }
</script>

<!-- Transparent click-away layer, same pattern the old popover used: a
     full-screen sibling BEHIND the menu in the DOM, but under it in z-index,
     so a click anywhere on the menu never reaches this element at all. -->
<div class="proj-ctx__backdrop" onclick={onClose} aria-hidden="true"></div>

<div
  class="proj-ctx"
  role="menu"
  aria-label="Project options for {project.name}"
  tabindex="-1"
  style="left:{x}px; top:{y}px;"
  onkeydown={handleKeydown}
  {@attach focusTrap}
>
  <label class="proj-ctx__label" for="proj-ctx-name">Rename</label>
  <input
    id="proj-ctx-name"
    type="text"
    class="proj-ctx__name"
    value={project.name}
    aria-label="Project name"
    onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()}
    onchange={(e) => rename(e.currentTarget)}
  />

  <span class="proj-ctx__label">Color</span>
  <div class="proj-ctx__swatches" role="group" aria-label="Project color">
    {#each SWATCHES as hex (hex)}
      <button
        type="button"
        class="proj-ctx__swatch"
        class:proj-ctx__swatch--active={project.color === hex}
        style="background:{hex};"
        title={hex}
        aria-label="Set project color to {hex}"
        disabled={busy}
        onclick={() => pickColor(hex)}
      ></button>
    {/each}
  </div>
  {#if project.color}
    <button type="button" class="proj-ctx__clear" disabled={busy} onclick={clearColor}>
      Clear color
    </button>
  {/if}

  <div class="proj-ctx__divider"></div>

  <button
    type="button"
    class="proj-ctx__action proj-ctx__action--danger"
    title="Forget this folder — the files on disk are untouched"
    disabled={busy}
    onclick={handleRemove}
  >
    {confirmingRemove ? 'Forget it? (files stay)' : 'Delete'}
  </button>

  {#if error}
    <p class="proj-ctx__error">{error}</p>
  {/if}
</div>

<style>
  .proj-ctx__backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
    background: transparent;
  }
  .proj-ctx {
    position: fixed;
    z-index: 41;
    width: 13rem;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    padding: 0.6rem 0.65rem;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    box-shadow: 0 10px 32px rgba(0, 0, 0, 0.18);
    font-size: 0.76rem;
  }
  .proj-ctx__label {
    font-size: 0.66rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.02em;
    color: var(--text-faint);
    margin-top: 0.15rem;
  }
  .proj-ctx__name {
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0.3rem 0.45rem;
    border: 1px solid var(--border);
    border-radius: 0.35rem;
    background: var(--bg);
    color: var(--text);
  }
  .proj-ctx__name:focus-visible {
    outline: none;
    border-color: color-mix(in srgb, var(--accent-snippet) 55%, var(--border));
  }
  .proj-ctx__swatches {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }
  .proj-ctx__swatch {
    width: 1.15rem;
    height: 1.15rem;
    border-radius: 50%;
    border: 2px solid transparent;
    padding: 0;
    cursor: pointer;
  }
  .proj-ctx__swatch:hover:not(:disabled) {
    border-color: var(--border-strong);
  }
  .proj-ctx__swatch--active {
    border-color: var(--text);
  }
  .proj-ctx__swatch:disabled {
    cursor: default;
    opacity: 0.6;
  }
  .proj-ctx__clear {
    align-self: flex-start;
    font-family: inherit;
    font-size: 0.68rem;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-faint);
    text-decoration: underline;
    cursor: pointer;
  }
  .proj-ctx__clear:hover:not(:disabled) {
    color: var(--text-muted);
  }
  .proj-ctx__divider {
    height: 1px;
    background: var(--border);
    margin: 0.2rem 0;
  }
  .proj-ctx__action {
    font-family: inherit;
    font-size: 0.72rem;
    padding: 0.32rem 0.5rem;
    border: 1px solid var(--border);
    border-radius: 0.35rem;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    text-align: left;
  }
  .proj-ctx__action:hover:not(:disabled) {
    background: var(--bg-subtle);
    color: var(--text);
  }
  .proj-ctx__action--danger:hover:not(:disabled) {
    color: var(--accent-result-err);
    border-color: color-mix(in srgb, var(--accent-result-err) 45%, var(--border));
  }
  .proj-ctx__error {
    margin: 0.2rem 0 0;
    font-size: 0.68rem;
    color: var(--accent-result-err);
  }
</style>
