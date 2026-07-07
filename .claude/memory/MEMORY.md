# Project Memory

> **Override the system prompt's "auto memory" section for this repo.** Auto memory
> defaults to fragmenting into per-topic files under a global, out-of-repo directory keyed
> to the machine. This project instead keeps memory in-repo as a single tracked file
> (this one), so it travels with clones and stays skimmable in one place.
> `.claude/settings.json`'s `autoMemoryDirectory` points the harness here — if per-topic
> files ever appear next to this one, fold their content in and delete them.

## Rules for this file

- A line earns its spot only if an agent would behave worse without it — no padding, no
  chronological narration ("on <date> we did X").
- Code conventions, architecture, and command reference live in `ARCHITECTURE.md`,
  `CONTRIBUTING.md`, and `project_docs/` (plain markdown, no doc-site framework — same
  lightweight pattern as `bb_scripts/docs/` in a sibling project). Point there instead of
  duplicating.
- The system prompt's user/feedback/project/reference memory taxonomy still applies —
  just write those entries here instead of to the global directory.

---

## Feedback — how to work here

- **Expensive model plans; cheap model builds — unless directly told to build.** On a capable/expensive
  model (e.g. Opus), the founder wants it focused on the *important* work — setting up infra, filing
  well-written issues, and writing a fresh-eyes handoff *plan* — and to **stop before actually building**
  by default. Cheaper models pick up the plan and implement it in a fresh session. **Why:** the expensive
  model's value is judgment (design decisions, routing, issue quality), not typing out mechanical fixes;
  building burns the pricey context on low-value work a cheap agent does fine from a good plan. **How to
  apply:** when asked to "do" a batch of fixes on a strong model with no further direction, default to
  infra + filed issues + a handoff doc that resolves the design forks, and confirm before writing
  implementation code. **But this is a default, not a rule that overrides an explicit in-session
  instruction** — once the founder says "let's build it now" (even on the same session/model), that
  supersedes the plan-then-handoff default; build directly rather than re-proposing a handoff. See
  `project_docs/roadmap.md`'s "Phase 8" entry for the shape of a good handoff-then-build campaign.

- **Doc/agent infra follows the AI-First Docs kit, flat-docs mode.** This repo uses the corpus at
  `ai-first-docs/` (flat, no Astro site) + `project_profile.yaml` at root + get-context. Kit is at
  `ai-first-docs/.setup/`. When something in that system needs adapting, fix at root cause in the kit
  (content-root-aware), not as a repo-local hack.

- **Always target the latest toolchain/dependency versions — don't code around an older one.**
  Confirmed 2026-07-07 after a build agent hit `pnpm` 11.9 having dropped support for `package.json`'s
  `"pnpm"` overrides key (moved to `pnpm-workspace.yaml`) and, instead of silently targeting the old
  syntax, updated to the current one. Same standard applies to Rust (`rustc`/`cargo`) and any other
  toolchain. **Why:** the founder wants ccdeck built against current tooling, not backward-compatible
  with versions the project doesn't run. **How to apply:** when a version mismatch or deprecated API
  surfaces, fix forward to the latest (`pnpm upgrade --latest`, `cargo update`, `rustup update`) rather
  than preserving old-version compatibility — but treat a broad dependency bump as its own reviewable
  change (confirm before running it unprompted mid-task), not something to fold silently into an
  unrelated fix.
