//! Drivers: what moves datagrams between [`crate::ServerNet`] and the outside world.
//!
//! - [`udp::UdpDriver`] owns ACE's two sockets (`P` and `P + 1`), each read by its own thread, so a
//!   datagram on either port is handed over the moment it arrives.
//! - [`memory::MemoryNet`] is an in-process network for tests: datagram pipes with a deterministic
//!   lossy, reordering and duplicating mode, and a clock that moves only when told to.

pub mod memory;
pub mod udp;
