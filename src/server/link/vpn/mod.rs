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

/// Which VPN the link is reached through (decision 3).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VpnChoice {
    /// The default.
    #[default]
    Tailscale,
    /// No VPN: nothing is run or read for one.
    None,
}

impl VpnChoice {
    /// `tailscale` or `none`, in any case; `what` names where it came from
    /// in the error.
    pub fn parse(name: &str, what: &str) -> Result<VpnChoice, String> {
        match name.trim().to_ascii_lowercase().as_str() {
            "tailscale" => Ok(VpnChoice::Tailscale),
            "none" => Ok(VpnChoice::None),
            _ => Err(format!(
                "{what} '{name}' is not known; use tailscale or none"
            )),
        }
    }

    /// `--vpn`, else `[link] vpn`, else Tailscale.
    pub fn choose(flag: Option<&str>, config: Option<&str>) -> Result<VpnChoice, String> {
        if let Some(flag) = flag {
            return VpnChoice::parse(flag, "--vpn");
        }
        match config {
            Some(config) => VpnChoice::parse(config, "[link] vpn ="),
            None => Ok(VpnChoice::default()),
        }
    }

    /// The VPN to use, `None` for no VPN.
    pub fn open(self) -> Option<std::sync::Arc<dyn Vpn>> {
        match self {
            VpnChoice::Tailscale => Some(std::sync::Arc::new(tailscale::Tailscale::find())),
            VpnChoice::None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flag_wins_then_the_config_then_tailscale() {
        assert_eq!(VpnChoice::choose(None, None), Ok(VpnChoice::Tailscale));
        assert_eq!(VpnChoice::choose(None, Some("none")), Ok(VpnChoice::None));
        assert_eq!(
            VpnChoice::choose(Some("tailscale"), Some("none")),
            Ok(VpnChoice::Tailscale)
        );
        assert_eq!(
            VpnChoice::choose(Some("None"), Some("tailscale")),
            Ok(VpnChoice::None)
        );
        let err = VpnChoice::choose(Some("wireguard"), None).unwrap_err();
        assert_eq!(err, "--vpn 'wireguard' is not known; use tailscale or none");
        let err = VpnChoice::choose(None, Some("zt")).unwrap_err();
        assert!(err.starts_with("[link] vpn = 'zt'"), "{err}");
    }

    #[test]
    fn addresses_print_with_brackets_for_ipv6() {
        let a = VpnAddress {
            host: "mini.ts.net".into(),
            port: 7347,
        };
        assert_eq!(a.to_string(), "mini.ts.net:7347");
        let a = VpnAddress {
            host: "fd7a::1".into(),
            port: 7347,
        };
        assert_eq!(a.to_string(), "[fd7a::1]:7347");
    }
}
