<script lang="ts">
  /**
   * UpdateBanner — in-app auto-update surface. Renders purely off the reactive
   * `update` state from updater.svelte.ts:
   *   available   → actionable banner (Update & restart / Later)
   *   downloading → progress bar
   *   checking/uptodate/error → transient toast (from a manual check)
   *   idle        → nothing
   */
  import { update, installUpdate, dismiss } from '$lib/updater.svelte';
</script>

{#if update.status === 'available'}
  <div class="update-banner" role="dialog" aria-label="Update available">
    <span class="update-banner__text">
      Update available — v{update.newVersion}
    </span>
    <div class="update-banner__actions">
      <button class="btn btn--primary btn--sm" onclick={installUpdate} type="button">
        Update &amp; restart
      </button>
      <button class="btn btn--ghost btn--sm" onclick={dismiss} type="button">
        Later
      </button>
    </div>
  </div>
{:else if update.status === 'downloading'}
  <div class="update-banner" role="status">
    <span class="update-banner__text">Downloading update… {update.progress}%</span>
    <div class="update-progress" aria-hidden="true">
      <div class="update-progress__fill" style="width:{update.progress}%"></div>
    </div>
  </div>
{:else if update.status === 'checking'}
  <div class="toast" role="status">Checking for updates…</div>
{:else if update.status === 'uptodate'}
  <div class="toast" role="status">You're on the latest version.</div>
{:else if update.status === 'error'}
  <div class="toast" role="status">Update check failed: {update.error}</div>
{/if}

<style>
  .update-banner {
    position: fixed;
    bottom: 1.25rem;
    left: 50%;
    transform: translateX(-50%);
    z-index: 200;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    min-width: 18rem;
    max-width: min(90vw, 28rem);
    padding: 0.75rem 1rem;
    border-radius: 0.5rem;
    background: var(--bg-card);
    color: var(--text);
    border: 1px solid var(--border-strong);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25);
    font-size: 0.8rem;
  }
  .update-banner__text {
    font-weight: 500;
  }
  .update-banner__actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }
  .update-progress {
    height: 6px;
    width: 100%;
    border-radius: 3px;
    background: var(--bg-subtle);
    overflow: hidden;
  }
  .update-progress__fill {
    height: 100%;
    background: var(--accent-user);
    border-radius: 3px;
    transition: width 0.15s ease;
  }
</style>
