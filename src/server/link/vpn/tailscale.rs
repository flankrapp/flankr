//! Tailscale, through its CLI (plan 20261102_03, decision 2; the reading
//! moved here from `link/tailscale.rs`).
//!
//! * The CLI is `tailscale` on `PATH`, else Homebrew's or `/usr/local`'s,
//!   else the macOS app's, in `/Applications` or `~/Applications`
//!   ([`candidates`]): a daemon started over SSH or by launchd may not have
//!   Homebrew on its `PATH`.
//! * `running` and the machine's name: `tailscale status --json`
//!   (`BackendState == "Running"`, `Self.DNSName` without its final dot).
//! * `forwards`: `tailscale serve status --json` has a TCP handler for the
//!   port to `127.0.0.1:<port>`.
//! * `forward`: `tailscale serve --bg --tcp <port> tcp://127.0.0.1:<port>`.
//! * Each command has [`TIMEOUT`]; not answering in time is an error.
//!
//! The provider is `libhardline::vpn::tailscale`'s, its commands run by
//! `libhardline::tokio::vpn::drive`
//! (docs/design/20261119_02_LIBHARDLINE_EXTRACTION_PLAN.md, step 9); this
//! finds the CLI and answers flankr's [`Vpn`].

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use libhardline::tokio::vpn::{NoServices, drive};

use super::{SERVICE_PREFIX, Vpn, VpnAddress, VpnError};

pub use libhardline::vpn::tailscale::{
    APP_CLI, INSTALLED_CLIS, TIMEOUT, candidates, forward_present, is_running, self_dns_name,
};

/// Tailscale through its CLI.
#[derive(Debug, Clone)]
pub struct Tailscale {
    vpn: libhardline::vpn::Vpn,
}

impl Tailscale {
    /// The first of [`candidates`] that is there, else none.
    pub fn find() -> Tailscale {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let program = candidates(std::env::var_os("PATH").as_deref(), home.as_deref())
            .into_iter()
            .find(|p| is_executable(p));
        Tailscale::new(program, TIMEOUT)
    }

    /// The CLI at `program`, each command limited to `timeout`.
    pub fn at(program: impl Into<PathBuf>, timeout: Duration) -> Tailscale {
        Tailscale::new(Some(program.into()), timeout)
    }

    fn new(program: Option<PathBuf>, timeout: Duration) -> Tailscale {
        let vpn = libhardline::vpn::Vpn::builder()
            .service_prefix(SERVICE_PREFIX)
            .tailscale(program)
            .timeout(timeout)
            .build()
            .expect("flankr's service prefix is valid and Tailscale needs nothing else");
        Tailscale { vpn }
    }

    fn cli(&self) -> &libhardline::vpn::tailscale::Tailscale {
        self.vpn.tailscale().expect("built as Tailscale")
    }

    /// The CLI it runs, if it found one.
    pub fn program(&self) -> Option<&Path> {
        self.cli().program()
    }

    /// The arguments of `forward` for `port`.
    pub fn forward_args(port: u16) -> Vec<String> {
        libhardline::vpn::tailscale::Tailscale::forward_args(port)
    }
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[async_trait]
impl Vpn for Tailscale {
    fn name(&self) -> &'static str {
        "tailscale"
    }

    async fn running(&self) -> Result<bool, VpnError> {
        // Tailscale's running does not depend on the port.
        drive(self.vpn.running(0), &NoServices).await
    }

    async fn address(&self, port: u16) -> Result<Option<VpnAddress>, VpnError> {
        drive(self.vpn.address(port), &NoServices).await
    }

    async fn forwards(&self, port: u16) -> Result<bool, VpnError> {
        drive(self.vpn.forwards(port), &NoServices).await
    }

    async fn forward(&self, port: u16) -> Result<(), VpnError> {
        drive(self.vpn.forward(port), &NoServices).await
    }

    fn forward_command(&self, port: u16) -> String {
        self.vpn.forward_command(port)
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn the_cli_is_looked_for_on_path_then_homebrew_then_the_app() {
        let path = std::ffi::OsString::from("/a:/b");
        let got = candidates(Some(&path), Some(Path::new("/Users/u")));
        let want: Vec<PathBuf> = [
            "/a/tailscale",
            "/b/tailscale",
            "/opt/homebrew/bin/tailscale",
            "/usr/local/bin/tailscale",
            APP_CLI,
            "/Users/u/Applications/Tailscale.app/Contents/MacOS/Tailscale",
        ]
        .iter()
        .map(PathBuf::from)
        .collect();
        assert_eq!(got, want);
        // No PATH and no home: the installed places and the app's.
        assert_eq!(candidates(None, None).len(), 3);
    }

    use super::*;
    use serde_json::json;

    // JSON written by hand, in the shape `tailscale status --json` and
    // `tailscale serve status --json` give; no real Tailscale is run.

    #[test]
    fn a_running_status_gives_the_name_without_its_dot() {
        let status = json!({
            "Version": "1.76.1",
            "BackendState": "Running",
            "Self": {"DNSName": "mini.tail1a2b3c.ts.net.", "HostName": "mini"}
        });
        assert!(is_running(&status));
        assert_eq!(
            self_dns_name(&status).as_deref(),
            Some("mini.tail1a2b3c.ts.net")
        );
    }

    #[test]
    fn a_stopped_status_is_not_running() {
        let status = json!({"BackendState": "Stopped", "Self": {"DNSName": ""}});
        assert!(!is_running(&status));
        assert_eq!(self_dns_name(&status), None);
        assert!(!is_running(&json!({"BackendState": "NeedsLogin"})));
        assert!(!is_running(&json!({})));
        assert_eq!(self_dns_name(&json!({})), None);
    }

    #[test]
    fn serve_status_with_and_without_the_tcp_handler() {
        assert!(!forward_present(&json!({}), 7347));
        assert!(!forward_present(
            &json!({
                "TCP": {"443": {"HTTPS": true}},
                "Web": {"host:443": {"Handlers": {"/": {"Proxy": "http://127.0.0.1:8787"}}}}
            }),
            7347
        ));
        assert!(forward_present(
            &json!({"TCP": {"443": {"HTTPS": true}, "7347": {"TCPForward": "127.0.0.1:7347"}}}),
            7347
        ));
        // Nested, as a foreground session is; with the scheme.
        assert!(forward_present(
            &json!({"Foreground": {"abc": {"TCP": {"7347": {"TCPForward": "tcp://127.0.0.1:7347"}}}}}),
            7347
        ));
        // A forward elsewhere is not the link's.
        assert!(!forward_present(
            &json!({"TCP": {"7347": {"TCPForward": "127.0.0.1:9999"}}}),
            7347
        ));
        // Another port.
        assert!(forward_present(
            &json!({"TCP": {"7400": {"TCPForward": "127.0.0.1:7400"}}}),
            7400
        ));
        assert!(!forward_present(
            &json!({"TCP": {"7400": {"TCPForward": "127.0.0.1:7400"}}}),
            7347
        ));
    }

    #[test]
    fn the_forward_command() {
        assert_eq!(
            Tailscale::forward_args(7347),
            vec!["serve", "--bg", "--tcp", "7347", "tcp://127.0.0.1:7347"]
        );
        let ts = Tailscale::at("/nonexistent/tailscale", TIMEOUT);
        assert_eq!(
            ts.forward_command(7347),
            "tailscale serve --bg --tcp 7347 tcp://127.0.0.1:7347"
        );
        assert_eq!(TIMEOUT, Duration::from_secs(10));
    }

    /// A stand-in CLI, a shell script in a temporary folder: never the
    /// real `tailscale`.
    fn script(dir: &Path, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("tailscale");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[tokio::test]
    async fn a_command_that_does_not_answer_in_time_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let ts = Tailscale::at(script(dir.path(), "sleep 5"), Duration::from_millis(200));
        let err = ts.running().await.unwrap_err();
        assert_eq!(err.command, "tailscale status --json");
        assert!(err.message.contains("did not finish"), "{err}");
        let err = ts.forwards(7347).await.unwrap_err();
        assert_eq!(err.command, "tailscale serve status --json");
        let err = ts.forward(7347).await.unwrap_err();
        assert_eq!(
            err.command,
            "tailscale serve --bg --tcp 7347 tcp://127.0.0.1:7347"
        );
        assert!(err.message.contains("did not finish"), "{err}");
    }

    #[tokio::test]
    async fn the_stand_in_cli_is_read_and_its_arguments_are_the_forward() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("args");
        let body = format!(
            r#"echo "$@" >> '{log}'
case "$1 $2" in
  "status --json") echo '{{"BackendState":"Running","Self":{{"DNSName":"mini.ts.net."}}}}' ;;
  "serve status") echo '{{}}' ;;
  "serve --bg") echo 'Serve is not enabled on your tailnet.' >&2; exit 1 ;;
esac"#,
            log = log.display()
        );
        let ts = Tailscale::at(script(dir.path(), &body), TIMEOUT);
        assert!(ts.running().await.unwrap());
        assert_eq!(
            ts.address(7347).await.unwrap(),
            Some(VpnAddress {
                host: "mini.ts.net".into(),
                port: 7347
            })
        );
        assert!(!ts.forwards(7347).await.unwrap());
        let err = ts.forward(7347).await.unwrap_err();
        assert_eq!(err.message, "Serve is not enabled on your tailnet.");
        let args = std::fs::read_to_string(&log).unwrap();
        assert!(
            args.lines()
                .any(|l| l == "serve --bg --tcp 7347 tcp://127.0.0.1:7347"),
            "{args}"
        );
    }

    #[tokio::test]
    async fn not_installed_is_not_running() {
        let ts = Tailscale::at("/nonexistent/flankr-test/tailscale", TIMEOUT);
        assert!(!ts.running().await.unwrap());
        assert_eq!(ts.address(7347).await.unwrap(), None);
        assert!(!ts.forwards(7347).await.unwrap());
        assert!(ts.forward(7347).await.is_err());
    }
}
