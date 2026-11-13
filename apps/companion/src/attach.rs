//! Images picked for a message or a terminal
//! (docs/design/20261113_01_ADD_IMAGES_PLAN.md, decisions 6 to 9): the
//! system chooser's request, and its reply sorted into the images the
//! client takes (`Input::AttachImage`) and the ones refused.
//!
//! The bytes go up as picked: no conversion and no EXIF changes (decision
//! 9). Nothing of the bytes or the items' names is logged; the app logs
//! counts only.

use libflankr_blob::{IMAGE_MAX, IMAGE_MIN, ImageType};
use libmorph::media::{
    CameraEntry, CameraPermission, Delivery, MediaData, MediaKind, MediaRequest, Picker,
};

use std::sync::Arc;

use crate::menu::IMAGES_MAX;
use crate::prompt::Thumb;
use crate::settings::copy;

/// Where the images picked go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// The message card: each its `[image N]` token in the text.
    Message,
    /// The open terminal: pasted as paths once all are uploaded
    /// (decision 14).
    Terminal,
}

/// The chooser's request (decision 6): the four image types, the gallery
/// and files with a camera entry once CAMERA is granted (never asked for
/// here, decision 7), up to the images still free, 20 MiB each, as bytes.
pub fn request(attached: usize) -> MediaRequest {
    let mut r = MediaRequest::default();
    r.kind = MediaKind::Mime(
        ["image/png", "image/jpeg", "image/webp", "image/gif"]
            .map(String::from)
            .to_vec(),
    );
    r.camera = CameraEntry::Photo;
    r.camera_permission = CameraPermission::IfGranted;
    r.picker = Picker::Chooser;
    r.max_items = IMAGES_MAX.saturating_sub(attached).max(1) as u16;
    r.max_bytes = IMAGE_MAX;
    r.delivery = Delivery::Bytes;
    r
}

/// Why a picked item was not attached.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// Over 20 MiB.
    TooLarge,
    /// Not PNG, JPEG, WebP or GIF by its bytes (a HEIC photo, a text
    /// file).
    NotImage,
    /// Too short to be an image, or not readable.
    Unreadable,
}

impl Refused {
    /// The notice shown for it.
    pub fn line(self) -> &'static str {
        match self {
            Refused::TooLarge => copy::ATTACH_TOO_LARGE.text,
            Refused::NotImage => copy::ATTACH_NOT_IMAGE.text,
            Refused::Unreadable => copy::ATTACH_UNREADABLE.text,
        }
    }
}

/// A reply's items, sorted in their order.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Sorted {
    /// The bytes and the sniffed MIME type of each image taken.
    pub images: Vec<(Vec<u8>, &'static str)>,
    pub refused: Vec<Refused>,
}

impl Sorted {
    /// The notice for the first item refused, if any was.
    pub fn notice(&self) -> Option<&'static str> {
        self.refused.first().map(|r| r.line())
    }
}

/// Sorts the items' contents (decision 8): under 16 bytes refused as
/// unreadable, over 20 MiB as too large, a type the sniffer does not know
/// as not an image; the rest taken with their sniffed type.
pub fn sort(items: impl IntoIterator<Item = MediaData>) -> Sorted {
    let mut out = Sorted::default();
    for data in items {
        match data {
            MediaData::Bytes(bytes) => {
                let len = bytes.len() as u64;
                if len < IMAGE_MIN {
                    out.refused.push(Refused::Unreadable);
                } else if len > IMAGE_MAX {
                    out.refused.push(Refused::TooLarge);
                } else if let Some(t) = ImageType::sniff(&bytes) {
                    out.images.push((bytes, t.mime()));
                } else {
                    out.refused.push(Refused::NotImage);
                }
            }
            MediaData::TooLarge { .. } => out.refused.push(Refused::TooLarge),
            // Asked for as bytes, a file is not expected; it is not read.
            _ => out.refused.push(Refused::Unreadable),
        }
    }
    out
}

/// An image sent to the client and not yet numbered (decisions 11, 12
/// and 14): matched to the next new number of its type and size.
#[derive(Clone, Debug)]
pub struct Pending {
    pub mime: String,
    pub size: u64,
    /// A PNG's thumbnail; none for the other types, or a PNG that does
    /// not decode.
    pub thumb: Option<Arc<Thumb>>,
    pub target: Target,
}

impl Pending {
    /// `bytes` of type `mime` going to `target`; a PNG for the message
    /// card is decoded for its tile.
    pub fn of(bytes: &[u8], mime: &str, target: Target) -> Pending {
        let thumb = (target == Target::Message && mime == "image/png")
            .then(|| crate::preview::image::decode_png(bytes).ok())
            .flatten()
            .and_then(|img| Thumb::from_rgba(img.width, img.height, &img.pixels))
            .map(Arc::new);
        Pending {
            mime: mime.to_string(),
            size: bytes.len() as u64,
            thumb,
            target,
        }
    }
}

/// How long a refusal's line stays over the terminal.
pub const NOTICE_FOR: std::time::Duration = std::time::Duration::from_secs(4);

/// The images picked for the terminal (decision 14): how many were sent
/// to the client, and the numbers they took as they came.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TermBatch {
    /// The session (pane) they go to.
    pub pane: String,
    pub expected: usize,
    pub numbers: Vec<u8>,
}

/// Where a terminal batch is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BatchState {
    Uploading,
    /// Every one is uploaded: paste these numbers.
    Ready(Vec<u8>),
    /// One failed: its line.
    Failed(String),
}

impl TermBatch {
    pub fn new(pane: String, expected: usize) -> TermBatch {
        TermBatch {
            pane,
            expected,
            numbers: Vec::new(),
        }
    }

    fn mine<'a>(
        &'a self,
        attached: &'a [flankr_companion::Attachment],
    ) -> impl Iterator<Item = &'a flankr_companion::Attachment> {
        attached
            .iter()
            .filter(|a| a.session == self.pane && self.numbers.contains(&a.n))
    }

    /// Its state against the client's attachments.
    pub fn state(&self, attached: &[flankr_companion::Attachment]) -> BatchState {
        if let Some(line) = self.mine(attached).find_map(|a| a.failed.clone()) {
            return BatchState::Failed(line);
        }
        let ready = self.mine(attached).filter(|a| a.ready).count();
        if self.numbers.len() == self.expected && ready == self.expected {
            BatchState::Ready(self.numbers.clone())
        } else {
            BatchState::Uploading
        }
    }

    /// The notice while it uploads: "Uploading image 1 of 2…".
    pub fn line(&self, attached: &[flankr_companion::Attachment]) -> String {
        let ready = self.mine(attached).filter(|a| a.ready).count();
        let k = (ready + 1).min(self.expected).to_string();
        copy::ATTACH_TERMINAL_UPLOADING
            .fill(None, &[("k", &k), ("m", &self.expected.to_string())])
            .trim()
            .to_string()
    }
}

/// `text` with image `n`'s token `[image N]` put at byte `at` (moved back
/// to a character boundary), with a space either side unless one is there
/// or it starts the text (decision 12). Returns the text and where the
/// cursor goes, after the token and its space.
pub fn with_token(text: &str, at: usize, n: u8) -> (String, usize) {
    let at = (0..=at.min(text.len()))
        .rev()
        .find(|&i| text.is_char_boundary(i))
        .unwrap_or(0);
    let (before, after) = text.split_at(at);
    let mut token = String::new();
    if !before.is_empty() && !before.ends_with(char::is_whitespace) {
        token.push(' ');
    }
    token.push_str(&flankr_companion::blob::token(n));
    token.push(' ');
    let after = after.strip_prefix(' ').unwrap_or(after);
    let cursor = before.len() + token.len();
    (format!("{before}{token}{after}"), cursor)
}

/// Why a send of `text` will be refused while an image it names is not
/// uploaded (decision 13), for the card's notes: the first such image
/// named, in its number order, failed or still uploading. The client
/// refuses the send itself; this only says why under the card.
pub fn send_refusal(text: &str, attached: &[&flankr_companion::Attachment]) -> Option<String> {
    let mut named: Vec<&&flankr_companion::Attachment> = attached
        .iter()
        .filter(|a| text.contains(&flankr_companion::blob::token(a.n)))
        .collect();
    named.sort_by_key(|a| a.n);
    if let Some(a) = named.iter().find(|a| a.failed.is_some()) {
        return Some(
            copy::ATTACH_FAILED
                .fill(None, &[("n", &a.n.to_string())])
                .trim()
                .to_string(),
        );
    }
    named.iter().find(|a| !a.ready).map(|a| {
        copy::ATTACH_UPLOADING
            .fill(None, &[("n", &a.n.to_string())])
            .trim()
            .to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attachment(n: u8, ready: bool, failed: bool) -> flankr_companion::Attachment {
        flankr_companion::Attachment {
            n,
            session: "w1-t1-p1".into(),
            mime: "image/png".into(),
            size: 100,
            transfer: 0,
            ready,
            failed: failed.then(|| "Upload failed.".to_string()),
        }
    }

    #[test]
    fn a_token_goes_in_at_the_cursor_with_a_space_either_side() {
        assert_eq!(with_token("", 0, 1), ("[image 1] ".to_string(), 10));
        assert_eq!(
            with_token("look at this", 12, 2),
            ("look at this [image 2] ".to_string(), 23)
        );
        assert_eq!(
            with_token("see  please", 4, 1),
            ("see [image 1] please".to_string(), 14)
        );
        assert_eq!(with_token("abc", 1, 3), ("a [image 3] bc".to_string(), 12));
        // Past the end, or inside a character: moved back.
        assert_eq!(with_token("é", 1, 1).0, "[image 1] é");
        assert_eq!(with_token("ab", 99, 1).0, "ab [image 1] ");
    }

    #[test]
    fn a_terminal_batch_pastes_once_every_image_is_uploaded() {
        let pane = "w1-t1-p1".to_string();
        let mut b = TermBatch::new(pane, 2);
        let mut list = vec![attachment(1, false, false)];
        b.numbers.push(1);
        assert_eq!(b.state(&list), BatchState::Uploading);
        assert_eq!(b.line(&list), "Uploading image 1 of 2\u{2026}");
        list[0].ready = true;
        assert_eq!(b.state(&list), BatchState::Uploading, "one still to come");
        assert_eq!(b.line(&list), "Uploading image 2 of 2\u{2026}");
        list.push(attachment(2, true, false));
        b.numbers.push(2);
        assert_eq!(b.state(&list), BatchState::Ready(vec![1, 2]));
        // The message's own image is not the batch's.
        list.push(attachment(3, false, false));
        assert_eq!(b.state(&list), BatchState::Ready(vec![1, 2]));
        list[1].failed = Some("Upload failed.".into());
        assert_eq!(b.state(&list), BatchState::Failed("Upload failed.".into()));
    }

    #[test]
    fn a_send_names_an_image_still_uploading_or_failed() {
        let (one, two, three) = (
            attachment(1, true, false),
            attachment(2, false, false),
            attachment(3, false, true),
        );
        let all = [&one, &two, &three];
        assert_eq!(send_refusal("hi [image 1]", &all), None);
        assert_eq!(
            send_refusal("[image 2] and [image 1]", &all).as_deref(),
            Some("Image 2 is still uploading.")
        );
        assert_eq!(
            send_refusal("[image 2] [image 3]", &all).as_deref(),
            Some("Image 3 failed; remove it or attach it again.")
        );
        // Not named in the text: not the send's concern.
        assert_eq!(send_refusal("no images", &all), None);
    }

    /// A PNG's signature and IHDR's start: enough for the sniffer.
    fn png() -> Vec<u8> {
        let mut b = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        b.extend_from_slice(&[0, 0, 0, 13, b'I', b'H', b'D', b'R', 0, 0, 0, 1]);
        b
    }

    #[test]
    fn a_png_is_taken_and_short_text_and_large_items_are_refused() {
        let jpeg = {
            let mut b = vec![0xFF, 0xD8, 0xFF, 0xE0];
            b.resize(64, 0);
            b
        };
        let sorted = sort([
            MediaData::Bytes(png()),
            MediaData::Bytes(vec![0x89; 12]),
            MediaData::Bytes(b"just some text, not an image".to_vec()),
            MediaData::TooLarge {
                size: Some(IMAGE_MAX + 1),
            },
            MediaData::Bytes(jpeg.clone()),
        ]);
        assert_eq!(
            sorted.images,
            [(png(), "image/png"), (jpeg, "image/jpeg")],
            "taken in order, with their sniffed types"
        );
        assert_eq!(
            sorted.refused,
            [Refused::Unreadable, Refused::NotImage, Refused::TooLarge]
        );
        assert_eq!(sorted.notice(), Some("This image can't be read."));
        assert_eq!(
            Refused::NotImage.line(),
            "flankr takes PNG, JPEG, WebP and GIF images."
        );
        assert_eq!(Refused::TooLarge.line(), "Too large (over 20 MB).");
        assert_eq!(sort([MediaData::Bytes(png())]).notice(), None);
    }

    #[test]
    fn a_heic_photo_is_refused_not_converted() {
        // `ftypheic` at 4: an ISO media file, not one of the four.
        let mut heic = vec![0, 0, 0, 24];
        heic.extend_from_slice(b"ftypheic");
        heic.resize(256, 0);
        assert_eq!(sort([MediaData::Bytes(heic)]).refused, [Refused::NotImage]);
    }

    #[test]
    fn the_request_asks_for_the_four_types_and_the_images_still_free() {
        let r = request(1);
        assert_eq!(
            r.kind,
            MediaKind::Mime(vec![
                "image/png".into(),
                "image/jpeg".into(),
                "image/webp".into(),
                "image/gif".into()
            ])
        );
        assert_eq!(r.camera, CameraEntry::Photo);
        assert_eq!(r.camera_permission, CameraPermission::IfGranted);
        assert_eq!(r.picker, Picker::Chooser);
        assert_eq!(r.max_items, 3);
        assert_eq!(r.max_bytes, 20 * 1024 * 1024);
        assert_eq!(r.delivery, Delivery::Bytes);
        assert_eq!(request(0).max_items, 4);
    }
}
