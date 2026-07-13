# Prompt Library — Interaction Contract

What the user *does*, and what happens back — scenario by scenario. The sibling to the
[engineering contract](prompts-design.md), which says what the system *is* (storage, command
surface, grammar, match engine, the compose model). Behavior questions belong here; seam shapes
belong there.

**Amend this doc first when an interaction changes.** A behavior that lives only in the code is a
behavior nobody agreed to.

The bar this product is held to: **the founder can open the app after two weeks away and still know
how everything works, without reading anything.** The v0.12 library failed that test — nine UI
surfaces and ~15 affordances, several of them unguessable without having read a contract. Every rule
below is chosen so a user can *infer* what a key does in a situation this doc never enumerated. When
you add an affordance, that is the test it has to pass.

---

## Conventions — the rules every scenario inherits

- **Enter accepts. Escape cancels the innermost open thing.** In the popup, Escape closes and
  returns you to the box; in the project manager, Escape closes.
- **The compose box is the one deliberate exception: Enter is a newline**, because you are writing
  prose and a multi-line prompt is the primary use. The match panel earns Enter only *after* you
  have explicitly stepped into it — see [S3](#s3-search-by-typing-arrow-in-enter-inserts).
- **The box is the only place you type a prompt. The popup is the only place you edit a snippet.**
  Those two sentences are the whole interaction model.
- **Toasts are transient — 5 seconds, or click to dismiss.** Nothing durable hides in one, so losing
  one costs nothing.
- **Nothing the app does deletes a file you did not ask it to delete.** Removing a project forgets a
  path; deleting a snippet from the popup removes exactly that one file.

---

## The surface at rest

Top to bottom: the **project tab row** (one tab per folder, plus a `⋯` manager), then two columns —
the **library panel** on the left (collapsible with `⟨`; a `⟩ Library` peek button brings it back)
and the **compose box** on the right. Under the box, the **variable fill list** appears whenever the
composed prompt contains variables. Two situational buttons sit inside the box, only when it has
content: `Save as snippet` (bottom-left) and `Copy prompt` (bottom-right).

**There is no Global tab, and its absence is the point.** A snippet lives in the folder it sits in,
so a scope belonging to no folder cannot exist. There is also no settings gear, no project colors, no
pins: a project is a name and a folder, and there is nothing else about it to decorate.

---

## S1. First launch — there is no library until you point at a folder

- **What you see.** No tabs. The library panel says: *No prompt folder yet. Add one with `⋯` above —
  pick any directory and every `.md` file in it becomes a snippet.*
- **What you do.** `⋯` → `+ Add a folder…` → the OS directory picker. The folder's own basename
  becomes the project name (you already named it once when you made it); rename it later in the
  manager if you like.
- **Result.** The folder is registered and becomes the active tab — you added it to work in it, so
  the manager closes and gets out of the way. Every `*.md` under it, recursively, is now a snippet.

Trying to save a snippet with no folder configured toasts *"Add a prompt folder first — ⋯ above the
compose box."* rather than failing quietly.

## S2. Type a prompt, copy it

- **Keys.** Type in the box. `Copy prompt`, or `Ctrl/Cmd+C` with nothing selected.
- **Result.** The composed prompt — typed text plus **every chip's body**, not the chip labels you
  see — goes through the copy pipeline (escapes resolved, variables rendered per each one's as-var
  toggle) and onto the clipboard. A toast confirms.

## S3. Search by typing, arrow in, Enter inserts

Each line you type is a live query. The library panel **filters down**: at rest it already lists
every snippet in the project, most-recently-used first, and typing narrows it. You never have to
type to make your own library appear.

- **Keys, in order.**
  1. Type a line — `senior review`. The panel narrows to matches, ranked.
  2. `↓` **steps focus into the panel**, highlighting the first hit.
  3. `↑`/`↓` move the highlight. `Enter` inserts. `Esc` (or `↑` past the first hit) returns focus to
     the box, leaving your typed line intact.
- **Result of insert.** The snippet lands as a **chip** that *replaces the query line* — the text
  from the line start to the caret. The caret sits just after the chip; focus is back in the box; the
  snippet's variables merge into the fill list. The insert is also what records "recently used",
  which is what makes the panel open on the things you actually reach for.

**Two rules that are really one decision — change either and you must change both.**

`↓` steps into the panel **only when the caret sits at the very end of the text** (and the panel has
a hit to land on). That is the one position where `↓` is natively inert in a text box; anywhere else
it moves the caret, as a user editing line 3 of a 10-line prompt rightly expects. And `Enter` inserts
**only after** that explicit step — the first hit is never pre-armed while the caret is still in the
box.

Why they are one decision: the query is the *whole current line*, so an insert that replaces the
query line also deletes whatever prose shares that line. That is safe only because the user opted in
by stepping into the panel. Pre-arm the first hit and a stray `Enter` at the end of an ordinary
sentence swallows the sentence. If the pre-arm rule is ever overruled, insert must go back to
appending at the caret rather than replacing.

Mid-document insert stays a mouse click — a known and accepted gap, because composing happens at the
end.

## S4. Insert with the mouse

Click a hit. Identical result to [S3](#s3-search-by-typing-arrow-in-enter-inserts): the query line is
replaced by the chip, the caret lands after it, focus returns to the box. **One insert path, two
triggers.**

Panel rows show the snippet's **name and nothing else**. The name is a path (`rust/code_review`), so
it already carries the folder grouping that replaced tags — and since the panel lists the *whole*
library at rest, a body preview per row would make it unscannable rather than informative.

## S5. A chip is a chip — you cannot edit it in the box

An inserted snippet renders as a **chip**: a small button showing its **name** and the **variable
names its body contains**. Not its body. Not a preview.

The founder's reasoning, which is the load-bearing rationale: *"I actually rarely read it. If I
really want to read it, it means I want to edit it. And if I want to edit it, I would click into
it."* Body text in the box is clutter that serves no reader.

- **Click a chip (or `Enter`/`Space` when it has focus) → the popup opens. Always. No exceptions.**
  Its value is that the interaction is *predictable*: you never have to ask "do I edit this here, or
  click something?" The answer is always "click the chip."
- **Backspace deletes the whole chip.** It is one atom to the browser — arrow keys step over it,
  selection takes it whole or not at all, and there is no caret position inside it.
- **Free-typed text around chips stays freely editable.** The box is still a text box.

This is not a guard that could be relaxed; it is what the model *is*. The old span-tiling model made
inline editing structurally possible, and inline editing was the defect — it silently diverged the
composed text from the stored snippet and wrote nothing to disk. See the engineering contract's
[compose model](prompts-design.md#the-compose-model--why-chips-are-atoms) before you consider
"just allowing a quick edit in the box."

## S6. The popup — the one and only edit surface

Opened two ways, and only two: **clicking a chip**, or **saving typed text** ([S8](#s8-save-what-you-typed-as-a-snippet)).

Fields: **Name**, **Content**, and fill inputs for the variables *that content* uses. Opened from a
chip, focus lands in the content (that is what you came for); creating from typed text, it lands in
the name.

| Action | Effect |
|---|---|
| **Save** | Writes `<name>.md`. **Same name → updates. New name → a new snippet**, original untouched. The chip then reflects whichever snippet it now actually is. |
| **Use once** | Applies the edit to **this chip, in this prompt only**. Nothing is written to the library. *(From a chip only.)* |
| **Delete** | Removes the file, and **the chip becomes plain typed text**. Two-step: `Delete` → `Really delete?`. *(From a chip only.)* |
| **Cancel** / `Esc` | Nothing changes. |

**Why there is no "Save as new" button.** The filename *is* the identity, so changing the name in the
popup and saving already creates a new file. A second button could only add ambiguity to a question
the name field has already answered — and the popup says which is about to happen: *"Saves as a new
snippet, `rust/code_review.md` — the original is left alone."* A `/` in the name makes a folder, and
that is the entire organization system.

**`Use once` is what makes "never editable in place" a simplification rather than a cage.** You can
tweak a prompt for one use without polluting the library with a near-duplicate — you just do it in
the one predictable place.

**Why Delete leaves the words behind.** Deleting a library file must not gut the prompt you are
halfway through writing: the link goes, the writing stays. It also leaves no chip pointing at a file
that no longer exists.

The popup's variable fills are the **same global cells** as the list under the box — it just shows
the subset this body uses. That is not a second place to edit: a variable is one value by name, and
showing one cell in two views is convenience. The one-place rule is about snippet **bodies**, where
two surfaces meant two divergent sources of truth.

## S7. Fill variables

A body like `Review {ticket} for {task}` exposes a row per distinct variable under the box — one row
per name across the **whole** composed prompt (typed text and every chip's body), in first-appearance
order.

- **A row is:** the name, a fill input, and that variable's **`as var`** toggle.
- **One name is one value, everywhere.** Two chips that both use `{language}` share a single cell.
  The model cannot tell two identically-named variables apart, so pretending they differ would be a
  fiction the UI maintains and the output discards.
- **Leave one unfilled and the prompt still works.** The input's placeholder is the literal text it
  will copy out as — `variable not set, ask user for it` — so the model asks instead of silently
  receiving a blank or a stray `{placeholder}`. There is no way to fill a variable with the empty
  string: to say nothing, delete the `{name}`.
- **`as var` defaults ON, for every variable.** ON hoists: each occurrence copies as
  `<prompt_var name="x"/>` and the value lands once in an appended `<prompt_vars>` block — state a
  long value once, reference it inline, never repeat it. OFF substitutes the value in place.

  The default is ON because the failure modes are **asymmetric**: hoisting never breaks a prompt,
  while substituting an unexpectedly-large value in place can silently bloat it. When one side of a
  choice can only cost you elegance and the other can cost you the prompt, the default takes the safe
  side and you opt out per variable. *(Two rejected alternatives, recorded so nobody re-proposes
  them: a length-based default keys off the resolved value, so the toggle would flip under the user's
  fingers as they type; an occurrence-based default optimizes token count, which is the wrong thing
  to optimize — it trades a guaranteed-safe default for a few saved characters.)*

  The setting is per-session and never written to the snippet.

**Variables are a Python format string** — `{name}` everywhere, `{{`/`}}` to escape, no carve-out for
code fences or backticks. A `{name}` inside a fenced code block *is* a variable, and you will see it:
the chip shows it and the fill list lists it, so you escape it `{{name}}` exactly as you would in
Python. The reasoning behind refusing the carve-out is in the
[engineering contract](prompts-design.md#there-is-no-markdown-awareness-do-not-add-any) — read it
before "fixing" this.

## S8. Save what you typed as a snippet

- **Keys.** `Save as snippet` (bottom-left of the box), or `Ctrl/Cmd+S`.
- **Selection-aware.** With a selection it saves the selection; with none it saves the whole box. The
  button's label says which (*"Save selection as snippet"* vs *"Save as snippet"*), so it never
  silently stores more than you meant.
- **Result.** The popup ([S6](#s6-the-popup--the-one-and-only-edit-surface)) opens prefilled with
  that text and an empty name, focused on the name.

A selection containing a chip contributes the chip's **body**, not its label — saving would otherwise
store the literal words "rust/code_review" in place of the code-review prompt itself.

## S9. Copy — the button and the hotkey

- `Copy prompt`, or `Ctrl/Cmd+C`.
- **`Ctrl/Cmd+C` is selection-aware, and this is the load-bearing edge.** If *anything* is selected
  where focus actually is — text in the box, text in a fill input, anywhere — the native
  copy-selection wins and we do not touch it. Hijacking the most reflexive shortcut on the keyboard
  is user-hostile. With nothing selected anywhere, `Ctrl/Cmd+C` copies the full composed prompt: that
  is dead key-space natively (there is nothing to copy), so we fill it without a fight.

## S10. Switch projects

Click a tab. The library panel and the match pool switch to that folder's snippets; your draft in the
box is untouched.

**There is no scope to choose and no tint to read.** A snippet lives in the folder it sits in, so
"which project does this save to?" has exactly one answer: the active one. Cross-project reference
still works — the chips you already inserted carry their bodies, so switching tabs cannot reach into
your draft.

Tabs are plain `Tab`-stop buttons, deliberately **not** a roving-tabindex tablist. The roving version
was one of the affordances nobody could guess without having read the contract, and a handful of tabs
does not need its own navigation model.

## S11. Manage folders

`⋯` opens the manager: every project as a row with its **path always visible** (a name alone cannot
tell two folders apart, and "which folder is this?" must never need a hover).

- **Add** — `+ Add a folder…`, OS directory picker.
- **Rename** — type in the row's name field. The path is the identity, so a rename is just that; it
  does not switch you to that project.
- **Remove** — two-step, and the confirm label states the consequence *before* the click rather than
  leaving it to be discovered after: `Remove` → **`Forget it? (files stay)`**. It drops the path from
  the roster. **It never deletes files.** Re-adding the folder restores the project intact. Removing
  the active project falls back to another, because a roster with no selection is a state the UI has
  no way to leave.

`Esc` or click-away closes. Focus is trapped inside while it is open — a product you can drive from
the keyboard is not actually operable if `Tab` walks focus behind an open dialog.

---

## Popovers, focus trap, and Escape

Two surfaces open over the view: the **snippet popup** (centered, modal) and the **project manager**
(under the tab row, with a click-away backdrop). Both behave the same way, and `src/lib/attachments/focusTrap.ts`
is the one implementation.

- **Focus moves in on open**, to the field you came for: the popup's content when editing a chip, its
  name when creating; the manager's first control.
- **`Tab`/`Shift+Tab` cycle within** and never escape to the page behind.
- **`Esc` or click-away closes**, and **focus returns to whatever held it before** — the chip you
  clicked, the `⋯` button, the caret in the box. You should never lose two contexts to one keypress.
- **The view's hotkeys disarm** while either is open, so a keystroke meant for a text field can never
  fire a command.

The trap is a **requirement, not polish**: a product you can drive from the keyboard is not actually
operable if `Tab` silently walks focus behind an open dialog.

---

## Hotkey map — two commands, fixed

Armed on the Prompts view, disarmed while the popup or the manager owns the keyboard. `Mod` is `Ctrl`
on Windows/Linux, `Cmd` on macOS. The global `Ctrl/Cmd+K` (go to search) still wins.

| Action | Key | Note |
|---|---|---|
| Copy the composed prompt | `Mod+C` | Selection-aware — native copy wins whenever anything is selected ([S9](#s9-copy--the-button-and-the-hotkey)). |
| Save as snippet | `Mod+S` | Selection-aware ([S8](#s8-save-what-you-typed-as-a-snippet)). Takes the key from the browser. |
| Step into the match panel | `↓` at the very end of the text | Spatial key, not a command. |
| Insert the highlighted snippet | `Enter` (in the panel) | Context key, not a command. |
| Close the innermost thing | `Esc` | Universal. |

**Rebinding was cut, and should stay cut.** Nobody ever rebound anything; the capture/conflict UI
cost ~410 lines to defend a capability with no users. Two chords, both carrying `Mod` by
construction, need no configuration surface — and the spatial keys (`↓`/`Enter`/`Esc`) were never
rebindable anyway, because rebinding them would break the conventions the rest of the design lets you
infer.

---

## What was removed, and why it stays removed

The 0.13 round was a **subtraction**. Each of these was built, shipped, and cut — so the next agent
does not helpfully rebuild one:

| Cut | Why |
|---|---|
| **The Global tab / snippet `scope`** | The folder is the project. A scope belonging to no folder cannot exist; keeping it would be the old design wearing new labels. |
| **Inline editing of an inserted snippet** (and the `linked-modified` state) | Two places to edit one thing, with different consequences and no signal which was which. The popup is the one edit surface. |
| **`Original` preview, `Update` vs `Save as new`** | Obsolete under popup-only editing — the name field already decides update-vs-create. |
| **Version history / `versions[]`** | If you want to keep the old one, save under a new name — more obvious, and a user's choice. And if the folder is a git repo, git already does this incomparably better. |
| **`keywords` / `tags` / `category`, and the browse-by-tag panel (#25)** | Never used, never surfaced. Search matches name + content; **subfolders replaced them**. |
| **The embeddings UI** — enable toggle, progress bars, `Download & index` | The engine stays and now runs silently in the background. Lexical match works immediately and unconditionally, so a failed download degrades to lexical with nothing for the user to see or decide. |
| **Hotkey rebinding, the Shortcuts section** | See above. |
| **Notices, the auto-repair notice, the config gear popover** | All of it existed to defend a JSON schema that no longer exists. A `.md` file cannot fail to parse. |
| **Project colors, pin/unpin** | Pure decoration on a thing that is now just a name and a path. |
| **Roving-tabindex project tabs** | Unguessable without the contract; a handful of tabs needs no navigation model. |

If you are about to add one back, the question to answer first is not "would this be useful?" but
"can a user who has read nothing guess it exists?"
