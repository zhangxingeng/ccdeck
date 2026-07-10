/**
 * Smoke test for the Prompt Library's pure logic (issue #24):
 * the compose-box provenance state machine (compose/doc.ts), the variable
 * grammar + copy output (compose/variables.ts — the Rust/TS seam, shared
 * vectors encoded verbatim), and copy flattening.
 * Run with: npx tsx tests/prompts_smoke.mjs   (from repo root)
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const __dir = dirname(fileURLToPath(import.meta.url));
const root = join(__dir, '..');

const {
  emptyDoc,
  docFromText,
  insertPiece,
  applyEdit,
  replaceSpan,
  linkRange,
  linkedSpanAt,
  spanText,
  flatten,
  caretQuery,
  diffTexts,
} = await import(join(root, 'src/lib/compose/doc.ts'));
const { parseVariables, copyText } = await import(join(root, 'src/lib/compose/variables.ts'));

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

/** Invariants that must hold after every transform. */
function checkInvariants(doc, msg) {
  const total = doc.spans.reduce((n, s) => n + s.length, 0);
  assert(total === doc.text.length, `${msg}: spans tile text (${total} != ${doc.text.length})`);
  assert(
    doc.spans.every((s) => s.length > 0),
    `${msg}: no zero-length spans`
  );
  for (let i = 1; i < doc.spans.length; i++) {
    assert(
      !(doc.spans[i - 1].state === 'typed' && doc.spans[i].state === 'typed'),
      `${msg}: adjacent typed spans merged`
    );
  }
  for (const s of doc.spans) {
    assert(
      (s.state === 'typed') === !s.link,
      `${msg}: link present iff not typed`
    );
  }
}

const link = (id = 'p1') => ({
  pieceId: id,
  title: id,
  scope: { kind: 'global' },
});

const states = (doc) => doc.spans.map((s) => s.state);

// ── span model: insertion ────────────────────────────────────────────────────
console.log('insertPiece');
{
  // Insert into empty doc.
  let d = insertPiece(emptyDoc(), 0, 'LINKED', link());
  checkInvariants(d, 'insert into empty');
  eq(states(d), ['linked'], 'empty doc: one linked span');
  eq(flatten(d), 'LINKED', 'flatten == visible text');

  // Insert at the middle of typed text: typed | linked | typed.
  d = insertPiece(docFromText('helloworld'), 5, 'LINKED', link());
  checkInvariants(d, 'insert mid-typed');
  eq(states(d), ['typed', 'linked', 'typed'], 'mid-typed split');
  eq(flatten(d), 'helloLINKEDworld', 'mid-typed text');

  // Insert at boundaries: no split.
  d = insertPiece(docFromText('abc'), 0, 'X', link());
  eq(states(d), ['linked', 'typed'], 'insert at start');
  d = insertPiece(docFromText('abc'), 3, 'X', link());
  eq(states(d), ['typed', 'linked'], 'insert at end');

  // Inserting into the middle of a linked span splits it into two
  // linked-modified halves (the original no longer appears intact).
  d = insertPiece(emptyDoc(), 0, 'AABB', link('outer'));
  d = insertPiece(d, 2, 'X', link('inner'));
  checkInvariants(d, 'insert into linked');
  eq(states(d), ['linked-modified', 'linked', 'linked-modified'], 'split linked -> modified halves');
  eq(d.spans[1].link.pieceId, 'inner', 'inner span links the inserted piece');
}

// ── span model: the inline-edit transition table ─────────────────────────────
console.log('applyEdit transitions');
{
  // Typing inside a linked span -> absorbed, linked-modified.
  let d = insertPiece(docFromText('ab'), 1, 'LINK', link());
  d = applyEdit(d, 3, 3, 'x'); // strictly inside the linked span [1,5)
  checkInvariants(d, 'type inside linked');
  eq(states(d), ['typed', 'linked-modified', 'typed'], 'interior typing modifies');
  eq(flatten(d), 'aLIxNKb', 'interior typing text');

  // Typing at a linked span's trailing edge -> typed text, span untouched.
  d = insertPiece(docFromText('ab'), 1, 'LINK', link());
  d = applyEdit(d, 5, 5, 'x'); // boundary between LINK and 'b'
  checkInvariants(d, 'type at edge');
  eq(states(d), ['typed', 'linked', 'typed'], 'edge typing stays typed');
  eq(flatten(d), 'aLINKxb', 'edge typing lands outside the span');

  // Deleting inside a linked span -> linked-modified.
  d = insertPiece(docFromText(''), 0, 'LINKED', link());
  d = applyEdit(d, 2, 4, '');
  checkInvariants(d, 'delete inside linked');
  eq(states(d), ['linked-modified'], 'interior deletion modifies');
  eq(flatten(d), 'LIED', 'interior deletion text');

  // Deleting a linked span exactly and fully -> span gone.
  d = insertPiece(docFromText('ab'), 1, 'LINK', link());
  d = applyEdit(d, 1, 5, '');
  checkInvariants(d, 'delete whole span');
  eq(states(d), ['typed'], 'whole-span deletion removes it');
  eq(flatten(d), 'ab', 'whole-span deletion text');

  // Replacing a whole linked span with typing -> typed (you replaced it).
  d = insertPiece(docFromText('ab'), 1, 'LINK', link());
  d = applyEdit(d, 1, 5, 'mine');
  checkInvariants(d, 'replace whole span');
  eq(states(d), ['typed'], 'whole-span replacement is typed');
  eq(flatten(d), 'amineb', 'whole-span replacement text');

  // A selection crossing a typed/linked boundary: linked part clipped ->
  // linked-modified, inserted text is typed.
  d = insertPiece(docFromText('abcd'), 2, 'LINK', link()); // ab LINK cd
  d = applyEdit(d, 1, 4, 'X'); // eats 'b' + 'LI'
  checkInvariants(d, 'cross-boundary edit');
  eq(states(d), ['typed', 'linked-modified', 'typed'], 'cross-boundary states');
  eq(flatten(d), 'aXNKcd', 'cross-boundary text');

  // Edits never mutate the input doc (pure transforms).
  const before = insertPiece(docFromText('ab'), 1, 'LINK', link());
  const snapshot = JSON.stringify(before);
  applyEdit(before, 2, 3, 'zz');
  eq(JSON.stringify(before), snapshot, 'applyEdit does not mutate its input');
}

// ── span model: replaceSpan / linkRange / linkedSpanAt ───────────────────────
console.log('replaceSpan / linkRange / linkedSpanAt');
{
  // Instance-mode Apply: replace the span's text, caller sets the state.
  let d = insertPiece(docFromText('ab'), 1, 'LINK', link());
  d = replaceSpan(d, 1, 'EDITED', { state: 'linked-modified', link: link() });
  checkInvariants(d, 'replaceSpan');
  eq(flatten(d), 'aEDITEDb', 'replaceSpan text');
  eq(spanText(d, 1), 'EDITED', 'replaceSpan spanText');
  eq(states(d), ['typed', 'linked-modified', 'typed'], 'replaceSpan state');

  // F4: a typed selection becomes a linked span; text unchanged.
  d = docFromText('reusable stuff here');
  d = linkRange(d, 0, 8, link('new-piece'));
  checkInvariants(d, 'linkRange');
  eq(flatten(d), 'reusable stuff here', 'linkRange keeps text');
  eq(states(d), ['linked', 'typed'], 'linkRange states');
  eq(d.spans[0].link.pieceId, 'new-piece', 'linkRange link identity');

  // Caret affordance: interior wins; boundary caret still finds the span.
  d = insertPiece(docFromText('ab'), 1, 'LINK', link('the-piece'));
  eq(linkedSpanAt(d, 3)?.span.link.pieceId, 'the-piece', 'caret interior');
  eq(linkedSpanAt(d, 5)?.span.link.pieceId, 'the-piece', 'caret at span end');
  eq(linkedSpanAt(d, 1)?.span.link.pieceId, 'the-piece', 'caret at span start');
  eq(linkedSpanAt(d, 0), null, 'caret in typed text -> null');
}

// ── copy flattening + raw-document integration ──────────────────────────────
console.log('copy flattening');
{
  // The doc holds RAW text — a piece body's {var} tokens land verbatim as a
  // linked span, unify with typed variables, and resolve only at copy.
  let d = docFromText('intro {ticket} ');
  d = insertPiece(d, d.text.length, 'Review {ticket:ABC-123} now.', link());
  d = applyEdit(d, d.text.length, d.text.length, ' outro');
  checkInvariants(d, 'mixed doc');
  eq(flatten(d), 'intro {ticket} Review {ticket:ABC-123} now. outro', 'flatten == raw visible text');
  eq(
    parseVariables(flatten(d)),
    [{ name: 'ticket' }],
    'one unified variable across typed + linked spans (first occurrence has no default)'
  );
  eq(
    copyText(flatten(d), { ticket: 'JUROR-412' }, false),
    'intro JUROR-412 Review JUROR-412 now. outro',
    'copy substitutes one fill into every occurrence'
  );
}

// ── variable grammar: the SHARED VECTORS (contract §Variable grammar) ────────
// Both sides (Rust + TS) assert all of these verbatim — the seam contract.
console.log('variable grammar (shared vectors)');
{
  eq(parseVariables('{task}'), [{ name: 'task' }], 'vector: {task} — variable, no default');
  eq(
    parseVariables('{task:write tests}'),
    [{ name: 'task', default: 'write tests' }],
    'vector: {task:write tests} — variable with default'
  );
  eq(parseVariables('{task:}'), [{ name: 'task', default: '' }], 'vector: {task:} — empty default');
  eq(
    parseVariables('{x:a:b}'),
    [{ name: 'x', default: 'a:b' }],
    'vector: {x:a:b} — first colon splits'
  );
  eq(parseVariables('{{task}}'), [], 'vector: {{task}} — escaped, no variable');
  eq(parseVariables('{"a": 1}'), [], 'vector: {"a": 1} — invalid name is literal');
  eq(parseVariables('{my var}'), [], 'vector: {my var} — space is literal');
  eq(parseVariables('{x-1_Y}'), [{ name: 'x-1_Y' }], 'vector: {x-1_Y} — hyphen/underscore/digit name');
  eq(parseVariables('{:x}'), [], 'vector: {:x} — empty name is literal');
  eq(
    parseVariables('{{{task}}}'),
    [{ name: 'task' }],
    'vector: {{{task}}} — literal { + variable + literal }'
  );
  eq(
    parseVariables('{x:a} {x:b}'),
    [{ name: 'x', default: 'a' }],
    'vector (rule 5): {x:a} {x:b} — one variable, first default wins'
  );
  eq(
    parseVariables('{x} {x:b}'),
    [{ name: 'x' }],
    'vector (rule 5, plain read): {x} {x:b} — first occurrence wins even with NO default'
  );
  eq(
    parseVariables('{a:{b}}'),
    [{ name: 'b' }],
    'vector: {a:{b}} — failed run consumes nothing; literal {a: + variable b + literal }'
  );
  eq(
    copyText('{a:{b}}', { b: 'X' }, false),
    '{a:X}',
    'vector: {a:{b}} substitute mode — inner variable fills, outer braces stay literal'
  );

  // Rule 4: one name = one variable across the whole document.
  eq(
    parseVariables('do {task} then {other} then {task:late}'),
    [{ name: 'task' }, { name: 'other' }],
    'dedupe keeps first-appearance order; later default never upgrades the first'
  );
}

// ── copy output (contract §Copy output) ──────────────────────────────────────
console.log('copy output');
{
  // The contract's own example, verbatim: ON mode dedups into a vars block.
  eq(
    copyText('Review the PR for {ticket:ABC-123} and summarize.', {}, true),
    'Review the PR for <prompt_var name="ticket"/> and summarize.\n\n' +
      '<prompt_vars>\n<prompt_var name="ticket">ABC-123</prompt_var>\n</prompt_vars>',
    'ON: contract example — occurrence element + appended block with default'
  );
  eq(
    copyText('do {x} and {x}', { x: 'A' }, true),
    'do <prompt_var name="x"/> and <prompt_var name="x"/>\n\n' +
      '<prompt_vars>\n<prompt_var name="x">A</prompt_var>\n</prompt_vars>',
    'ON: repeated occurrences, one block entry, user fill wins'
  );
  eq(
    copyText('need {x}', {}, true),
    'need <prompt_var name="x"/>\n\n<prompt_vars>\n<prompt_var name="x"></prompt_var>\n</prompt_vars>',
    'ON: unfilled + no default = empty element (honest fill-me signal)'
  );
  eq(copyText('plain {{json}} body', {}, true), 'plain {json} body', 'ON: no variables, no block, escapes resolved');

  // OFF mode: substitute in place.
  eq(copyText('do {task:write tests}!', {}, false), 'do write tests!', 'OFF: default substitutes');
  eq(copyText('do {task:write tests}!', { task: 'ship' }, false), 'do ship!', 'OFF: fill beats default');
  eq(copyText('keep {x}', {}, false), 'keep {x}', 'OFF: unfilled, no default — literal stays visible');
  eq(copyText('blank {task:}!', {}, false), 'blank !', 'OFF: {task:} fills as empty when unfilled');
  eq(copyText('{x:a} {x:b}', {}, false), 'a a', 'OFF (rule 5): first default serves every occurrence');
  eq(copyText('{{literal}} and {v:x}', {}, false), '{literal} and x', 'OFF: escapes resolve on copy');
  eq(copyText('', {}, true), '', 'empty document copies empty');
}

// ── editor plumbing: caretQuery + diffTexts ──────────────────────────────────
console.log('caretQuery / diffTexts');
{
  eq(caretQuery('hello\nreview this', 17), 'review this', 'query is the current line to caret');
  eq(caretQuery('hello\nreview this', 5), 'hello', 'first line');
  eq(caretQuery('abc', 0), '', 'caret at start -> empty');

  eq(diffTexts('abc', 'abXc'), { start: 2, end: 2, inserted: 'X' }, 'diff insertion');
  eq(diffTexts('abXc', 'abc'), { start: 2, end: 3, inserted: '' }, 'diff deletion');
  eq(diffTexts('abc', 'aZc'), { start: 1, end: 2, inserted: 'Z' }, 'diff replacement');
  eq(diffTexts('abc', 'abc'), null, 'diff equal -> null');
  eq(diffTexts('', 'abc'), { start: 0, end: 0, inserted: 'abc' }, 'diff from empty');
  // Ambiguous repeat: deterministic earliest attribution, text still correct.
  const dd = diffTexts('aab', 'aaab');
  eq('aab'.slice(0, dd.start) + dd.inserted + 'aab'.slice(dd.end), 'aaab', 'diff round-trips');
}

if (failures > 0) {
  console.error(`\nprompts_smoke: ${failures} failure(s)`);
  process.exit(1);
}
console.log('\nprompts_smoke: all assertions passed');
