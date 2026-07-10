/**
 * Smoke test for the pure notices module (src/lib/prompts/notices.ts):
 * deriving the durable data-event notices (repaired snippets + unreadable
 * files) and the gear badge count. No DOM.
 * Run with: npx tsx tests/notices_smoke.mjs
 */
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const __dir = dirname(fileURLToPath(import.meta.url));
const root = join(__dir, '..');

const { deriveNotices, noticeBadgeCount } = await import(
  join(root, 'src/lib/prompts/notices.ts')
);

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

console.log('deriveNotices');
{
  eq(deriveNotices([], []), [], 'no data events → no notices');
  eq(noticeBadgeCount(deriveNotices([], [])), 0, 'no notices → badge 0 (hidden)');

  const repaired = deriveNotices([{ id: 's1', title: 'tone-notes' }], []);
  eq(repaired.length, 1, 'one repaired snippet → one notice');
  eq(repaired[0].kind, 'repaired', 'repaired notice kind');
  eq(repaired[0].id, 's1', 'repaired notice keyed by snippet id');
  eq(repaired[0].title, 'tone-notes', 'repaired notice titled by snippet title');
  assert(/re-save/i.test(repaired[0].detail), 'repaired detail carries the re-save nudge');

  const unreadable = deriveNotices([], [{ file: 'broken.json', error: 'expected `,` at line 3' }]);
  eq(unreadable[0].kind, 'unreadable', 'unreadable notice kind');
  eq(unreadable[0].id, 'broken.json', 'unreadable notice keyed by file path');
  eq(unreadable[0].detail, 'expected `,` at line 3', 'unreadable detail is the parse error');

  // Both sources, stable order: repairs first, then unreadable files.
  const both = deriveNotices(
    [{ id: 's1', title: 'a' }, { id: 's2', title: 'b' }],
    [{ file: 'x.json', error: 'boom' }]
  );
  eq(both.map((n) => n.kind), ['repaired', 'repaired', 'unreadable'], 'repairs precede unreadable, order stable');
  eq(noticeBadgeCount(both), 3, 'badge counts every unresolved data event');

  // Config events (a hand-edited hotkey that fell back to its default) are the
  // third source — a durable trace, not a silent drop. They sort last.
  const cfg = deriveNotices(
    [{ id: 's1', title: 'a' }],
    [{ file: 'x.json', error: 'boom' }],
    [{ command: 'copyPrompt', label: 'Copy full prompt', chord: 'c', reason: 'needs Ctrl or Cmd.' }]
  );
  eq(cfg.map((n) => n.kind), ['repaired', 'unreadable', 'config'], 'config events sort after snippet events');
  const configNotice = cfg.find((n) => n.kind === 'config');
  eq(configNotice.id, 'hotkey:copyPrompt', 'config notice keyed by hotkey command');
  assert(/copy full prompt/i.test(configNotice.title), 'config notice titled by the command label');
  assert(configNotice.detail.includes('c') && /shortcuts/i.test(configNotice.detail), 'config detail carries the chord + the re-bind nudge');
  eq(noticeBadgeCount(cfg), 3, 'the config event counts toward the badge');
  // Absent third arg stays backward-compatible (default []).
  eq(deriveNotices([{ id: 's1', title: 'a' }], []).length, 1, 'invalidHotkeys defaults to none');
}

if (failures > 0) {
  console.error(`\nnotices_smoke: ${failures} failure(s)`);
  process.exit(1);
}
console.log('\nnotices_smoke: all assertions passed');
