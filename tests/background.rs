//! `flankr serve` in the background, `--foreground`, `--stop` and the
//! files (docs/design/20261102_01_BACKGROUND_DAEMON_PLAN.md, step 2).
//!
//! Every daemon here runs the built binary over the herdr mock, with a
//! temporary HOME, TMPDIR, working folder, config and listen socket, and
//! an environment cleared of everything else, so nothing reaches
//! `~/.config/flankr`, `~/.local/state/flankr` or a real herdr.
//! `SSH_CONNECTION` is set, which makes `flankr-daemon` start the server
//! directly in a new session rather than submit a launchd job.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use flankr::daemon::{DaemonFiles, answers, read_pid, state_dir};

const BIN: &str = env!("CARGO_BIN_EXE_flankr");

struct Rig {
    dir: tempfile::TempDir,
    /// The listen socket, absolute.
    socket: PathBuf,
}

impl Rig {
    fn new() -> Rig {
        // A short path: a Unix socket's path is limited to about 100 bytes.
        let dir = tempfile::Builder::new()
            .prefix("fbg")
            .tempdir_in("/tmp")
            .unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        std::fs::write(
            dir_path.join("config.toml"),
            "[link]\nlisten = []\n\n[llm]\nenabled = false\n",
        )
        .unwrap();
        Rig {
            socket: dir_path.join("f.sock"),
            dir,
        }
    }

    fn home(&self) -> PathBuf {
        self.dir.path().canonicalize().unwrap()
    }

    fn files(&self) -> DaemonFiles {
        DaemonFiles::for_socket(
            &state_dir(&self.home()),
            &self.socket,
            &self.home().join(".config/flankr/flankr.sock"),
        )
    }

    /// `flankr <command> --mock --config … --listen …` here, with only the
    /// test environment and `extra`.
    fn command(&self, command: &[&str], extra: &[(&str, &str)]) -> Command {
        let home = self.home();
        let mut cmd = Command::new(BIN);
        cmd.args(command)
            .arg("--mock")
            .arg("--config")
            .arg(home.join("config.toml"))
            .arg("--listen")
            .arg(&self.socket)
            .current_dir(&home)
            .env_clear()
            .env("HOME", &home)
            .env("TMPDIR", &home)
            .env("PATH", "/usr/bin:/bin")
            .env("RUST_LOG", "info")
            .env("SSH_CONNECTION", "127.0.0.1 1 127.0.0.1 2")
            .stdin(Stdio::null());
        for (k, v) in extra {
            cmd.env(k, v);
        }
        cmd
    }

    fn run(&self, command: &[&str], extra: &[(&str, &str)]) -> Output {
        self.command(command, extra).output().unwrap()
    }

    fn answers(&self) -> bool {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(answers(&self.socket)).is_some()
    }

    fn wait_until(&self, what: &str, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done() {
            assert!(Instant::now() < deadline, "timed out: {what}");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Rig {
    /// Never leave a daemon behind, whatever the test did.
    fn drop(&mut self) {
        if let Some(pid) = read_pid(&self.files().pid)
            && flankr::daemon::is_flankr(pid)
        {
            // SAFETY: a flankr this test started.
            unsafe { libc::kill(pid as i32, libc::SIGKILL) };
        }
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

/// Environment variable names of process `pid` as it was started.
fn environment_names(pid: u32) -> Vec<String> {
    #[cfg(target_os = "linux")]
    {
        let raw = std::fs::read(format!("/proc/{pid}/environ")).unwrap();
        raw.split(|b| *b == 0)
            .filter(|kv| !kv.is_empty())
            .map(|kv| text(kv.split(|b| *b == b'=').next().unwrap()))
            .collect()
    }
    #[cfg(not(target_os = "linux"))]
    {
        // `ps -E` adds the environment after the arguments; the arguments
        // here hold no `=`, and their words are known.
        let out = Command::new("/bin/ps")
            .args(["-E", "-ww", "-o", "command=", "-p", &pid.to_string()])
            .output()
            .unwrap();
        text(&out.stdout)
            .split_whitespace()
            .filter_map(|w| w.split_once('=').map(|(k, _)| k.to_string()))
            .collect()
    }
}

#[test]
fn serve_starts_in_the_background_says_already_running_and_stops() {
    let rig = Rig::new();
    let files = rig.files();
    let out = rig.run(&["serve"], &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        text(&out.stdout),
        format!(
            "flankr is running on {}; log: {}\n",
            rig.socket.display(),
            files.log.display()
        )
    );
    assert!(rig.answers(), "the server does not answer");
    // The files of decision 4, private.
    let name = files.log.file_name().unwrap().to_str().unwrap();
    assert!(
        name.starts_with("daemon-") && name.len() == "daemon-12345678.log".len(),
        "{name}"
    );
    assert_eq!(mode(files.log.parent().unwrap()), 0o700);
    assert_eq!(mode(&files.log), 0o600);
    rig.wait_until("the pid file", || files.pid.exists());
    assert_eq!(mode(&files.pid), 0o600);
    let pid = read_pid(&files.pid).unwrap();
    assert!(flankr::daemon::is_flankr(pid));
    let log = std::fs::read_to_string(&files.log).unwrap();
    assert!(log.contains("flankr serve listening on"), "{log}");

    // Again: already running, with the server's pid.
    let out = rig.run(&["serve"], &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        text(&out.stdout),
        format!(
            "flankr is already running on {} (pid {pid})\n",
            rig.socket.display()
        )
    );
    // Bare `flankr` is `serve`.
    let out = rig.run(&[], &[]);
    assert_eq!(
        text(&out.stdout),
        format!(
            "flankr is already running on {} (pid {pid})\n",
            rig.socket.display()
        )
    );

    // --stop: SIGTERM, a clean exit that removes the pid file.
    let out = rig.run(&["serve", "--stop"], &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        text(&out.stdout),
        format!("flankr stopped on {} (pid {pid})\n", rig.socket.display())
    );
    assert!(!files.pid.exists());
    assert!(!rig.answers());
    assert!(!flankr::daemon::alive(pid));
    let log = std::fs::read_to_string(&files.log).unwrap();
    assert!(log.contains("flankr server stopping on SIGTERM"), "{log}");

    // Nothing left to stop.
    let out = rig.run(&["serve", "--stop"], &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        text(&out.stdout),
        format!("flankr is not running on {}\n", rig.socket.display())
    );
}

#[test]
fn foreground_stays_in_the_foreground() {
    let rig = Rig::new();
    let files = rig.files();
    let mut child = rig
        .command(&["serve", "--foreground"], &[])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    rig.wait_until("the foreground server answers", || rig.answers());
    assert!(child.try_wait().unwrap().is_none(), "it returned");
    rig.wait_until("the pid file", || files.pid.exists());
    assert_eq!(read_pid(&files.pid), Some(child.id()));
    // SAFETY: the child this test started.
    unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(0));
    assert!(!files.pid.exists());

    // `flankr server` is the same.
    let mut child = rig
        .command(&["server"], &[])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    rig.wait_until("the server answers", || rig.answers());
    assert!(child.try_wait().unwrap().is_none(), "it returned");
    rig.wait_until("the pid file", || read_pid(&files.pid) == Some(child.id()));
    // SAFETY: as above.
    unsafe { libc::kill(child.id() as i32, libc::SIGINT) };
    assert_eq!(child.wait().unwrap().code(), Some(0));
    assert!(!files.pid.exists());
}

#[test]
fn a_stale_pid_file_is_removed() {
    let rig = Rig::new();
    let files = rig.files();
    std::fs::create_dir_all(files.pid.parent().unwrap()).unwrap();

    // A pid that is not a flankr: a sleeping shell tool.
    let mut other = Command::new("/bin/sleep").arg("30").spawn().unwrap();
    std::fs::write(&files.pid, format!("{}\n", other.id())).unwrap();
    let out = rig.run(&["serve", "--stop"], &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        text(&out.stdout),
        format!(
            "Removed the stale pid file {} (pid {} is not a running flankr)\n",
            files.pid.display(),
            other.id()
        )
    );
    assert!(!files.pid.exists());
    assert!(other.try_wait().unwrap().is_none(), "it signalled sleep");
    other.kill().unwrap();
    other.wait().unwrap();

    // A pid that no longer runs.
    let dead = other.id();
    std::fs::write(&files.pid, format!("{dead}\n")).unwrap();
    let out = rig.run(&["serve", "--stop"], &[]);
    assert_eq!(
        text(&out.stdout),
        format!(
            "Removed the stale pid file {} (pid {dead} is not a running flankr)\n",
            files.pid.display()
        )
    );
    assert!(!files.pid.exists());

    // A stale pid file does not stop `serve`; the server replaces it.
    std::fs::write(&files.pid, format!("{dead}\n")).unwrap();
    let out = rig.run(&["serve"], &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    rig.wait_until("the new pid file", || {
        read_pid(&files.pid).is_some_and(|p| p != dead)
    });
    let out = rig.run(&["serve", "--stop"], &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(!files.pid.exists());
}

#[test]
fn only_the_listed_environment_reaches_the_server() {
    let rig = Rig::new();
    let files = rig.files();
    let xdg = rig.home().join("xdg");
    let out = rig.run(
        &["serve"],
        &[
            ("GEMINI_API_KEY", "not-a-real-key"),
            ("FLANKR_TEST_OTHER", "1"),
            ("XDG_CONFIG_HOME", xdg.to_str().unwrap()),
            ("LANG", "C"),
        ],
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    // The key in this shell, and no key file: one line says so.
    let err = text(&out.stderr);
    assert_eq!(
        err.lines()
            .filter(|l| l.contains("GEMINI_API_KEY in this shell is not passed"))
            .count(),
        1,
        "{err}"
    );
    rig.wait_until("the pid file", || files.pid.exists());
    let pid = read_pid(&files.pid).unwrap();
    let mut names = environment_names(pid);
    names.sort();
    assert_eq!(
        names,
        [
            "HOME",
            "LANG",
            "PATH",
            "RUST_LOG",
            "TMPDIR",
            "XDG_CONFIG_HOME"
        ],
        "the server's environment"
    );
    let out = rig.run(&["serve", "--stop"], &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
}

#[test]
fn a_server_that_does_not_come_up_shows_its_log() {
    let rig = Rig::new();
    // A listen socket the server cannot create: it exits at once.
    let mut cmd = rig.command(&["serve"], &[]);
    cmd.arg("--listen").arg("/dev/null/f.sock");
    let started = Instant::now();
    let out = cmd.output().unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(started.elapsed() >= Duration::from_secs(5));
    let err = text(&out.stderr);
    assert!(
        err.contains("flankr did not start on /dev/null/f.sock within 5 s; the end of"),
        "{err}"
    );
    assert!(err.contains("/dev/null"), "{err}");
}
