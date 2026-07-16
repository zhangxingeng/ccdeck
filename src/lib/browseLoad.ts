/**
 * Browse-list loading: the pure glue between the two backend tiers.
 *
 * `list_sessions` returns stat-only stubs (instant first paint); `enrich_sessions`
 * streams the content-derived fields per session afterward. This module holds the
 * two pure transforms that sit between them — inflating a stub into a display row,
 * and folding one streamed enrichment into the row map — so the merge invariants
 * (a `cleaned` payload drops the row; an unknown path is ignored; a patch flips
 * `enriched`) are unit-testable without a component or the DOM.
 */
import type { SessionStub, SessionMeta, SessionEnrichment } from './types';

/**
 * Inflate a tier-1 stub into a browse row whose content fields are empty and
 * `enriched` is false. `enrich_sessions` patches these in; until then `cwd` /
 * `preview` / counts are genuinely empty (so, e.g., `projectLabel` falls back
 * to the decoded dir name rather than reading a stale value).
 */
export function stubToMeta(s: SessionStub): SessionMeta {
  return {
    ...s,
    enriched: false,
    preview: [],
    line_count: 0,
    user_count: 0,
    assistant_count: 0,
    subagent_count: 0,
    models: [],
    first_ts: '',
    last_ts: '',
    cwd: '',
    custom_title: '',
  };
}

/**
 * Fold one streamed enrichment into the row map (mutating it in place):
 *  - `cleaned: true` → the file was auto-removed as junk; drop its row.
 *  - otherwise → patch the matching row's content fields and set `enriched`.
 *  - a path with no existing stub (a file that appeared after the tier-1 scan)
 *    is ignored rather than inserted, since we have no stat metadata for it.
 *
 * Returns whether the map changed, so the caller only re-renders on a real edit.
 */
export function applyEnrichment(
  map: Map<string, SessionMeta>,
  e: SessionEnrichment
): boolean {
  if (e.cleaned) {
    return map.delete(e.path);
  }
  const cur = map.get(e.path);
  if (!cur) return false;
  map.set(e.path, {
    ...cur,
    enriched: true,
    preview: e.preview,
    line_count: e.line_count,
    user_count: e.user_count,
    assistant_count: e.assistant_count,
    subagent_count: e.subagent_count,
    models: e.models,
    first_ts: e.first_ts,
    last_ts: e.last_ts,
    cwd: e.cwd,
    custom_title: e.custom_title,
  });
  return true;
}
