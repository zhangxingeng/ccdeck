<script lang="ts">
  /**
   * The variable fill list: one row per distinct variable in the WHOLE composed
   * prompt — typed text and every chip's body — in first-appearance order. Lives
   * in its own column on the right of the screen (round 2), out from under the
   * compose box so the box can be as wide as possible.
   *
   * Variables are global by name. (The model cannot tell two identically-named
   * variables apart, so pretending they differ would be a fiction the UI maintains
   * and the output discards.) One name is one cell, and that cell appears in two
   * places: here, and in the popup of any chip whose body uses it. Editing either
   * updates the same value, and the other reflects it immediately.
   *
   * That is NOT the two-places-to-edit confusion this round exists to kill. That one
   * was about snippet BODIES, where two surfaces meant two divergent sources of
   * truth. A variable's value is a single global cell; showing one cell in two views
   * is convenience, not ambiguity.
   *
   * Round 1 gave each row its own as-variable toggle. Round 2 cut it: every
   * variable is now always hoisted into the appended `<prompt_vars>` block on
   * copy (see `compose/variables.ts`) — a toggle nobody flipped is the archetype
   * of the forgotten feature this whole effort exists to delete.
   */
  import { prompts, setFill } from '$lib/prompts.svelte';
  import { flatten } from '$lib/compose/doc';
  import { parseVariables, UNSET_VALUE } from '$lib/compose/variables';

  // flatten(), not the rendered text: a chip shows its NAME in the box but
  // contributes its BODY to the prompt, so its variables must surface here.
  const variables = $derived(parseVariables(flatten(prompts.doc)));
</script>

{#if variables.length}
  <div class="fill-list" aria-label="Variable fills">
    {#each variables as v (v.name)}
      <div class="fill-list__row">
        <span class="fill-list__name" title={v.name}>{v.name}</span>
        <input
          class="fill-list__value"
          type="text"
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

<style>
  .fill-list {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding: 0.15rem;
  }
  .fill-list__row {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }
  .fill-list__name {
    font-family: var(--font-mono);
    font-size: 0.72rem;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .fill-list__value {
    width: 100%;
    min-width: 0;
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0.3rem 0.55rem;
    border: 1px solid var(--border);
    border-radius: 0.35rem;
    background: var(--bg-card);
    color: var(--text);
    box-sizing: border-box;
  }
  /* The placeholder is the literal text an unfilled variable copies out as — the
     prompt still works, the model just asks. Italic, so it reads as a preview of
     what will happen rather than as a value already set. */
  .fill-list__value::placeholder {
    color: var(--text-faint);
    font-style: italic;
  }
  .fill-list__value:focus {
    outline: none;
    border-color: color-mix(in srgb, var(--accent-snippet) 60%, var(--border));
  }
</style>
