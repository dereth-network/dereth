//! The game-event sub-type catalogue and how the client dispatches game events.
//!
//! A **game event** is server→client and dispatched by
//! the UI queue's blob dispatcher, on a switch of the first dword after any header.
//! It **may** be wrapped in [`crate::OrderedEventHeader`] (`0xF7B0`, 12 bytes); the wrapper is the sender's
//! choice, because the wrapper reader rewinds when the magic is absent and the whole payload is
//! then dispatched unchanged. Measured over the corpus, 13 of the 45 recorded members of the
//! game-event table are sent **bare** — see [`is_game_event`].
//!
//! The event and action number spaces use the same small integers but are **independent**: `0x0005`
//! is `Character_PlayerOptionChangedEvent` as an *action* and `0x0013` is `Login_PlayerDescription`
//! as an *event*, and a value can exist in one space and not the other.
//!
//! Fifteen opcodes share the UI queue with the game events but are **not** game events — they arrive
//! unwrapped. [`NOT_GAME_EVENTS`] is that list.

use crate::archive::{Reader, Writer};
use crate::error::MessageError;
use crate::opcodes::{Direction, Opcode, OPCODES};
use crate::order::OrderedEventHeader;
use crate::Message;
use dereth_primitives::{NetQueue, ObjectId};

/// The UI-queue opcodes that are **not** wrapped in a `OrderedEventHeader`.
///
/// Everything else on the UI queue is a game event. The wrapper reader returning `None` is what
/// actually decides at run time — this list is the documentation of *why* it returns `None` for
/// these.
pub static NOT_GAME_EVENTS: &[Opcode] = &[
    Opcode::ADMIN_ENVIRONS,
    Opcode::CHARACTER_SET_PLAYER_VISUAL_DESC,
    Opcode::CHARACTER_CHAR_GEN_VERIFICATION_RESPONSE,
    Opcode::LOGIN_AWAITING_SUBSCRIPTION_EXPIRATION,
    Opcode::LOGIN_EXECUTE_LOG_OFF,
    Opcode::CHARACTER_CHARACTER_DELETE,
    Opcode::LOGIN_LOGIN_CHARACTER_SET,
    Opcode::CHARACTER_CHARACTER_ERROR,
    Opcode::LOGIN_ACCOUNT_BANNED,
    Opcode::ADMIN_RECEIVE_ACCOUNT_DATA,
    Opcode::ADMIN_RECEIVE_PLAYER_DATA,
    Opcode::LOGIN_ACCOUNT_BOOTED,
    Opcode::LOGIN_ENTER_GAME_SERVER_READY,
    Opcode::COMMUNICATION_TEXTBOX_STRING,
    Opcode::LOGIN_WORLD_INFO,
];

/// Whether an opcode is in the client's **ordered game-event table** — i.e. whether
/// the UI queue's blob dispatcher is the switch that handles it.
///
/// # This does NOT answer whether the message arrives inside a `0xF7B0`
///
/// Whether an opcode arrives wrapped or unwrapped on the UI queue is an **envelope** question and
/// this function cannot answer it, because all it reads is the `recv_queue` column — and the queue
/// column does not decide the envelope. The order header's unpack rewinds when the first dword
/// is not `0xF7B0` and then dispatches the whole payload, so the
/// wrapper is the *sender's* choice and a game event is accepted either way.
///
/// Over all 8,617 server-to-client blobs of the recorded corpus (`fixtures/message-corpus`), counting
/// both envelopes: of the **68** opcode values the corpus records s2c,
/// this predicate disagrees with the observed envelope on **13** — `0x0024`, `0x0197`, `0x02BB` and
/// the ten-member `Qualities_*` family, 1,244 occurrences in all, every one of them sent **bare**
/// while this function calls it a wrapped event. All 13 err in the same direction; **0** of 68 are
/// predicted bare and observed wrapped, so as an upper bound on "might be wrapped" it holds on this
/// corpus, and as an envelope predicate it is wrong for one row in five.
///
/// The neighbouring `0x0196`/`0x0197` pair is the shortest proof that no queue-derived predicate can
/// do better: consecutive rows in [`crate::opcodes`], identical `send_queue`/`recv_queue`, and 0
/// whole / 24 wrapped against 29 whole / 0 wrapped.
///
/// **The behaviour is deliberate.** Table membership is a real and useful question, it
/// is what every caller of this would want if it had one, and this function has **no caller anywhere
/// in the workspace outside its own tests** — so "fixing" it to answer the envelope question would
/// be inventing a consumer's requirements. To ask the envelope question, measure it over the
/// recorded corpus. The dispatch table and the wire framing are two separate questions;
/// conflating them is a known source of bugs.
#[must_use]
pub fn is_game_event(op: Opcode) -> bool {
    let Some(info) = op.info() else { return false };
    matches!(info.direction, Direction::S2C | Direction::Both)
        && info.recv_queue == Some(NetQueue::UiQueue)
        && !NOT_GAME_EVENTS.contains(&op)
}

/// Every opcode the client can receive as a game event, ascending.
pub fn game_event_opcodes() -> impl Iterator<Item = Opcode> {
    OPCODES
        .iter()
        .map(|i| i.opcode)
        .filter(|op| is_game_event(*op))
}

/// Frame a message as a game event: `[0xF7B0][iid][stamp][sub-type][payload]`.
///
/// Only the server produces these; the function exists so that a test or a rebuilt server can
/// construct one, and so that the round-trip gate covers the wrapper.
///
/// The payload's alignment origin is 16 — the wrapper is 12 bytes and the sub-type dword follows.
pub fn pack_event<M: Message>(iid: ObjectId, stamp: u32, m: &M) -> Result<Vec<u8>, MessageError> {
    let mut w = Writer::new();
    OrderedEventHeader { iid, stamp }.write(&mut w);
    w.u32(M::OPCODE.0);
    m.write(&mut w)?;
    Ok(w.into_inner())
}

/// A writer positioned for a game event's payload: origin 16.
#[must_use]
pub fn event_body_writer() -> Writer {
    Writer::with_origin(OrderedEventHeader::PACK_SIZE + 4)
}

/// One decoded UI-queue blob, ordered or not.
#[derive(Debug)]
pub struct UiBlob<'a> {
    /// `None` when the blob was not wrapped — the wrapper reader returned without consuming.
    pub order: Option<OrderedEventHeader>,
    pub sub_type: Opcode,
    pub body: Reader<'a>,
}

/// Split a UI-queue blob into its wrapper (if any), its type dword and its body.
///
/// This is the ordering step's first move: try the `OrderedEventHeader`, and
/// if the magic is absent treat the whole payload as an unordered event body. The body reader's
/// origin is set so that the alignment rule works out in both cases.
pub fn split_ui_blob(blob: &[u8]) -> Result<UiBlob<'_>, MessageError> {
    let mut r = Reader::new(blob);
    let order = OrderedEventHeader::read(&mut r);
    let sub_type = Opcode(r.u32()?);
    let consumed = r.position();
    Ok(UiBlob {
        order,
        sub_type,
        body: Reader::with_origin(&blob[consumed..], consumed),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::login::LoginPlayerDescription;
    use crate::qualities::{PrivateUpdate, QualitiesPrivateUpdateInt};

    /// Oracle: `docs/networking/messages/04-game-events.md` §1 —
    /// `[0xF7B0][iid][stamp][sub-type][payload]`.
    #[test]
    fn a_game_event_is_the_wrapper_then_the_sub_type_then_the_body() {
        let m = QualitiesPrivateUpdateInt(PrivateUpdate {
            sequence: 1,
            property_id: 25,
            value: 126,
        });
        let blob = pack_event(ObjectId(0x5000_0001), 3, &m).unwrap();
        assert_eq!(&blob[0..4], &0xF7B0u32.to_le_bytes());
        assert_eq!(&blob[4..8], &0x5000_0001u32.to_le_bytes());
        assert_eq!(&blob[8..12], &3u32.to_le_bytes());
        assert_eq!(&blob[12..16], &0x02CDu32.to_le_bytes());
        assert_eq!(blob.len(), 16 + 9);

        let split = split_ui_blob(&blob).unwrap();
        assert_eq!(
            split.order,
            Some(OrderedEventHeader {
                iid: ObjectId(0x5000_0001),
                stamp: 3
            })
        );
        assert_eq!(split.sub_type, QualitiesPrivateUpdateInt::OPCODE);
        assert_eq!(split.body.blob_offset(), 16);
    }

    /// An unwrapped UI-queue blob keeps its own opcode as the type dword — the wrapper reader must
    /// not have consumed it. That is the H2 acceptance test seen from the dispatcher's side.
    #[test]
    fn an_unwrapped_blob_still_splits_correctly() {
        // 0xF7E1 Login_WorldInfo, which is on the UI queue but is not a game event.
        let mut w = Writer::new();
        w.u32(Opcode::LOGIN_WORLD_INFO.0);
        w.i32(12);
        w.i32(800);
        w.pstring("Frostfell").unwrap();
        let blob = w.into_inner();

        let split = split_ui_blob(&blob).unwrap();
        assert_eq!(split.order, None);
        assert_eq!(split.sub_type, Opcode::LOGIN_WORLD_INFO);
        assert_eq!(split.body.blob_offset(), 4);
    }

    /// The fifteen UI-queue opcodes that are not game events, and one that is.
    #[test]
    fn the_not_game_events_list_is_the_documented_fifteen() {
        assert_eq!(NOT_GAME_EVENTS.len(), 15);
        for op in NOT_GAME_EVENTS {
            assert!(!is_game_event(*op), "{op:?} is not a game event");
            assert_eq!(
                op.info().unwrap().recv_queue,
                Some(NetQueue::UiQueue),
                "{op:?} still lives on the UI queue"
            );
        }
        assert!(is_game_event(LoginPlayerDescription::OPCODE));
        assert!(is_game_event(QualitiesPrivateUpdateInt::OPCODE));
        // A WorldObjects opcode is not a game event however it is framed.
        assert!(!is_game_event(Opcode::ITEM_CREATE_OBJECT));
    }

    /// The event and action number spaces are independent: `0x0005` exists only as an action and
    /// `0x0013` only as an event, and nothing may assume one implies the other.
    #[test]
    fn the_event_and_action_spaces_are_independent() {
        use crate::actions::outbound_kind;
        // 0x0013 Login_PlayerDescription: an event, never an action.
        assert!(is_game_event(Opcode(0x0013)));
        assert_eq!(outbound_kind(Opcode(0x0013)), None);
        // 0x0005 Character_PlayerOptionChangedEvent: an action, never an event.
        assert!(!is_game_event(Opcode(0x0005)));
        assert!(outbound_kind(Opcode(0x0005)).is_some());
    }

    /// The alignment origin of an event body is 16, not 0 and not 4.
    #[test]
    fn the_event_body_aligns_on_sixteen() {
        let w = event_body_writer();
        assert_eq!(w.blob_offset(), 16);
    }
}
