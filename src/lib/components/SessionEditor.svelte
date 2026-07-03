<script lang="ts">
  /**
   * SessionEditor.svelte — the single-page session view *is* the editor.
   *
   * This component is the orchestrator: it owns the byte-faithful edit Draft,
   * crash-safe persistence, save/discard/history/exit flows, and turns the draft
   * into a display model (chat bubbles interleaved with collapsed tool groups).
   * All rendering lives in focused children:
   *   SessionMetaCard · MessageCell · ToolGroup · SaveRail · RawJsonModal
   *
   * Safety model (JSON-safe by construction):
   *   - The edit model owns every line; the UI only ever edits a message *string*
   *     (per text block), the speaker, or a re-validated raw JSON line. Users can't
   *     hand-corrupt the structure.
   *   - Edits auto-persist to a crash-safe temp draft (resumed on reopen), deleted
   *     on save. "Save" snapshots a backup before overwriting the original file.
   *
   * Props:
   *   path        — source file path (read + save + backup + draft)
   *   onExit      — perform the actual navigation back to the browser
   *   requestExit — $bindable; parent calls this (from the header ← Back) to ask
   *                 the editor to handle exit with a dirty-guard prompt.
   */
  import { onMount, tick } from 'svelte';
  import type { BackupVersion, Entry } from '$lib/types';
  import {
    readSession,
    writeSession,
    readEditDraft,
    writeEditDraft,
    deleteEditDraft,
    snapshot,
    listBackups,
    restoreBackup,
    forkSession,
    resumeInTerminal,
  } from '$lib/api';
  import { copyToClipboard } from '$lib/copy';
  import { resumeCommand } from '$lib/resume';
  import { parseJsonl } from '$lib/parser';
  import {
    buildDraft,
    serializeDraft,
    isDirty,
    applyBlockTextEdit,
    applyRoleEdit,
    applyRawEdit,
    setActiveVersion,
    deleteRow,
    restoreRow,
    extractSessionInfo,
  } from '$lib/editDraft';
  import type { Draft, DraftRow } from '$lib/editDraft';
  import { groupDisplayItems } from '$lib/displayModel';
  import SessionMetaCard from './SessionMetaCard.svelte';
  import MessageCell from './MessageCell.svelte';
  import ToolGroup from './ToolGroup.svelte';
  import SaveRail from './SaveRail.svelte';
  import RawJsonModal from './RawJsonModal.svelte';

  // ── Props ──────────────────────────────────────────────────────────────────
  let {
    path,
    onExit = () => {},
    requestExit = $bindable<() => void>(),
    scrollToUuid = undefined,
    scrollNonce = 0,
  }: {
    path: string;
    onExit?: () => void;
    requestExit?: () => void;
    scrollToUuid?: string;
    scrollNonce?: number;
  } = $props();

  // ── State ──────────────────────────────────────────────────────────────────
  let draft = $state<Draft | null>(null);
  let rawText = $state('');
  let loading = $state(true);
  let loadError = $state<string | null>(null);
  let resumedBanner = $state(false);

  // Raw JSON escape hatch
  let rawEditKey = $state<string | null>(null);
  let rawEditInitial = $state('');

  // Windowing — cap rendered display items so huge sessions stay responsive.
  let visibleCount = $state(300);

  // Modals
  let showSaveModal = $state(false);
  let showDiscardModal = $state(false);
  let showExitModal = $state(false);
  let showHistoryModal = $state(false);
  let backups = $state<BackupVersion[]>([]);
  let pendingRestore = $state<BackupVersion | null>(null);
  let saving = $state(false);

  // Toast
  let toastMsg = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | null = null;

  // Persist debounce
  let persistTimer: ReturnType<typeof setTimeout> | null = null;

  // ── Derived ──────────────────────────────────────────────────────────────
  let sessionInfo = $derived(rawText ? extractSessionInfo(rawText) : null);
  let dirty = $derived(draft ? isDirty(draft) : false);
  let changeCount = $derived.by(() => {
    if (!draft) return 0;
    let n = 0;
    for (const key of Object.keys(draft.rows)) {
      const r = draft.rows[key];
      if (r.deleted || r.active !== 0) n++;
    }
    // Reorder counts as one aggregate change.
    const sortedByOriginal = [...draft.order].sort(
      (a, b) => draft!.rows[a].originalIndex - draft!.rows[b].originalIndex
    );
    for (let i = 0; i < draft.order.length; i++) {
      if (draft.order[i] !== sortedByOriginal[i]) { n++; break; }
    }
    return n;
  });

  interface RenderRow {
    key: string;
    row: DraftRow;
    entry: Entry;
    hasText: boolean;
  }

  function parseLine(line: string): Entry | null {
    const es = parseJsonl(line);
    return es.length > 0 ? es[0] : null;
  }

  // Renderable rows (conversational lines with visible blocks). Pure meta/echo
  // lines parse to nothing here but stay preserved in the draft and pass through
  // untouched on save.
  let renderable = $derived.by<RenderRow[]>(() => {
    if (!draft) return [];
    const out: RenderRow[] = [];
    for (const key of draft.order) {
      const row = draft.rows[key];
      const entry = parseLine(row.versions[row.active]);
      if (!entry || entry.blocks.length === 0) continue;
      out.push({
        key,
        row,
        entry,
        hasText: entry.blocks.some((b) => b.blockType === 'text'),
      });
    }
    return out;
  });

  // Fast key → RenderRow lookup for the display loop.
  let rmap = $derived(new Map(renderable.map((r) => [r.key, r])));

  // Chat bubbles interleaved with collapsed tool-activity groups.
  let displayItems = $derived(
    groupDisplayItems(renderable.map((r) => ({ key: r.key, hasText: r.hasText })))
  );
  let visibleItems = $derived(displayItems.slice(0, visibleCount));

  // ── Jump-to-hit (from search) ────────────────────────────────────────────
  // Find the display-item index whose message uuid matches, ensure it's within
  // the rendered window, then scroll its anchor into view and flash it.
  async function jumpTo(uuid: string): Promise<void> {
    if (!draft) return;
    const rr = renderable.find((r) => r.entry.uuid === uuid);
    if (!rr) return;
    const idx = displayItems.findIndex((it) =>
      it.kind === 'message' ? it.key === rr.key : it.keys.includes(rr.key)
    );
    if (idx < 0) return;
    if (idx >= visibleCount) visibleCount = idx + 50;
    await tick();
    const anchors = document.querySelectorAll('.session-turns > .jump-anchor');
    const el = anchors[idx] as HTMLElement | undefined;
    if (!el) return;
    el.scrollIntoView({ behavior: 'smooth', block: 'center' });
    el.classList.add('jump-flash');
    setTimeout(() => el.classList.remove('jump-flash'), 1800);
  }

  // Re-run whenever the target (or its nonce) changes and the draft is ready.
  $effect(() => {
    // Reference scrollNonce so repeat-clicks on the same hit re-trigger.
    scrollNonce;
    if (draft && scrollToUuid) jumpTo(scrollToUuid);
  });

  // ── Load on mount ──────────────────────────────────────────────────────────
  onMount(() => {
    (async () => {
      try {
        const raw = await readSession(path);
        rawText = raw;

        const existing = await readEditDraft(path);
        let resumed = false;

        if (existing) {
          try {
            const parsed = JSON.parse(existing) as Draft;
            const joinedVersions = parsed.order
              .map((k: string) => parsed.rows[k].versions[0])
              .join('\n');
            const rawJoined = raw.split('\n').filter((l: string) => l.trim() !== '').join('\n');
            if (joinedVersions === rawJoined) {
              draft = parsed;
              resumed = true;
              resumedBanner = true;
            }
          } catch {
            // fall through to fresh build
          }
        }

        if (!resumed) {
          draft = buildDraft(raw, path, Math.floor(Date.now() / 1000));
        }
      } catch (e) {
        loadError = e instanceof Error ? e.message : String(e);
      } finally {
        loading = false;
      }
    })();

    return () => {
      if (persistTimer) clearTimeout(persistTimer);
    };
  });

  // Expose the exit guard to the parent header's ← Back button.
  $effect(() => { requestExit = attemptExit; });

  // ── Draft persistence (debounced) ───────────────────────────────────────────
  function schedulePersist() {
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = setTimeout(() => {
      if (draft) writeEditDraft(path, JSON.stringify(draft)).catch(() => {});
      persistTimer = null;
    }, 300);
  }
  function cancelPersist() {
    if (persistTimer) { clearTimeout(persistTimer); persistTimer = null; }
  }
  function mutate(newDraft: Draft) {
    draft = newDraft;
    schedulePersist();
  }

  // ── Toast ────────────────────────────────────────────────────────────────
  function showToast(msg: string) {
    toastMsg = msg;
    if (toastTimer) clearTimeout(toastTimer);
    toastTimer = setTimeout(() => { toastMsg = null; toastTimer = null; }, 3500);
  }

  // ── Row mutations (wired to children) ────────────────────────────────────────
  function doBlockEdit(key: string, ordinal: number, text: string) {
    if (draft) mutate(applyBlockTextEdit(draft, key, ordinal, text));
  }
  function doDelete(key: string) { if (draft) mutate(deleteRow(draft, key)); }
  function doRestore(key: string) { if (draft) mutate(restoreRow(draft, key)); }
  function doSetActiveVersion(key: string, idx: number) {
    if (draft) mutate(setActiveVersion(draft, key, idx));
  }
  function doRole(key: string, role: string) {
    if (draft) mutate(applyRoleEdit(draft, key, role));
  }
  async function doResumeFrom(key: string) {
    if (!draft) return;
    const row = draft.rows[key];
    if (!row) return;
    try {
      const forked = await forkSession(path, row.originalIndex);
      const cwd = sessionInfo?.cwd ?? '';
      await copyToClipboard(resumeCommand(cwd, forked.id));
      try {
        await resumeInTerminal(cwd, forked.id);
        showToast('Forked session — opened in a terminal, command also copied to clipboard');
      } catch {
        showToast('Forked session — could not open a terminal, command copied to clipboard');
      }
    } catch (e) {
      showToast(`Fork failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  }
  function deleteKeys(keys: string[]) {
    if (!draft) return;
    let d = draft;
    for (const k of keys) d = deleteRow(d, k);
    mutate(d);
  }
  function restoreKeys(keys: string[]) {
    if (!draft) return;
    let d = draft;
    for (const k of keys) d = restoreRow(d, k);
    mutate(d);
  }

  // Move a chat bubble relative to the previous/next *chat bubble*, hopping over
  // any collapsed tool group between them as a unit.
  function moveMessage(key: string, dir: -1 | 1) {
    if (!draft) return;
    const msgKeys = displayItems
      .filter((i) => i.kind === 'message')
      .map((i) => (i as { key: string }).key);
    const pos = msgKeys.indexOf(key);
    if (pos < 0) return;
    const targetKey = msgKeys[pos + dir];
    if (targetKey === undefined) return; // already at an end

    const order = [...draft.order];
    order.splice(order.indexOf(key), 1);
    const ti = order.indexOf(targetKey);
    order.splice(dir === -1 ? ti : ti + 1, 0, key);
    mutate({ ...draft, order });
  }

  // ── Raw JSON escape hatch ───────────────────────────────────────────────────
  function openRawEdit(key: string) {
    if (!draft) return;
    const row = draft.rows[key];
    if (!row) return;
    // Pretty-print for editing; applyRawEdit re-collapses to one line on save.
    try {
      rawEditInitial = JSON.stringify(JSON.parse(row.versions[row.active]), null, 2);
    } catch {
      rawEditInitial = row.versions[row.active];
    }
    rawEditKey = key;
  }
  // Returns an error string to show in the modal, or null on success (closes it).
  function applyRaw(text: string): string | null {
    if (rawEditKey === null || !draft) return 'No line selected.';
    try {
      mutate(applyRawEdit(draft, rawEditKey, text));
      rawEditKey = null;
      return null;
    } catch (e) {
      return e instanceof Error ? e.message : 'Invalid JSON.';
    }
  }

  // ── Resume banner ─────────────────────────────────────────────────────────
  async function discardResume() {
    draft = buildDraft(rawText, path, Math.floor(Date.now() / 1000));
    resumedBanner = false;
    cancelPersist();
    await deleteEditDraft(path).catch(() => {});
  }

  // ── Save: overwrite original (with backup) ──────────────────────────────────
  async function confirmSave() {
    if (!draft) return;
    showSaveModal = false;
    saving = true;
    try {
      const bk = await snapshot(path);
      const content = serializeDraft(draft);
      await writeSession(path, content);
      cancelPersist();
      await deleteEditDraft(path);
      rawText = content;
      draft = buildDraft(content, path, Math.floor(Date.now() / 1000));
      showToast(`Saved. Backup v${bk.version} created.`);
    } catch (e) {
      showToast(`Save failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      saving = false;
    }
  }

  // ── Save as copy ────────────────────────────────────────────────────────────
  async function saveAsCopy() {
    if (!draft) return;
    saving = true;
    try {
      const content = serializeDraft(draft);
      const lastSlash = path.lastIndexOf('/');
      const dir = lastSlash >= 0 ? path.slice(0, lastSlash) : '.';
      const filename = lastSlash >= 0 ? path.slice(lastSlash + 1) : path;
      const stem = filename.endsWith('.jsonl') ? filename.slice(0, -6) : filename;
      const ts = Math.floor(Date.now() / 1000);
      const copyPath = `${dir}/${stem}-edited-${ts}.jsonl`;
      await writeSession(copyPath, content);
      showToast(`Saved a copy: ${copyPath.split('/').pop()}`);
    } catch (e) {
      showToast(`Save failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      saving = false;
    }
  }

  // ── Discard all edits ───────────────────────────────────────────────────────
  async function confirmDiscard() {
    showDiscardModal = false;
    draft = buildDraft(rawText, path, Math.floor(Date.now() / 1000));
    cancelPersist();
    await deleteEditDraft(path).catch(() => {});
    showToast('Edits discarded.');
  }

  // ── History / restore ───────────────────────────────────────────────────────
  async function openHistory() {
    backups = await listBackups(path);
    pendingRestore = null;
    showHistoryModal = true;
  }
  async function confirmRestoreBackup() {
    if (!pendingRestore) return;
    const bk = pendingRestore;
    pendingRestore = null;
    showHistoryModal = false;
    saving = true;
    try {
      await snapshot(path);
      const restored = await restoreBackup(bk.path);
      await writeSession(path, restored);
      rawText = restored;
      draft = buildDraft(restored, path, Math.floor(Date.now() / 1000));
      cancelPersist();
      await deleteEditDraft(path);
      showToast(`Restored v${bk.version}`);
    } catch (e) {
      showToast(`Restore failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      saving = false;
    }
  }

  // ── Exit (dirty guard, driven by parent's ← Back) ───────────────────────────
  function attemptExit() {
    if (!draft || !isDirty(draft)) {
      cancelPersist();
      deleteEditDraft(path).catch(() => {});
      onExit();
      return;
    }
    showExitModal = true;
  }
  async function exitSave() {
    showExitModal = false;
    saving = true;
    try {
      const bk = await snapshot(path);
      const content = serializeDraft(draft!);
      await writeSession(path, content);
      cancelPersist();
      await deleteEditDraft(path);
      showToast(`Saved. Backup v${bk.version}.`);
      onExit();
    } catch (e) {
      showToast(`Save failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      saving = false;
    }
  }
  async function exitSaveCopy() {
    showExitModal = false;
    await saveAsCopy();
    onExit();
  }
  async function exitDiscard() {
    showExitModal = false;
    cancelPersist();
    await deleteEditDraft(path).catch(() => {});
    onExit();
  }

  // ── Format helpers ──────────────────────────────────────────────────────────
  function formatTimestamp(unix: number): string {
    return new Date(unix * 1000).toLocaleString();
  }
</script>

{#if loading}
  <div class="empty-state">Loading session…</div>
{:else if loadError}
  <div class="empty-state">{loadError}</div>
{:else if draft}

  <!-- ── Resume banner ──────────────────────────────────────────────────────── -->
  {#if resumedBanner}
    <div class="resume-banner">
      <span>Resumed unsaved edits</span>
      <button class="btn btn--sm btn--ghost" onclick={discardResume} type="button">Discard</button>
      <button
        class="btn btn--sm btn--ghost resume-banner__dismiss"
        onclick={() => (resumedBanner = false)}
        type="button"
      >×</button>
    </div>
  {/if}

  <!-- ── Metadata card ──────────────────────────────────────────────────────── -->
  {#if sessionInfo}
    <SessionMetaCard info={sessionInfo} />
  {/if}

  <!-- ── Messages + tool groups ─────────────────────────────────────────────── -->
  <div class="session-turns">
    {#each visibleItems as item (item.kind === 'message' ? item.key : 'g:' + item.keys[0])}
      <div class="jump-anchor">
        {#if item.kind === 'message'}
          {@const rr = rmap.get(item.key)}
          {#if rr}
            <MessageCell
              msgKey={item.key}
              row={rr.row}
              entry={rr.entry}
              onBlockEdit={(o, t) => doBlockEdit(item.key, o, t)}
              onDelete={() => doDelete(item.key)}
              onRestore={() => doRestore(item.key)}
              onRole={(role) => doRole(item.key, role)}
              onMoveUp={() => moveMessage(item.key, -1)}
              onMoveDown={() => moveMessage(item.key, 1)}
              onRaw={() => openRawEdit(item.key)}
              onSetVersion={(idx) => doSetActiveVersion(item.key, idx)}
              onResumeFrom={() => doResumeFrom(item.key)}
            />
          {/if}
        {:else}
          {@const groupItems = item.keys.map((k) => rmap.get(k)).filter((x) => x !== undefined)}
          <ToolGroup
            items={groupItems}
            onDeleteGroup={() => deleteKeys(item.keys)}
            onRestoreGroup={() => restoreKeys(item.keys)}
            onRawLine={openRawEdit}
            onDeleteLine={doDelete}
            onRestoreLine={doRestore}
          />
        {/if}
      </div>
    {/each}

    {#if renderable.length === 0}
      <div class="empty-state">No conversation messages found in this session.</div>
    {/if}

    {#if displayItems.length > visibleCount}
      <div class="load-more">
        <span>Showing {visibleCount} of {displayItems.length} blocks</span>
        <button class="btn btn--sm" onclick={() => (visibleCount += 500)} type="button">Show 500 more</button>
        <button class="btn btn--sm btn--ghost" onclick={() => (visibleCount = displayItems.length)} type="button">Show all</button>
      </div>
    {/if}
  </div>

  <!-- ── Floating save rail (right edge) ────────────────────────────────────── -->
  <SaveRail
    {dirty}
    {changeCount}
    {saving}
    onSave={() => (showSaveModal = true)}
    onSaveCopy={saveAsCopy}
    onDiscard={() => (showDiscardModal = true)}
    onHistory={openHistory}
  />

{/if}

<!-- ── Raw JSON editor modal ─────────────────────────────────────────────────── -->
{#if rawEditKey}
  <RawJsonModal initial={rawEditInitial} onApply={applyRaw} onCancel={() => (rawEditKey = null)} />
{/if}

<!-- ── Save (overwrite original) modal ───────────────────────────────────────── -->
{#if showSaveModal}
  <div class="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="save-title">
    <div class="modal">
      <h3 id="save-title">Save to original file</h3>
      <div class="modal__warning">
        This rewrites your real Claude chat history at:<br /><strong data-copy-text={path}>{path}</strong>
      </div>
      <p>A backup snapshot is created first — restore any time from History.</p>
      <div class="modal__actions">
        <button class="btn btn--sm btn--ghost" onclick={() => (showSaveModal = false)} type="button">Cancel</button>
        <button class="btn btn--sm btn--primary" onclick={confirmSave} type="button">Save (backup first)</button>
      </div>
    </div>
  </div>
{/if}

<!-- ── Discard modal ─────────────────────────────────────────────────────────── -->
{#if showDiscardModal}
  <div class="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="discard-title">
    <div class="modal">
      <h3 id="discard-title">Discard all edits?</h3>
      <p>This throws away every unsaved change and clears the draft. The original file is untouched.</p>
      <div class="modal__actions">
        <button class="btn btn--sm btn--ghost" onclick={() => (showDiscardModal = false)} type="button">Keep editing</button>
        <button class="btn btn--sm btn--danger" onclick={confirmDiscard} type="button">Discard edits</button>
      </div>
    </div>
  </div>
{/if}

<!-- ── Exit dirty-guard modal ────────────────────────────────────────────────── -->
{#if showExitModal}
  <div class="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="exit-title">
    <div class="modal">
      <h3 id="exit-title">You have unsaved edits</h3>
      <p>Choose what to do before leaving:</p>
      <div class="modal__actions modal__actions--col">
        <button class="btn btn--sm btn--primary" onclick={exitSave} disabled={saving} type="button">Save to original (backup first)</button>
        <button class="btn btn--sm" onclick={exitSaveCopy} disabled={saving} type="button">Save as a copy</button>
        <button class="btn btn--sm btn--ghost" onclick={exitDiscard} disabled={saving} type="button">Discard edits &amp; leave</button>
        <button class="btn btn--sm btn--ghost" onclick={() => (showExitModal = false)} disabled={saving} type="button">Keep editing</button>
      </div>
    </div>
  </div>
{/if}

<!-- ── History modal ─────────────────────────────────────────────────────────── -->
{#if showHistoryModal}
  <div class="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="history-title">
    <div class="modal" style="max-width:520px;">
      <h3 id="history-title">Backup history</h3>
      <p>Snapshots are taken before every save. Restoring also snapshots first.</p>
      {#if backups.length === 0}
        <div class="empty-state" style="padding:1rem 0;">No backups yet.</div>
      {:else}
        <div class="history-list">
          {#each backups as bk (bk.version)}
            <div class="history-item">
              <div class="history-item__info">
                <strong>v{bk.version}</strong>
                <span style="color:var(--text-muted);font-size:0.78rem;">{formatTimestamp(bk.timestamp)}</span>
                <span style="color:var(--text-faint);font-size:0.72rem;">{(bk.size / 1024).toFixed(1)} KB</span>
              </div>
              {#if pendingRestore?.version === bk.version}
                <div class="history-item__confirm">
                  <span style="font-size:0.78rem;color:var(--accent-result-err);">Snapshot current, then restore?</span>
                  <button class="btn btn--sm btn--danger" onclick={confirmRestoreBackup} type="button">Yes, restore</button>
                  <button class="btn btn--sm btn--ghost" onclick={() => (pendingRestore = null)} type="button">Cancel</button>
                </div>
              {:else}
                <button class="btn btn--sm" onclick={() => (pendingRestore = bk)} type="button">Restore</button>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
      <div class="modal__actions">
        <button class="btn btn--sm btn--ghost" onclick={() => { showHistoryModal = false; pendingRestore = null; }} type="button">Close</button>
      </div>
    </div>
  </div>
{/if}

<!-- ── Toast ─────────────────────────────────────────────────────────────────── -->
{#if toastMsg}
  <div class="toast" role="status">{toastMsg}</div>
{/if}

<style>
  /* ── Jump-to-hit anchor (from search) ───────────────────────── */
  /* A plain block wrapper (one per display item) so scrollIntoView has a real
     box to target. It adds no margin of its own, and child margins collapse
     through it, so vertical rhythm is unchanged. */
  .jump-anchor { scroll-margin-top: 84px; }
  /* jump-flash is toggled via JS (classList), so mark it :global to survive
     Svelte's unused-selector pruning. */
  .jump-anchor:global(.jump-flash) {
    animation: jump-flash 1.8s ease-out;
    border-radius: 0.5rem;
  }
  @keyframes jump-flash {
    0%, 25% { background: color-mix(in srgb, var(--accent-user) 22%, transparent); }
    100% { background: transparent; }
  }

  /* ── Resume banner ──────────────────────────────────────────── */
  .resume-banner {
    display: flex; align-items: center; gap: 0.6rem;
    padding: 0.5rem 0.85rem; margin-bottom: 0.75rem;
    background: color-mix(in srgb, var(--accent-user) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent-user) 30%, transparent);
    border-radius: 0.4rem; font-size: 0.8rem; color: var(--accent-user);
  }
  .resume-banner span:first-child { flex: 1; font-weight: 500; }
  .resume-banner__dismiss { font-size: 0.75rem; padding: 0.15rem 0.4rem; opacity: 0.6; }
  .resume-banner__dismiss:hover { opacity: 1; }

  /* ── Load more ──────────────────────────────────────────────── */
  .load-more {
    display: flex; align-items: center; gap: 0.6rem; padding: 0.75rem 0; margin-top: 0.5rem;
    border-top: 1px solid var(--border); font-size: 0.78rem; color: var(--text-muted); flex-wrap: wrap;
  }
  .load-more span { flex: 1; }

  /* ── History list ───────────────────────────────────────────── */
  .history-list {
    display: flex; flex-direction: column; gap: 0.4rem; max-height: 320px; overflow-y: auto;
    margin: 0.75rem 0; padding-right: 0.25rem;
  }
  .history-item {
    display: flex; align-items: center; justify-content: space-between; gap: 0.75rem;
    padding: 0.45rem 0.65rem; border: 1px solid var(--border); border-radius: 0.35rem;
    background: var(--bg-subtle); flex-wrap: wrap;
  }
  .history-item__info { display: flex; align-items: center; gap: 0.6rem; flex: 1; min-width: 0; }
  .history-item__confirm { display: flex; align-items: center; gap: 0.4rem; flex-wrap: wrap; }

  /* ── Modal column actions ───────────────────────────────────── */
  .modal__actions--col { flex-direction: column; align-items: stretch; gap: 0.4rem; }
  .modal__actions--col .btn { width: 100%; justify-content: center; text-align: center; }
</style>
