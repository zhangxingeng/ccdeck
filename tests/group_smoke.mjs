/**
 * Smoke test for displayModel.ts (grouping).
 * Run with: npx tsx tests/group_smoke.mjs   (from repo root)
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const __dir = dirname(fileURLToPath(import.meta.url));
const root = join(__dir, '..');

const { groupDisplayItems } = await import(join(root, 'src/lib/displayModel.ts'));

let passed = 0, failed = 0;
function assert(cond, msg) {
  if (cond) { console.log(`  ok  ${msg}`); passed++; }
  else { console.error(`  FAIL ${msg}`); failed++; }
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
