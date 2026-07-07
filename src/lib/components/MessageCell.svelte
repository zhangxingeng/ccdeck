<script lang="ts">
  /**
   * MessageCell.svelte — one chat bubble (a single JSONL line that carries text).
   *
   * Renders exactly like the read-only viewer (shared Block.svelte styling) but:
   *   - every text block is editable in place on double-click (per-block, addressed
   *     by text ordinal — a bubble can hold several text blocks);
   *   - non-text blocks (thinking / tool calls) render collapsed via Block;
   *   - a hover toolbar exposes speaker change and the raw-JSON escape hatch.
   *
   * Editing state is LOCAL to this component (self-contained); the parent only
   * receives committed mutations through the callback props.
   */
  import type { Entry } from '$lib/types';
  import type { DraftRow } from '$lib/editDraft';
  import { MESSAGE_ROLES } from '$lib/editDraft';
  import { renderMarkdown } from '$lib/markdown';
  import Block from './Block.svelte';

  let {
    row,
    entry,
    onBlockEdit,
    onRole,
    onRaw,
    onResumeFrom,
  }: {
    row: DraftRow;
    entry: Entry;
    onBlockEdit: (ordinal: number, text: string) => void;
    onRole: (role: string) => void;
    onRaw: () => void;
    onResumeFrom: () => void;
  } = $props();

  // ── Derived shape ──────────────────────────────────────────────────────────
  let role = $derived<'user' | 'assistant'>(entry.type === 'user' ? 'user' : 'assistant');
  let label = $derived(role === 'user' ? 'User' : 'Assistant');
  let roleClass = $derived(role === 'user' ? 'msg--user' : 'msg--assistant');
  let canRole = $derived(entry.type === 'user' || entry.type === 'assistant');
  let edited = $derived(row.value !== row.original);

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
>
  <!-- Hover toolbar -->
  <div class="msg-tools">
    {#if canRole}
      <select
        class="msg-tools__role"
        value={entry.type}
        onchange={(e) => onRole((e.target as HTMLSelectElement).value)}
        title="Change speaker"
      >
        {#each MESSAGE_ROLES as r}<option value={r}>{r}</option>{/each}
      </select>
    {/if}

    <button class="msg-tools__btn" onclick={onRaw} title="Edit raw JSON" type="button">{'{ }'}</button>
    <button class="msg-tools__btn" onclick={onResumeFrom} title="Fork &amp; resume from here" type="button">⑂</button>
  </div>

  <!-- Blocks: text blocks are editable; everything else renders read-only -->
  {#each entry.blocks as block, bi (bi)}
    {#if block.blockType === 'text'}
      {@const ordinal = textOrdinals[bi]}
      <div class="msg {roleClass}">
        <div class="msg__inner">
          <div class="msg__header">
            <span class="msg__label">{label}</span>
            {#if edited}<span class="row-edited-badge">edited</span>{/if}
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
      <Block {block} {role} />
    {/if}
  {/each}
</div>

<style>
  .msg-group { position: relative; }
  .msg-group--editing { scroll-margin-top: 1rem; }

  .msg__header { display: flex; align-items: center; gap: 0.4rem; margin-bottom: 0.4rem; }
  .row-edited-badge {
    font-size: 0.58rem; font-weight: 600; text-transform: uppercase; letter-spacing: 0.06em;
    color: var(--accent-user);
    background: color-mix(in srgb, var(--accent-user) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent-user) 30%, transparent);
    padding: 0.08rem 0.35rem; border-radius: 0.2rem;
  }

  /* Hover toolbar, pinned to the message's top-right */
  .msg-tools {
    position: absolute; top: 0.2rem; right: 0.2rem; z-index: 3;
    display: flex; align-items: center; gap: 0.2rem;
    padding: 0.15rem 0.25rem; border-radius: 0.4rem;
    background: color-mix(in srgb, var(--bg-card) 92%, transparent);
    border: 1px solid var(--border); backdrop-filter: blur(4px);
    opacity: 0; transition: opacity 0.1s;
  }
  .msg-group:hover .msg-tools, .msg-group:focus-within .msg-tools { opacity: 1; }
  .msg-tools__btn {
    background: none; border: 0; cursor: pointer; color: var(--text-muted);
    font-size: 0.78rem; line-height: 1; padding: 0.18rem 0.32rem; border-radius: 0.25rem; font-family: inherit;
  }
  .msg-tools__btn:hover:not(:disabled) { background: var(--bg-subtle); color: var(--text); }
  .msg-tools__role {
    font-size: 0.62rem; padding: 0.1rem 0.25rem; border-radius: 0.25rem;
    border: 1px solid var(--border-strong); background: var(--bg-card);
    color: var(--text); font-family: var(--font-mono); cursor: pointer;
  }

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
</style>
