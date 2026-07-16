<script lang="ts">
  /**
   * MessageCell.svelte — one chat bubble (a single JSONL line that carries text).
   *
   * Renders exactly like the read-only viewer (shared Block.svelte styling) but:
   *   - every text block is editable in place on double-click (per-block, addressed
   *     by text ordinal — a bubble can hold several text blocks);
   *   - non-text blocks (thinking / tool calls) render read-only via Block;
   *   - every block (text or not) has a soft delete/undelete affordance — deleted
   *     blocks fade in place and show "Undelete"; nothing leaves the file until
   *     Save (see editDraft.ts's deletedBlocks);
   *   - a hover toolbar exposes fork-and-resume-from-here.
   *
   * Editing state is LOCAL to this component (self-contained); the parent only
   * receives committed mutations through the callback props.
   */
  import type { Entry } from '$lib/types';
  import type { DraftRow } from '$lib/editDraft';
  import { blockKey } from '$lib/editDraft';
  import { renderMarkdown } from '$lib/markdown';
  import Block from './Block.svelte';

  let {
    row,
    entry,
    deletedBlocks,
    selectMode = false,
    selected = false,
    onToggleSelect,
    onBlockEdit,
    onDeleteBlock,
    onUndeleteBlock,
    onResumeFrom,
  }: {
    row: DraftRow;
    entry: Entry;
    deletedBlocks: Set<string>;
    selectMode?: boolean;
    selected?: boolean;
    onToggleSelect?: () => void;
    onBlockEdit: (ordinal: number, text: string) => void;
    onDeleteBlock: (blockIndex: number) => void;
    onUndeleteBlock: (blockIndex: number) => void;
    /** Fork this session from here, then show its copyable resume facts at the
     *  click point (issue #34). The event carries the position for the popover. */
    onResumeFrom: (e: MouseEvent) => void;
  } = $props();

  // ── Derived shape ──────────────────────────────────────────────────────────
  let role = $derived<'user' | 'assistant'>(entry.type === 'user' ? 'user' : 'assistant');
  let label = $derived(role === 'user' ? 'User' : 'Assistant');
  let roleClass = $derived(role === 'user' ? 'msg--user' : 'msg--assistant');
  let edited = $derived(row.value !== row.original);

  function isDeleted(bi: number): boolean {
    return deletedBlocks.has(blockKey(row, bi));
  }

  // Map each block index → its ordinal among text blocks (-1 for non-text).
  let textOrdinals = $derived.by<number[]>(() => {
    let seen = -1;
    return entry.blocks.map(b => (b.blockType === 'text' ? ++seen : -1));
  });

  // ── Local edit state ───────────────────────────────────────────────────────
  let editingOrdinal = $state<number | null>(null);
  let editingText = $state('');

  // ── Handlers ───────────────────────────────────────────────────────────────
  function startEdit(ordinal: number, current: string) {
    editingOrdinal = ordinal;
    editingText = current;
  }
  function commitEdit() {
    if (editingOrdinal === null) return;
    onBlockEdit(editingOrdinal, editingText);
    editingOrdinal = null;
    editingText = '';
  }
  function cancelEdit() {
    editingOrdinal = null;
    editingText = '';
  }
  function onTextareaKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); commitEdit(); }
    else if (e.key === 'Escape') { e.preventDefault(); cancelEdit(); }
  }
</script>

<div
  class="msg-group"
  class:msg-group--editing={editingOrdinal !== null}
  class:msg-group--selected={selectMode && selected}
>
  <!-- Select-mode checkbox (bulk multi-select) — the whole bubble is one unit. -->
  {#if selectMode}
    <label class="msg-select">
      <input type="checkbox" checked={selected} onchange={() => onToggleSelect?.()} />
    </label>
  {/if}

  <!-- Fork-from-here, pinned in the RIGHT gutter (outside the bubble) mirroring
       the select-mode checkbox's left gutter — it used to float top-right and
       collide with the per-block delete ✕. Hidden in select mode, where
       forking a single message makes no sense. -->
  {#if !selectMode}
    <div class="msg-tools">
      <button
        class="msg-tools__btn"
        onclick={onResumeFrom}
        title="Fork from here, then copy its resume command"
        type="button"
      >⑂</button>
    </div>
  {/if}

  <!-- Blocks: text blocks are editable; everything else renders read-only.
       Every block gets a delete/undelete affordance (soft delete). -->
  {#each entry.blocks as block, bi (bi)}
    {@const deleted = isDeleted(bi)}
    {#if block.blockType === 'text'}
      {@const ordinal = textOrdinals[bi]}
      <div class="msg {roleClass} block-slot" class:block-slot--deleted={deleted}>
        <div class="msg__inner">
          <div class="msg__header">
            <span class="msg__label">{label}</span>
            {#if edited}<span class="row-edited-badge">edited</span>{/if}
            {#if deleted}<span class="row-deleted-badge">deleted</span>{/if}
            <span class="block-slot__spacer"></span>
            {#if deleted}
              <button class="msg-tools__btn" onclick={() => onUndeleteBlock(bi)} type="button">Undelete</button>
            {:else}
              <button class="msg-tools__btn msg-tools__btn--danger" onclick={() => onDeleteBlock(bi)} title="Delete" type="button">✕</button>
            {/if}
          </div>
          {#if editingOrdinal === ordinal}
            <!-- svelte-ignore a11y_autofocus -->
            <textarea
              class="editor-textarea"
              bind:value={editingText}
              rows={Math.min(20, Math.max(4, (editingText.match(/\n/g)?.length ?? 0) + 2))}
              onkeydown={onTextareaKeydown}
              autofocus
            ></textarea>
            <div class="cell-edit-actions">
              <span class="cell-edit-hint">Enter to save · Shift+Enter for newline · Esc to cancel</span>
              <button class="btn btn--sm btn--primary" onmousedown={(e) => { e.preventDefault(); commitEdit(); }} type="button">Save</button>
              <button class="btn btn--sm btn--ghost" onmousedown={(e) => { e.preventDefault(); cancelEdit(); }} type="button">Cancel</button>
            </div>
          {:else if deleted}
            <div class="msg__body">{@html renderMarkdown(block.text ?? '')}</div>
          {:else}
            <div
              class="msg__body msg__body--editable"
              ondblclick={() => startEdit(ordinal, block.text ?? '')}
              role="button"
              tabindex="0"
              title="Double-click to edit"
            >{@html renderMarkdown(block.text ?? '')}</div>
          {/if}
        </div>
      </div>
    {:else}
      <div class="block-slot" class:block-slot--deleted={deleted}>
        <div class="block-slot__tools">
          {#if deleted}
            <button class="msg-tools__btn" onclick={() => onUndeleteBlock(bi)} type="button">Undelete</button>
          {:else}
            <button class="msg-tools__btn msg-tools__btn--danger" onclick={() => onDeleteBlock(bi)} title="Delete" type="button">✕</button>
          {/if}
        </div>
        <Block {block} {role} />
      </div>
    {/if}
  {/each}
</div>

<style>
  .msg-group { position: relative; }
  .msg-group--editing { scroll-margin-top: 1rem; }
  .msg-group--selected {
    outline: 2px solid color-mix(in srgb, var(--accent-user) 55%, transparent);
    outline-offset: 2px; border-radius: 0.45rem;
  }

  /* Select-mode checkbox, pinned to the bubble's top-left */
  .msg-select {
    position: absolute; top: 0.3rem; left: -1.35rem; z-index: 3;
    display: flex; align-items: center; cursor: pointer;
  }
  .msg-select input { cursor: pointer; margin: 0; }

  .msg__header { display: flex; align-items: center; gap: 0.4rem; margin-bottom: 0.4rem; }
  .block-slot__spacer { flex: 1; }
  .row-edited-badge {
    font-size: 0.58rem; font-weight: 600; text-transform: uppercase; letter-spacing: 0.06em;
    color: var(--accent-user);
    background: color-mix(in srgb, var(--accent-user) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent-user) 30%, transparent);
    padding: 0.08rem 0.35rem; border-radius: 0.2rem;
  }
  .row-deleted-badge {
    font-size: 0.58rem; font-weight: 600; text-transform: uppercase; letter-spacing: 0.06em;
    color: var(--accent-result-err);
    background: color-mix(in srgb, var(--accent-result-err) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent-result-err) 30%, transparent);
    padding: 0.08rem 0.35rem; border-radius: 0.2rem;
  }

  /* Fork affordance, pinned in the right gutter (outside the bubble) — the
     mirror of .msg-select's left gutter (left: -1.35rem), so its clip behaviour
     at the min window width matches the checkbox's. No box needed here: it sits
     over the page background, not over message content. */
  .msg-tools {
    position: absolute; top: 0.3rem; right: -1.35rem; z-index: 3;
    display: flex; align-items: center;
    opacity: 0; transition: opacity 0.1s;
  }
  .msg-group:hover .msg-tools, .msg-group:focus-within .msg-tools { opacity: 1; }
  .msg-tools__btn {
    background: none; border: 0; cursor: pointer; color: var(--text-muted);
    font-size: 0.78rem; line-height: 1; padding: 0.18rem 0.32rem; border-radius: 0.25rem; font-family: inherit;
  }
  .msg-tools__btn:hover:not(:disabled) { background: var(--bg-subtle); color: var(--text); }
  .msg-tools__btn--danger:hover { color: var(--accent-result-err); background: color-mix(in srgb, var(--accent-result-err) 12%, transparent); }

  /* Editable text body */
  .msg__body--editable { cursor: text; border-radius: 0.25rem; }
  .msg__body--editable:hover {
    outline: 1px dashed color-mix(in srgb, var(--accent-user) 40%, transparent); outline-offset: 2px;
  }

  .editor-textarea {
    width: 100%; box-sizing: border-box; font-family: var(--font-sans); font-size: 0.9rem;
    line-height: 1.5; padding: 0.6rem 0.7rem; border-radius: 0.4rem;
    border: 1px solid var(--accent-user); background: var(--bg); color: var(--text); resize: vertical;
  }
  .editor-textarea:focus { outline: 2px solid var(--accent-user); outline-offset: 1px; }
  .cell-edit-actions { display: flex; align-items: center; gap: 0.4rem; margin-top: 0.45rem; }
  .cell-edit-hint { flex: 1; font-size: 0.68rem; color: var(--text-faint); }

  /* Soft-deleted blocks: fade in place (non-text blocks — text-block fade is
     handled inline via .row-deleted-badge + normal opacity below). */
  .block-slot { position: relative; }
  .block-slot--deleted { opacity: 0.45; }
  .block-slot--deleted :global(.msg__body) { text-decoration: line-through; }
  .block-slot__tools {
    position: absolute; top: 0.3rem; right: 0.3rem; z-index: 2;
    display: flex; gap: 0.2rem; opacity: 0; transition: opacity 0.1s;
  }
  .block-slot:hover .block-slot__tools, .block-slot--deleted .block-slot__tools { opacity: 1; }
  .block-slot__tools .msg-tools__btn {
    background: color-mix(in srgb, var(--bg-card) 90%, transparent); border: 1px solid var(--border);
  }
</style>
