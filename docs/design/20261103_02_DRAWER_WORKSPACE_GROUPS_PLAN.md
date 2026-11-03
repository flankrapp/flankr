---
title: Workspace groups in the drawer
date: 2026-11-03
status: Implemented
covers:
  - "A setting, *Drawer sessions*: **Grouped by workspace** or **Not
    grouped**, with libcontrols' `SelectableLabel`, saved on the phone"
  - "Grouped: a header per herdr workspace in herdr's order, its sessions
    and shell panes under it, collapsible, with a waiting dot when
    collapsed"
  - "Two added session fields from the daemon, `workspace_label` and
    `workspace_number`"
  - "Not covered: grouping in the overview's tiles; moving sessions between
    workspaces from the phone"
sources:
  - "Owner (2026-11-03, verbatim): \"I also want to plan adding workspace
    grouping in sidebar/drawer, maybe as a setting Grouped by workspace Not
    grouped\"; then, offered the outline below, \"Yes, write it as a draft
    plan\""
  - "crates/libflankr_proto/src/common.rs `AgentInfo` (`workspace_id`,
    `project` as the workspace label or the cwd's basename, `tab_id`,
    `tab_label`, `tab_number`); src/core/agents.rs (read from herdr's
    snapshot)"
  - "The owner's herdr, migrated on 2026-11-03 to one workspace per project"
related:
  - "docs/design/20261028_03_SESSION_DRAWER_PLAN.md (the drawer, its
    session rows in `agents.list` order)"
  - "docs/design/20261024_02_SETTINGS_SCREEN_PLAN.md (its 2026-10-30 15:20
    EDT amendment, *At startup*, the first `SelectableLabel` row)"
  - "docs/design/20261103_01_TERMINAL_TAPS_AND_SHELL_PANES_PLAN.md
    (decision 7: shell panes in the drawer)"
---

# Workspace groups in the drawer

## 1. Goal

The drawer lists the active remote's sessions in one flat list, in the
order `agents.list` gives them. With herdr arranged as one workspace per
project, the list has a natural grouping that the drawer does not show.
This plan adds that grouping, with a setting to keep the flat list.

## 2. Findings (read 2026-11-03)

1. `AgentInfo` carries `workspace_id`, `tab_id`, `tab_label` and
   `tab_number`, and `project`, which is the workspace's label or, without
   one, the cwd's basename. It does not carry the workspace's own label
   separately, nor its number, so herdr's workspace order is not known to
   the app.
2. The drawer's rows follow `agents.list` order
   (`apps/companion/src/drawer/content.rs`).
3. *At startup* (the settings plan's 15:20 EDT amendment) is a
   `SelectableLabel` row in the general group, saved in `settings.toml`.

## 3. Decisions

1. **Two added fields** on `AgentInfo`, both optional and skipped when
   absent: `workspace_label` (herdr's label as written, absent when the
   workspace has none) and `workspace_number` (herdr's workspace number,
   its order in herdr's sidebar). The daemon fills them from the snapshot
   it already reads in `src/core/agents.rs`.
2. **The setting.** *Drawer sessions*, in the general group under *At
   startup*, with an ⓘ: libcontrols' `SelectableLabel`, **Grouped by
   workspace** or **Not grouped**. Saved as `drawer_sessions = "grouped" |
   "flat"` in `settings.toml`; default `grouped` (open question 2). ⓘ text:
   "Group the drawer's sessions under their herdr workspace, in herdr's
   order, or list them all together."
3. **Grouped.**
   1. One header row per workspace that has at least one session or shell
      pane, in `workspace_number` order (workspaces without a number last,
      by label). Its text: the workspace label, else the `project` of its
      first session, else `Workspace <n>`; 11 pt, dim, with the count of
      its rows at the right. 28 pt high.
   2. Under it, the workspace's agent sessions in `tab_number` order, then
      its shell panes (the terminal plan's decision 7) in the same order.
   3. A row under a header drops the part of its name the header shows:
      "Claude, tab -mb" for "Claude in libmorph, tab -mb". The spoken name
      is unchanged.
   4. A tap on a header collapses or expands its group (open question 1).
      The collapsed state is kept per remote and workspace id in the
      remote's `state.redb`, across restarts. A collapsed header shows a
      filled dot at its right, as a remote card does, while any session
      in it has a question or an approval waiting.
   5. The session open in the agent view, and a session with a question
      or approval waiting, are never hidden by a collapse made before
      they started waiting: the group expands when one starts waiting.
   6. With only one workspace, its header still shows (open question 3).
4. **Not grouped.** Today's flat list, unchanged, shell panes at its end.
5. **Versions,** in their current series on the owner's instruction "keep
   the same version series": libflankr_proto a patch for the added fields
   (with its `VERSIONS.md` entry); the daemon a patch, with no `UPDATES.md`
   entry (nothing changes for the person running `flankr`);
   flankr-companion 0.8.x a patch; the app 0.7.x a patch with its
   `apps/UPDATES.md` entry.

## 4. Steps

1. **The fields**: decision 1, in libflankr_proto and the daemon.
   **Done when:** a daemon test on the mock herdr's snapshot gives each
   session its workspace's label and number, and a workspace with no label
   gives none.
2. **The setting and the grouped drawer**: decisions 2 to 4.
   **Done when:** app tests show headers in herdr's order with their
   sessions in tab order, shortened row names, a collapse kept across a
   restart, the dot on a collapsed header with a waiting session, a group
   expanding when a session starts waiting, and *Not grouped* giving
   today's list.
3. **On the S23**: the drawer shows the owner's workspaces as groups, and
   the setting switches between the two. **Done when:** recorded in
   `CHANGES.md`.

## 5. Risks

- A daemon older than step 1 sends no `workspace_number`: the groups then
  fall back to label order (decision 3.1), and still group correctly by
  `workspace_id`.
- Many small workspaces make a long drawer of headers; collapsing (decision
  3.4) is the answer, and *Not grouped* remains.

## 6. Open questions for the owner

1. Should groups collapse on a tap, or should headers only label them?
2. Should the default be *Grouped by workspace* or *Not grouped*?
3. With only one workspace, should its header still show?

## Amendment — 2026-11-03 13:05 EST: the open questions answered; no header for a workspace of one

On the owner's instruction, 2026-11-03: "default grouped. collapsible,
probably hide workspace if one member". This answers section 6 and
replaces decision 3.6:

1. **The default is Grouped by workspace** (open question 2), as decision
   2 says.
2. **Groups collapse on a tap** (open question 1), as decision 3.4 says.
3. **A workspace with one row gets no header** (open question 3,
   replacing decision 3.6): its one session or shell pane is listed on its
   own, with its full name ("Claude in libmorph, tab -mb", not shortened
   as decision 3.3 would), in the place the workspace's header would have
   had in herdr's order. A workspace that gains a second row gets its
   header, collapsed or not as it was kept; one that drops to one row
   loses it.

## Amendment — 2026-11-03 13:07 EST: approved

The owner approved the plan as it stands at `a663279` (2026-11-03:
"Approve the plan"). The first implementing commit sets `In
implementation`.

## Amendment — 2026-11-03 14:32 EST: implementation started; the document is now immutable

Step 1 has begun: libflankr_proto 0.5.12 appends `workspace_label` and
`workspace_number` to `AgentInfo`, and `status` is `In implementation`.
From here the document changes only on the owner's explicit instruction.

## Amendment — 2026-11-03 15:26 EST: implemented

Every step is done and its **Done when:** holds; `status` is
`Implemented`.

- **Step 1** (the fields): `0e2884e` (libflankr_proto 0.5.12), `b0ea208`
  (flankr 0.7.6), `af080e9` (flankr-companion test fixtures).
- **Step 2** (the setting and the grouped drawer): `98f18ab`
  (flankr-companion 0.8.4), `05d46f1` (app 0.7.11).
- **Step 3** (on the S23): `5d0e53b`, the CHANGES.md record of the owner's
  "grouping works".
