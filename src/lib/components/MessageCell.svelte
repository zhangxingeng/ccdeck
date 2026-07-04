<script lang="ts">
  /**
   * MessageCell.svelte — one chat bubble (a single JSONL line that carries text).
   *
   * Renders exactly like the read-only viewer (shared Block.svelte styling) but:
   *   - every text block is editable in place on double-click (per-block, addressed
   *     by text ordinal — a bubble can hold several text blocks);
   *   - non-text blocks (thinking / tool calls) render collapsed via Block;
   *   - a hover toolbar exposes version history (◀ ▶), a diff panel, speaker
   *     change, reorder, raw-JSON escape hatch, and delete/restore.
   *
   * Editing state is LOCAL to this component (self-contained); the parent only
   * receives committed mutations through the callback props.
   */
  import type { Entry, Session } from '$lib/types';
  import type { DraftRow } from '$lib/editDraft';
  import { extractText, MESSAGE_ROLES } from '$lib/editDraft';
  import { availableTargets, targetIndex, targetLabel, type DiffTarget } from '$lib/diff';
  import { renderMarkdown } from '$lib/markdown';
  import Block from './Block.svelte';
  import DiffView from './DiffView.svelte';

  let {
    msgKey,
    row,
    entry,
    onBlockEdit,
    onDelete,
    onRestore,
    onRole,
    onMoveUp,
    onMoveDown,
    onRaw,
    onSetVersion,
    onResumeFrom,
    onOpenSubagent,
  }: {
    msgKey: string;
    row: DraftRow;
    entry: Entry;
    onBlockEdit: (ordinal: number, text: string) => void;
    onDelete: () => void;
    onRestore: () => void;
    onRole: (role: string) => void;
    onMoveUp: () => void;
    onMoveDown: () => void;
    onRaw: () => void;
    onSetVersion: (idx: number) => void;
    onResumeFrom: () => void;
    onOpenSubagent?: (session: Session, label: string) => void;
  } = $props();

  // ── Derived shape ──────────────────────────────────────────────────────────
  let role = $derived<'user' | 'assistant'>(entry.type === 'user' ? 'user' : 'assistant');
  let label = $derived(role === 'user' ? 'User' : 'Assistant');
  let roleClass = $derived(role === 'user' ? 'msg--user' : 'msg--assistant');
  let canRole = $derived(entry.type === 'user' || entry.type === 'assistant');

  // Map each block index → its ordinal among text blocks (-1 for non-text).
  let textOrdinals = $derived.by<number[]>(() => {
    let seen = -1;
    return entry.blocks.map(b => (b.blockType === 'text' ? ++seen : -1));
  });

  let versionCount = $derived(row.versions.length);
  let diffTargets = $derived<DiffTarget[]>(availableTargets(row.active, versionCount));

  // ── Local edit state ───────────────────────────────────────────────────────
  let editingOrdinal = $state<number | null>(null);
  let editingText = $state('');

  // ── Local diff state ───────────────────────────────────────────────────────
  let showDiff = $state(false);
  let diffTarget = $state<DiffTarget | null>(null);

  // Reset transient UI whenever the active version changes underfoot.
  // Seeded to -1 so the first effect run (which just records the current version)
  // never fights a real version change.
  let lastActive = $state(-1);
  $effect(() => {
    if (row.active !== lastActive) {
      lastActive = row.active;
      editingOrdinal = null;
      editingText = '';
    }
  });

  // Keep the chosen diff target valid as versions change.
  let effectiveTarget = $derived<DiffTarget | null>(
    diffTargets.length === 0
      ? null
      : diffTarget && diffTargets.includes(diffTarget)
        ? diffTarget
        : diffTargets[0]
  );

  let diffOldText = $derived(
    effectiveTarget
      ? extractText(row.versions[targetIndex(effectiveTarget, row.active, versionCount)])
      : ''
  );
  let diffNewText = $derived(extractText(row.versions[row.active]));

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
  class:msg-group--deleted={row.deleted}
  class:msg-group--editing={editingOrdinal !== null}
>
  <!-- Hover toolbar -->
  <div class="msg-tools">
    {#if versionCount > 1}
      <span class="msg-tools__versions">
        <button
          class="msg-tools__btn"
          onclick={() => onSetVersion(row.active - 1)}
          disabled={row.active === 0}
          title="Previous version"
          type="button"
        >&#9664;</button>
        <span class="msg-tools__vlabel">v{row.active + 1}/{versionCount}</span>
        <button
          class="msg-tools__btn"
          onclick={() => onSetVersion(row.active + 1)}
          disabled={row.active === versionCount - 1}
          title="Next version"
          type="button"
        >&#9654;</button>
      </span>
      <button
        class="msg-tools__btn"
        class:msg-tools__btn--on={showDiff}
        onclick={() => (showDiff = !showDiff)}
        title="Show diff between versions"
        type="button"
      >&#8644;</button>
    {/if}

    {#if canRole && !row.deleted}
      <select
        class="msg-tools__role"
        value={entry.type}
        onchange={(e) => onRole((e.target as HTMLSelectElement).value)}
        title="Change speaker"
      >
        {#each MESSAGE_ROLES as r}<option value={r}>{r}</option>{/each}
      </select>
    {/if}

    {#if !row.deleted}
      <button class="msg-tools__btn" onclick={onMoveUp} title="Move up" type="button">↑</button>
      <button class="msg-tools__btn" onclick={onMoveDown} title="Move down" type="button">↓</button>
      <button class="msg-tools__btn" onclick={onRaw} title="Edit raw JSON" type="button">{'{ }'}</button>
      <button class="msg-tools__btn" onclick={onResumeFrom} title="Fork &amp; resume from here" type="button">⑂</button>
      <button class="msg-tools__btn msg-tools__btn--danger" onclick={onDelete} title="Delete message" type="button">✕</button>
    {:else}
      <button class="msg-tools__btn" onclick={onRestore} title="Restore message" type="button">Restore</button>
    {/if}
  </div>

  <!-- Diff panel (version comparison) -->
  {#if showDiff && effectiveTarget}
    <div class="diff-panel">
      <div class="diff-panel__bar">
        <span class="diff-panel__label">Compare v{row.active + 1} to</span>
        {#each diffTargets as t (t)}
          <button
            class="diff-panel__chip"
            class:diff-panel__chip--on={effectiveTarget === t}
            onclick={() => (diffTarget = t)}
            type="button"
          >{targetLabel(t)}</button>
        {/each}
      </div>
      <DiffView oldText={diffOldText} newText={diffNewText} />
    </div>
  {/if}

  <!-- Blocks: text blocks are editable; everything else renders read-only -->
  {#each entry.blocks as block, bi (bi)}
    {#if block.blockType === 'text'}
      {@const ordinal = textOrdinals[bi]}
      <div class="msg {roleClass}">
        <div class="msg__inner">
          <div class="msg__header">
            <span class="msg__label">{label}</span>
            {#if row.active !== 0}<span class="row-edited-badge">edited</span>{/if}
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
      <Block {block} {role} {onOpenSubagent} />
    {/if}
  {/each}
</div>

<style>
  .msg-group { position: relative; }
  .msg-group--deleted { opacity: 0.4; }
  .msg-group--deleted :global(.msg__body) { text-decoration: line-through; }
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
  .msg-tools__versions { display: inline-flex; align-items: center; gap: 0.15rem; }
  .msg-tools__vlabel {
    font-size: 0.6rem; color: var(--text-muted); font-family: var(--font-mono); min-width: 5ch; text-align: center;
  }
  .msg-tools__btn {
    background: none; border: 0; cursor: pointer; color: var(--text-muted);
    font-size: 0.78rem; line-height: 1; padding: 0.18rem 0.32rem; border-radius: 0.25rem; font-family: inherit;
  }
  .msg-tools__btn:hover:not(:disabled) { background: var(--bg-subtle); color: var(--text); }
  .msg-tools__btn:disabled { opacity: 0.3; cursor: default; }
  .msg-tools__btn--on { background: var(--bg-subtle); color: var(--accent-user); }
  .msg-tools__btn--danger:hover:not(:disabled) {
    color: var(--accent-result-err); background: color-mix(in srgb, var(--accent-result-err) 12%, transparent);
  }
  .msg-tools__role {
    font-size: 0.62rem; padding: 0.1rem 0.25rem; border-radius: 0.25rem;
    border: 1px solid var(--border-strong); background: var(--bg-card);
    color: var(--text); font-family: var(--font-mono); cursor: pointer;
  }

  /* Diff panel */
  .diff-panel { margin: 0.1rem 0 0.6rem; display: flex; flex-direction: column; gap: 0.4rem; }
  .diff-panel__bar { display: flex; align-items: center; gap: 0.3rem; flex-wrap: wrap; }
  .diff-panel__label {
    font-size: 0.62rem; font-weight: 600; text-transform: uppercase; letter-spacing: 0.06em;
    color: var(--text-faint); margin-right: 0.15rem;
  }
  .diff-panel__chip {
    font-size: 0.66rem; padding: 0.12rem 0.45rem; border-radius: 0.25rem; cursor: pointer;
    border: 1px solid var(--border); background: var(--bg-card); color: var(--text-muted); font-family: inherit;
  }
  .diff-panel__chip:hover { color: var(--text); }
  .diff-panel__chip--on {
    color: var(--accent-user); border-color: color-mix(in srgb, var(--accent-user) 45%, transparent);
    background: color-mix(in srgb, var(--accent-user) 10%, transparent);
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
