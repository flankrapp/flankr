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
}
