<script module lang="ts">
  /** How the popup was opened. From a chip (`cid` present) it edits that chip's
   *  snippet; from the save-as action it creates one out of typed text. Those are
   *  its only two entrances — and the only two ways a snippet body is ever edited. */
  export interface SnippetModalContext {
    /** The chip this was opened from; absent when creating from typed text. */
    cid?: string;
    /** Prefilled name — a chip's snippet name, or empty when creating. */
    name?: string;
    /** Prefilled body — the chip's content, or the typed text being saved. */
    content: string;
    /** Whether the chip has a session-only edit that diverged it from its saved
     *  file. Only meaningful when `cid` is present — a chip that doesn't exist
     *  yet cannot have diverged from anything. Absent/false = in sync. */
    dirty?: boolean;
  }
</script>

<script lang="ts">
  /**
   * The snippet popup: the ONE and only place a snippet body is edited.
   *
   * The defect it exists to kill was having two. A user could edit an inserted
   * snippet inline in the compose box — which silently diverged it from the stored
   * file and wrote nothing to disk — or open a modal that actually persisted. Two
   * places to edit one thing, different consequences, no signal which was which.
   * The founder's complaint, exactly: "editing one thing should be only one place."
   *
   * So chips are not editable in the box, and it all happens here, behind actions
   * split into two groups by blast radius (round 2, plan §5) plus a neutral Cancel:
   *
   *   LEFT — touches the library, writes a file to disk:
   *   Update    Writes <name>.md. The same name updates; a NEW name creates a new
   *             snippet. That IS the whole "Save as new" mechanism — the filename
   *             is the identity, so a second button could only add ambiguity.
   *   Delete    Removes the file. The chip becomes plain typed text — deleting a
   *             library entry must not gut the prompt you are halfway through.
   *             Only shown editing a chip whose file already exists.
   *
   *   RIGHT — touches only this prompt, session-only, nothing written to disk:
   *   Save      Applies the edit to THIS chip in THIS prompt (round 1's `Use once`,
   *             renamed — from the user's seat they ARE just saving their edit).
   *             This is what makes "never editable in place" a simplification
   *             rather than a cage: tweak a prompt for one use without polluting
   *             the library with a near-duplicate. It marks the chip `dirty`
   *             (diverged from its saved file) — ABSENT, not disabled, when there
   *             is no chip yet to apply it to (opened from the library's `+`).
   *
   * The right/left split is the whole point: right-hand buttons affect the prompt
   * you are composing, left-hand buttons affect the library. That distinction was
   * invisible when all three sat in one row.
   *
   * The variable fills below are the SAME global cells as the list under the compose
   * box; this popup just shows the subset THIS body uses. That is not a second place
   * to edit: a variable is one global value by name, and showing one cell in two
   * views is convenience. The one-place rule is about snippet BODIES, where two
   * surfaces meant two divergent sources of truth.
   */
  import { untrack } from 'svelte';
  import {
    prompts,
    saveSnippet,
    deleteSnippet,
    setFill,
    composeUseOnce,
    composeSaveChip,
    composeDissolveChip,
  } from '$lib/prompts.svelte';
  import { parseVariables, UNSET_VALUE } from '$lib/compose/variables';
  import { focusTrap } from '$lib/attachments/focusTrap';

  interface Props {
    context: SnippetModalContext;
    /** The active project's folder — a snippet is a file inside it. */
    project: string;
    onClose: () => void;
  }

  let { context, project, onClose }: Props = $props();

  // The parent remounts this per open, so the opening context IS the intended
  // initial state (untrack is the idiomatic "I know" signal).
  const cid = untrack(() => context.cid);
  const fromChip = cid !== undefined;
  const originalName = untrack(() => context.name ?? '');
  /** Whether the chip we opened on has a session-only edit not yet written to its
   *  file. Read once at open, same as the rest of the opening context — this popup
   *  doesn't need to react to it changing mid-edit. */
  const dirty = untrack(() => context.dirty ?? false);

  let name = $state(untrack(() => context.name ?? ''));
  let content = $state(untrack(() => context.content));
  let error = $state<string | null>(null);
  let busy = $state(false);
  let confirmingDelete = $state(false);
  let nameEl: HTMLInputElement | undefined = $state(undefined);
  let contentEl: HTMLTextAreaElement | undefined = $state(undefined);

  /** The variables THIS body uses. Derived as you type, so a variable you add shows
   *  up the moment it parses — which is also the feedback that tells you the grammar
   *  saw what you meant. */
  const variables = $derived(parseVariables(content));

  /** A changed name means a new file. Saying so up front is what dissolves the old
   *  "update, or save as new?" question — rather than answering it with a button. */
  const creatingNew = $derived(
    fromChip && name.trim() !== '' && name.trim() !== originalName
  );

  /** Left group. Writes the file to disk — the same button as round 1's `Save`,
   *  renamed to say what it does now that a peer `Save` means something else. */
  async function update(): Promise<void> {
    const trimmed = name.trim();
    if (!trimmed) {
      error = 'A snippet needs a name.';
      return;
    }
    busy = true;
    error = null;
    try {
      const saved = await saveSnippet(project, trimmed, content);
      // The chip now reflects the snippet it actually is: the same one (updated), or
      // the new one it was just saved as. Writing the file is what resolves the
      // divergence a session-only edit left behind, so the chip is clean again.
      if (cid) composeSaveChip(cid, saved.name, saved.content);
      onClose();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      busy = false;
    }
  }

  /** Right group. This chip, this prompt, nothing touches the library — round 1's
   *  `Use once`, renamed (the founder's naming: from the user's seat they ARE just
   *  saving their edit). Only reachable when `fromChip`, so `cid` is always set. */
  function save(): void {
    if (!cid) return;
    composeUseOnce(cid, content);
    onClose();
  }

  async function remove(): Promise<void> {
    if (!confirmingDelete) {
      confirmingDelete = true;
      return;
    }
    busy = true;
    error = null;
    try {
      await deleteSnippet(project, originalName);
      // The file is gone; the words stay in the prompt, as plain typed text.
      if (cid) composeDissolveChip(cid);
      onClose();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      busy = false;
    }
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }

  // Focus the body when editing (that is what you came for), the name when creating.
  $effect(() => {
    const target = untrack(() => (fromChip ? contentEl : nameEl));
    target?.focus();
  });
</script>

<div
  class="modal-backdrop"
  role="dialog"
  aria-modal="true"
  aria-labelledby="snippet-modal-title"
  onkeydown={handleKeydown}
  tabindex="-1"
>
  <div class="modal snippet-modal" tabindex="-1" {@attach focusTrap}>
    <h3 id="snippet-modal-title">{fromChip ? 'Edit snippet' : 'Save as snippet'}</h3>
    {#if fromChip}
      <p class="snippet-modal__status" class:snippet-modal__status--dirty={dirty}>
        {dirty ? 'Draft — not saved to file' : 'Up to date with the library'}
      </p>
    {/if}

    <label class="snippet-modal__field">
      <span>Name</span>
      <input
        type="text"
        bind:this={nameEl}
        bind:value={name}
        autocomplete="off"
        spellcheck="false"
        placeholder="e.g. rust/code_review"
      />
    </label>
    <p class="snippet-modal__hint">
      {#if creatingNew}
        Saves as a <strong>new snippet</strong>, <code>{name.trim()}.md</code> — the original is
        left alone.
      {:else}
        <!-- Subfolders ARE the organization system: the user groups prompts with a
             folder, which they can make here or in Finder. No tags, no UI. -->
        A <code>/</code> in the name makes a folder.
      {/if}
    </p>

    <label class="snippet-modal__field snippet-modal__field--body">
      <span>Content</span>
      <textarea
        class="snippet-modal__body"
        bind:this={contentEl}
        bind:value={content}
        spellcheck="false"
      ></textarea>
    </label>

    {#if variables.length}
      <div class="snippet-modal__vars">
        <span class="snippet-modal__vars-label">Variables</span>
        {#each variables as v (v.name)}
          <div class="snippet-modal__var-row">
            <span class="snippet-modal__var-name">{v.name}</span>
            <input
              type="text"
              class="snippet-modal__var-value"
              value={prompts.fills[v.name] ?? ''}
              oninput={(e) => setFill(v.name, e.currentTarget.value)}
              placeholder={UNSET_VALUE}
              autocomplete="off"
              spellcheck="false"
              aria-label="Value for {v.name}"
            />
          </div>
        {/each}
      </div>
    {/if}

    {#if error}
      <div class="modal__warning">{error}</div>
    {/if}

    <div class="modal__actions snippet-modal__actions">
      <!-- Left: touches the library, writes a file to disk. -->
      <div class="snippet-modal__group">
        {#if fromChip}
          <button
            type="button"
            class="btn btn--ghost btn--sm btn--danger"
            disabled={busy}
            onclick={remove}
          >
            {confirmingDelete ? 'Really delete?' : 'Delete'}
          </button>
        {/if}
        <button
          type="button"
          class="btn btn--sm"
          class:btn--ghost={fromChip}
          class:btn--primary={!fromChip}
          disabled={busy}
          onclick={update}
        >
          Update
        </button>
      </div>
      <span class="snippet-modal__spacer"></span>
      <!-- Right: touches only this prompt, session-only — the low-friction default
           a user reaches for while composing. `Save` is ABSENT (not disabled) when
           there's no chip yet for a session-only edit to apply to. When it's absent,
           `Update` is the only real action left in the dialog and takes the primary
           weight itself, rather than the dialog reading as unfinished with no
           emphasis on its one action; when `Save` is present it's the low-friction
           default, so `Update` steps back to ghost. -->
      <div class="snippet-modal__group">
        <button type="button" class="btn btn--ghost btn--sm" onclick={onClose}>Cancel</button>
        {#if fromChip}
          <button
            type="button"
            class="btn btn--primary btn--sm"
            disabled={busy}
            onclick={save}
            title="Save this edit to this prompt only — nothing is written to the library"
          >
            Save
          </button>
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  .snippet-modal {
    max-width: 560px;
  }

  /* Sync status vs. the library file — informational, not a warning: a dirty
     chip is an ordinary, expected state (that's the whole point of session-only
     Save), not an error. */
  .snippet-modal__status {
    margin: 0.15rem 0 0;
    font-size: 0.66rem;
    color: var(--text-faint);
  }
  .snippet-modal__status--dirty {
    color: color-mix(in srgb, var(--accent-template) 80%, var(--text));
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
  .snippet-modal__field input {
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0.3rem 0.5rem;
    border: 1px solid var(--border);
    border-radius: 0.35rem;
    background: var(--bg);
    color: var(--text);
  }

  .snippet-modal__hint {
    margin: 0.3rem 0 0;
    font-size: 0.66rem;
    color: var(--text-faint);
  }
  .snippet-modal__hint code {
    font-family: var(--font-mono);
  }

  .snippet-modal__body {
    width: 100%;
    min-height: 9rem;
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

  .snippet-modal__vars {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    margin-top: 0.6rem;
  }
  .snippet-modal__vars-label {
    font-size: 0.62rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-faint);
  }
  .snippet-modal__var-row {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }
  .snippet-modal__var-name {
    font-family: var(--font-mono);
    font-size: 0.72rem;
    color: color-mix(in srgb, var(--accent-template) 80%, var(--text));
    min-width: 7rem;
    text-align: right;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .snippet-modal__var-value {
    flex: 1;
    min-width: 0;
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0.3rem 0.55rem;
    border: 1px solid var(--border);
    border-radius: 0.35rem;
    background: var(--bg);
    color: var(--text);
  }
  .snippet-modal__var-value::placeholder {
    color: var(--text-faint);
    font-style: italic;
  }

  .snippet-modal__actions {
    align-items: center;
  }
  .snippet-modal__spacer {
    flex: 1;
  }
  /* The two blast-radius groups: gap inside a group is tighter than the gap the
     spacer opens up between them, so the grouping reads before you read a label. */
  .snippet-modal__group {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }
</style>
