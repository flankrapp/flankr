//! The pairing string and the device code (plan 02, amendment of
//! 23:07 EDT: pairing by public keys, with no shared secret).
//!
//! The pairing string's payload, as bytes: version (1 byte, `3`, or `2`
//! without an offer), the server's public key (32), addresses (1-byte
//! count, then each a 1-byte length and `host:port` in UTF-8), in version
//! 3 the offer (16 bytes; plan 20261028_01, the 13:59 and 14:37 EDT
//! amendments), and a checksum: the first 4 bytes of BLAKE2s-256 over
//! every field before it. The text form is `flankr1` followed by the
//! payload in base58 with the Bitcoin alphabet. It holds no secret, so it
//! may be shown, logged or kept freely: the offer only names one
//! `flankr pair` run, and the six pairing words bind it.
//!
//! A payload with an offer is written as version 3, one without as version
//! 2; both are read. A decoder older than version 3 refuses a version 3
//! payload as unknown, so the phone is updated before the daemon.
//!
//! The pairing URL (plan 20261022_06, decision 1, as amended at 23:07 EDT)
//! is `flankr://<host>:<port>/pair#<pairing string>`: the address a phone
//! reaches the server at, and the string. A QR code carries it.
//!
//! The device code is what the device shows for the user to paste into
//! `flankr pair`: version (1 byte, `1`), the device's public key (32), its
//! name (1-byte length, then 1 to 32 bytes of ASCII letters, digits, `-`,
//! `_` or `.`), and a checksum: the first 4 bytes of BLAKE2s-256 over the
//! **server's** public key, then every field before it. The text form is
//! `flankrkey1` and base58. Because the checksum covers the server's key, a
//! code made for another server, or mistyped, fails it.
//!
//! The encodings are libhardline-proto's. [`PairingPayload`] and
//! [`DeviceCode`] keep their fields and their methods' old signatures, fixed
//! to [`FLANKR_PROFILE`](super::FLANKR_PROFILE), and convert to and from
//! libhardline's types with `From`. [`PairingStringError`] keeps its
//! variants and its messages, which name flankr's prefixes.

use std::fmt;

use libhardline::proto as hl;

use super::{FLANKR_PROFILE, KEY_LEN};

pub use libhardline::proto::{
    DEVICE_CODE_VERSION, DEVICE_NAME_MAX, OFFER_LEN, PAIRING_VERSION, PAIRING_VERSION_NO_OFFER,
};

/// The pairing string's prefix.
pub const PAIRING_PREFIX: &str = "flankr1";
/// The pairing URL's scheme.
pub const PAIRING_URL_SCHEME: &str = "flankr";

/// The device code's prefix.
pub const DEVICE_CODE_PREFIX: &str = "flankrkey1";

#[cfg(test)]
const CHECKSUM_LEN: usize = 4;

/// What a pairing string carries: the server's key and where to reach it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingPayload {
    /// The server's static X25519 public key.
    pub server_key: [u8; KEY_LEN],
    /// The server's addresses, `host:port`, in order of preference.
    pub addresses: Vec<String>,
    /// The `flankr pair` run's random offer, which a pairing request must
    /// carry; `None` in a version 2 payload.
    pub offer: Option<[u8; OFFER_LEN]>,
}

/// What a device code carries: the device's key and the name it chose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCode {
    /// The device's static X25519 public key.
    pub key: [u8; KEY_LEN],
    /// 1 to 32 bytes of ASCII letters, digits, `-`, `_` or `.`.
    pub name: String,
}

/// Why a pairing string, a pairing URL or a device code was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PairingStringError {
    /// The text does not start with `flankr1` (a pairing string) or
    /// `flankrkey1` (a device code).
    BadPrefix,
    /// A character outside the base58 alphabet.
    BadBase58,
    /// The checksum does not match. For a device code, it may also have
    /// been made for another server.
    BadChecksum,
    /// A version this crate does not know.
    UnknownVersion(u8),
    /// The payload is truncated, has trailing bytes or invalid UTF-8.
    Malformed,
    /// Encoding: more than 255 addresses.
    TooManyAddresses,
    /// Encoding: an address longer than 255 bytes.
    AddressTooLong,
    /// A pairing URL that is not `flankr://<host>:<port>/pair#<string>`
    /// with a host the payload carries, or a payload with no address.
    BadUrl,
    /// A device name that is not 1 to 32 bytes of ASCII letters, digits,
    /// `-`, `_` or `.`.
    BadName,
}

impl fmt::Display for PairingStringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = match self {
            PairingStringError::BadPrefix => {
                "it does not start with \"flankr1\" (or \"flankrkey1\" for a device code)"
            }
            PairingStringError::BadBase58 => "it has a character outside base58",
            PairingStringError::BadChecksum => "its checksum does not match",
            PairingStringError::UnknownVersion(v) => {
                return write!(f, "invalid pairing string: unknown version {v}");
            }
            PairingStringError::Malformed => "its payload is malformed",
            PairingStringError::TooManyAddresses => "there are more than 255 addresses",
            PairingStringError::AddressTooLong => "an address is longer than 255 bytes",
            PairingStringError::BadUrl => {
                "it is not a flankr://<host>:<port>/pair# URL for one of its addresses"
            }
            PairingStringError::BadName => {
                "the name is not 1 to 32 ASCII letters, digits, '-', '_' or '.'"
            }
        };
        write!(f, "invalid pairing string: {what}")
    }
}

impl std::error::Error for PairingStringError {}

impl From<hl::PairingStringError> for PairingStringError {
    fn from(e: hl::PairingStringError) -> Self {
        match e {
            hl::PairingStringError::BadPrefix => PairingStringError::BadPrefix,
            hl::PairingStringError::BadBase58 => PairingStringError::BadBase58,
            hl::PairingStringError::BadChecksum => PairingStringError::BadChecksum,
            hl::PairingStringError::UnknownVersion(v) => PairingStringError::UnknownVersion(v),
            hl::PairingStringError::TooManyAddresses => PairingStringError::TooManyAddresses,
            hl::PairingStringError::AddressTooLong => PairingStringError::AddressTooLong,
            hl::PairingStringError::BadUrl => PairingStringError::BadUrl,
            hl::PairingStringError::BadName => PairingStringError::BadName,
            // `Malformed`, and any error a later libhardline adds.
            _ => PairingStringError::Malformed,
        }
    }
}

impl From<PairingPayload> for hl::PairingPayload {
    fn from(p: PairingPayload) -> Self {
        hl::PairingPayload {
            server_key: p.server_key,
            addresses: p.addresses,
            offer: p.offer,
        }
    }
}

impl From<hl::PairingPayload> for PairingPayload {
    fn from(p: hl::PairingPayload) -> Self {
        PairingPayload {
            server_key: p.server_key,
            addresses: p.addresses,
            offer: p.offer,
        }
    }
}

impl From<DeviceCode> for hl::DeviceCode {
    fn from(d: DeviceCode) -> Self {
        hl::DeviceCode {
            key: d.key,
            name: d.name,
        }
    }
}

impl From<hl::DeviceCode> for DeviceCode {
    fn from(d: hl::DeviceCode) -> Self {
        DeviceCode {
            key: d.key,
            name: d.name,
        }
    }
}

impl PairingPayload {
    fn hl(&self) -> hl::PairingPayload {
        self.clone().into()
    }

    /// The binary payload, with its checksum.
    pub fn to_bytes(&self) -> Result<Vec<u8>, PairingStringError> {
        Ok(self.hl().to_bytes()?)
    }

    /// Reads a binary payload: checksum first, then version, then fields.
    /// Version 3 has an offer, version 2 none (`offer` is `None`); any other
    /// version, such as 1 (with a secret), is
    /// [`PairingStringError::UnknownVersion`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PairingStringError> {
        Ok(hl::PairingPayload::from_bytes(bytes)?.into())
    }

    /// The text form: `flankr1` and the payload in base58.
    pub fn encode(&self) -> Result<String, PairingStringError> {
        Ok(self.hl().encode(&FLANKR_PROFILE)?)
    }

    /// The pairing URL: `flankr://<host>:<port>/pair#<string>`, where
    /// `<host>:<port>` is the first address that is not loopback (with
    /// `tailscale serve`, the Tailscale name), or the first address if all
    /// are.
    pub fn url(&self) -> Result<String, PairingStringError> {
        Ok(self.hl().url(&FLANKR_PROFILE)?)
    }

    /// Whether any address can be reached from another machine.
    pub fn has_remote_address(&self) -> bool {
        self.hl().has_remote_address()
    }

    /// Reads the text form or the pairing URL. Surrounding whitespace, as
    /// from a paste or a scan, is ignored. A URL's host must be one of the
    /// payload's addresses.
    pub fn decode(s: &str) -> Result<Self, PairingStringError> {
        Ok(hl::PairingPayload::decode(&FLANKR_PROFILE, s)?.into())
    }
}

impl std::str::FromStr for PairingPayload {
    type Err = PairingStringError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::decode(s)
    }
}

impl DeviceCode {
    /// Whether `name` may be a device's name: 1 to 32 bytes of ASCII
    /// letters, digits, `-`, `_` or `.`.
    pub fn valid_name(name: &str) -> bool {
        hl::DeviceCode::valid_name(name)
    }

    /// The text form for the server whose key is `server_key`:
    /// `flankrkey1` and the code in base58.
    pub fn encode(&self, server_key: &[u8; KEY_LEN]) -> Result<String, PairingStringError> {
        Ok(hl::DeviceCode::from(self.clone()).encode(&FLANKR_PROFILE, server_key)?)
    }

    /// Reads the text form, checking it against `server_key`.
    /// Surrounding whitespace is ignored. A code made for another server
    /// fails as [`PairingStringError::BadChecksum`].
    pub fn decode(s: &str, server_key: &[u8; KEY_LEN]) -> Result<Self, PairingStringError> {
        Ok(hl::DeviceCode::decode(&FLANKR_PROFILE, s, server_key)?.into())
    }
}

/// The body of `bytes`, once its trailing checksum matches BLAKE2s-256 of
/// `salt` then the body: the reference the tests check libhardline against.
#[cfg(test)]
fn checked<'a>(bytes: &'a [u8], salt: &[u8]) -> Result<&'a [u8], PairingStringError> {
    if bytes.len() < 1 + CHECKSUM_LEN {
        return Err(PairingStringError::Malformed);
    }
    let (body, sum) = bytes.split_at(bytes.len() - CHECKSUM_LEN);
    if blake2s_prefix(&[salt, body]) != sum {
        return Err(PairingStringError::BadChecksum);
    }
    Ok(body)
}

/// The first 4 bytes of BLAKE2s-256 over `parts`, in order.
#[cfg(test)]
fn blake2s_prefix(parts: &[&[u8]]) -> [u8; CHECKSUM_LEN] {
    use blake2::{Blake2s256, Digest};
    let mut hasher = Blake2s256::new();
    for part in parts {
        hasher.update(part);
    }
    let digest = hasher.finalize();
    [digest[0], digest[1], digest[2], digest[3]]
}

#[cfg(test)]
struct Reader<'a>(&'a [u8]);

#[cfg(test)]
impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], PairingStringError> {
        if self.0.len() < n {
            return Err(PairingStringError::Malformed);
        }
        let (head, tail) = self.0.split_at(n);
        self.0 = tail;
        Ok(head)
    }

    fn u8(&mut self) -> Result<u8, PairingStringError> {
        Ok(self.take(1)?[0])
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], PairingStringError> {
        self.take(N)?
            .try_into()
            .map_err(|_| PairingStringError::Malformed)
    }

    fn string(&mut self) -> Result<String, PairingStringError> {
        let len = usize::from(self.u8()?);
        String::from_utf8(self.take(len)?.to_vec()).map_err(|_| PairingStringError::Malformed)
    }

    fn end(&self) -> Result<(), PairingStringError> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(PairingStringError::Malformed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(offer: Option<[u8; OFFER_LEN]>) -> PairingPayload {
        PairingPayload {
            server_key: [7; KEY_LEN],
            addresses: vec!["box.tail1234.ts.net:7347".into(), "127.0.0.1:7347".into()],
            offer,
        }
    }

    /// The decoder of 0.4.3 and before, which read version 2 only: kept
    /// here to show what an older app does with a version 3 link.
    fn decode_v2_only(bytes: &[u8]) -> Result<PairingPayload, PairingStringError> {
        let mut r = Reader(checked(bytes, &[])?);
        let version = r.u8()?;
        if version != 2 {
            return Err(PairingStringError::UnknownVersion(version));
        }
        let server_key = r.array()?;
        let count = r.u8()?;
        let addresses = (0..count)
            .map(|_| r.string())
            .collect::<Result<Vec<_>, _>>()?;
        r.end()?;
        Ok(PairingPayload {
            server_key,
            addresses,
            offer: None,
        })
    }

    #[test]
    fn an_offer_is_written_as_version_3_and_read_back() {
        let p = payload(Some(std::array::from_fn(|i| i as u8)));
        let bytes = p.to_bytes().unwrap();
        assert_eq!(bytes[0], PAIRING_VERSION);
        assert_eq!(PAIRING_VERSION, 3);
        // version, key, count, two addresses, offer, checksum
        assert_eq!(
            bytes.len(),
            1 + KEY_LEN + 1 + 25 + 15 + OFFER_LEN + CHECKSUM_LEN
        );
        assert_eq!(PairingPayload::from_bytes(&bytes).unwrap(), p);
        assert_eq!(PairingPayload::decode(&p.url().unwrap()).unwrap(), p);
    }

    #[test]
    fn no_offer_is_written_as_version_2_and_read_back() {
        let p = payload(None);
        let bytes = p.to_bytes().unwrap();
        assert_eq!(bytes[0], PAIRING_VERSION_NO_OFFER);
        assert_eq!(PairingPayload::from_bytes(&bytes).unwrap(), p);
        // The old decoder reads it too.
        assert_eq!(decode_v2_only(&bytes).unwrap(), p);
    }

    #[test]
    fn a_version_2_decoder_refuses_version_3_as_unknown() {
        let bytes = payload(Some([9; OFFER_LEN])).to_bytes().unwrap();
        assert_eq!(
            decode_v2_only(&bytes),
            Err(PairingStringError::UnknownVersion(3))
        );
    }

    #[test]
    fn a_version_3_payload_without_its_offer_is_malformed() {
        // A version 2 body relabelled as version 3, with a fresh checksum.
        let mut body = payload(None).to_bytes().unwrap();
        body.truncate(body.len() - CHECKSUM_LEN);
        body[0] = PAIRING_VERSION;
        let sum = blake2s_prefix(&[&body]);
        body.extend_from_slice(&sum);
        assert_eq!(
            PairingPayload::from_bytes(&body),
            Err(PairingStringError::Malformed)
        );
        // And a version 2 payload with 16 trailing bytes is malformed too.
        let mut body = payload(Some([1; OFFER_LEN])).to_bytes().unwrap();
        body.truncate(body.len() - CHECKSUM_LEN);
        body[0] = PAIRING_VERSION_NO_OFFER;
        let sum = blake2s_prefix(&[&body]);
        body.extend_from_slice(&sum);
        assert_eq!(
            PairingPayload::from_bytes(&body),
            Err(PairingStringError::Malformed)
        );
    }

    #[test]
    fn other_versions_are_unknown() {
        for v in [0u8, 1, 4, 255] {
            let mut body = payload(None).to_bytes().unwrap();
            body.truncate(body.len() - CHECKSUM_LEN);
            body[0] = v;
            let sum = blake2s_prefix(&[&body]);
            body.extend_from_slice(&sum);
            assert_eq!(
                PairingPayload::from_bytes(&body),
                Err(PairingStringError::UnknownVersion(v))
            );
        }
    }
}
