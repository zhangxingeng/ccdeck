# Audit report: settings-editor removal + App Config page (#18, #19)

Verdict: this **merges cleanly** into the codebase's shape. The removal is thorough, the shared
script-builder is genuinely shared, command registration is intact, and the POSIX path is correctly
shell-quoted with adversarial test coverage. One real cross-platform gap and two doc/UX seams remain.

## Fixed in-pass

| file | what | why safe |
|---|---|---|
| — | None | Nothing was single-file + mechanical + judgment-free; the stale doc line below is a founder-recorded idea (reword-vs-delete is a call), so it is flagged, not edited. |

## Findings

- **MEDIUM — Windows resume script does not escape attacker-influenced input.**
  `src-tauri/src/appconfig.rs:158` (`build_resume_script_windows`) splices `session_title`,
  `cwd`, `session_id` raw into `set "VAR=..."` lines with no escaping, unlike the POSIX path which
  routes all three through `crate::shell_quote`. `session_title` comes from parsed chat content
  (`current.meta.title` / `sg.title` / `displayTitle`), so a session file crafted with a title like
  `x" & <cmd> & rem` breaks out of the `set` quoting and injects into the following `.bat` lines —
  arbitrary command execution on Resume. The design's stated safety property ("env-var injection,
  not string substitution") holds on macOS/Linux but not Windows. The code comment concedes "no
  shell-style quote-escaping is attempted" and the repo has no Windows CI, which is why this is
  MEDIUM not HIGH, but it is exactly the break-out the brief (Q4) asked to flag. A newline in a
  title would also split the script on Windows (single-quoting makes newlines harmless on POSIX).

- **LOW — clipboard fallback diverged from the actual launch mechanism.**
  `src/lib/resume.ts:18` (`resumeCommand`) still hardcodes `claude --resume <id>` for the
  copy-to-clipboard text every Resume call pairs with. Now that launch is a fully custom
  `launch_command`, a user with e.g. the tmux preset who clicks Resume and hits the terminal-launch
  failure path gets a clipboard command that does *not* reproduce their configured behavior. Not a
  regression (it was always the plain command), but the two representations of "how you resume" have
  silently split. This is the one remaining hand-built `claude --resume` string that is a real
  launch path rather than doc/placeholder text (Q3) — legitimate as a fallback, but stale.

- **LOW — `roadmap.md:511` "Future ideas" proposes building on a deleted surface.**
  The "Ask Claude about a setting … inline AI help inside `SettingsView`" idea now references a
  component and whole settings-editor system Phase 12 just removed. Historical Phase 1–11 entries
  referencing `settings.rs`/`SettingsView` are chronological records and correctly left intact, but
  a forward-looking idea scoped to a component that no longer exists is exactly the "didn't we delete
  this?" residue (Q1) a future reader trips over. Reword to the new hand-edit reality or drop it.

## Verified sound (no finding)

- **Command registry (Q2):** `resume_in_terminal` registered with the new 3-arg signature; both
  `settings::*` registrations and `mod settings` removed; `settings.rs` deleted. No orphan, no
  unregistered `#[tauri::command]`.
- **Shared builder (Q3):** all three `#[cfg(target_os)]` branches in `lib.rs` call
  `appconfig::build_resume_script[_windows]` + `write_resume_script`; no per-OS `claude --resume`
  string survives in the backend.
- **Call sites (Q2):** every `resumeInTerminal` caller passes a genuine display title
  (`current.meta.title`, `sg.title`, `displayTitle`) — none stubbed.
- **Default/serde agreement (Q5):** the hand-written `Default` impl (`update_check_on_launch: true`)
  matches the `#[serde(default = "default_true")]` field attr and container-level `default`; three
  round-trip tests (stale-key ignore, missing-key→true, explicit-false) confirm it. No mismatch trap.
- **Toggle gating (Q5):** `+layout.svelte` gates only the silent launch check on
  `updateCheckOnLaunch`; `+page.svelte`'s manual "Check for updates" stays ungated. Correct.
- **Residue (Q1):** `onOpenSettings`, `goSettings`, `settingsProject*`, `.project-group__settings`
  CSS, and the `SettingsTier`/`ClaudeSettings` types/imports/dev-shims are all gone; the
  `.claude/settings.json` `$schema` URL is the harness's own file, not the deleted vendored schema.
- **Roadmap Phase 12 prose (Q6):** spot-checked against the diff — 3 call sites, `terminal_args`
  retirement with no migration, single shared builder, toggle gating — all accurate.

## Out-of-scope but flagged

None. (Issues #20/#21 correctly absent per scope.)
