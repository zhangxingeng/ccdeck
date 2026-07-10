/**
 * The ONE place a stored palette key becomes a CSS token reference.
 * Components assign the result to a custom property (--project-color,
 * --tab-color, --swatch-color) and style with color-mix over that var —
 * they never branch on key names or see a hex (color-token protocol: stored
 * data carries intent, app.css owns the hue per theme).
 */
import type { PaletteKey } from './types';

export function projectColorVar(key: PaletteKey): string {
  return `var(--project-${key})`;
}
