---
title: Extracting the link, pairing and remotes code into libhardline, with a key-vault trait boundary and builders for pairing any backend
date: 2026-11-19
status: Draft
covers:
  - "A new project, ~/src/libhardline, set up under the critical rules
    (~/src/rules): AGENTS.md, CHANGES.md, docs/design/, README with its
    decision table, VERSIONS.md, repowiki.toml, LICENSE"
  - "What moves: the Noise link (IK reconnect, NK pairing request,
    framing, the stream), the pairing string and URL, device codes,
    pairing words and fingerprints, the paired-remotes list, the daemon's
    key file, authorized-keys file, pairing session and address
    discovery, and the pasted-link reader; all non-UI"
  - "What stays in flankr: the JSON-RPC protocol and envelope, the
    companion's client state machine and views, remote_state's redb
    cache, the VPN providers, the `flankr pair` CLI and its output,
    qrcodeurl, and every UI"
  - "A `KeyVault` trait for secure storage with the same lend-for-one-call
    closure as libmorph's `key_store::with_key`, so a private key never
    leaves the vault; a libmorph adapter crate; file and memory vaults"
  - "A `Profile` holding every wire name that now says flankr (URL
    scheme, string prefixes, Noise prologue, words context, key kind),
    default `hardline://`, and flankr's profile reproducing today's wire
    byte for byte"
  - "`Client::builder()` and `Server::builder()`, so another backend and
    app can pair the way `flankr pair` does"
  - "History carried over with git filter-repo, then generalized in
    logical commits, one crate per commit"
  - "Not covered: any change to the wire format, the pairing flow or the
    UI; publishing libhardline to crates.io; moving the VPN providers"
sources:
  - docs/design/20261022_02_LINK_AND_COMPANION_PLAN.md
  - docs/design/20261023_08_SIGNED_PAIRING_PLAN.md
  - docs/design/20261028_01_MULTIPLE_REMOTES_PLAN.md
  - docs/design/20261028_02_PAIRING_QR_SCAN_PLAN.md (its 2026-11-19 14:41 EST amendment, the pasted-link reader, on wt-paste-link)
  - ~/src/libmobile/src/key_store.rs (the `SecretKey`/`SealToken`/`with_key` engine libmorph re-exports)
  - ~/src/libmorph docs/design/20261022_06_KEY_STORE_PLAN.md, section 7
  - The owner, 2026-11-19: "extract all remote and noise related non-UI crates and code to a new project ~/src/libhardline setup under ~/src/rules, use git-filter as needed to create logical commits. this includes a trait boundary for access to secure storage sandbox which should implement the same fn-based closure as libmorph's. libhardline should be useable to pair with a backend via the same mechanism as flankr pair using a builder pattern, settable url prefix (default hardline://), etc. write a plan for the extraction first"
related:
  - docs/design/20261102_03_LINK_VPN_PLAN.md (VPN providers stay in flankr)
  - docs/design/20261114_03_PERSISTENT_THIRD_PARTY_SERVICES_PLAN.md
  - docs/design/20261116_01_TERMINAL_QR_AND_CAMERA_BLOB_NORMALIZATION_PLAN.md (the QR printing stays in flankr)
---

# Extracting libhardline

## Why

flankr's link is a general thing: a Noise channel between a phone and a
backend it has paired with by URL, QR code or pasted string, confirmed by
six words on both sides, with the phone's private key held in the
platform's key store. Nothing in it is about agents or herdr, but today it
is spread over `libflankr_proto::link`, `flankr-companion`'s `keys` and
`net`, the daemon's `src/server/link/` and the app's `keys.rs` and
`remotes.rs`, and every wire name in it says `flankr`. Moving it into its
own project lets another backend and another app pair the same way, with
their own names, and gives the code one place with one test suite.

## Decisions

### 1. The project

1. `~/src/libhardline` is a new git repository and a Cargo workspace,
   set up as the critical rules' "A project's `AGENTS.md`" section says:
   `AGENTS.md` (the rules byte for byte from `~/src/rules`, then
   `---` and `# This project: libhardline`), an empty `CHANGES.md`,
   `docs/design/`, a README laid out as in Rule 4, `VERSIONS.md` (it has
   a public API), `repowiki.toml` with `prefixes = ["docs", "crates"]`,
   and `LICENSE` (Apache-2.0, as flankr, whose code it is). Each in its
   own commit (Rule 3.1). It is registered in `~/src/repowiki/sites.toml`.
2. Its project section names its packages, its commands for Rule 3.4
   (`cargo fmt -p <crate> -- --check`, `cargo clippy -p <crate>
   --all-targets`, `cargo test -p <crate>`) and Rule 5 (`version` in each
   crate's `Cargo.toml`, tags `<crate>-v<version>`, since the workspace
   has more than one crate), and says that plan numbers in commit
   messages before the split refer to flankr's `docs/design/`.
3. It names no app but flankr, and flankr only as the first user (in the
   README and in the flankr profile's doc comment). Tests use invented
   names (`example.test`, `m4.example.test`).
4. This plan stays in flankr, since it changes flankr's architecture. The
   new project's README links to it by its GitHub URL in prose (not in the
   decision table, which lists only libhardline's own documents, and
   starts empty).

### 2. Crates

| Crate | Version | What it holds | From |
| --- | --- | --- | --- |
| `libhardline` | 0.1.0 | `Profile`; the link (handshake, stream, framing, kinds), pairing string and URL, device code, pairing request (NK, `MODE_PAIR`), words, fingerprints, `StaticKey`/`StaticKeypair`; the pasted-link reader; `KeyVault`, `BlobStore` and `NoiseSecret`; the remotes list (`RemoteId`, `ServerRecord`, `Remotes`, encode/decode, migration); `Client` | `libflankr_proto/src/link/*`, `flankr-companion/src/keys.rs` (all but `FileKeyStore`'s paths), `flankr-companion/src/scan.rs` (the URL and string forms), the transport parts of `flankr-companion/src/net.rs` (`tcp_connect`, `link_connect`, `pair_request`, `revoke`, `check`, `Backoff`), `apps/companion/src/remotes.rs`'s `clipboard_pairing_link` and its tests |
| `libhardline-server` | 0.1.0 | `Server`: key file, authorized-keys file, address discovery (`Machine`, `ThisMachine`, `FakeMachine`), the pairing session (offer, request, words, accept/deny, timeout), accepting reconnects | `src/server/link/{keyfile,auth,machine,pair_mode}.rs` and the listener parts of `src/server/link/mod.rs` |
| `libhardline-files` | 0.1.0 | `FileVault` and `FileBlobStore`: 0600 files in a 0700 directory, today's `FileKeyStore` layout | `flankr-companion/src/keys.rs` (`FileKeyStore`) |
| `libhardline-morph` | 0.1.0 | `MorphVault` and `MorphBlobStore` on libmorph's `key_store` and `SecureStore` (Android Keystore, macOS Keychain) | `apps/companion/src/keys.rs` |

`libhardline-server` and `libhardline-morph` are separate so a phone app
does not build a listener and a server does not build libmorph. The
data-URL QR reader (`png` + `rqrr`) in `scan.rs` stays in flankr: it is
how flankr's app reads a pasted image, not part of pairing.

### 3. The profile

Every wire name that says flankr today becomes a field of `Profile`:

```rust
#[derive(Clone, Debug)]
pub struct Profile {
    pub url_scheme: Cow<'static, str>,        // "hardline"      flankr: "flankr"
    pub pairing_prefix: Cow<'static, str>,    // "hardline1"     flankr: "flankr1"
    pub device_code_prefix: Cow<'static, str>,// "hardlinekey1"  flankr: "flankrkey1"
    pub prologue: Cow<'static, [u8]>,         // b"hardline.link/1"        flankr: b"flankr.link/1"
    pub static_marker: Cow<'static, [u8]>,    // b"hardline.link/static-key" flankr: b"flankr.link/static-key"
    pub words_context: Cow<'static, [u8]>,    // b"hardline.pair.words/1"  flankr: b"flankr.pair.words/1"
    pub key_kind: Cow<'static, str>,          // "hardline.noise.x25519.v1" flankr: "flankr.noise.x25519.v1"
}
impl Default for Profile { /* the hardline values */ }
impl Profile {
    pub fn builder() -> ProfileBuilder;       // .url_scheme("myapp") fills the rest from it
}
```

- `ProfileBuilder::url_scheme(s)` sets the scheme and, unless set
  explicitly, derives the others: `<s>1`, `<s>key1`, `<s>.link/1`,
  `<s>.link/static-key`, `<s>.pair.words/1`, `<s>.noise.x25519.v1`. A
  scheme is 1 to 16 ASCII lowercase letters or digits, starting with a
  letter; `build()` refuses anything else.
- `Profile::builder().url_scheme("flankr").build()` gives exactly
  today's flankr values; flankr keeps that in `libflankr_proto` as
  `FLANKR_PROFILE` (a `LazyLock`). No other value changes: the pairing
  versions (3, 2), mode bytes (`0x02`, `0x03`), patterns, frame and
  message limits, word list and checksum salts stay constants.
- Every function that reads or writes a wire name takes `&Profile`:
  `PairingPayload::{encode, decode, url, from_url}`,
  `DeviceCode::{encode, decode}`, the handshakes, `pairing_words`, the
  pasted-link reader. Constants `PAIRING_PREFIX`, `PAIRING_URL_SCHEME`,
  `DEVICE_CODE_PREFIX`, `PROLOGUE_PREFIX`, `PAIRING_WORDS_CONTEXT` go.

### 4. The key-vault boundary

The phone's private key is lent to code for one call and never returned,
as libmobile's `key_store::with_key` does. libhardline owns the key type
and the seal token; the vault owns storage.

```rust
/// An X25519 private key: a heap box, cleared on drop. Only libhardline
/// makes one, or reads its bytes.
pub struct NoiseSecret(Box<[u8; 32]>);
impl NoiseSecret {
    pub fn public(&self) -> [u8; 32];
    pub fn dh(&self, peer: &[u8; 32]) -> Option<[u8; 32]>; // None on all-zero
}

/// Proof that libhardline is the caller; lent by reference only.
pub struct SealToken(());

/// Lends `NoiseSecret`s by name, for one call each.
pub trait KeyVault: Send + Sync + 'static {
    /// New random key under `name`, replacing any; returns its public key.
    fn generate(&self, name: &str) -> Result<[u8; 32], VaultError>;
    fn public(&self, name: &str) -> Result<[u8; 32], VaultError>;
    /// Run `f` with the key for this call only. Object safe: `f` is
    /// `&mut dyn FnMut`, and `with_key` below wraps it in the generic,
    /// `FnOnce` form libmorph's callers know.
    fn with_secret(&self, name: &str, f: &mut dyn FnMut(&NoiseSecret)) -> Result<(), VaultError>;
    fn delete(&self, name: &str) -> Result<bool, VaultError>;
}

/// `key_store::with_key`'s shape: lend the key to `f`, return its result.
pub fn with_key<R>(vault: &dyn KeyVault, name: &str, f: impl FnOnce(&NoiseSecret) -> R)
    -> Result<R, VaultError>;

/// Sealing for vaults that store bytes: they call these with the token
/// libhardline lends them, never on their own.
impl NoiseSecret {
    pub fn seal(&self, _: &SealToken) -> Zeroizing<Vec<u8>>;
    pub fn unseal(kind: &str, bytes: &[u8], _: &SealToken, profile: &Profile) -> Result<Self, String>;
}

/// Small non-secret blobs (the remotes list) in the same sandbox.
pub trait BlobStore: Send + Sync + 'static {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, VaultError>;
    fn put(&self, name: &str, bytes: &[u8]) -> Result<(), VaultError>;
    fn delete(&self, name: &str) -> Result<bool, VaultError>;
}

#[non_exhaustive]
pub enum VaultError { NotFound, InvalidName, WrongKind(String), Corrupt(String), DeviceLocked, Platform(String) }
```

- Names are 1 to 64 of `[A-Za-z0-9._-]`, not starting with `.`, as
  libmobile's key store; keys keep today's names (`noise.static`,
  `noise.static.<id>`).
- `VaultStaticKey { vault: Arc<dyn KeyVault>, name, public }` implements
  `StaticKey`, its `dh` going through `with_key`: today's
  `apps/companion/src/keys.rs` `VaultStaticKey`, made generic.
- `libhardline-morph`'s `MorphVault` implements `KeyVault` with
  libmorph's `SecretKey` (its `NoiseStatic` adapter, `KIND` from the
  profile's `key_kind`, `accepts` also taking the flankr kind so keys
  already sealed on a phone read unchanged) and `key_store::with_key`.
  `MorphBlobStore` wraps `SecureStore`.
- `RemotesStore<V: KeyVault, B: BlobStore>` is today's `KeyStore` trait's
  behaviour (`remotes`, `save_remotes`, `static_key`, `generate`,
  `remove_key`, the `server` → `remotes` migration) on any vault and blob
  store; flankr-companion's `KeyStore` trait becomes a type alias or a
  thin wrapper over it.

### 5. The builders

Client (the app side):

```rust
let client = hardline::Client::builder()
    .profile(Profile::builder().url_scheme("myapp").build()?) // default: Profile::default()
    .vault(vault)              // Arc<dyn KeyVault>
    .blobs(blobs)              // Arc<dyn BlobStore>
    .device_name("pixel-8")    // shown on the backend's prompt
    .connect_timeout(Duration::from_secs(10))
    .build()?;

match hardline::read_pairing(client.profile(), pasted_text) {  // Found / Damaged / None
    PastedLink::Found(url) => {
        let pending = client.request_pairing(&url).await?;  // NK, MODE_PAIR
        show(pending.words());                              // the six words
        let remote = pending.wait().await?;                 // the backend accepted
        client.remotes()?.add(remote)?;
    }
    ...
}
let stream: LinkStream<TcpStream> = client.connect(remote_id, first_message).await?; // IK, MODE_RECONNECT
```

Server (the backend side, what `flankr pair` drives in the daemon):

```rust
let server = hardline_server::Server::builder()
    .profile(profile)
    .key_file("~/.local/state/myapp/link.key")      // 0600, made if missing
    .authorized_keys("~/.local/state/myapp/authorized.toml")
    .listen("0.0.0.0:7347".parse()?)
    .addresses(AddressPolicy::Discover)               // or ::Fixed(vec![...]), ::LoopbackOnly
    .build()
    .await?;

let offer = server.offer_pairing(Duration::from_secs(60)).await?; // offer.url(), offer.string()
while let Some(event) = offer.next().await {          // PairEvent::Request { words, device, key } / Ended
    if confirmed(&event) { offer.accept(&event, protocol)?; } else { offer.deny(&event)?; }
}
loop {
    let (stream, peer, first) = server.accept().await?; // authenticated LinkStream, the peer's AuthEntry
    tokio::spawn(handle(stream, peer, first));
}
```

- `protocol` is the authorized-keys entry's free-text protocol name;
  flankr passes `companion` or `harness` as today. libhardline knows no
  protocol names.
- What a backend does over the stream (JSON-RPC, anything) is its own.
  flankr's `link.pair.begin`/`answer` JSON-RPC methods between `flankr
  pair` and the daemon stay in flankr, now calling `offer_pairing`.
- An example pair, `examples/pair_server.rs` and `examples/pair_client.rs`
  with the default `hardline://` profile on loopback, shows the whole
  flow and is run by a test.

### 6. History

1. A fresh clone of flankr (never the working checkout) at the commit
   where wt-paste-link is merged, filtered with `git filter-repo`
   `--path` for every source file in decision 2's "From" column
   (including the `crates/libhandlr_proto/...` names before the rename)
   and `--path-rename` into the new layout. Its log is grepped for
   `.private`, `.env`, key material and the owner's private app names
   before it goes anywhere; a hit stops the step and is raised with the
   owner.
2. That filtered history is the start of `~/src/libhardline`'s `main`,
   before the setup commits. It keeps authorship and dates, so `git log
   --follow` and blame work. Files only partly moved (`net.rs`,
   `server/link/mod.rs`, `remotes.rs`, `scan.rs`) come with their
   history and are cut down in the first logical commit that touches them.
3. On top, logical commits, one crate per commit: the workspace, then the
   profile, the key-vault boundary, the remotes store, the client
   builder, the server builder, the files and morph adapters, the
   examples. Each passes Rule 3.4's checks.

### 7. flankr afterwards

- `libflankr_proto` depends on `libhardline` by git rev (as libmorph is
  pinned), drops its `link` module and its snow/bs58/blake2/bip39
  dependencies, and keeps `pub use libhardline as link` plus
  `FLANKR_PROFILE`, so most paths still resolve. Removing the constants
  and adding `&Profile` parameters is not additive: `libflankr_proto`
  goes to 0.6.0 (Rule 5.4; it is not in the held-versions table).
- flankr-companion is held at 0.8.x. Its public `keys` and
  `scan::Pairing` APIs change; under the held-versions section this
  must be raised with the owner before it lands (open question 3).
- The daemon's `src/server/link/` keeps `mod.rs` (wiring into the
  daemon, `Principal`, the VPN choice) and `vpn/`; the rest calls
  `libhardline-server`. No change a user of `flankr` sees: no daemon
  version bump.
- The app's `keys.rs` becomes a few lines constructing `MorphVault` and
  `MorphBlobStore`; `remotes.rs` calls `libhardline::read_pairing`. No
  user-visible change: no app version bump.
- Wire compatibility is required: a phone paired before the split
  reconnects to a daemon built after it, and the other way round,
  without pairing again; its Keystore key, sealed as
  `flankr.noise.x25519.v1`, is read unchanged.

## Steps

1. **Set up the project.** Create `~/src/libhardline` with the files of
   decision 1, each in its own commit, and register it with repowiki.
   **Done when:** `make serve` in `~/src/repowiki` lists libhardline,
   and its `AGENTS.md` starts with the rules byte for byte (`cmp` against
   `~/src/rules/rules/CRITICAL_RULES.md` on the leading part).

2. **Import the history.** Run decision 6.1–6.2 and put the filtered
   history under the setup commits' root (or the setup on top of it, so
   `main` is linear). **Done when:** `git log --follow
   crates/libhardline/src/pairing.rs` in libhardline shows the commits
   that touched `libhandlr_proto`/`libflankr_proto`'s `link/pairing.rs`,
   and the grep of 6.1 finds nothing.

3. **libhardline: profile and core.** The link, pairing, device code,
   words and fingerprints compile in `crates/libhardline` with `Profile`
   (decision 3) threaded through, and the pasted-link reader with its
   fuzz and degenerate tests. **Done when:** today's `link/tests.rs`
   passes under the flankr profile, a new test round-trips a pairing
   URL, device code and handshake under `Profile::default()` and under
   `url_scheme("myapp")`, and a pairing string made by flankr 0.12.1's
   daemon (a fixture committed in hex) decodes under the flankr profile.

4. **libhardline: key vault and remotes.** Decision 4's traits,
   `NoiseSecret`, `with_key`, `VaultStaticKey` and `RemotesStore`, with
   a `MemoryVault` for tests. **Done when:** today's `keys.rs` tests
   pass against `MemoryVault`, including the `server` → `remotes`
   migration, and a test shows a `with_key` closure cannot keep the key
   (a compile-fail doc test).

5. **libhardline: client.** `Client::builder()` and its `request_pairing`,
   `connect`, `check` and `revoke`. **Done when:** a loopback test pairs a
   client with a test server built from the raw handshake functions, and
   reconnects with IK.

6. **libhardline-server.** `Server::builder()`, key file, authorized
   keys, address discovery, pairing offer and accept. **Done when:** the
   examples of decision 5 pair and exchange a message on loopback under
   the default profile in a test, and the daemon's `pair_mode` tests
   pass against it.

7. **libhardline-files and libhardline-morph.** **Done when:**
   `FileVault` reads a key directory written by today's `FileKeyStore`
   (fixture), and `libhardline-morph` builds for
   `aarch64-linux-android` and its desktop tests pass.

8. **flankr: libflankr_proto 0.6.0.** Depend on libhardline at a pinned
   rev, re-export as decision 7. **Done when:** `cargo test -p
   libflankr_proto` passes and its `VERSIONS.md` entry lists the
   removals as **Breaking**.

9. **flankr: flankr-companion, the daemon and the app.** One crate per
   commit, after the owner's answer to open question 3. **Done when:**
   the workspace's tests pass, the S23, still paired with the Mac mini
   from before, reconnects to a daemon built from this step without
   pairing again, and a fresh pairing by pasted link, QR code and device
   code works on the S23.

10. **Publish.** Push libhardline to its remote and flankr to both of
    its remotes, with tags. **Done when:** flankr's `Cargo.lock` points
    at a libhardline rev that exists on the remote.

## Open questions

1. **Remote.** Create `github.com/tmzt/libhardline`, public, as flankr's
   other libraries? Until answered, step 10 does not run and flankr's
   step 8 uses a path dependency only in a worktree, never on
   `main-terminal`.
2. **History.** Keep the filtered history (decision 6, recommended), or
   start libhardline with only the logical commits and name the flankr
   commits they came from in each message?
3. **flankr-companion's held series.** Its `keys` and `scan` APIs change
   (decision 7). Keep it at 0.8.x by leaving thin wrappers with the old
   signatures (more code in flankr), or let it take 0.9.0 for this one
   change?
4. **The merge first.** This plan moves the pasted-link reader, which is
   only on `wt-paste-link`. Step 2 waits for that branch to be merged.
