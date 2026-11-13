//! The plan policy plugin's params and notifications, typed
//! (docs/design/20261112_02_PLAN_POLICY_AGENT_PLAN.md, 2.2 and 2.6, with
//! the 22:31 EST amendment's task calls). They travel as `plugin.call`'s
//! params and result and `plugin.notify`'s params; these types read and
//! write that JSON. Every field is defaulted when absent, so a daemon that
//! sends fewer still reads.

use libflankr_proto::common::AgentInfo;
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
    pub const OVERVIEW: &str = "overview";
}

/// Its `plugin.notify` methods.
pub mod notices {
    pub const DECISION: &str = "decision";
    pub const APPROVAL: &str = "approval";
    pub const STATE: &str = "state";
    pub const OVERVIEW: &str = "overview";
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

/// A pane's worktree, as `overview` gives it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyWorktreeRef {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
}

/// One pane linked to a plan in `overview`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyAgentLink {
    #[serde(default)]
    pub pane_id: String,
    /// How it was linked: `pinned`, `reply_path`, `summary_title`,
    /// `worktree_branch`, `gemini`, or a word a newer daemon adds.
    #[serde(default)]
    pub linked_by: String,
    #[serde(default)]
    pub worktree: Option<PolicyWorktreeRef>,
    #[serde(default)]
    pub last_decision: Option<PolicyDecision>,
}

/// One plan's group in `overview`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyPlanGroup {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub armed: bool,
    #[serde(default)]
    pub approved: bool,
    /// Build was chosen on it: an active task.
    #[serde(default)]
    pub task: bool,
    #[serde(default)]
    pub autocontinue: bool,
    #[serde(default)]
    pub steps_total: Option<u32>,
    #[serde(default)]
    pub current_step: Option<u32>,
    #[serde(default)]
    pub agents: Vec<PolicyAgentLink>,
}

/// `overview`'s result and notice (plan 20261112_03, 2.2 item 1).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyOverview {
    #[serde(default)]
    pub plans: Vec<PolicyPlanGroup>,
    #[serde(default)]
    pub unlinked: Vec<String>,
    #[serde(default)]
    pub paused: bool,
}

/// A plan group joined to the agents the phone lists; `plan` is `None`
/// for **Not on a plan**.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanGroup {
    pub plan: Option<PolicyPlanGroup>,
    /// Each linked pane's agent and its link (none under **Not on a
    /// plan**).
    pub agents: Vec<(AgentInfo, Option<PolicyAgentLink>)>,
}

impl PolicyOverview {
    /// The groups for `agents` (`agents.list`): each plan's linked panes
    /// that the phone lists, in the daemon's order, and last **Not on a
    /// plan**: `unlinked` and every agent pane the daemon did not
    /// mention, shells left out.
    pub fn join(&self, agents: &[AgentInfo]) -> Vec<PlanGroup> {
        let mut named = std::collections::HashSet::new();
        let mut out: Vec<PlanGroup> = Vec::new();
        for plan in &self.plans {
            let mut linked = Vec::new();
            for link in &plan.agents {
                if let Some(a) = agents.iter().find(|a| a.pane_id == link.pane_id) {
                    named.insert(a.pane_id.clone());
                    linked.push((a.clone(), Some(link.clone())));
                }
            }
            out.push(PlanGroup {
                plan: Some(plan.clone()),
                agents: linked,
            });
        }
        let rest: Vec<(AgentInfo, Option<PolicyAgentLink>)> = agents
            .iter()
            .filter(|a| a.agent != "shell" && !named.contains(&a.pane_id))
            .map(|a| (a.clone(), None))
            .collect();
        out.push(PlanGroup {
            plan: None,
            agents: rest,
        });
        out
    }
}

/// A `plugin.notify` of the plan policy plugin, read.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PolicyNotice {
    Decision(PolicyDecision),
    Approval(PolicyPlan),
    State(PolicyState),
    /// Plan 20261112_03's `overview`.
    Overview(PolicyOverview),
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
            notices::OVERVIEW => serde_json::from_value(params)
                .ok()
                .map(PolicyNotice::Overview),
            _ => None,
        }
    }
}

/// The longest message Build sends: the daemon's arbitrary-command limit.
pub const BUILD_MESSAGE_MAX: usize = 256;

/// What Build's message ends with (plan 20261112_03, the 2026-11-13
/// 00:09 EST amendment, item 1): the agent sets the plan's status first.
pub const BUILD_STATUS_CLAUSE: &str = ", setting its status to In implementation first";

/// Build's message since the 2026-11-13 00:09 EST amendment of plan
/// 20261112_03 (in place of [`build_message`], kept as it was): "approve and build the
/// <date> plan, <title>, setting its status to In implementation first",
/// the date the front matter's without its dashes, the title with every
/// character the daemon's arbitrary-command filter refuses made a space,
/// a `,` or `:` kept only where a space follows, and runs of spaces
/// folded; a title that would make it longer than [`BUILD_MESSAGE_MAX`]
/// is cut at a word boundary, and the status clause always stays. `None`
/// when the date is not `YYYY-MM-DD` or the title has no words left.
pub fn build_request(date: &str, title: &str) -> Option<String> {
    let digits: String = date.chars().filter(|c| *c != '-').collect();
    if digits.len() != 8 || !digits.bytes().all(|b| b.is_ascii_digit()) || date.len() != 10 {
        return None;
    }
    let cleaned = filter_clean(title);
    let head = format!("approve and build the {digits} plan, ");
    let room = BUILD_MESSAGE_MAX - head.len() - BUILD_STATUS_CLAUSE.len();
    let mut title = String::new();
    for word in cleaned.split_whitespace() {
        let next = if title.is_empty() {
            word.len()
        } else {
            title.len() + 1 + word.len()
        };
        if next > room {
            break;
        }
        if !title.is_empty() {
            title.push(' ');
        }
        title.push_str(word);
    }
    let title = title.trim_end_matches(|c: char| !c.is_ascii_alphanumeric());
    if !title.chars().any(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some(format!("{head}{title}{BUILD_STATUS_CLAUSE}"))
}

/// `text` as the daemon's filter takes it unchanged: ASCII letters,
/// digits and spaces; an apostrophe between letters; `. , ? :` only after
/// a letter or digit and before a space. Anything else is a space.
fn filter_clean(text: &str) -> String {
    let mut chars: Vec<char> = text
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '\'' | '.' | ',' | '?' | ':') {
                c
            } else {
                ' '
            }
        })
        .collect();
    loop {
        let mut changed = false;
        for i in 0..chars.len() {
            let c = chars[i];
            if c.is_ascii_alphanumeric() || c == ' ' {
                continue;
            }
            let prev = i.checked_sub(1).map(|p| chars[p]);
            let next = chars.get(i + 1).copied();
            let ok = if c == '\'' {
                prev.is_some_and(|p| p.is_ascii_alphabetic())
                    && next.is_some_and(|n| n.is_ascii_alphabetic())
            } else {
                prev.is_some_and(|p| p.is_ascii_alphanumeric()) && next.is_none_or(|n| n == ' ')
            };
            if !ok {
                chars[i] = ' ';
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    chars.into_iter().collect()
}

/// Build's message as the 22:21 EST amendment of plan 20261112_03 had
/// it; the app now sends [`build_request`]'s: "approve
/// and build the <date> plan, <title>", the date the front matter's
/// without its dashes, the title with every character the daemon's
/// arbitrary-command filter refuses made a space, a `,` or `:` kept only
/// where a space follows, and runs of spaces folded. `None` when the date
/// is not `YYYY-MM-DD` or the title has no words left.
pub fn build_message(date: &str, title: &str) -> Option<String> {
    let digits: String = date.chars().filter(|c| *c != '-').collect();
    if digits.len() != 8 || !digits.bytes().all(|b| b.is_ascii_digit()) || date.len() != 10 {
        return None;
    }
    let chars: Vec<char> = title.chars().collect();
    let mut cleaned = String::with_capacity(title.len());
    for (i, &c) in chars.iter().enumerate() {
        let next_space = chars.get(i + 1).is_none_or(|n| *n == ' ');
        let keep = c.is_ascii_alphanumeric()
            || c == ' '
            || matches!(c, '.' | '\'' | '?')
            || (matches!(c, ',' | ':') && next_space);
        cleaned.push(if keep { c } else { ' ' });
    }
    let title = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let title = title.trim_end_matches([',', ':']).trim();
    if !title.chars().any(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some(format!("approve and build the {digits} plan, {title}"))
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

    fn agent(pane: &str, kind: &str) -> AgentInfo {
        serde_json::from_value(json!({
            "pane_id": pane, "agent": kind, "status": "idle", "focused": false
        }))
        .unwrap()
    }

    #[test]
    fn the_overview_and_its_join() {
        let body = json!({
            "plans": [{
                "path": "docs/design/20261112_02_PLAN_POLICY_AGENT_PLAN.md",
                "title": "Plan policy agent", "status": "In implementation",
                "armed": true, "approved": true, "task": true, "autocontinue": false,
                "steps_total": 9, "current_step": 3,
                "agents": [
                    {"pane_id": "p12", "linked_by": "reply_path",
                     "worktree": {"path": "/r/.claude/worktrees/policy", "branch": "plan/20261112_02", "label": "policy"},
                     "last_decision": decision_json()},
                    {"pane_id": "p13", "linked_by": "a_new_way"}
                ]
            }],
            "unlinked": ["p7"],
            "paused": false,
            "something_new": 1
        });
        let o: PolicyOverview = serde_json::from_value(body).unwrap();
        assert_eq!(o.plans[0].current_step, Some(3));
        assert_eq!(o.plans[0].agents[1].worktree, None);
        assert_eq!(o.plans[0].agents[1].last_decision, None);
        assert_eq!(o.plans[0].agents[1].linked_by, "a_new_way");
        let back: PolicyOverview =
            serde_json::from_value(serde_json::to_value(&o).unwrap()).unwrap();
        assert_eq!(back, o);
        let groups = o.join(&[
            agent("p12", "claude"),
            agent("p7", "codex"),
            agent("p8", "claude"),
            agent("p9", "shell"),
        ]);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].agents.len(), 1, "p13 is not listed by the phone");
        assert_eq!(groups[0].agents[0].0.pane_id, "p12");
        assert!(groups[1].plan.is_none());
        let rest: Vec<&str> = groups[1]
            .agents
            .iter()
            .map(|(a, _)| a.pane_id.as_str())
            .collect();
        assert_eq!(
            rest,
            ["p7", "p8"],
            "unknown to the daemon, under Not on a plan; no shell"
        );
        let note = PluginNotify {
            ns: NS.into(),
            method: "overview".into(),
            params: json!({"paused": true}),
        };
        assert!(matches!(PolicyNotice::of(&note), Some(PolicyNotice::Overview(o)) if o.paused));
    }

    /// The characters the daemon's arbitrary-command filter takes as
    /// they are (`commands::validate_arbitrary_command`; the daemon's own
    /// test checks these messages against it).
    fn plain(text: &str) -> bool {
        text.is_ascii()
            && text.chars().all(|c| {
                c.is_ascii_alphanumeric() || matches!(c, ' ' | ',' | ':' | '\'' | '?' | '.')
            })
    }

    #[test]
    fn build_messages_pass_the_filter() {
        assert_eq!(
            build_message("2026-11-12", "Active plans, a plan-grouped home view").as_deref(),
            Some("approve and build the 20261112 plan, Active plans, a plan grouped home view")
        );
        assert_eq!(
            build_message("2026-11-12", "`libherdr`: worktree/API [v2]").as_deref(),
            Some("approve and build the 20261112 plan, libherdr : worktree API v2")
        );
        assert_eq!(
            build_message("2026-11-12", "a,b:c").as_deref(),
            Some("approve and build the 20261112 plan, a b c")
        );
        assert_eq!(build_message("20261112", "x"), None);
        assert_eq!(build_message("2026-11-12", "-- / --"), None);
    }

    #[test]
    fn build_requests_ask_for_the_status_and_pass_the_filter() {
        let m = build_request("2026-11-12", "Active plans, a plan-grouped home view").unwrap();
        assert_eq!(
            m,
            "approve and build the 20261112 plan, Active plans, a plan grouped home view, \
             setting its status to In implementation first"
        );
        assert!(plain(&m));
        let m = build_request("2026-11-12", "`libherdr`: worktree/API [v2]").unwrap();
        assert_eq!(
            m,
            "approve and build the 20261112 plan, libherdr worktree API v2, setting its status \
             to In implementation first"
        );
        assert!(plain(&m));
        assert_eq!(build_request("20261112", "x"), None);
        assert_eq!(build_request("2026-11-12", "-- / --"), None);
        // A long title is cut at a word, never the status clause.
        let long = "Garden irrigation schedule ".repeat(20);
        let m = build_request("2026-11-13", &long).unwrap();
        assert!(m.len() <= BUILD_MESSAGE_MAX, "{}", m.len());
        assert!(m.ends_with(BUILD_STATUS_CLAUSE));
        assert!(m.contains("Garden irrigation"));
        assert!(!m.contains("  "));
        let word = "x".repeat(300);
        assert_eq!(build_request("2026-11-13", &word), None, "no word fits");
        // Symbols only where the filter takes them.
        assert_eq!(
            build_request("2026-11-13", "What's next? v2.0 notes, 'quoted'?").unwrap(),
            "approve and build the 20261113 plan, What's next? v2 0 notes, quoted, setting its \
             status to In implementation first"
        );
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
