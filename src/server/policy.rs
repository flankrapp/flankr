//! The daemon's services for the plan policy plugin
//! (docs/design/20261112_02_PLAN_POLICY_AGENT_PLAN.md): its state folder
//! and its notifications to every phone. Made before the daemon, and
//! pointed at it once it exists.

use std::path::PathBuf;
use std::sync::{OnceLock, Weak};

use serde_json::Value;

use super::Daemon;
use crate::plugins::policy::{NS, Services};

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
}
