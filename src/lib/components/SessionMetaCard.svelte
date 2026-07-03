<script lang="ts">
  /**
   * SessionMetaCard.svelte — the pinned header card summarizing a session
   * (project, models, CLI versions, permission mode, date range, counts).
   * Pure presentation from an extracted SessionInfo.
   */
  import type { SessionInfo } from '$lib/editDraft';

  let { info }: { info: SessionInfo } = $props();

  function formatIso(iso: string): string {
    if (!iso) return '';
    try {
      return new Date(iso).toLocaleString(undefined, {
        year: 'numeric', month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
      });
    } catch { return iso; }
  }
</script>

<div class="meta-card">
  {#if info.cwd}
    <div class="meta-card__row">
      <div class="meta-card__item">
        <span class="meta-card__label">Project</span>
        <span
          class="meta-card__value meta-card__value--mono"
          title={info.cwd}
          data-copy-text={info.cwd}
        >{info.cwd}</span>
        {#if info.gitBranch}
          <span class="meta-card__chip meta-card__chip--branch" title={info.gitBranch} data-copy-text={info.gitBranch}>{info.gitBranch}</span>
        {/if}
      </div>
    </div>
  {/if}

  <div class="meta-card__row meta-card__row--wrap">
    {#if info.models.length > 0}
      <div class="meta-card__item">
        <span class="meta-card__label">Models</span>
        <span class="meta-card__chips">
          {#each info.models as m}
            <span class="meta-card__chip meta-card__chip--model" title={m} data-copy-text={m}>{m}</span>
          {/each}
        </span>
      </div>
    {/if}
    {#if info.versions.length > 0}
      <div class="meta-card__item">
        <span class="meta-card__label">CLI</span>
        <span class="meta-card__chips">
          {#each info.versions as v}<span class="meta-card__chip" title={v} data-copy-text={v}>{v}</span>{/each}
        </span>
      </div>
    {/if}
    {#if info.permissionMode}
      <div class="meta-card__item">
        <span class="meta-card__label">Permission</span>
        <span class="meta-card__chip meta-card__chip--perm" title={info.permissionMode} data-copy-text={info.permissionMode}>{info.permissionMode}</span>
      </div>
    {/if}
  </div>

  <div class="meta-card__row meta-card__row--wrap">
    {#if info.firstTs}
      <div class="meta-card__item">
        <span class="meta-card__label">Date range</span>
        <span class="meta-card__value">{formatIso(info.firstTs)}</span>
        {#if info.lastTs && info.lastTs !== info.firstTs}
          <span class="meta-card__sep">–</span>
          <span class="meta-card__value">{formatIso(info.lastTs)}</span>
        {/if}
      </div>
    {/if}
    <div class="meta-card__item">
      <span class="meta-card__label">Turns</span>
      <span class="meta-card__value">{info.userCount}</span>
      <span class="meta-card__label" style="margin-left:0.75rem;">Lines</span>
      <span class="meta-card__value">{info.lineCount}</span>
    </div>
  </div>

  <div class="meta-card__hint">Double-click any message to edit it in place. Tool activity is grouped &amp; collapsed — click a strip to expand.</div>
</div>

<style>
  .meta-card {
    background: var(--bg-card); border: 1px solid var(--border);
    border-radius: 0.5rem; padding: 0.85rem 1rem; margin-bottom: 1rem;
    display: flex; flex-direction: column; gap: 0.55rem;
  }
  .meta-card__row { display: flex; align-items: center; gap: 1rem; min-width: 0; }
  .meta-card__row--wrap { flex-wrap: wrap; gap: 0.6rem 1.25rem; }
  .meta-card__item { display: flex; align-items: center; gap: 0.4rem; min-width: 0; flex-shrink: 0; }
  .meta-card__label {
    font-size: 0.65rem; font-weight: 600; text-transform: uppercase;
    letter-spacing: 0.08em; color: var(--text-faint); white-space: nowrap;
  }
  .meta-card__value { font-size: 0.78rem; color: var(--text-muted); }
  .meta-card__value--mono {
    font-family: var(--font-mono); font-size: 0.72rem; overflow: hidden;
    text-overflow: ellipsis; white-space: nowrap; max-width: 36ch;
  }
  .meta-card__sep { color: var(--text-faint); font-size: 0.75rem; }
  .meta-card__chips { display: flex; flex-wrap: wrap; gap: 0.3rem; }
  .meta-card__chip {
    display: inline-block; font-size: 0.65rem; font-weight: 500;
    padding: 0.12rem 0.4rem; border-radius: 0.25rem; background: var(--bg-subtle);
    border: 1px solid var(--border); color: var(--text-muted); white-space: nowrap;
  }
  .meta-card__chip--model {
    color: var(--accent-thinking);
    background: color-mix(in srgb, var(--accent-thinking) 8%, transparent);
    border-color: color-mix(in srgb, var(--accent-thinking) 25%, transparent);
  }
  .meta-card__chip--branch {
    color: var(--accent-result-ok);
    background: color-mix(in srgb, var(--accent-result-ok) 8%, transparent);
    border-color: color-mix(in srgb, var(--accent-result-ok) 25%, transparent);
    font-family: var(--font-mono); font-size: 0.62rem;
  }
  .meta-card__chip--perm {
    color: var(--accent-tool);
    background: color-mix(in srgb, var(--accent-tool) 8%, transparent);
    border-color: color-mix(in srgb, var(--accent-tool) 25%, transparent);
  }
  .meta-card__hint {
    font-size: 0.7rem; color: var(--text-faint); font-style: italic;
    padding-top: 0.15rem; border-top: 1px dashed var(--border); margin-top: 0.1rem;
  }
</style>
