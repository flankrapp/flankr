//! The server's static key, `~/.config/flankr/link_key`
//! (plan 20261022_02, decision 4).
//!
//! The file holds the 32-byte X25519 private key in base64 (standard
//! alphabet, padded) and a newline. `serve` creates it on first start with
//! mode 0600, and refuses to start if it is open to anyone but its owner.
//!
//! The file is read and written by `libhardline::server::keyfile`
//! (docs/design/20261119_02_LIBHARDLINE_EXTRACTION_PLAN.md, step 9), with
//! the same checks and messages; the key is handed on as flankr's
//! [`StaticKeypair`].

use std::path::Path;

use libflankr_proto::link::StaticKeypair;
use libhardline::NoiseSecret;
use libhardline::server::keyfile;
// What the tests below use through `super::*`.
#[cfg(test)]
use std::io::Write;
#[cfg(test)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

pub use libhardline::server::keyfile::{decode_key, mode_is_private};

/// Load the key at `path`, creating it if it does not exist.
pub fn load_or_create(path: &Path) -> Result<StaticKeypair, String> {
    keyfile::load_or_create(path).and_then(|key| keypair(path, key))
}

/// Load an existing key, refusing one that others may read.
pub fn load(path: &Path) -> Result<StaticKeypair, String> {
    keyfile::load(path).and_then(|key| keypair(path, key))
}

fn keypair(path: &Path, key: NoiseSecret) -> Result<StaticKeypair, String> {
    StaticKeypair::from_private(*key.expose_bytes())
        .map_err(|e| format!("link key {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes() {
        assert!(mode_is_private(0o600));
        assert!(mode_is_private(0o400));
        assert!(!mode_is_private(0o644));
        assert!(!mode_is_private(0o640));
        assert!(!mode_is_private(0o604));
    }

    #[test]
    fn created_on_first_start_and_loaded_after() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("link_key");
        let first = load_or_create(&path).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(decode_key(text.trim()), Some(first.private));
        let second = load_or_create(&path).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn a_key_at_0644_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("link_key");
        load_or_create(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let err = load_or_create(&path).unwrap_err();
        assert!(err.contains("0644"), "{err}");
        // 0400 is stricter, and fine.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o400)).unwrap();
        assert!(load_or_create(&path).is_ok());
    }

    #[test]
    fn a_malformed_key_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("link_key");
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .unwrap()
            .write_all(b"not a key\n")
            .unwrap();
        assert!(load_or_create(&path).unwrap_err().contains("base64"));
    }
}
