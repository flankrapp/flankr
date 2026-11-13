//! Which plan a card is about, and the decisions (plan 20261112_02, 2.3
//! item 5 as the 22:31 EST amendment orders it, and 2.4).

use serde_json::{Value, json};

use super::plans::{PlanDoc, plan_path};
use super::store::Task;
use crate::sessions::hub::CardSummaryEvent;

/// What the policy decided for a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Continue,
    Stop,
    Ask,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Continue => "continue",
            Verdict::Stop => "stop",
            Verdict::Ask => "ask",
        }
    }

    pub fn from_name(name: &str) -> Option<Verdict> {
        match name.trim().to_ascii_lowercase().as_str() {
            "continue" => Some(Verdict::Continue),
            "stop" => Some(Verdict::Stop),
            "ask" => Some(Verdict::Ask),
            _ => None,
        }
    }
}

/// One decision, as the `decision` notification and the log carry it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub pane_id: String,
    pub agent: String,
    pub project: String,
    /// The plan's path, when one matched.
    pub plan: Option<String>,
    pub plan_title: Option<String>,
    pub step: Option<u32>,
    pub verdict: Verdict,
    pub reason: String,
    /// The rule that decided, or `model` / `judgement`.
    pub rule: &'static str,
    /// The Finished card's serial.
    pub summary_serial: u64,
    pub confirm_id: Option<u64>,
    /// The checkout the pane is in, when it is a worktree of the project.
    pub worktree: Option<String>,
    pub at: String,
}

impl Decision {
    pub fn new(
        event: &CardSummaryEvent,
        verdict: Verdict,
        rule: &'static str,
        reason: impl Into<String>,
    ) -> Decision {
        Decision {
            pane_id: event.pane.pane_id.clone(),
            agent: event.pane.agent.clone(),
            project: event.project.clone(),
            plan: None,
            plan_title: None,
            step: None,
            verdict,
            reason: reason.into(),
            rule,
            summary_serial: event.key.0,
            confirm_id: None,
            worktree: None,
            at: crate::server::link::auth::now_rfc3339(),
        }
    }

    pub fn on(mut self, plan: &PlanDoc) -> Decision {
        self.plan = Some(plan.path.clone());
        self.plan_title = Some(plan.title.clone());
        self
    }

    pub fn json(&self) -> Value {
        json!({
            "pane_id": self.pane_id,
            "agent": self.agent,
            "project": self.project,
            "plan": self.plan,
            "plan_title": self.plan_title,
            "step": self.step,
            "decision": self.verdict.as_str(),
            "reason": self.reason,
            "rule": self.rule,
            "summary_serial": self.summary_serial,
            "confirm_id": self.confirm_id,
            "worktree": self.worktree,
            "at": self.at,
        })
    }

    /// The log line of 2.4 item 4.
    pub fn log_line(&self) -> String {
        format!(
            "[policy] pane={} agent={} plan={} step={} decision={} rule={} serial={} reason={}",
            self.pane_id,
            self.agent,
            self.plan.as_deref().unwrap_or("-"),
            self.step.map_or("-".to_string(), |n| n.to_string()),
            self.verdict.as_str(),
            self.rule,
            self.summary_serial,
            crate::sessions::redact::redact(&self.reason)
        )
    }
}

/// The plan paths `text` names, in order, each once.
pub fn named_paths(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("docs/design/") {
        let tail = &rest[at..];
        let end = tail
            .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '/' | '.' | '-')))
            .unwrap_or(tail.len());
        let candidate = tail[..end].trim_end_matches('.');
        let candidate = if candidate.ends_with(".md") {
            candidate.to_string()
        } else {
            format!("{candidate}.md")
        };
        if let Some(path) = plan_path(&candidate)
            && !out.contains(&path)
        {
            out.push(path);
        }
        rest = &tail[end.max(1)..];
    }
    out
}

/// How a plan was matched to a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkedBy {
    /// The task Build made for the card's pane.
    Task,
    /// A plan path in the reply.
    Path,
    /// A plan's title in the reply or the summary.
    Title,
}

/// The plan a card is about (2.3 item 5, ordered by the 22:31 EST
/// amendment, item 5): the armed task of the card's pane, else a plan
/// path in the reply, else a plan's title in the reply or summary.
/// `plans` are newest first.
pub fn match_plan<'a>(
    event: &CardSummaryEvent,
    tasks: &[Task],
    plans: &'a [PlanDoc],
) -> Option<(&'a PlanDoc, LinkedBy)> {
    let by_path = |path: &str| plans.iter().find(|p| p.path == path);
    let mut own: Vec<&Task> = tasks
        .iter()
        .filter(|t| {
            t.armed
                && t.pane_id
                    .as_deref()
                    .is_some_and(|p| crate::watch::pane_ids_match(p, &event.pane.pane_id))
        })
        .collect();
    own.sort_by(|a, b| b.at.cmp(&a.at));
    if let Some(plan) = own.iter().find_map(|t| by_path(&t.path)) {
        return Some((plan, LinkedBy::Task));
    }
    if let Some(plan) = named_paths(&event.reply).iter().find_map(|p| by_path(p)) {
        return Some((plan, LinkedBy::Path));
    }
    let mut said = event.reply.to_lowercase();
    for point in &event.summary.points {
        said.push('\n');
        said.push_str(&point.to_lowercase());
    }
    if let Some(spoken) = &event.summary.spoken {
        said.push('\n');
        said.push_str(&spoken.to_lowercase());
    }
    plans
        .iter()
        .find(|p| p.title.chars().count() >= 8 && said.contains(&p.title.to_lowercase()))
        .map(|p| (p, LinkedBy::Title))
}

/// What changed in a plan since its task last took it: its status and
/// its new amendments, as one sentence.
pub fn describe_change(task: &Task, plan: &PlanDoc) -> String {
    let mut parts = Vec::new();
    if let Some(old) = &task.status
        && old != &plan.status
    {
        parts.push(format!("its status went from {old} to {}", plan.status));
    }
    if let Some(old) = task.amendments {
        let new: Vec<&String> = plan.amendments.iter().skip(old as usize).collect();
        if !new.is_empty() {
            let subjects: Vec<&str> = new.iter().map(|s| s.as_str()).collect();
            parts.push(format!(
                "{} new amendment(s): {}",
                new.len(),
                subjects.join("; ")
            ));
        }
    }
    if parts.is_empty() {
        "The plan changed since autocontinue was switched on.".to_string()
    } else {
        format!("The plan changed: {}.", parts.join(", and "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_paths_in_a_reply() {
        let text = "Step 3 of docs/design/20261112_02_PLAN_POLICY_AGENT_PLAN.md is done; see `docs/design/20261112_03_ACTIVE_PLANS_HOME_VIEW_PLAN.md`. Also docs/design/20261112_02_PLAN_POLICY_AGENT_PLAN and docs/design/notes.md.";
        assert_eq!(
            named_paths(text),
            [
                "docs/design/20261112_02_PLAN_POLICY_AGENT_PLAN.md",
                "docs/design/20261112_03_ACTIVE_PLANS_HOME_VIEW_PLAN.md"
            ]
        );
        assert!(named_paths("nothing here").is_empty());
    }
}
