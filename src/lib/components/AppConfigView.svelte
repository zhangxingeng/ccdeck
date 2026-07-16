<script lang="ts">
  /**
   * AppConfigView.svelte — CC Deck's own preferences page (never Claude Code's
   * settings.json — users hand-edit that themselves).
   *
   * Shrunk to a single preference in v0.14 (issue #34): the update-check-on-launch
   * toggle. The terminal launcher (terminal choice + resume-launch command) and
   * the provider-profile manager were both removed — Resume now surfaces the
   * session's facts as copyable text and the user acts in their own terminal, so
   * there is nothing left to configure here.
   */
  import { onMount, onDestroy } from 'svelte';
  import type { AppConfig } from '$lib/types';
  import { getAppConfig, setAppConfig } from '$lib/api';

  let { onClose = () => {} }: { onClose?: () => void } = $props();

  let loading = $state(true);
  let loadError = $state<string | null>(null);
  let config = $state<AppConfig>({ updateCheckOnLaunch: true });
  let original = $state('');
  let saveMsg = $state<string | null>(null);
  let saveMsgTimer: ReturnType<typeof setTimeout> | null = null;

  let dirty = $derived(JSON.stringify(config) !== original);

  async function load(): Promise<void> {
    loading = true;
    loadError = null;
    try {
      config = await getAppConfig();
      original = JSON.stringify(config);
    } catch (e) {
      loadError = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  onMount(load);

  function showSaved(msg: string): void {
    saveMsg = msg;
    if (saveMsgTimer) clearTimeout(saveMsgTimer);
    saveMsgTimer = setTimeout(() => {
      saveMsg = null;
      saveMsgTimer = null;
    }, 2500);
  }

  // Clear any pending auto-dismiss timer on unmount so it can't fire against a
  // torn-down component or leak.
  onDestroy(() => { if (saveMsgTimer) clearTimeout(saveMsgTimer); });

  async function save(): Promise<void> {
    try {
      await setAppConfig(config);
      original = JSON.stringify(config);
      showSaved('Saved');
    } catch (e) {
      loadError = e instanceof Error ? e.message : String(e);
    }
  }

  function discard(): void {
    config = JSON.parse(original);
  }
</script>

<div class="appconfig-view">
  <div class="appconfig-head">
    <div>
      <h2>App Config</h2>
      <div class="scope">CC Deck preferences — not Claude Code's own settings.json</div>
    </div>
    <button class="btn btn--ghost btn--sm" onclick={onClose} type="button">Close</button>
  </div>

  {#if loading}
    <div class="empty-state">Loading App Config…</div>
  {:else if loadError}
    <div class="empty-state">{loadError}</div>
  {:else}
    <!-- ── Update check ──────────────────────────────────────────────────── -->
    <fieldset class="group">
      <legend>Updates</legend>
      <label class="bool-row">
        <input type="checkbox" bind:checked={config.updateCheckOnLaunch} />
        Check for updates automatically on launch
      </label>
      <p class="hint">The footer's "Check for updates" button always works regardless of this setting.</p>
    </fieldset>

    <!-- ── Save bar ──────────────────────────────────────────────────────── -->
    <div class="save-bar">
      <button class="btn btn--ghost btn--sm" onclick={discard} disabled={!dirty} type="button">
        Discard changes
      </button>
      <button class="btn btn--primary btn--sm" onclick={save} disabled={!dirty} type="button"> Save </button>
    </div>
  {/if}

  {#if saveMsg}
    <div class="toast" role="status">{saveMsg}</div>
  {/if}
</div>

<style>
  .appconfig-view { display: flex; flex-direction: column; gap: 0.75rem; }
  .appconfig-head { display: flex; align-items: center; justify-content: space-between; }
  .appconfig-head h2 { margin: 0; font-size: 1.05rem; }
  .scope { font-size: 0.72rem; color: var(--text-faint); text-transform: uppercase; letter-spacing: 0.06em; }

  .group { border: 1px solid var(--border); border-radius: 0.45rem; padding: 0.6rem 0.75rem; display: flex; flex-direction: column; gap: 0.6rem; }
  .group legend { font-size: 0.78rem; font-weight: 600; color: var(--text); padding: 0 0.3rem; }
  .hint { font-size: 0.76rem; color: var(--text-muted); margin: 0; }

  .bool-row { display: flex; align-items: center; gap: 0.4rem; font-size: 0.8rem; }

  .save-bar { display: flex; justify-content: flex-end; gap: 0.5rem; }
</style>
