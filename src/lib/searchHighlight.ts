/**
 * Search-result rendering helpers shared by every surface that renders search
 * hits (the merged Browse+Search home view and the in-chat find panel). Kept in
 * one place so the highlight/badge/key logic can't drift between them.
 */
import type { SearchHit } from './types';

/** A snippet slice plus whether it falls inside a match range (rendered as
 *  `<mark>` when `hl`). */
export interface Seg {
  t: string;
  hl: boolean;
}

/** Stable identity for a hit within a result list — session + line + block. */
export function hitKey(h: SearchHit): string {
  return `${h.sessionPath}:${h.lineNo}:${h.blockNo}`;
}

// Search is messages-only (#35): the backend only ever indexes user/assistant
// text blocks, so `source` is one of those two. The default is a safety net.
export function sourceBadge(source: string): { label: string; cls: string } {
  switch (source) {
    case 'user': return { label: 'You', cls: 'b-user' };
    case 'assistant': return { label: 'Claude', cls: 'b-asst' };
    default: return { label: source, cls: 'b-user' };
  }
}

/** Split `snippet` into highlighted/plain segments per the match `ranges`.
 *  Slices via `Array.from` (not string indexing) because the Rust side emits
 *  match offsets as code-point (char) indices, not UTF-16 code-unit indices —
 *  indexing the JS string directly would mis-slice any snippet containing an
 *  astral character (emoji, etc.). Load-bearing: keep the `Array.from`. */
export function highlight(snippet: string, ranges: [number, number][]): Seg[] {
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
