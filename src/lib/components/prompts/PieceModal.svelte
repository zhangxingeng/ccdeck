<script module lang="ts">
  export interface PieceModalContext {
    /** 'span': opened from a linked span (chip / double-click).
     *  'new': save-selection-as-piece — opens straight into template mode. */
    kind: 'span' | 'new';
    /** span kind only */
    spanIndex?: number;
    /** new kind only: the selected range + its text */
    selStart?: number;
    selEnd?: number;
    selectionText?: string;
  }
</script>

<script lang="ts">
  /**
   * The piece modal (F3/F4): zooms in on one piece, in two modes whose
   * background color IS the signal (issue #7's load-bearing design call) —
   * default background = instance mode, "you are editing this prompt";
   * shifted background = template mode, "you are editing the reusable
   * definition". Opened only by explicit gesture (chip / double-click), or
   * directly in template mode by save-selection-as-piece.
   */
  import { untrack } from 'svelte';
  import type { PieceInput, PieceScope } from '$lib/prompts/types';
  import {
    prompts,
    savePiece,
    deletePiece,
    composeReplaceSpan,
    composeLinkRange,
  } from '$lib/prompts.svelte';
  import { spanText, type SpanLink } from '$lib/compose/doc';
  import {
    parsePlaceholders,
    substitute,
    markPlaceholder,
    unmarkPlaceholder,
    isValidTokenName,
  } from '$lib/compose/placeholders';

  let {
    context,
    onClose,
  }: {
    context: PieceModalContext;
    onClose: () => void;
  } = $props();

  // ── capture the opening context once (the doc may change under us; the
  //    parent remounts this component per open, so initial values are the
  //    intended semantics — untrack is the idiomatic "I know" signal) ────────
  const fromSpan = untrack(() => context.kind === 'span');
  const spanIndex = untrack(() => context.spanIndex ?? -1);
  const openSpan = untrack(() =>
    fromSpan ? prompts.doc.spans[spanIndex] : undefined
  );
  const link: SpanLink | undefined = openSpan?.link;
  const spanCurrentText = untrack(() =>
    fromSpan ? spanText(prompts.doc, spanIndex) : (context.selectionText ?? '')
  );
  /** The live stored piece, when it still exists (may have been deleted or
   *  hand-removed from ~/.ccdeck/prompts — the modal must survive that). */
  const piece = $derived(link ? prompts.pieces.find((p) => p.id === link.pieceId) : undefined);

  // ── mode ───────────────────────────────────────────────────────────────────
  let mode = $state<'instance' | 'template'>(fromSpan ? 'instance' : 'template');

  // ── instance-mode state ────────────────────────────────────────────────────
  let instanceText = $state(untrack(() => spanCurrentText));
  let fills = $state<Record<string, string>>(untrack(() => ({ ...(link?.fills ?? {}) })));
  const templateTokens = link ? parsePlaceholders(link.template) : [];

  function refillFromTemplate(): void {
    if (!link) return;
    instanceText = substitute(link.template, fills);
  }

  function applyInstance(): void {
    if (!link) return;
    const newLink: SpanLink = { ...link, fills: { ...fills } };
    const state =
      instanceText === substitute(link.template, fills) ? 'linked' : 'linked-modified';
    composeReplaceSpan(spanIndex, instanceText, { state, link: newLink });
    onClose();
  }

  // ── template-mode state ────────────────────────────────────────────────────
  const basePiece = untrack(() => (link ? prompts.pieces.find((p) => p.id === link.pieceId) : undefined));
  let title = $state(untrack(() => basePiece?.title ?? ''));
  let body = $state(untrack(() => (context.kind === 'new' ? (context.selectionText ?? '') : (basePiece?.body ?? link?.template ?? ''))));
  let keywordsStr = $state(untrack(() => (basePiece?.keywords ?? []).join(', ')));
  let tagsStr = $state(untrack(() => (basePiece?.tags ?? []).join(', ')));
  let category = $state(untrack(() => basePiece?.category ?? ''));
  let dest = $state<'global' | 'project'>(
    untrack(() => {
      if (basePiece) return basePiece.scope.kind;
      return prompts.project ? 'project' : 'global';
    })
  );
  let saveError = $state<string | null>(null);
  let saving = $state(false);
  let confirmingDelete = $state(false);

  const bodyTokens = $derived(parsePlaceholders(body));
  const canUseCurrentText = $derived(fromSpan && body !== instanceText);
  const projectAvailable = $derived(prompts.project !== null);

  // Placeholder marking (template mode): select text in the body, name it.
  let bodyEl: HTMLTextAreaElement | undefined = $state(undefined);
  let marking = $state<{ start: number; end: number } | null>(null);
  let markName = $state('');

  function startMark(): void {
    if (!bodyEl) return;
    const start = bodyEl.selectionStart;
    const end = bodyEl.selectionEnd;
    if (end <= start) return; // nothing selected — the button hints via title
    marking = { start, end };
    markName = body
      .slice(start, end)
      .trim()
      .toLowerCase()
      .replace(/[^\w.-]+/g, '-')
      .replace(/^-+|-+$/g, '')
      .slice(0, 24);
  }

  function confirmMark(): void {
    if (!marking || !isValidTokenName(markName)) return;
    body = markPlaceholder(body, marking.start, marking.end, markName);
    marking = null;
    markName = '';
  }

  function buildInput(id: string | undefined): PieceInput {
    const scope: PieceScope =
      dest === 'project' && prompts.project
        ? { kind: 'project', project: prompts.project }
        : { kind: 'global' };
    const csv = (s: string) => s.split(',').map((x) => x.trim()).filter(Boolean);
    return {
      ...(id ? { id } : {}),
      title: title.trim(),
      body,
      keywords: csv(keywordsStr),
      tags: csv(tagsStr),
      category: category.trim() || null,
      scope,
    };
  }

  /** Save the template — overwriting the existing piece or forking a new one
   *  — then point the originating span/selection at the result. */
  async function saveTemplate(fork: boolean): Promise<void> {
    if (!title.trim() || !body) {
      saveError = 'A piece needs a title and a body.';
      return;
    }
    saving = true;
    saveError = null;
    try {
      // A dangling id (piece deleted / hand-removed) saves as new.
      const id = !fork && piece ? piece.id : undefined;
      const saved = await savePiece(buildInput(id));
      const newLink: SpanLink = {
        pieceId: saved.id,
        title: saved.title,
        scope: saved.scope,
        template: saved.body,
        fills: { ...fills },
      };
      if (fromSpan) {
        // The box keeps showing the span's current text — saving the
        // template never silently rewrites the prompt you composed.
        const state =
          spanCurrentText === substitute(saved.body, fills) ? 'linked' : 'linked-modified';
        composeReplaceSpan(spanIndex, spanCurrentText, { state, link: newLink });
      } else if (context.kind === 'new') {
        const state =
          (context.selectionText ?? '') === saved.body ? 'linked' : 'linked-modified';
        composeLinkRange(context.selStart ?? 0, context.selEnd ?? 0, newLink, state);
      }
      onClose();
    } catch (e) {
      saveError = e instanceof Error ? e.message : String(e);
    } finally {
      saving = false;
    }
  }

  async function handleDelete(): Promise<void> {
    if (!piece) return;
    if (!confirmingDelete) {
      confirmingDelete = true;
      return;
    }
    saving = true;
    try {
      await deletePiece(piece.id);
      onClose();
    } catch (e) {
      saveError = e instanceof Error ? e.message : String(e);
      saving = false;
    }
  }

  function handleBackdropKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }
</script>

<div
  class="modal-backdrop"
  role="dialog"
  aria-modal="true"
  aria-labelledby="piece-modal-title"
  onkeydown={handleBackdropKeydown}
  tabindex="-1"
>
  <div class="modal piece-modal" class:piece-modal--template={mode === 'template'}>
    <div class="piece-modal__head">
      <h3 id="piece-modal-title">
        {#if mode === 'instance'}
          {link?.title ?? 'Piece'} <span class="piece-modal__mode-note">— editing this prompt</span>
        {:else}
          {title || link?.title || 'New piece'}
          <span class="piece-modal__mode-note">— editing the template</span>
        {/if}
      </h3>
      {#if fromSpan}
        <div class="piece-modal__tabs" role="tablist" aria-label="Edit scope">
          <button
            type="button"
            role="tab"
            aria-selected={mode === 'instance'}
            class="piece-modal__tab"
            class:piece-modal__tab--active={mode === 'instance'}
            onclick={() => (mode = 'instance')}
          >
            This prompt
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={mode === 'template'}
            class="piece-modal__tab"
            class:piece-modal__tab--active={mode === 'template'}
            onclick={() => (mode = 'template')}
          >
            Template
          </button>
        </div>
      {/if}
    </div>

    {#if mode === 'instance' && link}
      <p class="piece-modal__hint">
        Changes here apply only to this prompt — the library piece is untouched.
      </p>
      <textarea
        class="piece-modal__body"
        bind:value={instanceText}
        spellcheck="false"
        aria-label="Span text in this prompt"
      ></textarea>

      {#if templateTokens.length}
        <div class="piece-modal__fills">
          {#each templateTokens as t (t)}
            <label class="piece-modal__fill">
              <span class="piece-modal__fill-name">{t}</span>
              <input type="text" bind:value={fills[t]} autocomplete="off" spellcheck="false" />
            </label>
          {/each}
          <button
            type="button"
            class="btn btn--sm"
            onclick={refillFromTemplate}
            title="Regenerate the text above from the template with these values (overwrites manual edits)"
          >
            Re-fill from template
          </button>
        </div>
      {/if}

      <div class="modal__actions">
        <button type="button" class="btn btn--ghost btn--sm" onclick={onClose}>Cancel</button>
        <button type="button" class="btn btn--primary btn--sm" onclick={applyInstance}>
          Apply to this prompt
        </button>
      </div>
    {:else}
      <p class="piece-modal__hint piece-modal__hint--template">
        You're editing the reusable template — saving updates the library for every future prompt.
      </p>

      <label class="piece-modal__field">
        <span>Title</span>
        <input type="text" bind:value={title} autocomplete="off" spellcheck="false" placeholder="e.g. senior-reviewer" />
      </label>

      <div class="piece-modal__bodywrap">
        <textarea
          class="piece-modal__body"
          bind:this={bodyEl}
          bind:value={body}
          spellcheck="false"
          aria-label="Template body"
        ></textarea>
        <div class="piece-modal__ph-row">
          {#if marking}
            <input
              type="text"
              class="piece-modal__ph-name"
              bind:value={markName}
              placeholder="placeholder name"
              autocomplete="off"
              spellcheck="false"
              onkeydown={(e) => e.key === 'Enter' && confirmMark()}
            />
            <button type="button" class="btn btn--sm" disabled={!isValidTokenName(markName)} onclick={confirmMark}>
              Mark
            </button>
            <button type="button" class="btn btn--ghost btn--sm" onclick={() => (marking = null)}>✕</button>
          {:else}
            <button
              type="button"
              class="btn btn--ghost btn--sm"
              onclick={startMark}
              title="Select text in the body first, then click to turn it into a {'{{placeholder}}'}"
            >
              Mark selection as placeholder
            </button>
          {/if}
          {#each bodyTokens as t (t)}
            <span class="piece-modal__token">
              {'{{'}{t}{'}}'}
              <button
                type="button"
                class="piece-modal__token-x"
                title="Unmark — replace {'{{'}{t}{'}}'} with plain text"
                onclick={() => (body = unmarkPlaceholder(body, t))}
              >✕</button>
            </span>
          {/each}
        </div>
        {#if canUseCurrentText}
          <button
            type="button"
            class="btn btn--sm piece-modal__use-current"
            onclick={() => (body = instanceText)}
            title="Straight replace: the template body becomes exactly this span's current text (versioned like any save)"
          >
            Use current text as template body
          </button>
        {/if}
      </div>

      <div class="piece-modal__meta">
        <label class="piece-modal__field">
          <span>Keywords</span>
          <input type="text" bind:value={keywordsStr} placeholder="comma, separated" autocomplete="off" spellcheck="false" />
        </label>
        <label class="piece-modal__field">
          <span>Tags</span>
          <input type="text" bind:value={tagsStr} placeholder="optional" autocomplete="off" spellcheck="false" />
        </label>
        <label class="piece-modal__field">
          <span>Category</span>
          <input type="text" bind:value={category} placeholder="optional" autocomplete="off" spellcheck="false" />
        </label>
        <label class="piece-modal__field">
          <span>Save to</span>
          <select bind:value={dest}>
            <option value="global">Global (every project)</option>
            <option value="project" disabled={!projectAvailable}>
              This project{projectAvailable ? '' : ' (pick a project first)'}
            </option>
          </select>
        </label>
      </div>

      {#if piece && piece.versions.length}
        <p class="piece-modal__versions">
          {piece.versions.length} previous version{piece.versions.length === 1 ? '' : 's'} kept —
          saving never destroys the old body.
        </p>
      {/if}

      {#if saveError}
        <div class="modal__warning">{saveError}</div>
      {/if}

      <div class="modal__actions piece-modal__actions">
        {#if piece}
          <button type="button" class="btn btn--ghost btn--sm btn--danger" disabled={saving} onclick={handleDelete}>
            {confirmingDelete ? 'Really delete?' : 'Delete piece'}
          </button>
        {/if}
        <span class="piece-modal__actions-spacer"></span>
        <button type="button" class="btn btn--ghost btn--sm" onclick={onClose}>Cancel</button>
        {#if piece}
          <button type="button" class="btn btn--sm" disabled={saving} onclick={() => saveTemplate(true)}>
            Save as new piece
          </button>
        {/if}
        <button type="button" class="btn btn--primary btn--sm" disabled={saving} onclick={() => saveTemplate(false)}>
          {piece ? 'Save' : 'Save piece'}
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .piece-modal {
    max-width: 560px;
    /* Instance mode wears the default modal background; template mode shifts
       it — the color flip is the whole "which thing am I editing" signal. */
    transition: background 0.15s ease;
  }
  .piece-modal--template {
    background: color-mix(in srgb, var(--accent-template) 9%, var(--bg-card));
    border-color: color-mix(in srgb, var(--accent-template) 35%, var(--border));
  }

  .piece-modal__head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
  }
  .piece-modal__mode-note {
    font-size: 0.72rem;
    font-weight: 400;
    color: var(--text-muted);
  }
  .piece-modal__tabs {
    display: flex;
    gap: 0.25rem;
    flex-shrink: 0;
  }
  .piece-modal__tab {
    font-family: inherit;
    font-size: 0.7rem;
    padding: 0.25rem 0.6rem;
    border: 1px solid var(--border);
    border-radius: 1rem;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
  }
  .piece-modal__tab--active {
    background: var(--text);
    border-color: var(--text);
    color: var(--bg);
  }

  .piece-modal__hint {
    font-size: 0.72rem;
    color: var(--text-muted);
    margin: 0.4rem 0 0.6rem;
  }
  .piece-modal__hint--template {
    color: color-mix(in srgb, var(--accent-template) 75%, var(--text));
  }

  .piece-modal__body {
    width: 100%;
    min-height: 8rem;
    font-family: var(--font-mono);
    font-size: 0.78rem;
    line-height: 1.5;
    padding: 0.6rem 0.7rem;
    border: 1px solid var(--border);
    border-radius: 0.4rem;
    background: var(--bg);
    color: var(--text);
    resize: vertical;
    box-sizing: border-box;
  }
  .piece-modal__body:focus {
    outline: none;
    border-color: var(--accent-piece);
  }

  .piece-modal__fills {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    margin-top: 0.6rem;
  }
  .piece-modal__fill {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .piece-modal__fill-name {
    font-family: var(--font-mono);
    font-size: 0.72rem;
    color: var(--accent-template);
    min-width: 5.5rem;
    text-align: right;
  }
  .piece-modal__fill input {
    flex: 1;
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0.3rem 0.5rem;
    border: 1px solid var(--border);
    border-radius: 0.35rem;
    background: var(--bg);
    color: var(--text);
  }

  .piece-modal__bodywrap {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    margin-top: 0.4rem;
  }
  .piece-modal__ph-row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.35rem;
  }
  .piece-modal__ph-name {
    font-family: var(--font-mono);
    font-size: 0.72rem;
    padding: 0.25rem 0.5rem;
    border: 1px solid var(--border);
    border-radius: 0.35rem;
    background: var(--bg);
    color: var(--text);
    width: 11rem;
  }
  .piece-modal__token {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    font-family: var(--font-mono);
    font-size: 0.66rem;
    padding: 0.12rem 0.45rem;
    border-radius: 1rem;
    background: color-mix(in srgb, var(--accent-template) 14%, transparent);
    color: color-mix(in srgb, var(--accent-template) 80%, var(--text));
  }
  .piece-modal__token-x {
    border: 0;
    background: none;
    padding: 0;
    font-size: 0.6rem;
    cursor: pointer;
    color: inherit;
    opacity: 0.7;
  }
  .piece-modal__token-x:hover { opacity: 1; }
  .piece-modal__use-current { align-self: flex-start; }

  .piece-modal__meta {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.5rem 0.75rem;
    margin-top: 0.7rem;
  }
  .piece-modal__field {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    font-size: 0.68rem;
    color: var(--text-muted);
  }
  .piece-modal__field input,
  .piece-modal__field select {
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0.3rem 0.5rem;
    border: 1px solid var(--border);
    border-radius: 0.35rem;
    background: var(--bg);
    color: var(--text);
  }

  .piece-modal__versions {
    font-size: 0.68rem;
    color: var(--text-faint);
    margin: 0.6rem 0 0;
  }

  .piece-modal__actions { align-items: center; }
  .piece-modal__actions-spacer { flex: 1; }
</style>
