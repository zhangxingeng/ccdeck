<script lang="ts">
  /**
   * The compose surface — overlay technique: a transparent-background
   * <textarea> (the input device; it renders the text and the caret) sits on
   * top of a pixel-matched highlight <div> that paints provenance tints
   * behind the text. The store's Doc is the single source of truth; every
   * input event is translated into one applyEdit via minimal diff, so the
   * provenance state machine runs in pure, tested code — never inferred from
   * DOM mutations.
   *
   * Interaction guardrail (lead ruling on F1 vs F3): a plain click only sets
   * the caret — inline editing is primary and must never pay a modal tax.
   * The piece modal opens only through explicit gestures: the chip that
   * appears above the box while the caret is inside a linked span, or
   * double-clicking the span.
   */
  import { onMount } from 'svelte';
  import { prompts, composeEdit, setSelection } from '$lib/prompts.svelte';
  import { linkedSpanAt, spanStarts, diffTexts } from '$lib/compose/doc';

  let {
    onOpenSpan,
  }: {
    /** Explicit open-the-piece-modal gesture (chip click / span double-click). */
    onOpenSpan: (spanIndex: number) => void;
  } = $props();

  let textareaEl: HTMLTextAreaElement | undefined = $state(undefined);
  let highlightEl: HTMLDivElement | undefined = $state(undefined);

  /** Highlight-layer render list: span state + its slice of the text. */
  const renderSpans = $derived.by(() => {
    const starts = spanStarts(prompts.doc);
    return prompts.doc.spans.map((s, i) => ({
      state: s.state,
      title: s.link ? `${s.link.title} · ${s.link.scope.kind}` : '',
      text: prompts.doc.text.slice(starts[i], starts[i] + s.length),
    }));
  });

  /** The linked span the caret sits in — drives the edit-affordance chip. */
  const caretSpan = $derived(linkedSpanAt(prompts.doc, prompts.caret));

  function handleInput(): void {
    if (!textareaEl) return;
    const d = diffTexts(prompts.doc.text, textareaEl.value);
    if (d) composeEdit(d.start, d.end, d.inserted);
    // The browser's caret is authoritative after an input event.
    setSelection(textareaEl.selectionStart, textareaEl.selectionEnd);
  }

  function syncScroll(): void {
    if (!textareaEl || !highlightEl) return;
    highlightEl.scrollTop = textareaEl.scrollTop;
    highlightEl.scrollLeft = textareaEl.scrollLeft;
  }

  function handleDblclick(): void {
    if (!textareaEl) return;
    // A double-click selects a word; its midpoint locates the span.
    const mid = Math.floor((textareaEl.selectionStart + textareaEl.selectionEnd) / 2);
    const ref = linkedSpanAt(prompts.doc, mid);
    if (ref) onOpenSpan(ref.index);
  }

  // Track caret/selection moves (arrows, clicks, shift-selects). The
  // document-level selectionchange event covers textareas in all modern
  // browsers; filtering on activeElement keeps it cheap.
  onMount(() => {
    function onSelectionChange(): void {
      if (!textareaEl || document.activeElement !== textareaEl) return;
      setSelection(textareaEl.selectionStart, textareaEl.selectionEnd);
    }
    document.addEventListener('selectionchange', onSelectionChange);
    return () => document.removeEventListener('selectionchange', onSelectionChange);
  });

  // Restore focus + caret after the doc changed from outside the textarea
  // (match-panel insert, modal apply). Runs on mount too — focusing the
  // empty box on view entry is the behavior we want anyway.
  $effect(() => {
    void prompts.focusNonce;
    if (!textareaEl) return;
    textareaEl.focus();
    textareaEl.setSelectionRange(prompts.caret, prompts.caret);
    syncScroll();
  });
</script>

<div class="compose">
  <div class="compose__chip-row">
    {#if caretSpan?.span.link}
      {@const link = caretSpan.span.link}
      <button
        type="button"
        class="compose__chip compose__chip--{caretSpan.span.state}"
        onclick={() => onOpenSpan(caretSpan.index)}
        title="Open this piece (Instance / Template modes)"
      >
        <span class="compose__chip-dot" aria-hidden="true"></span>
        {link.title}
        <span class="compose__chip-scope">{link.scope.kind === 'global' ? 'global' : 'project'}</span>
        {#if caretSpan.span.state === 'linked-modified'}<span class="compose__chip-mod">edited</span>{/if}
        <span class="compose__chip-cta">Edit piece…</span>
      </button>
    {:else}
      <span class="compose__chip-hint">
        Type freely — click a match to insert it at the cursor. Double-click a tinted span (or place
        the caret in it) to open its piece.
      </span>
    {/if}
  </div>

  <div class="compose__stack">
    <div class="compose__highlight" bind:this={highlightEl} aria-hidden="true">
      <!-- Formatting inside this block is load-bearing: any whitespace between
           tags would desync the two layers' text metrics. -->
      {#each renderSpans as s}{#if s.state === 'typed'}{s.text}{:else}<span
          class="compose__span compose__span--{s.state}"
          title={s.title}>{s.text}</span>{/if}{/each}{'​'}
    </div>
    <textarea
      bind:this={textareaEl}
      class="compose__input"
      value={prompts.doc.text}
      oninput={handleInput}
      onscroll={syncScroll}
      ondblclick={handleDblclick}
      spellcheck="false"
      placeholder="Compose your prompt…"
      aria-label="Prompt compose box"
    ></textarea>
  </div>
</div>

<style>
  .compose {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    flex: 1;
    min-width: 0;
  }

  .compose__chip-row {
    min-height: 1.6rem;
    display: flex;
    align-items: center;
  }
  .compose__chip {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    font-family: inherit;
    font-size: 0.72rem;
    padding: 0.2rem 0.6rem;
    border-radius: 1rem;
    border: 1px solid color-mix(in srgb, var(--accent-piece) 45%, var(--border));
    background: color-mix(in srgb, var(--accent-piece) 10%, transparent);
    color: var(--text);
    cursor: pointer;
  }
  .compose__chip:hover {
    background: color-mix(in srgb, var(--accent-piece) 18%, transparent);
  }
  .compose__chip-dot {
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 50%;
    background: var(--accent-piece);
  }
  .compose__chip--linked-modified .compose__chip-dot {
    outline: 2px dotted var(--accent-piece);
    outline-offset: 1px;
    background: transparent;
  }
  .compose__chip-scope,
  .compose__chip-mod {
    font-size: 0.62rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
  }
  .compose__chip-cta {
    font-weight: 600;
    color: var(--accent-piece);
  }
  .compose__chip-hint {
    font-size: 0.7rem;
    color: var(--text-faint);
  }

  /* ── the two pixel-matched layers ──────────────────────────────────────── */
  .compose__stack {
    position: relative;
    flex: 1;
    min-height: 16rem;
  }
  /* Every text-metric property is declared identically on both layers — a
     single divergence (font, padding, border width, line-height, wrapping)
     desyncs tint from text. */
  .compose__highlight,
  .compose__input {
    font-family: var(--font-mono);
    font-size: 0.82rem;
    line-height: 1.55;
    letter-spacing: normal;
    white-space: pre-wrap;
    overflow-wrap: break-word;
    word-break: normal;
    padding: 0.8rem 0.9rem;
    border: 1px solid transparent;
    border-radius: 0.5rem;
    margin: 0;
    box-sizing: border-box;
    overflow-y: auto;
  }
  .compose__highlight {
    position: absolute;
    inset: 0;
    background: var(--bg-card);
    color: transparent;
    border-color: var(--border);
    pointer-events: none;
    z-index: 0;
  }
  .compose__input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    resize: none;
    background: transparent;
    color: var(--text);
    caret-color: var(--text);
    border-color: var(--border);
    z-index: 1;
  }
  .compose__input:focus {
    outline: none;
    border-color: var(--accent-piece);
  }

  /* Provenance tints paint on the back layer, behind the real text. The
     linked-modified marker is a dotted underline drawn as a border — the
     design's "tint + subtle marker" (same hue family; the marker carries
     the difference, per issue #7 F1). */
  .compose__span--linked {
    background: color-mix(in srgb, var(--accent-piece) 16%, transparent);
    border-radius: 2px;
  }
  .compose__span--linked-modified {
    background: color-mix(in srgb, var(--accent-piece) 16%, transparent);
    border-bottom: 2px dotted var(--accent-piece);
    border-radius: 2px;
  }
</style>
