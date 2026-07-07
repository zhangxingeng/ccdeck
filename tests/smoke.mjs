/**
 * Smoke test for parser.ts + builder.ts.
 * Run with: npx tsx tests/smoke.mjs   (from repo root)
 */

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const __dir = dirname(fileURLToPath(import.meta.url));
const root = join(__dir, '..');

// Use tsx to import TS modules
const { parseJsonl, extractMeta, decodeProject, cleanTitle } = await import(join(root, 'src/lib/parser.ts'));
const { buildSession } = await import(join(root, 'src/lib/builder.ts'));

// ── helpers ──────────────────────────────────────────────────────────────────
let passed = 0;
let failed = 0;

function assert(cond, msg) {
  if (cond) {
    console.log(`  ✓ ${msg}`);
    passed++;
  } else {
    console.error(`  ✗ FAIL: ${msg}`);
    failed++;
  }
}

// ── Load mock data ────────────────────────────────────────────────────────────
const sessionText = readFileSync(join(__dir, 'mock_data/session.jsonl'), 'utf8');

// ── Test parseJsonl ───────────────────────────────────────────────────────────
console.log('\n[parseJsonl]');
const entries = parseJsonl(sessionText);
assert(entries.length > 0, `parsed ${entries.length} entries`);

// ai-title and mode should be filtered
const hasMetaType = entries.some(e => e.type === 'mode' || e.type === 'ai-title');
assert(!hasMetaType, 'meta types filtered out');

// Internal echo check — no system-reminder blocks in user turns
const hasEcho = entries.some(e =>
  e.type === 'user' &&
  e.blocks.some(b => b.blockType === 'text' && b.text?.startsWith('<system-reminder>'))
);
assert(!hasEcho, 'internal echoes filtered');

// task-notification detected
const taskNotif = entries.find(e => e.taskNotification);
assert(!!taskNotif, 'task-notification entry detected');

// Interruption detected
const interruption = entries.find(e => e.isInterruption);
assert(!!interruption, 'interruption entry detected');

// ── Test buildSession ─────────────────────────────────────────────────────────
console.log('\n[buildSession]');
const session = buildSession(entries, { project: 'test', sourcePath: 'tests/mock_data/session.jsonl' });

assert(session.turns.length > 0, `built ${session.turns.length} turns`);

const assistantTurns = session.turns.filter(t => t.role === 'assistant');
const userTurns = session.turns.filter(t => t.role === 'user');
assert(assistantTurns.length > 0, `${assistantTurns.length} assistant turns`);
assert(userTurns.length > 0, `${userTurns.length} user turns`);

// Key: thinking/tool_use/tool_result blocks are trimmed entirely — the
// display model only ever carries user/assistant text blocks now.
const allBlocks = session.turns.flatMap(t => t.blocks);
const nonTextBlocks = allBlocks.filter(b => b.blockType !== 'text');
assert(nonTextBlocks.length === 0, `no non-text blocks survive parsing (found ${nonTextBlocks.length})`);
assert(allBlocks.every(b => b.toolOutput === undefined && b.agentId === undefined && b.subagent === undefined),
  'no tool/agent/subagent fields survive on any block');

// Interrupted turn
const interruptedTurn = session.turns.find(t => t.isInterrupted);
assert(!!interruptedTurn, 'interrupted turn marked');

// Meta
assert(session.meta.title.length > 0, `session title: "${session.meta.title}"`);
assert(session.meta.model.length > 0, `session model: "${session.meta.model}"`);

// ── Test extractMeta ──────────────────────────────────────────────────────────
console.log('\n[extractMeta]');
const rawLines = sessionText.split('\n').slice(0, 50);
const meta1 = extractMeta(rawLines);
assert(meta1.date.length > 0, `extractMeta from raw lines: date="${meta1.date}"`);

const meta2 = extractMeta(entries);
assert(meta2.date.length > 0, `extractMeta from entries: date="${meta2.date}"`);

// ── Test decodeProject ────────────────────────────────────────────────────────
console.log('\n[decodeProject]');
const decoded = decodeProject('-home-user-myproject');
assert(decoded.length > 0, `decodeProject: "${decoded}"`);
assert(!decoded.startsWith('-'), 'no leading dash');

// ── Test cleanTitle ───────────────────────────────────────────────────────────
console.log('\n[cleanTitle]');
assert(
  cleanTitle('# My Session Title') === 'My Session Title',
  'cleanTitle: strips leading "# "'
);
assert(
  cleanTitle('Hello\n\nWorld') === 'Hello World',
  'cleanTitle: collapses newlines to single space'
);

// ── Summary ───────────────────────────────────────────────────────────────────
console.log(`\n${'─'.repeat(50)}`);
if (failed === 0) {
  console.log(`All ${passed} assertions passed.`);
  process.exit(0);
} else {
  console.error(`${failed} FAILED, ${passed} passed.`);
  process.exit(1);
}
