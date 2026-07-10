/**
 * Smoke test for the pure hotkeys module (src/lib/prompts/hotkeys.ts):
 * chord parsing/normalization (Mod = Ctrl/Cmd), event→chord matching, override
 * merging, and conflict detection — the logic behind the rebinding UI and the
 * live keydown handler. No DOM: ChordEvent is a structural subset of
 * KeyboardEvent, so tests pass plain literals.
 * Run with: npx tsx tests/hotkeys_smoke.mjs
 */
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const __dir = dirname(fileURLToPath(import.meta.url));
const root = join(__dir, '..');

const {
  DEFAULT_HOTKEYS,
  HOTKEY_COMMANDS,
  parseChord,
  formatChord,
  normalizeChord,
  chordFromEvent,
  eventMatchesChord,
  resolveHotkeys,
  findConflict,
  overridesSystem,
} = await import(join(root, 'src/lib/prompts/hotkeys.ts'));

let failures = 0;
function assert(cond, msg) {
  if (!cond) {
    failures++;
    console.error(`  FAIL: ${msg}`);
  }
}
function eq(actual, expected, msg) {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  assert(a === e, `${msg}\n    expected ${e}\n    got      ${a}`);
}

const ev = (key, mods = {}) => ({
  key,
  ctrlKey: false,
  metaKey: false,
  shiftKey: false,
  altKey: false,
  ...mods,
});

console.log('parse / format / normalize');
{
  eq(parseChord('Mod+C'), { mod: true, alt: false, shift: false, key: 'C' }, 'Mod+C parses');
  eq(parseChord('mod+shift+c'), { mod: true, alt: false, shift: true, key: 'C' }, 'case-insensitive, letter uppercased');
  // Every alias for the platform-modifier folds to Mod.
  eq(normalizeChord('Ctrl+C'), 'Mod+C', 'Ctrl folds to Mod');
  eq(normalizeChord('Cmd+C'), 'Mod+C', 'Cmd folds to Mod');
  eq(normalizeChord('Meta+C'), 'Mod+C', 'Meta folds to Mod');
  // Canonical order is Mod, Alt, Shift, key — regardless of input order.
  eq(normalizeChord('Shift+Alt+Mod+K'), 'Mod+Alt+Shift+K', 'modifiers sort into canonical order');
  eq(normalizeChord('c'), 'C', 'bare letter is a valid (modifier-less) chord');
  eq(parseChord('Mod'), null, 'a bare modifier is not a chord');
  eq(parseChord(''), null, 'empty string is not a chord');
  eq(normalizeChord('  ctrl + shift + s '), 'Mod+Shift+S', 'whitespace tolerated');
  // Named (multi-char) keys are preserved verbatim, not uppercased char-by-char.
  eq(formatChord({ mod: true, alt: false, shift: false, key: 'Enter' }), 'Mod+Enter', 'named key preserved');
}

console.log('chordFromEvent / eventMatchesChord');
{
  eq(chordFromEvent(ev('c', { ctrlKey: true })), 'Mod+C', 'ctrl+c event → Mod+C');
  eq(chordFromEvent(ev('c', { metaKey: true })), 'Mod+C', 'cmd+c event → Mod+C (platform-neutral)');
  eq(chordFromEvent(ev('C', { ctrlKey: true, shiftKey: true })), 'Mod+Shift+C', 'ctrl+shift+c');
  eq(chordFromEvent(ev('Control')), null, 'a lone modifier keydown is not a chord');
  eq(chordFromEvent(ev('Shift', { shiftKey: true })), null, 'lone Shift press ignored');
  assert(eventMatchesChord(ev('s', { metaKey: true }), 'Mod+S'), 'cmd+s matches stored Mod+S');
  assert(eventMatchesChord(ev('S', { ctrlKey: true }), 'ctrl+s'), 'ctrl+S matches ctrl+s (case + alias)');
  assert(!eventMatchesChord(ev('s', { ctrlKey: true, shiftKey: true }), 'Mod+S'), 'extra Shift does not match');
  assert(!eventMatchesChord(ev('s'), 'Mod+S'), 'no modifier does not match a Mod chord');
}

console.log('resolveHotkeys');
{
  eq(resolveHotkeys(undefined), DEFAULT_HOTKEYS, 'undefined overrides → defaults');
  eq(resolveHotkeys({}), DEFAULT_HOTKEYS, 'empty overrides → defaults');
  eq(
    resolveHotkeys({ copyPrompt: 'Mod+Shift+C' }),
    { copyPrompt: 'Mod+Shift+C', saveAs: 'Mod+S' },
    'a valid override wins; unset command keeps its default'
  );
  eq(
    resolveHotkeys({ saveAs: 'ctrl+shift+p' }),
    { copyPrompt: 'Mod+C', saveAs: 'Mod+Shift+P' },
    'override is normalized to canonical form'
  );
  // A hand-edited garbage chord must fall back to the default, never bind to nothing.
  eq(resolveHotkeys({ copyPrompt: 'Mod' }).copyPrompt, 'Mod+C', 'garbage override falls back to default');
  eq(resolveHotkeys({ unknownCommand: 'Mod+Q' }), DEFAULT_HOTKEYS, 'unknown command id ignored');
}

console.log('findConflict / overridesSystem');
{
  const map = { copyPrompt: 'Mod+C', saveAs: 'Mod+S' };
  eq(findConflict(map, 'copyPrompt', 'Mod+S'), 'saveAs', 'rebinding copy onto save is rejected → names saveAs');
  eq(findConflict(map, 'copyPrompt', 'ctrl+s'), 'saveAs', 'conflict compares canonical forms (ctrl+s == Mod+S)');
  eq(findConflict(map, 'copyPrompt', 'Mod+K'), null, 'a free chord has no conflict');
  eq(findConflict(map, 'saveAs', 'Mod+S'), null, 'a command never conflicts with itself');
  assert(overridesSystem('Mod+C'), 'Mod+C overrides a system default');
  assert(overridesSystem('ctrl+s'), 'ctrl+s (canonical Mod+S) overrides a system default');
  assert(!overridesSystem('Mod+Shift+C'), 'Mod+Shift+C is not a claimed system chord');
  eq(HOTKEY_COMMANDS.length, 2, 'exactly two rebindable commands ship (JC-4: one copy binding)');
}

if (failures > 0) {
  console.error(`\nhotkeys_smoke: ${failures} failure(s)`);
  process.exit(1);
}
console.log('\nhotkeys_smoke: all assertions passed');
