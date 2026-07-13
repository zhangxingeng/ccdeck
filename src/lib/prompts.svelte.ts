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
  removeProject as apiRemoveProject,
  setActiveProject as apiSetActiveProject,
  matchSnippets,
  touchSnippet as apiTouchSnippet,
} from './api';
import {
  type Doc,
  type SpanLink,
  type Span,
  emptyDoc,
  applyEdit,
  insertSnippetOverRange,
  replaceSpan,
  linkRange,
  caretQuery,
  spanStarts,
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

/** Insert a snippet's RAW body as a linked span, REPLACING the query line the
 *  user typed to find it (line start → caret) — the single insert path behind
 *  both triggers, mouse click and ↓-into-panel + Enter (contract §S2/S3). The
 *  query was scaffolding; leaving it in front of the body is litter. {var}
 *  tokens land verbatim and merge into the unified fill list (variables resolve
 *  at copy time). */
export function composeInsertSnippet(snippet: Snippet): Doc {
  const link: SpanLink = { snippetId: snippet.id, title: snippet.title, scope: snippet.scope };
  const caret = prompts.caret;
  const lineStart = prompts.doc.text.lastIndexOf('\n', caret - 1) + 1;
  prompts.doc = insertSnippetOverRange(prompts.doc, lineStart, caret, snippet.body, link);
  const end = lineStart + snippet.body.length;
  setSelection(end, end);
  prompts.matchQuery = ''; // the query line was consumed by the insert
  scheduleMatch(); // clears the now-stale suggestions
  prompts.focusNonce++;
  return prompts.doc;
}

/** Replace one span's text + metadata (snippet-modal save refreshing the
 *  span's link metadata / provenance state — never the composed text). */
export function composeReplaceSpan(index: number, newText: string, span: Omit<Span, 'length'>): void {
  const start = spanStarts(prompts.doc)[index];
  prompts.doc = replaceSpan(prompts.doc, index, newText, span);
  setSelection(start + newText.length, start + newText.length);
  prompts.focusNonce++;
}

/** After save-selection-as-snippet: the saved selection becomes a linked span
 *  pointing at the new snippet (linked-modified when the saved body was edited
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
 *  (escapes resolved; per-variable XML dedup or substitute-in-place). */
export function copyOutput(): string {
  return copyText(prompts.doc.text, prompts.fills, prompts.asVars);
}

/** Set one variable's as-variable mode (contract §Copy output). Absent = ON, so
 *  an explicit `false` is how OFF is recorded. Session-only — never persisted to
 *  the snippet ([JC-9]). */
export function setAsVar(name: string, on: boolean): void {
  prompts.asVars[name] = on;
}

// ── snippet store ──────────────────────────────────────────────────────────────

/** Save (create or update) and sync the local list. Returns the stored snippet. */
export async function saveSnippet(input: SnippetInput): Promise<Snippet> {
  const saved = await apiSaveSnippet(input);
  const i = prompts.snippets.findIndex((p) => p.id === saved.id);
  if (i >= 0) prompts.snippets[i] = saved;
  else prompts.snippets.push(saved);
  scheduleMatch(); // the library changed under the current query
  return saved;
}

export async function deleteSnippet(id: string): Promise<void> {
  await apiDeleteSnippet(id);
  const i = prompts.snippets.findIndex((p) => p.id === id);
  if (i >= 0) prompts.snippets.splice(i, 1);
  scheduleMatch();
}
