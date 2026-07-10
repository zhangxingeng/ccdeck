/**
 * Compose-box document model — the provenance state machine from
 * project_docs/prompts-design.md (§Compose surface), as pure data + pure
 * transforms. No DOM, no Svelte: the <textarea> is only an input device; this
 * model is the single source of truth, which is what makes the state machine
 * unit-testable (tests/prompts_smoke.mjs) and the Copy promise trivial.
 *
 * Invariants (restored by normalize() after every transform):
 *   - spans tile the text exactly: sum(span.length) === text.length
 *   - no zero-length spans
 *   - no two adjacent 'typed' spans (merged)
 */
import type { PieceScope } from '../prompts/types';

export type SpanState = 'typed' | 'linked' | 'linked-modified';

/** Provenance carried by a linked / linked-modified span: which piece the
 *  text came from, nothing more. The document holds RAW literal text —
 *  {var} tokens included — and variables resolve only at copy time, so
 *  there is no per-span template/fills state to carry. */
export interface SpanLink {
  pieceId: string;
  title: string;
  scope: PieceScope;
}

export interface Span {
  state: SpanState;
  length: number;
  link?: SpanLink; // present iff state !== 'typed'
}

export interface Doc {
  text: string;
  spans: Span[];
}

export function emptyDoc(): Doc {
  return { text: '', spans: [] };
}

/** A doc that is all typed text (dev/test convenience). */
export function docFromText(text: string): Doc {
  return normalize({ text, spans: text ? [{ state: 'typed', length: text.length }] : [] });
}

function typed(length: number): Span {
  return { state: 'typed', length };
}

/** Drop zero-length spans and merge adjacent typed runs. */
function normalize(doc: Doc): Doc {
  const spans: Span[] = [];
  for (const s of doc.spans) {
    if (s.length <= 0) continue;
    const prev = spans[spans.length - 1];
    if (prev && prev.state === 'typed' && s.state === 'typed') prev.length += s.length;
    else spans.push({ ...s });
  }
  return { text: doc.text, spans };
}

/** Start offset of each span (parallel array). */
export function spanStarts(doc: Doc): number[] {
  const starts: number[] = [];
  let off = 0;
  for (const s of doc.spans) {
    starts.push(off);
    off += s.length;
  }
  return starts;
}

export interface SpanRef {
  index: number;
  start: number;
  end: number;
  span: Span;
}

/** The linked/linked-modified span the caret is "in" for the edit-affordance
 *  chip: interior wins, then the span ending exactly at the caret, then the
 *  one starting there. Returns null over typed text. */
export function linkedSpanAt(doc: Doc, caret: number): SpanRef | null {
  const starts = spanStarts(doc);
  let atEnd: SpanRef | null = null;
  let atStart: SpanRef | null = null;
  for (let i = 0; i < doc.spans.length; i++) {
    const span = doc.spans[i];
    if (span.state === 'typed') continue;
    const start = starts[i];
    const end = start + span.length;
    const ref = { index: i, start, end, span };
    if (caret > start && caret < end) return ref;
    if (caret === end) atEnd = ref;
    if (caret === start && !atStart) atStart = ref;
  }
  return atEnd ?? atStart;
}

/** Text content of span `index`. */
export function spanText(doc: Doc, index: number): string {
  const start = spanStarts(doc)[index];
  return doc.text.slice(start, start + doc.spans[index].length);
}

/**
 * Insert a piece's raw body text at `offset` as a fresh 'linked' span
 * ({var} tokens land verbatim — they resolve at copy time). Splitting a
 * linked span in two marks both halves linked-modified — the original piece
 * no longer appears intact, which is exactly what that state signals.
 */
export function insertPiece(doc: Doc, offset: number, text: string, link: SpanLink): Doc {
  if (!text) return doc;
  const starts = spanStarts(doc);
  const spans: Span[] = [];
  let placed = false;
  const newSpan: Span = { state: 'linked', length: text.length, link };

  for (let i = 0; i < doc.spans.length; i++) {
    const s = doc.spans[i];
    const start = starts[i];
    const end = start + s.length;
    if (!placed && offset > start && offset < end) {
      // Split this span around the insertion point.
      const demote = s.state !== 'typed';
      const state: SpanState = demote ? 'linked-modified' : s.state;
      spans.push({ ...s, state, length: offset - start });
      spans.push(newSpan);
      spans.push({ ...s, state, length: end - offset });
      placed = true;
    } else {
      if (!placed && offset === start) {
        spans.push(newSpan);
        placed = true;
      }
      spans.push({ ...s });
    }
  }
  if (!placed) spans.push(newSpan); // offset === text.length (or empty doc)

  return normalize({
    text: doc.text.slice(0, offset) + text + doc.text.slice(offset),
    spans,
  });
}

/**
 * The general inline edit: replace [start, end) with `inserted` (user-typed
 * text). This is the provenance state machine's core transition table:
 *
 * - Pure insertion strictly inside a span is absorbed by it; a linked span
 *   absorbing an edit becomes linked-modified (F1: inline edit diverges the
 *   span, never touches the stored piece).
 * - Pure insertion at a span boundary is new typed text — typing at the edge
 *   of a linked span must not silently grow the piece's claimed region.
 * - A replacement contained in one span (but not covering all of it) is an
 *   inline edit of that span: absorbed, linked → linked-modified.
 * - Replacing a span exactly and entirely removes it; the inserted text is
 *   typed (you replaced the piece's text wholesale with your own).
 * - A range crossing span boundaries clips every overlapped span (a clipped
 *   linked span becomes linked-modified; fully covered spans are removed) and
 *   the inserted text lands as a new typed span at the cut.
 */
export function applyEdit(doc: Doc, start: number, end: number, inserted: string): Doc {
  const text = doc.text.slice(0, start) + inserted + doc.text.slice(end);
  const starts = spanStarts(doc);
  const spans: Span[] = [];

  const containerIdx = doc.spans.findIndex((s, i) => {
    const s0 = starts[i];
    const s1 = s0 + s.length;
    return start === end
      ? s0 < start && start < s1 // pure insertion: strictly interior only
      : s0 <= start && end <= s1 && !(start === s0 && end === s1);
  });

  if (containerIdx >= 0) {
    // Absorbed by one span.
    for (let i = 0; i < doc.spans.length; i++) {
      const s = doc.spans[i];
      if (i !== containerIdx) {
        spans.push({ ...s });
        continue;
      }
      const newLen = s.length - (end - start) + inserted.length;
      const state: SpanState = s.state === 'typed' ? 'typed' : 'linked-modified';
      if (newLen > 0) spans.push({ ...s, state, length: newLen });
    }
    return normalize({ text, spans });
  }

  // Boundary insertion, whole-span replacement, or a cross-span range:
  // clip overlapped spans, put `inserted` as typed text at the cut.
  let insertedPlaced = false;
  for (let i = 0; i < doc.spans.length; i++) {
    const s = doc.spans[i];
    const s0 = starts[i];
    const s1 = s0 + s.length;

    const keptLeft = Math.max(0, Math.min(start, s1) - s0);
    const keptRight = Math.max(0, s1 - Math.max(end, s0));
    const clipped = keptLeft + keptRight < s.length;

    if (!insertedPlaced && s1 > start && keptLeft >= 0 && s0 <= start) {
      // This span carries the cut point: left remainder, inserted, right remainder.
      if (keptLeft > 0) {
        const state: SpanState = clipped && s.state !== 'typed' ? 'linked-modified' : s.state;
        spans.push({ ...s, state, length: keptLeft });
      }
      if (inserted) spans.push(typed(inserted.length));
      insertedPlaced = true;
      if (keptRight > 0) {
        const state: SpanState = clipped && s.state !== 'typed' ? 'linked-modified' : s.state;
        spans.push({ ...s, state, length: keptRight });
      }
      continue;
    }

    const kept = keptLeft + keptRight;
    if (kept > 0) {
      const state: SpanState = clipped && s.state !== 'typed' ? 'linked-modified' : s.state;
      spans.push({ ...s, state, length: kept });
    }
  }
  if (!insertedPlaced && inserted) spans.push(typed(inserted.length)); // append at doc end

  return normalize({ text, spans });
}

/** Replace span `index`'s text and metadata wholesale (instance-mode Apply,
 *  placeholder re-fill, save-back relink). The caller decides the new state —
 *  this transform is mechanical. */
export function replaceSpan(doc: Doc, index: number, newText: string, span: Omit<Span, 'length'>): Doc {
  const starts = spanStarts(doc);
  const start = starts[index];
  const old = doc.spans[index];
  const spans = doc.spans.map((s, i) =>
    i === index ? ({ ...span, length: newText.length } as Span) : { ...s }
  );
  return normalize({
    text: doc.text.slice(0, start) + newText + doc.text.slice(start + old.length),
    spans,
  });
}

/** Convert [start, end) into one linked span (F4: save-selection-as-piece —
 *  the selection becomes a linked span pointing at the new piece). Text is
 *  unchanged; overlapped spans are clipped (a clipped linked span keeps its
 *  own link but no longer appears intact → linked-modified). */
export function linkRange(
  doc: Doc,
  start: number,
  end: number,
  link: SpanLink,
  state: Exclude<SpanState, 'typed'> = 'linked'
): Doc {
  if (end <= start) return doc;
  const starts = spanStarts(doc);
  const spans: Span[] = [];
  let placed = false;
  for (let i = 0; i < doc.spans.length; i++) {
    const s = doc.spans[i];
    const s0 = starts[i];
    const s1 = s0 + s.length;
    const keptLeft = Math.max(0, Math.min(start, s1) - s0);
    const keptRight = Math.max(0, s1 - Math.max(end, s0));
    const clipped = keptLeft + keptRight < s.length;
    if (keptLeft > 0) {
      const state: SpanState = clipped && s.state !== 'typed' ? 'linked-modified' : s.state;
      spans.push({ ...s, state, length: keptLeft });
    }
    if (!placed && s1 > start) {
      spans.push({ state, length: end - start, link });
      placed = true;
    }
    if (keptRight > 0) {
      const state: SpanState = clipped && s.state !== 'typed' ? 'linked-modified' : s.state;
      spans.push({ ...s, state, length: keptRight });
    }
  }
  return normalize({ text: doc.text, spans });
}

/**
 * The document's raw literal text — provenance is a span-level annotation,
 * never markup inside `text`. Copy Prompt feeds this through the variable
 * copy pipeline (compose/variables.ts); everything else (the match query,
 * the fill list) reads the same raw text. Named (rather than callers
 * reading .text) because it IS the "spans tile the text" promise.
 */
export function flatten(doc: Doc): string {
  return doc.text;
}

/** The live-match query for a caret position: what you're typing right now =
 *  the current line up to the caret (trimmed, tail-capped so one long line
 *  doesn't swamp the matcher). */
export function caretQuery(text: string, caret: number, cap = 120): string {
  const upto = text.slice(0, Math.max(0, Math.min(caret, text.length)));
  const lineStart = upto.lastIndexOf('\n') + 1;
  const line = upto.slice(lineStart).trim();
  return line.length > cap ? line.slice(line.length - cap) : line;
}

/**
 * Minimal-diff between two texts as a single replacement: returns the range
 * [start, end) in `oldText` and the `inserted` replacement from `newText`.
 * Used by the compose box to translate a textarea `input` event (any
 * inputType — typing, paste, cut, undo, IME) into one applyEdit call: common
 * prefix/suffix trimming is deterministic and covers every mutation the
 * browser can make in one event. Returns null when the texts are equal.
 */
export function diffTexts(
  oldText: string,
  newText: string
): { start: number; end: number; inserted: string } | null {
  if (oldText === newText) return null;
  let p = 0;
  const maxP = Math.min(oldText.length, newText.length);
  while (p < maxP && oldText[p] === newText[p]) p++;
  let so = oldText.length;
  let sn = newText.length;
  while (so > p && sn > p && oldText[so - 1] === newText[sn - 1]) {
    so--;
    sn--;
  }
  return { start: p, end: so, inserted: newText.slice(p, sn) };
}
