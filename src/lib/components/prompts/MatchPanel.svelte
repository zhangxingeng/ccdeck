<script lang="ts">
  /**
   * Live match panel (F2): the debounced store feeds ranked hits for what's
   * being typed. One insert path, two triggers (contract §S2/S3): click a hit,
   * OR step in from the box with ↓ (parent calls `focusFirst`) then `Enter`.
   * `↑`/`↓` move the highlight; `Esc` returns focus to the box without
   * inserting. The panel is a suggestion strip, not a browser — nav stays
   * minimal.
   */
  import type { Snippet } from '$lib/prompts/types';
  import { prompts } from '$lib/prompts.svelte';

  let {
    onInsert,
    onEscape,
  }: {
    onInsert: (snippet: Snippet) => void;
    /** `Esc` (or `↑` past the first hit) — return focus to the compose box. */
    onEscape: () => void;
  } = $props();

  let listEl: HTMLDivElement | undefined = $state(undefined);

  function hitButtons(): HTMLButtonElement[] {
    return listEl ? [...listEl.querySelectorAll<HTMLButtonElement>('button.match-hit')] : [];
  }

  /** Step into the panel from the box (contract §S2): highlight the first hit.
   *  Returns whether there was a hit to land on, so the box can keep the
   *  keystroke as a caret move when the panel is empty. */
  export function focusFirst(): boolean {
    const items = hitButtons();
    items[0]?.focus();
    return items.length > 0;
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault();
      onEscape();
      return;
    }
    const items = hitButtons();
    if (!items.length) return;
    const i = items.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === 'Enter') {
      // Enter inserts only after the explicit ↓ step (the hit holds focus) —
      // never pre-armed while the caret is in the box (JC-2).
      if (i >= 0 && prompts.hits[i]) {
        e.preventDefault();
        onInsert(prompts.hits[i].snippet);
      }
      return;
    }
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      if (e.key === 'ArrowUp' && i <= 0) {
        // ↑ past the first hit hands focus back to the box — a natural exit.
        onEscape();
        return;
      }
      const next = e.key === 'ArrowDown' ? Math.min(items.length - 1, i + 1) : i - 1;
      items[next]?.focus();
    }
  }

  function snippet(body: string): string {
    const flat = body.replace(/\s+/g, ' ').trim();
    return flat.length > 90 ? flat.slice(0, 90) + '…' : flat;
  }
</script>

<div class="match-panel" bind:this={listEl} onkeydown={handleKeydown} role="listbox" tabindex="-1" aria-label="Matching snippets">
  {#if prompts.hits.length}
    {#each prompts.hits as hit (hit.snippet.id)}
      <button
        type="button"
        class="match-hit"
        role="option"
        aria-selected="false"
        onclick={() => onInsert(hit.snippet)}
        title="Insert at cursor"
      >
        <span class="match-hit__head">
          <span class="match-hit__title">{hit.snippet.title}</span>
          <span class="match-hit__scope">{hit.snippet.scope.kind === 'global' ? 'global' : 'project'}</span>
          {#if hit.snippet.placeholders.length}
            <span class="match-hit__ph" title="Has variables — fill them in the list under the compose box">
              {'{'}…{'}'}
            </span>
          {/if}
        </span>
        <span class="match-hit__snippet">{snippet(hit.snippet.body)}</span>
      </button>
    {/each}
  {:else if prompts.matchQuery.trim()}
    <div class="match-panel__empty">
      {prompts.matching ? 'Matching…' : 'No matching snippets.'}
    </div>
  {:else}
    <div class="match-panel__empty">
      Start typing in the compose box — snippets whose title, keywords, or body match what you write
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
    border-color: color-mix(in srgb, var(--accent-snippet) 55%, var(--border));
    background: color-mix(in srgb, var(--accent-snippet) 7%, var(--bg-card));
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
