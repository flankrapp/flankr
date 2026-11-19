//! The handshake, `IK`, client and server (plan 02, amendment of 23:07
//! EDT: one handshake): libhardline's tokio adapter, fixed to
//! [`FLANKR_PROFILE`]. The pairing request's `NK` handshake, which shares
//! the framing and the prologue, is in `pair_mode`.

use libhardline::tokio as hlt;
use tokio::io::{AsyncRead, AsyncWrite};

use super::{FLANKR_PROFILE, KEY_LEN, LinkError, LinkStream, StaticKey};

/// A connection the server accepted.
pub struct Accepted<S> {
    pub stream: LinkStream<S>,
    /// The client's static X25519 public key, which `authorize` allowed.
    pub peer_key: [u8; KEY_LEN],
    /// The message the client carried in its first handshake message, if
    /// any (`hello`).
    pub first_message: Option<Vec<u8>>,
}

impl<S: AsyncRead + AsyncWrite + Unpin> From<hlt::Accepted<S>> for Accepted<S> {
    fn from(a: hlt::Accepted<S>) -> Self {
        Accepted {
            stream: LinkStream::link(a.stream),
            peer_key: a.peer,
            first_message: a.first_message,
        }
    }
}

/// Client: connects to a server whose static key it knows, over `IK`.
/// The first connection after pairing is one of these too: the server's
/// key comes from the pairing string.
///
/// Sends the mode byte `0x02` and the first handshake message, carrying
/// `first_message` (encrypted) if given. A server that does not authorize
/// the client's key sends nothing and closes the connection, which this
/// returns as [`LinkError::Closed`].
pub async fn reconnect<S: AsyncRead + AsyncWrite + Unpin, K: StaticKey + Clone>(
    io: S,
    local: &K,
    server_key: &[u8; KEY_LEN],
    first_message: Option<&[u8]>,
) -> Result<LinkStream<S>, LinkError> {
    let stream = hlt::reconnect(
        io,
        &FLANKR_PROFILE,
        local.clone(),
        server_key,
        first_message,
    )
    .await?;
    Ok(LinkStream::link(stream))
}

/// Server: accepts one connection.
///
/// Reads the mode byte, which must be `0x02` (`IK`); any other byte
/// (`0x01`, the pairing handshake that was removed, included) is
/// [`LinkError::UnknownMode`] and closes the connection with no reply.
/// Then reads the client's first message, learns its static key and asks
/// `authorize`; an unauthorized key gets no reply, and the connection is
/// dropped.
///
/// On error, `io` is dropped, which closes the connection.
pub async fn accept<S, K, A>(io: S, local: &K, authorize: A) -> Result<Accepted<S>, LinkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
    K: StaticKey + Clone,
    A: FnOnce(&[u8; KEY_LEN]) -> bool,
{
    Ok(hlt::accept(io, &FLANKR_PROFILE, local.clone(), authorize)
        .await?
        .into())
}
