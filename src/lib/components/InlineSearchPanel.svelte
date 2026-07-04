<script lang="ts">
  /**
   * InlineSearchPanel.svelte — "find in this chat", reusing the same search
   * engine/store as the home page's merged Browse+Search, scoped to one
   * session. Trimmed filter set (query + case/whole-word/regex + tool name —
   * no source chips, date range, or project filter, since those don't apply
   * to a single already-open chat). Results are a flat list (no project/chat
   * grouping needed — every hit is already in this session); clicking one
   * scrolls to that message via the caller's jumpTo.
   */
  import { onMount, onDestroy, tick } from 'svelte';
  import type { SearchHit } from '$lib/types';
  import { search, setQuery, toggleOpt, setToolName, initSearch, disposeSearch } from '$lib/search.svelte';

  let {
    sessionPath,
    onJump,
    onClose,
  }: {
    sessionPath: string;
    onJump: (uuid: string) => void;
    onClose: () => void;
  } = $props();

  let inputEl: HTMLInputElement | undefined = $state(undefined);
  let focusedIdx = $state(-1);

  onMount(() => {
    initSearch(sessionPath);
    search.sessionOnly = true;
    setQuery('');
    tick().then(() => inputEl?.focus());
  });
  onDestroy(disposeSearch);

  // Reset keyboard focus whenever the result set changes underneath it.
  $effect(() => {
    search.query;
    focusedIdx = -1;
  });

  function hitKey(h: SearchHit): string {
    return `${h.sessionPath}:${h.lineNo}:${h.blockNo}`;
  }
  let focusedKey = $derived(
    focusedIdx >= 0 && focusedIdx < search.hits.length ? hitKey(search.hits[focusedIdx]) : null
  );

  function scrollFocusedIntoView(): void {
    tick().then(() => {
      if (!focusedKey) return;
      document.getElementById(`ics-hit-${focusedKey}`)?.scrollIntoView({ block: 'nearest' });
    });
  }

  function onInputKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
      return;
    }
    if (search.hits.length === 0) return;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      focusedIdx = Math.min(focusedIdx + 1, search.hits.length - 1);
      scrollFocusedIntoView();
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      focusedIdx = Math.max(focusedIdx - 1, 0);
      scrollFocusedIntoView();
    } else if (e.key === 'Enter' && focusedIdx >= 0) {
      e.preventDefault();
      onJump(search.hits[focusedIdx].uuid);
    }
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

  let isSearching = $derived(search.query.trim() !== '');
</script>

<div class="ics">
  <div class="ics__bar">
    <div class="ics__input">
      <input
        bind:this={inputEl}
        type="text"
        placeholder="Find in this chat…"
        value={search.query}
        oninput={(e) => setQuery(e.currentTarget.value)}
        onkeydown={onInputKeydown}
        spellcheck="false"
        autocomplete="off"
      />
      <div class="ics__toggles">
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
    <input
      type="text"
      class="ics__tool-input"
      placeholder="Tool name"
      value={search.toolName}
      oninput={(e) => setToolName(e.currentTarget.value)}
    />
    <button class="btn btn--ghost btn--sm ics__close" onclick={onClose} type="button" aria-label="Close find">✕</button>
  </div>

  {#if search.error || isSearching}
    <div class="ics__status">
      {#if search.error}
        <span class="err">⚠ {search.error}</span>
      {:else}
        <span>
          {search.hits.length}{search.truncated ? '+' : ''}
          match{search.hits.length === 1 ? '' : 'es'}
          {#if search.running}· searching…{/if}
        </span>
      {/if}
    </div>
  {/if}

  {#if isSearching}
    <div class="ics__results">
      {#each search.hits as h (h.uuid + ':' + h.lineNo + ':' + h.blockNo)}
        {@const badge = sourceBadge(h.source)}
        <button
          class="ics-hit" class:focused={focusedKey === hitKey(h)}
          id="ics-hit-{hitKey(h)}"
          onclick={() => onJump(h.uuid)} type="button">
          <span class="hit-badge {badge.cls}">{badge.label}</span>
          <span class="hit-snippet">
            {#each highlight(h.snippet, h.matchRanges) as seg}{#if seg.hl}<mark>{seg.t}</mark>{:else}{seg.t}{/if}{/each}
          </span>
        </button>
      {/each}
      {#if !search.running && search.hits.length === 0 && !search.error}
        <div class="ics__empty">No matches in this chat.</div>
      {/if}
    </div>
  {/if}
</div>

<style>
  /* top: var(--header-h) (not 0) — see the same note on BrowseView's .search-bar;
     this panel sits below the app header once both are pinned while scrolling. */
  .ics {
    position: sticky; top: var(--header-h); z-index: 5;
    background: var(--bg); border: 1px solid var(--border); border-radius: 0.45rem;
    padding: 0.6rem 0.65rem; margin-bottom: 0.85rem;
    box-shadow: 0 2px 10px color-mix(in srgb, black 8%, transparent);
  }

  .ics__bar { display: flex; align-items: center; gap: 0.5rem; }
  .ics__input { position: relative; display: flex; align-items: center; flex: 1; min-width: 0; }
  .ics__input input[type='text'] {
    width: 100%; font-size: 0.88rem; padding: 0.45rem 5.5rem 0.45rem 0.65rem;
    background: var(--bg-card); color: var(--text);
    border: 1px solid var(--border-strong); border-radius: 0.4rem; outline: none;
  }
  .ics__input input[type='text']:focus { border-color: var(--accent-user); }

  .ics__toggles { position: absolute; right: 0.3rem; display: flex; gap: 0.15rem; }
  .tg {
    min-width: 1.55rem; height: 1.55rem; padding: 0 0.3rem;
    font-size: 0.75rem; line-height: 1; display: inline-flex; align-items: center; justify-content: center;
    background: transparent; color: var(--text-muted);
    border: 1px solid transparent; border-radius: 0.3rem; cursor: pointer;
  }
  .tg:hover { background: var(--bg-subtle); color: var(--text); }
  .tg.on { background: color-mix(in srgb, var(--accent-user) 22%, transparent); color: var(--text); border-color: color-mix(in srgb, var(--accent-user) 45%, transparent); }
  .tg.mono { font-family: var(--font-mono, monospace); }
  .tg .ab { text-decoration: underline; }

  .ics__tool-input {
    flex: 0 0 auto; width: 8rem; font-size: 0.78rem; padding: 0.4rem 0.55rem;
    background: var(--bg-subtle); color: var(--text); border: 1px solid var(--border); border-radius: 0.35rem;
  }
  .ics__tool-input:focus { border-color: var(--accent-user); outline: none; }

  .ics__close { flex: 0 0 auto; }

  .ics__status { font-size: 0.74rem; color: var(--text-muted); padding: 0.4rem 0.1rem 0; }
  .ics__status .err { color: var(--accent-result-err); }

  /* Collapsed by default so the results list doesn't sit fully visible over
     the transcript while the panel isn't in active use; revealed when the
     input (or anything else in the panel) has focus, or on hover so a quick
     glance doesn't require re-focusing the input. Hover is checked on the
     whole `.ics` panel (not just `.ics__results`) so moving the mouse onto
     the search bar itself reveals the list growing beneath it, and so the
     list stays revealed for the whole mousedown→click sequence when a hit
     is clicked (hover holds through the click even though the click also
     blurs the input). */
  .ics__results {
    display: flex; flex-direction: column; gap: 0; margin-top: 0; overflow-y: auto;
    max-height: 0; opacity: 0; border-top: 1px solid transparent;
    transition: max-height 0.18s ease, opacity 0.15s ease, margin-top 0.18s ease, border-color 0.18s ease;
  }
  .ics:hover .ics__results,
  .ics:focus-within .ics__results {
    margin-top: 0.4rem; max-height: 260px; opacity: 1; border-top-color: var(--border);
  }
  .ics-hit {
    display: flex; gap: 0.55rem; align-items: flex-start; text-align: left;
    width: 100%; padding: 0.4rem 0.35rem; background: transparent; border: none;
    border-bottom: 1px solid var(--border); cursor: pointer; color: var(--text);
  }
  .ics-hit:hover { background: var(--bg-subtle); }
  .ics-hit.focused { background: color-mix(in srgb, var(--accent-user) 16%, transparent); outline: 1px solid color-mix(in srgb, var(--accent-user) 45%, transparent); outline-offset: -1px; }
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
    font-size: 0.8rem; line-height: 1.4; color: var(--text-muted);
    overflow: hidden; display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical;
    white-space: pre-wrap; word-break: break-word;
  }
  .hit-snippet mark { background: color-mix(in srgb, var(--accent-user) 40%, transparent); color: var(--text); border-radius: 0.15rem; padding: 0 0.05rem; }

  .ics__empty { font-size: 0.8rem; color: var(--text-faint); padding: 0.6rem 0.35rem; }
</style>
