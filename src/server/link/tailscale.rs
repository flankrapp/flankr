//! Reaching the link from the tailnet (plan 20261022_02, the 11:58 EDT
//! amendment, "Listening").
//!
//! flankr never changes Tailscale's configuration. It reads `tailscale
//! status --json` and `tailscale serve status --json`, and when Tailscale
//! is running without the forward, logs the command that adds it. Its
//! JSON is read by `vpn::tailscale` (plan 20261102_03, step 1).

use std::time::Duration;

use serde_json::Value;

use super::vpn::tailscale::{forward_present as forwards_port, is_running, self_dns_name};

/// The command that forwards the tailnet's port 7347 to the link.
pub const FORWARD_COMMAND: &str = "tailscale serve --bg --tcp 7347 tcp://127.0.0.1:7347";

const PORT: &str = "7347";
const TARGET: &str = "127.0.0.1:7347";
const TIMEOUT: Duration = Duration::from_secs(5);

/// What Tailscale says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tailscale {
    /// Not installed, not running, or not answering.
    NotRunning,
    /// Running, without the forward to the link.
    NoForward { dns_name: Option<String> },
    /// Running, with the forward; `dns_name` is the machine's MagicDNS name.
    Forwarded { dns_name: Option<String> },
}

impl Tailscale {
    /// The warning to give the person running flankr, if any.
    pub fn warning(&self) -> Option<String> {
        match self {
            Tailscale::NoForward { .. } => Some(format!(
                "Tailscale is running, but does not forward port 7347 to the link; to reach it from the tailnet, run: {FORWARD_COMMAND}"
            )),
            _ => None,
        }
    }

    /// The addresses a pairing string carries for the default listener.
    pub fn addresses(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Tailscale::Forwarded {
            dns_name: Some(name),
        } = self
        {
            out.push(format!("{name}:{PORT}"));
        }
        out.push(TARGET.to_string());
        out
    }
}

/// Ask Tailscale.
pub async fn detect() -> Tailscale {
    let Some(status) = run_json(&["status", "--json"]).await else {
        return Tailscale::NotRunning;
    };
    if !is_running(&status) {
        return Tailscale::NotRunning;
    }
    let dns_name = self_dns_name(&status);
    let serve = run_json(&["serve", "status", "--json"]).await;
    if serve.as_ref().is_some_and(|s| forwards_port(s, 7347)) {
        Tailscale::Forwarded { dns_name }
    } else {
        Tailscale::NoForward { dns_name }
    }
}

async fn run_json(args: &[&str]) -> Option<Value> {
    let mut command = tokio::process::Command::new("tailscale");
    command
        .args(args)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let output = match tokio::time::timeout(TIMEOUT, command.output()).await {
        Ok(Ok(output)) if output.status.success() => output,
        Ok(Ok(output)) => {
            log::debug!("tailscale {args:?} exited with {}", output.status);
            return None;
        }
        Ok(Err(e)) => {
            log::debug!("tailscale {args:?}: {e}");
            return None;
        }
        Err(_) => {
            log::debug!("tailscale {args:?} did not answer within {TIMEOUT:?}");
            return None;
        }
    };
    serde_json::from_slice(&output.stdout).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_and_warnings() {
        let fwd = Tailscale::Forwarded {
            dns_name: Some("mini.tail1.ts.net".into()),
        };
        assert_eq!(
            fwd.addresses(),
            vec!["mini.tail1.ts.net:7347", "127.0.0.1:7347"]
        );
        assert_eq!(fwd.warning(), None);
        let missing = Tailscale::NoForward { dns_name: None };
        assert_eq!(missing.addresses(), vec!["127.0.0.1:7347"]);
        assert!(missing.warning().unwrap().contains(FORWARD_COMMAND));
        assert_eq!(Tailscale::NotRunning.addresses(), vec!["127.0.0.1:7347"]);
        assert_eq!(Tailscale::NotRunning.warning(), None);
    }
}
