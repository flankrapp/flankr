//! The app's side of the paired remotes (docs/design/20261028_01_MULTIPLE_REMOTES_PLAN.md,
//! step 5): the pairing screen's state and lines, and *Delete data*'s
//! removals. A pairing's result stays on the pairing window and a
//! removal's in its confirm modal: Settings' Remotes has no note (plan
//! 20261024_02, the 2026-10-28 17:48 EDT amendment).
//!
//! - [`Pairing`]: whether the pairing screen is in its **add** mode (the
//!   **+** in Remotes, decision 9), the link last read ([`LinkRead`]),
//!   what a waiting pairing shows ([`Waiting`], from
//!   [`flankr_companion::Event::PairingWords`]), a finished pairing
//!   ([`Finished`]) and whether **Details** are open. Cancel, the add
//!   mode's "<" and the system Back end a pairing with
//!   [`Input::CancelPairing`].
//! - [`Pairing::lines`]: the pairing modal's lines while a pairing waits
//!   (the owner, 2026-10-28: "if we're using the six words to confirm the
//!   connection, shouldn't we show them in a confirmation modal?"). Quiet,
//!   as the 14:42 EDT amendment has it: the six pairing words, large, and
//!   one line saying to confirm on the Mac. Its Details: the daemon's name
//!   and `host:port`, then the remote details modal's Fingerprint row
//!   ([`Pairing::fingerprint_row`]: collapsed, and expanding to the same
//!   six words and BLAKE2s-256 hex; plan 20261028_02, the 2026-11-03 12:13
//!   EST amendment, item 2), the time the link has left, and in the
//!   device-code fallback the code to paste into `flankr pair`.
//! - A finished pairing (`Paired`, or `RemoteUpdated` for one already
//!   listed) becomes the remote's details modal in place, with a "Paired
//!   with <name>" flash at its top ([`Pairing::ready`]; plan 20261028_02,
//!   the 2026-11-03 12:40 EST amendment). Until the remote's row is known
//!   the waiting modal stays as it is.
//! - The lines are Markdown (drawn with `flankr-markdown`), so the copy can
//!   set `flankr pair` in bold; values put into them go through
//!   [`md_escape`].
//! - [`delete_all`]: *Delete data* removes every remote
//!   ([`Input::RemoveRemote`]), the one in use last; with none, it asks
//!   for [`Input::Unpair`], which answers [`flankr_companion::Event::Unpaired`]
//!   at once. The data directory goes once `Unpaired` comes.

use std::time::{Duration, Instant};

use flankr_companion::{DeviceCodeFallback, Input, RemoteId, RemoteInfo};
use libflankr_proto::link::FLANKR_PROFILE;
use libhardline::PastedLink;

use crate::settings::copy;
use crate::theme::{Role, theme};
use crate::ui::{FingerprintRow, Layout, Line, chunk, wrap};

/// The pairing words' size, in points: large, to be read against the
/// Mac's terminal at a glance.
pub const WORDS_PT: f32 = 34.0;
/// The notes' and labels' size, in points.
const NOTE_PT: f32 = 15.0;
/// The device code's size, in points.
const CODE_PT: f32 = 26.0;
/// The pairing words on one line: two lines of three, as `flankr pair`
/// prints six in a row.
const WORDS_PER_LINE: usize = 3;

/// `text` with every ASCII punctuation character backslash-escaped, so a
/// value (a name, a host, a reason) put into the pairing screen's Markdown
/// shows as written.
pub fn md_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii_punctuation() {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// The whole fingerprint's 64 hex digits (as the client gives them) in
/// groups of four.
pub fn hex_groups(hex: &str) -> String {
    hex.as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The host of `host:port`, without the port (an IPv6 address in brackets
/// is kept whole).
pub fn host_of(address: &str) -> &str {
    match address.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') && port.parse::<u16>().is_ok() => host,
        _ => address,
    }
}

/// What the app read of the pairing link it handed to the client, for
/// "Connecting to <host>…" and a finished pairing's name and address. The
/// fingerprint comes from the client: `PairingWords` and the remote's row
/// carry it whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkRead {
    /// The daemon's name, as the client names it.
    pub server: String,
    /// The link's first `host:port`.
    pub address: String,
}

impl LinkRead {
    /// The link `text`, if `flankr_companion::scan::read` reads it.
    pub fn of(text: &str) -> Option<Self> {
        let p = flankr_companion::scan::read(text).ok()?;
        Some(LinkRead {
            server: p.server,
            address: p.payload.addresses.first()?.clone(),
        })
    }
}

/// What a waiting pairing shows ([`flankr_companion::Event::PairingWords`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Waiting {
    /// The daemon's name, from the pairing link.
    pub server: String,
    /// The daemon's `host:port`.
    pub address: String,
    /// The six pairing words; `None` for a link without an offer.
    pub words: Option<String>,
    /// The daemon's fingerprint as six words.
    pub fingerprint: String,
    /// The whole fingerprint, 64 hex digits in groups of four, from the
    /// event.
    pub fingerprint_hex: String,
    /// When the link's offer expires; `None` in the fallback.
    pub expires_at: Option<Instant>,
    /// The device-code fallback, if this is it.
    pub fallback: Option<DeviceCodeFallback>,
}

impl Waiting {
    /// From the event's fields, received at `now`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        server: String,
        address: String,
        words: Option<String>,
        fingerprint: String,
        fingerprint_hex: &str,
        expires_in_secs: Option<u16>,
        fallback: Option<DeviceCodeFallback>,
        now: Instant,
    ) -> Self {
        Waiting {
            server,
            address,
            words,
            fingerprint,
            fingerprint_hex: hex_groups(fingerprint_hex),
            expires_at: expires_in_secs.map(|s| now + Duration::from_secs(u64::from(s))),
            fallback,
        }
    }
}

/// A pairing that ended well, until the app opens its remote's details
/// modal ([`Pairing::ready`]): at once when its row is known, else when
/// the next `Remotes` brings it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finished {
    /// The daemon's name.
    pub server: String,
    /// It was already paired (`RemoteUpdated`).
    pub already: bool,
    /// The remote, once known (`RemoteUpdated` names it; `Paired`'s is
    /// found by name in the next `Remotes`).
    pub id: Option<RemoteId>,
    /// The remotes listed when `Paired` came: a new row of the same name
    /// is taken before one of these.
    known: Vec<RemoteId>,
}

impl Finished {
    /// "Paired with m4", or "Already paired with m4": the details modal's
    /// flash, and the window's title while the row is not known.
    pub fn title(&self) -> String {
        let key = if self.already {
            copy::PAIRING_ALREADY
        } else {
            copy::PAIRING_PAIRED
        };
        key.fill(None, &[("name", &self.server)]).trim().to_string()
    }
}

/// The pairing screen's state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pairing {
    /// The add mode, opened by the **+** in Remotes, over Settings.
    pub adding: bool,
    /// The link last handed to the client.
    pub link: Option<LinkRead>,
    /// What a waiting pairing shows.
    pub waiting: Option<Waiting>,
    /// A pairing that ended well, until its remote's details open.
    pub finished: Option<Finished>,
    /// The waiting modal's Details are open.
    pub details: bool,
    /// The Fingerprint row in them is expanded; collapsed each time
    /// Details opens.
    pub fingerprint_open: bool,
}

impl Pairing {
    /// The **+**: the add mode, with nothing pairing yet.
    pub fn open_add(&mut self) {
        *self = Pairing {
            adding: true,
            ..Pairing::default()
        };
    }

    /// A link was handed to the client: what was read of it.
    pub fn read(&mut self, link: Option<LinkRead>) {
        self.link = link;
        self.finished = None;
        self.details = false;
    }

    /// A pairing waits: its words are shown in the modal, with Details and
    /// Cancel. With no words to show (a link without an offer) Details open
    /// at once, as the code there is all there is to do.
    pub fn show(&mut self, waiting: Waiting) {
        if waiting.words.is_none() {
            if !self.details {
                self.fingerprint_open = false;
            }
            self.details = true;
        } else if self.waiting.as_ref().map(|w| &w.words) != Some(&waiting.words) {
            self.details = false;
            self.fingerprint_open = false;
        }
        self.finished = None;
        self.waiting = Some(waiting);
    }

    /// A pairing waits for the owner: the modal shows.
    pub fn waiting(&self) -> bool {
        self.waiting.is_some()
    }

    /// The waiting modal's Details button: opens or closes them, the
    /// Fingerprint row collapsed.
    pub fn toggle_details(&mut self) {
        self.details = !self.details;
        self.fingerprint_open = false;
    }

    /// A tap on the Fingerprint row in the waiting modal's Details:
    /// expands or collapses it, as in the remote details modal.
    pub fn toggle_fingerprint(&mut self) {
        self.fingerprint_open = !self.fingerprint_open;
    }

    /// The Fingerprint row of the waiting modal's Details, while they are
    /// open: after their first line (the daemon's name and `host:port`),
    /// expanded to the remote details modal's own section.
    pub fn fingerprint_row(&self) -> Option<FingerprintRow> {
        let w = self.waiting.as_ref().filter(|_| self.details)?;
        Some(FingerprintRow {
            at: 1,
            open: self.fingerprint_open,
            section: crate::settings::remote_details::fingerprint_section(
                &w.fingerprint,
                &w.fingerprint_hex,
            ),
        })
    }

    /// Cancel, the add mode's "<", or the system Back: the pairing in
    /// progress ends (its connection closed and its new key deleted by the
    /// client) and the add mode closes, back to Settings. The input to
    /// send.
    pub fn cancel(&mut self) -> Input {
        *self = Pairing::default();
        Input::CancelPairing
    }

    /// The daemon added the key. The waiting modal stays as it is until
    /// the remote's row is known ([`Pairing::ready`]); the daemon's name,
    /// if known.
    pub fn paired(&mut self, rows: &[RemoteInfo]) -> Option<String> {
        let link = self.link.take();
        let server = self
            .waiting
            .as_ref()
            .map(|w| w.server.clone())
            .or_else(|| link.as_ref().map(|l| l.server.clone()));
        self.finished = Some(Finished {
            server: server.clone().unwrap_or_default(),
            already: false,
            id: None,
            known: rows.iter().map(|r| r.id).collect(),
        });
        server
    }

    /// A pairing named a remote already listed, and its addresses were
    /// checked: its row is known at once.
    pub fn remote_updated(&mut self, id: RemoteId, name: &str) {
        self.link = None;
        self.waiting = None;
        self.details = false;
        self.fingerprint_open = false;
        self.finished = Some(Finished {
            server: name.to_string(),
            already: true,
            id: Some(id),
            known: Vec::new(),
        });
    }

    /// The remotes changed: a finished pairing finds its row.
    pub fn remotes_changed(&mut self, rows: &[RemoteInfo]) {
        if let Some(f) = &mut self.finished {
            fill_from_rows(f, rows);
        }
    }

    /// A finished pairing whose remote's row is known: the remote, and the
    /// flash its details modal opens with ("Paired with m4"). The app then
    /// ends the pairing ([`Pairing::done`]) and opens the modal.
    pub fn ready(&self) -> Option<(RemoteId, String)> {
        let f = self.finished.as_ref()?;
        Some((f.id?, f.title()))
    }

    /// The finished pairing goes (its details modal opening, or Done or
    /// Back before its row is known). Whether it was the add mode (back to
    /// Settings) rather than the first run (on to the overview).
    pub fn done(&mut self) -> bool {
        let adding = self.adding;
        *self = Pairing::default();
        adding
    }

    /// A pairing failed or ended: the modal closes, and the window shows
    /// why.
    pub fn failed(&mut self) {
        self.waiting = None;
        self.link = None;
        self.details = false;
        self.fingerprint_open = false;
    }

    /// The pairing area's lines at `now`, Markdown: in the add mode with
    /// nothing pairing, what to do; while a pairing waits, the modal's: its
    /// words, large, and one line, or its Details; then the note, if any.
    pub fn lines(
        &self,
        layout: &Layout,
        note: Option<(&str, [f32; 4])>,
        now: Instant,
    ) -> Vec<Line> {
        let mut out = Vec::new();
        match &self.waiting {
            None if self.adding => para(
                &mut out,
                layout,
                copy::REMOTE_ADD_PASTE.text,
                theme().muted,
                0.0,
            ),
            None => {}
            Some(w) if self.details => details(&mut out, layout, w, now),
            Some(w) => {
                let escaped = md_escape(&w.server);
                let server = [("server", escaped.as_str())];
                let line = match (&w.words, &w.fallback) {
                    (Some(words), None) => {
                        words_lines(&mut out, layout, words);
                        copy::PAIRING_CONFIRM.fill(None, &server)
                    }
                    (Some(words), Some(_)) => {
                        words_lines(&mut out, layout, words);
                        copy::PAIRING_FALLBACK.fill(None, &server)
                    }
                    (None, _) => copy::PAIRING_FALLBACK_NO_WORDS.fill(None, &server),
                };
                para(&mut out, layout, &line, theme().muted, 16.0);
            }
        }
        if let Some((note, color)) = note {
            para(&mut out, layout, note, color, 8.0);
        }
        out
    }
}

/// `f`'s row, by name, once: a new one before one listed when `Paired`
/// came.
fn fill_from_rows(f: &mut Finished, rows: &[RemoteInfo]) {
    if f.id.is_some() {
        return;
    }
    let named = || rows.iter().filter(|r| r.name == f.server);
    f.id = named()
        .find(|r| !f.known.contains(&r.id))
        .or_else(|| named().next())
        .map(|r| r.id);
}

/// `text` (Markdown, one paragraph) at the notes' size, its first line
/// `gap` below the last. The screen wraps it to its width.
pub(crate) fn para(out: &mut Vec<Line>, _layout: &Layout, text: &str, color: [f32; 4], gap: f32) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    out.push(Line {
        text: text.to_string(),
        size: NOTE_PT,
        color,
        gap,
    });
}

/// Six words in lines of [`WORDS_PER_LINE`] at `size`, each wrapped if
/// still too wide.
fn word_rows(out: &mut Vec<Line>, layout: &Layout, words: &str, size: f32, color: [f32; 4]) {
    let words: Vec<&str> = words.split_whitespace().collect();
    for row in words.chunks(WORDS_PER_LINE) {
        for t in wrap(&row.join(" "), layout.columns(size)) {
            out.push(Line {
                text: md_escape(&t),
                size,
                color,
                gap: 4.0,
            });
        }
    }
}

/// The pairing words, large: two rows of three, at [`WORDS_PT`] or as
/// large as fits the longer row on one line (Roboto's lower case is about
/// 0.56 em a letter).
fn words_lines(out: &mut Vec<Line>, layout: &Layout, words: &str) {
    let list: Vec<&str> = words.split_whitespace().collect();
    let longest = list
        .chunks(WORDS_PER_LINE)
        .map(|row| row.join(" ").chars().count())
        .max()
        .unwrap_or(1)
        .max(1);
    let width_pt = layout.w / layout.scale.max(0.1) - 2.0 * Layout::PAD;
    let size = WORDS_PT.min((width_pt / (longest as f32 * 0.56)).floor());
    word_rows(out, layout, words, size, theme().text);
}

/// The daemon's name and `host:port`: the first line of the waiting
/// modal's Details, the Fingerprint row under it
/// ([`Pairing::fingerprint_row`]).
fn identity(out: &mut Vec<Line>, layout: &Layout, server: &str, address: &str) {
    let line = copy::PAIRING_ADDRESS.fill(
        None,
        &[
            ("server", &md_escape(server)),
            ("address", &md_escape(address)),
        ],
    );
    para(out, layout, &line, theme().text, 0.0);
}

/// The modal's Details: the daemon's name and `host:port` (then, drawn by
/// the screen, the Fingerprint row), the time the link has left, then the
/// device-code fallback's code (or, without words, why there are none).
fn details(out: &mut Vec<Line>, layout: &Layout, w: &Waiting, now: Instant) {
    let escaped = md_escape(&w.server);
    let server = [("server", escaped.as_str())];
    identity(out, layout, &w.server, &w.address);
    if let Some(at) = w.expires_at {
        let left = at.saturating_duration_since(now);
        let text = if left.is_zero() {
            copy::PAIRING_EXPIRED.text.to_string()
        } else {
            let secs = left.as_secs() + u64::from(left.subsec_nanos() > 0);
            copy::PAIRING_EXPIRES.fill(None, &[("seconds", &secs.to_string())])
        };
        para(out, layout, &text, theme().muted, 12.0);
    }
    if w.words.is_none() {
        para(
            out,
            layout,
            copy::PAIRING_NO_WORDS.text,
            theme().muted,
            12.0,
        );
    }
    if let Some(fallback) = &w.fallback {
        if fallback.replace {
            para(
                out,
                layout,
                &copy::PAIRING_REPLACE.fill(None, &server),
                theme().get(Role::Warn),
                12.0,
            );
        }
        para(
            out,
            layout,
            &copy::PAIRING_PASTE.fill(None, &server),
            theme().muted,
            12.0,
        );
        // Base58 is mostly capitals and wide letters: wider than the half
        // an em `columns` assumes.
        for part in chunk(&fallback.code, layout.columns(CODE_PT * 1.3)) {
            out.push(Line {
                text: part,
                size: CODE_PT,
                color: theme().code,
                gap: 4.0,
            });
        }
    }
}
/// *Delete data*'s inputs: [`Input::RemoveRemote`] for every remote, the
/// one in use last (the client asks the others by their own short
/// connections meanwhile, and the last removal gives `Unpaired`); with
/// none paired, [`Input::Unpair`], which gives `Unpaired` at once.
pub fn delete_all(remotes: &[RemoteInfo]) -> Vec<Input> {
    if remotes.is_empty() {
        return vec![Input::Unpair];
    }
    let (active, others): (Vec<_>, Vec<_>) = remotes.iter().partition(|r| r.active);
    others
        .into_iter()
        .chain(active)
        .map(|r| Input::RemoveRemote(r.id))
        .collect()
}

/// What [`clipboard_pairing_link`] finds in a text: libhardline's
/// [`PastedLink`], under the app's names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardLink {
    /// A complete pairing URL accepted by [`LinkRead::of`], rejoined when
    /// it had wrapped over lines.
    Found(String),
    /// The text holds `flankr://…/pair#…`, but no reading of it is a valid
    /// pairing URL: it was cut short or altered on the way.
    Damaged,
    /// The text holds no pairing link.
    None,
}

impl From<PastedLink> for ClipboardLink {
    fn from(link: PastedLink) -> Self {
        match link {
            PastedLink::Found(url) => ClipboardLink::Found(url),
            PastedLink::Damaged => ClipboardLink::Damaged,
            PastedLink::None => ClipboardLink::None,
        }
    }
}

/// The pairing URL in `text`, read by libhardline's pasted-link reader
/// under flankr's profile (plan 20261119_02, decision 3). The text may
/// include a label or sentence around the link, or have it wrapped over
/// lines in a terminal pane with the pane's border characters copied in
/// (the 2026-11-19 amendment to plan 20261028_02): from a `flankr://`, a
/// valid ASCII host and port and `/pair#` are read, then only base58,
/// skipping anything else. As text after the link may be base58 too, the
/// link's end is where its checksum holds: the longest reading
/// [`LinkRead::of`] accepts is [`ClipboardLink::Found`]. A link with
/// `/pair#` and no reading that holds is [`ClipboardLink::Damaged`].
pub fn clipboard_pairing_link(text: &str) -> ClipboardLink {
    libhardline::read_pairing(&FLANKR_PROFILE, text).into()
}

/// The paste button in the pairing field: `field` becomes the pairing URL
/// found in `clipboard` (read on the tap). With none found or a damaged
/// one, `field` is left as it is. Returns what was found.
pub fn paste_pairing_link(field: &mut String, clipboard: Option<&str>) -> ClipboardLink {
    let found = clipboard.map_or(ClipboardLink::None, clipboard_pairing_link);
    if let ClipboardLink::Found(link) = &found {
        *field = link.clone();
    }
    found
}

/// The pairing field's text, typed or pasted through the keyboard: when
/// it holds a valid pairing URL among other text, it is reduced to just
/// that URL. Returns whether it changed.
pub fn reduce_to_pairing_link(field: &mut String) -> bool {
    match clipboard_pairing_link(field) {
        ClipboardLink::Found(link) if link != *field => {
            *field = link;
            true
        }
        _ => false,
    }
}

/// Whether `text` is a pairing link, `flankr://<host>:<port>/pair#…`:
/// the test an opened link and a scanned QR code both pass before they
/// are acted on (plan 20261028_02, decision 8). Only the part before the
/// fragment (or a query) is looked at, as the fragment is the pairing
/// string.
pub fn is_pairing_link(text: &str) -> bool {
    let before_fragment = text.trim().split(['#', '?']).next().unwrap_or("");
    before_fragment
        .strip_prefix("flankr://")
        .is_some_and(|rest| rest.ends_with("/pair"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use flankr_companion::RemoteId;

    #[test]
    fn a_pairing_link_is_flankr_host_port_pair() {
        assert!(is_pairing_link("flankr://m4.ts.net:7347/pair#flankr1abc"));
        assert!(is_pairing_link(" flankr://127.0.0.1:7347/pair#x \n"));
        assert!(!is_pairing_link("https://example.com"));
        assert!(!is_pairing_link("flankr1abc"));
        assert!(!is_pairing_link("flankr://m4:7347/other#flankr1abc"));
        assert!(!is_pairing_link("flankr://m4:7347/x#/pair"));
        assert!(!is_pairing_link(""));
    }

    #[test]
    fn clipboard_pairing_link_extracts_only_a_valid_url() {
        let url = libflankr_proto::link::PairingPayload {
            server_key: [7; 32],
            addresses: vec!["m4.ts.net:7347".into()],
            offer: Some([1; libflankr_proto::link::OFFER_LEN]),
        }
        .url()
        .unwrap();
        assert_eq!(
            clipboard_pairing_link(&format!("Copy this: {url}.")),
            ClipboardLink::Found(url.clone())
        );
        assert_eq!(
            clipboard_pairing_link("https://example.com"),
            ClipboardLink::None
        );
        assert_eq!(
            clipboard_pairing_link("flankr://m4.ts.net:7347/pair#flankr1abc"),
            ClipboardLink::Damaged
        );
    }

    /// `url` cut into lines of 50 characters joined by `join`.
    fn wrapped(url: &str, join: &str) -> String {
        let chars: Vec<char> = url.chars().collect();
        chars
            .chunks(50)
            .map(|line| line.iter().collect::<String>())
            .collect::<Vec<_>>()
            .join(join)
    }

    #[test]
    fn a_link_wrapped_over_lines_is_rejoined() {
        let url = a_pairing_url();
        assert!(url.len() > 100, "the URL wraps over three lines or more");
        for join in [
            "\u{2595}\n",
            "\u{2502}\n",
            "  \n  ",
            "\r\n",
            "\u{2595}\r\n\u{258f} ",
        ] {
            let text = wrapped(&url, join);
            assert_eq!(
                clipboard_pairing_link(&text),
                ClipboardLink::Found(url.clone()),
                "joined by {join:?}"
            );
            let text = format!("Pair with this:\n{text}\u{2595}\n\nthanks.");
            assert_eq!(
                clipboard_pairing_link(&text),
                ClipboardLink::Found(url.clone()),
                "in a message, joined by {join:?}"
            );
        }
    }

    #[test]
    fn a_wrapped_link_with_a_changed_character_is_damaged() {
        let url = a_pairing_url();
        let at = url.len() - 10;
        let swap = if &url[at..at + 1] == "2" { "3" } else { "2" };
        let bad = format!("{}{swap}{}", &url[..at], &url[at + 1..]);
        assert_eq!(clipboard_pairing_link(&bad), ClipboardLink::Damaged);
        assert_eq!(
            clipboard_pairing_link(&wrapped(&bad, "\u{2595}\n")),
            ClipboardLink::Damaged
        );
        // A line lost from the middle.
        let lines: Vec<String> = wrapped(&url, "\n").lines().map(String::from).collect();
        let mut short = lines.clone();
        short.remove(1);
        assert_eq!(
            clipboard_pairing_link(&short.join("\u{2595}\n")),
            ClipboardLink::Damaged
        );
        assert_eq!(
            clipboard_pairing_link("Plain text,\nno link here."),
            ClipboardLink::None
        );
        assert_eq!(clipboard_pairing_link(""), ClipboardLink::None);
    }

    #[test]
    fn stray_characters_inside_a_link_are_skipped() {
        let url = a_pairing_url();
        for stray in ["|", "\u{a6}", "\u{a0}", "\t", " \u{2502} "] {
            let text = wrapped(&url, stray);
            assert_eq!(
                clipboard_pairing_link(&text),
                ClipboardLink::Found(url.clone()),
                "with {stray:?} inside"
            );
        }
    }

    #[test]
    fn text_after_a_link_is_not_read_into_it() {
        let url = a_pairing_url();
        for text in [
            format!("{url}. Paste it in the app"),
            format!(
                "{}\u{2595}\nPaste it in the app",
                wrapped(&url, "\u{2595}\n")
            ),
            format!("{url}Paste"),
        ] {
            assert_eq!(
                clipboard_pairing_link(&text),
                ClipboardLink::Found(url.clone()),
                "{text:?}"
            );
        }
    }

    #[test]
    fn a_bracketed_ipv6_host_is_read() {
        let url = libflankr_proto::link::PairingPayload {
            server_key: [9; 32],
            addresses: vec!["[fd7a:115c::1]:7347".into()],
            offer: Some([2; libflankr_proto::link::OFFER_LEN]),
        }
        .url()
        .unwrap();
        assert_eq!(
            clipboard_pairing_link(&wrapped(&url, "\u{2595}\n")),
            ClipboardLink::Found(url)
        );
    }

    #[test]
    fn the_paste_button_with_a_damaged_link_leaves_the_field() {
        let url = a_pairing_url();
        let mut field = "typed".to_string();
        let cut = &url[..url.len() - 3];
        assert_eq!(
            paste_pairing_link(&mut field, Some(cut)),
            ClipboardLink::Damaged
        );
        assert_eq!(field, "typed");
    }

    fn a_pairing_url() -> String {
        libflankr_proto::link::PairingPayload {
            server_key: [7; 32],
            addresses: vec!["m4.ts.net:7347".into()],
            offer: Some([1; libflankr_proto::link::OFFER_LEN]),
        }
        .url()
        .unwrap()
    }

    #[test]
    fn the_paste_button_fills_the_field_with_just_the_link() {
        let url = a_pairing_url();
        let mut field = "typed".to_string();
        let clipboard = format!("Run this on your phone:\n{url}\nthanks");
        assert_eq!(
            paste_pairing_link(&mut field, Some(&clipboard)),
            ClipboardLink::Found(url.clone())
        );
        assert_eq!(field, url);
    }

    #[test]
    fn the_paste_button_without_a_link_leaves_the_field() {
        let mut field = "typed".to_string();
        assert_eq!(
            paste_pairing_link(&mut field, Some("https://example.com")),
            ClipboardLink::None
        );
        assert_eq!(paste_pairing_link(&mut field, None), ClipboardLink::None);
        assert_eq!(field, "typed");
    }

    #[test]
    fn typed_text_holding_a_link_is_reduced_to_the_link() {
        let url = a_pairing_url();
        let mut field = format!("pair with {url} please");
        assert!(reduce_to_pairing_link(&mut field));
        assert_eq!(field, url);
        // Just the link, or no complete link yet: left as typed.
        assert!(!reduce_to_pairing_link(&mut field));
        let mut partial = format!("x {}", &url[..url.len() - 3]);
        let before = partial.clone();
        assert!(!reduce_to_pairing_link(&mut partial));
        assert_eq!(partial, before);
    }

    const PHONE: Layout = Layout {
        w: 1080.0,
        h: 2340.0,
        scale: 2.0,
        top: 80.0,
        bottom: 40.0,
    };

    const WORDS: &str = "abandon ability able about above absent";
    const FINGERPRINT: &str = "zoo zone youth wrist wrong yellow";

    /// The whole fingerprint the tests' events and rows carry.
    const HEX: &str = "3f9a12c4";

    fn remote(n: u8, active: bool) -> RemoteInfo {
        RemoteInfo {
            id: RemoteId([n; 8]),
            name: format!("m{n}"),
            host: format!("m{n}.ts.net:7347"),
            fingerprint: format!("{n}{n}{n}{n} 0000"),
            paired_at: Some(1_793_188_800 + u64::from(n)),
            active,
            online: active,
            fingerprint_hex: HEX.repeat(8),
            fingerprint_words: FINGERPRINT.into(),
            nickname: None,
            display_name: format!("m{n}"),
            last_session: None,
            collapsed_workspaces: Vec::new(),
        }
    }

    /// A request the daemon holds, received at `now`, with 60 s left.
    fn held(now: Instant) -> Waiting {
        Waiting::new(
            "m4".into(),
            "m4.ts.net:7347".into(),
            Some(WORDS.into()),
            FINGERPRINT.into(),
            &HEX.repeat(8),
            Some(60),
            None,
            now,
        )
    }

    fn code() -> String {
        "flankrkey1".to_string() + &"Ab3".repeat(20)
    }

    /// The device-code fallback, with or without words.
    fn fallback(words: bool, replace: bool, now: Instant) -> Waiting {
        Waiting::new(
            "m4".into(),
            "m4.ts.net:7347".into(),
            words.then(|| WORDS.to_string()),
            FINGERPRINT.into(),
            &HEX.repeat(8),
            None,
            Some(DeviceCodeFallback {
                code: code(),
                replace,
            }),
            now,
        )
    }

    /// A line's Markdown as it reads: no `**`, no escapes, one space
    /// between words.
    fn plain(md: &str) -> String {
        let mut out = String::new();
        let mut chars = md.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\\' if chars.peek().is_some_and(char::is_ascii_punctuation) => {
                    out.extend(chars.next())
                }
                '*' if chars.peek() == Some(&'*') => {
                    chars.next();
                }
                c => out.push(c),
            }
        }
        out.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn texts(p: &Pairing, now: Instant) -> Vec<String> {
        p.lines(&PHONE, None, now)
            .into_iter()
            .map(|l| plain(&l.text))
            .collect()
    }

    #[test]
    fn the_quiet_screen_is_the_six_words_large_and_one_line() {
        let now = Instant::now();
        let mut p = Pairing::default();
        assert!(p.lines(&PHONE, None, now).is_empty(), "nothing to say yet");
        p.open_add();
        assert!(p.adding && !p.waiting());
        let hint = texts(&p, now).join(" ");
        assert!(hint.starts_with("Run flankr pair on the machine"), "{hint}");

        p.show(held(now));
        assert!(p.waiting() && !p.details);
        let lines = p.lines(&PHONE, Some(("pairing…", theme().muted)), now);
        let large: Vec<&str> = lines
            .iter()
            .filter(|l| l.size == WORDS_PT)
            .map(|l| l.text.as_str())
            .collect();
        assert_eq!(large, ["abandon ability able", "about above absent"]);
        assert!(lines.iter().all(|l| l.size <= WORDS_PT));
        let rest: Vec<String> = lines
            .iter()
            .filter(|l| l.size != WORDS_PT)
            .map(|l| plain(&l.text))
            .collect();
        let line = rest[..rest.len() - 1].join(" ");
        assert!(
            lines.iter().any(|l| l.text.contains("**flankr pair**")),
            "the command in bold"
        );
        assert_eq!(
            line,
            "If flankr pair on m4 shows these six words, type yes there."
        );
        assert_eq!(*rest.last().unwrap(), "pairing…", "the note last");
        // Nothing else: no fingerprint, address, countdown or code.
        let all = texts(&p, now).join(" ");
        for hidden in [
            FINGERPRINT,
            "m4.ts.net:7347",
            "expires",
            "flankrkey1",
            "Daemon",
        ] {
            assert!(!all.contains(hidden), "{hidden} in {all}");
        }

        // Cancel ends it, back to Settings.
        assert_eq!(p.cancel(), Input::CancelPairing);
        assert_eq!(p, Pairing::default());
    }

    #[test]
    fn details_show_the_address_the_fingerprint_row_and_time_left() {
        let now = Instant::now();
        let mut p = Pairing::default();
        p.show(held(now));
        assert_eq!(p.fingerprint_row(), None, "no row while Details are shut");
        p.toggle_details();
        let lines = p.lines(&PHONE, None, now + Duration::from_millis(15_500));
        let all: Vec<String> = lines.iter().map(|l| plain(&l.text)).collect();
        let all: Vec<&str> = all.iter().map(String::as_str).collect();
        // The hostname first, then (drawn by the screen) the Fingerprint
        // row, then the time left; the fingerprint is not in the lines.
        assert_eq!(all[0], "m4 at m4.ts.net:7347");
        assert_eq!(all[1], "The link expires in 45 s.", "{all:?}");
        assert!(!all.join(" ").contains("zoo zone"), "{all:?}");
        // The row: after the first line, collapsed, and expanding to the
        // remote details modal's own section, built by the same function.
        let row = p.fingerprint_row().expect("the Fingerprint row");
        assert_eq!((row.at, row.open), (1, false));
        let mut info = remote(4, false);
        info.fingerprint_words = FINGERPRINT.into();
        info.fingerprint_hex = HEX.repeat(8);
        assert_eq!(
            row.section,
            crate::settings::remote_details::fingerprint_markdown(&info),
            "the same section as the details modal's"
        );
        let section = plain(&row.section);
        assert!(
            section.contains("zoo zone youth wrist wrong yellow"),
            "{section}"
        );
        assert!(section.contains("BLAKE2s-256 of its key"), "{section}");
        assert!(section.contains("`3f9a 12c4 3f9a 12c4`"), "{section}");
        p.toggle_fingerprint();
        assert!(p.fingerprint_row().unwrap().open);
        p.toggle_fingerprint();
        assert!(
            !p.fingerprint_row().unwrap().open,
            "a second tap collapses it"
        );
        // Opened again later, it starts collapsed.
        p.toggle_fingerprint();
        p.toggle_details();
        p.toggle_details();
        assert!(!p.fingerprint_row().unwrap().open);
        // The pairing words are not the fingerprint: not repeated here, and
        // no device code without the fallback.
        assert!(!all.join(" ").contains("abandon"));
        assert!(!all.join(" ").contains("flankrkey1"));
        assert!(lines.iter().all(|l| l.size < WORDS_PT));
        let later = texts(&p, now + Duration::from_secs(61)).join(" ");
        assert!(later.contains("The link has expired"), "{later}");
        // Closed again: the quiet screen, and no row.
        p.toggle_details();
        assert!(texts(&p, now)[0].starts_with("abandon"));
        assert_eq!(p.fingerprint_row(), None);
    }

    #[test]
    fn the_fallback_keeps_its_code_in_details() {
        let now = Instant::now();
        let mut p = Pairing::default();
        p.show(fallback(true, false, now));
        assert!(!p.details, "the words come first");
        let quiet = texts(&p, now);
        assert_eq!(quiet[0], "abandon ability able");
        let line = quiet[2..].join(" ");
        assert!(
            line.starts_with("m4 couldn't be asked directly: paste this phone's code from Details"),
            "{line}"
        );
        assert!(!line.contains("flankrkey1"));

        p.toggle_details();
        let lines = p.lines(&PHONE, None, now);
        let all: Vec<String> = lines.iter().map(|l| plain(&l.text)).collect();
        let all: Vec<&str> = all.iter().map(String::as_str).collect();
        assert!(all.contains(&"Paste this code into flankr pair on m4:"));
        let pasted: String = lines
            .iter()
            .filter(|l| l.size == CODE_PT)
            .map(|l| l.text.as_str())
            .collect();
        assert_eq!(pasted, code());
        // No countdown in the fallback.
        assert!(!all.join(" ").contains("expire"));
    }

    #[test]
    fn a_link_without_an_offer_opens_details_and_says_there_are_no_words() {
        let now = Instant::now();
        let mut p = Pairing::default();
        p.show(fallback(false, false, now));
        assert!(p.details, "the code is all there is to do");
        let all = texts(&p, now).join(" ");
        assert!(all.contains("no pairing words to compare"), "{all}");
        assert!(all.contains("flankrkey1"));
        // Closed: one line pointing at the code.
        p.toggle_details();
        let quiet = texts(&p, now).join(" ");
        assert!(
            quiet.starts_with("m4 couldn't be asked directly: paste this phone's code below"),
            "{quiet}"
        );
    }

    #[test]
    fn a_lost_key_says_flankr_pair_replace_is_needed() {
        let now = Instant::now();
        let mut p = Pairing::default();
        p.show(fallback(true, false, now));
        p.toggle_details();
        assert!(!texts(&p, now).join(" ").contains("--replace"));

        p.show(fallback(true, true, now));
        let all = texts(&p, now);
        let at = all
            .iter()
            .position(|t| t.starts_with("Paste this code"))
            .expect("the paste line");
        let warning = all
            .iter()
            .position(|t| t.starts_with("This phone's key for m4 was lost"))
            .expect("the warning");
        assert!(warning < at);
        assert!(all[warning..at].join(" ").contains("flankr pair --replace"));
    }

    #[test]
    fn paired_keeps_the_waiting_modal_until_the_remotes_row_comes() {
        let now = Instant::now();
        let mut p = Pairing::default();
        p.show(held(now));
        p.toggle_details();
        // Paired before its row: the waiting modal stays as it is, Details
        // open, and nothing is ready for the details modal yet.
        let old = remote(4, true);
        assert_eq!(p.paired(std::slice::from_ref(&old)), Some("m4".to_string()));
        assert!(p.waiting() && p.details, "the modal as it was");
        assert!(p.finished.is_some());
        assert_eq!(p.ready(), None, "no row yet");
        // Rows without it: still waiting.
        p.remotes_changed(&[remote(1, false)]);
        assert_eq!(p.ready(), None);
        // Its row comes, beside an older remote of the same name: the new
        // one is taken.
        let mut row = remote(5, false);
        row.name = "m4".into();
        p.remotes_changed(&[old.clone(), row.clone()]);
        assert_eq!(p.ready(), Some((row.id, "Paired with m4".to_string())));
        // Ended for its details modal: on to the overview, it was not the
        // add mode.
        assert!(!p.done());
        assert_eq!(p, Pairing::default());
        // In the add mode, back to Settings.
        p.open_add();
        p.show(held(now));
        p.paired(&[]);
        p.remotes_changed(&[row]);
        assert!(p.ready().is_some() && p.adding);
        assert!(p.done());
        assert!(!p.adding && p.finished.is_none());
        // A failure: the modal closes, and nothing is finished.
        p.show(held(now));
        p.failed();
        assert!(!p.waiting() && p.finished.is_none());
    }

    #[test]
    fn already_paired_is_ready_at_once_and_says_so() {
        let mut p = Pairing::default();
        p.open_add();
        p.show(held(Instant::now()));
        let old = remote(4, true);
        p.remote_updated(old.id, "m4");
        assert!(!p.waiting(), "the modal goes");
        assert_eq!(
            p.ready(),
            Some((old.id, "Already paired with m4".to_string()))
        );
    }

    #[test]
    fn the_fingerprint_row_shows_the_full_fingerprint_the_event_carries() {
        let now = Instant::now();
        let key = [7u8; 32];
        let url = libflankr_proto::link::PairingPayload {
            server_key: key,
            addresses: vec!["127.0.0.1:7347".into()],
            offer: Some([1; libflankr_proto::link::OFFER_LEN]),
        }
        .url()
        .unwrap();
        let read = LinkRead::of(&url).expect("a pairing link");
        assert_eq!(read.address, "127.0.0.1:7347");
        assert_eq!(host_of(&read.address), "127.0.0.1");
        // The client's hex, whatever link was read: it is not recomputed.
        let hex = flankr_companion::fingerprint_hex(&key);
        let grouped = hex_groups(&hex);
        assert_eq!(grouped.len(), 64 + 15, "64 digits in 16 groups");
        let mut p = Pairing::default();
        p.read(Some(read.clone()));
        p.show(Waiting::new(
            read.server.clone(),
            "127.0.0.1:7347".into(),
            Some(WORDS.into()),
            libflankr_proto::link::fingerprint_words_text(&key),
            &hex,
            Some(60),
            None,
            now,
        ));
        p.toggle_details();
        let all = texts(&p, now);
        assert_eq!(all[0], "127.0.0.1 at 127.0.0.1:7347", "{all:?}");
        let groups: Vec<&str> = grouped.split(' ').collect();
        let first = format!("`{}`", groups[..4].join(" "));
        let section = p.fingerprint_row().unwrap().section;
        assert!(section.contains(&first), "{section}");
    }

    #[test]
    fn delete_data_removes_every_remote_the_one_in_use_last() {
        let remotes = [remote(1, false), remote(2, true), remote(3, false)];
        assert_eq!(
            delete_all(&remotes),
            [
                Input::RemoveRemote(RemoteId([1; 8])),
                Input::RemoveRemote(RemoteId([3; 8])),
                Input::RemoveRemote(RemoteId([2; 8])),
            ]
        );
        // None paired: Unpair, which answers Unpaired at once.
        assert_eq!(delete_all(&[]), [Input::Unpair]);
    }

    /// Property-style and degenerate tests of [`clipboard_pairing_link`]:
    /// seeded, so a failure repeats; `FLANKR_LINK_FUZZ_CASES` raises the
    /// number of cases from 300.
    mod link_fuzz {
        use super::super::*;
        use libflankr_proto::link::{OFFER_LEN, PairingPayload};
        use std::time::{Duration, Instant};

        /// xorshift64*: enough for picking cases, with no dependency.
        struct Rng(u64);

        impl Rng {
            fn next(&mut self) -> u64 {
                self.0 ^= self.0 >> 12;
                self.0 ^= self.0 << 25;
                self.0 ^= self.0 >> 27;
                self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
            }
            fn below(&mut self, n: usize) -> usize {
                (self.next() % n as u64) as usize
            }
            fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
                &items[self.below(items.len())]
            }
            fn bytes<const N: usize>(&mut self) -> [u8; N] {
                std::array::from_fn(|_| self.next() as u8)
            }
        }

        fn cases() -> usize {
            std::env::var("FLANKR_LINK_FUZZ_CASES")
                .ok()
                .and_then(|n| n.parse().ok())
                .unwrap_or(300)
        }

        const BASE58: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

        /// Characters that are not ASCII and not base58: what a copy may
        /// bring in besides the listed separators.
        const STRAY: &[char] = &[
            '\u{2595}',
            '\u{258f}',
            '\u{2502}',
            '\u{2500}',
            '\u{a6}',
            '\u{a0}',
            '\u{2026}',
            '\u{e9}',
            '\u{3000}',
            '\u{200b}',
            '\u{1f600}',
        ];

        fn separator(rng: &mut Rng) -> String {
            const FIXED: &[&str] = &[
                "\n",
                "\r\n",
                "\u{2595}\n",
                "\u{2502}\n",
                " \n  ",
                "\t",
                "|",
                "\u{a0}",
            ];
            if rng.below(4) == 0 {
                (0..1 + rng.below(3)).map(|_| *rng.pick(STRAY)).collect()
            } else {
                rng.pick(FIXED).to_string()
            }
        }

        fn hostname(rng: &mut Rng) -> String {
            let label = |rng: &mut Rng| -> String {
                (0..1 + rng.below(10))
                    .map(|_| (b'a' + rng.below(26) as u8) as char)
                    .collect()
            };
            match rng.below(4) {
                0 => format!(
                    "10.{}.{}.{}",
                    rng.below(256),
                    rng.below(256),
                    rng.below(256)
                ),
                1 => label(rng),
                _ => format!("{}.{}.example", label(rng), label(rng)),
            }
        }

        fn random_url(rng: &mut Rng) -> String {
            let addresses = (0..1 + rng.below(3))
                .map(|_| format!("{}:{}", hostname(rng), 1 + rng.below(65535)))
                .collect();
            PairingPayload {
                server_key: rng.bytes::<32>(),
                addresses,
                offer: (rng.below(2) == 0).then(|| rng.bytes::<OFFER_LEN>()),
            }
            .url()
            .unwrap()
        }

        fn prose(rng: &mut Rng) -> String {
            const WORDS: &[&str] = &[
                "Pair", "with", "this", "link", "please", "Paste", "it", "in", "the", "app.",
                "thanks!", "(copied)", "\u{2014}", "ok:", "\n", "  ",
            ];
            (0..rng.below(6))
                .map(|_| *rng.pick(WORDS))
                .collect::<Vec<_>>()
                .join(" ")
        }

        /// `url` cut at a random width, 1 to 80, joined by random separators.
        fn wrap_randomly(rng: &mut Rng, url: &str) -> String {
            let width = 1 + rng.below(80);
            let chars: Vec<char> = url.chars().collect();
            let mut out = String::new();
            for (n, line) in chars.chunks(width).enumerate() {
                if n > 0 {
                    out += &separator(rng);
                }
                out.extend(line);
            }
            out
        }

        /// The URL with one base58 character of its fragment changed to
        /// another.
        fn mutate(rng: &mut Rng, url: &str) -> String {
            let fragment = url.find("#flankr1").unwrap() + "#flankr1".len();
            let at = fragment + rng.below(url.len() - fragment);
            let old = url.as_bytes()[at];
            let new = loop {
                let c = *rng.pick(BASE58);
                if c != old {
                    break c;
                }
            };
            let mut bytes = url.as_bytes().to_vec();
            bytes[at] = new;
            String::from_utf8(bytes).unwrap()
        }

        /// Whatever `text` gives, a `Found` is a link `LinkRead::of` takes.
        fn checked(text: &str) -> ClipboardLink {
            let got = clipboard_pairing_link(text);
            if let ClipboardLink::Found(url) = &got {
                assert!(LinkRead::of(url).is_some(), "{text:?} gave {url:?}");
            }
            got
        }

        #[test]
        fn wrapped_links_with_prose_around_them_are_found() {
            let mut rng = Rng(0x5eed_0001);
            for case in 0..cases() {
                let url = random_url(&mut rng);
                let text = format!(
                    "{}{}{}{}{}",
                    prose(&mut rng),
                    if rng.below(2) == 0 { " " } else { "\n" },
                    wrap_randomly(&mut rng, &url),
                    if rng.below(2) == 0 {
                        ". "
                    } else {
                        "\u{2595}\n"
                    },
                    prose(&mut rng),
                );
                assert_eq!(
                    checked(&text),
                    ClipboardLink::Found(url.clone()),
                    "case {case}: {text:?}"
                );
            }
        }

        #[test]
        fn a_changed_character_is_never_the_original_link() {
            let mut rng = Rng(0x5eed_0002);
            let mut damaged = 0;
            let n = cases();
            for case in 0..n {
                let url = random_url(&mut rng);
                let bad = mutate(&mut rng, &url);
                let text = wrap_randomly(&mut rng, &bad);
                let got = checked(&text);
                assert_ne!(got, ClipboardLink::Found(url), "case {case}: {text:?}");
                damaged += usize::from(got == ClipboardLink::Damaged);
            }
            // Another link validating by chance: 1 in 2^32 a reading.
            assert_eq!(damaged, n);
        }

        #[test]
        fn random_text_never_panics() {
            let mut rng = Rng(0x5eed_0003);
            const PIECES: &[&str] = &[
                "flankr://",
                "flankr",
                "://",
                ":",
                "/",
                "/pair#",
                "pair",
                "#",
                "flankr1",
                "flankrkey1",
                "[",
                "]",
                "fd7a::1",
                ".",
                "-",
                "7347",
                " ",
                "\n",
                "\r",
            ];
            for _ in 0..cases() * 3 {
                let text: String = (0..rng.below(60))
                    .map(|_| match rng.below(4) {
                        0 => rng.pick(PIECES).to_string(),
                        1 => (*rng.pick(BASE58) as char).to_string(),
                        2 => rng.pick(STRAY).to_string(),
                        _ => char::from_u32(rng.below(0x11_0000) as u32)
                            .map(String::from)
                            .unwrap_or_default(),
                    })
                    .collect();
                checked(&text);
            }
            // Some real links among random text.
            for _ in 0..cases() / 10 {
                let url = random_url(&mut rng);
                let text = format!("{}{url}{}", prose(&mut rng), prose(&mut rng));
                assert_eq!(checked(&text), ClipboardLink::Found(url));
            }
        }

        fn a_url() -> String {
            PairingPayload {
                server_key: [7; 32],
                addresses: vec!["host.example:443".into()],
                offer: Some([1; OFFER_LEN]),
            }
            .url()
            .unwrap()
        }

        #[test]
        fn degenerate_texts_are_none() {
            for text in [
                "",
                "flankr://",
                "flankr://host",
                "flankr://host.example:443",
                "flankr://host.example:443/other#flankr1abc",
                "FLANKR://host.example:443/pair#flankr1abc",
                "flankr1",
                "flankrkey1abcdef",
                "/pair#flankr1abc",
                "flankr:/host.example:443/pair#flankr1abc",
            ] {
                assert_eq!(checked(text), ClipboardLink::None, "{text:?}");
            }
        }

        #[test]
        fn degenerate_links_are_damaged() {
            let url = a_url();
            let long_host = "a".repeat(256);
            let device_code = format!(
                "flankr://host.example:443/pair#flankrkey1{}",
                "2".repeat(40)
            );
            let separators_only = "flankr://host.example:443/pair#\u{2595}\n\t | \u{a0}";
            for text in [
                "flankr://:443/pair#".to_string(),
                "flankr://host.example:443/pair#".to_string(),
                "flankr://host.example:443/pair#flankr1".to_string(),
                separators_only.to_string(),
                url[..url.len() - 1].to_string(),
                "flankr://ho st:443/pair#flankr1abc".to_string(),
                "flankr://host_:443/pair#flankr1abc".to_string(),
                "flankr://b\u{fc}cher.example:443/pair#flankr1abc".to_string(),
                "flankr://host:0/pair#flankr1abc".to_string(),
                "flankr://host:99999/pair#flankr1abc".to_string(),
                "flankr://host:44x3/pair#flankr1abc".to_string(),
                format!("flankr://{long_host}:443/pair#flankr1abc"),
                device_code,
                url.replace("host.example", "other.example"),
            ] {
                assert_eq!(checked(&text), ClipboardLink::Damaged, "{text:?}");
            }
            // A host past the head's cap is not read as a link at all.
            let longer = format!("flankr://{}:443/pair#flankr1abc", "a".repeat(400));
            assert_eq!(checked(&longer), ClipboardLink::None);
        }

        #[test]
        fn doubled_and_two_links_give_the_first() {
            let url = a_url();
            assert_eq!(
                checked(&format!("{url}{url}")),
                ClipboardLink::Found(url.clone())
            );
            let mut rng = Rng(0x5eed_0004);
            let other = random_url(&mut rng);
            assert_eq!(
                checked(&format!("{url} or {other}")),
                ClipboardLink::Found(url.clone())
            );
            assert_eq!(
                checked(&format!("{other}\n{url}")),
                ClipboardLink::Found(other)
            );
            // A damaged one before a good one: the good one.
            let cut = &url[..url.len() - 5];
            assert_eq!(
                checked(&format!("{cut}\n\nAgain: {url}")),
                ClipboardLink::Found(url)
            );
        }

        #[test]
        fn long_junk_stays_fast() {
            let url = a_url();
            let junk: String = "Paste it\u{2595}\n".repeat(1000);
            let separators: String = " \u{2595}\n".repeat(3400);
            let started = Instant::now();
            assert_eq!(
                checked(&format!("{url}{junk}")),
                ClipboardLink::Found(url.clone())
            );
            let (a, b) = url.split_at(url.len() / 2);
            assert_eq!(
                checked(&format!("{a}{separators}{b}")),
                ClipboardLink::Found(url.clone())
            );
            let (a, b) = url.split_at(12);
            assert_eq!(
                checked(&format!("{a}{separators}{b}{junk}")),
                ClipboardLink::Found(url.clone())
            );
            // Many starts, each damaged.
            let many = "flankr://h:1/pair#flankr1abc ".repeat(500);
            assert_eq!(checked(&many), ClipboardLink::Damaged);
            let base58_junk: String = "Z".repeat(10_000);
            assert_eq!(
                checked(&format!("{url}{base58_junk}")),
                ClipboardLink::Found(url.clone())
            );
            assert!(
                started.elapsed() < Duration::from_secs(20),
                "took {:?}",
                started.elapsed()
            );
        }
    }
}
