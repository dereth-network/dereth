// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventTell.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventTell.cs`.

use dereth_primitives::ObjectId;
use dereth_protocol::comms as proto;
use empyrean_entity::enums::ChatMessageType;
use empyrean_entity::enums::CreatureType;
use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;
use empyrean_net::SessionId;

use crate::network::game_event::game_event_message::game_event_from_proto_strings;
use crate::network::game_event::game_event_message::game_event_message;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;
use crate::world_objects::world_object::WorldObject;
use crate::World;

// ACE: GameEventTell.GameEventTell
/// `GameEventTell(Session session, string messageText, string senderName, uint senderID, uint targetID,
/// ChatMessageType chatMessageType)`.
#[must_use]
pub fn game_event_tell_from(
    session: &mut SessionData,
    message_text: &str,
    sender_name: &str,
    sender_id: u32,
    target_id: u32,
    chat_message_type: ChatMessageType,
) -> GameMessage {
    let body = proto::CommunicationHearDirectSpeech {
        message: ace_str(message_text),
        sender_name: ace_str(sender_name),
        sender_id: ObjectId(sender_id),
        target_id: ObjectId(target_id),
        text_type: chat_message_type.0,
        secret_flags: 0, // This is not documented in the xml's, but is found in the pcaps. The functionality seems the same with or without it.
    };
    game_event_from_proto_strings(
        GameEventType::Tell,
        GameMessageGroup::UIQueue,
        session,
        &body,
        &[message_text, sender_name],
    )
}

/// `GameEventTell(WorldObject worldObject, string messageText, Player player, ChatMessageType
/// chatMessageType)`: built for `player.Session`, which the caller passes as `session` (the
/// player-to-session link lives in the session map).
#[must_use]
pub fn game_event_tell(
    w: &mut World,
    world_object: ObjectGuid,
    message_text: &str,
    player: ObjectGuid,
    session: SessionId,
    chat_message_type: ChatMessageType,
) -> GameMessage {
    let mut msg = game_event_message(
        GameEventType::Tell,
        GameMessageGroup::UIQueue,
        session_data(w, session),
    );
    let creature_type = w
        .objects
        .get(world_object)
        .and_then(WorldObject::creature_type);
    let world_object_name = crate::dispatch::name::name(w, world_object);
    let name = if creature_type == Some(CreatureType::Olthoi) {
        Some(format!("{}&", world_object_name.as_deref().unwrap_or("")))
    } else {
        world_object_name
    };
    let body = proto::CommunicationHearDirectSpeech {
        message: ace_str(message_text),
        sender_name: ace_str(name.as_deref()),
        sender_id: world_object.into(),
        target_id: player.into(),
        text_type: chat_message_type.0,
        secret_flags: 0, // This is not documented in the xml's, but is found in the pcaps. The functionality seems the same with or without it.
    };
    msg.write_proto_strings(&body, &[message_text, name.as_deref().unwrap_or("")]);
    msg
}
