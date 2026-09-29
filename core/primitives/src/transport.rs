//! The message transport interface: complete application messages in, payloads out.
//!
//! [`Transport`] is what a message handler or a session layer sees of the network. Below it sit
//! the socket, packet framing, encryption, fragments and reassembly; above it, every message is an
//! [`IncomingMessage`] with its framing removed. The interface is synchronous and pollable, so a
//! handler is a pure function of its input and can be tested against a recorded capture, or a
//! scripted mock, with no socket and no runtime.

/// The ordering queues. The client registers five of the twelve; queue 2 is outbound-only and is
/// never drained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NetQueue {
    Control,
    Weenie,
    Logon,
    ClientCache,
    UiQueue,
    WorldObjects,
    /// Any queue value the session layer has not been taught to name.
    Other(u8),
}

/// Which peer a message arrived from.
///
/// The retail deployment splits the login server from the world server, and a handler's correct
/// behaviour can depend on which one spoke. Carried here rather than inferred, because inferring it
/// from the queue is wrong on a split deployment. The number is also the peer's index into the
/// session layer's 256-entry receiver table, which orders its peers by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct RecipientId(pub u16);

/// The blob identifier, whose high dword carries the ordering type and stamp.
///
/// A dispatcher needs this to decide ordering, so it travels with the message rather than being
/// consumed and discarded by the transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct NetBlobId(pub u64);

impl NetBlobId {
    #[inline]
    #[must_use]
    pub const fn high32(self) -> u32 {
        (self.0 >> 32) as u32
    }

    #[inline]
    #[must_use]
    pub const fn low32(self) -> u32 {
        // Truncation is the point: the low dword is a distinct field of the blob id.
        #[allow(clippy::cast_possible_truncation)]
        {
            self.0 as u32
        }
    }
}

/// A reassembled message with its framing removed. The opcode is the first dword of the payload in
/// the original; it is lifted out here because every dispatcher needs it.
///
/// `sender` and `blob_id` are both load-bearing: the first because a handler must know whether the
/// login or the world server spoke, the second because the ordering type and stamp drive the
/// dispatch decision.
#[derive(Debug, Clone)]
pub struct IncomingMessage {
    pub opcode: u32,
    pub queue: NetQueue,
    /// Which peer sent it.
    pub sender: RecipientId,
    /// The blob this message arrived in, carrying its ordering type and stamp.
    pub blob_id: NetBlobId,
    /// Payload after the opcode.
    pub body: Vec<u8>,
}

/// What the protocol and session layers are allowed to ask of the transport.
///
/// Deliberately synchronous and pollable: only the transport and session crates may be async, so a
/// message handler is a pure function of its input and can be tested against a recorded capture with
/// no runtime at all.
pub trait Transport {
    fn send(&mut self, queue: NetQueue, ordered: bool, payload: &[u8]);
    fn poll(&mut self) -> Option<IncomingMessage>;
}
