# Report: remove Claude Code settings editor, add App Config page (#18, #19)

## Diffs touched

| File | +/- | Summary |
|---|---|---|
| `src/lib/components/SettingsView.svelte` | -501 | Deleted whole file (#18) |
| `src-tauri/src/settings.rs` | -544 | Deleted whole file + `mod settings`/2 command regs |
| `src/lib/schema/claude-code-settings.json` | -~190KB | Deleted vendored schema |
| `src-tauri/src/appconfig.rs` | +180/-30 | `AppConfig`: dropped `terminal_args`, added `launch_command` + `update_check_on_launch` (default `true` via `default_true`); added `effective_launch_command`, `build_resume_script`/`build_resume_script_windows` (shared pure script-builders); 12 unit tests incl. stale-key round trip |
| `src-tauri/src/lib.rs` | +60/-90 | `shell_quote` un-gated + `pub(crate)`; new `write_resume_script` temp-file helper; `resume_in_terminal` gained `session_title`, rewritten to build one script via `appconfig::build_resume_script(_windows)` and invoke via `sh <path>`/`call <path>` on every OS/terminal candidate instead of 3 hand-built command strings; removed `settings::` registrations |
| `src/lib/api.ts` | -90 | Removed all Claude-settings functions + dev shims; `resumeInTerminal` gained `sessionTitle` |
| `src/lib/types.ts` | -40 | Removed `SettingsTier`/`SettingsTierData`/`SettingsConflictValue`/`SettingsConflict`/`ClaudeSettings`; `AppConfig` = `{terminal, launchCommand, updateCheckOnLaunch}` |
| `src/lib/components/AppConfigView.svelte` | +230 (new) | Terminal radio/input, launch-command textarea + 2 presets, update-check checkbox, save/discard bar — matches `SettingsView`'s fieldset/legend style |
| `src/routes/+page.svelte` | ~ | `view` state `'settings'`→`'appconfig'`; `goSettings(cwd,label)`→`goAppConfig()`; header button → "⚙ App Config"; `resumeSession()` passes `current.meta.title` |
| `src/lib/components/BrowseView.svelte` | -20 | Removed `onOpenSettings` prop + both gear-icon blocks (deleted, not relocated); `doResume` gained `title`, both call sites pass `sg.title`/`s.title` |
| `src/lib/components/SessionEditor.svelte` | ~ | `doResumeFrom` passes `displayTitle` as forked session's title |
| `src/routes/+layout.svelte` | ~ | Launch-time `checkForUpdates(true)` now gated behind `getAppConfig().updateCheckOnLaunch` |
| `src/app.css` | -6 | Removed dead `.project-group__settings` CSS |
| `project_docs/roadmap.md` | ~ | Removed moot ajv follow-up bullet; added Phase 12 entry |

## Tests run

- `cd src-tauri && cargo test --lib --manifest-path Cargo.toml` — **36/36 passing** (12 new in `appconfig`: stale-`terminalArgs`-key round trip, update-toggle default/explicit-false, `effective_launch_command` fallback, `build_resume_script`/`_windows` env-export + multi-line + shell-quote-adversarial-title cases).
- `pnpm check` — **0 errors, 0 warnings, 221 files**.
- `cargo build --bin ccstudio` and `pnpm build` — both clean (schema-JSON removal doesn't break bundling).
- `pnpm test:smoke` — 105/105 assertions, unaffected.
- Grep for `SettingsTier|ClaudeSettings|SettingsConflict|SettingsView|terminal_args|terminalArgs|onOpenSettings|goSettings` across `src/`, `src-tauri/src/` — zero remaining references outside intentional comments.

## Compromises bubbled up

- **Live GUI verification not performed.** Started `pnpm dev` and attempted to drive it via the Chrome browser-automation tool, but the extension reported "not connected" in this sandbox (same gap flagged in Phase 6/7). Founder should open App Config, confirm the default command shows `claude --resume "$CCDECK_SESSION_ID"`, toggle update-check, save, reload before shipping.
- **Windows script builder** is unit-tested only (pure string builder), no manual/CI Windows run — matches the prompt's explicit "don't over-invest" instruction; its quoting (`set "VAR=value"`) doesn't escape embedded quotes in title/cwd, a documented best-effort limitation.
- **All three `resumeInTerminal` title sources were verified, not guessed**: `+page.svelte::resumeSession` uses `current.meta.title`; `BrowseView.svelte::doResume` uses `sg.title`/`s.title` (both already rendered at each call site); `SessionEditor.svelte::doResumeFrom` uses the component's existing `displayTitle` derived value.
- Multi-line `launch_command` is joined as `cd <cwd> &&\n<launch_command>` — only the first line is `&&`-gated, later lines run unconditionally. Acceptable since `cwd` is pre-validated as an existing directory, so `cd` essentially never fails.
- `parse_args` left untouched per "Forbidden moves" — confirmed it still has a live caller (splitting the `terminal` field's command-prefix template) before assuming it was dead.

## Open questions

None.
