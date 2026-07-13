/**
 * Prompt Library data model — the TS mirror of the Rust command surface
 * (src-tauri/src/prompts/). This file and api.ts are the seam between the two,
 * and they have one author on purpose: `pnpm check` cannot catch a Rust↔TS
 * mismatch, so a drift here fails at runtime, not at build.
 *
 * The model, in one line: **a snippet is a Markdown file whose filename is its
 * name, and a project is a name and a folder.** Every `*.md` under the folder,
 * recursively, is one of its snippets. There is no uuid, no scope, no
 * project_id — the filesystem is the source of truth, which is what makes the
 * library hand-editable and git-committable.
 *
 * Pure TypeScript — no DOM, no Tauri, no Svelte.
 */

/** A project: a name and a folder. Nothing else — no id, no color, no pin.
 *
 *  `path` is its identity: the snippets are simply the `.md` files inside it,
 *  so there is nothing to cross-reference and nothing that can drift out of
 *  sync with the filesystem. */
export interface Project {
  name: string;
  path: string;
}

/** What `list_projects` answers with. The active project is persisted by the
 *  backend and restored on launch, so it rides along with the roster rather
 *  than living in frontend state. */
export interface ProjectList {
  projects: Project[];
  /** Absolute path of the active project, or null when the roster is empty. */
  active: string | null;
}

/** One snippet — the entire model.
 *
 *  `name` is the identity: it is the file's path relative to the project
 *  folder, minus the `.md`, always `/`-separated (`rust/code_review`). Saving
 *  under the same name updates that file; saving under a new one creates a new
 *  snippet — which is the whole of "Save as new". A slashed name means
 *  subfolders, and subfolders are how the library is organized now: the user
 *  groups prompts with `mkdir`, not with tags.
 *
 *  `content` is the prompt, verbatim — the file's entire contents. There is no
 *  frontmatter and no metadata. The backend treats it as an opaque string; the
 *  variable grammar lives on the frontend, in one implementation. */
export interface Snippet {
  name: string;
  content: string;
}

/** One `match_snippets` result. A snippet's name is its identity, so that is
 *  all a hit needs to carry. */
export interface MatchHit {
  name: string;
  score: number;
}
