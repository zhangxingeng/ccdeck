<script lang="ts">
  /**
   * SessionEditor.svelte — the single-page session view *is* the editor.
   *
   * This component is the orchestrator: it owns the byte-faithful edit Draft
   * and the save/discard/exit flows, and turns the draft into a
   * display model (one chat bubble per renderable row). All rendering lives in
   * focused children: SessionMetaCard · MessageCell. The dirty indicator and
   * Save / Save as copy / Discard controls live in the top nav (+page.svelte),
   * driven through the editor's $bindable save-state surface.
   *
   * Safety model (JSON-safe by construction):
   *   - The edit model owns every line; the UI only ever edits a message
   *     *string* (per text block). Users can't hand-corrupt the structure.
   *   - "Save" snapshots a single-slot backup before overwriting the original
   *     file. There is no crash-safe autosave draft and no version history —
   *     edit in place, then Save writes straight to disk.
   *
   * Props:
   *   path        — source file path (read + save + backup)
   *   onExit      — perform the actual navigation back to the browser
   *   requestExit — $bindable; parent calls this (from the header ← Back) to ask
   *                 the editor to handle exit with a dirty-guard prompt.
   */
  import { onMount, onDestroy, tick } from 'svelte';
  import type { Entry } from '$lib/types';
  import {
    readSession,
    writeSession,
    snapshot,
    forkSession,
  } from '$lib/api';
  import ResumeMenu from './ResumeMenu.svelte';
  import { parseJsonl, extractMeta, extractCustomTitle } from '$lib/parser';
  import { renameSession } from '$lib/sessionOps';
  import {
    buildDraft,
    serializeDraft,
    isDirty,
    applyBlockTextEdit,
    extractSessionInfo,
    blockKey,
    deleteMessage,
    deleteThinking,
    deleteToolGroup,
    deleteBulk,
    undelete,
  } from '$lib/editDraft';
  import type { Draft, DraftRow } from '$lib/editDraft';
  import { groupDisplayItems, deriveTurnSpans } from '$lib/displayModel';
  import SessionMetaCard from './SessionMetaCard.svelte';
  import MessageCell from './MessageCell.svelte';
  import ToolGroup from './ToolGroup.svelte';
  import TurnDivider from './TurnDivider.svelte';
  import InlineSearchPanel from './InlineSearchPanel.svelte';

  // ── Props ──────────────────────────────────────────────────────────────────
  let {
    path,
    onExit = () => {},
    requestExit = $bindable<() => void>(),
    // Save-state surface — the editor is the single owner of dirty/saving/
    // change-count, mirrored up to the top nav (the one place the controls
    // live now that the floating SaveRail is gone). The three request* fns let
    // the nav's Save / Save as copy / Discard buttons drive the editor's own
    // save flows (Save/Discard still pop the editor's confirm modals).
    requestSave = $bindable<() => void>(),
    requestSaveCopy = $bindable<() => void>(),
    requestDiscard = $bindable<() => void>(),
    editorDirty = $bindable(false),
    editorChangeCount = $bindable(0),
    editorSaving = $bindable(false),
    scrollToUuid = undefined,
    scrollNonce = 0,
  }: {
    path: string;
    onExit?: () => void;
    requestExit?: () => void;
    requestSave?: () => void;
    requestSaveCopy?: () => void;
    requestDiscard?: () => void;
    editorDirty?: boolean;
    editorChangeCount?: number;
    editorSaving?: boolean;
    scrollToUuid?: string;
    scrollNonce?: number;
  } = $props();

  // ── State ──────────────────────────────────────────────────────────────────
  let draft = $state<Draft | null>(null);
  let rawText = $state('');
  let loading = $state(true);
  let loadError = $state<string | null>(null);

  // Windowing — cap rendered display items so huge sessions stay responsive.
  let visibleCount = $state(300);

  // Find-in-chat — reuses the shared search store, scoped to this one session.
  let searchOpen = $state(false);

  // Bulk multi-select (issue #14 checkpoint 5). `selectMode` toggles checkboxes
  // on each deletable unit; `selectedUnits` holds unit ids (derived from stable
  // row keys, so the selection survives visibleCount windowing). "Delete
  // selected" soft-deletes the union of their blocks via deleteBulk.
  let selectMode = $state(false);
  let selectedUnits = $state<Set<string>>(new Set());

  // Back-to-top — shown once the page has scrolled past the header.
  let showBackToTop = $state(false);

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
  let saving = $state(false);

  // Toast
  let toastMsg = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | null = null;

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
    for (const key of draft.order) {
      if (draft.rows[key].value !== draft.rows[key].original) n++;
    }
    n += draft.deletedBlocks.size;
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

  // Renderable rows (conversational lines with visible blocks — text,
  // thinking, tool_use, tool_result). Pure meta/echo lines parse to nothing
  // here but stay preserved in the draft and pass through untouched on save.
  let renderable = $derived.by<RenderRow[]>(() => {
    if (!draft) return [];
    const out: RenderRow[] = [];
    for (const key of draft.order) {
      const row = draft.rows[key];
      const entry = parseLine(row.value);
      if (!entry || entry.blocks.length === 0) continue;
      out.push({ key, row, entry, hasText: entry.blocks.some((b) => b.blockType === 'text') });
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

  // Turn spans (delete-as-a-unit): a user-with-text bubble starts a turn and it
  // runs to the next such bubble (see displayModel.ts). A turn always begins on
  // a display-item boundary (a user-with-text row is its own message bubble,
  // never inside a tool group), so we can key the divider by the span's first
  // row key and render it above whichever display item starts there.
  let turnSpans = $derived(
    deriveTurnSpans(renderable.map((r) => ({ key: r.key, type: r.entry.type, hasText: r.hasText })))
  );
  let turnByStartKey = $derived.by(() => {
    const m = new Map<string, string[]>();
    for (const span of turnSpans) m.set(span.keys[0], span.keys);
    return m;
  });

  // ── Bulk selection units (issue #14 checkpoint 5) ────────────────────────
  // The atomic selectable units are the display items themselves: one per
  // message bubble, one per tool group. Each maps to the row keys it covers,
  // keyed by a stable id (derived from row keys, not DOM position) so the
  // selection is windowing-proof.
  function messageUnitId(key: string): string { return `m:${key}`; }
  function groupUnitId(firstKey: string): string { return `g:${firstKey}`; }
  let unitRowKeys = $derived.by(() => {
    const m = new Map<string, string[]>();
    for (const it of displayItems) {
      if (it.kind === 'message') m.set(messageUnitId(it.key), [it.key]);
      else m.set(groupUnitId(it.keys[0]), it.keys);
    }
    return m;
  });
  // The atomic unit ids that make up each turn (its child message + group
  // units), so the turn checkbox can select/deselect the whole span at once.
  let turnUnitIds = $derived.by(() => {
    const m = new Map<string, string[]>();
    for (const span of turnSpans) {
      const set = new Set(span.keys);
      const ids: string[] = [];
      for (const it of displayItems) {
        const firstKey = it.kind === 'message' ? it.key : it.keys[0];
        if (set.has(firstKey)) ids.push(it.kind === 'message' ? messageUnitId(it.key) : groupUnitId(it.keys[0]));
      }
      m.set(span.keys[0], ids);
    }
    return m;
  });

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
        draft = buildDraft(raw, path, Math.floor(Date.now() / 1000));
      } catch (e) {
        loadError = e instanceof Error ? e.message : String(e);
      } finally {
        loading = false;
      }
    })();
  });

  // Ctrl/Cmd+F opens find-in-chat instead of the browser's own find bar;
  // Ctrl/Cmd+S saves (same confirm-then-write flow as the top nav's Save
  // button — no new save logic here); Escape closes whichever of this editor's
  // own modals is open (it does NOT drive the exit flow — attemptExit/
  // requestExit above is the only path that leaves the editor).
  onMount(() => {
    function closeTopModal(): boolean {
      if (titleRenameConfirming) { titleRenameConfirming = false; return true; }
      if (showDiscardModal) { showDiscardModal = false; return true; }
      if (showSaveModal) { showSaveModal = false; return true; }
      if (showExitModal) { showExitModal = false; return true; }
      if (selectMode) { exitSelectMode(); return true; }
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
          showSaveModal || showDiscardModal || showExitModal ||
          renamingTitle
        ) {
          return;
        }
        showSaveModal = true;
        return;
      }
      if (e.key === 'Escape') {
        if (closeTopModal()) e.preventDefault();
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

  // Mirror the save-state surface up to the top nav (the single home for these
  // controls). Save/Discard still route through the editor's own confirm
  // modals; Save-as-copy runs directly, matching the old SaveRail behaviour.
  $effect(() => { requestSave = () => { showSaveModal = true; }; });
  $effect(() => { requestSaveCopy = saveAsCopy; });
  $effect(() => { requestDiscard = () => { showDiscardModal = true; }; });
  $effect(() => { editorDirty = dirty; });
  $effect(() => { editorChangeCount = changeCount; });
  $effect(() => { editorSaving = saving; });

  function mutate(newDraft: Draft) {
    draft = newDraft;
  }

  // ── Toast ────────────────────────────────────────────────────────────────
  function showToast(msg: string) {
    toastMsg = msg;
    if (toastTimer) clearTimeout(toastTimer);
    toastTimer = setTimeout(() => { toastMsg = null; toastTimer = null; }, 3500);
  }

  // Clear any pending auto-dismiss timer on unmount so it can't fire against a
  // torn-down component or leak.
  onDestroy(() => { if (toastTimer) clearTimeout(toastTimer); });

  // ── Row mutations (wired to children) ────────────────────────────────────────
  function doBlockEdit(key: string, ordinal: number, text: string) {
    if (draft) mutate(applyBlockTextEdit(draft, key, ordinal, text));
  }

  // ── Soft delete (issue #14) ──────────────────────────────────────────────
  // A single block's delete op picks the right wrapper by block type; the
  // pairing cascade (tool_use ↔ tool_result — never an orphan) lives inside
  // editDraft.ts's deleteToolGroup/undelete, not here.
  function doDeleteBlock(key: string, blockIndex: number) {
    if (!draft) return;
    const rr = rmap.get(key);
    if (!rr) return;
    const bk = blockKey(rr.row, blockIndex);
    const block = rr.entry.blocks[blockIndex];
    if (!block) return;
    if (block.blockType === 'text') mutate(deleteMessage(draft, bk));
    else if (block.blockType === 'thinking') mutate(deleteThinking(draft, bk));
    else mutate(deleteToolGroup(draft, [bk])); // tool_use / tool_result — cascades to its pair
  }
  function doUndeleteBlock(key: string, blockIndex: number) {
    if (!draft) return;
    const rr = rmap.get(key);
    if (!rr) return;
    mutate(undelete(draft, [blockKey(rr.row, blockIndex)]));
  }

  // Expand a set of row keys to every block key they carry. The single source
  // of truth for "which block keys does this unit cover" — used by tool-group,
  // turn, and bulk deletes alike, so none of them hand-roll key math (the
  // originalIndex:contentIndex invariant lives only in editDraft.blockKey).
  function rowsBlockKeys(keys: string[]): string[] {
    const out: string[] = [];
    for (const rowKey of keys) {
      const rr = rmap.get(rowKey);
      if (!rr) continue;
      rr.entry.blocks.forEach((_, bi) => out.push(blockKey(rr.row, bi)));
    }
    return out;
  }
  /** True when every block across the given rows is already soft-deleted (drives
   *  the Delete↔Restore label flip for groups and turns). */
  function rowsAllDeleted(keys: string[]): boolean {
    if (!draft) return false;
    const bks = rowsBlockKeys(keys);
    return bks.length > 0 && bks.every((k) => draft!.deletedBlocks.has(k));
  }
  function doDeleteToolGroup(keys: string[]) {
    if (draft) mutate(deleteToolGroup(draft, rowsBlockKeys(keys)));
  }
  function doUndeleteToolGroup(keys: string[]) {
    if (draft) mutate(undelete(draft, rowsBlockKeys(keys)));
  }

  // ── Turn-level delete (issue #14 checkpoint 4) ───────────────────────────
  // A turn is a span of row keys (see turnSpans). Delete soft-deletes every
  // block in the span via deleteBulk (cascade-aware — an in-span tool_use
  // still pulls its tool_result); restore undeletes the same keys. Both go
  // through rowsBlockKeys → the block-key primitives, no bespoke key math.
  function doDeleteTurn(keys: string[]) {
    if (draft) mutate(deleteBulk(draft, rowsBlockKeys(keys)));
  }
  function doUndeleteTurn(keys: string[]) {
    if (draft) mutate(undelete(draft, rowsBlockKeys(keys)));
  }

  // ── Bulk multi-select (issue #14 checkpoint 5) ───────────────────────────
  function enterSelectMode() { selectMode = true; }
  function exitSelectMode() { selectMode = false; selectedUnits = new Set(); }
  function clearSelection() { selectedUnits = new Set(); }

  function toggleUnit(unitId: string) {
    const next = new Set(selectedUnits);
    if (next.has(unitId)) next.delete(unitId);
    else next.add(unitId);
    selectedUnits = next;
  }
  /** A turn is "selected" when all of its child units are selected. Toggling
   *  it flips the whole span in one shot. */
  function turnSelected(startKey: string): boolean {
    const ids = turnUnitIds.get(startKey) ?? [];
    return ids.length > 0 && ids.every((id) => selectedUnits.has(id));
  }
  function toggleTurn(startKey: string) {
    const ids = turnUnitIds.get(startKey) ?? [];
    const next = new Set(selectedUnits);
    if (ids.every((id) => next.has(id))) for (const id of ids) next.delete(id);
    else for (const id of ids) next.add(id);
    selectedUnits = next;
  }

  /** Soft-delete every block covered by the currently selected units. Cascade
   *  in deleteBulk still pulls any paired tool_result whose tool_use is in the
   *  selection (or vice versa). Reversible, so no confirm. */
  function deleteSelected() {
    if (!draft || selectedUnits.size === 0) return;
    const rowKeys = new Set<string>();
    for (const id of selectedUnits) {
      const rks = unitRowKeys.get(id);
      if (rks) for (const k of rks) rowKeys.add(k);
    }
    mutate(deleteBulk(draft, rowsBlockKeys([...rowKeys])));
    selectedUnits = new Set();
  }

  // Fork-from-here (issue #34): the terminal launcher is gone, so ⑂ forks the
  // session at `key` (keeps lines 0..=index under a fresh id, unchanged on disk)
  // and then opens the copy popover on the NEW forked session's facts at the
  // click point — a ready-to-paste `claude --resume <new-id>` the user runs in
  // their own terminal.
  let resumeMenu = $state<{ x: number; y: number; cwd: string; id: string } | null>(null);

  async function forkAndShowResume(e: MouseEvent, key: string) {
    // Stop the opening click here: ResumeMenu mounts with a
    // `<svelte:window onclick={onClose}>` outside-click guard, and without this
    // the very click that opened it bubbles to window and self-closes it before
    // paint (the button looked dead). The header Resume button in +page.svelte
    // guards the same way — mirror it, don't invent a second pattern.
    e.stopPropagation();
    if (!draft) return;
    const row = draft.rows[key];
    if (!row) return;
    const { clientX: x, clientY: y } = e;
    try {
      const forked = await forkSession(path, row.originalIndex);
      resumeMenu = { x, y, cwd: sessionInfo?.cwd ?? '', id: forked.id };
    } catch (err) {
      showToast(`Fork failed: ${err instanceof Error ? err.message : String(err)}`);
    }
  }
  function closeResumeMenu() {
    resumeMenu = null;
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

  // ── Save: overwrite original (with backup) ──────────────────────────────────
  async function confirmSave() {
    if (!draft) return;
    showSaveModal = false;
    saving = true;
    try {
      const bk = await snapshot(path);
      const content = serializeDraft(draft);
      await writeSession(path, content);
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
  function confirmDiscard() {
    showDiscardModal = false;
    draft = buildDraft(rawText, path, Math.floor(Date.now() / 1000));
    showToast('Edits discarded.');
  }

  // ── Exit (dirty guard, driven by parent's ← Back) ───────────────────────────
  function attemptExit() {
    if (!draft || !isDirty(draft)) {
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
  function exitDiscard() {
    showExitModal = false;
    onExit();
  }
</script>

{#if loading}
  <div class="empty-state">Loading session…</div>
{:else if loadError}
  <div class="empty-state">{loadError}</div>
{:else if draft}

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

  <!-- ── Find in chat + Select mode ─────────────────────────────────────────── -->
  {#if searchOpen}
    <InlineSearchPanel sessionPath={path} onJump={jumpTo} onClose={() => (searchOpen = false)} />
  {:else}
    <div class="find-toggle-row">
      <button class="btn btn--ghost btn--sm" onclick={() => (searchOpen = true)} type="button">
        🔍 Find in chat <span class="find-toggle-row__kbd">Ctrl+F</span>
      </button>
      {#if selectMode}
        <button
          class="btn btn--sm btn--danger"
          onclick={deleteSelected}
          disabled={selectedUnits.size === 0}
          type="button"
        >Delete selected ({selectedUnits.size})</button>
        <button
          class="btn btn--sm btn--ghost"
          onclick={clearSelection}
          disabled={selectedUnits.size === 0}
          type="button"
        >Clear selection</button>
        <button class="btn btn--sm btn--ghost" onclick={exitSelectMode} type="button">Done</button>
      {:else}
        <button class="btn btn--ghost btn--sm" onclick={enterSelectMode} type="button">☑ Select</button>
      {/if}
    </div>
  {/if}

  <!-- ── Messages ─────────────────────────────────────────────────────────── -->
  <div class="session-turns">
    {#each visibleItems as item, ii (item.kind === 'message' ? item.key : item.keys.join('|') + ':' + ii)}
      {@const startKey = item.kind === 'message' ? item.key : item.keys[0]}
      {@const turnKeys = turnByStartKey.get(startKey)}
      {#if turnKeys}
        <TurnDivider
          deleted={rowsAllDeleted(turnKeys)}
          selectMode={selectMode}
          selected={turnSelected(startKey)}
          onDelete={() => doDeleteTurn(turnKeys)}
          onUndelete={() => doUndeleteTurn(turnKeys)}
          onToggleSelect={() => toggleTurn(startKey)}
        />
      {/if}
      <div class="jump-anchor">
        {#if item.kind === 'message'}
          {@const rr = rmap.get(item.key)}
          {#if rr}
            <MessageCell
              row={rr.row}
              entry={rr.entry}
              deletedBlocks={draft.deletedBlocks}
              selectMode={selectMode}
              selected={selectedUnits.has(messageUnitId(item.key))}
              onToggleSelect={() => toggleUnit(messageUnitId(item.key))}
              onBlockEdit={(o, t) => doBlockEdit(item.key, o, t)}
              onDeleteBlock={(bi) => doDeleteBlock(item.key, bi)}
              onUndeleteBlock={(bi) => doUndeleteBlock(item.key, bi)}
              onResumeFrom={(e) => forkAndShowResume(e, item.key)}
            />
          {/if}
        {:else}
          <ToolGroup
            items={item.keys.map((k) => rmap.get(k)).filter((r) => r !== undefined)}
            deletedBlocks={draft.deletedBlocks}
            selectMode={selectMode}
            selected={selectedUnits.has(groupUnitId(item.keys[0]))}
            onToggleSelect={() => toggleUnit(groupUnitId(item.keys[0]))}
            onDeleteBlock={(rowKey, bi) => doDeleteBlock(rowKey, bi)}
            onUndeleteBlock={(rowKey, bi) => doUndeleteBlock(rowKey, bi)}
            onDeleteGroup={() => doDeleteToolGroup(item.keys)}
            onUndeleteGroup={() => doUndeleteToolGroup(item.keys)}
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

  <!-- ── Back to top ─────────────────────────────────────────────────────────── -->
  {#if showBackToTop}
    <button
      class="back-to-top" onclick={scrollToTop} type="button" aria-label="Back to top"
    >↑ Top</button>
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

<!-- ── Save (overwrite original) modal ───────────────────────────────────────── -->
{#if showSaveModal}
  <div class="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="save-title">
    <div class="modal">
      <h3 id="save-title">Save to original file</h3>
      <div class="modal__warning">
        This rewrites your real Claude chat history at:<br /><strong data-copy-text={path}>{path}</strong>
      </div>
      <p>A backup snapshot is saved first as insurance before your history is overwritten.</p>
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
      <p>This throws away every unsaved change. The original file is untouched.</p>
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

<!-- ── forked-session resume copy popover ───────────────────────────────────── -->
{#if resumeMenu}
  <ResumeMenu
    x={resumeMenu.x}
    y={resumeMenu.y}
    cwd={resumeMenu.cwd}
    sessionId={resumeMenu.id}
    heading="Forked — resume in your terminal"
    onCopied={(what) => showToast(`${what} copied to clipboard`)}
    onClose={closeResumeMenu}
  />
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
  .find-toggle-row { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; margin-bottom: 0.75rem; }
  .find-toggle-row__kbd { color: var(--text-faint); font-size: 0.7rem; margin-left: 0.3rem; }

  /* ── Back to top ──────────────────────────────────────────────  */
  /* Pinned bottom-right — the only floating control now that the save rail
     has moved up into the top nav. */
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

  /* ── Modal column actions ───────────────────────────────────── */
  .modal__actions--col { flex-direction: column; align-items: stretch; gap: 0.4rem; }
  .modal__actions--col .btn { width: 100%; justify-content: center; text-align: center; }
</style>
