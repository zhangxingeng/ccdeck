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
   * The snippet modal opens only through explicit gestures: the chip that
   * appears above the box while the caret is inside a linked span, or
   * double-clicking the span.
   */
  import { onMount, untrack } from 'svelte';
  import { prompts, composeEdit, setSelection } from '$lib/prompts.svelte';
  import { linkedSpanAt, spanStarts, diffTexts } from '$lib/compose/doc';
  import type { SnippetScope } from '$lib/prompts/types';
  import { projectColorVar } from '$lib/prompts/palette';
  import VariableFillList from './VariableFillList.svelte';
  import SaveAsControl from './SaveAsControl.svelte';

  interface Props {
    /** Explicit open-the-snippet-modal gesture (chip click / span double-click). */
    onOpenSpan: (spanIndex: number) => void;
    /** Copy Prompt — the parent owns the clipboard call + toast. */
    onCopy: () => void;
    /** Save (selection-aware, in the parent) to `scope` — opens the snippet
     *  modal prefilled with the selection if there is one, else the whole box. */
    onSaveAs: (scope: SnippetScope) => void;
    /** ↓ at the very end of the text steps into the match panel; returns
     *  whether the panel had a hit to land on, so the box keeps ↓ as a caret
     *  move when the panel is empty (contract §S2). */
    onStepIntoPanel: () => boolean;
  }

  let { onOpenSpan, onCopy, onSaveAs, onStepIntoPanel }: Props = $props();

  let textareaEl: HTMLTextAreaElement | undefined = $state(undefined);
  let highlightEl: HTMLDivElement | undefined = $state(undefined);
  let stackEl: HTMLDivElement | undefined = $state(undefined);
  /** Bumped on scroll so the selection-anchored button re-measures. */
  let scrollNonce = $state(0);

  const hasText = $derived(prompts.doc.text.length > 0);

  /** Highlight-layer render list: span state, its slice of the text, and
   *  its hue — greyish for global snippets, the OWNING project's color for
   *  project snippets (a span keeps its own hue even under another tab). */
  const renderSpans = $derived.by(() => {
    const starts = spanStarts(prompts.doc);
    return prompts.doc.spans.map((s, i) => {
      const scope = s.link?.scope;
      const project =
        scope?.kind === 'project'
          ? prompts.projects.find((p) => p.id === scope.project_id)
          : undefined;
      return {
        state: s.state,
        // A roster miss (project deleted since insert) falls back to the
        // greyish global treatment rather than an unstyled hole.
        colorVar: project ? projectColorVar(project.color) : null,
        title: s.link ? `${s.link.title} · ${project ? project.name : 'global'}` : '',
        text: prompts.doc.text.slice(starts[i], starts[i] + s.length),
      };
    });
  });

  /** The linked span the caret sits in — drives the edit-affordance chip. */
  const caretSpan = $derived(linkedSpanAt(prompts.doc, prompts.caret));
  /** Live scope label for the chip — the same roster-backed derivation as
   *  the span tint, NOT the scope snapshotted into the link at insert: when
   *  the owning project is deleted the tint falls back to grey, and the
   *  label must fall back with it. */
  const caretScopeLabel = $derived.by(() => {
    const scope = caretSpan?.span.link?.scope;
    if (scope?.kind !== 'project') return 'global';
    return prompts.projects.some((p) => p.id === scope.project_id) ? 'project' : 'global';
  });

  function handleInput(): void {
    if (!textareaEl) return;
    const d = diffTexts(prompts.doc.text, textareaEl.value);
    if (d) composeEdit(d.start, d.end, d.inserted);
    // The browser's caret is authoritative after an input event.
    setSelection(textareaEl.selectionStart, textareaEl.selectionEnd);
  }

  function handleTextareaKeydown(e: KeyboardEvent): void {
    if (e.key !== 'ArrowDown' || !textareaEl) return;
    // ↓ is natively inert only when the caret sits at the very end of the text —
    // the one position where repurposing it to step into the match panel can't
    // steal a caret move (contract §S2). Anywhere else, ↓ moves the caret as a
    // user editing mid-document expects. onStepIntoPanel returns false when the
    // panel is empty, so ↓ then falls through to its default no-op.
    const atEnd =
      textareaEl.selectionStart === textareaEl.selectionEnd &&
      textareaEl.selectionEnd === textareaEl.value.length;
    if (atEnd && onStepIntoPanel()) e.preventDefault();
  }

  function syncScroll(): void {
    if (!textareaEl || !highlightEl) return;
    highlightEl.scrollTop = textareaEl.scrollTop;
    highlightEl.scrollLeft = textareaEl.scrollLeft;
    // Untracked: ++ is a read-modify-write, and syncScroll runs inside the
    // focus-restore $effect — a tracked read here would make that effect
    // depend on state it writes (effect_update_depth_exceeded).
    untrack(() => scrollNonce++);
  }

  // ── the floating Save-as-snippet affordance ──────────────────────────────────
  // The pixel-matched mirror doubles as a measuring surface: a collapsed
  // Range at the selection-end offset gives the caret rectangle the raw
  // <textarea> cannot expose.
  function rectAtOffset(container: HTMLElement, offset: number): DOMRect | null {
    const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
    let remaining = offset;
    let node: Node | null;
    while ((node = walker.nextNode())) {
      const text = node as Text;
      if (remaining <= text.data.length) {
        const range = document.createRange();
        range.setStart(text, remaining);
        range.collapse(true);
        const rects = range.getClientRects();
        return rects.length ? rects[0] : range.getBoundingClientRect();
      }
      remaining -= text.data.length;
    }
    return null;
  }

  /** Where the floating button sits (stack-relative), or null when there is
   *  no selection to anchor to. DOM measurement — $effect's legitimate job. */
  let savePos = $state<{ left: number; top: number } | null>(null);
  $effect(() => {
    const { selStart, selEnd } = prompts;
    void prompts.doc.text; // re-measure when the text reflows
    void scrollNonce; // …and when the box scrolls under the selection
    if (selEnd <= selStart || !highlightEl || !stackEl) {
      savePos = null;
      return;
    }
    const rect = rectAtOffset(highlightEl, selEnd);
    if (!rect) {
      savePos = null;
      return;
    }
    const stack = stackEl.getBoundingClientRect();
    // Float just past the selection end; clamp inside the box so a
    // selection near an edge (or scrolled half out) still shows a button.
    const left = Math.min(Math.max(rect.left - stack.left + 8, 8), stack.width - 130);
    const top = Math.min(Math.max(rect.top - stack.top - 34, 6), stack.height - 38);
    savePos = { left, top };
  });

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
  // empty box on view entry is the behavior we want anyway. The caret read
  // is untracked: depending on it would re-run this on every selection
  // change and collapse an in-progress mouse selection.
  $effect(() => {
    void prompts.focusNonce;
    if (!textareaEl) return;
    const caret = untrack(() => prompts.caret);
    textareaEl.focus();
    textareaEl.setSelectionRange(caret, caret);
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
        title="Open this snippet (Content / Metadata)"
      >
        <span class="compose__chip-dot" aria-hidden="true"></span>
        {link.title}
        <span class="compose__chip-scope">{caretScopeLabel}</span>
        {#if caretSpan.span.state === 'linked-modified'}<span class="compose__chip-mod">edited</span>{/if}
        <span class="compose__chip-cta">Edit snippet…</span>
      </button>
    {:else}
      <span class="compose__chip-hint">
        Type freely — click a match to insert it at the cursor. Double-click a tinted span (or place
        the caret in it) to open its snippet.
      </span>
    {/if}
  </div>

  <div class="compose__stack" bind:this={stackEl}>
    <div class="compose__highlight" bind:this={highlightEl} aria-hidden="true">
      <!-- Formatting inside this block is load-bearing: any whitespace between
           tags would desync the two layers' text metrics. -->
      {#each renderSpans as s}{#if s.state === 'typed'}{s.text}{:else}<span
          class="compose__span compose__span--{s.state}"
          class:compose__span--project={s.colorVar !== null}
          style={s.colorVar ? `--span-color: ${s.colorVar}` : null}
          title={s.title}>{s.text}</span>{/if}{/each}{'​'}<!-- zero-width space: makes a trailing newline render a line, matching the textarea -->
    </div>
    <textarea
      bind:this={textareaEl}
      class="compose__input"
      value={prompts.doc.text}
      oninput={handleInput}
      onkeydown={handleTextareaKeydown}
      onscroll={syncScroll}
      ondblclick={handleDblclick}
      spellcheck="false"
      placeholder="Compose your prompt…"
      aria-label="Prompt compose box"
    ></textarea>

    {#if savePos}
      <!-- Floating fast-path next to a selection; SaveAsControl swallows
           mousedown so the click doesn't collapse the selection it acts on. -->
      <div class="compose__save-sel" style="left: {savePos.left}px; top: {savePos.top}px">
        <SaveAsControl variant="floating" label="Save as snippet" onSave={onSaveAs} />
      </div>
    {/if}

    <!-- Always present (contract §S5): "save what I just wrote" needs no
         selection. Selection-aware in the parent — the selection if there is
         one, else the whole box. -->
    <div class="compose__save-as">
      <SaveAsControl label="Save as…" onSave={onSaveAs} />
    </div>

    {#if hasText}
      <div class="compose__copy">
        <button type="button" class="btn btn--primary btn--sm" onclick={onCopy}>
          Copy prompt
        </button>
      </div>
    {/if}
  </div>

  <VariableFillList />
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
    border: 1px solid color-mix(in srgb, var(--accent-snippet) 45%, var(--border));
    background: color-mix(in srgb, var(--accent-snippet) 10%, transparent);
    color: var(--text);
    cursor: pointer;
  }
  .compose__chip:hover {
    background: color-mix(in srgb, var(--accent-snippet) 18%, transparent);
  }
  .compose__chip-dot {
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 50%;
    background: var(--accent-snippet);
  }
  .compose__chip--linked-modified .compose__chip-dot {
    outline: 2px dotted var(--accent-snippet);
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
    color: var(--accent-snippet);
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
    /* The contract's contained tint: a faint hint of the active project's
       color, mixed over the card surface — Global (no --project-color)
       resolves to the plain card. The tint lives HERE and nowhere else. */
    background: color-mix(in srgb, var(--project-color, var(--bg-card)) 5%, var(--bg-card));
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
    border-color: color-mix(in srgb, var(--project-color, var(--accent-snippet)) 55%, var(--border));
  }
  /* Selection reads as a highlighter stroke: bright marker, dark ink — the
     pair is scoped to the compose surface only. */
  .compose__input::selection {
    background: var(--highlight);
    color: var(--highlight-foreground);
  }

  /* Provenance tints paint on the back layer, behind the real text:
     greyish translucent for global snippets, a darker translucent mix of the
     OWNING project's hue for project snippets (--span-color set per span).
     The linked-modified marker is a dotted underline drawn as a border —
     "tint + subtle marker", same hue family (issue #7 F1). */
  .compose__span--linked,
  .compose__span--linked-modified {
    background: color-mix(in srgb, var(--span-color, var(--text-faint)) 15%, transparent);
    border-radius: 2px;
  }
  .compose__span--project.compose__span--linked,
  .compose__span--project.compose__span--linked-modified {
    background: color-mix(in srgb, var(--span-color) 24%, transparent);
  }
  .compose__span--linked-modified {
    border-bottom: 2px dotted color-mix(in srgb, var(--span-color, var(--text-faint)) 75%, transparent);
  }

  /* ── situational affordances ───────────────────────────────────────────── */
  /* Floating save-as, anchored next to a live selection (fast mouse path). */
  .compose__save-sel {
    position: absolute;
    z-index: 3;
  }
  /* Always-present save-as, bottom-left (contract §S5). */
  .compose__save-as {
    position: absolute;
    left: 0.75rem;
    bottom: 0.75rem;
    z-index: 2;
  }
  .compose__copy {
    position: absolute;
    right: 0.75rem;
    bottom: 0.75rem;
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }
</style>
