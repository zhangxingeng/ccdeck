<script lang="ts">
  /**
   * The compose surface. A contenteditable box holding two kinds of thing: free
   * text, which you edit exactly as you would in any text box, and chips, which
   * you cannot edit here at all.
   *
   * A chip is an inserted snippet. It shows its NAME and the variables it
   * contains — never its body. The founder's reasoning: "I rarely read it. If I
   * want to read it, it means I want to edit it. And if I want to edit it, I'd
   * click into it." Body text in the box is clutter serving no reader.
   *
   * Clicking a chip opens the popup. Always, no exceptions — that is the one rule
   * this surface is built on, and its value is that it makes the interaction
   * predictable: the user never has to ask "do I edit this here, or click
   * something?" The answer is always "click the chip."
   *
   * The mechanism is `contenteditable="false"` on the chip element, which is why
   * the rule holds structurally rather than by guarding. The browser itself
   * refuses to put a caret inside a chip, treats it as one atom for arrow keys and
   * selection, and deletes it whole on Backspace. There is no inline edit to
   * intercept, because there is no inline edit.
   *
   * ── Why the DOM is read back wholesale ──────────────────────────────────────
   * Every user edit lands in `syncFromDom`: we read what the box now contains and
   * rebuild the Doc from it, carrying each surviving chip's body across by `cid`.
   * Typing, paste, cut, drag, undo and IME composition all arrive the same way, so
   * there is no per-inputType transition table to get wrong. The reverse direction
   * (`render`) runs ONLY for changes made from outside the box — an insert, a popup
   * save — because repainting under the user's own keystrokes would destroy their
   * caret.
   */
  import { onMount, tick } from 'svelte';
  import { prompts, composeSetDoc, composeSetCaret, clearPendingCaret } from '$lib/prompts.svelte';
  import {
    toRenderNodes,
    chipAt,
    flatten,
    ZWSP,
    type Caret,
    type RawNode,
    type RenderNode,
  } from '$lib/compose/doc';
  import VariableFillList from './VariableFillList.svelte';

  interface Props {
    /** Clicking a chip — the one and only way to edit a snippet. */
    onOpenChip: (cid: string) => void;
    /** Save typed text as a new snippet: the selection if there is one, else the
     *  whole prompt. Opens the same popup, prefilled — one of its two entrances. */
    onSaveAsSnippet: (text: string) => void;
    /** Copy Prompt — the parent owns the clipboard call + toast. */
    onCopy: () => void;
    /** ↓ at the very end steps into the match panel; returns whether the panel had
     *  a hit to land on, so ↓ stays a caret move when the panel is empty. */
    onStepIntoPanel: () => boolean;
  }

  let { onOpenChip, onSaveAsSnippet, onCopy, onStepIntoPanel }: Props = $props();

  let boxEl: HTMLDivElement | undefined = $state(undefined);
  /** Bumped on every selection change so the save label re-reads the selection. */
  let selectionNonce = $state(0);

  const hasContent = $derived(prompts.doc.nodes.length > 0);

  /**
   * The selected text, with any chip inside it contributing its BODY.
   *
   * The DOM carries a chip's label, so a naive `getSelection().toString()` would
   * save the literal words "rust/code_review" as a snippet instead of the
   * code-review prompt itself — silently storing the name in place of the thing.
   * Chips are atomic to the browser's selection model, so each is either wholly in
   * the range or wholly out; there is no half-chip to reason about.
   */
  function selectionText(): string {
    const sel = window.getSelection();
    if (!sel || sel.isCollapsed || sel.rangeCount === 0 || !boxEl) return '';
    const range = sel.getRangeAt(0);
    if (!boxEl.contains(range.commonAncestorContainer)) return '';

    let out = '';
    const walk = (node: globalThis.Node): void => {
      if (node.nodeType === globalThis.Node.TEXT_NODE) {
        out += node.nodeValue ?? '';
        return;
      }
      if (!(node instanceof HTMLElement)) return;
      const cid = node.dataset.cid;
      if (cid) {
        out += chipAt(prompts.doc, cid)?.content ?? '';
        return;
      }
      for (const child of Array.from(node.childNodes)) walk(child);
    };
    for (const child of Array.from(range.cloneContents().childNodes)) walk(child);
    return out.replaceAll(ZWSP, '');
  }

  /** Save the selection if there is one, else the whole prompt. The label says
   *  which, so the button never silently saves more than the user meant. */
  const savingSelection = $derived.by(() => {
    void selectionNonce;
    void prompts.doc;
    return selectionText() !== '';
  });

  function handleSaveAs(): void {
    onSaveAsSnippet(selectionText() || flatten(prompts.doc));
  }

  /** The Mod+S hotkey is owned by the view, but the selection it acts on lives
   *  here. Exporting the one implementation is what keeps the hotkey and the
   *  button selection-aware *together* — a second copy on the view could only
   *  drift, and a drift there silently saves more than the user meant. */
  export function saveAs(): void {
    handleSaveAs();
  }

  /** Esc out of the match panel puts the caret back in the box. */
  export function focus(): void {
    boxEl?.focus();
  }

  // ── render: model → DOM (external changes only) ─────────────────────────────

  /** One chip element. Built programmatically because Svelte must not own the
   *  children of a contenteditable — it would repaint them under the caret. They
   *  therefore carry no scoping class, and the styles below reach them with
   *  :global from the scoped box. */
  function chipElement(n: Extract<RenderNode, { kind: 'chip' }>): HTMLElement {
    const el = document.createElement('span');
    el.className = 'chip';
    el.contentEditable = 'false';
    el.dataset.cid = n.cid;
    el.setAttribute('role', 'button');
    el.setAttribute('tabindex', '0');
    el.title = `Edit ${n.name}`;

    const name = document.createElement('span');
    name.className = 'chip__name';
    name.textContent = n.name;
    el.append(name);

    for (const v of n.vars) {
      const badge = document.createElement('span');
      badge.className = 'chip__var';
      badge.textContent = v;
      el.append(badge);
    }
    return el;
  }

  /**
   * Paint the box from the Doc.
   *
   * A text node is guaranteed before the first chip, after the last, and between
   * any two adjacent chips — filled with a zero-width space where the model has no
   * text. Without it the browser has nowhere to put a caret, and a chip at the very
   * start or end of the box (or a pair of neighbours) becomes impossible to type
   * around. The ZWSP is display scaffolding: `fromRawNodes` strips it, so it can
   * never reach a copied prompt.
   */
  function render(): void {
    if (!boxEl) return;
    const rendered = toRenderNodes(prompts.doc);
    if (rendered.length === 0) {
      boxEl.replaceChildren(); // truly empty, so :empty shows the placeholder
      return;
    }

    const children: globalThis.Node[] = [];
    let needsFiller = true; // a leading chip needs a text node before it
    for (const n of rendered) {
      if (n.kind === 'text') {
        children.push(document.createTextNode(n.text));
        needsFiller = false;
        continue;
      }
      if (needsFiller) children.push(document.createTextNode(ZWSP));
      children.push(chipElement(n));
      needsFiller = true;
    }
    if (needsFiller) children.push(document.createTextNode(ZWSP));

    boxEl.replaceChildren(...children);
  }

  /** Repaint only when the doc changed from OUTSIDE the box (an insert, a popup
   *  save, a delete). `renderNonce` is bumped by exactly those paths — reacting to
   *  `doc` itself would repaint on every keystroke and take the caret with it. */
  $effect(() => {
    void prompts.renderNonce;
    render();
    void tick().then(placeCaretAfterInsert);
  });

  /** After an external insert the caret belongs just after the new chip, so the
   *  user's next keystroke continues the sentence instead of landing wherever the
   *  browser guessed. */
  function placeCaretAfterInsert(): void {
    const cid = prompts.pendingCaretCid;
    if (!cid || !boxEl) return;
    const chip = boxEl.querySelector(`[data-cid="${CSS.escape(cid)}"]`);
    const after = chip?.nextSibling;
    if (after && after.nodeType === globalThis.Node.TEXT_NODE) {
      const text = after.nodeValue ?? '';
      setCaret(after, text.startsWith(ZWSP) ? 1 : 0); // land past the filler
    }
    clearPendingCaret();
  }

  function setCaret(node: globalThis.Node, offset: number): void {
    const range = document.createRange();
    range.setStart(node, offset);
    range.collapse(true);
    const sel = window.getSelection();
    sel?.removeAllRanges();
    sel?.addRange(range);
    boxEl?.focus();
  }

  // ── read back: DOM → model (every user edit) ────────────────────────────────

  /** Flatten the box's children into raw nodes. Chips are recognized by their
   *  `data-cid`; their rendered text is ignored, because the body lives in the
   *  model keyed by that cid. */
  function readRawNodes(): RawNode[] {
    const raw: RawNode[] = [];
    if (!boxEl) return raw;

    const collect = (node: ChildNode, isLastChild: boolean): void => {
      if (node.nodeType === globalThis.Node.TEXT_NODE) {
        raw.push({ cid: null, text: node.nodeValue ?? '' });
        return;
      }
      if (!(node instanceof HTMLElement)) return;

      const cid = node.dataset.cid;
      if (cid) {
        raw.push({ cid, text: node.textContent ?? '' });
        return;
      }
      if (node.tagName === 'BR') {
        // Browsers append a filler <br> to keep the last line clickable. It is not
        // a newline the user typed, and counting it would grow the prompt by a
        // blank line on every render.
        if (!isLastChild) raw.push({ cid: null, text: '\n' });
        return;
      }
      // A wrapper we did not create. Enter and paste are both intercepted below so
      // this is rare — but recurse rather than take textContent, or a chip caught
      // inside it would lose its cid and take the user's snippet down with it.
      const kids = Array.from(node.childNodes);
      kids.forEach((k, i) => collect(k, i === kids.length - 1));
    };

    const children = Array.from(boxEl.childNodes);
    children.forEach((c, i) => collect(c, i === children.length - 1));
    return raw;
  }

  /**
   * Where the caret is in MODEL terms, plus the text of the node it sits in (the
   * live-match query reads that).
   *
   * A caret in a pure-filler text node maps to no model text node, so it reports as
   * an insertion point before the next model node — which is exactly right: the gap
   * between two adjacent chips is a real place to stand, and a real place to insert.
   */
  function caretFromDom(): { caret: Caret; text: string } | null {
    const sel = window.getSelection();
    if (!sel?.isCollapsed || !boxEl || !sel.anchorNode) return null;
    if (!boxEl.contains(sel.anchorNode)) return null;

    const anchor = sel.anchorNode;
    const children = Array.from(boxEl.childNodes);

    // The caret can resolve onto the box itself, with offset = a child index.
    if (anchor === boxEl) {
      const before = children.slice(0, sel.anchorOffset).filter(isModelNode).length;
      return { caret: { node: before, offset: 0 }, text: '' };
    }

    let index = 0;
    for (const child of children) {
      if (child.nodeType === globalThis.Node.TEXT_NODE) {
        const rawText = child.nodeValue ?? '';
        const stripped = rawText.replaceAll(ZWSP, '');
        if (child === anchor) {
          const offset = rawText.slice(0, sel.anchorOffset).replaceAll(ZWSP, '').length;
          return { caret: { node: index, offset }, text: stripped };
        }
        if (stripped) index++; // a pure filler is not a model node
        continue;
      }
      if (child instanceof HTMLElement && child.dataset.cid) {
        if (child === anchor || child.contains(anchor)) {
          return { caret: { node: index, offset: 0 }, text: '' };
        }
        index++;
      }
    }
    return null;
  }

  function isModelNode(node: ChildNode): boolean {
    if (node.nodeType === globalThis.Node.TEXT_NODE) {
      return (node.nodeValue ?? '').replaceAll(ZWSP, '') !== '';
    }
    return node instanceof HTMLElement && !!node.dataset.cid;
  }

  /** One user edit: rebuild the model from the box, then re-read the caret. This
   *  never repaints — the DOM is already what the user sees, and repainting it
   *  would take their caret with it. */
  function syncFromDom(): void {
    composeSetDoc(readRawNodes());
    syncCaret();
  }

  function syncCaret(): void {
    const at = caretFromDom();
    composeSetCaret(at?.caret ?? null, at?.text ?? '');
  }

  // ── input handling ──────────────────────────────────────────────────────────

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Enter') {
      // Own the newline. Left to itself the browser splits the box into <div>s (or
      // drops in a <br>), and the model would have to reverse-engineer block
      // structure back into text. A literal \n keeps the box one flat run.
      e.preventDefault();
      document.execCommand('insertText', false, '\n');
      return;
    }
    if (e.key === 'ArrowDown' && atEnd() && onStepIntoPanel()) {
      // ↓ is natively inert only at the very end of the text — the one position
      // where repurposing it to step into the match panel cannot steal a caret
      // move. onStepIntoPanel returns false when the panel is empty, so ↓ then
      // falls through to its default no-op.
      e.preventDefault();
    }
  }

  /** True when the caret sits at the very end of the box's content. */
  function atEnd(): boolean {
    const sel = window.getSelection();
    if (!sel?.isCollapsed || !boxEl || !sel.anchorNode) return false;
    const range = document.createRange();
    range.selectNodeContents(boxEl);
    range.setStart(sel.anchorNode, sel.anchorOffset);
    // Only ZWSP scaffolding may lie between the caret and the end.
    return range.toString().replaceAll(ZWSP, '') === '';
  }

  function handlePaste(e: ClipboardEvent): void {
    // Plain text only. Pasted markup would arrive as elements we did not create,
    // and a chip pasted from another app would carry a data-cid we have no body
    // for. execCommand keeps the browser's own undo stack intact.
    e.preventDefault();
    const text = e.clipboardData?.getData('text/plain') ?? '';
    if (text) document.execCommand('insertText', false, text);
  }

  /** A chip is a button that happens to live inside a text box. */
  function chipCidFrom(target: EventTarget | null): string | undefined {
    const el = target instanceof HTMLElement ? target.closest('[data-cid]') : null;
    return el instanceof HTMLElement ? el.dataset.cid : undefined;
  }

  function handleClick(e: MouseEvent): void {
    const cid = chipCidFrom(e.target);
    if (!cid) return;
    e.preventDefault();
    onOpenChip(cid);
  }

  function handleBoxKeydown(e: KeyboardEvent): void {
    // Enter / Space on a focused chip opens the popup, and must not also type.
    if (e.key === 'Enter' || e.key === ' ') {
      const cid = chipCidFrom(e.target);
      if (cid) {
        e.preventDefault();
        onOpenChip(cid);
        return;
      }
    }
    handleKeydown(e);
  }

  onMount(() => {
    // selectionchange is the only reliable way to track caret moves (arrows,
    // clicks, shift-selects) inside a contenteditable.
    function onSelectionChange(): void {
      const anchor = document.getSelection()?.anchorNode ?? null;
      if (!boxEl || !anchor || !boxEl.contains(anchor)) return;
      syncCaret();
      selectionNonce++; // the save-as label depends on whether a selection exists
    }
    document.addEventListener('selectionchange', onSelectionChange);
    return () => document.removeEventListener('selectionchange', onSelectionChange);
  });
</script>

<div class="compose">
  <div class="compose__stack">
    <div
      bind:this={boxEl}
      class="compose__box"
      contenteditable="true"
      role="textbox"
      tabindex="0"
      aria-multiline="true"
      aria-label="Prompt compose box"
      data-placeholder="Compose your prompt…"
      spellcheck="false"
      oninput={syncFromDom}
      onkeydown={handleBoxKeydown}
      onpaste={handlePaste}
      onclick={handleClick}
    ></div>

    <div class="compose__actions">
      {#if hasContent}
        <!-- Selection-aware: the label names what will actually be saved, so the
             button never silently stores more than the user meant. Swallows
             mousedown so clicking it cannot collapse the selection it acts on. -->
        <button
          type="button"
          class="btn btn--ghost btn--sm"
          onmousedown={(e) => e.preventDefault()}
          onclick={handleSaveAs}
          title="Save as a reusable library snippet"
        >
          {savingSelection ? 'Save selection as snippet' : 'Save as snippet'}
        </button>
        <button type="button" class="btn btn--primary btn--sm" onclick={onCopy}>Copy prompt</button>
      {/if}
    </div>
  </div>

  <VariableFillList />
</div>

<style>
  .compose {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    flex: 1;
    min-width: 0;
  }

  .compose__stack {
    position: relative;
    flex: 1;
    min-height: 16rem;
    display: flex;
  }

  .compose__box {
    flex: 1;
    font-family: var(--font-mono);
    font-size: 0.82rem;
    line-height: 1.9; /* room for a chip to sit on a line without crowding it */
    white-space: pre-wrap;
    overflow-wrap: break-word;
    padding: 0.8rem 0.9rem 3rem;
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    background: var(--bg-card);
    color: var(--text);
    box-sizing: border-box;
    overflow-y: auto;
  }
  .compose__box:focus {
    outline: none;
    border-color: color-mix(in srgb, var(--accent-snippet) 55%, var(--border));
  }
  .compose__box:empty::before {
    content: attr(data-placeholder);
    color: var(--text-faint);
    pointer-events: none;
  }
  /* The text inside the box is not Svelte-owned (chips are built by hand), so the
     selection style has to be reached globally — scoped to the box. */
  .compose__box :global(::selection) {
    background: var(--highlight);
    color: var(--highlight-foreground);
  }

  /* Chips are created programmatically (Svelte must not own the children of a
     contenteditable), so they carry no scoping class and are reached with :global
     from the scoped box. */
  .compose__box :global(.chip) {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    margin: 0 0.1rem;
    padding: 0.05rem 0.5rem;
    border-radius: 1rem;
    border: 1px solid color-mix(in srgb, var(--accent-snippet) 45%, var(--border));
    background: color-mix(in srgb, var(--accent-snippet) 12%, transparent);
    font-size: 0.74rem;
    line-height: 1.5;
    cursor: pointer;
    user-select: none;
    white-space: nowrap;
  }
  .compose__box :global(.chip:hover) {
    background: color-mix(in srgb, var(--accent-snippet) 22%, transparent);
  }
  .compose__box :global(.chip:focus-visible) {
    outline: 2px solid var(--accent-snippet);
    outline-offset: 1px;
  }
  .compose__box :global(.chip__name) {
    font-weight: 600;
    color: color-mix(in srgb, var(--accent-snippet) 85%, var(--text));
  }
  /* The variables the chip's body contains — the only thing shown besides the
     name. Never the body. */
  .compose__box :global(.chip__var) {
    font-size: 0.66rem;
    padding: 0 0.3rem;
    border-radius: 0.7rem;
    background: color-mix(in srgb, var(--accent-template) 18%, transparent);
    color: color-mix(in srgb, var(--accent-template) 80%, var(--text));
  }

  .compose__actions {
    position: absolute;
    left: 0.75rem;
    right: 0.75rem;
    bottom: 0.75rem;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.6rem;
    pointer-events: none;
  }
  .compose__actions > :global(*) {
    pointer-events: auto;
  }
</style>
