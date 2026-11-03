//! Start a program detached from the terminal that started it
//! (docs/design/20261102_01_BACKGROUND_DAEMON_PLAN.md, decision 2).
//!
//! A process started from a terminal app on macOS stays in that app's
//! launchd job, even after `setsid(2)`, and is killed when the app quits.
//! So on macOS, with a GUI domain and not over SSH, [`start`] submits the
//! program as a one-shot job to the user's GUI domain with
//! `launchctl bootstrap gui/<uid> <plist>` (launchctl(1), launchd.plist(5)).
//! The started program calls [`leave_job`] first thing: it forks, the child
//! makes itself a new session and carries on, and the job's own process
//! exits, so the program ends up in no job at all.
//!
//! Everywhere else (and when the job cannot be submitted) the program is
//! spawned directly in a new session, with its input from `/dev/null` and
//! its output appended to the log.
//!
//! Written from Apple's manual pages and the plan; no other implementation
//! was consulted.

use std::ffi::{OsStr, OsString};
use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// What to start, and how.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detached {
    /// The program, as an absolute path.
    pub program: PathBuf,
    /// Its arguments, after the program itself.
    pub args: Vec<OsString>,
    /// Its working folder, as an absolute path.
    pub working_dir: PathBuf,
    /// Its whole environment: only these variables, never secrets (on
    /// macOS they are written to a property list on disk).
    pub env: Vec<(OsString, OsString)>,
    /// Its standard output and standard error, appended (created 0600).
    pub log: PathBuf,
    /// The launchd label's prefix, such as `dev.flankr.server`; each job
    /// is labelled `<label>.<unix ms>`.
    pub label: String,
}

/// How the program was started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Started {
    /// As a one-shot job in the user's GUI launchd domain, with this label.
    Launchd { label: String },
    /// Spawned directly in a new session, with this process id.
    Direct { pid: u32 },
}

/// The variable that tells the started program it runs as a job and must
/// leave it ([`leave_job`]).
const LEAVE_JOB_ENV: &str = "FLANKR_DAEMON_LEAVE_JOB";

/// Jobs this crate submitted longer ago than this are booted out on the
/// next [`start`].
const STALE_JOB_MS: u64 = 60_000;

/// Start `d` detached and return how.
///
/// The log is created (mode 0600) or opened for appending first, so an
/// unwritable log is an error before anything starts. A direct start's
/// process is not waited for; it is reaped once the caller exits.
pub fn start(d: &Detached) -> io::Result<Started> {
    if !d.program.is_absolute() || !d.working_dir.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the program and the working folder must be absolute paths",
        ));
    }
    drop(open_log(&d.log)?);
    #[cfg(target_os = "macos")]
    if std::env::var_os("SSH_CONNECTION").is_none()
        && let Some(label) = launchd::submit(d)
    {
        return Ok(Started::Launchd { label });
    }
    start_direct(d)
}

/// The first thing the started program calls. In a job submitted by
/// [`start`] (the marker variable is set), it removes the marker, forks,
/// and returns in the child once the child is the leader of a new session;
/// the parent, the job's own process, exits at once. Otherwise it does
/// nothing.
///
/// Call it before any thread is started: only the calling thread carries
/// on in the child.
pub fn leave_job() -> io::Result<()> {
    if std::env::var_os(LEAVE_JOB_ENV).as_deref() != Some(OsStr::new("1")) {
        return Ok(());
    }
    // SAFETY: called first thing in the program, before any other thread
    // exists to read the environment at the same time.
    unsafe { std::env::remove_var(LEAVE_JOB_ENV) };

    let mut fds = [0 as libc::c_int; 2];
    // SAFETY: `fds` has room for the two descriptors pipe(2) writes.
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let [read_end, write_end] = fds;
    // SAFETY: fork(2) with no other threads (see above); each side only
    // closes, reads or writes its own descriptors before going on.
    match unsafe { libc::fork() } {
        -1 => {
            let e = io::Error::last_os_error();
            // SAFETY: both descriptors are ours and open.
            unsafe {
                libc::close(read_end);
                libc::close(write_end);
            }
            Err(e)
        }
        0 => {
            // The child: a new session, so launchd's clean-up of the job's
            // process group does not reach it, then tell the parent.
            // SAFETY: plain system calls on descriptors this side owns.
            let sid = unsafe {
                libc::close(read_end);
                let sid = libc::setsid();
                let byte = 1u8;
                libc::write(write_end, (&byte as *const u8).cast(), 1);
                libc::close(write_end);
                sid
            };
            if sid == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
        _ => {
            // The parent: wait until the child is in its own session (or
            // gone), then end the job.
            // SAFETY: plain system calls on descriptors this side owns;
            // `_exit` runs no destructors or atexit handlers.
            unsafe {
                libc::close(write_end);
                let mut byte = 0u8;
                loop {
                    let n = libc::read(read_end, (&mut byte as *mut u8).cast(), 1);
                    if n == -1 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    break;
                }
                libc::_exit(0)
            }
        }
    }
}

/// Open the log for appending, creating it with mode 0600.
fn open_log(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
}

/// Spawn `d` in a new session, with stdin from `/dev/null` and stdout and
/// stderr appended to its log.
fn start_direct(d: &Detached) -> io::Result<Started> {
    let out = open_log(&d.log)?;
    let err = out.try_clone()?;
    let mut cmd = Command::new(&d.program);
    cmd.args(&d.args)
        .current_dir(&d.working_dir)
        .env_clear()
        .envs(d.env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err);
    // SAFETY: setsid(2) is async-signal-safe, as pre_exec requires.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = cmd.spawn()?;
    Ok(Started::Direct { pid: child.id() })
}

/// `value` as plist string content, or `None` if a plist cannot carry it
/// exactly: not UTF-8, or holding a character XML 1.0 does not allow (a
/// NUL byte, most other control characters, U+FFFE, U+FFFF).
fn xml_text(value: &OsStr) -> Option<String> {
    let s = value.to_str()?;
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            // A literal CR would be read back as LF.
            '\r' => out.push_str("&#13;"),
            '\t' | '\n' => out.push(c),
            '\u{0}'..='\u{1f}' | '\u{fffe}' | '\u{ffff}' => return None,
            _ => out.push(c),
        }
    }
    Some(out)
}

/// The property list submitting `d` as the job `label`, or `None` if one
/// of its values cannot be carried exactly (then `start` starts directly).
fn plist(d: &Detached, label: &str) -> Option<String> {
    let string = |v: &OsStr| xml_text(v).map(|t| format!("<string>{t}</string>"));
    let mut x = String::new();
    x.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    x.push_str(
        "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
    );
    x.push_str("<plist version=\"1.0\">\n<dict>\n");
    x.push_str(&format!(
        "  <key>Label</key>\n  {}\n",
        string(OsStr::new(label))?
    ));
    x.push_str("  <key>ProgramArguments</key>\n  <array>\n");
    for arg in std::iter::once(d.program.as_os_str()).chain(d.args.iter().map(OsString::as_os_str))
    {
        x.push_str(&format!("    {}\n", string(arg)?));
    }
    x.push_str("  </array>\n");
    x.push_str(&format!(
        "  <key>WorkingDirectory</key>\n  {}\n",
        string(d.working_dir.as_os_str())?
    ));
    x.push_str("  <key>EnvironmentVariables</key>\n  <dict>\n");
    let marker = (OsString::from(LEAVE_JOB_ENV), OsString::from("1"));
    for (name, value) in d
        .env
        .iter()
        .filter(|(name, _)| name != LEAVE_JOB_ENV)
        .chain(std::iter::once(&marker))
    {
        x.push_str(&format!(
            "    <key>{}</key>\n    {}\n",
            xml_text(name)?,
            string(value)?
        ));
    }
    x.push_str("  </dict>\n");
    let log = string(d.log.as_os_str())?;
    x.push_str(&format!("  <key>StandardOutPath</key>\n  {log}\n"));
    x.push_str(&format!("  <key>StandardErrorPath</key>\n  {log}\n"));
    x.push_str("  <key>RunAtLoad</key>\n  <true/>\n");
    x.push_str("</dict>\n</plist>\n");
    Some(x)
}

/// From `launchctl list` output (PID, last status, label; launchctl(1)),
/// the labels of this crate's jobs under `prefix` submitted more than
/// [`STALE_JOB_MS`] before `now_ms` and no longer running (PID `-`), so a
/// job whose process somehow still runs is never booted out.
fn stale_jobs(list: &str, prefix: &str, now_ms: u64) -> Vec<String> {
    list.lines()
        .filter_map(|line| {
            let mut cols = line.split_whitespace();
            let (pid, _status, label) = (cols.next()?, cols.next()?, cols.next()?);
            if pid != "-" || cols.next().is_some() {
                return None;
            }
            let ms = label.strip_prefix(prefix)?.strip_prefix('.')?;
            if ms.is_empty() || !ms.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let ms: u64 = ms.parse().ok()?;
            (now_ms.saturating_sub(ms) > STALE_JOB_MS).then(|| label.to_string())
        })
        .collect()
}

/// Milliseconds since the Unix epoch.
fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

#[cfg(target_os = "macos")]
mod launchd {
    use super::*;
    use std::os::unix::fs::DirBuilderExt;

    const LAUNCHCTL: &str = "/bin/launchctl";

    fn domain() -> String {
        // SAFETY: getuid(2) cannot fail.
        format!("gui/{}", unsafe { libc::getuid() })
    }

    /// Submit `d` as a one-shot job; its label, or `None` to start directly.
    pub(super) fn submit(d: &Detached) -> Option<String> {
        let now = unix_ms();
        let label = format!("{}.{now}", d.label);
        let xml = plist(d, &label)?;
        boot_out_stale(&d.label, now);

        let (dir, file) = write_private(&d.label, &xml).ok()?;
        let status = Command::new(LAUNCHCTL)
            .arg("bootstrap")
            .arg(domain())
            .arg(&file)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = std::fs::remove_file(&file);
        let _ = std::fs::remove_dir(&dir);
        status.ok()?.success().then_some(label)
    }

    /// Write `xml` to `job.plist` (0600) in a fresh folder (0700) under the
    /// user's temp folder.
    fn write_private(prefix: &str, xml: &str) -> io::Result<(PathBuf, PathBuf)> {
        use std::io::Write;
        let tmp = std::env::temp_dir();
        for n in 0..100u32 {
            let dir = tmp.join(format!("{prefix}-{}-{}-{n}", std::process::id(), unix_ms()));
            match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
            let file = dir.join("job.plist");
            let written = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&file)
                .and_then(|mut f| f.write_all(xml.as_bytes()));
            if let Err(e) = written {
                let _ = std::fs::remove_file(&file);
                let _ = std::fs::remove_dir(&dir);
                return Err(e);
            }
            return Ok((dir, file));
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "no fresh folder for the job's property list",
        ))
    }

    /// Boot out this crate's finished jobs under `prefix` older than a minute.
    fn boot_out_stale(prefix: &str, now: u64) {
        let Ok(out) = Command::new(LAUNCHCTL)
            .arg("list")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
        else {
            return;
        };
        let list = String::from_utf8_lossy(&out.stdout);
        for label in stale_jobs(&list, prefix, now) {
            let _ = Command::new(LAUNCHCTL)
                .arg("bootout")
                .arg(format!("{}/{label}", domain()))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;
    use std::time::{Duration, Instant};

    fn sample() -> Detached {
        Detached {
            program: PathBuf::from("/usr/local/bin/flankr"),
            args: vec!["server".into(), "--config".into(), "a<b&c>.toml".into()],
            working_dir: PathBuf::from("/Users/someone/src/project"),
            env: vec![
                ("HOME".into(), "/Users/someone".into()),
                ("PATH".into(), "/usr/bin:/bin".into()),
            ],
            log: PathBuf::from("/Users/someone/.local/state/flankr/daemon.log"),
            label: "dev.flankr.server".into(),
        }
    }

    #[test]
    fn plist_has_the_keys_in_order() {
        let x = plist(&sample(), "dev.flankr.server.1700000000000").unwrap();
        let keys = [
            "<key>Label</key>\n  <string>dev.flankr.server.1700000000000</string>",
            "<key>ProgramArguments</key>\n  <array>\n    <string>/usr/local/bin/flankr</string>\n    <string>server</string>\n    <string>--config</string>\n    <string>a&lt;b&amp;c&gt;.toml</string>\n  </array>",
            "<key>WorkingDirectory</key>\n  <string>/Users/someone/src/project</string>",
            "<key>EnvironmentVariables</key>\n  <dict>\n    <key>HOME</key>\n    <string>/Users/someone</string>\n    <key>PATH</key>\n    <string>/usr/bin:/bin</string>\n    <key>FLANKR_DAEMON_LEAVE_JOB</key>\n    <string>1</string>\n  </dict>",
            "<key>StandardOutPath</key>\n  <string>/Users/someone/.local/state/flankr/daemon.log</string>",
            "<key>StandardErrorPath</key>\n  <string>/Users/someone/.local/state/flankr/daemon.log</string>",
            "<key>RunAtLoad</key>\n  <true/>",
        ];
        let mut at = 0;
        for key in keys {
            let found = x[at..]
                .find(key)
                .unwrap_or_else(|| panic!("{key}\n---\n{x}"));
            at += found + key.len();
        }
        assert!(x.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist"));
        assert!(x.ends_with("</dict>\n</plist>\n"));
        assert_eq!(x.matches("<key>").count(), 10, "{x}");
    }

    #[test]
    fn plist_escapes_and_keeps_the_marker_once() {
        let mut d = sample();
        d.env = vec![
            ("A".into(), "x & y < z > w\r\n\t'\"".into()),
            (LEAVE_JOB_ENV.into(), "0".into()),
        ];
        let x = plist(&d, "l.1").unwrap();
        assert!(
            x.contains("<string>x &amp; y &lt; z &gt; w&#13;\n\t'\"</string>"),
            "{x}"
        );
        assert_eq!(x.matches("FLANKR_DAEMON_LEAVE_JOB").count(), 1, "{x}");
        assert!(!x.contains("<string>0</string>"), "{x}");
    }

    #[test]
    fn a_value_a_plist_cannot_carry_falls_back() {
        // A NUL byte in an argument, an environment value or name, the
        // program, the folder or the log: no plist, so a direct start.
        let mut cases = Vec::new();
        let mut d = sample();
        d.args.push("a\0b".into());
        cases.push(d);
        let mut d = sample();
        d.env.push(("X".into(), "a\0b".into()));
        cases.push(d);
        let mut d = sample();
        d.env.push(("X\0".into(), "v".into()));
        cases.push(d);
        let mut d = sample();
        d.working_dir = PathBuf::from("/tmp/a\0b");
        cases.push(d);
        let mut d = sample();
        d.log = PathBuf::from("/tmp/\u{1}.log");
        cases.push(d);
        // Not UTF-8.
        let mut d = sample();
        d.args.push(OsStr::from_bytes(b"\xff").to_os_string());
        cases.push(d);
        for d in cases {
            assert_eq!(plist(&d, "l.1"), None, "{d:?}");
        }
        assert_eq!(xml_text(OsStr::new("\u{fffe}")), None);
        assert_eq!(xml_text(OsStr::new("é ✓ 𝄞")).as_deref(), Some("é ✓ 𝄞"));
    }

    #[test]
    fn stale_jobs_are_this_prefix_finished_and_older_than_a_minute() {
        let now = 1_700_000_100_000u64;
        let list = "PID\tStatus\tLabel\n\
            -\t0\tdev.flankr.server.1700000000000\n\
            -\t0\tdev.flankr.server.1700000039999\n\
            -\t0\tdev.flankr.server.1700000040000\n\
            -\t0\tdev.flankr.server.1700000090000\n\
            4242\t0\tdev.flankr.server.1700000000001\n\
            -\t-15\tdev.flankr.server.1700000000002\n\
            -\t0\tdev.flankr.serverx.1700000000000\n\
            -\t0\tdev.flankr.server.17000x\n\
            -\t0\tdev.flankr.server.\n\
            -\t0\tdev.flankr.server\n\
            -\t0\tcom.apple.something.1700000000000\n";
        assert_eq!(
            stale_jobs(list, "dev.flankr.server", now),
            [
                "dev.flankr.server.1700000000000",
                "dev.flankr.server.1700000039999",
                "dev.flankr.server.1700000000002",
            ]
        );
    }

    #[test]
    fn start_wants_absolute_paths() {
        let mut d = sample();
        d.program = PathBuf::from("flankr");
        assert_eq!(start(&d).unwrap_err().kind(), io::ErrorKind::InvalidInput);
        let mut d = sample();
        d.working_dir = PathBuf::from(".");
        assert_eq!(start(&d).unwrap_err().kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn leave_job_without_the_marker_does_nothing() {
        // The test process never has the marker unless a helper below set it.
        if std::env::var_os(LEAVE_JOB_ENV).is_none() {
            leave_job().unwrap();
        }
    }

    /// The test binary again, running only `test` with `env` added.
    /// Its output is dropped, so a process it leaves behind holds no pipe
    /// of this test open.
    fn helper(test: &str, env: &[(&str, &OsStr)]) -> std::process::ExitStatus {
        let mut cmd = Command::new(std::env::current_exe().unwrap());
        cmd.args([test, "--exact", "--test-threads=1"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        for (k, v) in env {
            cmd.env(k, v);
        }
        cmd.status().unwrap()
    }

    fn wait_for(path: &Path, text: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let got = std::fs::read_to_string(path).unwrap_or_default();
            if got.contains(text) || Instant::now() > deadline {
                return got;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn alive(pid: i32) -> bool {
        // SAFETY: signal 0 only checks the process exists.
        unsafe { libc::kill(pid, 0) == 0 }
    }

    /// Helper for `direct_start_survives_its_parent`: start the script
    /// directly, record its pid, and exit.
    #[test]
    fn helper_direct_start() {
        let Some(dir) = std::env::var_os("FLANKR_DAEMON_TEST_DIR") else {
            return;
        };
        let dir = PathBuf::from(dir);
        let d = Detached {
            program: PathBuf::from("/bin/sh"),
            args: vec![
                "-c".into(),
                "echo \"started in $(pwd -P) value=$V home=${HOME-unset} input=$(cat)\"; \
                 sleep 1; echo survived"
                    .into(),
            ],
            working_dir: dir.canonicalize().unwrap(),
            env: vec![
                ("PATH".into(), "/usr/bin:/bin".into()),
                ("V".into(), "given".into()),
            ],
            log: dir.join("out.log"),
            label: "dev.flankr.test".into(),
        };
        let Started::Direct { pid } = start_direct(&d).unwrap() else {
            unreachable!()
        };
        std::fs::write(dir.join("pid"), pid.to_string()).unwrap();
    }

    #[test]
    fn direct_start_survives_its_parent() {
        let dir = tempdir();
        std::fs::write(dir.join("out.log"), "earlier\n").unwrap();
        let out = helper(
            "tests::helper_direct_start",
            &[("FLANKR_DAEMON_TEST_DIR", dir.as_os_str())],
        );
        assert!(out.success(), "{out:?}");
        let pid: i32 = std::fs::read_to_string(dir.join("pid"))
            .unwrap()
            .parse()
            .unwrap();
        // The parent is gone; the started program runs in its own session.
        assert!(alive(pid), "pid {pid} is not running");
        // SAFETY: getsid(2) only reads.
        assert_eq!(unsafe { libc::getsid(pid) }, pid);
        let log = wait_for(&dir.join("out.log"), "survived");
        let real = dir.canonicalize().unwrap();
        assert_eq!(
            log,
            format!(
                "earlier\nstarted in {} value=given home=unset input=\nsurvived\n",
                real.display()
            )
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn start_direct_creates_the_log_0600() {
        let dir = tempdir();
        let log = dir.join("fresh.log");
        drop(open_log(&log).unwrap());
        let mode = std::os::unix::fs::PermissionsExt::mode(
            &std::fs::metadata(&log).unwrap().permissions(),
        );
        assert_eq!(mode & 0o777, 0o600);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Helper for `leave_job_forks_into_a_new_session`: with the marker
    /// set, leave the "job"; the child records itself and ends.
    #[test]
    fn helper_leave_job() {
        let Some(dir) = std::env::var_os("FLANKR_DAEMON_TEST_DIR") else {
            return;
        };
        let dir = PathBuf::from(dir);
        // SAFETY: getpid(2) only reads.
        let before = unsafe { libc::getpid() };
        leave_job().unwrap();
        // Only the child gets here; the parent has exited.
        // SAFETY: getpid(2) and getsid(2) only read.
        let (pid, sid) = unsafe { (libc::getpid(), libc::getsid(0)) };
        let marker = std::env::var_os(LEAVE_JOB_ENV).is_some();
        std::thread::sleep(Duration::from_millis(500));
        std::fs::write(
            dir.join("child"),
            format!("parent={before} pid={pid} sid={sid} marker={marker}"),
        )
        .unwrap();
        // SAFETY: end the child without running the test harness's exit.
        unsafe { libc::_exit(0) }
    }

    #[test]
    fn leave_job_forks_into_a_new_session() {
        let dir = tempdir();
        let out = helper(
            "tests::helper_leave_job",
            &[
                ("FLANKR_DAEMON_TEST_DIR", dir.as_os_str()),
                (LEAVE_JOB_ENV, OsStr::new("1")),
            ],
        );
        // The job's process exits at once, 0, while the child still waits
        // out its 500 ms.
        assert!(out.success(), "{out:?}");
        assert!(!dir.join("child").exists());
        let got = wait_for(&dir.join("child"), "marker");
        let field = |name: &str| -> String {
            got.split(' ')
                .find_map(|kv| kv.strip_prefix(&format!("{name}=")))
                .unwrap_or_else(|| panic!("{name} in {got:?}"))
                .to_string()
        };
        assert_ne!(field("pid"), field("parent"), "{got}");
        assert_eq!(field("pid"), field("sid"), "{got}");
        assert_eq!(field("marker"), "false", "{got}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn tempdir() -> PathBuf {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "flankr-daemon-test-{}-{}-{n}",
            std::process::id(),
            unix_ms()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
