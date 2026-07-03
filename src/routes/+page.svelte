<script lang="ts">
  /**
   * +page.svelte — top-level SPA shell for Claude Code Studio.
   *
   * States: browse | viewer
   * Orchestrates: session loading, subagent linking, HTML export, theme toggle.
   */
  import { tick } from 'svelte';
  import type { Session, SessionMeta } from '$lib/types';
  import { readSession, readSubagents, openSessionFile } from '$lib/api';
  import { parseJsonl, decodeProject } from '$lib/parser';
  import { buildSession, linkSubagents } from '$lib/builder';
  import { getTheme, toggleTheme } from '$lib/theme';
  import { cleanFilename } from '$lib/markdown';
  import BrowseView from '$lib/components/BrowseView.svelte';
  import SessionView from '$lib/components/SessionView.svelte';
  import SessionEditor from '$lib/components/SessionEditor.svelte';

  // Inline app.css for the standalone HTML export.
  import appCss from '../app.css?inline';

  // ── app state ─────────────────────────────────────────────────────────────
  let view = $state<'browse' | 'viewer'>('browse');
  let current = $state<Session | null>(null);
  let loading = $state(false);
  let loadError = $state<string | null>(null);
  let theme = $state(getTheme());

  // DOM ref for the read-only export render — used by exportHtml().
  let exportEl: HTMLDivElement | undefined = $state(undefined);
  // Lazily mount the read-only SessionView (only while exporting) so we never
  // pay to render the whole conversation twice during normal editing.
  let exporting = $state(false);

  // Exit guard installed by SessionEditor — the header ← Back calls this so the
  // editor can prompt about unsaved edits before we navigate away.
  let requestEditorExit = $state<(() => void) | undefined>(undefined);

  // ── session opening ───────────────────────────────────────────────────────
  async function openSession(meta: SessionMeta): Promise<void> {
    loading = true;
    loadError = null;
    try {
      const text = await readSession(meta.path);
      const entries = parseJsonl(text);
      const session = buildSession(entries, {
        project: decodeProject(meta.project_raw),
        sourcePath: meta.path,
      });
      const subagentFiles = await readSubagents(meta.path);
      linkSubagents(session, subagentFiles);
      current = session;
      view = 'viewer';
    } catch (e) {
      loadError = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  function backToBrowse(): void {
    view = 'browse';
    current = null;
    loadError = null;
    requestEditorExit = undefined;
  }

  // Header ← Back: let the editor handle unsaved-edit prompting first.
  function handleBack(): void {
    if (requestEditorExit) requestEditorExit();
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
</script>

<!-- ── header ──────────────────────────────────────────────────────────────── -->
<header class="app-header">
  <div>
    <h1>Claude Code Studio</h1>
    {#if view === 'viewer' && current}
      <div class="subtitle">
        {current.meta.project} · {current.turns.length} turn{current.turns.length === 1 ? '' : 's'}
      </div>
    {/if}
  </div>

  <div class="app-header__actions">
    {#if view === 'viewer'}
      <button class="btn btn--ghost btn--sm" onclick={handleBack} type="button">
        ← Back
      </button>
      <button class="btn btn--sm" onclick={exportHtml} type="button">
        Export HTML
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
    <BrowseView onOpen={openSession} />
  {:else if view === 'viewer' && current}
    <SessionEditor
      path={current.meta.sourcePath}
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
  <a href="https://github.com/zhangxingeng/claude-code-studio" target="_blank" rel="noopener noreferrer">
    Claude Code Studio — offline, open-source chat history viewer
  </a>
</footer>

<!-- ── View File error toast ──────────────────────────────────────────────── -->
{#if fileOpenError}
  <div class="toast" role="status">Couldn't open file: {fileOpenError}</div>
{/if}
