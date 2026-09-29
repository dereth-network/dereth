//! The client's side of the UDP connection: the socket, the client session above the transport, and
//! the capture and replay harness.
//!
//! **Depends on** `dereth-primitives`, the role-neutral transport (`dereth-transport`) and the
//! codecs (`dereth-protocol`). **Used by** the client runtime and the SDK, the client and its test
//! kit, the corpus tool (`dereth-corpus`), and the server's test bot (`empyrean-net`, behind a
//! feature).
//!
//! **Must never** let [`client_session`] reach the network except through
//! [`Transport`](dereth_primitives::Transport): the session names nothing else of this crate (the
//! socket, [`Net`], the wire types), and every session test runs against its mock transport (the
//! `cpu` test `client_session::isolation` checks it). Outside the session, nothing here looks past
//! a blob's first byte.
//!
//! [`client_session`] is the client's connection state machine, its queue dispatchers and ordering,
//! which decode bodies through `dereth-protocol`. It is not called `session` because
//! `dereth_transport::session` is the role-neutral per-connection state. The `cpu` tier drives the
//! transport's rules end to end through [`Net`].
//!
//! **Specified in** `docs/networking/02-reliability-and-flow.md`,
//! `docs/networking/03-connection-state-machine.md` and `docs/networking/04-netblobs-and-queues.md`
//! for the connection, and `docs/networking/messages/00-dispatch-and-queues.md` for how received
//! messages are dispatched.

// This crate is one of the two permitted unsafe. All of it lives in `socket`, guarded by a
// module-level `deny` everywhere else because only the platform socket wrapper requires it.
#![deny(unsafe_code)]
#![doc(html_no_source)]

/// The client-side state above the transport.
pub mod client_session;
/// Link-status snapshots and averages — the packet counters the link-status panel
/// divides.
pub mod linkstatus;
pub mod net;
pub mod queues;
/// What a raw recording says that only a datagram-header parse can read: the login request's
/// connection sequence number and each enter-world, for every harness that replays one.
pub mod recording;
/// The capture/replay harness. Behind a feature so the transport itself needs no JSON parser.
#[cfg(feature = "replay")]
pub mod replay;
pub mod sequence_gate;
pub mod socket;

pub use net::{Net, NetConfig, RecipientId};

/// Everything that can go wrong in this crate.
///
/// Note the naming collision the original creates and this crate keeps: `NetError` here is the
/// crate's error type, while [`dereth_transport::conn::NetErrorCode`] is the protocol's own 21-value error
/// enumeration carried in the `NetError` / `NetErrorDisconnect` optional headers. Context
/// distinguishes the crate's error type from the protocol enumeration.
#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("malformed packet: {0}")]
    Wire(#[from] dereth_transport::WireError),
    #[error("socket: {0}")]
    Io(#[from] std::io::Error),
    #[error("bad host specification {spec:?}: {reason}")]
    BadHost { spec: String, reason: &'static str },
    #[error("bad interface specification {spec:?}: {reason}")]
    BadInterface { spec: String, reason: &'static str },
}
