//! `flankr key set|import|status`
//! (docs/design/20261102_02_GEMINI_KEY_STORE_PLAN.md, step 2).
//!
//! Every run is the built binary with an environment cleared of everything
//! but a temporary `HOME` and `XDG_CONFIG_HOME`, `PATH` and
//! `FLANKR_KEY_STORE=file`. The Keychain is never reached, twice over:
//! `FLANKR_KEY_STORE=file` turns it off, and so does a `HOME` that is not
//! the account's own home folder. The keys are fake (`test-key-…`).

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_flankr");

struct Rig {
    dir: tempfile::TempDir,
}

impl Rig {
    fn new() -> Rig {
        Rig {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    fn home(&self) -> PathBuf {
        self.dir.path().canonicalize().unwrap()
    }

    fn xdg(&self) -> PathBuf {
        self.home().join("xdg")
    }

    /// Where `--file` (and the forced file store) keeps the key.
    fn secrets(&self) -> PathBuf {
        self.xdg().join("flankr/secrets/gemini_api_key")
    }

    /// `flankr key <args>` with `stdin` piped in (not a terminal).
    fn run(&self, args: &[&str], stdin: &str, extra: &[(&str, &str)]) -> Output {
        use std::io::Write;
        let home = self.home();
        let mut cmd = Command::new(BIN);
        cmd.arg("key")
            .args(args)
            .current_dir(&home)
            .env_clear()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", self.xdg())
            .env("PATH", "/usr/bin:/bin")
            .env("FLANKR_KEY_STORE", "file")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in extra {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Neither stream holds any part of a fake key.
fn assert_no_key(out: &Output) {
    let all = format!("{}{}", text(&out.stdout), text(&out.stderr));
    assert!(!all.contains("test-key"), "{all}");
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn set_reads_one_line_from_stdin_and_stores_it_in_the_config_folder() {
    let rig = Rig::new();
    let out = rig.run(
        &["set", "gemini"],
        "  test-key-from-stdin  \nsecond line\n",
        &[],
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_no_key(&out);
    assert_eq!(
        text(&out.stdout),
        format!(
            "Stored the Gemini key in file ~/xdg/flankr/secrets/gemini_api_key.\n{}",
            if cfg!(target_os = "macos") {
                "The Keychain is not used: FLANKR_KEY_STORE=file.\n"
            } else {
                ""
            }
        )
    );
    assert_eq!(
        std::fs::read_to_string(rig.secrets()).unwrap(),
        "test-key-from-stdin\n"
    );
    assert_eq!(mode(&rig.secrets()), 0o600);
    assert_eq!(mode(rig.secrets().parent().unwrap()), 0o700);

    // Again: the earlier key is replaced.
    let out = rig.run(&["set", "gemini", "--file"], "test-key-second\n", &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_no_key(&out);
    assert_eq!(
        std::fs::read_to_string(rig.secrets()).unwrap(),
        "test-key-second\n"
    );
}

#[test]
fn an_empty_key_is_refused_and_nothing_is_stored() {
    let rig = Rig::new();
    for input in ["", "\n", "   \n"] {
        let out = rig.run(&["set", "gemini"], input, &[]);
        assert_eq!(out.status.code(), Some(1), "{input:?} {out:?}");
        assert!(text(&out.stdout).is_empty());
        assert!(text(&out.stderr).contains("is empty"), "{out:?}");
    }
    // Not one token (a pasted service-account file): refused, not echoed.
    let out = rig.run(&["set", "gemini"], "{\"test-key\": 1}\n", &[]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert_no_key(&out);
    assert!(!rig.secrets().exists());
}

#[test]
fn import_stores_the_file_s_key_and_leaves_the_file() {
    let rig = Rig::new();
    // The old place, in a project folder, with the loose mode it may have.
    let private = rig.home().join("project/.private");
    std::fs::create_dir_all(&private).unwrap();
    let old = private.join("gemini_api_key");
    std::fs::write(&old, "test-key-imported\n").unwrap();
    std::fs::set_permissions(&old, std::fs::Permissions::from_mode(0o644)).unwrap();

    let out = rig.run(&["import", "gemini", old.to_str().unwrap()], "", &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_no_key(&out);
    let stdout = text(&out.stdout);
    assert!(
        stdout.starts_with("Stored the Gemini key in file ~/xdg/flankr/secrets/gemini_api_key.\n"),
        "{stdout}"
    );
    assert!(
        stdout.ends_with(&format!(
            "{} can now be deleted; flankr does not read it.\n",
            old.display()
        )),
        "{stdout}"
    );
    assert!(old.exists(), "not deleted");
    assert_eq!(
        std::fs::read_to_string(rig.secrets()).unwrap(),
        "test-key-imported\n"
    );

    // A missing or empty file stores nothing.
    let out = rig.run(&["import", "gemini", "nowhere"], "", &[]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    std::fs::write(&old, "\n").unwrap();
    let out = rig.run(&["import", "gemini", old.to_str().unwrap()], "", &[]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(text(&out.stderr).contains("is empty"), "{out:?}");
    assert_eq!(
        std::fs::read_to_string(rig.secrets()).unwrap(),
        "test-key-imported\n"
    );
}

#[test]
fn status_names_each_source_and_never_the_key() {
    let rig = Rig::new();
    let keychain = if cfg!(target_os = "macos") {
        "Keychain (dev.flankr/gemini): skipped (FLANKR_KEY_STORE=file)"
    } else {
        "Keychain (dev.flankr/gemini): skipped (not on this system)"
    };
    let out = rig.run(&["status"], "", &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        text(&out.stdout),
        format!(
            "api_key_file: not set\n\
             {keychain}\n\
             file ~/xdg/flankr/secrets/gemini_api_key: none\n\
             GEMINI_API_KEY: none\n\
             The daemon would use: none; run `flankr key set gemini`\n"
        )
    );

    let out = rig.run(&["set", "gemini"], "test-key-status\n", &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let out = rig.run(
        &["status"],
        "",
        &[("GEMINI_API_KEY", "test-key-in-the-shell")],
    );
    assert_no_key(&out);
    assert_eq!(
        text(&out.stdout),
        format!(
            "api_key_file: not set\n\
             {keychain}\n\
             file ~/xdg/flankr/secrets/gemini_api_key: found\n\
             GEMINI_API_KEY: found (in this shell: only `flankr serve --foreground` reads it)\n\
             The daemon would use: file ~/xdg/flankr/secrets/gemini_api_key\n"
        )
    );

    // Readable by others: refused, with the fix.
    std::fs::set_permissions(rig.secrets(), std::fs::Permissions::from_mode(0o644)).unwrap();
    let out = rig.run(&["status"], "", &[]);
    assert_no_key(&out);
    let stdout = text(&out.stdout);
    assert!(stdout.contains("chmod 600"), "{stdout}");
    assert!(
        stdout.contains(
            "The daemon would use: none (file ~/xdg/flankr/secrets/gemini_api_key is refused)"
        ),
        "{stdout}"
    );
}

#[test]
fn status_reads_an_api_key_file_next_to_its_config() {
    let rig = Rig::new();
    let config = rig.home().join("conf/config.toml");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(
        &config,
        "[llm]\nenabled = true\n[llm.backends.gemini]\nkind = \"gemini\"\napi_key_file = \"k\"\n",
    )
    .unwrap();
    let key = rig.home().join("conf/k");
    std::fs::write(&key, "test-key-own").unwrap();
    std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
    let out = rig.run(&["status", "--config", config.to_str().unwrap()], "", &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_no_key(&out);
    let stdout = text(&out.stdout);
    assert!(
        stdout.starts_with("api_key_file ~/conf/k ([llm.backends.gemini]): found\n"),
        "{stdout}"
    );
    assert!(
        stdout.ends_with("The daemon would use: api_key_file ~/conf/k\n"),
        "{stdout}"
    );
}

#[test]
fn usage_and_help() {
    let rig = Rig::new();
    for args in [
        &[][..],
        &["set"],
        &["set", "openai"],
        &["bogus"],
        &["set", "gemini", "-x"],
    ] {
        let out = rig.run(args, "test-key-x\n", &[]);
        assert_eq!(out.status.code(), Some(2), "{args:?} {out:?}");
        assert!(
            text(&out.stderr).contains("flankr key set gemini"),
            "{out:?}"
        );
        assert!(!rig.secrets().exists());
    }
    let out = rig.run(&["--help"], "", &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(text(&out.stdout).contains("flankr key import gemini <file>"));
}

/// A `HOME` that is not the account's own keeps the Keychain out even
/// without `FLANKR_KEY_STORE` (the guard behind every test).
#[test]
fn a_test_home_alone_keeps_the_keychain_out() {
    let rig = Rig::new();
    let out = rig.run(&["status"], "", &[("FLANKR_KEY_STORE", "")]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let want = if cfg!(target_os = "macos") {
        "Keychain (dev.flankr/gemini): skipped (HOME is not this account's home folder)\n"
    } else {
        "Keychain (dev.flankr/gemini): skipped (not on this system)\n"
    };
    assert!(text(&out.stdout).contains(want), "{out:?}");
}
