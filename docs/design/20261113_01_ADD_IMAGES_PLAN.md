---
title: Images through the message card's +, and the + at the left of the input row
date: 2026-11-13
status: In implementation
covers:
  - "The message card's input row shows a **+** at its far left on a phone,
    beside the microphone and keyboard glyphs it already has at its right;
    a tap opens the add menu of **Paste text** and **Attach image…**"
  - "**Attach image…** picks up to four images with libmorph's media
    request (the gallery, files and a camera entry), uploads them through
    the link's `images` namespace as the binary files plan built it, and
    shows them in an attachment grid above the text with `[image N]` tokens
    at the cursor"
  - "The terminal keys row's **+** gains the same **Attach image…**, through
    `terminal.paste`"
  - "What the daemon already does with the images (`agents.send` `parts`,
    the paths it composes, the confirmation), restated, not changed"
  - "Not covered: images on Gemini's card (the general agent takes none:
    the binary files plan, its 21:29 EDT amendment, item 9); images from
    the clipboard (libmorph has no clipboard-image call; open question 3);
    Share → flankr from other apps and the preview's share icon (they need
    libmobile's share plan); converting HEIC and baking rotation, and the
    *Photos* setting (the binary files plan's 21:29 EDT amendment, items
    6-8, which wait for libmorph's `decode_image`/`encode_image`); a
    *Take photo* through `ctx.camera()` (libmorph's camera capture is a
    Draft there); the desktop, where libmorph's media request answers
    `Unsupported`"
sources:
  - "Owner (2026-11-13, verbatim): \"plan to aypport the add button for
    imagee not just text, also add a + glyph in the input card which alread
    has voice and keyboard glyph, the + glyph shpuld be left aligned\""
  - "flankr at `79065d8` (branch `main-terminal`), read 2026-11-13:
    `apps/companion/src/prompt/input_row.rs` (`InputTools::attach`: the
    message card's + only with `mobile && images`; `InputRow::lay_out`: the
    + at `x`, the microphone and the toggle at the right),
    `apps/companion/src/lib.rs` (`input_tools()` with `images: false`;
    `Choice::AttachImage` and the message card's `PromptAttach` tap both
    log *not built yet*; `show_terminal_attach_menu`), `menu.rs`
    (`attach_items(images, clipboard)`), `crates/flankr-companion/src/
    client.rs` (`Input::AttachImage { bytes, mime }`, `Input::
    DetachImage(u8)`, `Input::TerminalPasteImages(Vec<u8>)`, `Event::
    Attachments`, `State::blob()` and `attachments()`; a send's `parts`
    from `Blobs::parts`), `crates/flankr-companion/src/blob.rs`
    (`Attachment`, the `images` upload), `src/server/blob/images.rs` and
    `src/server/conn.rs` (`send_parts`, `policy::check_parts`, the paths
    as herdr pastes on the attachment), `src/server/terminal.rs`
    (`terminal.paste` with `images`), `src/commands.rs`
    (`validate_arbitrary_command`)"
  - "libmorph at the app's pinned revision `befe374` (0.6.0), read
    2026-11-13: `MorphContext::media_request` and the `mobile-media-request`
    feature are present; the app's Android features do not enable it.
    libmorph's head (0.8.0): `MediaRequest` (`kind`, `camera`, `picker`,
    `max_items`, `max_bytes`, `delivery`), `MediaItem` (`mime`, `size`,
    `exif_orientation`, `data`), `clipboard_text` and `clipboard_has_text`
    (text only; no image), `camera()`; no `decode_image` or `encode_image`"
related:
  - "docs/design/20261028_05_BINARY_FILES_PLAN.md (In implementation; its
    step 9, *attaching images*, waits: this plan carries the part of it
    that libmorph can serve today, and leaves conversion and Share waiting)"
  - "docs/design/20261028_06_BOTTOM_BAR_UPDATE_PLAN.md (In implementation;
    decision 8: the input row and its +; decisions 9-10: the terminal's +;
    the 23:39 EDT amendment: Gemini's + for Paste text; the 2026-10-29
    15:09 EDT amendment: the microphone; the 2026-11-03 13:27 EST
    amendment: the keyboard glyph on the voice cards)"
  - "docs/design/20261103_01_TERMINAL_TAPS_AND_SHELL_PANES_PLAN.md (its
    2026-11-03 15:38 EST amendment, item 3: image paste in any terminal
    that takes typing)"
  - "docs/design/20261028_02_PAIRING_QR_SCAN_PLAN.md (the camera the app
    already uses, through `mobile-camera-qr`)"
---

# Images through the message card's +, and the + at the left of the input row

## 1. Why

The message card's input row has the microphone and the keyboard toggle at
its right and nothing at its left. Its + was planned (the bottom bar plan,
decision 8) but is shown only while the daemon offers `images`, and the app
never reads that from the daemon: `input_tools()` hard-codes `images:
false`, so the + is missing on the message card, and on Gemini's card it
offers Paste text alone. Tapping Attach image… anywhere logs *not built
yet*.

Everything under the app is built. The daemon stores images sent through
the link's kind 2 in its protected folder, composes a message from `parts`
with each image's absolute path in its place, pastes the paths through
herdr's attachment, and pastes them into a terminal through
`terminal.paste`. The client uploads an image given as `Input::AttachImage`,
reports the grid's state as `Event::Attachments`, and sends the draft's
`[image N]` tokens as `parts`. What is missing is the app: the + on the
message card, the chooser, the grid and the tokens, and the terminal +'s
Attach image….

## 2. Findings (read 2026-11-13)

1. **The row.** `InputRow::lay_out` puts the + at the row's left edge
   (`Rect::new(x, y, side, side)`, `side` 44 pt), the toggle at the far
   right and the microphone left of it. The + is drawn only when
   `InputTools::attach(kind)` is true: always for Gemini's card, and for the
   message card only when `mobile && images`. So the glyph's place is
   already the left; only its condition hides it.
2. **The menu.** `menu::attach_items(images, clipboard)` builds Paste text
   (enabled with clipboard text) and, with `images`, Attach image…. A tap
   on Gemini's + opens the menu directly (the 23:39 EDT amendment); a tap on
   the message card's + logs *the media chooser is not built yet*.
3. **The daemon's offer.** `HelloResult` and `server.capabilities` carry
   `blob` with `namespaces`; the daemon lists `images` when its store is
   on. The client exposes it as `State::blob()` (`BlobInfo::serves`). The
   app never reads it.
4. **The client.** `Input::AttachImage { bytes, mime }` (PNG, JPEG, WebP or
   GIF, sniffed), `Input::DetachImage(u8)`, `Event::Attachments(Vec<
   Attachment>)` (number 1-4, session, MIME, size, progress, ready or
   failed), `blob::token(n)` for `[image N]`, and `Blobs::parts` when the
   draft is sent, refusing a send while an upload is not done.
   `Input::TerminalPasteImages(Vec<u8>)` sends `terminal.paste { images }`
   with the attached numbers.
5. **The daemon.** `send_parts` checks the text parts with `check_message`
   (entry, reach, `allow_messages`, `validate_message`, `check_paths`) and
   inserts each image's path from a live reference held by this principal
   for this pane; a leading path is quoted. A companion's tap (`confirmed:
   true`) sends at once: text runs as keys, each path as a herdr `Paste` on
   the attachment (bracketed when the pane has bracketed paste on), then
   Enter; without the attachment the path is typed. `validate_arbitrary_
   command` is not on this route: it serves the general agent's spoken
   commands, and its `/` rule never sees a daemon-made path.
6. **libmorph.** The app's pinned revision has `media_request` behind
   `mobile-media-request`, which the app does not enable. `MediaRequest`
   takes MIME types, a camera entry, the chooser or the Photo Picker,
   `max_items`, `max_bytes` and `Delivery::Bytes`; `MediaItem` gives
   `mime`, `size`, `exif_orientation` and the bytes. The clipboard calls
   are text only. `decode_image`/`encode_image` do not exist, so the app
   cannot convert HEIC or bake rotation, and on Android cannot make
   thumbnails of JPEG, WebP or GIF; `png` 0.18 is in `Cargo.lock` and the
   preview decodes PNG with it on the desktop.

## 3. Decisions

### The + glyph

1. **Position and size.** The + is the input row's leftmost control, a
   44 × 44 pt target at the row's left edge (`x`, the card's inner left),
   drawn as now: two 2 pt strokes 18 pt long in `theme().accent`, no
   background. The microphone and the keyboard toggle stay right-aligned
   (the toggle at `x + w - 44`, the microphone at `x + w - 88`). Nothing
   else in the row or the card moves; the row's height stays 44 pt and the
   card already grows by 48 pt for it.
2. **When it shows.** `InputTools::attach(PromptKind::Message)` becomes
   `mobile` alone: on a phone the message card's + shows whenever its row
   does, whether or not the daemon offers `images`. Gemini's + stays as it
   is (everywhere, Paste text only). On the desktop the message card has
   no + (the bottom bar plan, 11.2), since the media request answers
   `Unsupported` there.
3. **The tap.** A tap on the message card's + opens the add menu, anchored
   to the +, as Gemini's does (`open_attach_menu(kind)`). No hold. This
   replaces the bottom bar plan's decision 8 tap, which opened the chooser
   directly; the 23:39 EDT amendment already made Gemini's tap open the
   menu, and one behaviour for both + is simpler to learn.

### The add menu

4. **Items.** `attach_items` gains an `images_enabled` meaning: on the
   message card and the terminal the menu always has both items, **Paste
   text** (enabled while the clipboard holds text, as now) and **Attach
   image…**, enabled while the daemon offers `images` and the card has
   fewer than 4 attachments; otherwise drawn dim with a one-line reason
   under the menu as the volume menu's Stop has it: "This flankr can't take
   images." when the daemon lacks `images`, "Only 4 images can go in one
   message." when the grid is full. Gemini's menu keeps Paste text alone.
5. **`images` from the daemon.** `input_tools().images` is
   `client.blob().is_some_and(|b| b.serves("images"))`, read each frame
   like the other tools; it changes when the connection does.

### The image sources

6. **The chooser.** Attach image… calls `ctx.media_request` with
   `MediaRequest { kind: MediaKind::Mime(["image/png", "image/jpeg",
   "image/webp", "image/gif"]), camera: CameraEntry::Photo,
   camera_permission: CameraPermission::IfGranted, picker:
   Picker::Chooser, max_items: 4 - attached, max_bytes: IMAGE_MAX (20 MiB),
   delivery: Delivery::Bytes, title: None }`. Android's chooser then offers
   the gallery, files and, with CAMERA granted, the camera. The app adds
   libmorph's `mobile-media-request` feature on Android; no libmorph change
   is needed (its pinned 0.6.0 has the call).
7. **`IfGranted`, not `Ask`.** The camera entry is left out rather than
   prompting for CAMERA from a picker; the QR scan already asks for CAMERA
   on its first use, so after one scan the entry is there. Open question 2
   asks whether the owner wants the prompt instead.
8. **The reply.** `MorphApp::on_media` matches the request's id, then for
   each item in order: bytes under 16 bytes or over 20 MiB are refused with
   the binary files plan's notice ("Too large (over 20 MB)."); a type the
   sniffer (`libflankr_blob::ImageType`) does not know is refused with
   "flankr takes PNG, JPEG, WebP and GIF images." (so a HEIC photo is
   refused, not converted: see *Not covered*); each accepted item goes to
   the client as `Input::AttachImage { bytes, mime }`, `mime` the sniffed
   type's. A `MediaError::Unsupported` or `Cancelled` does nothing;
   `PermissionDenied` shows the refused notice once. Nothing of the bytes
   or the display names is logged; the log says `attach: <n> image(s), <k>
   refused`.
9. **No conversion, no EXIF changes.** The bytes go up as picked. A rotated
   JPEG arrives with its orientation tag, which the agent's reader may or
   may not honour; this is the binary files plan's open question 3 and its
   21:29 EDT amendment's items 6-7, waiting for libmorph. Open question 4.

### The grid and the tokens

10. **The grid,** as the binary files plan's 21:29 EDT amendment, item 1:
    above the typed box, inside the card, up to four 64 pt squares in one
    row (two rows of two when the inner width is under 4 × 64 + 3 × 8 pt),
    8 pt gaps, 8 pt above the box; each tile its number in the bottom-left
    corner (12 pt, `theme().fg` on a `theme().bg` pill) and an ✕ in a
    24 × 24 pt target at its top-right (`AgentHit::PromptDetach(kind, n)`,
    new). The card grows by the grid's height plus 8 pt while it has
    attachments.
11. **Tiles without pixels for now.** A PNG tile shows its thumbnail,
    decoded with the `png` crate the preview already uses, scaled to fit by
    nearest neighbour into a 64 × 64 pt texture; a JPEG, WebP or GIF tile
    shows a `theme().input` square with its type in capitals (JPG, WEBP,
    GIF) centred at 12 pt, until libmorph's `decode_image` exists. The
    upload's progress ring (bytes written over size, 2 pt, `theme().
    accent`), ✓ when ready and ! when failed are drawn over the tile, as
    the binary files plan's 9.3 says.
12. **Tokens.** When `Event::Attachments` reports a new number, the app
    inserts `[image N]` with a space either side at the typed card's cursor
    through the keyboard route's commit path, as Paste text does. ✕ sends
    `Input::DetachImage(n)`; the client drops the token (`drop_token`) and
    the tile goes with the next `Event::Attachments`. Deleting a token by
    hand is the client's to notice already (its `DetachImage` path); the
    app only draws what `Event::Attachments` says.
13. **Sending.** The card's send button sends as today; the client builds
    `parts` from the text and refuses the send with a spoken and shown
    reason while an upload is still going or failed ("Image 2 is still
    uploading." / "Image 2 failed; remove it or attach it again."). No app
    change beyond showing that reason in the card's notes.

### The terminal's +

14. **Attach image… in the terminal's menu** opens the same chooser
    (decision 6, `max_items` 4); each accepted item goes as
    `Input::AttachImage` for the open terminal's pane, with no token and
    no grid: the agent view's notice line shows "Uploading image 1 of
    2…" as the bottom bar plan's decision 10.1 says, and when every item
    is ready the app sends `Input::TerminalPasteImages(numbers)`. The
    daemon pastes the absolute paths, space-separated, as it does now
    (`terminal.paste`), into any pane that takes typing (the terminal taps
    plan's 15:38 EST amendment, item 3). The menu's Attach image… is dim
    with "This flankr can't take images." without `images`.

### Transport and the agent (restated, unchanged)

15. **Transport.** Each image is one kind-2 `images` transfer: OFFER
    (session, name, MIME, size, SHA-256) → ACCEPT → CHUNKs → END → DONE
    with an 8-byte reference. Limits are `libflankr_blob`'s: 20 MiB an
    image, 4 a message, 8 incoming at once on the daemon. The client
    already does all of this.
16. **How the agent gets it.** The daemon writes the image to
    `<temp>/flankr-<uid>/images/<pane>/<YYYYMMDD-HHMMSS>-<hash8>.<ext>`
    and, on the user's send, delivers the message in order: text as keys,
    each path as a herdr `Paste` on the attachment (so Claude Code turns
    it into `[Image #N]`), then Enter; an older herdr gets the path typed.
    A message that starts with an image has its path quoted. The user's
    text is checked as any message is; the path is never the user's to
    type, so `check_paths` and the `/` rules do not touch it. In a
    terminal, `terminal.paste` does the same with the terminal's own
    rules. Whether Claude Code reads the pasted path as an image on the
    owner's herdr is this plan's step 4 check (the binary files plan's open
    question 2 is still open).

### Confirmation and safety

17. **Confirmation.** The message card's send button is the user's review:
    the companion sends `confirmed: true` and the daemon sends at once, as
    for typed messages today, logging "Message for pane '<pane>' with <n>
    image(s) confirmed by the tap". A message with images needs the
    agent's `[[agents]]` entry, reach and `allow_messages` (the binary
    files plan's 21:29 EDT amendment, item 4). The general agent never
    attaches an image, so the voice confirmation gate is untouched.
18. **Safety.** Images come only from the user's own pick in the system
    chooser; the app never reads the gallery on its own. The app logs
    counts and MIME, never bytes, names or paths; the daemon's logging is
    the binary files plan's section 8. Attachments belong to one session:
    switching the agent view's session while the grid has images keeps
    them in that card's draft (the client keeps them by pane), and the grid
    shows only the open session's.

### Versions

19. **The app:** each step that lands a visible change bumps the app's
    patch in the 0.7.x series (0.7.23 at `79065d8`; the next free patch at
    each commit) with its `apps/UPDATES.md` entry, tagged
    `flankr-app-v<version>` (FLANKR RULE 1). Minor bumps are not used, on
    the owner's standing instruction to keep the series.
20. **flankr-companion:** no change is expected (`AttachImage`,
    `DetachImage`, `TerminalPasteImages` and `Attachments` exist). If
    step 3 needs the client to carry a tile's type or progress it does not
    have, the field is appended to `Attachment` (additive) and the crate
    takes its next patch (0.8.12 or the next free) with a `VERSIONS.md`
    entry.
21. **libflankr_proto, libflankr_blob, the daemon:** no change. The daemon
    gets no `UPDATES.md` entry, since nothing the person running `flankr`
    sees changes.
22. **libmorph:** no change. The app enables an existing feature. A
    clipboard-image call (open question 3) would be a later, additive
    libmorph and libmobile addition: `MorphContext::clipboard_image() ->
    Option<(Vec<u8>, String)>` and `clipboard_has_image()`, a patch of
    each, named here only so the dependency is visible.
23. **Nothing breaking.** Every change is in the app or appends to a
    `#[non_exhaustive]` type.

## 4. Steps

1. **companion: the + on the message card, and `images` from the daemon.**
   `InputTools::attach(Message)` is `mobile`; `input_tools().images` reads
   `client.blob()`; the message card's `PromptAttach` tap opens the menu;
   `attach_items` takes the dim reasons of decision 4; the terminal menu's
   Attach image… follows the same rule. Tests in `prompt/input_row.rs` and
   `menu.rs`: the + at `x` on the message card with `images: false`; Attach
   image… present and dim without `images`, enabled with it. App patch and
   `apps/UPDATES.md` entry.
   **Done when:** on the phone the message card's row shows + at its left
   and the microphone and keyboard at its right; a tap opens Paste text and
   Attach image…, the latter dim against a daemon without `images` and
   enabled against the owner's.
2. **companion: the chooser.** The `mobile-media-request` feature on
   Android; `Choice::AttachImage` calls `media_request` as decision 6;
   `MorphApp::on_media` sniffs, refuses and sends `Input::AttachImage`
   (decision 8). A unit test on the reply handling with fixture bytes (a
   PNG accepted, a 12-byte file refused, a text file refused). App patch
   and entry.
   **Done when:** on the S23, Attach image… opens Android's chooser with
   the gallery and files (and the camera once CAMERA is granted); a picked
   PNG uploads, and the daemon's log shows the `images` transfer's DONE.
3. **companion: the grid and the tokens.** The grid from `Event::
   Attachments` (decisions 10-11), `[image N]` at the cursor, ✕ and
   `DetachImage`, the send refusals shown (decisions 12-13). Layout tests
   in `prompt/card.rs`: four tiles in one row at a phone's width, two by
   two when narrow, the card's height. App patch and entry.
   **Done when:** two attached images show as tiles with their numbers and
   progress, their tokens sit in the text, ✕ removes a tile and its token,
   and the send button is refused while an upload is going.
4. **companion: sending, checked end to end.** No code beyond fixes found:
   on the S23 against the owner's daemon, a message with text and one
   image goes to a Claude Code session; Claude shows `[Image #1]` and
   describes the image. Also a message that starts with an image (the
   quoted path). Evidence under `out/images/20261113_01_attach/` with the
   daemon's log lines (no paths) and the app's screen.
   **Done when:** Claude Code describes the sent image, and the quoted
   leading-path form either works or is recorded as not working in the
   evidence log for the binary files plan's open question 2.
5. **companion: the terminal's Attach image….** Decision 14: the chooser
   from the keys row's +, the notice line, `TerminalPasteImages`. App patch
   and entry.
   **Done when:** in a shell pane open in terminal view, Attach image…
   puts the image's absolute path at the prompt, and `ls -l` on it shows
   the file.
6. **README** (its own commit, by the lead): the decision table's row for
   this plan, and the features paragraph on images mentions the +.
   **Done when:** the row's status matches this plan's.

## 5. Open questions for the owner

1. **Which card.** This plan reads "the input card which already has voice
   and keyboard glyph" as the typed message card's row (microphone and
   keyboard toggle at the right). If the voice (dictation) card is meant
   too, where the keyboard glyph sits left of the up arrow on the header
   line, should a + go at the header's left as well, opening the same
   menu?
2. **The camera entry.** `CameraPermission::IfGranted` leaves the camera
   out of the chooser until the QR scan has asked for CAMERA. Prefer `Ask`,
   so the first Attach image… prompts for CAMERA itself?
3. **Clipboard images.** The owner's "images not just text" may include a
   screenshot copied to the clipboard. libmorph reads only clipboard text
   today; a `clipboard_image` call is a new libmorph and libmobile item
   (decision 22). Want it planned there now, so the menu gains **Paste
   image**?
4. **Unconverted photos.** Until libmorph's `decode_image`/`encode_image`,
   a HEIC photo from the camera roll is refused and a rotated JPEG goes up
   with its orientation tag. Acceptable for now, or should this plan wait
   for the conversion items of the binary files plan?
5. **Thumbnails.** PNG tiles get real thumbnails; JPEG, WebP and GIF tiles
   show their type only (decision 11). Fine for now?
6. **The tap on +.** The bottom bar plan's decision 8 had a tap open the
   chooser directly and a hold open the menu; this plan makes the tap open
   the menu on both cards (decision 3). Agree, or keep tap-for-chooser on
   the message card?

## Amendment — 2026-11-13 01:31 EST: approved by the owner, and the open questions settled

The owner, on 2026-11-13, verbatim and in full: "approve the image plan
and build it". That is this plan's approval. The owner gave no answers to
section 5's open questions, so the plan is built on its own stated
assumptions and recommendations; each question is settled as follows,
and the decisions they name stand as written:

1. **Which card:** the typed message card's input row only (decisions
   1-3). No + goes on the voice card's header.
2. **The camera entry:** `CameraPermission::IfGranted`, as decisions 6
   and 7 recommend: the menu never asks for CAMERA itself, and the
   chooser's camera entry is there once the QR scan has been granted it.
3. **Clipboard images:** none. The menu has no Paste image, and nothing
   is planned in libmorph or libmobile for it (decision 22 stays a note).
4. **Unconverted photos:** as decisions 8 and 9 say: a HEIC photo is
   refused with "flankr takes PNG, JPEG, WebP and GIF images.", and a
   JPEG goes up as it is, with its orientation tag.
5. **Thumbnails:** PNG tiles only; JPEG, WebP and GIF tiles show their
   type label (decision 11).
6. **The tap on +:** a tap opens the add menu on both cards (decision 3).

This amendment replaces nothing in the body; it closes section 5.

## Amendment — 2026-11-13 01:31 EST: implementation started; this document is now immutable

Implementation started with step 1 (the + on the message card, and
`images` from the daemon) in the commit that sets `status: In
implementation`. From now on this document changes only on the owner's
explicit instruction; departures go to `CHANGES.md`.
