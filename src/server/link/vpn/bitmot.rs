//! Bitmot's raw TCP edge-node relay, kept as a persistent user service.
//!
//! The relay's arguments, service specification, command line and log
//! reading are `libhardline::vpn::bitmot`'s, under flankr's service prefix
//! (`dev.flankr.vpn`), edge label (`flankr`), home and state directory, and
//! whether its service runs is asked through `libhardline::tokio::vpn::drive`
//! with [`FlankrServices`], flankr-daemon's [`UserServiceBackend`]
//! (docs/design/20261119_02_LIBHARDLINE_EXTRACTION_PLAN.md, step 9).
//!
//! The edge forward's commands stay flankr's: libhardline's refuse a public
//! port held under another label, and word a missing executable
//! differently, where flankr accepts any TCP forward covering the port; and
//! the relay's log is read as before, as UTF-8 or not at all.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use async_trait::async_trait;
use flankr_daemon::{ServiceSpec, ServiceStatus, UserServiceBackend};
use libhardline::tokio::vpn::{ServiceDriver, drive};
use libhardline::vpn::{ServiceSpec as HardlineSpec, ServiceState};

use super::{SERVICE_PREFIX, Vpn, VpnAddress, VpnError};

pub use libhardline::vpn::bitmot::{EdgeForward, PUBLIC_PORT, announced_host};

/// Stable launchd/user-service identity.
pub const SERVICE_ID: &str = "dev.flankr.vpn.bitmot";
/// The default local Bitmot executable location.
pub const DEFAULT_EXECUTABLE: &str = "~/.local/bin/bitmot";
/// The edge forward's label.
pub const LABEL: &str = "flankr";

/// flankr-daemon's user-service backend, as libhardline's VPN driver asks
/// for it: specifications and states converted both ways.
#[derive(Debug, Default, Clone, Copy)]
pub struct FlankrServices;

impl FlankrServices {
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
}

impl ServiceDriver for FlankrServices {
    fn ensure(&self, spec: &HardlineSpec) -> Result<ServiceState, String> {
        let backend = Self::backend()?;
        backend
            .ensure(&to_flankr(spec))
            .map(to_state)
            .map_err(|e| e.to_string())
    }

    fn status(&self, spec: &HardlineSpec) -> Result<ServiceState, String> {
        let backend = Self::backend()?;
        backend
            .status(&to_flankr(spec))
            .map(to_state)
            .map_err(|e| e.to_string())
    }
}

/// libhardline's service specification as flankr-daemon's.
pub fn to_flankr(spec: &HardlineSpec) -> ServiceSpec {
    ServiceSpec {
        id: spec.id.clone(),
        program: spec.program.clone(),
        args: spec.args.clone(),
        working_dir: spec.working_dir.clone(),
        env: spec.env.clone(),
        stdout: spec.stdout.clone(),
        stderr: spec.stderr.clone(),
        keep_alive: spec.keep_alive,
    }
}

/// flankr-daemon's service status as libhardline's state.
pub fn to_state(status: ServiceStatus) -> ServiceState {
    match status {
        ServiceStatus::Running { pid } => ServiceState::Running { pid },
        ServiceStatus::Loaded => ServiceState::Loaded,
        ServiceStatus::Stopped => ServiceState::Stopped,
        ServiceStatus::Failed(message) => ServiceState::Failed(message),
        ServiceStatus::Unsupported(message) => ServiceState::Unsupported(message),
    }
}

/// Bitmot raw relay for one edge-node name.
#[derive(Debug, Clone)]
pub struct Bitmot {
    relay: libhardline::vpn::Vpn,
}

impl Bitmot {
    /// Resolve the executable under the user's home directory.
    pub fn find(endpoint: &str) -> Self {
        let executable = std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join(".local/bin/bitmot"))
            .unwrap_or_else(|| PathBuf::from(DEFAULT_EXECUTABLE));
        Self::at(endpoint, executable)
    }

    /// Construct with explicit executable discovery, useful to tests and
    /// callers. `endpoint` is not blank: `[link] bitmot_endpoint` is
    /// checked before a relay is made.
    pub fn at(endpoint: impl Into<String>, executable: impl Into<PathBuf>) -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        let relay = libhardline::vpn::Vpn::builder()
            .service_prefix(SERVICE_PREFIX)
            .label(LABEL)
            .state_dir(home.join(".local/state/flankr/services"))
            .home(home)
            .program(executable)
            .bitmot(&endpoint.into())
            .build()
            .expect("a Bitmot relay with flankr's settings and an edge-node name");
        Self { relay }
    }

    fn relay(&self) -> &libhardline::vpn::bitmot::Bitmot {
        self.relay.bitmot().expect("built as Bitmot")
    }

    pub fn endpoint(&self) -> &str {
        self.relay().endpoint()
    }

    pub fn executable(&self) -> &Path {
        self.relay().executable()
    }

    /// Map the public Bitmot port to the local flankr link port.
    pub fn args(&self, port: u16) -> Vec<OsString> {
        self.relay().args(port)
    }

    /// Build the persistent service specification with no credentials.
    pub fn service_spec(&self, port: u16) -> ServiceSpec {
        to_flankr(&self.relay().service_spec(port))
    }

    pub fn command(&self, port: u16) -> String {
        self.relay().command(port)
    }

    fn status_sync(&self, port: u16) -> Result<ServiceStatus, VpnError> {
        let backend = FlankrServices::backend().map_err(|message| VpnError {
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
        announced_host(self.endpoint(), &log)
    }

    fn edge_forward_list(&self) -> Result<Vec<EdgeForward>, VpnError> {
        let command = format!(
            "{} io1 edge forward list {} --json",
            self.executable().display(),
            shell_quote(self.endpoint())
        );
        let output = Command::new(self.executable())
            .args(["io1", "edge", "forward", "list"])
            .arg(self.endpoint())
            .arg("--json")
            .output()
            .map_err(|e| VpnError {
                command: command.clone(),
                message: e.to_string(),
            })?;
        if !output.status.success() {
            return Err(VpnError {
                command,
                message: output_message(&output),
            });
        }
        libhardline::vpn::bitmot::parse_forwards(&output.stdout).map_err(|e| VpnError {
            command,
            message: format!("invalid forward list: {e}"),
        })
    }

    pub(crate) fn ensure_public_forward(&self) -> Result<(), VpnError> {
        if self.edge_forward_list()?.iter().any(covers_public_forward) {
            return Ok(());
        }

        let command = format!(
            "{} io1 edge forward add {} tcp {} --label {LABEL}",
            self.executable().display(),
            shell_quote(self.endpoint()),
            PUBLIC_PORT
        );
        let output = Command::new(self.executable())
            .args(["io1", "edge", "forward", "add"])
            .arg(self.endpoint())
            .args(["tcp", &PUBLIC_PORT.to_string(), "--label", LABEL])
            .output()
            .map_err(|e| VpnError {
                command: command.clone(),
                message: e.to_string(),
            })?;
        if output.status.success() {
            return Ok(());
        }
        if self
            .edge_forward_list()
            .is_ok_and(|forwards| forwards.iter().any(covers_public_forward))
        {
            return Ok(());
        }
        Err(VpnError {
            command,
            message: output_message(&output),
        })
    }
}

#[async_trait]
impl Vpn for Bitmot {
    fn name(&self) -> &'static str {
        "bitmot"
    }

    async fn running(&self) -> Result<bool, VpnError> {
        drive(
            self.relay.running(crate::server::link::DEFAULT_PORT),
            &FlankrServices,
        )
        .await
    }

    async fn address(&self, port: u16) -> Result<Option<VpnAddress>, VpnError> {
        if !self.running().await? {
            return Ok(None);
        }
        let host = self.announced_host(port).ok_or_else(|| VpnError {
            command: self.command(port),
            message: "the Bitmot relay has not announced its public host yet".to_string(),
        })?;
        Ok(Some(VpnAddress {
            host,
            port: PUBLIC_PORT,
        }))
    }

    async fn forwards(&self, port: u16) -> Result<bool, VpnError> {
        if !matches!(self.status_sync(port)?, ServiceStatus::Running { .. }) {
            return Ok(false);
        }
        Ok(self.edge_forward_list()?.iter().any(covers_public_forward))
    }

    async fn forward(&self, port: u16) -> Result<(), VpnError> {
        self.ensure_public_forward()?;
        let backend = FlankrServices::backend().map_err(|message| VpnError {
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

fn output_message(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        return stderr;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        return stdout;
    }
    format!("command exited with {}", output.status)
}

fn covers_public_forward(forward: &EdgeForward) -> bool {
    libhardline::vpn::bitmot::covers(forward, PUBLIC_PORT)
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
                "443=127.0.0.1:7347"
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
    fn a_declared_tcp_range_covers_the_public_port_but_udp_does_not() {
        let forwards: Vec<EdgeForward> = serde_json::from_str(
            r#"[
                {"protocol":"tcp","portFirst":400,"portLast":500},
                {"protocol":"udp","portFirst":443,"portLast":443}
            ]"#,
        )
        .unwrap();
        assert!(covers_public_forward(&forwards[0]));
        assert!(!covers_public_forward(&forwards[1]));
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
