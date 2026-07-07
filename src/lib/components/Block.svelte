<script lang="ts">
  /**
   * Block.svelte — renders ONE ContentBlock.
   *
   * Handles: text only. Thinking and tool_use/tool_result rendering (including
   * the subagent "Open →" navigation affordance) were removed — the display
   * model only ever carries user/assistant text now, so nothing else renders.
   *
   * `onOpenSubagent` is accepted but unused: MessageCell.svelte still threads
   * it through as a prop (that plumbing is out of scope for this change), but
   * there is no longer any affordance here that would invoke it.
   */
  import type { ContentBlock, Session } from '$lib/types';
  import { renderMarkdown } from '$lib/markdown';

  let {
    block,
    role = 'assistant',
    onOpenSubagent,
  }: {
    block: ContentBlock;
    role?: 'user' | 'assistant';
    onOpenSubagent?: (session: Session, label: string) => void;
  } = $props();

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
{/if}
