//! The bus between the clients on one machine: a plugin's messages to its own copies in the
//! player's other clients.
//!
//! A plugin sends a value on a named channel (`peers.send(channel, data)`), and the same plugin in
//! every other client on the machine hears it as `peers.message` with the sending client's
//! character. A plugin hears only its own id's messages. The transport is the product's: loopback
//! sockets on the desktop, a broadcast channel between a browser's tabs, a shared queue between
//! hosts in one process ([`LocalBus`]). A world's policy may cap how many other clients a plugin
//! coordinates (`peers_local`); messages from clients beyond the cap are dropped.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// The most bytes one message's data may take.
pub const MAX_MESSAGE: usize = 64 * 1024;
/// The most messages one plugin may send in a second.
pub const SENDS_PER_SECOND: usize = 20;

/// One message on the bus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerMessage {
    /// The sending client's character (empty before a character is in the world).
    pub from: String,
    /// The sending plugin's id; only the same plugin hears it.
    pub plugin: String,
    pub channel: String,
    /// The data, as JSON.
    pub data: String,
}

/// A product's transport between its clients.
pub trait PluginPeers: std::fmt::Debug {
    /// Send to every other client.
    fn send(&mut self, message: &PeerMessage);
    /// The messages other clients sent since the last call, in arrival order.
    fn receive(&mut self) -> Vec<PeerMessage>;
}

/// No other clients.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoPeers;

impl PluginPeers for NoPeers {
    fn send(&mut self, _message: &PeerMessage) {}

    fn receive(&mut self) -> Vec<PeerMessage> {
        Vec::new()
    }
}

#[derive(Debug, Default)]
struct Bus {
    queues: Vec<VecDeque<PeerMessage>>,
}

/// A bus between hosts in one process: every endpoint hears what the others send.
#[derive(Debug, Clone, Default)]
pub struct LocalBus {
    inner: Arc<Mutex<Bus>>,
}

impl LocalBus {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A new client's end of the bus.
    #[must_use]
    pub fn endpoint(&self) -> BusEndpoint {
        let mut bus = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        bus.queues.push(VecDeque::new());
        BusEndpoint {
            bus: self.clone(),
            index: bus.queues.len() - 1,
        }
    }
}

/// One client's end of a [`LocalBus`].
#[derive(Debug)]
pub struct BusEndpoint {
    bus: LocalBus,
    index: usize,
}

impl PluginPeers for BusEndpoint {
    fn send(&mut self, message: &PeerMessage) {
        let mut bus = self
            .bus
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for (i, q) in bus.queues.iter_mut().enumerate() {
            if i != self.index {
                q.push_back(message.clone());
            }
        }
    }

    fn receive(&mut self) -> Vec<PeerMessage> {
        let mut bus = self
            .bus
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        bus.queues
            .get_mut(self.index)
            .map(|q| q.drain(..).collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::{LocalBus, PeerMessage, PluginPeers};

    #[test]
    fn every_other_endpoint_hears_a_message_and_the_sender_does_not() {
        let bus = LocalBus::new();
        let (mut a, mut b, mut c) = (bus.endpoint(), bus.endpoint(), bus.endpoint());
        let m = PeerMessage {
            from: "Leader".into(),
            plugin: "network.dereth.follow".into(),
            channel: "pos".into(),
            data: "{}".into(),
        };
        a.send(&m);
        assert!(a.receive().is_empty());
        assert_eq!(b.receive(), vec![m.clone()]);
        assert_eq!(c.receive(), vec![m]);
        assert!(b.receive().is_empty());
    }
}
