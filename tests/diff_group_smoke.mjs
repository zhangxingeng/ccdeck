/**
 * Smoke test for diff.ts (version-diff helpers) and displayModel.ts (grouping).
 * Run with: npx tsx tests/diff_group_smoke.mjs   (from repo root)
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const __dir = dirname(fileURLToPath(import.meta.url));
const root = join(__dir, '..');

const { wordDiff, hasTextChange, availableTargets, targetIndex, targetLabel } =
  await import(join(root, 'src/lib/diff.ts'));
const { groupDisplayItems } = await import(join(root, 'src/lib/displayModel.ts'));

let passed = 0, failed = 0;
function assert(cond, msg) {
  if (cond) { console.log(`  ok  ${msg}`); passed++; }
  else { console.error(`  FAIL ${msg}`); failed++; }
}

// ── diff: wordDiff ────────────────────────────────────────────────────────────
console.log('\n[wordDiff]');
{
  const spans = wordDiff('the cat sat', 'the dog sat');
  const added = spans.filter(s => s.added).map(s => s.value.trim()).join('');
  const removed = spans.filter(s => s.removed).map(s => s.value.trim()).join('');
  assert(added === 'dog', `added word is "dog" (got "${added}")`);
  assert(removed === 'cat', `removed word is "cat" (got "${removed}")`);
  const same = wordDiff('identical', 'identical');
  assert(same.every(s => !s.added && !s.removed), 'identical strings produce no add/remove spans');
  assert(hasTextChange('a', 'b') === true, 'hasTextChange true when differ');
  assert(hasTextChange('a', 'a') === false, 'hasTextChange false when equal');
}

// ── diff: availableTargets / targetIndex ─────────────────────────────────────
console.log('\n[availableTargets / targetIndex]');
{
  // Single version → nothing to compare.
  assert(availableTargets(0, 1).length === 0, 'no targets for a single version');

  // 2 versions, on original (active 0): only "latest".
  assert(JSON.stringify(availableTargets(0, 2)) === JSON.stringify(['latest']),
    'v1 of 2 → [latest]');
  // 2 versions, on latest (active 1): only "original".
  assert(JSON.stringify(availableTargets(1, 2)) === JSON.stringify(['original']),
    'v2 of 2 → [original]');

  // 4 versions, middle (active 1): original, next, latest (previous == original, hidden).
  assert(JSON.stringify(availableTargets(1, 4)) === JSON.stringify(['original', 'next', 'latest']),
    'v2 of 4 → [original, next, latest]');
  // 4 versions, middle (active 2): original, previous, latest (next == latest, hidden).
  assert(JSON.stringify(availableTargets(2, 4)) === JSON.stringify(['original', 'previous', 'latest']),
    'v3 of 4 → [original, previous, latest]');
  // 5 versions, dead center (active 2): all four distinct.
  assert(JSON.stringify(availableTargets(2, 5)) === JSON.stringify(['original', 'previous', 'next', 'latest']),
    'v3 of 5 → all four targets');

  // targetIndex resolution.
  assert(targetIndex('original', 3, 5) === 0, 'original → 0');
  assert(targetIndex('latest', 3, 5) === 4, 'latest → last');
  assert(targetIndex('previous', 3, 5) === 2, 'previous → active-1');
  assert(targetIndex('next', 3, 5) === 4, 'next → active+1');
  assert(targetLabel('original') === 'Original', 'targetLabel original');
}

// ── displayModel: groupDisplayItems ──────────────────────────────────────────
// Tool-call/tool-result/thinking rendering (and the "toolgroup" collapsing it
// fed) was removed — groupDisplayItems now just wraps each row key as its own
// message, in order.
console.log('\n[groupDisplayItems]');
{
  // Empty input.
  assert(groupDisplayItems([]).length === 0, 'empty rows → no items');

  // Order preserved; every key becomes its own message.
  const items = groupDisplayItems(['u1', 'a1', 'u2']);
  assert(items.length === 3, `3 display items (got ${items.length})`);
  assert(items[0].kind === 'message' && items[0].key === 'u1', 'item0 message u1');
  assert(items[1].kind === 'message' && items[1].key === 'a1', 'item1 message a1');
  assert(items[2].kind === 'message' && items[2].key === 'u2', 'item2 message u2');
  assert(items.every(i => i.kind === 'message'), 'every item is a message');
}

// ── Summary ───────────────────────────────────────────────────────────────────
console.log(`\n${'─'.repeat(50)}`);
if (failed === 0) {
  console.log(`All ${passed} assertions passed.`);
  process.exit(0);
} else {
  console.error(`${failed} FAILED, ${passed} passed.`);
  process.exit(1);
}
