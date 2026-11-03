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

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

use super::{Vpn, VpnAddress, VpnError};

/// How long each `tailscale` command may take.
pub const TIMEOUT: Duration = Duration::from_secs(10);
/// The macOS app's CLI, when `tailscale` is not on `PATH`.
pub const APP_CLI: &str = "/Applications/Tailscale.app/Contents/MacOS/Tailscale";
/// Where `tailscale` is installed when it is not on `PATH`: Homebrew on
/// Apple silicon, then on Intel and other installs.
pub const INSTALLED_CLIS: [&str; 2] = ["/opt/homebrew/bin/tailscale", "/usr/local/bin/tailscale"];

/// Where to look for the CLI, in order: each `PATH` directory, then
/// [`INSTALLED_CLIS`], then the app's in `/Applications`, then in
/// `<home>/Applications`.
pub fn candidates(path: Option<&std::ffi::OsStr>, home: Option<&Path>) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = path
        .map(|p| {
            std::env::split_paths(p)
                .map(|dir| dir.join("tailscale"))
                .collect()
        })
        .unwrap_or_default();
    found.extend(INSTALLED_CLIS.iter().map(PathBuf::from));
    found.push(PathBuf::from(APP_CLI));
    if let Some(home) = home {
        found.push(home.join("Applications/Tailscale.app/Contents/MacOS/Tailscale"));
    }
    found
}

/// Tailscale through its CLI.
#[derive(Debug, Clone)]
pub struct Tailscale {
    /// `None`: not installed.
    program: Option<PathBuf>,
    timeout: Duration,
}

/// What a command gave.
enum Ran {
    /// The program is not there.
    Missing,
    /// It ran to the end.
    Done {
        success: bool,
        stdout: Vec<u8>,
        stderr: String,
    },
}

impl Tailscale {
    /// The first of [`candidates`] that is there, else none.
    pub fn find() -> Tailscale {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let program = candidates(std::env::var_os("PATH").as_deref(), home.as_deref())
            .into_iter()
            .find(|p| is_executable(p));
        Tailscale {
            program,
            timeout: TIMEOUT,
        }
    }

    /// The CLI at `program`, each command limited to `timeout`.
    pub fn at(program: impl Into<PathBuf>, timeout: Duration) -> Tailscale {
        Tailscale {
            program: Some(program.into()),
            timeout,
        }
    }

    /// The CLI it runs, if it found one.
    pub fn program(&self) -> Option<&Path> {
        self.program.as_deref()
    }

    /// The arguments of `forward` for `port`.
    pub fn forward_args(port: u16) -> Vec<String> {
        vec![
            "serve".to_string(),
            "--bg".to_string(),
            "--tcp".to_string(),
            port.to_string(),
            format!("tcp://127.0.0.1:{port}"),
        ]
    }

    async fn run(&self, args: &[String]) -> Result<Ran, VpnError> {
        let command_line = format!("tailscale {}", args.join(" "));
        let Some(program) = &self.program else {
            return Ok(Ran::Missing);
        };
        let mut command = tokio::process::Command::new(program);
        command
            .args(args)
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true);
        match tokio::time::timeout(self.timeout, command.output()).await {
            Ok(Ok(output)) => Ok(Ran::Done {
                success: output.status.success(),
                stderr: {
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    if stderr.is_empty() {
                        // `tailscale serve` says some things on stdout.
                        String::from_utf8_lossy(&output.stdout).trim().to_string()
                    } else {
                        stderr
                    }
                },
                stdout: output.stdout,
            }),
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => Ok(Ran::Missing),
            Ok(Err(e)) => Err(VpnError {
                command: command_line,
                message: e.to_string(),
            }),
            Err(_) => Err(VpnError {
                command: command_line,
                message: format!("did not finish within {} s", self.timeout.as_secs_f32()),
            }),
        }
    }

    /// `tailscale status --json`, `None` when not installed or it fails
    /// (tailscaled not running).
    async fn status(&self) -> Result<Option<Value>, VpnError> {
        let args = ["status".to_string(), "--json".to_string()];
        match self.run(&args).await? {
            Ran::Missing => Ok(None),
            Ran::Done {
                success: false,
                stderr,
                ..
            } => {
                log::debug!("tailscale status --json: {stderr}");
                Ok(None)
            }
            Ran::Done { stdout, .. } => {
                serde_json::from_slice(&stdout)
                    .map(Some)
                    .map_err(|e| VpnError {
                        command: "tailscale status --json".to_string(),
                        message: format!("not JSON: {e}"),
                    })
            }
        }
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
        Ok(self.status().await?.as_ref().is_some_and(is_running))
    }

    async fn address(&self, port: u16) -> Result<Option<VpnAddress>, VpnError> {
        Ok(self
            .status()
            .await?
            .filter(is_running)
            .and_then(|status| self_dns_name(&status))
            .map(|host| VpnAddress { host, port }))
    }

    async fn forwards(&self, port: u16) -> Result<bool, VpnError> {
        let args = ["serve", "status", "--json"].map(String::from);
        match self.run(&args).await? {
            Ran::Missing => Ok(false),
            Ran::Done {
                success: false,
                stderr,
                ..
            } => Err(VpnError {
                command: "tailscale serve status --json".to_string(),
                message: stderr,
            }),
            Ran::Done { stdout, .. } => {
                // No configuration prints nothing, or `{}`.
                if stdout.iter().all(u8::is_ascii_whitespace) {
                    return Ok(false);
                }
                let serve: Value = serde_json::from_slice(&stdout).map_err(|e| VpnError {
                    command: "tailscale serve status --json".to_string(),
                    message: format!("not JSON: {e}"),
                })?;
                Ok(forward_present(&serve, port))
            }
        }
    }

    async fn forward(&self, port: u16) -> Result<(), VpnError> {
        match self.run(&Tailscale::forward_args(port)).await? {
            Ran::Missing => Err(VpnError {
                command: self.forward_command(port),
                message: "tailscale is not installed".to_string(),
            }),
            Ran::Done { success: true, .. } => Ok(()),
            Ran::Done { stderr, .. } => Err(VpnError {
                command: self.forward_command(port),
                message: stderr,
            }),
        }
    }

    fn forward_command(&self, port: u16) -> String {
        format!("tailscale {}", Tailscale::forward_args(port).join(" "))
    }
}

/// `BackendState == "Running"` in `tailscale status --json`.
pub fn is_running(status: &Value) -> bool {
    status.get("BackendState").and_then(Value::as_str) == Some("Running")
}

/// `Self.DNSName` from `tailscale status --json`, without the final dot.
pub fn self_dns_name(status: &Value) -> Option<String> {
    let name = status.get("Self")?.get("DNSName")?.as_str()?;
    let name = name.trim_end_matches('.');
    (!name.is_empty()).then(|| name.to_string())
}

/// Whether `tailscale serve status --json` forwards TCP `port` to
/// `127.0.0.1:<port>`. Looks at every `TCP` map, wherever it is nested
/// (foreground sessions and services nest their own).
pub fn forward_present(serve: &Value, port: u16) -> bool {
    match serve {
        Value::Object(map) => map.iter().any(|(key, value)| {
            (key == "TCP" && tcp_map_forwards(value, port)) || forward_present(value, port)
        }),
        Value::Array(items) => items.iter().any(|v| forward_present(v, port)),
        _ => false,
    }
}

fn tcp_map_forwards(tcp: &Value, port: u16) -> bool {
    let Some(handler) = tcp.get(port.to_string()) else {
        return false;
    };
    handler
        .get("TCPForward")
        .and_then(Value::as_str)
        .map(|target| {
            let target = target.strip_prefix("tcp://").unwrap_or(target);
            target == format!("127.0.0.1:{port}") || target == format!("localhost:{port}")
        })
        .unwrap_or(false)
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
