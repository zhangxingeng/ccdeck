<script lang="ts">
  /**
   * Prompts — the Prompt Library view. Project tabs on top (a project is a name
   * and a folder); the compose box is the primary surface; the library panel
   * sits left, lists the active project's snippets, and collapses for a
   * distraction-free box. Orchestrates the snippet modal, the library's `+`
   * create button, the one fixed hotkey, the ↓-into-panel keyboard bridge, and
   * the toast stack.
   *
   * There is no scope and no tint: a snippet lives in the folder it sits in, so
   * "which project does this save to?" has exactly one answer — the active one.
   */
  import { onDestroy, onMount } from 'svelte';
  import type { Snippet } from '$lib/prompts/types';
  import {
    prompts,
    initPrompts,
    disposePrompts,
    composeInsertSnippet,
    copyOutput,
    touchSnippet,
  } from '$lib/prompts.svelte';
  import { chipAt, flatten } from '$lib/compose/doc';
  import { parseVariables } from '$lib/compose/variables';
  import { copyToClipboard } from '$lib/copy';
  import { toasts } from '$lib/prompts/toasts.svelte';
  import ComposeBox from './prompts/ComposeBox.svelte';
  import VariableFillList from './prompts/VariableFillList.svelte';
  import MatchPanel from './prompts/MatchPanel.svelte';
  import SnippetModal, { type SnippetModalContext } from './prompts/SnippetModal.svelte';
  import ProjectTabs from './prompts/ProjectTabs.svelte';

  let panelCollapsed = $state(false);
  /** True while a project's right-click context menu (rename/color/delete) is
   *  open — the replacement for the deleted `ProjectManagerPopover`'s
   *  `managerOpen`, same keyboard-disarm purpose. */
  let projectMenuOpen = $state(false);
  let modalContext = $state<SnippetModalContext | null>(null);
  /** MatchPanel instance — only its exported focusFirst() is called (the ↓ step
   *  into the panel). A structural type avoids the component-instance gymnastics. */
  let matchPanel = $state<{ focusFirst: () => boolean } | undefined>(undefined);
  /** ComposeBox instance. Only its exported `focus()` is called (Esc out of the
   *  match panel puts the caret back in the box). */
  let composeBox = $state<{ focus: () => void } | undefined>(undefined);

  /** True while a modal or popover owns the keyboard — the view-scoped hotkeys
   *  disarm so a modal keystroke never triggers a command. */
  const keyboardCaptured = $derived(modalContext !== null || projectMenuOpen);

  /** Whether the fill-list column has anything to show — gates the column
   *  itself (not just its rows), so an empty prompt doesn't reserve a blank
   *  right-hand strip. */
  const hasVariables = $derived(parseVariables(flatten(prompts.doc)).length > 0);

  onMount(() => {
    initPrompts();
    window.addEventListener('keydown', onWindowKeydown);
  });
  onDestroy(() => {
    disposePrompts();
    window.removeEventListener('keydown', onWindowKeydown);
  });

  // ── insert flow: one path, the chip replaces the query line ──────────────────
  async function handleInsert(snippet: Snippet): Promise<void> {
    composeInsertSnippet(snippet.name, snippet.content);
    // Using a snippet is the ONLY thing that feeds the at-rest sort, so the
    // insert path is where it has to be recorded — this is what makes the panel
    // open on what you actually reach for.
    await touchSnippet(snippet.name);
  }

  // ── the popup: two entrances, one surface ────────────────────────────────────
  // Clicking a chip, or the library's `+` button. Those are the only two ways a
  // snippet body is ever edited — which is the whole point of the redesign. The
  // compose box used to offer a third (Save as snippet); that path is cut —
  // "the compose box is for orchestrating snippets into a prompt. The library
  // is where snippets are made."

  /** Clicking a chip. The doc holds the chip's current name and content, so the
   *  popup opens on what the box actually shows. */
  function openChip(cid: string): void {
    const chip = chipAt(prompts.doc, cid);
    if (chip === undefined) return; // deleted out from under the click
    modalContext = { cid, name: chip.name, content: chip.content, dirty: chip.dirty };
  }

  /** The library's `+` button: create a new snippet from scratch. Blank context
   *  (no `cid`, no `name`) hits the modal's existing `!fromChip` create branch —
   *  this is just a new trigger for it, and now the ONLY in-app one. */
  function createSnippet(): void {
    if (prompts.activeProjectPath === null) {
      toasts.push('Add a prompt folder first');
      return;
    }
    modalContext = { content: '' };
  }

  // ── Copy Prompt ──────────────────────────────────────────────────────────────
  async function copyPrompt(): Promise<void> {
    const ok = await copyToClipboard(copyOutput());
    toasts.push(ok ? 'Prompt copied.' : 'Copy failed — select the text manually.');
  }

  // ── view-scoped hotkey — fixed, not rebindable ───────────────────────────────
  // One command, one constant: Mod+C copies the composed prompt ("Mod" = Ctrl on
  // Windows/Linux, Cmd on macOS). Mod+S (save as snippet) was cut along with the
  // compose box's save-as-snippet path — the library's `+` button is now the
  // only way to create one, and Ctrl/Cmd+S reverts to the browser's own binding.
  // Rebinding was cut separately (contract §Cuts) — nobody ever rebound this,
  // and the capture/conflict UI cost ~410 lines to defend a capability with no
  // users. The chord carries Mod by construction, so the old "a hand-edited
  // config bound a bare key, don't steal a keystroke a text field would insert"
  // backstop has nothing left to defend against and is gone with it.

  /** Does native copy have something to act on, wherever focus is? A text-entry
   *  element's own non-collapsed selection, or a non-collapsed document
   *  selection (contenteditable / plain DOM). This is the real "is anything
   *  selected anywhere" — the compose box's selStart/selEnd only track the box
   *  while IT is focused, which is exactly what let Ctrl+C hijack a fill-input
   *  copy (contract §S9). */
  function nativeSelectionActive(): boolean {
    const el = document.activeElement;
    if (el instanceof HTMLTextAreaElement || el instanceof HTMLInputElement) {
      return (
        el.selectionStart !== null &&
        el.selectionEnd !== null &&
        el.selectionStart !== el.selectionEnd
      );
    }
    const sel = window.getSelection();
    return sel !== null && !sel.isCollapsed && sel.toString().length > 0;
  }

  function onWindowKeydown(e: KeyboardEvent): void {
    if (keyboardCaptured) return; // a modal/popover owns the keyboard
    if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
    const key = e.key.toLowerCase();
    if (key === 'c') {
      // Selection-aware (JC-4 / §S9): native copy owns Ctrl/Cmd+C whenever
      // anything is selected where focus actually is; we claim only the empty
      // key-space the OS leaves us when nothing is selected anywhere. Without
      // this, Copy Prompt would hijack a copy out of a variable fill input.
      if (nativeSelectionActive()) return;
      e.preventDefault();
      void copyPrompt();
    }
  }
</script>

<div class="prompts-view">
  <div class="prompts-view__tabrow">
    <div class="prompts-view__tabrow-tabs">
      <ProjectTabs onProjectMenuOpenChange={(open) => (projectMenuOpen = open)} />
    </div>
  </div>

  {#if prompts.loadError}
    <div class="prompts-view__error">Couldn't load the snippet library: {prompts.loadError}</div>
  {/if}

  <div class="prompts-view__cols">
    {#if panelCollapsed}
      <button
        type="button"
        class="prompts-view__panel-peek"
        onclick={() => (panelCollapsed = false)}
        title="Show the library panel"
      >
        ⟩ Library
      </button>
    {:else}
      <aside class="prompts-view__panel">
        <div class="prompts-view__panel-head">
          <span class="prompts-view__panel-title">Library</span>
          <div class="prompts-view__panel-head-actions">
            <button
              type="button"
              class="btn btn--ghost btn--sm"
              onclick={createSnippet}
              title="New snippet"
              aria-label="New snippet"
            >
              +
            </button>
            <button
              type="button"
              class="btn btn--ghost btn--sm"
              onclick={() => (panelCollapsed = true)}
              title="Hide the library panel (distraction-free box)"
              aria-label="Hide the library panel"
            >
              ⟨
            </button>
          </div>
        </div>
        <MatchPanel bind:this={matchPanel} onInsert={handleInsert} onEscape={() => composeBox?.focus()} />
      </aside>
    {/if}

    <section class="prompts-view__compose">
      <ComposeBox
        bind:this={composeBox}
        onOpenChip={openChip}
        onCopy={copyPrompt}
        onStepIntoPanel={() => matchPanel?.focusFirst() ?? false}
      />
    </section>

    {#if hasVariables}
      <aside class="prompts-view__fills">
        <span class="prompts-view__fills-title">Variables</span>
        <VariableFillList />
      </aside>
    {/if}
  </div>
</div>

{#if modalContext && prompts.activeProjectPath !== null}
  <SnippetModal
    context={modalContext}
    project={prompts.activeProjectPath}
    onClose={() => (modalContext = null)}
  />
{/if}

{#if toasts.items.length}
  <div class="prompts-toasts" role="status" aria-live="polite">
    {#each toasts.items as t (t.id)}
      <button type="button" class="prompts-toast" onclick={() => toasts.dismiss(t.id)} title="Dismiss">
        {t.text}
      </button>
    {/each}
  </div>
{/if}

<style>
  .prompts-view {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    /* Fill the viewport under the header so the compose box gets real height. */
    min-height: calc(100vh - var(--header-h) - 9rem);
  }

  .prompts-view__tabrow {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .prompts-view__tabrow-tabs {
    flex: 1;
    min-width: 0;
  }

  .prompts-view__error {
    font-size: 0.75rem;
    color: var(--accent-result-err);
    border: 1px solid color-mix(in srgb, var(--accent-result-err) 25%, transparent);
    background: color-mix(in srgb, var(--accent-result-err) 8%, transparent);
    border-radius: 0.4rem;
    padding: 0.5rem 0.75rem;
  }

  .prompts-view__cols {
    display: flex;
    gap: 1rem;
    flex: 1;
    align-items: stretch;
    min-height: 0;
  }
  .prompts-view__panel-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 0.4rem;
  }
  .prompts-view__panel-head-actions {
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }
  .prompts-view__panel-peek {
    align-self: flex-start;
    font-family: inherit;
    font-size: 0.68rem;
    padding: 0.3rem 0.6rem;
    border: 1px solid var(--border);
    border-radius: 0.4rem;
    background: transparent;
    color: var(--text-faint);
    cursor: pointer;
    white-space: nowrap;
  }
  .prompts-view__panel-peek:hover {
    color: var(--text);
    background: var(--bg-subtle);
  }
  .prompts-view__panel-title {
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-faint);
  }
  .prompts-view__panel {
    width: 15.5rem;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    /* No overflow-y here on purpose: a hovered row's preview tooltip floats to
       the RIGHT of the panel (MatchPanel.svelte), and `overflow-y: auto` would
       force `overflow-x` to clip too, cutting the tooltip off. The page itself
       scrolls if the library list runs long. */
  }
  .prompts-view__compose {
    flex: 1;
    display: flex;
    min-width: 22rem; /* a variables column next door must not squeeze this thin */
    position: relative; /* anchors the placeholder popover */
  }

  .prompts-view__fills {
    width: 15.5rem;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    overflow-y: auto;
  }
  .prompts-view__fills-title {
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-faint);
  }

  /* Toast stack: newest at the bottom, above everything, click to dismiss. */
  .prompts-toasts {
    position: fixed;
    bottom: 1.25rem;
    left: 50%;
    transform: translateX(-50%);
    z-index: 200;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    align-items: center;
  }
  .prompts-toast {
    font-family: inherit;
    font-size: 0.8rem;
    padding: 0.6rem 1.1rem;
    border: 0;
    border-radius: 0.5rem;
    background: var(--text);
    color: var(--bg);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25);
    cursor: pointer;
    max-width: min(30rem, 92vw);
  }

  @media (max-width: 640px) {
    .prompts-view__cols { flex-direction: column; }
    .prompts-view__panel { width: 100%; }
    .prompts-view__fills { width: 100%; }
  }
</style>
