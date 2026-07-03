<script lang="ts">
  /**
   * SearchView.svelte — VS Code-style search across all sessions.
   *
   * Query box + case/word/regex toggles, source/date/project filters, and a
   * streamed, session-grouped results list. Clicking a hit calls `onJump`, which
   * the shell uses to open that session and scroll to the block.
   */
  import { onMount, onDestroy } from 'svelte';
  import type { SearchHit } from '$lib/types';
  import {
    search,
    setQuery,
    toggleOpt,
    toggleSource,
    toggleProject,
    clearProjects,
    setDateRange,
    scheduleSearch,
    initSearch,
    disposeSearch,
  } from '$lib/search.svelte';

  let { onJump = (_h: SearchHit) => {} }: { onJump?: (hit: SearchHit) => void } = $props();

  // Source filter presented as three friendly groups over the low-level sources.
  const SOURCE_GROUPS = [
    { label: 'Messages', sources: ['user', 'assistant'] },
    { label: 'Thinking', sources: ['thinking'] },
    { label: 'Tool calls', sources: ['tool_use', 'tool_result'] },
  ];

  function groupOn(sources: string[]): boolean {
    return sources.every((s) => search.sources.includes(s));
  }
  function toggleGroup(sources: string[]): void {
    // Flip the group as a unit via the store's per-source toggle.
    const on = groupOn(sources);
    for (const s of sources) {
      const present = search.sources.includes(s);
      if (on && present) toggleSource(s);
      else if (!on && !present) toggleSource(s);
    }
  }

  let showProjects = $state(false);
  let fromISO = $state('');
  let toISO = $state('');

  function onDate(): void {
    setDateRange(fromISO, toISO);
  }

  // Badge label + accent per source.
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

  function basename(p: string): string {
    const i = p.lastIndexOf('/');
    return i >= 0 ? p.slice(i + 1) : p;
  }
  function fmtDate(ts: number | null): string {
    return ts ? new Date(ts).toLocaleString() : '';
  }

  // Split a snippet into plain/highlighted segments by char-offset ranges.
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

  // Group hits by session, preserving append order.
  interface Group { sessionPath: string; project: string; hits: SearchHit[] }
  let groups = $derived.by<Group[]>(() => {
    const map = new Map<string, Group>();
    for (const h of search.hits) {
      let g = map.get(h.sessionPath);
      if (!g) {
        g = { sessionPath: h.sessionPath, project: h.project, hits: [] };
        map.set(h.sessionPath, g);
      }
      g.hits.push(h);
    }
    return [...map.values()];
  });

  let indexing = $derived(
    search.status?.building &&
      search.status.totalSessions > 0 &&
      search.status.indexedSessions < search.status.totalSessions
  );

  onMount(initSearch);
  onDestroy(disposeSearch);
</script>

<div class="search-view">
  <!-- ── Query + toggles ─────────────────────────────────────────────────── -->
  <div class="search-bar">
    <div class="search-input">
      <!-- svelte-ignore a11y_autofocus -->
      <input
        type="text"
        placeholder="Search all sessions…"
        value={search.query}
        oninput={(e) => setQuery(e.currentTarget.value)}
        autofocus
        spellcheck="false"
        autocomplete="off"
      />
      <div class="toggles">
        <button
          class="tg" class:on={search.opts.caseSensitive}
          title="Match case" aria-pressed={search.opts.caseSensitive}
          onclick={() => toggleOpt('caseSensitive')} type="button">Aa</button>
        <button
          class="tg" class:on={search.opts.wholeWord}
          title="Whole word" aria-pressed={search.opts.wholeWord}
          onclick={() => toggleOpt('wholeWord')} type="button">&#8203;<span class="ab">ab</span></button>
        <button
          class="tg mono" class:on={search.opts.regex}
          title="Use regular expression" aria-pressed={search.opts.regex}
          onclick={() => toggleOpt('regex')} type="button">.*</button>
      </div>
    </div>

    <!-- ── Filters ───────────────────────────────────────────────────────── -->
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

  <!-- ── Status line ─────────────────────────────────────────────────────── -->
  <div class="status-line">
    {#if search.error}
      <span class="err">⚠ {search.error}</span>
    {:else if search.query}
      <span>
        {search.hits.length}{search.truncated ? '+' : ''}
        result{search.hits.length === 1 ? '' : 's'}
        {#if search.running}· searching…{/if}
      </span>
    {:else}
      <span class="muted">Type to search across your Claude Code history.</span>
    {/if}
    {#if indexing}
      <span class="muted idx">indexing {search.status?.indexedSessions}/{search.status?.totalSessions}…</span>
    {/if}
  </div>

  <!-- ── Results ─────────────────────────────────────────────────────────── -->
  <div class="results">
    {#each groups as g (g.sessionPath)}
      <div class="group">
        <div class="group-head" title={g.sessionPath}>
          <span class="g-project">{g.project}</span>
          <span class="g-file">{basename(g.sessionPath)}</span>
          <span class="g-count">{g.hits.length}</span>
        </div>
        {#each g.hits as h (h.uuid + ':' + h.lineNo + ':' + h.blockNo)}
          {@const badge = sourceBadge(h.source)}
          <button class="hit" onclick={() => onJump(h)} type="button" title={fmtDate(h.ts)}>
            <span class="hit-badge {badge.cls}">{badge.label}</span>
            <span class="hit-snippet">
              {#each highlight(h.snippet, h.matchRanges) as seg}{#if seg.hl}<mark>{seg.t}</mark>{:else}{seg.t}{/if}{/each}
            </span>
          </button>
        {/each}
      </div>
    {/each}

    {#if search.query && !search.running && search.hits.length === 0 && !search.error}
      <div class="empty-state">No matches.</div>
    {/if}
  </div>
</div>

<style>
  .search-view { display: flex; flex-direction: column; gap: 0.5rem; }

  .search-bar {
    position: sticky; top: 0; z-index: 5;
    background: var(--bg); padding-bottom: 0.5rem;
    display: flex; flex-direction: column; gap: 0.5rem;
    border-bottom: 1px solid var(--border);
  }

  .search-input { position: relative; display: flex; align-items: center; }
  .search-input input[type='text'] {
    flex: 1; font-size: 0.95rem; padding: 0.55rem 6.5rem 0.55rem 0.75rem;
    background: var(--bg-card); color: var(--text);
    border: 1px solid var(--border-strong); border-radius: 0.45rem; outline: none;
  }
  .search-input input[type='text']:focus { border-color: var(--accent-user); }

  .toggles { position: absolute; right: 0.35rem; display: flex; gap: 0.15rem; }
  .tg {
    min-width: 1.7rem; height: 1.7rem; padding: 0 0.35rem;
    font-size: 0.8rem; line-height: 1; display: inline-flex; align-items: center; justify-content: center;
    background: transparent; color: var(--text-muted);
    border: 1px solid transparent; border-radius: 0.3rem; cursor: pointer;
  }
  .tg:hover { background: var(--bg-subtle); color: var(--text); }
  .tg.on { background: color-mix(in srgb, var(--accent-user) 22%, transparent); color: var(--text); border-color: color-mix(in srgb, var(--accent-user) 45%, transparent); }
  .tg.mono { font-family: var(--font-mono, monospace); }
  .tg .ab { text-decoration: underline; }

  .filters { display: flex; flex-wrap: wrap; gap: 0.75rem; align-items: center; }
  .filter-set { display: flex; align-items: center; gap: 0.3rem; flex-wrap: wrap; }
  .filter-set.dates label { font-size: 0.72rem; color: var(--text-muted); display: inline-flex; align-items: center; gap: 0.25rem; }
  .filter-set.dates input { font-size: 0.72rem; background: var(--bg-card); color: var(--text); border: 1px solid var(--border); border-radius: 0.3rem; padding: 0.15rem 0.3rem; }

  .chip {
    font-size: 0.74rem; padding: 0.22rem 0.55rem;
    background: var(--bg-subtle); color: var(--text-muted);
    border: 1px solid var(--border); border-radius: 999px; cursor: pointer;
  }
  .chip:hover { color: var(--text); }
  .chip.on { background: color-mix(in srgb, var(--accent-user) 18%, transparent); color: var(--text); border-color: color-mix(in srgb, var(--accent-user) 40%, transparent); }
  .chip.ghost { border-style: dashed; }

  .project-list {
    max-height: 190px; overflow-y: auto; display: flex; flex-direction: column; gap: 0.1rem;
    border: 1px solid var(--border); border-radius: 0.4rem; padding: 0.35rem; background: var(--bg-card);
  }
  .proj { display: flex; align-items: center; gap: 0.5rem; font-size: 0.78rem; padding: 0.15rem 0.25rem; border-radius: 0.25rem; }
  .proj:hover { background: var(--bg-subtle); }
  .proj-label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .proj-count { color: var(--text-faint); font-size: 0.7rem; }

  .status-line { display: flex; gap: 0.75rem; align-items: center; font-size: 0.76rem; color: var(--text-muted); padding: 0 0.15rem; }
  .status-line .muted { color: var(--text-faint); }
  .status-line .idx { margin-left: auto; }
  .status-line .err { color: var(--accent-result-err); }

  .results { display: flex; flex-direction: column; gap: 0.9rem; }
  .group { display: flex; flex-direction: column; }
  .group-head {
    display: flex; align-items: baseline; gap: 0.5rem;
    padding: 0.3rem 0.15rem; font-size: 0.76rem; border-bottom: 1px solid var(--border);
    position: sticky; top: 0;
  }
  .g-project { font-weight: 600; color: var(--text); }
  .g-file { color: var(--text-faint); font-size: 0.7rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .g-count { margin-left: auto; color: var(--text-faint); }

  .hit {
    display: flex; gap: 0.55rem; align-items: flex-start; text-align: left;
    width: 100%; padding: 0.4rem 0.35rem; background: transparent; border: none;
    border-bottom: 1px solid var(--border); cursor: pointer; color: var(--text);
  }
  .hit:hover { background: var(--bg-subtle); }
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
</style>
