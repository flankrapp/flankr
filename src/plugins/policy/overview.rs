//! The Active tasks view's data (docs/design/20261112_03_ACTIVE_PLANS_HOME_VIEW_PLAN.md,
//! 2.2, with its amendments): which plan each pane is on, and why; the
//! `overview` call and its notification, at most one per [`COALESCE`].
//!
//! A link only places a pane in the view; the policy acts on its own
//! match and a task's plan only (the plan policy agent plan, 2.3).

use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::{Value, json};

use super::plans::PlanDoc;
use super::store::Task;

/// At most one `overview` notification per this long; the latest wins.
pub const COALESCE: Duration = Duration::from_millis(500);
/// The prompt that matches a turn to a plan.
pub const PLAN_MATCH: &str = "plan_match";

/// An agent pane, as the daemon lists it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentPane {
    pub pane_id: String,
    /// Lowercased; `shell` for none.
    pub agent: String,
    pub cwd: Option<String>,
    /// Its herdr worktree's path, branch and label, when it is in one.
    pub worktree: Option<(String, Option<String>, Option<String>)>,
}

/// What the plugin knows of a plan, for rows it builds without reading.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlanSeen {
    pub root: String,
    pub title: String,
    pub status: String,
    pub steps_total: usize,
}

impl PlanSeen {
    pub fn of(root: &str, plan: &PlanDoc) -> PlanSeen {
        PlanSeen {
            root: root.to_string(),
            title: plan.title.clone(),
            status: plan.status.clone(),
            steps_total: plan.steps.len(),
        }
    }
}

/// One pane's link to a plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub path: String,
    /// `pinned`, `reply_path`, `summary_title`, `worktree_branch` or
    /// `gemini`: an open set.
    pub linked_by: &'static str,
    pub worktree: Option<String>,
}

/// The plan a `plan/<YYYYMMDD_NN>` branch was made for, among `plans`.
pub fn plan_of_branch<'a>(branch: &str, plans: &'a [PlanDoc]) -> Option<&'a PlanDoc> {
    let id = branch
        .strip_prefix("refs/heads/")
        .unwrap_or(branch)
        .strip_prefix("plan/")?;
    if id.len() != 11 {
        return None;
    }
    plans.iter().find(|p| p.base.starts_with(id))
}

/// The `plan_match` prompt for one turn and the active plans, or why
/// not.
pub fn match_prompt(
    shown: &str,
    reply: &str,
    summary: &str,
    plans: &[&PlanDoc],
) -> Result<(String, String), String> {
    let (prompts, _) = crate::prompts::Prompts::shipped();
    let prompt = prompts
        .get(PLAN_MATCH)
        .ok_or_else(|| format!("the prompt `{PLAN_MATCH}` did not load"))?;
    let list: Vec<String> = plans
        .iter()
        .map(|p| format!("- {}: {}", p.path, p.title))
        .collect();
    let values: BTreeMap<&str, String> = [
        ("plans", list.join("\n")),
        ("agent", shown.to_string()),
        ("reply", crate::sessions::cut(reply, 6000)),
        ("summary", summary.to_string()),
    ]
    .into_iter()
    .collect();
    let system = prompt.fill(&prompt.system, &values)?;
    let user = prompt.fill(prompt.user.as_deref().ok_or("no `user`")?, &values)?;
    Ok((system, user))
}

/// The path `plan_match` answered, when it is one of `plans`.
pub fn read_match(answer: &str, plans: &[&PlanDoc]) -> Option<String> {
    let start = answer.find('{')?;
    let end = answer.rfind('}')?;
    let value: Value = serde_json::from_str(answer.get(start..=end)?).ok()?;
    let path = value.get("plan")?.as_str()?;
    plans
        .iter()
        .find(|p| p.path == path)
        .map(|p| p.path.clone())
}

/// What `overview` answers and notifies (2.2 item 1, with the 23:23 EST
/// amendment's tasks).
pub struct Inputs<'a> {
    pub paused: bool,
    pub panes: &'a [AgentPane],
    pub links: &'a BTreeMap<String, Link>,
    pub decisions: &'a BTreeMap<String, Value>,
    pub plans: &'a BTreeMap<String, PlanSeen>,
    pub steps: &'a BTreeMap<String, u32>,
    pub tasks: &'a [Task],
}

pub fn build(i: &Inputs) -> Value {
    let mut groups: BTreeMap<&str, Vec<Value>> = BTreeMap::new();
    let mut unlinked = Vec::new();
    let known = |pane: &str| i.panes.is_empty() || i.panes.iter().any(|p| p.pane_id == pane);
    for (pane, link) in i.links {
        if !known(pane) {
            continue;
        }
        let listed = i.panes.iter().find(|p| &p.pane_id == pane);
        let worktree = listed
            .and_then(|p| p.worktree.clone())
            .map(|(path, branch, label)| json!({"path": path, "branch": branch, "label": label}))
            .or_else(|| link.worktree.as_ref().map(|p| json!({"path": p})));
        let mut agent = json!({ "pane_id": pane, "linked_by": link.linked_by });
        if let Some(w) = worktree {
            agent["worktree"] = w;
        }
        if let Some(d) = i.decisions.get(pane) {
            agent["last_decision"] = d.clone();
        }
        groups.entry(link.path.as_str()).or_default().push(agent);
    }
    // A task's plan shows with no pane linked yet.
    for t in i.tasks {
        groups.entry(t.path.as_str()).or_default();
    }
    for p in i.panes {
        if p.agent != crate::config::SHELL_AGENT && !i.links.contains_key(&p.pane_id) {
            unlinked.push(json!(p.pane_id));
        }
    }
    let mut plans: Vec<Value> = groups
        .into_iter()
        .rev()
        .map(|(path, agents)| {
            let seen = i.plans.get(path);
            let task = i.tasks.iter().find(|t| t.path == path);
            let mut row = json!({
                "path": path,
                "title": seen.map(|s| s.title.clone()),
                "status": seen.map(|s| s.status.clone()),
                "armed": task.is_some_and(|t| t.armed),
                "approved": task.is_some_and(|t| t.armed),
                "task": task.is_some(),
                "autocontinue": task.is_some_and(|t| t.autocontinue),
                "agents": agents,
            });
            if let Some(s) = seen {
                row["steps_total"] = json!(s.steps_total);
            }
            if let Some(n) = i.steps.get(path) {
                row["current_step"] = json!(n);
            }
            row
        })
        .collect();
    // Tasks first, then the rest, each newest path first.
    plans.sort_by_key(|r| !r["task"].as_bool().unwrap_or(false));
    json!({ "plans": plans, "unlinked": unlinked, "paused": i.paused })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(path: &str, title: &str) -> PlanDoc {
        super::super::plans::parse(
            path,
            &format!("---\ntitle: {title}\nstatus: Draft\n---\n"),
            b"x",
        )
    }

    #[test]
    fn branches_and_matches() {
        let plans = vec![
            plan("docs/design/20261112_02_A_PLAN.md", "A plan"),
            plan("docs/design/20261112_03_B_PLAN.md", "B plan"),
        ];
        assert_eq!(
            plan_of_branch("refs/heads/plan/20261112_03", &plans)
                .unwrap()
                .path,
            "docs/design/20261112_03_B_PLAN.md"
        );
        assert!(plan_of_branch("plan/20261112_09", &plans).is_none());
        assert!(plan_of_branch("main", &plans).is_none());
        let refs: Vec<&PlanDoc> = plans.iter().collect();
        assert_eq!(
            read_match(
                "{\"plan\": \"docs/design/20261112_02_A_PLAN.md\", \"matched\": \"A plan\"}",
                &refs
            )
            .as_deref(),
            Some("docs/design/20261112_02_A_PLAN.md")
        );
        assert_eq!(
            read_match("{\"plan\": \"docs/design/x.md\"}", &refs),
            None,
            "not listed"
        );
        assert_eq!(read_match("{\"plan\": null}", &refs), None);
        assert_eq!(read_match("nothing", &refs), None);
        let (system, user) = match_prompt("Claude", "Did step 2.", "Step two.", &refs).unwrap();
        assert!(system.contains("\"plan\""));
        assert!(
            user.contains("- docs/design/20261112_02_A_PLAN.md: A plan"),
            "{user}"
        );
    }
}
