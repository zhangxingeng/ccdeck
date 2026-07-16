<script lang="ts">
  /**
   * The library panel. At rest it lists EVERY snippet in the active project,
   * most-recently-used first; typing in the compose box filters that list down
   * by match score. It does not build up from empty — see `runMatch`.
   *
   * Rows show the snippet's NAME and nothing else. The name is a path now
   * (`rust/code_review`), so it already carries the folder grouping that
   * replaced tags/categories — and since the panel lists the whole library at
   * rest, a body preview per row would make it unscannable rather than
   * informative. The founder's own reasoning applies: "I actually rarely read
   * it. If I really want to read it, it means I want to edit it."
   *
   * One insert path, two triggers (contract §S2/S3): click a row, OR step in
   * from the box with ↓ (parent calls `focusFirst`) then `Enter`. `↑`/`↓` move
   * the highlight; `Esc` returns focus to the box without inserting.
   */
  import type { Snippet } from '$lib/prompts/types';
  import { prompts, MATCH_LIMIT } from '$lib/prompts.svelte';

  let {
    onInsert,
    onEscape,
  }: {
    onInsert: (snippet: Snippet) => void;
    /** `Esc` (or `↑` past the first hit) — return focus to the compose box. */
    onEscape: () => void;
  } = $props();

  let listEl: HTMLDivElement | undefined = $state(undefined);
  /** Index of the hit currently hovered/focused — the ONLY one whose full body is
   *  rendered. Hover-reveal is the rule everywhere in this redesign (chip, row);
   *  tying it to focus too (not just mouseenter) means the existing ↑/↓ keyboard
   *  nav gets the same reveal for free, with no separate keyboard path to build. */
  let expandedIdx = $state<number | null>(null);

  function hitButtons(): HTMLButtonElement[] {
    return listEl ? [...listEl.querySelectorAll<HTMLButtonElement>('button.match-hit')] : [];
  }

  /** Step into the panel from the box (contract §S2): highlight the first hit.
   *  Returns whether there was a hit to land on, so the box can keep the
   *  keystroke as a caret move when the panel is empty. */
  export function focusFirst(): boolean {
    const items = hitButtons();
    items[0]?.focus();
    return items.length > 0;
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault();
      onEscape();
      return;
    }
    const items = hitButtons();
    if (!items.length) return;
    const i = items.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === 'Enter') {
      // Enter inserts only after the explicit ↓ step (the hit holds focus) —
      // never pre-armed while the caret is in the box (JC-2).
      if (i >= 0 && prompts.hits[i]) {
        e.preventDefault();
        onInsert(prompts.hits[i].snippet);
      }
      return;
    }
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      if (e.key === 'ArrowUp' && i <= 0) {
        // ↑ past the first hit hands focus back to the box — a natural exit.
        onEscape();
        return;
      }
      const next = e.key === 'ArrowDown' ? Math.min(items.length - 1, i + 1) : i - 1;
      items[next]?.focus();
    }
  }

  /** Only true if the safety cap actually bit. The panel claims to be the whole
   *  library, so on the day that stops being true it must say so out loud — a
   *  silent truncation would make it lie. */
  const truncated = $derived(prompts.hits.length >= MATCH_LIMIT);

  // ── match highlighting (contract §1 + Clarifications) ────────────────────────
  //
  // The matcher (lexical fzf-style subsequence + a blended semantic/embedding
  // engine, src-tauri/src/prompts/{lexical,state}.rs) returns only `{ name,
  // score }` — no match positions, and no backend change is planned. So the
  // spans are derived here, client-side, from the query alone:
  //
  // For each whitespace-separated query token, search the hit's NAME first,
  // then its CONTENT (the same order the lexical scorer weighs the two
  // fields), case-insensitive substring only — and take the first span found.
  // A token that matches neither contributes no span. A hit that cleared the
  // bar purely through the semantic engine has no literal token anywhere, so
  // it ends up with zero spans and renders with NO highlight — never a
  // fabricated one. This is a known partial fix (stated in the plan): it makes
  // a lexical hit's ranking legible, not a semantic-only one's.

  interface Span {
    start: number;
    end: number;
  }

  function tokensOf(query: string): string[] {
    return query.trim().split(/\s+/).filter(Boolean);
  }

  const queryTokens = $derived(tokensOf(prompts.matchQuery));

  function mergeSpans(spans: Span[]): Span[] {
    if (spans.length <= 1) return spans;
    const sorted = [...spans].sort((a, b) => a.start - b.start);
    const merged: Span[] = [sorted[0]];
    for (const s of sorted.slice(1)) {
      const last = merged[merged.length - 1];
      if (s.start <= last.end) last.end = Math.max(last.end, s.end);
      else merged.push(s);
    }
    return merged;
  }

  function deriveSpans(
    name: string,
    content: string,
    tokens: string[]
  ): { nameSpans: Span[]; contentSpans: Span[] } {
    const nameSpans: Span[] = [];
    const contentSpans: Span[] = [];
    const lowerName = name.toLowerCase();
    const lowerContent = content.toLowerCase();
    for (const token of tokens) {
      const t = token.toLowerCase();
      const ni = lowerName.indexOf(t);
      if (ni >= 0) {
        nameSpans.push({ start: ni, end: ni + token.length });
        continue;
      }
      const ci = lowerContent.indexOf(t);
      if (ci >= 0) contentSpans.push({ start: ci, end: ci + token.length });
    }
    return { nameSpans: mergeSpans(nameSpans), contentSpans: mergeSpans(contentSpans) };
  }

  interface Seg {
    t: string;
    hl: boolean;
  }

  function renderSpans(text: string, spans: Span[]): Seg[] {
    if (!spans.length) return [{ t: text, hl: false }];
    const segs: Seg[] = [];
    let pos = 0;
    for (const { start, end } of spans) {
      if (start > pos) segs.push({ t: text.slice(pos, start), hl: false });
      segs.push({ t: text.slice(start, end), hl: true });
      pos = end;
    }
    if (pos < text.length) segs.push({ t: text.slice(pos), hl: false });
    return segs;
  }
</script>

<div class="match-panel" bind:this={listEl} onkeydown={handleKeydown} role="listbox" tabindex="-1" aria-label="Snippets">
  {#if prompts.hits.length}
    {#each prompts.hits as hit, i (hit.snippet.name)}
      {@const spans = deriveSpans(hit.snippet.name, hit.snippet.content, queryTokens)}
      <button
        type="button"
        class="match-hit"
        role="option"
        aria-selected="false"
        onclick={() => onInsert(hit.snippet)}
        onmouseenter={() => (expandedIdx = i)}
        onmouseleave={() => (expandedIdx = null)}
        onfocus={() => (expandedIdx = i)}
        onblur={() => (expandedIdx = null)}
        title="Insert at cursor"
      >
        <span class="match-hit__name">
          {#each renderSpans(hit.snippet.name, spans.nameSpans) as seg}{#if seg.hl}<mark>{seg.t}</mark>{:else}{seg.t}{/if}{/each}
        </span>
        {#if expandedIdx === i}
          <span class="match-hit__body">
            {#each renderSpans(hit.snippet.content, spans.contentSpans) as seg}{#if seg.hl}<mark>{seg.t}</mark>{:else}{seg.t}{/if}{/each}
          </span>
        {/if}
      </button>
    {/each}
    {#if truncated}
      <div class="match-panel__empty">
        Showing the first {MATCH_LIMIT} — narrow the list by typing.
      </div>
    {/if}
  {:else if prompts.activeProjectPath === null}
    <div class="match-panel__empty">
      No prompt folder yet. Add one with <strong>+</strong> above — pick any directory and every
      <code>.md</code> file in it becomes a snippet.
    </div>
  {:else if prompts.matchQuery.trim()}
    <div class="match-panel__empty">
      {prompts.matching ? 'Matching…' : 'No matching snippets.'}
    </div>
  {:else}
    <div class="match-panel__empty">
      No snippets in this folder yet. Write a prompt below and save it, or drop a <code>.md</code> file
      into the folder.
    </div>
  {/if}
</div>

<style>
  .match-panel {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }
  .match-hit {
    position: relative; /* anchors .match-hit__body */
    display: block;
    text-align: left;
    font-family: inherit;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 0.45rem;
    padding: 0.4rem 0.65rem;
    cursor: pointer;
    color: var(--text);
  }
  .match-hit:hover,
  .match-hit:focus-visible {
    border-color: color-mix(in srgb, var(--accent-snippet) 55%, var(--border));
    background: color-mix(in srgb, var(--accent-snippet) 7%, var(--bg-card));
    outline: none;
  }
  /* The name is a path (`rust/code_review`) — mono keeps the slash legible and
     the folder prefix scannable down a long list. */
  .match-hit__name {
    display: block;
    font-family: var(--font-mono);
    font-size: 0.72rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* Hover/focus-reveal of the full body (§1: "hover reveals, click edits" applied
     to the library too) as a floating tooltip — same pattern as the compose
     box's chip preview. Absolutely positioned so it overlays the list rather
     than pushing rows below it down; only the hovered/focused row pays for it,
     both here and in the {#if expandedIdx === i} above. */
  .match-hit__body {
    position: absolute;
    top: calc(100% + 0.3rem);
    left: 0;
    right: 0;
    z-index: 20;
    padding: 0.5rem 0.65rem;
    border: 1px solid var(--border);
    border-radius: 0.4rem;
    font-family: var(--font-mono);
    font-size: 0.7rem;
    line-height: 1.5;
    color: var(--text-muted);
    background: var(--bg-card);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25);
    white-space: pre-wrap;
    overflow-wrap: break-word;
    max-height: 12rem;
    overflow-y: auto;
    cursor: auto;
  }
  /* Query-token matches — same accent the row's own hover/focus border uses, so
     the highlight reads as "this is why this row is here" rather than a
     disconnected color. */
  .match-hit__name mark,
  .match-hit__body mark {
    background: color-mix(in srgb, var(--accent-snippet) 40%, transparent);
    color: var(--text);
    border-radius: 0.15rem;
    padding: 0 0.05rem;
  }
  .match-panel__empty {
    font-size: 0.72rem;
    color: var(--text-faint);
    padding: 0.4rem 0.2rem;
    line-height: 1.5;
  }
  .match-panel__empty code {
    font-family: var(--font-mono);
    font-size: 0.95em;
  }
</style>
