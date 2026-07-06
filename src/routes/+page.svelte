<script lang="ts">
  /**
   * +page.svelte — top-level SPA shell for CC Deck (Claude Code Control Center).
   *
   * States: browse | viewer | settings — search lives inside browse (BrowseView.svelte)
   * Orchestrates: session loading, subagent linking, HTML export, theme toggle.
   */
  import { onMount, tick } from 'svelte';
  import { getVersion } from '@tauri-apps/api/app';
  import { checkForUpdates, update as updateState } from '$lib/updater.svelte';
  import type { Session, SessionMeta, SearchHit } from '$lib/types';
  import { readSession, readSubagents, openSessionFile, resumeInTerminal } from '$lib/api';
  import { parseJsonl, decodeProject } from '$lib/parser';
  import { buildSession, linkSubagents } from '$lib/builder';
  import { extractSessionInfo } from '$lib/editDraft';
  import { getTheme, toggleTheme } from '$lib/theme';
  import { cleanFilename } from '$lib/markdown';
  import { sessionIdFromPath, resumeCommand } from '$lib/resume';
  import { copyToClipboard } from '$lib/copy';
  import BrowseView from '$lib/components/BrowseView.svelte';
  import SessionView from '$lib/components/SessionView.svelte';
  import SessionEditor from '$lib/components/SessionEditor.svelte';
  import SettingsView from '$lib/components/SettingsView.svelte';

  // Inline app.css for the standalone HTML export.
  import appCss from '../app.css?inline';

  // ── app state ─────────────────────────────────────────────────────────────
  // 'browse' is the home view — it merges what used to be separate Browse and
  // Search pages/views into one (see BrowseView.svelte).
  let view = $state<'browse' | 'viewer' | 'settings'>('browse');
  // Settings view scope: null = user/global; otherwise a specific project's real cwd.
  let settingsProjectCwd = $state<string | null>(null);
  let settingsProjectLabel = $state('');
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

  // Ctrl/Cmd+K "go to search" — set right before we ask to leave the viewer
  // (which may show a dirty-guard prompt) or settings, then consumed by
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
      const subagentFiles = await readSubagents(path);
      linkSubagents(session, subagentFiles);
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

  // Open Settings: cwd=null for user/global; a real project cwd scopes it to
  // that project's tiers (called from the header gear or a project's gear).
  function goSettings(cwd: string | null, label = ''): void {
    settingsProjectCwd = cwd;
    settingsProjectLabel = label;
    view = 'settings';
    loadError = null;
  }

  function backToBrowse(): void {
    view = 'browse';
    current = null;
    loadError = null;
    requestEditorExit = undefined;
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
    // (dirty-guarded exit from the viewer, or a plain settings close) focuses
    // the search input once it actually mounts.
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

  // ── Resume in claude --resume ───────────────────────────────────────────────
  let resumeMsg = $state<string | null>(null);
  let resumeMsgTimer: ReturnType<typeof setTimeout> | null = null;

  function showResumeMsg(msg: string): void {
    resumeMsg = msg;
    if (resumeMsgTimer) clearTimeout(resumeMsgTimer);
    resumeMsgTimer = setTimeout(() => { resumeMsg = null; resumeMsgTimer = null; }, 3500);
  }

  async function resumeSession(): Promise<void> {
    if (!current) return;
    const id = sessionIdFromPath(current.meta.sourcePath);
    const cwd = current.meta.cwd;
    await copyToClipboard(resumeCommand(cwd, id));
    try {
      await resumeInTerminal(cwd, id);
      showResumeMsg('Opened in a terminal — command also copied to clipboard');
    } catch {
      showResumeMsg('Could not open a terminal — command copied to clipboard instead');
    }
  }
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
      <button class="btn btn--ghost btn--sm" onclick={() => goSettings(null)} type="button">
        ⚙ Settings
      </button>
    {:else if view === 'settings'}
      <button class="btn btn--ghost btn--sm" onclick={backToBrowse} type="button">
        ← Back
      </button>
    {:else if view === 'viewer'}
      <button class="btn btn--ghost btn--sm" onclick={handleBack} type="button">
        ← Back
      </button>
      <button class="btn btn--sm" onclick={exportHtml} type="button">
        Export HTML
      </button>
      <button
        class="btn btn--ghost btn--sm"
        onclick={resumeSession}
        type="button"
        title={current ? resumeCommand(current.meta.cwd, sessionIdFromPath(current.meta.sourcePath)) : ''}
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
    <BrowseView onOpen={openSession} onJump={openHit} onOpenSettings={goSettings} />
  {:else if view === 'settings'}
    <SettingsView projectCwd={settingsProjectCwd} projectLabel={settingsProjectLabel} onClose={backToBrowse} />
  {:else if view === 'viewer' && current}
    <SessionEditor
      path={current.meta.sourcePath}
      {scrollToUuid}
      {scrollNonce}
      onExit={backToBrowse}
      bind:requestExit={requestEditorExit}
    />
    {#if exporting}
      <!-- Hidden read-only render captured by exportHtml(), then unmounted -->
      <div bind:this={exportEl} style="display:none;" aria-hidden="true">
        <SessionView session={current} />
      </div>
    {/if}
  {/if}
</main>

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
