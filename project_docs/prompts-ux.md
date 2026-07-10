# Prompt Library — Interaction Contract

Status: **living UX contract, in review** (founder Round-B feel-check, 2026-07-10; branch
`prompt-library`, unshipped — no release contains this feature). This is the sibling to the
[engineering contract](prompts-design.md): where that doc says *what the system is* (storage,
schema, the Rust↔JS command surface, the variable grammar, copy output), this one says *what the
user does and what happens back*. The two touch at named seams; where a UX rule here forces a
contract change, it is listed under [Contract implications](#contract-implications) and nowhere
else — the lead amends the contract from that list before any build.

It ages with the code, and it is meant to be tweaked over time. Read it as a **diff**: every
scenario is tagged **Today** (what the current code does) and **Change** (what this round asks
for) so a builder knows what to preserve and what to move. Several behaviors the founder asked
for already exist — those say so plainly, because the cheapest change is the one you don't make.

The north star, in the founder's words: *"streamline all use scenarios so the user can be as
mouse-off as possible."* Every mouse affordance in this doc has a keyboard equivalent, or the
gap is called out as a defect to fix. The product voice is **Apple, not geek**: plain, quiet,
confident. This doc's voice holds itself to the same bar.

Naming note: **`piece` is renamed to `snippet`** everywhere (UI copy, identifiers, command and
schema names) as one mechanical commit in the build phase. This doc uses "snippet" throughout;
where it names a current symbol (`ComposeBox`, `composeInsertPiece`) it names today's symbol so a
builder can find it, and the rename sweep renames those too.

---

## Conventions — the rules every scenario inherits

State these once; the scenarios below assume them. They exist so a user *infers* what a key does
in a situation this doc never enumerated, rather than memorizing a table.

- **Enter accepts. Escape cancels the innermost open thing.** In a modal, Enter saves and Escape
  closes. In a popover, Escape closes. In a rebinding field, Escape abandons the capture. The one
  deliberate exception is the compose box itself, where Enter is a newline (you are writing prose)
  — the match panel earns Enter only after you have explicitly stepped into it (see
  [Newline search](#s2-newline-search--arrow--enter-insert)). The reason to bend the rule there:
  a multi-line prompt is the primary use, and silently turning the first Enter into an insert
  would eat newlines the user meant to type.

- **Escape unwinds one layer at a time, innermost first.** With the snippet modal open over the
  compose box, the first Escape closes the modal and returns the caret to where it was in the box;
  it does not also blur the box. A user should never lose two contexts to one keypress.

- **Focus is always restored to where attention was.** Opening a modal/popover moves focus into
  it; closing it returns focus to the control that opened it (or the compose caret, when the
  trigger was the box). This is a hard requirement, not a nicety — see
  [Popover geometry & focus](#popover-geometry-focus-trap-and-escape).

- **Situational affordances stay; one save control is always present.** The Round-A rule holds —
  Copy appears only when the box has text, edit chips appear only on a linked span — with a single
  walk-back: **`Save as…` is always visible** (bottom-left of the box), because "save what I just
  wrote" is a first-class intent that should never require producing a selection first.

- **Notifications are transient; data events are recoverable.** Every toast auto-dismisses after
  **5 seconds**. A toast that reports a *data* event (a JSON auto-repair) also leaves a durable
  trace the user can return to — a transient surface must never be the only record of something
  that touched their files.

- **The active tab is the scope, and switching it never edits your draft.** The tab sets the
  match pool, the default save target, and the tint. Your composed text is yours across tab
  switches; scope is a decision made *at save time*, not a mode the draft lives inside.

---

## The surface at rest

The Prompts view, top to bottom: the **scope tab row** (Global + pinned projects + a `⋯`
manager), then a two-column body — the **library/match panel** on the left (collapsible), the
**compose box** on the right. The compose box is the only place text is typed. A **settings gear**
opens a popover *above* the box. Situational affordances live on the box: `Copy prompt`
(bottom-right, only with text), `Save as…` (bottom-left, always), the floating save button (next
to a selection), the edit chip (above the box, when the caret is in a linked span), and the
**variable fill list** (below the box, when the text has variables).

---

## Scenario catalog

Legend: **Keys** = what the user presses; **Result** = the exact resulting state; **Today** vs
**Change** mark the diff.

### S1. Type, then copy the whole box

The plain path: write a prompt, copy it.

- **Keys.** Type in the box. `Ctrl/Cmd+C` with no selection, or click `Copy prompt`.
- **Result.** The composed text is resolved through the copy pipeline (escapes resolved; variables
  rendered per each variable's as-variable toggle — see [S4](#s4-fill-variables-per-variable-toggle))
  and placed on the clipboard. A 5-second toast confirms: *"Prompt copied."*
- **Today.** Copy is a button that appears bottom-right whenever the box has text
  (`ComposeBox` → `onCopy`). The toast lives 2.5s (`PromptsView.copyPrompt`).
- **Change.** Toast lifetime → 5s. Add the `Ctrl/Cmd+C` hotkey with the selection-aware rule in
  [S9](#s9-copy--button-and-hotkey).

### S2. Newline search → arrow → Enter-insert

The founder's favorite, half-built today. Each line you type is a live query; you pick a snippet
with the keyboard and it drops in.

- **Keys, in order.**
  1. Type a line, e.g. `senior review`. The match panel fills with ranked snippets for that line.
  2. `↓` (ArrowDown) from the box **steps focus into the panel**, highlighting the first hit.
  3. `↓`/`↑` move the highlight. `Enter` inserts the highlighted snippet. `Esc` returns focus to
     the box and dismisses the highlight, leaving your typed line intact.
  4. Press `Enter` in the box (not in the panel) for a normal newline — which **starts a fresh
     query** on the new line.
- **Result of insert.** The snippet's raw body **replaces the query line you typed** (the text
  from the line start to the caret) and lands as a `linked` span; the caret sits just after it;
  focus returns to the box. Variables in the body merge into the fill list.
- **Today.** The query *is* the current line up to the caret — `caretQuery` in `compose/doc.ts`
  already does `lastIndexOf('\n')`, so **newline-as-search-start already works**; this is the
  behavior that delighted the founder, and it needs no change. Arrow-nav *within* the panel also
  exists (`MatchPanel.handleKeydown`). **But** three things are missing: (a) there is no keyboard
  path from the box into the panel — you must click a hit; (b) `Enter` never inserts; (c) insert
  **appends at the caret** (`composeInsertPiece`), it does not replace the query line. So today the
  founder's "arrow and enter, no mouse" is impossible end-to-end.
- **Change.**
  - `↓` from the box (when the panel has hits and the caret is at the end of a non-empty query
    line) moves focus to the first hit. This is the *only* auto-capture of a key the box would
    otherwise own, and it is safe because `↓` at end-of-text is inert in a textarea.
  - `Enter` on a highlighted hit inserts. `Enter` in the box stays a newline.
  - Insert **replaces the query line** (`caretQuery`'s span: from the current line start to the
    caret) rather than appending. Rationale: the query text was scaffolding to find the snippet;
    leaving it in front of the inserted body is litter the user then has to delete.
  - `Esc` in the panel returns to the box without inserting.
- **Judgment call (JC-2).** Enter-inserts only *after* an explicit `↓` step into the panel; the
  first hit is not pre-armed to swallow Enter while the caret is still in the box. Overrule
  option: auto-arm the first hit so `Enter` inserts without the `↓` — rejected here because it
  steals the newline key mid-compose, but it is a one-line change if the founder wants it.

### S3. Insert by mouse (the equivalence)

- **Keys.** Click a hit in the panel.
- **Result.** Identical to the keyboard insert in [S2](#s2-newline-search--arrow--enter-insert):
  the query line is replaced, a `linked` span is created, the caret lands after it, focus returns
  to the box.
- **Today.** Click inserts at the caret without replacing the query line.
- **Change.** Same query-line replacement as S2, so mouse and keyboard produce one result. One
  insert path, two triggers.

### S4. Fill variables (per-variable toggle)

A snippet body like `Review {ticket:ABC-123} for {task}` exposes a fill list under the box: one
row per distinct variable, first-appearance order. This round makes **as-variable a per-variable
choice**, not one switch over the whole prompt.

- **What each row shows.** The variable name, a fill input (the default is the placeholder text),
  and an **as-var toggle** for *that* variable.
- **Keyboard nav.** `Tab`/`Shift+Tab` move between the box, each fill input, and each row's toggle
  in visual order. `Space` flips the focused toggle. Typing in a fill input updates the value
  live; the value serves every occurrence of that name across the whole document (grammar rule 4).
- **What the toggle does** (semantics from the [copy-output contract](prompts-design.md#copy-output--the-as-variable-toggle)):
  - **ON** for a variable → each occurrence copies as `<prompt_var name="x"/>` and the value lands
    once in the appended `<prompt_vars>` block. This is the dedup form — state a long value once,
    reference it inline, never repeat it and waste the reading model's context.
  - **OFF** → the value substitutes in place; an unfilled OFF variable leaves the canonical `{x}`
    visible (never silently blank).
- **Today.** A single `as variables` checkbox sits in the box (`ComposeBox`, bound to
  `prompts.asVariable`, persisted as `promptsAsVariable` in AppConfig). It flips **all** variables
  together. The fill list (`VariableFillList`) has no per-row toggle.
- **Change.** Remove the global checkbox. Each fill row gains its own toggle. The copy pipeline
  (`copyText`) takes a per-name ON/OFF map instead of one boolean. See
  [Contract implications](#contract-implications) — this kills the `promptsAsVariable` AppConfig
  field and changes the copy API's signature.
- **Judgment call (JC-1): the sensible default.** The founder's rationale is that as-variable
  exists to avoid *repeating long content*; short values should just substitute. So a variable
  defaults **ON when its resolved value is multi-line or longer than ~80 characters, OFF
  otherwise** — the default matches the reason the feature exists, and the user overrides the few
  edge cases by hand. Overrule options: (a) default all OFF (simplest, but re-introduces long-value
  repetition the founder dislikes); (b) default all ON (today's behavior, but noisy `<prompt_var>`
  wrappers around one-word values). This default is per-document session state; it is **not**
  written back to the snippet (see [Open questions](#open-questions-for-the-founder) OQ-3).

### S5. Save the whole box

- **Keys.** Click `Save as…` (bottom-left, always present) with no selection. Or the hotkey (see
  [S9](#s9-copy--button-and-hotkey) map).
- **Result.** The snippet modal opens prefilled with the **entire box text** as the body, scoped
  to the active tab, title empty and focused. `Enter`/`Save` writes it; `Esc`/`Cancel` closes.
- **Today.** There is **no way to save the whole box.** The only save affordance (`Save as piece`)
  floats next to a selection and `saveSelectionAsPiece` early-returns unless `hasSelection`. The
  founder may not realize whole-box save is currently impossible — this is the walk-back on
  Round-A's "no persistent buttons."
- **Change.** Add the always-present `Save as…` control. With a selection it saves the selection
  ([S6](#s6-save-a-selection)); with none it saves the whole box. One control, selection-aware.

### S6. Save a selection

- **Keys.** Select text (mouse drag, or `Shift`+arrows). The floating save button appears next to
  the selection end; click it, or use `Save as…`.
- **Result.** The snippet modal opens prefilled with the **selected text**, scoped to the active
  tab. On save, the selection becomes a `linked` span pointing at the new snippet
  (`composeLinkRange`).
- **Today.** Works as described (`ComposeBox` floating `Save as piece`, `saveSelectionAsPiece`).
  The floating button's only trigger is the mouse; the selection itself is keyboard-reachable but
  the button is not.
- **Change.** `Save as…` (S5) is the keyboard-reachable equivalent of the floating button, closing
  the mouse-off gap. Keep the floating button as the fast mouse path. Both show the **save
  scope** — see [S8](#s8-switch-project-tabs-with-a-draft-in-the-box).

### S7. Edit a linked snippet inline, then save — Update vs Save as new

The compose box is a scratch surface. Editing text inside an inserted span **never** mutates the
stored snippet on its own; the choice happens at save time.

- **Keys.** Place the caret in a linked span and type. The span becomes `linked-modified` (dotted
  underline). To reconcile, open the snippet: click the edit chip above the box, or double-click
  the span; the modal opens.
- **In the modal.**
  - The body shows the **current edited text by default**, with an **`Original`** button (next to
    `Delete`) that previews the stored original read-only — so the user sees what they diverged
    from without losing their edit.
  - Two save actions: **`Update snippet`** (writes the edit back as the canonical body — appends a
    version, never destroys the old one) and **`Save as new`** (creates a fresh snippet from the
    edited text, leaving the original untouched).
  - `Esc`/`Cancel` leaves both the snippet and the span as they are.
- **Today.** The modal opens showing the **stored** body, not the edited span text
  (`PieceModal`: `body = basePiece?.body ?? spanCurrentText`) — the opposite of what the founder
  asked. There is no `Original` button. Saving always updates the same snippet id; there is **no
  `Save as new`** branch. So this whole flow is a genuine build, and the default-body inversion is
  a real contradiction of the founder's stated model, worth confirming.
- **Change.** Default the modal body to the edited span text. Add `Original` (read-only preview of
  the stored body). Add the `Save as new` action beside `Update snippet`. After `Update snippet`,
  the edited span relinks to the (now-updated) snippet as `linked`; after `Save as new`, the span
  relinks to the new snippet.
- **Judgment call (JC-3): `Original` previews, it does not revert.** `Original` is a non-destructive
  peek. Reverting the edit (discarding the user's changes back to the stored body) is a separate,
  clearly-labeled `Revert to original` action, because a one-click "Original" that silently threw
  away the user's typing would be exactly the kind of surprise data-loss this product avoids.

### S8. Switch project tabs with a draft in the box

- **Keys.** Click a tab, or (see [Change]) `←`/`→` while a tab has focus.
- **Result.** The draft text stays exactly as it is. The match pool, tint, and **default save
  scope** switch to the new tab. Nothing is moved or re-scoped; cross-project reference is allowed.
- **Today.** Switching (`setActiveProject`) changes the match pool and tint and re-runs matching;
  the draft is untouched — **keep-content-on-switch already works.** Tabs are plain buttons
  (`Tab`-navigable, no arrow roving).
- **Change.**
  - **No nag.** The founder asked whether switching mid-selection should suggest moving to Global.
    Decision: never proactively. Scope is surfaced *at save*, not pushed at switch time.
  - **Save-time scope switch.** The `Save as…` affordance and the floating button show the target
    scope (the active tab) as a small label with a chevron; clicking it offers **Global** or
    another project in one step — so a snippet typed under one tab can be saved elsewhere without
    switching tabs first. The snippet modal's existing `Save to` selector is the same choice; keep
    them consistent.
  - **Keyboard tabs.** Give the tab row roving-tabindex `←`/`→` navigation with `Enter`/`Space` to
    activate, so scope is switchable mouse-off.

### S9. Copy — button and hotkey

- **Keys.** `Copy prompt` button (bottom-right, with text). `Ctrl/Cmd+C` hotkey.
- **The `Ctrl+C` resolution (this is the load-bearing edge).** `Ctrl+C` is owned by the OS/browser
  for copy-selection, and fighting that is user-hostile. So the binding is **selection-aware**:
  - With a **text selection** in the box → `Ctrl+C` does the native copy-selection. We do not
    touch it. Hijacking selection-copy would break the most reflexive shortcut on the keyboard.
  - With **no selection** → `Ctrl+C` copies the full composed prompt (the `Copy prompt` output)
    and shows the 5s toast. This is dead key-space natively (nothing is selected to copy), so we
    fill it without a fight.
- **Judgment call (JC-4).** This selection-aware split is the recommendation, but `Ctrl+C` for
  full-prompt copy is inherently ambiguous to some users. Overrule option: bind full-prompt copy
  to an unambiguous chord (`Ctrl/Cmd+Shift+C`) and leave `Ctrl+C` fully native. The hotkey map
  below ships both so the founder can pick by feel.

### S10. Open and close the popovers

Three popovers exist: **settings/config** (gear), **project manager** (`⋯`), **snippet modal**.

- **Settings gear.** `Enter`/click the gear opens the popover **above** the compose box. `Esc` or
  click-away closes it; focus returns to the gear.
- **Project manager.** `Enter`/click `⋯` opens it under the tab row. `Esc`/click-away closes;
  focus returns to `⋯`.
- **Snippet modal.** Opens from the edit chip, a double-click on a span, `Save as…`, or the
  floating save button. `Esc`/`Cancel` closes; focus returns to the caret in the box.
- **Today.** The settings surface is the `EmbeddingsPopover` gear, anchored in the **library
  panel head** and opening **downward** (`top: 1.7rem`). The founder wants the settings popover
  **above** the compose box. All three have `Esc` handlers but **no focus trap and no focus
  restore** (a known Round-A follow-up).
- **Change.** Re-anchor the settings popover to open upward from a gear near the compose box (see
  [JC-5](#judgment-calls-collected)). Add focus trap + restore to all three — see
  [Popover geometry & focus](#popover-geometry-focus-trap-and-escape).

### S11. Download & index the semantic model

- **Keys.** Open the settings popover, click `Download & index`.
- **Result.** Two progress bars — Download (runtime + model) and Index (embedding the existing
  library). On completion the popover shows the enable toggle; lexical matching worked the whole
  time.
- **Today.** Works as described (`EmbeddingsPopover`, `startEmbedDownload`); the flow is sound.
  Only its **anchor** changes with S10 — it moves into the relocated settings popover.
- **Change.** None to the flow itself; it rides along with the settings-popover relocation.

### S12. Delete a snippet

- **Keys.** Open the snippet modal → `Delete` → it becomes `Really delete?` → click again to
  confirm. `Esc` at any point aborts.
- **Result.** The snippet file is removed. Spans in the box that pointed at it keep their text
  (they are point-in-time snapshots) but their link dangles; the edit chip falls back to a plain
  state. Deleting a **project** instead re-scopes its snippets to Global — nothing a user wrote
  ever vanishes as a side effect.
- **Today.** Works (`PieceModal.handleDelete` two-step confirm; `deleteProject` re-scopes). The
  two-step is a mouse pattern.
- **Change.** Keyboard parity: the confirm step is reachable by `Enter` on the focused `Delete`
  button (it advances to `Really delete?`, a second `Enter` confirms) — so destructive actions are
  never a mouse-only path, but still take two deliberate presses.

### S13. Toast lifecycle and recovering a repair notice

Two classes of message, and the difference matters:

- **Ephemeral confirmations** (copy succeeded, snippet saved): a 5s toast, then gone. Losing it
  costs nothing — the action already happened and is visible.
- **Data events** (a hand-edited snippet file was JSON-auto-repaired in memory; a snippet file
  couldn't be read at all): these touch the user's files and must be **recoverable after the toast
  vanishes**.
- **The recovery mechanism.** The transient toast announces the event for 5s. A durable trace
  lives on the **settings gear as a small badge** (a count of unresolved data events); opening the
  settings popover shows a **Notices** section listing each — the repaired snippet titles (with
  the "open and re-save to persist the repair" nudge) and any unreadable files. The badge clears
  when the underlying condition clears (the file is re-saved or fixed). This keeps the founder's
  "notifications should be temporary" while honoring "a data event must not disappear forever."
- **Today.** The recovered-pieces notice and the `pieceLoadErrors` notice are **permanent inline
  banners** (`PromptsView`); the recovered banner has no dismiss at all, the load-error banner
  dismisses per-mount. So today's behavior is the opposite extreme — permanent, not transient.
- **Change.** Convert both banners to the toast + settings-badge + Notices model above.
- **Judgment call (JC-5).** Housing the durable trace in the settings popover (rather than a
  standalone inbox) keeps the surface count low and matches the "config lives in a popover" line.
  Overrule option: a dedicated dismissible strip that only re-appears on a new event. Recommended
  against — a second persistent surface is exactly the clutter the popover consolidation avoids.

---

## Hotkey map

Scoped to the Prompts view (they arm on view enter, disarm on leave, and never fire while a modal
owns the keyboard). The global `Ctrl/Cmd+K` "go to search" (owned by `+page.svelte`) is unchanged
and takes precedence.

| Action | Default | Notes |
|---|---|---|
| Copy full prompt | `Ctrl/Cmd+C` (no selection) | Selection-aware — native copy wins when text is selected ([S9](#s9-copy--button-and-hotkey)). |
| Copy full prompt (unambiguous) | `Ctrl/Cmd+Shift+C` | Always full-prompt, regardless of selection. Ships alongside so `Ctrl+C` can stay native if the founder prefers. |
| Save as… | `Ctrl/Cmd+S` | Selection-aware ([S5](#s5-save-the-whole-box)/[S6](#s6-save-a-selection)). Must `preventDefault` — the browser owns `Ctrl+S`. |
| Step into match panel | `↓` at end of query line | Not user-rebindable — it is a spatial nav key, not a command ([S2](#s2-newline-search--arrow--enter-insert)). |
| Insert highlighted snippet | `Enter` (in panel) | Context key, not rebindable. |
| Dismiss panel / close innermost | `Esc` | Universal; not rebindable. |

**Configurability — what it concretely means.** The *command* hotkeys (the top three rows) are
rebindable; the spatial/context keys are not, because rebinding `↓`/`Enter`/`Esc` would break the
inferable conventions the whole design rests on.

- **Storage.** A new `hotkeys` map in AppConfig (`command → chord string`), persisted through the
  existing `get_app_config`/`set_app_config` commands — the same path the copy toggle used, so no
  new command surface. A chord is a normalized string (e.g. `"Mod+Shift+C"`, where `Mod` is
  `Ctrl` on Windows/Linux and `Cmd` on macOS, so one binding is correct cross-platform). Absent
  keys fall back to the defaults above, so an old config and a fresh install both just work.
- **Rebinding UI.** A **Shortcuts** section in the settings popover: each command is a row with
  its current chord shown as a key-cap; clicking `Change` puts the row into capture mode ("press a
  key…"), the next chord is captured, `Esc` abandons capture. A `Reset` per row restores the
  default.
- **Conflict on capture.** If the captured chord already belongs to another Prompts command, the
  capture is **rejected inline** ("`Mod+S` is already Save as…") and nothing is stored — we never
  silently steal a binding from another command. Chords that collide with a *browser/OS* key
  (`Ctrl+C`, `Ctrl+S`) are allowed but shown with a quiet "overrides system default" note, and the
  binding must `preventDefault` to take effect; the selection-aware carve-out for `Ctrl+C` is
  intrinsic to that command and survives rebinding.

---

## Popover geometry, focus trap, and Escape

| Popover | Anchor | Opens | Focus on open | Escape |
|---|---|---|---|---|
| Settings/config (gear) | gear near the compose box | **above** the box | first control (or Notices, if unresolved events) | close → focus gear |
| Project manager (`⋯`) | under the tab row | downward | the "New project" name input | close → focus `⋯` |
| Snippet modal | centered over the view | modal | Title (new) / Body (edit) | close → focus compose caret |

**Focus trap + restore (required — folds in the Round-A a11y debt).** While any of the three is
open: `Tab`/`Shift+Tab` cycle **within** it and never escape to the page behind; the element that
had focus before opening is remembered and re-focused on close. Today none of the three trap or
restore focus (`EmbeddingsPopover`, `ProjectManagerPopover`, `PieceModal` have `Esc` handlers
only). Rationale for making this a build requirement rather than deferred debt: a mouse-off
product where `Tab` silently walks focus behind an open modal is not actually operable without a
mouse — the trap is load-bearing for the north star, not polish.

**Click-away still closes** (the manager and settings popovers already have transparent backdrops)
— click-away and `Esc` are equivalent close gestures, and both restore focus.

---

## Keyboard operability audit — the mouse-only gaps to close

The single reference list of "works by mouse only today," each resolved by a scenario above.
A builder can treat this as the acceptance checklist for the north star.

| Mouse-only today | Keyboard equivalent to build | Where |
|---|---|---|
| Inserting a match (click a hit) | `↓` into panel, `Enter` to insert | [S2](#s2-newline-search--arrow--enter-insert) |
| Saving the whole box | impossible today; add `Save as…` + `Ctrl/Cmd+S` | [S5](#s5-save-the-whole-box) |
| Saving a selection (floating button) | `Save as…` / `Ctrl/Cmd+S` with a selection | [S6](#s6-save-a-selection) |
| Switching tabs | `←`/`→` roving + `Enter` | [S8](#s8-switch-project-tabs-with-a-draft-in-the-box) |
| Copying | `Ctrl/Cmd+C` selection-aware | [S9](#s9-copy--button-and-hotkey) |
| `Tab` walks focus behind open popovers | focus trap + restore | [Popover focus](#popover-geometry-focus-trap-and-escape) |

---

## Contract implications

The exact edits [prompts-design.md](prompts-design.md) needs before any build. The lead amends the
contract from this list; nothing here is built until it lands there.

1. **Per-variable as-variable replaces the global toggle** (§Copy output). The `promptsAsVariable`
   AppConfig field is **removed** (backend `appconfig.rs` + frontend `AppConfig` type + the dev
   fallback in `api.ts`). `copyText` changes signature from `(text, fills, asVariable: boolean)` to
   take a **per-name ON/OFF map** (e.g. `asVars: Record<string, boolean>`). The Rust side, which
   never renders copy output, is unaffected — copy stays frontend-only. Document the smart default
   (JC-1) as the seam both the fill list and the copy builder read.
2. **`piece` → `snippet` rename** touches the command names (`list_pieces`, `save_piece`,
   `delete_piece`, `piece_load_errors`, `MatchHit`/`Piece` types), the schema field references in
   §Piece schema, storage-path prose (`prompts/` file comments say "piece"), and all UI copy. One
   mechanical commit; the contract's own prose renames with it.
3. **Hotkey config storage.** A new `hotkeys: Record<string, string>` on AppConfig with
   default-fallback semantics, persisted via the existing `get_app_config`/`set_app_config`. Add a
   short §Hotkeys to the contract naming the chord-string normalization (`Mod` = Ctrl/Cmd) so both
   platforms store one canonical form.
4. **Snippet-modal save gains a `Save as new` path** (§Compose surface / piece modal). The modal
   today only updates the same id; the contract's modal description must name the two save actions
   and that the edited span text (not the stored body) is the default body when opened from a span.
5. **Auto-repair notice becomes transient + recoverable** (§Store robustness). The contract says
   the recovered flag drives a UI that "shows it needs attention"; tighten that to name the toast +
   settings-badge + Notices model so the transient/durable split is contractual, not incidental.

---

## Judgment calls (collected)

Marked inline above; gathered here so the founder can overrule cheaply. Each is a call I was
authorized to make, not a fork I need him to resolve.

- **JC-1 — per-variable default** = smart (ON for multi-line/long values, OFF for short).
  Overrule: all-OFF or all-ON. [S4](#s4-fill-variables-per-variable-toggle)
- **JC-2 — Enter-inserts only after an explicit `↓`** into the panel; the first hit is not
  pre-armed to swallow Enter. Overrule: auto-arm the first hit. [S2](#s2-newline-search--arrow--enter-insert)
- **JC-3 — `Original` previews, doesn't revert**; reverting is a separate labeled action.
  [S7](#s7-edit-a-linked-snippet-inline-then-save--update-vs-save-as-new)
- **JC-4 — `Ctrl+C` selection-aware** for full-prompt copy, with `Ctrl/Cmd+Shift+C` as the
  unambiguous alternative shipped alongside. Overrule: leave `Ctrl+C` fully native.
  [S9](#s9-copy--button-and-hotkey)
- **JC-5 — durable data-event trace lives on the settings gear** (badge + Notices), not a
  standalone strip. [S13](#s13-toast-lifecycle-and-recovering-a-repair-notice)
- **JC-6 — one consolidated settings popover** (semantic matching + shortcuts + Notices) anchored
  above the compose box, rather than several small popovers. Rationale: fewer surfaces, matches
  "config lives in a popover"; overrule if the founder wants matching settings kept in the library
  panel where they are today. [S10](#s10-open-and-close-the-popovers)

---

## Open questions for the founder

Genuine forks I can't resolve without you — each a concrete either/or with a recommendation.

1. **OQ-1 — When `Update snippet` writes an edit back, should *other* copies of that snippet
   already sitting in the box update too?** Today spans are point-in-time snapshots that never
   re-sync (a deliberate lead ruling: the tint marks origin, not liveness). Keeping that means if
   you inserted a snippet twice, editing-and-updating one leaves the other showing the old text.
   *Recommendation: keep point-in-time (don't re-sync), and show a quiet toast "2 other inserted
   copies left unchanged" so it's never a silent surprise.* Alternative: make `Update snippet`
   re-sync every live span of that id.

2. **OQ-2 — How deep should the rebinding UI go this round?** *Recommendation: ship the minimal
   per-row change/reset with inline conflict rejection (above), and defer any conflict-graph or
   import/export of keymaps.* Alternative: full keymap editor now.

3. **OQ-3 — Should a variable's as-var ON/OFF stick to the snippet, or stay session-only?** If a
   snippet's `{task}` is "always long," persisting ON to the snippet would save re-toggling.
   *Recommendation: session-only this round (keep the snippet schema clean; the smart default
   already gets it right most of the time).* Alternative: add an optional per-placeholder
   `as_var` hint to the snippet schema — a schema change, so it needs your call before it's a
   contract edit.

---

## Things the current code contradicts about the stated model

Surfaced plainly, because the founder was feel-checking from memory and these are high-value:

- **Newline-as-search already works** — his delight was well-placed; `caretQuery` already treats
  each line as the query. No change needed there; the missing piece is only the *keyboard path
  into the results* and Enter-insert.
- **"Arrow and Enter, no mouse" is currently impossible end-to-end.** Arrow-nav exists *inside*
  the panel, but there's no keyboard way to reach the panel from the box, and `Enter` never
  inserts. Half-built, not built.
- **You cannot save the whole box today.** The only save affordance requires a selection first.
  The always-present `Save as…` is genuinely new capability, not just a relocated button.
- **The snippet modal shows the *stored* body by default, not the edited one** — the exact
  opposite of "use the edited version by default." And there is no `Save as new` path at all;
  saving from a span always overwrites the same snippet.
- **The repair/error notices are permanent banners today**, not transient — the reverse of "5-
  second dismiss." The work is to make them transient *and* keep the data event recoverable.
- **`as variables` is one global switch today**, not per-variable — his "fine-grain control"
  ask is a real replacement, and it removes a persisted AppConfig field.
