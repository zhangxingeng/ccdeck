<script lang="ts">
  /**
   * ToolGroup.svelte — a collapsed strip standing in for a contiguous run of
   * non-chat rows (tool calls, tool results, standalone thinking) that sit
   * between two chat messages (see displayModel.ts's DisplayToolGroup).
   * Collapsed by default so the transcript reads as "just the chat"; expand
   * to see each member's read-only brief (Block.svelte — never raw JSON).
   *
   * This is a fresh component, NOT the old (deleted) editor ToolGroup and NOT
   * Turn.svelte (which is the separate export-only HTML-build path — see
   * builder.ts). Delete/undelete affordances land in a later checkpoint.
   */
  import type { Entry } from '$lib/types';
  import type { DraftRow } from '$lib/editDraft';
  import Block from './Block.svelte';

  export interface GroupItem {
    key: string;
    row: DraftRow;
    entry: Entry;
  }

  let { items }: { items: GroupItem[] } = $props();

  let open = $state(false);

  // Summarize what's inside so the collapsed header is informative, e.g.
  // "2 tool calls · 1 result · 1 thinking".
  let summary = $derived.by(() => {
    let tools = 0, results = 0, thinking = 0;
    for (const it of items) {
      for (const b of it.entry.blocks) {
        if (b.blockType === 'tool_use') tools++;
        else if (b.blockType === 'tool_result') results++;
        else if (b.blockType === 'thinking') thinking++;
      }
    }
    const parts: string[] = [];
    if (tools) parts.push(`${tools} tool call${tools === 1 ? '' : 's'}`);
    if (results) parts.push(`${results} result${results === 1 ? '' : 's'}`);
    if (thinking) parts.push(`${thinking} thinking`);
    return parts.length ? parts.join(' · ') : `${items.length} item${items.length === 1 ? '' : 's'}`;
  });
</script>

<div class="tool-group">
  <button
    class="tool-group__toggle"
    class:open
    onclick={() => (open = !open)}
    type="button"
  >
    <span class="toggle-icon">&#9654;</span>
    <span class="tool-group__gear">⚙</span>
    <span class="tool-group__summary">{summary}</span>
    <span class="tool-group__hint">{open ? 'hide' : 'show'}</span>
  </button>

  {#if open}
    <div class="tool-group__body">
      {#each items as it (it.key)}
        <div class="tool-line">
          {#each it.entry.blocks as block, bi (bi)}
            <Block {block} role={it.entry.type === 'user' ? 'user' : 'assistant'} />
          {/each}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .tool-group {
    margin: 0.15rem 0; border: 1px dashed var(--border); border-radius: 0.45rem;
    background: color-mix(in srgb, var(--bg-subtle) 55%, transparent);
  }
  .tool-group__toggle {
    width: 100%; display: inline-flex; align-items: center; gap: 0.45rem;
    background: none; border: 0; cursor: pointer; font-family: inherit; text-align: left;
    color: var(--text-muted); font-size: 0.75rem; padding: 0.4rem 0.55rem;
  }
  .tool-group__toggle .toggle-icon {
    display: inline-block; transition: transform 0.12s; font-size: 0.6rem; color: var(--text-faint);
  }
  .tool-group__toggle.open .toggle-icon { transform: rotate(90deg); }
  .tool-group__gear { opacity: 0.7; }
  .tool-group__summary { flex: 1; font-weight: 500; }
  .tool-group__hint { color: var(--text-faint); font-size: 0.68rem; }
  .tool-group__body {
    padding: 0.25rem 0.6rem 0.5rem; display: flex; flex-direction: column; gap: 0.3rem;
    border-top: 1px dashed var(--border);
  }
  .tool-line { position: relative; }
</style>
