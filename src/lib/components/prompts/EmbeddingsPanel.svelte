<script lang="ts">
  /**
   * Opt-in semantic matching — deliberately tucked behind an "Advanced"
   * disclosure at the bottom of the panel (simple by default, advanced on
   * demand): lexical matching always works; nothing here ever blocks the
   * common path. The model downloads only on an explicit click, with the
   * requirements stated first so weak-machine users can decline informed.
   */
  import { prompts, startEmbedDownload, toggleEmbedEnabled } from '$lib/prompts.svelte';

  const embed = $derived(prompts.embed);
  const pct = $derived.by(() => {
    const p = prompts.embedProgress;
    if (!p || !p.total_bytes) return 0;
    return Math.round((p.downloaded_bytes / p.total_bytes) * 100);
  });
</script>

<details class="embed-panel">
  <summary>Advanced: semantic matching</summary>
  {#if !embed}
    <p class="embed-panel__note">Engine status unavailable.</p>
  {:else}
    {#if embed.state === 'not_downloaded'}
      <p class="embed-panel__note">
        Optional: a small local embedding model ({embed.model_id}, ~{embed.model_size_mb} MB on
        disk) improves matching by meaning, not just spelling. CPU-only inference, fully offline —
        nothing ever leaves this machine. Lexical matching keeps working without it.
      </p>
      <button type="button" class="btn btn--sm" onclick={startEmbedDownload}>
        Download model (~{embed.model_size_mb} MB)
      </button>
    {:else if embed.state === 'downloading'}
      <p class="embed-panel__note">Downloading {embed.model_id}…</p>
      <div class="embed-panel__bar" role="progressbar" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100}>
        <div class="embed-panel__bar-fill" style="width: {pct}%"></div>
      </div>
      <p class="embed-panel__pct">{pct}%</p>
    {:else if embed.state === 'ready' || embed.state === 'off'}
      <label class="embed-panel__toggle">
        <input
          type="checkbox"
          checked={embed.state === 'ready'}
          onchange={(e) => toggleEmbedEnabled(e.currentTarget.checked)}
        />
        <span>Semantic matching {embed.state === 'ready' ? 'on' : 'off'} ({embed.model_id})</span>
      </label>
    {:else if embed.state === 'error'}
      <p class="embed-panel__note embed-panel__note--err">
        Engine error: {embed.error ?? 'unknown'}
      </p>
      <button type="button" class="btn btn--sm" onclick={startEmbedDownload}>Retry download</button>
    {/if}
  {/if}
  {#if prompts.embedError}
    <p class="embed-panel__note embed-panel__note--err">{prompts.embedError}</p>
  {/if}
</details>

<style>
  .embed-panel {
    margin-top: 0.75rem;
    border-top: 1px solid var(--border);
    padding-top: 0.5rem;
    font-size: 0.72rem;
  }
  .embed-panel summary {
    cursor: pointer;
    color: var(--text-faint);
    user-select: none;
  }
  .embed-panel summary:hover { color: var(--text-muted); }
  .embed-panel__note {
    color: var(--text-muted);
    line-height: 1.5;
    margin: 0.5rem 0;
  }
  .embed-panel__note--err { color: var(--accent-result-err); }
  .embed-panel__bar {
    height: 0.4rem;
    border-radius: 0.2rem;
    background: var(--bg-subtle);
    overflow: hidden;
  }
  .embed-panel__bar-fill {
    height: 100%;
    background: var(--accent-piece);
    transition: width 0.15s ease;
  }
  .embed-panel__pct {
    color: var(--text-faint);
    margin: 0.25rem 0 0;
  }
  .embed-panel__toggle {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    color: var(--text-muted);
    margin-top: 0.4rem;
    cursor: pointer;
  }
</style>
