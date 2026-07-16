/**
 * The variable grammar + copy-output builder. As of v0.13 this is the ONE AND
 * ONLY implementation — the Rust half (`prompts/grammar.rs`) is deleted, because
 * after the schema cut nothing in the backend parses variables. There is no
 * second implementation to drift from, and so no cross-language vector table to
 * keep in sync; the vectors live in tests/prompts_smoke.mjs, one copy.
 *
 * That also means there is no safety net: nothing else will catch a mistake in
 * here.
 *
 * ── The grammar, in one sentence ─────────────────────────────────────────────
 *
 * **A snippet body is a Python format string.** If Python would parse it as a
 * replacement field, it is a variable; if not, it is literal text. That is the
 * whole rule, and it is deliberately a rule the user already knows.
 *
 *   1. `{name}` is a variable, where name is [A-Za-z0-9_-]+ (case-sensitive).
 *   2. `{{` emits a literal `{`; `}}` emits a literal `}`. To get a literal
 *      `{{`, write `{{{{` — exactly as in Python.
 *   3. Anything else braced is literal, because Python could not read it as a
 *      plain field either: `{my var}`, `{a.b}`, `{:x}`, `{"json": 1}`,
 *      `{ return x }`, and `{task:write tests}` (the removed default form) all
 *      simply fail rule 1's name test. This is not a list of exceptions — it is
 *      rule 1, seen from the other side. Degrading the removed default form to
 *      VISIBLE literal text is the point: the user sees the stray text and fixes
 *      it, where a silent reinterpretation would quietly swallow what they wrote.
 *   4. One name = one variable, document-wide. First-appearance order; repeats
 *      dedupe. The model cannot tell two identically-named variables apart, so
 *      pretending they differ would be a fiction the UI maintains and the output
 *      discards.
 *   5. An unfilled variable resolves to the literal sentinel
 *      `variable not set, ask user for it`. A forgotten variable therefore still
 *      produces a working prompt: the model asks, rather than silently
 *      receiving a blank or a stray `{placeholder}`.
 *   6. Every variable is always hoisted on copy: `{name}` becomes
 *      `<prompt_var name="name"/>` in place, and one `<prompt_vars>` block,
 *      appended at the end, carries each distinct name's value once. Round 1
 *      had a per-variable as-variable toggle; round 2 cut it — a control nobody
 *      flipped is the archetype of the forgotten feature this whole redesign
 *      exists to delete. See `copyText` below.
 *
 * ── There is no Markdown awareness. Do not add any. ──────────────────────────
 *
 * The grammar is UNIFORM over the whole document. It does not know what a code
 * fence is, or a backtick. A `{name}` inside ```-fenced code IS a variable.
 *
 * An earlier draft excluded fenced blocks and inline code spans, to stop a code
 * sample's braces from parsing. It was cut on purpose. "Variables work
 * everywhere, except inside backticks, and except inside fences" is a rule you
 * have to be TOLD — and being unguessable without having read a contract is the
 * exact disease this round exists to cure. "It's a Python format string" is a
 * rule the user already knows, and so does every LLM reading the output. Less to
 * remember beats more-correct-in-a-corner. We do not invent protocols.
 *
 * The cost is accepted knowingly, and it is LOUD rather than silent: a fenced
 * code sample containing `{name}` does become a variable — and the user SEES it,
 * because the chip renders the variable names it contains and the fill list lists
 * them. A stray `name` appears, and they escape it as `{{name}}`, exactly as they
 * would in Python. The UI surfacing every parsed variable is what makes this
 * safe.
 *
 * ⚠ The one case that IS silent, named here so nobody "fixes" it back into a
 * carve-out: a body containing `{{` inside a code sample — say a Rust
 * `format!("{{}}")` — unescapes to `format!("{}")` on copy. That is not a bug.
 * Under Python semantics `{{` MEANS a literal brace, so unescaping it is correct,
 * and a user who wants a literal `{{` writes `{{{{` — again, exactly as in
 * Python. Re-introducing a fence carve-out to "protect" this would trade one
 * quiet surprise for an unguessable rule, which is the worse trade.
 */

/** What an unfilled variable becomes on copy (rule 5). */
export const UNSET_VALUE = 'variable not set, ask user for it';

/** One distinct variable: a name, and nothing else. Every variable is a string,
 *  and none carries a default — they all collapsed into UNSET_VALUE. */
export interface Variable {
  name: string;
}

/** A lexed run. A `literal` token is ready to emit — escapes already resolved. */
type Token = { kind: 'literal'; text: string } | { kind: 'variable'; name: string };

/** A variable at the start of the slice. This regex IS rule 3: everything that
 *  "stays literal" does so by failing to match here, not by a carve-out. The
 *  absence of a `:` branch is what makes `{task:write tests}` fall through to
 *  prose — the removed default form needs no special case. */
const VAR_AT = /^\{([A-Za-z0-9_-]+)\}/;

/** The token stream: one uniform left-to-right pass over the whole document. No
 *  Markdown awareness, by design — see the module header before adding any. */
function scan(text: string): Token[] {
  const tokens: Token[] = [];
  let literal = '';
  const flush = (): void => {
    if (literal) {
      tokens.push({ kind: 'literal', text: literal });
      literal = '';
    }
  };

  let i = 0;
  while (i < text.length) {
    const pair = text.slice(i, i + 2);
    if (pair === '{{' || pair === '}}') {
      literal += text[i]; // `{{` → `{`, `}}` → `}` (an escape consumes both chars)
      i += 2;
      continue;
    }

    if (text[i] === '{') {
      const m = VAR_AT.exec(text.slice(i));
      if (m) {
        flush();
        tokens.push({ kind: 'variable', name: m[1] });
        i += m[0].length;
        continue;
      }
    }

    literal += text[i];
    i++;
  }
  flush();
  return tokens;
}

/** Distinct variables in `text`, first-appearance order (rule 4). */
export function parseVariables(text: string): Variable[] {
  const seen = new Set<string>();
  const vars: Variable[] = [];
  for (const t of scan(text)) {
    if (t.kind === 'variable' && !seen.has(t.name)) {
      seen.add(t.name);
      vars.push({ name: t.name });
    }
  }
  return vars;
}

/** A variable's effective value. An empty input reads as untouched, so it
 *  resolves to the sentinel exactly as an absent one does (rule 5). There is
 *  deliberately no way to fill a variable with the empty string — to say
 *  nothing, delete the `{name}`. */
function resolve(name: string, fills: Record<string, string>): string {
  const filled = fills[name];
  return filled !== undefined && filled !== '' ? filled : UNSET_VALUE;
}

/** XML-escape a value interpolated into the <prompt_vars> block: the wrapper
 *  form exists to be parseable, and an unescaped value containing
 *  `</prompt_var>` would inject phantom variables into what the reading LLM
 *  sees. `&` first — escaping it later would re-escape the entities just
 *  produced. Names need no escaping: rule 1's name class is attribute-safe by
 *  construction. */
function escapeXml(value: string): string {
  return value.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
}

/**
 * The Copy Prompt output.
 *
 * Round 2 cut the per-variable as-variable toggle (a control nobody flipped —
 * the archetype of the forgotten feature this whole redesign exists to
 * delete). Every variable is now always hoisted: every occurrence becomes
 * `<prompt_var name="x"/>`, and one `<prompt_vars>` block, appended at the end,
 * carries each distinct name's value once, in first-appearance order,
 * XML-escaped.
 *
 * An unfilled variable resolves to UNSET_VALUE (rule 5) — a forgotten variable
 * still produces a working prompt, the model just asks.
 */
export function copyText(text: string, fills: Record<string, string>): string {
  const out: string[] = [];
  for (const t of scan(text)) {
    if (t.kind === 'literal') out.push(t.text);
    else out.push(`<prompt_var name="${t.name}"/>`);
  }

  const vars = parseVariables(text);
  if (vars.length) {
    const entries = vars.map(
      (v) => `<prompt_var name="${v.name}">${escapeXml(resolve(v.name, fills))}</prompt_var>`
    );
    out.push(`\n\n<prompt_vars>\n${entries.join('\n')}\n</prompt_vars>`);
  }
  return out.join('');
}
