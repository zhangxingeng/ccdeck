<script lang="ts">
  /**
   * Prompts — the Prompt Library view (issue #24, Round-B UX pass). Scope tabs
   * on top (Global + pinned projects — the active tab drives match pool, save
   * scope, and tint) with the app-level config gear at their right end; the
   * compose box is the primary surface with situational affordances; the
   * library/match panel sits left and collapses for a distraction-free box.
   * Orchestrates the snippet modal, Save as…, the view-scoped hotkeys, the
   * ↓-into-panel keyboard bridge, and the toast stack.
   */
  import { onDestroy, onMount } from 'svelte';
  import type { Snippet, SnippetScope } from '$lib/prompts/types';
  import {
    prompts,
    initPrompts,
    disposePrompts,
    activeProject,
    composeInsertSnippet,
    copyOutput,
    resolvedHotkeys,
  } from '$lib/prompts.svelte';
  import { projectColorVar } from '$lib/prompts/palette';
  import { copyToClipboard } from '$lib/copy';
  import { toasts } from '$lib/prompts/toasts.svelte';
  import { eventMatchesChord } from '$lib/prompts/hotkeys';
  import ComposeBox from './prompts/ComposeBox.svelte';
  import MatchPanel from './prompts/MatchPanel.svelte';
  import SnippetModal, { type SnippetModalContext } from './prompts/SnippetModal.svelte';
  import ProjectTabs from './prompts/ProjectTabs.svelte';
  import ProjectManagerPopover from './prompts/ProjectManagerPopover.svelte';
  import ConfigPopover from './prompts/ConfigPopover.svelte';

  let panelCollapsed = $state(false);
  let managerOpen = $state(false);
  let configOpen = $state(false);
  let modalContext = $state<SnippetModalContext | null>(null);
  /** MatchPanel instance — only its exported focusFirst() is called (the ↓ step
   *  into the panel). A structural type avoids the component-instance gymnastics. */
  let matchPanel = $state<{ focusFirst: () => boolean } | undefined>(undefined);

  const hasSelection = $derived(prompts.selEnd > prompts.selStart);
  /** True while a modal or popover owns the keyboard — the view-scoped hotkeys
   *  disarm so a rebind capture or a modal keystroke never triggers a command. */
  const keyboardCaptured = $derived(modalContext !== null || managerOpen || configOpen);

  /** The active tab's hue enters the CSS world here, once — everything below
   *  styles with color-mix over --project-color (unset on Global). */
  const tintStyle = $derived.by(() => {
    const active = activeProject();
    return active ? `--project-color: ${projectColorVar(active.color)}` : '';
  });

  /** The active tab as a save scope — the default target for a hotkey Save as… */
  function activeScope(): SnippetScope {
    const active = activeProject();
    return active ? { kind: 'project', project_id: active.id } : { kind: 'global' };
  }

  onMount(() => {
    initPrompts();
    window.addEventListener('keydown', onWindowKeydown);
  });
  onDestroy(() => {
    disposePrompts();
    window.removeEventListener('keydown', onWindowKeydown);
  });

  // ── insert flow: one path, the raw body replaces the query line ──────────────
  function handleInsert(snippet: Snippet): void {
    composeInsertSnippet(snippet);
  }

  // ── snippet modal ────────────────────────────────────────────────────────────
  function openSpan(spanIndex: number): void {
    modalContext = { kind: 'span', spanIndex };
  }

  /** Save as… — selection-aware (contract §S5/S6). With a selection, save it
   *  (it becomes a linked span). With none, save the whole box as a fresh
   *  snippet WITHOUT linking the draft (no range) — "save what I wrote". Either
   *  way the modal opens pre-scoped to `scope`. */
  function saveAs(scope: SnippetScope): void {
    if (hasSelection) {
      modalContext = {
        kind: 'new',
        selStart: prompts.selStart,
        selEnd: prompts.selEnd,
        selectionText: prompts.doc.text.slice(prompts.selStart, prompts.selEnd),
        scope,
      };
    } else if (prompts.doc.text.length > 0) {
      modalContext = { kind: 'new', selectionText: prompts.doc.text, scope };
    }
  }

  // ── Copy Prompt ──────────────────────────────────────────────────────────────
  async function copyPrompt(): Promise<void> {
    const ok = await copyToClipboard(copyOutput());
    toasts.push(ok ? 'Prompt copied.' : 'Copy failed — select the text manually.');
  }

  // ── view-scoped hotkeys (contract §Hotkey map) ───────────────────────────────
  function onWindowKeydown(e: KeyboardEvent): void {
    if (keyboardCaptured) return; // a modal/popover owns the keyboard
    const keys = resolvedHotkeys();
    if (eventMatchesChord(e, keys.copyPrompt)) {
      // Selection-aware (JC-4): native copy wins when text is selected in the
      // box; we only claim the dead key-space when nothing is selected.
      if (hasSelection) return;
      e.preventDefault();
      void copyPrompt();
      return;
    }
    if (eventMatchesChord(e, keys.saveAs)) {
      e.preventDefault(); // the browser owns Ctrl/Cmd+S
      saveAs(activeScope());
    }
  }
</script>

<div class="prompts-view" style={tintStyle}>
  <div class="prompts-view__tabs">
    <div class="prompts-view__tabrow">
      <div class="prompts-view__tabrow-tabs">
        <ProjectTabs onOpenManager={() => (managerOpen = !managerOpen)} />
      </div>
      <ConfigPopover bind:open={configOpen} />
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
