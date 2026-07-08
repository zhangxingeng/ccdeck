/**
 * Smoke test for resume.ts — resumeCommand with a provider profile (issue #21).
 * Run with: npx tsx tests/resume_provider_smoke.mjs  (from repo root)
 *
 * Guards the write-only posture: the clipboard fallback must carry the
 * provider's base_url (and model when set) but NEVER a real key — only a masked
 * placeholder. And the no-provider path must stay byte-identical to before.
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const __dir = dirname(fileURLToPath(import.meta.url));
const root = join(__dir, '..');

const { resumeCommand } = await import(join(root, 'src/lib/resume.ts'));

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

const CWD = '/home/user/proj';
const ID = 'a601b511-56ce-4b92-a3f0-7092553de44d';
const TITLE = 'My Session';

// ── Test 1: no provider ⇒ byte-identical to the legacy 5-line command ─────────
console.log('\n[resumeCommand — no provider path is unchanged]');
{
  const out = resumeCommand(CWD, ID, TITLE, '');
  const expected = [
    `export CCDECK_SESSION_ID='${ID}'`,
    `export CCDECK_SESSION_TITLE='My Session'`,
    `export CCDECK_CWD='/home/user/proj'`,
    `cd '/home/user/proj' &&`,
    `claude --resume "$CCDECK_SESSION_ID"`,
  ].join('\n');
  assert(out === expected, 'no-provider output is byte-identical to the legacy command');
  assert(!out.includes('ANTHROPIC_'), 'no ANTHROPIC_* exports leak when no provider is passed');
}

// ── Test 2: provider with model ⇒ masked key + base_url + model, right order ───
console.log('\n[resumeCommand — provider path masks the key]');
{
  const provider = {
    name: 'DeepSeek',
    baseUrl: 'https://api.deepseek.com/anthropic',
    defaultModel: 'deepseek-chat',
  };
  const out = resumeCommand(CWD, ID, TITLE, '', provider);

  assert(
    out.includes(`export ANTHROPIC_BASE_URL='https://api.deepseek.com/anthropic'`),
    'exports ANTHROPIC_BASE_URL'
  );
  assert(
    out.includes(`export ANTHROPIC_MODEL='deepseek-chat'`),
    'exports ANTHROPIC_MODEL when defaultModel set'
  );
  // The key is MASKED — a placeholder hint, never a real secret.
  assert(
    out.includes(`export ANTHROPIC_AUTH_TOKEN='<paste your DeepSeek key>'`),
    'ANTHROPIC_AUTH_TOKEN is the masked placeholder'
  );
  assert(!out.includes('sk-'), 'no real-key-shaped value appears in the command');

  // Order: CCDECK_* first, then ANTHROPIC_* (base_url, token, model), then cd.
  const cwdIdx = out.indexOf('export CCDECK_CWD=');
  const baseIdx = out.indexOf('export ANTHROPIC_BASE_URL=');
  const tokenIdx = out.indexOf('export ANTHROPIC_AUTH_TOKEN=');
  const modelIdx = out.indexOf('export ANTHROPIC_MODEL=');
  const cdIdx = out.indexOf('cd ');
  assert(cwdIdx < baseIdx, 'ANTHROPIC_BASE_URL comes after CCDECK_CWD');
  assert(baseIdx < tokenIdx && tokenIdx < modelIdx, 'order is base_url → token → model');
  assert(modelIdx < cdIdx, 'all provider exports come before cd');
}

// ── Test 3: provider without model ⇒ no ANTHROPIC_MODEL line ───────────────────
console.log('\n[resumeCommand — provider without a default model]');
{
  const provider = { name: 'Local', baseUrl: 'http://localhost:8080' };
  const out = resumeCommand(CWD, ID, TITLE, '', provider);
  assert(out.includes(`export ANTHROPIC_BASE_URL='http://localhost:8080'`), 'base_url present');
  assert(!out.includes('ANTHROPIC_MODEL'), 'no ANTHROPIC_MODEL when model is unset');
  assert(
    out.includes(`export ANTHROPIC_AUTH_TOKEN='<paste your Local key>'`),
    'masked key placeholder uses the profile name'
  );
}

// ── Test 4: empty/blank model string ⇒ still no ANTHROPIC_MODEL line ───────────
console.log('\n[resumeCommand — blank model string is treated as unset]');
{
  const out = resumeCommand(CWD, ID, TITLE, '', { name: 'X', baseUrl: 'https://x', defaultModel: '   ' });
  assert(!out.includes('ANTHROPIC_MODEL'), 'whitespace-only model does not emit ANTHROPIC_MODEL');
}

// ── Test 5: custom launch command is preserved on the provider path ────────────
console.log('\n[resumeCommand — provider path keeps the custom launch command]');
{
  const custom = 'tmux new-session -A -s "$CCDECK_SESSION_TITLE" "claude --resume $CCDECK_SESSION_ID"';
  const out = resumeCommand(CWD, ID, TITLE, custom, { name: 'DS', baseUrl: 'https://x' });
  assert(out.includes(custom), 'custom launch command still emitted verbatim as the last line');
  assert(out.trimEnd().endsWith(custom), 'launch command is the final line, after the exports + cd');
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
