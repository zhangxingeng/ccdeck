<script lang="ts">
  /**
   * Block.svelte — renders ONE ContentBlock, READ-ONLY.
   *
   * Handles: text | thinking | tool_use | tool_result
   *
   * text is the only block type with an edit affordance, and that edit lives
   * in MessageCell.svelte (double-click, applyBlockTextEdit) — this component
   * never edits anything itself, text included.
   *
   * thinking is collapsed by default; expand to read the full content.
   * tool_use ALWAYS shows a one-line intent brief (toolIntent.ts) — never raw
   * JSON, never editable. tool_result shows a short ok/error+size brief, not
   * the full output.
   *
   * There is no subagent "Open →" affordance here — that feature (and its
   * ContentBlock.subagent/agentId/isAsync fields) is gone (see issue #14).
   */
  import type { ContentBlock } from '$lib/types';
  import { renderMarkdown } from '$lib/markdown';
  import { toolIntent, toolResultBrief } from '$lib/toolIntent';

  let {
    block,
    role = 'assistant',
  }: {
    block: ContentBlock;
    role?: 'user' | 'assistant';
  } = $props();

  // Collapsible state — collapsed by default per spec.
  let thinkingOpen = $state(false);

  let label = $derived(role === 'user' ? 'User' : 'Assistant');
  let msgClass = $derived(role === 'user' ? 'msg--user' : 'msg--assistant');
</script>

<!-- ── text block ───────────────────────────────────────────────────────── -->
{#if block.blockType === 'text'}
  <div class="msg {msgClass}">
    <div class="msg__inner">
      <div class="msg__label">{label}</div>
      <div class="msg__body">{@html renderMarkdown(block.text ?? '')}</div>
    </div>
  </div>

<!-- ── thinking block ──────────────────────────────────────────────────── -->
{:else if block.blockType === 'thinking'}
  <div class="msg msg--thinking">
    <div class="msg__inner">
      {#if block.signature && !block.thinking}
        <!-- Encrypted thinking — no toggle, just a muted note -->
        <div class="msg__label">Thinking · encrypted</div>
        <div class="msg__body" style="color: var(--text-faint); font-style: normal;">
          [encrypted thinking]
        </div>
      {:else}
        <!-- Normal thinking — collapsible, collapsed by default -->
        <button
          class="collapsible"
          class:open={thinkingOpen}
          onclick={() => (thinkingOpen = !thinkingOpen)}
          type="button"
          style="background:none;border:0;padding:0;font-family:inherit;cursor:pointer;display:inline-flex;align-items:center;"
        >
          <span class="msg__label" style="margin-bottom:0;">Thinking</span>
          <span class="toggle-icon">&#9654;</span>
        </button>
        <div class="collapse-body" class:open={thinkingOpen}>
          <div class="msg__body">{@html renderMarkdown(block.thinking ?? block.text ?? '')}</div>
        </div>
      {/if}
    </div>
  </div>

<!-- ── tool_use block — intent brief only, NEVER raw JSON ─────────────────── -->
{:else if block.blockType === 'tool_use'}
  <div class="msg msg--tool">
    <div class="msg__inner">
      <div class="msg__label">Tool</div>
      <div class="msg__body tool-intent">{toolIntent(block)}</div>
    </div>
  </div>

<!-- ── tool_result block — short ok/error + size brief ─────────────────────── -->
{:else if block.blockType === 'tool_result'}
  <div class="msg msg--result" class:error={block.isError}>
    <div class="msg__inner">
      <div class="msg__label">{block.isError ? 'Error' : 'Result'}</div>
      <div class="msg__body tool-intent">{toolResultBrief(block)}</div>
    </div>
  </div>
{/if}

<style>
  .tool-intent { font-family: var(--font-mono); font-size: 0.78rem; word-break: break-word; }
</style>
