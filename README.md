# CC Deck

**A friendly control center for [Claude Code](https://claude.com/claude-code)** — browse your
history, save your best prompts, fix your settings, and launch sessions, all without living in a
terminal or hand-editing JSON.

**Offline** · **Your data never leaves your machine** · **Open source, MIT licensed**

![CC Deck — a session rendered as a clean conversation with version diffs and the save bar](project_docs/hero.png)

## Why CC Deck exists

Claude Code is remarkably capable, but a few things keep it from being for absolutely everyone: **the
command line**, **a settings system spread across nested JSON files that even experienced developers
lose track of**, and **every good prompt you've ever written, scrolled away into a terminal
buffer.**

CC Deck's whole job is to take that wall down. Everything it does follows one rule: **simple by
default, advanced on demand.** The things you do most are one click away and explained in plain
English. The power-user controls are all still there; they just stay out of your way until you go
looking for them.

## Your history, finally readable

CC Deck finds every Claude Code session on your machine automatically and renders each one as a
conversation — not a wall of raw JSON. Markdown renders properly. Thinking steps collapse so they
don't clutter the page. Tool calls, their results, and nested sub-agent threads are laid out so you
can actually follow what happened.

![A chat transcript with message metadata, a collapsible tool-call group, and a Bash tool result](project_docs/screenshots/chat-detail.png)

Spot something you want to fix? Edit any message in place. Every change is backed up automatically
with a word-level diff, full undo/redo, and one-click restore to any earlier version — so editing
history is never a one-way door.

![Editing an assistant message's raw markdown source in place inside a transcript](project_docs/screenshots/edit-message.png)

Search covers your entire history at once, with filters for message type, date, project, and tool
name, and keyboard navigation throughout. When you find the conversation you want, resume it right
where you left off — or fork a brand-new session from any single message in its history.

## Write a prompt once. Use it forever.

The best prompt you wrote last week is gone. You typed it, it worked, and it scrolled away.

CC Deck gives it a home. Save any prompt — or any fragment of one — as a **snippet**, and it's
waiting for you next time. Group snippets into colored **projects**, pin the ones you use, and keep
a *writing* voice separate from a *code review* voice.

![A composed prompt built from color-coded snippets with fillable variables and per-variable defaults](project_docs/screenshots/prompt-compose.png)

Composing is where it comes together. Start typing, and matching snippets appear as you go; press
`↓` and `Enter` and one drops into place. Each snippet keeps its own color, so you can see at a
glance what came from where. Anything you write in `{curly braces}` becomes a **variable** you fill
in at the end — `{ticket}`, `{task}`, or `{n:5}` with a default already set. Fill them once, and
every mention updates. Then copy the whole thing and paste it wherever you're working.

You never have to touch the mouse. Type to search, arrow to choose, Enter to insert, `Ctrl+S` to
save, `Ctrl+C` to copy — and if those aren't the keys you want, change them.

<p align="center">
  <img src="project_docs/screenshots/prompt-projects.png" alt="The project manager listing colored projects with pin, open, and delete actions" width="47%">
  <img src="project_docs/screenshots/prompt-settings.png" alt="The settings panel showing file-repair notices, the optional local search model, and editable keyboard shortcuts" width="47%">
</p>

Search finds your snippets by spelling out of the box. If you want it to find them by *meaning* —
so "fix a bug" turns up the snippet you titled `repro-first` — you can switch on a small local model
with one click. It's about 115 MB, runs on your CPU, and works entirely offline. Nothing ever leaves
your machine, and ordinary search keeps working whether you download it or not.

Your snippets are plain JSON files on your disk, one per snippet. Edit them in any text editor you
like. If you make a typo in one, CC Deck repairs it in memory, tells you it did, and leaves your file
exactly as you wrote it until you decide to save.

## Settings without the JSON

Claude Code settings can live in up to three separate files — user, project, and local — and it's
genuinely hard to know what's set where, or which one wins. CC Deck reads all three, shows every
field in plain language (pulled straight from Claude Code's own published schema, not guesswork), and
flags conflicts loudly: *"`model` is set in both User and Project — Project wins."*

![Settings search showing Claude Code config keys matching "model", each with an inline description](project_docs/screenshots/settings-search.png)

About 20 of the most common settings are shown by default; the rest — well over a hundred — are one
click away behind a "show advanced settings" toggle. Edit any tier directly and CC Deck writes
exactly the file you meant to change, nothing merged behind your back.

## Run it your way

CC Deck doesn't force you into its own built-in console. Launch Claude Code in whatever terminal you
already use — it auto-detects a sensible default, so it just works out of the box. Want more control?
Pick a specific terminal, write your own launch command, or point a session at a different provider
entirely.

![The app config page showing terminal launch mode, a customizable resume command, and provider profiles](project_docs/screenshots/app-config.png)

## Getting started

CC Deck is a companion to Claude Code, not a replacement for it — you'll need
[Claude Code installed](https://code.claude.com/docs/en/quickstart) first. Once that's set up, CC Deck
finds your sessions and settings automatically; there's nothing to configure.

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

CC Deck runs entirely on your local filesystem. It reads and writes the same session and settings
files Claude Code already uses under `~/.claude/`, and keeps its own data (snippets, backups, search
index) under `~/.ccdeck/`. Nothing is ever uploaded anywhere. The only network request CC Deck itself
makes is checking for its own updates, and those update artifacts are cryptographically signed so you
can trust they came from this project.

The optional semantic-search model is downloaded once, from Hugging Face, and then runs offline on
your CPU forever after. Your prompts are never sent anywhere to be embedded.

---

## For developers

CC Deck is a Tauri v2 + Svelte 5 (TypeScript) desktop app: a Rust backend that's the only thing that
touches the filesystem, and a SvelteKit frontend that does all the parsing and rendering logic in
plain TypeScript so it's easy to test and reason about. Full command contract and data model are in
[`ARCHITECTURE.md`](ARCHITECTURE.md).

```
src-tauri/  Rust — native file access only (reads ~/.claude, settings.json tiers, search index)
src/lib/    TypeScript — pure logic (parsing, session model, prompt grammar) + Tauri API wrappers
src/routes/ Svelte 5 — the UI (browse+search / view / edit / prompts / settings)
```

### The Prompt Library, precisely

Snippets are one hand-editable JSON file each, under `~/.ccdeck/prompts/`, with `id` canonical and
the body as the single source of truth. A loader that can't parse a file repairs it **in memory**
and never rewrites your disk; the repair persists only when you next save that snippet, and until
then it's reported rather than swallowed.

Variables use a single-brace, f-string-flavored grammar: `{name}` or `{name:default}`, where a name
is `[A-Za-z0-9_-]+` and `{{` escapes a literal brace. Anything else between braces — JSON examples in
a prompt body, for instance — stays verbatim. The same name is the same variable everywhere in a
composed document, so `{task}` fills once and updates everywhere. Rust and TypeScript implement this
grammar independently and assert the same shared test vectors, because a seam like that is exactly
where two sides drift apart.

On copy, each variable is either substituted in place or hoisted into a trailing `<prompt_vars>`
block and referenced inline as `<prompt_var name="task"/>` — per variable, your choice, defaulting to
hoisted. The point is to state a long value once instead of repeating it into a model's context every
time it appears.

Matching is lexical by default (fzf-style weighted scoring, always on) and hybrid when you opt into
the embedding model. Design notes live in [`project_docs/prompts-design.md`](project_docs/prompts-design.md)
(storage, schema, command surface) and [`project_docs/prompts-ux.md`](project_docs/prompts-ux.md)
(every interaction, key by key).

### Build from source

Requires [Rust](https://rustup.rs/), [pnpm](https://pnpm.io/installation), and the
[Tauri v2 system dependencies](https://v2.tauri.app/start/prerequisites/) for your OS.

```bash
pnpm install
pnpm dev              # frontend only, in a browser — fastest loop for UI work
pnpm exec tauri dev   # full desktop app with native file access
pnpm exec tauri build # installable bundles (.deb/.rpm/.AppImage on Linux, equivalent per-OS elsewhere)
```

`pnpm dev` runs against bundled mock fixtures — a seeded snippet library, projects, and a sample
session — so the whole UI is exercisable in a plain browser with no native shell.

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

**Does CC Deck replace Claude Code?** No — it's a control center *for* Claude Code. You still need
Claude Code installed; CC Deck makes it easier to see, configure, and launch.

**Where do my snippets live?** In `~/.ccdeck/prompts/`, one JSON file each. They're yours: readable,
diffable, and safe to keep in git.

**Will editing settings in CC Deck break something?** CC Deck writes exactly the tier you edit, in the
same JSON format Claude Code reads — nothing is merged behind your back, and conflicts across tiers
are called out before you save.

**Is CC Deck affiliated with Anthropic?** No — see the disclaimer below. It's an independent project
built to make an existing tool friendlier, not an official product.

## License

MIT

---

*CC Deck is an independent, unofficial project. It is not affiliated with, endorsed by, or sponsored by
Anthropic. Claude and Claude Code are trademarks of Anthropic, PBC.*
