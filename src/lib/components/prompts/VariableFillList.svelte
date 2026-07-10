<script lang="ts">
  /**
   * The unified variable fill list (contract §Compose surface): auto-appears
   * beneath the box whenever parsing finds variables — one row per distinct
   * name in first-appearance order, the default as placeholder text. One
   * name = one variable document-wide (grammar rule 4), so inserting a piece
   * just merges its names into this list; values substitute only at copy.
   */
  import { prompts, setFill } from '$lib/prompts.svelte';
  import { parseVariables } from '$lib/compose/variables';

  const variables = $derived(parseVariables(prompts.doc.text));
</script>

{#if variables.length}
  <div class="fill-list" aria-label="Variable fills">
    {#each variables as v (v.name)}
      <label class="fill-list__row">
        <span class="fill-list__name">{v.name}</span>
        <input
          type="text"
          value={prompts.fills[v.name] ?? ''}
          oninput={(e) => setFill(v.name, e.currentTarget.value)}
          placeholder={v.default ?? 'fill on copy'}
          autocomplete="off"
          spellcheck="false"
        />
      </label>
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
  .fill-list__row input {
    flex: 1;
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0.3rem 0.55rem;
    border: 1px solid var(--border);
    border-radius: 0.35rem;
    background: var(--bg-card);
    color: var(--text);
  }
  .fill-list__row input::placeholder {
    color: var(--text-faint);
  }
  .fill-list__row input:focus {
    outline: none;
    border-color: color-mix(in srgb, var(--project-color, var(--accent-piece)) 60%, var(--border));
  }
</style>
