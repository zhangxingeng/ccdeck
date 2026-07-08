<script lang="ts">
  /**
   * SettingsSearchView.svelte — search-and-popover editor for Claude Code's own
   * settings.json (schema-driven; never CC Deck's own App Config).
   *
   * Replaces the deleted SettingsView.svelte's always-everything-visible ~125-
   * field form (issue #18) with a much smaller surface: a fuzzy search box over
   * the schema's top-level properties, a capped candidate list, and a popover
   * that edits exactly one field at a time (issue #20). The backend/schema/
   * types this reads (read_claude_settings/write_claude_settings, the vendored
   * schema, ClaudeSettings/SettingsTier) are unchanged from before #18 — only
   * this frontend is new.
   */
  import { onMount, onDestroy } from 'svelte';
  import type { ClaudeSettings, SettingsTier, SettingsTierData } from '$lib/types';
  import { readClaudeSettings, writeClaudeSettings, isSettingsConflict } from '$lib/api';
  import schema from '$lib/schema/claude-code-settings.json';

  let {
    projectCwd = null,
    projectLabel = '',
    onClose = () => {},
  }: { projectCwd?: string | null; projectLabel?: string; onClose?: () => void } = $props();

  interface SchemaProp {
    type?: string | string[];
    description?: string;
    enum?: unknown[];
    default?: unknown;
    items?: { type?: string };
  }

  const SCHEMA_PROPS = (schema as { properties?: Record<string, SchemaProp> }).properties ?? {};
  const ALL_KEYS = Object.keys(SCHEMA_PROPS).filter((k) => k !== '$schema');

  const TIER_LABEL: Record<SettingsTier, string> = {
    local: 'Local',
    project: 'Workspace',
    user: 'User',
  };

  // ── data load (once on mount) ────────────────────────────────────────────
  let settings = $state<ClaudeSettings | null>(null);
  let loading = $state(true);
  let loadError = $state<string | null>(null);

  async function load(): Promise<void> {
    loading = true;
    loadError = null;
    try {
      settings = await readClaudeSettings(projectCwd);
    } catch (e) {
      loadError = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  onMount(load);

  function tierData(tier: SettingsTier): SettingsTierData | undefined {
    return settings?.tiers.find((t) => t.tier === tier);
  }

  function isSetAnywhere(key: string): boolean {
    return (
      settings?.tiers.some((t) => t.parsed && Object.prototype.hasOwnProperty.call(t.parsed, key)) ?? false
    );
  }

  // ── search + candidate list ──────────────────────────────────────────────
  let query = $state('');

  interface Candidate {
    key: string;
    desc: string;
  }

  const CANDIDATE_CAP = 30;

  let candidates = $derived.by((): Candidate[] => {
    const q = query.trim().toLowerCase();
    if (!q) return [];
    const scored: { key: string; desc: string; score: number }[] = [];
    for (const key of ALL_KEYS) {
      const desc = SCHEMA_PROPS[key]?.description ?? '';
      const keyLower = key.toLowerCase();
      let score: number;
      if (keyLower.includes(q)) {
        score = 1000 - keyLower.indexOf(q); // key match ranks highest
      } else if (desc.toLowerCase().includes(q)) {
        score = 500 - desc.toLowerCase().indexOf(q); // description match next
      } else {
        continue; // no match — excluded
      }
      scored.push({ key, desc, score });
    }
    scored.sort((a, b) => b.score - a.score || a.key.localeCompare(b.key));
    return scored.slice(0, CANDIDATE_CAP).map(({ key, desc }) => ({ key, desc }));
  });

  // ── popover: exactly one field at a time ─────────────────────────────────
  let selectedKey = $state<string | null>(null);
  let popTier = $state<SettingsTier>('user');
  let popValue = $state<unknown>(undefined);
  let popIsSet = $state<boolean>(false);
  let popFieldError = $state<string | null>(null);
  let popConflictMsg = $state<string | null>(null);

  let availableTiers = $derived<SettingsTier[]>(projectCwd ? ['local', 'project', 'user'] : ['user']);

  function syncPopValue(): void {
    const t = tierData(popTier);
    const has = !!t?.parsed && Object.prototype.hasOwnProperty.call(t.parsed, selectedKey ?? '');
    popIsSet = has;
    popValue = has ? (t!.parsed as Record<string, unknown>)[selectedKey!] : undefined;
    popFieldError = null;
  }

  let popoverEl = $state<HTMLDivElement | undefined>(undefined);

  function openPopover(key: string): void {
    selectedKey = key;
    popConflictMsg = null;
    // Default to the highest-precedence applicable tier for this scope.
    popTier = availableTiers[0] ?? 'user';
    syncPopValue();
  }

  // Focus the popover itself on open so Escape works immediately (the
  // backdrop's keydown handler only fires while something inside has focus)
  // instead of only after the user tabs/clicks into a field first.
  $effect(() => {
    if (selectedKey && popoverEl) popoverEl.focus();
  });

  // If the selected tier's on-disk JSON currently fails to parse, the backend
  // still returns it (as `parsed: null` + this message) so the tier stays
  // pickable, but editing it here must not proceed: `writeTier` below seeds
  // its write from `parsed`, and `null`/`{}` for an unparseable file would
  // silently discard everything else in it. Save/Clear are disabled while
  // this is set — see `writeTier`'s own guard for the non-UI backstop.
  let popTierParseError = $derived(tierData(popTier)?.parseError ?? null);

  function selectPopTier(tier: SettingsTier): void {
    popTier = tier;
    popConflictMsg = null;
    syncPopValue();
  }

  function closePopover(): void {
    selectedKey = null;
    popFieldError = null;
    popConflictMsg = null;
  }

  // ── widget kind per JSON-Schema property (same mapping SettingsView used) ──
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

  function setPopValue(v: unknown): void {
    popValue = v;
    popIsSet = true;
    popFieldError = null;
  }
  function clearPopValueLocal(): void {
    popValue = undefined;
    popIsSet = false;
    popFieldError = null;
  }

  function jsonText(): string {
    return popValue === undefined ? '' : JSON.stringify(popValue, null, 2);
  }
  function onJsonInput(text: string): void {
    if (text.trim() === '') {
      clearPopValueLocal();
      return;
    }
    try {
      setPopValue(JSON.parse(text));
    } catch {
      popFieldError = 'Invalid JSON';
    }
  }
  function stringArrayText(): string {
    return Array.isArray(popValue) ? popValue.join(', ') : '';
  }
  function onStringArrayInput(text: string): void {
    if (text.trim() === '') {
      clearPopValueLocal();
      return;
    }
    setPopValue(
      text.split(',').map((s) => s.trim()).filter(Boolean)
    );
  }

  // "Also set in <tier>: <value>" — reuses the backend's own conflicts array;
  // only shown when another tier sets this key to a *different* value.
  let otherTierHint = $derived.by((): string | null => {
    if (!settings || !selectedKey) return null;
    const conflict = settings.conflicts.find((c) => c.key === selectedKey);
    if (!conflict) return null;
    const other = conflict.tierValues.find((tv) => tv.tier !== popTier);
    if (!other) return null;
    return `Also set in ${TIER_LABEL[other.tier]}: ${JSON.stringify(other.value)}`;
  });

  let saveMsg = $state<string | null>(null);
  let saveMsgTimer: ReturnType<typeof setTimeout> | null = null;
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

  async function writeTier(mutate: (obj: Record<string, unknown>) => void): Promise<boolean> {
    if (!selectedKey) return false;
    const t = tierData(popTier);
    if (t?.parseError) {
      // Backstop for the disabled-button UI gate below: refuse rather than
      // spread `{...null}` and silently overwrite this tier's unparseable
      // file with just the one edited key.
      popFieldError = `${TIER_LABEL[popTier]} settings.json has invalid JSON — fix it by hand before editing here.`;
      return false;
    }
    const baseVersion = t?.raw ?? '';
    const nextObj: Record<string, unknown> = { ...(t?.parsed as Record<string, unknown> | undefined) };
    mutate(nextObj);
    try {
      await writeClaudeSettings(popTier, projectCwd, nextObj, baseVersion);
      await load();
      return true;
    } catch (e) {
      if (isSettingsConflict(e)) {
        popConflictMsg = `${TIER_LABEL[popTier]} settings changed on disk since this was loaded — reload before saving again.`;
      } else {
        loadError = e instanceof Error ? e.message : String(e);
      }
      return false;
    }
  }

  async function save(): Promise<void> {
    if (popFieldError || !selectedKey) return;
    const key = selectedKey;
    const ok = await writeTier((obj) => {
      if (popIsSet) obj[key] = popValue;
      else delete obj[key];
    });
    if (ok) {
      showSaved('Saved');
      closePopover();
    }
  }

  async function clearFromTier(): Promise<void> {
    if (!selectedKey) return;
    const key = selectedKey;
    const ok = await writeTier((obj) => {
      delete obj[key];
    });
    if (ok) {
      showSaved('Cleared');
      closePopover();
    }
  }

  async function reloadAfterConflict(): Promise<void> {
    popConflictMsg = null;
    await load();
    syncPopValue();
  }
</script>

<div class="settingssearch-view">
  <div class="settingssearch-head">
    <div>
      <h2>Settings</h2>
      <div class="scope">
        {projectCwd ? `Project — ${projectLabel || projectCwd}` : 'User (global)'}
      </div>
    </div>
    <button class="btn btn--ghost btn--sm" onclick={onClose} type="button">Close</button>
  </div>

  {#if loading}
    <div class="empty-state">Loading settings…</div>
  {:else if loadError}
    <div class="empty-state">{loadError}</div>
  {:else if settings}
    <input
      type="text"
      class="search-input"
      placeholder="Search settings — e.g. model, theme, permissions…"
      bind:value={query}
      aria-label="Search Claude Code settings"
    />

    {#if query.trim() === ''}
      <p class="hint">Start typing to find a setting by key or description.</p>
    {:else if candidates.length === 0}
      <div class="empty-state">No matching settings.</div>
    {:else}
      <ul class="candidates">
        {#each candidates as c (c.key)}
          <li>
            <button class="candidate-row" type="button" onclick={() => openPopover(c.key)}>
              <span class="candidate-row__main">
                <code>{c.key}</code>
                {#if isSetAnywhere(c.key)}<span class="dot" title="Set in at least one tier"></span>{/if}
              </span>
              <span class="candidate-row__desc">{c.desc}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}

  {#if saveMsg}
    <div class="toast" role="status">{saveMsg}</div>
  {/if}
</div>

<!-- ── Popover: exactly one field, never the whole schema ───────────────── -->
{#if selectedKey}
  {@const prop = SCHEMA_PROPS[selectedKey]}
  {@const kind = widgetKind(selectedKey)}
  <div
    class="popover-backdrop"
    onclick={(e) => { if (e.target === e.currentTarget) closePopover(); }}
    onkeydown={(e) => { if (e.key === 'Escape') closePopover(); }}
    role="presentation"
  >
    <div
      class="popover"
      role="dialog"
      aria-modal="true"
      aria-label="Edit {selectedKey}"
      tabindex="-1"
      bind:this={popoverEl}
    >
      <div class="popover__head">
        <code>{selectedKey}</code>
      </div>
      {#if prop?.description}
        <p class="popover__desc">{prop.description}</p>
      {/if}

      <div class="tier-radios" role="radiogroup" aria-label="Settings tier">
        {#each availableTiers as t (t)}
          <label class="radio-row">
            <input
              type="radio"
              name="pop-tier"
              checked={popTier === t}
              onchange={() => selectPopTier(t)}
            />
            {TIER_LABEL[t]}
          </label>
        {/each}
      </div>

      {#if otherTierHint}
        <p class="conflict-hint">{otherTierHint}</p>
      {/if}

      {#if popTierParseError}
        <p class="field-error">
          ⚠ {TIER_LABEL[popTier]} settings.json has invalid JSON, so it can't be safely edited here
          (editing would silently discard the rest of that file): {popTierParseError}
        </p>
      {/if}

      <fieldset class="popover-field" disabled={!!popTierParseError}>
        {#if kind === 'boolean'}
          <label class="bool-row">
            <input
              type="checkbox"
              checked={popValue === true}
              onchange={(e) => setPopValue(e.currentTarget.checked)}
            />
            Enabled
          </label>
        {:else if kind === 'enum'}
          <select
            class="text-input"
            value={popIsSet ? String(popValue) : ''}
            onchange={(e) => {
              const v = e.currentTarget.value;
              if (v === '') clearPopValueLocal();
              else setPopValue(v);
            }}
          >
            <option value="">— not set —</option>
            {#each prop?.enum ?? [] as opt}
              <option value={String(opt)}>{String(opt)}</option>
            {/each}
          </select>
        {:else if kind === 'string'}
          <input
            type="text"
            class="text-input"
            value={popIsSet ? String(popValue ?? '') : ''}
            oninput={(e) => {
              const v = e.currentTarget.value;
              if (v === '') clearPopValueLocal();
              else setPopValue(v);
            }}
          />
        {:else if kind === 'number'}
          <input
            type="number"
            class="text-input"
            value={popIsSet ? String(popValue) : ''}
            oninput={(e) => {
              const v = e.currentTarget.value;
              if (v === '') clearPopValueLocal();
              else setPopValue(Number(v));
            }}
          />
        {:else if kind === 'stringArray'}
          <input
            type="text"
            class="text-input"
            value={stringArrayText()}
            oninput={(e) => onStringArrayInput(e.currentTarget.value)}
            placeholder="comma-separated"
          />
        {:else}
          <textarea class="json-input" rows="4" value={jsonText()} oninput={(e) => onJsonInput(e.currentTarget.value)}
          ></textarea>
        {/if}
        {#if popFieldError}
          <p class="field-error">{popFieldError}</p>
        {/if}
      </fieldset>

      {#if popConflictMsg}
        <div class="conflicts">
          <div class="conflicts__title">⚠ Save refused — {popConflictMsg}</div>
          <button class="btn btn--ghost btn--sm" onclick={reloadAfterConflict} type="button">Reload</button>
        </div>
      {/if}

      <div class="popover-actions">
        <button
          class="btn btn--ghost btn--sm"
          onclick={clearFromTier}
          disabled={!popIsSet || !!popTierParseError}
          type="button"
        >
          Clear
        </button>
        <button class="btn btn--ghost btn--sm" onclick={closePopover} type="button">Cancel</button>
        <button
          class="btn btn--primary btn--sm"
          onclick={save}
          disabled={!!popFieldError || !!popTierParseError}
          type="button"
        >
          Save to {TIER_LABEL[popTier]}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .settingssearch-view { display: flex; flex-direction: column; gap: 0.75rem; }
  .settingssearch-head { display: flex; align-items: center; justify-content: space-between; }
  .settingssearch-head h2 { margin: 0; font-size: 1.05rem; }
  .scope { font-size: 0.72rem; color: var(--text-faint); text-transform: uppercase; letter-spacing: 0.06em; }

  .search-input {
    width: 100%; font-size: 0.85rem; padding: 0.5rem 0.65rem;
    background: var(--bg-card); color: var(--text); border: 1px solid var(--border-strong); border-radius: 0.4rem;
    font-family: inherit; box-sizing: border-box;
  }
  .hint { font-size: 0.78rem; color: var(--text-muted); margin: 0; }

  .candidates { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 0.2rem; max-height: 55vh; overflow-y: auto; }
  .candidate-row {
    width: 100%; text-align: left; display: flex; flex-direction: column; gap: 0.1rem;
    background: transparent; border: 1px solid transparent; border-radius: 0.35rem; padding: 0.45rem 0.6rem; cursor: pointer; color: inherit;
  }
  .candidate-row:hover { background: var(--bg-subtle); border-color: var(--border); }
  .candidate-row__main { display: flex; align-items: center; gap: 0.4rem; }
  .candidate-row__main code { font-size: 0.8rem; }
  .candidate-row__desc {
    font-size: 0.72rem; color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .dot { width: 0.4rem; height: 0.4rem; border-radius: 50%; background: var(--accent-user); flex-shrink: 0; }

  .popover-backdrop {
    position: fixed; inset: 0; background: color-mix(in srgb, black 45%, transparent);
    display: flex; align-items: center; justify-content: center; z-index: 300; padding: 1rem;
  }
  .popover {
    background: var(--bg-card); border: 1px solid var(--border); border-radius: 0.6rem;
    padding: 1rem 1.1rem; width: min(32rem, 100%); max-height: 85vh; overflow-y: auto;
    display: flex; flex-direction: column; gap: 0.6rem; box-shadow: 0 12px 36px rgba(0,0,0,0.3);
  }
  .popover__head code { font-size: 0.95rem; font-weight: 600; }
  .popover__desc { font-size: 0.8rem; color: var(--text-muted); margin: 0; }

  .tier-radios { display: flex; gap: 0.85rem; flex-wrap: wrap; }
  .radio-row, .bool-row { display: flex; align-items: center; gap: 0.4rem; font-size: 0.82rem; }

  .conflict-hint { font-size: 0.74rem; color: var(--text-faint); font-style: italic; margin: 0; }

  .popover-field { display: flex; flex-direction: column; gap: 0.3rem; border: none; margin: 0; padding: 0; }
  .popover-field:disabled { opacity: 0.55; }
  .text-input, .json-input {
    width: 100%; font-size: 0.82rem; padding: 0.4rem 0.55rem;
    background: var(--bg); color: var(--text); border: 1px solid var(--border-strong); border-radius: 0.35rem;
    font-family: inherit; box-sizing: border-box;
  }
  .json-input { font-family: var(--font-mono, monospace); resize: vertical; }
  .field-error { font-size: 0.72rem; color: var(--accent-result-err); margin: 0; }

  .conflicts {
    border: 1px solid color-mix(in srgb, var(--accent-result-err) 45%, var(--border));
    background: color-mix(in srgb, var(--accent-result-err) 8%, transparent);
    border-radius: 0.4rem; padding: 0.5rem 0.65rem; display: flex; flex-direction: column; gap: 0.3rem;
  }
  .conflicts__title { font-size: 0.78rem; font-weight: 600; }

  .popover-actions { display: flex; justify-content: flex-end; gap: 0.5rem; }
</style>
