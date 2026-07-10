<script lang="ts">
  /**
   * Semantic-matching config popover (contract §Compose surface) — replaces
   * the inline panel. One "Download & index" action gated on an informed
   * decline (TOTAL size disclosed: model + ONNX runtime), then two bars:
   * Download (runtime + model stages aggregated) and Index (snippet counts).
   * Lexical matching always works; nothing here blocks the common path.
   */
  import { prompts, startEmbedDownload, toggleEmbedEnabled } from '$lib/prompts.svelte';

  let open = $state(false);

  const embed = $derived(prompts.embed);
  /** The decline-informed gate discloses the TOTAL download (model + ONNX
   *  runtime) — the model number alone understates it, badly on Windows. */
  const totalMb = $derived((embed?.model_size_mb ?? 0) + (embed?.runtime_size_mb ?? 0));

  /** Download bar, both download stages aggregated: event totals drive the
   *  intra-stage fraction (unit-safe — bytes over bytes from the same
   *  channel); the disclosed MB sizes only weight the two stages, so the
   *  bar never mixes decimal-MB with byte counts. */
  const downloadPct = $derived.by(() => {
    const p = prompts.embedProgress;
    if (!p || !embed) return 0;
    if (p.stage === 'index') return 100;
    const runtimeShare = totalMb ? embed.runtime_size_mb / totalMb : 0.5;
    const frac = p.total ? p.done / p.total : 0;
    const before = p.stage === 'model' ? runtimeShare : 0;
    const share = p.stage === 'model' ? 1 - runtimeShare : runtimeShare;
    return Math.round((before + frac * share) * 100);
  });

  const indexPct = $derived.by(() => {
    const p = prompts.embedProgress;
    if (!p || p.stage !== 'index' || !p.total) return 0;
    return Math.round((p.done / p.total) * 100);
  });
  const indexCounts = $derived.by(() => {
    const p = prompts.embedProgress;
    return p?.stage === 'index' ? `${p.done} / ${p.total} snippets` : '';
  });

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault();
      open = false;
    }
  }
</script>

<div class="embed-pop">
  <button
    type="button"
    class="embed-pop__trigger"
    class:embed-pop__trigger--on={embed?.state === 'ready'}
    onclick={() => (open = !open)}
    title="Semantic matching settings"
    aria-label="Semantic matching settings"
    aria-expanded={open}
  >
    ⚙
  </button>

  {#if open}
    <div class="embed-pop__backdrop" onclick={() => (open = false)} aria-hidden="true"></div>
    <div
      class="embed-pop__panel"
      role="dialog"
      aria-label="Semantic matching"
      tabindex="-1"
      onkeydown={handleKeydown}
    >
      <div class="embed-pop__title">Semantic matching</div>

      {#if !embed}
        <p class="embed-pop__note">Engine status unavailable.</p>
      {:else if embed.state === 'not_downloaded'}
        <p class="embed-pop__note">
          Optional: a small local embedding model ({embed.model_id}) improves matching by meaning,
          not just spelling. ~{totalMb} MB on disk total ({embed.model_size_mb} MB model +
          {embed.runtime_size_mb} MB runtime), CPU-only inference, fully offline — nothing ever
          leaves this machine. Lexical matching keeps working without it.
        </p>
        <button type="button" class="btn btn--sm" onclick={startEmbedDownload}>
          Download &amp; index (~{totalMb} MB)
        </button>
      {:else if embed.state === 'downloading'}
        <div class="embed-pop__stage">
          <span class="embed-pop__stage-name">Download</span>
          <div class="embed-pop__bar" role="progressbar" aria-valuenow={downloadPct} aria-valuemin={0} aria-valuemax={100} aria-label="Download progress">
            <div class="embed-pop__bar-fill" style="width: {downloadPct}%"></div>
          </div>
          <span class="embed-pop__stage-pct">{downloadPct}%</span>
        </div>
        <div class="embed-pop__stage">
          <span class="embed-pop__stage-name">Index</span>
          <div class="embed-pop__bar" role="progressbar" aria-valuenow={indexPct} aria-valuemin={0} aria-valuemax={100} aria-label="Index progress">
            <div class="embed-pop__bar-fill" style="width: {indexPct}%"></div>
          </div>
          <span class="embed-pop__stage-pct">{indexCounts || `${indexPct}%`}</span>
        </div>
      {:else if embed.state === 'ready' || embed.state === 'off'}
        <label class="embed-pop__toggle">
          <input
            type="checkbox"
            checked={embed.state === 'ready'}
            onchange={(e) => toggleEmbedEnabled(e.currentTarget.checked)}
          />
          <span>Semantic matching {embed.state === 'ready' ? 'on' : 'off'} ({embed.model_id})</span>
        </label>
      {:else if embed.state === 'error'}
        <p class="embed-pop__note embed-pop__note--err">Engine error: {embed.error ?? 'unknown'}</p>
        <!-- "Retry", not "Retry download": the error may be transient inference
             with everything already on disk — the recovery path is the same
             either way, so the label must not promise a re-download. -->
        <button type="button" class="btn btn--sm" onclick={startEmbedDownload}>Retry</button>
      {/if}

      {#if prompts.embedError}
        <p class="embed-pop__note embed-pop__note--err">{prompts.embedError}</p>
      {/if}
    </div>
  {/if}
</div>

<style>
  .embed-pop {
    position: relative;
    display: inline-flex;
  }
  .embed-pop__trigger {
    font-family: inherit;
    font-size: 0.8rem;
    line-height: 1;
    padding: 0.2rem 0.35rem;
    border: 0;
    border-radius: 0.3rem;
    background: transparent;
    color: var(--text-faint);
    cursor: pointer;
  }
  .embed-pop__trigger:hover {
    color: var(--text);
    background: var(--bg-subtle);
  }
  .embed-pop__trigger--on {
    color: var(--accent-snippet);
  }
  .embed-pop__backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
    background: transparent;
  }
  .embed-pop__panel {
    position: absolute;
    top: 1.7rem;
    left: 0;
    z-index: 41;
    width: min(21rem, 88vw);
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding: 0.75rem 0.85rem;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    box-shadow: 0 10px 32px rgba(0, 0, 0, 0.18);
    font-size: 0.72rem;
  }
  .embed-pop__title {
    font-size: 0.68rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-faint);
  }
  .embed-pop__note {
    color: var(--text-muted);
    line-height: 1.5;
    margin: 0;
  }
  .embed-pop__note--err {
    color: var(--accent-result-err);
  }
  .embed-pop__stage {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .embed-pop__stage-name {
    min-width: 4.2rem;
    color: var(--text-muted);
  }
  .embed-pop__bar {
    flex: 1;
    height: 0.4rem;
    border-radius: 0.2rem;
    background: var(--bg-subtle);
    overflow: hidden;
  }
  .embed-pop__bar-fill {
    height: 100%;
    background: var(--accent-snippet);
    transition: width 0.15s ease;
  }
  .embed-pop__stage-pct {
    min-width: 4.5rem;
    text-align: right;
    color: var(--text-faint);
    font-variant-numeric: tabular-nums;
  }
  .embed-pop__toggle {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    color: var(--text-muted);
    cursor: pointer;
  }
</style>
