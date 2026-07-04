<script lang="ts">
  /**
   * Block.svelte — renders ONE ContentBlock.
   *
   * Handles: text | thinking | tool_use | tool_result
   * tool_use blocks with .subagent show an "Open →" affordance instead of
   * rendering the nested transcript inline; onOpenSubagent bubbles up to
   * whichever ancestor owns the stacked-navigation view (SessionEditor).
   */
  import type { ContentBlock, Session } from '$lib/types';
  import { renderMarkdown } from '$lib/markdown';
  import { highlightJson, isLongMarkdownish } from '$lib/jsonHighlight';

  let {
    block,
    role = 'assistant',
    onOpenSubagent,
  }: {
    block: ContentBlock;
    role?: 'user' | 'assistant';
    onOpenSubagent?: (session: Session, label: string) => void;
  } = $props();

  // Collapsible state — collapsed by default per spec.
  let thinkingOpen = $state(false);
  let toolOpen = $state(false);

  let label = $derived(role === 'user' ? 'User' : 'Assistant');
  let msgClass = $derived(role === 'user' ? 'msg--user' : 'msg--assistant');

  // Tool input: collapsed to key-name chips by default; a popover shows the
  // full highlighted/prettified JSON on click. Scoped to inputs only — output
  // stays a plain <pre> (results are often huge and already fine raw).
  let inputKeys = $derived(block.toolInput ? Object.keys(block.toolInput) : []);
  let inputPopoverOpen = $state(false);
  let highlightedInput = $derived(block.toolInput ? highlightJson(block.toolInput) : '');
  let longStringEntries = $derived.by<[string, string][]>(() => {
    if (!block.toolInput) return [];
    return Object.entries(block.toolInput).filter((e): e is [string, string] => isLongMarkdownish(e[1]));
  });
  let renderedKeys = $state<Set<string>>(new Set());
  function toggleRendered(key: string) {
    const next = new Set(renderedKeys);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    renderedKeys = next;
  }
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

<!-- ── tool_use block ──────────────────────────────────────────────────── -->
{:else if block.blockType === 'tool_use'}
  <div class="msg msg--tool">
    <div class="msg__inner">
      <!-- Header / toggle -->
      <button
        class="collapsible"
        class:open={toolOpen}
        onclick={() => (toolOpen = !toolOpen)}
        type="button"
        style="background:none;border:0;padding:0;font-family:inherit;cursor:pointer;display:inline-flex;align-items:center;gap:0.4rem;"
      >
        <span class="msg__label" style="margin-bottom:0;">Tool</span>
        <span style="font-size:0.8rem;color:var(--text-muted);font-weight:500;">{block.toolName ?? 'unknown'}</span>
        <span class="toggle-icon">&#9654;</span>
        {#if block.isAsync}
          <span style="font-size:0.65rem;color:var(--text-faint);margin-left:0.25rem;">async</span>
        {/if}
        {#if block.toolOutput !== undefined || block.isError !== undefined}
          <span style="font-size:0.65rem;color:{block.isError ? 'var(--accent-result-err)' : 'var(--accent-result-ok)'};margin-left:0.25rem;">
            {block.isError ? 'error' : 'ok'}
          </span>
        {/if}
      </button>

      <!-- Collapsible body -->
      <div class="collapse-body" class:open={toolOpen}>
        <!-- Input section: collapsed to key chips, click for the full
             highlighted JSON in a popover. -->
        {#if block.toolInput && inputKeys.length > 0}
          <div class="tool-section">
            <div class="tool-section__heading">Input</div>
            <button type="button" class="tool-input-chips" onclick={() => (inputPopoverOpen = true)}>
              {#each inputKeys as k (k)}
                <span class="tool-input-chip">{k}</span>
              {/each}
              <span class="tool-input-chips__expand">Expand ⤢</span>
            </button>
          </div>
        {/if}

        <!-- Result section (merged onto tool_use per builder.ts) -->
        {#if block.toolOutput !== undefined}
          <div class="msg--result" class:error={block.isError} style="margin-top:0.5rem;">
            <div class="tool-section">
              <div class="tool-section__heading">{block.isError ? 'Error' : 'Result'}</div>
              <pre class="tool-json">{block.toolOutput}</pre>
            </div>
          </div>
        {/if}

        <!-- Subagent affordance (if this tool_use launched an agent) — opens
             the nested transcript in the stacked navigation view rather than
             unspooling it inline. -->
        {#if block.subagent}
          {@const label =
            (typeof block.toolInput?.description === 'string' ? block.toolInput.description : null) ??
            (block.subagent.meta.project || 'Subagent')}
          <button
            type="button"
            class="subagent-open"
            onclick={() => onOpenSubagent?.(block.subagent as Session, label)}
          >
            <span class="subagent-open__icon">▸</span>
            <span class="subagent-open__label">Subagent · {label}</span>
            <span class="subagent-open__meta">
              {block.subagent.turns.length} turn{block.subagent.turns.length === 1 ? '' : 's'}
            </span>
            <span class="subagent-open__cta">Open →</span>
          </button>
        {/if}
      </div>
    </div>
  </div>

<!-- ── standalone tool_result block ────────────────────────────────────── -->
{:else if block.blockType === 'tool_result'}
  <div class="msg msg--result" class:error={block.isError}>
    <div class="msg__inner">
      <div class="msg__label">{block.isError ? 'Error' : 'Result'}</div>
      <pre class="tool-json">{block.toolOutput ?? block.text ?? ''}</pre>
    </div>
  </div>
{/if}

<!-- ── Tool input popover: read-only, prettified + highlighted. Editing still
     goes through the existing raw-JSON-line editor (the row's own "{ }"
     button) — this doesn't duplicate that mechanism. ──────────────────── -->
{#if inputPopoverOpen}
  <div class="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="tool-input-title">
    <div class="modal tool-input-modal">
      <h3 id="tool-input-title">Input — {block.toolName ?? 'unknown'}</h3>
      <pre class="tool-json">{@html highlightedInput}</pre>
      {#each longStringEntries as [key, value] (key)}
        <div class="tool-input-longstring">
          <div class="tool-input-longstring__bar">
            <span class="tool-input-longstring__key">{key}</span>
            <button type="button" class="btn btn--ghost btn--sm" onclick={() => toggleRendered(key)}>
              {renderedKeys.has(key) ? 'Show raw' : 'Show rendered'}
            </button>
          </div>
          {#if renderedKeys.has(key)}
            <div class="msg__body">{@html renderMarkdown(value)}</div>
          {:else}
            <pre class="tool-json">{value}</pre>
          {/if}
        </div>
      {/each}
      <div class="modal__actions">
        <button type="button" class="btn btn--sm btn--ghost" onclick={() => (inputPopoverOpen = false)}>Close</button>
      </div>
    </div>
  </div>
{/if}
