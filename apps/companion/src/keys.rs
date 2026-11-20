//! Key storage in libmorph's key store and secure store, as libmorph's key
//! store plan (`20261022_06_KEY_STORE_PLAN.md`, section 7) has flankr use
//! them, through libhardline's vault traits (plan 20261119_02, decision 6):
//!
//! - this device's Noise static keys are [`NoiseStatic`]s in the key store,
//!   one per paired remote: [`KEY_NAME`] for the pairing made before the
//!   remotes list, `noise.static.<id>` for each later one (plan
//!   20261028_01, decision 5). Its private half never leaves the store: each
//!   Diffie-Hellman borrows it for one call through `key_store::with_key`.
//!   [`MorphVault`] is the key store as a libhardline `KeyVault`.
//! - the paired remotes are a TOML blob, [`REMOTES_BLOB`], in the secure
//!   store, in `FileKeyStore`'s `remotes.toml` layout; the single
//!   [`SERVER_BLOB`] of before is migrated into it on first read.
//!   [`MorphBlobStore`] is the secure store as a libhardline `BlobStore`.
//!   [`VaultKeyStore`] keeps the list and keys on the two as libhardline's
//!   `RemotesStore` does, with libmorph's error texts.
//!
//! On Android these are the Keystore-sealed stores of libmobile, and on
//! macOS the login Keychain. Elsewhere libmorph has neither, and the
//! desktop falls back to `FileKeyStore` (see `lib.rs`); Android never does.

use std::sync::Arc;

use curve25519_dalek::montgomery::MontgomeryPoint;
use flankr_companion::keys::{decode_legacy_server, decode_remotes, encode_remotes, migrated};
use flankr_companion::{KeyStore, Remotes, ServerRecord};
use libflankr_proto::link::StaticKey;
use libhardline::VaultError;
use libmorph::key_store::{self, KeyStoreError, SealToken, SecretKey};
use libmorph::{SecureStore, SecureStoreError, Vault};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

/// The key store name of this device's static key.
pub const KEY_NAME: &str = "noise.static";
/// The secure store blob that held the paired server before the remotes
/// list; migrated into [`REMOTES_BLOB`] and deleted (plan 20261028_01,
/// decision 13).
pub const SERVER_BLOB: &str = "server";
/// The secure store blob that holds the paired remotes, in
/// `FileKeyStore`'s `remotes.toml` layout.
pub const REMOTES_BLOB: &str = "remotes";

/// An X25519 private key, kept behind a heap pointer so that moves do not
/// leave copies, and cleared on drop.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct NoiseStatic(Box<[u8; 32]>);

impl NoiseStatic {
    /// X25519 of this key with `peer`, or `None` for an all-zero result (a
    /// low-order peer key).
    fn dh(&self, peer: &[u8; 32]) -> Option<[u8; 32]> {
        let shared = MontgomeryPoint(*peer).mul_clamped(*self.0).to_bytes();
        (shared != [0u8; 32]).then_some(shared)
    }
}

impl SecretKey for NoiseStatic {
    type Public = [u8; 32];
    const KIND: &'static str = "flankr.noise.x25519.v1";

    fn generate(_: &SealToken) -> Self {
        let mut key = Box::new([0u8; 32]);
        getrandom::getrandom(&mut key[..]).expect("the OS random source");
        Self(key)
    }

    fn seal(&self, _: &SealToken) -> Zeroizing<Vec<u8>> {
        Zeroizing::new(self.0.to_vec())
    }

    fn unseal(_kind: &str, bytes: &[u8], _: &SealToken) -> Result<Self, String> {
        if bytes.len() != 32 {
            return Err(format!("a Noise key is 32 bytes, not {}", bytes.len()));
        }
        let mut key = Box::new([0u8; 32]);
        key.copy_from_slice(bytes);
        Ok(Self(key))
    }

    fn public(&self) -> [u8; 32] {
        MontgomeryPoint::mul_base_clamped(*self.0).to_bytes()
    }
}

/// What a libhardline handshake needs from the key, lent for one call.
impl libhardline::NoiseKey for NoiseStatic {
    fn public(&self) -> [u8; 32] {
        SecretKey::public(self)
    }

    fn dh(&self, peer: &[u8; 32]) -> Option<[u8; 32]> {
        NoiseStatic::dh(self, peer)
    }
}

/// libmorph's key store error as libhardline's, one variant for one.
fn vault_error(e: KeyStoreError) -> VaultError {
    match e {
        KeyStoreError::InvalidName => VaultError::InvalidName,
        KeyStoreError::NotFound => VaultError::NotFound,
        KeyStoreError::WrongKind(kind) => VaultError::WrongKind(kind),
        KeyStoreError::Corrupt(detail) => VaultError::Corrupt(detail),
        KeyStoreError::DeviceLocked => VaultError::DeviceLocked,
        KeyStoreError::Locked => VaultError::Locked,
        KeyStoreError::UnsupportedFlags(flags) => VaultError::UnsupportedFlags(flags),
        KeyStoreError::Platform(detail) => VaultError::Platform(detail),
        other => VaultError::Platform(other.to_string()),
    }
}

/// libmorph's secure store error as libhardline's: the variants they share
/// as themselves, the others (too large, Keystore, I/O) as a platform
/// failure that keeps their text.
fn blob_error(e: SecureStoreError) -> VaultError {
    match e {
        SecureStoreError::InvalidName => VaultError::InvalidName,
        SecureStoreError::DeviceLocked => VaultError::DeviceLocked,
        SecureStoreError::Corrupt(detail) => VaultError::Corrupt(detail),
        other => VaultError::Platform(other.to_string()),
    }
}

/// libmorph's key store as a libhardline [`KeyVault`](libhardline::KeyVault):
/// each key a [`NoiseStatic`], sealed as `flankr.noise.x25519.v1`, so a
/// key already sealed on a phone is read unchanged. Its own methods give
/// libmorph's errors, whose texts the app shows; the trait's map them.
#[derive(Clone)]
pub struct MorphVault(Vault);

impl MorphVault {
    pub fn new(vault: Vault) -> Self {
        Self(vault)
    }

    /// The key store, to lend a key to a handshake ([`VaultStaticKey`]).
    pub fn vault(&self) -> &Vault {
        &self.0
    }

    /// A new key under `name`, replacing any; its public key.
    pub fn generate_key(&self, name: &str) -> Result<[u8; 32], KeyStoreError> {
        // No flags: reconnecting in the background works while the phone is
        // locked (the key store plan, section 7).
        key_store::generate::<NoiseStatic>(&self.0, name, 0)
    }

    /// The public key of the key under `name`.
    pub fn public_key(&self, name: &str) -> Result<[u8; 32], KeyStoreError> {
        key_store::public::<NoiseStatic>(&self.0, name)
    }

    /// Deletes the key under `name`; whether there was one.
    pub fn delete_key(&self, name: &str) -> Result<bool, KeyStoreError> {
        key_store::delete(&self.0, name)
    }

    /// The handshakes' key for `name`, whose public half is `public`.
    fn static_key(&self, name: &str, public: [u8; 32]) -> Arc<dyn StaticKey> {
        Arc::new(VaultStaticKey {
            vault: self.0.clone(),
            name: name.to_string(),
            public,
        })
    }
}

impl libhardline::KeyVault for MorphVault {
    fn generate(&self, name: &str) -> Result<[u8; 32], VaultError> {
        self.generate_key(name).map_err(vault_error)
    }

    fn public(&self, name: &str) -> Result<[u8; 32], VaultError> {
        self.public_key(name).map_err(vault_error)
    }

    fn with_secret(
        &self,
        name: &str,
        f: &mut dyn FnMut(&dyn libhardline::NoiseKey),
    ) -> Result<(), VaultError> {
        key_store::with_key::<NoiseStatic, _>(&self.0, name, |key| f(key)).map_err(vault_error)
    }

    fn delete(&self, name: &str) -> Result<bool, VaultError> {
        self.delete_key(name).map_err(vault_error)
    }
}

/// libmorph's secure store as a libhardline [`BlobStore`](libhardline::BlobStore).
/// [`store`](Self::store) gives the secure store itself, whose errors'
/// texts the app shows.
#[derive(Clone)]
pub struct MorphBlobStore(SecureStore);

impl MorphBlobStore {
    pub fn new(store: SecureStore) -> Self {
        Self(store)
    }

    pub fn store(&self) -> &SecureStore {
        &self.0
    }
}

impl libhardline::BlobStore for MorphBlobStore {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, VaultError> {
        self.0.get(name).map_err(blob_error)
    }

    fn put(&self, name: &str, bytes: &[u8]) -> Result<(), VaultError> {
        self.0.put(name, bytes).map_err(blob_error)
    }

    fn delete(&self, name: &str) -> Result<bool, VaultError> {
        self.0.delete(name).map_err(blob_error)
    }
}

/// The static key the link's handshakes use: the stored public value, and a
/// Diffie-Hellman that borrows the private key named `name` from the store
/// for each call.
pub struct VaultStaticKey {
    vault: Vault,
    name: String,
    public: [u8; 32],
}

impl StaticKey for VaultStaticKey {
    fn public(&self) -> [u8; 32] {
        self.public
    }

    fn dh(&self, peer: &[u8; 32]) -> Option<[u8; 32]> {
        match key_store::with_key::<NoiseStatic, _>(&self.vault, &self.name, |key| key.dh(peer)) {
            Ok(shared) => shared,
            Err(e) => {
                log::error!("keys: {e}");
                None
            }
        }
    }
}

/// [`KeyStore`] on libmorph's key store and secure store, through
/// [`MorphVault`] and [`MorphBlobStore`]. It does what libhardline's
/// `RemotesStore` does, with libmorph's error texts, which the app shows.
pub struct VaultKeyStore {
    vault: MorphVault,
    store: MorphBlobStore,
}

impl VaultKeyStore {
    pub fn new(vault: Vault, store: SecureStore) -> Self {
        Self {
            vault: MorphVault::new(vault),
            store: MorphBlobStore::new(store),
        }
    }

    /// Where the secure store keeps its blobs, for the log.
    pub fn describe(&self) -> String {
        match self.store.store().protection() {
            Ok(protection) => format!("{:?}, {protection:?}", self.vault.vault()),
            Err(e) => format!("{:?}, protection unknown: {e}", self.vault.vault()),
        }
    }
}

impl KeyStore for VaultKeyStore {
    /// The `remotes` blob; on the first call with none, the `server` blob
    /// migrated into it (plan 20261028_01, decision 13): the list is
    /// written first, and the `server` blob deleted only after.
    fn remotes(&mut self) -> Result<Remotes, String> {
        let store = self.store.store();
        if let Some(bytes) = store.get(REMOTES_BLOB).map_err(|e| e.to_string())? {
            let text = std::str::from_utf8(&bytes).map_err(|e| format!("the remotes blob: {e}"))?;
            let remotes = decode_remotes(text)?;
            // A migration that stopped between its two writes.
            store
                .delete(SERVER_BLOB)
                .map_err(|e| format!("the server blob: {e}"))?;
            return Ok(remotes);
        }
        let server = match store.get(SERVER_BLOB).map_err(|e| e.to_string())? {
            Some(bytes) => Some(decode_server(&bytes)?),
            None => None,
        };
        let had_server = server.is_some();
        let remotes = migrated(server);
        self.set_remotes(&remotes)?;
        if had_server {
            self.store
                .store()
                .delete(SERVER_BLOB)
                .map_err(|e| format!("the server blob: {e}"))?;
        } else {
            // A key with no server: a pairing never stored.
            self.delete_static_key(KEY_NAME)?;
        }
        Ok(remotes)
    }

    fn set_remotes(&mut self, remotes: &Remotes) -> Result<(), String> {
        let text = encode_remotes(remotes)?;
        self.store
            .store()
            .put(REMOTES_BLOB, text.as_bytes())
            .map_err(|e| e.to_string())
    }

    fn static_key(&mut self, key_name: &str) -> Result<Option<Arc<dyn StaticKey>>, String> {
        match self.vault.public_key(key_name) {
            Ok(public) => Ok(Some(self.vault.static_key(key_name, public))),
            Err(KeyStoreError::NotFound) => Ok(None),
            // A key that cannot be read means pairing again, which makes a
            // new one in its place.
            Err(e @ (KeyStoreError::Corrupt(_) | KeyStoreError::WrongKind(_))) => {
                log::warn!("keys: the stored key is unusable ({e}); pair again");
                Ok(None)
            }
            Err(e) => Err(e.to_string()),
        }
    }

    fn generate_static_key(&mut self, key_name: &str) -> Result<Arc<dyn StaticKey>, String> {
        let public = self
            .vault
            .generate_key(key_name)
            .map_err(|e| e.to_string())?;
        log::info!("keys: made a new static key in the key store");
        Ok(self.vault.static_key(key_name, public))
    }

    fn delete_static_key(&mut self, key_name: &str) -> Result<(), String> {
        self.vault
            .delete_key(key_name)
            .map(|_| ())
            .map_err(|e| format!("the static key: {e}"))
    }
}

/// The `server` blob of before the remotes list: `FileKeyStore`'s
/// `server.toml` layout, the key in base64.
fn decode_server(bytes: &[u8]) -> Result<ServerRecord, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| format!("the server blob: {e}"))?;
    decode_legacy_server(text).map_err(|e| format!("the server blob: {e}"))
}

#[cfg(all(test, not(target_os = "android")))]
mod tests {
    use super::*;
    use libflankr_proto::link::generate_keypair;

    #[test]
    fn the_vault_key_agrees_with_snow() {
        let vault = Vault::memory();
        let public = key_store::generate::<NoiseStatic>(&vault, KEY_NAME, 0).unwrap();
        let ours = VaultStaticKey {
            vault,
            name: KEY_NAME.into(),
            public,
        };
        let peer = generate_keypair().unwrap();
        let shared = ours.dh(&peer.public).expect("a shared secret");
        assert_eq!(Some(shared), peer.dh(&ours.public()));
    }

    #[test]
    fn a_low_order_peer_gives_none() {
        let vault = Vault::memory();
        let public = key_store::generate::<NoiseStatic>(&vault, KEY_NAME, 0).unwrap();
        let ours = VaultStaticKey {
            vault,
            name: KEY_NAME.into(),
            public,
        };
        assert_eq!(ours.dh(&[0u8; 32]), None);
    }

    #[test]
    fn the_server_blob_reads_as_the_remote_it_migrates_to() {
        let blob = b"name = \"m4\"\npublic_key = \"BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc=\"\naddresses = [\"m4.tail.ts.net:7347\"]\nsession_id = \"companion:phone\"\n";
        let record = decode_server(blob).unwrap();
        assert_eq!(record.name, "m4");
        assert_eq!(record.public_key, [7u8; 32]);
        assert_eq!(record.addresses, ["m4.tail.ts.net:7347"]);
        assert_eq!(record.session_id.as_deref(), Some("companion:phone"));
        assert_eq!(record.key_name, KEY_NAME);
        assert_eq!(record.paired_at, None);
        assert!(decode_server(b"name = \"m4\"\npublic_key = \"AAAA\"\n").is_err());
    }

    #[test]
    fn each_remote_has_its_own_vault_key() {
        let vault = Vault::memory();
        let a = key_store::generate::<NoiseStatic>(&vault, KEY_NAME, 0).unwrap();
        let b =
            key_store::generate::<NoiseStatic>(&vault, "noise.static.0011223344556677", 0).unwrap();
        assert_ne!(a, b);
        key_store::delete(&vault, KEY_NAME).unwrap();
        // Deleting again is not an error.
        assert!(!key_store::delete(&vault, KEY_NAME).unwrap());
        assert!(key_store::public::<NoiseStatic>(&vault, "noise.static.0011223344556677").is_ok());
    }

    #[test]
    fn the_names_are_libhardlines() {
        assert_eq!(KEY_NAME, libhardline::LEGACY_KEY_NAME);
        assert_eq!(REMOTES_BLOB, libhardline::remotes::REMOTES_BLOB);
        assert_eq!(SERVER_BLOB, libhardline::remotes::SERVER_BLOB);
    }

    #[test]
    fn the_morph_vault_lends_its_key_to_libhardline() {
        use libhardline::KeyVault as _;
        let vault = Arc::new(MorphVault::new(Vault::memory()));
        let public = vault.generate(KEY_NAME).unwrap();
        // The key is sealed as the app always sealed it.
        assert_eq!(
            key_store::public::<NoiseStatic>(vault.vault(), KEY_NAME).unwrap(),
            public
        );
        let theirs = libhardline::VaultStaticKey::new(vault.clone(), KEY_NAME).unwrap();
        let peer = generate_keypair().unwrap();
        let shared = StaticKey::dh(&theirs, &peer.public).expect("a shared secret");
        assert_eq!(Some(shared), StaticKey::dh(&peer, &public));
        assert_eq!(vault.public("noise.static.none"), Err(VaultError::NotFound));
        assert_eq!(vault.generate(".hidden"), Err(VaultError::InvalidName));
        assert!(vault.delete(KEY_NAME).unwrap());
        assert!(!vault.delete(KEY_NAME).unwrap());
    }

    #[test]
    fn key_store_errors_map_one_for_one() {
        assert_eq!(
            vault_error(KeyStoreError::Corrupt("x".into())),
            VaultError::Corrupt("x".into())
        );
        assert_eq!(
            vault_error(KeyStoreError::WrongKind("k".into())),
            VaultError::WrongKind("k".into())
        );
        assert_eq!(
            vault_error(KeyStoreError::UnsupportedFlags(4)),
            VaultError::UnsupportedFlags(4)
        );
        assert_eq!(vault_error(KeyStoreError::Locked), VaultError::Locked);
        assert_eq!(
            blob_error(SecureStoreError::Io("disk".into())),
            VaultError::Platform("secure store: io: disk".into())
        );
    }
}
