//! The daemon in the background: `flankr serve` starts `flankr server`
//! detached and returns, `flankr serve --stop` stops it
//! (docs/design/20261102_01_BACKGROUND_DAEMON_PLAN.md, decisions 1 to 5).
//!
//! The detaching itself is `flankr-daemon`'s. This module holds what is
//! flankr's: the environment passed, the log and pid files named by the
//! listen socket, the already-running check, and the start and stop.

use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use libflankr_proto::common::{HelloParams, methods, protocols};
use libflankr_proto::{Id, Incoming, Request};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;
use tokio_util::codec::{FramedRead, LinesCodec};

/// The environment variables `serve` passes to the background server
/// (decision 3), besides every `XDG_*` that is set. Never a key: on macOS
/// they are written to a property list on disk.
pub const PASSED_ENV: &[&str] = &[
    "HOME",
    "PATH",
    "USER",
    "LANG",
    "TMPDIR",
    "RUST_LOG",
    "HERDR_SOCKET_PATH",
];

/// The background server's launchd label prefix.
pub const LABEL: &str = "dev.flankr.server";

/// How often, and for how long, `serve` waits for the server's socket.
pub const START_POLL: Duration = Duration::from_millis(100);
pub const START_WAIT: Duration = Duration::from_secs(5);
/// How long `--stop` waits after SIGTERM before SIGKILL.
pub const STOP_WAIT: Duration = Duration::from_secs(5);
/// How long a daemon has to answer `ping` to count as running.
pub const PING_WAIT: Duration = Duration::from_secs(1);
/// The lines of the log `serve` prints when the server does not come up.
pub const LOG_TAIL_LINES: usize = 20;

/// The variables of `vars` that `serve` passes on: [`PASSED_ENV`] and
/// `XDG_*`, in the order given.
pub fn passed_env(
    vars: impl IntoIterator<Item = (OsString, OsString)>,
) -> Vec<(OsString, OsString)> {
    vars.into_iter()
        .filter(|(name, _)| {
            let name = name.as_encoded_bytes();
            PASSED_ENV.iter().any(|p| p.as_bytes() == name) || name.starts_with(b"XDG_")
        })
        .collect()
}

/// `~/.local/state/flankr` under `home`.
pub fn state_dir(home: &Path) -> PathBuf {
    home.join(".local").join("state").join("flankr")
}

/// The daemon's log and pid files for one listen socket (decision 4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonFiles {
    pub log: PathBuf,
    pub pid: PathBuf,
}

impl DaemonFiles {
    /// `daemon.log` and `daemon.pid` in `state` for the default socket;
    /// `daemon-<8 hex>.log` and `.pid` for any other, the hex the start
    /// of the SHA-256 of the socket's absolute path.
    pub fn for_socket(state: &Path, socket: &Path, default_socket: &Path) -> DaemonFiles {
        let socket = absolute(socket);
        let stem = if socket == absolute(default_socket) {
            "daemon".to_string()
        } else {
            let hash = Sha256::digest(socket.as_os_str().as_encoded_bytes());
            format!(
                "daemon-{:02x}{:02x}{:02x}{:02x}",
                hash[0], hash[1], hash[2], hash[3]
            )
        };
        DaemonFiles {
            log: state.join(format!("{stem}.log")),
            pid: state.join(format!("{stem}.pid")),
        }
    }
}

/// `path` made absolute against the current folder (not resolved).
pub fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

/// Create `dir` (and its parents) with mode 0700 for the last one.
pub fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
        }
        Err(e) => Err(e),
    }
}

/// The server's pid file, removed when dropped if it still holds this pid.
pub struct PidFile {
    path: PathBuf,
    pid: u32,
}

impl PidFile {
    /// Write this process's pid to `path` (0600), creating its folder 0700.
    pub fn create(path: &Path) -> std::io::Result<PidFile> {
        if let Some(dir) = path.parent() {
            create_private_dir(dir)?;
        }
        let pid = std::process::id();
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        writeln!(f, "{pid}")?;
        Ok(PidFile {
            path: path.to_path_buf(),
            pid,
        })
    }
}

impl Drop for PidFile {
    fn drop(&mut self) {
        if read_pid(&self.path) == Some(self.pid) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// The pid in a pid file, if it holds one.
pub fn read_pid(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Whether a daemon answers on `socket`: it accepts a connection and
/// answers `hello` and `ping` within [`PING_WAIT`] (decision 5). The
/// peer's pid, when it answers and the system gives it.
pub async fn answers(socket: &Path) -> Option<Option<u32>> {
    tokio::time::timeout(PING_WAIT, ping(socket))
        .await
        .ok()
        .flatten()
}

async fn ping(socket: &Path) -> Option<Option<u32>> {
    let stream = UnixStream::connect(socket).await.ok()?;
    let pid = stream
        .peer_cred()
        .ok()
        .and_then(|c| c.pid())
        .and_then(|p| u32::try_from(p).ok());
    let (read, mut write) = stream.into_split();
    let hello = Request::new(
        Id::Number(0),
        methods::HELLO,
        HelloParams {
            protocol: protocols::HARNESS.to_string(),
            version: 1,
            client: "serve".to_string(),
        },
    );
    let ping = Request::new(Id::Number(1), methods::PING, serde_json::json!({}));
    let mut out = String::new();
    for message in [
        serde_json::to_value(&hello).ok()?,
        serde_json::to_value(&ping).ok()?,
    ] {
        out.push_str(&message.to_string());
        out.push('\n');
    }
    write.write_all(out.as_bytes()).await.ok()?;
    let mut lines = FramedRead::new(read, LinesCodec::new_with_max_length(1 << 20));
    while let Some(Ok(line)) = lines.next().await {
        if let Ok(Incoming::Response(r)) = serde_json::from_str::<Incoming>(&line)
            && r.id == Some(Id::Number(1))
        {
            return r.error.is_none().then_some(pid);
        }
    }
    None
}

/// Whether `socket` accepts a connection.
pub async fn accepts(socket: &Path) -> bool {
    UnixStream::connect(socket).await.is_ok()
}

/// Whether process `pid` exists.
pub fn alive(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    // SAFETY: signal 0 only checks that the process exists.
    let found = unsafe { libc::kill(pid, 0) } == 0;
    found || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// The executable of process `pid`, where the system gives it.
fn executable(pid: u32) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        // SAFETY: `buf` has the size given.
        let n = unsafe {
            libc::proc_pidpath(
                libc::c_int::try_from(pid).ok()?,
                buf.as_mut_ptr().cast(),
                buf.len() as u32,
            )
        };
        if n <= 0 {
            return None;
        }
        buf.truncate(n as usize);
        use std::os::unix::ffi::OsStringExt;
        Some(PathBuf::from(OsString::from_vec(buf)))
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::fs::read_link(format!("/proc/{pid}/exe")).ok()
    }
}

/// Whether process `pid` runs a flankr binary (its file name starts with
/// `flankr`). Without a way to tell, a living process counts.
pub fn is_flankr(pid: u32) -> bool {
    if !alive(pid) {
        return false;
    }
    match executable(pid) {
        Some(exe) => exe
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name.starts_with("flankr")),
        None => true,
    }
}

/// The last `n` lines of `path`.
pub fn log_tail(path: &Path, n: usize) -> String {
    let text = std::fs::read(path).unwrap_or_default();
    let text = String::from_utf8_lossy(&text);
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// What `serve` needs to start the server in the background.
pub struct Background {
    /// The listen socket, absolute.
    pub socket: PathBuf,
    pub files: DaemonFiles,
    /// The server's program, absolute (this binary).
    pub program: PathBuf,
    /// The server's arguments: `server` and `serve`'s own.
    pub args: Vec<OsString>,
    pub working_dir: PathBuf,
    /// The whole environment passed ([`passed_env`]).
    pub env: Vec<(OsString, OsString)>,
}

/// `flankr serve` (decision 1): report a daemon already answering, else
/// start the server detached and wait for its socket. Prints what it did
/// and returns the exit code.
pub async fn serve_background(b: &Background) -> i32 {
    if let Some(peer) = answers(&b.socket).await {
        let pid = peer
            .or_else(|| read_pid(&b.files.pid))
            .map_or_else(|| "unknown".to_string(), |p| p.to_string());
        println!(
            "flankr is already running on {} (pid {pid})",
            b.socket.display()
        );
        return 0;
    }
    if let Some(dir) = b.files.log.parent()
        && let Err(e) = create_private_dir(dir)
    {
        eprintln!("flankr: cannot create {}: {e}", dir.display());
        return 1;
    }
    let detached = flankr_daemon::Detached {
        program: b.program.clone(),
        args: b.args.clone(),
        working_dir: b.working_dir.clone(),
        env: b.env.clone(),
        log: b.files.log.clone(),
        label: LABEL.to_string(),
    };
    if let Err(e) = flankr_daemon::start(&detached) {
        eprintln!("flankr: could not start the server: {e}");
        return 1;
    }
    let deadline = Instant::now() + START_WAIT;
    loop {
        if accepts(&b.socket).await {
            println!(
                "flankr is running on {}; log: {}",
                b.socket.display(),
                b.files.log.display()
            );
            return 0;
        }
        if Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(START_POLL).await;
    }
    eprintln!(
        "flankr did not start on {} within {} s; the end of {}:",
        b.socket.display(),
        START_WAIT.as_secs(),
        b.files.log.display()
    );
    eprintln!("{}", log_tail(&b.files.log, LOG_TAIL_LINES));
    1
}

/// `flankr serve --stop` (decision 1): SIGTERM the pid in the pid file,
/// wait for it to stop answering, else SIGKILL. A stale pid file is
/// removed. Prints what it did and returns the exit code.
pub async fn stop(socket: &Path, files: &DaemonFiles) -> i32 {
    let recorded = read_pid(&files.pid);
    let pid = match recorded {
        Some(pid) if is_flankr(pid) => pid,
        Some(pid) => {
            let _ = std::fs::remove_file(&files.pid);
            println!(
                "Removed the stale pid file {} (pid {pid} is not a running flankr)",
                files.pid.display()
            );
            // A daemon may still answer without a pid file of its own.
            match answers(socket).await.flatten() {
                Some(peer) if is_flankr(peer) => peer,
                _ => return 0,
            }
        }
        None => match answers(socket).await {
            Some(Some(peer)) if is_flankr(peer) => peer,
            Some(_) => {
                eprintln!(
                    "flankr answers on {} but its pid is not known; not stopped",
                    socket.display()
                );
                return 1;
            }
            None => {
                println!("flankr is not running on {}", socket.display());
                return 0;
            }
        },
    };
    let Ok(target) = libc::pid_t::try_from(pid) else {
        return 1;
    };
    // SAFETY: kill(2) with a pid checked above to be a flankr.
    if unsafe { libc::kill(target, libc::SIGTERM) } != 0 {
        eprintln!(
            "flankr: could not signal pid {pid}: {}",
            std::io::Error::last_os_error()
        );
        return 1;
    }
    let deadline = Instant::now() + STOP_WAIT;
    let stopped = loop {
        if !alive(pid) && !accepts(socket).await {
            break true;
        }
        if Instant::now() >= deadline {
            break false;
        }
        tokio::time::sleep(START_POLL).await;
    };
    if stopped {
        println!("flankr stopped on {} (pid {pid})", socket.display());
    } else {
        // SAFETY: as above.
        unsafe { libc::kill(target, libc::SIGKILL) };
        println!(
            "flankr did not stop within {} s; killed pid {pid}",
            STOP_WAIT.as_secs()
        );
    }
    if read_pid(&files.pid) == Some(pid) {
        let _ = std::fs::remove_file(&files.pid);
    }
    0
}

/// Wait for SIGTERM or SIGINT; the signal's name.
pub async fn termination() -> &'static str {
    use tokio::signal::unix::{SignalKind, signal};
    let (Ok(mut term), Ok(mut int)) = (
        signal(SignalKind::terminate()),
        signal(SignalKind::interrupt()),
    ) else {
        return std::future::pending().await;
    };
    tokio::select! {
        _ = term.recv() => "SIGTERM",
        _ = int.recv() => "SIGINT",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_listed_variables_and_xdg_pass() {
        let vars = [
            ("HOME", "/h"),
            ("PATH", "/bin"),
            ("GEMINI_API_KEY", "secret"),
            ("USER", "u"),
            ("LANG", "C"),
            ("TMPDIR", "/t"),
            ("RUST_LOG", "debug"),
            ("HERDR_SOCKET_PATH", "/s"),
            ("XDG_STATE_HOME", "/x"),
            ("XDG", "no"),
            ("SSH_AUTH_SOCK", "/a"),
            ("HOMEX", "no"),
            ("ANTHROPIC_API_KEY", "secret"),
        ]
        .map(|(k, v)| (OsString::from(k), OsString::from(v)));
        let names: Vec<_> = passed_env(vars)
            .into_iter()
            .map(|(k, _)| k.into_string().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "HOME",
                "PATH",
                "USER",
                "LANG",
                "TMPDIR",
                "RUST_LOG",
                "HERDR_SOCKET_PATH",
                "XDG_STATE_HOME"
            ]
        );
    }

    #[test]
    fn files_are_named_by_the_socket() {
        let state = Path::new("/h/.local/state/flankr");
        let default = Path::new("/h/.config/flankr/flankr.sock");
        let files = DaemonFiles::for_socket(state, default, default);
        assert_eq!(files.log, state.join("daemon.log"));
        assert_eq!(files.pid, state.join("daemon.pid"));
        let other = DaemonFiles::for_socket(state, Path::new("/tmp/a.sock"), default);
        let hash = Sha256::digest(b"/tmp/a.sock");
        let hex: String = hash[..4].iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(other.log, state.join(format!("daemon-{hex}.log")));
        assert_eq!(other.pid, state.join(format!("daemon-{hex}.pid")));
        assert_ne!(
            DaemonFiles::for_socket(state, Path::new("/tmp/b.sock"), default),
            other
        );
    }
}
