//! Queue 4 — the login queue..
//!
//! The whole consumer is a peek: it checks that the blob is at least four bytes long and starts with
//! `0xF7DE`, and hands it to `chatclient.dll` through the client's chat manager. **Every other message
//! that arrives on queue 4 is discarded.**
//!
//! That is why the login and character-management replies (`0xF630`, `0xF643`, `0xF651`, `0xF653`,
//! `0xF655`, `0xF658`, `0xF659`, `0xF7C1`, `0xF7DC`, `0xF7DF`, `0xF7E1`) are sent by ACE on the
//! **UI queue**, not on the login queue, even though the client *sends* the corresponding requests
//! on queue 4. This asymmetric queue map is intentional.

use crate::client_session::{DropReason, SessionEvent};
use dereth_primitives::IncomingMessage;
use dereth_protocol::Opcode;

/// Dispatch one queue-4 blob.
pub fn dispatch(m: &IncomingMessage) -> SessionEvent {
    if m.opcode == Opcode::COMMUNICATION_TURBINE_CHAT.0 {
        // Keep the exact body at this boundary. The host's port of chatclient.dll decodes
        // recognized callback forms; unsupported forms must not be rewritten or truncated here.
        SessionEvent::TurbineChat(m.body.clone())
    } else {
        SessionEvent::Dropped {
            queue: m.queue,
            opcode: Opcode(m.opcode),
            reason: DropReason::LoginQueueIgnoresEverythingButTurbineChat,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::{NetBlobId, NetQueue, RecipientId};

    fn msg(opcode: u32, body: Vec<u8>) -> IncomingMessage {
        IncomingMessage {
            opcode,
            queue: NetQueue::Logon,
            sender: RecipientId(0),
            blob_id: NetBlobId(0),
            body,
        }
    }

    /// Oracle: only `0xF7DE` is inspected on the logon queue.
    #[test]
    fn only_turbine_chat_survives_the_login_queue() {
        let chat = dispatch(&msg(0xF7DE, vec![1, 2, 3]));
        assert_eq!(chat, SessionEvent::TurbineChat(vec![1, 2, 3]));

        // Even a message the client understands perfectly well elsewhere is dropped here.
        let other = dispatch(&msg(0xF7E1, vec![]));
        assert!(matches!(
            other,
            SessionEvent::Dropped {
                reason: DropReason::LoginQueueIgnoresEverythingButTurbineChat,
                ..
            }
        ));
    }
}
