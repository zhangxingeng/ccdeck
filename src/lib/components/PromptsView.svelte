<script lang="ts">
  /**
   * Prompts — the Prompt Library view (issue #24). Side-by-side layout per
   * settled decision #2 on issue #7: the compose box is the primary surface;
   * the library/match panel sits left and collapses for a distraction-free
   * box. Orchestrates the insert flow (placeholder popover when needed), the
   * piece modal, save-selection-as-piece, and Copy Prompt.
   */
  import { onDestroy, onMount } from 'svelte';
  import type { Piece } from '$lib/prompts/types';
  import {
    prompts,
    initPrompts,
    disposePrompts,
    setProject,
    composeInsertPiece,
  } from '$lib/prompts.svelte';
  import { flatten } from '$lib/compose/doc';
  import { parsePlaceholders } from '$lib/compose/placeholders';
  import { copyToClipboard } from '$lib/copy';
  import ComposeBox from './prompts/ComposeBox.svelte';
  import MatchPanel from './prompts/MatchPanel.svelte';
  import PlaceholderPopover from './prompts/PlaceholderPopover.svelte';
  import PieceModal, { type PieceModalContext } from './prompts/PieceModal.svelte';
  import EmbeddingsPanel from './prompts/EmbeddingsPanel.svelte';

  let panelCollapsed = $state(false);
  let pendingInsert = $state<Piece | null>(null);
  let modalContext = $state<PieceModalContext | null>(null);
  let copyMsg = $state<string | null>(null);
  let copyMsgTimer: ReturnType<typeof setTimeout> | null = null;

  const hasSelection = $derived(prompts.selEnd > prompts.selStart);
  const hasText = $derived(prompts.doc.text.length > 0);

  onMount(() => {
    initPrompts();
  });
  onDestroy(() => {
    disposePrompts();
    if (copyMsgTimer) clearTimeout(copyMsgTimer);
  });

  // ── insert flow (F2 + F5) ──────────────────────────────────────────────────
  function handleInsert(piece: Piece): void {
    if (parsePlaceholders(piece.body).length) {
      pendingInsert = piece; // fill-in popover first, then the span lands
    } else {
      composeInsertPiece(piece, {});
    }
  }

  function confirmFills(fills: Record<string, string>): void {
    if (pendingInsert) composeInsertPiece(pendingInsert, fills);
    pendingInsert = null;
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

  // ── Copy Prompt (F8) ───────────────────────────────────────────────────────
  async function copyPrompt(): Promise<void> {
    const ok = await copyToClipboard(flatten(prompts.doc));
    copyMsg = ok ? 'Prompt copied to clipboard' : 'Copy failed — select the text manually';
    if (copyMsgTimer) clearTimeout(copyMsgTimer);
    copyMsgTimer = setTimeout(() => (copyMsg = null), 2500);
  }
</script>

<div class="prompts-view">
  <div class="prompts-view__toolbar">
    <button
      type="button"
      class="btn btn--ghost btn--sm"
      onclick={() => (panelCollapsed = !panelCollapsed)}
      title={panelCollapsed ? 'Show the library panel' : 'Hide the library panel (distraction-free box)'}
    >
      {panelCollapsed ? '⟩ Library' : '⟨ Hide library'}
    </button>

    <label class="prompts-view__project">
      <span>Project</span>
      <select
        value={prompts.project ?? ''}
        onchange={(e) => setProject(e.currentTarget.value || null)}
      >
        <option value="">Global only</option>
        {#each prompts.availableProjects as p (p.cwd)}
          <option value={p.cwd}>{p.label}</option>
        {/each}
      </select>
    </label>

    <span class="prompts-view__spacer"></span>

    <button
      type="button"
      class="btn btn--sm"
      disabled={!hasSelection}
      onclick={saveSelectionAsPiece}
      title="Turn the selected text into a reusable library piece"
    >
      Save selection as piece
    </button>
    <button
      type="button"
      class="btn btn--primary btn--sm"
      disabled={!hasText}
      onclick={copyPrompt}
      title="Copy the box as clean plain text — provenance stripped, placeholders substituted"
    >
      Copy prompt
    </button>
  </div>

  {#if prompts.loadError}
    <div class="prompts-view__error">Couldn't load the piece library: {prompts.loadError}</div>
  {/if}

  <div class="prompts-view__cols">
    {#if !panelCollapsed}
      <aside class="prompts-view__panel">
        <MatchPanel onInsert={handleInsert} />
        <EmbeddingsPanel />
      </aside>
    {/if}

    <section class="prompts-view__compose">
      <ComposeBox onOpenSpan={openSpan} />
      {#if pendingInsert}
        <PlaceholderPopover
          piece={pendingInsert}
          onConfirm={confirmFills}
          onCancel={() => (pendingInsert = null)}
        />
      {/if}
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

  .prompts-view__toolbar {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .prompts-view__project {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.72rem;
    color: var(--text-muted);
  }
  .prompts-view__project select {
    font-family: inherit;
    font-size: 0.75rem;
    padding: 0.25rem 0.5rem;
    border: 1px solid var(--border);
    border-radius: 0.35rem;
    background: var(--bg-card);
    color: var(--text);
    max-width: 16rem;
  }
  .prompts-view__spacer { flex: 1; }

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
