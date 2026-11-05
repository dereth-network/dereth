//! The queues: which ones the client receives on, the order it reads them in each frame, and the
//! queue each client message goes out on.
//!
//! The client registers four receive queues, in the order 2, 1, 4, 5 (control, events, logon,
//! data). Each frame it reads the data queue first (the data download), then the events queue
//! (everything of the game, from the login and the world servers), then the control queue (read
//! and thrown away), then the logon queue, where it keeps only the chat service's messages. A
//! message for any other queue is dropped.
//!
//! A message the client sends is stamped from one of three counters ([`counter_of`]): the ordered
//! one for the game-events queue, the data one for the data download and chat, the unordered one
//! for the rest.

use auberean_protocol::{queue, ClientMessage};
use dereth_client_net::queues::{Queue, ReceiveQueues};

/// The client's receive queues, registered and drained in its own orders.
// D2-00073, D2-00049, D2-00131
#[must_use]
pub fn receive_queues() -> ReceiveQueues {
    ReceiveQueues {
        registered: vec![Queue::Control, Queue::Event, Queue::Login, Queue::Database],
        drain_order: vec![Queue::Database, Queue::Event, Queue::Control, Queue::Login],
    }
}

/// The one message type the logon queue keeps: the chat service's packet. Anything else on it is
/// read and dropped.
pub const LOGON_QUEUE_KEEPS: u32 = 0x100D3;

/// Whether a message read from `queue` reaches the game: the control queue's never do, and the
/// logon queue's only when they are the chat service's.
// D2-00053, D2-00054
#[must_use]
pub fn is_kept(queue_id: u16, message_type: u32) -> bool {
    match queue_id {
        queue::CONTROL => false,
        queue::LOGON => message_type == LOGON_QUEUE_KEEPS,
        _ => true,
    }
}

/// The queue a client message goes out on: the data download's on the data queue; the character
/// list's requests and chat on the logon queue; everything of the world on the game-events queue.
#[must_use]
pub fn queue_of(m: &ClientMessage) -> u16 {
    match m {
        ClientMessage::DataHeld(_)
        | ClientMessage::DataRequest(_)
        | ClientMessage::DownloadEnds(_) => queue::DATABASE,
        ClientMessage::PlayCharacter(_)
        | ClientMessage::LogOff(_)
        | ClientMessage::DeleteCharacter(_)
        | ClientMessage::RestoreCharacter(_)
        | ClientMessage::CreateCharacter(_)
        | ClientMessage::Chat(_) => queue::LOGON,
        ClientMessage::ServerCall(_)
        | ClientMessage::Position(_)
        | ClientMessage::ForceCreate(_)
        | ClientMessage::LookAt(_)
        | ClientMessage::TurnHead(_)
        | ClientMessage::SceneRenderingComplete(_) => queue::WEENIE,
    }
}

/// The queue the client's input goes out on besides the game-events queue; its messages are
/// ordered too.
pub const INPUT_QUEUE: u16 = 7;

/// The counter an outgoing message's ordering stamp comes from, which fixes its ordering type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Counter {
    /// Ordered (`0x01`): counted up before each send, back down when the send is refused, and
    /// restarted at zero when the client enters the world, so the first after that is 1.
    Ordered,
    /// Unordered (`0x03`): counted up before each send.
    Unordered,
    /// The data download's and chat's (`0x23`): the stamp is the count so far, from 0.
    Data,
}

impl Counter {
    /// The ordering type, in place in the blob id's high dword.
    #[must_use]
    pub const fn ordering_type(self) -> u32 {
        match self {
            Self::Ordered => 0x0100_0000,
            Self::Unordered => 0x0300_0000,
            Self::Data => 0x2300_0000,
        }
    }
}

/// The counter a client message is stamped from: chat (on the logon queue) and everything on the
/// data queue from the data counter; the game-events and input queues' from the ordered counter;
/// anything else from the unordered counter. The logon queue carries both unordered messages and
/// chat, so the counter follows the message, not only its queue.
// D2-05338, D2-12840, D2-00020
#[must_use]
pub fn counter_of(m: &ClientMessage) -> Counter {
    let q = queue_of(m);
    if m.message_type() == LOGON_QUEUE_KEEPS || q == queue::DATABASE {
        Counter::Data
    } else if q == queue::WEENIE || q == INPUT_QUEUE {
        Counter::Ordered
    } else {
        Counter::Unordered
    }
}
