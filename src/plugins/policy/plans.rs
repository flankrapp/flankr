//! The plans the policy reads: front matter by the planning reader, and
//! the `## Steps` list (plan 20261112_02, 2.3 item 3 and 2.4's inputs).

use serde_json::{Value, json};

use super::super::PluginCtx;
use super::super::PluginError;
use super::super::planning::reader;
use super::store::{Task, plan_hash};

/// The statuses a task may work under (2.3 item 3).
pub const WORKING: [&str; 2] = ["Draft", "In implementation"];

/// One numbered step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub n: u32,
    pub title: String,
    /// Its **Done when:** text, if it has one.
    pub done_when: Option<String>,
}

/// A plan as the policy reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanDoc {
    /// `docs/design/<base>.md`.
    pub path: String,
    pub base: String,
    pub title: String,
    /// The front matter's `status`, as written; empty when missing.
    pub status: String,
    pub steps: Vec<Step>,
    /// [`plan_hash`] (FNV-1a 64) of the whole file.
    pub hash: String,
    /// The amendments' subjects, in order.
    pub amendments: Vec<String>,
    /// The text read (at most 64 KiB).
    pub text: String,
}

impl PlanDoc {
    /// Whether its status lets a task work (2.3 item 3).
    pub fn status_allows(&self) -> bool {
        let (base, _) = reader::split_status(&self.status);
        WORKING.contains(&base.as_str())
    }

    /// The plan's row for `plans`, `approval` and `build`: with its task,
    /// if it has one.
    pub fn row(&self, task: Option<&Task>) -> Value {
        let steps: Vec<Value> = self
            .steps
            .iter()
            .map(|s| json!({ "n": s.n, "title": s.title }))
            .collect();
        let changed = task.is_some_and(|t| t.hash != self.hash);
        json!({
            "path": self.path,
            "base": self.base,
            "title": self.title,
            "status": self.status,
            "status_allows": self.status_allows(),
            "armed": task.is_some_and(|t| t.armed),
            "approved": task.is_some_and(|t| t.armed),
            "approved_at": task.map(|t| t.at.clone()),
            "approved_by": task.map(|t| t.by.clone()),
            "changed_since_approval": changed,
            "autocontinue": task.is_some_and(|t| t.autocontinue),
            "started_at": task.and_then(|t| t.started_at.clone()),
            "pane_id": task.and_then(|t| t.pane_id.clone()),
            "steps": steps,
            "continues_used": task.map_or(0, |t| t.continues_used),
        })
    }
}

/// `path` as a plan's path, `docs/design/<plan base>.md`, or `None`.
pub fn plan_path(path: &str) -> Option<String> {
    let path = path.trim().trim_start_matches("./");
    let file = path.strip_prefix("docs/design/")?;
    let name = reader::file_name(file)?;
    (name.kind == reader::Kind::Plan).then(|| path.to_string())
}

/// Reads the plan at `path` (already a [`plan_path`]): its text through
/// `read_file`, at most 64 KiB, and, once that read is allowed, the whole
/// file's bytes for its hash.
pub async fn read(ctx: &PluginCtx, path: &str) -> Result<PlanDoc, PluginError> {
    let read = ctx.read_file(path).await?;
    let full = ctx.project()?.root.join(path);
    let bytes = tokio::task::spawn_blocking(move || std::fs::read(full))
        .await
        .ok()
        .and_then(Result::ok);
    let bytes = bytes.unwrap_or_else(|| read.text.clone().into_bytes());
    Ok(parse(path, &read.text, &bytes))
}

/// The plan at `path` from its `text`, the whole file being `bytes`.
pub fn parse(path: &str, text: &str, bytes: &[u8]) -> PlanDoc {
    let file = path.rsplit('/').next().unwrap_or(path);
    let base = file.trim_end_matches(".md").to_string();
    let lines: Vec<&str> = text.lines().collect();
    let front = reader::front_matter(&lines).and_then(Result::ok);
    let (title, status) = match &front {
        Some((f, _)) => (f.title.clone(), f.status.clone()),
        None => (None, None),
    };
    let amendments = lines
        .iter()
        .filter_map(|l| reader::amendment_heading(l).map(|(_, subject)| subject))
        .collect();
    PlanDoc {
        path: path.to_string(),
        title: title.unwrap_or_else(|| base.clone()),
        base,
        status: status.unwrap_or_default(),
        steps: steps(text),
        hash: plan_hash(bytes),
        amendments,
        text: text.to_string(),
    }
}

/// Every plan in the project's `docs/design/`, newest first.
pub async fn list(ctx: &PluginCtx) -> Result<Vec<PlanDoc>, PluginError> {
    let listing = ctx.list_files(Some("docs/design/*.md")).await?;
    let mut paths: Vec<String> = listing.paths.iter().filter_map(|p| plan_path(p)).collect();
    paths.sort();
    paths.reverse();
    let mut plans = Vec::with_capacity(paths.len());
    for path in paths {
        match read(ctx, &path).await {
            Ok(plan) => plans.push(plan),
            Err(e) => log::info!("[policy] {path}: {}", e.message),
        }
    }
    Ok(plans)
}

/// The numbered steps of the plan's `## … Steps` section: `N. ` items at
/// the line's start, their title the bold words they open with, or
/// `### Step N` headings. Fenced code is skipped.
pub fn steps(text: &str) -> Vec<Step> {
    let mut out: Vec<Step> = Vec::new();
    let mut in_steps = false;
    let mut fenced = false;
    let mut body: Vec<String> = Vec::new();
    let finish = |out: &mut Vec<Step>, body: &mut Vec<String>| {
        if let Some(step) = out.last_mut() {
            let joined = body.join(" ");
            if let Some(at) = joined.find("**Done when:**") {
                let done = joined[at + "**Done when:**".len()..].trim();
                let done = done.split_whitespace().collect::<Vec<_>>().join(" ");
                if !done.is_empty() {
                    step.done_when = Some(done);
                }
            }
        }
        body.clear();
    };
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        if let Some(h) = line.strip_prefix("## ") {
            if in_steps {
                finish(&mut out, &mut body);
            }
            in_steps = h
                .split(|c: char| !c.is_alphanumeric())
                .any(|w| w.eq_ignore_ascii_case("steps"));
            continue;
        }
        if !in_steps {
            continue;
        }
        if let Some((n, title)) = step_heading(line).or_else(|| step_item(line)) {
            finish(&mut out, &mut body);
            out.push(Step {
                n,
                title,
                done_when: None,
            });
            body.push(line.to_string());
        } else if !out.is_empty() {
            body.push(line.trim().to_string());
        }
    }
    if in_steps {
        finish(&mut out, &mut body);
    }
    out
}

/// `### Step N: Title` (or `.` or `—`).
fn step_heading(line: &str) -> Option<(u32, String)> {
    let rest = line.strip_prefix("### Step ")?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let n = digits.parse().ok()?;
    let title = rest[digits.len()..]
        .trim_start_matches([':', '.', ' ', '—', '-'])
        .trim();
    Some((n, title.to_string()))
}

/// `N. **Title.** …` at the start of a line.
fn step_item(line: &str) -> Option<(u32, String)> {
    let digits: String = line.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    let rest = line[digits.len()..].strip_prefix(". ")?;
    let n = digits.parse().ok()?;
    let title = match rest.strip_prefix("**").and_then(|r| r.split_once("**")) {
        Some((bold, _)) => bold.trim().trim_end_matches('.').to_string(),
        None => rest
            .split_once(". ")
            .map_or(rest, |(first, _)| first)
            .trim()
            .to_string(),
    };
    Some((n, title))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAN: &str = "---\ntitle: A plan\ndate: 2026-11-12\nstatus: In implementation\n---\n\n# A plan\n\n## 1. Why\n\n1. Not a step.\n\n## 3. Steps\n\n1. **Daemon: the skeleton.** Some words\n   more words.\n   **Done when:** it lists\n   the plugin.\n2. **Daemon: approvals.** Words.\n   **Done when:** tests pass.\n\n```\n3. not a step\n```\n\n3. Plain step. Words.\n\n## 4. Open questions\n\n1. A question.\n\n## Amendment — 2026-11-12 22:00 EST: one\n\nText.\n";

    #[test]
    fn steps_and_front_matter() {
        let p = parse("docs/design/20261112_02_A_PLAN.md", PLAN, PLAN.as_bytes());
        assert_eq!(p.title, "A plan");
        assert_eq!(p.base, "20261112_02_A_PLAN");
        assert_eq!(p.status, "In implementation");
        assert!(p.status_allows());
        assert_eq!(p.amendments, ["one"]);
        let titles: Vec<(u32, &str)> = p.steps.iter().map(|s| (s.n, s.title.as_str())).collect();
        assert_eq!(
            titles,
            [
                (1, "Daemon: the skeleton"),
                (2, "Daemon: approvals"),
                (3, "Plain step")
            ]
        );
        assert_eq!(
            p.steps[0].done_when.as_deref(),
            Some("it lists the plugin.")
        );
        assert_eq!(p.steps[1].done_when.as_deref(), Some("tests pass."));
        assert_eq!(p.steps[2].done_when, None);
        let headed = steps("## Steps\n\n### Step 1: First\n\nText.\n\n### Step 2 — Second\n");
        assert_eq!(headed.len(), 2);
        assert_eq!(headed[1].title, "Second");
    }

    #[test]
    fn statuses_and_paths() {
        for (status, ok) in [
            ("Draft", true),
            ("In implementation", true),
            ("Hold", false),
            ("Implemented", false),
            ("Superseded (by x)", false),
            ("Retired (by x)", false),
            ("", false),
        ] {
            let text = format!("---\ntitle: t\nstatus: {status}\n---\n");
            let p = parse("docs/design/20261112_02_A_PLAN.md", &text, text.as_bytes());
            assert_eq!(p.status_allows(), ok, "{status}");
        }
        assert_eq!(
            plan_path("docs/design/20261112_02_A_PLAN.md").as_deref(),
            Some("docs/design/20261112_02_A_PLAN.md")
        );
        assert_eq!(plan_path("docs/design/20261112_02_A_SPEC.md"), None);
        assert_eq!(plan_path("docs/design/../x/20261112_02_A_PLAN.md"), None);
        assert_eq!(plan_path("README.md"), None);
    }
}
