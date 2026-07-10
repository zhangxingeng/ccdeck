/**
 * Portal attachment — relocates the host element to `document.body` so it
 * escapes every ancestor's `overflow` clipping and stacking context. The
 * config popover's occlusion bug (contract §S10: it rendered *behind* the
 * compose box because it lived inside the library panel's stacking context)
 * cannot be fixed by relocating the anchor alone — the panel would just move
 * the bug to the next ancestor. Rendering the popover under `<body>`, fixed to
 * the viewport, is the root fix: no intermediate ancestor can clip or out-stack
 * a child of `<body>`.
 *
 * Svelte 5 attachment (`{@attach portal}`). `element.remove()` is null-safe, so
 * teardown is idempotent even though Svelte also tears the node down when its
 * `{#if}` closes.
 */
export function portal(node: HTMLElement): () => void {
  document.body.appendChild(node);
  return () => {
    node.remove();
  };
}
