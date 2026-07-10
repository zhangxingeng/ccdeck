/**
 * Prompt Library data model (issue #24) — mirrors the CONTRACT
 * (project_docs/prompts-design.md §Project model, §Piece schema, §Command
 * contract), not the Rust source: the contract is the seam both sides build
 * against. Pure TypeScript — no DOM, no Tauri, no Svelte.
 */

/** The fixed project palette — keys only, never hexes: each key maps to a
 *  --project-<key> token in app.css (light + dark), so stored data carries
 *  intent and the theme file owns the hue. */
export const PALETTE_KEYS = [
  'red',
  'orange',
  'yellow',
  'green',
  'teal',
  'blue',
  'purple',
  'pink',
  'graphite',
] as const;
export type PaletteKey = (typeof PALETTE_KEYS)[number];

/** A named, colored grouping for pieces — the unit tabs, the compose-box
 *  tint, and piece-span hues key off. */
export interface Project {
  readonly id: string;
  name: string;
  color: PaletteKey;
  /** true renders the project as a tab atop the Prompts view. */
  pinned: boolean;
  /** Optional absolute project dir for future auto-scoping — no behavior
   *  hangs on it this round. */
  path: string | null;
  readonly created_at: number;
}

/** save_project input: no `id` = create; `id` present = update. */
export interface ProjectInput {
  id?: string;
  name: string;
  color: PaletteKey;
  pinned: boolean;
  path: string | null;
}

/** Where a piece is available: everywhere, or one project (by roster id).
 *  Legacy/unknown scope shapes load as global + a piece_load_errors entry —
 *  the backend owns that fallback; this type only names the v2 shape. */
export type PieceScope = { kind: 'global' } | { kind: 'project'; project_id: string };

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
  /** Derived from the variable grammar at save time (body is the single
   *  source of truth; this array exists so consumers don't re-parse). */
  placeholders: { name: string; default?: string }[];
  created_at: number;
  updated_at: number;
  /** Newest-first, append-only on body change. */
  versions: PieceVersion[];
  /** Transient, never written to disk: the loader repaired this piece from
   *  invalid JSON in memory — it needs attention (the repaired form persists
   *  only on the user's next explicit save of the piece). */
  recovered?: boolean;
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

/** A piece JSON file that failed to load cleanly on the last scan: broken
 *  JSON that repair could not recover, shadowed duplicate ids, legacy scope
 *  fallbacks. Pieces are hand-editable by design (F7) — without this surface
 *  a broken file silently vanishes from the library, which reads as data
 *  loss to exactly the hand-editing persona the feature bets on. */
export interface PieceLoadError {
  file: string;
  error: string;
}

/** Embedding engine states: 'off' = downloaded but user-disabled;
 *  'ready' + enabled = hybrid matching on. */
export interface EmbedStatus {
  state: 'off' | 'not_downloaded' | 'downloading' | 'ready' | 'error';
  model_id: string;
  model_size_mb: number;
  /** The ONNX runtime dylib downloaded alongside the model (platform-sized —
   *  ~64 MB on Windows). The decline-informed gate must disclose the TOTAL
   *  (model + runtime); showing only the model understates the download. */
  runtime_size_mb: number;
  error?: string;
}

/** Progress event streamed over the embed_download Channel. Three stages:
 *  the two downloads report bytes; 'index' (embedding the existing library,
 *  part of the same one-click flow) reports piece counts. Completion and
 *  errors are NOT channel events: when the embed_download promise settles,
 *  re-fetch embed_status and render from that. */
export interface EmbedProgress {
  stage: 'runtime' | 'model' | 'index';
  done: number;
  total: number;
}
