---
title: Terminal taps and shell panes
date: 2026-11-03
status: In implementation
covers:
  - "Terminal view's keys row shown whenever typing is allowed, with input
    armed by the first thing that sends input rather than by a tap"
  - "The soft keyboard raised only by a tap on the agent's input bar, which
    the daemon finds in each frame and sends with it"
  - "Any other tap: a click when the program asks for mouse reports; on a
    recognised choice question's option, the keys that pick it; otherwise
    nothing"
  - "Terminal view for any herdr pane, plain shells included, outside the
    `[[agents]]` filter, with `[terminal] shells` naming the shells that
    accept typing"
  - "Not covered: shell panes in the voice tools or the general agent;
    finding input bars for agents other than Claude Code, Codex and agy"
sources:
  - "Owner (2026-11-03, verbatim): \"plan to replace the tap on the terminal
    launching keyboard bar/ime, it should only do that if the tap is on
    identified ____ input bar, the keyboard bar should be shown either way
    (probably just make it always on on terminal view), this should also fix
    clicking the agent radio buttons in Claude (and the associated text).
    also, the agent filter should not apply to terminal view (or common
    shells should be allowed)\"; then, offered the outline below, \"yes,
    write it as a draft\""
  - "apps/companion/src/agent/terminal_view.rs (gestures: a tap arms input
    and raises the soft keyboard; in mouse mode a tap is a click);
    apps/companion/src/agent.rs `keys_row_shown` (the keys row shows while
    armed)"
  - "src/server/terminal.rs (opening is gated as reaching an agent is: an
    `[[agents]]` entry for the pane's agent; typing needs `[terminal]
    input`, the agent's `terminal_input`, a principal that may approve, and
    an arm)"
related:
  - "docs/design/20261027_07_TERMINAL_MODE_PLAN.md (decision 7, the
    terminal view; its gestures as amended)"
  - "docs/design/20261022_03_HARNESS_PROTOCOL_PLAN.md (the L3 amendment of
    2026-10-27 21:58 EDT: a tap arms and raises the soft keyboard; the
    10:42 EDT arm with no expiry; the terminal methods)"
  - "docs/design/20261028_06_BOTTOM_BAR_UPDATE_PLAN.md (decision 9: the
    keys row shows while the terminal is armed)"
  - "docs/design/20261029_01_CHOICE_QUESTIONS_PLAN.md (Claude Code's
    question screen, answered by keystrokes)"
---

# Terminal taps and shell panes

## 1. Goal

In terminal view a tap anywhere arms input and raises the soft keyboard.
Raising it shrinks the terminal, which resizes the pane, so the screen
reflows under the finger: a tap meant for one of Claude Code's options
lands on something else, or on nothing. The keys row (Esc, Tab, arrows,
CTRL, ALT) is there only once input is armed. And a pane whose agent has no
`[[agents]]` entry cannot be opened at all, so a plain shell pane is out of
reach.

This plan makes the keyboard rise only for a tap on the agent's input bar,
shows the keys row whenever typing is allowed, makes other taps act on what
they touch, and opens terminal view on any pane.

## 2. Findings (read 2026-11-03)

1. `apps/companion/src/agent/terminal_view.rs`: with input allowed, a tap
   arms input and raises the soft keyboard; a vertical drag arms and
   scrolls. When the daemon serves the mouse and the frame's cursor says
   the program asked for mouse reports, a tap is a left click
   (`terminal.mouse`) and no keyboard.
2. `apps/companion/src/agent.rs`: `keys_row_shown` is `terminal_armed()`;
   the soft keyboard's state is `set_keys_row`. The layout takes the
   keyboard's inset off the bottom and sends `TermAsk::Resize` when the
   fit changes.
3. `src/server/terminal.rs`: `terminal.open` is gated as reaching an agent
   is (the pane's agent has an `[[agents]]` entry and the principal may
   reach it); `input_allowed` adds `[terminal] input`, the agent's
   `terminal_input`, `can_approve`, and the arm. Only a local herdr serves
   terminals.
4. The choice questions plan already reads Claude Code's pending
   `AskUserQuestion` and answers it by keystrokes into its question screen.

## 3. Decisions

1. **The keys row is always shown in terminal view** while the daemon
   allows typing on that terminal (`TerminalInfo::input` and the pane's
   gates of decision 6). With the keyboard down it sits at the screen's
   bottom inset, under the bar, which it lifts; with the keyboard up, just
   above it, as now. Without typing allowed it is not shown. This replaces
   the bottom bar plan's decision 9 "while the terminal is armed".
2. **Arming moves to the first input.** A key in the keys row, typed text,
   a paste, a scroll or a click arms the terminal first if it is not armed
   (`terminal.input_arm`, then the input); a tap no longer arms by itself.
   The arm still has no expiry, and its notice still comes with the first
   accepted batch.
3. **The agent's input bar.** The daemon finds, per frame, the rows of the
   pane's input bar from the pane's agent kind, and sends them with the
   frame as an added optional field, `input_rows: Option<RowSpan>`
   (`first`, `last`, pane rows, inclusive; absent when none is found):
   - **Claude Code:** the rows between the two full-width horizontal rules
     (`─`) nearest the bottom that enclose a line starting `>` or `❯`
     after optional box characters;
   - **Codex:** the rows from the last line starting `›` to the next rule
     or the frame's bottom;
   - **agy:** the rows from its prompt line (`>` after optional box
     characters) to the next rule or the frame's bottom;
   - **anything else** (shells included): the cursor's row, when the cursor
     is visible and in the bottom third of the frame; otherwise none.
   Recognisers live in one daemon module with a test per agent from
   frames written by hand in the tests.
4. **A tap on the input bar** (its rows, any column) arms and raises the
   soft keyboard, as a tap does today. When the bar is absent, a tap on
   the cursor's row does the same.
5. **Any other tap**, in this order:
   1. when the program asks for mouse reports and the daemon serves the
      mouse: a left click at the tapped cell, as now;
   2. when a choice question is open for the pane and the tapped row is one
      of its options' rows (the option's label or its description lines,
      as drawn): the keys that pick that option on that screen, sent as
      input (arming first). The daemon matches the row to the option from
      the frame and the open question; the app sends
      `terminal.choice_tap {terminal_id, row}` and the daemon answers which
      option it picked, or `NOT_PERMITTED` when the row is no option;
   3. otherwise nothing: the keyboard does not rise and nothing is sent.
   A drag still scrolls (vertical) or pans (across), and a vertical drag
   still arms first.
6. **Terminal view opens on any pane.** `terminal.open` no longer needs an
   `[[agents]]` entry for the pane's agent; it needs terminals to be served
   (a local herdr, over the link) and a principal that may approve. Typing then needs, as well as `[terminal] input` and
   the arm:
   - for a pane with an agent that has an `[[agents]]` entry: its
     `terminal_input` (and `terminal_signals` for the signal keys), as now;
   - for a pane with an agent that has no entry: refused, as now
     (`NOT_PERMITTED`, naming the agent);
   - for a pane with no agent: its foreground program is in `[terminal]
     shells`, default `["zsh", "bash", "fish", "sh"]`; signal keys are
     allowed in a shell.
   Without typing allowed the view still opens, read-only, and pans.
7. **The drawer lists shell panes** of the active remote below its agent
   sessions, one row each with the pane's folder name and program
   (`zsh · flankr`), no summary and no activity spinner; tapping one opens
   it in terminal view, which is the only view a shell pane has.
8. **Logs.** The daemon logs each terminal opened on a shell pane and the
   first batch typed into one, as it does for agents, never the keys.
9. **Versions,** in their current series on the owner's instruction "keep
   the same version series": libflankr_proto takes a patch for the added
   field and method; flankr-companion 0.8.x and the app 0.7.x a patch,
   with an `apps/UPDATES.md` entry; the daemon 0.7.x a patch with its
   `UPDATES.md` entry naming `[terminal] shells`.

## 4. Steps

1. **The input bar's rows in the daemon**: decision 3's recognisers and
   the frame field. **Done when:** a unit test per agent kind and a shell
   finds the rows in hand-written frames, and a frame with no prompt has
   none.
2. **Shell panes in the daemon**: decisions 6 and 8, `[terminal] shells`,
   the session list carrying shell panes. **Done when:** tests on the mock
   herdr open a shell pane read-only and typing, refuse typing into a
   program not in `shells` and into an agent with no entry, and never
   log keys.
3. **`terminal.choice_tap`**: decision 5.2. **Done when:** a test with a
   hand-written Claude Code question frame maps each option's label row
   and description row to its keys, and a row outside the options to
   `NOT_PERMITTED`.
4. **The app**: decisions 1, 2, 4, 5 and 7. **Done when:** app tests show
   the keys row with typing allowed and unarmed, the first key arming
   before it is sent, a tap on the input bar raising the keyboard, a tap
   elsewhere raising nothing, a tap on an option sending
   `terminal.choice_tap`, and a shell row in the drawer opening terminal
   view.
5. **On the S23**: tapping one of Claude Code's options in terminal view
   picks it without the keyboard rising; tapping the prompt raises it; a
   zsh pane opens from the drawer and takes typing. **Done when:** recorded
   in `CHANGES.md`.

## 5. Risks

- The input bar is found from each agent's screen layout. If an agent
  changes it, the bar goes unfound and a tap on it falls back to the
  cursor's row (decision 4), never to typing somewhere else.
- A raw shell from the phone can run anything. The gates in decision 6
  (`[terminal] input`, a principal that may approve, the arm, the shells
  list) and the log of decision 8 are what bound it.
- Showing the keys row unarmed means a key in it arms the terminal, which
  sends the arm's notice; that notice now follows the first key instead of
  a tap.

## 6. Open questions for the owner

1. Decision 6's shell list: are zsh, bash, fish and sh the right default,
   or should every pane with no agent accept typing?
2. Decision 5.2: should a tap on an option only move to it (arrows), or
   pick it (its number, or arrows and Enter)? This draft picks it.
3. Decision 7: should shell panes sit in the drawer's session list, or in a
   separate collapsed *Shells* group?

## Amendment — 2026-11-03 12:34 EST: the open questions answered; typing on by default, any pane without an agent

On the owner's instruction, 2026-11-03: "should it just be any tab which
doesn't have a detected agent? the default for terminals should be allow
input"; then, offered the items below, "yes, write the amendment and push".
This answers section 6 and replaces decision 6 and the parts of decisions 7
and 9 and step 2 that name `[terminal] shells`:

1. **Any pane without a detected agent takes typing** in terminal view,
   whatever program runs in it. `[terminal] shells` is not added.
2. **Terminal input is on by default.** `[terminal] input` defaults to
   true, and each `[[agents]]` entry's `terminal_input` defaults to true;
   either set to false turns typing off where it applies. A principal that
   may approve and the arm are still needed; signal keys still need the
   agent's `terminal_signals` (a pane with no agent allows them).
3. **An agent with no `[[agents]]` entry** opens in terminal view and
   takes typing too, under the defaults of item 2; the `[[agents]]`
   filter still governs the voice tools and the general agent, not terminal
   view.
4. **A tap on a choice option picks it** (open question 2, as decision 5.2
   says).
5. **Shell panes sit in the drawer's session list**, below the active
   remote's agent sessions (open question 3, as decision 7 says).
6. **Versions.** The daemon's `UPDATES.md` entry says terminal typing is
   now on by default and how to turn it off (`[terminal] input = false`),
   in place of naming `[terminal] shells`.

## Amendment — 2026-11-03 13:01 EST: approved

The owner approved the plan as it stands at `4e499f2` (2026-11-03:
"approve"). The first implementing commit sets `In implementation`.

## Amendment — 2026-11-03 13:22 EST: implementation started; the document is now immutable

Step 1 has begun: libflankr_term 0.1.2 carries the input bar's rows on
a grid and a frame (`RowSpan`, a second trailer after the cursor's),
and `status` is `In implementation`. From here the document changes
only on the owner's explicit instruction.

## Amendment — 2026-11-03 15:36 EST: the keyboard's action key is Enter; a local blinking underline cursor

On the owner's report of 2026-11-03 15:27 EST in `docs/OWNER_REPORTS.md`
(app 0.7.13, Galaxy S23 Ultra, Samsung keyboard), verbatim: "tested la
with one enter in repowiki shell, did not execute as enter is replaced
with check on kbd. terminal mouse click doesn't appear to be worked by but
option row is hidden so I can't confirm. grouping works. is there input
for the terminal cursor in json or state? it should have a local blinking
underline cursor". This adds two decisions to section 3, beside decisions
1 to 5, and adds to step 4's app half:

1. **The soft keyboard's action key is Enter in the terminal.** While the
   soft keyboard types into the armed terminal, its action key, whatever
   it shows (Done, Go, Send, or Enter), sends `enter` to the pane, and
   the keyboard stays up; a newline in the keyboard's text is `enter`
   too. The keyboard is asked for as terminal apps ask for one: a single
   line, no action of its own (the key shows Enter), and no autocorrect or
   suggestions, so what is typed reaches the pane as typed. The prompts'
   and the nickname field's keyboards are unchanged.
   The app's half (the action as `enter`, the keyboard kept, these
   options) is built here. The keyboard's half is not in the libraries the
   app is pinned to: libmorph's Android runner does not pass
   `TextInputOptions` on, and libmobile's hidden field is single-line
   with Android's default Done action and no editor-action listener, so
   Done hides the keyboard and nothing reaches the app. It needs
   libmobile's text input options plan (`20261024_06`, Draft there) and
   libmorph passing the options and the action through; until the app
   moves onto those releases the check still only closes the keyboard.
2. **The cursor is a local blinking underline.** The terminal draws the
   frame's cursor as an underline at the foot of its cell, in the cell's
   text colour, whatever DECSCUSR shape the program asks for. It blinks on
   the phone: on for 530 ms, off for 530 ms, and on again from the start
   whenever it moves to another cell or a key is sent to the pane. It is
   still not drawn while the program hides it (`visible` false) or the
   frame is stale. The view draws again for the blink only while the
   terminal shows and its cursor is drawn, once each time it turns on or
   off; there are no frames for it otherwise. This replaces plan
   `20261027_08` step 9's "the app draws the cursor (block, underline or
   bar by `shape`...)" (amended there to point here).

The click (the report's second item) needs no change: the terminal's
rect already ends above the keys row and the bar, the pane is resized to
the rows that fit there with the keyboard up and down, and a tap on an
option or a click in mouse mode is sent as decisions 4 and 5 say.

## Amendment — 2026-11-03 15:38 EST: two keys rows with Enter, a toggle that hides flankr's UI, image paste in any terminal

On the owner's instruction (Rule 1.5), 2026-11-03: "what we could do is
expand the keyboard on screen keyboard row to two rows and add a hide show
for the menu item for the for the UI components above it above the
keyboard icon. And the bottom right so with this we would have when those
are within it would hide all of the UI elements that are part of flankr
leaving the terminal at full height. Except for the key on screen keyboard
row. That would also an enter key probably one of also what we're looking
at it we should fix the the plus is not showing"; then "the insert in
terminals it should be the absolute path to the temporary file"; then,
offered the items below, "Yes, write the amendment and build it after the
current fix". This replaces the keys row's one row of nine columns (the
bottom bar plan's decision 9, as decision 1 of this plan keeps it) and adds
to decisions 1 and 6:

1. **Two keys rows**, 40 pt each, of eight equal columns:

   ```
    +    ESC  TAB  ⏎ Enter      ↑         ⛶
    CTRL ALT            ←   ↓   →         ⌨
   ```

   - **⏎ Enter** spans columns 4 and 5 and sends `enter` (with any
     latched modifiers, as the other keys do).
   - **⌨**, the soft keyboard's toggle, is at the bottom right; **⛶**, the
     UI toggle of item 2, is above it.
   - The d-pad keeps its shape: **↑** in column 6 over **↓**, with **←**
     and **→** beside **↓**.
   - Columns 3 and 4 of the second row are left empty.
   The rows show when decision 1 says the one row did; the terminal's rect
   ends above them.
2. **⛶ hides flankr's UI.** A tap hides everything of flankr's in the
   agent view but the keys rows: the header, the bottom bar, pinned cards
   and input cards. The terminal then takes the full height above the keys
   rows (above the soft keyboard when it is up), and the pane is resized to
   that fit. Another tap on ⛶, or Android's Back, shows it again. The
   choice holds until changed, across sessions, and is not saved across
   restarts. While hidden, an approval or a question that arrives draws a
   dot on ⛶ (as a remote card's), and nothing else is shown until the UI
   is.
3. **Image paste in any terminal that takes typing.** The image upload's
   check no longer needs an `[[agents]]` entry for a pane in terminal view:
   a pane that takes typing (decision 6 as amended at 12:34 EST) takes
   images, so the keys row's **+** works in a shell. `terminal.paste`
   inserts each image's absolute path in the daemon's temporary folder,
   space-separated, as it already does; the path has no spaces and is not
   quoted. If the **+** is missing rather than drawn dim, that is fixed
   too.
4. **Versions,** in their series: the app's next patch with its
   `apps/UPDATES.md` entry; the daemon's next patch with its `UPDATES.md`
   entry for item 3.

## Amendment — 2026-11-03 17:35 EST: only Finished cards over the terminal; every other card a bar that opens the list view

On the owner's instruction (Rule 1.5), 2026-11-03: "It looks like input
cards are interfering on the terminal view. I think only finished cards
should be shown on the terminal view. Or input cards like question cards
should return to the list view when tapped"; then, offered the items
below, "Yes, write that amendment and build it". This replaces what the
terminal view draws of the session's cards (a live question's pinned card
drawn in full over the terminal) and adds to the 15:38 EST amendment's item
2:

1. **Finished cards** over the terminal stay as they are: folded to their
   one-line bar.
2. **Every other card** over the terminal (an approval, a question, a plan
   question, Something broke, *While you spoke*, a confirmation, and the
   message and Gemini input cards) is drawn only as a one-line bar: its
   title and the start of its text, cut with "…", and a "›" at its right,
   for example "Claude is asking: Which colour? ›". The bar is the height
   of a Finished card's bar and takes no taps but its own.
3. **A tap on such a bar** switches the session to the list view, scrolled
   to that card, with the card as it is drawn there (answered there, not
   over the terminal). The terminal stays open, as the view toggle leaves
   it today.
4. **The bottom bar's record and Gemini buttons** in terminal view (a tap
   or the 500 ms hold) switch to the list view first and open their input
   card there, so dictation and typing never open over the terminal.
5. **With ⛶ hiding flankr's UI** (the 15:38 EST amendment), no bar shows,
   and a waiting card still draws the dot on ⛶.
6. **Versions:** the app's next patch with its `apps/UPDATES.md` entry.
