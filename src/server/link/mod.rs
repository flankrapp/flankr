//! The link listener: Noise over TCP for paired devices
//! (docs/design/20261022_02_LINK_AND_COMPANION_PLAN.md, decisions 4–6 and
//! the amendments of 11:58, 18:20 and 23:07 EDT; the rules for step 3 in
//! CHANGES.md, 2026-10-22 18:24 EDT).
//!
//! * By default the link binds only `127.0.0.1:7347`; `--bind` or
//!   `[link] listen` replace that. Through a VPN ([`vpn`], Tailscale by
//!   default), the daemon forwards the VPN's port to the first
//!   `127.0.0.1` address it binds, adding the forward itself when it is
//!   missing; `--vpn none` runs and reads nothing for a VPN
//!   (docs/design/20261102_03_LINK_VPN_PLAN.md, decisions 3 to 8).
//! * A pairing offer carries the VPN's address first, then, with no VPN
//!   and a binding other machines can reach, this machine's network name
//!   ([`machine`]), then the bound addresses (each interface's for
//!   `0.0.0.0` and `::`). An offer only this machine could reach is
//!   refused (the plan's 23:53 EST amendment).
//! * A connection is `IK`, or a pairing request (`NK`) while a
//!   `flankr pair` run is in pair mode ([`pair_mode`]; plan 20261028_01,
//!   the amendments of 14:37 to 14:44 EDT). A handshake must finish within
//!   [`HANDSHAKE_TIMEOUT`], and a key not in `auth.toml` gets no reply.
//! * Pairing moves only public keys: `link.pair_begin` gives the pairing
//!   URL (the server's key, addresses and a fresh offer) and puts the
//!   daemon in pair mode for the connection that asked; a phone's request
//!   with the offer, or a pasted device code (`link.pair_code`), is added
//!   to `auth.toml` only when the owner answers `yes` to its six words
//!   (`link.pair_answer`). `link.pair_add` still adds a device code at
//!   once, for the daemon socket's own tools. The device's first `hello`
//!   after it is added is answered with `paired: true`, and
//!   `link.pair_done` goes to the connection that added it by
//!   `link.pair_add`.
//! * `link.unpair`, over a paired link only, removes the calling device's
//!   entry from `auth.toml` and closes its connections once the answer is
//!   queued (plan 20261024_02, decision 6).
//! * A handshake's first message may only be `hello`; anything else closes
//!   the connection without a reply.
//! * `auth.toml` is read for every new connection, and on each change the
//!   daemon's file watcher reports (plan 20261027_05, decision 2) and every
//!   `revoke_interval` as a slow fallback, open connections whose key is no
//!   longer in it are closed.
//! * Sockets get `SO_KEEPALIVE` with the OS's timers; there is no
//!   application idle timeout.
//! * Each link message is one whole JSON-RPC object, without a newline.

pub mod auth;
pub mod keyfile;
pub mod machine;
pub mod pair_mode;
pub mod vpn;

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use libflankr_proto::common::{
    LinkPairAddParams, LinkPairAddResult, LinkPairBeginResult, LinkPairDone,
};
use libflankr_proto::link::{
    DeviceCode, Incoming as LinkIncoming, KEY_LEN, PairingPayload, PairingStringError,
    StaticKeypair, accept_incoming, generate_offer,
};
use libflankr_proto::{Incoming, Request};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use self::auth::{AuthEntry, AuthFile};
use self::machine::{Machine, ThisMachine};
use self::pair_mode::{PairBeginExtra, PairBeginParams, PairEvent, PairRun};
use self::vpn::{Vpn, VpnAddress, VpnChoice};
use super::Daemon;
use crate::config::LinkConfig;

/// The link's port, when an address is given without one.
pub const DEFAULT_PORT: u16 = 7347;
/// The default listener.
pub const DEFAULT_LISTEN: &str = "127.0.0.1:7347";
/// How long a handshake may take.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// How often open connections are checked against `auth.toml`.
pub const DEFAULT_REVOKE_INTERVAL: Duration = Duration::from_secs(60);

/// Where the link keeps its files, where it listens, and how often it
/// checks for revocations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkSettings {
    pub key_path: PathBuf,
    pub auth_path: PathBuf,
    /// `None`: the default listener, `127.0.0.1:7347`.
    pub listen: Option<Vec<String>>,
    pub revoke_interval: Duration,
    /// The VPN the link is reached through (`--vpn`, `[link] vpn`).
    pub vpn: VpnChoice,
    /// Give pairing offers whose addresses are all loopback, which the
    /// daemon refuses: for tests over `127.0.0.1` only.
    pub loopback_offers: bool,
}

fn config_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".config").join("flankr")
}

fn expand_tilde(p: &str) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => std::env::var("HOME")
            .map(|home| PathBuf::from(home).join(rest))
            .unwrap_or_else(|_| PathBuf::from(p)),
        None => PathBuf::from(p),
    }
}

impl LinkSettings {
    /// From `[link]`; files default to `~/.config/flankr/`. An unknown
    /// `vpn` is an error.
    pub fn from_config(config: &LinkConfig) -> Result<Self, String> {
        Ok(LinkSettings {
            key_path: config
                .key
                .as_deref()
                .map(expand_tilde)
                .unwrap_or_else(|| config_dir().join("link_key")),
            auth_path: config
                .auth
                .as_deref()
                .map(expand_tilde)
                .unwrap_or_else(|| config_dir().join("auth.toml")),
            listen: config.listen.clone(),
            revoke_interval: config
                .revoke_check_secs
                .map(|s| Duration::from_secs(s.max(1)))
                .unwrap_or(DEFAULT_REVOKE_INTERVAL),
            vpn: VpnChoice::choose(None, config.vpn.as_deref())?,
            loopback_offers: false,
        })
    }

    /// With the command line's `--vpn` and `--bind`s, which replace
    /// `[link] vpn` and `[link] listen` for this run.
    pub fn with_flags(mut self, vpn: Option<&str>, bind: &[String]) -> Result<Self, String> {
        if let Some(vpn) = vpn {
            self.vpn = VpnChoice::parse(vpn, "--vpn")?;
        }
        if !bind.is_empty() {
            let mut listen = Vec::new();
            for addr in bind {
                let normal = normalize_listen(addr);
                if normal.parse::<SocketAddr>().is_err() {
                    return Err(format!(
                        "--bind '{addr}' is not an address: give an IP and port, such as 127.0.0.1:7347, 0.0.0.0:7347 or [::]:7347"
                    ));
                }
                listen.push(normal);
            }
            self.listen = Some(listen);
        }
        Ok(self)
    }

    /// Whether the link is on: anything but an explicit empty list.
    pub fn enabled(&self) -> bool {
        self.listen.as_ref().is_none_or(|l| !l.is_empty())
    }
}

/// `<addr>:<port>`, adding port 7347 to a bare address.
pub fn normalize_listen(addr: &str) -> String {
    let addr = addr.trim();
    if addr.parse::<SocketAddr>().is_ok() {
        return addr.to_string();
    }
    if let Ok(ip) = addr.parse::<std::net::IpAddr>() {
        return SocketAddr::new(ip, DEFAULT_PORT).to_string();
    }
    match addr.rsplit_once(':') {
        Some((_, port)) if port.parse::<u16>().is_ok() => addr.to_string(),
        _ => format!("{addr}:{DEFAULT_PORT}"),
    }
}

/// How a pairing offer only this machine can reach begins.
pub const ONLY_THIS_MACHINE: &str = "flankr is only reachable from this machine";

/// What the VPN's set-up found.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Reach {
    /// Not looked yet, or no VPN.
    Unknown,
    /// No `127.0.0.1` address bound to forward to.
    NoLoopback,
    /// Not installed or not running.
    NotRunning,
    /// Forwarding, at this address.
    Forwarded(VpnAddress),
    /// Forwarding, but with no name for this machine.
    NoName,
    /// The forward could not be read or made: the error and the command.
    Failed(String),
}

/// `tailscale` as a name in a sentence.
fn title(name: &str) -> String {
    match name {
        "tailscale" => "Tailscale".to_string(),
        other => other.to_string(),
    }
}

fn host_of(address: &str) -> &str {
    let host = address.rsplit_once(':').map_or(address, |(host, _)| host);
    host.trim_start_matches('[').trim_end_matches(']')
}

/// Whether only this machine can reach `address`: `127.0.0.0/8`, `::1`
/// or `localhost`.
pub fn is_loopback(address: &str) -> bool {
    let host = host_of(address);
    host.eq_ignore_ascii_case("localhost")
        || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// Whether `address` is `0.0.0.0` or `::`, every address.
pub fn is_unspecified(address: &str) -> bool {
    address
        .parse::<SocketAddr>()
        .is_ok_and(|a| a.ip().is_unspecified())
}

/// The port of the first bound `127.0.0.1` address: the one a VPN's
/// forward to `127.0.0.1:<port>` reaches.
pub fn forward_port(bound: &[String]) -> Option<u16> {
    bound.iter().find_map(|a| {
        a.parse::<SocketAddr>()
            .ok()
            .filter(|a| a.ip() == IpAddr::from([127, 0, 0, 1]))
            .map(|a| a.port())
    })
}

/// A pairing offer's addresses, in the order a phone tries them: the
/// VPN's, this machine's name (with the first reachable bound port), then
/// each bound address, `0.0.0.0` and `::` replaced by the interfaces'
/// addresses (IPv4 for `0.0.0.0`, both for `::`). No address twice.
pub fn offer_addresses(
    bound: &[String],
    vpn: Option<&str>,
    host_name: Option<&str>,
    interfaces: &[IpAddr],
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |a: String| {
        if !out.contains(&a) {
            out.push(a);
        }
    };
    if let Some(vpn) = vpn {
        push(vpn.to_string());
    }
    if let Some(host) = host_name
        && let Some(port) = bound
            .iter()
            .filter(|a| !is_loopback(a))
            .find_map(|a| a.parse::<SocketAddr>().ok().map(|a| a.port()))
    {
        push(format!("{host}:{port}"));
    }
    for address in bound {
        match address.parse::<SocketAddr>() {
            Ok(a) if a.ip().is_unspecified() => {
                for ip in interfaces {
                    if a.is_ipv6() || ip.is_ipv4() {
                        push(SocketAddr::new(*ip, a.port()).to_string());
                    }
                }
            }
            _ => push(address.clone()),
        }
    }
    out
}

/// A device `link.pair_add` added that has not said `hello` yet.
struct Awaiting {
    name: String,
    /// To the connection that added it.
    done: oneshot::Sender<LinkPairDone>,
}

/// An open link connection, for the revocation check.
struct OpenConn {
    key: [u8; KEY_LEN],
    name: String,
    close: CancellationToken,
}

/// A device added by `link.pair_add`.
pub struct Added {
    pub result: LinkPairAddResult,
    /// Resolves when the device's first `hello` is answered.
    pub done: oneshot::Receiver<LinkPairDone>,
}

/// The link's shared state.
pub struct Link {
    settings: LinkSettings,
    keypair: StaticKeypair,
    /// Devices added and not yet heard from, by key.
    awaiting: Mutex<HashMap<[u8; KEY_LEN], Awaiting>>,
    conns: Mutex<HashMap<u64, OpenConn>>,
    next_conn: AtomicU64,
    /// Serializes writes to `auth.toml`.
    auth_lock: Mutex<()>,
    /// The addresses bound, as bound (a port 0 replaced by the OS's).
    bound: Mutex<Vec<String>>,
    /// The VPN, `None` with `--vpn none`.
    vpn: Option<Arc<dyn Vpn>>,
    machine: Arc<dyn Machine>,
    /// What the VPN's set-up found; held while it runs.
    reach: tokio::sync::Mutex<Reach>,
    /// Whether "not running" was logged.
    not_running_logged: AtomicBool,
    /// The `flankr pair` run in pair mode.
    pair_run: Mutex<Option<PairRun>>,
}

/// What `link.pair_begin` gives: the result to answer with, and the run's
/// events for its connection.
pub struct Begun {
    pub result: LinkPairBeginResult,
    pub extra: PairBeginExtra,
    pub events: tokio::sync::mpsc::UnboundedReceiver<PairEvent>,
}

/// A device name: 1 to 64 of ASCII letters, digits, `.`, `_` and `-`.
pub fn valid_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

impl Link {
    /// Load (or create) the server key. Refuses a key file that others may
    /// read.
    pub fn open(settings: LinkSettings) -> Result<Arc<Link>, String> {
        let vpn = settings.vpn.open();
        Link::open_with(settings, vpn, Arc::new(ThisMachine))
    }

    /// [`Link::open`] with this VPN (`None`: none, whatever
    /// `settings.vpn` says) and machine, as tests give them.
    pub fn open_with(
        settings: LinkSettings,
        vpn: Option<Arc<dyn Vpn>>,
        machine: Arc<dyn Machine>,
    ) -> Result<Arc<Link>, String> {
        let keypair = keyfile::load_or_create(&settings.key_path)?;
        Ok(Arc::new(Link {
            settings,
            keypair,
            awaiting: Mutex::new(HashMap::new()),
            conns: Mutex::new(HashMap::new()),
            next_conn: AtomicU64::new(1),
            auth_lock: Mutex::new(()),
            bound: Mutex::new(Vec::new()),
            vpn,
            machine,
            reach: tokio::sync::Mutex::new(Reach::Unknown),
            not_running_logged: AtomicBool::new(false),
            pair_run: Mutex::new(None),
        }))
    }

    pub fn settings(&self) -> &LinkSettings {
        &self.settings
    }

    /// The server's static public key.
    pub fn public_key(&self) -> [u8; KEY_LEN] {
        self.keypair.public
    }

    /// The paired clients, as `auth.toml` says now.
    pub fn auth(&self) -> Result<AuthFile, String> {
        AuthFile::load(&self.settings.auth_path)
    }

    /// Bind the listeners: `listen`, else `127.0.0.1:7347`. Then sets up
    /// the VPN's forward (decisions 4 to 6), and logs when only this
    /// machine can reach the link.
    pub async fn bind(&self) -> Result<Vec<TcpListener>, String> {
        let listen = self
            .settings
            .listen
            .clone()
            .unwrap_or_else(|| vec![DEFAULT_LISTEN.to_string()]);
        let mut listeners = Vec::new();
        let mut bound = Vec::new();
        for addr in &listen {
            let addr = normalize_listen(addr);
            let listener = TcpListener::bind(&addr)
                .await
                .map_err(|e| format!("link: bind {addr}: {e}"))?;
            // Port 0 asks the OS for one; advertise the one it gave.
            let advertised = match listener.local_addr() {
                Ok(local) if addr.ends_with(":0") => local.to_string(),
                _ => addr.clone(),
            };
            log::info!("Link listening on {advertised}");
            bound.push(advertised);
            listeners.push(listener);
        }
        *self.bound.lock().unwrap() = bound;
        if self.vpn.is_none() {
            log::info!("[link] vpn: none");
        }
        self.set_up_vpn().await;
        if let Err(only_here) = self.offer_addresses().await {
            log::warn!("[link] {}", only_here.replace('\n', " "));
        }
        Ok(listeners)
    }

    fn bound(&self) -> Vec<String> {
        self.bound.lock().unwrap().clone()
    }

    /// Decisions 4 and 6: forward the VPN's port to the first
    /// `127.0.0.1` address bound, unless it already is.
    async fn set_up_vpn(&self) {
        let Some(vpn) = &self.vpn else {
            return;
        };
        let mut reach = self.reach.lock().await;
        let name = vpn.name();
        let bound = self.bound();
        let Some(port) = forward_port(&bound) else {
            log::info!(
                "[link] {name}: no address on 127.0.0.1 among {}; no forward is made",
                bound.join(", ")
            );
            *reach = Reach::NoLoopback;
            return;
        };
        let running = vpn.running().await.unwrap_or_else(|e| {
            log::warn!("[link] {name}: {e}");
            false
        });
        if !running {
            if !self.not_running_logged.swap(true, Ordering::Relaxed) {
                log::info!(
                    "[link] {name}: not running; the link is on {} only",
                    bound.join(", ")
                );
            }
            *reach = Reach::NotRunning;
            return;
        }
        let address = vpn.address(port).await.unwrap_or_else(|e| {
            log::warn!("[link] {name}: {e}");
            None
        });
        let shown = address
            .as_ref()
            .map_or_else(|| format!("this machine:{port}"), VpnAddress::to_string);
        let command = vpn.forward_command(port);
        let failed = match vpn.forwards(port).await {
            Ok(true) => {
                log::info!("[link] {name}: {shown} already forwards to the link");
                None
            }
            Ok(false) => match vpn.forward(port).await {
                Ok(()) => {
                    log::info!("[link] {name}: forwarding {shown} to the link");
                    None
                }
                Err(e) => Some(format!(
                    "{} could not forward {shown} to the link: {e}; to do it by hand, run: {command}",
                    title(name)
                )),
            },
            Err(e) => Some(format!(
                "{} could not say whether it forwards {shown} to the link: {e}; to forward it by hand, run: {command}",
                title(name)
            )),
        };
        *reach = match (failed, address) {
            (Some(failed), _) => {
                log::warn!("[link] {name}: {failed}");
                Reach::Failed(failed)
            }
            (None, Some(address)) => Reach::Forwarded(address),
            (None, None) => Reach::NoName,
        };
    }

    /// The addresses a pairing offer carries now, or, when only this
    /// machine could reach every one, why (the 23:53 EST amendment).
    async fn offer_addresses(&self) -> Result<Vec<String>, String> {
        let bound = self.bound();
        let reach = self.reach.lock().await.clone();
        let vpn_address = match &reach {
            Reach::Forwarded(address) => Some(address.to_string()),
            _ => None,
        };
        let reachable = bound.iter().any(|a| !is_loopback(a));
        let machine = Arc::clone(&self.machine);
        let wants_name = self.vpn.is_none() && reachable;
        let wants_interfaces = bound.iter().any(|a| is_unspecified(a));
        let (host_name, interfaces) = tokio::task::spawn_blocking(move || {
            (
                wants_name.then(|| machine.host_name()).flatten(),
                if wants_interfaces {
                    machine.interface_addresses()
                } else {
                    Vec::new()
                },
            )
        })
        .await
        .unwrap_or_default();
        let addresses = offer_addresses(
            &bound,
            vpn_address.as_deref(),
            host_name.as_deref(),
            &interfaces,
        );
        if addresses.iter().any(|a| !is_loopback(a)) {
            return Ok(addresses);
        }
        let port = forward_port(&bound).unwrap_or(DEFAULT_PORT);
        let hint = match (&self.vpn, &reach) {
            (None, _) => format!(
                "Bind an address other machines can reach: run flankr with --vpn none --bind 0.0.0.0:{port} to use this network, or without --vpn none to use Tailscale"
            ),
            (Some(_), Reach::Failed(failed)) => failed.clone(),
            (Some(vpn), Reach::NoName) => format!(
                "{} forwards the link but did not give this machine's name; run flankr with --vpn none --bind 0.0.0.0:{port} to use this network",
                title(vpn.name())
            ),
            (Some(_), Reach::NoLoopback) => {
                "No network interface has an address to offer".to_string()
            }
            (Some(vpn), _) => format!(
                "{} is not running; start it, or run flankr with --vpn none --bind 0.0.0.0:{port} to use this network",
                title(vpn.name())
            ),
        };
        let shown = if addresses.is_empty() {
            bound.join(", ")
        } else {
            addresses.join(", ")
        };
        Err(format!(
            "{ONLY_THIS_MACHINE} ({shown}): a phone can't connect to it.\n{hint}"
        ))
    }

    /// [`Link::offer_addresses`], or every loopback address when
    /// `loopback_offers`.
    async fn offer(&self) -> Result<Vec<String>, String> {
        match self.offer_addresses().await {
            Err(_) if self.settings.loopback_offers => Ok(self.bound()),
            other => other,
        }
    }

    /// `link.pair_begin` from the daemon-socket connection `owner`: the
    /// pairing URL and string, with the server's public key, its addresses
    /// and a fresh offer, and pair mode for `owner` until the offer
    /// expires or `owner` closes (plan 20261028_01, the 14:37 and 14:40
    /// EDT amendments).
    pub async fn pair_begin(
        self: &Arc<Self>,
        owner: u64,
        params: PairBeginParams,
    ) -> Result<Begun, String> {
        let secs = pair_mode::timeout_secs(params.timeout_secs)?;
        if self.vpn.is_some() && !matches!(*self.reach.lock().await, Reach::Forwarded(_)) {
            // The VPN may have come up since the start.
            self.set_up_vpn().await;
        }
        let addresses = self.offer().await?;
        let warnings = Vec::new();
        let offer = generate_offer().map_err(|e| format!("pairing offer: {e}"))?;
        let payload = PairingPayload {
            server_key: self.keypair.public,
            addresses: addresses.clone(),
            offer: Some(offer),
        };
        let pairing = payload.encode().map_err(|e| e.to_string())?;
        let url = payload.url().map_err(|e| e.to_string())?;
        let events = self.start_run(owner, offer, secs);
        Ok(Begun {
            result: LinkPairBeginResult {
                url,
                pairing,
                addresses,
                warnings,
            },
            extra: PairBeginExtra {
                expires_in_secs: secs,
            },
            events,
        })
    }

    /// `link.pair_add`: checks `code` against the server's key and appends
    /// the device to `auth.toml`, under its own name or `name`, for
    /// `protocols` (by default the companion). A name in use is refused
    /// unless `replace`, which swaps the entry and closes the old key's
    /// connections; a key already paired under another name is refused.
    pub fn pair_add(&self, params: LinkPairAddParams) -> Result<Added, String> {
        let code = DeviceCode::decode(&params.code, &self.keypair.public).map_err(code_error)?;
        let name = match params.name.as_deref().map(str::trim) {
            Some(name) if !name.is_empty() => name.to_string(),
            _ => code.name.clone(),
        };
        let (result, done) =
            self.add_key_awaiting(&code.key, &name, params.protocols, params.replace)?;
        Ok(Added { result, done })
    }

    /// Adds `key` to `auth.toml` as `name`, for `protocols`.
    fn add_key(
        &self,
        key: &[u8; KEY_LEN],
        name: &str,
        protocols: Option<Vec<String>>,
        replace: bool,
    ) -> Result<LinkPairAddResult, String> {
        self.add_key_awaiting(key, name, protocols, replace)
            .map(|(result, _)| result)
    }

    /// [`Link::add_key`], awaiting the device's first `hello`.
    fn add_key_awaiting(
        &self,
        key: &[u8; KEY_LEN],
        name: &str,
        protocols: Option<Vec<String>>,
        replace: bool,
    ) -> Result<(LinkPairAddResult, oneshot::Receiver<LinkPairDone>), String> {
        let name = name.to_string();
        if !valid_name(&name) {
            return Err(format!(
                "'{name}' is not a valid name: use 1 to 64 letters, digits, '.', '_' or '-'"
            ));
        }
        let protocols = protocols.unwrap_or_else(|| vec![auth::COMPANION.to_string()]);
        let protocols: Vec<String> = protocols
            .iter()
            .map(|p| auth::short_protocol(p.trim()).to_ascii_lowercase())
            .collect();
        if protocols.is_empty() {
            return Err("give at least one protocol".to_string());
        }
        if let Some(bad) = protocols
            .iter()
            .find(|p| ![auth::COMPANION, auth::HARNESS].contains(&p.as_str()))
        {
            return Err(format!(
                "unknown protocol '{bad}'; use companion or harness"
            ));
        }

        let entry = AuthEntry::new(&name, key, protocols.clone());
        let replaced = {
            let _guard = self.auth_lock.lock().unwrap();
            auth::add(&self.settings.auth_path, entry, replace).map_err(|e| e.to_string())?
        };
        if let Some(old) = replaced.and_then(|old| old.public_key())
            && old != *key
        {
            self.close_where(|c| c.key == old, "its entry was replaced");
            self.awaiting.lock().unwrap().remove(&old);
        }
        let (done_tx, done) = oneshot::channel();
        self.awaiting.lock().unwrap().insert(
            *key,
            Awaiting {
                name: name.clone(),
                done: done_tx,
            },
        );
        log::info!("Added '{name}' ({}) to auth.toml", protocols.join(", "));
        Ok((LinkPairAddResult { name, protocols }, done))
    }

    /// A connection with `key` had its `hello` accepted. Returns whether
    /// it is the first since `link.pair_add` added the key, and if so tells
    /// the connection that added it.
    pub fn first_hello(&self, key: &[u8; KEY_LEN]) -> bool {
        let Some(awaiting) = self.awaiting.lock().unwrap().remove(key) else {
            return false;
        };
        log::info!("'{}' connected for the first time", awaiting.name);
        let _ = awaiting.done.send(LinkPairDone {
            name: awaiting.name,
            paired: true,
            error: None,
        });
        true
    }

    /// `link.unpair` (plan 20261024_02, decision 6): removes the entry with
    /// `key` from `auth.toml`, so the key no longer connects. Its open
    /// connections are closed by [`Link::close_key`], once the answer is on
    /// its way. Returns the name it was paired as.
    pub fn unpair(&self, key: &[u8; KEY_LEN]) -> Result<Option<String>, String> {
        let removed = {
            let _guard = self.auth_lock.lock().unwrap();
            auth::remove(&self.settings.auth_path, key)?
        };
        self.awaiting.lock().unwrap().remove(key);
        if let Some(entry) = &removed {
            log::info!("Removed '{}' from auth.toml: it unpaired", entry.name);
        }
        Ok(removed.map(|e| e.name))
    }

    /// Close every open connection with `key`.
    pub fn close_key(&self, key: &[u8; KEY_LEN], why: &str) {
        self.close_where(|c| &c.key == key, why);
    }

    fn register(&self, key: [u8; KEY_LEN], name: &str) -> (u64, CancellationToken) {
        let id = self.next_conn.fetch_add(1, Ordering::Relaxed);
        let close = CancellationToken::new();
        self.conns.lock().unwrap().insert(
            id,
            OpenConn {
                key,
                name: name.to_string(),
                close: close.clone(),
            },
        );
        (id, close)
    }

    fn unregister(&self, id: u64) {
        self.conns.lock().unwrap().remove(&id);
    }

    fn close_where(&self, mut pred: impl FnMut(&OpenConn) -> bool, why: &str) {
        for conn in self.conns.lock().unwrap().values() {
            if pred(conn) && !conn.close.is_cancelled() {
                log::info!("Closing the link connection of '{}': {why}", conn.name);
                conn.close.cancel();
            }
        }
    }

    /// Close open connections whose key is no longer in `auth.toml`. A file
    /// that cannot be read closes them all.
    pub fn check_revocations(&self) {
        match self.auth() {
            Ok(file) => self.close_where(|c| file.find_key(&c.key).is_none(), "its entry is gone"),
            Err(e) => {
                log::error!("auth.toml: {e}; closing every link connection");
                self.close_where(|_| true, "auth.toml is unreadable");
            }
        }
    }

    /// How many link connections are open.
    pub fn open_connections(&self) -> usize {
        self.conns.lock().unwrap().len()
    }
}

/// Serve link connections on `listeners`, and run the revocation check.
pub fn spawn(daemon: Arc<Daemon>, listeners: Vec<TcpListener>) {
    let Some(link) = daemon.link.clone() else {
        return;
    };
    // On each change to auth.toml the file watcher reports (plan
    // 20261027_05, decision 2), and every `revoke_interval` as a slow
    // fallback in case the watcher fails.
    let mut changes = daemon.file_watcher().and_then(|files| {
        files
            .subscribe(std::slice::from_ref(&link.settings.auth_path))
            .inspect_err(|e| {
                log::warn!(
                    "{} can't be watched ({e}); revocations are checked every {:?}",
                    link.settings.auth_path.display(),
                    link.settings.revoke_interval
                )
            })
            .ok()
    });
    let checker = Arc::clone(&link);
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(checker.settings.revoke_interval);
        tick.tick().await;
        loop {
            match &mut changes {
                Some(sub) => {
                    tokio::select! {
                        _ = tick.tick() => {}
                        alive = sub.changed() => {
                            if !alive {
                                changes = None;
                            }
                        }
                    }
                }
                None => {
                    tick.tick().await;
                }
            }
            checker.check_revocations();
        }
    });
    for listener in listeners {
        let daemon = Arc::clone(&daemon);
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, peer)) => {
                        tokio::spawn(handle(Arc::clone(&daemon), stream, peer));
                    }
                    Err(e) => {
                        log::warn!("link accept failed: {e}");
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }
        });
    }
}

/// Whether `bytes` is a `hello` request.
fn is_hello(bytes: &[u8]) -> bool {
    matches!(
        serde_json::from_slice::<Incoming>(bytes),
        Ok(Incoming::Request(Request { ref method, .. })) if method == libflankr_proto::common::methods::HELLO
    )
}

async fn handle(daemon: Arc<Daemon>, stream: TcpStream, peer: SocketAddr) {
    let Some(link) = daemon.link.clone() else {
        return;
    };
    if let Err(e) = socket2::SockRef::from(&stream).set_keepalive(true) {
        log::warn!("link {peer}: SO_KEEPALIVE: {e}");
    }

    let mut known: Option<AuthEntry> = None;
    let accepted = tokio::time::timeout(
        HANDSHAKE_TIMEOUT,
        accept_incoming(stream, &link.keypair, |key| {
            known = match link.auth() {
                Ok(file) => file.find_key(key).cloned(),
                Err(e) => {
                    log::error!("auth.toml: {e}");
                    None
                }
            };
            known.is_some()
        }),
    )
    .await;
    let accepted = match accepted {
        Ok(Ok(LinkIncoming::Link(accepted))) => accepted,
        Ok(Ok(LinkIncoming::Pair(incoming))) => {
            link.pair_incoming(*incoming, &peer.to_string()).await;
            return;
        }
        Ok(Err(e)) => {
            log::info!("Refused a link connection from {peer}: {e}");
            return;
        }
        Err(_) => {
            log::info!("Refused a link connection from {peer}: the handshake timed out");
            return;
        }
    };
    let Some(entry) = known else {
        return;
    };

    // Only hello may ride in a handshake: anything else could be a replay.
    if let Some(first) = &accepted.first_message
        && !is_hello(first)
    {
        log::warn!(
            "Closed the link connection of '{}' from {peer}: its first message was not hello",
            entry.name
        );
        return;
    }

    let (conn_id, close) = link.register(accepted.peer_key, &entry.name);
    log::info!("Link connection from '{}' ({peer})", entry.name);
    super::conn::handle_link(
        Arc::clone(&daemon),
        accepted.stream,
        accepted.first_message,
        entry,
        accepted.peer_key,
        close,
    )
    .await;
    link.unregister(conn_id);
}

/// What `link.pair_add` says of a device code it refused.
fn code_error(e: PairingStringError) -> String {
    match e {
        PairingStringError::BadChecksum => {
            "that code does not match this server: it was made for another server, or mistyped"
                .to_string()
        }
        PairingStringError::BadPrefix => {
            "that is not a device code: a device code starts with flankrkey1".to_string()
        }
        PairingStringError::BadName => {
            "the device's name in that code is not valid; pass --name".to_string()
        }
        other => format!("that is not a valid device code: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listen_addresses_get_the_default_port() {
        assert_eq!(normalize_listen("127.0.0.1"), "127.0.0.1:7347");
        assert_eq!(normalize_listen("100.64.0.1:9000"), "100.64.0.1:9000");
        assert_eq!(normalize_listen("::1"), "[::1]:7347");
        assert_eq!(normalize_listen("[::1]:80"), "[::1]:80");
        assert_eq!(normalize_listen("mini.ts.net"), "mini.ts.net:7347");
        assert_eq!(normalize_listen("mini.ts.net:1"), "mini.ts.net:1");
    }

    #[test]
    fn names() {
        assert!(valid_name("s23"));
        assert!(valid_name("t3-code.mini_1"));
        assert!(!valid_name(""));
        assert!(!valid_name("has space"));
        assert!(!valid_name(&"x".repeat(65)));
    }

    #[test]
    fn settings_default_to_the_config_dir_and_an_empty_list_is_off() {
        let s = LinkSettings::from_config(&LinkConfig::default()).unwrap();
        assert!(s.key_path.ends_with(".config/flankr/link_key"));
        assert!(s.auth_path.ends_with(".config/flankr/auth.toml"));
        assert_eq!(s.revoke_interval, DEFAULT_REVOKE_INTERVAL);
        assert!(s.enabled());
        assert_eq!(s.vpn, VpnChoice::Tailscale);
        let off = LinkSettings::from_config(&LinkConfig {
            listen: Some(vec![]),
            ..Default::default()
        })
        .unwrap();
        assert!(!off.enabled());
    }

    #[test]
    fn the_link_does_not_open_with_a_key_at_0644() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let settings = LinkSettings {
            key_path: dir.path().join("link_key"),
            auth_path: dir.path().join("auth.toml"),
            listen: Some(vec!["127.0.0.1:0".to_string()]),
            revoke_interval: DEFAULT_REVOKE_INTERVAL,
            vpn: VpnChoice::None,
            loopback_offers: true,
        };
        let first = Link::open(settings.clone()).unwrap().public_key();
        assert_eq!(Link::open(settings.clone()).unwrap().public_key(), first);
        std::fs::set_permissions(&settings.key_path, std::fs::Permissions::from_mode(0o644))
            .unwrap();
        let err = Link::open(settings).err().unwrap();
        assert!(err.contains("0644"), "{err}");
    }

    #[test]
    fn only_hello_is_a_first_message() {
        assert!(is_hello(
            br#"{"jsonrpc":"2.0","id":1,"method":"hello","params":{}}"#
        ));
        assert!(!is_hello(
            br#"{"jsonrpc":"2.0","id":1,"method":"agents.send","params":{}}"#
        ));
        assert!(!is_hello(b"not json"));
    }

    // The link VPN plan (20261102_03), decisions 4 to 6 and the 23:53 EST
    // amendment, with the fake VPN and machine: nothing real is run.

    use self::machine::FakeMachine;
    use self::vpn::fake::{FakeCall, FakeVpn};

    fn strings(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn ips(list: &[&str]) -> Vec<IpAddr> {
        list.iter().map(|s| s.parse().unwrap()).collect()
    }

    #[test]
    fn every_address_binding_offers_the_interfaces() {
        let interfaces = ips(&["192.168.1.20", "fd7a:115c:a1e0::5", "10.0.0.7"]);
        assert_eq!(
            offer_addresses(&strings(&["0.0.0.0:7347"]), None, None, &interfaces),
            vec!["192.168.1.20:7347", "10.0.0.7:7347"]
        );
        assert_eq!(
            offer_addresses(&strings(&["[::]:7347"]), None, None, &interfaces),
            vec![
                "192.168.1.20:7347",
                "[fd7a:115c:a1e0::5]:7347",
                "10.0.0.7:7347"
            ]
        );
    }

    #[test]
    fn the_name_comes_first_with_no_vpn_and_the_vpn_first_with_one() {
        let interfaces = ips(&["192.168.1.20"]);
        assert_eq!(
            offer_addresses(
                &strings(&["127.0.0.1:7347", "0.0.0.0:7400"]),
                None,
                Some("mini.local"),
                &interfaces
            ),
            vec!["mini.local:7400", "127.0.0.1:7347", "192.168.1.20:7400"]
        );
        assert_eq!(
            offer_addresses(
                &strings(&["192.168.1.20:7347"]),
                None,
                Some("mini.local"),
                &[]
            ),
            vec!["mini.local:7347", "192.168.1.20:7347"]
        );
        assert_eq!(
            offer_addresses(
                &strings(&["127.0.0.1:7347"]),
                Some("mini.ts.net:7347"),
                None,
                &[]
            ),
            vec!["mini.ts.net:7347", "127.0.0.1:7347"]
        );
        // Loopback alone gets no name.
        assert_eq!(
            offer_addresses(&strings(&["127.0.0.1:7347"]), None, Some("mini.local"), &[]),
            vec!["127.0.0.1:7347"]
        );
    }

    #[test]
    fn loopback_and_the_forward_port() {
        assert!(is_loopback("127.0.0.1:7347"));
        assert!(is_loopback("127.0.0.2:7347"));
        assert!(is_loopback("[::1]:7347"));
        assert!(is_loopback("localhost:7347"));
        assert!(!is_loopback("0.0.0.0:7347"));
        assert!(!is_loopback("mini.local:7347"));
        assert_eq!(
            forward_port(&strings(&["[::1]:9", "0.0.0.0:7400", "127.0.0.1:7401"])),
            Some(7401)
        );
        assert_eq!(
            forward_port(&strings(&["0.0.0.0:7347", "[::1]:7347"])),
            None
        );
    }

    #[test]
    fn bind_and_vpn_flags_replace_the_config() {
        let base = LinkSettings::from_config(&LinkConfig {
            vpn: Some("none".into()),
            listen: Some(strings(&["127.0.0.1:7347"])),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(base.vpn, VpnChoice::None);
        let s = base
            .clone()
            .with_flags(Some("tailscale"), &strings(&["0.0.0.0", "[::]:7400"]))
            .unwrap();
        assert_eq!(s.vpn, VpnChoice::Tailscale);
        assert_eq!(s.listen, Some(strings(&["0.0.0.0:7347", "[::]:7400"])));
        assert_eq!(base.clone().with_flags(None, &[]).unwrap(), base);
        assert!(
            base.clone()
                .with_flags(None, &strings(&["mini.local:7347"]))
                .unwrap_err()
                .starts_with("--bind 'mini.local:7347' is not an address")
        );
        assert!(base.with_flags(Some("wg"), &[]).is_err());
        assert!(
            LinkSettings::from_config(&LinkConfig {
                vpn: Some("wg".into()),
                ..Default::default()
            })
            .is_err()
        );
        assert!(VpnChoice::None.open().is_none());
    }

    struct Rig {
        _dir: tempfile::TempDir,
        link: Arc<Link>,
        fake: Option<Arc<FakeVpn>>,
        port: u16,
    }

    async fn rig(listen: &[&str], fake: Option<FakeVpn>) -> Rig {
        let dir = tempfile::tempdir().unwrap();
        let settings = LinkSettings {
            key_path: dir.path().join("link_key"),
            auth_path: dir.path().join("auth.toml"),
            listen: Some(strings(listen)),
            revoke_interval: DEFAULT_REVOKE_INTERVAL,
            vpn: VpnChoice::None,
            loopback_offers: false,
        };
        let fake = fake.map(Arc::new);
        let vpn = fake.clone().map(|f| f as Arc<dyn Vpn>);
        let machine = Arc::new(FakeMachine {
            host_name: Some("mini.local".into()),
            addresses: ips(&["192.168.1.20"]),
        });
        let link = Link::open_with(settings, vpn, machine).unwrap();
        let listeners = link.bind().await.unwrap();
        let port = listeners[0].local_addr().unwrap().port();
        Rig {
            _dir: dir,
            link,
            fake,
            port,
        }
    }

    async fn begin(link: &Arc<Link>) -> Result<Vec<String>, String> {
        link.pair_begin(1, PairBeginParams { timeout_secs: None })
            .await
            .map(|b| b.result.addresses)
    }

    #[tokio::test]
    async fn a_missing_forward_is_made_once_and_offered_first() {
        let r = rig(&["127.0.0.1:0"], Some(FakeVpn::running_as("mini.ts.net"))).await;
        let fake = r.fake.as_ref().unwrap();
        assert_eq!(fake.forward_calls(), 1);
        assert!(fake.calls().contains(&FakeCall::Forward(r.port)));
        let want = vec![
            format!("mini.ts.net:{}", r.port),
            format!("127.0.0.1:{}", r.port),
        ];
        assert_eq!(begin(&r.link).await.unwrap(), want);
        assert_eq!(begin(&r.link).await.unwrap(), want);
        assert_eq!(fake.forward_calls(), 1, "{:?}", fake.calls());
    }

    #[tokio::test]
    async fn a_forward_already_there_is_not_made_again() {
        // The port is the OS's, so the fake learns it after binding.
        let r = rig(&["127.0.0.1:0"], Some(FakeVpn::running_as("mini.ts.net"))).await;
        let fake = r.fake.as_ref().unwrap();
        let before = fake.forward_calls();
        // A fresh link on the same port, the forward now in place.
        let again = Arc::new(FakeVpn::running_as("mini.ts.net").with_forward(r.port));
        let dir = tempfile::tempdir().unwrap();
        let link = Link::open_with(
            LinkSettings {
                key_path: dir.path().join("link_key"),
                auth_path: dir.path().join("auth.toml"),
                listen: None,
                revoke_interval: DEFAULT_REVOKE_INTERVAL,
                vpn: VpnChoice::None,
                loopback_offers: false,
            },
            Some(Arc::clone(&again) as Arc<dyn Vpn>),
            Arc::new(FakeMachine::default()),
        )
        .unwrap();
        // Not bound: set up as `bind` would, on the first link's port.
        *link.bound.lock().unwrap() = vec![format!("127.0.0.1:{}", r.port)];
        link.set_up_vpn().await;
        assert_eq!(again.forward_calls(), 0, "{:?}", again.calls());
        assert_eq!(
            begin(&link).await.unwrap(),
            vec![
                format!("mini.ts.net:{}", r.port),
                format!("127.0.0.1:{}", r.port)
            ]
        );
        assert_eq!(again.forward_calls(), 0);
        assert_eq!(fake.forward_calls(), before);
    }

    #[tokio::test]
    async fn not_running_gives_no_offer_and_says_why() {
        let stopped = FakeVpn::running_as("mini.ts.net");
        stopped.set_running(false);
        let r = rig(&["127.0.0.1:0"], Some(stopped)).await;
        let fake = r.fake.as_ref().unwrap();
        let err = begin(&r.link).await.unwrap_err();
        assert_eq!(
            err,
            format!(
                "flankr is only reachable from this machine (127.0.0.1:{p}): a phone can't connect to it.\n\
                 fake is not running; start it, or run flankr with --vpn none --bind 0.0.0.0:{p} to use this network",
                p = r.port
            )
        );
        assert_eq!(fake.forward_calls(), 0);
        // Started later: the next pairing forwards and offers it.
        fake.set_running(true);
        assert_eq!(
            begin(&r.link).await.unwrap(),
            vec![
                format!("mini.ts.net:{}", r.port),
                format!("127.0.0.1:{}", r.port)
            ]
        );
        assert_eq!(fake.forward_calls(), 1);
    }

    #[tokio::test]
    async fn a_failed_forward_names_the_error_and_the_command() {
        let r = rig(
            &["127.0.0.1:0"],
            Some(FakeVpn::running_as("mini.ts.net").failing_forward("Serve is not enabled")),
        )
        .await;
        let err = begin(&r.link).await.unwrap_err();
        assert!(err.starts_with(ONLY_THIS_MACHINE), "{err}");
        let hint = err.lines().nth(1).unwrap();
        assert!(hint.contains("Serve is not enabled"), "{hint}");
        assert!(
            hint.ends_with(&format!("to do it by hand, run: fake forward {}", r.port)),
            "{hint}"
        );
    }

    #[tokio::test]
    async fn no_127_0_0_1_binding_makes_no_forward() {
        let r = rig(&["[::1]:0"], Some(FakeVpn::running_as("mini.ts.net"))).await;
        assert!(r.fake.as_ref().unwrap().calls().is_empty());
        assert!(
            begin(&r.link)
                .await
                .unwrap_err()
                .starts_with(ONLY_THIS_MACHINE)
        );
    }

    #[tokio::test]
    async fn no_vpn_on_loopback_gives_no_offer() {
        let r = rig(&["127.0.0.1:0"], None).await;
        let err = begin(&r.link).await.unwrap_err();
        assert_eq!(
            err,
            format!(
                "flankr is only reachable from this machine (127.0.0.1:{p}): a phone can't connect to it.\n\
                 Bind an address other machines can reach: run flankr with --vpn none --bind 0.0.0.0:{p} to use this network, or without --vpn none to use Tailscale",
                p = r.port
            )
        );
    }
}
