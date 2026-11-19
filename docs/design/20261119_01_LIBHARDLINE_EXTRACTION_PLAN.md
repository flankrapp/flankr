---
title: Extracting the link, pairing and remotes code into libhardline, with a key-vault trait boundary and builders for pairing any backend
date: 2026-11-19
status: "Superseded (by [Extracting the link, pairing, remotes and VPN code into libhardline as sans-IO crates, with the wire unchanged](docs/design/20261119_02_LIBHARDLINE_EXTRACTION_PLAN.md))"
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

## Amendment — 2026-11-19 15:25 EST: VPN providers move, no default scheme or tunnel prefix, protocol-numbered framing

Agreed with the owner in discussion (Rule 2). The owner, verbatim: "no,
the VPN provides should move also, to dev.tmzt.hardline.vpn for now.
private repo for now. do we need to make the binary protocol part of this
new library allowing json-rpc etc. to be layered over it with protocol
numbers?"; "the task prefix should also be configurable, as each app will
have it's own tunnels"; "no default, if a cli app registers it needs to
set that prefix (the default was for the internal package name
identifying the plugins)"; "should we drop the hardline:// default, the
likely usage is for an app to register as a handler for that url with the
OS and I don't want other apps registering hardline://"; "yes merge
wt-paste-line first". Where this disagrees with the body, this wins.

### 1. No default URL scheme (replaces decision 3's defaults)

- `Profile` has no `Default` and no default values. `Profile::builder()`
  requires `url_scheme(...)`; `build()` refuses without one. The other
  names are derived from the scheme as decision 3 says, unless set
  explicitly.
- The scheme `hardline`, and any scheme starting with `hardline`, is
  refused, so no app built on the library registers `hardline://` with
  its OS.
- Tests and examples use the invented scheme `hltest`. The examples take
  the scheme as an argument. Decision 5's `Profile::default()` and
  decision 3's "default `hardline://`" go; step 3's test under
  `Profile::default()` becomes one under `hltest`, and step 6's examples
  run under `hltest`.
- flankr's profile is `url_scheme("flankr")`, as before.

### 2. The VPN providers move (replaces "the VPN providers" in "What stays in flankr" and decision 7's `vpn/` line)

- A fifth crate, `libhardline-vpn` 0.1.0, takes `src/server/link/vpn/`
  (`mod.rs`'s `Vpn` trait and choice, `tailscale.rs`, `bitmot.rs`,
  `fake.rs`) and the background-job definition Bitmot's relay needs,
  with features `vpn-tailscale` and `vpn-bitmot` as today. flankr keeps
  `flankr service`, its registry (`src/service.rs`) and its config keys,
  and calls into the crate.
- Each provider's own id is fixed by the library:
  `dev.tmzt.hardline.vpn.<provider>` (for example
  `dev.tmzt.hardline.vpn.bitmot`). It is what `Vpn::id()` returns and
  what logs and the service registry show.
- A tunnel's job id is `<service prefix>.<provider>`. The prefix has no
  default: `VpnBuilder::build()`, and `Server::builder()` with a tunnel
  enabled, refuse without `service_prefix(...)`. A prefix is a
  reverse-DNS name of 3 to 64 characters of `[a-z0-9.-]` with at least
  two dots. `Profile` holds no prefix.
- Also the app's: the edge forward's label (default: the profile's
  scheme), the state directory for the relay's log and state (no
  default), and Bitmot's public port (default 443). Setting up a tunnel
  reads the edge node's forwards and refuses, naming the other label,
  when another label already holds the public port.
- flankr sets the prefix `dev.flankr.vpn` and keeps the label `flankr`,
  so its Bitmot job stays `dev.flankr.vpn.bitmot`: no migration, no
  change its user sees, no daemon version bump.

### 3. Protocol-numbered framing (adds to decisions 2 and 5)

- libhardline owns the framing: the 2-byte length, Noise, chunking and
  the flag byte. The flag byte's high nibble, today's message kind,
  becomes a protocol number. Numbers 0 to 14 keep their one-byte form.
  15 is defined: an unsigned LEB128 protocol number (15 and up) follows
  the flag byte in each frame of the message. It is sent only to a peer
  that has declared it reads that number; a peer that has not still
  closes on it, as today.
- The library knows no protocols. `LinkStream` gains
  `send(protocol: u32, bytes)` and `recv() -> (u32, Vec<u8>)`; today's
  `Message` enum and `KIND_*` constants stay in flankr, mapped to 0
  (JSON-RPC), 1 (terminal) and 2 (blob), so flankr's frames are byte for
  byte as today.
- An opt-in protocol list: one reserved protocol number carries a
  message listing the protocol numbers each side reads. flankr does not
  use it and keeps negotiating through its JSON `hello` (`blob`).
- A sixth crate, `libhardline-jsonrpc` 0.1.0, optional: the JSON-RPC
  2.0 types (`Request`, `Notification`, `Incoming`, `Id`,
  `ErrorObject`) moved from `libflankr_proto`, sent on protocol 0 by
  default. flankr's own methods and types stay in `libflankr_proto`.
- The authorized-keys entry's `protocol` (`companion`, `harness`) is
  called `role` in the library's API and still read and written as the
  file's `protocol` field.

### 4. Answers to the open questions

1. Remote: `github.com/tmzt/libhardline`, **private** for now. Step 10
   pushes there.
2. History: the filtered history is kept (decision 6), as recommended.
3. flankr-companion's held series: still open; step 9 waits for it.
4. `wt-paste-link`: merged first. It is merged (`7755c9f`), so step 2
   may start.

### 5. Added to the steps' Done when

- Step 3: a test sends a message on a protocol number of 15 or more
  between two peers that both declared it, and a test shows flankr's
  JSON, terminal and blob frames are unchanged byte for byte; a builder
  with no scheme, or with `hardline`, is refused.
- A new step 7a, `libhardline-vpn`: **Done when:** a builder with no
  service prefix is refused; two apps with different prefixes set up
  fake tunnels side by side without touching each other; a public port
  held by another label is refused; and flankr's `flankr service status
  vpn.bitmot` still reports the existing `dev.flankr.vpn.bitmot` job on
  the Mac mini.
- Step 8 also: `libhardline-jsonrpc`'s types replace
  `libflankr_proto`'s, and `cargo test -p libflankr_proto` passes.

## Amendment — 2026-11-19 15:26 EST: flankr's crates keep their APIs through wrappers, patch bumps only

The owner, verbatim: "with the wrappers it should already be compatible
and the use of a different library is an internal matter (thie UI
shouldn't change at all), we also burned a lot of bumps recently". This
answers open question 3 and replaces decision 7's first two bullets and
step 8's **Breaking** condition.

- flankr-companion stays in its held 0.8.x series. Its public `keys` and
  `scan` APIs keep their current signatures as thin wrappers over
  libhardline with flankr's profile; the move is internal, so it takes at
  most a patch bump, and none if no public item changes.
- `libflankr_proto` does the same: it keeps `link`'s public paths,
  constants (`PAIRING_PREFIX`, `PAIRING_URL_SCHEME`,
  `DEVICE_CODE_PREFIX`, `PROLOGUE_PREFIX`, `PAIRING_WORDS_CONTEXT`,
  `KIND_*`), `Message` and function signatures as re-exports and
  wrappers fixed to `FLANKR_PROFILE`, and its JSON-RPC types as
  re-exports of `libhardline-jsonrpc`'s. With nothing removed or changed
  it is a patch, 0.5.16 to 0.5.17, not 0.6.0. Step 8's **Done when:**
  becomes: `cargo test -p libflankr_proto` passes with its existing
  tests unchanged, and its `VERSIONS.md` entry names no breaking change.
- The app and the daemon change nothing a user sees: no app or daemon
  version bump, and no `apps/UPDATES.md` or `UPDATES.md` entry.
- Step 9 no longer waits on an answer.

## Amendment — 2026-11-19 15:38 EST: sans-IO core, crates and features, published vault traits, the all-ones link protocol and its versioning

Agreed with the owner in discussion (Rule 2). The owner, verbatim: "also,
the library should be sans-IO with async-friendly enums, not a specific
async implementation"; "opt in to tokio or async-compat with features,
separate crates inside the project (and separate per-crate dependency on
this new library)"; "no morph dependecy, libhardline should publish
compatble traits instead"; "right, it would be better to move that onto
protocol 0, binary frames so we don't force jsonrpc on users. this would
mean a protocol bump for flankr, with a compatible hello message"; "that's
the other option, use all-ones for the link protocol"; "message_major and
major/minor/patch with semver rules (one byte per value), offers lowest
compatible accepts highest compatible, tied to the -proto or -core crate
version"; "yes". Where this disagrees with the body or the 15:25 EST
amendment, this wins.

### 1. Crates (replaces decision 2's table and the 15:25 amendment's crate additions)

| Crate | Holds | I/O |
| --- | --- | --- |
| `libhardline-proto` | the wire format only: frames, the flag byte, the link protocol's messages, pairing strings and URLs, device codes, pairing request messages, words, fingerprints, `Profile` | none |
| `libhardline-core` | the state machines: `Connection`, `PairRequest` (client) and `PairSession` (server side of one request), the client (`Client::builder()`), the vault traits (section 3), `NoiseSecret`, `VaultStaticKey`, `RemotesStore`, the pasted-link reader | none |
| `libhardline-server` | `Server::builder()`, key-file and authorized-keys formats, address choice (`Machine`, `FakeMachine`), pairing offers, `revoke(key)` | none |
| `libhardline-vpn` | the providers as state machines emitting commands, features `vpn-tailscale`, `vpn-bitmot` | none |
| `libhardline-jsonrpc` | JSON-RPC 2.0 types (`Request`, `Notification`, `Incoming`, `Id`, `ErrorObject`), on an app-chosen protocol number | none |
| `libhardline-tokio` | `LinkStream<T: tokio AsyncRead + AsyncWrite>`, connect, pairing request, accept loop over `TcpListener`, VPN commands by `tokio::process`, `ThisMachine` | tokio |
| `libhardline-async` | the same over `futures-io`'s traits, for smol or async-std, and for tokio code through the `async-compat` crate | the caller's executor |
| `libhardline-files` | `FileVault`, `FileBlobStore` (today's `FileKeyStore` layout) | `std::fs` |
| `libhardline` | facade, no default features: the core re-exported; features `server`, `vpn-tailscale`, `vpn-bitmot`, `jsonrpc`, `files`, `tokio`, `async-compat`, each pulling in its crate and re-exporting it as a module (`libhardline::server`, `libhardline::tokio`, …) | per feature |

`libhardline-morph` and the `morph` feature are dropped. All crates start
at 0.1.0.

### 2. Sans-IO (replaces decision 5's async signatures)

- The core does no I/O and depends on no runtime. Each link is a
  `Connection` (client or server side), driven by the caller:

  ```rust
  conn.handle_input(&bytes)?;            // bytes read from the transport
  conn.send(protocol, &message)?;        // queue an app message
  conn.handle_timeout(now);              // now: std::time::Instant
  while let Some(out) = conn.poll_output() {
      match out {
          Output::Transmit(bytes) => { /* write to the transport */ }
          Output::Event(Event::Ready { peer, first, version }) => {}
          Output::Event(Event::Message { protocol, bytes }) => {}
          Output::Event(Event::Closed(reason)) => {}
          Output::Timeout(at) => { /* wake at `at` */ }
      }
  }
  ```

- `PairSession` emits `PairEvent::Request { words, device, key }` and
  `PairEvent::Ended`, and takes `accept(role)` and `deny()`. The client
  emits `Output::Dial(address)` and takes `handle_dialed(address,
  result)`; dialing and racing are the caller's. VPN providers emit
  `VpnAction::Run(CommandSpec { program, args, env })` and take
  `handle_output(status, stdout, stderr)`.
- Builders only configure: `Client::builder()…build()` and
  `Server::builder()…build()` return state machines; `.listen(addr)` is in
  the adapters.
- Synchronous by design, documented as such: the vault's `with_secret`
  (one call per handshake's Diffie-Hellman) and the OS random source.

### 3. Published vault traits, no libmorph (replaces decision 4's `KeyVault` and `BlobStore`)

```rust
pub trait NoiseKey {
    fn public(&self) -> [u8; 32];
    fn dh(&self, peer: &[u8; 32]) -> Option<[u8; 32]>; // None on all-zero
}
pub trait KeyVault: Send + Sync + 'static {
    fn generate(&self, name: &str) -> Result<[u8; 32], VaultError>;
    fn public(&self, name: &str) -> Result<[u8; 32], VaultError>;
    fn with_secret(&self, name: &str, f: &mut dyn FnMut(&dyn NoiseKey)) -> Result<(), VaultError>;
    fn delete(&self, name: &str) -> Result<bool, VaultError>;
}
pub fn with_key<R>(vault: &dyn KeyVault, name: &str, f: impl FnOnce(&dyn NoiseKey) -> R)
    -> Result<R, VaultError>;
pub trait BlobStore: Send + Sync + 'static {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, VaultError>;
    fn put(&self, name: &str, bytes: &[u8]) -> Result<(), VaultError>;
    fn delete(&self, name: &str) -> Result<bool, VaultError>;
}
#[non_exhaustive]
pub enum VaultError { InvalidName, NotFound, WrongKind(String), Corrupt(String),
                      DeviceLocked, Locked, UnsupportedFlags(u8), Platform(String) }
```

- The shape is libmorph's `key_store::{generate, public, with_key,
  delete}` and `SecureStore`'s; `VaultError`'s variants are
  `KeyStoreError`'s one for one; names follow its rules (1 to 64 of
  `[A-Za-z0-9._-]`, not starting with `.`).
- `NoiseSecret` (zeroized, boxed) implements `NoiseKey` for the file and
  memory vaults; other vaults lend their own key type.
- `Profile` loses `key_kind`. The libmorph adapter stays in flankr's
  `apps/companion/src/keys.rs`: `NoiseStatic` implements libmorph's
  `SecretKey` as today (kind `flankr.noise.x25519.v1`) and libhardline's
  `NoiseKey`; `MorphVault` implements `KeyVault` through
  `key_store::with_key`; `MorphBlobStore` implements `BlobStore` on
  `SecureStore`.

### 4. Protocol numbers (replaces the 15:25 amendment's section 3, first two bullets and its protocol list)

- The flag byte's high nibble: 0 to 13 are the app's protocols; 14 is
  extended (an unsigned LEB128 protocol number, 14 and up, follows the
  flag byte in each frame of the message); 15 (`0b1111`) is libhardline's
  link protocol. A peer that has not shown it reads 14 or 15 is not sent
  them; today's peers close on both.
- flankr keeps 0 (JSON-RPC), 1 (terminal) and 2 (blob), byte for byte.
  `libhardline-jsonrpc` takes its protocol number from the app; no number
  is JSON-RPC's by default.
- Link protocol messages are binary: `type` (1 byte), `message_major`
  (1 byte), then the body. A receiver closes with
  `Close { reason: UnknownMessage }` on a `message_major` it does not
  know. Types: `Hello`, `PairResult`, `Unpair`, `Unpaired`, `Close`.
- `Unpair` replaces flankr's `link.unpair` between peers that negotiated
  the link protocol; the server's `revoke(key)` removes the key and closes
  its connections. Between other peers flankr keeps `link.unpair`.
- A pairing session is not a general connection. The pairing request's
  message version goes from 1 to 2: a version 2 request gets its
  `PairResult` on 15; a version 1 request gets it on 0, as today. A new
  phone pairing with a daemon that predates this sends version 1 (it
  learns which from the pairing string's version, bumped from 3 to 4 for
  daemons that accept version 2).

### 5. Versioning of the link protocol

- `Hello` carries `lowest` and `own`, each three bytes, major, minor and
  patch, 0 to 255 each. The opener sends `Hello { lowest, own }`; the
  other side computes `min(own_a, own_b)`, checks it is at or above both
  `lowest` values and that both `own` versions are compatible, and
  replies `Hello { accepted }`, or `Close { reason: Incompatible {
  lowest, own } }` with its own range.
- Compatible follows cargo's semver: the same major, and while the major
  is 0, the same minor as well. `lowest` is the first version of the
  sender's compatible series unless the code deliberately keeps an older
  series.
- The protocol version is `libhardline-proto`'s crate version, read from
  `CARGO_PKG_VERSION_*` at compile time; a build check fails if a part
  exceeds 255. A wire change peers cannot read is a breaking change under
  Rule 5.4 and bumps the minor while at 0.x; an added message type is a
  patch.
- flankr's JSON `hello` gains `"link": true` in both `HelloParams` and
  `HelloResult` (neither refuses unknown fields today). When both carry
  it, the peers exchange the binary `Hello` on 15 and use the link
  protocol from then on. flankr's own `hello` `version` stays for its
  JSON-RPC methods. Native apps send the binary `Hello` as the first
  message.

### 6. flankr's dependencies (adds to the 15:26 amendment)

Each flankr crate depends on the `libhardline` facade itself, at one
shared pinned rev, with only its features: `libflankr_proto`
(`jsonrpc`, `tokio`, for its compatibility wrappers); `flankr-companion`
(`tokio`, `files`); the daemon (`server`, `tokio`, and `vpn-tailscale`,
`vpn-bitmot` from its own `vpn-*` features); the app (nothing beyond what
flankr-companion brings, plus its own libmorph adapter). No flankr crate
reaches libhardline only through `libflankr_proto`.

### 7. Steps (replaces steps 3–7 and the 15:25 amendment's step 7a; steps 1, 2, 8–10 stay, with step 2's path read as `crates/libhardline-proto/src/pairing.rs`)

3. **libhardline-proto.** The wire format under `Profile`, the flag byte
   with 14 and 15, the link protocol's messages and `Hello` versioning.
   **Done when:** today's `link/tests.rs` encoding tests pass under the
   flankr profile; flankr's JSON, terminal and blob frames are byte for
   byte today's (recorded fixtures); a message on protocol 14 with number
   20 round-trips; `Hello` negotiation gives 0.1.3 for 0.1.3 and 0.1.7,
   refuses 0.1.x against 0.2.x with both ranges, refuses a `lowest` above
   the other side's `own`, and closes on an unknown `message_major`; a
   test checks the protocol version equals `CARGO_PKG_VERSION`; a builder
   with no scheme or with `hardline…` is refused.
4. **libhardline-core.** `Connection`, pairing, client, vault traits,
   remotes, pasted-link reader. **Done when:** with no features,
   `cargo tree -p libhardline` shows no tokio, futures-io, smol or
   async-std; a test pairs and reconnects two cores in memory with no
   runtime, time driven by hand; a handshake runs through a test vault
   whose key type is foreign to libhardline; the `keys.rs` tests,
   including the `server` → `remotes` migration, pass against a memory
   vault; version 1 and version 2 pairing requests get their result on 0
   and on 15.
5. **libhardline-server, libhardline-vpn, libhardline-jsonrpc,
   libhardline-files.** One crate per commit. **Done when:** the
   daemon's `pair_mode` tests pass against the server state machine; the
   VPN providers' tests check emitted commands and recorded output with
   no process run; a VPN builder with no service prefix is refused; two
   prefixes set up fake tunnels side by side; a public port held by
   another label is refused; `FileVault` reads a key directory written by
   today's `FileKeyStore` (fixture).
6. **libhardline-tokio and libhardline-async, and the facade.** **Done
   when:** the same pairing-and-reconnect test passes through each
   adapter (the async one on a small executor); with `async-compat`,
   `cargo tree` shows no tokio; every facade feature builds alone; with
   every feature on, the workspace's `cargo tree` shows no libmorph or
   libmobile; the examples pair and exchange a message on loopback under
   `hltest`.
7. **flankr compatibility checks**, added to step 9's **Done when:** a
   phone built before this with a daemon built after, and the other way
   round, use today's frames exactly; two new flankr peers exchange
   `Hello` on 15 after the JSON hello; flankr's `flankr service status
   vpn.bitmot` still reports the `dev.flankr.vpn.bitmot` job; the S23's
   current build reconnects to the new daemon, and a new build
   reconnects to the current daemon, without pairing again.

## Amendment — 2026-11-19 16:16 EST: superseded by the extraction plan

This draft is superseded before implementation by [Extracting the link,
pairing, remotes and VPN code into libhardline as sans-IO crates, with the
wire unchanged](docs/design/20261119_02_LIBHARDLINE_EXTRACTION_PLAN.md).
The owner, 2026-11-19, in discussion: "this plan is about the extraction.
let's review the finalized plan and write a new version focused on that".

What moved where: the extraction content of the body and of the 15:25,
15:26 and 15:38 EST amendments (the project and its private remote, the
history import, the nine crates, `Profile` with no default scheme, the
published vault traits with no libmorph dependency, the sans-IO
`Connection`, the builders, the VPN providers and their naming, the
compatibility wrappers and patch-only bumps, and the steps) goes to the
extraction plan, rewritten with the wire unchanged. The link protocol
design discussed after the 15:38 EST amendment (protocol numbers 15 and
up, the all-ones link protocol and its messages, channels, the versioned
`Hello`, the cut-over, and the 15:25 amendment's protocol-numbered
framing and the 15:38 amendment's sections 4 and 5) goes to a later plan
in libhardline's own `docs/design/`, once that repository exists; the
extraction plan names it only under "Not covered". Nothing in this
document is implemented.
