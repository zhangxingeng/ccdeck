<script lang="ts">
  /**
   * AppConfigView.svelte — CC Deck's own preferences page (never Claude Code's
   * settings.json — that schema-driven editor lives at SettingsSearchView.svelte,
   * a separate view/entry point; see issue #18's partial reversal + #20).
   *
   * Holds three preferences, all persisted via getAppConfig/setAppConfig to
   * ~/.claude/.ccstudio-config.json:
   *   - Terminal launcher choice (auto-detect, or a custom terminal command prefix)
   *   - Resume-launch command: a fully custom, multi-line-capable shell script,
   *     run with CCDECK_SESSION_ID / CCDECK_SESSION_TITLE / CCDECK_CWD exported
   *     into its environment. Defaults to `claude --resume "$CCDECK_SESSION_ID"`.
   *   - Update-check-on-launch toggle (default on; the footer's manual "Check
   *     for updates" button always runs regardless of this toggle).
   *
   * This is a single global-scope page — launch command / terminal / update
   * toggle are app-level preferences, not per-project, so (unlike
   * SettingsSearchView) there is no project-cwd scoping here.
   */
  import { onMount, onDestroy } from 'svelte';
  import type { AppConfig } from '$lib/types';
  import { getAppConfig, setAppConfig } from '$lib/api';

  let { onClose = () => {} }: { onClose?: () => void } = $props();

  const PRESETS: { label: string; command: string }[] = [
    { label: 'Plain (default)', command: 'claude --resume "$CCDECK_SESSION_ID"' },
    {
      label: 'tmux session',
      command: 'tmux new-session -A -s "$CCDECK_SESSION_TITLE" "claude --resume $CCDECK_SESSION_ID"',
    },
  ];

  let loading = $state(true);
  let loadError = $state<string | null>(null);
  let config = $state<AppConfig>({ terminal: '', launchCommand: '', updateCheckOnLaunch: true });
  let original = $state('');
  let saveMsg = $state<string | null>(null);
  let saveMsgTimer: ReturnType<typeof setTimeout> | null = null;

  let dirty = $derived(JSON.stringify(config) !== original);

  function isAutoTerminal(t: string): boolean {
    const s = t.trim().toLowerCase();
    return s === '' || s === 'auto';
  }

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

  function applyPreset(command: string): void {
    config.launchCommand = command;
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
    <!-- ── Terminal ──────────────────────────────────────────────────────── -->
    <fieldset class="group">
      <legend>Terminal</legend>
      <p class="hint">How CC Deck launches Claude Code when you click Resume. Automatic works for most setups.</p>
      <label class="radio-row">
        <input
          type="radio"
          name="terminal-mode"
          checked={isAutoTerminal(config.terminal)}
          onchange={() => (config.terminal = '')}
        />
        Automatic (recommended)
      </label>
      <label class="radio-row">
        <input
          type="radio"
          name="terminal-mode"
          checked={!isAutoTerminal(config.terminal)}
          onchange={() => {
            if (isAutoTerminal(config.terminal)) config.terminal = 'gnome-terminal --';
          }}
        />
        Custom
      </label>

      {#if !isAutoTerminal(config.terminal)}
        <div class="field">
          <label for="terminal-cmd">Terminal command</label>
          <input
            id="terminal-cmd"
            type="text"
            class="text-input"
            bind:value={config.terminal}
            placeholder="e.g. gnome-terminal --, konsole -e, iTerm, wt"
          />
        </div>
      {/if}
    </fieldset>

    <!-- ── Resume-launch command ────────────────────────────────────────── -->
    <fieldset class="group">
      <legend>Resume launch command</legend>
      <p class="hint">
        Run when you click Resume. Three env vars are exported before it runs:
        <code>CCDECK_SESSION_ID</code>, <code>CCDECK_SESSION_TITLE</code>, <code>CCDECK_CWD</code>.
        Multi-line is fine — this can be a small script, not just a one-liner.
      </p>

      <div class="presets">
        {#each PRESETS as p (p.label)}
          <button class="btn btn--ghost btn--sm" type="button" onclick={() => applyPreset(p.command)}>
            {p.label}
          </button>
        {/each}
      </div>

      <div class="field">
        <label for="launch-command">Command</label>
        <textarea
          id="launch-command"
          class="command-input"
          rows="4"
          bind:value={config.launchCommand}
          placeholder={'claude --resume "$CCDECK_SESSION_ID"'}
        ></textarea>
        <p class="hint">Empty = the default shown above.</p>
      </div>
    </fieldset>

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
  .hint code { font-size: 0.72rem; }

  .field { display: flex; flex-direction: column; gap: 0.3rem; }
  .field label { font-size: 0.78rem; color: var(--text); }

  .text-input, .command-input {
    width: 100%; font-size: 0.8rem; padding: 0.35rem 0.5rem;
    background: var(--bg-card); color: var(--text); border: 1px solid var(--border-strong); border-radius: 0.35rem;
    font-family: inherit; box-sizing: border-box;
  }
  .command-input { font-family: var(--font-mono, monospace); resize: vertical; }

  .bool-row, .radio-row { display: flex; align-items: center; gap: 0.4rem; font-size: 0.8rem; }

  .presets { display: flex; gap: 0.4rem; flex-wrap: wrap; }

  .save-bar { display: flex; justify-content: flex-end; gap: 0.5rem; }
</style>
