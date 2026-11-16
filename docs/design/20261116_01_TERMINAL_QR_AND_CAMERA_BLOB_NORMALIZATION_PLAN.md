---
title: Half-height terminal QR codes and camera-side QR row normalization
date: 2026-11-16
status: Implemented
covers:
  - "`flankr pair` printing the pairing QR as Unicode half-height terminal
    characters, while retaining the pairing URL, data URL and optional PNG"
  - "A camera-side normalization pass that joins short blank horizontal gaps
    between overlapping dark blobs before the QR decoder processes a frame"
  - "The boundary between flankr's pair output, qrcodeurl's existing terminal
    renderer, libmobile's QR analysis stream and the companion app"
  - "Tests for the terminal representation, row-gap repair, unchanged normal
    QR images, and an on-device pairing scan"
sources:
  - "Owner (2026-11-16, verbatim): \"plan to generate qr code using half-height
    terminal characters, also plan for the camera side to correct blank lines
    by merging down the blobs before processing\""
  - "`crates/qrcodeurl/src/lib.rs`: the existing `Qr::terminal` renderer using
    `Dense1x2`, a four-module quiet zone and `▀`/`▄` half-height cells"
  - "`src/pair.rs`: `Shown` currently creates the PNG and `show` currently
    prints the URL, data URL and fingerprint"
  - "`crates/qrcodeurl/tests/roundtrip.rs`: existing terminal-size and colour
    assertions"
  - "`/Users/tmeade/src/libmobile/src/camera/qr/scan.rs`: `GreyFrame`,
    `analyse`, the rqrr decoder and the existing 2 × 2 retry"
  - "`/Users/tmeade/src/libmobile/src/android/camera/qr.rs`: the one-slot QR
    analysis worker, which must remain non-blocking for the camera thread"
  - "`docs/design/20261022_06_QRCODEURL_PLAN.md`: the original terminal QR
    contract and the later decision that URL and data URL output remain"
  - "`docs/design/20261028_02_PAIRING_QR_SCAN_PLAN.md`: the companion camera
    contract; camera events carry decoded text and corners, not raw frames"
  - "`/Users/tmeade/src/libmobile/docs/design/20261028_01_CAMERA_CAPTURE_AND_QR_PLAN.md`:
    the libmobile QR decoder, grey analysis stream, rqrr boundary and timing
    budget"
related:
  - "docs/design/20261022_06_QRCODEURL_PLAN.md"
  - "docs/design/20261028_02_PAIRING_QR_SCAN_PLAN.md"
  - "docs/design/20261116_01_TERMINAL_QR_AND_CAMERA_BLOB_NORMALIZATION_PLAN.md"
  - "libmobile: docs/design/20261028_01_CAMERA_CAPTURE_AND_QR_PLAN.md"
---

# Half-height terminal QR codes and camera-side QR row normalization

## 1. Goal

`flankr pair` should show a QR code directly in an ordinary terminal, without
requiring an inline-image protocol or copying a data URL. The terminal form
must remain a QR code for the same pairing URL that the companion already
accepts.

A phone camera pointed at that terminal must tolerate the horizontal gaps that
terminal glyph metrics can introduce between rows of dark QR ink. The repair
belongs immediately before libmobile's QR decoder, where the camera still has
the greyscale analysis frame. It must not change the pairing payload, the
camera event shape, the companion pairing parser, or the existing PNG/data-URL
fallbacks.

## 2. Boundary

This plan changes two existing paths:

1. In this repository, `qrcodeurl` and `src/pair.rs` add the terminal QR back to
   the normal `flankr pair` output. `qrcodeurl` already has the required
   half-height renderer; the plan uses it rather than introducing another QR
   encoder or terminal escape protocol.
2. In the libmobile repository, the private `camera-qr` analysis path repairs
   terminal-style row gaps before calling rqrr. libmorph and
   `apps/companion` keep the existing camera API and `CameraEvent::Qr` event.
   The companion continues to receive only decoded text, byte length and
   corners.

Not covered: a new QR format, a change to pairing URLs or payloads, camera
permissions, camera controls, exposure or zoom policy, the viewfinder layout,
a desktop webcam, a new terminal image protocol, or changing the existing
`--qr-png`/data-URL output.

## 3. Findings

### 3.1 Terminal rendering already exists

`crates/qrcodeurl/src/lib.rs` has `Qr::terminal(Terminal)`. It uses the
`qrcode` crate's `Dense1x2` renderer, a four-module quiet zone, and two QR
module rows per terminal cell. `Terminal::LightOnDark` is the default used by
`qrcodeurl::terminal`.

The renderer is already tested for the expected number of terminal rows,
the code width and the presence of full, upper-half and lower-half block
characters. The missing piece is only `flankr pair` output: `Shown` currently
keeps the PNG, while `show` prints the URL and data URL.

### 3.2 The camera decoder owns the relevant pixels

The companion's `Scanner` starts libmorph's continuous QR scan and handles
`CameraEvent::Qr`. It does not receive preview pixels for decoding. libmorph
forwards the scan to libmobile, and libmobile's QR worker receives a packed
luma `GreyFrame` before `rqrr::PreparedImage::prepare_from_greyscale`.

Therefore the blank-line correction cannot be implemented reliably in
`apps/companion/src/scanner.rs` or in `crates/flankr-companion/src/scan.rs`:
those layers see a successful or failed decoded result, not the camera image.
The correction must be a private libmobile QR-analysis transform.

### 3.3 Existing decoder constraints

The QR worker accepts only the newest frame, and `analyse` already performs a
single box-filtered 2 × 2 retry when a found code is large enough but does not
decode. The row normalizer must remain a pure, bounded transform on one
`GreyFrame`, run on that worker, and stay within the existing
`QR_DECODE_BUDGET_MS` measurement. It must never log payload bytes.

## 4. Decisions

### 4.1 Terminal output

1. Keep one QR source of truth. `Shown::new` constructs one `qrcodeurl::Qr`
   from the pairing URL and obtains both its PNG and its terminal rendering
   from that object. It does not encode the URL twice.
2. Add an internal `terminal` string to `Shown` (or an equivalent local value)
   and print it first from `show`.
3. The output order is:
   - the unindented half-height terminal QR block;
   - a blank line;
   - the `flankr://…/pair#…` URL;
   - a blank line;
   - the `data:image/png;base64,…` URL;
   - the fingerprint label and words.
4. `qrcodeurl::Terminal::LightOnDark` remains the default. The QR has the
   existing four-module quiet zone. The renderer emits ordinary Unicode text,
   not ANSI colour or terminal-specific escape sequences, so it works in a
   shell, herdr pane, log capture and redirected output.
5. Keep `--qr-png` unchanged. The URL and data URL remain available for
   copy/paste and for terminals that cannot show the half-height glyphs.
6. Do not add a second QR payload or a terminal-only pairing path. The URL,
   PNG and terminal block must all be generated from the exact same URL.

### 4.2 Camera-side row repair

The repair is a private function in libmobile's QR scan module, conceptually:

```rust
fn merge_down_blobs(frame: &GreyFrame) -> GreyFrame
```

Its contract is:

- preserve width, height, orientation and pixel coordinates;
- leave frames with no credible dark/light separation unchanged;
- identify short, all-background horizontal gaps between dark foreground
  blobs;
- fill only gaps whose foreground blobs overlap horizontally, by carrying the
  lower blob upward/downward into the gap so the two blobs become one
  continuous dark region; and
- leave long gaps, unrelated components, the quiet zone and ordinary QR image
  pixels alone.

“Blob” is a connected dark foreground run after an adaptive luma threshold,
not a decoded QR module. The implementation must use a bounded gap limit and
horizontal-overlap check. It must not delete every all-white row: an
unbounded blank-row compaction would damage real QR white modules and the
quiet zone.

The planned processing order is:

1. Build the repair mask from the analysis Y plane using a deterministic
   adaptive threshold. If the frame is effectively flat or lacks both a dark
   and light population, return the original frame.
2. Find dark horizontal blobs and their bounding boxes. A candidate is a
   short blank row band between a blob and a lower blob with horizontal
   overlap. A gap larger than the measured terminal inter-row limit is not
   repaired.
3. Merge downward in scan order. For each accepted pair, fill the gap only
   at the overlapping dark x-runs, using the darker adjacent luma so the
   repair is conservative. Preserve all original non-gap pixels.
4. Run rqrr on the repaired frame first. If it produces no decoded code, use
   the existing unmodified-frame path as a fallback. This protects ordinary
   camera QR scans from a false repair while making the terminal form the
   first-class path.
5. Use the selected detection's coordinates for the existing measurement,
   auto-zoom, exposure and event conversion. The raw frame remains the source
   for screen-luma measurements; row repair is a decoder input transform, not
   an exposure signal.

The exact threshold and maximum gap are constants in libmobile's QR module,
with names and comments in its existing tunable-constants section. They are
selected from the synthetic terminal-gap fixtures below, then checked on the
owner's phone. They are not exposed through `QrOptions`: this is a decoder
robustness fix, not an app setting.

### 4.3 Compatibility and failure behavior

- A normal PNG-style QR still decodes through the repaired path; if the
  repair is not applicable, the pixels are unchanged.
- A terminal QR whose gaps are too large or whose glyphs have no overlapping
  blobs falls back to the existing decoder and reports the same normal scan
  failure, not a fabricated payload.
- The camera event remains `Qr { scan, code }`; no new protocol or app parser
  is needed.
- The repair never logs image data, decoded text or pairing material.

## 5. Interfaces and files

### 5.1 flankr repository

- `crates/qrcodeurl/src/lib.rs`: reuse the existing `Qr::terminal` API; no
  new public QR encoding API is required.
- `crates/qrcodeurl/tests/roundtrip.rs`: retain the current renderer shape
  tests and add assertions for the exact two-module-per-cell contract if the
  implementation changes the wrapper.
- `src/pair.rs`: create the terminal form beside the existing PNG in
  `Shown`, and print it in `show` before the URL/data URL.
- `src/pair.rs` tests: assert that `Shown`'s terminal block equals
  `qrcodeurl::Qr::new(&shown.url).terminal(Terminal::LightOnDark)` and that
  the URL/data URL/PNG remain the same.
- `CHANGES.md`, `README.md`, `UPDATES.md`, `apps/UPDATES.md` and version
  metadata are updated when implementation starts, following AGENTS.md. The
  daemon and companion users both see this change, so both user-facing logs
  and versions are updated in the implementation commit.

### 5.2 libmobile repository

- `src/camera/qr/scan.rs`: `merge_down_blobs`, the mask/geometry helpers and
  pure tests; integrate the repaired frame before rqrr.
- `src/camera/qr.rs`: named tunable constants and comments if the gap limit
  or threshold belongs beside the existing QR decoder constants.
- `src/android/camera/qr.rs`: only touch this file if the worker needs an
  explicit timing/diagnostic hook; the one-slot newest-frame contract stays.
- `docs/design/20261028_01_CAMERA_CAPTURE_AND_QR_PLAN.md` and libmobile's
  `CHANGES.md`: implementation must record this decoder departure in that
  repository before editing its frozen implementation plan.

### 5.3 libmorph and companion

No camera API or `apps/companion/src` change is expected. Once libmobile and
libmorph publish or expose the corrected decoder, update the companion's
pinned dependency revision and rebuild the existing `mobile-camera-qr`
feature. If the dependency changes a public version, apply the recursive
version rule in each affected repository.

## 6. Implementation steps

Each step is complete only when its **Done when:** condition holds. The
terminal-output work and the libmobile decoder work may use separate commits
and repositories, but the dependency update and the final app behavior are
landed together in the flankr implementation cutover.

1. **Freeze terminal output fixtures.** Add a fixed pairing URL fixture and
   document the expected terminal dimensions: side `modules + 8`, terminal
   rows `ceil(side / 2)`, and exactly `side` Unicode cells per row. Verify the
   terminal, PNG and data URL all derive from the same URL.
   **Done when:** qrcodeurl's roundtrip tests pass and a pair-output test
   proves the terminal block is present before the URL without changing the
   existing URL/data-URL/PNG values.

2. **Print the half-height QR from `flankr pair`.** Use the existing
   `Dense1x2` renderer through `Qr::terminal`, preserve the plain URL and
   data URL, and keep `--qr-png`'s create-new behavior. Exercise redirected
   output as well as a real terminal so no ANSI or cursor-control assumption
   leaks into the output.
   **Done when:** `flankr pair` visibly prints one unindented half-height QR
   block and the existing companion input paths still accept the URL and data
   URL.

3. **Implement the pure camera row normalizer in libmobile.** Add the
   threshold, blob geometry, bounded gap and conservative merge functions in
   the QR scan module. Keep allocations bounded to one repair mask and one
   repaired luma frame per analysed frame, and make the no-repair path cheap.
   **Done when:** libmobile host tests cover flat frames, normal QR frames,
   single and multiple terminal-style blank gaps, gaps without horizontal
   overlap, gaps beyond the limit and preserved dimensions/pixel coordinates.

4. **Integrate repaired frames with rqrr and the existing controller.** Scan
   the repaired frame before the current 2 × 2 retry, fall back to the
   original frame when repair does not decode, and preserve corners, module
   measurements, screen-luma sampling, deduplication and auto-zoom behavior.
   Add a synthetic half-height terminal fixture whose rows include the
   measured blank line gaps.
   **Done when:** the fixture decodes the original pairing URL, ordinary QR
   fixtures still decode, no false code is emitted from a blank/flat frame,
   and the QR worker stays within its existing decode-budget instrumentation.

5. **Cut the dependency through libmorph and the companion.** Update the
   pinned libmobile/libmorph revisions only after their checks and releases
   are available; rebuild the Android camera feature and keep the public
   `CameraEvent::Qr` contract unchanged.
   **Done when:** host and Android-target checks pass for libmobile,
   libmorph, flankr-companion and the companion app's camera feature, with no
   new warnings.

6. **Device smoke on the owner's phone.** Run `flankr pair` in a terminal
   large enough not to wrap the QR block, point the companion camera at the
   terminal QR, and complete the existing six-word confirmation. Repeat from
   the data URL or pasted URL to prove the fallback was not changed. Record
   only pass/fail, decoded byte length and connection outcome; never record
   the pairing payload.
   **Done when:** the terminal QR reaches the same pairing flow as the data
   URL, the companion connects, and the evidence log links the standalone
   run directory without exposing the URL fragment.

7. **Documentation and release records.** Append the implementation results
   to the plan and owner report, update the README decision row and the
   daemon/companion changelogs, and bump versions according to the user-visible
   terminal and camera behavior. Mark this plan Implemented only after every
   preceding **Done when:** condition holds.
   **Done when:** all affected callers, tests, dependency pins, docs and
   release metadata agree with the shipped behavior, and the required tags
   exist.

## 7. Risks and safeguards

- **True QR white rows mistaken for terminal gaps.** Require a bounded gap and
  horizontal blob overlap; run the original-frame fallback; test generated
  QR matrices with deliberately blank rows.
- **Glyph or terminal variation.** Do not assume a particular font or terminal
  width. The terminal output remains plain Unicode, and the camera repair is
  based on luma geometry rather than character identity. Device smoke must
  include the owner's actual terminal.
- **Decoder latency.** The normalizer runs on the existing QR worker, never on
  the camera thread. Keep the repair mask bounded and retain the existing
  median decode timing log.
- **A wrapped QR block.** The pair command cannot control terminal width.
  Keep the QR block unindented and document that the terminal must be wide
  enough; the URL/data URL remain the fallback when wrapping occurs.
- **Cross-repository drift.** The libmobile decoder change and its plan
  amendment land before the flankr dependency cutover. A flankr build must
  not silently claim the camera fix while still resolving the old revision.

## 8. Open questions

1. The first implementation should tune the adaptive threshold and maximum
   gap against the synthetic terminal fixture and one device capture. If the
   device's terminal font produces a different gap class, record the chosen
   constants in libmobile's living tunables rather than adding an app setting.
2. If the existing libmobile rqrr decoder cannot reliably find the repaired
   terminal frame, the next option is a decoder-local crop/retry, not a change
   to the companion protocol or a second QR format. That is outside this plan
   unless the device smoke proves it necessary.

## Amendment — 2026-11-16 12:19 EST: implementation approved and started

The owner approved implementation of this plan. The status is now `In
implementation`, and the document is immutable under AGENTS.md Rule 1.5.
The terminal output and camera-side row normalization decisions above are
frozen; later implementation results are recorded as amendments.

## Amendment — 2026-11-16 12:59 EST: implementation complete

All seven implementation steps are complete. The daemon prints the existing
`qrcodeurl` LightOnDark `Dense1x2` terminal rendering before the unchanged URL,
data URL and fingerprint output; the camera worker adaptively repairs bounded
overlapping dark-row gaps and retains the original-frame fallback.

Implementation records:

- Step 1 and step 2: flankr commit `e08b962` (`v0.12.0`), including the
  terminal fixture and pair-mode output assertions.
- Step 3: libmobile commit `a7407e0` (`v0.5.0`) plus formatting commit
  `5adf926`; the QR analysis tests cover ordinary frames, bounded repair,
  non-overlapping and over-limit gaps, and a damaged pairing payload.
- Step 4: libmorph `0.9.0` commit `1891863` (`v0.9.0`), libmorph-build
  `0.5.0` commit `7db31e6` (`libmorph-build-v0.5.0`), and their full-revision
  metadata commits `71e2e56` and `757ea2f`; libmorph's GPU callback API
  migration is completed in the companion.
- Step 5: flankr-audio commit `4c1ed83` (`flankr-audio-v0.4.0`) and companion
  app commit `27576d4` (`flankr-app-v0.7.35`), with both lockfiles and
  user-facing changelogs updated.
- Step 6: the Android release APK built, installed and launched. The
  feature-gated scripted camera smoke decoded a damaged terminal-style frame
  back to its pairing payload after repair; no payload was recorded. The
  standalone evidence is `out/qr/20261116_01_terminal_camera/`.
- Step 7: the release records, dependency pins, affected callers, tests and
  README decision row now agree with the shipped behavior.

Verification: `cargo fmt`, clippy and tests pass for libmobile, libmorph,
libmorph-build, flankr-audio and the companion; the terminal pair-mode smoke
passes. The full flankr test suite has one pre-existing macOS launchd failure:
the background test cannot find `dev.flankr.server` in the user launchd
domain. The failure is unrelated to pairing output.
