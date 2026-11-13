# libherdr_term

`libherdr_term` attaches flankr's daemon to the local herdr as one of
herdr's own shell clients, over herdr's stable endpoint contract
(generation 1) on `herdr-client.sock`. It hands the daemon the session
pane's cells, cursor, size and mouse mode, and sends the pane keys, text,
paste, mouse and wheel events. herdr is not changed: its protocol types are
compiled from its own source in the `external/herdr` submodule.

The plan is
[libherdr_term, attaching to herdr as a client from herdr's own protocol types](../../docs/design/20261027_08_HERDR_TERMINAL_CRATE_PLAN.md).

## Building

`build.rs` generates herdr's protocol types from the `external/herdr`
submodule (or from `HERDR_SRC`) into `$OUT_DIR/herdr_wire.rs`, which the
crate includes as `libherdr_term::herdr`.
[How herdr's source is used](#how-herdrs-source-is-used) says exactly what
it takes. Without the submodule the build stops with the fix:

```text
git submodule update --init external/herdr
```

The submodule moved to `external/herdr` on 2026-11-13. A checkout made
before then runs `git submodule sync` and
`git submodule update --init external/herdr`, then deletes the old
submodule's directory and its directory under `.git/modules` by hand, as
[docs/HERDR_WIRE.md](../../docs/HERDR_WIRE.md)'s 2026-11-13 section says.

To review the extracted types when the pin moves:

```text
cargo run -p libherdr_term --example dump_herdr_types
```

`libherdr_term::HERDR_PROTOCOL_VERSION` is herdr's `PROTOCOL_VERSION` at
the pin (22 at `2713feb`), `ENDPOINT_PROTOCOL_GENERATION` the endpoint
generation spoken (1).

## Use

```rust,ignore
let socket = libherdr_term::client_socket_for(&api_socket);
let (attachment, events) = Attachment::connect(&AttachOptions::new(socket, 80, 24))?;
attachment.focus_pane("w1:p1")?;
for event in events {
    if let Event::Surface(view) = event {
        if let Some(pane) = view.pane("w1:p1") {
            let grid: libflankr_term::Grid = pane.into();
        }
    }
}
```

`cargo run -p libherdr_term --example attach -- <pane_id> 80 24` prints a
pane's rows and cursor while herdr lays its tab out at 80 by 24; herdr
gives the tab back when it exits.

Tests run against an in-process fake herdr only; nothing in them touches a
real herdr socket.

## How herdr's source is used

herdr's protocol types are not written by hand in this crate. `build.rs`
generates them from herdr's own source on every build, through
[build/extract.rs](build/extract.rs). herdr is never a Cargo dependency, and
the workspace excludes its checkout, so herdr is never built or linked: five
of its files are read, and nothing else.

**Source.** The pinned herdr checkout in `external/herdr`, or the directory in
`HERDR_SRC`.

**Files read.**

- `src/protocol/wire.rs`
- `src/protocol/endpoint.rs`
- `src/input/model.rs`
- `src/api/schema/common.rs`
- `src/config/model.rs`

**Tools.** `syn` (version 2, features `full` and `visit-mut`) parses each
file into a syntax tree; the extractor picks items from that tree and edits
them there. `quote` and `proc-macro2` handle the items as token streams:
matching paths, rebuilding the trimmed `derive` attributes, and scanning
every kept item, macros included, for herdr paths left over. `prettyplease`
prints the final tree as formatted Rust.

**What is kept.** From `wire.rs` and `endpoint.rs`:

- the constants and type aliases;
- the structs and enums that derive `Serialize` or `Deserialize`, and
  `FramingError`;
- the `use` lines that start at `std` or `serde`;
- the seven functions in `KEPT_FNS`: `serialize_graphics_bytes`,
  `deserialize_graphics_bytes`, `deserialize_client_shell_agent_status`,
  `default_true`, `write_message`, `read_message` and `read_exact_or_eof`;
- the four trait impls in `KEPT_IMPLS`: `Clone` for `CellData`, and
  `Display`, `Error` and `From` for `FramingError`.

From the other three files, only the three types in `OUTSIDE` that the wire
types name: `WindowsKeyRecord` from `src/input/model.rs`, `AgentStatus` from
`src/api/schema/common.rs` and `ToastHerdrPosition` from
`src/config/model.rs`.

**What is removed or changed.**

- Everything else in those files: other functions, other `impl` blocks,
  modules (herdr's tests among them), and structs and enums that do not
  derive serde's traits.
- Items, fields and enum variants under a `cfg` that is false for flankr:
  `test` and `windows` are false and `unix` is true. A `cfg` that holds is
  removed; one that names anything else is left for the compiler.
- The `bincode::Decode` and `schemars::JsonSchema` derives.
- herdr's `crate::…` and `super::…` paths, rewritten by a `syn` `VisitMut`
  pass to the names in the one flat module.

Nothing else is edited.

**Checks.** The build stops with one line saying what to fix, and generates
nothing, when herdr's source no longer matches the extractor:

- a file cannot be read or parsed;
- a function in `KEPT_FNS`, an impl in `KEPT_IMPLS` or a type in `OUTSIDE`
  is missing;
- any of the 24 items in `REQUIRED` is missing: `PROTOCOL_VERSION`,
  `MAX_FRAME_SIZE`, `MAX_GRAPHICS_FRAME_SIZE`,
  `ENDPOINT_PROTOCOL_GENERATION`, `ENDPOINT_HELLO_KIND`,
  `ENDPOINT_WELCOME_KIND`, `SNAPSHOT_CODEC_V1`, `SURFACE_CODEC_V1`,
  `INPUT_CODEC_V1`, `BLOB_CODEC_V1`, `ClientMessage`, `ServerMessage`,
  `ClientPaneInputEvent`, `ClientSurfaceSize`, `CellData`, `CursorState`,
  `FrameData`, `PaneSurfaceFrame`, `PaneSurfacePatch`, `PaneSurfacePane`,
  `ClientShellSnapshot`, `EndpointClientHello`, `EndpointServerWelcome` and
  `FramingError`;
- a kept item still names a herdr path that is not extracted;
- `PROTOCOL_VERSION` is not an integer literal.

**Output.** `$OUT_DIR/herdr_wire.rs`, which opens with a header naming
herdr's five files and its licence. `src/lib.rs` includes it as
`libherdr_term::herdr`. Nothing generated is committed;
`cargo run -p libherdr_term --example dump_herdr_types` prints it.

**Distribution.** The compiled crate, and so the flankr daemon, contains
herdr's definitions. Whoever distributes it carries this crate's
[LICENSE](LICENSE) and [NOTICE](NOTICE) with it.

## Licence

libherdr_term is part of flankr and is licensed under the Apache License,
Version 2.0: [LICENSE](LICENSE). The code generated from herdr is herdr's,
Copyright the herdr authors, also under Apache-2.0, as [NOTICE](NOTICE)
records. The rest of the crate is flankr's.
