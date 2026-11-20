//! Key storage (plan 20261022_04, decision 5), with a list of paired
//! remotes (plan 20261028_01, decisions 1-7 and 13).
//!
//! A **remote** is one paired daemon, identified by its link static public
//! key; its [`RemoteId`] is the first 8 bytes of BLAKE2s-256 over that key.
//! The phone keeps one [`ServerRecord`] per remote in a [`Remotes`] list,
//! written whole, and a separate Noise static key per remote, named by the
//! record's `key_name`: [`LEGACY_KEY_NAME`] for the pairing made before the
//! list, [`RemoteId::key_name`] (`noise.static.<id>`) for every other.
//!
//! These are wrappers over libhardline (plan 20261119_02, step 8): [`RemoteId`], [`ServerRecord`], the
//! encodings and the name helpers are libhardline's, and [`FileKeyStore`]
//! writes through libhardline's `FileVault` and `FileBlobStore`, in the
//! same files. [`Remotes`] and [`RemoteInfo`] stay this crate's, since a
//! row carries what the remote's `state.redb` adds.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
#[cfg(test)]
use libflankr_proto::link;
use libflankr_proto::link::{StaticKey, StaticKeypair};
use libhardline::files::{FileBlobStore, FileVault};
use libhardline::remotes::{REMOTES_BLOB, SERVER_BLOB};
use libhardline::{BlobStore, KeyVault, VaultError};

pub use libhardline::remotes::{
    LEGACY_KEY_NAME, MAX_REMOTES, NICKNAME_MAX, RemoteId, ServerRecord, base_hostname,
    decode_legacy_server, fingerprint_hex, nickname,
};

use libhardline::remotes as hl;

/// Where a client keeps its static keys and the remotes it is paired with
/// (plan 20261028_01, decision 6).
///
/// A static key is a [`StaticKey`], which does its own Diffie-Hellman, so
/// a store backed by a platform key store (the Android Keystore, a
/// Keychain) never hands out the private key. Keys are named; each
/// [`ServerRecord`] names its own (`key_name`).
pub trait KeyStore: Send {
    /// The paired remotes. The first call on a store that still holds the
    /// single `server` record of before migrates it (decision 13).
    fn remotes(&mut self) -> Result<Remotes, String>;
    /// Writes the whole list.
    fn set_remotes(&mut self, remotes: &Remotes) -> Result<(), String>;
    /// The static key named `key_name`, if there is one.
    fn static_key(&mut self, key_name: &str) -> Result<Option<Arc<dyn StaticKey>>, String>;
    /// Makes and stores a new static key named `key_name`, replacing any
    /// there was.
    fn generate_static_key(&mut self, key_name: &str) -> Result<Arc<dyn StaticKey>, String>;
    /// Deletes the static key named `key_name`. Nothing stored is not an
    /// error.
    fn delete_static_key(&mut self, key_name: &str) -> Result<(), String>;
}

/// The paired remotes, in pairing order (oldest first), and the one
/// connected to (decision 6). libhardline's `Remotes`, kept here so that
/// [`info`](Remotes::info) gives this crate's [`RemoteInfo`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Remotes {
    /// The remote connected to; `None`: none (the local socket).
    pub active: Option<RemoteId>,
    pub list: Vec<ServerRecord>,
}

/// The list as libhardline's.
fn to_hl(remotes: &Remotes) -> hl::Remotes {
    hl::Remotes {
        active: remotes.active,
        list: remotes.list.clone(),
    }
}

/// libhardline's list as this crate's.
fn from_hl(remotes: hl::Remotes) -> Remotes {
    Remotes {
        active: remotes.active,
        list: remotes.list,
    }
}

impl Remotes {
    pub fn get(&self, id: RemoteId) -> Option<&ServerRecord> {
        self.list.iter().find(|r| r.id == id)
    }

    pub fn get_mut(&mut self, id: RemoteId) -> Option<&mut ServerRecord> {
        self.list.iter_mut().find(|r| r.id == id)
    }

    /// The active remote's record.
    pub fn active_record(&self) -> Option<&ServerRecord> {
        self.active.and_then(|id| self.get(id))
    }

    /// Sets `id`'s nickname, as [`nickname`] keeps it. Whether it changed.
    pub fn set_nickname(&mut self, id: RemoteId, text: Option<&str>) -> bool {
        let mut list = hl::Remotes {
            active: self.active,
            list: std::mem::take(&mut self.list),
        };
        let changed = list.set_nickname(id, text);
        self.list = list.list;
        changed
    }

    /// The name each listed remote shows, in list order (plan 20261024_02,
    /// the 17:56 and 17:57 EDT amendments): its nickname if it has one;
    /// otherwise its base hostname, with `:port` added only when two or
    /// more listed remotes would otherwise show the same; and when two
    /// defaults are still the same with it (the same host and port: two
    /// daemons reached through different tailnets), each adds a space and
    /// the first group of its fingerprint, `m4:7347 3f9a` (plan 20261028_03,
    /// the 18:48 EDT amendment, item 5).
    pub fn display_names(&self) -> Vec<String> {
        to_hl(self).display_names()
    }

    /// The most recently paired remote, by `paired_at` and then list
    /// order, leaving out `except` (decision 8: the next active remote).
    pub fn most_recent(&self, except: &[RemoteId]) -> Option<RemoteId> {
        to_hl(self).most_recent(except)
    }
}

/// One row of the Remotes section ([`Event::Remotes`](crate::Event::Remotes);
/// plan 20261028_01, decision 26): libhardline's `RemoteInfo` and what the
/// remote's `state.redb` adds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteInfo {
    pub id: RemoteId,
    pub name: String,
    /// Its first address, `host:port`.
    pub host: String,
    /// `3f9a 12c4`.
    pub fingerprint: String,
    /// Unix seconds; `None`: paired before the list.
    pub paired_at: Option<u64>,
    /// The remote in use.
    pub active: bool,
    /// Its link's `hello` is answered: for the remote in use, and, since
    /// every listed remote keeps a link (plan 20261028_03, the 18:51 EDT
    /// amendment), for those in the background too.
    pub online: bool,
    /// The whole fingerprint, BLAKE2s-256 of the daemon's key, as 64
    /// lower-case hex digits ([`fingerprint_hex`]; plan 20261024_02, the
    /// 2026-10-28 17:46 EDT amendment).
    pub fingerprint_hex: String,
    /// The daemon's fingerprint as six words, as `flankr pair` prints them
    /// ([`libflankr_proto::link::fingerprint_words_text`]).
    pub fingerprint_words: String,
    /// The owner's nickname for it; `None`: the default.
    pub nickname: Option<String>,
    /// The name shown ([`Remotes::display_names`]).
    pub display_name: String,
    /// The session last open on it, from its `state.redb`, while that
    /// session's pane is in the remote's agent list as last known (kept
    /// from the last run, or the daemon's): for *At startup*'s *Last
    /// session* (plan 20261024_02, the 2026-10-30 15:20 EDT amendment,
    /// item 3). `None` from [`Remotes::info_with`], which does not read
    /// the state; the `net` thread fills it in. Added in 0.7.3.
    pub last_session: Option<crate::remote_state::LastSession>,
    /// The workspaces whose groups are collapsed in the drawer, by
    /// workspace id, from its `state.redb` (plan 20261103_02, decision
    /// 3.4); set with [`crate::Input::SetCollapsedWorkspaces`]. Empty from
    /// [`Remotes::info_with`]; the `net` thread fills it in. Added in 0.8.4.
    pub collapsed_workspaces: Vec<String>,
}

impl Remotes {
    /// The rows of the Remotes section, in pairing order; `online` says
    /// whether the active remote's connection is up.
    pub fn info(&self, online: bool) -> Vec<RemoteInfo> {
        let active = self.active;
        self.info_with(|id| online && active == Some(id))
    }

    /// The rows, each remote's `online` from `online(id)`: whether its own
    /// link's `hello` is answered (plan 20261028_03, the 18:51 EDT
    /// amendment: every listed remote keeps a link).
    pub fn info_with(&self, online: impl Fn(RemoteId) -> bool) -> Vec<RemoteInfo> {
        to_hl(self)
            .info_with(online)
            .into_iter()
            .map(|r| RemoteInfo {
                id: r.id,
                name: r.name,
                host: r.host,
                fingerprint: r.fingerprint,
                paired_at: r.paired_at,
                active: r.active,
                online: r.online,
                fingerprint_hex: r.fingerprint_hex,
                fingerprint_words: r.fingerprint_words,
                nickname: r.nickname,
                display_name: r.display_name,
                last_session: None,
                collapsed_workspaces: Vec::new(),
            })
            .collect()
    }
}

/// The `remotes` blob, as TOML (decision 6).
pub fn encode_remotes(remotes: &Remotes) -> Result<String, String> {
    hl::encode_remotes(&to_hl(remotes))
}

/// Reads the `remotes` blob. A record whose `id` is not its key's is
/// refused, as is an `active` naming no record.
pub fn decode_remotes(text: &str) -> Result<Remotes, String> {
    hl::decode_remotes(text).map(from_hl)
}

/// The list a store migrates to when it has no `remotes` blob yet: the
/// old record, if there was one, as the one remote and the active one;
/// otherwise empty (decision 13). The caller writes it, then deletes the
/// old record; with none, it deletes the [`LEGACY_KEY_NAME`] key.
pub fn migrated(server: Option<ServerRecord>) -> Remotes {
    from_hl(hl::migrated(server))
}

/// Keys in files: each static key, base64, in `<dir>/key` (the key named
/// [`LEGACY_KEY_NAME`]) or `<dir>/key.<id>` (the key named
/// `noise.static.<id>`) with mode 0600, and the list in
/// `<dir>/remotes.toml`. A key file that anyone but its owner has any
/// permission on is refused. `<dir>/server.toml`, the single record of
/// before, is migrated on the first [`remotes`](KeyStore::remotes).
///
/// Files are written and deleted through libhardline's `FileVault` and
/// `FileBlobStore`, whose layout this is. Errors keep this store's texts,
/// each naming its file: a key is read here, so that one of the wrong
/// length still says how long it is.
#[derive(Debug, Clone)]
pub struct FileKeyStore {
    dir: PathBuf,
}

impl FileKeyStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        FileKeyStore { dir: dir.into() }
    }

    /// `~/.config/flankr-companion/`.
    pub fn default_dir() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".config").join("flankr-companion")
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The file of the key named `key_name`.
    pub fn key_path(&self, key_name: &str) -> Result<PathBuf, String> {
        self.vault()
            .key_path(key_name)
            .map_err(|e| key_error(key_name, e))
    }

    fn vault(&self) -> FileVault {
        FileVault::new(&self.dir)
    }

    fn blobs(&self) -> FileBlobStore {
        FileBlobStore::new(&self.dir)
    }

    fn server_path(&self) -> PathBuf {
        self.dir.join("server.toml")
    }

    fn remotes_path(&self) -> PathBuf {
        self.dir.join("remotes.toml")
    }

    /// The blob `name` (in `path`) as text, or `None` when there is none.
    fn read_blob(&self, name: &str, path: &Path) -> Result<Option<String>, String> {
        match self.blobs().get(name).map_err(store_error)? {
            Some(bytes) => String::from_utf8(bytes)
                .map(Some)
                .map_err(|_| format!("{}: stream did not contain valid UTF-8", path.display())),
            None => Ok(None),
        }
    }

    /// Writes `contents` to `path` with mode 0600, through a temporary file
    /// renamed into place, creating the directory with mode 0700: the
    /// tests' way to lay out the files of before.
    #[cfg(test)]
    fn write_private(&self, path: &Path, contents: &[u8]) -> Result<(), String> {
        use std::fs::OpenOptions;
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        fs::create_dir_all(&self.dir).map_err(|e| format!("{}: {e}", self.dir.display()))?;
        let _ = fs::set_permissions(&self.dir, fs::Permissions::from_mode(0o700));
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        let write = || -> std::io::Result<()> {
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&tmp)?;
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
            file.write_all(contents)?;
            file.sync_all()?;
            fs::rename(&tmp, path)
        };
        write().map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// A file store's error in this store's words: the file system's
/// (`<path>: …`) and a corrupt file's as they are, others as the vault
/// says them.
fn store_error(e: VaultError) -> String {
    match e {
        VaultError::Platform(detail) | VaultError::Corrupt(detail) => detail,
        other => other.to_string(),
    }
}

/// [`store_error`], for the key named `key_name`.
fn key_error(key_name: &str, e: VaultError) -> String {
    match e {
        VaultError::InvalidName => format!("'{key_name}' is not a key name this store keeps"),
        other => store_error(other),
    }
}

fn decode_key(text: &str, what: &str) -> Result<[u8; 32], String> {
    let bytes = STANDARD
        .decode(text.trim())
        .map_err(|e| format!("{what} is not base64: {e}"))?;
    bytes
        .try_into()
        .map_err(|b: Vec<u8>| format!("{what} is {} bytes, not 32", b.len()))
}

impl FileKeyStore {
    /// The private key in `path`, if there is one.
    fn read_key(&self, path: &Path) -> Result<Option<[u8; 32]>, String> {
        let meta = match fs::metadata(path) {
            Ok(meta) => meta,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        let mode = meta.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            return Err(format!(
                "{} has mode {mode:04o}; only its owner may have access (chmod 600)",
                path.display()
            ));
        }
        match fs::read_to_string(path) {
            Ok(text) => decode_key(&text, &path.display().to_string()).map(Some),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }
}

impl KeyStore for FileKeyStore {
    fn remotes(&mut self) -> Result<Remotes, String> {
        let path = self.remotes_path();
        let server_path = self.server_path();
        let blobs = self.blobs();
        if let Some(text) = self.read_blob(REMOTES_BLOB, &path)? {
            let remotes = decode_remotes(&text).map_err(|e| format!("{}: {e}", path.display()))?;
            // A migration that stopped between its two writes.
            blobs.delete(SERVER_BLOB).map_err(store_error)?;
            return Ok(remotes);
        }
        let server = match self.read_blob(SERVER_BLOB, &server_path)? {
            Some(text) => Some(
                decode_legacy_server(&text)
                    .map_err(|e| format!("{}: {e}", server_path.display()))?,
            ),
            None => None,
        };
        let had_server = server.is_some();
        let remotes = migrated(server);
        // The new list first; the old record only once it is written.
        self.set_remotes(&remotes)?;
        if had_server {
            blobs.delete(SERVER_BLOB).map_err(store_error)?;
        } else {
            // A key with no server: a pairing never stored.
            self.delete_static_key(LEGACY_KEY_NAME)?;
        }
        Ok(remotes)
    }

    fn set_remotes(&mut self, remotes: &Remotes) -> Result<(), String> {
        let text = encode_remotes(remotes)?;
        self.blobs()
            .put(REMOTES_BLOB, text.as_bytes())
            .map_err(store_error)
    }

    fn static_key(&mut self, key_name: &str) -> Result<Option<Arc<dyn StaticKey>>, String> {
        let Some(private) = self.read_key(&self.key_path(key_name)?)? else {
            return Ok(None);
        };
        let keypair = StaticKeypair::from_private(private).map_err(|e| e.to_string())?;
        Ok(Some(Arc::new(keypair)))
    }

    fn generate_static_key(&mut self, key_name: &str) -> Result<Arc<dyn StaticKey>, String> {
        self.vault()
            .generate(key_name)
            .map_err(|e| key_error(key_name, e))?;
        self.static_key(key_name)?
            .ok_or_else(|| format!("the key {key_name} was not kept"))
    }

    fn delete_static_key(&mut self, key_name: &str) -> Result<(), String> {
        self.vault()
            .delete(key_name)
            .map(|_| ())
            .map_err(|e| key_error(key_name, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, FileKeyStore) {
        let dir = tempfile::tempdir().unwrap();
        let keys = FileKeyStore::new(dir.path().join("keys"));
        (dir, keys)
    }

    fn record(n: u8, paired_at: Option<u64>) -> ServerRecord {
        let mut r = ServerRecord::new(
            format!("m{n}"),
            [n; 32],
            vec![format!("m{n}.tail.ts.net:7347"), "127.0.0.1:7347".into()],
        );
        r.paired_at = paired_at;
        r
    }

    /// Today's layout: `server.toml` and `key`, as the old store wrote
    /// them.
    fn legacy(keys: &FileKeyStore, server: &[u8; 32]) -> [u8; 32] {
        let keypair = link::generate_keypair().unwrap();
        let text = format!("{}\n", STANDARD.encode(keypair.private));
        keys.write_private(&keys.dir.join("key"), text.as_bytes())
            .unwrap();
        let toml = format!(
            "name = \"m4\"\npublic_key = \"{}\"\naddresses = [\"m4.tail.ts.net:7347\"]\nsession_id = \"companion:s23\"\n",
            STANDARD.encode(server)
        );
        keys.write_private(&keys.server_path(), toml.as_bytes())
            .unwrap();
        keypair.public
    }

    #[test]
    fn ids_and_fingerprints_follow_the_proto_crate() {
        let key = [0u8; 32];
        let id = RemoteId::of(&key);
        assert_eq!(id.to_string(), link::remote_id(&key));
        assert_eq!(id.to_string(), "320b5ea99e653bc2");
        assert_eq!(id.fingerprint(), link::fingerprint_text(&key));
        assert_eq!(id.key_name(), "noise.static.320b5ea99e653bc2");
        assert_eq!("320b5ea99e653bc2".parse::<RemoteId>(), Ok(id));
        assert!("320B5EA99E653BC2".parse::<RemoteId>().is_err());
        assert!("320b5e".parse::<RemoteId>().is_err());
    }

    #[test]
    fn an_empty_store_reads_as_an_empty_list() {
        let (_dir, mut keys) = store();
        assert_eq!(keys.remotes().unwrap(), Remotes::default());
        assert!(keys.static_key(LEGACY_KEY_NAME).unwrap().is_none());
        // And says so again, from the list it wrote.
        assert!(keys.remotes_path().exists());
        assert_eq!(keys.remotes().unwrap(), Remotes::default());
    }

    #[test]
    fn a_list_of_three_round_trips_with_active() {
        let (_dir, mut keys) = store();
        let mut first = record(1, None);
        first.key_name = LEGACY_KEY_NAME.into();
        first.session_id = Some("companion:s23".into());
        let mut third = record(3, Some(1_793_212_400));
        third.signing_key = Some("c2lnbmluZw==".into());
        let remotes = Remotes {
            active: Some(record(2, None).id),
            list: vec![first, record(2, Some(1_793_212_345)), third],
        };
        keys.set_remotes(&remotes).unwrap();
        assert_eq!(keys.remotes().unwrap(), remotes);
        let text = fs::read_to_string(keys.remotes_path()).unwrap();
        assert!(text.starts_with("version = 1\n"), "{text}");
        assert!(text.contains("[[remote]]"), "{text}");
        let mode = fs::metadata(keys.remotes_path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn a_bad_list_is_refused() {
        let mut r = record(1, None);
        r.id = record(2, None).id;
        let text = encode_remotes(&Remotes {
            active: None,
            list: vec![r],
        })
        .unwrap();
        assert!(decode_remotes(&text).unwrap_err().contains("not its key's"));
        let text = encode_remotes(&Remotes {
            active: Some(record(2, None).id),
            list: vec![record(1, None)],
        })
        .unwrap();
        assert!(
            decode_remotes(&text)
                .unwrap_err()
                .contains("not in the list")
        );
    }

    #[test]
    fn todays_pairing_migrates_to_one_active_remote() {
        let (_dir, mut keys) = store();
        let server = [9u8; 32];
        let phone = legacy(&keys, &server);
        let remotes = keys.remotes().unwrap();
        let id = RemoteId::of(&server);
        assert_eq!(remotes.active, Some(id));
        assert_eq!(remotes.list.len(), 1);
        let r = &remotes.list[0];
        assert_eq!(r.public_key, server);
        assert_eq!(r.name, "m4");
        assert_eq!(r.addresses, ["m4.tail.ts.net:7347"]);
        assert_eq!(r.session_id.as_deref(), Some("companion:s23"));
        assert_eq!(r.paired_at, None);
        assert_eq!(r.key_name, LEGACY_KEY_NAME);
        // The old key is kept, under its old name and file.
        let key = keys.static_key(&r.key_name).unwrap().expect("the old key");
        assert_eq!(key.public(), phone);
        assert!(keys.dir.join("key").exists());
        assert!(!keys.server_path().exists(), "server.toml is gone");
        assert_eq!(keys.remotes().unwrap(), remotes);
    }

    #[test]
    fn both_files_read_the_list_and_delete_the_old_record() {
        let (_dir, mut keys) = store();
        let remotes = Remotes {
            active: Some(record(1, None).id),
            list: vec![record(1, None), record(2, Some(5))],
        };
        keys.set_remotes(&remotes).unwrap();
        legacy(&keys, &[9u8; 32]);
        assert_eq!(keys.remotes().unwrap(), remotes);
        assert!(!keys.server_path().exists());
        // The key of the pairing in the list is not touched.
        assert!(keys.dir.join("key").exists());
    }

    #[test]
    fn a_key_with_no_server_is_deleted_at_migration() {
        let (_dir, mut keys) = store();
        keys.generate_static_key(LEGACY_KEY_NAME).unwrap();
        assert_eq!(keys.remotes().unwrap(), Remotes::default());
        assert!(keys.static_key(LEGACY_KEY_NAME).unwrap().is_none());
    }

    #[test]
    fn each_remote_has_its_own_key_file() {
        let (_dir, mut keys) = store();
        let a = record(1, None);
        let b = record(2, None);
        let ka = keys.generate_static_key(&a.key_name).unwrap();
        let kb = keys.generate_static_key(&b.key_name).unwrap();
        assert_ne!(ka.public(), kb.public());
        let path = keys.key_path(&a.key_name).unwrap();
        assert_eq!(path, keys.dir.join(format!("key.{}", a.id)));
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        keys.delete_static_key(&a.key_name).unwrap();
        assert!(keys.static_key(&a.key_name).unwrap().is_none());
        let stored = keys.static_key(&b.key_name).unwrap().expect("b's key");
        assert_eq!(stored.public(), kb.public());
        assert!(keys.key_path("../server").is_err());
        assert!(keys.key_path("noise.static./x").is_err());
    }

    #[test]
    fn deleting_a_missing_key_is_not_an_error() {
        let (_dir, mut keys) = store();
        keys.delete_static_key(LEGACY_KEY_NAME).unwrap();
        keys.delete_static_key(&record(1, None).key_name).unwrap();
    }

    #[test]
    fn a_key_file_at_0644_is_refused() {
        let (_dir, mut keys) = store();
        let name = record(1, None).key_name;
        keys.generate_static_key(&name).unwrap();
        fs::set_permissions(
            keys.key_path(&name).unwrap(),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        let err = keys.static_key(&name).err().expect("refused");
        assert!(err.contains("0644"), "{err}");
    }

    #[test]
    fn a_key_of_the_wrong_length_is_refused() {
        let (_dir, mut keys) = store();
        keys.write_private(&keys.dir.join("key"), STANDARD.encode([1u8; 16]).as_bytes())
            .unwrap();
        let err = keys.static_key(LEGACY_KEY_NAME).err().expect("refused");
        assert!(err.contains("16 bytes"), "{err}");
    }

    /// A remote at `address`, its key `[n; 32]`.
    fn at(n: u8, address: &str) -> ServerRecord {
        ServerRecord::new(format!("m{n}"), [n; 32], vec![address.into()])
    }

    #[test]
    fn the_base_hostname_is_the_first_label_or_the_whole_ip() {
        assert_eq!(base_hostname("m4.tail1a2b3c.ts.net:7347"), "m4");
        assert_eq!(base_hostname("m4:7347"), "m4");
        assert_eq!(base_hostname("m4.local"), "m4");
        assert_eq!(base_hostname("127.0.0.1:7347"), "127.0.0.1");
        assert_eq!(base_hostname("100.64.0.7"), "100.64.0.7");
        assert_eq!(base_hostname("[fd7a::1]:7347"), "[fd7a::1]");
    }

    #[test]
    fn a_default_name_adds_the_port_only_on_a_conflict() {
        let mut remotes = Remotes {
            active: None,
            list: vec![at(1, "m4.tail.ts.net:7347"), at(2, "mini.tail.ts.net:7348")],
        };
        // Not the default port, but no conflict: the base hostname alone.
        assert_eq!(remotes.display_names(), ["m4", "mini"]);
        // Two that would both show m4: each shows host:port, whatever the
        // port; the other is left alone.
        remotes.list.push(at(3, "m4.lan:7347"));
        let group = |r: &ServerRecord| r.id.fingerprint()[..4].to_string();
        let (g1, g3) = (group(&remotes.list[0]), group(&remotes.list[2]));
        assert_eq!(
            remotes.display_names(),
            [
                format!("m4:7347 {g1}"),
                "mini".into(),
                format!("m4:7347 {g3}")
            ],
            "the same host and port: each adds its fingerprint's first group"
        );
        remotes.list[2].addresses = vec!["m4.lan:7400".into()];
        assert_eq!(remotes.display_names(), ["m4:7347", "mini", "m4:7400"]);
        // A nickname is shown as set, and ends the conflict it was in.
        assert!(remotes.set_nickname(remotes.list[2].id, Some("  Studio  ")));
        assert_eq!(remotes.display_names(), ["m4", "mini", "Studio"]);
        // A nickname equal to another's default: the default takes the
        // port, the nickname does not.
        assert!(remotes.set_nickname(remotes.list[2].id, Some("mini")));
        assert_eq!(remotes.display_names(), ["m4", "mini:7348", "mini"]);
        // Cleared (empty is the default): back to the conflict.
        assert!(remotes.set_nickname(remotes.list[2].id, Some("   ")));
        assert_eq!(remotes.list[2].nickname, None);
        assert_eq!(remotes.display_names(), ["m4:7347", "mini", "m4:7400"]);
        assert!(!remotes.set_nickname(remotes.list[2].id, None), "unchanged");
        // With no address, its name.
        let bare = Remotes {
            active: None,
            list: vec![ServerRecord::new("m9", [9; 32], Vec::new())],
        };
        assert_eq!(bare.display_names(), ["m9"]);
    }

    #[test]
    fn the_same_host_and_port_add_the_fingerprints_first_group() {
        let mut remotes = Remotes {
            active: None,
            list: vec![
                at(1, "m4.tail1.ts.net:7347"),
                at(2, "m4.tail2.ts.net:7347"),
                at(3, "mini.tail1.ts.net:7347"),
            ],
        };
        let group = |r: &ServerRecord| r.id.fingerprint()[..4].to_string();
        let (g1, g2) = (group(&remotes.list[0]), group(&remotes.list[1]));
        assert_ne!(g1, g2, "two daemons, two keys");
        assert_eq!(
            remotes.display_names(),
            [
                format!("m4:7347 {g1}"),
                format!("m4:7347 {g2}"),
                "mini".to_string()
            ]
        );
        // A set nickname is shown as set, and the other is left alone.
        assert!(remotes.set_nickname(remotes.list[1].id, Some("Work")));
        assert_eq!(remotes.display_names(), ["m4", "Work", "mini"]);
        // Through the rows the drawer's cards and Settings show.
        assert!(remotes.set_nickname(remotes.list[1].id, None));
        let rows = remotes.info_with(|_| false);
        assert_eq!(rows[0].display_name, format!("m4:7347 {g1}"));
        assert_eq!(rows[1].display_name, format!("m4:7347 {g2}"));
    }

    #[test]
    fn a_nickname_is_trimmed_cut_and_without_control_characters() {
        assert_eq!(nickname(None), None);
        assert_eq!(nickname(Some("")), None);
        assert_eq!(nickname(Some(" \t ")), None);
        assert_eq!(nickname(Some(" Mac\nmini ")).as_deref(), Some("Macmini"));
        let long = "x".repeat(NICKNAME_MAX + 10);
        assert_eq!(
            nickname(Some(&long)).map(|n| n.chars().count()),
            Some(NICKNAME_MAX)
        );
    }

    #[test]
    fn info_carries_the_whole_fingerprint_and_the_display_name() {
        let mut remotes = Remotes {
            active: Some(at(1, "m4.tail.ts.net:7347").id),
            list: vec![at(1, "m4.tail.ts.net:7347"), at(2, "m4.lan:7350")],
        };
        remotes.list[1].nickname = Some("Office".into());
        let info = remotes.info(true);
        let key = [1u8; 32];
        assert_eq!(info[0].fingerprint_hex, fingerprint_hex(&key));
        assert_eq!(info[0].fingerprint_hex.len(), 64);
        assert!(info[0].fingerprint_hex.starts_with(&link::remote_id(&key)));
        assert_eq!(
            info[0].fingerprint_words,
            link::fingerprint_words_text(&key)
        );
        assert_eq!(info[0].nickname, None);
        assert_eq!(info[0].display_name, "m4");
        assert_eq!(info[1].nickname.as_deref(), Some("Office"));
        assert_eq!(info[1].display_name, "Office");
        assert!(info[0].active && info[0].online && !info[1].active);
    }

    #[test]
    fn a_nickname_round_trips_and_an_old_list_reads_without_one() {
        let (_dir, mut keys) = store();
        let mut named = record(1, Some(5));
        named.nickname = Some("Studio".into());
        let remotes = Remotes {
            active: Some(named.id),
            list: vec![named, record(2, None)],
        };
        keys.set_remotes(&remotes).unwrap();
        let text = fs::read_to_string(keys.remotes_path()).unwrap();
        assert_eq!(text.matches("nickname = ").count(), 1, "{text}");
        assert_eq!(keys.remotes().unwrap(), remotes);
        // A list written before nicknames: none.
        let old = text.replace("nickname = \"Studio\"\n", "");
        let read = decode_remotes(&old).unwrap();
        assert!(read.list.iter().all(|r| r.nickname.is_none()));
    }

    #[test]
    fn the_next_active_is_the_most_recently_paired() {
        let remotes = Remotes {
            active: Some(record(1, None).id),
            list: vec![record(1, None), record(2, Some(20)), record(3, Some(10))],
        };
        let one = record(1, None).id;
        assert_eq!(remotes.most_recent(&[one]), Some(record(2, None).id));
        assert_eq!(
            remotes.most_recent(&[one, record(2, None).id]),
            Some(record(3, None).id)
        );
        // Undated ones: the later in the list.
        let undated = Remotes {
            active: None,
            list: vec![record(1, None), record(2, None)],
        };
        assert_eq!(undated.most_recent(&[]), Some(record(2, None).id));
        assert_eq!(undated.most_recent(&[one, record(2, None).id]), None);
    }
}
