//! Which plan a card is about, and the decisions (plan 20261112_02, 2.3
//! item 5, ordered as plan 20261112_03's 2026-11-13 11:58 EST amendment
//! orders it, and 2.4).

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

/// The plans a goal's `text` names (plan 20261112_03, the 2026-11-13
/// 11:58 EST amendment, item 1): each word `YYYYMMDD_NN_<NAME>_PLAN`, with
/// or without a path before it or `.md` after it, as
/// `docs/design/<word>.md`, in order, each once.
pub fn goal_paths(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let digits = |from: usize, n: usize| {
        bytes
            .get(from..from + n)
            .is_some_and(|d| d.iter().all(u8::is_ascii_digit))
    };
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let starts = (i == 0 || !word(bytes[i - 1]))
            && digits(i, 8)
            && bytes.get(i + 8) == Some(&b'_')
            && digits(i + 9, 2)
            && bytes.get(i + 11) == Some(&b'_');
        if !starts {
            i += 1;
            continue;
        }
        let end = (i..bytes.len())
            .find(|&j| !word(bytes[j]))
            .unwrap_or(bytes.len());
        let name = &text[i..end];
        if name.ends_with("_PLAN")
            && let Some(path) = plan_path(&format!("docs/design/{name}.md"))
            && !out.contains(&path)
        {
            out.push(path);
        }
        i = end;
    }
    out
}

/// How far a closed plan is followed (the 11:58 EST amendment, item 2).
const FOLLOW_MAX: usize = 8;

/// `plan`, or, while it is closed, the plan its `(by <plan>)` names, else
/// the newest plan whose `consolidates:` lists it, at most
/// [`FOLLOW_MAX`] times; `None` when that ends at no plan or a closed one.
/// `plans` are newest first.
pub fn follow<'a>(plan: &'a PlanDoc, plans: &'a [PlanDoc]) -> Option<&'a PlanDoc> {
    let mut at = plan;
    for _ in 0..FOLLOW_MAX {
        if !at.is_closed() {
            return Some(at);
        }
        let by = at
            .closed_by()
            .and_then(|path| plans.iter().find(|p| p.path == path));
        at = by.or_else(|| plans.iter().find(|p| p.consolidates.contains(&at.path)))?;
    }
    (!at.is_closed()).then_some(at)
}

/// How a plan was matched to a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkedBy {
    /// The Claude Code goal set in the card's pane.
    Goal,
    /// The task Build made for the card's pane.
    Task,
    /// A plan path in the reply.
    Path,
    /// A plan's title in the reply or the summary.
    Title,
}

/// The plan a card is about (2.3 item 5, ordered by plan 20261112_03's
/// 2026-11-13 11:58 EST amendment): the plan the pane's Claude Code
/// `goal` names, else the armed task of the card's pane, else a plan path
/// in the reply, else a plan's title in the reply or summary. A closed
/// plan any of them finds is followed on ([`follow`]); one that leads to
/// no open plan gives nothing, and the next is tried. `plans` are newest
/// first.
pub fn match_plan<'a>(
    event: &CardSummaryEvent,
    goal: Option<&str>,
    tasks: &[Task],
    plans: &'a [PlanDoc],
) -> Option<(&'a PlanDoc, LinkedBy)> {
    let open = |path: &str| {
        plans
            .iter()
            .find(|p| p.path == path)
            .and_then(|p| follow(p, plans))
    };
    if let Some(plan) = goal.and_then(|g| goal_paths(g).iter().find_map(|p| open(p))) {
        return Some((plan, LinkedBy::Goal));
    }
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
    if let Some(plan) = own.iter().find_map(|t| open(&t.path)) {
        return Some((plan, LinkedBy::Task));
    }
    if let Some(plan) = named_paths(&event.reply).iter().find_map(|p| open(p)) {
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
        .filter(|p| p.title.chars().count() >= 8 && said.contains(&p.title.to_lowercase()))
        .find_map(|p| follow(p, plans))
        .map(|p| (p, LinkedBy::Title))
}

/// The words before `step N` that say the agent is on it (the 11:58 EST
/// amendment, item 3), lowercased.
const STEP_BEFORE: [&str; 10] = [
    "starting",
    "start",
    "starting on",
    "now",
    "on to",
    "onto",
    "moving to",
    "moving on to",
    "next is",
    "working on",
];

/// The words after `step N` that say it is done, lowercased.
const STEP_AFTER: [&str; 10] = [
    "is in",
    "in",
    "done",
    "is done",
    "complete",
    "is complete",
    "completed",
    "landed",
    "finished",
    "committed",
];

/// The highest step `reply` says the agent is on or has done, in the
/// phrases of the 11:58 EST amendment, item 3: at least 1 and, when
/// `total` is not 0, at most `total`.
pub fn step_named(reply: &str, total: usize) -> Option<u32> {
    let lower = reply.to_lowercase();
    let mut best: Option<u32> = None;
    let mut from = 0;
    while let Some(at) = lower[from..].find("step ").map(|i| from + i) {
        from = at + "step ".len();
        let rest = &lower[from..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let Ok(n) = digits.parse::<u32>() else {
            continue;
        };
        // A word, not the end of another (`substep 2`).
        if lower[..at]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric)
        {
            continue;
        }
        let before: Vec<&str> = lower[..at].split_whitespace().rev().take(3).collect();
        let before_ok = STEP_BEFORE.iter().any(|phrase| {
            let words: Vec<&str> = phrase.split(' ').rev().collect();
            words.len() <= before.len()
                && words
                    .iter()
                    .zip(&before)
                    .all(|(w, b)| b.trim_matches(|c: char| !c.is_alphanumeric()) == *w)
        });
        let after: Vec<String> = rest[digits.len()..]
            .split_whitespace()
            .take(2)
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
            .collect();
        let after_ok = !rest[digits.len()..].starts_with(|c: char| c.is_alphanumeric())
            && STEP_AFTER.iter().any(|phrase| {
                let words: Vec<&str> = phrase.split(' ').collect();
                words.len() <= after.len() && words.iter().zip(&after).all(|(w, a)| a == w)
            });
        if (before_ok || after_ok) && n >= 1 && (total == 0 || n as usize <= total) {
            best = Some(best.map_or(n, |b| b.max(n)));
        }
    }
    best
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

    fn doc(path: &str, title: &str, status: &str, extra: &str) -> PlanDoc {
        let text = format!("---\ntitle: {title}\nstatus: {status}\n{extra}---\n");
        super::super::plans::parse(path, &text, text.as_bytes())
    }

    const WIDGET: &str = "docs/design/20261101_01_WIDGET_PLAN.md";
    const GADGET: &str = "docs/design/20261102_01_GADGET_PLAN.md";

    #[test]
    fn plans_a_goal_names() {
        assert_eq!(
            goal_paths("build the plan 20261101_01_WIDGET_PLAN"),
            [WIDGET]
        );
        assert_eq!(
            goal_paths(
                "finish docs/design/20261102_01_GADGET_PLAN.md, then 20261101_01_WIDGET_PLAN.md and 20261102_01_GADGET_PLAN again"
            ),
            [GADGET, WIDGET]
        );
        assert!(goal_paths("the spec 20261101_01_WIDGET_SPEC").is_empty());
        assert!(
            goal_paths("x120261101_01_WIDGET_PLAN").is_empty(),
            "inside a word"
        );
        assert!(goal_paths("make the tests pass").is_empty());
    }

    #[test]
    fn a_closed_plan_is_followed_on() {
        let plans = vec![
            doc("docs/design/20261104_01_LAST_PLAN.md", "Last", "Draft", ""),
            doc(
                "docs/design/20261103_01_GATHER_PLAN.md",
                "Gather",
                "In implementation",
                &format!("consolidates:\n  - {WIDGET}\n"),
            ),
            doc(
                GADGET,
                "Gadget",
                "Superseded (by [last](docs/design/20261104_01_LAST_PLAN.md))",
                "",
            ),
            doc(WIDGET, "Widget", "Consolidated", ""),
            doc(
                "docs/design/20261031_01_DONE_PLAN.md",
                "Done",
                "Implemented",
                "",
            ),
            doc(
                "docs/design/20261030_01_LOOP_PLAN.md",
                "Loop",
                "Retired (by 20261030_01_LOOP_PLAN)",
                "",
            ),
        ];
        let at = |i: usize| follow(&plans[i], &plans).map(|p| p.path.as_str());
        assert_eq!(at(0), Some("docs/design/20261104_01_LAST_PLAN.md"));
        assert_eq!(at(2), Some("docs/design/20261104_01_LAST_PLAN.md"), "by");
        assert_eq!(
            at(3),
            Some("docs/design/20261103_01_GATHER_PLAN.md"),
            "consolidates"
        );
        assert_eq!(at(4), None, "Implemented, nothing after it");
        assert_eq!(at(5), None, "a loop ends");
    }

    #[test]
    fn steps_the_agent_names() {
        for (reply, step) in [
            ("Goal set. Starting step 1.", Some(1)),
            ("Now step 2: the reader.", Some(2)),
            ("Step 3 is in. Moving on to step 4.", Some(4)),
            ("step 2 done, step 1 done", Some(2)),
            ("Step 5 complete.", Some(5)),
            ("Step 9 done.", None),
            ("See step 2 of the other plan.", None),
            ("Substep 3 done.", None),
            ("Step 0 done.", None),
            ("Working on step 3 now", Some(3)),
        ] {
            assert_eq!(step_named(reply, 5), step, "{reply}");
        }
        assert_eq!(step_named("Step 9 done.", 0), Some(9), "no steps known");
    }
}
