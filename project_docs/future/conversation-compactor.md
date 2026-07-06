# Conversation Compactor — Future Feature Plan

Status: **FUTURE / not scheduled.** Docs-only entry in the future-plan folder (our low-churn,
no-GitHub-issue backlog). **Do not build this in the current version** — the current direction
trims and *demotes* the chat viewer/editor (see the repositioning issues), so a new editor feature
is explicitly out of scope for now. This records the idea and, more importantly, the design insight
that makes it worth doing later.

## The idea

A **selective, hybrid compactor** for a session's history inside CC Deck: instead of Claude Code's
all-or-nothing `/compact` (summarize everything the model isn't actively holding), let the user
**mark specific messages or ranges to keep verbatim** and have the tool summarize only the rest.
The output is a shorter, still-coherent session where the parts that still matter survive word-for-word
and the filler collapses into a summary.

This is the fleshed-out version of the "Selective / smarter chat compaction" line under
`project_docs/roadmap.md` § Future ideas.

## The insight that makes it clean (from Claude Code's own format)

Claude Code already proves the pattern: its compaction is **not** pure summarization — it keeps a set
of messages **verbatim** alongside the summary. In a session `.jsonl`, a compaction appends two lines
without deleting anything (the file is append-only):

- a `{"type":"system","subtype":"compact_boundary"}` marker whose `compactMetadata` records
  `trigger` (manual/auto), `preTokens`/`postTokens`, `durationMs`, and — the key part — a
  **`preservedMessages`** list of `uuid`s that were carried through **un-summarized**;
- a `{"type":"user","isCompactSummary":true}` message whose `message.content` is the summary prose
  for everything else.

So "keep some messages, summarize the rest" is not a novel mechanism we'd invent — it's the shape the
CLI already ships, and it's simple: a summary blob + a preserved set, with the raw history left intact
above the boundary.

## Why it fits CC Deck

- **Non-destructive by construction.** Because CC Deck reads the same append-only JSONL, a compaction
  can be written the way the CLI writes it — append a summary + a boundary that names the preserved
  `uuid`s, and leave the original lines untouched above it. Nothing is lost; a "show full history"
  toggle can always re-expand. This sidesteps the data-loss worry that would otherwise make an
  editing feature scary.
- **Beats blanket `/compact` for complex sessions.** The founder's original motivation: blanket
  compaction loses detail that still matters. Letting the user pin the messages that matter and
  collapse only the rest keeps the signal.
- **Local & private**, consistent with the rest of the app — the summary can be produced by a
  bring-your-own-key model call (off by default), never leaving the machine.

## Rough shape (for whenever this is picked up)

- **Selection UI**: in the (trimmed) viewer, let the user check messages/turns to *keep*; everything
  unchecked is a candidate for summarization.
- **Summarize the rest**: one model call over the unchecked span → summary text. (Reuse the same
  OpenAI-compatible, BYO-key interface noted for the Prompt Library's future LLM features, so there's
  one AI-config surface.)
- **Write it like the CLI**: append a summary entry + a `compact_boundary`-style marker recording the
  preserved `uuid`s; keep raw lines above. Optionally mark our own boundary so CC Deck can distinguish
  a user-driven compaction from a CLI one.
- **Reversible view**: a toggle to expand back to the full pre-compaction history (trivial, since it's
  all still on disk).

## Open questions (settle at build time, not now)

- Does writing our own compaction into a session the real CLI later resumes cause any confusion for the
  CLI's own reader? (It reads `isCompactSummary` / `compact_boundary`; verify our appended shape is
  benign or clearly namespaced.)
- Granularity: per-message, per-turn, or arbitrary range selection?
- Auto-suggest which messages are low-value to summarize vs. leaving it fully manual.
- Interaction with CC Deck's plain-edit save path (post-viewer-trim) — compaction is a *write*, so it
  shares the same on-disk-write discipline.

## Relationship to current work

- **Depends on nothing scheduled.** It builds on reading the session JSONL, which CC Deck already does.
- The current viewer-trim work removes CC Deck's old diff/undo/backup machinery; this feature does
  **not** need it — the append-only, CLI-mirroring approach above is its own safety net. (Noted because
  the roadmap's older phrasing assumed it would lean on that now-removed infrastructure.)
