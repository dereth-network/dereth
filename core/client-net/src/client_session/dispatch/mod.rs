//! The four opcode dispatchers, one per drained queue.
//!
//! Source: `docs/networking/messages/00-dispatch-and-queues.md` §1. The client has
//! exactly two large switches — [`ui`] (queue 9) and [`world_objects`] (queue 10) — plus two small ones,
//! [`database`] (queue 5) and [`logon`] (queue 4).
//!
//! **Do not merge the queues.** The ordering rules, the crucial-events gate and the object-blocked
//! replay all differ per queue, and the client's visible behaviour under loss — an object stalled
//! for up to 300 s and then a forced skip — depends on them.

pub mod database;
pub mod logon;
pub mod ui;
pub mod world_objects;
