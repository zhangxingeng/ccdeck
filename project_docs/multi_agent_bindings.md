# Multi-Agent Bindings

Resolves issue #17. Records ccdeck's actual multi-agent posture so the next reader doesn't
re-discover the gap the issue flagged.

**Posture: not solo-agent-only, but not concurrently-self-claiming either.** Phase 14
([`project_docs/roadmap.md`](roadmap.md), "Phase 14 — Editor simplification…") already ran three teammates
concurrently, each worktree-isolated, under the **iterative teammate protocol**
(`ai-first-docs/orchestration/iterative_teammate_protocol.mdx`). That is the live precedent this
repo runs on — the lead spawns and steers each teammate explicitly through the gated
investigate → implement+commit → update-issue pipeline; teammates never pull work off the tracker
on their own.

- **Issue-claim mechanism: N/A.** The claim requirement in
  `ai-first-docs/craft/workflow/issue_driven/issue_driven_development_protocol.mdx` (§ Claiming) only bites when agents *self-select* work
  from a shared backlog. ccdeck's concurrency is lead-assigned, not backlog-pulled — the lead hands
  each teammate its issue in the spawn brief, so there is no race to arbitrate and no claim
  primitive to bind.
- **Digest channel: N/A.** No periodic automated sweep/audit routine runs against this repo yet.
  Status reporting today is per-issue, via the gated pipeline's "update issue" phase — there is no
  standing health-check output that would need a low-noise channel.

Revisit both if ccdeck ever adds backlog-pulling agents or a scheduled audit routine.

LLM-tier bindings (`build_provider`, `audited_providers`, `cold_audit_provider`) are filled in
[`project_profile.yaml`](../project_profile.yaml) as `"sonnet"` — the provider that actually ran the Phase 14 build.
