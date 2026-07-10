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
}

if (failures > 0) {
  console.error(`\nnotices_smoke: ${failures} failure(s)`);
  process.exit(1);
}
console.log('\nnotices_smoke: all assertions passed');
