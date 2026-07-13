<script lang="ts">
  /**
   * Project manager: add a folder, rename it, forget it. That is the whole
   * surface, because a project is now a name and a folder and there is nothing
   * else about it to manage — no color, no pin, no activate button (the tab is
   * the activate button).
   *
   * **Removing a project never deletes files.** It forgets a path. The user's
   * prompts are their own — that is the entire reason the folder is theirs to
   * choose, and the confirm label says so before the click rather than leaving
   * the consequence to be discovered after.
   */
  import { isTauri } from '$lib/api';
  import type { Project } from '$lib/prompts/types';
  import { prompts, addProject, renameProject, removeProject } from '$lib/prompts.svelte';
  import { focusTrap } from '$lib/attachments/focusTrap';

  interface Props {
    onClose: () => void;
  }

  let { onClose }: Props = $props();

  let confirmingRemove = $state<string | null>(null);
  let error = $state<string | null>(null);
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

  /** The OS directory picker. In browser-dev there is no OS dialog, so fall back
   *  to a typed path — the add-a-project flow stays exercisable without Tauri. */
  async function pickFolder(): Promise<string | null> {
    if (!isTauri()) {
      return window.prompt('Folder path (browser-dev only):', '/dev/mock/prompts');
    }
    const { open } = await import('@tauri-apps/plugin-dialog');
    const picked = await open({
      directory: true,
      multiple: false,
      title: 'Choose a folder for your prompts',
    });
    return typeof picked === 'string' ? picked : null;
  }

  /** The folder's own name is the obvious default — the user picked it, so they
   *  already named it once. They can still rename it in the row afterwards. */
  function basename(path: string): string {
    const parts = path.split(/[\\/]/).filter(Boolean);
    return parts[parts.length - 1] ?? path;
  }

  async function add(): Promise<void> {
    const path = await pickFolder();
    if (path === null || path.trim() === '') return;
    await run(async () => {
      await addProject(basename(path.trim()), path.trim());
      onClose(); // you added it to work in it — get out of the way
    });
  }

  /** The input is uncontrolled (value= + onchange), so a rejected rename would
   *  otherwise leave typed-but-unpersisted text on screen lying about the
   *  roster. On failure (or a no-op edit) it resets to the stored name. */
  async function rename(p: Project, input: HTMLInputElement): Promise<void> {
    const trimmed = input.value.trim();
    if (!trimmed || trimmed === p.name) {
      input.value = p.name;
      return;
    }
    error = null;
    try {
      await renameProject(trimmed, p.path);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      input.value = p.name;
    }
  }

  function handleRemove(p: Project): void {
    if (confirmingRemove !== p.path) {
      confirmingRemove = p.path;
      return;
    }
    confirmingRemove = null;
    void run(() => removeProject(p.path));
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

<div
  class="proj-mgr"
  role="dialog"
  aria-label="Manage prompt folders"
  tabindex="-1"
  onkeydown={handleKeydown}
  {@attach focusTrap}
>
  {#if prompts.projects.length === 0}
    <p class="proj-mgr__empty">
      No prompt folders yet. Pick any directory — every <code>.md</code> file under it becomes a
      snippet, so a git repo works and your prompts stay yours.
    </p>
  {/if}

  {#each prompts.projects as p (p.path)}
    <div class="proj-mgr__row">
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
        class="proj-mgr__action proj-mgr__action--danger"
        title="Forget this folder — the files on disk are untouched"
        onclick={() => handleRemove(p)}
      >
        {confirmingRemove === p.path ? 'Forget it? (files stay)' : 'Remove'}
      </button>
    </div>
    <p class="proj-mgr__path" title={p.path}>{p.path}</p>
  {/each}

  <button type="button" class="proj-mgr__add" disabled={busy} onclick={add}>
    + Add a folder…
  </button>

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
    width: min(28rem, 92vw);
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
    margin: 0 0 0.2rem;
    line-height: 1.5;
  }
  .proj-mgr__empty code {
    font-family: var(--font-mono);
  }
  .proj-mgr__row {
    display: flex;
    align-items: center;
    gap: 0.45rem;
  }
  .proj-mgr__name {
    flex: 1;
    min-width: 0;
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0.3rem 0.45rem;
    border: 1px solid var(--border);
    border-radius: 0.35rem;
    background: var(--bg);
    color: var(--text);
  }
  .proj-mgr__name:focus-visible {
    outline: none;
    border-color: color-mix(in srgb, var(--accent-snippet) 55%, var(--border));
  }
  /* The path is the identity, so it is always visible — a name alone cannot tell
     two folders apart, and "which folder is this?" must never need a hover. */
  .proj-mgr__path {
    margin: 0 0 0.45rem;
    font-family: var(--font-mono);
    font-size: 0.62rem;
    color: var(--text-faint);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .proj-mgr__action {
    font-family: inherit;
    font-size: 0.68rem;
    padding: 0.28rem 0.5rem;
    border: 1px solid var(--border);
    border-radius: 0.35rem;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    white-space: nowrap;
    flex-shrink: 0;
  }
  .proj-mgr__action:hover {
    background: var(--bg-subtle);
    color: var(--text);
  }
  .proj-mgr__action--danger:hover {
    color: var(--accent-result-err);
    border-color: color-mix(in srgb, var(--accent-result-err) 45%, var(--border));
  }
  .proj-mgr__add {
    align-self: flex-start;
    margin-top: 0.2rem;
    font-family: inherit;
    font-size: 0.72rem;
    padding: 0.35rem 0.6rem;
    border: 1px dashed var(--border);
    border-radius: 0.35rem;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
  }
  .proj-mgr__add:hover:not(:disabled) {
    background: var(--bg-subtle);
    color: var(--text);
  }
  .proj-mgr__add:disabled {
    opacity: 0.55;
    cursor: default;
  }
  .proj-mgr__error {
    margin: 0.3rem 0 0;
    font-size: 0.7rem;
    color: var(--accent-result-err);
  }
</style>
