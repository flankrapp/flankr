//! The plan policy plugin with fakes only: a temporary project, a fake
//! lookup and fake daemon services. No herdr, no Gemini.

use std::path::Path;
use std::sync::Arc;

use serde_json::json;

use super::*;
use crate::core::principal::Principal;
use crate::plugins::ctx::tests::{FakeLookup, patterns};
use crate::plugins::ctx::{Commands, CtxParts};

pub(super) fn owner() -> Principal {
    Principal {
        id: "paired:phone".into(),
        agents: None,
        can_approve: true,
    }
}

pub(super) async fn ctx_as(root: &Path, principal: Principal, config: toml::Table) -> PluginCtx {
    PluginCtx::build(CtxParts {
        ns: NS,
        principal,
        pane: Some("w1:p1".into()),
        lookup: Arc::new(FakeLookup(vec![(
            "w1:p1".into(),
            "claude".into(),
            root.display().to_string(),
        )])),
        read: patterns(&["*.md", "docs/**/*.md"]),
        plugin_read: patterns(&READ),
        commands: Commands::default(),
        config,
        notifier: None,
    })
    .await
}

#[test]
fn settings_and_their_defaults() {
    let s = Settings::of(&toml::Table::new());
    assert_eq!(s, Settings::default());
    assert!(s.agents.is_empty(), "drives nothing by default");
    assert_eq!(s.continue_command, "continue");
    assert_eq!((s.max_continues, s.quiet_secs), (3, 15));
    let t: toml::Table = toml::from_str(
        "agents = [\"Claude\"]\ncontinue_command = \"go on\"\nmax_continues = 5\nquiet_secs = 0\nconfirm = \"agent\"",
    )
    .unwrap();
    let s = Settings::of(&t);
    assert!(s.drives("claude"));
    assert!(!s.drives("codex"));
    assert_eq!(s.continue_command, "go on");
    assert_eq!((s.max_continues, s.quiet_secs), (5, 0));
    let bad: toml::Table =
        toml::from_str("agents = \"claude\"\nmax_continues = -1\nquiet_secs = \"x\"").unwrap();
    assert_eq!(Settings::of(&bad), Settings::default());
}

#[tokio::test]
async fn state_and_pause() {
    let dir = tempfile::tempdir().unwrap();
    let p = PlanPolicy::new();
    let c = ctx_as(dir.path(), owner(), toml::Table::new()).await;
    assert_eq!(
        p.call("state", json!({}), &c).await.unwrap(),
        json!({"paused": false, "sessions": []})
    );
    assert_eq!(
        p.call("pause", json!({"paused": true}), &c).await.unwrap(),
        json!({"paused": true})
    );
    assert!(p.paused());
    assert_eq!(
        p.call("state", json!({}), &c).await.unwrap()["paused"],
        true
    );
    assert!(p.call("pause", json!({}), &c).await.is_err());
    // A device that may not approve cannot pause.
    let other = ctx_as(dir.path(), Principal::local("t"), toml::Table::new()).await;
    let e = p
        .call("pause", json!({"paused": false}), &other)
        .await
        .unwrap_err();
    assert_eq!(e.code, wire::NOT_PERMITTED);
    assert!(p.paused());
    assert_eq!(
        p.call("nope", json!({}), &c).await.unwrap_err().code,
        wire::METHOD_NOT_FOUND
    );
}

/// The daemon's services, faked: a state folder and the notifications
/// sent.
#[derive(Default)]
pub(super) struct Fake {
    pub(super) state: Option<std::path::PathBuf>,
    pub(super) notes: std::sync::Mutex<Vec<(String, serde_json::Value)>>,
    /// The open choice question, if any.
    pub(super) choice: std::sync::Mutex<Option<String>>,
    /// The reply's pending question, if any.
    pub(super) question: std::sync::Mutex<Option<String>>,
    /// The scripted model: each call takes the first answer; none left is
    /// an error.
    pub(super) answers: std::sync::Mutex<std::collections::VecDeque<String>>,
    /// The user messages it was sent.
    pub(super) asked: std::sync::Mutex<Vec<String>>,
    /// A real broker over a recording sink, and the `[[agents]]`.
    pub(super) broker: Option<Arc<crate::core::confirm::Broker>>,
    pub(super) agents: Vec<crate::config::AgentConfig>,
    /// The worktrees asked for: (pane, branch).
    pub(super) worktrees: std::sync::Mutex<Vec<(String, String)>>,
    /// The panes herdr shows.
    pub(super) panes: Vec<overview::AgentPane>,
}

/// What an approved confirmation sent: (pane, command).
#[derive(Default)]
pub(super) struct Sink(pub(super) std::sync::Mutex<Vec<(String, String)>>);

#[async_trait::async_trait]
impl crate::core::confirm::CommandSink for Sink {
    async fn send(&self, pane_id: &str, command: &str) -> Result<(), String> {
        self.0
            .lock()
            .unwrap()
            .push((pane_id.to_string(), command.to_string()));
        Ok(())
    }
}

impl Fake {
    pub(super) fn answer(&self, text: &str) {
        self.answers.lock().unwrap().push_back(text.to_string());
    }
}

impl Fake {
    pub(super) fn notes(&self, method: &str) -> Vec<serde_json::Value> {
        self.notes
            .lock()
            .unwrap()
            .iter()
            .filter(|(m, _)| m == method)
            .map(|(_, p)| p.clone())
            .collect()
    }
}

#[async_trait::async_trait]
impl Services for Fake {
    fn state_dir(&self) -> Option<std::path::PathBuf> {
        self.state.clone()
    }
    fn notify_all(&self, method: &str, params: serde_json::Value) {
        self.notes
            .lock()
            .unwrap()
            .push((method.to_string(), params));
    }
    fn pending_question(&self, _pane_id: &str) -> Option<String> {
        self.question.lock().unwrap().clone()
    }
    async fn open_choice(&self, _pane_id: &str) -> Option<String> {
        self.choice.lock().unwrap().clone()
    }
    fn agents(&self) -> Vec<crate::config::AgentConfig> {
        self.agents.clone()
    }
    fn request_confirmation(
        &self,
        principal: &Principal,
        pane_id: &str,
        agent: &str,
        command: &str,
    ) -> Result<(u64, tokio::sync::oneshot::Receiver<&'static str>), String> {
        use crate::core::confirm::{ConfirmEvent, Lifetime, Outcome};
        let broker = self.broker.as_ref().ok_or("no broker")?;
        let mut events = broker.subscribe();
        let id = broker
            .request(principal, pane_id, agent, command, None, Lifetime::Standing)
            .map_err(|e| e.to_string())?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            while let Ok(event) = events.recv().await {
                if let ConfirmEvent::Resolved { view, outcome, .. } = event
                    && view.confirm_id == id
                    && outcome != Outcome::Approved
                {
                    let _ = tx.send(outcome.as_str());
                    return;
                }
            }
        });
        Ok((id, rx))
    }
    fn cancel_confirmation(&self, principal: &Principal, confirm_id: u64) {
        if let Some(broker) = &self.broker {
            let _ = broker.cancel(principal, confirm_id);
        }
    }
    async fn agent_panes(&self) -> Vec<overview::AgentPane> {
        self.panes.clone()
    }
    async fn worktree_create(&self, pane_id: &str, branch: &str) -> Result<String, String> {
        self.worktrees
            .lock()
            .unwrap()
            .push((pane_id.to_string(), branch.to_string()));
        Ok(format!("/wt/{branch}"))
    }
    async fn judge(&self, _system: &str, user: &str) -> Result<String, String> {
        self.asked.lock().unwrap().push(user.to_string());
        self.answers
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| "no answer scripted".to_string())
    }
}

pub(super) const PLAN_PATH: &str = "docs/design/20261112_02_A_PLAN.md";

pub(super) fn plan_text(status: &str) -> String {
    format!(
        "---\ntitle: A sample plan\ndate: 2026-11-12\nstatus: {status}\n---\n\n## 3. Steps\n\n1. **First.** Words.\n   **Done when:** one works.\n2. **Second.** Words.\n   **Done when:** two works.\n3. **Third.** Words.\n   **Done when:** three works.\n4. **Fourth.** Words.\n   **Done when:** four works.\n5. **Fifth.** Words.\n   **Done when:** five works.\n"
    )
}

/// A project with one plan, a state folder, and the plugin over fakes.
pub(super) struct Env {
    pub(super) _dir: tempfile::TempDir,
    pub(super) root: std::path::PathBuf,
    pub(super) fake: Arc<Fake>,
    pub(super) policy: PlanPolicy,
}

pub(super) fn env_with(fake: impl FnOnce(std::path::PathBuf) -> Fake) -> Env {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("proj");
    std::fs::create_dir_all(root.join("docs/design")).unwrap();
    std::fs::write(root.join(PLAN_PATH), plan_text("Draft")).unwrap();
    std::fs::write(
        root.join("docs/design/20261111_01_OLD_PLAN.md"),
        "---\ntitle: Old\nstatus: Implemented\n---\n",
    )
    .unwrap();
    std::fs::write(root.join("docs/design/20261111_02_A_SPEC.md"), "---\n---\n").unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let fake = Arc::new(fake(dir.path().join("state")));
    let policy = PlanPolicy::with_services(fake.clone());
    Env {
        _dir: dir,
        root,
        fake,
        policy,
    }
}

pub(super) fn env() -> Env {
    env_with(|state| Fake {
        state: Some(state),
        ..Fake::default()
    })
}

#[tokio::test]
async fn build_arms_a_plan_and_the_task_calls_change_it() {
    let e = env();
    let c = ctx_as(&e.root, owner(), toml::Table::new()).await;
    let plans = e.policy.call("plans", json!({}), &c).await.unwrap();
    let rows = plans["plans"].as_array().unwrap();
    let paths: Vec<&str> = rows.iter().map(|r| r["path"].as_str().unwrap()).collect();
    assert_eq!(
        paths,
        [PLAN_PATH, "docs/design/20261111_01_OLD_PLAN.md"],
        "plans only, newest first"
    );
    assert_eq!(rows[0]["armed"], false);
    assert_eq!(rows[0]["steps"].as_array().unwrap().len(), 5);
    assert_eq!(rows[0]["steps"][0], json!({"n": 1, "title": "First"}));
    assert_eq!(rows[1]["status_allows"], false);

    // Autocontinue needs Build first.
    let err = e
        .policy
        .call("task_enable", json!({"path": PLAN_PATH}), &c)
        .await
        .unwrap_err();
    assert_eq!(err.code, NO_TASK);

    let row = e
        .policy
        .call("build", json!({"path": PLAN_PATH, "pane_id": "w1:p1"}), &c)
        .await
        .unwrap();
    assert_eq!(row["armed"], true);
    assert_eq!(row["autocontinue"], false, "Build alone sends nothing");
    assert_eq!(row["pane_id"], "w1:p1");
    assert_eq!(row["approved_by"], "paired:phone");
    assert_eq!(row["changed_since_approval"], false);
    assert_eq!(e.fake.notes("approval"), std::slice::from_ref(&row));

    let row = e
        .policy
        .call("task_enable", json!({"path": PLAN_PATH}), &c)
        .await
        .unwrap();
    assert_eq!(row["autocontinue"], true);
    assert!(row["started_at"].is_string());

    // Any edit to the plan shows as a change since it was armed.
    std::fs::write(
        e.root.join(PLAN_PATH),
        format!(
            "{}\n## Amendment — 2026-11-12 23:00 EST: more\n",
            plan_text("Draft")
        ),
    )
    .unwrap();
    let plans = e.policy.call("plans", json!({}), &c).await.unwrap();
    assert_eq!(plans["plans"][0]["changed_since_approval"], true);
    assert_eq!(plans["plans"][0]["armed"], true);
    // Switching autocontinue on again takes the plan as it now is.
    let row = e
        .policy
        .call("task_enable", json!({"path": PLAN_PATH}), &c)
        .await
        .unwrap();
    assert_eq!(row["changed_since_approval"], false);

    let row = e
        .policy
        .call("task_disable", json!({"path": PLAN_PATH}), &c)
        .await
        .unwrap();
    assert_eq!(
        (row["armed"].clone(), row["autocontinue"].clone()),
        (json!(true), json!(false))
    );
    let row = e
        .policy
        .call("task_remove", json!({"path": PLAN_PATH}), &c)
        .await
        .unwrap();
    assert_eq!(row["armed"], false, "removing the task disarms the plan");
    assert_eq!(e.fake.notes("approval").len(), 5);

    // A status that does not allow work: armed, but no autocontinue.
    e.policy
        .call(
            "build",
            json!({"path": "docs/design/20261111_01_OLD_PLAN.md"}),
            &c,
        )
        .await
        .unwrap();
    let err = e
        .policy
        .call(
            "task_enable",
            json!({"path": "docs/design/20261111_01_OLD_PLAN.md"}),
            &c,
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, NO_TASK);

    for bad in [
        json!({}),
        json!({"path": "README.md"}),
        json!({"path": "docs/design/20261111_02_A_SPEC.md"}),
    ] {
        let err = e.policy.call("build", bad.clone(), &c).await.unwrap_err();
        assert_eq!(
            err.code,
            libflankr_proto::envelope::codes::INVALID_PARAMS,
            "{bad}"
        );
    }
    let err = e
        .policy
        .call(
            "build",
            json!({"path": "docs/design/20261101_01_NONE_PLAN.md"}),
            &c,
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, NO_PLAN);
}

#[tokio::test]
async fn only_a_principal_that_may_approve_changes_tasks() {
    let e = env();
    let c = ctx_as(&e.root, Principal::local("t"), toml::Table::new()).await;
    for method in ["build", "task_enable", "task_disable", "task_remove"] {
        let err = e
            .policy
            .call(method, json!({"path": PLAN_PATH}), &c)
            .await
            .unwrap_err();
        assert_eq!(err.code, wire::NOT_PERMITTED, "{method}");
    }
    // It may still read them.
    let plans = e.policy.call("plans", json!({}), &c).await.unwrap();
    assert_eq!(plans["plans"][0]["armed"], false);
    assert!(e.fake.notes("approval").is_empty());
    // Without the daemon's state folder nothing is kept.
    let bare = PlanPolicy::new();
    let c = ctx_as(&e.root, owner(), toml::Table::new()).await;
    assert_eq!(
        bare.call("build", json!({"path": PLAN_PATH}), &c)
            .await
            .unwrap_err()
            .code,
        NO_STORE
    );
}

/// The policy's settings for the tests: Claude driven, no quiet time.
pub(super) fn on() -> toml::Table {
    toml::from_str("agents = [\"claude\"]\nquiet_secs = 0\n").unwrap()
}

/// A Finished card's summary from `w1:p1`.
pub(super) fn card(serial: u64, reply: &str, spoken: &str) -> CardSummaryEvent {
    CardSummaryEvent {
        pane: crate::sessions::hub::PaneOf {
            pane_id: "w1:p1".into(),
            agent: "claude".into(),
            cwd: None,
        },
        shown: "Claude".into(),
        project: "proj".into(),
        key: (serial, 7),
        summary: crate::sessions::hub::CardSummary {
            points: vec![],
            spoken: Some(spoken.into()),
        },
        reply: reply.into(),
    }
}

impl Env {
    /// The decisions sent so far: (decision, rule).
    pub(super) fn decisions(&self) -> Vec<(String, String)> {
        self.fake
            .notes("decision")
            .iter()
            .map(|d| {
                (
                    d["decision"].as_str().unwrap().to_string(),
                    d["rule"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    }

    pub(super) fn pair(decision: &str, rule: &str) -> (String, String) {
        (decision.to_string(), rule.to_string())
    }
}

#[tokio::test]
async fn the_hard_rules_in_order() {
    let e = env();
    let c = ctx_as(&e.root, owner(), on()).await;
    let pair = Env::pair;
    // No plan named, no task: nothing to work on.
    e.policy
        .on_card_summary(&card(1, "Done.", "Done."), &c)
        .await;
    assert_eq!(e.decisions(), [pair("stop", "no_plan")]);
    // The plan named, not built: unapproved.
    let named = format!("Step 1 of {PLAN_PATH} is done. Continue with step 2?");
    e.policy
        .on_card_summary(&card(2, &named, "Step one is done."), &c)
        .await;
    assert_eq!(e.decisions()[1], pair("stop", "unapproved"));
    // Named by its title instead.
    e.policy
        .on_card_summary(&card(3, "Working on A sample plan now.", "x"), &c)
        .await;
    assert_eq!(e.decisions()[2], pair("stop", "unapproved"));
    assert_eq!(e.fake.notes("decision")[2]["plan"], PLAN_PATH);
    // Built: the pane's task is its plan, but autocontinue is off.
    e.policy
        .call("build", json!({"path": PLAN_PATH, "pane_id": "w1:p1"}), &c)
        .await
        .unwrap();
    e.policy
        .on_card_summary(&card(4, "Done.", "Done."), &c)
        .await;
    assert_eq!(e.decisions()[3], pair("ask", "autocontinue_off"));
    e.policy
        .call("task_enable", json!({"path": PLAN_PATH}), &c)
        .await
        .unwrap();
    // A choice question blocks the task.
    *e.fake.choice.lock().unwrap() = Some("Which way?".into());
    e.policy
        .on_card_summary(&card(5, "Done.", "Done."), &c)
        .await;
    assert_eq!(e.decisions()[4], pair("ask", "question"));
    let state = e.policy.call("state", json!({}), &c).await.unwrap();
    assert_eq!(state["sessions"][0]["blocked"], true);
    assert_eq!(state["sessions"][0]["question"], "Which way?");
    let plans = e.policy.call("plans", json!({}), &c).await.unwrap();
    assert_eq!(plans["plans"][0]["blocked"], true);
    // Answered: no longer blocked.
    *e.fake.choice.lock().unwrap() = None;
    e.policy
        .on_card_summary(&card(6, "Done.", "Done."), &c)
        .await;
    let state = e.policy.call("state", json!({}), &c).await.unwrap();
    assert_eq!(state["sessions"][0]["blocked"], false);
    assert_eq!(e.decisions()[5], pair("ask", "model"), "past the rules");
    // A changed plan pauses autocontinue, saying what changed.
    std::fs::write(
        e.root.join(PLAN_PATH),
        format!(
            "{}\n## Amendment — 2026-11-12 23:00 EST: a new step\n",
            plan_text("Draft")
        ),
    )
    .unwrap();
    e.policy
        .on_card_summary(&card(7, "Done.", "Done."), &c)
        .await;
    assert_eq!(e.decisions()[6], pair("ask", "changed"));
    let reason = e.fake.notes("decision")[6]["reason"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(reason.contains("a new step"), "{reason}");
    let rows = e.fake.notes("approval");
    assert_eq!(rows.last().unwrap()["autocontinue"], false);
    // A status that does not allow work stops it.
    std::fs::write(e.root.join(PLAN_PATH), plan_text("Hold")).unwrap();
    e.policy
        .call("build", json!({"path": PLAN_PATH, "pane_id": "w1:p1"}), &c)
        .await
        .unwrap();
    e.policy
        .on_card_summary(&card(8, "Done.", "Done."), &c)
        .await;
    assert_eq!(e.decisions()[7], pair("stop", "status"));
}

#[tokio::test]
async fn nothing_while_paused_for_other_agents_or_for_an_older_card() {
    let e = env();
    let c = ctx_as(&e.root, owner(), on()).await;
    e.policy
        .call("pause", json!({"paused": true}), &c)
        .await
        .unwrap();
    e.policy
        .on_card_summary(&card(1, "Done.", "Done."), &c)
        .await;
    assert!(e.decisions().is_empty(), "paused");
    e.policy
        .call("pause", json!({"paused": false}), &c)
        .await
        .unwrap();
    // Not one of `agents`, as by default: no decision at all.
    let off = ctx_as(&e.root, owner(), toml::Table::new()).await;
    e.policy
        .on_card_summary(&card(2, "Done.", "Done."), &off)
        .await;
    assert!(e.decisions().is_empty(), "drives no agent");
    // Only the newest card of a pane is judged after the quiet time.
    let quiet: toml::Table = toml::from_str("agents = [\"claude\"]\nquiet_secs = 1\n").unwrap();
    let q = ctx_as(&e.root, owner(), quiet).await;
    let older = card(3, "Done.", "Done.");
    let newer = card(4, "Done.", "Done.");
    tokio::join!(e.policy.on_card_summary(&older, &q), async {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        e.policy.on_card_summary(&newer, &q).await;
    });
    let sent = e.fake.notes("decision");
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0]["summary_serial"], 4);
}

/// A built plan with autocontinue on, in `w1:p1`.
pub(super) async fn armed(e: &Env) -> PluginCtx {
    let c = ctx_as(&e.root, owner(), on()).await;
    e.policy
        .call("build", json!({"path": PLAN_PATH, "pane_id": "w1:p1"}), &c)
        .await
        .unwrap();
    e.policy
        .call("task_enable", json!({"path": PLAN_PATH}), &c)
        .await
        .unwrap();
    c
}

#[tokio::test]
async fn the_judgement_with_a_scripted_model() {
    let e = gated(Arc::new(Sink::default()), &["continue"]);
    let c = armed(&e).await;
    *e.fake.question.lock().unwrap() = Some("Continue with step 4?".into());
    e.fake.answer(
        "{\"asks_continue\": true, \"decision\": \"continue\", \"step\": 4, \"reason\": \"Step 3 is done and its tests pass.\"}",
    );
    e.policy
        .on_card_summary(
            &card(
                1,
                "Step 3 done, tests pass. Continue with step 4?",
                "Step three is done.",
            ),
            &c,
        )
        .await;
    let d = e.fake.notes("decision").pop().unwrap();
    assert_eq!(
        (d["decision"].clone(), d["rule"].clone(), d["step"].clone()),
        (json!("continue"), json!("judgement"), json!(4))
    );
    let asked = e.fake.asked.lock().unwrap()[0].clone();
    assert!(asked.contains("Continue with step 4?"), "{asked}");
    assert!(
        asked.contains("4. Fourth (Done when: four works.)"),
        "{asked}"
    );

    e.fake.answer(
        "{\"asks_continue\": true, \"decision\": \"stop\", \"step\": 4, \"reason\": \"Tests are failing.\"}",
    );
    e.policy
        .on_card_summary(&card(2, "Tests failing.", "Tests fail."), &c)
        .await;
    assert_eq!(e.decisions()[1], Env::pair("stop", "judgement"));

    e.fake.answer("I think it should go on.");
    e.policy
        .on_card_summary(&card(3, "Done.", "Done."), &c)
        .await;
    assert_eq!(e.decisions()[2], Env::pair("ask", "model"));
    // No answer at all, as with no model: ask.
    e.policy
        .on_card_summary(&card(4, "Done.", "Done."), &c)
        .await;
    assert_eq!(e.decisions()[3], Env::pair("ask", "model"));
    // "continue" without the agent asking to: only ask.
    e.fake.answer(
        "{\"asks_continue\": false, \"decision\": \"continue\", \"step\": 4, \"reason\": \"Done.\"}",
    );
    e.policy
        .on_card_summary(&card(5, "Done.", "Done."), &c)
        .await;
    assert_eq!(e.decisions()[4], Env::pair("ask", "no_question"));
    // Past the last step: complete.
    e.fake.answer(
        "{\"asks_continue\": true, \"decision\": \"continue\", \"step\": 6, \"reason\": \"Done.\"}",
    );
    e.policy
        .on_card_summary(&card(6, "Done.", "Done."), &c)
        .await;
    assert_eq!(e.decisions()[5], Env::pair("stop", "complete"));
}

const CONTINUE_4: &str = "{\"asks_continue\": true, \"decision\": \"continue\", \"step\": 4, \"reason\": \"Step 3 is done.\"}";

/// An env whose fake has a real broker over `sink`, and Claude allowed
/// `continue` (or not).
fn gated(sink: Arc<Sink>, allowed: &[&str]) -> Env {
    let agents = vec![crate::config::AgentConfig {
        name: "claude".into(),
        allowed_commands: allowed.iter().map(|c| c.to_string()).collect(),
        allow_arbitrary_commands: true,
        ..toml::from_str("name = \"claude\"").unwrap()
    }];
    env_with(move |state| Fake {
        state: Some(state),
        broker: Some(crate::core::confirm::Broker::new(
            sink,
            std::time::Duration::from_secs(60),
        )),
        agents,
        ..Fake::default()
    })
}

async fn settle() {
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
}

#[tokio::test]
async fn a_continue_is_only_ever_a_confirmation() {
    use crate::core::confirm::ConfirmEvent;
    let sink = Arc::new(Sink::default());
    let e = gated(Arc::clone(&sink), &["continue"]);
    let broker = e.fake.broker.clone().unwrap();
    let mut events = broker.subscribe();
    let c = ctx_as(&e.root, owner(), on()).await;
    // Not built: the model is never asked and the broker never reached.
    e.fake.answer(CONTINUE_4);
    let named = format!("{PLAN_PATH}: step 3 done. Continue with step 4?");
    e.policy
        .on_card_summary(&card(1, &named, "Done."), &c)
        .await;
    assert_eq!(e.decisions()[0], Env::pair("stop", "unapproved"));
    assert!(e.fake.asked.lock().unwrap().is_empty(), "no model call");
    assert!(events.try_recv().is_err(), "nothing asked");
    e.fake.answers.lock().unwrap().clear();

    let c = armed(&e).await;
    e.fake.answer(CONTINUE_4);
    e.policy
        .on_card_summary(&card(2, &named, "Done."), &c)
        .await;
    let d = e.fake.notes("decision").pop().unwrap();
    assert_eq!(d["decision"], "continue");
    let id = d["confirm_id"].as_u64().expect("a confirmation");
    let Ok(ConfirmEvent::Requested { view, .. }) = events.try_recv() else {
        panic!("one confirm.requested");
    };
    assert_eq!(view.confirm_id, id);
    assert_eq!(view.command, "continue");
    assert_eq!(view.requested_by, PRINCIPAL_ID);
    assert!(events.try_recv().is_err(), "exactly one");
    settle().await;
    assert!(
        sink.0.lock().unwrap().is_empty(),
        "nothing runs without confirm.reply"
    );
    // The plugin's own principal could never approve it.
    assert!(!PlanPolicy::principal(&Settings::of(&on())).can_approve);
    broker.reply(&owner(), id, true, None).await.unwrap();
    assert_eq!(
        *sink.0.lock().unwrap(),
        [("w1:p1".to_string(), "continue".to_string())]
    );
    settle().await;
    let plans = e.policy.call("plans", json!({}), &c).await.unwrap();
    assert_eq!(plans["plans"][0]["continues_used"], 1);
}

#[tokio::test]
async fn a_denied_continue_stops_and_the_fourth_asks() {
    let sink = Arc::new(Sink::default());
    // Arbitrary commands are on, but `continue` must be on the allow list.
    let e = gated(Arc::clone(&sink), &["/compact"]);
    let c = armed(&e).await;
    e.fake.answer(CONTINUE_4);
    e.policy
        .on_card_summary(&card(1, "Continue with step 4?", "Done."), &c)
        .await;
    assert_eq!(e.decisions()[0], Env::pair("stop", "denied"));

    let sink = Arc::new(Sink::default());
    let e = gated(Arc::clone(&sink), &["continue"]);
    let broker = e.fake.broker.clone().unwrap();
    let c = armed(&e).await;
    for serial in 1..=3 {
        e.fake.answer(CONTINUE_4);
        e.policy
            .on_card_summary(&card(serial, "Continue with step 4?", "Done."), &c)
            .await;
        let d = e.fake.notes("decision").pop().unwrap();
        assert_eq!(d["decision"], "continue", "{d}");
        broker
            .reply(&owner(), d["confirm_id"].as_u64().unwrap(), true, None)
            .await
            .unwrap();
        settle().await;
    }
    e.fake.answer(CONTINUE_4);
    e.policy
        .on_card_summary(&card(4, "Continue with step 4?", "Done."), &c)
        .await;
    assert_eq!(e.decisions()[3], Env::pair("ask", "limit"));
    assert_eq!(sink.0.lock().unwrap().len(), 3);

    // Stop cancels the waiting continue and clears the counters.
    e.policy
        .call("stop", json!({"pane_id": "w1:p1"}), &c)
        .await
        .unwrap();
    e.fake.answer(CONTINUE_4);
    e.policy
        .on_card_summary(&card(5, "Continue with step 4?", "Done."), &c)
        .await;
    let d = e.fake.notes("decision").pop().unwrap();
    assert_eq!(d["decision"], "continue");
    let id = d["confirm_id"].as_u64().unwrap();
    e.policy
        .call("stop", json!({"pane_id": "w1:p1"}), &c)
        .await
        .unwrap();
    assert!(broker.pending(id).is_none(), "cancelled");
    assert!(broker.reply(&owner(), id, true, None).await.is_err());
    assert_eq!(sink.0.lock().unwrap().len(), 3, "nothing more sent");
    let viewer = ctx_as(&e.root, Principal::local("t"), on()).await;
    assert_eq!(
        e.policy
            .call("stop", json!({"pane_id": "w1:p1"}), &viewer)
            .await
            .unwrap_err()
            .code,
        wire::NOT_PERMITTED
    );
}

fn git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A context for a pane whose folder is `folder`, with the declared
/// `git worktree list`.
async fn ctx_in(folder: &Path, principal: Principal) -> PluginCtx {
    let env = crate::plugins::command::CommandEnv::from_process();
    let git = env.find("git");
    PluginCtx::build(CtxParts {
        ns: NS,
        principal,
        pane: Some("w1:p1".into()),
        lookup: Arc::new(FakeLookup(vec![(
            "w1:p1".into(),
            "claude".into(),
            folder.display().to_string(),
        )])),
        read: patterns(&["*.md", "docs/**/*.md"]),
        plugin_read: patterns(&READ),
        commands: Commands {
            on: vec![(&COMMANDS[0], git)],
            env,
        },
        config: on(),
        notifier: None,
    })
    .await
}

#[tokio::test]
async fn a_pane_in_a_worktree_works_on_the_main_checkouts_plans() {
    if crate::git::binary().is_none() {
        return;
    }
    let e = env();
    git(&e.root, &["init", "-q", "-b", "main"]);
    git(&e.root, &["add", "."]);
    git(
        &e.root,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "-m",
            "x",
        ],
    );
    let wt = e.root.parent().unwrap().join("proj-wt");
    git(
        &e.root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "plan/20261112_02",
            wt.to_str().unwrap(),
        ],
    );
    let wt = std::fs::canonicalize(&wt).unwrap();
    // The worktree's own copy of the plan changes; the main checkout's
    // is what is armed and read.
    std::fs::write(wt.join(PLAN_PATH), plan_text("Hold")).unwrap();
    let c = ctx_in(&wt, owner()).await;
    // Before Build, the worktree's plan/<id> branch links the pane in the
    // Active tasks view (plan 20261112_03, 2.2 item 2).
    e.policy
        .on_card_summary(&card(9, "Done.", "Done."), &c)
        .await;
    let o = e.policy.call("overview", json!({}), &c).await.unwrap();
    assert_eq!(
        o["plans"][0]["agents"][0]["linked_by"], "worktree_branch",
        "{o}"
    );
    assert_eq!(
        o["plans"][0]["agents"][0]["worktree"]["path"],
        wt.display().to_string()
    );
    let row = e
        .policy
        .call("build", json!({"path": PLAN_PATH, "pane_id": "w1:p1"}), &c)
        .await
        .unwrap();
    assert_eq!(row["status"], "Draft", "the main checkout's plan");
    let store = e.policy.store().unwrap();
    assert_eq!(store.load()[0].project_root, e.root.display().to_string());
    e.policy
        .on_card_summary(&card(1, "Done.", "Done."), &c)
        .await;
    let d = e.fake.notes("decision").pop().unwrap();
    assert_eq!(d["rule"], "autocontinue_off");
    assert_eq!(d["worktree"], wt.display().to_string());
    // In the main checkout, no worktree.
    let main = ctx_in(&e.root, owner()).await;
    e.policy
        .on_card_summary(&card(2, "Done.", "Done."), &main)
        .await;
    let d = e.fake.notes("decision").pop().unwrap();
    assert_eq!(d["worktree"], serde_json::Value::Null);
}

#[tokio::test]
async fn worktree_create_only_for_an_approver() {
    let e = env();
    let viewer = ctx_as(&e.root, Principal::local("t"), on()).await;
    let err = e
        .policy
        .call("worktree_create", json!({"path": PLAN_PATH}), &viewer)
        .await
        .unwrap_err();
    assert_eq!(err.code, wire::NOT_PERMITTED);
    assert!(e.fake.worktrees.lock().unwrap().is_empty());
    let c = ctx_as(&e.root, owner(), on()).await;
    let r = e
        .policy
        .call(
            "worktree_create",
            json!({"path": PLAN_PATH, "pane_id": "w1:p1"}),
            &c,
        )
        .await
        .unwrap();
    assert_eq!(
        r,
        json!({"worktree_path": "/wt/plan/20261112_02", "branch": "plan/20261112_02"})
    );
    assert_eq!(
        *e.fake.worktrees.lock().unwrap(),
        [("w1:p1".to_string(), "plan/20261112_02".to_string())]
    );
    // Never on its own: no decision made one.
    e.policy
        .on_card_summary(&card(1, "Done.", "Done."), &c)
        .await;
    assert_eq!(e.fake.worktrees.lock().unwrap().len(), 1);
}

fn pane(id: &str, agent: &str) -> overview::AgentPane {
    overview::AgentPane {
        pane_id: id.into(),
        agent: agent.into(),
        cwd: None,
        worktree: None,
    }
}

fn card_in(pane_id: &str, serial: u64, reply: &str) -> CardSummaryEvent {
    let mut c = card(serial, reply, "Done.");
    c.pane.pane_id = pane_id.into();
    c
}

#[tokio::test]
async fn the_overview_links_panes_to_plans() {
    let mut e = env_with(|state| Fake {
        state: Some(state),
        panes: vec![
            pane("w1:p1", "claude"),
            pane("w1:p2", "claude"),
            pane("w1:p3", "codex"),
            pane("w1:p4", "shell"),
        ],
        ..Fake::default()
    });
    let _ = &mut e;
    let c = ctx_as(&e.root, owner(), on()).await;
    // p1 names the plan's path: reply_path, even for an agent the policy
    // does not drive.
    let off = ctx_as(&e.root, owner(), toml::Table::new()).await;
    e.policy
        .on_card_summary(
            &card_in("w1:p1", 1, &format!("{PLAN_PATH} step 2 done.")),
            &off,
        )
        .await;
    // p2: Gemini's best match by title.
    e.fake.answer(&format!(
        "{{\"plan\": \"{PLAN_PATH}\", \"matched\": \"the sample\"}}"
    ));
    e.policy
        .on_card_summary(&card_in("w1:p2", 2, "Working on the sample thing."), &off)
        .await;
    let o = e.policy.call("overview", json!({}), &c).await.unwrap();
    let plans = o["plans"].as_array().unwrap();
    assert_eq!(plans.len(), 1, "{o}");
    assert_eq!(plans[0]["path"], PLAN_PATH);
    assert_eq!(plans[0]["title"], "A sample plan");
    assert_eq!(plans[0]["steps_total"], 5);
    let agents = plans[0]["agents"].as_array().unwrap();
    let by: Vec<(&str, &str)> = agents
        .iter()
        .map(|a| {
            (
                a["pane_id"].as_str().unwrap(),
                a["linked_by"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(by, [("w1:p1", "reply_path"), ("w1:p2", "gemini")]);
    // The codex pane is unlinked; the shell is in neither list.
    assert_eq!(o["unlinked"], json!(["w1:p3"]));
    // A step a decision names is the plan's current step.
    let c = armed(&e).await;
    e.fake.answers.lock().unwrap().clear();
    e.fake.answer(
        "{\"asks_continue\": false, \"decision\": \"ask\", \"step\": 3, \"reason\": \"Unclear.\"}",
    );
    e.policy
        .on_card_summary(&card_in("w1:p1", 3, "Done."), &c)
        .await;
    let o = e.policy.call("overview", json!({}), &c).await.unwrap();
    assert_eq!(o["plans"][0]["current_step"], 3, "{o}");
    assert_eq!(o["plans"][0]["task"], true);
    assert_eq!(o["plans"][0]["agents"][0]["linked_by"], "pinned");
    assert_eq!(
        o["plans"][0]["agents"][0]["last_decision"]["decision"],
        "ask"
    );
}

#[tokio::test]
async fn overview_notices_are_coalesced() {
    let e = env();
    let c = ctx_as(&e.root, owner(), on()).await;
    e.policy
        .call("pause", json!({"paused": true}), &c)
        .await
        .unwrap();
    e.policy
        .call("pause", json!({"paused": false}), &c)
        .await
        .unwrap();
    assert_eq!(e.fake.notes("state").len(), 2);
    tokio::time::sleep(overview::COALESCE + std::time::Duration::from_millis(200)).await;
    let sent = e.fake.notes("overview");
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0]["paused"], false, "the latest wins");
}
