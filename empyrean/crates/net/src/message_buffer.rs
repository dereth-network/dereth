// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/MessageBuffer.cs

use crate::client_message::ClientMessage;
use crate::client_packet::ClientPacketFragment;

/// ACE `MessageBuffer`: the fragments of one split inbound message.
#[derive(Debug, Clone)]
pub struct MessageBuffer {
    fragments: Vec<ClientPacketFragment>,
    pub sequence: u32,
    pub total_fragments: u32,
}

impl MessageBuffer {
    // ACE: MessageBuffer.MessageBuffer
    #[must_use]
    pub fn new(sequence: u32, total_fragments: u32) -> Self {
        Self {
            fragments: Vec::new(),
            sequence,
            total_fragments,
        }
    }

    // ACE: MessageBuffer.Count
    /// ACE `Count`.
    #[must_use]
    pub fn count(&self) -> usize {
        self.fragments.len()
    }

    // ACE: MessageBuffer.Complete
    /// ACE `Complete`.
    #[must_use]
    pub fn complete(&self) -> bool {
        self.fragments.len() as u64 == u64::from(self.total_fragments)
    }

    // ACE: MessageBuffer.AddFragment
    /// A duplicate index, or any fragment once complete, is ignored.
    pub fn add_fragment(&mut self, fragment: ClientPacketFragment) {
        if !self.complete() && self.fragments.iter().all(|x| x.index != fragment.index) {
            self.fragments.push(fragment);
        }
    }

    // ACE: MessageBuffer.TryGetMessage
    /// The fragments in index order, joined; `None` when shorter than the 4-byte opcode.
    pub fn try_get_message(&mut self) -> Option<ClientMessage> {
        self.fragments.sort_by_key(|f| f.index);
        let data: Vec<u8> = self
            .fragments
            .iter()
            .flat_map(|f| f.data.iter().copied())
            .collect();
        if data.len() < 4 {
            return None;
        }
        ClientMessage::new(data)
    }
}
