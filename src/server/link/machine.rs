//! What the link asks of this machine for a pairing offer with no VPN
//! (plan 20261102_03, decision 5 and the 23:53 EST amendment, item 2): its
//! network name and its interface addresses. Behind a trait, so tests do
//! not depend on the machine they run on.
//!
//! The trait, [`usable`] and [`FakeMachine`] are
//! `libhardline::server::machine`'s, and [`ThisMachine`] is
//! `libhardline::tokio`'s (docs/design/20261119_02_LIBHARDLINE_EXTRACTION_PLAN.md,
//! step 9), moved there unchanged.

pub use libhardline::server::machine::{FakeMachine, Machine, usable, valid_host};
pub use libhardline::tokio::ThisMachine;

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
