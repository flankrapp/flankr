---
title: Bitmot raw-TCP relay for a named flankr link endpoint
date: 2026-11-14
status: "Superseded (by [Persistent third-party services, with Bitmot as the first VPN provider](docs/design/20261114_03_PERSISTENT_THIRD_PARTY_SERVICES_PLAN.md))"
covers:
  - "A Bitmot driver selected by `--vpn bitmot` or `[link] vpn = \"bitmot\"` that exposes the existing flankr link's raw TCP port through Bitmot's edge relay"
  - "A required user-supplied Bitmot endpoint/node name, advertised first in pairing offers as the endpoint host and the link's public port"
  - "A supervised `~/.local/bin/bitmot io1 relay --node ... --tcp ...` child whose lifetime follows the flankr daemon"
  - "The current Noise/TCP link and pairing protocol, without a WebSocket bridge or companion transport change"
  - "Not covered: installing or mutating Bitmot's root `io1d` LaunchDaemon, arbitrary HTTP-hostname relays for raw TCP, or a separate public-port mapping"
sources:
  - "Owner (2026-11-14, verbatim): \"plan to implement the new vpn driver for bitmot in ~/.local/bin it will be used to expose access to the flankr daemon via a named endpoint (subdomain) which the user will supply\""
  - "Owner correction (2026-11-14, verbatim): \"what is the websocket bridge for? there should be support for exposing a tcp port via relay for the existing flankr protocol\""
  - "Existing link/VPN design: docs/design/20261102_03_LINK_VPN_PLAN.md"
  - "Current link driver seam: `src/server/link/vpn/mod.rs`, `src/server/link/mod.rs`, and `src/config.rs`"
  - "Current raw link contract: `crates/libflankr_proto/src/link/` and `crates/flankr-companion/src/net.rs`"
  - "Installed Bitmot CLI 1.0.71 (`~/.local/bin/bitmot io1 relay --help`, `edge --help`, and `daemon service --help`, observed 2026-11-14)"
related:
  - docs/design/20261102_03_LINK_VPN_PLAN.md
---

# 1. Problem and boundary

The link already speaks a raw TCP stream carrying the flankr Noise protocol. The
new driver must make that same stream reachable through Bitmot; it must not
translate the protocol into HTTP or WebSocket.

Bitmot 1.0.71 has two different relay modes:

- `bitmot io1 relay --host ... --to ...` claims HTTP hostnames and proxies an
  HTTP service.
- `bitmot io1 relay --node <NODE> --tcp <FORWARD>` serves an edge node's raw
  TCP forwards.

The driver uses the second mode:

```text
bitmot io1 relay --node <NODE> --tcp <PORT>=127.0.0.1:<PORT> --exit-on-stdin-close
```

The normal HTTP hostname mode is not a fallback. An HTTP subdomain cannot carry
the current Noise/TCP link without a protocol adapter, which this plan
explicitly does not add.

The user-supplied named endpoint therefore has to identify a Bitmot raw-TCP
edge endpoint, normally the edge node name whose public host serves the TCP
port. If the requested value is only an HTTP child subdomain such as
`app.node.io1.io`, Bitmot's current raw relay command cannot use it; startup
must refuse it with an actionable error instead of silently selecting a
WebSocket bridge.

# 2. Decisions

## 2.1 Endpoint and configuration

Add a link setting for the Bitmot raw endpoint:

```toml
[link]
vpn = "bitmot"
bitmot_endpoint = "edge-node"
```

`bitmot_endpoint` is required when the selected VPN is Bitmot. It is supplied
for one run with:

```text
flankr serve --foreground --vpn bitmot --bitmot-endpoint edge-node
flankr pair --vpn bitmot --bitmot-endpoint edge-node
```

The flag wins over `[link]`. `pair` passes the same endpoint setting to the
server/link setup as `serve`; a pairing run must never accidentally publish a
different address than the daemon.

The endpoint value is validated as one Bitmot node/host token: non-empty,
ASCII, no scheme, slash, whitespace, query, fragment, or port. A bare Bitmot
node name is passed unchanged to `--node` and is normalized to the public
Bitmot hostname used in `VpnAddress`; a dotted value is used as the supplied
host only when it is valid for Bitmot's raw endpoint mode. The driver does not
claim or reserve an HTTP hostname.

The public TCP port equals the local link port. Thus the existing `Vpn` trait
can keep `forward(port)`, and the command maps `port` to
`127.0.0.1:port`. A different public port is intentionally out of scope until
Bitmot endpoint configuration and pairing offers can represent that mapping
explicitly.

## 2.2 Relay ownership and `io1d`

The flankr daemon owns one foreground relay child for the lifetime of its link.
It uses a piped stdin and `--exit-on-stdin-close`; the child is also configured
to terminate when its process handle is dropped. This gives these invariants:

1. A relay is started only after flankr has bound the loopback link port.
2. A second `forward` call reuses the live child rather than claiming a second
   relay.
3. A Bitmot child that exits is reported as a failed forward, with bounded
   stderr in the error and logs.
4. Stopping flankr closes the relay; a later daemon start claims it again.
5. No shell is used, and no credentials are placed in argv or logs.

Bitmot also supports `bitmot io1 daemon service install`, which installs a root
`io1d` macOS LaunchDaemon and publishes configuration from `/etc/io1d`. This
plan does not install, edit, restart, or remove that service. It is a separate
system-wide persistence model with root ownership and a different lifecycle.
Using it for a per-flankr link would make ordinary flankr configuration mutate
system files and would leave endpoint ownership after flankr stopped. A future
plan may integrate `io1d` if the requirement becomes “survive flankr
restarts”; that is not implicit here.

## 2.3 VPN driver seam

Add `src/server/link/vpn/bitmot.rs` and extend `VpnChoice` with `Bitmot`.
Keep the existing `Vpn` trait as the link-facing abstraction, but make the
Bitmot implementation stateful:

```text
Bitmot {
    program: Option<PathBuf>,
    endpoint: BitmotEndpoint,
    relay: async mutex containing the optional child and last exit,
    command timeout/startup grace,
}
```

The executable search checks `~/.local/bin/bitmot` explicitly because a daemon
started by launchd may not inherit the interactive PATH, then checks PATH
entries. There is no automatic authorization mode. Bitmot uses its cached
credential; a missing credential fails the relay instead of opening an
unattended authorization flow.

Driver behavior:

- `name()` returns `bitmot`.
- `running()` reports whether the selected executable is available to launch;
  it does not claim that a relay exists before `forward()` starts it.
- `address(port)` returns the normalized Bitmot host and `port`.
- `forwards(port)` checks the supervised child for that port and reports false
  after an exit.
- `forward(port)` starts exactly:

  ```text
  <bitmot> io1 relay --node <endpoint> --tcp <port>=127.0.0.1:<port> --exit-on-stdin-close
  ```

  with piped stdin/stdout/stderr, no shell, and a bounded startup/early-exit
  check.
- `forward_command(port)` returns the same safe command for a user to run by
  hand. It never includes a credential value.

The command's stdout and stderr are drained continuously. Output is capped and
redacted before logging. A process that exits before the startup grace period
returns `VpnError` containing the command, exit status, and bounded diagnostic
text without secrets.

## 2.4 Link and pairing behavior

Keep the existing `Link::bind` flow and first-loopback-port rule:

1. Bind the configured listener.
2. Resolve the first `127.0.0.1:<port>` as the Bitmot local target.
3. Verify the Bitmot executable and endpoint.
4. Start or reuse the raw TCP relay.
5. Put the resulting `VpnAddress` first in the pairing offer.

The pairing payload remains a `host:port` address. `libflankr_proto`, the
companion's `TcpStream` connection, Noise framing, and stored `ServerRecord`
addresses do not change. Existing Tailscale, `none`, explicit bind, and
loopback-offer behavior remains unchanged.

The endpoint's TCP exposure is protected by the existing link protocol: a
remote can reach the port, but only a paired key can complete the Noise
handshake. The driver does not pass Bitmot's `--public` HTTP option and does
not broaden the link's application authorization.

## 2.5 Errors and operator guidance

Errors must distinguish these cases:

- no `bitmot` executable at `~/.local/bin/bitmot` or PATH;
- `bitmot` is not authorized or the cached credential is unavailable;
- the endpoint is missing or is an HTTP URL/child hostname rather than a raw
  Bitmot node endpoint;
- the edge node has no matching TCP forward or the requested port is refused;
- the relay exited after starting.

The error includes the safe command that can be run by hand, and says that the
link remains on its explicitly bound local addresses when Bitmot cannot be
started. It must not suggest `--public`, expose credential paths unnecessarily,
or claim that an HTTP hostname can transport raw TCP.

# 3. Implementation steps

## Step 1 — Freeze the Bitmot raw endpoint contract

Confirm the installed Bitmot command and document the accepted endpoint form
with a fake-command test plus the real `--help` output. Define the normalization
from a bare edge node name to the pairing host, and reject an HTTP-only
subdomain before any listener or child process is started.

**Done when:** the endpoint grammar, raw `--node` command, same-port mapping,
and failure message are represented by deterministic tests; no WebSocket or
HTTP adapter is in the design.

## Step 2 — Implement the stateful Bitmot driver

Create `vpn/bitmot.rs`, executable discovery, endpoint validation, raw command
construction, child supervision, stdout/stderr draining, startup/exit
handling, and `VpnError` conversion. Add the `Bitmot` `VpnChoice` variant and
open it with the configured endpoint.

Use test constructors that accept a fake executable and timeout so tests do not
need a Bitmot account. Test exact argv, no shell interpretation, cached-child
reuse, early exit diagnostics, and child cleanup.

**Done when:** the driver passes the `Vpn` contract and a fake relay proves
start, already-forwarded, failed, and shutdown states without touching a real
Bitmot account.

## Step 3 — Add config and command-line plumbing

Add `[link].bitmot_endpoint`, `--bitmot-endpoint`, parser/help/usage text, flag
precedence, and the `serve`/`pair` propagation already used for `--vpn` and
`--bind`. Update comments to distinguish Bitmot raw node endpoints from
Tailscale and `none`.

**Done when:** a configured endpoint and a command-line endpoint produce the
same `LinkSettings`, the flag wins, Bitmot without an endpoint is rejected, and
existing VPN choices retain their current defaults and behavior.

## Step 4 — Integrate the link offer and lifecycle

Pass the configured endpoint into `Link::open`, keep the existing forwarding
state machine, make the normalized Bitmot address the first offer address, and
ensure daemon shutdown drops the child. Preserve the existing local fallback
and diagnostics when no loopback listener exists or the relay fails.

**Done when:** link tests show a Bitmot `host:port` is advertised first after a
successful fake relay, no companion/protocol code changes are needed, and a
relay exit changes the reach state to a useful failure rather than leaving a
stale address advertised.

## Step 5 — Verify against a real raw Bitmot endpoint

After implementation, with an authorized `~/.local/bin/bitmot` and a Bitmot
edge node supplied by the owner:

1. run the foreground daemon with `--vpn bitmot --bitmot-endpoint <node>`;
2. verify the logged command contains `--node` and `--tcp`, not `--host` or a
   WebSocket URL;
3. pair a companion and verify it connects using the existing raw TCP path;
4. stop flankr and verify the relay terminates; restart and verify it can claim
   the endpoint again;
5. verify an invalid HTTP-only subdomain fails with the planned diagnostic.

Run the affected crate format check, clippy, tests, and the actual pairing smoke
scenario. Do not install or alter `io1d` as part of this verification.

**Done when:** a phone reaches the daemon through the owner-supplied Bitmot raw
endpoint using the unchanged flankr protocol, and relay cleanup/restart are
observed.

## Step 6 — Complete user-facing records

Because this adds a daemon-visible VPN choice and configuration key, bump the
root daemon version by the next minor version, append the plain-language entry
to `UPDATES.md`, tag the release `v<version>`, and document the Bitmot example
and raw endpoint limitation. The companion app version does not change because
its transport remains raw TCP and its user-visible protocol is unchanged.

**Done when:** the implementation commit contains the daemon version, update
entry, tag, CLI/config documentation, and no stale `tailscale|none` wording.

# 4. Open questions

1. **What exactly will the owner supply as the endpoint?** The recommended
   input is a Bitmot edge node name (the value accepted by `--node`), not an
   arbitrary HTTP child subdomain. If the required value is instead
   `app.example.com`, Bitmot must provide an L4 mapping for that hostname; the
   current CLI evidence does not show one.
2. **Should the public TCP port differ from the local link port?** The
   recommendation is no for the first implementation: use the existing link
   port on both sides. A separate public port needs an explicit config field
   and a pairing/address decision.
3. **Should endpoint service survive flankr restarts?** The recommendation is
   no: flankr-owned foreground relay, with `io1d` left untouched. Selecting
   persistent `io1d` would require a separate plan for root-owned config,
   installation, status, upgrades, and ownership handoff.

## Amendment — 2026-11-14 20:10 EST: the relay is a persistent user launchd job

The owner asked to extend flankr's existing launchctl-based background-start
concept so a Bitmot relay survives a flankr restart or binary upgrade. The
Bitmot Edge documentation confirms the endpoint model: an edge node owns
stable public IPv4/IPv6 addresses, raw TCP forwards go directly to those
addresses, and the node name is a real DNS label such as
`mynode.io1.io`. Hostnames beneath the node are L7 HTTP hostnames and are not
used for the raw flankr protocol.

This amendment replaces the earlier foreground-child decision in section 2.2,
the stateful-child portion of section 2.3, the relay cleanup assertions in
section 2.4, Step 2's child-supervision work, Step 4's shutdown requirement,
Step 5's cleanup check, and Open question 3:

1. **Persistent job.** On macOS with a user GUI launchd domain, the Bitmot
   driver ensures a separate user-owned launchd job for the relay. It is not
   Bitmot's root `io1d` service and does not require root access. The job
   directly runs the absolute Bitmot executable with:

   ```text
   bitmot io1 relay --node <NODE> --tcp <PORT>=127.0.0.1:<PORT>
   ```

   It omits `--exit-on-stdin-close`, because launchd owns the process and its
   standard input is `/dev/null`. `RunAtLoad` and `KeepAlive` keep the relay
   connected while flankr itself is restarting. No shell, `--public` flag, or
   credential value is used.

2. **Reusable launchd API.** Extend `crates/flankr-daemon` with a persistent
   job API beside, not through, `Detached`. `Detached` remains the one-shot
   bootstrap-and-leave mechanism for `flankr server`. The new API accepts an
   absolute program, argv, working directory, restricted environment, log
   paths, and a stable label, then writes a private mode-0600 plist, bootstraps
   it into `gui/<uid>`, and removes the temporary plist. Its plist includes
   `RunAtLoad`, `KeepAlive`, `StandardInPath=/dev/null`, and separate or
   shared mode-0600 stdout/stderr logs.

3. **Stable identity and reconciliation.** The relay label is deterministic
   from the Bitmot node and public port, for example
   `dev.flankr.bitmot.<hash>`. Desired state is recorded under
   `~/.local/state/flankr/bitmot/` with mode 0700/0600 files: label, node,
   port, executable path, and an argv/environment fingerprint. A matching
   loaded job is reused. A missing, stale, or differently fingerprinted job is
   booted out and bootstrapped with the new arguments. The state never stores a
   credential.

4. **Vpn behavior.** `Bitmot::forwards` checks the deterministic job through
   `launchctl print` and its recorded identity. `forward` reconciles or
   kickstarts the job and returns only after launchd accepts it. `running`
   means that the Bitmot executable and the required launchd domain are
   available. `address(port)` remains the node's public host and `port`.
   The existing `Vpn` trait does not expose a `Child`; the driver owns the
   launchd job instead.

5. **Restart and stop semantics.** An unexpected server exit, a server
   restart, and a flankr binary replacement leave the relay job running. The
   new server instance reconciles and reuses it before publishing pairing
   offers. `flankr serve --restart` preserves the relay while replacing the
   server. An explicit full `flankr serve --stop` boots out the relay for the
   selected link, and selecting `vpn = "none"` or a different Bitmot endpoint
   removes the old deterministic job during reconciliation. This prevents a
   stopped daemon from leaving a public raw port indefinitely while preserving
   the upgrade/restart path.

6. **Other platforms.** Where user launchd is unavailable, the driver keeps a
   direct-child fallback with the previous supervision behavior. It cannot
   promise survival across a process restart there; the limitation is reported
   rather than pretending the job is persistent.

The implementation steps are therefore reordered as follows:

1. Freeze the raw endpoint contract using the Bitmot Edge documentation:
   the input is a raw Bitmot edge-node name/host, its stable node address is
   advertised with the link port, and HTTP child subdomains are rejected for
   this transport.
2. Extend `flankr-daemon` with persistent user launchd job creation,
   reconciliation, status, kickstart, and bootout. Test plist escaping,
   `KeepAlive`, standard input, stable labels, replacement, and the direct
   fallback.
3. Implement the Bitmot driver on that API, including executable discovery at
   `~/.local/bin/bitmot`, exact raw TCP argv, endpoint validation, state
   fingerprints, and safe errors.
4. Add config/CLI plumbing and link integration. Ensure `serve`, `pair`,
   `serve --restart`, `serve --stop`, and endpoint changes use the same
   deterministic relay identity.
5. Verify on macOS with an authorized edge node: change the flankr binary or
   restart the server, observe the relay job remain loaded, reconnect the
   companion through the unchanged raw TCP protocol, then explicitly stop and
   observe the relay job removed.
6. Complete the daemon version, `UPDATES.md`, CLI/config documentation, and
   release tag as already required by this plan.

Open question 3 is answered: **yes, the Bitmot relay persists across flankr
restarts and upgrades through a separate user launchd job**. Bitmot's own
root `io1d` service remains outside this plan.

## Amendment — 2026-11-14 20:23 EST: superseded by the persistent services plan

This narrow Bitmot draft is superseded before implementation by [Persistent
third-party services, with Bitmot as the first VPN provider](docs/design/20261114_03_PERSISTENT_THIRD_PARTY_SERVICES_PLAN.md).
The new plan carries forward the raw Bitmot edge-node relay and persistent
`dev.flankr.vpn.bitmot` job, and adds the platform-neutral user-service trait,
the `flankr service` registry and shortcuts, `vpn-*` feature gates, and the
separate implementation branch. Nothing in this document is implemented by
the superseding plan.
