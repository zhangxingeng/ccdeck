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
  import type { AppConfig, ProviderProfile, KeyBackend } from '$lib/types';
  import {
    getAppConfig,
    setAppConfig,
    listProviderProfiles,
    saveProviderProfile,
    deleteProviderProfile,
    setProviderKey,
    providerKeyStatus,
    providerProbeKeychain,
    isKeychainUnavailable,
  } from '$lib/api';

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
  let config = $state<AppConfig>({
    terminal: '',
    launchCommand: '',
    updateCheckOnLaunch: true,
  });
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

  // ── Provider profiles (issue #21) ────────────────────────────────────────
  // The API key is write-only and never round-trips to the UI: we only ever
  // learn whether a key IS set (providerKeyStatus) and push a NEW key
  // (setProviderKey). Name is immutable once created.

  let profiles = $state<ProviderProfile[]>([]);
  let keyStatus = $state<Record<string, boolean>>({});
  let keychainAvailable = $state<boolean | null>(null);
  let providersError = $state<string | null>(null);
  let savingProvider = $state(false);

  // The inline add/edit form. `null` when the list is at rest.
  type ProviderDraft = {
    mode: 'add' | 'edit';
    name: string;
    baseUrl: string;
    defaultModel: string;
    keyInput: string;      // a NEW pasted key only — never a stored value
    replacingKey: boolean; // edit mode: reveal the key input over "set ✓"
  };
  let draft = $state<ProviderDraft | null>(null);

  // When setProviderKey rejects with KEYCHAIN_UNAVAILABLE we stash the pending
  // key here and show the explicit plaintext opt-in — never auto-retry.
  let plaintextPrompt = $state<{ name: string; key: string } | null>(null);

  async function loadProviders(): Promise<void> {
    try {
      const list = await listProviderProfiles();
      const status: Record<string, boolean> = {};
      for (const p of list) status[p.name] = await providerKeyStatus(p.name);
      profiles = list;
      keyStatus = status;
    } catch (e) {
      providersError = e instanceof Error ? e.message : String(e);
    }
  }

  async function loadProvidersAndProbe(): Promise<void> {
    await loadProviders();
    try {
      keychainAvailable = await providerProbeKeychain();
    } catch {
      keychainAvailable = false;
    }
  }

  onMount(loadProvidersAndProbe);

  function badgeFor(p: ProviderProfile): { icon: string; label: string; cls: string } {
    if (p.keyBackend === 'keychain') return { icon: '🔒', label: 'keychain', cls: 'badge--keychain' };
    if (p.keyBackend === 'plaintext') return { icon: '⚠', label: 'plaintext', cls: 'badge--plaintext' };
    return { icon: '○', label: 'no key', cls: 'badge--none' };
  }

  function startAdd(): void {
    providersError = null;
    plaintextPrompt = null;
    draft = { mode: 'add', name: '', baseUrl: '', defaultModel: '', keyInput: '', replacingKey: false };
  }

  function startEdit(p: ProviderProfile): void {
    providersError = null;
    plaintextPrompt = null;
    draft = {
      mode: 'edit',
      name: p.name,
      baseUrl: p.baseUrl,
      defaultModel: p.defaultModel ?? '',
      keyInput: '',
      replacingKey: false,
    };
  }

  function cancelDraft(): void {
    draft = null;
    plaintextPrompt = null;
    providersError = null;
  }

  /** Push a new key; returns the backend used, or null if the keychain is
   *  unavailable (in which case the plaintext opt-in prompt is shown). */
  async function applyKey(name: string, key: string, allowPlaintext: boolean): Promise<KeyBackend | null> {
    try {
      return await setProviderKey(name, key, allowPlaintext);
    } catch (e) {
      if (isKeychainUnavailable(e)) {
        plaintextPrompt = { name, key };
        return null;
      }
      throw e;
    }
  }

  async function saveDraft(): Promise<void> {
    if (!draft) return;
    const name = draft.name.trim();
    const baseUrl = draft.baseUrl.trim();
    if (!name) { providersError = 'Profile name is required'; return; }
    if (!baseUrl) { providersError = 'Base URL is required'; return; }

    savingProvider = true;
    providersError = null;
    try {
      const profile: ProviderProfile = {
        name,
        baseUrl,
        defaultModel: draft.defaultModel.trim() || undefined,
        keyBackend: 'none',
      };
      await saveProviderProfile(profile);

      // Only send a key when the user actually entered one: always in add mode,
      // and in edit mode only when they chose to replace it.
      const wantKey = draft.mode === 'add' || draft.replacingKey ? draft.keyInput : '';
      if (wantKey.trim() !== '') {
        const backend = await applyKey(name, wantKey, false);
        if (backend === null) {
          // Keychain unavailable — leave the editor open on the opt-in prompt.
          savingProvider = false;
          return;
        }
      }
      await loadProviders();
      draft = null;
      showSaved('Provider saved');
    } catch (e) {
      providersError = e instanceof Error ? e.message : String(e);
    } finally {
      savingProvider = false;
    }
  }

  async function confirmPlaintext(): Promise<void> {
    if (!plaintextPrompt) return;
    savingProvider = true;
    providersError = null;
    try {
      await setProviderKey(plaintextPrompt.name, plaintextPrompt.key, true);
      plaintextPrompt = null;
      await loadProviders();
      draft = null;
      showSaved('Provider saved (plaintext)');
    } catch (e) {
      providersError = e instanceof Error ? e.message : String(e);
    } finally {
      savingProvider = false;
    }
  }

  function cancelPlaintext(): void {
    plaintextPrompt = null;
  }

  // Inline delete confirm (mirrors BrowseView's confirmPendingPath pattern —
  // a pending-name state + inline Confirm/Cancel, not a window.confirm dialog).
  let pendingDeleteProvider = $state<string | null>(null);
  function requestRemoveProvider(name: string): void {
    providersError = null;
    pendingDeleteProvider = name;
  }
  function cancelRemoveProvider(): void {
    pendingDeleteProvider = null;
  }
  async function confirmRemoveProvider(): Promise<void> {
    const name = pendingDeleteProvider;
    if (!name) return;
    pendingDeleteProvider = null;
    try {
      await deleteProviderProfile(name);
      if (draft && draft.name === name) draft = null;
      await loadProviders();
      showSaved('Provider deleted');
    } catch (e) {
      providersError = e instanceof Error ? e.message : String(e);
    }
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

    <!-- ── Provider profiles (issue #21) ────────────────────────────────── -->
    <fieldset class="group">
      <legend>Provider profiles</legend>
      <p class="hint">
        Named alternate providers (e.g. DeepSeek). Right-click Resume / Fork to launch a session against
        one — it injects <code>ANTHROPIC_BASE_URL</code>, <code>ANTHROPIC_AUTH_TOKEN</code>, and
        (if set) <code>ANTHROPIC_MODEL</code>. The API key is stored in your OS keychain and never
        written to the profiles file.
      </p>

      {#if keychainAvailable === false}
        <p class="notice notice--warn">
          ⚠ No OS keychain / Secret Service detected on this machine. You can still save a key, but only
          via the explicit plaintext fallback (you'll be asked to confirm).
        </p>
      {/if}

      {#if providersError}
        <p class="notice notice--error">{providersError}</p>
      {/if}

      {#if profiles.length === 0}
        <p class="hint">No provider profiles yet.</p>
      {:else}
        <ul class="provider-list">
          {#each profiles as p (p.name)}
            {@const badge = badgeFor(p)}
            <li class="provider-row">
              <div class="provider-main">
                <span class="provider-name">{p.name}</span>
                <span class="provider-url">{p.baseUrl}</span>
                {#if p.defaultModel}<span class="provider-model">{p.defaultModel}</span>{/if}
              </div>
              <div class="provider-actions">
                {#if pendingDeleteProvider === p.name}
                  <span class="hint-inline">Delete profile & its key?</span>
                  <button class="btn btn--sm btn--danger" type="button" onclick={confirmRemoveProvider}>Confirm</button>
                  <button class="btn btn--ghost btn--sm" type="button" onclick={cancelRemoveProvider}>Cancel</button>
                {:else}
                  <span class="badge {badge.cls}" title={badge.label}>{badge.icon} {badge.label}</span>
                  <button class="btn btn--ghost btn--sm" type="button" onclick={() => startEdit(p)}>Edit</button>
                  <button class="btn btn--ghost btn--sm" type="button" onclick={() => requestRemoveProvider(p.name)}>Delete</button>
                {/if}
              </div>
            </li>
          {/each}
        </ul>
      {/if}

      {#if !draft}
        <div>
          <button class="btn btn--ghost btn--sm" type="button" onclick={startAdd}>+ Add provider</button>
        </div>
      {:else}
        <div class="provider-editor">
          <div class="field">
            <label for="provider-name">Name {#if draft.mode === 'edit'}<span class="hint-inline">(immutable)</span>{/if}</label>
            <input
              id="provider-name"
              type="text"
              class="text-input"
              bind:value={draft.name}
              readonly={draft.mode === 'edit'}
              placeholder="e.g. DeepSeek"
            />
          </div>

          <div class="field">
            <label for="provider-url">Base URL</label>
            <input
              id="provider-url"
              type="text"
              class="text-input"
              bind:value={draft.baseUrl}
              placeholder="https://api.deepseek.com/anthropic"
            />
          </div>

          <div class="field">
            <label for="provider-model">Default model <span class="hint-inline">(optional)</span></label>
            <input
              id="provider-model"
              type="text"
              class="text-input"
              bind:value={draft.defaultModel}
              placeholder="e.g. deepseek-chat"
            />
          </div>

          <div class="field">
            <label for="provider-key">API key</label>
            {#if draft.mode === 'edit' && keyStatus[draft.name] && !draft.replacingKey}
              <div class="key-set-row">
                <span class="key-set">•••• set ✓</span>
                <button class="btn btn--ghost btn--sm" type="button" onclick={() => (draft!.replacingKey = true)}>
                  Replace key
                </button>
              </div>
            {:else}
              <input
                id="provider-key"
                type="password"
                class="text-input"
                bind:value={draft.keyInput}
                autocomplete="off"
                placeholder={draft.mode === 'edit' ? 'Paste a new key to replace the stored one' : 'Paste your provider API key'}
              />
              <p class="hint">Write-only: the key is saved to your keychain and never shown again.</p>
            {/if}
          </div>

          {#if plaintextPrompt}
            <div class="notice notice--warn plaintext-optin">
              <p><strong>You're the 1% outlier.</strong> This machine has no usable OS keychain / Secret Service,
                so the key can't be stored securely.</p>
              <p class="hint">
                Fix (recommended): install/start a Secret Service (e.g. GNOME Keyring / KWallet), then retry —
                the key will go to the keychain. Or, if you understand the risk, store it in plaintext at
                <code>~/.claude/.ccstudio-providers-plaintext.json</code> (owner-readable, 0600) instead.
              </p>
              <div class="provider-editor-actions">
                <button class="btn btn--ghost btn--sm" type="button" onclick={cancelPlaintext} disabled={savingProvider}>
                  Cancel
                </button>
                <button class="btn btn--sm btn--danger" type="button" onclick={confirmPlaintext} disabled={savingProvider}>
                  Store in plaintext anyway
                </button>
              </div>
            </div>
          {:else}
            <div class="provider-editor-actions">
              <button class="btn btn--ghost btn--sm" type="button" onclick={cancelDraft} disabled={savingProvider}>
                Cancel
              </button>
              <button class="btn btn--primary btn--sm" type="button" onclick={saveDraft} disabled={savingProvider}>
                {draft.mode === 'add' ? 'Add profile' : 'Save profile'}
              </button>
            </div>
          {/if}
        </div>
      {/if}
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

  /* Provider profiles (issue #21) */
  .hint-inline { font-size: 0.72rem; color: var(--text-faint); font-weight: 400; }

  .notice { font-size: 0.76rem; margin: 0; padding: 0.4rem 0.55rem; border-radius: 0.35rem; }
  .notice--warn { color: var(--text); background: color-mix(in srgb, orange 14%, transparent); border: 1px solid color-mix(in srgb, orange 40%, transparent); }
  .notice--error { color: var(--text); background: color-mix(in srgb, red 12%, transparent); border: 1px solid color-mix(in srgb, red 40%, transparent); }

  .provider-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 0.35rem; }
  .provider-row {
    display: flex; align-items: center; justify-content: space-between; gap: 0.5rem;
    padding: 0.4rem 0.5rem; border: 1px solid var(--border); border-radius: 0.35rem; background: var(--bg-card);
  }
  .provider-main { display: flex; flex-direction: column; gap: 0.1rem; min-width: 0; }
  .provider-name { font-size: 0.82rem; font-weight: 600; color: var(--text); }
  .provider-url { font-size: 0.72rem; color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .provider-model { font-size: 0.7rem; color: var(--text-faint); font-family: var(--font-mono, monospace); }
  .provider-actions { display: flex; align-items: center; gap: 0.35rem; flex-shrink: 0; }

  .badge { font-size: 0.68rem; padding: 0.12rem 0.4rem; border-radius: 0.5rem; white-space: nowrap; border: 1px solid var(--border-strong); }
  .badge--keychain { color: var(--text); }
  .badge--plaintext { color: var(--text); background: color-mix(in srgb, orange 16%, transparent); border-color: color-mix(in srgb, orange 45%, transparent); }
  .badge--none { color: var(--text-faint); }

  .provider-editor { display: flex; flex-direction: column; gap: 0.5rem; padding: 0.5rem; border: 1px dashed var(--border-strong); border-radius: 0.4rem; }
  .provider-editor-actions { display: flex; justify-content: flex-end; gap: 0.5rem; }
  .key-set-row { display: flex; align-items: center; gap: 0.5rem; }
  .key-set { font-size: 0.8rem; color: var(--text-muted); font-family: var(--font-mono, monospace); }
  .plaintext-optin { display: flex; flex-direction: column; gap: 0.4rem; }
  .plaintext-optin p { margin: 0; }
</style>
