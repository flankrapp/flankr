//! A VPN for tests: it answers as told and records each call; it runs
//! nothing (plan 20261102_03, decision 9).

use std::sync::Mutex;

use async_trait::async_trait;

use super::{Vpn, VpnAddress, VpnError};

/// What [`FakeVpn`] was asked, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FakeCall {
    Running,
    Address(u16),
    Forwards(u16),
    Forward(u16),
}

#[derive(Debug, Default)]
struct State {
    running: bool,
    host: Option<String>,
    /// Ports forwarded to `127.0.0.1:<port>`.
    forwarded: Vec<u16>,
    /// What `forward` fails with, if anything.
    forward_error: Option<String>,
    calls: Vec<FakeCall>,
}

/// A fake VPN named `fake`.
#[derive(Debug, Default)]
pub struct FakeVpn {
    state: Mutex<State>,
}

impl FakeVpn {
    /// Not running.
    pub fn stopped() -> FakeVpn {
        FakeVpn::default()
    }

    /// Running as `host`, forwarding nothing yet.
    pub fn running_as(host: &str) -> FakeVpn {
        let fake = FakeVpn::default();
        {
            let mut s = fake.state.lock().unwrap();
            s.running = true;
            s.host = Some(host.to_string());
        }
        fake
    }

    /// Already forwarding `port`.
    pub fn with_forward(self, port: u16) -> FakeVpn {
        self.state.lock().unwrap().forwarded.push(port);
        self
    }

    /// `forward` fails with `message`.
    pub fn failing_forward(self, message: &str) -> FakeVpn {
        self.state.lock().unwrap().forward_error = Some(message.to_string());
        self
    }

    /// Start or stop it.
    pub fn set_running(&self, running: bool) {
        self.state.lock().unwrap().running = running;
    }

    /// Every call so far.
    pub fn calls(&self) -> Vec<FakeCall> {
        self.state.lock().unwrap().calls.clone()
    }

    /// How many times `forward` was called.
    pub fn forward_calls(&self) -> usize {
        self.calls()
            .iter()
            .filter(|c| matches!(c, FakeCall::Forward(_)))
            .count()
    }
}

#[async_trait]
impl Vpn for FakeVpn {
    fn name(&self) -> &'static str {
        "fake"
    }

    async fn running(&self) -> Result<bool, VpnError> {
        let mut s = self.state.lock().unwrap();
        s.calls.push(FakeCall::Running);
        Ok(s.running)
    }

    async fn address(&self, port: u16) -> Result<Option<VpnAddress>, VpnError> {
        let mut s = self.state.lock().unwrap();
        s.calls.push(FakeCall::Address(port));
        Ok(s.host
            .clone()
            .filter(|_| s.running)
            .map(|host| VpnAddress { host, port }))
    }

    async fn forwards(&self, port: u16) -> Result<bool, VpnError> {
        let mut s = self.state.lock().unwrap();
        s.calls.push(FakeCall::Forwards(port));
        Ok(s.forwarded.contains(&port))
    }

    async fn forward(&self, port: u16) -> Result<(), VpnError> {
        let mut s = self.state.lock().unwrap();
        s.calls.push(FakeCall::Forward(port));
        if let Some(message) = s.forward_error.clone() {
            return Err(VpnError {
                command: self.forward_command(port),
                message,
            });
        }
        s.forwarded.push(port);
        Ok(())
    }

    fn forward_command(&self, port: u16) -> String {
        format!("fake forward {port}")
    }
}
