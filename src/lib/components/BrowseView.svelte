<script lang="ts">
  /**
   * BrowseView.svelte — session browser.
   *
   * Loads listSessions(), enriches with extractMeta(), groups by home-relative
   * project path (from the session's real cwd — see projectLabel()), supports
   * search (title/project) and sort (newest/oldest/title).
   * Calls onOpen(meta) when the user selects a session.
   * Provides a per-card Rename action (double-confirm, no backup).
   */
  import { onMount } from 'svelte';
  import type { SessionMeta } from '$lib/types';
  import { listSessions, homeDir as fetchHomeDir } from '$lib/api';
  import { extractMeta, projectLabel, cleanTitle } from '$lib/parser';
  import { renameSession } from '$lib/sessionOps';

  let { onOpen }: { onOpen: (meta: SessionMeta) => void } = $props();

  // ── state ──────────────────────────────────────────────────────────────────
  let sessions = $state<SessionMeta[]>([]);
  let loadError = $state<string | null>(null);
  let loading = $state(true);
  let search = $state('');
  let sortBy = $state<'newest' | 'oldest' | 'title'>('newest');
  /** Home directory, used to render project paths as "~/...". Null until loaded. */
  let homeDir = $state<string | null>(null);

  /** Per-id title overrides applied after a successful rename. */
  let renamedTitles = $state<Record<string, string>>({});

  /** Id of the card currently showing the inline rename editor. */
  let renamingId = $state<string | null>(null);
  /** Current value of the rename text input. */
  let renameInput = $state('');
  /** Pending double-confirm data (set when user clicks Save in the editor). */
  let confirmPending = $state<{ id: string; path: string } | null>(null);
  /** Error message shown inside the inline editor. */
  let renameError = $state<string | null>(null);

  /** Toast message (shown for ~2.5 s after a successful rename). */
  let toast = $state<string | null>(null);
  let toastTimer: number | null = null;

  // ── lifecycle ───────────────────────────────────────────────────────────────
  onMount(async () => {
    try {
      sessions = await listSessions();
    } catch (e) {
      loadError = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
    // Best-effort: falls back to decoded project names if this fails.
    try {
      homeDir = await fetchHomeDir();
    } catch {
      // ignore
    }
  });

  // ── derived data ────────────────────────────────────────────────────────────

  /** Sessions enriched with extracted meta (title, date, model, project). */
  let enriched = $derived(
    sessions.map((s) => {
      const m = extractMeta(s.preview);
      return {
        meta: s,
        title: renamedTitles[s.id] ?? cleanTitle(m.title),
        date: m.date,
        model: m.model,
        project: projectLabel(s.cwd, s.project_raw, homeDir),
      };
    })
  );

  /** After search filter. */
  let filtered = $derived(
    enriched.filter((s) => {
      if (!search.trim()) return true;
      const q = search.trim().toLowerCase();
      return s.title.toLowerCase().includes(q) || s.project.toLowerCase().includes(q);
    })
  );

  /** After sort. */
  let sorted = $derived(
    [...filtered].sort((a, b) => {
      if (sortBy === 'newest') return (b.date || '').localeCompare(a.date || '');
      if (sortBy === 'oldest') return (a.date || '').localeCompare(b.date || '');
      return a.title.localeCompare(b.title);
    })
  );

  /** Grouped by project name → entries. */
  let groups = $derived.by(() => {
    const g = new Map<string, typeof sorted>();
    for (const s of sorted) {
      const existing = g.get(s.project);
      if (existing) {
        existing.push(s);
      } else {
        g.set(s.project, [s]);
      }
    }
    return g;
  });

  // ── helpers ─────────────────────────────────────────────────────────────────
  function fmtDate(ts: string): string {
    if (!ts) return '';
    try {
      return new Date(ts).toLocaleDateString(undefined, {
        month: 'short',
        day: 'numeric',
        year: 'numeric',
      });
    } catch {
      return ts;
    }
  }

  function fmtModel(model: string): string {
    // Trim anything after '[' (usage info appended by some Claude versions).
    return model ? model.replace(/\[.*/, '').trim() : '';
  }

  function humanSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  function fmtDateRange(firstTs: string, lastTs: string, fallback: string): string {
    if (!firstTs) return fmtDate(fallback);
    try {
      const d1 = new Date(firstTs);
      const d2 = lastTs ? new Date(lastTs) : d1;
      const opts: Intl.DateTimeFormatOptions = { month: 'short', day: 'numeric' };
      const s1 = d1.toLocaleDateString(undefined, opts);
      if (
        d1.getFullYear() === d2.getFullYear() &&
        d1.getMonth() === d2.getMonth() &&
        d1.getDate() === d2.getDate()
      ) {
        return s1;
      }
      const s2 = d2.toLocaleDateString(undefined, opts);
      return `${s1} – ${s2}`;
    } catch {
      return fmtDate(fallback);
    }
  }

  function sessionStats(meta: import('$lib/types').SessionMeta, model: string, date: string): string {
    const parts: string[] = [];
    const uc = meta.user_count;
    parts.push(`${uc} ${uc === 1 ? 'turn' : 'turns'}`);
    if (meta.subagent_count > 0) parts.push(`${meta.subagent_count} subagents`);
    parts.push(humanSize(meta.size));
    const dateStr = fmtDateRange(meta.first_ts, meta.last_ts, date);
    if (dateStr) parts.push(dateStr);
    const mdl = fmtModel((meta.models && meta.models.length > 0 ? meta.models[0] : '') || model);
    if (mdl) parts.push(mdl);
    return parts.join(' · ');
  }

  // ── rename helpers ──────────────────────────────────────────────────────────

  function startRename(id: string, currentTitle: string) {
    renamingId = id;
    renameInput = currentTitle;
    renameError = null;
  }

  function cancelRename() {
    renamingId = null;
    renameInput = '';
    renameError = null;
    confirmPending = null;
  }

  function requestSaveRename(id: string, path: string) {
    const t = renameInput.trim();
    if (!t) {
      renameError = 'Title cannot be empty.';
      return;
    }
    renameError = null;
    confirmPending = { id, path };
  }

  async function confirmRename() {
    if (!confirmPending) return;
    const { id, path } = confirmPending;
    const newTitle = renameInput.trim();
    confirmPending = null;
    try {
      await renameSession(path, newTitle);
      renamedTitles[id] = newTitle;
      renamingId = null;
      renameInput = '';
      showToast('Renamed.');
    } catch (e) {
      renameError = e instanceof Error ? e.message : String(e);
    }
  }

  function showToast(msg: string) {
    toast = msg;
    if (toastTimer !== null) clearTimeout(toastTimer);
    toastTimer = setTimeout(() => {
      toast = null;
      toastTimer = null;
    }, 2500) as unknown as number;
  }
</script>

<!-- ── toolbar ────────────────────────────────────────────────────────────── -->
<div class="toolbar">
  <input
    type="search"
    placeholder="Search sessions or projects..."
    bind:value={search}
    aria-label="Search sessions"
  />
  <select bind:value={sortBy} aria-label="Sort order">
    <option value="newest">Newest</option>
    <option value="oldest">Oldest</option>
    <option value="title">Title</option>
  </select>
</div>

<!-- ── content ───────────────────────────────────────────────────────────── -->
{#if loading}
  <div class="empty-state">Loading sessions...</div>
{:else if loadError}
  <div class="empty-state">{loadError}</div>
{:else if groups.size === 0}
  <div class="empty-state">
    {search.trim() ? 'No sessions match your search.' : 'No sessions found.'}
  </div>
{:else}
  {#each groups as [project, items]}
    <div class="project-group">
      <div class="project-group__name" title={project} data-copy-text={project}>{project}</div>

      {#each items as s (s.meta.id)}
        <div class="session-card" class:session-card--editing={renamingId === s.meta.id}>
          {#if renamingId === s.meta.id}
            <!-- ── inline rename editor ──────────────────────────────────── -->
            <div class="rename-editor">
              <input
                type="text"
                class="rename-input"
                bind:value={renameInput}
                aria-label="New session title"
                onkeydown={(e) => {
                  if (e.key === 'Enter') requestSaveRename(s.meta.id, s.meta.path);
                  if (e.key === 'Escape') cancelRename();
                }}
              />
              {#if renameError}
                <p class="rename-error">{renameError}</p>
              {/if}
              <div class="rename-actions">
                <button
                  type="button"
                  class="btn btn--sm btn--primary"
                  onclick={() => requestSaveRename(s.meta.id, s.meta.path)}
                >Save</button>
                <button
                  type="button"
                  class="btn btn--sm btn--ghost"
                  onclick={cancelRename}
                >Cancel</button>
              </div>
            </div>
          {:else}
            <!-- ── normal card row ───────────────────────────────────────── -->
            <button
              class="session-card__open"
              type="button"
              onclick={() => onOpen(s.meta)}
            >
              <span class="session-card__title" title={s.title} data-copy-text={s.title}>{s.title}</span>
              <span class="session-card__stats">{sessionStats(s.meta, s.model, s.date)}</span>
            </button>
            <button
              type="button"
              class="btn btn--ghost btn--sm rename-btn"
              onclick={(e) => { e.stopPropagation(); startRename(s.meta.id, s.title); }}
              aria-label="Rename session"
            >Rename</button>
          {/if}
        </div>
      {/each}
    </div>
  {/each}
{/if}

<!-- ── double-confirm modal ──────────────────────────────────────────────── -->
{#if confirmPending}
  <div class="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="rename-modal-title">
    <div class="modal">
      <h3 id="rename-modal-title">Rename this session?</h3>
      <p>This updates the title saved in the chat file.</p>
      <div class="modal__warning">
        This change is not backed up and cannot be undone here.
      </div>
      <div class="modal__actions">
        <button type="button" class="btn btn--ghost" onclick={() => { confirmPending = null; }}>
          Cancel
        </button>
        <button type="button" class="btn btn--primary" onclick={confirmRename}>
          Rename
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- ── toast ─────────────────────────────────────────────────────────────── -->
{#if toast}
  <div class="toast" role="status" aria-live="polite">{toast}</div>
{/if}

<style>
  /* The outer card is now a div; give it the same flex layout as before. */
  .session-card {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  /* When the rename editor is open, disable the hover highlight cursor. */
  .session-card--editing {
    cursor: default;
  }

  /* Open-area button fills available width; resets button chrome. */
  .session-card__open {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.15rem;
    background: none;
    border: 0;
    padding: 0;
    cursor: pointer;
    font-family: inherit;
    color: inherit;
    text-align: left;
  }

  /* Compact stats row beneath the card title. */
  .session-card__stats {
    font-size: 0.73rem;
    color: var(--text-muted);
    line-height: 1.4;
    white-space: normal;
    word-break: break-word;
    opacity: 0.85;
  }

  /* Rename button: hidden until the card is hovered. */
  .rename-btn {
    flex-shrink: 0;
    opacity: 0;
    transition: opacity 0.1s;
  }
  .session-card:hover .rename-btn {
    opacity: 1;
  }

  /* Inline rename editor layout. */
  .rename-editor {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    padding: 0.15rem 0;
  }

  .rename-input {
    width: 100%;
    padding: 0.4rem 0.65rem;
    border-radius: 0.35rem;
    border: 1px solid var(--border-strong);
    background: var(--bg-subtle);
    color: var(--text);
    font-family: var(--font-sans);
    font-size: 0.85rem;
    line-height: 1.4;
  }
  .rename-input:focus {
    outline: 2px solid var(--accent-user);
    outline-offset: 1px;
  }

  .rename-actions {
    display: flex;
    gap: 0.4rem;
  }

  .rename-error {
    font-size: 0.75rem;
    color: var(--accent-result-err);
    margin: 0;
  }
</style>
