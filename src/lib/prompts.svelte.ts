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
  Snippet,
  MatchHit,
  SnippetInput,
  SnippetLoadError,
  EmbedStatus,
  EmbedProgress,
  Project,
  ProjectInput,
} from './prompts/types';
import {
  listSnippets,
  snippetLoadErrors,
  saveSnippet as apiSaveSnippet,
  deleteSnippet as apiDeleteSnippet,
  listProjects,
  saveProject as apiSaveProject,
  deleteProject as apiDeleteProject,
  matchSnippets,
  embedStatus,
  embedDownload,
  setEmbedEnabled,
  getAppConfig,
  setAppConfig,
} from './api';
import {
  type Doc,
  type Caret,
  type RawNode,
  emptyDoc,
  newCid,
  fromRawNodes,
  insertChip,
  replaceChipContent,
  retargetChip,
  dissolveChip,
  flatten,
  caretQuery,
} from './compose/doc';
import { copyText } from './compose/variables';
import {
  resolveHotkeys,
  resolveHotkeysReport,
  HOTKEY_LABELS,
  type HotkeyCommand,
} from './prompts/hotkeys';
import { deriveNotices, type Notice } from './prompts/notices';
import { toasts } from './prompts/toasts.svelte';

/** Light debounce so we don't hit the matcher on every literal keystroke. */
const DEBOUNCE_MS = 110;
/** Match panel size — small on purpose; it's a suggestion strip, not a browser. */
const MATCH_LIMIT = 8;

export interface ResolvedHit {
  snippet: Snippet;
  score: number;
  source: MatchHit['source'];
}

export const prompts = $state({
  // library
  snippets: [] as Snippet[],
  loadError: null as string | null,
  /** Hand-edited snippet files that failed to parse on the last load pass —
   *  shown as a dismissable notice so a typo never reads as a lost snippet. */
  snippetLoadErrors: [] as SnippetLoadError[],
  // project roster + active tab (null = the Global tab)
  projects: [] as Project[],
  activeProjectId: null as string | null,
  // compose surface
  doc: emptyDoc() as Doc,
  /** Where the caret sits, in MODEL terms (null = not in the box). */
  caret: null as Caret | null,
  /** The text of the node the caret is in — the live-match query reads it. */
  caretText: '',
  /** Bumped ONLY when the doc changes from outside the box (a panel insert, a
   *  popup save/delete). The box repaints on this and nothing else: repainting on
   *  `doc` itself would fire on every keystroke and destroy the user's caret. */
  renderNonce: 0,
  /** After an external insert, the chip the caret should land after — so the next
   *  keystroke continues the sentence rather than landing where the browser
   *  guessed. Consumed (nulled) by the box once placed. */
  pendingCaretCid: null as string | null,
  /** Unified variable fill values, keyed by name (grammar rule 4: one name =
   *  one variable document-wide). Entries for names no longer in the doc are
   *  kept — retyping a name recalls its value; copy only reads live names. */
  fills: {} as Record<string, string>,
  /** Per-variable as-variable state (contract §Copy output), keyed by name. A
   *  name absent here is ON — the founder's safe default (as-var never breaks;
   *  in-place substitution of unexpected data can bloat the prompt). Session-
   *  only, never persisted to the snippet ([JC-9]). */
  asVars: {} as Record<string, boolean>,
  /** Stored hotkey overrides (command id → chord), loaded from app config;
   *  merged over the defaults by resolveHotkeys. Absent command = default. */
  hotkeyOverrides: {} as Record<string, string>,
  /** Non-fatal config persistence failure (hotkey rebind) — the in-session
   *  binding still works; only restart survival is at risk. Never hidden. */
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

/** Load snippets, the project roster, embed status, and (once) the persisted
 *  hotkey overrides. Idempotent per session — re-entering the view refreshes
 *  the library but keeps the compose doc and fills. */
export async function initPrompts(): Promise<void> {
  try {
    prompts.snippets = await listSnippets();
    prompts.loadError = null;
  } catch (e) {
    prompts.loadError = e instanceof Error ? e.message : String(e);
  }
  try {
    prompts.snippetLoadErrors = await snippetLoadErrors();
  } catch {
    // Diagnostic surface only — if the command itself fails, the primary
    // listSnippets error above already tells the user loading is broken;
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
      prompts.hotkeyOverrides = (await getAppConfig()).hotkeys;
    } catch {
      // Defaults stand (resolveHotkeys handles an empty map); a persist
      // failure surfaces on rebind, not here.
    }
  }
  // Data events (repairs / unreadable files / a reset hotkey) flash a 5s toast;
  // the durable trace lives on in the gear's Notices (contract §S13). Runs on
  // EVERY view entry, not just the first — a repair discovered on a later load
  // is a new data event and must announce too — but toasts only what's new
  // since the last announcement, so re-entering a view with the same unresolved
  // events stays quiet.
  announceDataEvents();
}

/** The durable data-event notices (contract §S13 / §Store robustness): loader-
 *  repaired snippets first (a one-click re-save fixes them), then unreadable
 *  files. Derived, not stored — the sources are the snippet list + load errors. */
export function notices(): Notice[] {
  // Invalid hotkey overrides (hand-edited config that fell back to a default)
  // are a config data event — derived from the same overrides resolveHotkeys
  // reads, so the trace stays consistent with what actually bound.
  const invalidHotkeys = resolveHotkeysReport(prompts.hotkeyOverrides).invalid.map((e) => ({
    command: e.command,
    label: HOTKEY_LABELS[e.command],
    chord: e.chord,
    reason: e.reason,
  }));
  return deriveNotices(
    prompts.snippets.filter((s) => s.recovered).map((s) => ({ id: s.id, title: s.title })),
    prompts.snippetLoadErrors,
    invalidHotkeys
  );
}

/** Notice ids already announced this session — so a data event toasts once, when
 *  it first appears, and re-entering the view with the same unresolved events
 *  stays quiet. Keyed by the stable notice id (snippet id / file path / hotkey
 *  command), the same key the Notices list and badge use. */
const announcedNoticeIds = new Set<string>();

/** Flash a single 5s toast summarizing the data events that are NEW since the
 *  last announcement (contract §S13's transient half). The toast is the
 *  transient announce; the gear badge + Notices section is the record that
 *  outlives it. Announcing per-new-event (not once per session) is why a repair
 *  surfaced on a later view entry still toasts. */
function announceDataEvents(): void {
  const fresh = notices().filter((n) => !announcedNoticeIds.has(n.id));
  if (fresh.length === 0) return;
  for (const n of fresh) announcedNoticeIds.add(n.id);
  const repaired = fresh.filter((n) => n.kind === 'repaired').length;
  const unreadable = fresh.filter((n) => n.kind === 'unreadable').length;
  const config = fresh.filter((n) => n.kind === 'config').length;
  const parts: string[] = [];
  if (repaired) parts.push(`${repaired} snippet${repaired === 1 ? '' : 's'} auto-repaired`);
  if (unreadable) parts.push(`${unreadable} file${unreadable === 1 ? '' : 's'} couldn't be read`);
  if (config) parts.push(`${config} shortcut${config === 1 ? '' : 's'} reset to default`);
  toasts.push(`${parts.join(' · ')} — see the gear for details.`);
}

/** Stop timers when leaving the view (the doc itself is kept — see header). */
export function disposePrompts(): void {
  if (debounceTimer) clearTimeout(debounceTimer);
  debounceTimer = null;
  matchId++; // ignore any in-flight match
}

// ── projects / tabs ──────────────────────────────────────────────────────────

/** Switch the active tab (null = Global). Drives the match pool, the save
 *  scope for new snippets, and the view tint. */
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

/** Delete a project. Its snippets rescope to GLOBAL (contract semantics — the
 *  writing never vanishes), so the snippet list is re-fetched; an active tab
 *  pointing at it falls back to Global. */
export async function deleteProject(id: string): Promise<void> {
  await apiDeleteProject(id);
  prompts.projects = prompts.projects.filter((p) => p.id !== id);
  if (prompts.activeProjectId === id) prompts.activeProjectId = null;
  try {
    prompts.snippets = await listSnippets();
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
    const hits = await matchSnippets(query, prompts.activeProjectId, MATCH_LIMIT);
    if (id !== matchId) return; // superseded
    const byId = new Map(prompts.snippets.map((p) => [p.id, p]));
    prompts.hits = hits.flatMap((h) => {
      const snippet = byId.get(h.id);
      return snippet ? [{ snippet, score: h.score, source: h.source }] : [];
    });
    prompts.matching = false;
  } catch (e) {
    if (id !== matchId) return;
    prompts.matching = false;
    prompts.hits = [];
    // The one failure we expect here is a transient backend/IPC error — Tauri's
    // Result<_, String> rejects with a *string*. The match panel is a
    // suggestion strip, not a save path, so that degrades quietly to "no
    // suggestions" (store errors surface on the save path, which is guarded).
    if (typeof e === 'string') return;
    // Anything else is a programming error wearing a "No matching snippets."
    // costume — a user reads that as "nothing matched," not "matching is
    // broken." Don't let it hide: log and re-throw so it surfaces as a failure.
    console.error('Prompt match failed unexpectedly:', e);
    throw e;
  }
}

// ── compose surface ──────────────────────────────────────────────────────────

/**
 * The box's content changed. Rebuilt from what the DOM now holds rather than
 * patched edit-by-edit, so typing, paste, cut, undo and IME all arrive the same
 * way. Deliberately does NOT bump `renderNonce`: the DOM is already what the user
 * sees, and repainting it would take their caret with it.
 */
export function composeSetDoc(raw: RawNode[]): void {
  prompts.doc = fromRawNodes(raw, prompts.doc);
  scheduleMatch();
}

/** The caret moved. `text` is the text node it sits in — the live-match query is
 *  the current line of it, up to the caret. */
export function composeSetCaret(caret: Caret | null, text: string): void {
  prompts.caret = caret;
  prompts.caretText = text;
  prompts.matchQuery = caret ? caretQuery(text, caret.offset) : '';
  scheduleMatch();
}

/** The box has placed the caret after a freshly inserted chip. */
export function clearPendingCaret(): void {
  prompts.pendingCaretCid = null;
}

/**
 * Insert a snippet as a chip, consuming the query line the user typed to find it.
 * The single insert path behind both triggers (clicking a match, and ↓-into-panel
 * then Enter).
 *
 * The chip carries the body; the box shows only the name and the variables. The
 * body's `{var}` tokens merge into the one global fill list by name, and resolve
 * at copy time.
 */
export function composeInsertSnippet(name: string, content: string): void {
  const cid = newCid();
  prompts.doc = insertChip(prompts.doc, prompts.caret ?? { node: 0, offset: 0 }, {
    cid,
    name,
    content,
  });
  prompts.matchQuery = ''; // the query line was consumed by the insert
  scheduleMatch(); // clears the now-stale suggestions
  prompts.pendingCaretCid = cid;
  prompts.renderNonce++;
}

/** `Use once`: this chip, this prompt, nothing written to the library. The escape
 *  hatch that makes "a chip is never editable in place" tolerable rather than a
 *  cage — tweak a prompt without polluting the library. */
export function composeUseOnce(cid: string, content: string): void {
  prompts.doc = replaceChipContent(prompts.doc, cid, content);
  prompts.renderNonce++;
}

/** The popup saved this chip under `name`. Same name → the file was updated and
 *  the chip just reflects it; a new name → a new file, and the chip retargets to
 *  the snippet it now actually is. One transform covers both, which is exactly why
 *  "Save as new" no longer needs a button of its own. */
export function composeSaveChip(cid: string, name: string, content: string): void {
  prompts.doc = retargetChip(prompts.doc, cid, name, content);
  prompts.renderNonce++;
}

/** The popup deleted this chip's snippet. The file is gone; the words stay, as
 *  plain typed text. Deleting a library entry must not silently mutilate the
 *  prompt someone is halfway through writing. */
export function composeDissolveChip(cid: string): void {
  prompts.doc = dissolveChip(prompts.doc, cid);
  prompts.renderNonce++;
}

/** One fill input changed. Variables are global by name, so this one value serves
 *  every occurrence — in the fill list under the box and in every chip's popup. */
export function setFill(name: string, value: string): void {
  prompts.fills[name] = value;
}

/** The Copy Prompt deliverable: the composed prompt (typed text + every chip's
 *  BODY) through the copy pipeline. */
export function copyOutput(): string {
  return copyText(flatten(prompts.doc), prompts.fills, prompts.asVars);
}

/** Set one variable's as-variable mode (contract §Copy output). Absent = ON, so
 *  an explicit `false` is how OFF is recorded. Session-only — never persisted to
 *  the snippet ([JC-9]). */
export function setAsVar(name: string, on: boolean): void {
  prompts.asVars[name] = on;
}

/** The effective hotkey map — stored overrides merged over the defaults, each
 *  normalized. Read inside a $derived to stay reactive to a rebind. */
export function resolvedHotkeys(): Record<HotkeyCommand, string> {
  return resolveHotkeys(prompts.hotkeyOverrides);
}

/** Rebind a command to `chord` and persist (app config, read-modify-write so
 *  unrelated fields survive). The caller rejects conflicts before calling —
 *  this just records and persists. A failed persist keeps the in-session
 *  binding and surfaces on `configError`; losing it on restart must not be
 *  silent. */
export async function setHotkey(command: HotkeyCommand, chord: string): Promise<void> {
  prompts.hotkeyOverrides = { ...prompts.hotkeyOverrides, [command]: chord };
  await persistHotkeys();
}

/** Reset a command to its default (drop the override) and persist. */
export async function resetHotkey(command: HotkeyCommand): Promise<void> {
  const next = { ...prompts.hotkeyOverrides };
  delete next[command];
  prompts.hotkeyOverrides = next;
  await persistHotkeys();
}

async function persistHotkeys(): Promise<void> {
  prompts.configError = null;
  try {
    const cfg = await getAppConfig();
    await setAppConfig({ ...cfg, hotkeys: prompts.hotkeyOverrides });
  } catch (e) {
    prompts.configError = `Couldn't save the shortcut: ${
      e instanceof Error ? e.message : String(e)
    }`;
  }
}

// ── snippet store ──────────────────────────────────────────────────────────────

/**
 * Save a snippet: `<name>.md` in the project folder. Same name updates, a new name
 * creates — the filename IS the identity, which is the whole "Save vs Save as new"
 * mechanism, collapsed into one button and disambiguated by the name field.
 */
export async function saveSnippet(project: string, name: string, content: string): Promise<Snippet> {
  const saved = await apiSaveSnippet(project, name, content);
  const i = prompts.snippets.findIndex((s) => s.name === saved.name);
  if (i >= 0) prompts.snippets[i] = saved;
  else prompts.snippets.push(saved);
  scheduleMatch(); // the library changed under the current query
  return saved;
}

/** Remove the file from the project. What happens to a chip pointing at it is the
 *  compose surface's business (`composeDissolveChip`) — the words stay. */
export async function deleteSnippet(project: string, name: string): Promise<void> {
  await apiDeleteSnippet(project, name);
  prompts.snippets = prompts.snippets.filter((s) => s.name !== name);
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
