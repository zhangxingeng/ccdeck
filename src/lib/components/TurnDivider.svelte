<script lang="ts">
  /**
   * TurnDivider.svelte — a subtle separator rendered at each turn boundary
   * (see displayModel.ts's deriveTurnSpans). It carries the turn-level soft
   * delete/undelete affordance without disrupting the "just the chat" reading
   * flow: a thin rule, with the action revealed on hover (consistent with the
   * existing message hover-toolbar styling). Deletion itself is soft and
   * reversible, so there is no confirm — a fully-deleted turn flips the button
   * to "Restore turn".
   *
   * This is NOT Turn.svelte (that's the separate export-only HTML-build path).
   *
   * The optional select-mode checkbox (issue #14 checkpoint 5) selects/deselects
   * the whole turn as one unit; it's inert unless `selectMode` is set.
   */
  let {
    deleted,
    selectMode = false,
    selected = false,
    onDelete,
    onUndelete,
    onToggleSelect,
  }: {
    deleted: boolean;
    selectMode?: boolean;
    selected?: boolean;
    onDelete: () => void;
    onUndelete: () => void;
    onToggleSelect?: () => void;
  } = $props();
</script>

<div class="turn-divider" class:turn-divider--deleted={deleted} class:turn-divider--select={selectMode}>
  <span class="turn-divider__rule"></span>
  <div class="turn-divider__actions">
    {#if selectMode}
      <label class="turn-divider__check">
        <input type="checkbox" checked={selected} onchange={() => onToggleSelect?.()} />
        <span>Turn</span>
      </label>
    {/if}
    {#if deleted}
      <button class="turn-divider__btn" onclick={onUndelete} type="button">Restore turn</button>
    {:else}
      <button class="turn-divider__btn turn-divider__btn--danger" onclick={onDelete} type="button">Delete turn</button>
    {/if}
  </div>
</div>

<style>
  .turn-divider {
    display: flex; align-items: center; gap: 0.5rem;
    margin: 0.65rem 0 0.35rem;
  }
  .turn-divider__rule {
    flex: 1; height: 1px; background: var(--border);
  }
  .turn-divider--deleted .turn-divider__rule {
    background: color-mix(in srgb, var(--accent-result-err) 45%, var(--border));
  }
  .turn-divider__actions { display: flex; align-items: center; gap: 0.4rem; }
  .turn-divider__check {
    display: inline-flex; align-items: center; gap: 0.25rem;
    font-size: 0.66rem; color: var(--text-muted); cursor: pointer; user-select: none;
  }
  .turn-divider__check input { cursor: pointer; margin: 0; }
  .turn-divider__btn {
    background: none; border: 1px solid var(--border); border-radius: 0.3rem; cursor: pointer;
    font-size: 0.62rem; padding: 0.12rem 0.4rem; color: var(--text-faint); font-family: inherit;
    opacity: 0; transition: opacity 0.1s, color 0.1s, border-color 0.1s;
  }
  /* Reveal on hover of the divider (or always, once select mode / deleted). */
  .turn-divider:hover .turn-divider__btn,
  .turn-divider--deleted .turn-divider__btn,
  .turn-divider--select .turn-divider__btn { opacity: 0.85; }
  .turn-divider__btn:hover { opacity: 1; color: var(--text); }
  .turn-divider__btn--danger:hover {
    color: var(--accent-result-err); border-color: color-mix(in srgb, var(--accent-result-err) 40%, transparent);
  }
</style>
