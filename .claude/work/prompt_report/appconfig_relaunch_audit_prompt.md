# Audit brief: settings-editor removal + App Config page (issues #18, #19)

You are auditing a just-completed feature build, cold — you were not involved in writing it. Read
the diff and the surrounding surface fresh, and answer one question: **does this code merge
cleanly into the codebase's existing shape, or does it sit on top of it as a patch?**

Do not read any `.claude/work/` prompt/report files from the build pass — form your own read of
the code, not a read colored by what the build agent claims it did.

## Surface — read cold

Run `git diff` and `git status` at the repo root to see the full change (13 modified files, 3
deleted, 1 new). Focus your attention on:

- `src-tauri/src/appconfig.rs` — the new `AppConfig` shape, the shared `build_resume_script`/
  `build_resume_script_windows` functions, the `default_true` bool-default pattern
- `src-tauri/src/lib.rs` — `resume_in_terminal` (search for `fn resume_in_terminal`) and its three
  `#[cfg(target_os = ...)]` branches, plus the command registration list near the bottom of the
  file (search for `.invoke_handler` / `tauri::generate_handler!`)
- `src/lib/components/AppConfigView.svelte` — the new page, compare its shape against
  `src/lib/components/BrowseView.svelte` and `src/lib/components/SessionEditor.svelte` for
  established conventions (Props destructuring, `$state`/`$derived` usage, save/discard bar
  pattern, CSS custom-property usage)
- `src/routes/+page.svelte`, `src/routes/+layout.svelte`, `src/lib/components/BrowseView.svelte`,
  `src/lib/components/SessionEditor.svelte`, `src/lib/api.ts`, `src/lib/types.ts` — the entry-point
  rewiring and the `resumeInTerminal(cwd, sessionId, sessionTitle)` signature change threaded
  through all three call sites
- `project_docs/roadmap.md`, `CONTRIBUTING.md` — doc accuracy after the removal

Adjacent files to skim for convention comparison only (don't need deep review): any other
`src-tauri/src/*.rs` module for the project's Tauri-command style; any other `src/lib/components/
*.svelte` for the fieldset/legend form style this repo already uses.

## Vocabulary docs — the lens, not a checklist

| Doc | Why it matters here |
|---|---|
| `ARCHITECTURE.md` (repo root) | The Rust/TS layering contract this change must not violate — `src-tauri/` is the only FS/process-touching layer, commands are snake_case, `api.ts` is the thin invoke wrapper with a dev fallback. |
| `ai-first-docs/stack/svelte/design_protocol.mdx` | Svelte 5 rune conventions the new `AppConfigView.svelte` should match, not diverge from. |
| `.claude/memory/MEMORY.md` | Standing founder preferences: cut unused surfaces cleanly (no half-removed residue), round-trip test anything parse/serialize-shaped, target current toolchain. |

## The wholesomeness questions

Frame findings around these, not a mechanical checklist:

1. **Residue from the old settings editor.** Is there anything left over — a dead export, an
   unused import, a CSS class with no remaining consumer, a doc reference to a deleted file, a
   `SettingsTier`-shaped concept surviving somewhere it shouldn't — that a future reader would trip
   over and wonder "wait, didn't we delete this system?"
2. **The new command-registry entry.** `resume_in_terminal`'s signature grew a parameter. Does
   every caller genuinely pass a meaningful title, or did any call site fake/stub it just to
   satisfy the compiler? Does the Tauri command registration list still match the actual set of
   `#[tauri::command]`-annotated functions (no orphaned registration for a deleted command, no
   command defined but never registered)?
3. **The shared script-builder — is it actually shared, or "shared" in name only?** The design
   intent was one pure function every OS/terminal path calls, replacing three duplicated
   command-string builders. Confirm this actually happened — grep for any place that still
   hand-builds a `claude --resume` string outside `appconfig::build_resume_script`/
   `build_resume_script_windows`.
4. **Shell-injection surface.** `launch_command` is now fully free-text, run as a shell script.
   The design's safety property is "env-var injection, not string substitution" — confirm
   `session_id`/`session_title`/`cwd` are shell-quoted before being exported as env vars (not
   spliced unquoted into the script), and that `launch_command` itself is the *only* thing that
   runs with the user's own authored trust (the user wrote it, so it running as shell script is
   expected — that's the feature). Flag if `session_title` (attacker-influenced if a session file
   were ever crafted maliciously — it comes from parsed chat content) could break out of its
   shell-quoted export and inject into the *following* script lines.
5. **Adjacent-surface avoidance.** Did the build punt anything to a cheaper-to-touch surface
   instead of the correct one — e.g., does the update-check toggle actually gate the right call
   site (`+layout.svelte`'s silent launch check), leaving the manual "check for updates" button
   correctly ungated? Does `AppConfig`'s `Default` impl actually match what `#[serde(default)]`
   field-level attributes produce (a mismatch between the two is exactly the "two representations
   agree with each other yet both are wrong" trap)?
6. **Naming/doc coherence.** Does `roadmap.md`'s new Phase 12 entry accurately describe what
   shipped (spot-check a few claims against the actual diff, don't just trust the prose)?

## Out of scope

Issues #20 (fuzzy schema search) and #21 (provider profiles / credential storage) are not part of
this build — don't flag their absence as a gap.

## What auditors may fix in-pass vs. flag

Per this project's audit convention: fix in-pass only if the change is single-file, local-semantics
(no signature/contract change), and mechanically obvious (a dead import, a stale doc line, a typo).
Flag anything touching more than one file, changing a signature, or requiring a judgment call —
list it under `Fixed in-pass` vs `Findings` respectively.

## Response format

Write your report to `.claude/work/prompt_report/appconfig_relaunch_audit_report.md`, ~500 words:

- **Fixed in-pass** — table: file | what | why it was safe to fix directly (or "None")
- **Findings** — severity-tagged (CRITICAL/HIGH/MEDIUM/LOW/NIT) list, each with file:line and a
  one-sentence failure scenario, not just "doesn't match pattern X"
- **Out-of-scope but flagged** — pre-existing issues noticed incidentally (or "None")
