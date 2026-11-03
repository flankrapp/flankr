---
title: The daemon in the background by default, with a server subcommand
date: 2026-11-02
status: In implementation
covers:
  - "`flankr serve` (and bare `flankr`) starting the daemon in the
    background and returning; `flankr server` as the server process itself,
    in the foreground; `--foreground` to run in the foreground from
    `serve`; `--stop`"
  - "A new crate, `crates/flankr-daemon`, that starts a command detached:
    on macOS as a one-shot job in the user's GUI launchd domain, else (and
    over SSH) in a new session with `setsid`"
  - "The daemon's log file, pid file and the already-running check"
  - "Not covered: starting at login (a LaunchAgent); a Linux systemd unit;
    flankr's licence"
sources:
  - "Owner (2026-11-02, verbatim): \"how does herdr start itself in the
    background?\"; \"can we implement our own crate in this project without
    copying it (due to license)\"; \"it should default to running in the
    background and support a server subcommand like herdr, we can run our
    own copy with --foreground (or --skip-service or something like
    that)\"; asked to write the plan as a draft: \"yes\""
  - "herdr 0.9.5 (vendor/herdr) as prior art for the behaviour only:
    `herdr` starts `herdr server` detached when no server listens, through a
    one-shot launchd job on macOS because a process started from a terminal
    app stays in that app's launchd job and is killed when the app quits
    (herdr issue 4920). herdr is Apache-2.0; none of its code, structure,
    names, comments or plist text is used here (decision 9)"
  - "Apple's man pages: launchctl(1) (`bootstrap`, `bootout`, domains
    `gui/<uid>`), launchd.plist(5) (`Label`, `ProgramArguments`,
    `WorkingDirectory`, `EnvironmentVariables`, `RunAtLoad`,
    `StandardOutPath`, `StandardErrorPath`, `Umask`); setsid(2), fork(2)"
related:
  - "docs/design/20261022_01_ORCHESTRATION_DAEMON_PLAN.md (the daemon and its
    socket)"
  - "docs/design/20261027_02_INFERENCE_CONFIG_PLAN.md (the key file
    `.private/gemini_api_key`, read against the daemon's working folder)"
---

# The daemon in the background by default

## 1. Goal

Today `flankr serve` (and `flankr` with no subcommand) runs the daemon in
the foreground of the terminal that started it: closing that terminal, or
quitting the terminal app on macOS, stops it. herdr instead starts its
server detached and returns. flankr should do the same by default, keep a
foreground form for development and for the test rig, and keep its log.

## 2. Findings (read 2026-11-02)

1. `src/main.rs` dispatches `serve`, `ctl`, `pair`, `session` and `prompt`;
   anything else, and nothing, is `serve`. `serve` takes `--config`,
   `--transport`/`--remote`/`--socket`/`--mock`, `--listen <flankr.sock>`
   and `--show-tokens`.
2. The daemon's own socket defaults to `~/.config/flankr/flankr.sock`
   (`server::default_socket_path`); `--listen` overrides it. Its state lives
   under `~/.local/state/flankr/` (session journals).
3. The daemon reads the Gemini key from `.private/gemini_api_key` against
   its working folder, or `GEMINI_API_KEY`; `dotenvy` loads `.env` from the
   working folder at start. So the working folder matters to a detached
   start.
4. The log goes to stderr through `env_logger` (`RUST_LOG`, default `info`).
5. A process started from a terminal on macOS stays in the terminal app's
   launchd job even after `setsid`, and is killed when the app quits. A
   job submitted to the user's GUI domain (`launchctl bootstrap gui/<uid>
   <plist>`) is outside it. launchd starts a job's process with a fresh
   environment: only what the plist's `EnvironmentVariables` give.

## 3. Decisions

1. **The commands.**
   - **`flankr serve`** and bare **`flankr`**: if a daemon already answers
     on the listen socket, print `flankr is already running on <socket>
     (pid <n>)` and exit 0. Otherwise start `flankr server` detached with
     the same arguments, wait for the listen socket to accept a connection
     (every 100 ms, at most 5 s), then print `flankr is running on
     <socket>; log: <file>` and exit 0. If it does not come up in time,
     print the last 20 lines of its log and exit 1.
   - **`flankr server`**: the server process itself, in the foreground,
     exactly what `serve` runs today. It is what the detached start runs,
     and what a supervisor or a LaunchAgent would run.
   - **`flankr serve --foreground`**: the same as `flankr server`, for
     development and the test rig.
   - **`flankr serve --stop`**: stop the daemon for that listen socket: send
     SIGTERM to the pid in its pid file (decision 4), wait up to 5 s for the
     socket to stop answering, then SIGKILL; print what was done. A stale
     pid file (no such process, or not a flankr) is removed and reported.
   - `--config`, `--transport`/`--remote`/`--socket`/`--mock`, `--listen`
     and `--show-tokens` are accepted by all three and passed through.
2. **The crate `crates/flankr-daemon`.** One function, with no flankr
   knowledge:

   ```rust
   pub struct Detached {
       pub program: PathBuf,          // absolute
       pub args: Vec<OsString>,
       pub working_dir: PathBuf,      // absolute
       pub env: Vec<(OsString, OsString)>, // only these, never secrets
       pub log: PathBuf,              // stdout and stderr, appended
       pub label: String,             // e.g. "dev.flankr.server"
   }
   pub enum Started { Launchd { label: String }, Direct { pid: u32 } }
   pub fn start(d: &Detached) -> io::Result<Started>;
   /// The first thing the started program calls: in a launchd job, forks
   /// the real process into its own session and lets the job exit.
   pub fn leave_job() -> io::Result<()>;
   ```

   - **macOS, with a GUI domain and not over SSH** (`SSH_CONNECTION` unset):
     write a property list with `Label` = `<label>.<unix ms>`,
     `ProgramArguments`, `WorkingDirectory`, `EnvironmentVariables` (the
     given `env` plus a marker variable `FLANKR_DAEMON_LEAVE_JOB=1`),
     `StandardOutPath`/`StandardErrorPath` = `log`, `RunAtLoad` = true, to a
     file mode 0600 in a fresh directory mode 0700 under the user's temp
     folder; run `/bin/launchctl bootstrap gui/<uid> <plist>`; remove the
     file. In the started process, `leave_job` sees the marker, removes it,
     `fork()`s, the child calls `setsid()` and returns, the parent exits at
     once, so the job ends and the server is in no job. Jobs this crate
     submitted more than 60 s ago are removed with `launchctl bootout` on
     the next `start`.
   - **Otherwise** (Linux, other Unix, macOS over SSH or without a GUI
     domain, or `bootstrap` failing): spawn the command with stdin from
     `/dev/null`, stdout and stderr appended to `log`, the given working
     folder and environment, and `setsid()` in a `pre_exec` hook; return
     its pid.
   - Values are written to the plist as XML-escaped strings; a value the
     plist cannot carry exactly (a NUL byte) makes `start` fall back to the
     direct form.
   - Dependencies: `libc` only, beside the standard library.
3. **Environment.** `serve` passes only: `HOME`, `PATH`, `USER`, `LANG`,
   `TMPDIR`, `RUST_LOG`, `HERDR_SOCKET_PATH` and every `XDG_*` that is set.
   **Never** `GEMINI_API_KEY` or any other variable: a launchd plist is a
   file on disk. The server finds its key in `.private/gemini_api_key` and
   its `.env` through the working folder, which is passed (finding 3); a
   key only in the shell's environment does not reach a background
   daemon, and `serve` prints one line saying so when `GEMINI_API_KEY` is
   set and no key file is there (use `--foreground` to keep it).
4. **Files**, under `~/.local/state/flankr/` (0700), named by the listen
   socket: the default socket gives `daemon.log` and `daemon.pid`; any
   other gives `daemon-<8 hex of a hash of the socket's absolute path>.log`
   and `.pid`, so the test rig's daemons each have their own. `flankr
   server` writes its pid file (0600) once its socket is bound and removes
   it on a clean exit. The log is appended; `serve` prints its path.
5. **Already running** means the listen socket accepts a connection and
   answers a `ping` within 1 s; a socket file nobody answers is stale and
   left for the server to replace as it does today.
6. **Logging in the background** uses the same `env_logger` output, which
   now lands in the log file; `RUST_LOG` is passed (decision 3).
7. **The test rig** starts its copies with `flankr serve --foreground`
   (or `flankr server`) as before, each with its own `--config`, `--socket`
   and `--listen`.
8. **Versions.** `flankr serve` changes what it does by default, which the
   person running it notices: the daemon's version takes the next minor
   with its `UPDATES.md` entry saying to use `--foreground` for the old
   behaviour (FLANKR RULE 2). `flankr-daemon` starts at 0.1.0 with its
   `VERSIONS.md` entry; it is added to the workspace in its first commit.
9. **Written from the documentation.** The crate is written from the man
   pages listed in the sources and this plan, without herdr's source open
   while writing; no code, names, comments or plist text are taken from
   it. The behaviour it shares with herdr (detach through a GUI-domain job,
   fork out of the job) is a documented property of macOS, not herdr's
   expression.

## 4. Steps

1. **`crates/flankr-daemon` 0.1.0**: `Detached`, `Started`, `start`,
   `leave_job`, the plist writer and the direct start. **Done when:** unit
   tests cover the plist's keys and escaping, the fallback on a NUL byte,
   the job-label cleanup selection, and the direct start (a test program
   started detached writes to its log and survives its parent); `cargo
   fmt`, `clippy` and `test` pass; macOS's `bootstrap` path is checked by
   hand on the Mac (step 3).
2. **The daemon's commands**: `server`, `serve` in the background,
   `--foreground`, `--stop`, the files of decision 4, the environment of
   decision 3, the usage text, and the version with its `UPDATES.md`
   entry. **Done when:** integration tests (with `--mock`, never the real
   daemon) cover: `serve` starts a server that answers and returns 0;
   `serve` again says it is already running; `--stop` stops it and removes
   the pid file; `--foreground` stays in the foreground; a stale pid file is
   removed; the environment passed contains none but decision 3's names.
3. **On the Mac**: `flankr serve` from a terminal, then quitting the
   terminal app, leaves the daemon running and answering; `flankr serve
   --stop` stops it; the test rig still runs its copies with
   `--foreground`. **Done when:** that is recorded in `CHANGES.md` with the
   log lines.

## 5. Risks

- A key kept only in the shell environment no longer reaches the daemon
  started by `serve` (decision 3); the line `serve` prints, and
  `--foreground`, cover it.
- launchd's `bootstrap` syntax and errors differ across macOS versions;
  the direct fallback keeps `serve` working, only without surviving the
  terminal app's quit.

## 6. Open questions for the owner

1. **The flag's name:** `--foreground` (recommended), or `--skip-service`.
2. **Bare `flankr`:** keep it as `serve` in the background (recommended,
   as today it is `serve`), or make it print the usage.

## Amendment — 2026-11-02 21:58 EST: the open questions answered

The owner (2026-11-02) agreed to the recommended answers to section 6:
"yes, those are fine".

1. **The flag** is `--foreground`; there is no `--skip-service`.
2. **Bare `flankr`** stays `serve`, now in the background.

Section 6 has no open questions left; the rest of the plan is unchanged.
The status stays `Draft` until the owner approves it.

## Amendment — 2026-11-02 22:07 EST: approved

The owner approved the plan as it stands at `fa8ec0b` (2026-11-02:
"Approve the background daemon plan and build it?" "yes"). It is built
now; the first implementing commit sets `In implementation`.

## Amendment — 2026-11-02 22:12 EST: implementation started

Step 1 is implemented: `crates/flankr-daemon` 0.1.0, which this commit
adds to the workspace. The status is `In implementation`, and this
document is now immutable. It changes only on the owner's explicit
instruction (AGENTS.md, Rule 1.5).

## Amendment — 2026-11-02 22:26 EST: `flankr pair` starts the daemon when it is not running

On the owner's instruction (Rule 1.5), 2026-11-02: "flankr pair should
also launch flankr if it's [not] running (and --foreground isn't used)".
Adds to decision 1:

1. **`flankr pair`** first checks the listen socket as `serve` does
   (decision 5). If no daemon answers, it starts one in the background
   exactly as `flankr serve` would, prints the same `flankr is running on
   <socket>; log: <file>` line, then pairs. If the start fails, `pair`
   prints `serve`'s failure lines and exits with its code, without
   pairing.
2. **Only the server's flags** are passed to the started server:
   `--config`, `--listen`, `--transport`/`--remote`/`--socket`/`--mock`,
   `--show-tokens`, `--prompts`, `--suggestions`. Pairing's own flags
   (`--name`, `--protocols`, `--replace`, `--code`, `--qr-png`,
   `--timeout`) are not.
3. **`flankr pair --foreground`** never starts a daemon: with none
   running it fails as `pair` does today.
4. **The daemon** takes the next patch with its `UPDATES.md` entry
   (FLANKR RULE 2).

## Amendment — 2026-11-02 22:33 EST: `--version`, `version`, and unknown arguments are errors

On the owner's instruction (Rule 1.5), 2026-11-02: "neither --version nor
version seem to be supported, which should we add?", then, offered both
and making unknown flags an error, "go ahead". `flankr --version` on the
MacBook had started a daemon: an unknown flag was only warned about, and
bare `flankr` is `serve`. Adds to decision 1:

1. **`flankr --version`, `flankr -V` and `flankr version`** print one
   line, `flankr <version> (<short commit>)`, the commit the binary was
   built from (`unknown` when built outside a git checkout), and exit 0.
   Nothing is started.
2. **An unknown argument is an error** for every command: `flankr: unknown
   argument '<arg>'; see flankr --help` on stderr, exit 2, nothing
   started. Bare `flankr` with no arguments is still `serve`.
3. **The daemon** takes the next patch with its `UPDATES.md` entry
   (FLANKR RULE 2).
