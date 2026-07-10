/**
 * Reactive Prompt Library store (Svelte 5 runes) — same idiom as
 * search.svelte.ts: one exported $state object + setter functions, a light
 * debounce on the live matcher, and a monotonic id so superseded match runs
 * are ignored.
 *
 * The compose doc, the variable fills, and the active tab live here (not in
 * components) so a draft prompt survives switching views — leaving Prompts
 * to check a session and coming back must not eat your composition.
 */
import type {
  Piece,
  MatchHit,
  PieceInput,
  PieceLoadError,
  EmbedStatus,
  EmbedProgress,
  Project,
  ProjectInput,
} from './prompts/types';
import {
  listPieces,
  pieceLoadErrors,
  savePiece as apiSavePiece,
  deletePiece as apiDeletePiece,
  listProjects,
  saveProject as apiSaveProject,
  deleteProject as apiDeleteProject,
  matchPieces,
  embedStatus,
  embedDownload,
  setEmbedEnabled,
  getAppConfig,
  setAppConfig,
} from './api';
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
import { copyText } from './compose/variables';

/** Light debounce so we don't hit the matcher on every literal keystroke. */
const DEBOUNCE_MS = 110;
/** Match panel size — small on purpose; it's a suggestion strip, not a browser. */
const MATCH_LIMIT = 8;

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
  // project roster + active tab (null = the Global tab)
  projects: [] as Project[],
  activeProjectId: null as string | null,
  // compose surface
  doc: emptyDoc() as Doc,
  caret: 0,
  selStart: 0,
  selEnd: 0,
  /** Bumped when the doc changes from OUTSIDE the textarea (panel insert,
   *  modal apply) — the compose box watches it to restore focus + caret. */
  focusNonce: 0,
  /** Unified variable fill values, keyed by name (grammar rule 4: one name =
   *  one variable document-wide). Entries for names no longer in the doc are
   *  kept — retyping a name recalls its value; copy only reads live names. */
  fills: {} as Record<string, string>,
  /** Copy-output mode (contract §Copy output), persisted in app config. */
  asVariable: true,
  /** Non-fatal persistence failure for the toggle — the in-session value
   *  still works; only restart survival is at risk. Never silently hidden. */
  configError: null as string | null,
  // live matching
  matchQuery: '',
  hits: [] as ResolvedHit[],
  matching: false,
  // embeddings (opt-in, behind the config popover)
  embed: null as EmbedStatus | null,
  embedProgress: null as EmbedProgress | null,
  embedError: null as string | null,
});

let matchId = 0;
let debounceTimer: ReturnType<typeof setTimeout> | null = null;
let configLoaded = false;

// ── lifecycle ────────────────────────────────────────────────────────────────

/** Load pieces, the project roster, embed status, and (once) the persisted
 *  copy-mode toggle. Idempotent per session — re-entering the view refreshes
 *  the library but keeps the compose doc and fills. */
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
  try {
    prompts.projects = await listProjects();
    if (
      prompts.activeProjectId !== null &&
      !prompts.projects.some((p) => p.id === prompts.activeProjectId)
    ) {
      prompts.activeProjectId = null; // the active project vanished — fall back to Global
    }
  } catch (e) {
    // Tabs degrade to Global-only, but say why rather than rendering a
    // mysteriously bare tab row.
    prompts.loadError ??= e instanceof Error ? e.message : String(e);
  }
  refreshEmbedStatus();
  if (!configLoaded) {
    configLoaded = true;
    try {
      prompts.asVariable = (await getAppConfig()).promptsAsVariable;
    } catch {
      // The default (true) stands; a persist failure surfaces on toggle.
    }
  }
}

/** Stop timers when leaving the view (the doc itself is kept — see header). */
export function disposePrompts(): void {
  if (debounceTimer) clearTimeout(debounceTimer);
  debounceTimer = null;
  matchId++; // ignore any in-flight match
}

// ── projects / tabs ──────────────────────────────────────────────────────────

/** Switch the active tab (null = Global). Drives the match pool, the save
 *  scope for new pieces, and the view tint. */
export function setActiveProject(id: string | null): void {
  prompts.activeProjectId = id;
  scheduleMatch();
}

/** The active tab's project record (null on the Global tab). Reactive when
 *  read inside a $derived. */
export function activeProject(): Project | null {
  return prompts.projects.find((p) => p.id === prompts.activeProjectId) ?? null;
}

/** Create or update a project and sync the roster. Returns the stored record. */
export async function saveProject(input: ProjectInput): Promise<Project> {
  const saved = await apiSaveProject(input);
  const i = prompts.projects.findIndex((p) => p.id === saved.id);
  if (i >= 0) prompts.projects[i] = saved;
  else prompts.projects.push(saved);
  return saved;
}

/** Delete a project. Its pieces rescope to GLOBAL (contract semantics — the
 *  writing never vanishes), so the piece list is re-fetched; an active tab
 *  pointing at it falls back to Global. */
export async function deleteProject(id: string): Promise<void> {
  await apiDeleteProject(id);
  prompts.projects = prompts.projects.filter((p) => p.id !== id);
  if (prompts.activeProjectId === id) prompts.activeProjectId = null;
  try {
    prompts.pieces = await listPieces();
  } catch (e) {
    prompts.loadError = e instanceof Error ? e.message : String(e);
  }
  scheduleMatch();
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
    const hits = await matchPieces(query, prompts.activeProjectId, MATCH_LIMIT);
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

/** Insert a piece's RAW body at the caret as a linked span — {var} tokens
 *  land verbatim and merge into the unified fill list (fill-at-insert is
 *  retired; variables resolve at copy time). */
export function composeInsertPiece(piece: Piece): Doc {
  const link: SpanLink = { pieceId: piece.id, title: piece.title, scope: piece.scope };
  const at = prompts.caret;
  prompts.doc = docInsertPiece(prompts.doc, at, piece.body, link);
  setSelection(at + piece.body.length, at + piece.body.length);
  prompts.focusNonce++;
  return prompts.doc;
}

/** Replace one span's text + metadata (piece-modal save refreshing the
 *  span's link metadata / provenance state — never the composed text). */
export function composeReplaceSpan(index: number, newText: string, span: Omit<Span, 'length'>): void {
  const start = spanStarts(prompts.doc)[index];
  prompts.doc = replaceSpan(prompts.doc, index, newText, span);
  setSelection(start + newText.length, start + newText.length);
  prompts.focusNonce++;
}

/** After save-selection-as-piece: the saved selection becomes a linked span
 *  pointing at the new piece (linked-modified when the saved body was edited
 *  away from the selection before saving). */
export function composeLinkRange(
  start: number,
  end: number,
  link: SpanLink,
  state: 'linked' | 'linked-modified' = 'linked'
): void {
  prompts.doc = linkRange(prompts.doc, start, end, link, state);
  prompts.focusNonce++;
}

/** One fill input changed (the unified variable list under the box). */
export function setFill(name: string, value: string): void {
  prompts.fills[name] = value;
}

/** The Copy Prompt deliverable: the doc's raw text through the copy pipeline
 *  (escapes resolved; XML dedup or substitute-in-place per the toggle). */
export function copyOutput(): string {
  return copyText(prompts.doc.text, prompts.fills, prompts.asVariable);
}

/** Flip the copy-output mode and persist it (app config, read-modify-write
 *  so unrelated fields survive). A failed persist keeps the in-session value
 *  and surfaces — losing the preference on restart must not be silent. */
export async function setAsVariable(value: boolean): Promise<void> {
  prompts.asVariable = value;
  prompts.configError = null;
  try {
    const cfg = await getAppConfig();
    await setAppConfig({ ...cfg, promptsAsVariable: value });
  } catch (e) {
    prompts.configError = `Couldn't save the copy-mode preference: ${
      e instanceof Error ? e.message : String(e)
    }`;
  }
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

// ── embeddings (opt-in, behind the config popover) ───────────────────────────

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
  prompts.embedProgress = { stage: 'runtime', done: 0, total: 0 };
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
