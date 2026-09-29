// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/NetworkBundle.cs

use std::collections::VecDeque;

use crate::OutboundMessage;

/// ACE `NetworkBundle`: the messages and header requests gathered for one group between sends.
#[derive(Debug, Clone)]
pub struct NetworkBundle {
    prop_changed: bool,
    messages: VecDeque<OutboundMessage>,
    client_time: f32,
    time_sync: bool,
    ack_seq: bool,
    pub encrypted_checksum: bool,
    pub current_size: i32,
}

impl Default for NetworkBundle {
    fn default() -> Self {
        Self {
            prop_changed: false,
            messages: VecDeque::new(),
            client_time: -1.0,
            time_sync: false,
            ack_seq: false,
            encrypted_checksum: false,
            current_size: 0,
        }
    }
}

impl NetworkBundle {
    // ACE: NetworkBundle.NeedsSending
    /// ACE `NeedsSending`.
    #[must_use]
    pub fn needs_sending(&self) -> bool {
        self.prop_changed || !self.messages.is_empty()
    }

    // ACE: NetworkBundle.HasMoreMessages
    /// ACE `HasMoreMessages`.
    #[must_use]
    pub fn has_more_messages(&self) -> bool {
        !self.messages.is_empty()
    }

    // ACE: NetworkBundle.ClientTime
    /// ACE `ClientTime` (getter). `-1` means no echo is owed.
    #[must_use]
    pub const fn client_time(&self) -> f32 {
        self.client_time
    }

    // ACE: NetworkBundle.ClientTime
    /// ACE `ClientTime` (setter).
    pub fn set_client_time(&mut self, value: f32) {
        self.client_time = value;
        self.prop_changed = true;
    }

    // ACE: NetworkBundle.TimeSync
    /// ACE `TimeSync` (getter).
    #[must_use]
    pub const fn time_sync(&self) -> bool {
        self.time_sync
    }

    // ACE: NetworkBundle.TimeSync
    /// ACE `TimeSync` (setter).
    pub fn set_time_sync(&mut self, value: bool) {
        self.time_sync = value;
        self.prop_changed = true;
    }

    // ACE: NetworkBundle.SendAck
    /// ACE `SendAck` (getter).
    #[must_use]
    pub const fn send_ack(&self) -> bool {
        self.ack_seq
    }

    // ACE: NetworkBundle.SendAck
    /// ACE `SendAck` (setter).
    pub fn set_send_ack(&mut self, value: bool) {
        self.ack_seq = value;
        self.prop_changed = true;
    }

    // ACE: NetworkBundle.Enqueue
    pub fn enqueue(&mut self, message: OutboundMessage) {
        self.current_size = self
            .current_size
            .wrapping_add(i32::try_from(message.data.len()).unwrap_or(i32::MAX));
        self.messages.push_back(message);
    }

    // ACE: NetworkBundle.Dequeue
    pub fn dequeue(&mut self) -> Option<OutboundMessage> {
        self.messages.pop_front()
    }
}
