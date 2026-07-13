<script lang="ts">
  /**
   * Prompts — the Prompt Library view. Project tabs on top (a project is a name
   * and a folder); the compose box is the primary surface; the library panel
   * sits left, lists the active project's snippets, and collapses for a
   * distraction-free box. Orchestrates the snippet modal, Save as…, the two
   * fixed hotkeys, the ↓-into-panel keyboard bridge, and the toast stack.
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
  import { copyToClipboard } from '$lib/copy';
  import { toasts } from '$lib/prompts/toasts.svelte';
  import ComposeBox from './prompts/ComposeBox.svelte';
  import MatchPanel from './prompts/MatchPanel.svelte';
  import SnippetModal, { type SnippetModalContext } from './prompts/SnippetModal.svelte';
  import ProjectTabs from './prompts/ProjectTabs.svelte';
  import ProjectManagerPopover from './prompts/ProjectManagerPopover.svelte';

  let panelCollapsed = $state(false);
  let managerOpen = $state(false);
  let modalContext = $state<SnippetModalContext | null>(null);
  /** MatchPanel instance — only its exported focusFirst() is called (the ↓ step
   *  into the panel). A structural type avoids the component-instance gymnastics. */
  let matchPanel = $state<{ focusFirst: () => boolean } | undefined>(undefined);

  const hasSelection = $derived(prompts.selEnd > prompts.selStart);
  /** True while a modal or popover owns the keyboard — the view-scoped hotkeys
   *  disarm so a modal keystroke never triggers a command. */
  const keyboardCaptured = $derived(modalContext !== null || managerOpen);

  onMount(() => {
    initPrompts();
    window.addEventListener('keydown', onWindowKeydown);
  });
  onDestroy(() => {
    disposePrompts();
    window.removeEventListener('keydown', onWindowKeydown);
  });

  // ── insert flow: one path, the raw body replaces the query line ──────────────
  async function handleInsert(snippet: Snippet): Promise<void> {
    composeInsertSnippet(snippet);
    // Using a snippet is the ONLY thing that feeds the at-rest sort, so the
    // insert path is where it has to be recorded — this is what makes the panel
    // open on what you actually reach for.
    await touchSnippet(snippet.name);
  }

  // ── snippet modal ────────────────────────────────────────────────────────────
  function openSpan(spanIndex: number): void {
    modalContext = { kind: 'span', spanIndex };
  }

  /** Save as… — selection-aware (contract §S5/S6). With a selection, save it
   *  (it becomes a linked span). With none, save the whole box as a fresh
   *  snippet WITHOUT linking the draft (no range) — "save what I wrote". It
   *  saves into the active project; there is no scope to choose. */
  function saveAs(): void {
    if (prompts.activeProjectPath === null) {
      toasts.push('Add a prompt folder first — ⋯ above the compose box.');
      return;
    }
    if (hasSelection) {
      modalContext = {
        kind: 'new',
        selStart: prompts.selStart,
        selEnd: prompts.selEnd,
        selectionText: prompts.doc.text.slice(prompts.selStart, prompts.selEnd),
      };
    } else if (prompts.doc.text.length > 0) {
      modalContext = { kind: 'new', selectionText: prompts.doc.text };
    }
  }

  // ── Copy Prompt ──────────────────────────────────────────────────────────────
  async function copyPrompt(): Promise<void> {
    const ok = await copyToClipboard(copyOutput());
    toasts.push(ok ? 'Prompt copied.' : 'Copy failed — select the text manually.');
  }

  // ── view-scoped hotkeys — fixed, not rebindable ──────────────────────────────
  // Two commands, two constants: Mod+C copies the composed prompt, Mod+S saves
  // as a snippet ("Mod" = Ctrl on Windows/Linux, Cmd on macOS). Rebinding was cut
  // (contract §Cuts) — nobody ever rebound them, and the capture/conflict UI cost
  // ~410 lines to defend a capability with no users. Both chords carry Mod by
  // construction now, so the old "a hand-edited config bound a bare key, don't
  // steal a keystroke a text field would insert" backstop has nothing left to
  // defend against and is gone with it.

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
      return;
    }
    if (key === 's') {
      // saveAs — the browser owns Ctrl/Cmd+S, so we take it.
      e.preventDefault();
      saveAs();
    }
  }
</script>

<div class="prompts-view">
  <div class="prompts-view__tabs">
    <div class="prompts-view__tabrow">
      <div class="prompts-view__tabrow-tabs">
        <ProjectTabs onOpenManager={() => (managerOpen = !managerOpen)} />
      </div>
    </div>
    {#if managerOpen}
      <ProjectManagerPopover onClose={() => (managerOpen = false)} />
    {/if}
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
        <MatchPanel bind:this={matchPanel} onInsert={handleInsert} onEscape={() => prompts.focusNonce++} />
      </aside>
    {/if}

    <section class="prompts-view__compose">
      <ComposeBox
        onOpenSpan={openSpan}
        onCopy={copyPrompt}
        onSaveAs={saveAs}
        onStepIntoPanel={() => matchPanel?.focusFirst() ?? false}
      />
    </section>
  </div>
</div>

{#if modalContext}
  <SnippetModal context={modalContext} onClose={() => (modalContext = null)} />
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

  .prompts-view__tabs {
    position: relative; /* anchors the project-manager popover */
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
    overflow-y: auto;
  }
  .prompts-view__compose {
    flex: 1;
    display: flex;
    min-width: 0;
    position: relative; /* anchors the placeholder popover */
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
  }
</style>
