//! The capture-replay tier (feature `captures`, which needs `real-content`).
//!
//! Every recording in the flat corpus (`fixtures/packet-captures/<slug>.jsonl`) is an ACE session: a
//! retail client talking to a live ACE. For each one:
//!
//! 1. the recording is reassembled and put in the server's send order ([`recording`]);
//! 2. the stream invariants ([`invariants`]) are checked on the recording, which is ACE's truth;
//! 3. a [`TestServer`](crate::TestServer) is stood up on the retail dats and `world.pack`, with the
//!    recorded characters: created over the wire from the recording's own character creation, or
//!    reconstructed from the recording's PlayerDescription and CreateObjects ([`character`]);
//!    synthetic account and character names only;
//! 4. the client's recorded messages are fed in order, closed loop ([`replay`]): each waits for the
//!    server message the client was answering, recorded guids are translated to ours, and the gaps
//!    between messages keep their recorded length;
//! 5. our outbound stream is checked with the same invariants and compared with the recording's
//!    per phase ([`compare`]), and each difference is matched, explained, or left unexplained.
//!
//! Nothing here is an ACE port. Nothing here prints or keeps a payload: reports carry opcodes,
//! event and action types, guids and counts only (the recordings are pseudonymised, but chat and
//! names stay out of logs regardless).

pub mod character;
pub mod compare;
pub mod invariants;
pub mod recording;
pub mod replay;
pub mod steer;
pub mod wire;

pub use compare::SessionReport;
pub use replay::{replay_session, ReplayOutcome};
