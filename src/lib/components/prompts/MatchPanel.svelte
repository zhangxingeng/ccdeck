<script lang="ts">
  /**
   * The library panel. At rest it lists EVERY snippet in the active project,
   * most-recently-used first; typing in the compose box filters that list down
   * by match score. It does not build up from empty — see `runMatch`.
   *
   * Rows show the snippet's NAME and nothing else. The name is a path now
   * (`rust/code_review`), so it already carries the folder grouping that
   * replaced tags/categories — and since the panel lists the whole library at
   * rest, a body preview per row would make it unscannable rather than
   * informative. The founder's own reasoning applies: "I actually rarely read
   * it. If I really want to read it, it means I want to edit it."
   *
   * One insert path, two triggers (contract §S2/S3): click a row, OR step in
   * from the box with ↓ (parent calls `focusFirst`) then `Enter`. `↑`/`↓` move
   * the highlight; `Esc` returns focus to the box without inserting.
   */
  import type { Snippet } from '$lib/prompts/types';
  import { prompts, MATCH_LIMIT } from '$lib/prompts.svelte';

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

  /** Only true if the safety cap actually bit. The panel claims to be the whole
   *  library, so on the day that stops being true it must say so out loud — a
   *  silent truncation would make it lie. */
  const truncated = $derived(prompts.hits.length >= MATCH_LIMIT);
</script>

<div class="match-panel" bind:this={listEl} onkeydown={handleKeydown} role="listbox" tabindex="-1" aria-label="Snippets">
  {#if prompts.hits.length}
    {#each prompts.hits as hit (hit.snippet.name)}
      <button
        type="button"
        class="match-hit"
        role="option"
        aria-selected="false"
        onclick={() => onInsert(hit.snippet)}
        title="Insert at cursor"
      >
        <span class="match-hit__name">{hit.snippet.name}</span>
      </button>
    {/each}
    {#if truncated}
      <div class="match-panel__empty">
        Showing the first {MATCH_LIMIT} — narrow the list by typing.
      </div>
    {/if}
  {:else if prompts.activeProjectPath === null}
    <div class="match-panel__empty">
      No prompt folder yet. Add one with <strong>⋯</strong> above — pick any directory and every
      <code>.md</code> file in it becomes a snippet.
    </div>
  {:else if prompts.matchQuery.trim()}
    <div class="match-panel__empty">
      {prompts.matching ? 'Matching…' : 'No matching snippets.'}
    </div>
  {:else}
    <div class="match-panel__empty">
      No snippets in this folder yet. Write a prompt below and save it, or drop a <code>.md</code> file
      into the folder.
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
    display: block;
    text-align: left;
    font-family: inherit;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 0.45rem;
    padding: 0.4rem 0.65rem;
    cursor: pointer;
    color: var(--text);
  }
  .match-hit:hover,
  .match-hit:focus-visible {
    border-color: color-mix(in srgb, var(--accent-snippet) 55%, var(--border));
    background: color-mix(in srgb, var(--accent-snippet) 7%, var(--bg-card));
    outline: none;
  }
  /* The name is a path (`rust/code_review`) — mono keeps the slash legible and
     the folder prefix scannable down a long list. */
  .match-hit__name {
    display: block;
    font-family: var(--font-mono);
    font-size: 0.72rem;
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
  .match-panel__empty code {
    font-family: var(--font-mono);
    font-size: 0.95em;
  }
</style>
