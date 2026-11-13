//! The plan policy plugin's daemon half, `dev.flankr.plugins.policy`
//! (docs/design/20261112_02_PLAN_POLICY_AGENT_PLAN.md, with its
//! amendments).
//!
//! Off by default, like every plugin. It reads each Finished card's
//! summary for a plan the owner chose Build on (an active task), decides
//! `continue`, `stop` or `ask` for the coding agent's next step, tells
//! the phones over `plugin.notify`, and, only while the task's
//! autocontinue is on and only as the answer to the agent's own
//! "continue with step N?" question, asks to send `continue_command`
//! through `policy::check` and the confirmation broker, never directly.

pub mod judge;
pub mod plans;
pub mod rules;
pub mod store;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use crate::sessions::hub::CardSummaryEvent;
use rules::{Decision, Verdict};
use store::{Store, Task};

use super::command::CommandSpec;
use super::{MenuItem, Opens, Plugin, PluginCtx, PluginError, PluginVersion, Screens};
use libflankr_proto::envelope::codes as wire;

/// The plugin's package name.
pub const NS: &str = "dev.flankr.plugins.policy";
/// The principal its confirmations are asked as (2.5 item 2).
pub const PRINCIPAL_ID: &str = "plugin:policy";

const MENU: [MenuItem; 1] = [MenuItem {
    id: "policy",
    label: "Plan policy",
    opens: Opens::Menu,
    screens: Screens::Both,
    icon: None,
}];

const READ: [&str; 3] = ["docs/design/*.md", "AGENTS.md", "CLAUDE.md"];

/// Its one command (2.8 item 2): which checkouts the project has.
const COMMANDS: [CommandSpec; 1] = [CommandSpec {
    name: "worktree_list",
    binary: "git",
    root_flag: Some("-C"),
    fixed: &["worktree", "list", "--porcelain"],
    args: &[],
    max_output: 64 * 1024,
}];

/// `[plugins."dev.flankr.plugins.policy"]`, past `enabled` (2.1 item 2;
/// `confirm` has only `"always"`, the 22:17 EST amendment, item 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The `[[agents]]` names it may drive, lowercased; empty is none.
    pub agents: Vec<String>,
    pub continue_command: String,
    pub max_continues: u32,
    pub quiet_secs: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            agents: Vec::new(),
            continue_command: "continue".to_string(),
            max_continues: 3,
            quiet_secs: 15,
        }
    }
}

impl Settings {
    /// The table's settings; a bad value is logged and the default kept.
    pub fn of(table: &toml::Table) -> Settings {
        let mut s = Settings::default();
        if let Some(v) = table.get("agents") {
            match v.as_array() {
                Some(list) => {
                    s.agents = list
                        .iter()
                        .filter_map(|a| a.as_str())
                        .map(|a| a.trim().to_lowercase())
                        .filter(|a| !a.is_empty())
                        .collect();
                }
                None => log::warn!("[plugins.\"{NS}\"] agents is not a list; it drives none"),
            }
        }
        if let Some(v) = table.get("continue_command") {
            match v.as_str().map(str::trim).filter(|c| !c.is_empty()) {
                Some(c) => s.continue_command = c.to_string(),
                None => log::warn!(
                    "[plugins.\"{NS}\"] continue_command is not a word; \"continue\" is used"
                ),
            }
        }
        if let Some(v) = table.get("max_continues") {
            match v.as_integer().filter(|n| (0..=1000).contains(n)) {
                Some(n) => s.max_continues = n as u32,
                None => log::warn!("[plugins.\"{NS}\"] max_continues is not 0 to 1000; 3 is used"),
            }
        }
        if let Some(v) = table.get("quiet_secs") {
            match v.as_integer().filter(|n| (0..=3600).contains(n)) {
                Some(n) => s.quiet_secs = n as u64,
                None => log::warn!("[plugins.\"{NS}\"] quiet_secs is not 0 to 3600; 15 is used"),
            }
        }
        match table.get("confirm").map(|v| v.as_str()) {
            None | Some(Some("always")) => {}
            Some(other) => log::warn!(
                "[plugins.\"{NS}\"] confirm = {other:?}: only \"always\" is built; every \
                 continue is confirmed on the phone"
            ),
        }
        s
    }

    /// Whether it may drive `agent` (lowercased).
    pub fn drives(&self, agent: &str) -> bool {
        self.agents.iter().any(|a| a.eq_ignore_ascii_case(agent))
    }
}

/// What the plugin reaches in the daemon, beyond its [`PluginCtx`]: the
/// daemon gives the real ones, tests fakes.
#[async_trait::async_trait]
pub trait Services: Send + Sync {
    /// The daemon's state folder, `~/.local/state/flankr`; `None` keeps
    /// no tasks.
    fn state_dir(&self) -> Option<PathBuf>;
    /// A `plugin.notify` of this plugin to every phone.
    fn notify_all(&self, method: &str, params: Value);
    /// The question the agent in `pane_id` ended its turn with, in its
    /// reply's words, if any.
    fn pending_question(&self, _pane_id: &str) -> Option<String> {
        None
    }
    /// The choice question open on `pane_id`, its first question's words,
    /// if one is.
    async fn open_choice(&self, _pane_id: &str) -> Option<String> {
        None
    }
    /// The `[[agents]]` entries, as the daemon checks sends against them.
    fn agents(&self) -> Vec<crate::config::AgentConfig> {
        Vec::new()
    }
    /// Opens a confirmation for `command` on `pane_id`, already checked by
    /// `core::policy::check`, as `principal`, standing: its id, and its
    /// outcome once it is resolved (`sent`, `failed`, `denied`, `expired`
    /// or `cancelled`). Only its approval sends the command.
    fn request_confirmation(
        &self,
        _principal: &crate::core::principal::Principal,
        _pane_id: &str,
        _agent: &str,
        _command: &str,
    ) -> Result<(u64, tokio::sync::oneshot::Receiver<&'static str>), String> {
        Err("no confirmations here".to_string())
    }
    /// Withdraws `principal`'s pending confirmation `confirm_id`.
    fn cancel_confirmation(
        &self,
        _principal: &crate::core::principal::Principal,
        _confirm_id: u64,
    ) {
    }
    /// One answer of `[llm.uses.policy]` to `system` and `user`, or why
    /// there is none (no model, a timeout, an error).
    async fn judge(&self, _system: &str, _user: &str) -> Result<String, String> {
        Err("no model".to_string())
    }
}

/// One pane the policy has seen a card from: reusable by a later
/// `overview` (plan 20261112_03).
#[derive(Debug, Clone, Default)]
struct Session {
    agent: String,
    project: String,
    plan: Option<String>,
    last_decision: Option<Value>,
    /// The choice question that blocks the pane's task, while it is open
    /// (the owner's ruling of 2026-11-12).
    blocked: Option<String>,
    /// The newest card's serial.
    latest: u64,
    /// The continue waiting for the owner's tap: its confirmation id.
    pending: Option<u64>,
}

/// What the plugin keeps while the daemon runs.
#[derive(Debug, Default)]
struct State {
    paused: bool,
    /// By pane id.
    sessions: BTreeMap<String, Session>,
}

/// The plan policy plugin.
#[derive(Default)]
pub struct PlanPolicy {
    state: Arc<Mutex<State>>,
    services: Option<Arc<dyn Services>>,
}

impl PlanPolicy {
    /// Without the daemon: it answers `state` and `pause`, and keeps no
    /// tasks.
    pub fn new() -> Self {
        Self::default()
    }

    /// With the daemon's services.
    pub fn with_services(services: Arc<dyn Services>) -> Self {
        PlanPolicy {
            services: Some(services),
            ..Self::default()
        }
    }

    fn store(&self) -> Result<Store, PluginError> {
        self.services
            .as_ref()
            .and_then(|s| s.state_dir())
            .map(|d| Store::under(&d))
            .ok_or_else(|| PluginError::new(NO_STORE, "The plan policy keeps no tasks here."))
    }

    fn notify(&self, method: &str, params: Value) {
        if let Some(s) = &self.services {
            s.notify_all(method, params);
        }
    }

    /// Whether it is paused.
    pub fn paused(&self) -> bool {
        self.state.lock().unwrap().paused
    }

    /// `state {}`'s answer, also the `state` notification.
    fn state_json(&self) -> Value {
        let state = self.state.lock().unwrap();
        let sessions: Vec<Value> = state
            .sessions
            .iter()
            .map(|(pane_id, s)| {
                json!({
                    "pane_id": pane_id,
                    "agent": s.agent,
                    "project": s.project,
                    "plan": s.plan,
                    "last_decision": s.last_decision,
                    "blocked": s.blocked.is_some(),
                    "question": s.blocked,
                })
            })
            .collect();
        json!({ "paused": state.paused, "sessions": sessions })
    }

    /// A plan's row with whether its task is blocked by a choice question.
    fn row(&self, plan: &plans::PlanDoc, task: Option<&Task>) -> Value {
        let mut row = plan.row(task);
        let blocked = task.and_then(|t| t.pane_id.as_deref()).and_then(|p| {
            let state = self.state.lock().unwrap();
            state.sessions.get(p).and_then(|s| s.blocked.clone())
        });
        row["blocked"] = json!(blocked.is_some());
        row["question"] = json!(blocked);
        row
    }

    /// Notes a choice question blocking `pane_id`'s task, or none; tells
    /// every phone when that changes.
    fn set_blocked(&self, pane_id: &str, question: Option<String>) {
        let changed = {
            let mut state = self.state.lock().unwrap();
            let session = state.sessions.entry(pane_id.to_string()).or_default();
            let changed = session.blocked != question;
            session.blocked = question;
            changed
        };
        if changed {
            self.notify("state", self.state_json());
        }
    }

    /// Sends `decision` to every phone and logs it.
    fn emit(&self, decision: &Decision) {
        log::info!("{}", decision.log_line());
        let value = decision.json();
        {
            let mut state = self.state.lock().unwrap();
            let session = state.sessions.entry(decision.pane_id.clone()).or_default();
            session.plan = decision.plan.clone();
            session.last_decision = Some(value.clone());
        }
        self.notify("decision", value);
    }

    /// Changes `plan`'s task with `change`, and tells every phone.
    fn change_task(&self, root: &str, plan: &plans::PlanDoc, change: impl FnOnce(Task) -> Task) {
        let Ok(store) = self.store() else {
            return;
        };
        match store.update(root, &plan.path, |old| old.map(change)) {
            Ok(task) => {
                let row = self.row(plan, task.as_ref());
                self.notify("approval", row);
            }
            Err(e) => log::warn!("[policy] {}: {e}", plan.path),
        }
    }

    /// The decision for one Finished card, past the quiet time; `None`
    /// when nothing is to be said.
    async fn decide(
        &self,
        event: &CardSummaryEvent,
        ctx: &PluginCtx,
        settings: &Settings,
    ) -> Option<Decision> {
        let root = match root_of(ctx) {
            Ok(root) => root,
            Err(e) => {
                log::info!(
                    "[policy] pane={}: no project: {}",
                    event.pane.pane_id,
                    e.message
                );
                return None;
            }
        };
        let tasks: Vec<Task> = self
            .store()
            .map(|s| s.load())
            .unwrap_or_default()
            .into_iter()
            .filter(|t| t.project_root == root)
            .collect();
        let plans = plans::list(ctx).await.unwrap_or_default();
        let Some((plan, _linked)) = rules::match_plan(event, &tasks, &plans) else {
            return Some(Decision::new(
                event,
                Verdict::Stop,
                "no_plan",
                "No plan matched this card.",
            ));
        };
        let plan = plan.clone();
        let decided = |verdict, rule, reason: String| {
            Some(Decision::new(event, verdict, rule, reason).on(&plan))
        };
        let Some(task) = tasks
            .iter()
            .find(|t| t.path == plan.path && t.armed)
            .cloned()
        else {
            return decided(
                Verdict::Stop,
                "unapproved",
                "Build has not been chosen on this plan.".to_string(),
            );
        };
        if !plan.status_allows() {
            if task.autocontinue {
                self.change_task(&root, &plan, |t| Task {
                    autocontinue: false,
                    ..t
                });
            }
            return decided(
                Verdict::Stop,
                "status",
                format!(
                    "The plan's status, {}, does not allow work.",
                    if plan.status.is_empty() {
                        "missing"
                    } else {
                        &plan.status
                    }
                ),
            );
        }
        if !task.autocontinue {
            return decided(
                Verdict::Ask,
                "autocontinue_off",
                "Autocontinue is off for this task.".to_string(),
            );
        }
        if task.hash != plan.hash {
            // The 21:58 EST amendment: a change pauses the task for review.
            let reason = rules::describe_change(&task, &plan);
            self.change_task(&root, &plan, |t| Task {
                autocontinue: false,
                ..t
            });
            return decided(
                Verdict::Ask,
                "changed",
                format!("{reason} Autocontinue is paused until you switch it on again."),
            );
        }
        // The owner's ruling: a choice question blocks the task.
        match self
            .services
            .as_ref()?
            .open_choice(&event.pane.pane_id)
            .await
        {
            Some(question) => {
                self.set_blocked(&event.pane.pane_id, Some(question.clone()));
                return decided(
                    Verdict::Ask,
                    "question",
                    format!("The agent is asking a choice question: {question}"),
                );
            }
            None => self.set_blocked(&event.pane.pane_id, None),
        }
        if task.continues_used >= settings.max_continues {
            return decided(
                Verdict::Ask,
                "limit",
                format!(
                    "{} continues were sent for this task; the limit is {}.",
                    task.continues_used, settings.max_continues
                ),
            );
        }
        // 2.4 item 3, as the 22:03 EST amendment narrows it.
        let services = self.services.as_ref()?;
        let question = services.pending_question(&event.pane.pane_id);
        let answer = match judge::prompt(event, &plan, question.as_deref()) {
            Ok((system, user)) => services.judge(&system, &user).await,
            Err(e) => Err(e),
        };
        let judged = match answer {
            Ok(answer) => judge::parse(&answer),
            Err(e) => {
                log::info!("[policy] pane={}: no judgement: {e}", event.pane.pane_id);
                None
            }
        };
        let Some(judged) = judged else {
            return decided(
                Verdict::Ask,
                "model",
                "The policy's model gave no answer it could read.".to_string(),
            );
        };
        let with_step = |d: Option<Decision>, step: Option<u32>| {
            d.map(|mut d| {
                d.step = step;
                d
            })
        };
        match judged.verdict {
            Verdict::Stop => with_step(
                decided(Verdict::Stop, "judgement", judged.reason),
                judged.step,
            ),
            Verdict::Ask => with_step(
                decided(Verdict::Ask, "judgement", judged.reason),
                judged.step,
            ),
            Verdict::Continue if !judged.asks_continue => with_step(
                decided(
                    Verdict::Ask,
                    "no_question",
                    "The agent did not ask to go on to the next step.".to_string(),
                ),
                judged.step,
            ),
            Verdict::Continue => match judged.step {
                None => decided(
                    Verdict::Ask,
                    "model",
                    "The policy's model named no step.".to_string(),
                ),
                Some(n) if n as usize > plan.steps.len() => with_step(
                    decided(
                        Verdict::Stop,
                        "complete",
                        "Every step of the plan is done.".to_string(),
                    ),
                    Some(n),
                ),
                Some(n) => with_step(
                    decided(Verdict::Continue, "judgement", judged.reason),
                    Some(n),
                ),
            },
        }
    }
}

/// No task store: the plugin runs without the daemon's state folder.
pub const NO_STORE: i64 = -32120;
/// No plan at the path asked for.
pub const NO_PLAN: i64 = -32121;
/// The plan has no task, or its status does not allow one to work.
pub const NO_TASK: i64 = -32122;

/// The project's root, as tasks are kept by.
fn root_of(ctx: &PluginCtx) -> Result<String, PluginError> {
    Ok(ctx.project()?.root.display().to_string())
}

/// A call's `path`, a plan's.
fn path_param(params: &Value) -> Result<String, PluginError> {
    params
        .get("path")
        .and_then(Value::as_str)
        .and_then(plans::plan_path)
        .ok_or_else(|| PluginError::invalid("Give the plan's path, docs/design/<name>_PLAN.md."))
}

/// The plan at the call's `path`.
async fn plan_of(ctx: &PluginCtx, path: &str) -> Result<plans::PlanDoc, PluginError> {
    plans::read(ctx, path).await.map_err(|e| {
        if e.code == super::codes::READ_REFUSED && e.message.contains("does not exist") {
            PluginError::new(NO_PLAN, format!("No plan at {path}"))
        } else {
            e
        }
    })
}

/// `NOT_PERMITTED` unless the caller may approve.
fn need_approver(ctx: &PluginCtx, method: &str) -> Result<(), PluginError> {
    if ctx.principal().can_approve {
        return Ok(());
    }
    log::info!(
        "[policy] {method} refused for {}: it may not approve",
        ctx.principal().id
    );
    Err(PluginError::new(
        wire::NOT_PERMITTED,
        format!("{method} needs a device that may approve."),
    ))
}

#[async_trait::async_trait]
impl Plugin for PlanPolicy {
    fn ns(&self) -> &'static str {
        NS
    }

    fn name(&self) -> &'static str {
        "Plan policy"
    }

    fn description(&self) -> &'static str {
        "Reads each Finished card's summary for a plan you chose Build on, and asks you to confirm \
         continue when the agent asks to go on to the next step."
    }

    /// A git hash until its first release, 0.1.0, once its methods are
    /// all in (plan 20261028_04, decision 3.4).
    fn version(&self) -> PluginVersion {
        PluginVersion::Hash("4703cb8")
    }

    fn menu(&self) -> &'static [MenuItem] {
        &MENU
    }

    fn read_patterns(&self) -> &'static [&'static str] {
        &READ
    }

    fn commands(&self) -> &'static [CommandSpec] {
        &COMMANDS
    }

    async fn call(
        &self,
        method: &str,
        params: Value,
        ctx: &PluginCtx,
    ) -> Result<Value, PluginError> {
        match method {
            "state" => Ok(self.state_json()),
            "pause" => {
                need_approver(ctx, method)?;
                let Some(paused) = params.get("paused").and_then(Value::as_bool) else {
                    return Err(PluginError::invalid("pause needs { paused: bool }."));
                };
                self.state.lock().unwrap().paused = paused;
                log::info!(
                    "[policy] {} by {}",
                    if paused { "paused" } else { "resumed" },
                    ctx.principal().id
                );
                self.notify("state", self.state_json());
                Ok(json!({ "paused": paused }))
            }
            "plans" => {
                let store = self.store()?;
                let root = root_of(ctx)?;
                let rows: Vec<Value> = plans::list(ctx)
                    .await?
                    .iter()
                    .map(|p| self.row(p, store.find(&root, &p.path).as_ref()))
                    .collect();
                Ok(json!({ "plans": rows }))
            }
            "stop" => {
                need_approver(ctx, method)?;
                let Some(pane_id) = params
                    .get("pane_id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                else {
                    return Err(PluginError::invalid("stop needs { pane_id }."));
                };
                self.stop_pane(pane_id, ctx);
                Ok(json!({}))
            }
            "build" | "task_enable" | "task_disable" | "task_remove" => {
                need_approver(ctx, method)?;
                self.task_call(method, &params, ctx).await
            }
            other => Err(PluginError::no_method(other)),
        }
    }

    /// 2.4, after the quiet time of the 22:17 EST amendment, item 7.
    async fn on_card_summary(&self, event: &CardSummaryEvent, ctx: &PluginCtx) {
        let settings = Settings::of(ctx.config());
        if self.paused() {
            return;
        }
        // 2.1 item 2: an agent it may not drive gets no decision at all.
        if !settings.drives(&event.pane.agent) {
            return;
        }
        let pane_id = event.pane.pane_id.clone();
        let serial = event.key.0;
        {
            let mut state = self.state.lock().unwrap();
            let session = state.sessions.entry(pane_id.clone()).or_default();
            session.latest = serial;
            session.agent = event.pane.agent.clone();
            session.project = event.project.clone();
        }
        if settings.quiet_secs > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(settings.quiet_secs)).await;
        }
        let newest = {
            let state = self.state.lock().unwrap();
            state.sessions.get(&pane_id).map(|s| s.latest) == Some(serial)
        };
        if !newest || self.paused() {
            return;
        }
        if let Some(mut decision) = self.decide(event, ctx, &settings).await {
            match decision.verdict {
                Verdict::Continue => self.continue_through_the_gate(&mut decision, ctx, &settings),
                Verdict::Stop => self.clear_counters(&decision, ctx),
                Verdict::Ask => {}
            }
            self.emit(&decision);
        }
    }
}

impl PlanPolicy {
    /// The principal its continues are asked as (2.5 item 2): only the
    /// configured agents, never an approver.
    fn principal(settings: &Settings) -> crate::core::principal::Principal {
        crate::core::principal::Principal {
            id: PRINCIPAL_ID.to_string(),
            agents: Some(settings.agents.clone()),
            can_approve: false,
        }
    }

    /// 2.5: `continue_command` through `policy::check` (on the agent's
    /// `allowed_commands` only) and a standing confirmation, never sent
    /// directly (`confirm = "always"`). A refusal makes the decision
    /// `stop`, rule `denied`. `continues_used` counts the confirmations
    /// that end `sent`.
    fn continue_through_the_gate(
        &self,
        decision: &mut Decision,
        ctx: &PluginCtx,
        settings: &Settings,
    ) {
        let Some(services) = self.services.clone() else {
            return;
        };
        let principal = Self::principal(settings);
        let agents = services.agents();
        let command = settings.continue_command.as_str();
        let deny = |decision: &mut Decision, why: String| {
            log::info!(
                "[policy] pane={} agent={}: '{command}' denied: {why}",
                decision.pane_id,
                decision.agent
            );
            decision.verdict = Verdict::Stop;
            decision.rule = "denied";
            decision.reason = why;
        };
        let checked = match crate::core::policy::check(
            &principal,
            &agents,
            &decision.agent,
            &decision.pane_id,
            command,
        ) {
            Ok(checked) => checked,
            Err(denied) => return deny(decision, denied.to_string()),
        };
        if !crate::core::policy::is_allow_listed(&agents, &decision.agent, command) {
            return deny(
                decision,
                format!(
                    "'{command}' is not on {}'s allowed_commands.",
                    decision.agent
                ),
            );
        }
        let (confirm_id, outcome) = match services.request_confirmation(
            &principal,
            &decision.pane_id,
            &decision.agent,
            &checked.command,
        ) {
            Ok(opened) => opened,
            Err(e) => return deny(decision, e),
        };
        log::info!(
            "[policy] pane={} plan={}: confirmation {confirm_id} opened for '{}'",
            decision.pane_id,
            decision.plan.as_deref().unwrap_or("-"),
            checked.command
        );
        decision.confirm_id = Some(confirm_id);
        let pane_id = decision.pane_id.clone();
        {
            let mut state = self.state.lock().unwrap();
            state.sessions.entry(pane_id.clone()).or_default().pending = Some(confirm_id);
        }
        let root = root_of(ctx).ok();
        let path = decision.plan.clone();
        let state = Arc::clone(&self.state);
        let store = self.store().ok();
        tokio::spawn(async move {
            let outcome = outcome.await.unwrap_or("cancelled");
            log::info!("[policy] pane={pane_id}: confirmation {confirm_id} {outcome}");
            {
                let mut state = state.lock().unwrap();
                if let Some(s) = state.sessions.get_mut(&pane_id)
                    && s.pending == Some(confirm_id)
                {
                    s.pending = None;
                }
            }
            if outcome == "sent"
                && let (Some(root), Some(path), Some(store)) = (root, path, store)
            {
                count_continue(&store, &root, &path);
            }
        });
    }
}

/// One more continue sent for the task of `path` in `root`.
fn count_continue(store: &Store, root: &str, path: &str) {
    {
        match store.update(root, path, |old| {
            old.map(|t| Task {
                continues_used: t.continues_used + 1,
                ..t
            })
        }) {
            Ok(Some(task)) => log::info!(
                "[policy] {path}: {} continue(s) sent for this task",
                task.continues_used
            ),
            Ok(None) => {}
            Err(e) => log::warn!("[policy] {path}: {e}"),
        }
    }
}

impl PlanPolicy {
    /// A `stop` clears the counters of the decision's task (2.5 item 4).
    fn clear_counters(&self, decision: &Decision, ctx: &PluginCtx) {
        let (Some(path), Ok(root), Ok(store)) = (&decision.plan, root_of(ctx), self.store()) else {
            return;
        };
        if store
            .find(&root, path)
            .is_some_and(|t| t.continues_used > 0)
        {
            let _ = store.update(&root, path, |old| {
                old.map(|t| Task {
                    continues_used: 0,
                    ..t
                })
            });
        }
    }

    /// `stop { pane_id }`: its waiting continue cancelled, its tasks'
    /// counters cleared.
    fn stop_pane(&self, pane_id: &str, ctx: &PluginCtx) {
        let pending = {
            let mut state = self.state.lock().unwrap();
            state
                .sessions
                .get_mut(pane_id)
                .and_then(|s| s.pending.take())
        };
        let settings = Settings::of(ctx.config());
        if let (Some(id), Some(services)) = (pending, &self.services) {
            services.cancel_confirmation(&Self::principal(&settings), id);
        }
        if let Ok(store) = self.store() {
            for task in store.load() {
                if task
                    .pane_id
                    .as_deref()
                    .is_some_and(|p| crate::watch::pane_ids_match(p, pane_id))
                {
                    let _ = store.update(&task.project_root, &task.path, |old| {
                        old.map(|t| Task {
                            continues_used: 0,
                            ..t
                        })
                    });
                }
            }
        }
        log::info!(
            "[policy] stop pane={pane_id} by {}{}",
            ctx.principal().id,
            pending.map_or(String::new(), |id| format!(", confirmation {id} cancelled"))
        );
        self.notify("state", self.state_json());
    }

    /// `build`, `task_enable`, `task_disable` and `task_remove` (the
    /// 22:31 EST amendment, items 1 to 3): the plan's row as it now is,
    /// also sent to every phone as `approval`.
    async fn task_call(
        &self,
        method: &str,
        params: &Value,
        ctx: &PluginCtx,
    ) -> Result<Value, PluginError> {
        let store = self.store()?;
        let root = root_of(ctx)?;
        let path = path_param(params)?;
        let plan = plan_of(ctx, &path).await?;
        let by = ctx.principal().id.clone();
        let now = crate::server::link::auth::now_rfc3339();
        let failed = |e: String| PluginError::new(NO_STORE, e);
        let task = match method {
            "build" => {
                let pane_id = params
                    .get("pane_id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                    .map(str::to_string)
                    .or_else(|| ctx.project().ok().map(|p| p.pane_id.clone()));
                store
                    .update(&root, &path, |old| {
                        Some(Task {
                            project_root: root.clone(),
                            path: path.clone(),
                            pane_id,
                            hash: plan.hash.clone(),
                            armed: true,
                            autocontinue: old.as_ref().is_some_and(|t| t.autocontinue),
                            by: by.clone(),
                            at: now.clone(),
                            started_at: old.and_then(|t| t.started_at),
                            continues_used: 0,
                            status: Some(plan.status.clone()),
                            amendments: Some(plan.amendments.len() as u32),
                        })
                    })
                    .map_err(failed)?
            }
            "task_enable" => {
                if store.find(&root, &path).is_none() {
                    return Err(PluginError::new(NO_TASK, "Choose Build on the plan first."));
                }
                if !plan.status_allows() {
                    return Err(PluginError::new(
                        NO_TASK,
                        format!("The plan's status, {}, does not allow work.", plan.status),
                    ));
                }
                store
                    .update(&root, &path, |old| {
                        old.map(|t| Task {
                            autocontinue: true,
                            hash: plan.hash.clone(),
                            started_at: Some(now.clone()),
                            status: Some(plan.status.clone()),
                            amendments: Some(plan.amendments.len() as u32),
                            ..t
                        })
                    })
                    .map_err(failed)?
            }
            "task_disable" => store
                .update(&root, &path, |old| {
                    old.map(|t| Task {
                        autocontinue: false,
                        ..t
                    })
                })
                .map_err(failed)?,
            _ => store.update(&root, &path, |_| None).map_err(failed)?,
        };
        log::info!(
            "[policy] {method} {path} in {root} by {by}: {}",
            match &task {
                None => "no task (disarmed)".to_string(),
                Some(t) => format!(
                    "armed, autocontinue {}",
                    if t.autocontinue { "on" } else { "off" }
                ),
            }
        );
        let row = self.row(&plan, task.as_ref());
        self.notify("approval", row.clone());
        Ok(row)
    }
}

#[cfg(test)]
mod tests;
