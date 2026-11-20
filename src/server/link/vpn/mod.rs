//! The VPN the link is reached through
//! (docs/design/20261102_03_LINK_VPN_PLAN.md, decisions 1, 2 and 9).
//!
//! The link takes a `dyn Vpn`: [`tailscale::Tailscale`] in the daemon,
//! [`fake::FakeVpn`] in tests, which never run a real VPN.
//!
//! With a provider built, the providers, the address, the error and the
//! fake are `libhardline::vpn`'s, their tasks driven by
//! `libhardline::tokio::vpn::drive`; the choice's names are flankr's
//! (docs/design/20261119_02_LIBHARDLINE_EXTRACTION_PLAN.md, step 9).

#[cfg(feature = "vpn-bitmot")]
pub mod bitmot;
pub mod fake;
#[cfg(feature = "vpn-tailscale")]
pub mod tailscale;

use async_trait::async_trait;

#[cfg(any(feature = "vpn-tailscale", feature = "vpn-bitmot"))]
pub use libhardline::vpn::{VpnAddress, VpnError};

/// The service prefix of flankr's VPN jobs: `dev.flankr.vpn.<provider>`.
#[cfg(any(feature = "vpn-tailscale", feature = "vpn-bitmot"))]
pub const SERVICE_PREFIX: &str = "dev.flankr.vpn";

/// This machine's address on a VPN: a name or an IP, and a port.
#[cfg(not(any(feature = "vpn-tailscale", feature = "vpn-bitmot")))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VpnAddress {
    pub host: String,
    pub port: u16,
}

#[cfg(not(any(feature = "vpn-tailscale", feature = "vpn-bitmot")))]
impl std::fmt::Display for VpnAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.host.contains(':') {
            write!(f, "[{}]:{}", self.host, self.port)
        } else {
            write!(f, "{}:{}", self.host, self.port)
        }
    }
}

/// A VPN command that failed: the command and what it said on stderr
/// (or why it did not finish), never secrets.
#[cfg(not(any(feature = "vpn-tailscale", feature = "vpn-bitmot")))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VpnError {
    pub command: String,
    pub message: String,
}

#[cfg(not(any(feature = "vpn-tailscale", feature = "vpn-bitmot")))]
impl std::fmt::Display for VpnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "`{}`: {}", self.command, self.message)
    }
}

#[cfg(not(any(feature = "vpn-tailscale", feature = "vpn-bitmot")))]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VpnChoice {
    #[cfg(feature = "vpn-tailscale")]
    /// The default when Tailscale is built.
    Tailscale,
    #[cfg(feature = "vpn-bitmot")]
    /// Bitmot's persistent raw-TCP edge relay.
    Bitmot,
    /// No VPN: nothing is run or read for one.
    None,
}

impl Default for VpnChoice {
    #[cfg(feature = "vpn-tailscale")]
    fn default() -> Self {
        Self::Tailscale
    }

    #[cfg(all(not(feature = "vpn-tailscale"), feature = "vpn-bitmot"))]
    fn default() -> Self {
        Self::None
    }

    #[cfg(all(not(feature = "vpn-tailscale"), not(feature = "vpn-bitmot")))]
    fn default() -> Self {
        Self::None
    }
}

#[cfg(any(feature = "vpn-tailscale", feature = "vpn-bitmot"))]
impl From<libhardline::vpn::VpnChoice> for VpnChoice {
    fn from(choice: libhardline::vpn::VpnChoice) -> Self {
        match choice {
            #[cfg(feature = "vpn-tailscale")]
            libhardline::vpn::VpnChoice::Tailscale => VpnChoice::Tailscale,
            #[cfg(feature = "vpn-bitmot")]
            libhardline::vpn::VpnChoice::Bitmot => VpnChoice::Bitmot,
            libhardline::vpn::VpnChoice::None => VpnChoice::None,
        }
    }
}

impl VpnChoice {
    /// `tailscale`, `bitmot` or `none`, any case; `what` names where it
    /// came from in errors.
    #[cfg(any(feature = "vpn-tailscale", feature = "vpn-bitmot"))]
    pub fn parse(name: &str, what: &str) -> Result<VpnChoice, String> {
        libhardline::vpn::VpnChoice::parse(name, what).map(VpnChoice::from)
    }

    /// `tailscale`, `bitmot` or `none`, any case; `what` names where it
    /// came from in errors.
    #[cfg(not(any(feature = "vpn-tailscale", feature = "vpn-bitmot")))]
    pub fn parse(name: &str, what: &str) -> Result<VpnChoice, String> {
        match name.trim().to_ascii_lowercase().as_str() {
            "tailscale" => Err(format!(
                "{what} requests tailscale, but it is not built; enable Cargo feature vpn-tailscale"
            )),
            "bitmot" => Err(format!(
                "{what} requests bitmot, but it is not built; enable Cargo feature vpn-bitmot"
            )),
            "none" => Ok(VpnChoice::None),
            _ => Err(format!(
                "{what} '{name}' is not known; use tailscale, bitmot, or none"
            )),
        }
    }

    /// `--vpn`, else `[link] vpn`, else the default.
    #[cfg(any(feature = "vpn-tailscale", feature = "vpn-bitmot"))]
    pub fn choose(flag: Option<&str>, config: Option<&str>) -> Result<VpnChoice, String> {
        libhardline::vpn::VpnChoice::choose(flag, "--vpn", config, "[link] vpn =")
            .map(VpnChoice::from)
    }

    /// `--vpn`, else `[link] vpn`, else the default.
    #[cfg(not(any(feature = "vpn-tailscale", feature = "vpn-bitmot")))]
    pub fn choose(flag: Option<&str>, config: Option<&str>) -> Result<VpnChoice, String> {
        if let Some(flag) = flag {
            return VpnChoice::parse(flag, "--vpn");
        }
        match config {
            Some(config) => VpnChoice::parse(config, "[link] vpn ="),
            None => Ok(VpnChoice::default()),
        }
    }

    pub fn open(self, _endpoint: Option<&str>) -> Option<std::sync::Arc<dyn Vpn>> {
        match self {
            #[cfg(feature = "vpn-tailscale")]
            VpnChoice::Tailscale => Some(std::sync::Arc::new(tailscale::Tailscale::find())),
            #[cfg(feature = "vpn-bitmot")]
            VpnChoice::Bitmot => _endpoint
                .map(bitmot::Bitmot::find)
                .map(|vpn| std::sync::Arc::new(vpn) as std::sync::Arc<dyn Vpn>),
            VpnChoice::None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "vpn-tailscale")]
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
        assert!(err.contains("not known"), "{err}");
        let err = VpnChoice::choose(None, Some("zt")).unwrap_err();
        assert!(err.starts_with("[link] vpn = 'zt'"), "{err}");
    }

    #[cfg(not(feature = "vpn-tailscale"))]
    #[test]
    fn no_provider_defaults_to_none_and_names_missing_feature() {
        assert_eq!(VpnChoice::choose(None, None), Ok(VpnChoice::None));
        let err = VpnChoice::choose(Some("tailscale"), None).unwrap_err();
        assert!(err.contains("vpn-tailscale"), "{err}");
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
