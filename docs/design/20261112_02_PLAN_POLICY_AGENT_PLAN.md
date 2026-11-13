---
title: Plan policy agent
date: 2026-11-12
status: Draft
covers:
  - "A built-in daemon plugin, `dev.flankr.plugins.policy`, off until its
    config table says `enabled = true`, that reads each Finished card's
    existing Gemini summary and decides `continue`, `stop` or `ask` for
    the coding agent's next plan step"
  - "Approval of a plan by a checkbox in the companion app, kept by the
    daemon with the plan's content hash, so the plugin never acts on a plan
    that is unapproved, changed, on hold, implemented, superseded or
    retired"
  - "The plugin's events to the app over the existing `plugin.notify`
    channel, and its calls from the app over `plugin.call`"
  - "The continue text, `continue`, sent through the same allow list
    (`policy::check`) and confirmation broker as any `agents.send`"
  - "herdr worktrees: `libherdr` wrappers for herdr's `worktree.*` API,
    and the plugin's use of them on an explicit tap only"
  - "Not covered: starting a coding agent in a pane (the screen drivers
    plan); writing anything into a plan file; the plugin approving its own
    confirmations; remote (SSH) herdr; a second model backend"
sources:
  - "Owner (2026-11-12, verbatim): \"let's work on a different problem, a
    pol8cy based agent usibg the existing gemini summary to decide whether
    to continue with next step, this has to be enabled in thr daemon config
    as a plugin, communicates with the fronend via events, should be based
    on only work8ng on approved plans via checkbox and may use herdr
    worktrees. research and write the plan\""
  - "src/sessions/hub/summaries.rs (`CardSummary { points, spoken }`,
    `CardSummarizer`, `SummaryJob`, `CardSummaries`); src/server/speech.rs
    (`DaemonCards`, `SpeechService::card_summary`, `[llm.uses.card_summaries]`)"
  - "src/plugins/mod.rs (`Plugin` trait), src/plugins/ctx.rs (`PluginCtx`:
    `read_file`, `list_files`, `run_command`, `notify`, `config`),
    src/plugins/host.rs (`PluginHost`, `write_enabled`), src/plugins/planning
    (the planning plugin, its reader's front-matter `status`)"
  - "src/server/conn.rs `AGENTS_SEND` (`policy::check` →
    `broker.request(.., Lifetime::Standing)` → `client.run_command`);
    src/core/policy.rs `check`; src/core/confirm.rs `Broker::request`;
    src/core/principal.rs `Principal { id, agents, can_approve }`"
  - "crates/libflankr_proto/src/companion.rs (`PluginCall`, `PluginResult`,
    `PluginNotify`, `PluginInfo`, `PluginMenuItem`, `plugin.set_enabled`);
    common.rs methods (`agents.send`, `confirm.requested`,
    `confirm.resolved`, `session.delivery`)"
  - "vendor/herdr/src/api/schema/worktrees.rs (`worktree.create` with
    `workspace_id`, `cwd`, `branch`, `base`; `worktree.open` with `path`;
    `worktree.list`; `worktree.remove`; events `worktree.created`,
    `worktree.opened`, `worktree.removed`); crates/libherdr/src/client.rs
    (`send_request`, `run_command`, no worktree wrapper yet)"
  - "config.default.toml (`[llm.uses.*]`, `[plugins.\"<ns>\"]` with
    `enabled`, `commands`, `agent_tools`)"
related:
  - "docs/design/20261028_04_PLUGINS_PLAN.md (the plugin host, `plugin.*`
    messages, Settings → Plugins; its step 6, the command path)"
  - "docs/design/20261027_04_GENERAL_AGENT_PLANNING_PLAN.md (`[llm.uses.*]`,
    `[llm.uses.planning]` `read` patterns, the plans reader)"
  - "docs/design/20261023_05_SPOKEN_SUMMARIES_PLAN.md (card summaries: the
    input this plugin reads)"
  - "docs/design/20261023_03_SESSION_SUBSCRIPTIONS_PLAN.md (Finished cards
    and `session.delivery`)"
  - "docs/design/20261023_01_CONFIRMATION_HANDLES_PLAN.md and
    docs/design/20261022_08_PROMPT_CONFIRMATION_PLAN.md (the broker and
    `confirm.*`)"
  - "docs/design/20261029_02_SCREEN_DRIVERS_PLAN.md (Draft: starting and
    driving agents; this plan does not depend on it)"
---

# Plan policy agent

## 1. Why

A coding agent working through a plan stops after each step and waits.
The owner reads the Finished card, usually its Gemini summary, and types
or says "continue". Most of those decisions are mechanical: the step is
done, nothing failed, no question was asked, the plan is still the one
that was approved. This plan adds a daemon plugin that makes that
decision from the summary flankr already produces, tells the phone what
it decided and why, and, when it decides to continue, sends `continue`
through exactly the gates a person's command goes through. It can never
start work on a plan the owner has not ticked.

## 2. Decisions

### 2.1 A built-in plugin, off by default

1. The plugin is `dev.flankr.plugins.policy`, name **Plan policy**,
   registered in `src/plugins/registry.rs` beside the planning plugin and
   implemented under `src/plugins/policy/`. Like every plugin it is listed
   in Settings → Plugins and is off until its table says `enabled = true`
   (the host's `enabled()` check); the phone's switch writes that key
   with the existing `write_enabled`.
2. Its config table and defaults, documented in `config.default.toml`
   under the plugins section:

   ```toml
   # [plugins."dev.flankr.plugins.policy"]
   # enabled = false            # off: nothing is read, decided or sent
   # agents = []                # [[agents]] names it may drive; empty = none
   # continue_command = "continue"   # must be on each agent's allowed_commands
   # max_continues = 3          # per approval, then it asks instead
   # confirm = "always"         # "always": every continue is a confirmation
   #                            # on the phone; "agent": the agent's own
   #                            # require_confirmation decides
   # quiet_secs = 15            # a card is judged only after this long
   #                            # with no newer card in the session
   # agent_tools = false        # none offered to the general agent
   ```

   With `agents = []` the plugin produces decisions for nothing and sends
   nothing, so `enabled = true` alone is still safe. `confirm = "agent"`
   is the only unattended mode and is opt-in.
3. The model it uses is `[llm.uses.policy]`, a new `Use`, defaulting to
   `"flash-lite:oldest"` like the summaries, on until `enabled = false`.
   It is needed only for the judgement in 2.4 item 3; the hard rules need
   no model.

### 2.2 The plugin's interface

The `Plugin` trait as it stands is enough; no trait change.

- `read_patterns()`: `["docs/design/*.md", "AGENTS.md", "CLAUDE.md"]`,
  narrowed as always by `[llm.uses.planning] read`.
- `commands()`: `git` with `worktree list --porcelain` only (2.8).
- `call(method, params, ctx)`, from the phone's `plugin.call`:

  | method | params | result |
  | --- | --- | --- |
  | `plans` | `{}` | the project's plans: `[{ path, title, status, approved, approved_at, changed_since_approval, steps: [{ n, title }], continues_used }]` |
  | `approve` | `{ path, approved: bool }` | the plan's row as above |
  | `state` | `{}` | `{ paused, sessions: [{ pane_id, agent, project, plan, last_decision }] }` |
  | `pause` | `{ paused: bool }` | `{ paused }`; paused, no decision is made or sent |
  | `stop` | `{ pane_id }` | `{}`: cancels the pane's pending continue (`broker` cancel) and clears its continues |
  | `worktree_create` | `{ path }` | `{ worktree_path, branch }` (2.8) |

  `approve`, `pause`, `stop` and `worktree_create` need a caller whose
  `Principal.can_approve` is true; others get `NOT_PERMITTED`.
- `ctx.notify(method, params)`, to every subscribed phone as
  `plugin.notify { ns, method, params }`:

  | method | params |
  | --- | --- |
  | `decision` | `{ pane_id, agent, project, plan, step, decision, reason, rule, summary_serial, confirm_id?, at }` |
  | `approval` | the plan's row from `plans` |
  | `state` | as the `state` result |

- `menu()`: one item, **Plan policy**, opening the plugin's screen (2.7).
- A new hub hook: `CardSummaries::put` already wakes the tailer; the
  hub additionally sends `(pane_id, agent, project, CardKey,
  CardSummary)` on a `broadcast` channel the plugin host hands to plugins
  that implement a new optional trait method `on_card_summary`, default
  no-op. Only this plugin implements it. This is the one addition to
  `src/plugins/mod.rs`, additive and defaulted.

### 2.3 Approval: the checkbox and where it lives

**Recommendation:** the checkbox is in the app, on the Plan policy
screen, one per plan; the daemon keeps the approvals, not the plan file.

1. The store is `~/.local/state/flankr/plugins/policy/approvals.toml`
   (mode 0600), one `[[approval]]` per plan: `project_root`, `path`
   (relative), `sha256` of the plan file's bytes when approved, `by`
   (the principal id), `at` (RFC 3339), `continues_used`.
2. An approval holds only while the file's current `sha256` equals the
   stored one. Any edit, an amendment included, makes
   `changed_since_approval = true`, the box shows unchecked with a note,
   and the plugin treats the plan as unapproved until ticked again.
3. The plan's front-matter `status`, read with the planning reader's
   parser (`split_status` for `(by …)`), must be `Draft` or
   `In implementation`. `Hold`, `Implemented`, `Superseded`, `Retired` or
   missing: the box can be ticked but the plugin's answer is `stop`,
   rule `status`.
4. Why not the plan file: Rule 1.3 lets only `status` change in place
   and Rule 2 wants a discussion before any amendment; a checkbox toggled
   from a phone fits neither. The alternatives are open questions 1 and 2.
5. Which plan a session is working on: the plugin matches a plan path
   named in the Finished card's reply (`docs/design/YYYYMMDD_NN_*.md`), or
   failing that the newest plan the summary names by its title, or failing
   that the plan the owner picked for that pane on the screen
   (`approve` carries an optional `pane_id` to pin it). No match: `stop`,
   rule `no_plan`.

### 2.4 The policy: inputs, rules, judgement

Inputs, all already in the daemon: the Finished card's `CardSummary`
(`points`, `spoken`), the redacted reply the card was made from, the
plan's `## Steps` list parsed as `N.`-numbered or `### Step N` headings
with their **Done when:** lines, the approval row, the pane's
`agent_status` and whether the hub holds a `pending_question` or choice
for it, and the plugin's own counters.

1. Hard rules, in this order; the first that fires is the decision and
   its `rule`:

   | rule | condition | decision |
   | --- | --- | --- |
   | `disabled` / `paused` | plugin off or `pause` set | nothing is emitted |
   | `agent` | agent not in `agents` | `stop` |
   | `no_plan` | no plan matched (2.3 item 5) | `stop` |
   | `unapproved` | no approval, or `changed_since_approval` | `stop` |
   | `status` | plan status not `Draft` / `In implementation` | `stop` |
   | `question` | a pending question or choice on the pane | `ask` |
   | `limit` | `continues_used >= max_continues` | `ask` |
   | `quiet` | a newer card arrived within `quiet_secs` | nothing yet |
   | `complete` | every step's title appears done per the judgement | `stop` |

2. A rule hit never calls the model.
3. Otherwise the judgement: one call on `[llm.uses.policy]` with a prompt
   `plan_policy` added to `ADDITIONAL_PROMPTS.md` beside the summary
   prompts, filled with the summary points, the spoken line, the plan's
   steps, the last step the agent reported, and the rule table's names.
   It must answer JSON `{ "decision": "continue"|"stop"|"ask", "step": N,
   "reason": "<one sentence>" }`. Anything else, a timeout, or a model
   error is `ask`, rule `model`. The prompt says: `stop` on an error,
   failing tests, a formatter or clippy failure the agent did not fix,
   a departure from the plan, or a step the plan says needs the owner;
   `ask` when the agent asked anything or the step is unclear;
   `continue` only when the reported step's **Done when:** reads as met
   and a next step exists.
4. Every decision is logged at `info` as
   `[policy] pane=<id> agent=<name> plan=<path> step=<n> decision=<d>
   rule=<r> serial=<card serial> reason=<text>`, and emitted as the
   `decision` notification.

### 2.5 Continuing: the text and the gate

1. The text is `continue_command` exactly, default `continue`. The plugin
   never composes free text and never uses arbitrary commands.
2. It goes through the same two functions `agents.send` uses in
   `src/server/conn.rs`, with no third path:
   - `policy::check(&principal, &daemon.agents, &agent, &pane_id,
     continue_command)`, where `principal` is
     `Principal { id: "plugin:policy", agents: Some(config.agents),
     can_approve: false }`. The command must be on the agent's
     `allowed_commands`; a denial is logged and emitted as `stop`, rule
     `denied`.
   - `daemon.broker.request(&principal, &pane_id, &agent, &checked.command,
     None, Lifetime::Standing)` when `confirm = "always"` or
     `checked.require_confirmation`; the phone sees the usual
     `confirm.requested` card and only its `confirm.reply` runs
     `client.run_command`. The `decision` notification carries the
     `confirm_id` so the screen can show the card's state.
   - Only with `confirm = "agent"` and `require_confirmation = false` is
     `client.run_command` called at once, as it is for a person.
3. `continues_used` increments when the command is actually run (the
   broker's `confirm.resolved` approve, or the direct run), not when
   asked.
4. `ask` sends nothing; it is the decision notification plus, when the
   phone has voice on, the existing spoken summary line. `stop` sends
   nothing and clears the pane's counters.

### 2.6 Protocol: no new messages

`plugin.call`, `plugin.result`, `plugin.notify`, `plugin.set_enabled`
and `PluginInfo`/`PluginMenuItem` carry everything in 2.2, so
`libflankr_proto` needs no new method or struct. `flankr-companion` gets
typed helpers for the plugin's params (`PolicyDecision`, `PolicyPlan`,
`PolicyState`) in a new `policy.rs`, serialised to and from the JSON in
2.2; additive, a patch bump.

### 2.7 The app's screen

1. **Plan policy**, opened from the plugin's menu item, for the active
   remote's current project:
   - **Plans**: one row per plan from `plans`, newest first: title,
     status, a checkbox **Approved for the policy agent**, and a grey
     note *Changed since approval* or *Status does not allow work* when
     those hold. Ticking calls `approve`; the row updates on `approval`.
   - **Decisions**: the last decision per session: agent name, project,
     plan title, step, the decision word, its reason, and for a
     `continue` the confirmation's state (*Waiting for you*, *Sent*,
     *Declined*).
   - A **Paused** switch (calls `pause`) and a **Stop** button per
     session row (calls `stop`).
2. On a session's Finished card, one line under the summary points:
   *Policy: continue, step 4 — waiting for you*, or the `stop`/`ask`
   word with its reason. Comes from the `decision` notification matched
   on `pane_id` and `summary_serial`.
3. The confirmation card for a continue is the existing one; its
   requester line reads **Plan policy** so the owner knows who asked.

### 2.8 herdr worktrees

herdr offers `worktree.create { workspace_id, cwd, branch, base }`,
`worktree.open { workspace_id, cwd, path, branch }`, `worktree.list` and
`worktree.remove`, with events `worktree.created`, `worktree.opened` and
`worktree.removed`. `libherdr` has only `send_request`; no wrapper.

1. `libherdr` gets `worktree_create`, `worktree_open`, `worktree_list`
   and `worktree_remove` over `send_request`, returning herdr's JSON.
2. The plugin reads: a pane whose `cwd` is a worktree of the project
   (from `git worktree list --porcelain`, its one declared command) is
   judged against the plans of the main checkout, and its decisions carry
   `worktree: <path>`. Approval is per plan, not per checkout.
3. The plugin writes a worktree only on the explicit `worktree_create`
   call from a tap on the screen's **Work in a worktree** action, with
   `branch = "plan/<YYYYMMDD_NN>"` and `base` the current HEAD, through
   the pane's workspace. It never creates, opens or removes a worktree
   on its own decision, and it does not start an agent there: that is the
   screen drivers plan's.

### 2.9 Safety, in one place

- Never on an unapproved plan: rules `no_plan`, `unapproved` and
  `status` fire before any model call or send, and approval is void the
  moment the plan file changes (2.3).
- Never to a pane except through `policy::check` and the broker (2.5);
  the plugin's principal cannot approve, cannot widen `[[agents]]`, and
  reaches only the configured `agents`.
- How the owner stops it: decline the confirmation card; **Stop** on the
  session; **Paused**; Settings → Plugins off (`plugin.set_enabled`);
  `enabled = false` in the config, re-read by the host's watcher; or
  remove the agent from `agents`. Any one is enough.
- What it logs: every decision (2.4 item 4), every denial, every
  confirmation id it opened and how it resolved, and every approval
  change with its principal.

### 2.10 Versions

Patch bumps only, in the current series, as the owner asked:
`libflankr_proto` unchanged (0.5.13); `libherdr` patch (new methods);
daemon 0.7.11 → 0.7.12 with an `UPDATES.md` entry (the new plugin, its
config keys, `[llm.uses.policy]`); `flankr-companion` 0.8.5 → 0.8.6
(new `policy.rs`); app 0.7.20 → 0.7.21 with an `apps/UPDATES.md` entry.
Should a reviewer prefer a first-class `policy.decision` event over
`plugin.notify`, adding a method constant is additive, but a variant in
any closed `Event` enum of `flankr-companion` is breaking under Rule 5.4;
see open question 4.

## 3. Steps

1. **Daemon: the plugin skeleton.** `src/plugins/policy/mod.rs`
   implementing `Plugin` with `ns`, `name`, `description`, `version`,
   `menu`, `read_patterns`, `commands`; `call` answering `state` and
   `pause`; registered in `registry.rs`; the config table and
   `[llm.uses.policy]` documented in `config.default.toml`; `Use::Policy`
   added to `src/llm/mod.rs`.
   **Done when:** Settings → Plugins lists *Plan policy* off; enabling it
   writes `enabled = true`; `plugin.call state` answers; `cargo test -p
   flankr` passes with a registry test that the list is still clean.
2. **Daemon: approvals.** `src/plugins/policy/approvals.rs`: the store in
   2.3, `plans` and `approve` over the planning reader's front matter and
   a steps parser, the `approval` notification, `can_approve` enforced.
   **Done when:** tests on a temporary project show tick, untick, a
   changed file voiding approval, and a non-approving principal refused.
3. **Daemon: the summary hook and hard rules.** `on_card_summary` on the
   trait (default no-op), the hub's broadcast of
   `(pane, agent, project, CardKey, CardSummary)`, plan matching (2.3
   item 5), the rule table in 2.4 item 1, the `decision` notification and
   log line. No model and no sends yet.
   **Done when:** with a mock summariser, a hub test yields `stop
   unapproved` for an unticked plan and `ask question` for a pane with a
   pending question, and nothing is emitted while paused or disabled.
4. **Daemon: the judgement.** The `plan_policy` prompt in
   `ADDITIONAL_PROMPTS.md`, the `[llm.uses.policy]` call, JSON parsing,
   `ask model` on any failure.
   **Done when:** a test with a scripted engine turns a "step 3 done,
   tests pass" summary into `continue step 4` and an "tests failing"
   summary into `stop`, and garbage into `ask model`.
5. **Daemon: continuing through the gate.** 2.5 end to end:
   `policy::check`, the broker request, `continues_used`, `stop`'s
   cancel, the `limit` rule.
   **Done when:** tests show an unapproved plan never reaches the broker;
   an approved one yields exactly one `confirm.requested` per decision;
   nothing runs without `confirm.reply`; a denied command is `stop
   denied`; the fourth continue is `ask limit`. Daemon 0.7.12 and the
   `UPDATES.md` entry go in this commit.
6. **libherdr: worktree wrappers.** The four methods in 2.8 item 1.
   **Done when:** unit tests build the exact `worktree.*` requests and
   parse herdr's `worktree.list` reply from a fixture; patch bump and
   `VERSIONS.md` entry.
7. **Daemon: worktrees.** 2.8 items 2 and 3: the declared `git` command,
   worktree detection in decisions, the `worktree_create` call.
   **Done when:** a decision for a pane in a worktree carries its path,
   and `worktree_create` is refused without `can_approve` and sends the
   expected request with a fake herdr.
8. **flankr-companion: typed params.** `policy.rs` with
   `PolicyDecision`, `PolicyPlan`, `PolicyState`; 0.8.6.
   **Done when:** round-trip tests against the JSON of 2.2.
9. **App: the Plan policy screen and card line.** 2.7 in full; app
   0.7.21 and the `apps/UPDATES.md` entry.
   **Done when:** on the S23, ticking a plan calls `approve` and the row
   updates; a decision appears on the Finished card and in the list; the
   continue's confirmation card names *Plan policy*; Stop cancels it.

## 4. Open questions for the owner

1. **Checkbox in the plan file instead?** An alternative is an amendment
   appended to the plan containing `- [x] Approved for the policy agent`,
   detected by the reader. It lives in git and peers can see it, but
   each tick or untick is an amendment and needs the Rule 2 discussion.
   Recommendation stays with 2.3 (app checkbox, daemon store). Both?
2. **Should approval also be shown in the README's decision table?** Not
   under Rule 4 as written; recommend no.
3. **The continue text.** `continue` alone, or `continue with step N`
   (which would need an allow-list entry per step or arbitrary commands,
   which the plugin never uses)? Recommend `continue` alone.
4. **Events.** Stay on `plugin.notify` (no proto change, 2.6), or add a
   first-class `policy.decision` method? A new method constant is
   additive; a new variant in a closed `flankr-companion` `Event` enum
   would be breaking and against the current patch-only series.
5. **Unattended mode.** Keep `confirm = "agent"` at all? It is the only
   way a continue runs without a tap; it still needs
   `require_confirmation = false` on the agent.
6. **Steps I could not confirm in the time:** whether the planning
   reader already parses `## Steps` into a list (it parses front matter
   and `split_status`; a steps parser may be new in step 2); what the
   `Opens`/`Screens` variants of `MenuItem` allow for a plugin screen
   (step 1 should pick the one the plugins plan defines for the planning
   screen); whether `libherdr` on a remote SSH herdr should be allowed to
   call `worktree.*` at all (recommend local socket only, like terminal
   input).
7. **`quiet_secs` default** of 15 s: long enough that a card followed by
   another is judged once, or should the plugin judge only the newest
   card at the end of a `session.delivery` burst?
