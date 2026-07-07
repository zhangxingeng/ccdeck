/**
 * displayModel.ts — turn a flat, ordered list of renderable row keys into
 * display items for the editor: one chat bubble per row.
 *
 * Pure TypeScript — no DOM, no Svelte.
 *
 * Tool-call/tool-result/thinking rows no longer exist in the parsed model
 * (see parser.ts / builder.ts), so there is nothing left to collapse into a
 * "tool activity" group — every renderable row is its own bubble.
 */

export interface DisplayMessage {
  kind: 'message';
  key: string;
}

export type DisplayItem = DisplayMessage;

/** Wrap each row key as a display message, preserving order. */
export function groupDisplayItems(keys: string[]): DisplayItem[] {
  return keys.map((key) => ({ kind: 'message', key }));
}
