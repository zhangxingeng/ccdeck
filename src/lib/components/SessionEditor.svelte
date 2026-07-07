<script lang="ts">
  /**
   * SessionEditor.svelte — the single-page session view *is* the editor.
   *
   * This component is the orchestrator: it owns the byte-faithful edit Draft,
   * crash-safe persistence, save/discard/history/exit flows, and turns the draft
   * into a display model (one chat bubble per renderable row).
   * All rendering lives in focused children:
   *   SessionMetaCard · MessageCell · SaveRail · RawJsonModal
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
  import type { BackupVersion, Entry, Session } from '$lib/types';
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
    readSubagents,
  } from '$lib/api';
  import { copyToClipboard } from '$lib/copy';
  import { resumeCommand } from '$lib/resume';
  import { parseJsonl, extractMeta, extractCustomTitle } from '$lib/parser';
  import { renameSession } from '$lib/sessionOps';
  import { buildSubagentSessions } from '$lib/builder';
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
  import SaveRail from './SaveRail.svelte';
  import RawJsonModal from './RawJsonModal.svelte';
  import InlineSearchPanel from './InlineSearchPanel.svelte';
  import Turn from './Turn.svelte';

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

  // Find-in-chat — reuses the shared search store, scoped to this one session.
  let searchOpen = $state(false);

  // Back-to-top — shown once the page has scrolled past the header.
  let showBackToTop = $state(false);

  // Subagent stacked navigation — popped via Esc/← Back. Subagent transcripts
  // aren't part of the editable Draft, so the top of the stack renders
  // read-only (plain Turn loop, same as SessionView.svelte) instead of the
  // MessageCell editing UI. Nothing currently pushes onto this stack: it was
  // previously reached via a tool_use "Open →" affordance in Block.svelte,
  // which was removed along with tool-call rendering.
  let subagentStack = $state<{ session: Session; label: string }[]>([]);
  // Resolved once per load (in parallel with the session/draft load below).
  // Currently unconsumed — see subagentStack note above — kept alongside it.
  let subagentSessions = $state<Map<string, Session>>(new Map());
  function pushSubagent(session: Session, label: string) {
    subagentStack = [...subagentStack, { session, label }];
  }
  function popSubagentTo(depth: number) {
    subagentStack = subagentStack.slice(0, depth);
  }

  // Title + inline rename — same renameSession() BrowseView's list uses, applied
  // directly to this open file. Optimistic override avoids a full reparse.
  let titleOverride = $state<string | null>(null);
  let renamingTitle = $state(false);
  let titleRenameInput = $state('');
  let titleRenameError = $state<string | null>(null);
  let titleRenameConfirming = $state(false);

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
  // A rename can land anywhere in the file, so scan the whole raw text (not
  // just the browse list's 50-line preview) for a real custom-title entry;
  // fall back to the first-user-message title extractMeta() derives.
  let derivedTitle = $derived.by(() => {
    if (!rawText) return '';
    const custom = extractCustomTitle(rawText);
    if (custom) return custom;
    return extractMeta(rawText.split('\n')).title;
  });
  let displayTitle = $derived(titleOverride ?? derivedTitle);
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
  }

  function parseLine(line: string): Entry | null {
    const es = parseJsonl(line);
    return es.length > 0 ? es[0] : null;
  }

  // Renderable rows (conversational lines with visible blocks — user/assistant
  // text only). Pure meta/echo lines parse to nothing here but stay preserved
  // in the draft and pass through untouched on save.
  let renderable = $derived.by<RenderRow[]>(() => {
    if (!draft) return [];
    const out: RenderRow[] = [];
    for (const key of draft.order) {
      const row = draft.rows[key];
      const entry = parseLine(row.versions[row.active]);
      if (!entry || entry.blocks.length === 0) continue;
      out.push({ key, row, entry });
    }
    return out;
  });

  // Fast key → RenderRow lookup for the display loop.
  let rmap = $derived(new Map(renderable.map((r) => [r.key, r])));

  // One chat bubble per renderable row.
  let displayItems = $derived(groupDisplayItems(renderable.map((r) => r.key)));
  let visibleItems = $derived(displayItems.slice(0, visibleCount));

  // ── Jump-to-hit (from search) ────────────────────────────────────────────
  // Find the display-item index whose message uuid matches, ensure it's within
  // the rendered window, then scroll its anchor into view and flash it.
  async function jumpTo(uuid: string): Promise<void> {
    if (!draft) return;
    const rr = renderable.find((r) => r.entry.uuid === uuid);
    if (!rr) return;
    const idx = displayItems.findIndex((it) => it.key === rr.key);
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
        readSubagents(path)
          .then((files) => { subagentSessions = buildSubagentSessions(files); })
          .catch(() => {});

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

  // Ctrl/Cmd+F opens find-in-chat instead of the browser's own find bar;
  // Ctrl/Cmd+S saves (same confirm-then-write flow as the SaveRail's Save
  // button — no new save logic here); Escape closes whichever of this editor's
  // own modals is open (it does NOT drive the exit flow — attemptExit/
  // requestExit above is the only path that leaves the editor).
  onMount(() => {
    function closeTopModal(): boolean {
      if (rawEditKey !== null) { rawEditKey = null; return true; }
      if (titleRenameConfirming) { titleRenameConfirming = false; return true; }
      if (showHistoryModal) { showHistoryModal = false; pendingRestore = null; return true; }
      if (showDiscardModal) { showDiscardModal = false; return true; }
      if (showSaveModal) { showSaveModal = false; return true; }
      if (showExitModal) { showExitModal = false; return true; }
      return false;
    }
    function onKeydown(e: KeyboardEvent) {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f') {
        e.preventDefault();
        searchOpen = true;
        return;
      }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
        // Always prevent the browser/OS "Save Page" dialog; only actually pop
        // the save-confirm modal when there's something to save and no other
        // modal is already mid-flow.
        e.preventDefault();
        if (
          !draft || !dirty ||
          showSaveModal || showDiscardModal || showExitModal || showHistoryModal ||
          rawEditKey !== null || renamingTitle
        ) {
          return;
        }
        showSaveModal = true;
        return;
      }
      if (e.key === 'Escape') {
        if (closeTopModal()) { e.preventDefault(); return; }
        if (subagentStack.length > 0) {
          e.preventDefault();
          subagentStack = subagentStack.slice(0, -1);
        }
      }
    }
    window.addEventListener('keydown', onKeydown);
    return () => window.removeEventListener('keydown', onKeydown);
  });

  // Back-to-top button visibility — the whole page scrolls (no inner container).
  onMount(() => {
    function onScroll() {
      showBackToTop = window.scrollY > 600;
    }
    window.addEventListener('scroll', onScroll, { passive: true });
    return () => window.removeEventListener('scroll', onScroll);
  });
  function scrollToTop(): void {
    window.scrollTo({ top: 0, behavior: 'smooth' });
  }

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

  // ── Title rename (immediate, direct file write — same as BrowseView) ───────
  function startTitleRename() {
    renamingTitle = true;
    titleRenameInput = displayTitle;
    titleRenameError = null;
  }
  function cancelTitleRename() {
    renamingTitle = false;
    titleRenameInput = '';
    titleRenameError = null;
    titleRenameConfirming = false;
  }
  function requestTitleRename() {
    const t = titleRenameInput.trim();
    if (!t) {
      titleRenameError = 'Title cannot be empty.';
      return;
    }
    titleRenameError = null;
    titleRenameConfirming = true;
  }
  // Reloads rawText/draft from disk after the rename write — safe only because
  // the Rename control is disabled while dirty, so there are no in-memory
  // edits this reload could silently discard.
  async function confirmTitleRename() {
    if (!titleRenameConfirming) return;
    const newTitle = titleRenameInput.trim();
    titleRenameConfirming = false;
    try {
      await renameSession(path, newTitle);
      const fresh = await readSession(path);
      rawText = fresh;
      draft = buildDraft(fresh, path, Math.floor(Date.now() / 1000));
      titleOverride = newTitle;
      renamingTitle = false;
      showToast('Renamed.');
    } catch (e) {
      titleRenameError = e instanceof Error ? e.message : String(e);
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
    // Back out of the subagent stack one level at a time before this ever
    // reaches the real "leave the editor" flow below.
    if (subagentStack.length > 0) {
      subagentStack = subagentStack.slice(0, -1);
      return;
    }
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

  {#if subagentStack.length > 0}
    {@const top = subagentStack[subagentStack.length - 1]}
    <!-- ── Subagent stacked navigation (read-only) ──────────────────────────── -->
    <!-- No local "back" button here — the header's ← Back (and Esc) already
         pop one stack level via attemptExit(); a second same-labeled button
         here would just be confusing. Crumbs below jump directly to any level. -->
    <div class="subagent-crumbs">
      <span class="subagent-crumbs__trail">
        <button type="button" class="crumb" onclick={() => popSubagentTo(0)}>Main session</button>
        {#each subagentStack as level, i (i)}
          <span class="crumb-sep">▸</span>
          <button
            type="button" class="crumb"
            class:crumb--current={i === subagentStack.length - 1}
            onclick={() => popSubagentTo(i + 1)}
          >{level.label}</button>
        {/each}
      </span>
    </div>
    <div class="session-turns">
      {#each top.session.turns as turn, i (i)}
        <Turn {turn} onOpenSubagent={pushSubagent} />
      {/each}
      {#if top.session.turns.length === 0}
        <div class="empty-state">No conversation turns found in this subagent.</div>
      {/if}
    </div>
  {:else}

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

  <!-- ── Title + inline rename ──────────────────────────────────────────────── -->
  <div class="viewer-title-row">
    {#if renamingTitle}
      <div class="rename-editor">
        <input
          type="text" class="rename-input" bind:value={titleRenameInput}
          aria-label="New session title"
          onkeydown={(e) => {
            if (e.key === 'Enter') requestTitleRename();
            if (e.key === 'Escape') cancelTitleRename();
          }}
        />
        {#if titleRenameError}<p class="rename-error">{titleRenameError}</p>{/if}
        <div class="rename-actions">
          <button type="button" class="btn btn--sm btn--primary" onclick={requestTitleRename}>Save</button>
          <button type="button" class="btn btn--sm btn--ghost" onclick={cancelTitleRename}>Cancel</button>
        </div>
      </div>
    {:else}
      <h2 class="viewer-title" title={displayTitle}>{displayTitle}</h2>
      <button
        type="button" class="btn btn--ghost btn--sm"
        onclick={startTitleRename}
        disabled={dirty}
        title={dirty ? 'Save or discard your edits before renaming' : 'Rename this session'}
      >Rename</button>
    {/if}
  </div>

  <!-- ── Metadata card ──────────────────────────────────────────────────────── -->
  {#if sessionInfo}
    <SessionMetaCard info={sessionInfo} />
  {/if}

  <!-- ── Find in chat ───────────────────────────────────────────────────────── -->
  {#if searchOpen}
    <InlineSearchPanel sessionPath={path} onJump={jumpTo} onClose={() => (searchOpen = false)} />
  {:else}
    <div class="find-toggle-row">
      <button class="btn btn--ghost btn--sm" onclick={() => (searchOpen = true)} type="button">
        🔍 Find in chat <span class="find-toggle-row__kbd">Ctrl+F</span>
      </button>
    </div>
  {/if}

  <!-- ── Messages ─────────────────────────────────────────────────────────── -->
  <div class="session-turns">
    {#each visibleItems as item (item.key)}
      {@const rr = rmap.get(item.key)}
      <div class="jump-anchor">
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
            onOpenSubagent={pushSubagent}
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

  <!-- ── Back to top ─────────────────────────────────────────────────────────── -->
  {#if showBackToTop}
    <button
      class="back-to-top" onclick={scrollToTop} type="button" aria-label="Back to top"
    >↑ Top</button>
  {/if}

  {/if}

{/if}

<!-- ── Title rename confirm modal ────────────────────────────────────────────── -->
{#if titleRenameConfirming}
  <div class="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="title-rename-title">
    <div class="modal">
      <h3 id="title-rename-title">Rename this session?</h3>
      <p>This updates the title saved in the chat file.</p>
      <div class="modal__warning">This change is not backed up and cannot be undone here.</div>
      <div class="modal__actions">
        <button type="button" class="btn btn--ghost" onclick={() => (titleRenameConfirming = false)}>Cancel</button>
        <button type="button" class="btn btn--primary" onclick={confirmTitleRename}>Rename</button>
      </div>
    </div>
  </div>
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
      <h3 id="history-title">Backup</h3>
      <p>A backup is taken before every save. Restoring also backs up first.</p>
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

  /* ── Title + inline rename ─────────────────────────────────────────── */
  .viewer-title-row { display: flex; align-items: center; gap: 0.6rem; margin-bottom: 0.85rem; }
  .viewer-title {
    flex: 1; min-width: 0; font-size: 1.05rem; font-weight: 600; margin: 0;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .rename-editor { flex: 1; display: flex; flex-direction: column; gap: 0.35rem; }
  .rename-input {
    width: 100%; padding: 0.4rem 0.65rem; border-radius: 0.35rem;
    border: 1px solid var(--border-strong); background: var(--bg-subtle); color: var(--text);
    font-family: var(--font-sans); font-size: 0.85rem; line-height: 1.4;
  }
  .rename-input:focus { outline: 2px solid var(--accent-user); outline-offset: 1px; }
  .rename-actions { display: flex; gap: 0.4rem; }
  .rename-error { font-size: 0.75rem; color: var(--accent-result-err); margin: 0; }

  /* ── Find-in-chat toggle ────────────────────────────────────── */
  .find-toggle-row { margin-bottom: 0.75rem; }
  .find-toggle-row__kbd { color: var(--text-faint); font-size: 0.7rem; margin-left: 0.3rem; }

  /* ── Subagent stacked navigation breadcrumbs ─────────────────── */
  .subagent-crumbs {
    display: flex; align-items: center; gap: 0.75rem; flex-wrap: wrap;
    margin-bottom: 0.85rem; padding-bottom: 0.6rem; border-bottom: 1px solid var(--border);
  }
  .subagent-crumbs__trail {
    display: flex; align-items: center; gap: 0.3rem; flex-wrap: wrap;
    font-size: 0.8rem; color: var(--text-muted); min-width: 0;
  }
  .crumb-sep { color: var(--text-faint); font-size: 0.7rem; }
  .crumb {
    background: none; border: 0; padding: 0.1rem 0.3rem; border-radius: 0.25rem;
    font-family: inherit; font-size: inherit; color: var(--text-muted); cursor: pointer;
    max-width: 22rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .crumb:hover { background: var(--bg-subtle); color: var(--text); }
  .crumb--current { color: var(--accent-subagent); font-weight: 600; cursor: default; }
  .crumb--current:hover { background: none; }

  /* ── Back to top ──────────────────────────────────────────────  */
  /* Bottom-right, clear of SaveRail (which is vertically centered at the
     same right edge) so the two floating controls never overlap. */
  .back-to-top {
    position: fixed; right: 1.25rem; bottom: 1.25rem; z-index: 20;
    padding: 0.45rem 0.8rem; font-size: 0.78rem;
    background: var(--bg-card); color: var(--text-muted);
    border: 1px solid var(--border-strong); border-radius: 999px;
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.16); cursor: pointer;
  }
  .back-to-top:hover { color: var(--text); border-color: var(--accent-user); }

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
