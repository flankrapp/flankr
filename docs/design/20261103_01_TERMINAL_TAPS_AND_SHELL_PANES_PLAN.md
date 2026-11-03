---
title: Terminal taps and shell panes
date: 2026-11-03
status: Draft
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
