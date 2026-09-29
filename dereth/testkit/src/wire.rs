//! What actually left on the datagrams.
//!
//! # Why the frame's outbox is not the wire
//!
//! [`crate::HeadlessClient::outbound`] is the frame's own outbox -- the requests the interaction
//! layer composed this pass -- and for almost everything the client sends that is the same list
//! the datagrams carry. It is not the same list for at least one message: **the cancel that breaks
//! an automatic attack never reaches the outbox at all**, because the abort puts its message
//! straight on the flow queue. A scenario that asserted "one cancel was sent" through the outbox
//! alone would be asserting nothing.
//!
//! So there are two readers and not one, and this is why:
//!
//! | reader | what it answers | when the two differ |
//! |---|---|---|
//! | [`crate::HeadlessClient::outbound`] | the requests the frame composed, as values | a message the client puts on the flow queue itself is absent |
//! | [`crate::HeadlessClient::outbound_wire`] | the ordered sub-types the client really framed into a datagram | it needs an attached endpoint, and it sees nothing else |
//!
//! On the **model backend** this reader is empty by construction: that backend runs no sender, so
//! nothing it produces is ever framed into a datagram and the outbox is the only honest reader
//! there is.
//!
//! Neither is a superset of the other and neither is being deleted. The outbox carries the
//! request's own fields, which is what most claims are about; the wire carries only an opcode and
//! a sub-type, but it carries every one of them.
//!
//! # Nothing leaves this process
//!
//! The endpoint under a scenario is [`crate::replay`]'s: its transport queues datagrams and binds
//! no socket. Reading them is taking that queue.

use dereth_primitives::LocalTime;
use dereth_transport::indicator::Indicator;

use crate::client::HeadlessClient;

/// The envelope every ordered game **action** is inside -- what the client sends.
pub const GAME_ACTION: u32 = 0xF7B1;

/// The ordered game actions a client really built into a datagram.
///
/// It is stateful because reassembly is: a blob can span datagrams, so a reader that started again
/// every time it was asked would lose every fragmented message. A scenario keeps one of these for
/// as long as it keeps the client.
#[derive(Default)]
pub struct Wire {
    reassembly: Indicator,
    sub_types: Vec<u32>,
}

impl std::fmt::Debug for Wire {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Wire")
            .field("sub_types", &self.sub_types)
            .finish_non_exhaustive()
    }
}

impl Wire {
    /// A fresh reader.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Take everything the client has written since the last look, and add its ordered sub-types
    /// to this reader's own list.
    ///
    /// # Panics
    /// Panics when the client has no endpoint attached -- a scenario reading the wire of a client
    /// that has no link is reading nothing and must not pass.
    pub fn observe(&mut self, client: &mut HeadlessClient) {
        let now = client.wire_clock();
        let datagrams = client
            .replay_net_mut()
            .expect(
                "this scenario reads the wire, so it must have an endpoint: attach one with \
                 dereth_testkit::replay::Peer::attach or give it a recorded login",
            )
            .take_outgoing();
        self.ingest(&datagrams, now);
    }

    /// Fold datagrams the caller has already taken off an endpoint into this reader.
    ///
    /// # Panics
    /// Panics when the client wrote a datagram the shipping parser refuses, or an action envelope
    /// it cannot unpack -- either would be a defect in the writer rather than a state the client
    /// could be in.
    pub fn ingest<T>(&mut self, datagrams: &[(Vec<u8>, T)], now: LocalTime) {
        for (bytes, _) in datagrams {
            let packet =
                dereth_transport::ParsedPacket::parse(bytes).expect("a real output packet");
            for blob in
                self.reassembly
                    .check_in_packet(&packet.fragments, packet.header.rec_id, now)
            {
                if blob.payload.get(..4) != Some(GAME_ACTION.to_le_bytes().as_slice()) {
                    continue; // login, cache and acknowledgement housekeeping
                }
                let action = dereth_protocol::actions::unpack_action(&blob.payload)
                    .expect("an action envelope");
                self.sub_types.push(action.sub_type.0);
            }
        }
    }

    /// Everything this reader has seen, oldest first, without clearing it.
    #[must_use]
    pub fn sub_types(&self) -> &[u32] {
        &self.sub_types
    }

    /// Everything this reader has seen since the last take, and clear it.
    pub fn take(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.sub_types)
    }

    /// How many of `sub_type` this reader has seen since the last take, and clear it. The question
    /// a scenario about one message asks.
    pub fn take_count(&mut self, sub_type: u32) -> usize {
        self.take().into_iter().filter(|s| *s == sub_type).count()
    }
}
