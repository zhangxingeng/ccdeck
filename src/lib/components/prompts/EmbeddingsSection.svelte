<script lang="ts">
  /**
   * Semantic-matching config — a SECTION of the app-level config popover
   * (contract §S10/§S11). One "Download & index" action gated on an informed
   * decline (TOTAL size disclosed: model + ONNX runtime), then two bars:
   * Download (runtime + model stages aggregated) and Index (snippet counts).
   * Lexical matching always works; nothing here blocks the common path.
   *
   * Extracted from the former standalone EmbeddingsPopover — the gear and
   * popover chrome now belong to ConfigPopover, which hosts this alongside
   * Shortcuts and Notices. The flow is unchanged; only its home moved.
   */
  import { prompts, startEmbedDownload, toggleEmbedEnabled } from '$lib/prompts.svelte';

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
</script>

<section class="embed-sec">
  <div class="config-sec__title">Semantic matching</div>

  {#if !embed}
    <p class="embed-sec__note">Engine status unavailable.</p>
  {:else if embed.state === 'not_downloaded'}
    <p class="embed-sec__note">
      Optional: a small local embedding model ({embed.model_id}) improves matching by meaning,
      not just spelling. ~{totalMb} MB on disk total ({embed.model_size_mb} MB model +
      {embed.runtime_size_mb} MB runtime), CPU-only inference, fully offline — nothing ever
      leaves this machine. Lexical matching keeps working without it.
    </p>
    <button type="button" class="btn btn--sm" onclick={startEmbedDownload}>
      Download &amp; index (~{totalMb} MB)
    </button>
  {:else if embed.state === 'downloading'}
    <div class="embed-sec__stage">
      <span class="embed-sec__stage-name">Download</span>
      <div class="embed-sec__bar" role="progressbar" aria-valuenow={downloadPct} aria-valuemin={0} aria-valuemax={100} aria-label="Download progress">
        <div class="embed-sec__bar-fill" style="width: {downloadPct}%"></div>
      </div>
      <span class="embed-sec__stage-pct">{downloadPct}%</span>
    </div>
    <div class="embed-sec__stage">
      <span class="embed-sec__stage-name">Index</span>
      <div class="embed-sec__bar" role="progressbar" aria-valuenow={indexPct} aria-valuemin={0} aria-valuemax={100} aria-label="Index progress">
        <div class="embed-sec__bar-fill" style="width: {indexPct}%"></div>
      </div>
      <span class="embed-sec__stage-pct">{indexCounts || `${indexPct}%`}</span>
    </div>
  {:else if embed.state === 'ready' || embed.state === 'off'}
    <label class="embed-sec__toggle">
      <input
        type="checkbox"
        checked={embed.state === 'ready'}
        onchange={(e) => toggleEmbedEnabled(e.currentTarget.checked)}
      />
      <span>Semantic matching {embed.state === 'ready' ? 'on' : 'off'} ({embed.model_id})</span>
    </label>
  {:else if embed.state === 'error'}
    <p class="embed-sec__note embed-sec__note--err">Engine error: {embed.error ?? 'unknown'}</p>
    <!-- "Retry", not "Retry download": the error may be transient inference
         with everything already on disk — the recovery path is the same
         either way, so the label must not promise a re-download. -->
    <button type="button" class="btn btn--sm" onclick={startEmbedDownload}>Retry</button>
  {/if}

  {#if prompts.embedError}
    <p class="embed-sec__note embed-sec__note--err">{prompts.embedError}</p>
  {/if}
</section>

<style>
  .embed-sec {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    font-size: 0.72rem;
  }
  .config-sec__title {
    font-size: 0.66rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-faint);
  }
  .embed-sec__note {
    color: var(--text-muted);
    line-height: 1.5;
    margin: 0;
  }
  .embed-sec__note--err {
    color: var(--accent-result-err);
  }
  .embed-sec__stage {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .embed-sec__stage-name {
    min-width: 4.2rem;
    color: var(--text-muted);
  }
  .embed-sec__bar {
    flex: 1;
    height: 0.4rem;
    border-radius: 0.2rem;
    background: var(--bg-subtle);
    overflow: hidden;
  }
  .embed-sec__bar-fill {
    height: 100%;
    background: var(--accent-snippet);
    transition: width 0.15s ease;
  }
  .embed-sec__stage-pct {
    min-width: 4.5rem;
    text-align: right;
    color: var(--text-faint);
    font-variant-numeric: tabular-nums;
  }
  .embed-sec__toggle {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    color: var(--text-muted);
    cursor: pointer;
  }
</style>
