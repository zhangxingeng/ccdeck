/**
 * Placeholder ({{token}}) handling — pure string transforms (F5).
 * The piece body is the single source of truth for placeholders; the
 * schema's `placeholders` array is derived from it at save time.
 */

/** Matches {{token}}. Token names: word chars plus - and ., no braces —
 *  surrounding whitespace inside the braces is tolerated and trimmed
 *  ({{ ticket }} names "ticket") so hand-edited JSON stays forgiving. */
const TOKEN_RE = /\{\{\s*([\w.-]+)\s*\}\}/g;

/** Ordered, de-duplicated token names in `body` (first-occurrence order). */
export function parsePlaceholders(body: string): string[] {
  const seen = new Set<string>();
  const names: string[] = [];
  for (const m of body.matchAll(TOKEN_RE)) {
    if (!seen.has(m[1])) {
      seen.add(m[1]);
      names.push(m[1]);
    }
  }
  return names;
}

/**
 * Substitute filled tokens into `body`. A token with no entry in `fills`
 * stays literal ({{token}}) — leaving it visible is the honest signal that
 * it wasn't filled, and Copy Prompt copies exactly what's visible. An empty
 * string IS a fill (the user chose to blank it).
 */
export function substitute(body: string, fills: Record<string, string>): string {
  return body.replace(TOKEN_RE, (whole, name: string) =>
    name in fills ? fills[name] : whole
  );
}

/** Template-mode "mark as placeholder": wrap [start, end) of `body` in a
 *  {{name}} token, replacing the selected text. */
export function markPlaceholder(body: string, start: number, end: number, name: string): string {
  return body.slice(0, start) + `{{${name.trim()}}}` + body.slice(end);
}

/** Template-mode "unmark": replace every {{name}} occurrence with plain
 *  `name` text (the inverse gesture of marking). */
export function unmarkPlaceholder(body: string, name: string): string {
  return body.replace(TOKEN_RE, (whole, n: string) => (n === name ? name : whole));
}

/** True if `name` is a usable token name (what markPlaceholder will accept). */
export function isValidTokenName(name: string): boolean {
  return /^[\w.-]+$/.test(name.trim());
}
