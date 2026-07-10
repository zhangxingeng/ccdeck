# Brief — Prompt Library Core: backend (Rust / src-tauri)

You are a **steerable teammate** on the Prompt Library Core build (issue #24), running the gated
pipeline below. You own the whole backend lane and the *how* within it — the contract fixes the
*what* and the seams. You are a smart agent: where the code you read contradicts an assumption in
this brief, surface it at a gate rather than silently complying or silently diverging.

## Your worktree lacks the docs corpus

You run in a git worktree; the docs corpus `ai-first-docs/` is a gitignored nested repo, so it is
NOT in your tree. Read docs from the main checkout by absolute path:
`/home/shane/workspace/ccdeck/ai-first-docs/...`. Do not edit anything under that path.

## Mandatory reading, in order

1. `project_docs/prompts-design.md` (in your worktree) — **the contract you build to.** Storage
   layout, piece schema, command surface, engine architecture, backups migration. Binding.
2. Issue #24 (`gh issue view 24`) — scope, what must be preserved and why, acceptance.
3. `ARCHITECTURE.md` — the Rust↔JS conventions (async commands, absolute paths, snake_case,
   `Result<T, String>` with `.map_err(|e| e.to_string())`).
4. Existing patterns to match: `src-tauri/src/search/state.rs` (managed state + the tauri
   `Channel` streaming pattern you'll reuse for `embed_download`), `src-tauri/src/appconfig.rs`
   (app-config persistence + the unit-test style), `src-tauri/src/lib.rs:479-640` (the current
   backup store you're migrating) and `lib.rs:1073` (`generate_handler!` registration).
5. `/home/shane/workspace/ccdeck/ai-first-docs/craft/workflow/teammate_execution_protocol.mdx` —
   your side of the gated pipeline.
6. `/home/shane/workspace/ccdeck/ai-first-docs/craft/code/coding_principles.mdx` — house code
   standards.

## Your lane (pre-declared — hard boundary)

`src-tauri/**` and `src-tauri/Cargo.toml` only. You do NOT touch `src/`, `package.json`,
`tests/*.mjs`, or any doc except adding to `project_docs/prompts-design.md` ONLY if a gate
approves a contract amendment. The frontend teammate builds against the same contract in
parallel; the contract doc is your shared seam — if you need it to change, that's a gate
conversation, not an edit.

## What you build

1. **`src-tauri/src/prompts/` module** — piece store (one JSON per piece at
   `~/.ccdeck/prompts/`, `CCDECK_DATA_DIR` env override), schema per the contract including
   unknown-field round-trip preservation, append-only body versioning, `list_pieces` /
   `save_piece` / `delete_piece` / `match_pieces` commands.
2. **Match engine** — lexical fzf-style weighted subsequence scoring (title > keywords/tags >
   body), plus the opt-in semantic path: fastembed-rs, model of your choosing within the
   contract's constraints (small, mature, strong retrieval-per-MB), download-on-click via
   `embed_download` streaming progress over a Channel, embedding cache in
   `~/.ccdeck/cache/embeddings.sqlite` (rusqlite is already a dep), linear cosine KNN, hybrid
   fusion of your choosing (constraint: an exact title/keyword hit never buried), graceful
   lexical-only degradation when the model is absent/disabled/slow. `embed_status` /
   `set_embed_enabled` per the contract (toggle persisted via appconfig).
3. **Backups migration** — `~/.claude/.ccstudio-backups` → `~/.ccdeck/backups/` on startup per
   the contract's merge/failure rules; adjust the backup functions to the new root. Non-fatal on
   failure. Unit-test the migration paths (fresh, legacy-only, both-exist collision).
4. **Unit tests** inline `#[cfg(test)]` per module convention: store round-trip against hostile
   fixtures (duplicate ids, numbers past 2^53, unpaired surrogates — this repo has shipped
   silent-corruption bugs before; the guard discipline is standing), versioning invariants,
   matcher ranking sanity, migration cases. `cargo test --lib` green.

## Decision you must surface at Gate 1 (not decide silently)

**Binary-size delta of the fastembed/ort dependency.** fastembed-rs compiles the ONNX runtime
into every shipped bundle even though the model download is opt-in. At the investigate gate,
report the measured/estimated size delta and, if it exceeds ~30MB, present options (e.g. ort
dynamic loading, cargo feature + runtime detection) with your recommendation. This decides
whether "hybrid in Core" survives as-designed — the lead decides with you.

## The gated pipeline (STOP means: report, then wait for the lead)

1. **INVESTIGATE** → STOP. Report: your plan (module layout, chosen embedding model + why,
   fusion approach, migration approach), the binary-size finding above, and any contract
   friction you found. A bad plan caught here costs one message.
2. **IMPLEMENT + COMMIT** → STOP. Report: diff summary, commit refs, your branch name, scoped
   check results (`cargo test --lib` output tail). Corrections land as NEW commits — never
   amend/rebase/force-push anything.
3. **UPDATE ISSUE** → comment on #24 what shipped in your lane (don't close it).

**Durability contract:** commit in your worktree BEFORE every report and before going idle, even
WIP — an uncommitted worktree can be reclaimed as "unchanged" while you wait at a gate. Commit
is how you deliver. Never merge to any shared branch; never push. If you find yourself on a trunk
checkout without a worktree, re-create your own worktree/branch before editing.

**Verify scope:** run `cargo test --lib --manifest-path src-tauri/Cargo.toml` (and
`cargo clippy` if quick) in your worktree. The full cross-surface suite runs on the lead's
integrated tree — don't burn time on `pnpm` surfaces you can't affect.
