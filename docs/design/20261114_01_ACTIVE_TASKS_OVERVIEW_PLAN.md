---
title: Active tasks as the overview, with a glyph for each kind of row, a spinner while an agent is working, inactive agents folded away, and an arrow to the project's agent
date: 2026-11-14
status: In implementation
covers:
  - "A glyph at the left of every row in Active tasks for its kind: a plan
    from the planning plugin, a subagent, an autocontinue task, or an
    agent's own goal (Claude Code's goal and its equivalents), the last two
    drawn as one family"
  - "Working means the agent is still producing: the daemon reads the
    agent's own updating timer from its pane, and falls back to its
    transcript (an open tool call, or a write within a minute); a working
    row's glyph becomes the drawer's Braille spinner"
  - "Inactive agents, subagents and idle plans folded under a collapsible
    `N inactive` section at the end of each project, closed by default"
  - "The drawn libcontrols chevron for every collapsible in the view, in
    place of the ▸ and ▾ characters the font lacks"
  - "An arrow at the right edge of a project's title that opens that
    project's main agent's turns view; a tap elsewhere on the title still
    folds the project"
  - "Not covered: a page of its own for a project (open question 1); any
    change to how plans are matched to panes (the Active plans home view
    plan's 2026-11-13 11:58 EST amendment); the All agents view; the
    drawer"
sources:
  - "Owner (2026-11-14, verbatim): \"good, now the active tasks screen, new
    plan. this is becoming the place the user looks to get an overview of
    what's going on with all of their agents. first, there should should be
    an glyph whether it's a plan (from the planning plugin), a subagent, or
    an auto-complete, there's also the various agent's concept like
    Claude's goal, that works like our auto-complete. for a subagent that's
    actually working, whether on one of our plans or on it's own, it should
    use the Braille spinner like the drawer does now. this should actively
    indicate the agent is working and new turns are being added, not just
    the state given by herdr. inactive agents should be hidden by a
    collapsable section under each project/directory. currently a missing
    glyph is shown, and it's not clear what was intended, but collapsables
    should use the chevrons. an arrow on the right edge of the
    project/directory title should take you to that page instead of
    expanding/collapsing\""
  - "Owner (2026-11-14, verbatim), on the proposal: \"yes, that works, the
    new page. I'm not sure if it's different than the main agent's turns
    view though, maybe it should just go there for now? it can't be last
    ten seconds, some turns take longer while toolcalls are running, if
    there's an updating timer use that.\""
  - "flankr at `d60efb9` (branch `main-terminal`), read 2026-11-14:
    `apps/companion/src/overview/plan_list.rs` (the project title drawn as
    `\\u{25b8}`/`\\u{25be}` and a subagent line as `\\u{21b3}`, around lines
    777 to 805); `apps/companion/src/drawer/spinner.rs` (`FRAMES`, ten
    Braille frames, `FRAME` 80 ms, `glyph_at` from one clock, drawn from the
    terminal face's symbol page); `apps/companion/src/settings/
    remote_details.rs` (`Disclosure`, a drawn chevron, right when closed and
    down when open); `apps/companion/src/drawer.rs` (`DrawerHit::Workspace`
    only folds its group); `src/sessions/claude.rs` (`subagents`: running
    while the meta is not finished and the transcript was written within
    `SUBAGENT_LIVE`, 60 s, in `src/sessions/mod.rs`); `crates/libherdr/src/
    protocol.rs` (`pane.read` with `source` and `lines`);
    `src/sessions/goal.rs` (a session's Claude Code goal lines)"
related:
  - docs/design/20261112_03_ACTIVE_PLANS_HOME_VIEW_PLAN.md (extended; its
    rows, grouping and matching stay as amended)
  - docs/design/20261112_02_PLAN_POLICY_AGENT_PLAN.md (the autocontinue
    task)
  - docs/design/20261028_03_SESSION_DRAWER_PLAN.md (the Braille spinner)
  - docs/design/20261028_04_PLUGINS_PLAN.md (the planning plugin)
---

# Active tasks as the overview

## 1. Why

Active tasks is becoming where the owner looks to see what all of their
agents are doing. Today it can't tell a plan from a subagent from an
autocontinue task at a glance. An agent shows as working whenever herdr
says so, even when it has stopped producing anything. Agents that finished
hours ago fill the list. And the project title draws ▸ and ▾, which the
app's font does not have, so a missing-glyph box shows.

## 2. Decisions

### 2.1 A glyph for each kind of row

Each row starts with one glyph, at the row's text size, in the row's ink.
All of them come from the terminal face's symbol page, which the drawer's
spinner already uses.

| Kind | Glyph | Code point | A row of this kind |
| --- | --- | --- | --- |
| Plan | ☑ | U+2611 | a plan from the planning plugin, at any stage |
| Subagent | ⑂ | U+2442 | one of a pane's subagents |
| Autocontinue | ↻ | U+21BB | the policy plugin's autocontinue on a plan's task, while it is on |
| Goal | ◎ | U+25CE | an agent's own goal: Claude Code's goal, and any agent's equivalent the daemon learns to read |

1. Autocontinue and Goal are one family: both mean "keep going until this
   is done". A plan row with autocontinue on shows ↻ after its title, where
   the Auto switch's state is today. A plan that an agent's goal names
   shows ◎ there, and both show when both hold.
2. An agent's goal that names no plan gets its own row under the agent's
   project, with ◎ and the goal's text cut to one line.
3. The subagent line's current `↳` becomes ⑂.
4. If step 1 finds a glyph missing from the symbol page, the replacement
   is named in a CHANGES.md addendum before anything draws it. It is
   chosen from the page as it stands, with no font change.

### 2.2 Working means still producing

A row is **working** while its agent is still producing turns. herdr's
state is not enough, because herdr says `working` for as long as the
agent's process is busy, whether or not anything is being added.

1. **The agent's own timer.** While herdr says a pane is `working`, the
   daemon reads the pane's last 12 lines with `pane.read` (`source:
   "visible"`) once per snapshot poll, which is every 2 s, and looks for an
   elapsed timer: a run such as `(12s ·`, `(1m 23s ·` or `(1h 2m 3s ·`,
   matched as `\((?:\d+h )?(?:\d+m )?\d+s\b`. The pane is working while the
   timer it shows is later than the one it showed on the previous read.
   This holds through long tool calls, which is why the timer is read
   rather than a fixed window.
2. **No timer.** When no timer is found in the pane, the pane is working
   while its transcript has a tool call with no result yet, or it was
   written within `SUBAGENT_LIVE` (60 s).
3. **Subagents.** A subagent has no timer of its own. It is working while
   its meta does not say `finished` and either its transcript has a tool
   call with no result yet or it was written within 60 s. That is the
   current rule for "running" with the open tool call added.
4. **What is sent.** `AgentInfo` and each subagent entry gain `working:
   bool` and `last_activity`, an RFC 3339 time: the last time the timer
   advanced or the transcript was written. This is a libflankr_proto patch,
   appended fields only. The app runs the spinner itself, so nothing is
   polled per frame.
5. **The spinner.** A working row's kind glyph is replaced by the drawer's
   Braille spinner: `FRAMES` at `FRAME` 80 ms, from the one clock in
   `drawer::spinner::glyph_at`, so every spinning row in the drawer and in
   Active tasks shows the same frame. The app redraws at the spinner's rate
   only while at least one row in view is spinning.
6. herdr's state stays as the row's text, as today.

### 2.3 Inactive agents are folded away

1. Each project lists its rows as today, most recently active first, and
   ends with a collapsible **N inactive** row when any row is inactive.
2. A row is **inactive** when it is not working and `last_activity` is
   more than 10 minutes old. That covers a plan with no working agent, a
   finished subagent, and an agent whose turn ended. An agent's own goal
   row is inactive when its agent is.
3. A row is never inactive while something waits on the owner: a question
   card, a confirmation, or a policy `ask` on its plan.
4. The section is closed by default. It opens and closes with a tap, and
   its state is kept per project for the session, not saved.
5. A row becomes active again as soon as it is working or something waits
   on the owner, and moves out of the section.

### 2.4 Chevrons for every collapsible

Every collapsible in the view draws the libcontrols chevron that Settings'
`Disclosure` uses: pointing right when closed, down when open, at the same
thickness relative to the row's scale. That covers the project title and
the **N inactive** row. No ▸ or ▾ character is drawn anywhere in the view.

### 2.5 The project title's arrow

1. A project's title row is: chevron, project name, then an arrow at the
   right edge, the drawn right-pointing chevron at the same size, in the
   secondary ink.
2. The arrow has its own touch area, the right 48 dp of the title row by
   its full height. A tap there opens the project's **main agent**'s turns
   view, the same view a tap on that agent opens from All agents. A tap
   anywhere else on the title folds and unfolds the project, as today.
3. The main agent is the project's top-level agent (not a subagent) with
   the newest `last_activity`. A project with no agent draws no arrow.
4. A page of its own for a project is left for later (open question 1).

## 3. Steps

### Step 1 — the glyphs and the chevrons

Check that the symbol page has ☑, ⑂, ↻ and ◎, and name any replacement in
CHANGES.md. Draw the kind glyph on every row (2.1), ⑂ for subagents, and
the drawn chevrons for the project title (2.4). App 0.7.30.

**Done when:** every row in Active tasks starts with its kind's glyph; ↻
and ◎ show after a plan's title as 2.1 item 1 says; no missing-glyph box
appears; the project title's chevron points right when the project is
folded and down when it is open; app tests cover each kind.

### Step 2 — working, in the daemon

Read the timer from working panes (2.2 items 1 and 2), the subagent rule
(item 3), and send `working` and `last_activity` (item 4): libflankr_proto
patch, then flankr-companion 0.8.16 passing them through, then daemon
0.7.17.

**Done when:** a test pane whose visible lines show `(1m 2s ·` and then
`(1m 4s ·` is working, and one showing `(1m 4s ·` twice is not; a pane
with no timer and an open tool call is working; a subagent with an open
tool call is working after 60 s without a write; `AgentInfo` carries
`working` and `last_activity`, and old clients ignore them.

### Step 3 — the spinner

Replace a working row's glyph with the shared spinner (2.2 item 5), and
redraw only while something spins. App 0.7.31.

**Done when:** a working row in Active tasks shows the same Braille frame
as a spinning row in the drawer at the same moment; with nothing working
the app does not redraw at the spinner's rate.

### Step 4 — the inactive section

Fold inactive rows under **N inactive** in each project (2.3), with its
chevron. App 0.7.32.

**Done when:** an agent idle for more than 10 minutes moves into its
project's closed **N inactive** section; a row with a question card waiting
never does; a row that starts working again leaves the section at once;
tests cover each.

### Step 5 — the arrow to the project's agent

Draw the title's arrow and its touch area (2.5). App 0.7.33.

**Done when:** a tap on the arrow opens the project's most recently active
top-level agent's turns view; a tap elsewhere on the title folds the
project; a project with no agent has no arrow; tests cover both touch
areas.

Each step is one crate per commit, with its version bump and UPDATES
entries, and is tried on the S23.

## 4. Open questions

1. **A project page.** The owner agreed to a page for each project: every
   plan from the planning plugin with its status and step, then all of the
   project's agents and subagents, active and inactive, unfolded. The
   owner wasn't sure it would differ from the main agent's turns view, so
   for now the arrow opens that view (2.5). Whether to build the page is
   left for a later amendment or plan.
2. **The inactive threshold.** 10 minutes since `last_activity` (2.3 item
   2). The owner has not stated a value.
3. **Other agents' timers and goals.** Codex and others may draw their
   timers differently, and only Claude Code's goal is read today
   (`src/sessions/goal.rs`). The pattern in 2.2 item 1 is Claude Code's.
   Another agent's pattern, or its goal, is added by an amendment when one
   is seen.

## Amendment — 2026-11-14 15:42 EST: approved by the owner; implementation started, and this document is now immutable

The owner, on 2026-11-14, verbatim and in full: "approve both, build and
push to s23". That is this plan's approval. Its open questions get no
answers with it, and stay as section 4 states them: the title's arrow
opens the project's main agent's turns view (2.5), the inactive
threshold is 10 minutes (2.3 item 2), and only Claude Code's timer and
goal are read (2.2 item 1).

Implementation started with step 1 (the glyphs and the chevrons) in the
commit that sets `status: In implementation`. From now on this
document changes only on the owner's explicit instruction; departures go
to `CHANGES.md`. This amendment replaces nothing in the body.
