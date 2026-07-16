/**
 * The compose-box document model: a flat list of nodes, each either free-typed
 * text or an inserted snippet (a "chip"). Pure data + pure transforms — no DOM,
 * no Svelte. The Doc is the single source of truth; the contenteditable is only
 * an input device.
 *
 * ── Why this is a node list and not the old span model ───────────────────────
 *
 * Until v0.13 a Doc was one `text` string plus `spans[]` annotating ranges of it,
 * under the invariant `sum(span.length) === text.length` — spans TILE the text.
 * That invariant is precisely what had to die.
 *
 * A chip RENDERS as its name and the variables it contains, but CONTRIBUTES its
 * whole body to the copied prompt. Rendered ≠ contributed. The tiling invariant
 * asserts they are equal, so no amount of guarding could have grown a chip on top
 * of it: a <textarea> can only render the characters it contains, which is exactly
 * why a snippet's body used to sit in the box as editable text — and exactly why
 * editing it in place was possible at all.
 *
 * That inline edit was the whole defect. It silently diverged the composed text
 * from the stored snippet and flipped the span into a third provenance state,
 * `linked-modified`, which persisted nothing: two places to edit one thing, with
 * different consequences and no signal about which was which.
 *
 * So a chip is an ATOM. It cannot be split, clipped, or partially selected —
 * which is why the entire clip-and-demote algebra of the old model (applyEdit's
 * transition table, linkRange, replaceSpan, spanStarts, diffTexts) is deleted
 * rather than ported, and why `linked-modified` goes with it. A chip that cannot
 * be edited in place cannot be modified in place. Editing happens in the popup,
 * always, and nowhere else.
 */
import { parseVariables } from './variables';

/** Free-typed text. Freely editable — the box is still a text box. */
export interface TextNode {
  kind: 'text';
  text: string;
}

/**
 * An inserted snippet. Atomic: never inline-editable, never split.
 *
 * `content` is the snippet's body, carried ON the chip rather than read through
 * to the library by name. That is deliberate, and it is what makes `Use once`
 * possible at all: a chip may legitimately differ from the file of the same name
 * because the user tweaked it for this one prompt without polluting their
 * library. Carrying the body also makes a composed draft durable — the library
 * changing, or the snippet being deleted outright, cannot reach in and mutate or
 * gut a prompt someone is halfway through writing.
 *
 * `cid` identifies this chip INSTANCE, not the snippet: the same snippet can be
 * inserted twice, and `Use once` on one copy must not touch the other.
 *
 * `dirty` marks a chip that has diverged from the library file its name still
 * points at — set by a session-only `Save` (round 2's renamed `Use once`),
 * cleared by `Update` (the disk-write path) and by a fresh insert. Absent or
 * `false` means "still identical to the saved file." Never written to disk —
 * it describes a draft's relationship to the file, not the file itself.
 */
export interface ChipNode {
  kind: 'chip';
  cid: string;
  name: string;
  content: string;
  dirty?: boolean;
}

export type Node = TextNode | ChipNode;

export interface Doc {
  nodes: Node[];
}

/** A fresh chip-instance id. Only ever compared for equality — its shape is not
 *  a contract, and it is never persisted (a chip lives in a draft, not on disk). */
export function newCid(): string {
  return crypto.randomUUID();
}

export function emptyDoc(): Doc {
  return { nodes: [] };
}

/** A doc that is all free-typed text. */
export function docFromText(text: string): Doc {
  return normalize({ nodes: text ? [{ kind: 'text', text }] : [] });
}

/**
 * Canonical form: no empty text nodes, no two adjacent text nodes, no duplicate
 * chip ids.
 *
 * The duplicate-id sweep is a correctness guard, not tidiness. Copy/pasting a chip
 * inside the box hands us the same `cid` twice; if both survived, `Use once` on one
 * instance would silently rewrite the other — the very silent-divergence class this
 * redesign exists to kill, arriving through the clipboard. The later copy keeps the
 * body and gets a fresh identity, which is what a copy IS.
 */
export function normalize(doc: Doc): Doc {
  const nodes: Node[] = [];
  const seen = new Set<string>();

  for (const n of doc.nodes) {
    if (n.kind === 'text') {
      if (!n.text) continue;
      const prev = nodes[nodes.length - 1];
      if (prev?.kind === 'text') prev.text += n.text;
      else nodes.push({ ...n });
      continue;
    }
    const cid = seen.has(n.cid) ? newCid() : n.cid;
    seen.add(cid);
    nodes.push({ ...n, cid });
  }
  return { nodes };
}

/**
 * The composed prompt's raw text: typed text and each chip's CONTENT, in order.
 *
 * This is the seam where rendered and contributed diverge — the box shows a chip
 * as a short label, and this returns its whole body. Everything downstream (the
 * variable fill list, Copy Prompt) reads this, never the rendered form.
 */
export function flatten(doc: Doc): string {
  return doc.nodes.map((n) => (n.kind === 'text' ? n.text : n.content)).join('');
}

/** The chip with this id, or undefined. */
export function chipAt(doc: Doc, cid: string): ChipNode | undefined {
  const n = doc.nodes.find((node) => node.kind === 'chip' && node.cid === cid);
  return n?.kind === 'chip' ? n : undefined;
}

/** The variable names a chip's body uses — the chip's label shows these, and its
 *  popup fills exactly these. Derived from the body, never stored: the body is the
 *  single source of truth, and a stored copy could only drift from it. */
export function chipVariables(chip: ChipNode): string[] {
  return parseVariables(chip.content).map((v) => v.name);
}

/**
 * Where a caret sits.
 *
 * `node` indexes doc.nodes. If that node is TEXT, `offset` is the character
 * offset within it. If it is a CHIP — or `node` is past the end — the caret is
 * an insertion point *before* that index, with no text to split. Both cases are
 * real: a caret genuinely can sit between two adjacent chips, where the model
 * holds no text node at all.
 *
 * Chips are atomic, so a caret is never *inside* one.
 */
export interface Caret {
  node: number;
  offset: number;
}

/**
 * Insert a snippet as a chip, replacing the query line the user typed to find it
 * (from the start of the caret's line up to the caret).
 *
 * The query was scaffolding — the user typed "senior review" only to summon the
 * snippet, so leaving it sitting in front of the inserted chip is litter. This is
 * the single insert path behind both triggers: clicking a match, and ↓-into-panel
 * then Enter.
 */
export function insertChip(doc: Doc, caret: Caret, chip: Omit<ChipNode, 'kind'>): Doc {
  const at = Math.max(0, Math.min(caret.node, doc.nodes.length));
  const node = doc.nodes[at];
  const chipNode: ChipNode = { kind: 'chip', ...chip };

  // The caret sits between nodes (before a chip, or past the end): there is no
  // query text to consume, so just land the chip there.
  if (!node || node.kind !== 'text') {
    return normalize({
      nodes: [...doc.nodes.slice(0, at), chipNode, ...doc.nodes.slice(at)],
    });
  }

  const offset = Math.max(0, Math.min(caret.offset, node.text.length));
  const lineStart = node.text.lastIndexOf('\n', offset - 1) + 1;

  return normalize({
    nodes: [
      ...doc.nodes.slice(0, at),
      { kind: 'text', text: node.text.slice(0, lineStart) },
      chipNode,
      { kind: 'text', text: node.text.slice(offset) },
      ...doc.nodes.slice(at + 1),
    ],
  });
}

/** Replace a chip's body — the popup's session-only `Save` (this prompt only,
 *  nothing written to the library). Marks the chip `dirty` by default: this is
 *  precisely the transform that diverges a chip from its saved file, so the one
 *  real caller wants `dirty: true` — a caller with a different need can pass
 *  `false` explicitly rather than the function guessing wrong for everyone. */
export function replaceChipContent(doc: Doc, cid: string, content: string, dirty = true): Doc {
  return normalize({
    nodes: doc.nodes.map((n) => (n.kind === 'chip' && n.cid === cid ? { ...n, content, dirty } : n)),
  });
}

/** Retarget a chip at a different snippet — the popup's Update, which writes
 *  the file (same name updates it, a new name creates one). The chip now
 *  reflects the snippet it actually is, and — having just been written to disk
 *  — is no longer diverged from it: Update always clears `dirty`. */
export function retargetChip(doc: Doc, cid: string, name: string, content: string): Doc {
  return normalize({
    nodes: doc.nodes.map((n) =>
      n.kind === 'chip' && n.cid === cid ? { ...n, name, content, dirty: false } : n
    ),
  });
}

/**
 * Dissolve a chip into plain typed text, keeping its body.
 *
 * This is what the popup's `Delete` does to the chip it was opened from: the file
 * is removed from the library, and the words stay in the prompt. Deleting a library
 * entry must not silently mutilate the prompt someone is in the middle of writing —
 * the link is gone; the writing is theirs. It also leaves no chip pointing at a
 * snippet that no longer exists.
 *
 * (Removing a chip *from the prompt* needs no transform: it is an atom, so
 * Backspace already does exactly that.)
 */
export function dissolveChip(doc: Doc, cid: string): Doc {
  return normalize({
    nodes: doc.nodes.map(
      (n): Node => (n.kind === 'chip' && n.cid === cid ? { kind: 'text', text: n.content } : n)
    ),
  });
}

// ── the contenteditable seam ─────────────────────────────────────────────────
// The box renders these and reads them back. Keeping both directions as pure
// functions over plain data — rather than letting the component walk the DOM
// straight into state — is what makes the round-trip testable:
//
//     doc → toRenderNodes → (DOM) → fromRawNodes → doc
//
// must be the identity. If it is not, a user's prompt silently corrupts into
// something that still LOOKS plausible in the box and copies out wrong.

/** What the box renders for one node. A chip shows its name and its variables —
 *  never its body. The founder's reasoning: "I rarely read it. If I want to read
 *  it, it means I want to edit it. And if I want to edit it, I'd click into it."
 *  Body text in the box is clutter serving no reader. */
export type RenderNode =
  | { kind: 'text'; text: string }
  | { kind: 'chip'; cid: string; name: string; vars: string[]; dirty: boolean };

export function toRenderNodes(doc: Doc): RenderNode[] {
  return doc.nodes.map((n) =>
    n.kind === 'text'
      ? { kind: 'text' as const, text: n.text }
      : {
          kind: 'chip' as const,
          cid: n.cid,
          name: n.name,
          vars: chipVariables(n),
          dirty: n.dirty === true,
        }
  );
}

/** One child of the contenteditable, read back off the DOM: either a chip element
 *  (identified by its `data-cid`) or a run of text. */
export interface RawNode {
  /** The element's data-cid, or null for a plain text run. */
  cid: string | null;
  text: string;
}

/** Zero-width space. The renderer pads around chips with one so the browser always
 *  has somewhere to put a caret — a chip at the very start or end of the box, or two
 *  adjacent chips, otherwise leave nowhere to click. It is display scaffolding, never
 *  content: stripped on the way back in, so it can never reach a copied prompt. */
export const ZWSP = '​';

/**
 * Rebuild the Doc from what the DOM now holds, carrying each surviving chip's body
 * across by `cid`.
 *
 * Reading the DOM back wholesale — rather than intercepting each edit and patching
 * the model — is what makes this robust. Typing, paste, cut, drag, undo, IME
 * composition and every other inputType all arrive here as "the box now contains
 * this", with no per-event transition table to get wrong. The browser gives chips
 * atomicity for free (they are contenteditable="false"), so a chip either survives
 * an edit intact or is gone entirely.
 *
 * A chip whose `cid` is unknown to `prev` is DROPPED, not coerced into text: its body
 * lives only in the model, so there is nothing faithful to put in its place, and
 * rendering its label instead would silently substitute the words "code_review" for
 * the code-review prompt itself. (In practice this only arises if a chip element is
 * pasted in from outside the box.)
 */
export function fromRawNodes(raw: RawNode[], prev: Doc): Doc {
  const byCid = new Map(
    prev.nodes.filter((n): n is ChipNode => n.kind === 'chip').map((n) => [n.cid, n])
  );

  const nodes: Node[] = [];
  for (const r of raw) {
    if (r.cid === null) {
      nodes.push({ kind: 'text', text: r.text.replaceAll(ZWSP, '') });
      continue;
    }
    const chip = byCid.get(r.cid);
    if (chip) nodes.push({ ...chip });
    // Unknown cid → dropped (see doc comment).
  }
  // normalize() re-issues a duplicate cid — that is how a copy/pasted chip becomes
  // its own instance rather than a shared one.
  return normalize({ nodes });
}

/**
 * The live-match query for a caret: what you are typing right now — the current line
 * of the caret's text node, up to the caret. Tail-capped so one very long line cannot
 * swamp the matcher.
 *
 * A chip ends the query by construction (it is a different node), which is the
 * behavior we want: having just inserted a snippet, you are not still querying for
 * one.
 */
export function caretQuery(text: string, offset: number, cap = 120): string {
  const upto = text.slice(0, Math.max(0, Math.min(offset, text.length)));
  const lineStart = upto.lastIndexOf('\n') + 1;
  const line = upto.slice(lineStart).trim();
  return line.length > cap ? line.slice(line.length - cap) : line;
}
