//! The established link: libhardline's tokio `LinkStream`, with flankr's
//! message kinds over its protocol numbers.
//!
//! A [`LinkStream`] is a link, or the session of a pairing request (which
//! carries one [`PairResult`] and nothing else): libhardline gives the two
//! as different types, which this one keeps behind the old methods.

use libhardline::tokio as hlt;
use libhardline::{CloseReason, proto::PairResult};
use tokio::io::{AsyncRead, AsyncWrite, ReadHalf, WriteHalf};

use super::{KEY_LEN, KIND_BINARY, KIND_JSON, KIND_TERMINAL, LinkError};

/// A message's kind, as the high nibble of its frames' flag byte carries
/// it (see the [module](super) header). Open: later kinds are additive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Kind {
    /// A JSON-RPC message ([`KIND_JSON`](super::KIND_JSON)).
    Json,
    /// A terminal frame ([`KIND_TERMINAL`](super::KIND_TERMINAL); plan
    /// 20261027_07, decision 3): its bytes are `libflankr_term`'s encoding.
    Terminal,
    /// A binary transfer message ([`KIND_BINARY`](super::KIND_BINARY); plan
    /// 20261028_05, decision 3.1): its bytes are `libflankr_blob`'s
    /// encoding.
    Binary,
}

impl Kind {
    /// The kind's nibble, before it is shifted into the flag byte's high
    /// half.
    pub fn nibble(self) -> u8 {
        match self {
            Kind::Json => KIND_JSON,
            Kind::Terminal => KIND_TERMINAL,
            Kind::Binary => KIND_BINARY,
        }
    }

    /// The kind a nibble names, if it is one this crate reads.
    pub fn from_nibble(nibble: u8) -> Option<Kind> {
        match nibble {
            KIND_JSON => Some(Kind::Json),
            KIND_TERMINAL => Some(Kind::Terminal),
            KIND_BINARY => Some(Kind::Binary),
            _ => None,
        }
    }
}

/// One whole message as [`LinkStream::recv`] returns it, by its kind.
/// Open: later kinds are additive.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Message {
    /// A JSON-RPC message: what [`LinkStream::recv_message`] returns.
    Json(Vec<u8>),
    /// A terminal frame.
    Terminal(Vec<u8>),
    /// A binary transfer message (plan 20261028_05).
    Binary(Vec<u8>),
}

impl Message {
    pub fn kind(&self) -> Kind {
        match self {
            Message::Json(_) => Kind::Json,
            Message::Terminal(_) => Kind::Terminal,
            Message::Binary(_) => Kind::Binary,
        }
    }

    pub fn bytes(&self) -> &[u8] {
        match self {
            Message::Json(b) | Message::Terminal(b) | Message::Binary(b) => b,
        }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        match self {
            Message::Json(b) | Message::Terminal(b) | Message::Binary(b) => b,
        }
    }

    pub(super) fn new(kind: Kind, bytes: Vec<u8>) -> Message {
        match kind {
            Kind::Json => Message::Json(bytes),
            Kind::Terminal => Message::Terminal(bytes),
            Kind::Binary => Message::Binary(bytes),
        }
    }
}

/// An established link connection. Messages are whole: one
/// [`send_message`](Self::send_message) is one
/// [`recv_message`](Self::recv_message) on the other side, and one
/// `send_message`, [`send_terminal`](Self::send_terminal) or
/// [`send_binary`](Self::send_binary) is one [`recv`](Self::recv).
///
/// Since 0.5.17 `recv_message` and `recv` are cancel-safe, as libhardline's
/// are: a message is taken only when it is returned. To read and write at
/// once, [`into_split`](Self::into_split) the stream.
///
/// The session of a pairing request ([`PairResponse::stream`](super::PairResponse)
/// on the phone, [`PairIncoming::respond`](super::PairIncoming::respond)'s
/// on the daemon) is a `LinkStream` too. It carries the one
/// [`PairResult`](super::PairResult): [`send_pair_result`](super::send_pair_result)
/// sends it, [`recv_pair_result`](super::recv_pair_result) reads it, and on
/// the daemon `recv_message` waits for the phone to close the session. It
/// cannot be split: [`into_split`](Self::into_split) gives closed halves.
pub struct LinkStream<S> {
    inner: Inner<S>,
    peer_key: [u8; KEY_LEN],
}

enum Inner<S> {
    /// An `IK` link.
    Link(Box<hlt::LinkStream<S>>),
    /// The phone's side of an accepted pairing request.
    PairPending(Box<hlt::PairPending<S>>),
    /// The daemon's side of an accepted pairing request.
    PairHeld(Box<hlt::PairHeld<S>>),
    /// Closed: a refused pairing request, a pairing session after its
    /// result, or a stream shut down.
    Closed,
}

/// The error a closed pairing session reports: its own error, or
/// [`LinkError::Closed`].
pub(super) fn close_error(reason: CloseReason) -> LinkError {
    match reason {
        CloseReason::Error(e) => e,
        CloseReason::Unreachable { last: Some(last) } => close_error(*last),
        _ => LinkError::Closed,
    }
}

/// The message a protocol number carries, if it is a kind this crate
/// reads.
fn message(protocol: u16, bytes: Vec<u8>) -> Option<Message> {
    let kind = Kind::from_nibble(u8::try_from(protocol).ok()?)?;
    Some(Message::new(kind, bytes))
}

impl<S: AsyncRead + AsyncWrite + Unpin> LinkStream<S> {
    pub(super) fn link(stream: hlt::LinkStream<S>) -> Self {
        let peer_key = *stream.peer();
        LinkStream {
            inner: Inner::Link(Box::new(stream)),
            peer_key,
        }
    }

    pub(super) fn pair_pending(pending: hlt::PairPending<S>, peer_key: [u8; KEY_LEN]) -> Self {
        LinkStream {
            inner: Inner::PairPending(Box::new(pending)),
            peer_key,
        }
    }

    pub(super) fn pair_held(held: hlt::PairHeld<S>) -> Self {
        LinkStream {
            inner: Inner::PairHeld(Box::new(held)),
            peer_key: [0u8; KEY_LEN],
        }
    }

    pub(super) fn closed(peer_key: [u8; KEY_LEN]) -> Self {
        LinkStream {
            inner: Inner::Closed,
            peer_key,
        }
    }

    /// The peer's static X25519 public key, authenticated by the handshake.
    /// On the daemon's side of a pairing session it is all zeros: `NK` does
    /// not authenticate the phone.
    pub fn peer_key(&self) -> &[u8; KEY_LEN] {
        &self.peer_key
    }

    /// Sends one message, in as many frames as it needs. A message over
    /// [`MAX_MESSAGE`](super::MAX_MESSAGE) is refused without sending
    /// anything.
    pub async fn send_message(&mut self, msg: &[u8]) -> Result<(), LinkError> {
        self.send_kind(Kind::Json, msg).await
    }

    /// Sends one terminal frame, as [`send_message`](Self::send_message)
    /// sends a JSON-RPC message. Send it only to a peer that reads with
    /// [`recv`](Self::recv): `recv_message` is `BadFrame` on it.
    pub async fn send_terminal(&mut self, msg: &[u8]) -> Result<(), LinkError> {
        self.send_kind(Kind::Terminal, msg).await
    }

    /// Sends one binary transfer message (plan 20261028_05), as
    /// [`send_message`](Self::send_message) sends a JSON-RPC message. Send
    /// it only to a peer that reads kind 2 (decision 3.2).
    pub async fn send_binary(&mut self, msg: &[u8]) -> Result<(), LinkError> {
        self.send_kind(Kind::Binary, msg).await
    }

    /// Sends one message of `kind`. A message over
    /// [`MAX_MESSAGE`](super::MAX_MESSAGE) is refused without sending
    /// anything.
    ///
    /// On the daemon's side of a pairing session the only message is the
    /// result: a JSON-RPC message whose bytes are a [`PairResult`]'s, sent
    /// once, after which the session is closed. Anything else there, and
    /// anything sent on the phone's side, is refused: a message that is not
    /// a result is [`LinkError::Pairing`], and a session that cannot send
    /// is [`LinkError::Closed`].
    pub async fn send_kind(&mut self, kind: Kind, msg: &[u8]) -> Result<(), LinkError> {
        if msg.len() > super::MAX_MESSAGE {
            return Err(LinkError::MessageTooLarge);
        }
        match &mut self.inner {
            Inner::Link(s) => s.send(u16::from(kind.nibble()), msg).await,
            Inner::PairHeld(_) if kind == Kind::Json => {
                let result = PairResult::from_bytes(msg)?;
                let Inner::PairHeld(held) = std::mem::replace(&mut self.inner, Inner::Closed)
                else {
                    unreachable!("matched above")
                };
                held.send_result(result).await
            }
            Inner::PairHeld(_) | Inner::PairPending(_) | Inner::Closed => Err(LinkError::Closed),
        }
    }

    /// Receives one whole JSON-RPC message. A message of any other kind is
    /// [`LinkError::BadFrame`]. On any error, including a message
    /// reassembled past [`MAX_MESSAGE`](super::MAX_MESSAGE), the connection
    /// is closed.
    pub async fn recv_message(&mut self) -> Result<Vec<u8>, LinkError> {
        match &mut self.inner {
            Inner::Link(s) => s.recv_message().await,
            Inner::PairPending(p) => {
                let r = p.wait().await;
                self.inner = Inner::Closed;
                r.map(PairResult::to_bytes)
            }
            Inner::PairHeld(h) => {
                let reason = h.closed().await;
                self.inner = Inner::Closed;
                Err(close_error(reason))
            }
            Inner::Closed => Err(LinkError::Closed),
        }
    }

    /// Receives one whole message of any known kind. Errors as
    /// [`recv_message`](Self::recv_message) does, but a terminal frame is
    /// [`Message::Terminal`] and a binary message [`Message::Binary`]; a
    /// message of a reserved kind (3 to 14) is [`LinkError::BadFrame`] and
    /// closes the connection.
    pub async fn recv(&mut self) -> Result<Message, LinkError> {
        let Inner::Link(s) = &mut self.inner else {
            return self.recv_message().await.map(Message::Json);
        };
        let (protocol, bytes) = s.recv().await?;
        match message(protocol, bytes) {
            Some(m) => Ok(m),
            None => {
                let _ = s.close().await;
                Err(LinkError::BadFrame)
            }
        }
    }

    /// Shuts the stream down; the peer then reads end of stream.
    pub async fn shutdown(&mut self) -> Result<(), LinkError> {
        match std::mem::replace(&mut self.inner, Inner::Closed) {
            Inner::Link(mut s) => {
                let r = s.close().await;
                self.inner = Inner::Link(s);
                r
            }
            Inner::PairPending(p) => {
                p.cancel().await;
                Ok(())
            }
            // Dropping the session closes the connection.
            Inner::PairHeld(_) | Inner::Closed => Ok(()),
        }
    }

    /// Splits the stream into halves that can be used from different tasks.
    /// A pairing session's halves are closed.
    pub fn into_split(self) -> (LinkReader<ReadHalf<S>>, LinkWriter<WriteHalf<S>>) {
        match self.inner {
            Inner::Link(s) => {
                let (r, w) = (*s).into_split();
                (
                    LinkReader {
                        inner: Some(r),
                        closed: false,
                    },
                    LinkWriter { inner: Some(w) },
                )
            }
            _ => (
                LinkReader {
                    inner: None,
                    closed: true,
                },
                LinkWriter { inner: None },
            ),
        }
    }
}

/// The receiving half of a [`LinkStream`]. After an error, drop it and the
/// writer: the reader cannot shut the stream down by itself.
pub struct LinkReader<R> {
    inner: Option<hlt::LinkReader<R>>,
    closed: bool,
}

impl<R: AsyncRead + Unpin> LinkReader<R> {
    /// As [`LinkStream::recv_message`]. Cancel-safe.
    pub async fn recv_message(&mut self) -> Result<Vec<u8>, LinkError> {
        let r = match (&mut self.inner, self.closed) {
            (Some(r), false) => r.recv_message().await,
            _ => return Err(LinkError::Closed),
        };
        self.closed = r.is_err();
        r
    }

    /// As [`LinkStream::recv`]. Cancel-safe.
    pub async fn recv(&mut self) -> Result<Message, LinkError> {
        let r = match (&mut self.inner, self.closed) {
            (Some(r), false) => r.recv().await,
            _ => return Err(LinkError::Closed),
        };
        let r = r.and_then(|(protocol, bytes)| message(protocol, bytes).ok_or(LinkError::BadFrame));
        self.closed = r.is_err();
        r
    }
}

/// The sending half of a [`LinkStream`].
pub struct LinkWriter<W> {
    inner: Option<hlt::LinkWriter<W>>,
}

impl<W: AsyncWrite + Unpin> LinkWriter<W> {
    /// As [`LinkStream::send_message`].
    pub async fn send_message(&mut self, msg: &[u8]) -> Result<(), LinkError> {
        self.send_kind(Kind::Json, msg).await
    }

    /// As [`LinkStream::send_terminal`].
    pub async fn send_terminal(&mut self, msg: &[u8]) -> Result<(), LinkError> {
        self.send_kind(Kind::Terminal, msg).await
    }

    /// As [`LinkStream::send_binary`].
    pub async fn send_binary(&mut self, msg: &[u8]) -> Result<(), LinkError> {
        self.send_kind(Kind::Binary, msg).await
    }

    /// As [`LinkStream::send_kind`].
    pub async fn send_kind(&mut self, kind: Kind, msg: &[u8]) -> Result<(), LinkError> {
        if msg.len() > super::MAX_MESSAGE {
            return Err(LinkError::MessageTooLarge);
        }
        match &mut self.inner {
            Some(w) => w.send(u16::from(kind.nibble()), msg).await,
            None => Err(LinkError::Closed),
        }
    }

    /// Shuts the stream down; the peer then reads end of stream.
    pub async fn shutdown(&mut self) -> Result<(), LinkError> {
        match &mut self.inner {
            Some(w) => w.close().await,
            None => Ok(()),
        }
    }
}
