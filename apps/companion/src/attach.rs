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

use crate::menu::IMAGES_MAX;
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

#[cfg(test)]
mod tests {
    use super::*;

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
