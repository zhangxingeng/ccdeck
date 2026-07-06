# Contributing to CC Deck

Thanks for considering a contribution — CC Deck is a small open-source project and every bit of
help (code, bug reports, docs, design feedback) is welcome.

## Ways to help that aren't code

- **Report a bug.** [Open an issue](https://github.com/zhangxingeng/ccdeck/issues/new?template=bug_report.md)
  with your OS, CC Deck version (shown in the footer), and steps to reproduce.
- **Suggest a feature.** [Open a feature request](https://github.com/zhangxingeng/ccdeck/issues/new?template=feature_request.md).
  Check `project_docs/roadmap.md` first — your idea might already be planned or explicitly deferred, in
  which case a 👍 on the existing note is more useful than a duplicate issue.
- **Improve the docs.** README clarity, typos, a missing FAQ answer — all fair game, no issue
  needed, just open a PR.

## Project layout

See [`ARCHITECTURE.md`](ARCHITECTURE.md) for the full Rust ↔ JS command contract. Short version:

```
src-tauri/  Rust — native file access only (reads ~/.claude, settings.json tiers, search index)
src/lib/    TypeScript — pure logic (parsing, session model) + Tauri API wrappers
src/routes/ Svelte 5 — the UI (browse / view / edit / search / settings)
```

`src/lib/api.ts` is the seam: every Tauri command has a browser-dev mock behind an `isTauri()`
guard, so you can run the full UI in a normal browser (`pnpm dev`) without building the native
shell at all — useful for most frontend work.

## Development setup

Prerequisites: [Rust](https://rustup.rs/), [pnpm](https://pnpm.io/installation), and the
[Tauri v2 system dependencies](https://v2.tauri.app/start/prerequisites/) for your OS.

```bash
pnpm install
pnpm dev              # frontend only, in a browser — fastest loop for UI work
pnpm exec tauri dev   # full desktop app with native file access
```

Before opening a PR, please run:

```bash
pnpm check                       # svelte-check, must be 0 errors/warnings
cd src-tauri && cargo test --lib # Rust unit tests, must pass
pnpm build                       # production frontend build
```

`pnpm exec tauri build` produces installable bundles (`.deb`/`.rpm`/`.AppImage` on Linux,
equivalent per-OS bundles elsewhere) if you want to test a real install.

## Making a change

1. Fork the repo and create a branch off `main`.
2. Keep PRs focused — one feature or fix per PR is easier to review than a bundle of unrelated
   changes.
3. Add or update Rust unit tests for backend logic changes (see `src-tauri/src/settings.rs` or
   `src-tauri/src/search/query.rs` for examples of the test style used here).
4. If your change is user-facing, mention it in the PR description — no changelog file to update,
   just a clear description.
5. Open the PR against `main`. The PR template will ask what changed and how it was tested.

## Design principle

CC Deck's guiding rule is **simple by default, advanced on demand**: the common path should stay
approachable for non-technical users, while power-user options (custom terminals, advanced
settings, raw JSON) stay available but out of the way. When in doubt about where a new control
should live, default to hiding it behind an "Advanced" toggle rather than surfacing it up front.

## Code of conduct

Be respectful and assume good faith. This is a small project maintained in spare time — response
times may vary, but every issue and PR gets read.
