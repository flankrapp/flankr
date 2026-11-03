//! `flankr key set|import|status` (docs/design/20261102_02_GEMINI_KEY_STORE_PLAN.md,
//! decisions 3 to 5): the Gemini key stored in the Keychain or flankr's
//! config folder without reaching a command line, a shell history or a
//! log. Nothing here ever prints any part of a key.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::llm::keys::{self, FileKeySource, KeySource, Lookup};
use crate::llm::{KeyEnv, LlmSources, gemini};

pub const USAGE: &str = "\
flankr key set gemini [--file]
flankr key import gemini <file> [--file]
flankr key status [--config <file>]
  set reads the key from the terminal with echo off (or one line from
  stdin), import from a file; both store it in the macOS Keychain
  (dev.flankr/gemini), or with --file (and on other systems) in
  ~/.config/flankr/secrets/gemini_api_key. status says where the daemon
  finds a key, never the key.";

/// The parsed command line after `key`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct KeyArgs {
    pub words: Vec<String>,
    pub file_store: bool,
    pub config: Option<PathBuf>,
}

/// `argv` after `key`, or the usage error.
pub fn parse(argv: &[String]) -> Result<KeyArgs, String> {
    let mut args = KeyArgs::default();
    let mut it = argv.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--file" => args.file_store = true,
            "--config" => match it.next() {
                Some(path) => args.config = Some(PathBuf::from(path)),
                None => return Err("--config needs a file".to_string()),
            },
            flag if flag.starts_with('-') => return Err(format!("unknown argument '{flag}'")),
            word => args.words.push(word.to_string()),
        }
    }
    Ok(args)
}

/// Runs `flankr key` with `argv` (after `key`). `env` is where keys are
/// looked for (with the Keychain when it is used); `read_key` reads the
/// key for `set`. Returns the exit code: 0, 1 when refused, 2 for usage.
pub fn run(
    argv: &[String],
    env: &KeyEnv,
    sources: &LlmSources,
    read_key: &mut dyn FnMut() -> std::io::Result<String>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let args = match parse(argv) {
        Ok(args) => args,
        Err(e) => {
            let _ = writeln!(err, "flankr key: {e}\n{USAGE}");
            return 2;
        }
    };
    let words: Vec<&str> = args.words.iter().map(String::as_str).collect();
    let result = match words.as_slice() {
        ["set", "gemini"] => match read_key() {
            Ok(text) => store(&text, None, env, args.file_store, out),
            Err(e) => Err(format!("cannot read the key: {e}")),
        },
        ["import", "gemini", file] => import(Path::new(file), env, args.file_store, out),
        ["status"] | ["status", "gemini"] if !args.file_store => {
            status(env, sources, out);
            Ok(())
        }
        _ => {
            let _ = writeln!(err, "{USAGE}");
            return 2;
        }
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(err, "flankr key: {e}");
            1
        }
    }
}

/// Where `set` and `import` store the key: the Keychain unless `--file`
/// or it is not used, else the config-folder file.
fn store_for(env: &KeyEnv, file_store: bool) -> Result<Arc<dyn KeySource>, String> {
    if !file_store && let Some(keychain) = &env.keychain {
        return Ok(keychain.clone());
    }
    match &env.secrets_file {
        Some(path) => Ok(Arc::new(FileKeySource { path: path.clone() })),
        None => Err("neither HOME nor XDG_CONFIG_HOME is set: no config folder".to_string()),
    }
}

/// Decision 3: `text` trimmed, refused empty or not a key, then stored.
fn store(
    text: &str,
    from: Option<&Path>,
    env: &KeyEnv,
    file_store: bool,
    out: &mut dyn Write,
) -> Result<(), String> {
    let what = match from {
        Some(file) => format!("{} ", file.display()),
        None => "the key given ".to_string(),
    };
    let key = match keys::parse_key(text) {
        Ok(Some(key)) => key,
        Ok(None) => return Err(format!("{what}is empty; nothing was stored")),
        Err(why) => return Err(format!("{what}{why}; nothing was stored")),
    };
    let store = store_for(env, file_store)?;
    store.set(&key)?;
    let _ = writeln!(out, "Stored the Gemini key in {}.", store.name());
    if env.keychain.is_none()
        && cfg!(target_os = "macos")
        && !file_store
        && let Some(why) = keys::keychain_skipped()
    {
        let _ = writeln!(out, "The Keychain is not used: {why}.");
    }
    if let Some(file) = from {
        let _ = writeln!(
            out,
            "{} can now be deleted; flankr does not read it.",
            file.display()
        );
    }
    Ok(())
}

/// Decision 4: the key from `file` (whatever its mode: it is being moved),
/// stored as `set` does; the file is left where it is.
fn import(file: &Path, env: &KeyEnv, file_store: bool, out: &mut dyn Write) -> Result<(), String> {
    let meta = std::fs::metadata(file).map_err(|e| format!("{}: {e}", file.display()))?;
    if !meta.is_file() || meta.len() > 64 * 1024 {
        return Err(format!(
            "{} is not a key file; nothing was stored",
            file.display()
        ));
    }
    let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    store(&text, Some(file), env, file_store, out)
}

/// Decision 5: each source in decision 1's order and what it holds, then
/// which one the daemon would use, per `gemini` backend.
fn status(env: &KeyEnv, sources: &LlmSources, out: &mut dyn Write) {
    let mut backends = crate::llm::gemini_backends(sources);
    if backends.is_empty() {
        backends.push(("gemini".to_string(), None));
    }
    let mut lines = Vec::new();
    let mut own = Vec::new();
    for (name, file) in &backends {
        let lookup = file.as_deref().map(|file| {
            let source = FileKeySource {
                path: file_path(env, file),
            };
            let lookup = source.get();
            lines.push(format!(
                "api_key_file {} ([llm.backends.{name}]): {}",
                keys::tilde(&source.path),
                lookup.word()
            ));
            (
                format!("api_key_file {}", keys::tilde(&source.path)),
                lookup,
            )
        });
        own.push(lookup);
    }
    if own.iter().all(Option::is_none) {
        lines.push("api_key_file: not set".to_string());
    }
    let shared = env.lookups(None, true);
    if env.keychain.is_none() {
        let why = keys::keychain_skipped().unwrap_or_else(|| "not given".to_string());
        lines.push(format!("{}: skipped ({why})", keys::keychain_name()));
    }
    if env.secrets_file.is_none() {
        lines.push(format!(
            "file {}: no config folder (neither HOME nor XDG_CONFIG_HOME is set)",
            gemini::KEY_FILE
        ));
    }
    for (name, lookup) in &shared {
        let mut line = format!("{name}: {}", lookup.word());
        if name == gemini::KEY_ENV && matches!(lookup, Lookup::Found(_)) {
            line.push_str(" (in this shell: only `flankr serve --foreground` reads it)");
        }
        lines.push(line);
    }
    let single = backends.len() == 1;
    for ((name, _), own) in backends.iter().zip(&own) {
        let used = own
            .iter()
            .chain(&shared)
            .find(|(_, l)| matches!(l, Lookup::Found(_) | Lookup::Refused(_)));
        let what = match used {
            Some((source, Lookup::Found(_))) => source.clone(),
            Some((source, _)) => format!("none ({source} is refused)"),
            None => "none; run `flankr key set gemini`".to_string(),
        };
        if single {
            lines.push(format!("The daemon would use: {what}"));
        } else {
            lines.push(format!("[llm.backends.{name}] would use: {what}"));
        }
    }
    for line in lines {
        let _ = writeln!(out, "{line}");
    }
}

fn file_path(env: &KeyEnv, file: &str) -> PathBuf {
    let path = PathBuf::from(file);
    if path.is_relative() {
        env.config_dir.join(path)
    } else {
        path
    }
}

/// One line from the terminal with echo off, or from stdin when it is not
/// a terminal (decision 3). The prompt goes to stderr.
pub fn read_key_line() -> std::io::Result<String> {
    let stdin = std::io::stdin();
    // SAFETY: isatty only inspects the descriptor.
    if unsafe { libc::isatty(libc::STDIN_FILENO) } != 1 {
        let mut line = String::new();
        stdin.lock().read_line(&mut line)?;
        return Ok(line);
    }
    eprint!("Gemini API key (not shown): ");
    let _ = std::io::stderr().flush();
    let _echo = EchoOff::new()?;
    let mut line = String::new();
    let read = stdin.lock().read_line(&mut line);
    eprintln!();
    read.map(|_| line)
}

/// The terminal's echo off while it lives.
struct EchoOff(libc::termios);

impl EchoOff {
    fn new() -> std::io::Result<EchoOff> {
        // SAFETY: a zeroed termios is filled in by tcgetattr before use.
        let mut term: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(libc::STDIN_FILENO, &mut term) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        let saved = term;
        term.c_lflag &= !libc::ECHO;
        term.c_lflag |= libc::ECHONL;
        if unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &term) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(EchoOff(saved))
    }
}

impl Drop for EchoOff {
    fn drop(&mut self) {
        // SAFETY: restores the attributes read in `new`.
        unsafe {
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::ApiKey;
    use std::sync::Mutex;

    /// A Keychain stand-in: what it holds, or that it is locked.
    #[derive(Default)]
    struct Fake {
        key: Mutex<Option<ApiKey>>,
        locked: bool,
    }

    impl KeySource for Fake {
        fn name(&self) -> String {
            "Keychain (fake)".to_string()
        }
        fn get(&self) -> Lookup {
            if self.locked {
                return Lookup::Locked;
            }
            match &*self.key.lock().unwrap() {
                Some(key) => Lookup::Found(key.clone()),
                None => Lookup::None,
            }
        }
        fn set(&self, key: &ApiKey) -> Result<(), String> {
            *self.key.lock().unwrap() = Some(key.clone());
            Ok(())
        }
    }

    fn run_with(argv: &[&str], env: &KeyEnv, input: &str) -> (i32, String, String) {
        let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let input = input.to_string();
        let code = run(
            &argv,
            env,
            &LlmSources::default(),
            &mut || Ok(input.clone()),
            &mut out,
            &mut err,
        );
        (
            code,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    #[test]
    fn set_stores_in_the_keychain_when_there_is_one_else_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Arc::new(Fake::default());
        let env = KeyEnv {
            keychain: Some(fake.clone()),
            ..KeyEnv::in_dir(dir.path())
        };
        let (code, out, err) = run_with(&["set", "gemini"], &env, "  test-key-1\n");
        assert_eq!(code, 0, "{err}");
        assert_eq!(out, "Stored the Gemini key in Keychain (fake).\n");
        assert_eq!(fake.get(), Lookup::Found(ApiKey::new("test-key-1")));
        assert!(!dir.path().join(gemini::KEY_FILE).exists());

        // --file: the config-folder file, 0600 in a 0700 folder.
        let (code, out, _) = run_with(&["set", "gemini", "--file"], &env, "test-key-2");
        assert_eq!(code, 0);
        assert!(out.starts_with("Stored the Gemini key in file "), "{out}");
        assert!(!out.contains("test-key"), "{out}");
        assert_mode(&dir.path().join(gemini::KEY_FILE), 0o600);
        assert_mode(&dir.path().join("secrets"), 0o700);
        assert_eq!(
            std::fs::read_to_string(dir.path().join(gemini::KEY_FILE)).unwrap(),
            "test-key-2\n"
        );
    }

    fn assert_mode(path: &Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        let got = std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(got, mode, "{}", path.display());
    }

    #[test]
    fn an_empty_or_odd_key_is_refused_and_never_echoed() {
        let dir = tempfile::tempdir().unwrap();
        let env = KeyEnv::in_dir(dir.path());
        let (code, out, err) = run_with(&["set", "gemini"], &env, "  \n");
        assert_eq!(code, 1);
        assert!(out.is_empty());
        assert!(err.contains("is empty"), "{err}");
        let (code, _, err) = run_with(&["set", "gemini"], &env, "test-key with spaces\n");
        assert_eq!(code, 1);
        assert!(!err.contains("test-key"), "{err}");
        assert!(!dir.path().join(gemini::KEY_FILE).exists());
    }

    #[test]
    fn status_names_each_source_and_the_one_used() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Arc::new(Fake {
            locked: true,
            ..Fake::default()
        });
        let env = KeyEnv {
            keychain: Some(fake),
            gemini_api_key: Some(ApiKey::new("test-key-env")),
            ..KeyEnv::in_dir(dir.path())
        };
        let (code, out, _) = run_with(&["status"], &env, "");
        assert_eq!(code, 0);
        let file = keys::tilde(&dir.path().join(gemini::KEY_FILE));
        assert_eq!(
            out,
            format!(
                "api_key_file: not set\n\
                 Keychain (fake): locked\n\
                 file {file}: none\n\
                 GEMINI_API_KEY: found (in this shell: only `flankr serve --foreground` reads it)\n\
                 The daemon would use: GEMINI_API_KEY\n"
            )
        );
        assert!(!out.contains("test-key"));
    }

    #[test]
    fn usage_for_anything_else() {
        let env = KeyEnv::none();
        for argv in [&[][..], &["set"], &["set", "openai"], &["import", "gemini"]] {
            assert_eq!(run_with(argv, &env, "").0, 2, "{argv:?}");
        }
        assert_eq!(run_with(&["status", "--bogus"], &env, "").0, 2);
    }
}
