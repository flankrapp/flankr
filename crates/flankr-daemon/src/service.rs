//! Persistent user-level services behind a platform-neutral backend seam.

use std::ffi::{OsStr, OsString};
use std::fs::{self, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

/// A long-lived process owned by the current user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceSpec {
    /// Stable service identity (and, on launchd, the job label).
    pub id: String,
    /// Absolute executable path.
    pub program: PathBuf,
    /// Fixed arguments, never interpreted by a shell.
    pub args: Vec<OsString>,
    /// Absolute working directory.
    pub working_dir: PathBuf,
    /// Restricted environment for the process.
    pub env: Vec<(OsString, OsString)>,
    /// Standard output log path.
    pub stdout: PathBuf,
    /// Standard error log path.
    pub stderr: PathBuf,
    /// Whether the supervisor should keep the process alive.
    pub keep_alive: bool,
}

/// State reported by a user-service backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceStatus {
    Running { pid: Option<u32> },
    Loaded,
    Stopped,
    Failed(String),
    Unsupported(String),
}

impl ServiceStatus {
    /// Human-readable status suitable for the CLI.
    pub fn description(&self) -> String {
        match self {
            Self::Running { pid: Some(pid) } => format!("running (pid {pid})"),
            Self::Running { pid: None } => "running".to_string(),
            Self::Loaded => "loaded".to_string(),
            Self::Stopped => "stopped".to_string(),
            Self::Failed(message) => format!("failed: {message}"),
            Self::Unsupported(message) => format!("unsupported: {message}"),
        }
    }
}

/// Platform-neutral operations on a persistent user service.
pub trait UserServiceBackend: Send + Sync {
    fn status(&self, spec: &ServiceSpec) -> io::Result<ServiceStatus>;
    fn ensure(&self, spec: &ServiceSpec) -> io::Result<ServiceStatus>;
    fn stop(&self, spec: &ServiceSpec) -> io::Result<ServiceStatus>;
    fn restart(&self, spec: &ServiceSpec) -> io::Result<ServiceStatus>;
}

/// Backend used on platforms without an installed user-service supervisor.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnsupportedBackend;

const UNSUPPORTED: &str = "persistent user-service backend is not installed on this platform";

impl UserServiceBackend for UnsupportedBackend {
    fn status(&self, _spec: &ServiceSpec) -> io::Result<ServiceStatus> {
        Ok(ServiceStatus::Unsupported(UNSUPPORTED.to_string()))
    }

    fn ensure(&self, _spec: &ServiceSpec) -> io::Result<ServiceStatus> {
        Ok(ServiceStatus::Unsupported(UNSUPPORTED.to_string()))
    }

    fn stop(&self, _spec: &ServiceSpec) -> io::Result<ServiceStatus> {
        Ok(ServiceStatus::Unsupported(UNSUPPORTED.to_string()))
    }

    fn restart(&self, _spec: &ServiceSpec) -> io::Result<ServiceStatus> {
        Ok(ServiceStatus::Unsupported(UNSUPPORTED.to_string()))
    }
}

/// Return the backend for this operating system.
#[cfg(not(target_os = "macos"))]
pub fn platform_backend() -> UnsupportedBackend {
    UnsupportedBackend
}

/// A macOS per-user launchd backend.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone)]
pub struct LaunchdUserBackend {
    uid: u32,
    state_dir: PathBuf,
}

#[cfg(target_os = "macos")]
impl LaunchdUserBackend {
    /// Construct a backend for `gui/<uid>` using the standard private state folder.
    pub fn new(uid: u32, state_dir: PathBuf) -> Self {
        Self { uid, state_dir }
    }

    /// Construct a backend for the current login user.
    pub fn current() -> io::Result<Self> {
        let home = home_dir()?;
        // SAFETY: getuid has no preconditions.
        let uid = unsafe { libc::getuid() };
        Ok(Self::new(uid, home.join(".local/state/flankr/services")))
    }

    fn domain(&self) -> String {
        format!("gui/{}", self.uid)
    }

    fn label(spec: &ServiceSpec) -> io::Result<&str> {
        if spec.id.is_empty()
            || spec.id.contains('/')
            || spec.id.contains('\0')
            || spec.id.chars().any(char::is_whitespace)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid service id",
            ));
        }
        Ok(&spec.id)
    }

    fn plist_path(&self, spec: &ServiceSpec) -> io::Result<PathBuf> {
        let label = Self::label(spec)?;
        Ok(self.state_dir.join(format!("{label}.plist")))
    }

    fn state_path(&self, spec: &ServiceSpec) -> io::Result<PathBuf> {
        let label = Self::label(spec)?;
        Ok(self.state_dir.join(format!("{label}.fingerprint")))
    }

    fn target(&self, spec: &ServiceSpec) -> io::Result<String> {
        Ok(format!("{}/{}", self.domain(), Self::label(spec)?))
    }

    fn run(&self, args: &[&OsStr]) -> io::Result<std::process::Output> {
        Command::new("/bin/launchctl").args(args).output()
    }

    fn loaded(&self, spec: &ServiceSpec) -> io::Result<Option<ServiceStatus>> {
        let target = self.target(spec)?;
        let output = self.run(&[OsStr::new("print"), OsStr::new(&target)])?;
        if !output.status.success() {
            return Ok(None);
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let pid = text.lines().find_map(|line| {
            let value = line.trim().strip_prefix("pid = ")?;
            value.parse().ok()
        });
        Ok(Some(
            if pid.is_some() || text.lines().any(|line| line.trim() == "state = running") {
                ServiceStatus::Running { pid }
            } else {
                ServiceStatus::Loaded
            },
        ))
    }

    fn bootstrap(&self, spec: &ServiceSpec) -> io::Result<ServiceStatus> {
        validate_spec(spec)?;
        private_dir(&self.state_dir)?;
        ensure_private_file(&spec.stdout)?;
        ensure_private_file(&spec.stderr)?;
        let plist = self.plist_path(spec)?;
        write_private(&plist, &plist_text(spec)?)?;
        let domain = self.domain();
        let output = self.run(&[
            OsStr::new("bootstrap"),
            OsStr::new(&domain),
            plist.as_os_str(),
        ])?;
        if !output.status.success() {
            return Err(command_error("launchctl bootstrap", &output));
        }
        let target = self.target(spec)?;
        let output = self.run(&[
            OsStr::new("kickstart"),
            OsStr::new("-k"),
            OsStr::new(&target),
        ])?;
        if !output.status.success() {
            return Err(command_error("launchctl kickstart", &output));
        }
        write_private(&self.state_path(spec)?, &fingerprint(spec))?;
        self.loaded(spec)
            .map(|state| state.unwrap_or(ServiceStatus::Loaded))
    }
}

#[cfg(target_os = "macos")]
impl UserServiceBackend for LaunchdUserBackend {
    fn status(&self, spec: &ServiceSpec) -> io::Result<ServiceStatus> {
        Ok(self.loaded(spec)?.unwrap_or(ServiceStatus::Stopped))
    }

    fn ensure(&self, spec: &ServiceSpec) -> io::Result<ServiceStatus> {
        let current = self.loaded(spec)?;
        let expected = fingerprint(spec);
        let saved = self
            .state_path(spec)
            .ok()
            .and_then(|path| fs::read_to_string(path).ok());
        if current.is_some() && saved.as_deref() == Some(expected.as_str()) {
            let target = self.target(spec)?;
            let output = self.run(&[
                OsStr::new("kickstart"),
                OsStr::new("-k"),
                OsStr::new(&target),
            ])?;
            if !output.status.success() {
                return Err(command_error("launchctl kickstart", &output));
            }
            return self
                .loaded(spec)
                .map(|state| state.unwrap_or(ServiceStatus::Loaded));
        }
        if current.is_some() {
            let _ = self.stop(spec)?;
        }
        self.bootstrap(spec)
    }

    fn stop(&self, spec: &ServiceSpec) -> io::Result<ServiceStatus> {
        let target = self.target(spec)?;
        if self.loaded(spec)?.is_some() {
            let output = self.run(&[OsStr::new("bootout"), OsStr::new(&target)])?;
            if !output.status.success() {
                return Err(command_error("launchctl bootout", &output));
            }
        }
        let _ = fs::remove_file(self.plist_path(spec)?);
        let _ = fs::remove_file(self.state_path(spec)?);
        Ok(ServiceStatus::Stopped)
    }

    fn restart(&self, spec: &ServiceSpec) -> io::Result<ServiceStatus> {
        let _ = self.stop(spec)?;
        self.ensure(spec)
    }
}

#[cfg(target_os = "macos")]
pub fn platform_backend() -> io::Result<LaunchdUserBackend> {
    LaunchdUserBackend::current()
}

fn validate_spec(spec: &ServiceSpec) -> io::Result<()> {
    if spec.id.is_empty() || spec.id.contains('/') || spec.id.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid service id",
        ));
    }
    if !spec.program.is_absolute() || !spec.working_dir.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "service program and working directory must be absolute",
        ));
    }
    for path in [&spec.stdout, &spec.stderr] {
        if !path.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "service logs must be absolute",
            ));
        }
    }
    Ok(())
}

fn home_dir() -> io::Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))
}

fn private_dir(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

fn write_private(path: &Path, text: &str) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    use std::io::Write;
    file.write_all(text.as_bytes())?;
    file.set_permissions(fs::Permissions::from_mode(0o600))
}

fn ensure_private_file(path: &Path) -> io::Result<()> {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))
}

fn fingerprint(spec: &ServiceSpec) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    spec.id.hash(&mut hasher);
    spec.program.hash(&mut hasher);
    spec.args.hash(&mut hasher);
    spec.working_dir.hash(&mut hasher);
    spec.env.hash(&mut hasher);
    spec.stdout.hash(&mut hasher);
    spec.stderr.hash(&mut hasher);
    spec.keep_alive.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(target_os = "macos")]
fn plist_text(spec: &ServiceSpec) -> io::Result<String> {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>",
    );
    field(&mut out, "Label", &spec.id)?;
    out.push_str("<key>ProgramArguments</key><array>");
    value(&mut out, &spec.program)?;
    for arg in &spec.args {
        value(&mut out, arg)?;
    }
    out.push_str("</array>");
    field(&mut out, "WorkingDirectory", &spec.working_dir)?;
    out.push_str("<key>EnvironmentVariables</key><dict>");
    for (key, val) in &spec.env {
        plist_key(&mut out, key)?;
        value(&mut out, val)?;
    }
    out.push_str("</dict>");
    field(&mut out, "StandardOutPath", &spec.stdout)?;
    field(&mut out, "StandardErrorPath", &spec.stderr)?;
    field(&mut out, "StandardInPath", Path::new("/dev/null"))?;
    out.push_str("<key>RunAtLoad</key><true/>");
    out.push_str(if spec.keep_alive {
        "<key>KeepAlive</key><true/>"
    } else {
        "<key>KeepAlive</key><false/>"
    });
    out.push_str("</dict></plist>\n");
    Ok(out)
}

#[cfg(target_os = "macos")]
fn field<T: AsRef<OsStr> + ?Sized>(out: &mut String, key: &str, value_: &T) -> io::Result<()> {
    xml_text(out, key, "<key>", "</key>")?;
    value(out, value_)
}

#[cfg(target_os = "macos")]
fn plist_key<T: AsRef<OsStr> + ?Sized>(out: &mut String, key: &T) -> io::Result<()> {
    xml_text(out, key, "<key>", "</key>")
}

#[cfg(target_os = "macos")]
fn value<T: AsRef<OsStr> + ?Sized>(out: &mut String, value_: &T) -> io::Result<()> {
    xml_text(out, value_, "<string>", "</string>")
}

#[cfg(target_os = "macos")]
fn xml_text<T: AsRef<OsStr> + ?Sized>(
    out: &mut String,
    value_: &T,
    opening: &str,
    closing: &str,
) -> io::Result<()> {
    let text = value_.as_ref().to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "service plist value is not UTF-8",
        )
    })?;
    if text
        .chars()
        .any(|c| c == '\0' || (c.is_control() && c != '\n' && c != '\t'))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "service plist value has invalid XML characters",
        ));
    }
    out.push_str(opening);
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\'' => out.push_str("&apos;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out.push_str(closing);
    Ok(())
}

#[cfg(target_os = "macos")]
fn command_error(command: &str, output: &std::process::Output) -> io::Error {
    let stderr = String::from_utf8_lossy(&output.stderr);
    io::Error::other(format!("{command} failed: {}", stderr.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> ServiceSpec {
        ServiceSpec {
            id: "dev.example.service".to_string(),
            program: "/bin/example".into(),
            args: vec!["--fixed".into(), "x y".into()],
            working_dir: "/tmp".into(),
            env: vec![("HOME".into(), "/home/test".into())],
            stdout: "/tmp/out.log".into(),
            stderr: "/tmp/err.log".into(),
            keep_alive: true,
        }
    }

    #[test]
    fn unsupported_backend_is_explicit() {
        let status = UnsupportedBackend.status(&spec()).unwrap();
        assert!(matches!(status, ServiceStatus::Unsupported(_)));
        assert!(status.description().contains("unsupported"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn plist_contains_fixed_process_contract() {
        let text = plist_text(&spec()).unwrap();
        assert!(
            text.contains("<key>RunAtLoad</key><true/>")
                && text.contains("<key>KeepAlive</key><true/>")
        );
        assert!(text.contains(
            "<key>EnvironmentVariables</key><dict><key>HOME</key><string>/home/test</string></dict>"
        ));
        assert!(!text.contains("<string>HOME</string>"));
        assert!(text.contains("&amp;") || !text.contains("&"));
    }
}
