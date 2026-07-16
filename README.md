# CC Deck

**A friendly companion for [Claude Code](https://claude.com/claude-code)** — search your entire
conversation history, edit it, and share it, without living in a terminal or reading raw JSON.

**Offline** · **Your data never leaves your machine** · **Open source, MIT licensed**

## Why CC Deck exists

Claude Code writes every conversation to disk — and then gives you almost no way to get back into
that history: no search, no readable view, no way to hand a transcript to someone else.

CC Deck does two things with that history, and does them properly: **search** (find any conversation
you've ever had, instantly) and **edit & share** (redact what's sensitive, then export a clean,
readable file). Everything follows one rule: **simple by default, advanced on demand** — the things
you do most are one click away, and power-user controls stay out of your way until you go looking.

## Your history, finally readable

CC Deck finds every Claude Code session on your machine automatically and renders each one as a
conversation — not a wall of raw JSON. Markdown renders properly. Thinking steps collapse so they
don't clutter the page. Tool calls and their results are laid out so you can actually follow what
happened.

![A chat transcript with message metadata, a collapsible tool-call group, and a Bash tool result](project_docs/screenshots/chat-detail.png)

Spot something you want to fix — or redact before sharing? Edit or delete any message in place.
CC Deck silently keeps a backup of the previous state each time you save, so a mistaken edit is
never a one-way door — the snapshot sits on disk, ready to restore by hand if you ever need it.

![Editing an assistant message's raw markdown source in place inside a transcript](project_docs/screenshots/edit-message.png)

Search covers your entire history at once — just your messages and Claude's replies, no tool noise —
with date and project filters and keyboard navigation throughout. When you find the conversation
you want, resume it right
where you left off — or fork a brand-new session from any single message in its history.

Managing prompt snippets moved to its own app, [Prompt Compose](https://github.com/zhangxingeng/prompt-compose)
— CC Deck now focuses on search and edit/share.

## Share a conversation as a file

Export any session as a single self-contained HTML file — styles inlined, no external requests —
that opens in any browser, or print it straight to PDF from the same view. Combined with in-place
editing, that's the whole share flow: redact what's sensitive, export, send.

## Resume in your own terminal

CC Deck never launches or controls a terminal for you. To pick a conversation back up, click Resume
(or right-click a session) and copy the facts you need — the ready-to-paste
`cd '<project>' && claude --resume '<id>'` command, the project path, or the session id — then run
it wherever you already work. Forking is the same: fork from any message and CC Deck hands you the
new session's resume command.

## Getting started

CC Deck is a companion to Claude Code, not a replacement for it — you'll need
[Claude Code installed](https://code.claude.com/docs/en/quickstart) first. Once that's set up, CC Deck
finds your sessions automatically; there's nothing to configure.

Download the installer for your platform from the [Releases page](https://github.com/zhangxingeng/ccdeck/releases):

| Platform | Files |
|----------|-------|
| Windows  | `.exe` or `.msi` |
| macOS    | `.dmg` (Apple Silicon and Intel) |
| Linux    | `.AppImage` or `.deb` |

### First launch (unsigned builds)

These builds aren't OS-code-signed — per-platform signing certificates are a paid expense a small
open-source project doesn't carry — so your OS may warn you the first time you open the app. That's
expected; here's how to get past it:

- **Windows** — SmartScreen: click **More info** → **Run anyway**.
- **macOS** — Right-click the app → **Open** → confirm (or System Settings → Privacy & Security →
  Open Anyway).
- **Linux** — For the AppImage, run `chmod +x <file>.AppImage` first.

## Privacy / how it works

CC Deck runs entirely on your local filesystem. It reads and writes the same session files Claude
Code already uses under `~/.claude/`, and keeps its own data (backups, search index) under
`~/.ccdeck/`. Nothing is ever uploaded anywhere.

CC Deck makes exactly one kind of network request, to fetch, never to send: its own update check —
those artifacts are cryptographically signed, so you can trust they came from this project.

---

## For developers

CC Deck is a Tauri v2 + Svelte 5 (TypeScript) desktop app: a Rust backend that's the only thing that
touches the filesystem, and a SvelteKit frontend that does all the parsing and rendering logic in
plain TypeScript so it's easy to test and reason about. Full command contract and data model are in
[`ARCHITECTURE.md`](ARCHITECTURE.md).

```
src-tauri/  Rust — native file access only (reads ~/.claude history, the ~/.ccdeck data root, search index)
src/lib/    TypeScript — pure logic (parsing, session model), Tauri API wrappers, and the Svelte 5 UI components
src/routes/ SvelteKit — the single-page shell that mounts the views: browse+search, view, edit, app config
```

### Build from source

Requires [Rust](https://rustup.rs/), [pnpm](https://pnpm.io/installation), and the
[Tauri v2 system dependencies](https://v2.tauri.app/start/prerequisites/) for your OS.

```bash
pnpm install
pnpm dev              # frontend only, in a browser — fastest loop for UI work
pnpm exec tauri dev   # full desktop app with native file access
pnpm exec tauri build # installable bundles (.deb/.rpm/.AppImage on Linux, equivalent per-OS elsewhere)
```

`pnpm dev` runs against bundled mock fixtures — a sample session — so the whole UI is exercisable in
a plain browser with no native shell.

## Contributing

CC Deck is a small open-source project, and contributions of any size are genuinely welcome — code,
bug reports, docs fixes, or just telling us something's confusing.

- **Found a bug or have an idea?** Open an
  [issue](https://github.com/zhangxingeng/ccdeck/issues/new/choose) — there are templates for bug
  reports and feature requests to help you include what we need.
- **Want to send a PR?** [`CONTRIBUTING.md`](CONTRIBUTING.md) has the full dev setup, the checks to
  run before opening one (`pnpm check`, `cargo test --lib`, `pnpm build`), and the PR template will
  walk you through the rest.
- **Not sure where to start?** Docs fixes and README improvements never need an issue first — just
  open a PR. For code, `project_docs/roadmap.md` tracks what's shipped, what's planned, and what's been
  explicitly deferred, which is a good way to find something that isn't already spoken for.

The guiding principle for any change: **simple by default, advanced on demand.** If you're not sure
whether a new control belongs up front or behind an "Advanced" toggle, default to hiding it.

## FAQ

**Does CC Deck send my conversations anywhere?** No. Everything happens locally; the only network call
CC Deck makes is its own update check.

**Does CC Deck replace Claude Code?** No — it's a companion *for* Claude Code. You still need Claude
Code installed; CC Deck makes its history searchable, editable, and shareable.

**Will editing a conversation break something?** CC Deck writes the same JSONL format Claude Code
reads, and snapshots a backup before every save. Sessions stay resumable after an edit.

**Is CC Deck affiliated with Anthropic?** No — see the disclaimer below. It's an independent project
built to make an existing tool friendlier, not an official product.

## License

MIT

---

*CC Deck is an independent, unofficial project. It is not affiliated with, endorsed by, or sponsored by
Anthropic. Claude and Claude Code are trademarks of Anthropic, PBC.*
