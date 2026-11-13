---
title: Active plans, a plan-grouped home view
date: 2026-11-12
status: In implementation
covers:
  - "A choice on the home screen, libcontrols' `SelectableLabel`: **All
    agents** (today's overview, the card grid) or **Active plans** (a new
    view), saved on the phone"
  - "The Active plans view: one group per plan being worked on, with its
    title, status and current step; under it the agents on that plan,
    their running subagents and the herdr worktree each is in; agents on
    no plan in a last group"
  - "Events in time order inside each group: an agent's questions and
    choice questions, flankr's confirmations, and the Plan policy's
    decisions, each answerable where it is through the calls the agent
    view already makes"
  - "One added `plugin.call` method and one added notification on the
    Plan policy plugin, `overview`, carrying the plan-to-pane links and
    worktrees; no change to `libflankr_proto`"
  - "How the view reads when the Plan policy plugin is off or the daemon
    is older"
  - "Not covered: approving a plan from this view (the Plan policy screen
    does that); the policy's rules and the continue text (the plan policy
    agent plan); starting an agent or a worktree (the screen drivers plan
    and the policy plan's **Work in a worktree**); grouping by plan in the
    drawer or the agent view; speech for the new view; remote (SSH) herdr"
sources:
  - "Owner (2026-11-12, verbatim): \"one more plan, plan for a combined
    plan/task based overview as an alternate homescreen with
    question/continue events interleaved from active agents, and the new
    autocontinue functionality. it should be clear at a glance whs5 plans
    are being worked on, if possible including suagents and herdr
    worktrees. this should start as an option select label for All
    agents/Active plans. use 5m fable run and reference the autocomplete
    plan when finished as a further plan extending it\""
  - "apps/companion/src/overview.rs (`Overview`: title and status lines
    (`headings`, `divider_y`), `CardGrid`, pinned plugin cards, `Prompts`,
    the confirmation `Panel { lines, confirm }`, `OverviewHit::Agent`);
    overview/card_grid.rs (two cards per row by `(workspace_id,
    tab_number)`, never reordered); overview/agent_card.rs (`NeedsInput`,
    amber approval and blue question outlines, \"· N running\", Yes and No
    drawn inactive, a tap opens the agent view)"
  - "apps/companion/src/settings/startup_row.rs (`SelectRow<T: Choice>`
    over `SelectableLabel`, \"‹ Overview ›\"); settings.rs (`Choice::options`,
    `Startup`, `DrawerSessions`, saved as strings in `settings.toml`)"
  - "apps/companion/src/plugins/mod.rs (`Screen { Overview, Agent,
    Settings }`, `MenuEntry`, `plugin.call` from the app)"
  - "crates/libflankr_proto/src/common.rs (`AgentInfo`: `pane_id`, `name`,
    `agent`, `project`, `cwd`, `workspace_id`, `tab_number`, `status`,
    `focused`, `session { session_id, pending_question }`, `turn`,
    `idle_ms`, `subagents_running`, `summary`, `children: Vec<ChildInfo>`,
    `workspace_label`, `workspace_number`; no worktree or branch field.
    `PendingQuestion { text, ts, about_plan, plan, links, answers }`;
    `Answer { label, style, sends, starts }`; methods `agents.send`,
    `agents.focus`, `choice.answer`, `choice.cancel`, `confirm.reply`,
    `confirm.requested`, `confirm.resolved`, `session.delivery`)"
  - "crates/flankr-companion/src/client.rs (`Event`, `#[non_exhaustive]`:
    a new variant is not a breaking change, per the session drawer plan's
    18:48 EDT amendment, item 6)"
  - "vendor/herdr/src/api/schema/worktrees.rs (`WorktreeInfo { path,
    branch, is_linked_worktree, open_workspace_id, label, .. }`,
    `WorktreeSourceInfo { repo_root, source_checkout_path,
    source_workspace_id, .. }` from `worktree.list`)"
  - "Versions read today: daemon 0.7.11, libflankr_proto 0.5.13,
    flankr-companion 0.8.5, libherdr 0.3.1, app 0.7.20"
related:
  - "docs/design/20261112_02_PLAN_POLICY_AGENT_PLAN.md (Draft): the plan
    this one extends and depends on. It supplies the plugin
    `dev.flankr.plugins.policy`, the approval checkbox kept by the daemon
    with the plan's hash (its 2.3), the `plans`/`state` calls and the
    `decision`/`approval`/`state` notifications (its 2.2), plan matching
    (its 2.3 item 5), the `libherdr` `worktree.*` wrappers and worktree
    detection (its 2.8), and `flankr-companion`'s `policy.rs` (its 2.6).
    Nothing of those is restated here; this plan adds to them"
  - "docs/design/20261103_02_DRAWER_WORKSPACE_GROUPS_PLAN.md (the
    `SelectableLabel` setting *Drawer sessions*, saved in `settings.toml`;
    collapsible group headers with a waiting dot: the pattern the plan
    groups follow)"
  - "docs/design/20261028_03_SESSION_DRAWER_PLAN.md (the drawer, session
    rows, `idle_ms`)"
  - "docs/design/20261023_03_SESSION_SUBSCRIPTIONS_PLAN.md (Finished cards,
    `session.delivery`)"
  - "docs/design/20261023_07_SUBAGENTS_PLAN.md (`children`,
    `subagents_running`, a subagent's question on the card)"
  - "docs/design/20261029_01_CHOICE_QUESTIONS_PLAN.md (`answers`,
    `choice.answer`) and docs/design/20261029_02_SCREEN_DRIVERS_PLAN.md
    (Draft; this plan does not depend on it)"
  - "docs/design/20261023_01_CONFIRMATION_HANDLES_PLAN.md (`confirm.*`,
    the confirmation card's Approve and Deny)"
  - "docs/design/20261028_04_PLUGINS_PLAN.md (`plugin.call`,
    `plugin.notify`, Settings → Plugins)"
---

# Active plans, a plan-grouped home view

## 1. Why

The home screen shows one card per pane, in herdr's order. When three
agents work the steps of two plans, with subagents and a worktree each,
the cards do not say which plan is moving, which step it is on, who is
waiting for the owner, or what the Plan policy last decided. The plan
policy agent plan makes the daemon know those things: which plan a
session is on, its step, its approval, its decisions, and whether a pane
sits in a worktree. This plan puts them on the home screen as a second
view the owner can switch to, grouped by plan, with the questions,
confirmations and decisions of each plan's agents in the order they
happened, answerable where they are.

## 2. Decisions

### 2.1 The choice on the home screen

1. A row under the overview's status lines and above the divider, full
   width, 28 pt high like a drawer group header: libcontrols'
   `SelectableLabel`, built with the settings' `SelectRow<T: Choice>`
   pattern (`apps/companion/src/settings/startup_row.rs`) over a new
   `Choice` type `HomeView { Agents, Plans }`, options in that order,
   showing **‹ All agents ›** or **‹ Active plans ›**. Its arrows step
   the view; a swipe left or right on the row does the same. No ⓘ.
2. **All agents** is today's overview, unchanged: the `CardGrid`, the
   pinned plugin cards, the prompts, the confirmation card and the bottom
   row. **Active plans** replaces only the card grid and the pinned
   plugin cards with the plan list of 2.3; title, status lines, prompts,
   confirmation card, bottom row, ≡ and ⋮ stay as they are and keep
   working.
3. The choice is saved on the phone as `home_view = "agents" | "plans"`
   in `settings.toml`, beside `startup` and `drawer_sessions`, written
   when the row changes; default `agents`. The app opens on the saved
   view. No row is added to the settings screen (open question 1).
4. The ⋮ menu's `Screen::Overview` covers both views; plugin menu items
   do not change.

### 2.2 The data: linking a pane to a plan

The daemon knows the links; the phone draws them. Everything here lives
in the Plan policy plugin and is reached with `plugin.call` and
`plugin.notify`, so `libflankr_proto` does not change.

1. **One added call**, `overview`, params `{}`, open to any paired
   principal (it reads, never acts):

   ```json
   {
     "plans": [
       {
         "path": "docs/design/20261112_02_PLAN_POLICY_AGENT_PLAN.md",
         "title": "Plan policy agent",
         "status": "In implementation",
         "approved": true,
         "changed_since_approval": false,
         "steps_total": 9,
         "current_step": 3,
         "agents": [
           {
             "pane_id": "p12",
             "linked_by": "reply_path",
             "worktree": { "path": "/Users/x/src/flankr/.claude/worktrees/policy", "branch": "plan/20261112_02", "label": "policy" },
             "last_decision": { "decision": "continue", "step": 3, "reason": "…", "rule": null, "confirm_id": 41, "at": "2026-11-12T21:30:00-05:00" }
           }
         ]
       }
     ],
     "unlinked": ["p7", "p9"],
     "paused": false
   }
   ```

   `plans` lists only plans with at least one linked pane, newest path
   first. A plan's row fields are the `plans` row of the policy plan's
   2.2 (`title`, `status`, `approved`, `changed_since_approval`) plus
   `steps_total` (the length of its `steps`) and `current_step`: the
   highest `step` in the plan's decisions, else the step number the
   newest linked Finished card's summary names, else absent. `worktree`
   and `last_decision` are absent when unknown. `unlinked` is every agent
   pane (not shells) with no plan.
2. **How a pane is linked**, in this order, the first that holds, named
   in `linked_by`:
   - `pinned`: the owner pinned the plan to the pane (`approve` with
     `pane_id`, policy plan 2.3 item 5);
   - `reply_path`: a plan path in the pane's newest Finished card's reply;
   - `summary_title`: the newest plan whose title the card's summary
     names;
   - `question`: added by this plan. The pane's `pending_question` has
     `about_plan` and a `plan` of the form `plan NN`, and exactly one
     plan of the newest day in `docs/design/` has that `NN` (open
     question 6);
   - `worktree_branch`: added by this plan. The pane's worktree branch is
     `plan/<YYYYMMDD_NN>` and a plan with that prefix exists (the branch
     the policy plan's `worktree_create` makes).

   The first three are the policy plan's matching, unchanged; the two
   added ones are used by `overview` only and never make the policy act
   (its `no_plan` rule keeps its own three). A link holds while the pane
   exists; a newer card that names another plan moves the pane.
3. **Worktrees.** The plugin learns a pane's worktree first from herdr's
   `worktree.list` (the `libherdr` wrapper of the policy plan's step 6):
   the `WorktreeInfo` whose `path` is the pane's `cwd` or a parent of it
   gives `path`, `branch` and `label`; failing that, from the policy
   plan's `git worktree list --porcelain` (path and branch, no label).
   The main checkout is not a worktree: `worktree` is absent for it.
4. **Subagents** are not repeated by the plugin. The app joins each
   `agents[].pane_id` to its `AgentInfo` from `agents.list`, which
   already carries `children`, `subagents_running`, `turn`, `status`,
   `focused`, `idle_ms` and `session.pending_question`.
5. **One added notification**, `overview`, with the same body as the
   call's result, sent whenever the plugin would send `state`
   (policy plan 2.2), whenever a plan row changes (`approval`), and when
   `agents.list` changes a linked pane's cwd or closes it. At most one
   per 500 ms; the latest wins.
6. **The app** calls `overview` when the Active plans view is shown, when
   the active remote changes, and on `Event::Connected`; it applies each
   `overview` notification as it arrives, and re-draws the join on every
   `agents.list`.

### 2.3 The Active plans view

1. **Plan groups**, one per `plans[]` row, newest path first, a stable
   order that does not change as events arrive (as the card grid's does
   not); a group that needs the owner is marked where it is. A last
   group **Not on a plan** holds `unlinked` panes. Shells appear only
   under **Not on a plan** and only while a confirmation is pending for
   them.
2. **The plan row** (the group header), 2 lines, tap to collapse or
   expand, kept per remote and plan path as the drawer keeps collapsed
   workspaces:
   - line 1: the plan's title, one line, ellipsised; at the right, a
     waiting mark: an amber dot with a count when any of its agents has a
     confirmation pending, a blue dot with a count when any has a
     question, both when both (the drawer's waiting dot, with a count);
   - line 2, dim, 11 pt: `<status> · step <current_step> of <steps_total>
     · <n> agents · <m> subagents · policy: <decision> <age>` where
     `<m>` is the sum of `subagents_running`, parts that are unknown are
     left out, `<decision>` is the newest `last_decision` among its agents
     with its age from `at` (`2 m`, `1 h`), and a plan that is
     `changed_since_approval` or unapproved shows `· not approved` in
     amber instead of the policy part.

   Collapsed, the header alone shows all of this, so the view "at a
   glance" is the headers.
3. **Agent lines**, one per linked pane in `(workspace_id, tab_number)`
   order under the header, 2 lines, the herdr-focused pane's with the
   focus bar at the left edge as on its card:
   - line 1: the `AgentInfo.name` as written; then, when `worktree` is
     present, `⎇ <branch>` or the herdr `label` when there is no branch,
     dim;
   - line 2, dim: the turn word as the card shows it (*Asking*,
     *Stopped*, *Thinking…*, the newest working line one-lined), then
     `· N running` while subagents run, then the policy's last decision
     for this pane as the Finished card's policy line reads (policy plan
     2.7 item 2): *Policy: continue, step 4 — waiting for you*.
   - Under line 2, one line per running subagent from `children`, 11 pt,
     indented: its name and its own turn word.
   - A tap on the agent line (not on an event's buttons) opens the agent
     view on that pane, `OverviewHit::Agent(pane_id)`.
4. **Events** (2.4) follow the agent line they belong to, indented one
   level, so a plan's events read in time order across its agents only
   when the group has one agent; with several, each agent's events are
   in its own time order under it. Open question 2 offers the other
   choice (one merged timeline per plan, each event naming its agent).
5. **Heights.** Header 40 pt; agent line 44 pt plus 16 pt per subagent
   line; event rows as in 2.4. The list scrolls as one, with the card
   grid's `Scroll`. Nothing in it moves except by the owner's taps and
   new rows being added at the end of their agent's events.
6. **Empty states.** No linked plans: one dim line *No plan is being
   worked on* above **Not on a plan**. No agents at all: the overview's
   existing status lines already say so; the list is empty.

### 2.4 Events: which, in what order, acted on in place

1. **Which.** For each pane, the view keeps the events the app already
   receives or can read, nothing new from the daemon:

   | kind | source | row | act in place |
   | --- | --- | --- | --- |
   | question | `session.pending_question` of the pane or a `children[]` entry; cleared when it clears | blue label *Asking* (or *Asking · <subagent name>*), the text to 3 lines | **Yes** / **No**: `agents.send` with `confirmed: true`, as the agent view's buttons |
   | choice question | the same, with `answers` non-empty | the text, then the `answers` as the agent view draws them | an `approve`/`deny`-styled answer: `choice.answer`; a `starts` answer opens the message card with its text, as the agent view does |
   | confirmation | `confirm.requested` for the pane; closed by `confirm.resolved` | amber label *Approval*, the requester (**Plan policy** or the device), the command | **Approve** / **Deny**: `confirm.reply`, as the overview's confirmation card |
   | decision | the policy's `decision` notification for the pane; one row per `summary_serial`, newest replacing the same serial | the decision word, *step N*, the reason in one line; for `continue` with `confirm_id`, its state (*waiting for you*, *sent*, *declined*) from `confirm.resolved` | `continue` waiting: **Approve** / **Deny** on its `confirm_id`; `ask`: **Continue** (`agents.send` the pane the policy's `continue_command`, which goes through the same allow list and confirmation as any send) and **Open**; `stop`: **Open** |

   **Open** is `OverviewHit::Agent(pane_id)`. No other call is made from
   the view; in particular it never calls `approve`, `pause` or `stop` on
   the plugin.
2. **Order.** Within an agent, by time, oldest first, newest last. The
   time is the daemon's where it gives one (`PendingQuestion.ts`,
   `decision.at`) and the phone's receipt time otherwise
   (`confirm.requested`); daemon times and receipt times are compared as
   given, since the daemon and phone clocks are within seconds in
   practice, and ties keep arrival order (open question 7).
3. **How many.** At most 6 event rows per agent: every open one
   (unanswered question, pending confirmation, a `continue` waiting) is
   kept, and the newest closed ones fill the rest; a line *N earlier*
   under them opens the agent view. Closed events older than 30 minutes
   are dropped. Nothing is kept across app restarts: at open, the view
   has the open questions from `agents.list`, the pending confirmations
   from `confirm.status`, and the last decisions from `overview`.
4. **Live buttons.** Unlike the agent card, whose Yes and No are drawn
   inactive, the event rows' buttons are live (the owner asked to act in
   place). They are the same `OutlineButton`s, 36 pt, and the same calls
   the agent view makes; a pressed button is dim until the reply or the
   event's close arrives, then the row changes or goes.
5. **Speech.** Unchanged: the daemon's spoken summaries and the
   confirmation's spoken line are not affected by which view is shown.

### 2.5 When the plugin is off, or the daemon is older

1. `overview` fails with the plugin disabled (`plugin.call` to a disabled
   plugin), is unknown to a daemon without this plan's step 1, or there
   is no policy plugin at all. In every such case the view shows one
   line at the top, *Turn on Plan policy in Settings → Plugins to group
   by plan* (or *This flankr cannot group by plan* for an older daemon,
   using the plugins module's `Availability`), tappable to Settings →
   Plugins, and then the whole list as a single **Not on a plan** group:
   every agent line of 2.3 item 3 with its questions and confirmations
   of 2.4 (there are no decisions). Worktrees are not shown (the daemon
   is what reads them).
2. Enabling the plugin re-calls `overview` on the next `plugin.notify
   state` or within 2 s, whichever comes first; the groups appear
   without leaving the view.
3. Turning the plugin off while the view is shown collapses the groups
   into **Not on a plan** the same way; open events are kept.

### 2.6 Versions

Patch bumps only, in the current series, after the plan policy agent
plan's own bumps (its 2.10). As read today those would be: daemon
0.7.12 → 0.7.13 with an `UPDATES.md` entry (the `overview` call and
notification, the two added link sources, what it logs); flankr-companion
0.8.6 → 0.8.7 (typed `PolicyOverview` in `policy.rs`, additive); app
0.7.21 → 0.7.22 with an `apps/UPDATES.md` entry (the home choice and the
new view). `libflankr_proto` stays 0.5.13 and `libherdr` keeps the policy
plan's version: nothing here touches either. The numbers move with the
policy plan's landing order; the series do not.

Finding for the lead: `flankr_companion::Event` is `#[non_exhaustive]`,
so a new variant there is additive, not breaking (the policy plan's open
question 4 assumed a closed enum). This plan still adds none; the plugin
channel carries everything. `apps/companion`'s `Screen` enum is closed
but internal to the app (no public API), so it is not touched either way.

## 3. Steps

1. **Daemon: `overview` on the Plan policy plugin.** In
   `src/plugins/policy/`: the `overview` call of 2.2 item 1, the two
   added link sources of 2.2 item 2 (`question`, `worktree_branch`),
   worktree lookup through herdr's `worktree.list` with the git fallback
   (2.2 item 3), `current_step`, and the `overview` notification with its
   500 ms coalescing (2.2 item 5). Logs one `info` line per link change:
   `[policy] pane=<id> plan=<path> linked_by=<source> worktree=<path|->`.
   Depends on the policy plan's steps 1, 2, 3 and 6 being in.
   **Done when:** with a fake herdr and a temporary project, a test shows
   a pane whose reply names a plan path under that plan with
   `linked_by: "reply_path"`, a pane in a `plan/<id>` worktree branch
   linked by `worktree_branch` with its `worktree` filled from the fake
   `worktree.list`, a pane with a `plan 06` question linked by `question`
   only when one plan of the newest day has `_06_`, and a shell in
   neither list; `current_step` follows the highest decision step; two
   `state` changes within 500 ms yield one `overview` notification.
   Daemon patch bump and `UPDATES.md` entry in this commit.
2. **flankr-companion: typed overview.** `policy.rs` gains
   `PolicyOverview`, `PolicyPlanGroup`, `PolicyAgentLink`,
   `PolicyWorktree`, `LinkedBy` (an open string, not a closed enum),
   serialised from the JSON of 2.2 item 1 with unknown fields ignored;
   a helper `PolicyOverview::join(&[AgentInfo]) -> Vec<PlanGroup>` that
   attaches each link's `AgentInfo` and builds **Not on a plan** from
   `unlinked` plus any agent pane the daemon did not mention. Patch bump
   and `VERSIONS.md` entry.
   **Done when:** round-trip tests on the JSON of 2.2 item 1, including a
   row with no `worktree` and no `last_decision`; a join test puts a
   pane unknown to the daemon under **Not on a plan**.
3. **App: the home choice.** `HomeView` as a `Choice` in `settings.rs`,
   saved as `home_view`; the `SelectRow` under the status lines (2.1
   item 1), its arrows and swipe; `Overview` holding which view is shown
   and drawing the card grid or an empty placeholder for the plan list;
   the choice restored at start. The app's version is not bumped yet
   (the view is not usable until step 5).
   **Done when:** an `Overview` test shows the row placed under the
   status lines and above the divider, stepping the view both ways,
   `home_view` written and read back, and the card grid untouched in
   **All agents**; the bottom row, prompts and confirmation card hit-test
   the same in both views.
4. **App: the plan list, without events.** `apps/companion/src/overview/
   plan_list.rs`: groups and headers (2.3 items 1 and 2) with collapse,
   agent lines with worktree, turn and subagent lines (2.3 item 3),
   `OverviewHit::Agent` on an agent line, scrolling, the empty states
   (2.3 item 6), and the degraded single group with its top line (2.5).
   The `overview` call and notification wired through the plugins module.
   **Done when:** tests with a fixture of two plans and four panes show
   the header text of 2.3 item 2 exactly (status, step, counts, policy
   word and age, *not approved* in amber), the waiting marks' counts, a
   collapsed group keeping its header, a tap on an agent line yielding
   its pane, and the degraded group when the call fails.
5. **App: events in place.** 2.4 in full: the event store per pane (open
   kept, 6 rows, 30 minutes), the four row kinds, their live buttons
   calling `agents.send`, `choice.answer` and `confirm.reply` through the
   agent view's existing paths, the dim-until-answered state, and *N
   earlier*. App patch bump and `apps/UPDATES.md` entry in this commit.
   **Done when:** tests show a question, a confirmation and a decision
   for one pane ordered by their times; a `continue` decision showing its
   confirmation's state and closing on `confirm.resolved`; a seventh
   closed event dropping the oldest closed one while an open one stays;
   an Approve tap producing exactly one `confirm.reply` with the right
   id; on the S23, with the policy plugin on and one agent on an
   approved plan, the home row reads **‹ Active plans ›** after a restart,
   the plan's header shows its step and the policy's last word, and a
   Yes tapped in the list answers the agent.

## 4. Open questions for the owner

1. **Saving the choice.** Save `home_view` in `settings.toml` so the app
   opens on the last view (recommended), or always open on **All
   agents**? And should a *Home screen* row also appear in Settings?
   Recommend no row: the label on the home screen is the control.
2. **Events per agent or per plan.** 2.3 item 4 nests events under their
   agent, so a plan with two agents has two short timelines. The other
   reading of "interleaved" is one timeline per plan, each row naming
   its agent. Recommend per agent: the buttons then need no agent name,
   and a subagent's question sits under its parent.
3. **Group order.** Stable, newest plan first (recommended, as the card
   grid never reorders), or plans with something waiting first?
4. **Worktree on `AgentInfo`.** Here the worktree rides on the plugin's
   `overview`, so the drawer and the cards cannot show it. An optional
   `worktree` field on `AgentInfo` would be an additive patch on
   `libflankr_proto` (0.5.x) and let every screen show it. Recommend
   later, as its own small plan, once this view proves the lookup.
5. **Live Yes and No.** The card grid draws them inactive and opens the
   agent view; this view makes them live (2.4 item 4), as asked. Keep
   the cards as they are? Recommend yes.
6. **The `question` link** matches `plan 06` to the newest day's `_06_`
   plan. Accept that it can be wrong on a day with no `_06_` and an
   older one with it (it then links nothing), or drop this source?
7. **Times.** Daemon timestamps and phone receipt times are compared as
   given (2.4 item 2). If the two clocks are ever far apart, a
   confirmation could sort before the question it answers; the fix would
   be ages from the daemon (as `idle_ms`) for every event, which needs a
   daemon change. Recommend as given, revisit if seen.
8. **Approval from this view.** The header shows *not approved* but
   ticking stays on the Plan policy screen. Should a tap on *not
   approved* open that screen? Recommend yes; it is one `MenuEntry`
   open, no new call.
9. **Could not confirm in the time.** Whether `confirm.requested` carries
   a daemon timestamp (2.4 item 2 assumes not); where the drawer keeps
   its collapsed-group state (2.3 item 2 copies it, whatever it is);
   whether `children[]` carries a subagent's `pending_question` in the
   shape the agent card reads (2.4 assumes the subagents plan's); whether
   the status-lines area leaves 28 pt for the row on the smallest
   supported screen, or the row should take the second status line's
   place; and whether `plugin.call` to a disabled plugin fails with a
   distinct error the app can tell from an unknown method (2.5 item 1
   wants to word the two cases differently).

## Amendment — 2026-11-12 21:58 EST: the owner's answers: a Settings row, grouping by agent and project, recency, worktrees on every agent, prompt matching, two approvals, timestamps

The owner answered the open questions on 2026-11-12: "only settings row
for now, on home will require rethinking some ux. group events by
agent/project, try to keep events of related projects together such as an
agent working on plans in another repo. sort by mst active/recent. yrs,
add worktree field elsewhere, based on what herdr exposes. try to match
plans with best match using prompt. as far as yes/no/approve appriving the
plan arms it, the autocontinue is a different approval and is like an
active task producing resulys, rev8ewing status changes, and applying
policy. cards should aleays have a timestamp, presumably from daemon or
agent". The plan stays `Draft`. Where this amendment and the text above
disagree, this amendment wins.

1. **The choice is a Settings row only (replaces 2.1 item 1, step 3's
   home-screen row, and answers open question 1).** `HomeView { Agents,
   Plans }` is a row on the Settings screen, **Home view: ‹ All agents ›
   / ‹ Active plans ›**, built with the `SelectRow<T: Choice>` pattern
   and saved as `home_view` in `settings.toml`, default `agents`. The
   overview has no row of its own. A control on the home screen needs a
   rethink of its layout and is left to a later plan.

2. **Grouping by agent and project, related projects together (replaces
   2.3 items 1 and 3, and answers open question 2).** Within a plan's
   group, events are grouped under their agent, and agents under their
   project (the pane's workspace label, or its cwd's repository). An
   agent working in one repository on a plan that lives in another
   appears in that plan's group, under its own project, so work on one
   plan stays together across repositories. A project whose agents serve
   the same plan is placed next to that plan's own project. Events within
   an agent stay oldest first (2.4 item 2).

3. **Most active and most recent first (replaces 2.3 item 1's stable
   order, and answers open question 3).** Plan groups, the projects in
   them and the agents in those are sorted by their newest activity: the
   latest event, delivery or status change among their panes, newest
   first. **Not on a plan** stays last. To keep the list from jumping
   under the owner's finger, a re-sort waits until no touch has been down
   for 2 s.

4. **A worktree on every agent (replaces 2.2 item 3's plugin-only
   worktree, and answers open question 4).** `AgentInfo` in
   libflankr_proto gains an optional `worktree: Option<WorktreeInfo>`,
   with the fields herdr's `worktree.list` exposes (`path`, `branch`,
   `label`, and any others herdr's `WorktreeInfo` has, each optional),
   filled by the daemon for every agent whose cwd lies in a herdr
   worktree, and absent for a main checkout. An optional field appended
   to a struct is additive, a libflankr_proto patch. The drawer and the
   agent cards show it too, as the branch or label in the card's dim
   second line. `overview` takes the worktree from `AgentInfo` instead
   of finding it itself; the `git worktree list` fallback stays for a
   herdr that has no `worktree.list`. A new step, before step 1:
   libflankr_proto's field, then the daemon filling it, each its own
   commit. **Done when:** a pane in a herdr worktree shows its branch in
   the drawer, on its card and in the Active plans view.

5. **Plans matched by the best match against the prompt (replaces 2.2
   item 2's first-that-holds order for this view, and answers open
   question 6).** For each pane, `overview` scores every plan in the
   projects it knows against the session's prompts: the owner's messages
   to the agent in its transcript, newest weighted most, plus the reply
   and summary the policy plan already reads. A plan path, a plan's
   `YYYYMMDD_NN` or a bare "plan NN" for one day, and the plan's title
   score in that order; the best score above a threshold links the pane,
   with `linked_by: "prompt"` and the matched text. A pin (`pinned`)
   still wins over any score. The `question` rule is folded into this
   score as one signal. As before, this match only places a pane in the
   view: the policy agent acts only on its own match and an armed plan
   (item 6).

6. **Two approvals (adds to 2.4 item 4, and answers open questions 5
   and 8).** Approving a plan **arms** it: the plan policy agent plan's
   approval (its 2.3), given from this view by an **Approve** button in
   the plan's header, or on the Plan policy screen. **Autocontinue** is a
   separate approval: switching it on for an armed plan starts an active
   task that produces results (its decisions and their reasons, as event
   rows), reviews the plan's status changes as they happen, and applies
   the policy. The header shows both: *not armed* / *armed*, and
   *autocontinue off* / *autocontinue running* with its last result.
   Stopping autocontinue leaves the plan armed; disarming a plan stops
   its autocontinue. The Yes and No of confirmation and question events
   stay live in this view (2.4 item 4). The plan policy agent plan gets a
   matching amendment.

7. **Every card and event row has a timestamp (replaces 2.4 item 2's
   receipt times, and answers open question 7 and the first part of 9).**
   The time comes from the daemon, or from the agent's transcript where
   the event is the agent's own (a question, a reply). A message that
   carries none today, such as `confirm.requested` if it has no time,
   gains an optional `at` (RFC 3339) set by the daemon: an additive
   field, a patch. Phone receipt time is used only for a daemon too old
   to send one, and the row then shows it dimmed. The agent cards on All
   agents show the time of their newest event as well.

8. **Versions (adds to 2.6).** Item 4 and item 7 add a libflankr_proto
   patch (0.5.14) before the daemon's; the daemon, flankr-companion and
   app bumps stay patches in the current series.

## Amendment — 2026-11-12 22:03 EST: a Gemini best-effort match by title, and Build as the plan's fourth action

The owner, on 2026-11-12: "right, Gemini prompt making best effort match
against active plans in context by title. for now, the autoapprove can
only respond to the coding agent asking which asks things like \"continue
wuth step 2\", the owner still has to approve and request build if a
specific plan (could have build as fourth option on plan)". The plan
stays `Draft`.

1. **Matching is a Gemini prompt (replaces the 21:58 amendment's item 5).**
   For each pane whose session has a new Finished turn, `overview` makes
   one call on `[llm.uses.policy]` with a new prompt, `plan_match`, in
   `ADDITIONAL_PROMPTS.md`: given the session's recent prompts (the
   owner's messages to the agent, newest first), its newest reply and
   summary, and the titles of the active plans in context (`Draft` or
   `In implementation`, in the projects the daemon knows, each with its
   path), it returns the best-matching plan's path, or none, as JSON
   `{ "plan": <path or null>, "matched": <the text it matched on> }`. It
   is best effort: a call that fails or returns a path not in the list
   leaves the pane's previous link, or **Not on a plan**. The result is
   kept per pane until the next Finished turn, so one turn costs at most
   one call. A pin still wins. `linked_by` is `"gemini"`. As before, this
   link only places a pane in the view; the policy acts on its own rules.
2. **The plan's four actions (adds to the 21:58 amendment's item 6).** The
   plan's header offers **Approve** (arm), **Autocontinue** (on or off,
   only when armed), **Open** (the plan's text, in the preview), and
   **Build**: the owner's request that the plan's agent start building
   it. Build is enabled once the plan is armed and a pane is linked to
   it; it sends `approve and build <plan path>` to that pane as the
   owner's own message, through `agents.send` and its usual checks and
   confirmation, never through the policy plugin. Autocontinue then only
   answers that agent's own "continue with step N?" questions (the plan
   policy agent plan's 22:03 amendment).

## Amendment — 2026-11-12 22:12 EST: Build names the plan by its title

The owner, on 2026-11-12, told that the arbitrary-command filter
(`commands::validate_arbitrary_command`) refuses the `/` of a plan path:
"by title is fine". The plan stays `Draft`. This replaces the text Build
sends in the 22:03 amendment's item 2.

Build sends `approve and build the <name> plan`, where `<name>` is the
words of the plan's file name, lower-cased, without its date, sequence
number or `_PLAN` (as the voice agents say a plan:
`20261112_02_PLAN_POLICY_AGENT_PLAN.md` is "the plan policy agent plan").
Those are letters, digits and spaces only, so the message passes the
filter unchanged, whatever punctuation the plan's title has. The agent
finds the plan in `docs/design/` by those words. If the words match more
than one active plan in the pane's project, Build is disabled for that
plan with a note, and the owner sends their own message.

## Amendment — 2026-11-12 22:21 EST: Build names the plan by its date and title

The owner, on 2026-11-12, of the 22:12 amendment's file-name words: "no,
date and title, no /". The plan stays `Draft`. This replaces the 22:12
amendment's text in full.

Build sends `approve and build the <date> plan, <title>`:

- `<date>` is the plan's front-matter `date` without its dashes, eight
  digits (`20261112`);
- `<title>` is its front-matter `title`, with every character the
  arbitrary-command filter (`commands::validate_arbitrary_command`) would
  refuse replaced by a space and runs of spaces folded to one: a `-`, `/`,
  backtick or bracket becomes a space, and a `,` or `:` stays only where a
  space follows it.

For example, `approve and build the 20261112 plan, Active plans, a plan
grouped home view`. The message never contains a `/` or a path, and is
checked by the same filter before it is sent, as any owner message. If
the date and title match more than one active plan in the pane's
project, Build is disabled for that plan with a note, and the owner sends
their own message.

## Amendment — 2026-11-12 22:18 EST: a correction to the previous amendment's time

The amendment headed "2026-11-12 22:21 EST: Build names the plan by its
date and title" was written at 2026-11-12 22:18 EST by the system clock;
its heading's time was typed wrongly. Its content is unchanged.

## Amendment — 2026-11-12 23:23 EST: Active tasks: Build creates the task, and the row's glyph is its autocontinue

The owner, on 2026-11-12 at 22:25 EST: "if this is first focus on it, I
had the order backwards. the checkbox is more like an enaled setting in
the plan list,this coyld be a right side enable/dusable glyph for the
build task, or we just use the upcoming Active tasks screen to manage
them. if ut makes sense to build both in the same worktree do that"; and
at 22:28 EST: "it oribably makes more sense to mak Active tasks the
management UI once Build us chosen on a plan instwad if the plans
screen". The owner's instruction to build this plan is its approval. This
replaces the header's **Approve** and **Autocontinue** actions of the
21:58 amendment's item 6 and the 22:03 amendment's item 2, and the view's
name.

1. **The view is titled *Active tasks*** in the app (the Settings row's
   choice reads **‹ Active tasks ›**; `home_view = "plans"` is kept as
   the saved value).
2. **A plan appears as a task once Build is chosen** on it. Build stays
   on the plans screen (the planning plugin's plan list) and on the
   plan's preview; it calls the policy plugin's `build { path, pane_id }`
   (the plan policy agent plan's 22:31 EST amendment), which arms the
   plan and records the task, then sends the 22:21 amendment's message
   through `agents.send`, with its usual checks and confirmation.
3. **The task's row has a right-side enable/disable glyph** that turns
   its autocontinue on and off (`task_enable`, `task_disable`). Removing
   or finishing the task (`task_remove`) disarms the plan.
4. **Open** stays, opening the plan's text in the preview. The plans
   screen does no management: no arm glyph and no checkbox.
5. Plans with no task but a linked pane still appear, as the 21:58
   amendment groups them, without the glyph; the policy acts only on a
   task's plan with autocontinue on.

## Amendment — 2026-11-12 23:23 EST: implementation started; this document is now immutable

Implementation started with the 21:58 amendment's item 4 (libflankr_proto
0.5.14's `AgentInfo::worktree`, with item 7's `ConfirmRequested::at`) in
the commit that sets `status: In implementation`. From now on this
document changes only on the owner's explicit instruction; departures go
to `CHANGES.md`.
