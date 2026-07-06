<script lang="ts">
  /**
   * SettingsView.svelte — schema-driven Claude Code settings editor.
   *
   * Reads user/project/local settings.json tiers, renders each field from the
   * vendored Claude Code settings schema (label + description + typed input),
   * and surfaces conflicts (same key, different value, in >1 tier) up front.
   * Simple/Advanced toggle keeps the ~120-property schema from overwhelming the
   * common case. Global scope (no project) also shows CC Deck's own Terminal
   * launcher preference, since that isn't a per-project Claude Code setting.
   */
  import { onMount } from 'svelte';
  import type { ClaudeSettings, SettingsTier, AppConfig } from '$lib/types';
  import { readClaudeSettings, writeClaudeSettings, getAppConfig, setAppConfig } from '$lib/api';
  import schema from '$lib/schema/claude-code-settings.json';

  let {
    projectCwd = null,
    projectLabel = '',
    onClose = () => {},
  }: { projectCwd?: string | null; projectLabel?: string; onClose?: () => void } = $props();

  const SCHEMA_PROPS = (schema as { properties?: Record<string, SchemaProp> }).properties ?? {};

  interface SchemaProp {
    type?: string | string[];
    description?: string;
    enum?: unknown[];
    default?: unknown;
    items?: { type?: string };
  }

  // Curated "Simple" grouping — the long tail of ~100 remaining schema keys is
  // still fully editable, just tucked behind Advanced.
  const SIMPLE_GROUPS: { label: string; keys: string[] }[] = [
    { label: 'Model', keys: ['model', 'fallbackModel', 'effortLevel', 'fastMode', 'alwaysThinkingEnabled', 'availableModels'] },
    { label: 'Permissions & sandbox', keys: ['permissions', 'sandbox'] },
    { label: 'Environment', keys: ['env', 'cleanupPeriodDays', 'respectGitignore', 'claudeMdExcludes'] },
    { label: 'Git & pull requests', keys: ['includeGitInstructions', 'attribution', 'prUrlTemplate'] },
    { label: 'Hooks', keys: ['hooks', 'disableAllHooks'] },
    { label: 'MCP servers', keys: ['enableAllProjectMcpServers', 'enabledMcpjsonServers', 'disabledMcpjsonServers'] },
    { label: 'Interface', keys: ['theme', 'language', 'outputStyle', 'editorMode', 'statusLine', 'verbose', 'showTurnDuration'] },
  ];
  const SIMPLE_KEYS = new Set(SIMPLE_GROUPS.flatMap((g) => g.keys));
  const ADVANCED_KEYS = Object.keys(SCHEMA_PROPS)
    .filter((k) => k !== '$schema' && !SIMPLE_KEYS.has(k))
    .sort((a, b) => a.localeCompare(b));

  // ── settings state ─────────────────────────────────────────────────────────
  let settings = $state<ClaudeSettings | null>(null);
  let loading = $state(true);
  let loadError = $state<string | null>(null);
  let selectedTier = $state<SettingsTier>('user');
  // Per-tier working copies so switching tabs doesn't lose unsaved edits.
  let working = $state<Record<string, Record<string, unknown>>>({});
  let original = $state<Record<string, string>>({}); // JSON snapshot per tier, for dirty-check
  let showAdvanced = $state(false);
  let highlightKey = $state<string | null>(null);
  let fieldErrors = $state<Record<string, string>>({});
  let saveMsg = $state<string | null>(null);
  let saveMsgTimer: ReturnType<typeof setTimeout> | null = null;

  const TIER_LABEL: Record<SettingsTier, string> = {
    local: 'Local (this machine)',
    project: 'Project (shared)',
    user: 'User (global)',
  };

  async function load(): Promise<void> {
    loading = true;
    loadError = null;
    try {
      settings = await readClaudeSettings(projectCwd);
      const w: Record<string, Record<string, unknown>> = {};
      const o: Record<string, string> = {};
      for (const t of settings.tiers) {
        w[t.tier] = { ...(t.parsed ?? {}) };
        o[t.tier] = JSON.stringify(t.parsed ?? {});
      }
      working = w;
      original = o;
      fieldErrors = {};
      if (!settings.tiers.some((t) => t.tier === selectedTier)) {
        selectedTier = settings.tiers[0]?.tier ?? 'user';
      }
    } catch (e) {
      loadError = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  onMount(load);

  function isDirty(tier: string): boolean {
    return JSON.stringify(working[tier] ?? {}) !== (original[tier] ?? '{}');
  }

  function fieldValue(key: string): unknown {
    return working[selectedTier]?.[key];
  }
  function isSet(key: string): boolean {
    return !!working[selectedTier] && Object.prototype.hasOwnProperty.call(working[selectedTier], key);
  }
  function setField(key: string, value: unknown): void {
    if (!working[selectedTier]) working[selectedTier] = {};
    working[selectedTier][key] = value;
    const { [key]: _drop, ...rest } = fieldErrors;
    fieldErrors = rest;
  }
  function clearField(key: string): void {
    if (!working[selectedTier]) return;
    delete working[selectedTier][key];
  }
  function setFieldError(key: string, msg: string): void {
    fieldErrors = { ...fieldErrors, [key]: msg };
  }

  // ── widget kind per JSON-Schema property ────────────────────────────────────
  type Kind = 'boolean' | 'enum' | 'string' | 'number' | 'stringArray' | 'json';
  function widgetKind(key: string): Kind {
    const p = SCHEMA_PROPS[key];
    if (!p) return 'json';
    if (Array.isArray(p.enum)) return 'enum';
    if (p.type === 'boolean') return 'boolean';
    if (p.type === 'string') return 'string';
    if (p.type === 'integer' || p.type === 'number') return 'number';
    if (p.type === 'array' && (!p.items || p.items.type === 'string')) return 'stringArray';
    return 'json';
  }

  function jsonText(key: string): string {
    const v = fieldValue(key);
    return v === undefined ? '' : JSON.stringify(v, null, 2);
  }
  function onJsonInput(key: string, text: string): void {
    if (text.trim() === '') {
      clearField(key);
      return;
    }
    try {
      setField(key, JSON.parse(text));
    } catch {
      setFieldError(key, 'Invalid JSON');
    }
  }
  function stringArrayText(key: string): string {
    const v = fieldValue(key);
    return Array.isArray(v) ? v.join(', ') : '';
  }
  function onStringArrayInput(key: string, text: string): void {
    if (text.trim() === '') {
      clearField(key);
      return;
    }
    setField(
      key,
      text.split(',').map((s) => s.trim()).filter(Boolean)
    );
  }

  // ── conflicts: jump-to-field ─────────────────────────────────────────────
  function jumpTo(key: string, winner: SettingsTier): void {
    selectedTier = winner;
    highlightKey = key;
    setTimeout(() => {
      document.getElementById(`field-${key}`)?.scrollIntoView({ behavior: 'smooth', block: 'center' });
    }, 30);
  }

  function showSaved(msg: string): void {
    saveMsg = msg;
    if (saveMsgTimer) clearTimeout(saveMsgTimer);
    saveMsgTimer = setTimeout(() => { saveMsg = null; saveMsgTimer = null; }, 2500);
  }

  async function save(): Promise<void> {
    if (Object.keys(fieldErrors).length > 0) return;
    try {
      await writeClaudeSettings(selectedTier, projectCwd, working[selectedTier] ?? {});
      showSaved('Saved');
      await load();
    } catch (e) {
      loadError = e instanceof Error ? e.message : String(e);
    }
  }

  function discard(): void {
    const t = settings?.tiers.find((t) => t.tier === selectedTier);
    working[selectedTier] = { ...(t?.parsed ?? {}) };
    fieldErrors = {};
  }

  // ── CC Deck's own terminal preference (global scope only) ──────────────────
  let appConfig = $state<AppConfig>({ terminal: '', terminalArgs: '' });
  let appConfigDirty = $state(false);
  let showTerminalAdvanced = $state(false);

  async function loadAppConfig(): Promise<void> {
    appConfig = await getAppConfig();
    appConfigDirty = false;
  }
  function isAuto(t: string): boolean {
    const s = t.trim().toLowerCase();
    return s === '' || s === 'auto';
  }
  async function saveAppConfig(): Promise<void> {
    await setAppConfig(appConfig);
    appConfigDirty = false;
    showSaved('Saved');
  }

  $effect(() => {
    if (!projectCwd) loadAppConfig();
  });
</script>

<div class="settings-view">
  <div class="settings-head">
    <div>
      <h2>Settings</h2>
      <div class="scope">{projectCwd ? `Project — ${projectLabel || projectCwd}` : 'User (global)'}</div>
    </div>
    <button class="btn btn--ghost btn--sm" onclick={onClose} type="button">Close</button>
  </div>

  {#if loading}
    <div class="empty-state">Loading settings…</div>
  {:else if loadError}
    <div class="empty-state">{loadError}</div>
  {:else if settings}
    <!-- ── Conflict banner ──────────────────────────────────────────────── -->
    {#if settings.conflicts.length > 0}
      <div class="conflicts">
        <div class="conflicts__title">⚠ {settings.conflicts.length} setting{settings.conflicts.length === 1 ? '' : 's'} set in more than one place</div>
        {#each settings.conflicts as c (c.key)}
          <button class="conflict-row" onclick={() => jumpTo(c.key, c.winner)} type="button">
            <code>{c.key}</code>
            <span class="conflict-detail">
              {c.tierValues.map((tv) => `${TIER_LABEL[tv.tier]}: ${JSON.stringify(tv.value)}`).join('  ·  ')}
              — <strong>{TIER_LABEL[c.winner]} wins</strong>
            </span>
          </button>
        {/each}
      </div>
    {/if}

    <!-- ── Tier switcher ─────────────────────────────────────────────────── -->
    <div class="tier-tabs">
      {#each settings.tiers as t (t.tier)}
        <button
          class="tier-tab" class:on={selectedTier === t.tier}
          onclick={() => (selectedTier = t.tier)} type="button">
          {TIER_LABEL[t.tier]}
          {#if isDirty(t.tier)}<span class="dot" title="Unsaved changes"></span>{/if}
        </button>
      {/each}
    </div>
    <div class="tier-path" title={settings.tiers.find((t) => t.tier === selectedTier)?.path}>
      {settings.tiers.find((t) => t.tier === selectedTier)?.path}
      {#if !settings.tiers.find((t) => t.tier === selectedTier)?.exists}
        <span class="muted">(doesn't exist yet — saving will create it)</span>
      {/if}
    </div>

    <!-- ── Simple/Advanced ───────────────────────────────────────────────── -->
    <label class="advanced-toggle">
      <input type="checkbox" bind:checked={showAdvanced} />
      Show advanced settings ({ADVANCED_KEYS.length})
    </label>

    <!-- ── Form ──────────────────────────────────────────────────────────── -->
    <div class="form">
      {#each SIMPLE_GROUPS as group (group.label)}
        {@const keys = group.keys.filter((k) => SCHEMA_PROPS[k])}
        {#if keys.length > 0}
          <fieldset class="group">
            <legend>{group.label}</legend>
            {#each keys as key (key)}
              {@render field(key)}
            {/each}
          </fieldset>
        {/if}
      {/each}

      {#if showAdvanced}
        <fieldset class="group">
          <legend>Advanced</legend>
          {#each ADVANCED_KEYS as key (key)}
            {@render field(key)}
          {/each}
        </fieldset>
      {/if}
    </div>

    <!-- ── Save bar ──────────────────────────────────────────────────────── -->
    <div class="save-bar">
      <button class="btn btn--ghost btn--sm" onclick={discard} disabled={!isDirty(selectedTier)} type="button">
        Discard changes
      </button>
      <button class="btn btn--primary btn--sm" onclick={save}
        disabled={!isDirty(selectedTier) || Object.keys(fieldErrors).length > 0} type="button">
        Save to {TIER_LABEL[selectedTier]}
      </button>
    </div>

    <!-- ── Terminal launcher (global scope only) ────────────────────────── -->
    {#if !projectCwd}
      <fieldset class="group terminal-group">
        <legend>Terminal</legend>
        <p class="hint">How CC Deck launches Claude Code when you click Resume. Automatic works for most setups.</p>
        <label class="radio-row">
          <input type="radio" name="terminal-mode" checked={isAuto(appConfig.terminal)}
            onchange={() => { appConfig.terminal = ''; appConfigDirty = true; }} />
          Automatic (recommended)
        </label>
        <label class="radio-row">
          <input type="radio" name="terminal-mode" checked={!isAuto(appConfig.terminal)}
            onchange={() => { if (isAuto(appConfig.terminal)) appConfig.terminal = 'gnome-terminal --'; appConfigDirty = true; }} />
          Custom
        </label>

        {#if !isAuto(appConfig.terminal)}
          <div class="field">
            <label for="terminal-cmd">Terminal command</label>
            <input id="terminal-cmd" type="text" class="text-input"
              bind:value={appConfig.terminal}
              oninput={() => (appConfigDirty = true)}
              placeholder="e.g. gnome-terminal --, konsole -e, iTerm, wt" />
          </div>
        {/if}

        <button class="advanced-link" onclick={() => (showTerminalAdvanced = !showTerminalAdvanced)} type="button">
          {showTerminalAdvanced ? '▾' : '▸'} Advanced: extra arguments
        </button>
        {#if showTerminalAdvanced}
          <div class="field">
            <label for="terminal-args">Extra arguments passed to <code>claude</code></label>
            <input id="terminal-args" type="text" class="text-input"
              bind:value={appConfig.terminalArgs}
              oninput={() => (appConfigDirty = true)}
              placeholder="e.g. --dangerously-skip-permissions" />
            <p class="caution">⚠ Only add flags you understand — some (like <code>--dangerously-skip-permissions</code>) remove normal safety prompts.</p>
          </div>
        {/if}

        <div class="save-bar">
          <button class="btn btn--primary btn--sm" onclick={saveAppConfig} disabled={!appConfigDirty} type="button">
            Save terminal preference
          </button>
        </div>
      </fieldset>
    {/if}
  {/if}

  {#if saveMsg}
    <div class="toast" role="status">{saveMsg}</div>
  {/if}
</div>

{#snippet field(key: string)}
  {@const prop = SCHEMA_PROPS[key]}
  {@const kind = widgetKind(key)}
  <div class="field" id="field-{key}" class:highlight={highlightKey === key}>
    <div class="field-head">
      <label for="f-{key}"><code>{key}</code></label>
      {#if isSet(key)}
        <button class="clear" onclick={() => clearField(key)} type="button" title="Remove from this tier">clear</button>
      {:else}
        <span class="unset">not set here</span>
      {/if}
    </div>
    {#if prop?.description}
      <p class="desc">{prop.description}</p>
    {/if}

    {#if kind === 'boolean'}
      <label class="bool-row">
        <input id="f-{key}" type="checkbox"
          checked={fieldValue(key) === true}
          onchange={(e) => setField(key, e.currentTarget.checked)} />
        Enabled
      </label>
    {:else if kind === 'enum'}
      <select id="f-{key}" class="text-input"
        value={isSet(key) ? String(fieldValue(key)) : ''}
        onchange={(e) => { const v = e.currentTarget.value; if (v === '') clearField(key); else setField(key, v); }}>
        <option value="">— not set —</option>
        {#each prop?.enum ?? [] as opt}
          <option value={String(opt)}>{String(opt)}</option>
        {/each}
      </select>
    {:else if kind === 'string'}
      <input id="f-{key}" type="text" class="text-input"
        value={isSet(key) ? String(fieldValue(key) ?? '') : ''}
        oninput={(e) => { const v = e.currentTarget.value; if (v === '') clearField(key); else setField(key, v); }} />
    {:else if kind === 'number'}
      <input id="f-{key}" type="number" class="text-input"
        value={isSet(key) ? String(fieldValue(key)) : ''}
        oninput={(e) => { const v = e.currentTarget.value; if (v === '') clearField(key); else setField(key, Number(v)); }} />
    {:else if kind === 'stringArray'}
      <input id="f-{key}" type="text" class="text-input"
        value={stringArrayText(key)}
        oninput={(e) => onStringArrayInput(key, e.currentTarget.value)}
        placeholder="comma-separated" />
    {:else}
      <textarea id="f-{key}" class="json-input" rows="3"
        value={jsonText(key)}
        oninput={(e) => onJsonInput(key, e.currentTarget.value)}></textarea>
    {/if}
    {#if fieldErrors[key]}
      <p class="field-error">{fieldErrors[key]}</p>
    {/if}
  </div>
{/snippet}

<style>
  .settings-view { display: flex; flex-direction: column; gap: 0.75rem; }
  .settings-head { display: flex; align-items: center; justify-content: space-between; }
  .settings-head h2 { margin: 0; font-size: 1.05rem; }
  .scope { font-size: 0.72rem; color: var(--text-faint); text-transform: uppercase; letter-spacing: 0.06em; }

  .conflicts {
    border: 1px solid color-mix(in srgb, var(--accent-result-err) 45%, var(--border));
    background: color-mix(in srgb, var(--accent-result-err) 8%, transparent);
    border-radius: 0.4rem; padding: 0.5rem 0.65rem; display: flex; flex-direction: column; gap: 0.3rem;
  }
  .conflicts__title { font-size: 0.8rem; font-weight: 600; }
  .conflict-row {
    display: flex; flex-direction: column; gap: 0.1rem; text-align: left;
    background: transparent; border: none; padding: 0.2rem 0; cursor: pointer; color: inherit;
  }
  .conflict-row:hover .conflict-detail { color: var(--text); }
  .conflict-row code { font-size: 0.78rem; }
  .conflict-detail { font-size: 0.72rem; color: var(--text-muted); }

  .tier-tabs { display: flex; gap: 0.35rem; flex-wrap: wrap; }
  .tier-tab {
    font-size: 0.76rem; padding: 0.3rem 0.65rem; border-radius: 999px;
    background: var(--bg-subtle); color: var(--text-muted); border: 1px solid var(--border); cursor: pointer;
    display: inline-flex; align-items: center; gap: 0.3rem;
  }
  .tier-tab.on { background: color-mix(in srgb, var(--accent-user) 20%, transparent); color: var(--text); border-color: color-mix(in srgb, var(--accent-user) 45%, transparent); }
  .tier-tab .dot { width: 0.4rem; height: 0.4rem; border-radius: 50%; background: var(--accent-result-err); }

  .tier-path { font-size: 0.7rem; color: var(--text-faint); font-family: var(--font-mono, monospace); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tier-path .muted { color: var(--text-faint); font-style: italic; }

  .advanced-toggle { display: flex; align-items: center; gap: 0.4rem; font-size: 0.78rem; color: var(--text-muted); cursor: pointer; }

  .form { display: flex; flex-direction: column; gap: 0.75rem; max-height: 55vh; overflow-y: auto; padding-right: 0.25rem; }
  .group { border: 1px solid var(--border); border-radius: 0.45rem; padding: 0.6rem 0.75rem; display: flex; flex-direction: column; gap: 0.6rem; }
  .group legend { font-size: 0.78rem; font-weight: 600; color: var(--text); padding: 0 0.3rem; }

  .field { display: flex; flex-direction: column; gap: 0.25rem; border-radius: 0.3rem; padding: 0.2rem 0.3rem; }
  .field.highlight { background: color-mix(in srgb, var(--accent-user) 16%, transparent); outline: 1px solid color-mix(in srgb, var(--accent-user) 45%, transparent); }
  .field-head { display: flex; align-items: center; justify-content: space-between; gap: 0.5rem; }
  .field-head code { font-size: 0.8rem; }
  .field-head .unset { font-size: 0.68rem; color: var(--text-faint); font-style: italic; }
  .clear { font-size: 0.68rem; color: var(--text-faint); background: transparent; border: none; cursor: pointer; text-decoration: underline; }
  .clear:hover { color: var(--text-muted); }
  .desc { font-size: 0.74rem; color: var(--text-muted); margin: 0; }

  .text-input, .json-input {
    width: 100%; font-size: 0.8rem; padding: 0.35rem 0.5rem;
    background: var(--bg-card); color: var(--text); border: 1px solid var(--border-strong); border-radius: 0.35rem;
    font-family: inherit; box-sizing: border-box;
  }
  .json-input { font-family: var(--font-mono, monospace); resize: vertical; }
  .bool-row, .radio-row { display: flex; align-items: center; gap: 0.4rem; font-size: 0.8rem; }
  .field-error { font-size: 0.72rem; color: var(--accent-result-err); margin: 0; }

  .save-bar { display: flex; justify-content: flex-end; gap: 0.5rem; }

  .terminal-group .hint { font-size: 0.76rem; color: var(--text-muted); margin: 0 0 0.2rem; }
  .caution { font-size: 0.72rem; color: var(--accent-result-err); }
  .advanced-link { align-self: flex-start; font-size: 0.76rem; color: var(--text-muted); background: transparent; border: none; cursor: pointer; padding: 0; }
  .advanced-link:hover { color: var(--text); }
</style>
