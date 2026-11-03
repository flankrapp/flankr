---
title: The link's reach through a VPN (Tailscale by default) or an explicit address
date: 2026-11-02
status: Draft
covers:
  - "A `Vpn` trait the daemon reaches its link through, with `Tailscale` as
    the first implementation and the default, and no VPN as the other"
  - "The daemon adding Tailscale's forward to the link itself, when
    Tailscale is up and the forward is missing"
  - "`--vpn tailscale|none`, `[link] vpn`, and with no VPN, a local port or
    an explicit bind address through `--bind` or `[link] listen`"
  - "The addresses a pairing offer carries in each case"
  - "Not covered: VPNs other than Tailscale (the trait leaves room);
    removing a forward"
sources:
  - "Owner (2026-11-02, verbatim): \"we should be able to start the
    tailscale commands, also it should support --vpn as an option with
    tailscale as a default (using a trait)\"; then, asked to write the plan
    as a draft: \"yes, but it should also support no vpn just using a local
    port or explicit binding address\""
  - "The MacBook, 2026-11-02: its daemon listened only on 127.0.0.1:7347 and
    logged \"Tailscale is running, but does not forward port 7347 to the
    link; to reach it from the tailnet, run: tailscale serve --bg --tcp 7347
    tcp://127.0.0.1:7347\"; the phone scanned an offer naming 127.0.0.1 and
    could not pair"
related:
  - "docs/design/20261022_02_LINK_AND_COMPANION_PLAN.md (the encrypted link; its 11:58 EDT
    amendment, \"Listening\": flankr never changes Tailscale's
    configuration; this plan replaces that rule)"
  - "docs/design/20261102_01_BACKGROUND_DAEMON_PLAN.md (the flags `serve`,
    `server` and `pair` share)"
---

# The link's reach through a VPN or an explicit address

## 1. Goal

The link listens on 127.0.0.1:7347 by default and leaves reaching it from
the phone to the owner: on a machine where Tailscale is up but not
forwarding, the daemon only logs the command to run, and a pairing offer
names 127.0.0.1, which a phone cannot reach. The daemon should set up its
own reach: through Tailscale by default, behind a trait so other VPNs can
follow, or with no VPN on a local port or an explicit address.

## 2. Findings (read 2026-11-02)

1. `src/server/link/tailscale.rs` reads `tailscale status --json` and
   `tailscale serve status --json`; when Tailscale runs without the
   forward it logs the command. Its rule: flankr never changes Tailscale's
   configuration.
2. `src/server/link/mod.rs`: with no `[link] listen`, the link binds
   127.0.0.1:7347 and a pairing offer carries the addresses `tailscale::
   detect()` gives; with an explicit `listen`, it binds those and the offer
   carries them as bound. `listen = []` turns the link off.
3. `serve`, `server` and `pair` share one flag parser (plan 20261102_01);
   `pair` passes the server's flags to a daemon it starts.

## 3. Decisions

1. **The trait**, in `src/server/link/vpn/`:

   ```rust
   pub struct VpnAddress { pub host: String, pub port: u16 } // name or IP
   pub trait Vpn: Send + Sync {
       fn name(&self) -> &'static str;
       /// Whether the VPN is installed and up on this machine.
       async fn running(&self) -> Result<bool, VpnError>;
       /// This machine's address on the VPN for `port`, once forwarded.
       async fn address(&self, port: u16) -> Result<Option<VpnAddress>, VpnError>;
       /// Whether the VPN already sends `port` to 127.0.0.1:`port`.
       async fn forwards(&self, port: u16) -> Result<bool, VpnError>;
       /// Make it do so.
       async fn forward(&self, port: u16) -> Result<(), VpnError>;
   }
   ```

   `VpnError` carries the command and its stderr, never secrets.
2. **`Tailscale`**, the first implementation, through the `tailscale` CLI
   (found on `PATH`, else `/Applications/Tailscale.app/Contents/MacOS/
   Tailscale`): `running` and the machine's name from `tailscale status
   --json` (`BackendState == "Running"`, `Self.DNSName` without its final
   dot); `forwards` from `tailscale serve status --json` (a TCP handler for
   the port to `127.0.0.1:<port>`); `forward` runs `tailscale serve --bg
   --tcp <port> tcp://127.0.0.1:<port>`, each with a 10 s timeout.
   `src/server/link/tailscale.rs`'s reading moves into it.
3. **Choosing**, the first that says wins: `--vpn tailscale|none`, then
   `[link] vpn = "tailscale" | "none"`, then the default `tailscale`. An
   unknown name is an error at start. `serve`, `server` and `pair` take
   `--vpn` and pass it on as they pass the server's other flags.
4. **With a VPN** (Tailscale), at start, the link binds 127.0.0.1:<port>
   (7347 unless `--bind`/`listen` give a loopback port), then:
   - VPN not installed or not running: no forward, logged once (`[link]
     tailscale: not running; the link is on 127.0.0.1:7347 only`), and the
     offer carries the loopback address with a warning, as today;
   - running and forwarding: logged (`[link] tailscale: <name>:7347 already
     forwards to the link`);
   - running and not forwarding: the daemon runs `forward` and logs
     `[link] tailscale: forwarding <name>:7347 to the link`; on failure it
     logs the error and the command to run by hand, and goes on.
   The offer carries the VPN address first, then the loopback one.
5. **With no VPN** (`--vpn none`): nothing is run or read for a VPN. The
   link binds what is given, else the default 127.0.0.1:7347:
   - **`--bind <addr:port>`** (repeatable) replaces `[link] listen` for that
     run; `addr` is an IP (v4 or v6) or `0.0.0.0`/`::` for every address;
   - the offer carries each bound address; for `0.0.0.0` or `::` it carries
     the machine's non-loopback interface addresses instead (each
     `addr:port`), so a phone on the same network can reach it;
   - bound only on loopback, the offer carries it with the warning that a
     phone cannot reach it.
6. **An explicit `listen`/`--bind` with a VPN** is kept: the link binds
   those, and the VPN's forward targets the first loopback one among them,
   if any; with none on loopback no forward is made and that is logged.
7. **Forwards are left in place** when the daemon stops: `tailscale serve
   --bg` persists, and the next start finds it (decision 4).
8. **This replaces** the link plan's rule that flankr never changes
   Tailscale's configuration: it adds the one forward to its own port, and
   nothing else. `--vpn none` is the way to keep it from touching
   Tailscale at all.
9. **Tests never run the real VPN**: the link takes a `dyn Vpn`; tests use
   a fake that records calls and answers as told. `Tailscale`'s parsing is
   tested on captured JSON (written by hand in the tests, not taken from a
   real machine). The real forward is checked by hand on the MacBook.
10. **Versions.** The daemon now changes Tailscale's forwards and can bind
    other addresses, which the person running it notices: the next minor,
    with its `UPDATES.md` entry naming `--vpn none` and `--bind`.

## 4. Steps

1. **The trait and `Tailscale`**, with the reading moved from
   `tailscale.rs`. **Done when:** unit tests cover parsing a running and a
   stopped `status`, a `serve status` with and without the TCP handler, the
   command `forward` would run, and timeouts as errors.
2. **The link on it**: decisions 3 to 7, the flags, `[link] vpn`,
   `--bind`, the offer's addresses, the logs, usage and config docs, the
   version and its `UPDATES.md` entry. **Done when:** tests with the fake
   VPN cover each case of decisions 4 to 6 (forward made once, not when
   already there, not with `none`; `--bind 0.0.0.0:7347` giving interface
   addresses in the offer; loopback-only warning), and `pair` passing
   `--vpn` and `--bind` on.
3. **On the MacBook**: `flankr serve` adds the forward (`tailscale serve
   status` shows it), `flankr pair`'s offer names the MacBook's tailnet
   name, and the S23 pairs with it. **Done when:** recorded in
   `CHANGES.md`.

## 5. Risks

- `tailscale serve` may need the user to allow serving on a tailnet that
  has it off (an admin setting); `forward` then fails, the error and the
  command are logged, and the link stays local.
- A binding to `0.0.0.0` exposes the link on every network the machine
  joins; the link's own encryption and pairing still guard it, and the
  start log names each bound address.

## Amendment — 2026-11-02 23:51 EST: approved

The owner approved the plan as it stands at `6e69460` (2026-11-02: "yes,
approve and build it"). It is built now; the first implementing commit sets
`In implementation`.

## Amendment — 2026-11-02 23:53 EST: no QR code for an offer only this machine can reach; the hostname with no VPN

On the owner's instruction (2026-11-02): "it doesn't make much sense to
serve the QR code for localhost, it should report that the only connect is
local or use the machine hostname if no vpn". This replaces decision 4's
"the offer carries the loopback address with a warning, as today" and
decision 5's last item, and adds to decision 5:

1. **Loopback only, no offer.** When every address a pairing offer would
   carry is a loopback one (127.0.0.0/8, ::1), `flankr pair` shows no QR
   code and no pairing URL. It prints `flankr is only reachable from this
   machine (127.0.0.1:7347): a phone can't connect to it.` and one line on
   what would make it reachable (`Tailscale is not running; start it, or run
   flankr with --vpn none --bind 0.0.0.0:7347 to use this network`, or, with
   Tailscale running but its forward failing, that error and the command to
   run by hand), and exits 1. The device-code path is offered only once a
   reachable address exists.
2. **The hostname with no VPN.** With `--vpn none` and a non-loopback
   binding (`0.0.0.0`, `::` or a specific address), the offer carries the
   machine's network name first: on macOS its Bonjour name
   (`<LocalHostName>.local`, from `scutil --get LocalHostName`), elsewhere
   `hostname`; then the bound address, or for `0.0.0.0`/`::` the
   interface addresses, as fallbacks. The phone tries them in order.
3. **The daemon's log** says the same at start when the link is reachable
   only from the machine itself, so `flankr serve`'s log shows it before any
   `pair`.
