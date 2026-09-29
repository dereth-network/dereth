//! The twelve net queues, their drain API, and the queue -> recipient mapping.
//!
//! The packet controller has twelve queue slots, all null until a subsystem registers
//! one. The controller's add-to-queue:
//!
//! takes the blob's queue id; a queue id of 0 or 12 and above is dropped, and so is one whose
//! queue slot is null (both format the sender's IP for a log line). Otherwise the blob
//! is referenced and appended to the tail of that queue,
//!
//! so the valid range is **1..=11**, an unregistered queue **silently discards**, and neither case
//! is an error. The client's net init registers exactly five, in the order 2, 10, 9, 4, 5.
//!
//! See `docs/networking/04-netblobs-and-queues.md` §§4-5.

use std::collections::VecDeque;

use dereth_transport::blob::NetBlobId;
use dereth_transport::indicator::CompletedBlob;

/// The twelve queue ids. The retail client names them the same way the server does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum Queue {
    /// Rejected by `add_received_blob`.
    Invalid = 0,
    /// Not registered by the client; inbound blobs are discarded.
    Event = 1,
    /// Registered and **never drained**, on purpose: control sending is outbound-only and the
    /// server answers on queue 9.
    Control = 2,
    /// Client-to-server object messages only.
    Weenie = 3,
    /// The logon event queue. Its only consumer looks for exactly
    /// `0xF7DE Communication_TurbineChat` and **discards everything else**.
    Login = 4,
    /// The dat cache / DDD queue.
    Database = 5,
    /// Not registered; inbound blobs are discarded.
    SecureControl = 6,
    /// Client -> server only (autonomous position).
    SecureWeenie = 7,
    /// Client -> server only; blob sending treats it as login-bound.
    SecureLogin = 8,
    /// The UI queue.
    Ui = 9,
    /// The smart box's queue.
    WorldObjects = 10,
    /// Not registered; inbound blobs are discarded.
    Observer = 11,
}

impl Queue {
    /// The twelve, in id order.
    pub const ALL: [Queue; 12] = [
        Queue::Invalid,
        Queue::Event,
        Queue::Control,
        Queue::Weenie,
        Queue::Login,
        Queue::Database,
        Queue::SecureControl,
        Queue::SecureWeenie,
        Queue::SecureLogin,
        Queue::Ui,
        Queue::WorldObjects,
        Queue::Observer,
    ];

    /// Decode a wire queue id. Returns `None` for 0 and for anything at or above 12 — both of
    /// which are **drops, not errors**.
    #[must_use]
    pub fn from_wire(id: u16) -> Option<Self> {
        match id {
            1 => Some(Queue::Event),
            2 => Some(Queue::Control),
            3 => Some(Queue::Weenie),
            4 => Some(Queue::Login),
            5 => Some(Queue::Database),
            6 => Some(Queue::SecureControl),
            7 => Some(Queue::SecureWeenie),
            8 => Some(Queue::SecureLogin),
            9 => Some(Queue::Ui),
            10 => Some(Queue::WorldObjects),
            11 => Some(Queue::Observer),
            _ => None,
        }
    }

    #[must_use]
    pub fn id(self) -> u16 {
        self as u16
    }

    /// Does this queue's traffic go to the **login** server rather than the world server?
    ///
    /// The blob send's login-bound test is `q >= 4 && (q < 6 || q == 8)`, i.e. queues 4, 5
    /// and 8. Against a single-process server such as ACE the two recipient ids are identical, so a
    /// rebuild that hard-codes one recipient passes every local test and breaks on a retail-style
    /// split deployment.
    #[must_use]
    pub fn is_login_bound(self) -> bool {
        let q = self.id();
        q >= 4 && (q < 6 || q == 8)
    }

    /// The ordering type stamped into the blob id's high dword.
    ///
    /// `0x23000000` for the login-bound queues, `0x03000000` otherwise. The `0x20` is bit 61, which
    /// nothing in the client reads back — but it is on the wire, so it must be right.
    #[must_use]
    pub fn ordering_type(self) -> u32 {
        if self.is_login_bound() {
            NetBlobId::ORDERING_TYPE_LOGIN
        } else {
            NetBlobId::ORDERING_TYPE_WORLD
        }
    }
}

/// The priority given to every outbound blob. There is no other priority in the
/// client, so the flow queue's priority heap is effectively FIFO.
pub const SEND_PRIORITY: u32 = 5;

/// The packet controller's twelve queue slots.
///
/// A slot is `None` until a subsystem registers it, and a blob for an unregistered slot is dropped
/// without complaint. That is not a defensive measure — queues 1, 6, 7 and 11 are never registered
/// in the retail client at all.
#[derive(Debug, Default)]
pub struct NetQueues {
    slots: [Option<VecDeque<CompletedBlob>>; 12],
    /// Blobs dropped because the queue id was out of range or the slot was unregistered. The client
    /// formats the sender's IP for a log line here; counting is enough for a test.
    pub dropped: usize,
}

impl NetQueues {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The five the retail client registers, in its own order:
    /// **2, 10, 9, 4, 5**.
    #[must_use]
    pub fn with_retail_registrations() -> Self {
        let mut q = Self::new();
        for id in [
            Queue::Control,
            Queue::WorldObjects,
            Queue::Ui,
            Queue::Login,
            Queue::Database,
        ] {
            q.add_receive_queue(id);
        }
        q
    }

    /// Add a receive queue.
    pub fn add_receive_queue(&mut self, queue: Queue) {
        self.slots[queue.id() as usize] = Some(VecDeque::new());
    }

    #[must_use]
    pub fn is_registered(&self, queue: Queue) -> bool {
        self.slots[queue.id() as usize].is_some()
    }

    /// Add a received blob to its queue.
    ///
    /// Returns `true` if the blob was filed. A `false` is a **drop, not an error**: queue id 0 or
    /// >= 12, or an unregistered slot.
    pub fn add_received_blob(&mut self, blob: CompletedBlob) -> bool {
        let Some(queue) = Queue::from_wire(blob.queue_id) else {
            self.dropped += 1;
            return false;
        };
        let Some(slot) = self.slots[queue.id() as usize].as_mut() else {
            self.dropped += 1;
            return false;
        };
        slot.push_back(blob);
        true
    }

    /// Pop the oldest blob from one queue.
    pub fn pop(&mut self, queue: Queue) -> Option<CompletedBlob> {
        self.slots[queue.id() as usize].as_mut()?.pop_front()
    }

    #[must_use]
    pub fn len(&self, queue: Queue) -> usize {
        self.slots[queue.id() as usize]
            .as_ref()
            .map_or(0, VecDeque::len)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots
            .iter()
            .all(|s| s.as_ref().is_none_or(VecDeque::is_empty))
    }

    /// The order the retail frame drains the queues in, including the client-specific override:
    ///
    /// ```text
    /// process the logon-event queue     (queue 4)
    /// run packet-controller send time
    /// run cache time                    (queue 5)
    /// run UI-element-manager time
    /// run smart-box time                (queue 10)
    ///    ... run the UI queue from the UI system (queue 9)
    /// ```
    ///
    /// Queue 2 is registered and deliberately absent: nothing drains it, so it would grow without
    /// bound if a server ever sent on it.
    pub const DRAIN_ORDER: [Queue; 4] = [
        Queue::Login,
        Queue::Database,
        Queue::WorldObjects,
        Queue::Ui,
    ];

    /// Pop the next blob in frame-drain order.
    pub fn pop_next(&mut self) -> Option<CompletedBlob> {
        for queue in Self::DRAIN_ORDER {
            if let Some(blob) = self.pop(queue) {
                return Some(blob);
            }
        }
        None
    }
}

/// The opcode looks for on queue 4. **Everything else
/// on that queue is discarded.**
///
/// Its payload lives inside `chatclient.dll` and is not decoded; the blob is handed on intact.
pub const COMMUNICATION_TURBINE_CHAT: u32 = 0xF7DE;

#[cfg(test)]
mod tests {
    use super::*;

    fn blob(queue_id: u16, opcode: u32) -> CompletedBlob {
        CompletedBlob {
            id: NetBlobId(1),
            queue_id,
            sender: 1,
            payload: opcode.to_le_bytes().to_vec(),
        }
    }

    /// Oracle: `docs/networking/04-netblobs-and-queues.md` §4, the twelve-row queue table.
    #[test]
    fn all_twelve_queue_ids_exist_and_number_correctly() {
        for (i, q) in Queue::ALL.iter().enumerate() {
            assert_eq!(usize::from(q.id()), i);
        }
        for id in 1..12u16 {
            assert_eq!(Queue::from_wire(id).map(Queue::id), Some(id));
        }
    }

    /// Ids 0 and >= 12 are **dropped, not errors**.
    ///
    /// Oracle: the add-to-queue's `q == 0 || q >= 12` test.
    #[test]
    fn queue_zero_and_twelve_and_above_are_dropped_not_errors() {
        assert_eq!(Queue::from_wire(0), None);
        assert_eq!(Queue::from_wire(12), None);
        assert_eq!(Queue::from_wire(0xFFFF), None);

        let mut q = NetQueues::with_retail_registrations();
        assert!(!q.add_received_blob(blob(0, 0xF7B0)));
        assert!(!q.add_received_blob(blob(12, 0xF7B0)));
        assert!(!q.add_received_blob(blob(0xFFFF, 0xF7B0)));
        assert_eq!(q.dropped, 3);
        assert!(q.is_empty());
    }

    /// An unregistered queue silently discards. Queues 1, 6, 7 and 11 are never registered.
    #[test]
    fn an_unregistered_queue_silently_discards() {
        let q = NetQueues::with_retail_registrations();
        for unregistered in [
            Queue::Event,
            Queue::Weenie,
            Queue::SecureControl,
            Queue::SecureWeenie,
            Queue::SecureLogin,
            Queue::Observer,
        ] {
            assert!(!q.is_registered(unregistered), "{unregistered:?}");
        }
        for registered in [
            Queue::Control,
            Queue::Login,
            Queue::Database,
            Queue::Ui,
            Queue::WorldObjects,
        ] {
            assert!(q.is_registered(registered), "{registered:?}");
        }

        let mut q = NetQueues::with_retail_registrations();
        assert!(!q.add_received_blob(blob(1, 0xF7B0)));
        assert!(!q.add_received_blob(blob(11, 0xF7B0)));
        assert_eq!(q.dropped, 2);
        assert!(q.is_empty());
    }

    /// Queue 2 is registered and never drained, on purpose.
    /// Compatibility note #1, resolved by the audit.
    #[test]
    fn queue_two_is_registered_and_never_drained() {
        let mut q = NetQueues::with_retail_registrations();
        assert!(q.is_registered(Queue::Control));
        assert!(q.add_received_blob(blob(2, 0xF6EA)), "it is filed...");
        assert_eq!(q.len(Queue::Control), 1);
        assert!(
            !NetQueues::DRAIN_ORDER.contains(&Queue::Control),
            "...and never drained"
        );
        assert!(q.pop_next().is_none());
    }

    /// `poll` returns whole blobs in queue-drain order: 4, then 5, then 10, then 9.
    #[test]
    fn blobs_come_out_in_frame_drain_order() {
        let mut q = NetQueues::with_retail_registrations();
        // Filed in the opposite order to the drain order.
        assert!(q.add_received_blob(blob(9, 0xF7B0)));
        assert!(q.add_received_blob(blob(10, 0xF745)));
        assert!(q.add_received_blob(blob(5, 0xF7E3)));
        assert!(q.add_received_blob(blob(4, 0xF7DE)));

        let order: Vec<u16> = std::iter::from_fn(|| q.pop_next())
            .map(|b| b.queue_id)
            .collect();
        assert_eq!(order, vec![4, 5, 10, 9]);
    }

    /// Within one queue, blobs come out in arrival order — the client does not reorder packets, and
    /// appending to a queue's tail is a FIFO.
    #[test]
    fn one_queue_is_fifo() {
        let mut q = NetQueues::with_retail_registrations();
        for op in [1u32, 2, 3] {
            assert!(q.add_received_blob(blob(9, op)));
        }
        let ops: Vec<u32> = std::iter::from_fn(|| q.pop(Queue::Ui))
            .map(|b| u32::from_le_bytes([b.payload[0], b.payload[1], b.payload[2], b.payload[3]]))
            .collect();
        assert_eq!(ops, vec![1, 2, 3]);
    }

    /// Queues 4, 5 and 8 go to the **login** recipient; everything else to the world recipient.
    ///
    /// Oracle: the blob send's `toLogin = (q >= 4 && (q < 6 || q == 8))`.
    /// `docs/networking/04-netblobs-and-queues.md` §5.1.
    #[test]
    fn queues_4_5_and_8_are_login_bound_and_nothing_else_is() {
        let login: Vec<u16> = Queue::ALL
            .iter()
            .filter(|q| q.is_login_bound())
            .map(|q| q.id())
            .collect();
        assert_eq!(login, vec![4, 5, 8]);
    }

    /// And the ordering type follows the same split: `0x23000000` login-bound, `0x03000000`
    /// otherwise. Compatibility note #38: bit 61 has no reader but is on the wire.
    #[test]
    fn ordering_type_follows_the_login_split() {
        assert_eq!(Queue::Login.ordering_type(), 0x2300_0000);
        assert_eq!(Queue::Database.ordering_type(), 0x2300_0000);
        assert_eq!(Queue::SecureLogin.ordering_type(), 0x2300_0000);
        assert_eq!(Queue::Ui.ordering_type(), 0x0300_0000);
        assert_eq!(Queue::Weenie.ordering_type(), 0x0300_0000);
        assert_eq!(Queue::Control.ordering_type(), 0x0300_0000);
    }

    /// Every outbound blob carries priority 5 and there is no other priority in the client.
    #[test]
    fn the_only_send_priority_is_five() {
        assert_eq!(SEND_PRIORITY, 5);
    }
}
