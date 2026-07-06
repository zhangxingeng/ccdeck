# CC Deck — Architecture & Contracts

Desktop app (Tauri v2 + SvelteKit static SPA, Svelte 5 + TS) for Claude Code. Fully offline.
Reads Claude Code chat history from `~/.claude/projects/`, renders it, allows
editing with a versioned backup/undo system. No data ever leaves the machine.

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

read_subagents(session_path) -> SubagentFile[]
    // Look in <dir-of-session>/subagents/ for agent-*.jsonl and agent-*.meta.json.
    // Return raw contents; JS parses + links them.
    SubagentFile { name: string, content: string, is_meta: boolean }

write_session(path, content) -> null
    // Overwrite the original .jsonl. Caller MUST call snapshot(path) first.

snapshot(path) -> BackupVersion
    // Copy current on-disk file into the backup store BEFORE an override.
    // Store: <projects-dir>/../.ccstudio-backups/<sanitized session id>/vNNN-<unixsecs>.jsonl
    //   (i.e. ~/.claude/.ccstudio-backups/...). gzip optional; plain .jsonl is fine for v1.
    BackupVersion { version: number, timestamp: number, path: string, size: number }

list_backups(session_path) -> BackupVersion[]
    // All snapshots for a session, newest first.

restore_backup(backup_path) -> string
    // Return the raw contents of a backup version (frontend decides what to do:
    // it will snapshot current state, then write this content back).

read_edit_draft(session_path) -> string | null
    // Return the raw JSON of a saved edit draft for this session, or null if
    // no draft file exists. Draft store: ~/.claude/.ccstudio-edits/<sanitized_id>.json
    // where sanitized_id = sanitize_id(relative path from projects dir).

write_edit_draft(session_path, content) -> null
    // Persist an edit draft (creates ~/.claude/.ccstudio-edits/ if needed).
    // content is the JSON.stringify of the Draft object (see editDraft.ts).

delete_edit_draft(session_path) -> null
    // Delete the edit draft for this session. No-op if already absent.
```

Rust crates to add: `dirs` (home dir), `serde`/`serde_json` (already present),
`walkdir` optional. Register all commands in `invoke_handler`. Capabilities:
the commands are custom (`#[tauri::command]`) so no extra ACL entries are needed
beyond `core:default` already in capabilities/default.json.

## JS data model (src/lib/types.ts) — ported from the old docs/app.js

Recover the reference implementation: `git show e47e27d:docs/app.js`
Also reference (Python source of truth, in git history e47e27d):
  `git show e47e27d:src/claude_code_display/builder.py`
  `git show e47e27d:src/claude_code_display/parser.py`
  `git show e47e27d:src/claude_code_display/models.py`

Core shapes (keep these names; UI depends on them):
```
ContentBlock {
  blockType: 'thinking'|'text'|'tool_use'|'tool_result',
  text?, thinking?, signature?,
  toolName?, toolId?, toolInput?,        // tool_use
  toolOutput?, isError?, isAsync?,       // tool_result (matched in)
  agentId?,                              // set when tool_use is an Agent spawn
  subagent?,                             // attached Session of the subagent
}
Entry  { type, role, uuid, parentUuid, requestId, timestamp, model, isSidechain,
         blocks: ContentBlock[], isInterruption?, taskNotification? }
Turn   { role: 'user'|'assistant', blocks: ContentBlock[], timestamp, model }
Session{ turns: Turn[], meta: { title, date, model, project, sourcePath } }
```

Functions to export:
```
parseJsonl(text: string): Entry[]          // filters meta types + internal echoes
buildSession(entries, opts): Session        // groups by requestId into turns,
                                             // GLOBAL tool_result registry matches
                                             // tool_result -> tool_use across all turns
linkSubagents(session, subagentFiles)        // attach subagent Session to Agent tool_use by agentId
extractMeta(preview: string[]|Entry[]): {title,date,model}  // for the browse list
decodeProject(raw: string): string           // encoded dir name -> readable
```

Internal-echo prefixes to filter from user text / titles:
`<command-name>` `<local-command-stdout>` `<command-message>` `<command-args>`
`<local-command-caveat>` `<system-reminder>` `<teammate-message` `<task-notification>`
