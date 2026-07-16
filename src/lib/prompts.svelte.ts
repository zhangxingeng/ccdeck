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
import type { Snippet, Project } from './prompts/types';
import {
  listSnippets,
  saveSnippet as apiSaveSnippet,
  deleteSnippet as apiDeleteSnippet,
  listProjects,
  addProject as apiAddProject,
  setProjectColor as apiSetProjectColor,
  removeProject as apiRemoveProject,
  setActiveProject as apiSetActiveProject,
  matchSnippets,
  touchSnippet as apiTouchSnippet,
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

/** Light debounce so we don't hit the matcher on every literal keystroke. */
const DEBOUNCE_MS = 110;
/** Safety cap on one match run, not a UX feature.
 *
 *  The panel is the LIBRARY now, not a suggestion strip: at rest it lists every
 *  snippet in the active project, and typing filters that list *down*. So the
 *  cap has to be far above any real library — a cap that actually bites would
 *  make the panel quietly lie about what it contains ("this is everything")
 *  while hiding snippets. If a library ever exceeds it, the panel says so
 *  rather than truncating in silence. */
export const MATCH_LIMIT = 500;

export interface ResolvedHit {
  snippet: Snippet;
  score: number;
}

export const prompts = $state({
  // library — the snippets of the ACTIVE project (every *.md under its folder)
  snippets: [] as Snippet[],
  loadError: null as string | null,
  /** The project roster. A project is a name and a folder — nothing else. */
  projects: [] as Project[],
  /** Absolute path of the active project, persisted by the backend and restored
   *  on launch. `null` does NOT mean "global" — there is no global scope now, a
   *  snippet lives in the folder it sits in. It means **no project is
   *  configured yet**, which renders as the empty state that asks for a folder. */
  activeProjectPath: null as string | null,
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
  // live matching
  matchQuery: '',
  hits: [] as ResolvedHit[],
  matching: false,
});

let matchId = 0;
let debounceTimer: ReturnType<typeof setTimeout> | null = null;

// ── lifecycle ────────────────────────────────────────────────────────────────

/** Load the project roster + the active project's snippets, then run the first
 *  match so the panel is already populated when the view paints. Idempotent per
 *  session — re-entering refreshes the library but keeps the compose doc. */
export async function initPrompts(): Promise<void> {
  try {
    const { projects, active } = await listProjects();
    prompts.projects = projects;
    prompts.activeProjectPath = active;
    prompts.loadError = null;
  } catch (e) {
    prompts.loadError = e instanceof Error ? e.message : String(e);
    return; // no roster ⇒ no project ⇒ nothing to list or match
  }
  await refreshSnippets();
  await runMatch(); // at rest this is the whole library, recency-first
}

/** Stop timers when leaving the view (the doc itself is kept — see header). */
export function disposePrompts(): void {
  if (debounceTimer) clearTimeout(debounceTimer);
  debounceTimer = null;
  matchId++; // ignore any in-flight match
}

// ── projects ─────────────────────────────────────────────────────────────────

/** The active project's record, or null when none is configured yet. Reactive
 *  when read inside a $derived. */
export function activeProject(): Project | null {
  return prompts.projects.find((p) => p.path === prompts.activeProjectPath) ?? null;
}

/** Switch projects: persist the choice (the backend restores it on launch),
 *  then reload the library and the panel — a project IS its folder, so its
 *  snippets are a different set of files entirely. */
export async function setActiveProject(path: string): Promise<void> {
  prompts.activeProjectPath = path;
  try {
    await apiSetActiveProject(path);
  } catch (e) {
    // The in-session switch already happened and is what the user sees; only
    // restore-on-next-launch is at risk. Say so rather than silently reverting
    // the tab they just clicked.
    prompts.loadError = `Couldn't remember the active project: ${errText(e)}`;
  }
  await refreshSnippets();
  await runMatch();
}

/** Register a folder as a project and switch to it — you added it to work in it. */
export async function addProject(name: string, path: string): Promise<Project> {
  const saved = await upsertProject(name, path);
  await setActiveProject(saved.path);
  return saved;
}

/** Rename a project. The PATH is the identity, so a rename is just re-registering
 *  the same folder under a new name — and unlike `addProject` it must not steal
 *  the active tab, since renaming a folder you are not working in is not a
 *  request to switch to it. */
export async function renameProject(name: string, path: string): Promise<Project> {
  return upsertProject(name, path);
}

async function upsertProject(name: string, path: string): Promise<Project> {
  const saved = await apiAddProject(name, path);
  const i = prompts.projects.findIndex((p) => p.path === saved.path);
  if (i >= 0) prompts.projects[i] = saved;
  else prompts.projects.push(saved);
  return saved;
}

/** Set (or clear, with `color: null`) a project's color — round 2's restore of
 *  the round-1 cut (see `prompts/types.ts`). Like `renameProject`, this must
 *  not touch the active tab: recoloring a project you are not working in is
 *  not a request to switch to it. */
export async function setProjectColor(path: string, color: string | null): Promise<Project> {
  const saved = await apiSetProjectColor(path, color);
  const i = prompts.projects.findIndex((p) => p.path === saved.path);
  if (i >= 0) prompts.projects[i] = saved;
  return saved;
}

/** Forget a project. **Never deletes files** — the user's prompts are their own;
 *  this drops the path from the roster and nothing else.
 *
 *  Re-reads the roster rather than re-deriving what happened: removing the ACTIVE
 *  project has to re-point `active` at something, and the backend already owns
 *  that rule. Duplicating it here would put one rule on both sides of the seam,
 *  where the two copies can only drift apart. */
export async function removeProject(path: string): Promise<void> {
  await apiRemoveProject(path);
  const { projects, active } = await listProjects();
  prompts.projects = projects;
  prompts.activeProjectPath = active;
  await refreshSnippets();
  await runMatch();
}

// ── library ──────────────────────────────────────────────────────────────────

/** Re-read every `*.md` under the active project's folder. */
export async function refreshSnippets(): Promise<void> {
  const project = prompts.activeProjectPath;
  if (project === null) {
    prompts.snippets = [];
    return;
  }
  try {
    prompts.snippets = await listSnippets(project);
    prompts.loadError = null;
  } catch (e) {
    prompts.loadError = e instanceof Error ? e.message : String(e);
  }
}

/** Record that a snippet was used. This is the ONLY input to the at-rest sort,
 *  and it is app-local (never a sidecar in the project folder) — a `last_used`
 *  write into a git-tracked prompt file would dirty the tree on every insert. */
export async function touchSnippet(name: string): Promise<void> {
  const project = prompts.activeProjectPath;
  if (project === null) return;
  try {
    await apiTouchSnippet(project, name);
  } catch {
    // Usage tracking only orders the at-rest list. Losing one touch costs a
    // slightly stale sort, never a lost snippet — not worth interrupting an
    // insert the user already got.
  }
}

// ── live matching ────────────────────────────────────────────────────────────

function scheduleMatch(): void {
  if (debounceTimer) clearTimeout(debounceTimer);
  debounceTimer = setTimeout(runMatch, DEBOUNCE_MS);
}

/** The list FILTERS DOWN, it does not build up.
 *
 *  An empty query is not "no results" — it is "no filter", and the answer to it
 *  is the whole library, most-recently-used first (the backend owns that sort;
 *  it holds the usage map). Typing narrows that list by match score. The old
 *  behavior bailed out on an empty query in BOTH layers, so the user was shown
 *  an empty panel and had to type to make anything appear at all — backwards,
 *  and the single thing the founder hit every day.
 *
 *  No "recent or relevant?" toggle exists because the question answers itself:
 *  with no query there is no score to sort by, so recency is the only meaningful
 *  order; with a query, the score is. */
async function runMatch(): Promise<void> {
  if (debounceTimer) {
    clearTimeout(debounceTimer);
    debounceTimer = null;
  }
  const id = ++matchId;
  const project = prompts.activeProjectPath;
  if (project === null) {
    prompts.hits = [];
    prompts.matching = false;
    return;
  }
  prompts.matching = true;
  try {
    const hits = await matchSnippets(project, prompts.matchQuery, MATCH_LIMIT);
    if (id !== matchId) return; // superseded
    const byName = new Map(prompts.snippets.map((s) => [s.name, s]));
    prompts.hits = hits.flatMap((h) => {
      const snippet = byName.get(h.name);
      return snippet ? [{ snippet, score: h.score }] : [];
    });
    prompts.matching = false;
  } catch (e) {
    if (id !== matchId) return;
    prompts.matching = false;
    prompts.hits = [];
    // The one failure we expect here is a transient backend/IPC error — Tauri's
    // Result<_, String> rejects with a *string*. Matching is a read path, not a
    // save path, so that degrades quietly (store errors surface on save, which
    // is guarded).
    if (typeof e === 'string') return;
    // Anything else is a programming error wearing a "No matching snippets."
    // costume — a user reads that as "nothing matched," not "matching is
    // broken." Don't let it hide: log and re-throw so it surfaces as a failure.
    console.error('Prompt match failed unexpectedly:', e);
    throw e;
  }
}

function errText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
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
