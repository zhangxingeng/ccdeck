/**
 * Prompt Library data model (issue #24) — mirrors src-tauri/src/prompts/
 * serde-default snake_case structs, exactly like SessionMeta does for
 * sessions. Pure TypeScript — no DOM, no Tauri, no Svelte.
 *
 * Contract: project_docs/prompts-design.md (§Piece schema, §Command contract).
 */

/** Where a piece is available: everywhere, or one project (by absolute cwd). */
export type PieceScope = { kind: 'global' } | { kind: 'project'; project: string };

/** A prior body pushed onto the history on body-changing save (newest-first).
 *  Product promise (issue #7 F7): a save never destroys the previous body. */
export interface PieceVersion {
  body: string;
  saved_at: number; // unix seconds
}

/** One reusable prompt fragment, stored as one hand-editable JSON file at
 *  ~/.ccdeck/prompts/<id>.json. Unknown extra fields in hand-edited files are
 *  preserved by the backend on round-trip; this interface only names the
 *  fields the UI reads. */
export interface Piece {
  id: string;
  title: string;
  body: string;
  keywords: string[];
  tags: string[];
  category: string | null;
  scope: PieceScope;
  /** Derived from {{token}} occurrences in body at save time (body is the
   *  single source of truth; this array exists so consumers don't re-parse). */
  placeholders: { name: string }[];
  created_at: number;
  updated_at: number;
  /** Newest-first, append-only on body change. */
  versions: PieceVersion[];
}

/** save_piece input: no `id` = create; `id` present = update (backend handles
 *  versioning). Derived fields (placeholders, timestamps, versions) are the
 *  backend's to compute — never sent. */
export interface PieceInput {
  id?: string;
  title: string;
  body: string;
  keywords: string[];
  tags: string[];
  category: string | null;
  scope: PieceScope;
}

/** One match_pieces result. Callers never know which engine ran beyond the
 *  provenance tag. */
export interface MatchHit {
  id: string;
  score: number;
  source: 'lexical' | 'semantic' | 'hybrid';
}

/** Embedding engine states: 'off' = downloaded but user-disabled;
 *  'ready' + enabled = hybrid matching on. */
export interface EmbedStatus {
  state: 'off' | 'not_downloaded' | 'downloading' | 'ready' | 'error';
  model_id: string;
  model_size_mb: number;
  error?: string;
}

/** Progress event streamed over the embed_download Channel. */
export interface EmbedProgress {
  downloaded_bytes: number;
  total_bytes: number;
}
