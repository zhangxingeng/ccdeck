# Brief — Prompt Library revision round, BACKEND lane (Rust)

You are an opus teammate on ccdeck's `prompt-library` branch, working in a git worktree. You own
the Rust side of the founder's post-feel-check design revision (issue #24 continuation). You are
a smart agent: the contract fixes the WHAT at the seams; the HOW inside your lane is yours.
Decisions that only become visible after reading code are yours to make — but a decision that
changes the seam contract or the founder's stated design comes to the lead at a gate, never
improvised.

## The contract — read first, it is the spec

`project_docs/prompts-design.md` (in your worktree, freshly amended). Everything below is
orientation on top of it, not a replacement for reading it.

## Your lane (exclusive) — `src-tauri/**` only

A frontend teammate works `src/**` in parallel. The seam is the command contract in the doc —
if you find the contract wrong or ambiguous, STOP and raise it at your gate; do not adjust the
shapes unilaterally (last round's audit found exactly one cross-side divergence — placeholder
grammar — born from each side "fixing" the seam alone).

## The work

1. **Project roster**: `~/.ccdeck/projects.json` per the contract's Project model —
   list/save/delete commands, palette-key colors (validate against the fixed key set), pinned
   flag. `delete_project` rescopes that project's pieces to global (nothing vanishes).
2. **Scope v2**: piece scope becomes `{kind:"project", project_id}` — legacy/unknown scope loads
   as global + a `piece_load_errors` entry, file untouched. No dual-schema machinery (feature
   never shipped in a release; only the founder's feel-check data exists).
3. **Variable grammar** (the seam that must not diverge): implement the contract's § Variable
   grammar exactly — escapes first, `[A-Za-z0-9_-]+` names, first-colon defaults — and encode
   ALL the shared test vectors as unit tests verbatim. Placeholder derivation at save now emits
   `{name, default?}`.
4. **Store robustness**: on parse failure, attempt in-memory jsonrepair-style recovery. First
   verify whether a mature, maintained Rust crate exists (research before hand-rolling; judge
   maturity — downloads, recency, tests); else port bounded semantics (unquoted keys, trailing
   commas, comments, single quotes, truncation). Recovered pieces get transient
   `recovered: true`; the loader NEVER rewrites the user's file — the repaired form persists
   only on the next explicit save (which versions, as any body change).
5. **Embed flow gains the index stage**: `embed_download` progress events become
   `{stage: "runtime"|"model"|"index", done, total}` (bytes / bytes / piece counts) and the
   flow ends by embedding the existing library — the popover's "Download & index" must be
   literally what one click does. Keep the Core security posture untouched: pinned URLs,
   sha256-verified-before-any-write-or-load, degrade to lexical on failure.
6. **`match_pieces`** takes `project_id: string | null` (pool: global + that project).
7. **App config** gains `prompts_as_variable: bool` default `true`, reachable through the
   existing config get/set path so the frontend can persist the compose toggle.

## Quality mandate (founder's words: this is the tweaking/optimization pass)

The skeleton exists; enforce the standards on everything you touch — refactor touched code up
to standard, don't tiptoe around it. Mandatory reading (the docs corpus is gitignored and NOT
in your worktree — use these absolute paths into the main checkout):

- /home/shane/workspace/ccdeck/ai-first-docs/craft/code/coding_principles.mdx
- /home/shane/workspace/ccdeck/ai-first-docs/craft/workflow/git_protocol.mdx
- /home/shane/workspace/ccdeck/ai-first-docs/craft/workflow/teammate_execution_protocol.mdx

Plus in-worktree: `ARCHITECTURE.md` (command conventions), `CONTRIBUTING.md` (verify commands).
Existing clippy baseline has 6 pre-existing warnings — add zero new ones.

## Pipeline (gated — do not skip gates)

1. **INVESTIGATE**: read the contract, the docs, and the existing `src-tauri/src/prompts/` +
   `datadir.rs` + `appconfig.rs`. Produce a short plan: files touched, the shapes you'll add,
   risks, and any contract ambiguity you found. **STOP and report — wait for lead approval.**
2. **IMPLEMENT + COMMIT** on approval: conventional commits, cohesive chunks. Commit before
   every report; corrections are NEW commits — never amend/rebase/force-push (durability
   contract: your commits must survive you). Verify:
   `cargo test --lib --manifest-path src-tauri/Cargo.toml` green, no new clippy warnings.
   **STOP and report** what you built, what you verified, what you judged and why.
3. The lead integrates; you may get correction rounds — each lands as new commits.
