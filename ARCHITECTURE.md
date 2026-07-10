# CC Deck — Architecture & Contracts

Desktop app (Tauri v2 + SvelteKit static SPA, Svelte 5 + TS) for Claude Code. Fully offline.
Reads Claude Code chat history from `~/.claude/projects/`, renders it, allows
editing with a single-slot pre-save backup. No data ever leaves the machine.

## Layers

```
src-tauri/ (Rust)   native file access ONLY — the browser can't reach the FS
src/lib/   (TS)     pure logic: parse JSONL, build session model (no DOM, no Tauri)
src/lib/api.ts      thin wrapper over Tauri `invoke` + a browser-dev fallback
src/routes/ (Svelte) UI: browse / view / edit
```

## Rust <-> JS command contract (Tauri `invoke`)

All commands are async from JS. Paths are absolute strings. snake_case names.

```
find_projects_dir() -> string | null
    // Absolute path to the Claude projects dir, or null if not found.
    // Resolution order: env CLAUDE_CONFIG_DIR + "/projects",
    //   then <home>/.claude/projects. <home> per-OS (dirs crate).

list_sessions() -> SessionMeta[]
    // Walk each immediate subdir of the projects dir (each = one project).
    // Skip dirs named "subagents", "tool-results". For every *.jsonl that is
    // NOT named "agent-*.jsonl", return one SessionMeta.
    SessionMeta {
      id: string,              // stable id = relative path from projects dir
      path: string,            // absolute path to the .jsonl
      project_raw: string,     // the encoded project dir name
      mtime: number,           // unix seconds (file modified time)
      size: number,            // bytes
      preview: string[],       // first up to 50 lines of the file (for JS metadata extraction)
      // Cheap stats — computed in one pass over the full file content:
      line_count: number,      // non-empty lines in the file
      user_count: number,      // lines whose "type" == "user"
      assistant_count: number, // lines whose "type" == "assistant"
      subagent_count: number,  // count of subagents/agent-*.jsonl next to the session file
      models: string[],        // distinct message.model values, first-seen order
      first_ts: string,        // first "timestamp" value seen ("" if none)
      last_ts: string,         // last "timestamp" value seen ("" if none)
    }

read_session(path) -> string
    // Raw UTF-8 contents of the .jsonl file.

write_session(path, content) -> null
    // Overwrite the original .jsonl. By convention the caller calls
    // snapshot(path) first when overwriting existing content; not enforced
    // inside write_session itself.

snapshot(path) -> BackupVersion
    // Copy current on-disk file into the backup store BEFORE an override.
    // Single backup slot per session: any existing backup file(s) for the
    // session are deleted first, so exactly one file exists after the call.
    // Store: <projects-dir>/../.ccstudio-backups/<sanitized session id>/v001-<unixsecs>.jsonl
    //   (i.e. ~/.claude/.ccstudio-backups/...). gzip optional; plain .jsonl is fine for v1.
    BackupVersion { version: number, timestamp: number, path: string, size: number }

list_backups(session_path) -> BackupVersion[]
    // The session's backup, if any — 0 or 1 entries (single-slot backup).

restore_backup(backup_path) -> string
    // Return the raw contents of the backup (frontend decides what to do:
    // it will snapshot current state, then write this content back).
```

As of issue #6 (Phase B: plain edit surface), there is no crash-safe autosave
draft. Editing is plain edit-in-place -> Save: [`editDraft.ts`](src/lib/editDraft.ts)'s `Draft` holds
each row's original line plus its current (possibly edited) value entirely
in memory, and `write_session` is the only path that touches disk for an
edit. The editor's "Restore backup" affordance is a single button + confirm
(no version picker, no history list) backed by `list_backups`/`restore_backup`
above — there is only ever one backup file per session.

Rust crates to add: `dirs` (home dir), `serde`/`serde_json` (already present),
`walkdir` optional. Register all commands in `invoke_handler`. Capabilities:
the commands are custom (`#[tauri::command]`) so no extra ACL entries are needed
beyond `core:default` already in [`capabilities/default.json`](src-tauri/capabilities/default.json).

## JS data model ([`src/lib/types.ts`](src/lib/types.ts)) — ported from the old docs/app.js

Recover the reference implementation: `git show e47e27d:docs/app.js`
Also reference (Python source of truth, in git history e47e27d):
  `git show e47e27d:src/claude_code_display/builder.py`
  `git show e47e27d:src/claude_code_display/parser.py`
  `git show e47e27d:src/claude_code_display/models.py`

Core shapes (keep these names; UI depends on them):
```
ContentBlock {
  blockType: 'text',
  text?,
}
Entry  { type, role, uuid, parentUuid, requestId, timestamp, model, isSidechain,
         blocks: ContentBlock[], isInterruption?, taskNotification? }
Turn   { role: 'user'|'assistant', blocks: ContentBlock[], timestamp, model }
Session{ turns: Turn[], meta: { title, date, model, project, sourcePath } }
```

As of issue #6 (Phase A: render-trim), thinking/tool_use/tool_result blocks are
dropped during parsing — the display model only ever carries user/assistant
text. `ContentBlock` previously also carried `thinking`/`signature` (thinking
blocks) and `toolName`/`toolId`/`toolInput`/`toolOutput`/`isError`/`isAsync`/
`agentId`/`subagent` (tool_use/tool_result blocks, including the subagent
"Open →" navigation affordance); all were removed along with the rendering
that used them.

Functions to export:
```
parseJsonl(text: string): Entry[]          // filters meta types + internal echoes
buildSession(entries, opts): Session        // groups by requestId into turns
extractMeta(preview: string[]|Entry[]): {title,date,model}  // for the browse list
decodeProject(raw: string): string           // encoded dir name -> readable
```

Internal-echo prefixes to filter from user text / titles:
`<command-name>` `<local-command-stdout>` `<command-message>` `<command-args>`
`<local-command-caveat>` `<system-reminder>` `<teammate-message` `<task-notification>`

## Prompt Library (issue #24)

Full engineering contract: [`project_docs/prompts-design.md`](project_docs/prompts-design.md)
(storage layout, project model, snippet schema, variable grammar, copy-output XML mode, match
engine, compose-surface behavior, store robustness). Code: `src-tauri/src/prompts/`,
`src/lib/compose/`. Summary of the 11-command surface (same conventions as above):

```
list_snippets() -> Snippet[]                 // one JSON file per snippet at ~/.ccdeck/prompts/
save_snippet(snippet) -> Snippet               // create/update; append-only body versioning
delete_snippet(id) -> null
snippet_load_errors() -> {file,error}[]    // broken hand-edited files, surfaced not hidden
list_projects() -> Project[]             // roster in ~/.ccdeck/projects.json
save_project(project) -> Project         // create/update (rename, color, pin)
delete_project(id) -> null               // rescopes the project's snippets to global
match_snippets(query, project_id, limit) -> MatchHit[]  // pool = global + project_id's snippets
embed_status() -> EmbedStatus            // incl. model+runtime download sizes (decimal MB)
embed_download(channel) -> null          // streams {stage: runtime|model|index, done, total}
set_embed_enabled(bool) -> null
```

Snippets scope to a project id (`scope: {kind: global|project, project_id}`), not a filesystem
path. Variables are single-brace f-string syntax (`{name}` / `{name:default}`, Rust+TS
implemented identically against shared test vectors); copy output offers a dedup "as variable"
XML mode alongside plain substitution. The snippet store repairs hand-edited JSON in memory on
load but never rewrites the file — a repair persists only on the snippet's next explicit save.

**Data root `~/.ccdeck/`** (env `CCDECK_DATA_DIR` overrides; `src-tauri/src/datadir.rs`): snippets,
`projects.json`, `backups/` (session-edit backups), `index/` (search cache), `models/` + `cache/`
(opt-in embeddings, download ending in an index stage). On startup
`migrate_legacy_state()` moves the pre-0.12 artifacts out of `~/.claude` (`.ccstudio-backups`,
`.ccstudio-config.json`, `.ccstudio-index`) — invariant since: nothing ccdeck-owned lives under
`~/.claude`.
