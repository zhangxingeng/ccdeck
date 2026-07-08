# Iterative Teammate Protocol (ccdeck)

**Status:** active. This is ccdeck's project-specific operational recipe for
**event-based, steerable teammates** — agents the lead (manager) holds a live,
multi-turn dialogue with, in contrast to one-shot passive workers.

It is the *concrete pipeline*. The *mindset* and *mechanism* live in the generic
corpus and are not repeated here — read them first if you haven't:

- `ai-first-docs/orchestration/team_management_protocol.mdx` — the lead-side
  paradigm: coordination as an open channel, the approval gate that *emerges*
  from that channel, casting narrow specialists, lead-orchestrates-a-workflow.
- `ai-first-docs/craft/team/teammate_execution_protocol.mdx` — the callee side:
  a standing specialist delivers only via explicit message, surfaces lane edges
  live, holds the gate as dialogue.
- `ai-first-docs/orchestration/agent_teams_protocol.mdx` — the mechanism and its
  load-bearing caveats (resume gap, idle-may-need-a-poke, one-team-per-session).

**This is *not* the passive concurrent-agent model.** A passive worker gets one
brief, runs to completion, returns once, and cannot be talked to. A teammate here
is reachable across turns: it reports, we review, we approve or nudge, it
continues. Different mindset — hold the integration picture and *steer*, don't
front-load one perfect brief.

---

## The pipeline

Each teammate moves through gated phases. The lead reviews at every `▸ STOP`.

```
  spawn (worktree + sonnet)
        │
   [1] INVESTIGATE ──▸ STOP: brief plan (≤200 words), await approval
        │  lead reviews. approve → nudge with corrections until the plan is right.
   [2] IMPLEMENT + COMMIT ──▸ STOP: report diff summary + commit SHAs + branch
        │  lead reviews the actual diff. good → nudge (append-only) until good.
   [3] UPDATE ISSUE ──▸ STOP: teammate comments/updates its GitHub issue
        │
   [4] lead INTEGRATES (merges worktree branch into main), then pushes + verifies
```

### Standing rules

1. **Spawn with worktree isolation + `sonnet`, always.** Every teammate runs in
   its own git worktree (`isolation: "worktree"`) so concurrent teammates never
   collide on the working tree. Model is **sonnet**, explicitly, for these
   fix/refactor tasks.

2. **Commit early, commit often — append-only. This is the durability contract.**
   The teammate MUST commit its work in its worktree before reporting, and on
   every subsequent revision. **Never** amend, reset, rebase-away, force-push, or
   otherwise *remove* an existing commit — corrections are *new* commits stacked
   on top. Rationale: commits are the only artifact that survives a lost teammate
   (a compaction/resume can silently sever the live channel — see the resume gap
   in `agent_teams_protocol.mdx`). If a teammate vanishes, its committed branch is
   still there and the lead can inspect it, finish it, or respawn against it. A
   teammate that did great work but never committed = work lost. So: **commit is
   how you deliver, not a nicety.**

   **Field-proven sharpening (first run, 2026-07):** a harness-created isolation
   worktree with *no commits yet* is reclaimed as "unchanged" at the idle/resume
   boundary — i.e. exactly when a teammate goes idle after Phase-1 (investigate,
   plan-only) to await approval. Two of three teammates found their original
   worktree and branch gone on resume; both recovered by re-establishing a fresh
   worktree off `main` and redoing the (still uncommitted, so not-yet-lost) work.
   Lessons baked in: (a) the teammate should commit *something* — even a WIP
   commit — before going idle if it has any edits in flight; (b) a teammate that
   finds itself back on `main` in the bare checkout must re-create its own
   worktree/branch rather than working on `main`; (c) the lead must **not** assume
   the `agentId`-named branch survived — always re-confirm the actual branch name
   from the teammate's report and `git branch -vv` before integrating.

3. **Gate as dialogue, not go/no-go.** Being sent back with a correction is a
   normal turn, not a failure. Nudge with specifics; the teammate revises with a
   *new* commit and resubmits. Repeat until the lead is satisfied.

4. **The teammate stays in its lane.** When two teammates must touch the same
   file (common here — `SessionEditor.svelte` is a hub), the lead pre-declares
   each teammate's *region/lane* inside that file in the brief. A teammate that
   hits the edge of its lane surfaces it live instead of guessing.

5. **The lead owns integration.** Teammates never merge to `main` and never push.
   The lead merges each approved worktree branch into `main`, resolves any
   cross-lane conflicts (it holds the integration picture), then runs the full
   verify and pushes. Worktrees share the repo's object store and refs, so a
   teammate's branch is visible from the main checkout with no push/pull — merge
   is local.

6. **Verify on trunk, once, after integration.** Individual teammates may run
   scoped checks in their worktree, but the authoritative gate is the lead running
   the project check on the merged `main`:
   `pnpm check && cargo test --lib --manifest-path src-tauri/Cargo.toml`
   (`check_cmd` in `project_profile.yaml`). Only push once this is green.

7. **Issue hygiene is the teammate's phase-3 job, the lead confirms.** The
   teammate updates/comments its own issue (what shipped, commit refs). Auto-close
   keywords (`Fixes #N`) in the *merge/commit* are the lead's call at integration,
   so the issue closes when the work actually lands on `main`, not before.

### Idle-may-need-a-poke

Per `agent_teams_protocol.mdx`: a teammate can go idle *after* a message lands in
its inbox (an approval, a nudge) without acting on it, or idle right after its
first dispatch without starting. Treat idle as "may need a poke" at any point —
if a teammate parks without delivering the expected report, nudge it once.

---

## Why worktrees here specifically

ccdeck's fix backlog clusters on a few hub files (`SessionEditor.svelte`,
`MessageCell.svelte`, `BrowseView.svelte`, `+page.svelte`). Two teammates on the
same tree would clobber each other mid-edit. Worktrees give each its own tree; the
lead reconciles at merge. Where lanes *are* knowable up front (e.g. "you own the
`showToast` region, you own the restore-UI removal"), the lead pre-declares them
in the briefs — pre-partitioning inside the shared file — so the merge is
mechanical rather than a semantic three-way tangle.
