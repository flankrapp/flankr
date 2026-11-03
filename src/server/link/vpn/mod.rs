//! The VPN the link is reached through
//! (docs/design/20261102_03_LINK_VPN_PLAN.md, decisions 1, 2 and 9).
//!
//! The link takes a `dyn Vpn`: [`tailscale::Tailscale`] in the daemon,
//! [`fake::FakeVpn`] in tests, which never run a real VPN.

pub mod fake;
pub mod tailscale;

use std::fmt;

use async_trait::async_trait;

/// This machine's address on a VPN: a name or an IP, and a port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VpnAddress {
    pub host: String,
    pub port: u16,
}

impl fmt::Display for VpnAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.host.contains(':') {
            write!(f, "[{}]:{}", self.host, self.port)
        } else {
            write!(f, "{}:{}", self.host, self.port)
        }
    }
}

/// A VPN command that failed: the command and what it said on stderr
/// (or why it did not finish), never secrets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VpnError {
    pub command: String,
    pub message: String,
}

impl fmt::Display for VpnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}`: {}", self.command, self.message)
    }
}

impl std::error::Error for VpnError {}

/// A VPN that can forward a port on this machine's VPN address to
/// `127.0.0.1:<port>`.
#[async_trait]
pub trait Vpn: Send + Sync {
    /// Its name in logs and in `--vpn`, e.g. `tailscale`.
    fn name(&self) -> &'static str;
    /// Whether the VPN is installed and up on this machine.
    async fn running(&self) -> Result<bool, VpnError>;
    /// This machine's address on the VPN for `port`, once forwarded.
    async fn address(&self, port: u16) -> Result<Option<VpnAddress>, VpnError>;
    /// Whether the VPN already sends `port` to `127.0.0.1:<port>`.
    async fn forwards(&self, port: u16) -> Result<bool, VpnError>;
    /// Make it do so.
    async fn forward(&self, port: u16) -> Result<(), VpnError>;
    /// The command [`Vpn::forward`] runs, to print for running by hand.
    fn forward_command(&self, port: u16) -> String;
}
