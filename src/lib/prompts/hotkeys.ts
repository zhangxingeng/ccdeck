/**
 * Prompts-view hotkeys — the chord grammar and the pure logic behind rebinding
 * (contract project_docs/prompts-design.md §Hotkeys, project_docs/prompts-ux.md
 * §Hotkey map). No DOM, no Svelte: chord parsing, normalization, event
 * matching, and conflict detection are pure string/record transforms so they
 * are unit-testable (tests/hotkeys_smoke.mjs) and the same logic serves both
 * the live keydown handler and the rebinding UI.
 *
 * A chord is a normalized string like `"Mod+Shift+C"`. `Mod` stands for `Ctrl`
 * on Windows/Linux and `Cmd` on macOS, so one stored binding is correct on
 * every platform and a config file is portable. Only *command* hotkeys live
 * here; the spatial/context keys (↓ / Enter / Esc) are not commands and never
 * rebind — encoding them would let a rebind break the conventions the whole
 * interaction infers from.
 */

/** The rebindable Prompts-view commands. */
export type HotkeyCommand = 'copyPrompt' | 'saveAs';

export const HOTKEY_COMMANDS: readonly HotkeyCommand[] = ['copyPrompt', 'saveAs'];

/** Defaults (contract §Hotkey map). An absent command id falls back here, so a
 *  fresh install and a pre-hotkeys config are the same case — no migration. */
export const DEFAULT_HOTKEYS: Record<HotkeyCommand, string> = {
  copyPrompt: 'Mod+C',
  saveAs: 'Mod+S',
};

/** Human labels for the rebinding UI and inline conflict messages. */
export const HOTKEY_LABELS: Record<HotkeyCommand, string> = {
  copyPrompt: 'Copy full prompt',
  saveAs: 'Save as…',
};

/** The chords the OS/browser owns; rebinding onto one is allowed but must
 *  `preventDefault` and is flagged "overrides system default" in the UI. */
const SYSTEM_CHORDS = new Set(['Mod+C', 'Mod+S', 'Mod+V', 'Mod+X', 'Mod+Z', 'Mod+A']);

/** A modifier key pressed alone is not a chord — ignore it as a keystroke. */
const MODIFIER_KEYS = new Set(['Control', 'Meta', 'Shift', 'Alt', 'AltGraph']);

/** The minimum an event needs to form a chord — a structural subset of
 *  KeyboardEvent, so callers pass the event directly and tests pass a literal. */
export interface ChordEvent {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
}

export interface ParsedChord {
  mod: boolean;
  alt: boolean;
  shift: boolean;
  /** The main key, normalized: single letters uppercased, others verbatim. */
  key: string;
}

/** Normalize a key token: a single letter uppercases (so `c` and `C` are one
 *  chord), everything else (digits, `Enter`, `,`) is left as typed. */
function normalizeKey(key: string): string {
  return key.length === 1 ? key.toUpperCase() : key;
}

/** Parse a chord string into its parts, or null if it carries no main key
 *  (e.g. `"Mod"` alone, or `""`). Modifier tokens are case-insensitive;
 *  `Ctrl`/`Cmd`/`Meta`/`Command` all fold to `Mod`. */
export function parseChord(chord: string): ParsedChord | null {
  const parts = chord.split('+').map((p) => p.trim()).filter(Boolean);
  let mod = false;
  let alt = false;
  let shift = false;
  let key: string | null = null;
  for (const part of parts) {
    const lower = part.toLowerCase();
    if (lower === 'mod' || lower === 'ctrl' || lower === 'control' || lower === 'cmd' || lower === 'meta' || lower === 'command') {
      mod = true;
    } else if (lower === 'alt' || lower === 'option') {
      alt = true;
    } else if (lower === 'shift') {
      shift = true;
    } else {
      // The last non-modifier token wins as the main key (a well-formed chord
      // has exactly one; being lenient here keeps hand-edited configs working).
      key = normalizeKey(part);
    }
  }
  return key === null ? null : { mod, alt, shift, key };
}

/** Canonical chord string: modifiers in fixed order (Mod, Alt, Shift), key
 *  last. Two chords are equal iff their canonical forms match. */
export function formatChord(parsed: ParsedChord): string {
  const parts: string[] = [];
  if (parsed.mod) parts.push('Mod');
  if (parsed.alt) parts.push('Alt');
  if (parsed.shift) parts.push('Shift');
  parts.push(parsed.key);
  return parts.join('+');
}

/** Round-trip a chord string through parse+format to its canonical form;
 *  returns null for an incomplete chord. */
export function normalizeChord(chord: string): string | null {
  const parsed = parseChord(chord);
  return parsed ? formatChord(parsed) : null;
}

/** The chord an event represents, or null when it carries no main key (a bare
 *  modifier press). `Mod` is set for Ctrl OR Cmd, so the result is platform-
 *  neutral and compares directly against a stored binding. */
export function chordFromEvent(e: ChordEvent): string | null {
  if (MODIFIER_KEYS.has(e.key)) return null;
  return formatChord({
    mod: e.ctrlKey || e.metaKey,
    alt: e.altKey,
    shift: e.shiftKey,
    key: normalizeKey(e.key),
  });
}

/** Does this event match this stored chord? */
export function eventMatchesChord(e: ChordEvent, chord: string): boolean {
  const evChord = chordFromEvent(e);
  const target = normalizeChord(chord);
  return evChord !== null && target !== null && evChord === target;
}

/** Merge stored overrides onto the defaults, keeping only known command ids and
 *  only well-formed chords (a hand-edited garbage chord falls back to default
 *  rather than binding to nothing). */
export function resolveHotkeys(overrides: Record<string, string> | undefined): Record<HotkeyCommand, string> {
  const resolved: Record<HotkeyCommand, string> = { ...DEFAULT_HOTKEYS };
  if (!overrides) return resolved;
  for (const command of HOTKEY_COMMANDS) {
    const chord = overrides[command];
    if (typeof chord === 'string') {
      const normal = normalizeChord(chord);
      if (normal) resolved[command] = normal;
    }
  }
  return resolved;
}

/** Which command (if any) already holds `chord`, ignoring `self`. Drives the
 *  inline "already bound to X" rejection — we never silently steal a binding
 *  from another command. Compares canonical forms so `ctrl+c` and `Mod+C`
 *  collide. */
export function findConflict(
  hotkeys: Record<HotkeyCommand, string>,
  self: HotkeyCommand,
  chord: string
): HotkeyCommand | null {
  const target = normalizeChord(chord);
  if (target === null) return null;
  for (const command of HOTKEY_COMMANDS) {
    if (command === self) continue;
    if (normalizeChord(hotkeys[command]) === target) return command;
  }
  return null;
}

/** True when a chord collides with a browser/OS default — allowed, but shown
 *  with an "overrides system default" note and only effective with
 *  preventDefault. */
export function overridesSystem(chord: string): boolean {
  const normal = normalizeChord(chord);
  return normal !== null && SYSTEM_CHORDS.has(normal);
}
