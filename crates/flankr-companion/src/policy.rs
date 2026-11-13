//! The plan policy plugin's params and notifications, typed
//! (docs/design/20261112_02_PLAN_POLICY_AGENT_PLAN.md, 2.2 and 2.6, with
//! the 22:31 EST amendment's task calls). They travel as `plugin.call`'s
//! params and result and `plugin.notify`'s params; these types read and
//! write that JSON. Every field is defaulted when absent, so a daemon that
//! sends fewer still reads.

use libflankr_proto::companion::PluginNotify;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The plugin's package name.
pub const NS: &str = "dev.flankr.plugins.policy";

/// Its `plugin.call` methods.
pub mod methods {
    pub const STATE: &str = "state";
    pub const PAUSE: &str = "pause";
    pub const STOP: &str = "stop";
    pub const PLANS: &str = "plans";
    pub const BUILD: &str = "build";
    pub const TASK_ENABLE: &str = "task_enable";
    pub const TASK_DISABLE: &str = "task_disable";
    pub const TASK_REMOVE: &str = "task_remove";
    pub const WORKTREE_CREATE: &str = "worktree_create";
}

/// Its `plugin.notify` methods.
pub mod notices {
    pub const DECISION: &str = "decision";
    pub const APPROVAL: &str = "approval";
    pub const STATE: &str = "state";
}

/// The principal id its confirmations are requested by: a card whose
/// `requested_by` is this reads **Plan policy**.
pub const PRINCIPAL_ID: &str = "plugin:policy";

/// One numbered step of a plan.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyStep {
    #[serde(default)]
    pub n: u32,
    #[serde(default)]
    pub title: String,
}

/// A plan's row: `plans`, `build`, `task_*` and the `approval` notice.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyPlan {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub base: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub status_allows: bool,
    /// Build was chosen: the plan has an active task.
    #[serde(default)]
    pub armed: bool,
    #[serde(default)]
    pub approved: bool,
    #[serde(default)]
    pub approved_at: Option<String>,
    #[serde(default)]
    pub approved_by: Option<String>,
    #[serde(default)]
    pub changed_since_approval: bool,
    #[serde(default)]
    pub autocontinue: bool,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub pane_id: Option<String>,
    #[serde(default)]
    pub steps: Vec<PolicyStep>,
    #[serde(default)]
    pub continues_used: u32,
    /// A choice question from the agent holds the task.
    #[serde(default)]
    pub blocked: bool,
    #[serde(default)]
    pub question: Option<String>,
}

/// One decision: the `decision` notice, and a session's last.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyDecision {
    #[serde(default)]
    pub pane_id: String,
    #[serde(default)]
    pub agent: String,
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub plan: Option<String>,
    #[serde(default)]
    pub plan_title: Option<String>,
    #[serde(default)]
    pub step: Option<u32>,
    /// `continue`, `stop` or `ask`.
    #[serde(default)]
    pub decision: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub rule: String,
    /// The Finished card's serial (`Serial::to_u64`).
    #[serde(default)]
    pub summary_serial: u64,
    /// For a `continue`: the confirmation it waits on.
    #[serde(default)]
    pub confirm_id: Option<u64>,
    #[serde(default)]
    pub worktree: Option<String>,
    #[serde(default)]
    pub at: String,
}

impl PolicyDecision {
    /// The Finished card's policy line (2.7 item 2) without the
    /// confirmation's state: "continue, step 4", "stop: <reason>".
    pub fn line(&self) -> String {
        match (self.decision.as_str(), self.step) {
            ("continue", Some(n)) => format!("continue, step {n}"),
            ("continue", None) => "continue".to_string(),
            (word, _) if self.reason.is_empty() => word.to_string(),
            (word, _) => format!("{word}: {}", self.reason),
        }
    }
}

/// One pane the policy has seen a card from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicySession {
    #[serde(default)]
    pub pane_id: String,
    #[serde(default)]
    pub agent: String,
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub plan: Option<String>,
    #[serde(default)]
    pub last_decision: Option<PolicyDecision>,
    #[serde(default)]
    pub blocked: bool,
    #[serde(default)]
    pub question: Option<String>,
}

/// `state`'s result and the `state` notice.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyState {
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub sessions: Vec<PolicySession>,
}

/// `plans`' result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyPlans {
    #[serde(default)]
    pub plans: Vec<PolicyPlan>,
}

/// `worktree_create`'s result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyWorktree {
    #[serde(default)]
    pub worktree_path: String,
    #[serde(default)]
    pub branch: String,
}

/// A `plugin.notify` of the plan policy plugin, read.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PolicyNotice {
    Decision(PolicyDecision),
    Approval(PolicyPlan),
    State(PolicyState),
}

impl PolicyNotice {
    /// `note` read, when it is this plugin's and one of its notices.
    pub fn of(note: &PluginNotify) -> Option<PolicyNotice> {
        if note.ns != NS {
            return None;
        }
        let params = note.params.clone();
        match note.method.as_str() {
            notices::DECISION => serde_json::from_value(params)
                .ok()
                .map(PolicyNotice::Decision),
            notices::APPROVAL => serde_json::from_value(params)
                .ok()
                .map(PolicyNotice::Approval),
            notices::STATE => serde_json::from_value(params).ok().map(PolicyNotice::State),
            _ => None,
        }
    }
}

/// The params of a call on one plan: `{ path }`, with `pane_id` for
/// `build` and `worktree_create`.
pub fn plan_params(path: &str, pane_id: Option<&str>) -> Value {
    match pane_id {
        Some(p) => serde_json::json!({ "path": path, "pane_id": p }),
        None => serde_json::json!({ "path": path }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The JSON of the plan's 2.2, as the daemon sends it.
    fn decision_json() -> Value {
        json!({
            "pane_id": "w1:p1", "agent": "claude", "project": "flankr",
            "plan": "docs/design/20261112_02_PLAN_POLICY_AGENT_PLAN.md",
            "plan_title": "Plan policy agent", "step": 4, "decision": "continue",
            "reason": "Step 3 is done.", "rule": "judgement", "summary_serial": 42,
            "confirm_id": 7, "worktree": null, "at": "2026-11-12T22:00:00-05:00"
        })
    }

    fn plan_json() -> Value {
        json!({
            "path": "docs/design/20261112_02_PLAN_POLICY_AGENT_PLAN.md",
            "base": "20261112_02_PLAN_POLICY_AGENT_PLAN", "title": "Plan policy agent",
            "status": "In implementation", "status_allows": true, "armed": true,
            "approved": true, "approved_at": "2026-11-12T22:00:00-05:00",
            "approved_by": "paired:phone", "changed_since_approval": false,
            "autocontinue": true, "started_at": "2026-11-12T22:01:00-05:00",
            "pane_id": "w1:p1", "steps": [{"n": 1, "title": "Daemon: the plugin skeleton"}],
            "continues_used": 1, "blocked": false, "question": null
        })
    }

    #[test]
    fn round_trips() {
        let d: PolicyDecision = serde_json::from_value(decision_json()).unwrap();
        assert_eq!(d.step, Some(4));
        assert_eq!(d.confirm_id, Some(7));
        assert_eq!(serde_json::to_value(&d).unwrap(), decision_json());
        assert_eq!(d.line(), "continue, step 4");
        let p: PolicyPlan = serde_json::from_value(plan_json()).unwrap();
        assert!(p.armed && p.autocontinue);
        assert_eq!(p.steps[0].title, "Daemon: the plugin skeleton");
        assert_eq!(serde_json::to_value(&p).unwrap(), plan_json());
        let state = json!({
            "paused": false,
            "sessions": [{"pane_id": "w1:p1", "agent": "claude", "project": "flankr",
                "plan": null, "last_decision": decision_json(), "blocked": true,
                "question": "Which way?"}]
        });
        let s: PolicyState = serde_json::from_value(state.clone()).unwrap();
        assert!(s.sessions[0].blocked);
        assert_eq!(serde_json::to_value(&s).unwrap(), state);
        // Fewer fields still read.
        let short: PolicyDecision =
            serde_json::from_value(json!({"decision": "stop", "reason": "Tests fail."})).unwrap();
        assert_eq!(short.line(), "stop: Tests fail.");
        let plans: PolicyPlans = serde_json::from_value(json!({"plans": [plan_json()]})).unwrap();
        assert_eq!(plans.plans.len(), 1);
    }

    #[test]
    fn notices() {
        let note = |ns: &str, method: &str, params: Value| PluginNotify {
            ns: ns.into(),
            method: method.into(),
            params,
        };
        assert!(matches!(
            PolicyNotice::of(&note(NS, "decision", decision_json())),
            Some(PolicyNotice::Decision(d)) if d.step == Some(4)
        ));
        assert!(matches!(
            PolicyNotice::of(&note(NS, "approval", plan_json())),
            Some(PolicyNotice::Approval(p)) if p.armed
        ));
        assert!(matches!(
            PolicyNotice::of(&note(NS, "state", json!({"paused": true}))),
            Some(PolicyNotice::State(s)) if s.paused
        ));
        assert_eq!(
            PolicyNotice::of(&note("dev.flankr.plugins.planning", "decision", json!({}))),
            None
        );
        assert_eq!(PolicyNotice::of(&note(NS, "other", json!({}))), None);
        assert_eq!(
            plan_params("docs/design/x.md", Some("w1:p1")),
            json!({"path": "docs/design/x.md", "pane_id": "w1:p1"})
        );
    }
}
