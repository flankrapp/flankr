//! Bitmot's raw TCP edge-node relay, kept as a persistent user service.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use flankr_daemon::{ServiceSpec, ServiceStatus, UserServiceBackend};

use super::{Vpn, VpnAddress, VpnError};

/// Stable launchd/user-service identity.
pub const SERVICE_ID: &str = "dev.flankr.vpn.bitmot";
/// The default local Bitmot executable location.
pub const DEFAULT_EXECUTABLE: &str = "~/.local/bin/bitmot";

/// Bitmot raw relay for one edge-node name.
#[derive(Debug, Clone)]
pub struct Bitmot {
    endpoint: String,
    executable: PathBuf,
}

impl Bitmot {
    /// Resolve the executable under the user's home directory.
    pub fn find(endpoint: &str) -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let executable = home
            .map(|home| home.join(".local/bin/bitmot"))
            .unwrap_or_else(|| PathBuf::from(DEFAULT_EXECUTABLE));
        Self {
            endpoint: endpoint.to_string(),
            executable,
        }
    }

    /// Construct with explicit executable discovery, useful to tests and callers.
    pub fn at(endpoint: impl Into<String>, executable: impl Into<PathBuf>) -> Self {
        Self {
            endpoint: endpoint.into(),
            executable: executable.into(),
        }
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// The one command used by both the link driver and service registry.
    pub fn args(&self, port: u16) -> Vec<OsString> {
        vec![
            "io1".into(),
            "relay".into(),
            "--node".into(),
            self.endpoint.clone().into(),
            "--tcp".into(),
            format!("{port}=127.0.0.1:{port}").into(),
        ]
    }

    /// Build the persistent service specification with no credentials.
    pub fn service_spec(&self, port: u16) -> ServiceSpec {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        let state = home.join(".local/state/flankr/services");
        ServiceSpec {
            id: SERVICE_ID.to_string(),
            program: self.executable.clone(),
            args: self.args(port),
            working_dir: home.clone(),
            env: vec![
                ("HOME".into(), home.as_os_str().to_os_string()),
                ("PATH".into(), "/usr/bin:/bin:/usr/sbin:/sbin".into()),
            ],
            stdout: state.join("vpn.bitmot.log"),
            stderr: state.join("vpn.bitmot.log"),
            keep_alive: true,
        }
    }

    pub fn command(&self, port: u16) -> String {
        let mut text = self.executable.display().to_string();
        for arg in self.args(port) {
            text.push(' ');
            text.push_str(&shell_quote(&arg.to_string_lossy()));
        }
        text
    }

    fn backend() -> Result<Box<dyn UserServiceBackend>, String> {
        #[cfg(target_os = "macos")]
        {
            flankr_daemon::platform_backend()
                .map(|backend| Box::new(backend) as Box<dyn UserServiceBackend>)
                .map_err(|e| e.to_string())
        }
        #[cfg(not(target_os = "macos"))]
        {
            Ok(Box::new(flankr_daemon::platform_backend()))
        }
    }

    fn status_sync(&self, port: u16) -> Result<ServiceStatus, VpnError> {
        let backend = Self::backend().map_err(|message| VpnError {
            command: self.command(port),
            message,
        })?;
        backend
            .status(&self.service_spec(port))
            .map_err(|e| VpnError {
                command: self.command(port),
                message: e.to_string(),
            })
    }

    fn announced_host(&self, port: u16) -> Option<String> {
        let log = std::fs::read_to_string(self.service_spec(port).stdout).ok()?;
        announced_host(&self.endpoint, &log)
    }
}

#[async_trait]
impl Vpn for Bitmot {
    fn name(&self) -> &'static str {
        "bitmot"
    }

    async fn running(&self) -> Result<bool, VpnError> {
        match self.status_sync(crate::server::link::DEFAULT_PORT)? {
            ServiceStatus::Unsupported(message) | ServiceStatus::Failed(message) => Err(VpnError {
                command: self.command(crate::server::link::DEFAULT_PORT),
                message,
            }),
            _ => Ok(true),
        }
    }

    async fn address(&self, port: u16) -> Result<Option<VpnAddress>, VpnError> {
        if !self.running().await? {
            return Ok(None);
        }
        let host = self.announced_host(port).ok_or_else(|| VpnError {
            command: self.command(port),
            message: "the Bitmot relay has not announced its public host yet".to_string(),
        })?;
        Ok(Some(VpnAddress { host, port }))
    }

    async fn forwards(&self, port: u16) -> Result<bool, VpnError> {
        Ok(matches!(
            self.status_sync(port)?,
            ServiceStatus::Running { .. }
        ))
    }

    async fn forward(&self, port: u16) -> Result<(), VpnError> {
        let backend = Self::backend().map_err(|message| VpnError {
            command: self.command(port),
            message,
        })?;
        let status = backend
            .ensure(&self.service_spec(port))
            .map_err(|e| VpnError {
                command: self.command(port),
                message: e.to_string(),
            })?;
        if matches!(
            status,
            ServiceStatus::Unsupported(_) | ServiceStatus::Failed(_)
        ) {
            return Err(VpnError {
                command: self.command(port),
                message: status.description(),
            });
        }
        Ok(())
    }

    fn forward_command(&self, port: u16) -> String {
        self.command(port)
    }
}

fn announced_host(endpoint: &str, log: &str) -> Option<String> {
    let prefix = format!("relay: edge node '{endpoint}' is live on ");
    log.lines().rev().find_map(|line| {
        line.strip_prefix(&prefix)
            .and_then(|rest| rest.split_whitespace().next())
            .filter(|host| !host.is_empty())
            .map(str::to_string)
    })
}
fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | ':'))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_and_spec_share_exact_raw_tcp_arguments() {
        let bitmot = Bitmot::at("mynode.io1.io", "/Users/u/.local/bin/bitmot");
        assert_eq!(
            bitmot.args(7347),
            vec![
                "io1",
                "relay",
                "--node",
                "mynode.io1.io",
                "--tcp",
                "7347=127.0.0.1:7347"
            ]
            .into_iter()
            .map(OsString::from)
            .collect::<Vec<_>>()
        );
        let spec = bitmot.service_spec(7347);
        assert_eq!(spec.id, SERVICE_ID);
        assert_eq!(spec.args, bitmot.args(7347));
        assert!(!bitmot.command(7347).contains("--public"));
    }
    #[test]
    fn announced_host_uses_the_latest_matching_relay_line() {
        let log = "\
relay: edge node 'other' is live on other.e.io1.io
relay: edge node 'mynode' is live on old.e.io1.io
relay: edge node 'mynode' is live on current.e.io1.io
";
        assert_eq!(
            announced_host("mynode", log).as_deref(),
            Some("current.e.io1.io")
        );
        assert_eq!(announced_host("missing", log), None);
    }
}
