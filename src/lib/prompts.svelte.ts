/**
 * Reactive Prompt Library store (Svelte 5 runes) — same idiom as
 * search.svelte.ts: one exported $state object + setter functions, a light
 * debounce on the live matcher, and a monotonic id so superseded match runs
 * are ignored.
 *
 * The compose doc lives here (not in the component) so a draft prompt
 * survives switching views — leaving Prompts to check a session and coming
 * back must not eat your composition.
 */
import type {
  Piece,
  MatchHit,
  PieceInput,
  PieceLoadError,
  EmbedStatus,
  EmbedProgress,
} from './prompts/types';
import {
  listPieces,
  pieceLoadErrors,
  savePiece as apiSavePiece,
  deletePiece as apiDeletePiece,
  matchPieces,
  embedStatus,
  embedDownload,
  setEmbedEnabled,
  listSessions,
  homeDir,
} from './api';
import { projectLabel } from './parser';
import {
  type Doc,
  type SpanLink,
  type Span,
  emptyDoc,
  applyEdit,
  insertPiece as docInsertPiece,
  replaceSpan,
  linkRange,
  caretQuery,
  spanStarts,
} from './compose/doc';
import { substitute } from './compose/placeholders';

/** Light debounce so we don't hit the matcher on every literal keystroke. */
const DEBOUNCE_MS = 110;
/** Match panel size — small on purpose; it's a suggestion strip, not a browser. */
const MATCH_LIMIT = 8;

/** A project option for the picker: `cwd` is the identity the backend
 *  matches piece scope against (absolute path, per the contract); `label` is
 *  the home-relative display name the rest of the app uses. */
export interface PromptProjectOption {
  cwd: string;
  label: string;
}

export interface ResolvedHit {
  piece: Piece;
  score: number;
  source: MatchHit['source'];
}

export const prompts = $state({
  // library
  pieces: [] as Piece[],
  loadError: null as string | null,
  /** Hand-edited piece files that failed to parse on the last load pass —
   *  shown as a dismissable notice so a typo never reads as a lost piece. */
  pieceLoadErrors: [] as PieceLoadError[],
  // active scope: null = "Global only", else a project's absolute cwd
  project: null as string | null,
  availableProjects: [] as PromptProjectOption[],
  // compose surface
  doc: emptyDoc() as Doc,
  caret: 0,
  selStart: 0,
  selEnd: 0,
  /** Bumped when the doc changes from OUTSIDE the textarea (panel insert,
   *  modal apply) — the compose box watches it to restore focus + caret. */
  focusNonce: 0,
  // live matching
  matchQuery: '',
  hits: [] as ResolvedHit[],
  matching: false,
  // embeddings (advanced, opt-in)
  embed: null as EmbedStatus | null,
  embedProgress: null as EmbedProgress | null,
  embedError: null as string | null,
});

let matchId = 0;
let debounceTimer: ReturnType<typeof setTimeout> | null = null;
let initialized = false;

// ── lifecycle ────────────────────────────────────────────────────────────────

/** Load pieces, the project list, and embed status. Idempotent per session —
 *  re-entering the view refreshes the library but keeps the compose doc. */
export async function initPrompts(): Promise<void> {
  try {
    prompts.pieces = await listPieces();
    prompts.loadError = null;
  } catch (e) {
    prompts.loadError = e instanceof Error ? e.message : String(e);
  }
  try {
    prompts.pieceLoadErrors = await pieceLoadErrors();
  } catch {
    // Diagnostic surface only — if the command itself fails, the primary
    // listPieces error above already tells the user loading is broken;
    // keep whatever list we last had rather than flapping the notice.
  }
  refreshEmbedStatus();
  if (initialized) return;
  initialized = true;
  try {
    const [sessions, home] = await Promise.all([listSessions(), homeDir()]);
    const byCwd = new Map<string, string>();
    for (const s of sessions) {
      // Only sessions with a real cwd can anchor project-scoped pieces — the
      // scope identity is the absolute path, not the display label.
      if (s.cwd && !byCwd.has(s.cwd)) byCwd.set(s.cwd, projectLabel(s.cwd, s.project_raw, home));
    }
    prompts.availableProjects = [...byCwd.entries()]
      .map(([cwd, label]) => ({ cwd, label }))
      .sort((a, b) => a.label.localeCompare(b.label));
  } catch {
    // non-fatal; the picker just offers "Global only"
  }
}

/** Stop timers when leaving the view (the doc itself is kept — see header). */
export function disposePrompts(): void {
  if (debounceTimer) clearTimeout(debounceTimer);
  debounceTimer = null;
  matchId++; // ignore any in-flight match
}

// ── scope ────────────────────────────────────────────────────────────────────

export function setProject(cwd: string | null): void {
  prompts.project = cwd;
  scheduleMatch();
}

/** Display label for the active project ("" when global-only). */
export function activeProjectLabel(): string {
  return prompts.availableProjects.find((p) => p.cwd === prompts.project)?.label ?? '';
}

// ── live matching ────────────────────────────────────────────────────────────

function scheduleMatch(): void {
  if (debounceTimer) clearTimeout(debounceTimer);
  debounceTimer = setTimeout(runMatch, DEBOUNCE_MS);
}

async function runMatch(): Promise<void> {
  if (debounceTimer) {
    clearTimeout(debounceTimer);
    debounceTimer = null;
  }
  const id = ++matchId;
  const query = prompts.matchQuery;
  if (!query.trim()) {
    prompts.hits = [];
    prompts.matching = false;
    return;
  }
  prompts.matching = true;
  try {
    const hits = await matchPieces(query, prompts.project, MATCH_LIMIT);
    if (id !== matchId) return; // superseded
    const byId = new Map(prompts.pieces.map((p) => [p.id, p]));
    prompts.hits = hits.flatMap((h) => {
      const piece = byId.get(h.id);
      return piece ? [{ piece, score: h.score, source: h.source }] : [];
    });
    prompts.matching = false;
  } catch {
    if (id !== matchId) return;
    // Matching is a live suggestion strip — a failed run degrades to "no
    // suggestions" without nuking the panel; store errors surface on save.
    prompts.hits = [];
    prompts.matching = false;
  }
}

// ── compose surface ──────────────────────────────────────────────────────────

/** Track the caret/selection (from the textarea) without triggering a match. */
export function setSelection(start: number, end: number): void {
  prompts.selStart = start;
  prompts.selEnd = end;
  prompts.caret = end;
}

/** One inline edit (any input event, translated by the compose box into a
 *  single replacement). Drives both the provenance state machine and the
 *  live matcher. */
export function composeEdit(start: number, end: number, inserted: string): void {
  prompts.doc = applyEdit(prompts.doc, start, end, inserted);
  const caret = start + inserted.length;
  setSelection(caret, caret);
  prompts.matchQuery = caretQuery(prompts.doc.text, caret);
  scheduleMatch();
}

/** Insert a piece at the caret as a linked span (F2). `fills` are the
 *  placeholder values from the popover (empty for placeholder-less pieces). */
export function composeInsertPiece(piece: Piece, fills: Record<string, string>): Doc {
  const rendered = substitute(piece.body, fills);
  const link: SpanLink = {
    pieceId: piece.id,
    title: piece.title,
    scope: piece.scope,
    template: piece.body,
    fills,
  };
  const at = prompts.caret;
  prompts.doc = docInsertPiece(prompts.doc, at, rendered, link);
  setSelection(at + rendered.length, at + rendered.length);
  prompts.focusNonce++;
  return prompts.doc;
}

/** Replace one span's text + metadata (instance-mode Apply, re-fill,
 *  save-back relink). */
export function composeReplaceSpan(index: number, newText: string, span: Omit<Span, 'length'>): void {
  const start = spanStarts(prompts.doc)[index];
  prompts.doc = replaceSpan(prompts.doc, index, newText, span);
  setSelection(start + newText.length, start + newText.length);
  prompts.focusNonce++;
}

/** After save-selection-as-piece (F4): the saved selection becomes a linked
 *  span pointing at the new piece (linked-modified when the saved body was
 *  edited away from the selection before saving). */
export function composeLinkRange(
  start: number,
  end: number,
  link: SpanLink,
  state: 'linked' | 'linked-modified' = 'linked'
): void {
  prompts.doc = linkRange(prompts.doc, start, end, link, state);
  prompts.focusNonce++;
}

// ── piece store ──────────────────────────────────────────────────────────────

/** Save (create or update) and sync the local list. Returns the stored piece. */
export async function savePiece(input: PieceInput): Promise<Piece> {
  const saved = await apiSavePiece(input);
  const i = prompts.pieces.findIndex((p) => p.id === saved.id);
  if (i >= 0) prompts.pieces[i] = saved;
  else prompts.pieces.push(saved);
  scheduleMatch(); // the library changed under the current query
  return saved;
}

export async function deletePiece(id: string): Promise<void> {
  await apiDeletePiece(id);
  const i = prompts.pieces.findIndex((p) => p.id === id);
  if (i >= 0) prompts.pieces.splice(i, 1);
  scheduleMatch();
}

// ── embeddings (opt-in, advanced) ────────────────────────────────────────────

export async function refreshEmbedStatus(): Promise<void> {
  try {
    prompts.embed = await embedStatus();
    prompts.embedError = null;
  } catch (e) {
    prompts.embedError = e instanceof Error ? e.message : String(e);
  }
}

export async function startEmbedDownload(): Promise<void> {
  prompts.embedError = null;
  prompts.embedProgress = { stage: 'runtime', downloaded_bytes: 0, total_bytes: 0 };
  if (prompts.embed) prompts.embed = { ...prompts.embed, state: 'downloading' };
  try {
    await embedDownload((p) => {
      prompts.embedProgress = p;
    });
  } catch (e) {
    prompts.embedError = e instanceof Error ? e.message : String(e);
  } finally {
    prompts.embedProgress = null;
    await refreshEmbedStatus();
  }
}

export async function toggleEmbedEnabled(enabled: boolean): Promise<void> {
  try {
    await setEmbedEnabled(enabled);
  } catch (e) {
    prompts.embedError = e instanceof Error ? e.message : String(e);
  }
  await refreshEmbedStatus();
  scheduleMatch(); // engine change can reorder the current suggestions
}
