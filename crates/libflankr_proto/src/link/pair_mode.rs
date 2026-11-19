//! The pairing request: a phone asks a daemon in pair mode to add its key
//! (docs/design/20261028_01_MULTIPLE_REMOTES_PLAN.md, the 14:37 EDT
//! amendment's item 1, the 14:40 EDT amendment and the 14:44 EDT amendment).
//!
//! The phone has scanned a `flankr pair` run's link: it knows the daemon's
//! static key and the run's offer. It makes a new static key `D` for this
//! daemon and connects with **`Noise_NK_25519_ChaChaPoly_BLAKE2s`**: the
//! daemon's key is pinned from the link, so only that daemon can read the
//! request; the phone has no static key in this handshake.
//!
//! On the wire, the client opens with the mode byte [`MODE_PAIR`] (`0x03`);
//! the prologue is `b"flankr.link/1"` followed by it. Every Noise message
//! is a frame (a 2-byte big-endian length, then the message), as in `IK`.
//!
//! 1. `-> e, es` carries a [`PairRequest`]: `D`'s public key, the device's
//!    name and the offer.
//! 2. `<- e, ee` carries a [`PairAck`]: accepted (with the seconds left on
//!    the offer), or refused with a [`PairRefusal`] (a wrong offer, an
//!    expired one, a request already held, a malformed request).
//! 3. After an accepted ack both sides show the six
//!    [`pairing_words`](super::pairing_words) of the daemon's key, the offer
//!    and `D`. The connection stays open as a [`LinkStream`]; the daemon
//!    sends one [`PairResult`] when the owner has answered: added, refused
//!    or expired. The phone cancels by closing the connection.
//!
//! The NK handshake does not prove that the phone holds `D`'s private key;
//! the owner's check of the words binds `D` to this request, and the
//! phone's first `IK` connection afterwards proves the key.
//!
//! Each message is versioned by its first byte, [`PAIR_MESSAGE_VERSION`]:
//!
//! * request: version, `D` (32), name (1-byte length, then the name, as
//!   [`DeviceCode::valid_name`] allows), offer (16);
//! * ack: version, `0x00` accepted and the seconds left (2 bytes,
//!   big-endian), or `0x01` refused and the reason (1 byte:
//!   [`PairRefusal::code`]);
//! * result: version, and `0x00` added, `0x01` refused or `0x02` expired.
//!
//! The result travels as one message on the stream (the kind nibble is
//! JSON-RPC's, `0`, though the bytes are this encoding: a pairing session
//! carries nothing else).
//!
//! The messages are libhardline-proto's, re-exported; the handshakes are
//! libhardline's tokio adapter, fixed to [`FLANKR_PROFILE`]. libhardline
//! gives the two sessions as their own types; here each is a
//! [`LinkStream`], as before.

use libhardline::tokio as hlt;
use tokio::io::{AsyncRead, AsyncWrite};

use super::handshake::Accepted;
use super::{FLANKR_PROFILE, KEY_LEN, LinkError, LinkStream, StaticKey};

pub use libhardline::proto::{
    MODE_PAIR, PAIR_MESSAGE_VERSION, PAIR_PATTERN, PairAck, PairMessageError, PairRefusal,
    PairRequest, PairResult,
};
pub use libhardline::{PAIR_TIMEOUT_DEFAULT_SECS, PAIR_TIMEOUT_MAX_SECS};

/// A new random offer for a `flankr pair` run, from the operating system's
/// random source.
pub fn generate_offer() -> Result<[u8; super::OFFER_LEN], LinkError> {
    libhardline::generate_offer()
}

/// What the phone has after [`request_pairing`]: the daemon's ack, and the
/// session on which an accepted request's [`PairResult`] will come.
pub struct PairResponse<S> {
    /// The daemon's answer.
    pub ack: PairAck,
    /// The session. Its [`peer_key`](LinkStream::peer_key) is the daemon's
    /// key, which NK authenticated. After a refusal, drop it: it is closed.
    pub stream: LinkStream<S>,
}

/// Phone: sends a pairing request to the daemon whose static key is
/// `server_key` (from the link), over `NK`, and reads its ack.
///
/// A daemon with another key cannot read the request, and a daemon that is
/// not in pair mode, both close the connection with no reply
/// ([`LinkError::Closed`]); a reply that does not decrypt is
/// [`LinkError::Noise`]. A message that does not read is
/// [`LinkError::Pairing`].
pub async fn request_pairing<S: AsyncRead + AsyncWrite + Unpin>(
    io: S,
    server_key: &[u8; KEY_LEN],
    request: &PairRequest,
) -> Result<PairResponse<S>, LinkError> {
    // As before: a request that does not encode is refused before sending.
    request.to_bytes()?;
    Ok(
        match hlt::request_pairing_over(io, &FLANKR_PROFILE, server_key, request).await? {
            hlt::PairAnswer::Accepted(pending) => PairResponse {
                ack: PairAck::Accepted {
                    expires_in_secs: pending.expires_in_secs(),
                },
                stream: LinkStream::pair_pending(pending, *server_key),
            },
            hlt::PairAnswer::Refused(reason) => PairResponse {
                ack: PairAck::Refused(reason),
                stream: LinkStream::closed(*server_key),
            },
        },
    )
}

/// Phone: waits for the daemon's [`PairResult`] on an accepted session.
pub async fn recv_pair_result<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut LinkStream<S>,
) -> Result<PairResult, LinkError> {
    let msg = stream.recv_message().await?;
    Ok(PairResult::from_bytes(&msg)?)
}

/// Daemon: tells the phone how its accepted request ended. The session is
/// closed afterwards.
pub async fn send_pair_result<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut LinkStream<S>,
    result: PairResult,
) -> Result<(), LinkError> {
    stream.send_message(&result.to_bytes()).await
}

/// A pairing request the daemon has read but not yet answered. Answer it
/// with [`respond`](Self::respond), or drop it to close the connection with
/// no reply (as when no `flankr pair` run is in pair mode).
pub struct PairIncoming<S> {
    inner: hlt::PairIncoming<S>,
    /// The request, or why it could not be read: answer that with
    /// [`PairRefusal::Malformed`].
    pub request: Result<PairRequest, PairMessageError>,
}

impl<S: AsyncRead + AsyncWrite + Unpin> PairIncoming<S> {
    fn new(inner: hlt::PairIncoming<S>) -> Self {
        let request = inner.request.clone().map(|r| PairRequest {
            device_key: r.key,
            name: r.device,
            offer: r.offer,
        });
        PairIncoming { inner, request }
    }

    /// Sends `ack` in the second handshake message and returns the
    /// session. After [`PairAck::Accepted`], keep it to
    /// [`send_pair_result`] and to notice the phone closing (its Cancel);
    /// after a refusal, drop it: it is closed.
    ///
    /// The session's [`peer_key`](LinkStream::peer_key) is all zeros: NK
    /// does not authenticate the phone. Its claimed key is the request's
    /// `device_key`.
    pub async fn respond(self, ack: PairAck) -> Result<LinkStream<S>, LinkError> {
        Ok(match self.inner.respond(ack).await? {
            Some(held) => LinkStream::pair_held(held),
            None => LinkStream::closed([0u8; KEY_LEN]),
        })
    }
}

/// Daemon: accepts one pairing request. Reads the mode byte, which must be
/// [`MODE_PAIR`] (any other is [`LinkError::UnknownMode`]), then the NK
/// first message with the request.
pub async fn accept_pair<S, K>(io: S, local: &K) -> Result<PairIncoming<S>, LinkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
    K: StaticKey + Clone,
{
    Ok(PairIncoming::new(
        hlt::accept_pair(io, &FLANKR_PROFILE, local.clone()).await?,
    ))
}

/// A connection the daemon accepted on a listener that takes both kinds.
pub enum Incoming<S> {
    /// An `IK` connection from an authorized device, as [`accept`](super::accept) returns.
    Link(Accepted<S>),
    /// A pairing request, as [`accept_pair`] returns.
    Pair(Box<PairIncoming<S>>),
}

/// Daemon: accepts one connection of either kind, by its mode byte:
/// [`MODE_RECONNECT`](super::MODE_RECONNECT) as [`accept`](super::accept)
/// (with `authorize`), [`MODE_PAIR`] as [`accept_pair`]. Any other byte is
/// [`LinkError::UnknownMode`]. A daemon not in pair mode drops a
/// [`Incoming::Pair`] unanswered.
pub async fn accept_incoming<S, K, A>(
    io: S,
    local: &K,
    authorize: A,
) -> Result<Incoming<S>, LinkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
    K: StaticKey + Clone,
    A: FnOnce(&[u8; KEY_LEN]) -> bool,
{
    Ok(
        match hlt::accept_incoming(io, &FLANKR_PROFILE, local.clone(), authorize).await? {
            hlt::Incoming::Link(a) => Incoming::Link(a.into()),
            hlt::Incoming::Pair(p) => Incoming::Pair(Box::new(PairIncoming::new(*p))),
        },
    )
}
