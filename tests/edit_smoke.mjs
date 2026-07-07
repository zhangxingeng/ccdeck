/**
 * Smoke test for editDraft.ts — the plain edit-in-place model (no version
 * history, no reorder, no delete/restore; see issue #6 Phase B).
 * Run with: npx tsx tests/edit_smoke.mjs   (from repo root)
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const __dir = dirname(fileURLToPath(import.meta.url));
const root = join(__dir, '..');

const {
  buildDraft,
  serializeDraft,
  isDirty,
  applyBlockTextEdit,
  applyRoleEdit,
  applyRawEdit,
  extractSessionInfo,
  MESSAGE_ROLES,
} = await import(join(root, 'src/lib/editDraft.ts'));

// ── Test helpers ──────────────────────────────────────────────────────────────
let passed = 0;
let failed = 0;

function assert(cond, msg) {
  if (cond) {
    console.log(`  ok  ${msg}`);
    passed++;
  } else {
    console.error(`  FAIL ${msg}`);
    failed++;
  }
}

// ── Mock JSONL fixtures ───────────────────────────────────────────────────────

// uuid-bearing user line (string content)
const LINE_USER = JSON.stringify({
  type: 'user',
  uuid: 'user-uuid-001',
  timestamp: '2024-01-01T10:00:00Z',
  cwd: '/home/user/project',
  gitBranch: 'main',
  version: '1.0.0',
  message: { role: 'user', content: 'Hello, world!' },
});

// assistant line with tool_use + thinking + text blocks
const LINE_ASSISTANT = JSON.stringify({
  type: 'assistant',
  uuid: 'asst-uuid-001',
  timestamp: '2024-01-01T10:00:01Z',
  message: {
    role: 'assistant',
    model: 'claude-opus-4-8',
    content: [
      { type: 'thinking', thinking: 'I am thinking...' },
      { type: 'tool_use', id: 'tu1', name: 'Bash', input: { command: 'ls' } },
      { type: 'text', text: 'Here is my answer.' },
    ],
  },
});

// uuid-less ai-title line (fallback to idx key)
const LINE_AI_TITLE = JSON.stringify({
  type: 'ai-title',
  timestamp: '2024-01-01T10:00:02Z',
  message: { content: 'My Session Title' },
});

// permission-mode line (no uuid, no timestamp)
const LINE_PERM = JSON.stringify({
  type: 'permission-mode',
  permissionMode: 'bypassPermissions',
});

const RAW_TEXT = [LINE_USER, LINE_ASSISTANT, LINE_AI_TITLE, LINE_PERM].join('\n') + '\n';

// ── Test: buildDraft keys ─────────────────────────────────────────────────────
console.log('\n[buildDraft — keys]');
const draft0 = buildDraft(RAW_TEXT, '/fake/path.jsonl', 1700000000);
const keys = draft0.order;
assert(keys.length === 4, `order has 4 keys (got ${keys.length})`);
assert(keys[0] === 'user-uuid-001', `user key is uuid: "${keys[0]}"`);
assert(keys[1] === 'asst-uuid-001', `assistant key is uuid: "${keys[1]}"`);
assert(keys[2] === 'idx:2', `ai-title key is idx fallback: "${keys[2]}"`);
assert(keys[3] === 'idx:3', `permission-mode key is idx fallback: "${keys[3]}"`);
assert(draft0.rows['user-uuid-001'].uuid === 'user-uuid-001', 'user row uuid set');
assert(draft0.rows['idx:2'].uuid === null, 'ai-title row uuid is null');
assert(draft0.rows['idx:2'].type === 'ai-title', 'ai-title row type correct');
assert(draft0.createdAt === 1700000000, 'createdAt passed through');

// ── Test: original/value byte-faithful ────────────────────────────────────────
console.log('\n[buildDraft — original/value byte-faithful]');
assert(draft0.rows['user-uuid-001'].original === LINE_USER, 'user row original exact');
assert(draft0.rows['user-uuid-001'].value === LINE_USER, 'user row value starts equal to original');

// ── Test: isDirty false on fresh draft ────────────────────────────────────────
console.log('\n[isDirty]');
assert(!isDirty(draft0), 'isDirty false on fresh draft');

// ── Test: serializeDraft byte-preserves untouched lines ──────────────────────
console.log('\n[serializeDraft — untouched]');
const serialized0 = serializeDraft(draft0);
const sLines0 = serialized0.split('\n');
assert(sLines0[sLines0.length - 1] === '', 'trailing newline');
const sLines = sLines0.slice(0, -1);
assert(sLines[0] === LINE_USER, 'user line byte-identical');
assert(sLines[1] === LINE_ASSISTANT, 'assistant line byte-identical');
assert(sLines[2] === LINE_AI_TITLE, 'ai-title line byte-identical');
assert(sLines[3] === LINE_PERM, 'permission-mode line byte-identical');

// ── Test: applyBlockTextEdit — string content ─────────────────────────────────
console.log('\n[applyBlockTextEdit — string content]');
const draft1 = applyBlockTextEdit(draft0, 'user-uuid-001', 0, 'Updated text');
assert(draft1.rows['user-uuid-001'].value !== draft1.rows['user-uuid-001'].original, 'user row value changed');
assert(isDirty(draft1), 'isDirty true after text edit');
const s1Lines = serializeDraft(draft1).split('\n').slice(0, -1);
const parsed1 = JSON.parse(s1Lines[0]);
assert(parsed1.message.content === 'Updated text', 'serialized string content matches');
// Other lines byte-identical
assert(s1Lines[1] === LINE_ASSISTANT, 'untouched assistant line unchanged');
assert(s1Lines[2] === LINE_AI_TITLE, 'untouched ai-title line unchanged');

// ── Test: applyBlockTextEdit — array with text block ──────────────────────────
console.log('\n[applyBlockTextEdit — array with text block]');
const draft2 = applyBlockTextEdit(draft0, 'asst-uuid-001', 0, 'New assistant text');
assert(draft2.rows['asst-uuid-001'].value !== draft2.rows['asst-uuid-001'].original, 'assistant row value changed');
const asstParsed = JSON.parse(draft2.rows['asst-uuid-001'].value);
const editedTextBlock = asstParsed.message.content.find(b => b.type === 'text');
assert(editedTextBlock?.text === 'New assistant text', `assistant text updated: "${editedTextBlock?.text}"`);
// Verify thinking/tool_use blocks preserved
const thinkingBlock = asstParsed.message.content.find(b => b.type === 'thinking');
assert(thinkingBlock?.thinking === 'I am thinking...', 'thinking block preserved after text edit');
const toolBlock = asstParsed.message.content.find(b => b.type === 'tool_use');
assert(toolBlock?.name === 'Bash', 'tool_use block preserved after text edit');

// ── Test: extractSessionInfo ──────────────────────────────────────────────────
console.log('\n[extractSessionInfo]');
const info = extractSessionInfo(RAW_TEXT);
assert(info.cwd === '/home/user/project', `cwd: "${info.cwd}"`);
assert(info.gitBranch === 'main', `gitBranch: "${info.gitBranch}"`);
assert(info.versions.includes('1.0.0'), `versions includes '1.0.0': ${JSON.stringify(info.versions)}`);
assert(info.models.includes('claude-opus-4-8'), `models includes 'claude-opus-4-8': ${JSON.stringify(info.models)}`);
assert(info.permissionMode === 'bypassPermissions', `permissionMode: "${info.permissionMode}"`);
assert(info.firstTs === '2024-01-01T10:00:00Z', `firstTs: "${info.firstTs}"`);
assert(info.lastTs === '2024-01-01T10:00:02Z', `lastTs: "${info.lastTs}"`);
assert(info.userCount === 1, `userCount: ${info.userCount}`);
assert(info.assistantCount === 1, `assistantCount: ${info.assistantCount}`);
assert(info.lineCount === 4, `lineCount: ${info.lineCount}`);

// ── Test: enum catalogs ───────────────────────────────────────────────────────
console.log('\n[enum catalogs]');
assert(Array.isArray(MESSAGE_ROLES) && MESSAGE_ROLES.includes('user'), 'MESSAGE_ROLES has user');
assert(Array.isArray(MESSAGE_ROLES) && MESSAGE_ROLES.includes('assistant'), 'MESSAGE_ROLES has assistant');

// ── Test: applyRoleEdit flips both type and message.role atomically ───────────
console.log('\n[applyRoleEdit]');
{
  const line = JSON.stringify({ type: 'assistant', uuid: 'r1', message: { role: 'assistant', content: 'hi' } });
  let d = buildDraft(line + '\n', '/p', 0);
  d = applyRoleEdit(d, 'r1', 'user');
  const obj = JSON.parse(serializeDraft(d).trim());
  assert(obj.type === 'user', `top-level type flipped: ${obj.type}`);
  assert(obj.message.role === 'user', `message.role flipped: ${obj.message.role}`);
  assert(d.rows['r1'].type === 'user', 'row.type mirrors the flipped role');
  assert(isDirty(d), 'isDirty true after role edit');
}

// ── Test: applyRawEdit validates + normalizes; rejects invalid JSON ──────────
console.log('\n[applyRawEdit]');
{
  const line = JSON.stringify({ type: 'user', uuid: 'x1', message: { role: 'user', content: 'a' } });
  let d = buildDraft(line + '\n', '/p', 0);

  // Valid raw edit (pretty/multi-line input) is accepted and collapsed to one line
  const pretty = '{\n  "type": "user",\n  "uuid": "x1",\n  "message": { "role": "user", "content": "b" }\n}';
  d = applyRawEdit(d, 'x1', pretty);
  const out = serializeDraft(d).trim();
  assert(out.split('\n').length === 1, 'raw edit result is a single JSONL line');
  assert(JSON.parse(out).message.content === 'b', 'raw edit applied new content');
  assert(d.rows['x1'] !== undefined, 'row key stays stable across raw edit');

  // Invalid JSON is rejected (throws), draft untouched by caller
  let threw = false;
  try { applyRawEdit(d, 'x1', '{ not valid json '); } catch { threw = true; }
  assert(threw, 'invalid JSON raw edit throws');

  // Non-object/array top-level is rejected
  let threw2 = false;
  try { applyRawEdit(d, 'x1', '42'); } catch { threw2 = true; }
  assert(threw2, 'bare scalar raw edit throws');
}

// ── Test: no-op edits leave the draft byte-identical and non-dirty ──────────
console.log('\n[no-op edit detection]');
{
  // Editing a block to the SAME text must not mark the row dirty.
  const same = applyBlockTextEdit(draft0, 'user-uuid-001', 0, 'Hello, world!');
  assert(same === draft0, 'applyBlockTextEdit no-op returns the same draft object');
  assert(!isDirty(same), 'applyBlockTextEdit no-op leaves draft clean');

  // Same via per-block edit on the array-content assistant text block.
  const sameBlock = applyBlockTextEdit(draft0, 'asst-uuid-001', 0, 'Here is my answer.');
  assert(sameBlock === draft0, 'applyBlockTextEdit (array content) no-op returns same draft');

  // Role edit to the same role is a no-op.
  const sameRole = applyRoleEdit(draft0, 'user-uuid-001', 'user');
  assert(sameRole === draft0, 'applyRoleEdit to same role is a no-op');

  // Raw edit that re-stringifies to the identical line is a no-op.
  const sameRaw = applyRawEdit(draft0, 'user-uuid-001', draft0.rows['user-uuid-001'].original);
  assert(sameRaw === draft0, 'applyRawEdit to identical line is a no-op');

  // A genuine change still mutates (guards against over-eager no-op).
  const changed = applyBlockTextEdit(draft0, 'user-uuid-001', 0, 'Different!');
  assert(changed.rows['user-uuid-001'].value !== changed.rows['user-uuid-001'].original, 'genuine edit still mutates value');
}

// ── Test: applyBlockTextEdit targets a specific text block by ordinal ─────────
console.log('\n[applyBlockTextEdit — ordinal targeting]');
{
  // Message with TWO text blocks around a tool_use — edit each independently.
  const line = JSON.stringify({
    type: 'assistant',
    uuid: 'multi-1',
    message: {
      role: 'assistant',
      content: [
        { type: 'text', text: 'first' },
        { type: 'tool_use', id: 't', name: 'Bash', input: {} },
        { type: 'text', text: 'second' },
      ],
    },
  });
  let d = buildDraft(line + '\n', '/p', 0);

  // Edit ordinal 1 (the SECOND text block) — first must be untouched.
  d = applyBlockTextEdit(d, 'multi-1', 1, 'SECOND-EDITED');
  const c = JSON.parse(d.rows['multi-1'].value).message.content;
  assert(c[0].text === 'first', 'ordinal edit left first text block untouched');
  assert(c[2].text === 'SECOND-EDITED', 'ordinal 1 edited the second text block');
  assert(c[1].type === 'tool_use' && c[1].name === 'Bash', 'tool_use between text blocks preserved');

  // Edit ordinal 0 (the FIRST text block) on the freshly-edited draft.
  d = applyBlockTextEdit(d, 'multi-1', 0, 'FIRST-EDITED');
  const c2 = JSON.parse(d.rows['multi-1'].value).message.content;
  assert(c2[0].text === 'FIRST-EDITED', 'ordinal 0 edited the first text block');
  assert(c2[2].text === 'SECOND-EDITED', 'previous ordinal-1 edit retained');

  // Out-of-range ordinal is a no-op.
  const oor = applyBlockTextEdit(d, 'multi-1', 5, 'nope');
  assert(oor === d, 'out-of-range ordinal is a no-op');
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
