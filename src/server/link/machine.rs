//! What the link asks of this machine for a pairing offer with no VPN
//! (plan 20261102_03, decision 5 and the 23:53 EST amendment, item 2): its
//! network name and its interface addresses. Behind a trait, so tests do
//! not depend on the machine they run on.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// This machine's name and addresses on its networks.
pub trait Machine: Send + Sync {
    /// Its name on the local network: on macOS `<LocalHostName>.local`
    /// (`scutil --get LocalHostName`), elsewhere the host name.
    fn host_name(&self) -> Option<String>;
    /// Its interfaces' addresses that are up, not loopback and not IPv6
    /// link-local.
    fn interface_addresses(&self) -> Vec<IpAddr>;
}

/// The machine this runs on.
#[derive(Debug, Default, Clone, Copy)]
pub struct ThisMachine;

impl Machine for ThisMachine {
    fn host_name(&self) -> Option<String> {
        if cfg!(target_os = "macos") {
            let out = std::process::Command::new("/usr/sbin/scutil")
                .args(["--get", "LocalHostName"])
                .stdin(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .output()
                .ok()?;
            let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
            (out.status.success() && valid_host(&name)).then(|| format!("{name}.local"))
        } else {
            let mut buf = [0u8; 256];
            // SAFETY: `buf` is valid for its length; gethostname writes at
            // most that and we stop at the first NUL.
            let rc = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) };
            if rc != 0 {
                return None;
            }
            let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
            let name = String::from_utf8_lossy(&buf[..end]).trim().to_string();
            valid_host(&name).then_some(name)
        }
    }

    fn interface_addresses(&self) -> Vec<IpAddr> {
        let mut out = Vec::new();
        let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
        // SAFETY: getifaddrs fills `list` with a linked list we only read,
        // then free with freeifaddrs.
        if unsafe { libc::getifaddrs(&mut list) } != 0 {
            return out;
        }
        let mut cur = list;
        while !cur.is_null() {
            // SAFETY: `cur` is a node of the list getifaddrs gave.
            let ifa = unsafe { &*cur };
            cur = ifa.ifa_next;
            let flags = ifa.ifa_flags;
            if flags & (libc::IFF_UP as u32) == 0 || flags & (libc::IFF_LOOPBACK as u32) != 0 {
                continue;
            }
            if ifa.ifa_addr.is_null() {
                continue;
            }
            // SAFETY: a non-null `ifa_addr` points at a sockaddr whose
            // family says which struct it is.
            let family = i32::from(unsafe { (*ifa.ifa_addr).sa_family });
            let ip = if family == libc::AF_INET {
                // SAFETY: AF_INET: a sockaddr_in.
                let sin = unsafe { &*(ifa.ifa_addr as *const libc::sockaddr_in) };
                IpAddr::V4(Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr)))
            } else if family == libc::AF_INET6 {
                // SAFETY: AF_INET6: a sockaddr_in6.
                let sin6 = unsafe { &*(ifa.ifa_addr as *const libc::sockaddr_in6) };
                IpAddr::V6(Ipv6Addr::from(sin6.sin6_addr.s6_addr))
            } else {
                continue;
            };
            if usable(&ip) && !out.contains(&ip) {
                out.push(ip);
            }
        }
        // SAFETY: the list getifaddrs gave, freed once.
        unsafe { libc::freeifaddrs(list) };
        out
    }
}

/// Not loopback, not unspecified, and not IPv6 link-local (which needs a
/// scope a pairing address cannot carry).
pub fn usable(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => !v4.is_loopback() && !v4.is_unspecified(),
        IpAddr::V6(v6) => {
            !v6.is_loopback() && !v6.is_unspecified() && (v6.segments()[0] & 0xffc0) != 0xfe80
        }
    }
}

fn valid_host(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 253
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.'))
}

/// A machine for tests: the name and addresses it is given.
#[derive(Debug, Default, Clone)]
pub struct FakeMachine {
    pub host_name: Option<String>,
    pub addresses: Vec<IpAddr>,
}

impl Machine for FakeMachine {
    fn host_name(&self) -> Option<String> {
        self.host_name.clone()
    }

    fn interface_addresses(&self) -> Vec<IpAddr> {
        self.addresses.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_local_and_loopback_are_not_offered() {
        assert!(usable(&"192.168.1.20".parse().unwrap()));
        assert!(usable(&"fd7a:115c:a1e0::1".parse().unwrap()));
        assert!(!usable(&"127.0.0.1".parse().unwrap()));
        assert!(!usable(&"::1".parse().unwrap()));
        assert!(!usable(&"fe80::1".parse().unwrap()));
        assert!(!usable(&"0.0.0.0".parse().unwrap()));
    }

    #[test]
    fn host_names_are_checked() {
        assert!(valid_host("Timothys-MacBook-Pro"));
        assert!(!valid_host(""));
        assert!(!valid_host("has space"));
    }
}
