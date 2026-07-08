# Build brief: remove Claude Code settings editor, add App Config page

## Why this work exists

Two coordinated GitHub issues, both founder-approved and ready to build (no open design
questions left):

- **#18** — delete the Claude Code settings editor entirely (`SettingsView.svelte`,
  `settings.rs`, vendored schema). No replacement UI: users hand-edit `settings.json` by hand.
  Same call already made for the chat-viewer diff/version machinery (#6) — this repo's founder
  prefers lean+used over impressive+idle.
- **#19** — a new "App Config" page (CC Deck's own preferences, never Claude Code's
  `settings.json`) holding: a fully custom, env-var-based resume-launch command, a
  terminal-emulator choice, and an update-check-on-launch toggle. Becomes the new entry point
  that replaces the removed "⚙ Settings" button.

**They are being built together, in one pass, not sequentially** — #19's design explicitly says
"coordinate with #18 ... either land together or keep a stopgap entry point." Landing together
is cleaner than a stopgap and the two issues share every file that matters (the entry-point
button in `+page.svelte`, `appconfig.rs`, `lib.rs`'s `resume_in_terminal`), so splitting them
across two agents would just create merge friction on the same files. Treat this as one feature.

## Mandatory reading

| Doc | What it floors |
|---|---|
| `.claude/memory/MEMORY.md` | Standing rules: target latest toolchain (don't code around an old `pnpm`/`cargo` version), cut features with near-zero usage rather than keep them "just in case", and — if you touch anything parse/serialize-shaped — round-trip test it rather than reasoning from a code read alone. |
| `ai-first-docs/stack/svelte/design_protocol.mdx` | Svelte 5 rune rules (`$state`/`$derived`/`$effect`), Props conventions, when raw/scoped CSS is justified over utility classes — the new `AppConfigView.svelte` should match `SettingsView.svelte`'s existing style (see below), not invent a new pattern. |
| `ai-first-docs/stack/svelte/insight.mdx` | Svelte 5 / SvelteKit gotchas (rune edge cases, deprecated APIs) — skim for anything relevant to a form-heavy component with local mutable state. |
| `ARCHITECTURE.md` (repo root) | The Rust/TS/Svelte layering contract — `src-tauri/` is the only FS-touching layer, `src/lib/api.ts` is the thin `invoke` wrapper with a browser-dev fallback (`isTauri()` guard), commands are snake_case. Your new/changed Tauri commands and their JS mirrors must follow this exactly. |

## The system surface — read cold before editing

Files to delete (issue #18):
- `src/lib/components/SettingsView.svelte` (whole file, 501 lines)
- `src-tauri/src/settings.rs` (whole file, 544 lines) — and its `mod settings;` declaration
  (`src-tauri/src/lib.rs:15`) plus its two command registrations
  (`src-tauri/src/lib.rs:915-916`, `settings::read_claude_settings` / `write_claude_settings`)
- `src/lib/schema/claude-code-settings.json` (vendored ~190KB schema, only consumer is
  `SettingsView.svelte`)
- In `src/lib/api.ts`: `readClaudeSettings`, `writeClaudeSettings`, `isSettingsConflict`, the
  `devSettingsStore`/`devTierPath`/`devReadClaudeSettings` dev-mode shims (lines ~117-207), and
  the `ClaudeSettings, SettingsTier` import at the top
- In `src/lib/types.ts`: `SettingsTier`, `SettingsTierData`, `SettingsConflictValue`,
  `SettingsConflict`, `ClaudeSettings` (lines ~120-155) — verify nothing else references them
  before deleting (`grep -rn "SettingsTier\|ClaudeSettings\|SettingsConflict" src/`)

Files to change (both issues touch these — read as one surface, not two):
- `src-tauri/src/appconfig.rs` (111 lines) — the whole file; see "AppConfig schema" below
- `src-tauri/src/lib.rs` — `resume_in_terminal` (lines 554-681, all three `#[cfg(target_os)]`
  branches) and `shell_quote` (line 214)
- `src/lib/api.ts` — `getAppConfig`/`setAppConfig`/`resumeInTerminal` (lines ~211-245) and the
  `devAppConfig` dev shim
- `src/lib/types.ts` — `AppConfig` interface (line ~162)
- `src/routes/+page.svelte` — the `view` state machine (`'browse' | 'viewer' | 'settings'`), the
  "⚙ Settings" header button (~line 284), `goSettings`/`settingsProjectCwd`/`settingsProjectLabel`
  (~lines 32-34, 140-143), the `resumeSession()` function (~line 257-270)
- `src/lib/components/BrowseView.svelte` — the per-project settings gear icon (two near-identical
  blocks, ~lines 590-598 and 687-695) and `doResume()` (~line 471-476)
- `src/lib/components/SessionEditor.svelte` — `doResumeFrom()` (~line 285-300)
- `src/routes/+layout.svelte` — the launch-time `checkForUpdates(true)` call (line 13)
- `project_docs/roadmap.md` — the "Open follow-ups" ajv note (~line 455) and a new phase entry
  (see Doc sync below)

Adjacent files for context (read, don't need to change):
- `src/lib/updater.svelte.ts` — `checkForUpdates(silent)` signature; you're gating one call site,
  not changing this file
- `src/routes/+page.svelte`'s `handleCheckForUpdates()` (~line 86-88) — this is the **manual**
  "check for updates" action (non-silent). It must stay ungated — only the silent launch-time
  check in `+layout.svelte` respects the new toggle.

## Decisions already settled — implement these, don't relitigate

1. **Env var names are fixed**: `CCDECK_SESSION_ID`, `CCDECK_SESSION_TITLE`, `CCDECK_CWD`. Export
   all three into the launched command's environment.
2. **Default command** (zero-config, must reproduce today's exact behavior):
   `claude --resume "$CCDECK_SESSION_ID"`.
3. **`terminal_args` is retired, folded into the command field.** Today's `AppConfig.terminal_args`
   ("extra CLI args appended after `--resume <id>`") is superseded by the fully-custom command
   field — a user who wants `--dangerously-skip-permissions` now just writes
   `claude --resume "$CCDECK_SESSION_ID" --dangerously-skip-permissions` in the command field.
   Keeping both fields would be two mechanisms doing the same job. Drop `terminal_args` from
   `AppConfig`. This is schema-compatible with existing on-disk configs: the struct has no
   `deny_unknown_fields`, so an old `.ccstudio-config.json` with a stale `terminalArgs` key just
   has that key ignored on next load — no migration code needed, verify this with a unit test
   (deserialize a JSON blob containing `terminalArgs` and confirm it loads without error).
4. **New `AppConfig` shape** (`src-tauri/src/appconfig.rs`):
   - `terminal: String` — unchanged: empty/`"auto"` ⇒ auto-detect; otherwise a terminal-emulator
     command prefix (`gnome-terminal --`, `konsole -e`, `iTerm`, `wt`, etc.) — same semantics as
     today, just no longer paired with `terminal_args`.
   - `launch_command: String` — new. Defaults to
     `claude --resume "$CCDECK_SESSION_ID"` when empty. Multi-line text is valid (a small script,
     not just a one-liner) — treat it as shell script body, not a single argv.
   - `update_check_on_launch: bool` — new, **defaults to `true`** (preserves today's always-check
     behavior for anyone who never opens App Config; `#[derive(Default)]` on a `bool` gives
     `false`, so this field needs an explicit default — use
     `#[serde(default = "default_true")]` with a small `fn default_true() -> bool { true }`, or
     implement `Default` by hand instead of deriving it).
5. **One shared script-generation function, not three duplicated per-OS command builders.**
   Today's three `#[cfg(target_os)]` branches each hand-build a `claude --resume <id> <extra_args>`
   string differently. Replace with: a single pure function (fully unit-testable, no process
   spawning) that takes `(cwd, session_id, session_title, launch_command) -> String` and returns a
   shell-script body: env exports for the three `CCDECK_*` vars (shell-quoted via the existing
   `shell_quote` helper in `lib.rs`, extended to the new call sites) followed by `cd <cwd> &&`
   followed by the raw `launch_command` text. Write this script to a temp file (mirroring what
   the macOS branch already does with `std::env::temp_dir().join(...)`, extended to Linux too —
   `.sh` extension, `0o755` on Unix), then have every terminal-emulator candidate (Linux
   auto-detect list, the macOS `open -a <Terminal>`, the configured custom terminal prefix on any
   OS) simply invoke `sh <script-path>` (or the terminal's native "run this" arg shape) instead of
   re-deriving `claude --resume <id> <args>` per platform. This removes duplicated command-string
   assembly and is the only way `launch_command` (which may be multi-line) works uniformly across
   `open -a`, `gnome-terminal --`, `konsole -e`, `wt`, etc.
   - Windows: build an equivalent `.bat`/`.cmd` temp script (`set CCDECK_SESSION_ID=...` etc.,
     `cd /D <cwd>`, then the command text), launched the same way today's `cmd /C start ...`
     path does. Best-effort consistency with the Unix path; this repo has no Windows CI/dev
     machine, so don't over-invest in exhaustive Windows testing — a unit test on the pure
     script-string-builder function is sufficient coverage there too.
6. **`resume_in_terminal`'s Tauri command signature grows a `session_title: String` parameter**
   (in addition to today's `cwd`, `session_id`). All three call sites
   (`+page.svelte::resumeSession`, `BrowseView.svelte::doResume`, `SessionEditor.svelte::doResumeFrom`)
   already have a session's display title available (`current.meta.title`,
   `sg.title`/enriched session title, and the forked session's title respectively — reuse
   whatever the surrounding component already computes for display, don't re-derive it). Update
   `resumeInTerminal(cwd, sessionId, sessionTitle)` in `api.ts` and all three call sites.
7. **Per-project settings gear icon in `BrowseView.svelte` is deleted, not relocated.** App Config
   is a single global-scope page (launch command / terminal / update toggle are app-level
   preferences, not per-project) — there is no per-project analog to relocate it to. Delete the
   `onOpenSettings` prop, both gear-icon blocks, and the prop plumbing back through `+page.svelte`
   (`openSettings`/`goSettings` becomes global-only — actually just remove the cwd/label params
   entirely from the button's `onclick` and go straight to the App Config route).
8. **Starter-profile presets in the new `AppConfigView.svelte`**: a small set of buttons/select
   above the command textarea that just overwrite the textarea's text with a preset (plain
   `claude --resume "$CCDECK_SESSION_ID"`, and one tmux example like
   `tmux new-session -A -s "$CCDECK_SESSION_TITLE" "claude --resume $CCDECK_SESSION_ID"`) — these
   are pure UI convenience, not a separate storage mechanism; whatever the preset inserts is then
   just the same free-text `launch_command` value.
9. **Command field is a multi-line `<textarea>`**, not a single-line `<input>` (per the issue's
   resolved open question).

## Forbidden moves — do not silently decide these

- Do not touch `appconfig.rs`'s `parse_args`/`is_auto` beyond what's needed — `is_auto` is still
  used for the terminal-emulator field; only `parse_args`'s *caller* for `terminal_args` goes away
  (the function itself may still be dead code to remove — check for other callers first).
- Do not invent a different set of env-var names or switch to string-placeholder substitution
  (`{id}`/`{title}` splicing into the command text) — the whole point of env-var injection is
  avoiding shell-quoting/injection complexity; a text-substitution scheme reopens exactly that.
- Do not add JSON-schema validation, credential/provider-profile UI, or fuzzy search over
  anything — those are #20/#21, explicitly out of scope, not decided, don't touch.
- Do not leave a moment where Resume has no working launch mechanism — the old `SettingsView`
  terminal UI and the new `AppConfigView` terminal+command UI should not coexist in the diff; land
  the replacement in the same pass the removal happens.

## Deployment topology

None relevant here — this is a single-process desktop app (Tauri backend + one webview), no state
shared across processes/containers. `AppConfig` is read/written to one file
(`~/.claude/.ccstudio-config.json`) by the same process that reads it; no cross-process coherence
concern applies.

## Verification sequence

Run in this order, each with an explicit timeout:

```bash
cd src-tauri && cargo test --lib --manifest-path Cargo.toml 2>&1 | tail -40   # timeout 120000ms
```
```bash
pnpm check 2>&1 | tail -60   # timeout 120000ms — SvelteKit/TS type-check across the whole frontend
```

Both must be clean before you report done. If either fails, root-cause it (don't suppress a type
error with `as any`, don't skip a broken test) — this is exactly the kind of load-bearing surface
(a registry-style command list in `lib.rs`, a config file's on-disk shape) where a quiet mistake
compounds.

Also worth a quick manual sanity check if you can run the dev app (`pnpm tauri dev` or similar —
check `package.json` scripts / `CONTRIBUTING.md` for the right command): open App Config, confirm
the default command field shows `claude --resume "$CCDECK_SESSION_ID"`, toggle the update-check
box, save, reload the page, confirm it persisted. If you can't get a GUI up in your environment,
say so explicitly in the report rather than claiming it works — this repo's convention (see
`project_docs/roadmap.md`'s past "Not performed: live GUI verification" notes) is to flag skipped
GUI checks, not silently skip them.

## Doc sync (do this as part of the same pass — small, mechanical)

In `project_docs/roadmap.md`:
- Remove the "Heavier JSON-Schema validation (`ajv`...)" bullet from "Open follow-ups" — it's moot
  once the settings form is gone.
- Add a new `## Phase 12 — Remove settings editor; App Config page (env-var launch command +
  update toggle) (DONE)` entry, following the existing phase-entry style (see Phase 9's entry for
  a similarly-shaped "remove a UI surface, replace with something leaner" precedent) — what was
  removed, what was added, the env-var mechanism, and note the two source issues (#18, #19).

## What to return

Write your report to `.claude/work/prompt_report/appconfig_relaunch_report.md`, capped ~500 words:

- **Diffs touched** — table: file | added | deleted | one-line summary
- **Tests run** — exact commands + pass/fail
- **Compromises bubbled up** — every shortcut, uncertain call, deferred item (mandatory section;
  "None — clean" only if genuinely clean — e.g. did you actually find and update all three
  `resumeInTerminal` call sites' title sources, or guess one?)
- **Open questions** — "none" if clean

Do not paste your exploration/scratch reasoning into the report — just the structured sections
above.
