<script lang="ts">
  /**
   * BrowseView.svelte — the home view: browse + search merged into one.
   *
   * No query: shows every session grouped by project (title, stats, sort,
   * inline rename) — what used to be the whole of this file.
   * Query typed: same project grouping, but each project's sessions collapse
   * to just the ones with a hit, each showing its matched lines underneath
   * (title instead of the raw jsonl filename), reusing the exact same search
   * engine/store `SearchView.svelte` used to own alone. There is no more a
   * separate "basic" search box or a separate Search page — this is the one
   * search surface, always visible, fuzzy/intent-matched by default (no
   * case/whole-word/regex mode — see issue #5).
   */
  import { onMount, onDestroy, tick } from 'svelte';
  import type { SessionMeta, SearchHit } from '$lib/types';
  import { listSessions, homeDir as fetchHomeDir, resumeInTerminal } from '$lib/api';
  import { extractMeta, projectLabel, cleanTitle } from '$lib/parser';
  import { renameSession } from '$lib/sessionOps';
  import { copyToClipboard } from '$lib/copy';
  import { sessionIdFromPath, resumeCommand } from '$lib/resume';
  import {
    search,
    setQuery,
    toggleSource,
    toggleProject,
    clearProjects,
    setDateRange,
    setToolName,
    scheduleSearch,
    loadMore,
    initSearch,
    disposeSearch,
  } from '$lib/search.svelte';

  let {
    onOpen,
    onJump,
    onOpenSettings,
  }: {
    onOpen: (meta: SessionMeta) => void;
    onJump: (hit: SearchHit) => void;
    onOpenSettings?: (cwd: string, label: string) => void;
  } = $props();

  // Source filter presented as three friendly groups over the low-level sources.
  const SOURCE_GROUPS = [
    { label: 'Messages', sources: ['user', 'assistant'] },
    { label: 'Thinking', sources: ['thinking'] },
    { label: 'Tool calls', sources: ['tool_use', 'tool_result'] },
  ];

  // ── session list state ──────────────────────────────────────────────────────
  let sessions = $state<SessionMeta[]>([]);
  let loadError = $state<string | null>(null);
  let loading = $state(true);
  let sortBy = $state<'newest' | 'oldest' | 'title'>('newest');
  /** Home directory, used to render project paths as "~/...". Null until loaded. */
  let homeDir = $state<string | null>(null);

  /** Per-path title overrides applied after a successful rename. */
  let renamedTitles = $state<Record<string, string>>({});
  /** Path of the card currently showing the inline rename editor. */
  let renamingPath = $state<string | null>(null);
  /** Current value of the rename text input. */
  let renameInput = $state('');
  /** Pending double-confirm data (set when user clicks Save in the editor). */
  let confirmPendingPath = $state<string | null>(null);
  /** Error message shown inside the inline editor. */
  let renameError = $state<string | null>(null);

  /** Toast message (shown for ~2.5 s after a successful rename). */
  let toast = $state<string | null>(null);
  let toastTimer: number | null = null;

  // ── search UI-only state (query/filters/hits themselves live in the store) ──
  let showProjects = $state(false);
  let fromISO = $state('');
  let toISO = $state('');
  /** Per-session collapse state in search results (VS Code-style fold). */
  let collapsed = $state<Set<string>>(new Set());
  function toggleCollapse(sessionPath: string): void {
    const next = new Set(collapsed);
    if (next.has(sessionPath)) next.delete(sessionPath);
    else next.add(sessionPath);
    collapsed = next;
  }
  let focusedIdx = $state(-1);
  /** Keyboard-highlighted card in browse mode (no query) — a separate index
   *  from `focusedIdx` above, which walks search *hits*, not session cards. */
  let browseFocusedIdx = $state(-1);
  $effect(() => {
    search.query;
    collapsed = new Set();
    focusedIdx = -1;
    browseFocusedIdx = -1;
  });

  let isSearching = $derived(search.query.trim() !== '');

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
    initSearch();
    // Re-entering the home view always starts from a clean slate — any query
    // left over from an inline in-chat search (a different scope entirely)
    // shouldn't leak back in here.
    setQuery('');
  });
  onDestroy(disposeSearch);
  // Clear any pending toast auto-dismiss timer on unmount so it can't fire
  // against a torn-down component or leak.
  onDestroy(() => { if (toastTimer !== null) clearTimeout(toastTimer); });

  // ── shared derived data ─────────────────────────────────────────────────────

  interface EnrichedSession {
    meta: SessionMeta;
    path: string;
    title: string;
    date: string;
    model: string;
    project: string;
  }

  /** Sessions enriched with extracted meta (title, date, model, project). */
  let enriched = $derived.by<EnrichedSession[]>(() =>
    sessions.map((s) => {
      const m = extractMeta(s.preview);
      return {
        meta: s,
        path: s.path,
        // A rename can land anywhere in the file, not just the 50-line preview
        // extractMeta() sees — s.custom_title is scanned server-side across the
        // whole file, so it's the source of truth once a rename exists.
        title: renamedTitles[s.path] ?? (s.custom_title || cleanTitle(m.title)),
        date: m.date,
        model: m.model,
        project: projectLabel(s.cwd, s.project_raw, homeDir),
      };
    })
  );

  /** path -> enriched lookup, to resolve a search hit's session to a real title/meta. */
  let byPath = $derived(new Map(enriched.map((e) => [e.path, e])));

  function sortItems<T extends { date: string; title: string }>(items: T[]): T[] {
    return [...items].sort((a, b) => {
      if (sortBy === 'newest') return (b.date || '').localeCompare(a.date || '');
      if (sortBy === 'oldest') return (a.date || '').localeCompare(b.date || '');
      return a.title.localeCompare(b.title);
    });
  }

  function groupByProject<T extends { project: string }>(
    items: T[]
  ): { project: string; items: T[] }[] {
    const m = new Map<string, T[]>();
    for (const it of items) {
      const arr = m.get(it.project);
      if (arr) arr.push(it);
      else m.set(it.project, [it]);
    }
    return [...m.entries()].map(([project, items]) => ({ project, items }));
  }

  /** The "only show these projects" chip filter — applies uniformly to both modes. */
  function projectAllowed(project: string): boolean {
    return search.projects.length === 0 || search.projects.includes(project);
  }

  // ── browse mode (no query): every session, grouped by project ───────────────
  let browseGroups = $derived.by(() =>
    groupByProject(sortItems(enriched.filter((e) => projectAllowed(e.project))))
  );

  // Flat, display-order view of browseGroups — what browse-mode Up/Down/Enter walks.
  let flatBrowseItems = $derived.by<EnrichedSession[]>(() =>
    browseGroups.flatMap((pg) => pg.items)
  );
  let browseFocusedPath = $derived(
    browseFocusedIdx >= 0 && browseFocusedIdx < flatBrowseItems.length
      ? flatBrowseItems[browseFocusedIdx].path
      : null
  );

  // ── search mode (query typed): hits nested project -> session -> lines ──────
  interface HitSessionGroup {
    path: string;
    project: string;
    title: string;
    date: string;
    meta: SessionMeta | null;
    hits: SearchHit[];
  }

  function basenameNoExt(p: string): string {
    const b = p.split('/').pop() ?? p;
    return b.replace(/\.jsonl$/, '');
  }

  let searchSessionGroups = $derived.by<HitSessionGroup[]>(() => {
    const bySession = new Map<string, HitSessionGroup>();
    for (const h of search.hits) {
      let g = bySession.get(h.sessionPath);
      if (!g) {
        const e = byPath.get(h.sessionPath);
        g = {
          path: h.sessionPath,
          project: h.project,
          title: e ? e.title : basenameNoExt(h.sessionPath),
          date: e ? e.date : '',
          meta: e ? e.meta : null,
          hits: [],
        };
        bySession.set(h.sessionPath, g);
      }
      g.hits.push(h);
    }
    return sortItems([...bySession.values()]);
  });

  let searchGroups = $derived.by(() =>
    groupByProject(searchSessionGroups.filter((g) => projectAllowed(g.project)))
  );

  // Hits belonging to non-collapsed session groups, in display order — what ↑/↓ walks.
  let visibleHits = $derived.by<SearchHit[]>(() => {
    const out: SearchHit[] = [];
    for (const pg of searchGroups) {
      for (const sg of pg.items) {
        if (collapsed.has(sg.path)) continue;
        out.push(...sg.hits);
      }
    }
    return out;
  });

  function hitKey(h: SearchHit): string {
    return `${h.sessionPath}:${h.lineNo}:${h.blockNo}`;
  }
  let focusedKey = $derived(
    focusedIdx >= 0 && focusedIdx < visibleHits.length ? hitKey(visibleHits[focusedIdx]) : null
  );

  let indexing = $derived(
    search.status?.building &&
      search.status.totalSessions > 0 &&
      search.status.indexedSessions < search.status.totalSessions
  );

  function groupOn(sources: string[]): boolean {
    return sources.every((s) => search.sources.includes(s));
  }
  function toggleGroup(sources: string[]): void {
    const on = groupOn(sources);
    for (const s of sources) {
      const present = search.sources.includes(s);
      if (on && present) toggleSource(s);
      else if (!on && !present) toggleSource(s);
    }
  }

  function onDate(): void {
    setDateRange(fromISO, toISO);
  }

  function sourceBadge(source: string): { label: string; cls: string } {
    switch (source) {
      case 'user': return { label: 'You', cls: 'b-user' };
      case 'assistant': return { label: 'Claude', cls: 'b-asst' };
      case 'thinking': return { label: 'Thinking', cls: 'b-think' };
      case 'tool_use': return { label: 'Tool', cls: 'b-tool' };
      case 'tool_result': return { label: 'Result', cls: 'b-res' };
      default: return { label: source, cls: 'b-user' };
    }
  }

  interface Seg { t: string; hl: boolean }
  function highlight(snippet: string, ranges: [number, number][]): Seg[] {
    const chars = Array.from(snippet);
    const segs: Seg[] = [];
    let pos = 0;
    for (const [s, e] of ranges) {
      if (s > pos) segs.push({ t: chars.slice(pos, s).join(''), hl: false });
      segs.push({ t: chars.slice(s, e).join(''), hl: true });
      pos = e;
    }
    if (pos < chars.length) segs.push({ t: chars.slice(pos).join(''), hl: false });
    return segs;
  }

  function scrollFocusedIntoView(): void {
    tick().then(() => {
      if (!focusedKey) return;
      document.getElementById(`hit-${focusedKey}`)?.scrollIntoView({ block: 'nearest' });
    });
  }

  function scrollBrowseFocusedIntoView(): void {
    tick().then(() => {
      if (!browseFocusedPath) return;
      document.getElementById(`browse-card-${browseFocusedPath}`)?.scrollIntoView({ block: 'nearest' });
    });
  }

  // Browse-mode (no query) session-list navigation — works from anywhere on the
  // page as long as no text input has focus, so Up/Down don't require clicking
  // into the search box first. Scoped to plain browse-mode cards; search-hit
  // navigation (isSearching) stays on the input's own onSearchKeydown below.
  onMount(() => {
    function onKeydown(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        if (confirmPendingPath) {
          e.preventDefault();
          confirmPendingPath = null;
        } else if (browseFocusedIdx >= 0) {
          browseFocusedIdx = -1;
        }
        return;
      }
      if (isSearching) return;
      const active = document.activeElement as HTMLElement | null;
      const tag = active?.tagName.toLowerCase();
      if (tag === 'input' || tag === 'textarea' || tag === 'select' || active?.isContentEditable) return;
      if (flatBrowseItems.length === 0) return;
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        browseFocusedIdx = Math.min(browseFocusedIdx + 1, flatBrowseItems.length - 1);
        scrollBrowseFocusedIntoView();
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        browseFocusedIdx = Math.max(browseFocusedIdx - 1, 0);
        scrollBrowseFocusedIntoView();
      } else if (e.key === 'Enter' && browseFocusedIdx >= 0 && browseFocusedIdx < flatBrowseItems.length) {
        e.preventDefault();
        onOpen(flatBrowseItems[browseFocusedIdx].meta);
      }
    }
    window.addEventListener('keydown', onKeydown);
    return () => window.removeEventListener('keydown', onKeydown);
  });

  function onSearchKeydown(e: KeyboardEvent): void {
    if (visibleHits.length === 0) return;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      focusedIdx = Math.min(focusedIdx + 1, visibleHits.length - 1);
      scrollFocusedIntoView();
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      focusedIdx = Math.max(focusedIdx - 1, 0);
      scrollFocusedIntoView();
    } else if (e.key === 'Enter' && focusedIdx >= 0 && focusedIdx < visibleHits.length) {
      e.preventDefault();
      onJump(visibleHits[focusedIdx]);
    }
  }

  // ── format helpers (browse-mode card stats) ──────────────────────────────────
  function fmtDate(ts: string): string {
    if (!ts) return '';
    try {
      return new Date(ts).toLocaleDateString(undefined, {
        month: 'short', day: 'numeric', year: 'numeric',
      });
    } catch { return ts; }
  }
  function fmtModel(model: string): string {
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
    } catch { return fmtDate(fallback); }
  }
  function fmtHitDate(ts: number | null): string {
    return ts ? new Date(ts).toLocaleString() : '';
  }
  function basename(p: string): string {
    const i = p.lastIndexOf('/');
    return i >= 0 ? p.slice(i + 1) : p;
  }

  function sessionStats(meta: SessionMeta, model: string, date: string): string {
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

  // ── rename helpers (shared by both browse cards and search-mode headers) ────
  function startRename(path: string, currentTitle: string) {
    renamingPath = path;
    renameInput = currentTitle;
    renameError = null;
  }
  function cancelRename() {
    renamingPath = null;
    renameInput = '';
    renameError = null;
    confirmPendingPath = null;
  }
  function requestSaveRename(path: string) {
    const t = renameInput.trim();
    if (!t) {
      renameError = 'Title cannot be empty.';
      return;
    }
    renameError = null;
    confirmPendingPath = path;
  }
  async function confirmRename() {
    if (!confirmPendingPath) return;
    const path = confirmPendingPath;
    const newTitle = renameInput.trim();
    confirmPendingPath = null;
    try {
      await renameSession(path, newTitle);
      renamedTitles[path] = newTitle;
      renamingPath = null;
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

  // ── resume (from a list card, without opening the session first) ──────────
  async function doResume(sessionPath: string, cwd: string, title: string) {
    const id = sessionIdFromPath(sessionPath);
    await copyToClipboard(resumeCommand(cwd, id));
    try {
      await resumeInTerminal(cwd, id, title);
      showToast('Opened in a terminal — command also copied to clipboard');
    } catch {
      showToast('Could not open a terminal — command copied to clipboard instead');
    }
  }
</script>

<!-- ── search bar (query + toggles), always visible ─────────────────────────── -->
<div class="search-bar">
  <div class="search-input">
    <input
      id="browse-search-input"
      type="text"
      placeholder="Search titles or type to search all your Claude Code history… (Ctrl/Cmd+K)"
      value={search.query}
      oninput={(e) => setQuery(e.currentTarget.value)}
      onkeydown={onSearchKeydown}
      spellcheck="false"
      autocomplete="off"
    />
  </div>

  <!-- ── Filters ─────────────────────────────────────────────────────────── -->
  <div class="filters">
    <div class="filter-set">
      {#each SOURCE_GROUPS as g (g.label)}
        <button
          class="chip" class:on={groupOn(g.sources)}
          onclick={() => toggleGroup(g.sources)} type="button">{g.label}</button>
      {/each}
    </div>

    <div class="filter-set dates">
      <label>From <input type="date" bind:value={fromISO} onchange={onDate} /></label>
      <label>To <input type="date" bind:value={toISO} onchange={onDate} /></label>
    </div>

    <div class="filter-set">
      <input
        type="text"
        class="tool-name-input"
        placeholder="Tool name (e.g. Bash)"
        value={search.toolName}
        oninput={(e) => setToolName(e.currentTarget.value)}
      />
    </div>

    {#if search.availableProjects.length > 0}
      <div class="filter-set">
        <button class="chip" class:on={search.projects.length > 0}
          onclick={() => (showProjects = !showProjects)} type="button">
          {search.projects.length > 0 ? `Projects (${search.projects.length})` : 'All projects'} ▾
        </button>
        {#if search.projects.length > 0}
          <button class="chip ghost" onclick={clearProjects} type="button">Clear</button>
        {/if}
      </div>
    {/if}

    <div class="filter-set sort-set">
      <select bind:value={sortBy} aria-label="Sort order">
        <option value="newest">Newest</option>
        <option value="oldest">Oldest</option>
        <option value="title">Title</option>
      </select>
    </div>
  </div>

  {#if showProjects}
    <div class="project-list">
      {#each search.availableProjects as p (p.label)}
        <label class="proj">
          <input
            type="checkbox"
            checked={search.projects.includes(p.label)}
            onchange={() => toggleProject(p.label)} />
          <span class="proj-label">{p.label}</span>
          <span class="proj-count">{p.count}</span>
        </label>
      {/each}
    </div>
  {/if}
</div>

<!-- ── Status line (search mode only) ───────────────────────────────────────── -->
{#if search.error || isSearching || indexing}
  <div class="status-line">
    {#if search.error}
      <span class="err">⚠ {search.error}</span>
    {:else if isSearching}
      <span>
        {search.hits.length}{search.truncated ? '+' : ''}
        result{search.hits.length === 1 ? '' : 's'}
        {#if search.running}· searching…{/if}
      </span>
    {/if}
    {#if indexing}
      <span class="muted idx">indexing {search.status?.indexedSessions}/{search.status?.totalSessions}…</span>
    {/if}
  </div>
{/if}

<!-- ── content ───────────────────────────────────────────────────────────── -->
{#if loading}
  <div class="empty-state">Loading sessions...</div>
{:else if loadError}
  <div class="empty-state">{loadError}</div>
{:else if isSearching}
  <!-- ── search results, grouped project -> chat -> hits ────────────────────── -->
  <div class="results">
    {#each searchGroups as pg (pg.project)}
      <div class="project-group">
        <div class="project-group__head">
          <div class="project-group__name" title={pg.project} data-copy-text={pg.project}>{pg.project}</div>
          {#if onOpenSettings && pg.items[0]?.meta?.cwd}
            <button
              type="button"
              class="project-group__settings"
              title="Claude Code settings for this project"
              aria-label="Claude Code settings for this project"
              onclick={() => onOpenSettings?.(pg.items[0].meta!.cwd, pg.project)}
            >⚙</button>
          {/if}
        </div>

        {#each pg.items as sg (sg.path)}
          <div class="group">
            {#if renamingPath === sg.path}
              <div class="rename-editor">
                <input
                  type="text" class="rename-input" bind:value={renameInput}
                  aria-label="New session title"
                  onkeydown={(e) => {
                    if (e.key === 'Enter') requestSaveRename(sg.path);
                    if (e.key === 'Escape') cancelRename();
                  }}
                />
                {#if renameError}<p class="rename-error">{renameError}</p>{/if}
                <div class="rename-actions">
                  <button type="button" class="btn btn--sm btn--primary" onclick={() => requestSaveRename(sg.path)}>Save</button>
                  <button type="button" class="btn btn--sm btn--ghost" onclick={cancelRename}>Cancel</button>
                </div>
              </div>
            {:else}
              <div class="group-head-row">
                <button
                  class="group-head" title={sg.path}
                  onclick={() => toggleCollapse(sg.path)} type="button"
                  aria-expanded={!collapsed.has(sg.path)}>
                  <span class="g-chevron">{collapsed.has(sg.path) ? '▸' : '▾'}</span>
                  <span class="g-title" title={sg.title} data-copy-text={sg.title}>{sg.title}</span>
                  <span class="g-count">{sg.hits.length} match{sg.hits.length === 1 ? '' : 'es'}</span>
                </button>
                {#if sg.meta}
                  <button
                    type="button" class="btn btn--ghost btn--sm resume-btn"
                    onclick={(e) => { e.stopPropagation(); doResume(sg.path, sg.meta!.cwd, sg.title); }}
                    aria-label="Resume this session in a terminal"
                    title="claude --resume"
                  >Resume</button>
                {/if}
                <button
                  type="button" class="btn btn--ghost btn--sm rename-btn"
                  onclick={(e) => { e.stopPropagation(); startRename(sg.path, sg.title); }}
                  aria-label="Rename session"
                >Rename</button>
                {#if sg.meta}
                  <button
                    type="button" class="btn btn--ghost btn--sm open-btn"
                    onclick={() => onOpen(sg.meta!)}
                  >Open</button>
                {/if}
              </div>
              {#if !collapsed.has(sg.path)}
                {#each sg.hits as h (h.uuid + ':' + h.lineNo + ':' + h.blockNo)}
                  {@const badge = sourceBadge(h.source)}
                  <button
                    class="hit" class:focused={focusedKey === hitKey(h)}
                    id="hit-{hitKey(h)}"
                    onclick={() => onJump(h)} type="button" title={fmtHitDate(h.ts)}>
                    <span class="hit-badge {badge.cls}">{badge.label}</span>
                    <span class="hit-snippet">
                      {#each highlight(h.snippet, h.matchRanges) as seg}{#if seg.hl}<mark>{seg.t}</mark>{:else}{seg.t}{/if}{/each}
                    </span>
                  </button>
                {/each}
              {/if}
            {/if}
          </div>
        {/each}
      </div>
    {/each}

    {#if search.moreAvailable && !search.running}
      <button class="load-more" onclick={loadMore} type="button">
        Load more results…
      </button>
    {/if}

    {#if !search.running && search.hits.length === 0 && !search.error}
      <div class="empty-state">No matches.</div>
    {/if}
  </div>
{:else if browseGroups.length === 0}
  <div class="empty-state">No sessions found.</div>
{:else}
  <!-- ── browse mode: every session, grouped by project ─────────────────────── -->
  {#each browseGroups as pg (pg.project)}
    <div class="project-group">
      <div class="project-group__head">
        <div class="project-group__name" title={pg.project} data-copy-text={pg.project}>{pg.project}</div>
        {#if onOpenSettings && pg.items[0]?.meta.cwd}
          <button
            type="button"
            class="project-group__settings"
            title="Claude Code settings for this project"
            aria-label="Claude Code settings for this project"
            onclick={() => onOpenSettings?.(pg.items[0].meta.cwd, pg.project)}
          >⚙</button>
        {/if}
      </div>

      {#each pg.items as s (s.path)}
        <div
          class="session-card"
          class:session-card--editing={renamingPath === s.path}
          class:focused={browseFocusedPath === s.path}
          id="browse-card-{s.path}"
        >
          {#if renamingPath === s.path}
            <div class="rename-editor">
              <input
                type="text" class="rename-input" bind:value={renameInput}
                aria-label="New session title"
                onkeydown={(e) => {
                  if (e.key === 'Enter') requestSaveRename(s.path);
                  if (e.key === 'Escape') cancelRename();
                }}
              />
              {#if renameError}<p class="rename-error">{renameError}</p>{/if}
              <div class="rename-actions">
                <button type="button" class="btn btn--sm btn--primary" onclick={() => requestSaveRename(s.path)}>Save</button>
                <button type="button" class="btn btn--sm btn--ghost" onclick={cancelRename}>Cancel</button>
              </div>
            </div>
          {:else}
            <button class="session-card__open" type="button" onclick={() => onOpen(s.meta)}>
              <span class="session-card__title" title={s.title} data-copy-text={s.title}>{s.title}</span>
              <span class="session-card__stats">{sessionStats(s.meta, s.model, s.date)}</span>
            </button>
            <button
              type="button" class="btn btn--ghost btn--sm resume-btn"
              onclick={(e) => { e.stopPropagation(); doResume(s.path, s.meta.cwd, s.title); }}
              aria-label="Resume this session in a terminal"
              title="claude --resume"
            >Resume</button>
            <button
              type="button" class="btn btn--ghost btn--sm rename-btn"
              onclick={(e) => { e.stopPropagation(); startRename(s.path, s.title); }}
              aria-label="Rename session"
            >Rename</button>
          {/if}
        </div>
      {/each}
    </div>
  {/each}
{/if}

<!-- ── double-confirm rename modal ──────────────────────────────────────────── -->
{#if confirmPendingPath}
  <div class="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="rename-modal-title">
    <div class="modal">
      <h3 id="rename-modal-title">Rename this session?</h3>
      <p>This updates the title saved in the chat file.</p>
      <div class="modal__warning">
        This change is not backed up and cannot be undone here.
      </div>
      <div class="modal__actions">
        <button type="button" class="btn btn--ghost" onclick={() => { confirmPendingPath = null; }}>
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
  /* ── Search bar ──────────────────────────────────────────────────────── */
  /* top: var(--header-h) (not 0) — otherwise once both the app header and this
     bar are pinned, the bar sticks to the same y=0 as the header and ends up
     visually covered by it. See --header-h in app.css. */
  .search-bar {
    position: sticky; top: var(--header-h); z-index: 5;
    background: var(--bg); padding-bottom: 0.75rem; margin-bottom: 0.75rem;
    display: flex; flex-direction: column; gap: 0.5rem;
    border-bottom: 1px solid var(--border);
  }
  .search-input { position: relative; display: flex; align-items: center; }
  .search-input input[type='text'] {
    flex: 1; font-size: 0.95rem; padding: 0.55rem 0.75rem;
    background: var(--bg-card); color: var(--text);
    border: 1px solid var(--border-strong); border-radius: 0.45rem; outline: none;
  }
  .search-input input[type='text']:focus { border-color: var(--accent-user); }

  .filters { display: flex; flex-wrap: wrap; gap: 0.75rem; align-items: center; }
  .filter-set { display: flex; align-items: center; gap: 0.3rem; flex-wrap: wrap; }
  .filter-set.dates label { font-size: 0.72rem; color: var(--text-muted); display: inline-flex; align-items: center; gap: 0.25rem; }
  .filter-set.dates input { font-size: 0.72rem; background: var(--bg-card); color: var(--text); border: 1px solid var(--border); border-radius: 0.3rem; padding: 0.15rem 0.3rem; }
  .filter-set.sort-set { margin-left: auto; }
  .filter-set.sort-set select {
    padding: 0.3rem 0.5rem; border-radius: 999px; border: 1px solid var(--border);
    background: var(--bg-subtle); color: var(--text-muted); font-size: 0.74rem; font-family: inherit; cursor: pointer;
  }

  .chip {
    font-size: 0.74rem; padding: 0.22rem 0.55rem;
    background: var(--bg-subtle); color: var(--text-muted);
    border: 1px solid var(--border); border-radius: 999px; cursor: pointer;
  }
  .chip:hover { color: var(--text); }
  .chip.on { background: color-mix(in srgb, var(--accent-user) 18%, transparent); color: var(--text); border-color: color-mix(in srgb, var(--accent-user) 40%, transparent); }
  .chip.ghost { border-style: dashed; }

  .tool-name-input {
    font-size: 0.74rem; padding: 0.22rem 0.55rem; width: 9.5rem;
    background: var(--bg-subtle); color: var(--text); border: 1px solid var(--border); border-radius: 999px;
  }
  .tool-name-input:focus { border-color: var(--accent-user); outline: none; }

  .project-list {
    max-height: 190px; overflow-y: auto; display: flex; flex-direction: column; gap: 0.1rem;
    border: 1px solid var(--border); border-radius: 0.4rem; padding: 0.35rem; background: var(--bg-card);
  }
  .proj { display: flex; align-items: center; gap: 0.5rem; font-size: 0.78rem; padding: 0.15rem 0.25rem; border-radius: 0.25rem; }
  .proj:hover { background: var(--bg-subtle); }
  .proj-label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .proj-count { color: var(--text-faint); font-size: 0.7rem; }

  .status-line { display: flex; gap: 0.75rem; align-items: center; font-size: 0.76rem; color: var(--text-muted); padding: 0 0.15rem; margin-bottom: 0.75rem; }
  .status-line .muted { color: var(--text-faint); }
  .status-line .idx { margin-left: auto; }
  .status-line .err { color: var(--accent-result-err); }

  /* ── Search results ──────────────────────────────────────────────────── */
  .results { display: flex; flex-direction: column; gap: 0; }
  .group { display: flex; flex-direction: column; margin-bottom: 0.4rem; }
  .group-head-row { display: flex; align-items: center; gap: 0.25rem; }
  .group-head {
    display: flex; align-items: baseline; gap: 0.5rem; flex: 1; min-width: 0;
    padding: 0.3rem 0.15rem; font-size: 0.8rem; border-bottom: 1px solid var(--border);
    font-family: inherit; color: inherit; text-align: left;
    border-left: none; border-right: none; border-top: none;
    background: none; cursor: pointer;
  }
  .group-head:hover { background: var(--bg-subtle); }
  .g-chevron { flex: 0 0 auto; color: var(--text-faint); font-size: 0.65rem; width: 0.8rem; }
  .g-title { font-weight: 600; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .g-count { margin-left: auto; color: var(--text-faint); font-size: 0.72rem; white-space: nowrap; }
  .open-btn, .rename-btn { flex-shrink: 0; }

  .hit {
    display: flex; gap: 0.55rem; align-items: flex-start; text-align: left;
    width: 100%; padding: 0.4rem 0.35rem; background: transparent; border: none;
    border-bottom: 1px solid var(--border); cursor: pointer; color: var(--text);
  }
  .hit:hover { background: var(--bg-subtle); }
  .hit.focused { background: color-mix(in srgb, var(--accent-user) 16%, transparent); outline: 1px solid color-mix(in srgb, var(--accent-user) 45%, transparent); outline-offset: -1px; }
  .hit-badge {
    flex: 0 0 auto; font-size: 0.62rem; text-transform: uppercase; letter-spacing: 0.03em;
    padding: 0.1rem 0.35rem; border-radius: 0.25rem; margin-top: 0.1rem; white-space: nowrap;
    background: var(--bg-subtle); color: var(--text-muted);
  }
  .b-user { background: color-mix(in srgb, var(--accent-user) 20%, transparent); color: var(--text); }
  .b-asst { background: color-mix(in srgb, var(--accent-assistant, var(--accent-user)) 18%, transparent); color: var(--text); }
  .b-think { background: color-mix(in srgb, var(--text-faint) 18%, transparent); }
  .b-tool, .b-res { background: color-mix(in srgb, var(--accent-tool, var(--text-muted)) 18%, transparent); }

  .hit-snippet {
    font-size: 0.82rem; line-height: 1.4; color: var(--text-muted);
    overflow: hidden; display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical;
    white-space: pre-wrap; word-break: break-word;
  }
  .hit-snippet mark { background: color-mix(in srgb, var(--accent-user) 40%, transparent); color: var(--text); border-radius: 0.15rem; padding: 0 0.05rem; }

  .load-more {
    width: 100%; padding: 0.55rem; font-size: 0.82rem; margin-top: 0.25rem;
    background: var(--bg-subtle); color: var(--text-muted);
    border: 1px solid var(--border); border-radius: 0.4rem; cursor: pointer;
  }
  .load-more:hover { color: var(--text); border-color: var(--border-strong); }

  /* ── Browse mode cards ───────────────────────────────────────────────── */
  /* .session-card's own box model (padding/border-radius/etc.) is global,
     in app.css — only add to it here, don't restate/override it. */
  .session-card { display: flex; align-items: center; gap: 0.5rem; }
  .session-card--editing { cursor: default; }
  .session-card.focused {
    background: color-mix(in srgb, var(--accent-user) 16%, transparent);
    border-color: color-mix(in srgb, var(--accent-user) 45%, transparent);
  }
  .session-card__open {
    flex: 1; min-width: 0; display: flex; flex-direction: column; align-items: flex-start; gap: 0.15rem;
    background: none; border: 0; padding: 0; cursor: pointer; font-family: inherit; color: inherit; text-align: left;
  }
  .session-card__stats { font-size: 0.73rem; color: var(--text-muted); line-height: 1.4; white-space: normal; word-break: break-word; opacity: 0.85; }
  .rename-btn, .resume-btn { flex-shrink: 0; opacity: 0; transition: opacity 0.1s; }
  .session-card:hover .rename-btn, .session-card:hover .resume-btn,
  .group:hover .rename-btn, .group:hover .resume-btn, .group:hover .open-btn { opacity: 1; }
  .open-btn { opacity: 0; transition: opacity 0.1s; }

  /* ── Inline rename editor (shared) ───────────────────────────────────── */
  .rename-editor { flex: 1; display: flex; flex-direction: column; gap: 0.35rem; padding: 0.15rem 0; }
  .rename-input {
    width: 100%; padding: 0.4rem 0.65rem; border-radius: 0.35rem;
    border: 1px solid var(--border-strong); background: var(--bg-subtle); color: var(--text);
    font-family: var(--font-sans); font-size: 0.85rem; line-height: 1.4;
  }
  .rename-input:focus { outline: 2px solid var(--accent-user); outline-offset: 1px; }
  .rename-actions { display: flex; gap: 0.4rem; }
  .rename-error { font-size: 0.75rem; color: var(--accent-result-err); margin: 0; }
</style>
