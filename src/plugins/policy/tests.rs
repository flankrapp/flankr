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
