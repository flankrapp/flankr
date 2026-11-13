//! The judgement (plan 20261112_02, 2.4 item 3, as the 22:03 EST
//! amendment narrows it): the `plan_policy` prompt of
//! `ADDITIONAL_PROMPTS.md`, one call on `[llm.uses.policy]` through
//! [`super::Services::judge`], and its JSON answer. Anything else is
//! `ask`, rule `model`.

use std::collections::BTreeMap;

use serde_json::Value;

use super::plans::PlanDoc;
use super::rules::Verdict;
use crate::prompts::Prompts;
use crate::sessions::hub::CardSummaryEvent;

/// The prompt's name.
pub const PLAN_POLICY: &str = "plan_policy";
/// The hard rules' names, as the prompt is told them.
pub const RULES: &str = "disabled, paused, agent, no_plan, unapproved, status, autocontinue_off, \
                         changed, question, limit, quiet";

/// The system text and user message for one card, or why not.
pub fn prompt(
    event: &CardSummaryEvent,
    plan: &PlanDoc,
    question: Option<&str>,
) -> Result<(String, String), String> {
    let (prompts, _) = Prompts::shipped();
    let prompt = prompts
        .get(PLAN_POLICY)
        .ok_or_else(|| format!("the prompt `{PLAN_POLICY}` did not load"))?;
    let max = prompt.number("max_reply_chars").unwrap_or(6000.0) as usize;
    let steps: Vec<String> = plan
        .steps
        .iter()
        .map(|s| match &s.done_when {
            Some(done) => format!("{}. {} (Done when: {done})", s.n, s.title),
            None => format!("{}. {}", s.n, s.title),
        })
        .collect();
    let mut summary = event.summary.spoken.clone().unwrap_or_default();
    for point in &event.summary.points {
        summary.push_str("\n- ");
        summary.push_str(point);
    }
    let values: BTreeMap<&str, String> = [
        ("rules", RULES.to_string()),
        ("plan", plan.title.clone()),
        ("steps", steps.join("\n")),
        ("agent", event.shown.clone()),
        ("project", event.project.clone()),
        ("reply", crate::sessions::cut(&event.reply, max)),
        ("summary", summary),
        ("question", question.unwrap_or("(none)").to_string()),
    ]
    .into_iter()
    .collect();
    let system = prompt.fill(&prompt.system, &values)?;
    let user = prompt.fill(
        prompt.user.as_deref().ok_or("the prompt has no `user`")?,
        &values,
    )?;
    Ok((system, user))
}

/// The model's answer, read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Judged {
    pub asks_continue: bool,
    pub verdict: Verdict,
    pub step: Option<u32>,
    pub reason: String,
}

/// The JSON object in `answer`, code fences and words around it allowed;
/// `None` for anything else.
pub fn parse(answer: &str) -> Option<Judged> {
    let start = answer.find('{')?;
    let end = answer.rfind('}')?;
    let value: Value = serde_json::from_str(answer.get(start..=end)?).ok()?;
    let verdict = Verdict::from_name(value.get("decision")?.as_str()?)?;
    let step = match value.get("step") {
        None | Some(Value::Null) => None,
        Some(v) => Some(u32::try_from(v.as_u64()?).ok()?),
    };
    let reason = value
        .get("reason")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|r| !r.is_empty())?;
    Some(Judged {
        asks_continue: value
            .get("asks_continue")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        verdict,
        step,
        reason: crate::sessions::cut(reason, 300),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_read_and_refused() {
        let j = parse(
            "```json\n{\"asks_continue\": true, \"decision\": \"continue\", \"step\": 4, \"reason\": \"Step 3 is done.\"}\n```",
        )
        .unwrap();
        assert_eq!(
            j,
            Judged {
                asks_continue: true,
                verdict: Verdict::Continue,
                step: Some(4),
                reason: "Step 3 is done.".into()
            }
        );
        let j =
            parse("{\"decision\": \"stop\", \"step\": null, \"reason\": \"Tests fail.\"}").unwrap();
        assert!(!j.asks_continue);
        assert_eq!(j.step, None);
        for bad in [
            "",
            "continue",
            "{}",
            "{\"decision\": \"go\", \"reason\": \"x\"}",
            "{\"decision\": \"continue\"}",
            "{\"decision\": \"continue\", \"step\": -1, \"reason\": \"x\"}",
            "{\"decision\": \"continue\", \"reason\": \"x\"",
        ] {
            assert_eq!(parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_shipped_prompt_fills() {
        let plan = super::super::plans::parse(
            "docs/design/20261112_02_A_PLAN.md",
            "---\ntitle: A sample plan\nstatus: Draft\n---\n## Steps\n\n1. **One.** x\n   **Done when:** it works.\n",
            b"x",
        );
        let event = super::super::tests::card(1, "Step 1 done. Continue with step 2?", "Done.");
        let (system, user) = prompt(&event, &plan, Some("Continue with step 2?")).unwrap();
        assert!(system.contains("asks_continue"), "{system}");
        assert!(system.contains("autocontinue_off"));
        assert!(user.contains("1. One (Done when: it works.)"), "{user}");
        assert!(user.contains("Continue with step 2?"));
        assert!(user.contains("A sample plan"));
    }
}
