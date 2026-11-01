//! An in-memory network: datagrams between addresses, delivered when asked, nothing lost unless
//! a test says so. The server's four sockets and any number of clients share it.

use std::collections::VecDeque;
use std::net::SocketAddr;

/// One datagram in flight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InFlight {
    pub from: SocketAddr,
    pub to: SocketAddr,
    pub bytes: Vec<u8>,
}

/// The network.
#[derive(Debug, Default)]
pub struct MemoryNet {
    queue: VecDeque<InFlight>,
    /// Drop every n-th datagram (0: none).
    pub drop_every: usize,
    sent: usize,
    pub dropped: usize,
}

impl MemoryNet {
    /// Put a datagram on the wire.
    pub fn send(&mut self, from: SocketAddr, to: SocketAddr, bytes: Vec<u8>) {
        self.sent += 1;
        if self.drop_every != 0 && self.sent.is_multiple_of(self.drop_every) {
            self.dropped += 1;
            return;
        }
        self.queue.push_back(InFlight { from, to, bytes });
    }

    /// Everything in flight, in the order sent.
    pub fn take(&mut self) -> Vec<InFlight> {
        self.queue.drain(..).collect()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}
