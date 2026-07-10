<script lang="ts">
  /**
   * Prompts — the Prompt Library view (issue #24, revised per the founder's
   * feel-check). Scope tabs on top (Global + pinned projects — the active
   * tab drives match pool, save scope, and tint); the compose box is the
   * primary surface with situational affordances only; the library/match
   * panel sits left and collapses for a distraction-free box. Orchestrates
   * the piece modal, save-selection-as-piece, and the Copy toast.
   */
  import { onDestroy, onMount } from 'svelte';
  import type { Piece } from '$lib/prompts/types';
  import {
    prompts,
    initPrompts,
    disposePrompts,
    activeProject,
    composeInsertPiece,
    copyOutput,
  } from '$lib/prompts.svelte';
  import { projectColorVar } from '$lib/prompts/palette';
  import { copyToClipboard } from '$lib/copy';
  import ComposeBox from './prompts/ComposeBox.svelte';
  import MatchPanel from './prompts/MatchPanel.svelte';
  import PieceModal, { type PieceModalContext } from './prompts/PieceModal.svelte';
  import ProjectTabs from './prompts/ProjectTabs.svelte';
  import ProjectManagerPopover from './prompts/ProjectManagerPopover.svelte';
  import EmbeddingsPanel from './prompts/EmbeddingsPanel.svelte';

  let panelCollapsed = $state(false);
  let managerOpen = $state(false);
  // Dismissal is per-mount on purpose: the broken files are still broken on
  // the next visit, and a data-loss warning that never comes back would
  // itself be the silent loss it exists to prevent.
  let loadErrorsDismissed = $state(false);
  let modalContext = $state<PieceModalContext | null>(null);
  let copyMsg = $state<string | null>(null);
  let copyMsgTimer: ReturnType<typeof setTimeout> | null = null;

  const hasSelection = $derived(prompts.selEnd > prompts.selStart);
  /** The active tab's hue enters the CSS world here, once — everything
   *  below styles with color-mix over --project-color (unset on Global,
   *  so every fill falls back to its neutral). */
  const tintStyle = $derived.by(() => {
    const active = activeProject();
    return active ? `--project-color: ${projectColorVar(active.color)}` : '';
  });
  /** Loader-repaired pieces (transient flag): fine to use, but the repair
   *  persists only on an explicit re-save — worth a quiet nudge. */
  const recoveredPieces = $derived(prompts.pieces.filter((p) => p.recovered));

  onMount(() => {
    initPrompts();
  });
  onDestroy(() => {
    disposePrompts();
    if (copyMsgTimer) clearTimeout(copyMsgTimer);
  });

  // ── insert flow: the raw body lands as-is; variables go to the fill list ──
  function handleInsert(piece: Piece): void {
    composeInsertPiece(piece);
  }

  // ── piece modal (F3 / F4) ──────────────────────────────────────────────────
  function openSpan(spanIndex: number): void {
    modalContext = { kind: 'span', spanIndex };
  }

  function saveSelectionAsPiece(): void {
    if (!hasSelection) return;
    modalContext = {
      kind: 'new',
      selStart: prompts.selStart,
      selEnd: prompts.selEnd,
      selectionText: prompts.doc.text.slice(prompts.selStart, prompts.selEnd),
    };
  }

  // ── Copy Prompt ────────────────────────────────────────────────────────────
  async function copyPrompt(): Promise<void> {
    const ok = await copyToClipboard(copyOutput());
    copyMsg = ok ? 'Prompt copied to clipboard' : 'Copy failed — select the text manually';
    if (copyMsgTimer) clearTimeout(copyMsgTimer);
    copyMsgTimer = setTimeout(() => (copyMsg = null), 2500);
  }
</script>

<div class="prompts-view" style={tintStyle}>
  <div class="prompts-view__tabs">
    <ProjectTabs onOpenManager={() => (managerOpen = !managerOpen)} />
    {#if managerOpen}
      <ProjectManagerPopover onClose={() => (managerOpen = false)} />
    {/if}
  </div>

  {#if prompts.loadError}
    <div class="prompts-view__error">Couldn't load the piece library: {prompts.loadError}</div>
  {/if}
  {#if prompts.configError}
    <div class="prompts-view__error">{prompts.configError}</div>
  {/if}

  {#if recoveredPieces.length}
    <div class="prompts-view__load-warn" role="status">
      <div class="prompts-view__load-warn-text">
        <strong>
          {recoveredPieces.length} piece file{recoveredPieces.length === 1 ? '' : 's'} auto-repaired
        </strong>
        — the JSON was invalid and was recovered in memory (the file on disk is untouched). Open
        and save {recoveredPieces.length === 1 ? 'it' : 'each'} to keep the repair:
        {#each recoveredPieces as p, i (p.id)}{i > 0 ? ', ' : ' '}<code>{p.title}</code>{/each}
      </div>
    </div>
  {/if}

  {#if prompts.pieceLoadErrors.length && !loadErrorsDismissed}
    <div class="prompts-view__load-warn" role="status">
      <div class="prompts-view__load-warn-text">
        <strong>
          {prompts.pieceLoadErrors.length} piece file{prompts.pieceLoadErrors.length === 1 ? '' : 's'}
          couldn't be read
        </strong>
        — fix or remove {prompts.pieceLoadErrors.length === 1 ? 'it' : 'them'} (the rest of the
        library loaded fine):
        <ul>
          {#each prompts.pieceLoadErrors as e (e.file)}
            <li><code>{e.file}</code>: {e.error}</li>
          {/each}
        </ul>
      </div>
      <button
        type="button"
        class="btn btn--ghost btn--sm"
        onclick={() => (loadErrorsDismissed = true)}
        aria-label="Dismiss piece-file warning"
      >
        ✕
      </button>
    </div>
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
        <MatchPanel onInsert={handleInsert} />
        <EmbeddingsPanel />
      </aside>
    {/if}

    <section class="prompts-view__compose">
      <ComposeBox onOpenSpan={openSpan} onCopy={copyPrompt} onSaveSelection={saveSelectionAsPiece} />
    </section>
  </div>
</div>

{#if modalContext}
  <PieceModal context={modalContext} onClose={() => (modalContext = null)} />
{/if}

{#if copyMsg}
  <div class="toast" role="status">{copyMsg}</div>
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

  /* Non-blocking, dismissable: pieces that DID load work normally; this only
     flags the files that didn't. Amber (template accent), not error red —
     the library isn't broken, some files are. */
  .prompts-view__load-warn {
    display: flex;
    align-items: flex-start;
    gap: 0.5rem;
    font-size: 0.72rem;
    color: var(--text-muted);
    border: 1px solid color-mix(in srgb, var(--accent-template) 30%, transparent);
    background: color-mix(in srgb, var(--accent-template) 7%, transparent);
    border-radius: 0.4rem;
    padding: 0.5rem 0.75rem;
  }
  .prompts-view__load-warn-text { flex: 1; }
  .prompts-view__load-warn strong { color: var(--text); }
  .prompts-view__load-warn ul {
    margin: 0.25rem 0 0;
    padding-left: 1.1rem;
  }
  .prompts-view__load-warn code {
    font-family: var(--font-mono);
    font-size: 0.68rem;
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

  @media (max-width: 640px) {
    .prompts-view__cols { flex-direction: column; }
    .prompts-view__panel { width: 100%; }
  }
</style>
