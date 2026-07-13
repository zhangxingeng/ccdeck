<script lang="ts">
  /**
   * The variable fill list under the compose box: one row per distinct variable in
   * the WHOLE composed prompt — typed text and every chip's body — in
   * first-appearance order.
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
   * Each row carries its own as-variable toggle, default ON: as-variable never breaks
   * a prompt, while substituting unexpected data in place can silently bloat it — so
   * the safe side is the default and the user opts out per variable.
   */
  import { prompts, setFill, setAsVar } from '$lib/prompts.svelte';
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
        <!-- Absent from asVars = ON (the safe default); an explicit false is OFF. -->
        <label
          class="fill-list__asvar"
          title="On: copies as a <prompt_var> reference, with the value hoisted into one block. Off: the value substitutes in place."
        >
          <input
            type="checkbox"
            checked={prompts.asVars[v.name] !== false}
            onchange={(e) => setAsVar(v.name, e.currentTarget.checked)}
            aria-label="Copy {v.name} as a variable reference"
          />
          <span>as var</span>
        </label>
      </div>
    {/each}
  </div>
{/if}

<style>
  .fill-list {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    padding: 0.5rem 0.15rem 0;
  }
  .fill-list__row {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }
  .fill-list__name {
    font-family: var(--font-mono);
    font-size: 0.72rem;
    color: var(--text-muted);
    min-width: 7rem;
    text-align: right;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .fill-list__value {
    flex: 1;
    min-width: 0;
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0.3rem 0.55rem;
    border: 1px solid var(--border);
    border-radius: 0.35rem;
    background: var(--bg-card);
    color: var(--text);
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
  .fill-list__asvar {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    font-size: 0.66rem;
    color: var(--text-muted);
    cursor: pointer;
    user-select: none;
    flex-shrink: 0;
  }
</style>
