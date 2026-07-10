<script lang="ts">
  /**
   * Live match panel (F2): the debounced store feeds ranked hits for what's
   * being typed; clicking one inserts it at the cursor. Arrow keys move
   * within the list once it has focus (the BrowseView keyboard-nav idiom,
   * kept minimal — the panel is a suggestion strip, not a browser).
   */
  import type { Piece } from '$lib/prompts/types';
  import { prompts } from '$lib/prompts.svelte';

  let {
    onInsert,
  }: {
    onInsert: (piece: Piece) => void;
  } = $props();

  let listEl: HTMLDivElement | undefined = $state(undefined);

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    if (!listEl) return;
    const items = [...listEl.querySelectorAll<HTMLButtonElement>('button.match-hit')];
    if (!items.length) return;
    const i = items.indexOf(document.activeElement as HTMLButtonElement);
    const next = e.key === 'ArrowDown' ? Math.min(items.length - 1, i + 1) : Math.max(0, i - 1);
    items[next]?.focus();
    e.preventDefault();
  }

  function snippet(body: string): string {
    const flat = body.replace(/\s+/g, ' ').trim();
    return flat.length > 90 ? flat.slice(0, 90) + '…' : flat;
  }
</script>

<div class="match-panel" bind:this={listEl} onkeydown={handleKeydown} role="listbox" tabindex="-1" aria-label="Matching pieces">
  {#if prompts.hits.length}
    {#each prompts.hits as hit (hit.piece.id)}
      <button
        type="button"
        class="match-hit"
        role="option"
        aria-selected="false"
        onclick={() => onInsert(hit.piece)}
        title="Insert at cursor"
      >
        <span class="match-hit__head">
          <span class="match-hit__title">{hit.piece.title}</span>
          <span class="match-hit__scope">{hit.piece.scope.kind === 'global' ? 'global' : 'project'}</span>
          {#if hit.piece.placeholders.length}
            <span class="match-hit__ph" title="Has placeholders — you'll fill them on insert">
              {'{{'}…{'}}'}
            </span>
          {/if}
        </span>
        <span class="match-hit__snippet">{snippet(hit.piece.body)}</span>
      </button>
    {/each}
  {:else if prompts.matchQuery.trim()}
    <div class="match-panel__empty">
      {prompts.matching ? 'Matching…' : 'No matching pieces.'}
    </div>
  {:else}
    <div class="match-panel__empty">
      Start typing in the compose box — pieces whose title, keywords, or body match what you write
      show up here. Click one to drop it in at the cursor.
    </div>
  {/if}
</div>

<style>
  .match-panel {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }
  .match-hit {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    text-align: left;
    font-family: inherit;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 0.45rem;
    padding: 0.5rem 0.65rem;
    cursor: pointer;
    color: var(--text);
  }
  .match-hit:hover,
  .match-hit:focus-visible {
    border-color: color-mix(in srgb, var(--accent-piece) 55%, var(--border));
    background: color-mix(in srgb, var(--accent-piece) 7%, var(--bg-card));
    outline: none;
  }
  .match-hit__head {
    display: flex;
    align-items: baseline;
    gap: 0.45rem;
    min-width: 0;
  }
  .match-hit__title {
    font-size: 0.78rem;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .match-hit__scope {
    font-size: 0.6rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-faint);
    flex-shrink: 0;
  }
  .match-hit__ph {
    font-size: 0.62rem;
    font-family: var(--font-mono);
    color: var(--accent-template);
    flex-shrink: 0;
  }
  .match-hit__snippet {
    font-size: 0.7rem;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .match-panel__empty {
    font-size: 0.72rem;
    color: var(--text-faint);
    padding: 0.4rem 0.2rem;
    line-height: 1.5;
  }
</style>
