//! The outbound path: the single global game-action counter and its rollback, plus queue selection.
//!
//! Source: `docs/networking/messages/11-game-actions.md` (outbound queue selection), transcribing
//! the counter's advance, its put-back and its outright set.
//!
//! **The counter is global and must roll back.** One counter is shared by all 157
//! senders; it is incremented before the send and decremented again when the send fails, so the
//! client's action sequence has no holes. The server drops everything after a hole, so a rebuild
//! that forgets the rollback loses every subsequent action after the first failed send.
//!
//! ACE does not validate `OrderedActionHeader.stamp_` at all, so a broken counter passes every ACE test. That
//! is why this is tested directly rather than through the server.

use dereth_primitives::NetQueue;
use dereth_protocol::actions::{pack_action, OutboundKind};
use dereth_protocol::{Message, MessageError};

/// The stamp the **last** game action carried.
///
/// Holding the last value rather than the next one is not a stylistic choice: it is what the
/// client holds, and the difference is visible on the wire. The client's counter
/// **pre**-increments —
///
/// ```text
/// counter = counter + 1;
/// return counter;
/// ```
///
/// So a fresh client's very first game action carries **1**, not 0, and setting the counter to 0
/// means "the next action is 1 again" rather than "the next action is 0".
///
/// Holding the *next* value instead makes the first stamp 0 and every stamp after it one low.
/// Nothing in this workspace can see that: ACE does not validate
/// `OrderedActionHeader.stamp_` at all, and a test that asserts the counter's *increments* rather
/// than its base misses it. The capture corpus is the only oracle that can, and it is unanimous —
/// the first game action of **all five** recorded sessions that contain one carries stamp 1
/// (`fixtures/message-corpus/{first-login-walk-jump,early-inventory-and-casting,short-second-connection,long-solo-play,short-play-with-training}`).
#[derive(Debug, Clone, Copy, Default)]
pub struct ActionCounter {
    /// Last action stamp issued. Zero at construction, so the first stamp issued is 1.
    last: u32,
}

impl ActionCounter {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The value the next action will carry — the last stamp plus 1.
    #[must_use]
    pub fn peek(&self) -> u32 {
        self.last.wrapping_add(1)
    }

    /// Advance, then hand back the new value.
    ///
    /// Named for the client's advance-and-return step; it is not `Iterator::next` and never ends.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u32 {
        self.last = self.last.wrapping_add(1);
        self.last
    }

    /// Put it back.
    pub fn rollback(&mut self) {
        self.last = self.last.wrapping_sub(1);
    }

    /// Set the counter outright.
    ///
    /// Setting it to 0 therefore makes the next action's stamp 1 again. The log-off execution
    /// calls it when the server's `0xF653` comes back; that is
    /// the `LOGIN_EXECUTE_LOG_OFF` arm of [`crate::client_session::Session::tick`].
    pub fn set(&mut self, v: u32) {
        self.last = v;
    }
}

/// Where a message goes and how it is framed, ready to hand to [`dereth_primitives::Transport::send`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundBlob {
    pub queue: NetQueue,
    /// Whether the transport should mark the blob ordered. Only game actions are.
    pub ordered: bool,
    pub payload: Vec<u8>,
    /// The stamp this blob consumed, when it is a game action. Kept so a failed send can roll the
    /// counter back to exactly this value.
    pub stamp: Option<u32>,
}

/// Build an outbound blob for a message the client sends, choosing the framing and the queue from
/// the master opcode table.
///
/// Game actions consume a counter value; everything else does not.
pub fn build<M: Message>(counter: &mut ActionCounter, m: &M) -> Result<OutboundBlob, MessageError> {
    let kind =
        dereth_protocol::actions::outbound_kind(M::OPCODE).ok_or(MessageError::Unencodable {
            field: "outbound",
            reason: "the master opcode table has no client sender for this opcode",
        })?;
    match kind {
        OutboundKind::GameAction => {
            let stamp = counter.next();
            match pack_action(stamp, m) {
                Ok(payload) => Ok(OutboundBlob {
                    queue: NetQueue::Weenie,
                    ordered: true,
                    payload,
                    stamp: Some(stamp),
                }),
                Err(e) => {
                    // The counter was taken before the body was built; give it back rather than
                    // leaving a hole.
                    counter.rollback();
                    Err(e)
                }
            }
        }
        OutboundKind::Bare(queue) => Ok(OutboundBlob {
            queue,
            ordered: false,
            payload: dereth_protocol::write_blob(m)?,
            stamp: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::ObjectId;
    use dereth_protocol::comms::CommunicationTalk;
    use dereth_protocol::login::{LoginExecuteLogOffRequest, LoginSendEnterWorldRequest};

    /// One counter is shared by every category.
    #[test]
    fn one_counter_is_shared_by_every_category() {
        let mut c = ActionCounter::new();
        let a = build(
            &mut c,
            &CommunicationTalk {
                message: "hi".into(),
            },
        )
        .unwrap();
        let b = build(
            &mut c,
            &dereth_protocol::items::InventoryDropItem { item: ObjectId(1) },
        )
        .unwrap();
        assert_eq!(
            a.stamp,
            Some(1),
            "the client's first game action carries stamp 1"
        );
        assert_eq!(
            b.stamp,
            Some(2),
            "a different category advances the same counter"
        );
        assert_eq!(c.peek(), 3);
    }

    /// The rollback: the server drops everything after a hole in the sequence.
    #[test]
    fn a_failed_send_rolls_the_counter_back_so_there_is_no_hole() {
        let mut c = ActionCounter::new();
        let first = build(
            &mut c,
            &CommunicationTalk {
                message: "one".into(),
            },
        )
        .unwrap();
        assert_eq!(first.stamp, Some(1));

        // The transport reports failure: put the stamp back.
        c.rollback();
        assert_eq!(c.peek(), 1);

        let retry = build(
            &mut c,
            &CommunicationTalk {
                message: "one".into(),
            },
        )
        .unwrap();
        assert_eq!(
            retry.stamp,
            Some(1),
            "the retry reuses the stamp, leaving no hole"
        );
        let next = build(
            &mut c,
            &CommunicationTalk {
                message: "two".into(),
            },
        )
        .unwrap();
        assert_eq!(next.stamp, Some(2));
    }

    /// A message the codec refuses to encode must not consume a stamp either.
    #[test]
    fn an_unencodable_body_does_not_consume_a_stamp() {
        let mut c = ActionCounter::new();
        // A character with no windows-1252 representation makes `pstring` fail.
        let bad = CommunicationTalk {
            message: "\u{4E00}".into(),
        };
        assert!(build(&mut c, &bad).is_err());
        assert_eq!(c.peek(), 1, "the counter is unchanged");
    }

    /// Queue selection matches the four outbound protocol entry points; the non-game-actions carry no
    /// `OrderedActionHeader` and are not marked ordered.
    #[test]
    fn queue_selection_matches_proto_uis_entry_points() {
        let mut c = ActionCounter::new();

        let talk = build(
            &mut c,
            &CommunicationTalk {
                message: "hi".into(),
            },
        )
        .unwrap();
        assert_eq!(talk.queue, NetQueue::Weenie);
        assert!(talk.ordered);
        assert_eq!(&talk.payload[0..4], &0xF7B1u32.to_le_bytes());

        let enter = build(&mut c, &LoginSendEnterWorldRequest).unwrap();
        assert_eq!(enter.queue, NetQueue::Logon);
        assert!(!enter.ordered);
        assert_eq!(
            enter.payload,
            0xF7C8u32.to_le_bytes().to_vec(),
            "four bytes, opcode only"
        );
        assert_eq!(enter.stamp, None, "it does not consume a stamp");

        let log_off = build(
            &mut c,
            &LoginExecuteLogOffRequest {
                character: ObjectId(1),
            },
        )
        .unwrap();
        assert_eq!(log_off.queue, NetQueue::Logon);
        assert_eq!(log_off.payload.len(), 8);

        let force = build(
            &mut c,
            &dereth_protocol::objects::ObjectSendForceObjdesc { id: ObjectId(1) },
        )
        .unwrap();
        assert_eq!(force.queue, NetQueue::Control);
        assert!(!force.ordered);

        assert_eq!(c.peek(), 2, "only the game action took a stamp");
    }

    /// The counter wraps rather than panicking, and setting the event counter resets it.
    ///
    /// `set(v)` writes the counter itself, so `set(0)` leaves the *next* stamp at 1 — which is
    /// exactly what does and what
    /// `fixtures/message-corpus/early-inventory-and-casting` shows on the wire: 289, log off, then 1
    /// again.
    #[test]
    fn the_counter_wraps_and_can_be_reset() {
        let mut c = ActionCounter::new();
        c.set(u32::MAX - 1);
        assert_eq!(c.next(), u32::MAX);
        assert_eq!(c.peek(), 0, "the next stamp after u32::MAX wraps to 0");
        assert_eq!(c.next(), 0);
        c.rollback();
        assert_eq!(c.peek(), 0);
        c.set(0);
        assert_eq!(
            c.peek(),
            1,
            "setting the event counter to 0 makes the next action's stamp 1"
        );
    }
}
