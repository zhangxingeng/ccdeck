/**
 * Smoke test for resume.ts — the copyable resume command (issue #34).
 * Run with: npx tsx tests/resume_smoke.mjs  (from repo root)
 *
 * The terminal launcher and provider profiles were removed; Resume now surfaces
 * a single ready-to-paste line the user runs in their own terminal. This guards
 * its shape: `cd '<cwd>' && claude --resume '<id>'`, shell-quoted so a path or
 * id with spaces/quotes can't break the line, with the `cd` dropped when the cwd
 * is unknown.
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const __dir = dirname(fileURLToPath(import.meta.url));
const root = join(__dir, '..');

const { resumeCommand, sessionIdFromPath, shellQuote } = await import(
  join(root, 'src/lib/resume.ts')
);

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

const ID = 'a601b511-56ce-4b92-a3f0-7092553de44d';

// ── Test 1: the happy path — cd into the project, then resume ──────────────────
console.log('\n[resumeCommand — cwd + id]');
{
  const out = resumeCommand('/home/user/proj', ID);
  assert(
    out === `cd '/home/user/proj' && claude --resume '${ID}'`,
    'produces the paste-ready `cd <cwd> && claude --resume <id>` line'
  );
}

// ── Test 2: unknown cwd ⇒ just the resume command, no cd ───────────────────────
console.log('\n[resumeCommand — empty cwd drops the cd]');
{
  assert(resumeCommand('', ID) === `claude --resume '${ID}'`, 'omits cd when cwd is empty');
  assert(resumeCommand('   ', ID) === `claude --resume '${ID}'`, 'omits cd when cwd is whitespace-only');
}

// ── Test 3: shell-quoting — a path/id with spaces or quotes can't break out ─────
console.log('\n[resumeCommand — hostile cwd is shell-quoted]');
{
  const out = resumeCommand("/home/user/it's a proj", ID);
  // Single quotes inside a single-quoted string are escaped as '\'' .
  assert(
    out === `cd '/home/user/it'\\''s a proj' && claude --resume '${ID}'`,
    "an embedded single quote in the path is escaped, not left to break the line"
  );
  assert(shellQuote("a'b") === `'a'\\''b'`, 'shellQuote escapes embedded single quotes');
}

// ── Test 4: sessionIdFromPath strips the dir + .jsonl extension ─────────────────
console.log('\n[sessionIdFromPath — basename minus .jsonl]');
{
  assert(
    sessionIdFromPath(`/home/user/.claude/projects/-proj/${ID}.jsonl`) === ID,
    'returns the file stem (the real Claude session id), not the app path id'
  );
  assert(sessionIdFromPath(`${ID}.jsonl`) === ID, 'works on a bare filename too');
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
