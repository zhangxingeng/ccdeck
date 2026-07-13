# Prompt Library — Round 2: the UX pass

**Status:** contract of record. Build teammates build against *this file*.
**Branch:** continue on `prompt-simplify` (unmerged). **Target:** still v0.13.0 — this ships *with*
round 1, not after it, so users see one change, not two.

**Round 1** (`.claude/work/prompt_library_simplification_plan.md`) rebuilt the *model*: a snippet is
a Markdown file, a project is a folder, search filters down, a chip is an atom edited only in a
popup. It is built, merged to `prompt-simplify`, and green. **Round 2 does not touch the model.** It
fixes the interaction on top of it.

The founder's verdict on round 1, which sets the tone: *"the current tag based prompt is really
nice."* The chip model works. What is missing is the **information** the chips hide.

---

## Reversals — read this first

Three round-1 decisions are **overturned**. They are listed here, loudly, because the reasoning that
cut them is still in the docs and the git history — and an agent who reads that reasoning without
reading this will "helpfully" re-apply the cut.

| Round 1 did | Round 2 does | Why it flipped |
|---|---|---|
| **Cut project colors** as decoration on "a name and a path" | **Restore them.** A project has a color, set by right-click → change color | Colors are not decoration when there are several projects — they are how you know *which library you are in* at a glance. The cut was right about pins and wrong about color. |
| **Kept the per-variable as-var toggle** (founder's explicit call last round) | **Cut the toggle. Always emit the XML block.** | *"For as-var, I think we don't need it. Default as append would be good. No need to override."* A toggle nobody flips is the archetype of the forgotten feature this whole effort exists to delete. |
| Popup actions: `Save` / `Use once` / `Delete`, all in one row | **Split by blast radius** (below) | Three peer buttons hid the only distinction that matters: which of them touch the library on disk. |

---

## The theme: a chip hides its body. Give it back on demand.

Round 1 made a chip show only its **name**, and that was right — body text in the box is clutter. But
it went one step too far: it left the user with **no way to see the text at all** without opening the
popup, and it left the match panel showing *what* matched without showing *why*.

Every item below is one instance of the same fix: **hover reveals, click edits.** Nothing is
permanently on screen that the user did not ask for; everything is one hover away.

> The founder's standard, quoted because it is the acceptance test: *"simple in UX, not simple in
> mechanism. Simple meaning good UX while being simple."* A hover-preview is more code than no
> hover-preview. It is still the simpler product.

---

## 1. The library panel (left)

**Match highlighting.** When a query is typed, highlight the matched substrings in the row. Today the
panel ranks results and shows nothing about *why* a row is there, so a hit that scored on a body
keyword looks arbitrary — the user cannot tell a good match from a coincidence.

**Hover reveals the full prompt.** At rest, a row is its name (unchanged — this is right). On hover,
expand to the **full text**, with the typed query's matches highlighted inside it. That is what makes
the ranking legible: you see the sentence you matched.

**A `+` button, right of the library header** (next to the "Library" title — *not* on a row).
This is now the **only** way a snippet is created.

**Delete the "Save as snippet" button** from the compose box, and the `Mod+S` path that feeds it.
The reason is a clean separation the founder named, and it should be preserved in the code comments:
**the compose box is for orchestrating snippets into a prompt. The library is where snippets are
made.** Mixing "author a snippet" into the composing surface is what produced the two-places-to-edit
confusion in the first place.

---

## 2. The compose box (centre)

**Hover a chip → show its text.** Same rule as the library. The chip stays a name; hovering shows
what it will contribute.

**Copy button moves to the top-right, as an icon**, semi-transparent — the affordance every code
block on the web already has. It is currently a labelled button at the bottom-right.

**Select-all + copy must copy the *expanded* prompt.** Today a native copy over a selection
containing chips yields the chips' **labels** — you would paste the literal words `rust/code_review`
instead of the code-review prompt. This is the same defect Lane B caught on the save path and fixed
there; the clipboard path still has it.

Fix: intercept the box's `copy` event and write the **flattened** text (chips → their bodies,
variables resolved) to the clipboard. `Ctrl+A` then `Ctrl+C` must produce exactly what the Copy
button produces.

> This supersedes round 1's selection-aware `Ctrl+C` rule *inside the box*. That rule exists to stop
> Copy-Prompt hijacking a copy out of a **variable fill input** — that part still stands. Inside the
> compose box, a selection copy is now *ours*, because the DOM text is wrong there.

**The variable fill list moves to the right side of the screen**, out of the space under the box, so
the box can be as wide as possible.

---

## 3. Variables — always XML, no toggle

Cut the per-variable as-var toggle entirely. Every variable is always emitted as an appended XML
block; the body keeps its `{name}` references.

An unfilled variable still resolves to the sentinel `variable not set, ask user for it` — a forgotten
variable must still produce a working prompt that makes the model ask.

**Open question (confirm by feel, do not block on it):** "append" is read literally — the block goes
at the **end** of the copy output. Round 1 emitted it at the top. If the founder wants it at the top,
it is a one-line move.

---

## 4. Projects

**A `+` button replaces the `⋯` button** for adding a project.

**Adding a project opens a native folder picker.** Not a text field for a path. The
`@tauri-apps/plugin-dialog` dependency is already present on both sides — no new dependency.

**Right-click a project tab → context menu:** *change name*, *change color*, *delete*.

**There is deliberately no "change path".** To move a library, delete the project and add the new
folder. The founder's reasoning, worth keeping: re-pointing a project at a different folder is a
rename of a thing that *is* its folder — the operation is incoherent, and offering it costs a menu
item and a confusing edge case to buy nothing that delete-then-add doesn't already do.

**No pinning.** The round-1 cut stands.

`Project` gains a `color`, persisted in `~/.ccdeck/prompts-state.json` (app-local — **never** in the
user's git-tracked prompt folder; that invariant from round 1 is absolute).

---

## 5. The popup — split the buttons by blast radius

The three actions are not peers. Two of them **write to a file on disk**; one does not. Today they
sit in one row, so nothing on screen says which is which.

```
┌──────────────────────────────────────────────────────┐
│  name  [ rust/code_review                          ] │
│  body  [ …                                         ] │
│                                                      │
│  [Delete] [Update]              [Cancel]  [ Save ]   │
│   ╰── touches the library ──╯     ╰── this prompt ──╯│
└──────────────────────────────────────────────────────┘
```

| Button | Side | Effect |
|---|---|---|
| **Delete** | left | Removes the `.md` file. The chip in the current prompt **dissolves to typed text** — deleting a library file must not gut the prompt you are halfway through writing (round-1 rule, still holds). |
| **Update** | left | Writes the `.md` file. A changed name writes a **new** file (this is still how "save as new" works — filename is identity). |
| **Cancel** | right | Discards. |
| **Save** | right | Applies the edit **to this chip in this prompt only**. Nothing is written to disk. This is round 1's `Use once`, renamed — the founder's naming, and it is better: from the user's seat they *are* just saving their edit. |

The left/right split is the whole point: **right-hand buttons affect the prompt you are composing;
left-hand buttons affect the library.** That distinction was invisible before.

---

## Deliberately dropped

**Live Markdown rendering in the compose box and popup** — rendering `**bold**` as bold while still
showing the asterisks, Enter continuing a list, and so on. The founder raised it, thought it through,
and dropped it himself: *"this is the 20% not the 80% and we drop it."* It is a rich-text editor
plugin masquerading as a small feature, on a surface whose entire redesign was about removing moving
parts. **Do not build it. Do not file it as a nice-to-have that someone picks up later.**

---

## Lanes

Four worktree-isolated teammates, iterative teammate protocol, gated
investigate → implement+commit → update-issue. **Lead owns integration.**

| Lane | Owns |
|---|---|
| **E — Library panel** | Match highlighting, hover-to-full-text, the `+` create button, deleting the compose box's "Save as snippet" path |
| **F — Compose box** | Chip hover preview, copy button (top-right icon), the expanded-copy clipboard fix, moving the fill list right |
| **G — Projects** | Colors (incl. the `Project.color` field + state persistence), `+` button, folder picker dialog, right-click context menu |
| **H — Popup** | The left/right button split and the `Use once` → `Save` rename |

**Hub-file lanes** (`src/lib/prompts.svelte.ts`, `PromptsView.svelte`): E owns the panel + create
path; F owns compose ops, fills, and copy; G owns `projects` and active-project state; H owns the
modal context. **A teammate that needs another's region surfaces it to the lead rather than guessing.**

**The gate is the same as round 1, and so is its correction:** a whole-project `pnpm check` is *not*
a per-lane gate — it is unsatisfiable across concurrent frontend lanes. Per lane: `cargo test` +
`test:smoke` + `pnpm check` clean **in owned files** + report residual errors. The lead verifies the
full suite on the merged branch **and drives the app**.

**Worktree base:** Agent-tool worktrees fork from `main`. Every brief must carry *verify your base
commit; reset with `git checkout -B <lane> prompt-simplify` if wrong.*
