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

use std::sync::Mutex;

use serde_json::{Value, json};

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

/// What the plugin keeps while the daemon runs.
#[derive(Debug, Default)]
struct State {
    paused: bool,
}

/// The plan policy plugin.
#[derive(Default)]
pub struct PlanPolicy {
    state: Mutex<State>,
}

impl PlanPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether it is paused.
    pub fn paused(&self) -> bool {
        self.state.lock().unwrap().paused
    }

    /// `state {}`'s answer.
    fn state_json(&self) -> Value {
        let state = self.state.lock().unwrap();
        json!({ "paused": state.paused, "sessions": [] })
    }
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
                Ok(json!({ "paused": paused }))
            }
            other => Err(PluginError::no_method(other)),
        }
    }
}

#[cfg(test)]
mod tests;
