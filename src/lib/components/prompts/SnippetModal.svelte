<script module lang="ts">
  export interface SnippetModalContext {
    /** 'span': opened from a linked span (chip / double-click) — edits the
     *  stored snippet. 'new': save-selection-as-snippet — prefilled from the
     *  selection, scoped to the active tab. */
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
   * The snippet modal (contract §Compose surface): one snippet, two tabs —
   * Content (title, body, read-only variable preview) and Metadata
   * (keywords/tags/category, deliberately demoted out of the primary view).
   * Editing here changes the STORED snippet only; spans already in the compose
   * box are point-in-time snapshots and never re-sync under the user's
   * cursor (lead ruling: provenance tint is about origin, not liveness).
   */
  import { untrack } from 'svelte';
  import type { SnippetInput, SnippetScope } from '$lib/prompts/types';
  import {
    prompts,
    activeProject,
    saveSnippet,
    deleteSnippet,
    composeReplaceSpan,
    composeLinkRange,
  } from '$lib/prompts.svelte';
  import { spanText, type SpanLink } from '$lib/compose/doc';
  import { parseVariables } from '$lib/compose/variables';

  interface Props {
    context: SnippetModalContext;
    onClose: () => void;
  }

  let { context, onClose }: Props = $props();

  // Capture the opening context once — the parent remounts this component per
  // open, so initial values are the intended semantics (untrack is the
  // idiomatic "I know" signal).
  const fromSpan = untrack(() => context.kind === 'span');
  const spanIndex = untrack(() => context.spanIndex ?? -1);
  const openSpan = untrack(() => (fromSpan ? prompts.doc.spans[spanIndex] : undefined));
  const link: SpanLink | undefined = openSpan?.link;
  const spanCurrentText = untrack(() =>
    fromSpan ? spanText(prompts.doc, spanIndex) : (context.selectionText ?? '')
  );
  /** The live stored snippet, when it still exists (it may have been deleted or
   *  hand-removed from ~/.ccdeck/prompts — the modal must survive that). */
  const snippet = $derived(link ? prompts.snippets.find((p) => p.id === link.snippetId) : undefined);
  const baseSnippet = untrack(() =>
    link ? prompts.snippets.find((p) => p.id === link.snippetId) : undefined
  );

  let tab = $state<'content' | 'metadata'>('content');

  let title = $state(untrack(() => baseSnippet?.title ?? ''));
  let body = $state(
    untrack(() =>
      context.kind === 'new' ? (context.selectionText ?? '') : (baseSnippet?.body ?? spanCurrentText)
    )
  );
  let keywordsStr = $state(untrack(() => (baseSnippet?.keywords ?? []).join(', ')));
  let tagsStr = $state(untrack(() => (baseSnippet?.tags ?? []).join(', ')));
  let category = $state(untrack(() => baseSnippet?.category ?? ''));
  let dest = $state<'global' | 'project'>(
    untrack(() => {
      if (baseSnippet) return baseSnippet.scope.kind;
      // New snippets are born scoped to the active tab (contract: the tab IS
      // the save scope).
      return prompts.activeProjectId ? 'project' : 'global';
    })
  );
  let saveError = $state<string | null>(null);
  let saving = $state(false);
  let confirmingDelete = $state(false);

  /** Read-only preview: what the body's variables parse to (names +
   *  defaults) — feedback that the grammar saw what the author meant. */
  const bodyVariables = $derived(parseVariables(body));
  /** A project destination needs an anchor: the snippet's own project (when
   *  editing one) or the active tab. */
  const destProjectId = $derived(
    baseSnippet?.scope.kind === 'project' ? baseSnippet.scope.project_id : prompts.activeProjectId
  );
  const destProjectName = $derived(
    prompts.projects.find((p) => p.id === destProjectId)?.name ?? activeProject()?.name ?? ''
  );

  function buildInput(id: string | undefined): SnippetInput {
    const scope: SnippetScope =
      dest === 'project' && destProjectId
        ? { kind: 'project', project_id: destProjectId }
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

  /** Save the snippet, then refresh the originating span's link metadata (new
   *  kind: link the saved selection). The span TEXT is never rewritten —
   *  saving updates the library, not the prompt being composed. */
  async function save(): Promise<void> {
    if (!title.trim() || !body) {
      saveError = 'A snippet needs a title and a body.';
      return;
    }
    saving = true;
    saveError = null;
    try {
      // A dangling id (snippet deleted / hand-removed) saves as new.
      const saved = await saveSnippet(buildInput(snippet?.id));
      const newLink: SpanLink = { snippetId: saved.id, title: saved.title, scope: saved.scope };
      if (fromSpan) {
        // linked = the span still shows the stored body verbatim; anything
        // else is linked-modified (origin preserved, divergence marked).
        const state = spanCurrentText === saved.body ? 'linked' : 'linked-modified';
        composeReplaceSpan(spanIndex, spanCurrentText, { state, link: newLink });
      } else if (context.kind === 'new') {
        const state = (context.selectionText ?? '') === saved.body ? 'linked' : 'linked-modified';
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
    if (!snippet) return;
    if (!confirmingDelete) {
      confirmingDelete = true;
      return;
    }
    saving = true;
    try {
      await deleteSnippet(snippet.id);
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
  aria-labelledby="snippet-modal-title"
  onkeydown={handleBackdropKeydown}
  tabindex="-1"
>
  <div class="modal snippet-modal">
    <div class="snippet-modal__head">
      <h3 id="snippet-modal-title">{title || baseSnippet?.title || 'New snippet'}</h3>
      <div class="snippet-modal__tabs" role="tablist" aria-label="Snippet sections">
        <button
          type="button"
          role="tab"
          aria-selected={tab === 'content'}
          class="snippet-modal__tab"
          class:snippet-modal__tab--active={tab === 'content'}
          onclick={() => (tab = 'content')}
        >
          Content
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={tab === 'metadata'}
          class="snippet-modal__tab"
          class:snippet-modal__tab--active={tab === 'metadata'}
          onclick={() => (tab = 'metadata')}
        >
          Metadata
        </button>
      </div>
    </div>

    {#if tab === 'content'}
      <label class="snippet-modal__field">
        <span>Title</span>
        <input
          type="text"
          bind:value={title}
          autocomplete="off"
          spellcheck="false"
          placeholder="e.g. senior-reviewer"
        />
      </label>

      <label class="snippet-modal__field snippet-modal__field--body">
        <span>Body</span>
        <textarea class="snippet-modal__body" bind:value={body} spellcheck="false"></textarea>
      </label>

      {#if bodyVariables.length}
        <div class="snippet-modal__vars" aria-label="Variables found in the body">
          <span class="snippet-modal__vars-label">Variables</span>
          {#each bodyVariables as v (v.name)}
            <!-- `{x:}` (explicit empty default — fills as "") is distinct
                 from `{x}` (no default — stays literal); the parser keeps
                 the difference on purpose, so the preview must too. -->
            <span class="snippet-modal__var" title={v.default !== undefined ? `Default: ${JSON.stringify(v.default)}` : 'No default'}>
              {v.name}{#if v.default !== undefined}<span class="snippet-modal__var-default">: {v.default === '' ? '""' : v.default}</span>{/if}
            </span>
          {/each}
        </div>
      {/if}
    {:else}
      <div class="snippet-modal__meta">
        <label class="snippet-modal__field">
          <span>Keywords</span>
          <input type="text" bind:value={keywordsStr} placeholder="comma, separated" autocomplete="off" spellcheck="false" />
        </label>
        <label class="snippet-modal__field">
          <span>Tags</span>
          <input type="text" bind:value={tagsStr} placeholder="optional" autocomplete="off" spellcheck="false" />
        </label>
        <label class="snippet-modal__field">
          <span>Category</span>
          <input type="text" bind:value={category} placeholder="optional" autocomplete="off" spellcheck="false" />
        </label>
        <label class="snippet-modal__field">
          <span>Save to</span>
          <select bind:value={dest}>
            <option value="global">Global (every project)</option>
            <option value="project" disabled={!destProjectId}>
              {destProjectId ? `Project: ${destProjectName}` : 'Project (open a project tab first)'}
            </option>
          </select>
        </label>
      </div>
    {/if}

    {#if snippet && snippet.versions.length}
      <p class="snippet-modal__versions">
        {snippet.versions.length} previous version{snippet.versions.length === 1 ? '' : 's'} kept —
        saving never destroys the old body.
      </p>
    {/if}

    {#if saveError}
      <div class="modal__warning">{saveError}</div>
    {/if}

    <div class="modal__actions snippet-modal__actions">
      {#if snippet}
        <button type="button" class="btn btn--ghost btn--sm btn--danger" disabled={saving} onclick={handleDelete}>
          {confirmingDelete ? 'Really delete?' : 'Delete snippet'}
        </button>
      {/if}
      <span class="snippet-modal__actions-spacer"></span>
      <button type="button" class="btn btn--ghost btn--sm" onclick={onClose}>Cancel</button>
      <button type="button" class="btn btn--primary btn--sm" disabled={saving} onclick={save}>
        {snippet ? 'Save' : 'Save snippet'}
      </button>
    </div>
  </div>
</div>

<style>
  .snippet-modal {
    max-width: 560px;
  }

  .snippet-modal__head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    margin-bottom: 0.6rem;
  }
  .snippet-modal__tabs {
    display: flex;
    gap: 0.25rem;
    flex-shrink: 0;
  }
  .snippet-modal__tab {
    font-family: inherit;
    font-size: 0.7rem;
    padding: 0.25rem 0.6rem;
    border: 1px solid var(--border);
    border-radius: 1rem;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
  }
  .snippet-modal__tab--active {
    background: var(--text);
    border-color: var(--text);
    color: var(--bg);
  }

  .snippet-modal__body {
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
  .snippet-modal__body:focus {
    outline: none;
    border-color: var(--accent-snippet);
  }

  /* Read-only variable preview: names + defaults as parsed — quiet feedback
     that the grammar saw what the author meant, no controls. */
  .snippet-modal__vars {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.35rem;
    margin-top: 0.5rem;
  }
  .snippet-modal__vars-label {
    font-size: 0.62rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-faint);
  }
  .snippet-modal__var {
    font-family: var(--font-mono);
    font-size: 0.66rem;
    padding: 0.12rem 0.45rem;
    border-radius: 1rem;
    background: color-mix(in srgb, var(--accent-template) 14%, transparent);
    color: color-mix(in srgb, var(--accent-template) 80%, var(--text));
  }
  .snippet-modal__var-default {
    color: var(--text-muted);
  }

  .snippet-modal__meta {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.5rem 0.75rem;
  }
  .snippet-modal__field {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    font-size: 0.68rem;
    color: var(--text-muted);
  }
  .snippet-modal__field--body {
    margin-top: 0.5rem;
  }
  .snippet-modal__field input,
  .snippet-modal__field select {
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0.3rem 0.5rem;
    border: 1px solid var(--border);
    border-radius: 0.35rem;
    background: var(--bg);
    color: var(--text);
  }

  .snippet-modal__versions {
    font-size: 0.68rem;
    color: var(--text-faint);
    margin: 0.6rem 0 0;
  }

  .snippet-modal__actions {
    align-items: center;
  }
  .snippet-modal__actions-spacer {
    flex: 1;
  }
</style>
