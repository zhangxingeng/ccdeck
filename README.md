# Claude Code Studio

An offline desktop app for browsing, reading, and editing your [Claude Code](https://claude.com/claude-code) chat history.

**Your conversations never leave your machine.** There is no server, no upload, and no telemetry. The app reads the JSONL files Claude Code already writes to `~/.claude/projects/` (or `$CLAUDE_CONFIG_DIR` if set). It is open source under the MIT license.

![Claude Code Studio — session metadata, word-level version diff, edited message badge, collapsed tool group, and the save rail](docs/hero.png)

## Features

### Offline and private

- 100% local — no network requests, no analytics, no accounts.
- Auto-discovers `~/.claude/projects/` on launch. Honors `CLAUDE_CONFIG_DIR` if set.
- All reading, editing, and backups happen on your local filesystem.

### Session listing

Each session card shows at a glance:

- Turn count (user messages) and total line count.
- Subagent count (only shown when greater than zero).
- File size (human-readable).
- Date range (first to last timestamp in the session).
- Model(s) used.

Sessions are grouped by project, sortable, and searchable by title and date.

### Rich rendering

When you open a session, the app renders:

- User and assistant messages with full markdown.
- Collapsible thinking blocks.
- Tool calls with their inputs.
- Tool results with correct success/error state.
- Nested subagent conversations (each subagent is a separate `.jsonl` file in `subagents/`).

### Editor

There is no separate "edit mode" — **the view is the editor.** Open any session and you get the same rich, read-only-looking transcript, except every message is editable in place. A metadata card at the top shows the project, models, CLI version, permission mode, date range and counts.

**In-place editing**

- Double-click the body of any message to edit its text right where it sits. Press Enter (without Shift) to confirm; Shift+Enter inserts a newline; Esc cancels.
- A message can hold several text blocks (e.g. text before and after a tool call); each block is edited independently, exactly where it renders.
- You only ever edit the message **string**, never raw JSON — so normal editing can't corrupt the file's structure.
- Committing a cell without actually changing anything is a no-op — it won't manufacture an empty "edit" or a phantom version.

**Just the chat by default — tool activity is grouped**

- Tool calls, tool results, and standalone thinking that sit between two chat messages are collapsed into a single **tool-activity strip** (`⚙ 3 tool calls · 2 results`). The default view reads as a clean conversation.
- Click a strip to expand it and inspect or edit the underlying lines; delete the whole run in one click, or any single line inside it.

**Per-message version history + diff**

- Every edit appends a new version. Step through versions with the `◀ v1/N ▶` controls on each message's hover toolbar.
- v1 is always the exact original line — stepping back to v1 and saving fully reverts that message.
- The `⇄` button opens a word-level **diff** of the current version against the **Original / Previous / Next / Latest** version (only the ones that exist are offered), so you can see exactly what changed.
- An "edited" marker appears when the active version is not the original.

**Crash-safe drafts**

- Every change is auto-saved to disk in `~/.claude/.ccstudio-edits/` within ~300 ms.
- If you close and reopen mid-edit, a "Resumed unsaved edits" banner appears with a Discard option to start fresh.
- The draft is deleted automatically once you save.

**Change the speaker, reorder, delete**

- A per-message hover toolbar lets you flip the speaker (user ↔ assistant) via a dropdown, move the message up/down, or delete it (shown struck-through and restorable until you save).

**Raw-JSON escape hatch**

- Tool calls and results render read-only. If you really need to edit one, the `{ }` button opens the underlying JSON line. It's re-validated on save — invalid JSON is rejected outright, so your history never ends up with an unparseable line.

**Saving (floating right-edge bar)**

- When you have unsaved changes, a save bar appears pinned to the right with the change count and three choices:
  - **Save** — overwrites the original file. Gated by a confirmation; takes an automatic timestamped backup to `~/.claude/.ccstudio-backups/` first.
  - **Save as copy** — writes a sibling file `<stem>-edited-<timestamp>.jsonl`, leaving the original untouched.
  - **Discard** — throws away all unsaved edits.
- **History** (always available on the bar) lists every backup for the session and restores any of them.
- Leaving via ← Back with unsaved edits prompts: Save / Save as copy / Discard / Keep editing. There is no silent data-loss path.

## Install

Download the installer for your platform from the [Releases page](https://github.com/zhangxingeng/claude-code-studio/releases):

| Platform | Files |
|----------|-------|
| Windows  | `.exe` or `.msi` |
| macOS    | `.dmg` (Apple Silicon and Intel) |
| Linux    | `.AppImage` or `.deb` |

### First-launch warning (unsigned builds)

These builds are unsigned — code-signing certificates are a paid, per-platform expense. The app is open source, but the OS has no way to verify it, so:

- **Windows** — SmartScreen may say "Windows protected your PC." Click **More info**, then **Run anyway**.
- **macOS** — Gatekeeper may refuse to open it. Right-click the `.app`, choose **Open**, then confirm. Alternatively: System Settings > Privacy & Security > Open Anyway.
- **Linux** — If using the AppImage, run `chmod +x <file>.AppImage` first.

## Build from source

Requires [Node.js](https://nodejs.org), [Rust](https://rust-lang.org), and the [Tauri v2 prerequisites](https://tauri.app/start/prerequisites/) for your OS.

```bash
npm install
npm run build          # build the SvelteKit frontend
npm run tauri build    # compile Rust + bundle a native installer
```

**Note on `npm run dev`:** Do not use `npm run dev` in this repo. Because the repo lives inside `~/.claude/projects/`, Vite's file watcher hits an EMFILE (too many open files) error caused by Claude Code's own project files. For browser-only development, use:

```bash
npm run build
npm run preview        # serves the bundled output with mock fixtures, no Tauri required
```

## Tests

```bash
npx tsx tests/*.mjs
```

Tests cover the edit model (draft building, version history, serialization, per-block/text/role/raw-JSON edits, no-op detection, reorder, dirty detection, preview extraction), the version-diff and tool-grouping helpers, plus the parser and title handling.

## How it works

The Rust layer (Tauri v2) does only filesystem work: listing sessions, reading and writing JSONL files, managing backups and edit drafts, and computing cheap per-session stats (one pass over the file). All parsing, rendering, and editing logic lives in the SvelteKit frontend (Svelte 5, TypeScript). There is no server process and no IPC beyond the Tauri command bridge. See `ARCHITECTURE.md` for the full command table.

## License

MIT
