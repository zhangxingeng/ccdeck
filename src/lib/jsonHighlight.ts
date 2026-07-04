/**
 * Lightweight, dependency-free JSON syntax highlighting for tool_use inputs.
 * Tokenizes the already-prettified JSON.stringify(...,null,2) text via a
 * single regex pass rather than a real parser — matches this project's
 * minimal-deps ethos (see Cargo.toml's no-FTS5/no-clock-feature comments).
 */

function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) =>
    c === '&' ? '&amp;' : c === '<' ? '&lt;' : c === '>' ? '&gt;' : c === '"' ? '&quot;' : '&#39;'
  );
}

const TOKEN_RE =
  /("(?:\\u[0-9a-fA-F]{4}|\\[^u]|[^\\"])*"(\s*:)?|\b(?:true|false|null)\b|-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)/g;

/** Returns HTML with each token wrapped in a `<span class="jt-*">`. */
export function highlightJson(value: unknown): string {
  let text: string;
  try {
    text = JSON.stringify(value, null, 2) ?? 'null';
  } catch {
    text = String(value);
  }
  return text.replace(TOKEN_RE, (match) => {
    const esc = escapeHtml(match);
    if (match.startsWith('"')) {
      return /:\s*$/.test(match) ? `<span class="jt-key">${esc}</span>` : `<span class="jt-str">${esc}</span>`;
    }
    if (match === 'true' || match === 'false') return `<span class="jt-bool">${esc}</span>`;
    if (match === 'null') return `<span class="jt-null">${esc}</span>`;
    return `<span class="jt-num">${esc}</span>`;
  });
}

function looksLikeMarkdown(s: string): boolean {
  return /^#{1,6}\s|\*\*[^*]+\*\*|`{1,3}[^`]+`{1,3}|^[-*]\s|^\d+\.\s|\[[^\]]+\]\([^)]+\)/m.test(s);
}

/** Long or markdown-flavored string values are worth an inline raw/rendered
 *  toggle in the tool-input popover rather than sitting as an unreadable
 *  single-line JSON string. */
export function isLongMarkdownish(value: unknown): value is string {
  return typeof value === 'string' && (value.length > 200 || (value.includes('\n') && looksLikeMarkdown(value)));
}
