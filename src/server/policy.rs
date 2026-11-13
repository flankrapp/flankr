//! The daemon's services for the plan policy plugin
//! (docs/design/20261112_02_PLAN_POLICY_AGENT_PLAN.md): its state folder
//! and its notifications to every phone, the pane's questions, and the
//! judgement on `[llm.uses.policy]`. Made before the daemon, and pointed
//! at it once it exists.

use std::path::PathBuf;
use std::sync::{OnceLock, Weak};

use serde_json::Value;

use super::Daemon;
use crate::llm::{Profile, Use};
use crate::plugins::policy::{NS, Services};
use crate::reasoning::Message;

/// The plan policy plugin's way into the daemon.
#[derive(Default)]
pub struct DaemonPolicy {
    daemon: OnceLock<Weak<Daemon>>,
}

impl DaemonPolicy {
    pub fn set(&self, daemon: Weak<Daemon>) {
        let _ = self.daemon.set(daemon);
    }

    fn daemon(&self) -> Option<std::sync::Arc<Daemon>> {
        self.daemon.get()?.upgrade()
    }
}

#[async_trait::async_trait]
impl Services for DaemonPolicy {
    /// `~/.local/state/flankr`: the folder above the approval journals'.
    fn state_dir(&self) -> Option<PathBuf> {
        let daemon = self.daemon()?;
        daemon.sessions.state.parent().map(PathBuf::from)
    }

    fn notify_all(&self, method: &str, params: Value) {
        if let Some(daemon) = self.daemon() {
            daemon.plugins.broadcast(NS, method, params);
        }
    }

    /// The pending question of the tailed session in `pane_id`.
    fn pending_question(&self, pane_id: &str) -> Option<String> {
        let daemon = self.daemon()?;
        daemon
            .hub
            .tailers()
            .into_iter()
            .find(|t| {
                t.pane()
                    .is_some_and(|p| crate::watch::pane_ids_match(&p.pane_id, pane_id))
            })?
            .pending_question()
            .map(|q| q.text)
    }

    /// The choice question open on `pane_id`, as `agents.send` refuses
    /// for (plan 20261029_01, decision 6); herdr local only.
    async fn open_choice(&self, pane_id: &str) -> Option<String> {
        let daemon = self.daemon()?;
        if matches!(
            daemon.client.transport(),
            libherdr::transport::HerdrTransport::RemoteSsh { .. }
        ) {
            return None;
        }
        let snapshot = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            daemon.client.list_panes(),
        )
        .await
        .ok()?
        .ok()?;
        let pane = crate::core::agents::agents(&snapshot)
            .into_iter()
            .find(|a| crate::watch::pane_ids_match(&a.pane_id, pane_id))?;
        let open = daemon.sessions.open_choice(&pane)?;
        let first = open.set.questions.first()?;
        Some(first.text.clone())
    }

    /// herdr's panes, with each agent's worktree as `agents.list` gives it.
    async fn agent_panes(&self) -> Vec<crate::plugins::policy::overview::AgentPane> {
        let Some(daemon) = self.daemon() else {
            return Vec::new();
        };
        let Ok(Ok(snapshot)) = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            daemon.client.list_panes(),
        )
        .await
        else {
            return Vec::new();
        };
        let agents = crate::core::agents::agents(&snapshot);
        let workspaces: std::collections::BTreeSet<String> = agents
            .iter()
            .filter_map(|a| a.workspace_id.clone())
            .collect();
        let trees = daemon.worktrees.of(&daemon.client, workspaces).await;
        agents
            .into_iter()
            .map(|a| {
                let worktree = a
                    .workspace_id
                    .as_ref()
                    .and_then(|ws| trees.get(ws))
                    .zip(a.cwd.as_deref())
                    .and_then(|(list, cwd)| super::worktrees::worktree_of(list, cwd))
                    .map(|w| (w.path, w.branch, w.label));
                crate::plugins::policy::overview::AgentPane {
                    pane_id: a.pane_id,
                    agent: a.agent,
                    cwd: a.cwd,
                    worktree,
                }
            })
            .collect()
    }

    fn agents(&self) -> Vec<crate::config::AgentConfig> {
        self.daemon().map(|d| d.agents.clone()).unwrap_or_default()
    }

    /// The broker's standing confirmation, as `agents.send` opens it; a
    /// task waits for its resolution.
    fn request_confirmation(
        &self,
        principal: &crate::core::principal::Principal,
        pane_id: &str,
        agent: &str,
        command: &str,
    ) -> Result<(u64, tokio::sync::oneshot::Receiver<&'static str>), String> {
        use crate::core::confirm::{ConfirmEvent, Lifetime, Outcome};
        let daemon = self.daemon().ok_or("the daemon is gone")?;
        let mut events = daemon.broker.subscribe();
        let id = daemon
            .broker
            .request(principal, pane_id, agent, command, None, Lifetime::Standing)
            .map_err(|e| e.to_string())?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(ConfirmEvent::Resolved { view, outcome, .. })
                        if view.confirm_id == id && outcome != Outcome::Approved =>
                    {
                        let _ = tx.send(outcome.as_str());
                        return;
                    }
                    Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                }
            }
        });
        Ok((id, rx))
    }

    fn cancel_confirmation(&self, principal: &crate::core::principal::Principal, confirm_id: u64) {
        if let Some(daemon) = self.daemon() {
            let _ = daemon.broker.cancel(principal, confirm_id);
        }
    }

    /// `worktree.create` on the pane's workspace, herdr local only.
    async fn worktree_create(&self, pane_id: &str, branch: &str) -> Result<String, String> {
        let daemon = self.daemon().ok_or("the daemon is gone")?;
        if !matches!(
            daemon.client.transport(),
            libherdr::transport::HerdrTransport::LocalSocket { .. }
                | libherdr::transport::HerdrTransport::Mock { .. }
        ) {
            return Err("herdr worktrees are made on the local herdr only".to_string());
        }
        let snapshot = daemon
            .client
            .list_panes()
            .await
            .map_err(|e| e.to_string())?;
        let pane = crate::core::agents::agents(&snapshot)
            .into_iter()
            .find(|a| crate::watch::pane_ids_match(&a.pane_id, pane_id))
            .ok_or_else(|| format!("no pane {pane_id}"))?;
        let reply = daemon
            .client
            .worktree_create(&libherdr::WorktreeCreate {
                workspace_id: pane.workspace_id.clone(),
                cwd: pane
                    .workspace_id
                    .is_none()
                    .then(|| pane.cwd.clone())
                    .flatten(),
                branch: Some(branch.to_string()),
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;
        let result = reply.get("result").unwrap_or(&reply);
        result
            .pointer("/worktree/path")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| "herdr's reply names no worktree".to_string())
    }

    /// One call on `[llm.uses.policy]`: the system text, one user message,
    /// no tools, at most [`JUDGE_TIMEOUT`].
    async fn judge(&self, system: &str, user: &str) -> Result<String, String> {
        let daemon = self.daemon().ok_or("the daemon is gone")?;
        let engine = daemon
            .llm
            .client(Use::Policy, Profile::Summary, Some(system))?;
        let turns = [Message {
            role: "user".to_string(),
            content: user.to_string(),
            name: None,
            thought_signature: None,
        }];
        tokio::time::timeout(
            JUDGE_TIMEOUT,
            engine.process_turns(&turns, &super::speech::NoTools),
        )
        .await
        .map_err(|_| "the judgement timed out".to_string())??
        .filter(|a| !a.trim().is_empty())
        .ok_or_else(|| "the judgement was empty".to_string())
    }
}

/// How long the judgement may take; past it the card is `ask`.
pub const JUDGE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
