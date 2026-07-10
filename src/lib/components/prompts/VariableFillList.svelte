<script lang="ts">
  /**
   * The unified variable fill list (contract §Compose surface / §S4): auto-
   * appears beneath the box whenever parsing finds variables — one row per
   * distinct name in first-appearance order, the default as placeholder text.
   * One name = one variable document-wide (grammar rule 4), so inserting a
   * snippet just merges its names into this list; values substitute only at copy.
   *
   * Each row carries its own **as-variable toggle** (this round's replacement
   * for the single global switch). Default ON for every variable — the founder's
   * ruling: as-variable never breaks anything, while an in-place substitution of
   * unexpected data can silently bloat the prompt. `Tab` reaches the fill input
   * then the toggle; `Space` flips the focused toggle (native checkbox).
   */
  import { prompts, setFill, setAsVar } from '$lib/prompts.svelte';
  import { parseVariables } from '$lib/compose/variables';

  const variables = $derived(parseVariables(prompts.doc.text));
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
          placeholder={v.default ?? 'fill on copy'}
          autocomplete="off"
          spellcheck="false"
          aria-label="Value for {v.name}"
        />
        <!-- Absent from asVars = ON (the safe default); an explicit false is OFF. -->
        <label
          class="fill-list__asvar"
          title="On: this variable copies as a <prompt_var> reference with its value hoisted into one block. Off: its value substitutes in place."
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
  .fill-list__value::placeholder {
    color: var(--text-faint);
  }
  .fill-list__value:focus {
    outline: none;
    border-color: color-mix(in srgb, var(--project-color, var(--accent-snippet)) 60%, var(--border));
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
