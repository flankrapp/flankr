---
title: Extracting the link, pairing, remotes and VPN code into libhardline as sans-IO crates, with the wire unchanged
date: 2026-11-19
status: Draft
covers:
  - "A new project, ~/src/libhardline, set up under the critical rules
    (~/src/rules): AGENTS.md, CHANGES.md, docs/design/, README with its
    decision table, VERSIONS.md, repowiki.toml, LICENSE; a private
    GitHub remote"
  - "What moves: the Noise link (IK reconnect, NK pairing request,
    framing, the stream), the pairing string and URL, device codes,
    pairing words and fingerprints, the paired-remotes list, the daemon's
    key file, authorized-keys file, pairing session and address
    discovery, the VPN providers, and the pasted-link reader; all non-UI"
  - "What stays in flankr: flankr's JSON-RPC methods and types, the
    companion's client state machine and views, remote_state's redb
    cache, the `flankr service` registry, the `flankr pair` CLI and its
    QR printing, qrcodeurl, the data-URL PNG reader, the libmorph
    adapter, and every UI"
  - "Nine crates: a wire-format crate written with a `wire!` macro, a
    sans-IO core, server, VPN, JSON-RPC, tokio and futures-io adapters, a
    file vault, and a facade with no default features"
  - "A `Profile` with no default scheme; published vault traits in
    libmorph's lend-for-one-call shape with no libmorph dependency;
    `Client::builder()` and `Server::builder()` that only configure"
  - "History carried over with git filter-repo, then generalized in
    logical commits, one crate per commit"
  - "How flankr moves onto it in two stages with its wire unchanged, its
    tests unchanged, and at most patch bumps"
  - "Not covered: any change to the wire format, the link protocol,
    protocol numbers 15 and up, channels, `Hello` versioning or a
    cut-over (a later plan in libhardline); the pairing flow or the UI;
    publishing to crates.io"
sources:
  - docs/design/20261119_01_LIBHARDLINE_EXTRACTION_PLAN.md (the draft this replaces: its body and its 15:25, 15:26 and 15:38 EST amendments)
  - docs/design/20261022_02_LINK_AND_COMPANION_PLAN.md
  - docs/design/20261023_08_SIGNED_PAIRING_PLAN.md
  - docs/design/20261028_01_MULTIPLE_REMOTES_PLAN.md
  - docs/design/20261028_02_PAIRING_QR_SCAN_PLAN.md (its 2026-11-19 14:41 EST amendment, the pasted-link reader)
  - docs/design/20261114_03_PERSISTENT_THIRD_PARTY_SERVICES_PLAN.md (the Bitmot relay's service spec)
  - ~/src/libmobile/src/key_store.rs (the `SecretKey`/`with_key` engine libmorph re-exports)
  - The owner, 2026-11-19, in the draft and its amendments (quoted there): "extract all remote and noise related non-UI crates and code to a new project ~/src/libhardline setup under ~/src/rules, use git-filter as needed to create logical commits. this includes a trait boundary for access to secure storage sandbox which should implement the same fn-based closure as libmorph's. libhardline should be useable to pair with a backend via the same mechanism as flankr pair using a builder pattern, settable url prefix (default hardline://), etc."; "the VPN provides should move also, to dev.tmzt.hardline.vpn for now. private repo for now"; "no default, if a cli app registers it needs to set that prefix"; "should we drop the hardline:// default ... I don't want other apps registering hardline://"; "with the wrappers it should already be compatible and the use of a different library is an internal matter (thie UI shouldn't change at all), we also burned a lot of bumps recently"; "the library should be sans-IO with async-friendly enums, not a specific async implementation"; "opt in to tokio or async-compat with features, separate crates inside the project"; "no morph dependecy, libhardline should publish compatble traits instead"
  - The owner, 2026-11-19, in discussion: "this plan is about the extraction. let's review the finalized plan and write a new version focused on that"; the wire is unchanged during the extraction, the link protocol is a later plan
related:
  - docs/design/20261102_03_LINK_VPN_PLAN.md (the VPN providers' origin)
  - docs/design/20261116_01_TERMINAL_QR_AND_CAMERA_BLOB_NORMALIZATION_PLAN.md (the QR printing stays in flankr)
  - docs/design/20261025_01_RENAME_PROJECT_TO_FLANKR_PLAN.md (the libhandlr_proto → libflankr_proto rename, whose old paths the history import names)
---

# Extracting libhardline

## Why

flankr's link is a general thing: a Noise channel between a phone and a
backend it has paired with by URL, QR code or pasted string, confirmed by
six words on both sides, with the phone's private key held in the
platform's key store, and reached over a VPN when the two are not on one
network. Nothing in it is about agents or herdr, but today it is spread
over `libflankr_proto::link`, `flankr-companion`'s `keys`, `scan` and
`net`, the daemon's `src/server/link/` and the app's `keys.rs` and
`remotes.rs`, every wire name in it says `flankr`, and it is written
against tokio. Moving it into its own project lets another backend and
another app pair the same way, with their own names and their own
executor, and gives the code one place with one test suite.

This plan is the extraction only. flankr's wire does not change: a phone
built before it and a daemon built after it, and the other way round,
exchange today's bytes. The link protocol that was discussed alongside it
(protocol numbers 15 and up, channels, a versioned `Hello`, a cut-over) is
a later plan written in libhardline once the repository exists; this plan
only leaves room for it (decision 9).

## Decisions

### 1. The project

1. `~/src/libhardline` is a new git repository and a Cargo workspace,
   set up as the critical rules' "A project's `AGENTS.md`" section says:
   `AGENTS.md` (the rules byte for byte from
   `~/src/rules/rules/CRITICAL_RULES.md`, then `---` and
   `# This project: libhardline`), an empty `CHANGES.md`, `docs/design/`,
   a README laid out as in Rule 4, `VERSIONS.md` (it has a public API),
   `repowiki.toml` with `prefixes = ["docs", "crates"]`, and `LICENSE`
   (Apache-2.0, as flankr's, whose code it is). Each in its own commit
   (Rule 3.1). It is registered in `~/src/repowiki/sites.toml`.
2. Its project section names its packages (decision 2), its commands for
   Rule 3.4 (`cargo fmt -p <crate> -- --check`, `cargo clippy -p <crate>
   --all-targets`, `cargo test -p <crate>`) and for Rule 5 (`version` in
   each crate's `Cargo.toml`, tags `<crate>-v<version>` since the
   workspace has more than one crate), and says that plan numbers in
   commit messages from before the split refer to flankr's
   `docs/design/`.
3. The remote is `github.com/tmzt/libhardline`, **private** for now. It
   names no app but flankr, and flankr only as the first user (in the
   README and in the flankr profile's doc comment). Tests and examples
   use the invented scheme `hltest` and invented hosts (`example.test`).
4. This plan stays in flankr, since it changes flankr's architecture.
   libhardline's README links to it by GitHub URL in prose, not in its
   decision table, which lists only libhardline's own documents and
   starts empty.

### 2. Crates

All start at 0.1.0. Only the adapters and the file vault do I/O.

| Crate | Holds | I/O |
| --- | --- | --- |
| `libhardline-proto` | the wire format only: frames and the flag byte, the pairing string and URL, device codes, the pairing request and result messages, words, fingerprints, `Profile`; written with the `wire!` macro (decision 5) | none |
| `libhardline-core` | the state machines: `Connection` (client or server side of one link), `PairRequest` (client side) and `PairSession` (server side of one request); `Client::builder()`; the vault traits (decision 6), `NoiseSecret`, `VaultStaticKey`, `RemotesStore` and a `MemoryVault`; the pasted-link reader | none |
| `libhardline-server` | `Server::builder()`, the key-file and authorized-keys formats, address choice (`Machine`, `FakeMachine`), pairing offers, `revoke(key)` | none |
| `libhardline-vpn` | the providers as state machines emitting commands, with features `vpn-tailscale` and `vpn-bitmot`; the service spec Bitmot's relay needs | none |
| `libhardline-jsonrpc` | JSON-RPC 2.0 types (`Request`, `Notification`, `Incoming`, `Id`, `ErrorObject`) on an app-chosen protocol number | none |
| `libhardline-tokio` | `LinkStream<T: AsyncRead + AsyncWrite>`, `connect`, the pairing request, an accept loop over `TcpListener`, VPN commands by `tokio::process`, `ThisMachine` | tokio |
| `libhardline-async` | the same over `futures-io`'s traits, for smol or async-std, and for tokio code through the `async-compat` crate | the caller's executor |
| `libhardline-files` | `FileVault`, `FileBlobStore`: today's `FileKeyStore` layout, 0600 files in a 0700 directory | `std::fs` |
| `libhardline` | the facade, **no default features**: `libhardline-core` re-exported at the root (which brings `libhardline-proto`); features `server`, `vpn-tailscale`, `vpn-bitmot`, `jsonrpc`, `files`, `tokio`, `async-compat`, each pulling in its crate and re-exporting it as a module (`libhardline::server`, `libhardline::tokio`, …) | per feature |

There is no `libhardline-morph`: no crate in the workspace depends on
libmorph or libmobile. The libmorph adapter stays in flankr's app
(decision 6).

### 3. What moves and what stays

Every path is in flankr at `7755c9f` (wt-paste-link merged). The old
names before the rename of 2026-10-25 (`crates/libhandlr_proto/...`,
`crates/handlr-companion/...`) are the same files and are named in the
history import (decision 11).

| Source in flankr | Destination | Notes |
| --- | --- | --- |
| `crates/libflankr_proto/src/link/mod.rs` | `libhardline-proto` (constants, `LinkError`, the frame layout) and `libhardline-core` (the module wiring) | the `KIND_*` constants and `Message` stay in flankr as wrappers (decision 9) |
| `crates/libflankr_proto/src/link/stream.rs` | framing to `libhardline-proto`; the stream becomes `Connection` in `libhardline-core` and `LinkStream` in the two adapters | |
| `crates/libflankr_proto/src/link/handshake.rs` | `libhardline-core` (`Connection`'s IK handshake) | |
| `crates/libflankr_proto/src/link/pair_mode.rs` | messages to `libhardline-proto`; NK handshake, `PairRequest` and `PairSession` to `libhardline-core` | |
| `crates/libflankr_proto/src/link/pairing.rs` | `libhardline-proto` (`PairingPayload`, `DeviceCode`, their encodings) | its private `Reader` becomes the shared `Reader`/`Writer` of decision 5 |
| `crates/libflankr_proto/src/link/keys.rs` | `libhardline-core` (`StaticKey`, `StaticKeypair`, `generate_keypair`, `public_key`) | |
| `crates/libflankr_proto/src/link/identity.rs` | `libhardline-proto` (`fingerprint`, `remote_id`, `fingerprint_text`, `REMOTE_ID_LEN`) | |
| `crates/libflankr_proto/src/link/words.rs` | `libhardline-proto` (`pairing_words`, `fingerprint_words`, the context takes `&Profile`) | |
| `crates/libflankr_proto/src/link/resolver.rs` | `libhardline-core` (the snow resolver) | |
| `crates/libflankr_proto/src/link/tests.rs` | split: encoding tests to `libhardline-proto`, stream tests to `libhardline-core` driven by hand and to each adapter | the tests also stay in flankr unchanged as the compatibility check (decision 10) |
| `crates/libflankr_proto/src/envelope.rs` | `libhardline-jsonrpc` (`Id`, `Request`, `Notification`, `ErrorObject`, `Incoming`) | flankr's envelope re-exports them |
| `crates/flankr-companion/src/keys.rs` | `libhardline-core` (`RemoteId`, `ServerRecord`, `Remotes`, `RemoteInfo`, `encode_remotes`, `decode_remotes`, `decode_legacy_server`, `migrated`, `nickname`, `base_hostname`, `fingerprint_hex`, `RemotesStore` from the `KeyStore` trait's behaviour); `libhardline-files` (`FileKeyStore` as `FileVault` + `FileBlobStore`) | `KeyStore` stays in flankr as a thin trait over `RemotesStore` |
| `crates/flankr-companion/src/scan.rs` | `read` (the URL and bare-string forms), `is_loopback` and `Pairing` to `libhardline-core` | `read_data_url` and `read_qr` (png + rqrr) stay |
| `crates/flankr-companion/src/net.rs` | `tcp_connect`, `link_connect`, `probe_connect`, `pair_request`, `check`, `revoke` to `libhardline-tokio` (as `Client` methods driving the core); `Backoff` to `libhardline-core` | `run`, `probe`, `local_connect` (the Unix socket), `response_to` and the client loop stay |
| `apps/companion/src/remotes.rs` | `ClipboardLink`, `clipboard_pairing_link`, `read_link_at`, `after_scheme`, `is_base58`, the `LINK_*` limits and their tests (the wrapped-link rejoin, fuzz and degenerate cases) to `libhardline-core` as `read_pairing(&Profile, text) -> PastedLink` | the views stay |
| `src/server/link/keyfile.rs` | `libhardline-server` (`load_or_create`, `load`, `decode_key`, `mode_is_private`) | |
| `src/server/link/auth.rs` | `libhardline-server` (`AuthEntry`, `AuthFile`, `add`, `remove`, `AddError`, `format_key`, `now_rfc3339`) | the file's `protocol` field is `role` in the API, still read and written as `protocol` |
| `src/server/link/machine.rs` | `Machine` trait, `usable`, `FakeMachine` to `libhardline-server`; `ThisMachine` to `libhardline-tokio` | |
| `src/server/link/pair_mode.rs` | the session (`PairRun`, `PairEvent`, `timeout_secs`, offer, request, words, accept/deny, timeout) to `libhardline-server` as `PairSession` | `PairBeginParams`, `PairAnswerParams` and the other JSON-RPC parameter types stay |
| `src/server/link/mod.rs` | `offer_addresses`, `normalize_listen`, `is_loopback`, `is_unspecified`, `forward_port`, `valid_name`, the accept loop (`spawn`, `handle`) to `libhardline-server` and `libhardline-tokio` | `LinkSettings`, `Link`, the config reading and the wiring into the daemon stay |
| `src/server/link/vpn/mod.rs` | `libhardline-vpn` (`Vpn`, `VpnAddress`, `VpnError`, `VpnChoice`) | |
| `src/server/link/vpn/tailscale.rs` | `libhardline-vpn`, feature `vpn-tailscale` | |
| `src/server/link/vpn/bitmot.rs` | `libhardline-vpn`, feature `vpn-bitmot`, with a `ServiceSpec` of its own (decision 8) | `SERVICE_ID` becomes `<service_prefix>.bitmot` |
| `src/server/link/vpn/fake.rs` | `libhardline-vpn` | |

What stays in flankr, besides the notes above: flankr's JSON-RPC methods
and types (`companion.rs`, `common.rs`, the rest of `envelope.rs`), the
companion's client state machine and views, `remote_state.rs` (redb),
the `flankr service` registry (`src/service.rs`) and `flankr-daemon`'s
`UserServiceBackend`, the `flankr pair` CLI and its QR printing,
`qrcodeurl`, the libmorph adapter in `apps/companion/src/keys.rs`
(`NoiseStatic`, `VaultStaticKey`, `VaultKeyStore`), and every UI.

### 4. The profile

Every wire name that says flankr today becomes a field of `Profile`, in
`libhardline-proto`:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub url_scheme: Cow<'static, str>,         // flankr: "flankr"
    pub pairing_prefix: Cow<'static, str>,     // flankr: "flankr1"
    pub device_code_prefix: Cow<'static, str>, // flankr: "flankrkey1"
    pub prologue: Cow<'static, [u8]>,          // flankr: b"flankr.link/1"
    pub static_marker: Cow<'static, [u8]>,     // flankr: b"flankr.link/static-key"
    pub words_context: Cow<'static, [u8]>,     // flankr: b"flankr.pair.words/1"
}
impl Profile {
    pub fn builder() -> ProfileBuilder;
}
impl ProfileBuilder {
    pub fn url_scheme(self, s: &str) -> Self;          // required
    pub fn pairing_prefix(self, s: &str) -> Self;      // optional, likewise the rest
    pub fn build(self) -> Result<Profile, ProfileError>;
}
```

- There is no `Default` and no default values. `build()` refuses without
  `url_scheme`. A scheme is 1 to 16 ASCII lowercase letters or digits,
  starting with a letter; `hardline`, and any scheme starting with
  `hardline`, is refused, so no app built on the library registers
  `hardline://` with its OS.
- Unless set explicitly, the other names derive from the scheme `s`:
  `<s>1`, `<s>key1`, `<s>.link/1`, `<s>.link/static-key`,
  `<s>.pair.words/1`.
- `Profile::builder().url_scheme("flankr").build()` gives today's values
  byte for byte; flankr keeps it in `libflankr_proto` as
  `FLANKR_PROFILE` (a `LazyLock<Profile>`). No other value changes: the
  pairing versions (3, 2), mode bytes (`0x02`, `0x03`), patterns, frame
  and message limits, word list and checksum salts stay constants.
- Every function that reads or writes a wire name takes `&Profile`:
  `PairingPayload::{encode, decode, url, from_url}`,
  `DeviceCode::{encode, decode}`, the handshakes, `pairing_words`, the
  pasted-link reader. There is no key kind in `Profile`: the sealed key's
  kind belongs to the vault that seals it (decision 6).

### 5. The wire-format crate and the `wire!` macro

`libhardline-proto` has no serde, no proc-macro and no postcard, deku
or binrw. Each message is one `macro_rules!` declaration that gives the
struct, `encode(&self, &mut Writer)`, `decode(&mut Reader) -> Result<Self,
WireError>` and a `LAYOUT` constant:

```rust
wire! {
    /// A pairing request's payload, version 1 (the `MODE_PAIR` handshake's
    /// first message).
    pub struct PairRequestV1 {
        version: u8,                      // PAIR_MESSAGE_VERSION, 1
        device_key: [u8; 32],             // the phone's new static key, D
        name: bytes<len u8>,              // 1 to DEVICE_NAME_MAX ASCII bytes
        offer: [u8; 16],                  // OFFER_LEN
    }
}
```

- Field kinds: `u8`; `u16` (big-endian); `[u8; N]`; `version` (three
  bytes, major, minor, patch); `list<u16, count u8>`; `bytes<len u8>`;
  `rest` (the remaining bytes, last field only).
- The flag byte's 4-bit fields (kind nibble, *more* bit, reserved bits)
  stay hand-written in one function, `Frame::flag`, with its own tests.
- `Reader<'a>` and `Writer` are shared by every message and replace
  `pairing.rs`'s private `Reader`. `Reader` refuses trailing bytes;
  `Writer` refuses a `bytes` or `list` over its count's range.
- A test renders every `LAYOUT` as a Markdown table (field, kind, bytes,
  meaning) and compares the result with the checked-in `docs/WIRE.md`;
  `UPDATE=1 cargo test -p libhardline-proto` rewrites it. Each message's
  rustdoc carries its table.
- Every message has a round-trip test and a no-panic fuzz test over
  random and truncated bytes.

### 6. The vault traits

The phone's private key is lent to code for one call and never returned,
in the shape of libmobile's `key_store::{generate, public, with_key,
delete}`, but without depending on libmorph or libmobile. In
`libhardline-core`:

```rust
/// What a handshake needs from a key: its public half and one DH.
pub trait NoiseKey {
    fn public(&self) -> [u8; 32];
    fn dh(&self, peer: &[u8; 32]) -> Option<[u8; 32]>; // None on all-zero
}

/// Lends keys by name, for one call each.
pub trait KeyVault: Send + Sync + 'static {
    /// New random key under `name`, replacing any; returns its public key.
    fn generate(&self, name: &str) -> Result<[u8; 32], VaultError>;
    fn public(&self, name: &str) -> Result<[u8; 32], VaultError>;
    /// Run `f` with the key for this call only. Object safe: `f` is
    /// `&mut dyn FnMut`; `with_key` wraps it in the `FnOnce` form.
    fn with_secret(&self, name: &str, f: &mut dyn FnMut(&dyn NoiseKey)) -> Result<(), VaultError>;
    fn delete(&self, name: &str) -> Result<bool, VaultError>;
}

/// `key_store::with_key`'s shape: lend the key to `f`, return its result.
pub fn with_key<R>(vault: &dyn KeyVault, name: &str, f: impl FnOnce(&dyn NoiseKey) -> R)
    -> Result<R, VaultError>;

/// Small non-secret blobs (the remotes list) in the same sandbox.
pub trait BlobStore: Send + Sync + 'static {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, VaultError>;
    fn put(&self, name: &str, bytes: &[u8]) -> Result<(), VaultError>;
    fn delete(&self, name: &str) -> Result<bool, VaultError>;
}

/// libmobile's `KeyStoreError`, one variant for one.
#[non_exhaustive]
pub enum VaultError {
    InvalidName, NotFound, WrongKind(String), Corrupt(String),
    DeviceLocked, Locked, UnsupportedFlags(u8), Platform(String),
}

/// An X25519 private key for vaults that store bytes: a heap box,
/// cleared on drop. Implements `NoiseKey`.
pub struct NoiseSecret(Box<[u8; 32]>);
```

- Names are 1 to 64 of `[A-Za-z0-9._-]`, not starting with `.`. Keys keep
  today's names: `noise.static` (legacy) and `noise.static.<remote id>`.
- `VaultStaticKey { vault: Arc<dyn KeyVault>, name, public }` implements
  `StaticKey`, its `dh` going through `with_key`: today's
  `apps/companion/src/keys.rs` `VaultStaticKey`, made generic over the
  trait.
- `RemotesStore<V: KeyVault, B: BlobStore>` is today's `KeyStore` trait's
  behaviour (`remotes`, `save_remotes`, `static_key`, `generate`,
  `remove_key`, the `server` → `remotes` migration) on any vault and
  blob store. `MemoryVault` implements both traits for tests.
- `libhardline-files`' `FileVault` and `FileBlobStore` keep today's
  `FileKeyStore` layout, so a key directory written before the split is
  read unchanged.
- flankr's app keeps its libmorph adapter in `apps/companion/src/keys.rs`:
  `NoiseStatic` implements libmorph's `SecretKey` as today (kind
  `flankr.noise.x25519.v1`, so a key already sealed on a phone is read
  unchanged) and libhardline's `NoiseKey`; today's `VaultKeyStore`
  becomes `MorphVault` (implementing `KeyVault` through
  `key_store::with_key`) and `MorphBlobStore` (implementing `BlobStore`
  on `SecureStore`).
- Synchronous by design, and documented as such: `with_secret` (one call
  per handshake's Diffie-Hellman) and the OS random source.

### 7. Sans-IO: the connection and the pairing session

The core does no I/O and depends on no runtime. Each link is a
`Connection` (client or server side), driven by whoever owns the socket:

```rust
pub struct Connection { /* handshake or transport state, queues, timers */ }
impl Connection {
    pub fn handle_input(&mut self, bytes: &[u8]) -> Result<(), LinkError>; // read from the transport
    pub fn send(&mut self, protocol: u16, message: &[u8]) -> Result<(), LinkError>; // queue an app message
    pub fn handle_timeout(&mut self, now: std::time::Instant);
    pub fn poll_output(&mut self) -> Option<Output>;
    pub fn close(&mut self, reason: CloseReason);
}
pub enum Output {
    Transmit(Vec<u8>),           // write to the transport
    Event(Event),
    Timeout(std::time::Instant), // wake at this instant
    Dial(SocketAddr),            // client only: open this address
}
pub enum Event {
    Ready { peer: [u8; 32], first: Vec<u8> },   // handshake done; `first` is the first message
    Message { protocol: u16, bytes: Vec<u8> },
    Closed(CloseReason),
}
```

- `protocol` is `u16`; see decision 9 for its range during this plan.
- The client side emits `Output::Dial(address)` for each address of a
  remote and takes `handle_dialed(address, Result<(), io::ErrorKind>)`;
  dialing and racing the addresses are the adapter's.
- `PairRequest` (client) emits `Output::Transmit` and `Event::Paired {
  result }` or `Event::Closed`. `PairSession` (server) emits
  `PairEvent::Request { words, device, key }` and `PairEvent::Ended`,
  and takes `accept(role)` and `deny()`.
- VPN providers (decision 8) emit `VpnAction::Run(CommandSpec { program,
  args, env })` and take `handle_output(status, stdout, stderr)`.
- The adapters wrap these: `libhardline-tokio`'s `LinkStream<T>` reads
  into `handle_input`, writes `Transmit`, sleeps to `Timeout`, and
  surfaces `Event::Message` as `recv()`. `libhardline-async` does the
  same over `futures_io::{AsyncRead, AsyncWrite}`.
- Builders only configure: `Client::builder()…build()` and
  `Server::builder()…build()` return state machines. `.listen(addr)` and
  `.connect()` are in the adapters.

### 8. The builders

Client (the app side; `libhardline-core`, driven here through
`libhardline-tokio`):

```rust
let client = libhardline::Client::builder()
    .profile(Profile::builder().url_scheme("myapp").build()?)
    .vault(vault)              // Arc<dyn KeyVault>
    .blobs(blobs)              // Arc<dyn BlobStore>
    .device_name("pixel-8")    // shown on the backend's prompt; at most DEVICE_NAME_MAX
    .connect_timeout(Duration::from_secs(10))
    .build()?;                 // a state machine, no I/O

match libhardline::read_pairing(client.profile(), pasted_text) { // PastedLink::{Found, Damaged, None}
    PastedLink::Found(url) => {
        let pending = libhardline::tokio::request_pairing(&client, &url).await?; // NK, MODE_PAIR
        show(pending.words());                                                   // the six words
        let remote = pending.wait().await?;                                      // the backend accepted
        client.remotes()?.add(remote)?;
    }
    ...
}
let stream: LinkStream<TcpStream> =
    libhardline::tokio::connect(&client, remote_id, first_message).await?;       // IK, MODE_RECONNECT
```

Server (the backend side, what `flankr pair` drives in the daemon):

```rust
let server = libhardline::server::Server::builder()
    .profile(profile)
    .key_file("~/.local/state/myapp/link.key")      // 0600, made if missing
    .authorized_keys("~/.local/state/myapp/authorized.toml")
    .addresses(AddressPolicy::Discover)               // or ::Fixed(vec![...]), ::LoopbackOnly
    .vpn(vpn)                                          // optional; needs service_prefix
    .service_prefix("dev.myapp.vpn")
    .build()?;                                         // a state machine, no I/O

let listener = libhardline::tokio::listen(&server, "0.0.0.0:7347".parse()?).await?;
let offer = server.offer_pairing(Duration::from_secs(60))?;      // offer.url(), offer.string()
while let Some(event) = listener.pair_events(&offer).next().await { // PairEvent::Request { words, device, key } / Ended
    if confirmed(&event) { offer.accept(&event, "companion")?; } else { offer.deny(&event)?; }
}
loop {
    let (stream, peer, first) = listener.accept().await?;  // an authenticated LinkStream, the peer's AuthEntry
    tokio::spawn(handle(stream, peer, first));
}
```

- `accept`'s `role` is the authorized-keys entry's free text; flankr
  passes `companion` or `harness` as today. libhardline knows no roles.
- What a backend does over the stream (JSON-RPC, anything) is its own.
  flankr's `link.pair.begin`/`link.pair.answer` JSON-RPC methods between
  `flankr pair` and the daemon stay in flankr, now calling
  `offer_pairing`.
- VPN naming, with no defaults: a provider's own id is fixed by the
  library as `dev.tmzt.hardline.vpn.<provider>` (what `Vpn::id()`
  returns and what logs and the service registry show). A tunnel's job
  id is `<service_prefix>.<provider>`; the prefix is required
  (`VpnBuilder::build()`, and `Server::builder()` with a VPN set, refuse
  without it) and is a reverse-DNS name of 3 to 64 characters of
  `[a-z0-9.-]` with at least two dots. The edge forward's label defaults
  to the profile's scheme; the state directory for the relay's log and
  state has no default; Bitmot's public port defaults to 443; setting up
  a tunnel reads the edge node's forwards and refuses, naming the other
  label, when another label already holds the public port.
  `libhardline-vpn` declares the `ServiceSpec` Bitmot's relay needs (id,
  program, args, working directory, env, stdout and stderr paths) and
  emits `VpnAction::EnsureService(ServiceSpec)` and
  `VpnAction::ServiceStatus(ServiceSpec)`; flankr's root crate converts
  it to `flankr_daemon::ServiceSpec` and keeps its registry. flankr sets
  the prefix `dev.flankr.vpn` and the label `flankr`, so its Bitmot job
  stays `dev.flankr.vpn.bitmot`.
- `libhardline-jsonrpc` is initialised on a protocol number the app
  chooses, `JsonRpc::on(protocol: u16)`; there is no default. flankr uses
  0.
- Two examples, `examples/pair_server.rs` and `examples/pair_client.rs`,
  take the scheme as an argument, run under `hltest` on loopback, and
  show the whole flow; a test runs them.

### 9. Protocol numbers during the extraction: the wire is unchanged

- `Connection::send(protocol: u16, …)` and `Event::Message { protocol:
  u16, … }` carry today's kind nibble as a number: 0 to 14 are sent and
  received as the flag byte's high nibble, exactly as today. 15 and up
  are refused by `send` (`LinkError::Protocol`) and, on receipt, close
  the connection with `BadFrame`, as today's reader does with
  `KIND_EXTENDED` (`0xF`) and every nibble it does not know.
- flankr keeps 0 (JSON-RPC), 1 (terminal) and 2 (blob), byte for byte:
  `libflankr_proto`'s `Message`, `Kind` and `KIND_*` stay as wrappers
  (decision 10) and refuse any other number as today.
- The pairing request's message version stays 1 and the pairing result
  stays today's frame.
- Left open on purpose for the later plan: the `u16` range above 14, and
  a `Channels` trait that `Connection` can take without changing its
  methods. Nothing in this plan defines them.

### 10. flankr afterwards: the transition in two stages

Stage 1: libhardline stands alone and is tested first (steps 1 to 6),
with flankr unchanged. Stage 2: each flankr crate moves onto it in its
own commit (steps 7 to 10), the wire unchanged and flankr's existing
tests unchanged as the check.

Each flankr crate depends on the `libhardline` facade directly, at one
shared pinned git rev (as libmorph is pinned), with only its features.
No flankr crate reaches libhardline only through `libflankr_proto`.

| flankr crate | Version today | libhardline features | What it keeps as wrappers | Bump |
| --- | --- | --- | --- | --- |
| `libflankr_proto` | 0.5.16 | `jsonrpc`, `tokio` | `link`'s public paths, constants (`PAIRING_PREFIX`, `PAIRING_URL_SCHEME`, `DEVICE_CODE_PREFIX`, `PROLOGUE_PREFIX`, `PAIRING_WORDS_CONTEXT`, `KIND_*`), `Kind`, `Message`, `LinkStream` and function signatures, fixed to `FLANKR_PROFILE`; `envelope`'s JSON-RPC types as re-exports | patch, 0.5.17: nothing removed or changed |
| `flankr-companion` | 0.8.16 | `tokio`, `files` | `keys::KeyStore`, `FileKeyStore`, `scan::Pairing` and `scan::read` with today's signatures | held at 0.8.x: at most a patch, none if no public item changes |
| `flankr` (the daemon) | 0.12.1 | `server`, `tokio`, and `vpn-tailscale`, `vpn-bitmot` passed through from its own `vpn-*` features | `src/server/link/mod.rs`'s `LinkSettings` and wiring | none: nothing the person running `flankr` sees changes; no `UPDATES.md` entry |
| `companion` (the app) | 0.8.2 | none (the core's traits only) | its libmorph adapter, implementing the traits | none: nothing the user sees changes; no `apps/UPDATES.md` entry |
| `flankr-daemon` | unchanged | none | `ServiceSpec`, `UserServiceBackend` as today | none |

- `libflankr_proto` drops its `snow`, `bs58`, `blake2` and `bip39`
  dependencies; its `link` feature stays and now enables the
  libhardline dependency.
- The wrappers may be removed later one at a time, each under Rule 5.4
  and the held-versions table; this plan removes none.
- Wire compatibility is required: a phone paired before the split
  reconnects to a daemon built after it, and the other way round,
  without pairing again; its Keystore key, sealed as
  `flankr.noise.x25519.v1`, is read unchanged.

### 11. History

1. A fresh clone of flankr (never the working checkout) at `7755c9f`,
   filtered with `git filter-repo` `--path` for every source file in
   decision 3's table, including the names before the rename
   (`crates/libhandlr_proto/src/link/*`,
   `crates/handlr-companion/src/{keys,net,scan}.rs`), and
   `--path-rename` into the new layout (`crates/libhardline-proto/src/`,
   `crates/libhardline-core/src/`, …). Its log is grepped for `.private`,
   `.env`, key material and the owner's private app names before it goes
   anywhere; a hit stops the step and is raised with the owner.
2. That filtered history is the start of `~/src/libhardline`'s `main`,
   under the setup commits, so `main` is linear. It keeps authorship and
   dates, so `git log --follow` and blame work. Files only partly moved
   (`net.rs`, `scan.rs`, `server/link/mod.rs`, `remotes.rs`,
   `envelope.rs`) come with their history and are cut down in the first
   logical commit that touches them.
3. On top, logical commits, one crate per commit: the workspace, then
   `libhardline-proto`, `libhardline-core`, `libhardline-server`,
   `libhardline-vpn`, `libhardline-jsonrpc`, `libhardline-files`,
   `libhardline-tokio`, `libhardline-async`, the facade, the examples.
   Each passes Rule 3.4's checks.

## Not covered

- The link protocol: protocol numbers 15 and up, extended numbers, link
  message types, runtime channels, a versioned `Hello` and the cut-over
  that moves flankr onto them. That is libhardline's own later plan,
  written in `~/src/libhardline/docs/design/` once the repository
  exists (step 1); it builds on decision 9's `u16` and `Connection`.
- Any change to the pairing flow or the UI.
- Publishing libhardline to crates.io, or making its remote public.

## Steps

1. **Set up the project.** Create `~/src/libhardline` with the files of
   decision 1, each in its own commit, create the private remote and
   register the project with repowiki. **Done when:** `make serve` in
   `~/src/repowiki` lists libhardline, its `AGENTS.md` starts with the
   rules byte for byte (`cmp` against
   `~/src/rules/rules/CRITICAL_RULES.md` on the leading part), and `git
   remote -v` names `github.com/tmzt/libhardline`.

2. **Import the history.** Run decision 11.1–11.2. **Done when:** `git
   log --follow crates/libhardline-proto/src/pairing.rs` in libhardline
   shows the commits that touched `libhandlr_proto`/`libflankr_proto`'s
   `link/pairing.rs` (the first is `3fb935d`'s), the grep of 11.1 finds
   nothing, and `main` is linear.

3. **libhardline-proto.** `Profile`, the `wire!` macro, `Reader`/`Writer`,
   every message, the flag byte, words and fingerprints, and
   `docs/WIRE.md`. **Done when:** today's `link/tests.rs` encoding tests
   pass under the flankr profile; recorded fixtures (hex, committed) of
   a flankr 0.12.1 pairing string, device code, and JSON, terminal and
   blob frames decode and re-encode byte for byte; a pairing URL,
   device code and words round-trip under `hltest`; a builder with no
   scheme, with `hardline`, or with `hardlinex` is refused; every
   message has a round-trip and a no-panic fuzz test; the `LAYOUT` test
   matches `docs/WIRE.md`.

4. **libhardline-core.** `Connection`, `PairRequest`, `PairSession`'s
   client half, `Client::builder()`, the vault traits, `NoiseSecret`,
   `VaultStaticKey`, `RemotesStore`, `MemoryVault`, `Backoff` and
   `read_pairing`. **Done when:** `cargo tree -p libhardline-core` shows
   no tokio, futures-io, smol or async-std; a test pairs and reconnects
   two cores in memory with no runtime, time driven by hand; a
   handshake runs through a test vault whose key type is foreign to
   libhardline; today's `keys.rs` tests, including the `server` →
   `remotes` migration, pass against `MemoryVault`; the pasted-link
   reader's tests pass under the flankr profile; `send(15, …)` is
   refused and a received nibble 15 closes with `BadFrame`.

5. **libhardline-server, libhardline-vpn, libhardline-jsonrpc,
   libhardline-files.** One crate per commit. **Done when:** the
   daemon's `pair_mode` tests pass against `PairSession`; `AuthFile`
   reads and writes today's authorized-keys file (fixture) with `role`
   stored as `protocol`; the VPN providers' tests check emitted commands
   against recorded output with no process run; a VPN builder with no
   service prefix is refused; two prefixes set up fake tunnels side by
   side without touching each other; a public port held by another
   label is refused; `JsonRpc::on(0)` round-trips today's envelope
   fixtures; `FileVault` reads a key directory written by today's
   `FileKeyStore` (fixture).

6. **libhardline-tokio, libhardline-async, the facade and the examples.**
   **Done when:** the pairing-and-reconnect test of step 4 passes
   through each adapter (the async one on a small executor); with
   `async-compat`, `cargo tree` shows no tokio; every facade feature
   builds alone and the facade with no features builds; with every
   feature on, the workspace's `cargo tree` shows no libmorph or
   libmobile; the examples pair and exchange a message on loopback under
   `hltest` in a test; each crate has its `VERSIONS.md` entry at 0.1.0.

7. **flankr: libflankr_proto 0.5.17.** Depend on the facade at a pinned
   rev with `jsonrpc` and `tokio`; replace `link`'s and `envelope`'s
   bodies with the wrappers of decision 10; drop snow, bs58, blake2 and
   bip39. **Done when:** `cargo test -p libflankr_proto` passes with its
   existing tests unchanged, `cargo tree -p libflankr_proto` shows no
   snow, bs58, blake2 or bip39 except through libhardline, and its
   `VERSIONS.md` entry names no breaking change.

8. **flankr: flankr-companion.** Depend on the facade with `tokio` and
   `files`; `keys` and `scan` become the wrappers of decision 10;
   `net.rs` calls the adapter. **Done when:** `cargo test -p
   flankr-companion` passes with its existing tests unchanged, and the
   version is unchanged or bumped one patch with a `VERSIONS.md` entry
   naming the public item that changed.

9. **flankr: the daemon.** Depend on the facade with `server`, `tokio`
   and the `vpn-*` pass-through; `src/server/link/` keeps `mod.rs`'s
   settings and wiring and calls `libhardline::server` and
   `libhardline::vpn`; the root crate converts `ServiceSpec`. **Done
   when:** `cargo test` at the root passes with its existing tests
   unchanged; `flankr service status vpn.bitmot` on the Mac mini still
   reports the existing `dev.flankr.vpn.bitmot` job; a daemon built from
   this step accepts a reconnect from the S23's current build without
   pairing again.

10. **flankr: the app.** `apps/companion/src/keys.rs` implements the
    vault traits over libmorph; `remotes.rs` calls
    `libhardline::read_pairing` with `FLANKR_PROFILE`. **Done when:**
    the workspace's tests pass; the S23, still paired with the Mac mini
    from before, reconnects to the daemon of step 9 without pairing
    again, and a build of this step reconnects to the daemon from before
    step 9; a fresh pairing by pasted link, QR code and device code
    works on the S23; no `apps/UPDATES.md` or `UPDATES.md` entry was
    needed.

11. **Publish.** Push libhardline to its private remote and flankr to
    both of its remotes, with tags (`<crate>-v0.1.0` in libhardline,
    `libflankr_proto`'s and any flankr-companion tag in flankr). **Done
    when:** flankr's `Cargo.lock` points at a libhardline rev that
    exists on `github.com/tmzt/libhardline`.
