---
title: Active tasks cards, project history and non-overlapping input layout
date: 2026-11-16
status: Draft
covers:
  - "The Active tasks overview as a vertically scrolling sequence of session-log-style cards, with dynamic heights and one non-overlapping hit map for every card, expandable section and owner-input control"
  - "Policy text shown only while a policy decision is actionable, except that the last stop decision remains visible as the terminal outcome"
  - "The latest five eligible plans in each project, excluding Superseded and Consolidated plans and dropping older untouched plans after three days unless they are In implementation or have an active task"
  - "Active projects first, a dotted divider, inactive projects after it, and no project disclosure; the only inactive-task disclosure is for inactive tasks that still have allocated sessions under the project root"
  - "No change to the answer, confirmation, agent-focus or plan-policy actions used by the current Active tasks screen"
sources:
  - "Owner (2026-11-16): plan an improved active tasks screen using the scrolling card layout used in session log screens, preventing any overlap between expandable sections, input requests, etc. (screenshot in 3s, look at the bottom of the screen above the butons). cards that don't have an active policy (other than stop) don't need to show the policy. the last 5 plans for each project should be shown that aren't in superceded or consolidated-by status, unless they are untouched in 3 days and aren't being implemented (no active task references them). the inactive projects should be below a dotted line with the disclosure only used for inactive tasks which still have allocated sessions within the project/dirs"
  - "The current app at apps/companion/src/overview/plan_list.rs: fixed-height Item rows, EVENT_H buttons, project and inactive disclosures, one shared hit lookup, and PlanList's existing Scroll"
  - "The session-log implementation at apps/companion/src/agent.rs: Flow rows/fills/buttons, dynamic card_block heights, card expansion taps, viewport clipping, and the existing Scroll physics"
  - "The current policy data at crates/flankr-companion/src/policy.rs and src/plugins/policy/overview.rs: plan groups, project roots, linked panes, policy decisions, task flags, statuses and plan discovery"
related:
  - docs/design/20261114_01_ACTIVE_TASKS_OVERVIEW_PLAN.md
  - docs/design/20261112_03_ACTIVE_PLANS_HOME_VIEW_PLAN.md
  - docs/design/20261112_02_PLAN_POLICY_AGENT_PLAN.md
  - docs/design/20261022_03_HARNESS_PROTOCOL_PLAN.md
---

# Active tasks cards, project history and non-overlapping input layout

## 1. Why

Active tasks has become the overview, but its rows use fixed heights. A long
policy line, an expandable section, or an answer request can therefore meet
the next row or the bottom controls. The session log already solves the same
class of problem with a vertical flow of cards whose content determines its
height and whose controls are placed after the content. Active tasks should use
that layout model instead of adding more fixed row exceptions.

The overview also needs enough recent plan history to explain a project after
its current task is quiet. That history must not bury active work or turn every
old project into another disclosure tree.

## 2. Decisions

### 2.1 The screen is one scrolling card flow

1. Keep `PlanList` as the owner of the Active tasks state, `ListHit` as the
   action vocabulary, and `controls::scroll::Scroll` as the one scrolling and
   fling implementation. Replace its fixed-height `Item` composition with a
   card flow shaped like the session log's `Flow`: rows, fills, buttons, fold
   taps and a computed `height` in content coordinates.
2. Each project contributes a project heading, then plan cards. A plan card's
   layout cursor advances through its title, status, agent/session summary,
   policy line when eligible under 2.2, events, expandable details and answer
   or confirmation controls. Every block returns the y-coordinate after its
   own content; no block places a control at a fixed offset from a row whose
   text may wrap.
3. A card stores its content-space top and height plus its local rows, fills,
   buttons and expansion taps. The screen assembles the cards at their actual
   tops, translates them by the scroll offset, clips drawing to the viewport,
   and registers only controls whose translated rectangles intersect the
   viewport. A card's fold/expand tap is considered after its buttons and
   answer controls, matching the session log's hit priority.
4. Expanding or folding a card or one of its sections invalidates layout,
   recomputes every following top and the scroll extent, then rebuilds the hit
   map before the next input can be handled. The bottom controls remain the
   overview chrome; the list viewport ends above them and no list hit is
   created outside it.
5. Use the session log card geometry and spacing: rounded card fill and shadow,
   coloured leading edge, title followed by wrapped content, and action
   buttons below the content. Reuse the existing terminal glyph/spinner and
   theme colours. Do not duplicate scroll physics or introduce a second
   screen-level overlay for input requests.

**Done when:** a long plan title, multiline event, expanded details section,
and two adjacent answer/confirmation requests lay out as separate cards with
no intersecting card, button or fold rectangles; changing one expansion moves
every later card and its hit rectangles together; a scripted scroll at the
bottom leaves the last card above the overview buttons.

### 2.2 Policy visibility and input cards

1. Treat a policy decision as actionable while it can still cause an action in
   the current view: an `ask` decision with its Continue/Open controls, or a
   `continue` decision whose confirmation is unresolved. A resolved or stale
   `continue`/`ask` line is omitted from the plan card.
2. Keep the newest `stop` decision as a compact terminal outcome even when it
   has no live control. This is the exception to the omission rule, so a card
   can explain why work stopped without claiming that a stop is waiting for
   input.
3. Render an owner question or confirmation as an explicit card section with
   its wrapped request followed by its buttons. The section owns the complete
   vertical span of its text and controls. It uses the existing `Answer`,
   `Confirm` and `Continue` hits and does not share a rectangle with the plan
   fold tap or the bottom bar.
4. Keep the current answer/confirmation dispatch and pressed-state behaviour;
   this plan changes placement and visibility, not what an action sends.

**Done when:** inactive policy lines disappear from cards, a stopped task still
shows its stop reason, actionable policy and agent questions remain answerable,
and long input text never overlaps another section or the bottom buttons.

### 2.3 Which plans are shown

1. Extend the policy overview's plan metadata with an additive `updated_at`
   value in a machine-comparable form. The daemon derives it from the plan's
   newest amendment, falling back to the plan date/filename date when there is
   no amendment; it sends no plan body. The client treats a missing value as
   oldest, preserving compatibility with older daemons.
2. For each project root, first remove plans whose status base is `Superseded`
   or `Consolidated`. Sort the remaining plans by `updated_at` descending,
   then by path descending as a stable tie-breaker, and retain at most five.
3. A retained plan is hidden only when all of these hold: its update is older
   than three days, its status base is not `In implementation`, and its
   overview row has no active task reference. An active task reference is the
   policy task flag, not merely a historical agent or event. A plan in
   `In implementation` remains visible even with no current session, so its
   project history does not vanish while its agent is between turns.
4. The daemon's overview refresh replaces the cached plan metadata with the
   current project's plan set before applying this selection. Missing files
   must disappear; stale cache entries must not keep old cards alive.
5. Preserve each plan's project root and linked sessions. A linked policy pane
   is an allocated session for its plan. The client also uses the existing
   project-root directory test for sessions whose cwd is below the project
   root when deciding whether an inactive task has allocated sessions.

**Done when:** tests cover separate five-plan limits per project, status-base
exclusion, the three-day boundary, the In implementation exception, the active
task exception, stable ties, missing metadata from an older daemon, and cache
replacement after a plan disappears.

### 2.4 Project order and inactive-task disclosure

1. Classify a project as active when at least one retained plan has an active
   task, an actionable owner input, a working/recent allocated session, or an
   active policy decision. Keep active projects in the current activity order.
2. Draw a non-interactive dotted divider after the last active project when an
   inactive project follows. The divider is visual only and has no hit area.
3. Draw inactive project headings below the divider without a project-level
   disclosure. A project heading remains visible so its recent plan history
   can be found without opening a second container.
4. For an inactive project, show its retained plan cards in compact form. Use
   the one disclosure row only for the subset of inactive task cards that
   still have allocated sessions under the project root; the disclosure opens
   and closes that subset per project for the current app session. A plan with
   no allocated session is not placed behind that disclosure. Active cards are
   never put behind it.
5. Keep **Not on a plan** after project groups. Its sessions continue to use
   the existing agent/event actions, but it does not create a project
   disclosure or move above the dotted inactive-project boundary.

**Done when:** active projects precede inactive projects, exactly one dotted
separator appears when both classes exist, project headings themselves have no
inactive disclosure, and only inactive task cards with allocated sessions are
hidden by/opened through the disclosure.

## 3. Implementation steps

### Step 1 — freeze the card-flow contract and selection fixtures

Add the policy metadata and client-side fixture types needed to express
`updated_at`, retained plans, allocated sessions, active policy and project
classification. Keep the existing wire fields and action variants; new fields
are optional for older daemons.

**Done when:** selection and policy-display tests describe the requested
boundary cases without drawing code changes.

### Step 2 — replace fixed rows with dynamic task cards

Refactor `apps/companion/src/overview/plan_list.rs` around the card-flow layout
from 2.1. Keep the public `PlanList` methods used by `lib.rs`, but make layout,
drawing and hit testing consume computed card heights and clipped local
controls. Add expansion state for card sections and the per-project inactive
task disclosure.

**Done when:** the Active tasks screen's scripted layout tests prove disjoint
rectangles and stable hits after expansion, collapse, scroll and viewport
changes.

### Step 3 — apply plan history, policy and project placement

Update the policy overview producer and `flankr-companion::policy` join to
carry plan freshness and all eligible recent plans. Apply the five-plan and
three-day rules, omit non-actionable policy text except stop, and place
projects around the dotted divider with the restricted disclosure.

**Done when:** daemon and companion tests prove the same plan set and project
classification for active tasks, stale history, closed statuses and allocated
sessions.

### Step 4 — integrate the visual surface

Update app copy only where the new card labels or disclosure wording needs
reader-facing text. Preserve the existing answer, confirmation, continue,
focus, refresh and plan-policy calls. Bump the app and daemon versions and
append their user-facing changelog entries in the commits that move each
component onto the new overview contract.

**Done when:** the new screen builds with no protocol-action changes, and the
README decision row and plan status record the implemented steps.

### Step 5 — visual and interaction evidence

Run the existing scripted app tests with a project containing a long card,
expanded sections, an active question, a confirmation and a stopped policy.
Capture the bottom-of-screen state above the buttons and one scrolled inactive
project state on a device or the scripted renderer. Record the run under
`out/` with a dated evidence log; redact pairing URLs and session contents.

**Done when:** the evidence shows no card/section/input overlap, the bottom
controls remain clear, active projects are above the dotted line, and the
inactive-task disclosure contains only inactive tasks with allocated sessions.
