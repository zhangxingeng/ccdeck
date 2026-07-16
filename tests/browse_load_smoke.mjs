/**
 * Smoke test for browseLoad.ts — the pure glue between the two browse-loading
 * tiers (stat-only stubs + streamed enrichment).
 * Run with: npx tsx tests/browse_load_smoke.mjs   (from repo root)
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const __dir = dirname(fileURLToPath(import.meta.url));
const root = join(__dir, '..');

const { stubToMeta, applyEnrichment } = await import(join(root, 'src/lib/browseLoad.ts'));

let passed = 0, failed = 0;
function assert(cond, msg) {
  if (cond) { console.log(`  ok  ${msg}`); passed++; }
  else { console.error(`  FAIL ${msg}`); failed++; }
}

const stub = (path, mtime = 100) => ({
  id: path, path, project_raw: '-home-u-app', mtime, size: 42,
});
const enrichment = (path, over = {}) => ({
  path, cleaned: false, preview: ['{"type":"user"}'], line_count: 3,
  user_count: 2, assistant_count: 1, subagent_count: 0, models: ['claude'],
  first_ts: 't1', last_ts: 't2', cwd: '/home/u/app', custom_title: '', ...over,
});

// ── stubToMeta: a stub inflates to an un-enriched row with empty content ─────
{
  const m = stubToMeta(stub('/a.jsonl', 55));
  assert(m.enriched === false, 'inflated stub starts un-enriched');
  assert(m.mtime === 55 && m.size === 42, 'stat fields are carried through');
  assert(m.cwd === '' && m.preview.length === 0 && m.user_count === 0,
    'content fields start empty (so projectLabel falls back to the dir name)');
}

// ── applyEnrichment: a normal patch fills content and flips `enriched` ────────
{
  const map = new Map([['/a.jsonl', stubToMeta(stub('/a.jsonl'))]]);
  const changed = applyEnrichment(map, enrichment('/a.jsonl', { custom_title: 'My chat' }));
  const row = map.get('/a.jsonl');
  assert(changed === true, 'a real patch reports the map changed');
  assert(row.enriched === true, 'patched row is marked enriched');
  assert(row.user_count === 2 && row.cwd === '/home/u/app' && row.custom_title === 'My chat',
    'content fields are populated from the enrichment');
  assert(row.mtime === 100 && row.size === 42, 'the stub stat fields survive the patch');
}

// ── applyEnrichment: a `cleaned` payload drops the row entirely ───────────────
{
  const map = new Map([['/junk.jsonl', stubToMeta(stub('/junk.jsonl'))]]);
  const changed = applyEnrichment(map, enrichment('/junk.jsonl', { cleaned: true }));
  assert(changed === true, 'a cleaned payload reports a change');
  assert(!map.has('/junk.jsonl'), 'the cleaned file is removed from the map');
}

// ── applyEnrichment: an unknown path is ignored, never inserted ───────────────
{
  const map = new Map();
  const changed = applyEnrichment(map, enrichment('/ghost.jsonl'));
  assert(changed === false, 'an enrichment with no stub reports no change');
  assert(map.size === 0, 'no phantom row is inserted for an unknown path');

  // ...and a cleaned signal for an already-absent path is likewise a no-op.
  const changed2 = applyEnrichment(map, enrichment('/ghost.jsonl', { cleaned: true }));
  assert(changed2 === false, 'cleaning an absent path is a no-op');
}

console.log('');
if (failed > 0) {
  console.error(`browse_load_smoke: ${failed} assertion(s) failed`);
  process.exit(1);
}
console.log(`browse_load_smoke: all ${passed} assertions passed`);
