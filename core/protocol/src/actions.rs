//! The game-action wrapper and the index of the 157 senders —
//! `docs/networking/messages/11-game-actions.md`.
//!
//! Every message this client sends to the *world* server is a game action:
//!
//! ```text
//! +0x00  uint32  0xF7B1       the order header's magic
//! +0x04  uint32  stamp        the next UI counter
//! +0x08  uint32  sub-type
//! +0x0C  ...     payload, DWORD-aligned after every variable-length field
//! ```
//!
//! The stamp comes from a **single global counter** shared by every category, incremented before the
//! send and **rolled back** by when the send fails — the
//! server drops everything after a hole in the sequence. The counter lives in
//! `dereth_client_net::client_session::outbound`, because it is state, not a codec.
//!
//! Twelve outbound messages -- four on the control queue, five on the logon queue and three on the
//! database queue -- are **not** game actions and carry no `OrderedActionHeader`: they start with
//! the opcode dword directly. See [`OutboundKind`].

use crate::archive::{Reader, Writer};
use crate::error::MessageError;
use crate::opcodes::{Direction, Opcode, OPCODES};
use crate::order::OrderedActionHeader;
use crate::Message;
use dereth_primitives::NetQueue;

/// How an outbound message is framed and where it goes.
///
/// The Control-, Logon- and Database-queue messages carry no `OrderedActionHeader`. Queues
/// **4, 5 and 8 go to the login server**, everything else to the world server. Against ACE both
/// ids are the same, so a hard-coded recipient passes every local test and breaks on a split
/// deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboundKind {
    /// An ordered game action on the Weenie queue (3), wrapped in `OrderedActionHeader`.
    GameAction,
    /// A bare `[opcode][body]` blob on the queue named.
    Bare(NetQueue),
}

impl OutboundKind {
    /// The queue this message is sent on.
    #[must_use]
    pub fn queue(self) -> NetQueue {
        match self {
            Self::GameAction => NetQueue::Weenie,
            Self::Bare(q) => q,
        }
    }

    /// Whether the send goes to the login/patch server rather than the world server.
    ///
    /// Queues 4 (Logon), 5 (ClientCache/Database) and 8 (SecureLogin, which this
    /// client never uses) go to the login server. The blob send also stamps a
    /// different ordering-type nibble for that recipient — `0x23000000` versus `0x03000000` — which
    /// is the session layer's business, not this crate's.
    #[must_use]
    pub fn goes_to_login_server(self) -> bool {
        matches!(self.queue(), NetQueue::Logon | NetQueue::ClientCache)
    }
}

/// How the client frames a given outbound opcode.
///
/// Derived from the master opcode table's queue column, which splits the senders exactly: a
/// sender on the Weenie queue is a game action, and the Control, Logon and Database senders are not.
#[must_use]
pub fn outbound_kind(op: Opcode) -> Option<OutboundKind> {
    let info = op.info()?;
    if !matches!(info.direction, Direction::C2S | Direction::Both) {
        return None;
    }
    match info.send_queue? {
        NetQueue::Weenie => Some(OutboundKind::GameAction),
        q => Some(OutboundKind::Bare(q)),
    }
}

/// Every opcode this client sends as a game action, ascending.
pub fn game_action_opcodes() -> impl Iterator<Item = Opcode> {
    OPCODES
        .iter()
        .filter(|i| {
            matches!(i.direction, Direction::C2S | Direction::Both)
                && i.send_queue == Some(NetQueue::Weenie)
        })
        .map(|i| i.opcode)
}

/// Frame a message as a game action: `[0xF7B1][stamp][sub-type][payload]`.
///
/// The payload's alignment origin is offset 12 — the sub-type dword is at 8 and the body starts at
/// 12 — because the align rule is computed from the **blob's** start. Framing the body with
/// [`crate::write_body`] and then prefixing it would align on the wrong origin.
pub fn pack_action<M: Message>(stamp: u32, m: &M) -> Result<Vec<u8>, MessageError> {
    let mut w = Writer::new();
    OrderedActionHeader { stamp }.write(&mut w);
    w.u32(M::OPCODE.0);
    m.write(&mut w)?;
    Ok(w.into_inner())
}

/// Frame an arbitrary already-encoded body as a game action.
///
/// `body` must have been produced with an alignment origin of 12.
#[must_use]
pub fn pack_action_raw(stamp: u32, sub_type: Opcode, body: &[u8]) -> Vec<u8> {
    let mut w = Writer::new();
    OrderedActionHeader { stamp }.write(&mut w);
    w.u32(sub_type.0);
    w.bytes(body);
    w.into_inner()
}

/// A game action, split back into its parts. The body reader's origin is set to 12.
#[derive(Debug)]
pub struct UnpackedAction<'a> {
    pub stamp: u32,
    pub sub_type: Opcode,
    pub body: Reader<'a>,
}

/// Undo [`pack_action`]: read the `OrderedActionHeader`, the sub-type, and hand back a reader over the body
/// whose alignment origin is correct.
pub fn unpack_action(blob: &[u8]) -> Result<UnpackedAction<'_>, MessageError> {
    let mut r = Reader::new(blob);
    let hdr = OrderedActionHeader::read(&mut r)?;
    let sub_type = Opcode(r.u32()?);
    let consumed = r.position();
    Ok(UnpackedAction {
        stamp: hdr.stamp,
        sub_type,
        body: Reader::with_origin(&blob[consumed..], consumed),
    })
}

/// A writer positioned for a game action's payload: origin 12, so the alignment rule matches.
#[must_use]
pub fn action_body_writer() -> Writer {
    Writer::with_origin(OrderedActionHeader::PACK_SIZE + 4)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comms::CommunicationTalk;
    use crate::items::InventoryDropItem;
    use dereth_primitives::ObjectId;

    /// Oracle: `docs/networking/messages/11-game-actions.md` —
    /// `[0xF7B1][stamp][sub-type][payload]`, with the payload starting at offset 12 of the blob.
    #[test]
    fn a_game_action_is_the_order_header_then_the_sub_type_then_the_body() {
        let m = InventoryDropItem {
            item: ObjectId(0x5000_0001),
        };
        let blob = pack_action(7, &m).unwrap();
        assert_eq!(&blob[0..4], &0xF7B1u32.to_le_bytes());
        assert_eq!(&blob[4..8], &7u32.to_le_bytes());
        assert_eq!(&blob[8..12], &0x001Bu32.to_le_bytes());
        assert_eq!(&blob[12..16], &0x5000_0001u32.to_le_bytes());
        assert_eq!(blob.len(), 16);

        let a = unpack_action(&blob).unwrap();
        assert_eq!(a.stamp, 7);
        assert_eq!(a.sub_type, InventoryDropItem::OPCODE);
        assert_eq!(
            a.body.blob_offset(),
            12,
            "the body's alignment origin is the blob offset"
        );
    }

    /// The alignment origin matters: a string in the payload pads relative to the **blob's** start,
    /// which is 12 bytes before the payload, not zero.
    ///
    /// `Communication_Talk` is `[0xF7B1][stamp][0x0015][string]`. "hi" packs as 2 length + 2 bytes
    /// = 4 at blob offset 12, which is already aligned, so no pad. A three-character message needs
    /// three bytes of pad — and it needs them *because* the origin is 12, not because it is 0.
    #[test]
    fn the_payload_aligns_on_the_blobs_origin() {
        let blob = pack_action(
            1,
            &CommunicationTalk {
                message: "hi".into(),
            },
        )
        .unwrap();
        assert_eq!(
            blob.len(),
            16,
            "12 header + 2 length + 2 bytes, already aligned"
        );

        let blob = pack_action(
            1,
            &CommunicationTalk {
                message: "hey".into(),
            },
        )
        .unwrap();
        assert_eq!(blob.len(), 20, "12 + 2 + 3 + 3 pad");
        assert_eq!(&blob[17..20], &[0, 0, 0]);
    }

    /// Oracle: the client has 157 game-action senders
    /// (`docs/networking/messages/11-game-actions.md`), and the early clients' spell research
    /// panel one more (`0x004B`). The master table's Weenie-queue rows are exactly that set.
    #[test]
    fn the_master_table_holds_the_documented_number_of_game_actions() {
        assert_eq!(game_action_opcodes().count(), 158);
        // And they are all C2S or both-ways.
        for op in game_action_opcodes() {
            assert_eq!(outbound_kind(op), Some(OutboundKind::GameAction), "{op:?}");
        }
    }

    /// The four Control-queue messages, the five Logon-queue
    /// ones and the three Database-queue ones are **not** game actions and carry no `OrderedActionHeader`; and
    /// the Logon and Database queues go to the login server.
    #[test]
    fn the_non_game_actions_are_bare_and_the_queue_map_is_a_contract() {
        for (op, queue) in [
            (Opcode::OBJECT_SEND_FORCE_OBJDESC, NetQueue::Control),
            (
                Opcode::ADMIN_SEND_ADMIN_GET_SERVER_VERSION,
                NetQueue::Control,
            ),
            (Opcode::SOCIAL_SEND_FRIENDS_COMMAND, NetQueue::Control),
            (
                Opcode::ADMIN_SEND_ADMIN_RESTORE_CHARACTER,
                NetQueue::Control,
            ),
            (Opcode::LOGIN_EXECUTE_LOG_OFF, NetQueue::Logon),
            (Opcode::CHARACTER_CHARACTER_DELETE, NetQueue::Logon),
            (Opcode::CHARACTER_SEND_CHAR_GEN_RESULT, NetQueue::Logon),
            (Opcode::LOGIN_SEND_ENTER_WORLD, NetQueue::Logon),
            (Opcode::LOGIN_SEND_ENTER_WORLD_REQUEST, NetQueue::Logon),
            (Opcode::DDD_REQUEST_DATA_MESSAGE, NetQueue::ClientCache),
            (
                Opcode::DDD_INTERROGATION_RESPONSE_MESSAGE,
                NetQueue::ClientCache,
            ),
            (Opcode::DDD_ON_END_DDD, NetQueue::ClientCache),
        ] {
            assert_eq!(outbound_kind(op), Some(OutboundKind::Bare(queue)), "{op:?}");
        }

        assert!(!OutboundKind::GameAction.goes_to_login_server());
        assert!(!OutboundKind::Bare(NetQueue::Control).goes_to_login_server());
        assert!(OutboundKind::Bare(NetQueue::Logon).goes_to_login_server());
        assert!(OutboundKind::Bare(NetQueue::ClientCache).goes_to_login_server());
    }

    /// `0xF7EB` has **no sender**: the client's end-of-patching message is `0xF7EA` both ways. The
    /// community catalogue says otherwise; see `docs/networking/messages/11-game-actions.md` §4.
    #[test]
    fn the_client_never_sends_f7eb() {
        assert_eq!(outbound_kind(Opcode::DDD_END_DDDMESSAGE), None);
        assert_eq!(
            outbound_kind(Opcode::DDD_ON_END_DDD),
            Some(OutboundKind::Bare(NetQueue::ClientCache))
        );
    }

    /// A body written through [`action_body_writer`] can be framed with [`pack_action_raw`] and come
    /// out identical to the one-shot path — that is what makes the two APIs interchangeable.
    #[test]
    fn the_two_framing_paths_agree() {
        let m = CommunicationTalk {
            message: "hey".into(),
        };
        let one_shot = pack_action(3, &m).unwrap();
        let mut w = action_body_writer();
        m.write(&mut w).unwrap();
        let two_step = pack_action_raw(3, CommunicationTalk::OPCODE, w.as_slice());
        assert_eq!(one_shot, two_step);
    }
}
