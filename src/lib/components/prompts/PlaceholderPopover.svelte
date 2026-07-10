<script lang="ts">
  /**
   * Fill-in popover (F5): shown before a piece with {{tokens}} lands in the
   * box. One field per token; Enter inserts, Escape cancels. The same form
   * is reused by instance mode's re-fill (via PieceModal), so this stays a
   * dumb fields+confirm surface.
   */
  import { untrack } from 'svelte';
  import type { Piece } from '$lib/prompts/types';
  import { parsePlaceholders } from '$lib/compose/placeholders';

  let {
    piece,
    onConfirm,
    onCancel,
  }: {
    piece: Piece;
    onConfirm: (fills: Record<string, string>) => void;
    onCancel: () => void;
  } = $props();

  // Body is the source of truth for tokens; the stored array is a cache.
  // Initial capture is intended — the parent remounts the popover per piece.
  const tokens = untrack(() => parsePlaceholders(piece.body));
  let values = $state<Record<string, string>>(Object.fromEntries(tokens.map((t) => [t, ''])));

  function confirm(): void {
    // Only submit tokens the user actually filled — an untouched field keeps
    // its {{token}} literal in the box (visible == honest).
    const fills: Record<string, string> = {};
    for (const t of tokens) if (values[t] !== '') fills[t] = values[t];
    onConfirm(fills);
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Enter') {
      e.preventDefault();
      confirm();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      onCancel();
    }
  }

</script>

<div class="ph-popover" role="dialog" aria-label="Fill placeholders" onkeydown={handleKeydown} tabindex="-1">
  <div class="ph-popover__title">
    Fill in <strong>{piece.title}</strong>
  </div>
  {#each tokens as t, i (t)}
    <label class="ph-popover__field">
      <span class="ph-popover__name">{t}</span>
      <input
        type="text"
        bind:value={values[t]}
        {@attach (el: HTMLInputElement) => {
          if (i === 0) el.focus();
        }}
        placeholder={'{{' + t + '}}'}
        autocomplete="off"
        spellcheck="false"
      />
    </label>
  {/each}
  <div class="ph-popover__actions">
    <button type="button" class="btn btn--ghost btn--sm" onclick={onCancel}>Cancel</button>
    <button type="button" class="btn btn--primary btn--sm" onclick={confirm}>Insert</button>
  </div>
</div>

<style>
  .ph-popover {
    position: absolute;
    top: 2rem;
    left: 50%;
    transform: translateX(-50%);
    z-index: 30;
    background: var(--bg-card);
    border: 1px solid color-mix(in srgb, var(--accent-piece) 45%, var(--border));
    border-radius: 0.5rem;
    box-shadow: 0 10px 32px rgba(0, 0, 0, 0.18);
    padding: 0.8rem 0.9rem;
    width: min(22rem, 90%);
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .ph-popover__title {
    font-size: 0.78rem;
    color: var(--text-muted);
  }
  .ph-popover__field {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .ph-popover__name {
    font-family: var(--font-mono);
    font-size: 0.72rem;
    color: var(--accent-template);
    min-width: 5.5rem;
    text-align: right;
  }
  .ph-popover__field input {
    flex: 1;
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0.3rem 0.5rem;
    border: 1px solid var(--border);
    border-radius: 0.35rem;
    background: var(--bg);
    color: var(--text);
  }
  .ph-popover__field input:focus {
    outline: none;
    border-color: var(--accent-piece);
  }
  .ph-popover__actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.4rem;
    margin-top: 0.2rem;
  }
</style>
