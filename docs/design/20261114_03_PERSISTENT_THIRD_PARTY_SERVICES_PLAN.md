---
title: Persistent third-party services, with Bitmot as the first VPN provider
date: 2026-11-14
status: Draft
covers:
  - "A platform-neutral user-service trait and service specification, with a macOS user launchd backend first and later backends such as systemd --user added behind the same trait"
  - "A `flankr service` command that controls flankr and enabled third-party services, while `flankr serve`, `flankr server`, and existing stop/restart forms remain shortcuts"
  - "The existing `Vpn` trait as the identical link-facing seam for Tailscale and Bitmot, with each provider enabled by a root `vpn-*` Cargo feature"
  - "Bitmot's raw TCP edge-node relay as the first persistent third-party service, named `dev.flankr.vpn.bitmot`, surviving flankr restarts and upgrades"
  - "Implementation on a separate branch from the planning branch"
  - "Not covered: Bitmot's root `io1d` service, HTTP/WebSocket adaptation, or a non-Bitmot VPN implementation"
sources:
  - "Owner (2026-11-14, verbatim): \"dev.flankr.vpn.bitmot is a better name. this should be a plan for persistent third-party services. also, we need to make sure the trait seem is the same between vpns and enabled them with vpn- features, it should be on a different branch also (the plan can stay on this branch), the flankr serve subcommand should support these third-party services or we should add a new subcommand and treat flankr as one of the services it can control, with the existing methods remaining as shortcuts\""
  - "Owner (2026-11-14, verbatim): \"as for non-MacOS we should use a trait for that as well so it can be added later, the concept of a user-level service isn't Mac specific\""
  - "Bitmot Edge documentation: https://docs.bitmot.com/edge (read 2026-11-14)"
  - "Superseded draft: docs/design/20261114_02_BITMOT_VPN_PLAN.md"
  - "Existing background-start design: docs/design/20261102_01_BACKGROUND_DAEMON_PLAN.md"
  - "Current VPN seam: `src/server/link/vpn/mod.rs` and `src/server/link/mod.rs`"
  - "Current command dispatch: `src/main.rs`"
related:
  - docs/design/20261114_02_BITMOT_VPN_PLAN.md
  - docs/design/20261102_01_BACKGROUND_DAEMON_PLAN.md
---

# 1. Goal and boundary

Flankr needs a user-level service layer for processes that must outlive one
flankr server process. The first service is the Bitmot raw-TCP relay used by the
link, but the lifecycle API must not be a Bitmot or macOS abstraction.

The service layer has three separate contracts:

1. `flankr-daemon` supplies a platform-neutral persistent user-service trait and
   service specification.
2. A platform backend implements that trait. macOS user launchd is first;
   systemd `--user` or another user-level supervisor can be added later without
   changing service providers or the CLI.
3. The flankr binary registers service providers. `flankr` itself and enabled
   third-party providers are controlled through one service command. Existing
   `serve` forms remain compatibility shortcuts.

The Bitmot provider uses the existing raw TCP flankr protocol. It does not
introduce HTTP, WebSocket, a companion transport, or a second VPN trait.

# 2. Findings and decisions

## 2.1 Existing server and VPN seams

The current `flankr-daemon` crate has `Detached`, a one-shot macOS GUI-domain
bootstrap that starts `flankr server`; the started server forks out of the job.
It also has a direct `setsid` fallback. This is useful process-launch code but
is not a persistent service API.

The current link uses one trait for all VPN drivers:

```rust
#[async_trait]
pub trait Vpn: Send + Sync {
    fn name(&self) -> &'static str;
    async fn running(&self) -> Result<bool, VpnError>;
    async fn address(&self, port: u16) -> Result<Option<VpnAddress>, VpnError>;
    async fn forwards(&self, port: u16) -> Result<bool, VpnError>;
    async fn forward(&self, port: u16) -> Result<(), VpnError>;
    fn forward_command(&self, port: u16) -> String;
}
```

Tailscale and Bitmot must both implement this exact link-facing seam. A
provider may use the service layer internally, but `Link` must not know which
VPN is persistent, which process manager it uses, or which operating system it
runs on.

## 2.2 Cargo feature gates

Add root-package features:

```toml
[features]
default = ["vpn-tailscale"]
vpn-tailscale = []
vpn-bitmot = []
```

The feature names are part of the build contract. `vpn-tailscale` gates the
Tailscale implementation and `vpn-bitmot` gates the Bitmot implementation.
The common `Vpn` trait, `VpnChoice`, `VpnError`, `VpnAddress`, and fake driver
remain available so link tests do not depend on a real provider.

The default build keeps today's Tailscale behavior. A Bitmot-enabled build
uses `--features vpn-bitmot`; a build may explicitly select both providers.
When a config or flag names a provider whose feature is absent, parsing fails
with a message naming the missing `vpn-*` feature. It must not silently fall
back to `none` or Tailscale.

Feature-gated modules must retain the same constructors and `Vpn` behavior:

```text
src/server/link/vpn/mod.rs       common trait, choice, errors, fake
src/server/link/vpn/tailscale.rs #[cfg(feature = "vpn-tailscale")]
src/server/link/vpn/bitmot.rs    #[cfg(feature = "vpn-bitmot")]
```

`cargo test --all-features` exercises every provider; a default/no-feature
build verifies that disabled providers do not leak symbols or accepted config
values.

## 2.3 Platform-neutral user-service API

Extend `crates/flankr-daemon` with a generic service layer that does not mention
flankr, VPNs, Bitmot, launchd, or systemd in its public contract. The core
shapes are:

```rust
pub struct ServiceSpec {
    pub id: String,
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub working_dir: PathBuf,
    pub env: Vec<(OsString, OsString)>,
    pub stdout: PathBuf,
    pub stderr: PathBuf,
    pub keep_alive: bool,
}

pub enum ServiceStatus {
    Running { pid: Option<u32> },
    Loaded,
    Stopped,
    Failed(String),
    Unsupported(String),
}

pub trait UserServiceBackend: Send + Sync {
    fn status(&self, spec: &ServiceSpec) -> io::Result<ServiceStatus>;
    fn ensure(&self, spec: &ServiceSpec) -> io::Result<ServiceStatus>;
    fn stop(&self, spec: &ServiceSpec) -> io::Result<ServiceStatus>;
    fn restart(&self, spec: &ServiceSpec) -> io::Result<ServiceStatus>;
}
```

The exact Rust names may follow the existing crate style, but the separation is
required:

- `ServiceSpec` describes one long-lived process with fixed argv and a
  restricted environment. Values are never passed through a shell.
- `UserServiceBackend` is selected by platform/runtime context. Its callers do
  not invoke `launchctl` directly.
- `ServiceStatus` distinguishes loaded, running, stopped, failed, and no
  backend. A backend must not claim persistence it cannot provide.
- The existing `Detached` API remains for the flankr server's one-shot
  background start. It is not made to serve both semantics through flags.

Implement first:

- `LaunchdUserBackend` on macOS, using `launchctl bootstrap`, `print`,
  `kickstart`, and `bootout` in `gui/<uid>`; private mode-0600 plists in
  mode-0700 temporary directories; `RunAtLoad`, `KeepAlive`, and
  `StandardInPath=/dev/null`.
- `UnsupportedBackend` elsewhere, with a clear status/error saying a
  persistent user-service backend is not installed yet. The trait remains
  available for a later `SystemdUserBackend` or other implementation. The
  existing direct detached daemon fallback remains independent of this API.

Service state and logs use `~/.local/state/flankr/services/` with directory
0700 and files 0600. A stable service id is used for reconciliation; a plist is
not left in a writable temporary directory after bootstrap. The state records
the provider's safe argv/environment fingerprint, not credentials.

## 2.4 Service registry and command

Add a root-level service registry/provider seam, separate from the low-level
backend:

```rust
pub trait ServiceProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn spec(&self, context: &ServiceContext) -> Result<ServiceSpec, String>;
    fn status(&self, context: &ServiceContext) -> Result<ServiceStatus, String>;
    fn start(&self, context: &ServiceContext) -> Result<ServiceStatus, String>;
    fn stop(&self, context: &ServiceContext) -> Result<ServiceStatus, String>;
    fn restart(&self, context: &ServiceContext) -> Result<ServiceStatus, String>;
}
```

The registry initially contains:

- `flankr`: the daemon. Its provider wraps the existing socket/pid/log
  background start and stop logic.
- `vpn.bitmot`: present only when `vpn-bitmot` is enabled and the selected link
  configuration supplies a Bitmot endpoint. Its provider uses the same Bitmot
  command builder and service spec as the `Bitmot` implementation of `Vpn`;
  command construction must exist in one place.

Tailscale remains an external service managed by its own installation. Its
link driver keeps the same `Vpn` trait and does not become a launchd job merely
because the registry exists. If a later provider needs user-service control,
it registers through the same `ServiceProvider` seam.

Add:

```text
flankr service list
flankr service status <service>
flankr service start <service>
flankr service stop <service>
flankr service restart <service>
```

Names are stable (`flankr`, `vpn.bitmot`) and the output reports backend and
persistence status. The command accepts the same config and endpoint context
needed to construct a provider. It never installs Bitmot's root `io1d`
service.

Existing commands remain shortcuts:

```text
flankr serve                  = service start flankr
flankr serve --stop           = service stop flankr
flankr serve --restart        = service restart flankr
flankr serve --foreground     = run flankr in the foreground
flankr server                 = run flankr in the foreground
```

A server restart preserves `vpn.bitmot`; a full `service stop flankr` also
stops the Bitmot service selected as its link dependency. `service stop
vpn.bitmot` controls only that relay. `service start vpn.bitmot` is explicit
and does not implicitly start the flankr daemon; it requires a valid endpoint
configuration and reports if the local link target is not listening.

The registry must make dependency behavior visible in status and logs rather
than hiding a second service operation inside the old `serve` implementation.

## 2.5 Bitmot provider and service identity

Bitmot's Edge documentation says an edge node owns stable public IPv4/IPv6
addresses and forwards raw TCP directly to the local relay. The node name is a
real DNS label such as `mynode.io1.io`. Hostnames beneath the node are L7 HTTP
hostnames and are not used for this protocol.

The setting remains:

```toml
[link]
vpn = "bitmot"
bitmot_endpoint = "mynode"
```

The endpoint is a raw Bitmot edge-node name/host accepted by `--node`, not an
arbitrary HTTP child subdomain. The local and public ports are the same in the
first implementation. The provider's exact argv is:

```text
~/.local/bin/bitmot io1 relay --node mynode --tcp 7347=127.0.0.1:7347
```

The persistent job label is exactly:

```text
dev.flankr.vpn.bitmot
```

The provider's `Vpn` implementation returns the same `VpnAddress` and pairing
offer behavior as Tailscale. It does not change `libflankr_proto`, companion
TCP connection code, Noise framing, or `host:port` pairing payloads.

The launchd service uses the absolute Bitmot executable, a restricted
`HOME`/`PATH` environment, `/dev/null` input, mode-0600 logs, `RunAtLoad`, and
`KeepAlive`. It uses Bitmot's cached authorization and never includes a
credential value in argv, plist, or logs. It does not pass `--public`.

A Bitmot service spec is reconciled by safe fingerprint:

- matching `dev.flankr.vpn.bitmot` job: reuse it;
- missing job: bootstrap it;
- changed node, port, executable, or safe environment: bootout and bootstrap
  the replacement;
- explicit service stop or a non-Bitmot link choice: bootout it.

The server's normal crash, restart, or flankr binary replacement does not
remove the relay. The next server reconciles it before publishing pairing
addresses. An explicit stop removes it so a stopped daemon does not leave a
public raw port running indefinitely.

# 3. Branch and commit policy

The plan and its planning-record updates remain on the current planning
branch. Before implementation, create this branch from the commit containing
the approved plan:

```text
feature/persistent-third-party-services
```

No implementation commits land on the planning branch. The implementation
branch is cut before the plan changes to `In implementation`; its first commit
sets that status and appends the immutable-marker amendment.

Implementation commits follow the repository's one-crate rule:

1. `flankr-daemon`: persistent service spec/backend trait, launchd backend,
   unsupported backend, state/log handling, tests, and its additive crate
   version/VERSIONS entry.
2. Root `flankr`: service registry/command, server shortcuts, feature-gated
   VPN modules, Bitmot provider, link/config/CLI integration, daemon version,
   and `UPDATES.md` entry.
3. Repository-level documentation or tooling only when needed, in its own
   commit. The companion app is not changed.

# 4. Implementation steps

## Step 1 — Freeze service and branch contracts

Approve the platform-neutral `UserServiceBackend`, `ServiceSpec`, provider
IDs, command shortcuts, feature names, Bitmot label, and branch name. Create
the implementation branch from the plan commit before code changes.

**Done when:** the implementation branch exists, the planning branch has no
code changes, and the service/VPN/feature interfaces are written down without
macOS-specific types leaking into the common seam.

## Step 2 — Implement the service backend crate

Add the persistent service API to `flankr-daemon`, keeping `Detached` intact.
Implement macOS launchd user jobs and an explicit unsupported backend elsewhere.
Cover plist escaping, fixed argv, `KeepAlive`, `/dev/null` input, state/log
permissions, status, reconciliation, replacement, restart, and bootout.

**Done when:** crate tests prove the launchd spec and unsupported-platform
behavior without installing a real service, and its format/clippy/test checks
pass.

## Step 3 — Add the generic registry and `flankr service`

Add `ServiceProvider`, the registry, `flankr` and enabled provider entries, and
the five service operations. Route `serve`, `serve --stop`, `serve --restart`,
`serve --foreground`, and `server` through the registry or documented
foreground path while preserving current socket/pid/log behavior.

**Done when:** command tests prove list/status/start/stop/restart dispatch,
shortcut equivalence, unknown service errors, dependency stop behavior, and no
implicit Bitmot activation when its feature/config is absent.

## Step 4 — Feature-gate the common VPN seam

Add `vpn-tailscale` and `vpn-bitmot`, gate only the provider modules and
provider-specific variants, preserve the default Tailscale behavior, and make
missing-feature errors explicit. Keep fake link tests provider-neutral.

**Done when:** default, no-default, Bitmot-only, and all-feature builds compile
and test; Tailscale and Bitmot are both accepted only when their corresponding
feature is enabled, and both implement the same `Vpn` trait.

## Step 5 — Implement Bitmot as a persistent provider

Add the Bitmot endpoint setting/flag, executable discovery, raw command builder,
`dev.flankr.vpn.bitmot` service spec, service reconciliation, `Vpn` methods,
link integration, pairing address, and safe diagnostics. Ensure server restart
and binary replacement reuse the service while an explicit service stop removes
it.

**Done when:** fake backend tests prove exact argv and lifecycle, link tests
show the Bitmot `host:port` first, and no WebSocket or companion changes are
needed.

## Step 6 — Run macOS end-to-end verification

On macOS with an authorized Bitmot edge node:

1. build flankr with `--features vpn-bitmot`;
2. start `flankr service start vpn.bitmot` and inspect
   `launchctl print gui/<uid>/dev.flankr.vpn.bitmot`;
3. start/restart flankr and verify the relay job remains loaded;
4. pair a companion and verify the unchanged raw TCP/Noise path;
5. replace or rebuild flankr and verify the relay process is unaffected;
6. run `flankr service stop vpn.bitmot` and verify the job is gone;
7. verify `flankr serve`, `serve --restart`, and `serve --stop` retain their
   shortcut behavior.

**Done when:** service status, launchd state, pairing, restart persistence, and
explicit cleanup are observed and recorded.

## Step 7 — Complete release records

Bump `flankr-daemon` for its additive public API with `VERSIONS.md` and tag;
bump the root daemon by the next minor version for the user-visible `service`
command and Bitmot feature, append `UPDATES.md`, and document feature-enabled
builds and service commands. Do not bump the companion app.

**Done when:** all affected documentation, versions, tags, and feature/help
text agree with the implementation branch.

# 5. Risks and controls

- **Service manager divergence:** all providers use `ServiceSpec` and the
  platform-neutral backend trait; launchd commands are confined to one backend.
- **Stale public relay:** explicit stop and endpoint-change reconciliation
  boot out `dev.flankr.vpn.bitmot`; server restart intentionally preserves it.
- **Credential leakage:** cached Bitmot authorization is resolved by Bitmot;
  plist, argv, state, and logs contain no secret.
- **Protocol exposure:** raw TCP is reachable, but the existing Noise key and
  pairing authorization remain mandatory. Bitmot HTTP access controls do not
  apply to raw forwards.
- **Feature mismatch:** disabled providers fail at config/flag parsing with
  the feature needed to build them.
- **Unsupported platforms:** the service trait reports no persistent backend
  rather than pretending a direct child survives a restart. A future backend
  can be added without changing service providers or commands.

## Amendment — 2026-11-14 20:26 EST: feature-disabled default

The feature matrix keeps the current default only when its provider is built:

- with `vpn-tailscale` (the default feature), an omitted VPN remains Tailscale;
- with `--no-default-features`, an omitted VPN is `none`;
- with only `vpn-bitmot`, Bitmot must be selected explicitly by config or flag;
- a named provider whose feature is absent is rejected with the required
  `vpn-*` feature, never silently replaced.

`VpnChoice::None` and the common trait remain unconditional. Provider-specific
variants, constructors, parser branches, and tests are feature-gated. This
keeps `--no-default-features` compilable while preserving today's behavior for
the default build.
