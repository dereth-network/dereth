// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/GameEventMessage.cs
//! Port of `Source/ACE.Server/Network/GameEvent/GameEventMessage.cs`.
//!
//! A game event is a [`GameMessage`] with opcode `GameEvent` (`0xF7B0`) whose constructor writes
//! the player's guid, **consumes** `session.GameEventSequence` (post-increment) and writes the event
//! type. Construction therefore takes `&mut SessionData`: the order in which events are built is
//! the order of their sequence numbers, exactly as in ACE.

use empyrean_entity::ObjectGuid;
use empyrean_net::{GameMessageGroup, SessionId};

use crate::network::game_messages::game_message::{BinaryWriter, GameMessage};
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::sessions::SessionData;
use crate::World;

use super::game_event_type::GameEventType;

// ACE: GameEventMessage.GameEventMessage
/// `protected GameEventMessage(GameEventType eventType, GameMessageGroup group, Session session)`.
#[must_use]
pub fn game_event_message(
    event_type: GameEventType,
    group: GameMessageGroup,
    session: &mut SessionData,
) -> GameMessage {
    game_event_message_with_capacity(event_type, group, session, 0)
}

/// The `dataInitialCapacity` overload (the capacity is only an allocation hint).
#[must_use]
pub fn game_event_message_with_capacity(
    event_type: GameEventType,
    group: GameMessageGroup,
    session: &mut SessionData,
    data_initial_capacity: i32,
) -> GameMessage {
    let mut msg =
        GameMessage::with_capacity(GameMessageOpcode::GameEvent, group, data_initial_capacity);

    let guid = session.player.unwrap_or(ObjectGuid::new(0));

    msg.data.write_guid(guid);
    // `Writer.Write(session.GameEventSequence++)`: the uint wraps like C#'s unchecked `++`.
    let sequence = session.game_event_sequence;
    session.game_event_sequence = sequence.wrapping_add(1);
    msg.data.write_u32(sequence);
    msg.data.write_u32(event_type.0);
    msg
}

/// An event whose body (everything after the event type) is `body`, as dereth-protocol writes it: the
/// event constructor writing the same fields in the same order, after [`game_event_message`]'s
/// header (which consumes the session's event sequence exactly as before). See
/// [`GameMessage::write_proto`].
#[must_use]
pub fn game_event_from_proto<M: dereth_protocol::Message>(
    event_type: GameEventType,
    group: GameMessageGroup,
    session: &mut SessionData,
    body: &M,
) -> GameMessage {
    game_event_from_proto_strings(event_type, group, session, body, &[])
}

/// [`game_event_from_proto`] for a body with strings that may be 65,535 or more UTF-16 units long
/// (see [`GameMessage::write_proto_strings`]).
#[must_use]
pub fn game_event_from_proto_strings<M: dereth_protocol::Message>(
    event_type: GameEventType,
    group: GameMessageGroup,
    session: &mut SessionData,
    body: &M,
    strings: &[&str],
) -> GameMessage {
    debug_assert_eq!(
        event_type.0,
        M::OPCODE.0,
        "ACE's event type is the body's own"
    );
    let mut msg = game_event_message(event_type, group, session);
    msg.write_proto_strings(body, strings);
    msg
}

/// The `Session` an event is built for, when the builder also reads the world (`session.Player`).
///
/// # Panics
/// When the session is unknown: ACE always holds the `Session` object, so this is its
/// `NullReferenceException`.
pub fn session_data(w: &mut World, session: SessionId) -> &mut SessionData {
    w.sessions
        .get_mut(session)
        .expect("ACE: the event's Session is null (NullReferenceException)")
}

/// `session.Player`, as a guid.
///
/// # Panics
/// When the session has no player (ACE: `NullReferenceException` on `session.Player.<member>`).
#[must_use]
pub fn session_player(w: &World, session: SessionId) -> ObjectGuid {
    w.sessions
        .player(session)
        .expect("ACE: session.Player is null (NullReferenceException)")
}
