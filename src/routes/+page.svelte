<script lang="ts">
  /**
   * +page.svelte — top-level SPA shell for CC Deck (Claude Code Control Center).
   *
   * States: browse | viewer | appconfig | prompts — search lives inside browse
   * (BrowseView.svelte). "appconfig" is CC Deck's own preferences.
   * Orchestrates: session loading, HTML/PDF export, resume-copy popover, theme.
   */
  import { onMount, tick } from 'svelte';
  import { getVersion } from '@tauri-apps/api/app';
  import { checkForUpdates, update as updateState } from '$lib/updater.svelte';
  import type { Session, SessionMeta, SearchHit } from '$lib/types';
  import { readSession, openSessionFile } from '$lib/api';
  import { parseJsonl, decodeProject } from '$lib/parser';
  import { buildSession } from '$lib/builder';
  import { extractSessionInfo } from '$lib/editDraft';
  import { getTheme, toggleTheme } from '$lib/theme';
  import { cleanFilename } from '$lib/markdown';
  import { sessionIdFromPath } from '$lib/resume';
  import BrowseView from '$lib/components/BrowseView.svelte';
  import SessionView from '$lib/components/SessionView.svelte';
  import SessionEditor from '$lib/components/SessionEditor.svelte';
  import AppConfigView from '$lib/components/AppConfigView.svelte';
  import PromptsView from '$lib/components/PromptsView.svelte';
  import ResumeMenu from '$lib/components/ResumeMenu.svelte';

  // Inline app.css for the standalone HTML export.
  import appCss from '../app.css?inline';

  // ── app state ─────────────────────────────────────────────────────────────
  // 'browse' is the home view — it merges what used to be separate Browse and
  // Search pages/views into one (see BrowseView.svelte).
  let view = $state<'browse' | 'viewer' | 'appconfig' | 'prompts'>('browse');
  let current = $state<Session | null>(null);
  let loading = $state(false);
  let loadError = $state<string | null>(null);
  let theme = $state(getTheme());

  // Jump-to-hit: message uuid to scroll to in the editor, bumped per open so
  // re-opening the same hit re-triggers the scroll.
  let scrollToUuid = $state<string | undefined>(undefined);
  let scrollNonce = $state(0);

  // App version for the footer — only available in the packaged desktop app.
  let appVersion = $state('');
  const isTauri = () =>
    typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

  // Header height varies with view (the viewer subtitle line adds a row), so we
  // measure it live rather than hardcode a value — see --header-h in app.css.
  let headerEl: HTMLElement | undefined = $state(undefined);

  onMount(async () => {
    if (isTauri()) {
      try {
        appVersion = await getVersion();
      } catch (e) {
        console.error('[app] getVersion failed', e);
      }
    }
  });

  onMount(() => {
    if (!headerEl) return;
    const setVar = () =>
      document.documentElement.style.setProperty('--header-h', `${headerEl!.offsetHeight}px`);
    setVar();
    const ro = new ResizeObserver(setVar);
    ro.observe(headerEl);
    return () => ro.disconnect();
  });

  // Ctrl/Cmd+K — global "go to search", from any view.
  onMount(() => {
    function onKeydown(e: KeyboardEvent) {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        goToSearch();
      }
    }
    window.addEventListener('keydown', onKeydown);
    return () => window.removeEventListener('keydown', onKeydown);
  });

  function handleCheckForUpdates(): void {
    if (isTauri()) checkForUpdates(false);
  }

  // DOM ref for the read-only export render — used by exportHtml().
  let exportEl: HTMLDivElement | undefined = $state(undefined);
  // Lazily mount the read-only SessionView (only while exporting) so we never
  // pay to render the whole conversation twice during normal editing.
  let exporting = $state(false);

  // Exit guard installed by SessionEditor — the header ← Back calls this so the
  // editor can prompt about unsaved edits before we navigate away.
  let requestEditorExit = $state<(() => void) | undefined>(undefined);

  // Save-state surface, hoisted out of SessionEditor so the dirty indicator +
  // Save / Save as copy / Discard controls live in the top nav (the one place
  // that manages editor state). The editor owns the logic; these bindings just
  // mirror its state up and let the nav buttons trigger its save flows.
  let requestEditorSave = $state<(() => void) | undefined>(undefined);
  let requestEditorSaveCopy = $state<(() => void) | undefined>(undefined);
  let requestEditorDiscard = $state<(() => void) | undefined>(undefined);
  let editorDirty = $state(false);
  let editorChangeCount = $state(0);
  let editorSaving = $state(false);

  // Ctrl/Cmd+K "go to search" — set right before we ask to leave the viewer
  // (which may show a dirty-guard prompt) or App Config, then consumed by
  // backToBrowse() once we actually land back on the browse view.
  let focusSearchPending = false;

  // ── session opening ───────────────────────────────────────────────────────
  async function loadSession(
    path: string,
    project: string,
    scroll: string | undefined
  ): Promise<void> {
    loading = true;
    loadError = null;
    try {
      const text = await readSession(path);
      const entries = parseJsonl(text);
      const session = buildSession(entries, { project, sourcePath: path });
      session.meta.cwd = extractSessionInfo(text).cwd;
      current = session;
      scrollToUuid = scroll;
      scrollNonce++;
      view = 'viewer';
    } catch (e) {
      loadError = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  function openSession(meta: SessionMeta): void {
    loadSession(meta.path, decodeProject(meta.project_raw), undefined);
  }

  // Open the session for a search hit and scroll to the matched message.
  function openHit(hit: SearchHit): void {
    loadSession(hit.sessionPath, hit.project, hit.uuid);
  }

  // Open App Config — a single global-scope page (launch command / terminal /
  // update toggle are app-level preferences, not per-project), so there's no
  // cwd/label to thread through.
  function goAppConfig(): void {
    view = 'appconfig';
    loadError = null;
  }

  // Open the Prompt Library (issue #24) — its store keeps the compose draft
  // alive across view switches, so this is a plain view swap.
  function goPrompts(): void {
    view = 'prompts';
    loadError = null;
  }

  function backToBrowse(): void {
    view = 'browse';
    current = null;
    loadError = null;
    requestEditorExit = undefined;
    // Reset the mirrored save state so a stale dirty indicator can't flash
    // before the next editor mounts and re-syncs.
    requestEditorSave = undefined;
    requestEditorSaveCopy = undefined;
    requestEditorDiscard = undefined;
    editorDirty = false;
    editorChangeCount = 0;
    editorSaving = false;
    if (focusSearchPending) {
      focusSearchPending = false;
      tick().then(focusBrowseSearch);
    }
  }

  // Header ← Back: let the editor handle unsaved-edit prompting first.
  function handleBack(): void {
    if (requestEditorExit) requestEditorExit();
    else backToBrowse();
  }

  // ── Ctrl/Cmd+K: jump to Browse + focus its search input ─────────────────────
  function focusBrowseSearch(): void {
    const el = document.getElementById('browse-search-input') as HTMLInputElement | null;
    el?.focus();
    el?.select();
  }
  function goToSearch(): void {
    if (view === 'browse') {
      focusBrowseSearch();
      return;
    }
    // Not on browse yet — flag it so whichever path lands us back on browse
    // (dirty-guarded exit from the viewer, or a plain App Config/Settings close)
    // focuses the search input once it actually mounts.
    focusSearchPending = true;
    if (view === 'viewer') handleBack();
    else backToBrowse();
  }

  // ── theme ─────────────────────────────────────────────────────────────────
  function handleToggleTheme(): void {
    theme = toggleTheme();
  }

  // ── HTML export ───────────────────────────────────────────────────────────
  // Renders the read-only SessionView into a hidden node, captures its markup,
  // then unmounts it — the editor chrome never leaks into the exported file.
  async function exportHtml(): Promise<void> {
    if (!current) return;
    exporting = true;
    await tick();
    if (!exportEl) { exporting = false; return; }

    const title = current.meta.title;
    const project = current.meta.project;
    const dataTheme = document.documentElement.getAttribute('data-theme') ?? 'light';
    const contentHtml = exportEl.innerHTML;
    exporting = false;

    const htmlDoc = `<!DOCTYPE html>
<html lang="en" data-theme="${dataTheme}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${title.replace(/</g, '&lt;').replace(/>/g, '&gt;')}</title>
<style>
${appCss}
</style>
</head>
<body>
<div class="container-main">
${contentHtml}
</div>
</body>
</html>`;

    const fname = cleanFilename(project || 'project') + '_' + cleanFilename(title || 'session') + '.html';
    const blob = new Blob([htmlDoc], { type: 'text/html;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = fname;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  }

  // ── View raw file ─────────────────────────────────────────────────────────
  let fileOpenError = $state<string | null>(null);
  let fileOpenErrorTimer: ReturnType<typeof setTimeout> | null = null;

  async function viewFile(): Promise<void> {
    if (!current) return;
    try {
      await openSessionFile(current.meta.sourcePath);
    } catch (e) {
      fileOpenError = e instanceof Error ? e.message : String(e);
      if (fileOpenErrorTimer) clearTimeout(fileOpenErrorTimer);
      fileOpenErrorTimer = setTimeout(() => { fileOpenError = null; fileOpenErrorTimer = null; }, 3500);
    }
  }

  // ── Resume: copyable-facts popover ──────────────────────────────────────────
  // The terminal launcher was removed (issue #34). The header Resume button now
  // opens a small popover of the session's copyable facts at the click point.
  let resumeMsg = $state<string | null>(null);
  let resumeMsgTimer: ReturnType<typeof setTimeout> | null = null;
  let resumeMenu = $state<{ x: number; y: number; cwd: string; id: string } | null>(null);

  function showResumeMsg(msg: string): void {
    resumeMsg = msg;
    if (resumeMsgTimer) clearTimeout(resumeMsgTimer);
    resumeMsgTimer = setTimeout(() => { resumeMsg = null; resumeMsgTimer = null; }, 3500);
  }

  function openResumeMenu(e: MouseEvent): void {
    if (!current) return;
    e.preventDefault();
    e.stopPropagation();
    resumeMenu = {
      x: e.clientX,
      y: e.clientY,
      cwd: current.meta.cwd,
      id: sessionIdFromPath(current.meta.sourcePath),
    };
  }

  // ── PDF export ──────────────────────────────────────────────────────────────
  // Reuses the same clean read-only SessionView render as HTML export, revealed
  // for print via an @media print stylesheet (app.css) that hides the app chrome
  // so the OS "Save as PDF" produces a clean document. window.print() is the only
  // path — no bundled renderer. See report note: on some Linux webviews (wry /
  // webkit2gtk) window.print() can be a no-op; the HTML export remains the
  // guaranteed fallback per issue #36.
  let printing = $state(false);

  async function exportPdf(): Promise<void> {
    if (!current) return;
    printing = true;
    await tick();
    try {
      window.print();
    } finally {
      // afterprint (below) also clears this; belt-and-suspenders for webviews
      // that return synchronously without firing the event.
      printing = false;
    }
  }

  onMount(() => {
    const done = () => { printing = false; };
    window.addEventListener('afterprint', done);
    return () => window.removeEventListener('afterprint', done);
  });
</script>

<!-- ── header ──────────────────────────────────────────────────────────────── -->
<header class="app-header" bind:this={headerEl}>
  <div>
    <h1>CC Deck</h1>
    {#if view === 'viewer' && current}
      <div class="subtitle">
        {current.meta.project} · {current.turns.length} turn{current.turns.length === 1 ? '' : 's'}
      </div>
    {/if}
  </div>

  <div class="app-header__actions">
    {#if view === 'browse'}
      <button class="btn btn--ghost btn--sm" onclick={goPrompts} type="button">
        ✎ Prompts
      </button>
      <button class="btn btn--ghost btn--sm" onclick={goAppConfig} type="button">
        ⚙ App Config
      </button>
    {:else if view === 'appconfig' || view === 'prompts'}
      <button class="btn btn--ghost btn--sm" onclick={backToBrowse} type="button">
        ← Back
      </button>
    {:else if view === 'viewer'}
      <button class="btn btn--ghost btn--sm" onclick={handleBack} type="button">
        ← Back
      </button>
      {#if editorDirty}
        <span class="editor-dirty">
          {editorChangeCount} unsaved {editorChangeCount === 1 ? 'change' : 'changes'}
        </span>
        <button class="btn btn--sm btn--primary" onclick={() => requestEditorSave?.()} disabled={editorSaving} type="button">
          Save
        </button>
        <button class="btn btn--sm" onclick={() => requestEditorSaveCopy?.()} disabled={editorSaving} type="button">
          Save as copy
        </button>
        <button class="btn btn--ghost btn--sm" onclick={() => requestEditorDiscard?.()} disabled={editorSaving} type="button">
          Discard
        </button>
      {/if}
      <button class="btn btn--sm" onclick={exportHtml} type="button">
        Export HTML
      </button>
      <button class="btn btn--sm" onclick={exportPdf} type="button">
        Export PDF
      </button>
      <button
        class="btn btn--ghost btn--sm"
        onclick={openResumeMenu}
        oncontextmenu={openResumeMenu}
        type="button"
        title="Copy the resume command, project path, or session id"
      >
        Resume
      </button>
      <button
        class="btn btn--ghost btn--sm"
        onclick={viewFile}
        type="button"
        title={current?.meta.sourcePath}
      >
        View File
      </button>
    {/if}
    <button class="btn btn--ghost btn--sm" onclick={handleToggleTheme} type="button">
      {theme === 'dark' ? 'Dark' : 'Light'}
    </button>
  </div>
</header>

<!-- ── main content ────────────────────────────────────────────────────────── -->
<main class="container-main">
  {#if loadError}
    <div class="empty-state">{loadError}</div>
  {:else if loading}
    <div class="empty-state">Loading session...</div>
  {:else if view === 'browse'}
    <BrowseView onOpen={openSession} onJump={openHit} />
  {:else if view === 'prompts'}
    <PromptsView />
  {:else if view === 'appconfig'}
    <AppConfigView onClose={backToBrowse} />
  {:else if view === 'viewer' && current}
    <SessionEditor
      path={current.meta.sourcePath}
      {scrollToUuid}
      {scrollNonce}
      onExit={backToBrowse}
      bind:requestExit={requestEditorExit}
      bind:requestSave={requestEditorSave}
      bind:requestSaveCopy={requestEditorSaveCopy}
      bind:requestDiscard={requestEditorDiscard}
      bind:editorDirty
      bind:editorChangeCount
      bind:editorSaving
    />
    {#if exporting}
      <!-- Hidden read-only render captured by exportHtml(), then unmounted -->
      <div bind:this={exportEl} style="display:none;" aria-hidden="true">
        <SessionView session={current} />
      </div>
    {/if}
  {/if}
</main>

<!-- Print-only clean render for Export PDF. Hidden on screen (.print-only), shown
     only in @media print (app.css), where the app chrome is hidden — so the OS
     "Save as PDF" captures just the conversation. Lazily mounted while printing
     so we never pay to render the whole conversation twice during normal use. -->
{#if printing && current}
  <div class="print-only" aria-hidden="true">
    <div class="container-main">
      <SessionView session={current} />
    </div>
  </div>
{/if}

<!-- ── footer ──────────────────────────────────────────────────────────────── -->
<footer class="app-footer">
  <a href="https://github.com/zhangxingeng/ccdeck" target="_blank" rel="noopener noreferrer">
    CC Deck{appVersion ? ` v${appVersion}` : ''} — offline, open-source control center for Claude Code
  </a>
  <button
    class="app-footer__check"
    onclick={handleCheckForUpdates}
    disabled={updateState.status === 'checking' || updateState.status === 'downloading'}
    type="button"
  >
    Check for updates
  </button>
</footer>

<!-- ── View File error toast ──────────────────────────────────────────────── -->
{#if fileOpenError}
  <div class="toast" role="status">Couldn't open file: {fileOpenError}</div>
{/if}
{#if resumeMsg}
  <div class="toast" role="status">{resumeMsg}</div>
{/if}

<!-- ── resume copy popover ──────────────────────────────────────────────────── -->
{#if resumeMenu}
  <ResumeMenu
    x={resumeMenu.x}
    y={resumeMenu.y}
    cwd={resumeMenu.cwd}
    sessionId={resumeMenu.id}
    onCopied={(what) => showResumeMsg(`${what} copied to clipboard`)}
    onClose={() => (resumeMenu = null)}
  />
{/if}

<style>
  /* Unsaved-change indicator in the top nav (moved here from the old floating
     SaveRail — the nav is now the single home for editor save state). */
  .editor-dirty {
    align-self: center;
    font-size: 0.72rem;
    font-weight: 600;
    color: var(--accent-user);
    white-space: nowrap;
  }
</style>
