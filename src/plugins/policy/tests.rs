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
}

pub(super) const PLAN_PATH: &str = "docs/design/20261112_02_A_PLAN.md";

pub(super) fn plan_text(status: &str) -> String {
    format!(
        "---\ntitle: A plan\ndate: 2026-11-12\nstatus: {status}\n---\n\n## 3. Steps\n\n1. **First.** Words.\n   **Done when:** one works.\n2. **Second.** Words.\n   **Done when:** two works.\n3. **Third.** Words.\n   **Done when:** three works.\n4. **Fourth.** Words.\n   **Done when:** four works.\n5. **Fifth.** Words.\n   **Done when:** five works.\n"
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
