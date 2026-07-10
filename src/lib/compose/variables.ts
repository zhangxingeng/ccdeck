/**
 * Variable grammar + copy-output builders — the shared seam spec from
 * project_docs/prompts-design.md (§Variable grammar, §Copy output). Rust and
 * TS MUST implement this identically; both sides encode the shared test
 * vectors verbatim (tests/prompts_smoke.mjs here). Pure string transforms —
 * no DOM, no Svelte.
 *
 * Grammar (single-brace, python-f-string flavored), scanning left-to-right:
 *   1. `{{` emits literal `{`; `}}` emits literal `}` (escapes consume first).
 *   2. `{name}` / `{name:default}` is a variable when name matches
 *      [A-Za-z0-9_-]+ (case-sensitive). First `:` splits name from default;
 *      the default runs to the closing `}` and may not contain braces.
 *   3. Any other braced run stays verbatim — JSON in prompt bodies never
 *      parses as a variable.
 *   4. The same name is the same variable everywhere in a document — one fill
 *      serves every occurrence.
 *   5. Duplicate names with differing defaults: the FIRST occurrence's
 *      default wins (consistent with first-appearance ordering everywhere).
 */

/** One distinct variable: first-appearance order; `default` is whatever the
 *  first occurrence declared — `undefined` when it declared none (rule 5 is
 *  strictly first-occurrence, so a later `{x:b}` never upgrades a first
 *  `{x}`), `''` for the explicit empty default (`{x:}` fills as empty). */
export interface Variable {
  name: string;
  default?: string;
}

/** A lexed run of the document. Literal text carries escapes RESOLVED
 *  (`{{` → `{`) — copy always resolves escapes, and nothing else consumes
 *  the token stream. */
type Token =
  | { kind: 'literal'; text: string }
  | { kind: 'variable'; name: string; default?: string };

/** Matches a variable at the start of the slice (after escape handling). */
const VAR_AT = /^\{([A-Za-z0-9_-]+)(?::([^{}]*))?\}/;

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
      literal += text[i];
      i += 2;
      continue;
    }
    if (text[i] === '{') {
      const m = VAR_AT.exec(text.slice(i));
      if (m) {
        flush();
        tokens.push({ kind: 'variable', name: m[1], default: m[2] });
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

/** Distinct variables in `text`, first-appearance order (rule 4/5: one entry
 *  per name; the first occurrence fixes the default). */
export function parseVariables(text: string): Variable[] {
  const seen = new Set<string>();
  const vars: Variable[] = [];
  for (const t of scan(text)) {
    if (t.kind === 'variable' && !seen.has(t.name)) {
      seen.add(t.name);
      vars.push({ name: t.name, default: t.default });
    }
  }
  return vars;
}

/** A variable's effective value: user fill (empty input = untouched, the
 *  default applies), else the unified default, else undefined ("unfilled"). */
function resolve(
  variable: Variable,
  fills: Record<string, string>
): string | undefined {
  const filled = fills[variable.name];
  if (filled !== undefined && filled !== '') return filled;
  return variable.default;
}

/**
 * Copy Prompt output (§Copy output). Escapes always resolve. `asVariable` ON
 * (dedup mode): occurrences become `<prompt_var name="x"/>` and a
 * `<prompt_vars>` block appends after the body, one entry per distinct
 * variable in first-appearance order — an empty element is the honest "fill
 * me" signal. OFF (substitute in place): each occurrence becomes the value,
 * else the literal `{x}` stays visible — never silently blanked.
 */
export function copyText(
  text: string,
  fills: Record<string, string>,
  asVariable: boolean
): string {
  const vars = parseVariables(text);
  const byName = new Map(vars.map((v) => [v.name, v]));
  const out: string[] = [];
  for (const t of scan(text)) {
    if (t.kind === 'literal') {
      out.push(t.text);
      continue;
    }
    const variable = byName.get(t.name);
    if (!variable) continue; // unreachable: scan produced the name
    if (asVariable) {
      out.push(`<prompt_var name="${t.name}"/>`);
    } else {
      const value = resolve(variable, fills);
      out.push(value !== undefined ? value : `{${t.name}}`);
    }
  }
  if (asVariable && vars.length) {
    const entries = vars.map((v) => {
      const value = resolve(v, fills) ?? '';
      return `<prompt_var name="${v.name}">${value}</prompt_var>`;
    });
    out.push(`\n\n<prompt_vars>\n${entries.join('\n')}\n</prompt_vars>`);
  }
  return out.join('');
}
