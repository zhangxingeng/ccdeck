---
name: cut-release
description: "Use when about to cut, tag, or publish a CC Deck release — the version-bump file set, the lockfile step people forget, and what CI does on tag push so you don't duplicate it."
---

# Cut Release

CC Deck releases are tag-driven: you bump versions and push a tag; CI builds the installers and
creates the GitHub Release. There is no release wrapper script — the steps below ARE the flow
(shape recovered from the v0.11.0 release, commit `cc01870`).

## Steps

1. **Green tree first.** `pnpm check && pnpm run test:smoke && cargo test --lib --manifest-path src-tauri/Cargo.toml` — never tag a red tree; the tag is what users' updaters see.
   Also run `cargo test --lib --manifest-path src-tauri/Cargo.toml -- --ignored` once (network
   e2e, ~20s): it live-verifies the pinned embedding-artifact URLs + sha256s in
   `src-tauri/src/prompts/embed.rs` — the one contract no offline test can check, and a stale
   pin ships a broken opt-in download.
2. **Bump the version in all THREE files** (they must agree — Tauri reads its own two, npm reads the third):
   - `package.json`
   - `src-tauri/tauri.conf.json`
   - `src-tauri/Cargo.toml`
3. **Sync the Cargo lockfile:** `cargo update -p ccstudio` (the crate is named `ccstudio`, not ccdeck). Skipping this leaves `Cargo.lock` at the old version and the release build dirties the tree in CI.
4. **Commit** the four files: `Bump version to X.Y.Z`.
5. **Tag and push:** `git tag vX.Y.Z && git push origin main vX.Y.Z`.
6. **CI does the rest** — `.github/workflows/release.yml` triggers on the `v*` tag: tauri-action builds macOS/Windows/Linux installers, creates the GitHub Release (name, body, artifacts) itself. Do **not** also run `gh release create` — and know that it wouldn't stick anyway: tauri-action creates-or-UPDATES the release for the tag and **overwrites a hand-crafted title/body** (verified on v0.11.0: its manual "Provider profiles" title was clobbered by the workflow's "CC Deck v0.11.0"). Release copy changes belong in release.yml's `releaseBody`, not in a manual release edit.
7. **Verify:** `gh run watch` (or check Actions) until the matrix finishes, then `gh release view vX.Y.Z` — all platform artifacts attached.

## If the build fails after tagging

Fix forward: land the fix on main, delete and re-push the tag only if the release never published
(`gh release delete` + `git push --delete origin vX.Y.Z` + retag) — an already-published release
that updaters may have seen gets a new patch version instead, never a moved tag.
