//! Where the Gemini key is read from, and where `flankr key` stores it
//! (docs/design/20261102_02_GEMINI_KEY_STORE_PLAN.md).
//!
//! The order (decision 1), first found wins: the backend's `api_key_file`
//! (absolute, or against its config file's folder), the macOS Keychain
//! (`dev.flankr`/`gemini`), flankr's config-folder file
//! (`…/flankr/secrets/gemini_api_key`), then `GEMINI_API_KEY`. The working
//! folder is never looked in. No key text is ever put in a message here.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{ApiKey, gemini};

/// The Keychain item's service and account (decision 1.2).
pub const KEYCHAIN_SERVICE: &str = "dev.flankr";
pub const KEYCHAIN_ACCOUNT: &str = "gemini";
/// `FLANKR_KEY_STORE=file`: the Keychain is never looked in nor written,
/// whatever the system (tests; a login keychain that stays locked).
pub const STORE_ENV: &str = "FLANKR_KEY_STORE";

/// What a source holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    Found(ApiKey),
    None,
    /// The Keychain is locked (or may not ask): the next source is tried.
    Locked,
    /// There, but not to be read: why, naming the place and the fix.
    Refused(String),
}

impl Lookup {
    /// `found`, `none`, `locked` or `refused: <why>` (decision 5).
    pub fn word(&self) -> String {
        match self {
            Lookup::Found(_) => "found".to_string(),
            Lookup::None => "none".to_string(),
            Lookup::Locked => "locked".to_string(),
            Lookup::Refused(why) => format!("refused: {why}"),
        }
    }
}

/// A place a key is kept (decision 8): the Keychain, a file, or a fake.
pub trait KeySource: Send + Sync {
    /// The place as the log and `flankr key status` name it.
    fn name(&self) -> String;
    fn get(&self) -> Lookup;
    /// Store `key`, replacing an earlier one. Errors never hold the key.
    fn set(&self, key: &ApiKey) -> Result<(), String>;
}

/// A key file, alone, mode 0600 (decision 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileKeySource {
    pub path: PathBuf,
}

impl KeySource for FileKeySource {
    fn name(&self) -> String {
        format!("file {}", tilde(&self.path))
    }

    fn get(&self) -> Lookup {
        match super::read_key_file(&self.path) {
            Ok(Some(key)) => Lookup::Found(ApiKey(key)),
            Ok(None) => Lookup::None,
            Err(e) => Lookup::Refused(e),
        }
    }

    fn set(&self, key: &ApiKey) -> Result<(), String> {
        write_key_file(&self.path, key)
    }
}

/// Write `key` to `path` alone, the folder made 0700 and the file 0600 as
/// they are made (decision 2); an earlier file is replaced whole.
pub fn write_key_file(path: &Path, key: &ApiKey) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    let dir = path
        .parent()
        .ok_or_else(|| format!("{} has no folder", path.display()))?;
    if !dir.exists() {
        crate::daemon::create_private_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("new");
    let _ = std::fs::remove_file(&tmp);
    let result = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut f| {
            f.write_all(key.expose().as_bytes())?;
            f.write_all(b"\n")?;
            f.sync_all()
        })
        .and_then(|()| std::fs::rename(&tmp, path));
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("{}: {e}", path.display()));
    }
    Ok(())
}

/// The login keychain's generic password `dev.flankr`/`gemini`, through
/// the Security framework, so the key is never on a command line
/// (decision 6). Only the daemon's and `flankr key`'s own setup makes one;
/// tests never do.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keychain;

#[cfg(target_os = "macos")]
impl KeySource for Keychain {
    fn name(&self) -> String {
        keychain_name()
    }

    fn get(&self) -> Lookup {
        use security_framework::passwords::get_generic_password;
        match get_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => match parse_key(&text) {
                    Ok(Some(key)) => Lookup::Found(key),
                    Ok(None) => Lookup::None,
                    Err(why) => Lookup::Refused(format!(
                        "the Keychain item {} {why}; replace it with `flankr key set gemini`",
                        keychain_name()
                    )),
                },
                Err(_) => Lookup::Refused(format!(
                    "the Keychain item {} is not text; replace it with `flankr key set gemini`",
                    keychain_name()
                )),
            },
            Err(e) => match e.code() {
                ERR_SEC_ITEM_NOT_FOUND => Lookup::None,
                ERR_SEC_INTERACTION_NOT_ALLOWED | ERR_SEC_AUTH_FAILED => Lookup::Locked,
                code => Lookup::Refused(format!(
                    "the Keychain item {}: {e} ({code}); `flankr key set gemini --file` keeps \
                     the key in flankr's config folder instead",
                    keychain_name()
                )),
            },
        }
    }

    fn set(&self, key: &ApiKey) -> Result<(), String> {
        security_framework::passwords::set_generic_password(
            KEYCHAIN_SERVICE,
            KEYCHAIN_ACCOUNT,
            key.expose().as_bytes(),
        )
        .map_err(|e| format!("the Keychain item {}: {e}", keychain_name()))
    }
}

#[cfg(target_os = "macos")]
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
#[cfg(target_os = "macos")]
const ERR_SEC_INTERACTION_NOT_ALLOWED: i32 = -25308;
#[cfg(target_os = "macos")]
const ERR_SEC_AUTH_FAILED: i32 = -25293;

/// `Keychain (dev.flankr/gemini)`.
pub fn keychain_name() -> String {
    format!("Keychain ({KEYCHAIN_SERVICE}/{KEYCHAIN_ACCOUNT})")
}

/// Why the Keychain is not used in this process, or `None` when it is:
/// another system, `FLANKR_KEY_STORE=file`, or a `HOME` that is not the
/// account's own home folder (a test's temporary one), so nothing but a
/// real run reaches the login keychain.
pub fn keychain_skipped() -> Option<String> {
    if !cfg!(target_os = "macos") {
        return Some("not on this system".to_string());
    }
    if std::env::var(STORE_ENV).is_ok_and(|v| v.trim() == "file") {
        return Some(format!("{STORE_ENV}=file"));
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match (home, account_home()) {
        (Some(home), Some(own)) if same_dir(&home, &own) => None,
        _ => Some("HOME is not this account's home folder".to_string()),
    }
}

fn same_dir(a: &Path, b: &Path) -> bool {
    a == b
        || matches!(
            (a.canonicalize(), b.canonicalize()),
            (Ok(a), Ok(b)) if a == b
        )
}

/// The account's home folder from the user database, not `HOME`.
fn account_home() -> Option<PathBuf> {
    use std::ffi::CStr;
    use std::os::unix::ffi::OsStrExt;
    let mut buf = vec![0 as libc::c_char; 4096];
    let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut result: *mut libc::passwd = std::ptr::null_mut();
    // SAFETY: the buffers outlive the call, and `pw_dir` is read only
    // while `buf` holds it.
    let rc = unsafe {
        libc::getpwuid_r(
            libc::getuid(),
            &mut pwd,
            buf.as_mut_ptr(),
            buf.len(),
            &mut result,
        )
    };
    if rc != 0 || result.is_null() || pwd.pw_dir.is_null() {
        return None;
    }
    let dir = unsafe { CStr::from_ptr(pwd.pw_dir) };
    Some(PathBuf::from(std::ffi::OsStr::from_bytes(dir.to_bytes())))
}

/// The Keychain for this process, unless [`keychain_skipped`] says why not.
pub fn keychain() -> Option<Arc<dyn KeySource>> {
    if keychain_skipped().is_some() {
        return None;
    }
    #[cfg(target_os = "macos")]
    {
        Some(Arc::new(Keychain))
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// flankr's config folder: `$XDG_CONFIG_HOME/flankr`, else
/// `~/.config/flankr`.
pub fn flankr_config_dir() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        let xdg = PathBuf::from(xdg);
        if xdg.is_absolute() {
            return Some(xdg.join("flankr"));
        }
    }
    let home = std::env::var_os("HOME").filter(|v| !v.is_empty())?;
    Some(PathBuf::from(home).join(".config").join("flankr"))
}

/// The config-folder key file (decision 1.3).
pub fn secrets_file() -> Option<PathBuf> {
    flankr_config_dir().map(|dir| dir.join(gemini::KEY_FILE))
}

/// `text` as a key: trimmed, `None` when empty; refused, with why, when it
/// is not one short token (a service-account JSON, several lines).
pub fn parse_key(text: &str) -> Result<Option<ApiKey>, &'static str> {
    let key = text.trim();
    if key.is_empty() {
        return Ok(None);
    }
    let looks_like_key =
        !key.contains(char::is_whitespace) && !key.starts_with(['{', '[']) && key.len() <= 256;
    if !looks_like_key {
        return Err("does not hold an API key (a service-account file is not an API key)");
    }
    Ok(Some(ApiKey(key.to_string())))
}

/// `path` with the home folder as `~`.
pub fn tilde(path: &Path) -> String {
    if let Some(home) = std::env::var_os("HOME").filter(|h| !h.is_empty())
        && let Ok(rest) = path.strip_prefix(Path::new(&home))
    {
        return format!("~/{}", rest.display());
    }
    path.display().to_string()
}
