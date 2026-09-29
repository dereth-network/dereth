// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/MessageFragment.cs
//
// `PacketFragment.cs`'s two constants live here too.

use empyrean_common::dotnet::CsCast;

use crate::server_packet::ServerPacketFragment;
use crate::OutboundMessage;

/// ACE `PacketFragment.MaxFragementSize`.
pub const MAX_FRAGEMENT_SIZE: i32 = 464;
/// ACE `PacketFragment.MaxFragmentDataSize`.
pub const MAX_FRAGMENT_DATA_SIZE: i32 = 448;
/// ACE `PacketFragmentHeader.HeaderSize`.
pub const FRAGMENT_HEADER_SIZE: i32 = 16;

/// ACE `MessageFragment`: one outbound message being cut into fragments.
#[derive(Debug, Clone)]
pub struct MessageFragment {
    pub message: OutboundMessage,
    pub sequence: u32,
    pub index: u16,
    pub count: u16,
    pub data_remaining: i32,
    pub tail_sent: bool,
}

impl MessageFragment {
    // ACE: MessageFragment.MessageFragment
    #[must_use]
    pub fn new(message: OutboundMessage, sequence: u32) -> Self {
        let data_length = i32::try_from(message.data.len()).unwrap_or(i32::MAX);
        // `(ushort)(Math.Ceiling((double)DataLength / MaxFragmentDataSize))`.
        let count: u16 = (f64::from(data_length) / f64::from(MAX_FRAGMENT_DATA_SIZE))
            .ceil()
            .cs_cast();
        Self {
            message,
            sequence,
            index: 0,
            count,
            data_remaining: data_length,
            tail_sent: count == 1,
        }
    }

    // ACE: MessageFragment.DataLength
    /// ACE `DataLength`.
    #[must_use]
    pub fn data_length(&self) -> i32 {
        i32::try_from(self.message.data.len()).unwrap_or(i32::MAX)
    }

    // ACE: MessageFragment.NextSize
    /// ACE `NextSize`.
    #[must_use]
    pub fn next_size(&self) -> i32 {
        FRAGMENT_HEADER_SIZE + self.data_remaining.min(MAX_FRAGMENT_DATA_SIZE)
    }

    // ACE: MessageFragment.TailSize
    /// ACE `TailSize`: the size of the last fragment, header included.
    ///
    /// Not ACE's (a fix): ACE's `length % 448` is 0 for a message whose
    /// length is a whole multiple of 448, so it answered 16 although that tail carries 448 bytes,
    /// and `SendBundle` could place it in a packet with less room and send a datagram over 484
    /// bytes. Retail never built one over 484.
    #[must_use]
    pub fn tail_size(&self) -> i32 {
        let rem = self.data_length() % MAX_FRAGMENT_DATA_SIZE;
        let tail = if rem == 0 && self.data_length() > 0 {
            MAX_FRAGMENT_DATA_SIZE
        } else {
            rem
        };
        FRAGMENT_HEADER_SIZE + tail
    }

    // ACE: MessageFragment.GetTailFragment
    pub fn get_tail_fragment(&mut self) -> Option<ServerPacketFragment> {
        let index = self.count.wrapping_sub(1);
        self.tail_sent = true;
        self.create_server_fragment(index)
    }

    // ACE: MessageFragment.GetNextFragment
    pub fn get_next_fragment(&mut self) -> Option<ServerPacketFragment> {
        let index = self.index;
        self.index = self.index.wrapping_add(1);
        self.create_server_fragment(index)
    }

    // ACE: MessageFragment.CreateServerFragment
    /// `None` where ACE throws (an index past the count, or no data left); `SendBundle` never
    /// reaches those with a non-empty message.
    fn create_server_fragment(&mut self, index: u16) -> Option<ServerPacketFragment> {
        if index >= self.count {
            return None;
        }
        let position = i32::from(index) * MAX_FRAGMENT_DATA_SIZE;
        if position > self.data_length() {
            return None;
        }
        if self.data_remaining <= 0 {
            return None;
        }
        let data_to_send = (self.data_length() - position).min(MAX_FRAGMENT_DATA_SIZE);
        if self.data_remaining < data_to_send {
            return None;
        }
        #[allow(clippy::cast_sign_loss)]
        let (start, end) = (position as usize, (position + data_to_send) as usize);
        let mut fragment = ServerPacketFragment::new(self.message.data[start..end].to_vec());
        fragment.header.blob_id_low = self.sequence;
        fragment.header.blob_id_high = 0x8000_0000;
        fragment.header.num_frags = self.count;
        fragment.header.blob_num = index;
        fragment.header.queue_id = self.message.group as u16;
        self.data_remaining -= data_to_send;
        Some(fragment)
    }
}
