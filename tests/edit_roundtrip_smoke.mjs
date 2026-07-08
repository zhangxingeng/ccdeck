/**
 * Round-trip corruption hunt for editDraft.ts (issue TBD — "editing a chat
 * corrupts the session file / Claude Code no longer recognizes it").
 *
 * Two properties any edit pipeline over someone else's file format must hold:
 *
 *   1. IDENTITY: buildDraft(raw) -> serializeDraft(...) with NO edits applied
 *      must reproduce `raw` byte-for-byte. Any drift here is a bug in the
 *      draft's own bookkeeping (key collisions, dropped/duplicated lines),
 *      independent of JSON semantics.
 *   2. EDIT FIDELITY: editing ONE field of a line must leave every OTHER
 *      field byte-for-value identical to the original parse. Any drift here
 *      is a bug in the JSON.parse -> mutate -> JSON.stringify round trip
 *      itself (e.g. numeric precision loss), not in our bookkeeping.
 *
 * Fixtures are deliberately adversarial (duplicate uuids, huge integers,
 * unicode/surrogates, numeric-looking object keys) — real Claude Code
 * session files can and do contain any of these, and a smoke fixture that
 * only has "nice" data would never have caught this class of bug.
 *
 * Run with: npx tsx tests/edit_roundtrip_smoke.mjs   (from repo root)
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync } from 'node:fs';

const __dir = dirname(fileURLToPath(import.meta.url));
const root = join(__dir, '..');

const {
  buildDraft, serializeDraft, isDirty, applyBlockTextEdit,
  blockKey, isBlockDeleted, setDeleted,
  deleteMessage, deleteThinking, deleteToolGroup, deleteBulk, undelete,
} = await import(join(root, 'src/lib/editDraft.ts'));
const { parseJsonl } = await import(join(root, 'src/lib/parser.ts'));
const { deriveTurnSpans } = await import(join(root, 'src/lib/displayModel.ts'));

// Replicate SessionEditor's renderable + rowsBlockKeys derivation so these
// tests exercise the EXACT key math the UI uses (block key == originalIndex:
// contentIndex, via editDraft.blockKey), not a bespoke reimplementation.
function renderableRows(draft) {
  const out = [];
  for (const key of draft.order) {
    const row = draft.rows[key];
    const es = parseJsonl(row.value);
    const entry = es.length > 0 ? es[0] : null;
    if (!entry || entry.blocks.length === 0) continue;
    out.push({ key, row, entry, hasText: entry.blocks.some((b) => b.blockType === 'text') });
  }
  return out;
}
function rowsBlockKeys(rr, rowKeys) {
  const map = new Map(rr.map((r) => [r.key, r]));
  const out = [];
  for (const k of rowKeys) {
    const r = map.get(k);
    if (!r) continue;
    r.entry.blocks.forEach((_, bi) => out.push(blockKey(r.row, bi)));
  }
  return out;
}

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

// ── Property 1: identity round trip on the real fixture ──────────────────────
console.log('\n[identity round trip — tests/mock_data/session.jsonl]');
{
  const raw = readFileSync(join(root, 'tests/mock_data/session.jsonl'), 'utf-8');
  const draft = buildDraft(raw, '/fake/path.jsonl', 0);
  const out = serializeDraft(draft);
  assert(out === raw, 'buildDraft -> serializeDraft reproduces the real fixture byte-for-byte');
  if (out !== raw) {
    const rawLines = raw.split('\n');
    const outLines = out.split('\n');
    console.error(`    raw has ${rawLines.length} lines, output has ${outLines.length} lines`);
    for (let i = 0; i < Math.max(rawLines.length, outLines.length); i++) {
      if (rawLines[i] !== outLines[i]) {
        console.error(`    first divergent line ${i}:`);
        console.error(`      raw: ${rawLines[i]}`);
        console.error(`      out: ${outLines[i]}`);
        break;
      }
    }
  }
}

// ── Adversarial fixtures ──────────────────────────────────────────────────────

const BIG_INT_UNSAFE = '9223372036854775807'; // i64::MAX — a plausible real field width
const BIG_INT_2_53_PLUS_1 = '9007199254740993'; // first integer JS f64 cannot represent exactly
const LONE_SURROGATE_TEXT = 'before\\ud800after'; // unpaired UTF-16 surrogate, valid inside a JSON string
const EMOJI_TEXT = '😀 unicode 你好世界 مرحبا test';

function line(obj) {
  return JSON.stringify(obj);
}

// Two DIFFERENT lines sharing the same uuid — plausible if a session was
// forked/resumed/merged, or if any producer ever reuses an id.
const DUP_A = line({ type: 'user', uuid: 'dup-1', message: { role: 'user', content: 'first message' } });
const DUP_B = line({ type: 'assistant', uuid: 'dup-1', message: { role: 'assistant', content: 'second message, same uuid' } });

// A line carrying a huge integer alongside editable text (simulates a
// token-usage-style field sitting next to message content).
const BIGINT_LINE = `{"type":"assistant","uuid":"big-1","tokenCount":${BIG_INT_UNSAFE},"message":{"role":"assistant","content":[{"type":"text","text":"edit me"}]}}`;

const BIGINT_2_53_LINE = `{"type":"assistant","uuid":"big-2","tokenCount":${BIG_INT_2_53_PLUS_1},"message":{"role":"assistant","content":[{"type":"text","text":"edit me"}]}}`;

// A line whose text content already has an unpaired surrogate.
const SURROGATE_LINE = line({
  type: 'user',
  uuid: 'surrogate-1',
  message: { role: 'user', content: LONE_SURROGATE_TEXT },
});

// A line with emoji/CJK/RTL in an UNEDITED sibling field, to check the
// identity round trip preserves multi-byte text untouched.
const UNICODE_LINE = line({
  type: 'user',
  uuid: 'unicode-1',
  message: { role: 'user', content: EMOJI_TEXT },
});

// Numeric-looking object keys inside a tool_use input — JS engines reorder
// integer-index-like keys to numeric-ascending order on stringify,
// regardless of insertion order.
const NUMERIC_KEYS_LINE = line({
  type: 'assistant',
  uuid: 'numkeys-1',
  message: {
    role: 'assistant',
    content: [
      { type: 'tool_use', id: 't1', name: 'X', input: { z: 1, '1': 'b', '0': 'a' } },
      { type: 'text', text: 'edit me' },
    ],
  },
});

const TORTURE_LINES = [
  DUP_A, DUP_B, BIGINT_LINE, BIGINT_2_53_LINE, SURROGATE_LINE, UNICODE_LINE, NUMERIC_KEYS_LINE,
];
const TORTURE_RAW = TORTURE_LINES.join('\n') + '\n';

// ── Property 1 on the torture fixture ─────────────────────────────────────────
console.log('\n[identity round trip — adversarial fixture]');
{
  const draft = buildDraft(TORTURE_RAW, '/fake/torture.jsonl', 0);
  assert(
    draft.order.length === TORTURE_LINES.length,
    `order has ${TORTURE_LINES.length} entries (got ${draft.order.length}) — duplicate uuids must not collapse rows`
  );
  const out = serializeDraft(draft);
  assert(out === TORTURE_RAW, 'adversarial fixture round-trips byte-for-byte with no edits');
  if (out !== TORTURE_RAW) {
    console.error(`    raw lines: ${TORTURE_LINES.length}, order keys: ${JSON.stringify(draft.order)}`);
    console.error(`    duplicate uuid 'dup-1' rows in order: ${draft.order.filter((k) => k === 'dup-1').length}`);
    console.error(`    dup-1 row value: ${draft.rows['dup-1']?.value}`);
  }
}

// ── Property 2: editing one field must not disturb the rest of the line ──────
console.log('\n[edit fidelity — huge integer sibling field, i64::MAX]');
{
  let draft = buildDraft(BIGINT_LINE + '\n', '/p', 0);
  draft = applyBlockTextEdit(draft, 'big-1', 0, 'EDITED');
  const savedLine = draft.rows['big-1'].value;
  assert(
    savedLine.includes(`"tokenCount":${BIG_INT_UNSAFE}`),
    `i64::MAX tokenCount survives an unrelated text edit exactly (expected ...${BIG_INT_UNSAFE}...): ${savedLine}`
  );
}

console.log('\n[edit fidelity — integer just past 2^53 precision boundary]');
{
  let draft = buildDraft(BIGINT_2_53_LINE + '\n', '/p', 0);
  draft = applyBlockTextEdit(draft, 'big-2', 0, 'EDITED');
  const savedLine = draft.rows['big-2'].value;
  assert(
    savedLine.includes(`"tokenCount":${BIG_INT_2_53_PLUS_1}`),
    `2^53+1 tokenCount survives an unrelated text edit exactly (expected ...${BIG_INT_2_53_PLUS_1}...): ${savedLine}`
  );
}

console.log('\n[edit fidelity — unpaired UTF-16 surrogate in an untouched sibling row]');
{
  const raw = SURROGATE_LINE + '\n' + BIGINT_LINE + '\n';
  let draft = buildDraft(raw, '/p', 0);
  // Edit an unrelated row; the surrogate-bearing row must stay byte-identical.
  draft = applyBlockTextEdit(draft, 'big-1', 0, 'EDITED');
  assert(
    draft.rows['surrogate-1'].value === draft.rows['surrogate-1'].original,
    'untouched surrogate-bearing row is byte-identical after an unrelated edit'
  );
}

console.log('\n[edit fidelity — numeric-looking object keys preserve their VALUES]');
{
  let draft = buildDraft(NUMERIC_KEYS_LINE + '\n', '/p', 0);
  draft = applyBlockTextEdit(draft, 'numkeys-1', 0, 'EDITED');
  const parsed = JSON.parse(draft.rows['numkeys-1'].value);
  const input = parsed.message.content.find((b) => b.type === 'tool_use').input;
  assert(input.z === 1 && input['1'] === 'b' && input['0'] === 'a', 'tool_use.input values survive regardless of any key reordering');
}

// ── Soft delete (issue #14) ────────────────────────────────────────────────
//
// Deletion granularity is the content block, keyed via blockKey(row, blockIndex).
// The correctness invariant that matters most: deleting a tool_use MUST
// cascade to delete its paired tool_result (and vice versa) — the real
// Claude Code CLI rejects a session with an orphaned half of a tool pair.

const REAL_RAW = readFileSync(join(root, 'tests/mock_data/session.jsonl'), 'utf-8');

// (a) Identity round trip with ZERO deletions — unchanged from pre-#14.
console.log('\n[delete — (a) identity round trip, zero deletions]');
{
  const draft = buildDraft(REAL_RAW, '/fake/path.jsonl', 0);
  assert(draft.deletedBlocks.size === 0, 'fresh draft has no deleted blocks');
  assert(!isDirty(draft), 'fresh draft is not dirty');
  assert(serializeDraft(draft) === REAL_RAW, 'serializeDraft with zero deletions reproduces the fixture byte-for-byte');
}

// (b) Partial-line delete preserves surviving blocks' + siblings' bytes.
console.log('\n[delete — (b) partial-line delete preserves siblings byte-exact]');
{
  // thinking + tool_use + text on ONE line, plus a huge-int sibling field —
  // deleting just the thinking block must leave tool_use, text, and the
  // sibling field byte-identical.
  const BIG = '9223372036854775807';
  const rawLine = `{"type":"assistant","uuid":"multi-1","tokenCount":${BIG},"message":{"role":"assistant","content":[{"type":"thinking","thinking":"hmm"},{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}},{"type":"text","text":"here is the answer"}]}}`;
  let draft = buildDraft(rawLine + '\n', '/p', 0);
  const row = draft.rows['multi-1'];
  const thinkingKey = blockKey(row, 0);
  assert(!isBlockDeleted(draft, row, 0), 'thinking block starts undeleted');

  draft = deleteThinking(draft, thinkingKey);
  assert(isBlockDeleted(draft, draft.rows['multi-1'], 0), 'thinking block marked deleted');
  assert(isDirty(draft), 'draft is dirty after a delete');
  assert(draft.rows['multi-1'].value === rawLine, 'row VALUE is untouched by a delete (deletion lives in deletedBlocks, not the line text, until Save)');

  const out = serializeDraft(draft);
  const parsed = JSON.parse(out.trim());
  assert(parsed.tokenCount === undefined || String(parsed.tokenCount) === BIG || out.includes(`"tokenCount":${BIG}`),
    `huge-int sibling field survives exactly: ${out}`);
  assert(out.includes(`"tokenCount":${BIG}`), 'sibling scalar field byte-exact after partial delete');
  const content = parsed.message.content;
  assert(content.length === 2, `2 surviving blocks (got ${content.length})`);
  assert(content[0].type === 'tool_use' && content[0].id === 't1' && content[0].name === 'Bash', 'tool_use block survives untouched');
  assert(content[1].type === 'text' && content[1].text === 'here is the answer', 'text block survives untouched');
  assert(!out.includes('"type":"thinking"'), 'deleted thinking block is gone from the output');
}

// A line with ALL blocks deleted is dropped entirely.
console.log('\n[delete — a fully-deleted line is dropped entirely]');
{
  const rawLine = `{"type":"user","uuid":"solo-1","message":{"role":"user","content":"bye"}}`;
  let draft = buildDraft(rawLine + '\n', '/p', 0);
  draft = deleteMessage(draft, blockKey(draft.rows['solo-1'], 0));
  const out = serializeDraft(draft);
  assert(out === '\n' || out === '', `fully-deleted single-line file serializes to nothing but the trailing newline (got ${JSON.stringify(out)})`);
}

// (c) tool_use delete cascades to its tool_result — no orphan survives.
console.log('\n[delete — (c) tool_use ↔ tool_result cascade, no orphan]');
{
  let draft = buildDraft(REAL_RAW, '/fake/path.jsonl', 0);
  // toolu_ls1: tool_use on the row at originalIndex 4, tool_result on the
  // very next row (originalIndex 5) — see tests/mock_data/session.jsonl.
  const toolUseRowKey = draft.order.find((k) => draft.rows[k].originalIndex === 4);
  const toolUseKey = blockKey(draft.rows[toolUseRowKey], 0);

  draft = deleteToolGroup(draft, [toolUseKey]);
  assert(draft.deletedBlocks.size === 2, `deleting ONE half cascades to mark both blocks deleted (got ${draft.deletedBlocks.size})`);

  const out = serializeDraft(draft);
  assert(!out.includes('toolu_ls1'), 'no orphan: neither the tool_use nor its tool_result survives serialize');
  // Every OTHER tool_use/tool_result pair in the fixture is untouched.
  assert(out.includes('toolu_read1'), 'unrelated tool_use/tool_result pairs are untouched');

  // Undelete cascades the same way — restoring one half restores both, and
  // the draft goes back to a byte-identical serialize.
  draft = undelete(draft, [toolUseKey]);
  assert(draft.deletedBlocks.size === 0, 'undelete cascades back to zero deleted blocks');
  assert(serializeDraft(draft) === REAL_RAW, 'undelete fully restores byte-identical output');
}

// Deleting the tool_result half cascades to the tool_use half too (symmetry).
console.log('\n[delete — cascade is symmetric (deleting the RESULT half)]');
{
  let draft = buildDraft(REAL_RAW, '/fake/path.jsonl', 0);
  const toolResultRowKey = draft.order.find((k) => draft.rows[k].originalIndex === 5);
  const toolResultKey = blockKey(draft.rows[toolResultRowKey], 0);
  draft = deleteBulk(draft, [toolResultKey]);
  assert(draft.deletedBlocks.size === 2, 'deleting the tool_result half also cascades to its tool_use');
  const out = serializeDraft(draft);
  assert(!out.includes('toolu_ls1'), 'no orphan when deleting from the result side either');
}

// Deleting a text or thinking block never cascades (nothing to pair with).
console.log('\n[delete — text/thinking blocks never cascade]');
{
  let draft = buildDraft(REAL_RAW, '/fake/path.jsonl', 0);
  const userRowKey = draft.order.find((k) => draft.rows[k].originalIndex === 2);
  const textKey = blockKey(draft.rows[userRowKey], 0);
  draft = deleteMessage(draft, textKey);
  assert(draft.deletedBlocks.size === 1, `deleting a lone text block marks exactly 1 (got ${draft.deletedBlocks.size})`);
}

// setDeleted is the bare primitive — no cascade, mark or unmark directly.
console.log('\n[delete — setDeleted is a non-cascading primitive]');
{
  let draft = buildDraft(REAL_RAW, '/fake/path.jsonl', 0);
  const toolUseRowKey = draft.order.find((k) => draft.rows[k].originalIndex === 4);
  const toolUseKey = blockKey(draft.rows[toolUseRowKey], 0);
  draft = setDeleted(draft, [toolUseKey], true);
  assert(draft.deletedBlocks.size === 1, 'setDeleted marks only the given key, no cascade');
  draft = setDeleted(draft, [toolUseKey], false);
  assert(draft.deletedBlocks.size === 0, 'setDeleted unmarks cleanly');
}

// ── Block-index alignment: unhandled content types must NOT skew keys ────────
//
// Regression for the block-index-skew bug: the delete key is blockKey(row, i)
// where `i` is the block's position in the PARSED entry.blocks array (that's
// what every UI call site uses), while serializeDraft removes blocks by
// filtering message.content at that same `i`. If the parser drops any content
// element it doesn't model (image, redacted_thinking, server_tool_use, …),
// entry.blocks becomes shorter than content and the two index spaces diverge —
// deleting one block silently removes a DIFFERENT one on Save. The fix makes
// the parser index-preserving (a 'unknown' placeholder per unmodeled element),
// so entry.blocks is 1:1 with message.content. These tests derive the key the
// way the UI does — parse the line, find the target block's index in
// entry.blocks — so they'd fail against the skewed (pre-fix) parser.
console.log('\n[delete — unhandled content type does not skew the delete index]');
{
  // image (unmodeled) BEFORE text — pre-fix, text parses to index 0 and the
  // Save filter removes content[0] = the IMAGE. Post-fix, text is index 1.
  const IMG_TEXT_LINE = JSON.stringify({
    type: 'user',
    uuid: 'imgtext-1',
    message: {
      role: 'user',
      content: [
        { type: 'image', source: { type: 'base64', media_type: 'image/png', data: 'iVBORw0KGgo=' } },
        { type: 'text', text: 'keep me' },
      ],
    },
  });

  // Delete the TEXT — the image must survive byte-exact.
  {
    const entry = parseJsonl(IMG_TEXT_LINE)[0];
    assert(entry.blocks.length === 2, `parser keeps 1:1 with content (got ${entry.blocks.length} blocks for 2 content elements)`);
    assert(entry.blocks[0].blockType === 'unknown' && entry.blocks[0].rawType === 'image', 'unmodeled image element becomes an unknown placeholder carrying its raw type');
    const textBi = entry.blocks.findIndex((b) => b.blockType === 'text'); // UI-derived index
    assert(textBi === 1, `text block index in entry.blocks is 1, aligned with content (got ${textBi})`);

    let draft = buildDraft(IMG_TEXT_LINE + '\n', '/p', 0);
    draft = deleteMessage(draft, blockKey(draft.rows['imgtext-1'], textBi));
    const out = serializeDraft(draft);
    const parsed = JSON.parse(out.trim());
    assert(parsed.message.content.length === 1, `one surviving block (got ${parsed.message.content.length})`);
    assert(parsed.message.content[0].type === 'image', 'the IMAGE survives (not accidentally deleted in the text\'s place)');
    assert(out.includes('"data":"iVBORw0KGgo="'), 'image survives byte-exact');
    assert(!out.includes('keep me'), 'the deleted text is actually gone');
  }

  // Mirror: delete the IMAGE (an unknown block) — the text must survive.
  {
    const entry = parseJsonl(IMG_TEXT_LINE)[0];
    const imgBi = entry.blocks.findIndex((b) => b.blockType === 'unknown'); // UI-derived index
    let draft = buildDraft(IMG_TEXT_LINE + '\n', '/p', 0);
    // The editor routes non-text/non-thinking blocks (incl. unknown) through
    // deleteToolGroup; an unknown block has no tool pair so it deletes alone.
    draft = deleteToolGroup(draft, [blockKey(draft.rows['imgtext-1'], imgBi)]);
    assert(draft.deletedBlocks.size === 1, 'deleting a lone unknown block cascades to nothing');
    const out = serializeDraft(draft);
    const parsed = JSON.parse(out.trim());
    assert(parsed.message.content.length === 1 && parsed.message.content[0].type === 'text', 'the TEXT survives when the image is deleted');
    assert(parsed.message.content[0].text === 'keep me', 'surviving text is byte-exact');
    assert(!out.includes('iVBORw0KGgo='), 'the deleted image is actually gone');
  }
}

// "Delete group" on a member row containing an unknown block drops the whole
// line (all its blocks deleted → line removed), leaving no orphan.
console.log('\n[delete — delete-group over a row with an unknown block drops the line]');
{
  // Pure non-text line (a toolgroup member): thinking + an unmodeled
  // server_tool_use. Deleting the whole group marks BOTH → line dropped.
  const GROUP_LINE = JSON.stringify({
    type: 'assistant',
    uuid: 'grp-1',
    message: {
      role: 'assistant',
      content: [
        { type: 'thinking', thinking: 'pondering' },
        { type: 'server_tool_use', id: 'st1', name: 'web_search', input: { query: 'x' } },
      ],
    },
  });
  const KEEP_LINE = JSON.stringify({ type: 'user', uuid: 'keep-1', message: { role: 'user', content: 'still here' } });
  const raw = GROUP_LINE + '\n' + KEEP_LINE + '\n';

  const entry = parseJsonl(GROUP_LINE)[0];
  assert(entry.blocks.length === 2 && entry.blocks[1].blockType === 'unknown', 'server_tool_use is an unknown placeholder, 1:1 with content');

  let draft = buildDraft(raw, '/p', 0);
  // groupBlockKeys (UI): every block index of every member row.
  const row = draft.rows['grp-1'];
  const groupKeys = entry.blocks.map((_, bi) => blockKey(row, bi));
  draft = deleteToolGroup(draft, groupKeys);
  const out = serializeDraft(draft);
  assert(!out.includes('grp-1') && !out.includes('server_tool_use'), 'the whole group line is dropped (no orphan half survives)');
  assert(out.includes('still here'), 'the unrelated line is untouched');
  assert(out.split('\n').filter((l) => l.trim()).length === 1, 'exactly one line remains');
}

// ── Turn-level delete (issue #14 checkpoint 4) ───────────────────────────────
// Deleting a whole turn (deleteBulk over the span's block keys) then restoring
// it (undelete over the same keys) must round-trip serializeDraft back to the
// fixture byte-for-byte — the soft model gives this for free, but the cascade
// (an in-span tool_use pulling its tool_result) must also restore cleanly.
console.log('\n[delete — turn delete → undelete is an identity round-trip]');
{
  const draft0 = buildDraft(REAL_RAW, '/fake/path.jsonl', 0);
  const rr = renderableRows(draft0);
  const spans = deriveTurnSpans(rr.map((r) => ({ key: r.key, type: r.entry.type, hasText: r.hasText })));
  assert(spans.length >= 2, `fixture has multiple turns (got ${spans.length})`);

  // Pick a turn that actually contains a tool_use, so the round-trip also
  // exercises the pairing cascade inside the span.
  const rrMap = new Map(rr.map((r) => [r.key, r]));
  const turn = spans.find((s) =>
    s.keys.some((k) => rrMap.get(k)?.entry.blocks.some((b) => b.blockType === 'tool_use'))
  ) ?? spans[1];

  const bks = rowsBlockKeys(rr, turn.keys);
  assert(bks.length > 0, 'turn expands to at least one block key');

  let d = deleteBulk(draft0, bks);
  assert(isDirty(d), 'draft is dirty after a turn delete');
  assert(serializeDraft(d) !== REAL_RAW, 'a turn delete actually changes the serialized output');
  // The span's tool_use/tool_result pairs are self-contained (a tool_result is
  // a no-text user line, which never starts a new turn), so no orphaned half
  // survives the turn delete.
  for (const k of turn.keys) {
    const r = rrMap.get(k);
    if (!r) continue;
    for (const b of r.entry.blocks) {
      if (b.blockType === 'tool_use' && b.toolId) {
        assert(!serializeDraft(d).includes(b.toolId), `in-turn tool_use ${b.toolId} and its result are both gone (no orphan)`);
      }
    }
  }

  d = undelete(d, bks);
  assert(!isDirty(d), 'turn delete → undelete leaves the draft clean');
  assert(d.deletedBlocks.size === 0, 'no residual deleted blocks after undelete');
  assert(serializeDraft(d) === REAL_RAW, 'turn delete → undelete round-trips byte-for-byte');
}

// ── Bulk multi-select delete (issue #14 checkpoint 5) ────────────────────────
// deleteBulk over the union of a MIXED selection (a text bubble + a tool_use
// whose paired tool_result is NOT itself selected) must still remove the
// paired result — no orphan.
console.log('\n[delete — bulk delete over a mixed selection drops paired results, no orphan]');
{
  const draft0 = buildDraft(REAL_RAW, '/fake/path.jsonl', 0);
  const rr = renderableRows(draft0);
  const rrMap = new Map(rr.map((r) => [r.key, r]));

  // Unit A: the tool_use row at originalIndex 4 (toolu_ls1) — its tool_result
  // lives on the SEPARATE row at originalIndex 5, which we deliberately do NOT
  // select. Unit B: an unrelated assistant text bubble.
  const toolUseRow = rr.find((r) => r.row.originalIndex === 4);
  const textRow = rr.find((r) => r.hasText && r.entry.type === 'assistant');
  assert(!!toolUseRow && !!textRow, 'found a tool_use row and a text bubble to select');

  const selectionRowKeys = [toolUseRow.key, textRow.key];
  const unionBlockKeys = rowsBlockKeys(rr, selectionRowKeys);

  let d = deleteBulk(draft0, unionBlockKeys);
  const out = serializeDraft(d);
  assert(!out.includes('toolu_ls1'), 'the selected tool_use AND its non-selected paired tool_result are both gone (cascade)');
  // The selected text bubble's content is gone too.
  const textContent = textRow.entry.blocks.find((b) => b.blockType === 'text')?.text ?? '';
  if (textContent) {
    assert(!out.includes(textContent.slice(0, 24)), 'the selected text bubble is deleted');
  }
  // An unrelated pair is untouched.
  assert(out.includes('toolu_read1'), 'unrelated tool pairs survive a bulk delete');
}

// ── Summary ────────────────────────────────────────────────────────────────
console.log(`\n${'─'.repeat(50)}`);
if (failed === 0) {
  console.log(`All ${passed} assertions passed.`);
  process.exit(0);
} else {
  console.error(`${failed} FAILED, ${passed} passed.`);
  process.exit(1);
}
