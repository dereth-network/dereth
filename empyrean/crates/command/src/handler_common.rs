//! The small lookups and replies the ported handler files share: what ACE writes inline as
//! `session.Player`, `wo.Name` or `bool.ToString()`, and dereferences without a check.

use empyrean_entity::enums::{ChatMessageType, PropertyString};
use empyrean_entity::{ObjectGuid, Position};
use empyrean_net::SessionId;
use empyrean_world::network::game_messages::game_message::enqueue_send;
use empyrean_world::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::World;

/// `session.Player`.
pub(crate) fn session_player(w: &World, session: SessionId) -> ObjectGuid {
    w.sessions
        .player(session)
        .expect("ACE: session.Player is null (NullReferenceException)")
}

/// `wo.Name`.
pub(crate) fn name_of(w: &World, wo: ObjectGuid) -> String {
    w.objects
        .get(wo)
        .and_then(|o| o.get_property(PropertyString::Name))
        .unwrap_or_default()
}

/// The object `g`, which ACE dereferences.
pub(crate) fn obj(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects
        .get(g)
        .unwrap_or_else(|| panic!("NullReferenceException: object 0x{:08X}", g.full()))
}

/// The object `g`, mutably.
pub(crate) fn obj_mut(w: &mut World, g: ObjectGuid) -> &mut WorldObject {
    w.objects
        .get_mut(g)
        .unwrap_or_else(|| panic!("NullReferenceException: object 0x{:08X}", g.full()))
}

/// `wo.Location`, which ACE dereferences.
pub(crate) fn location_of(w: &World, wo: ObjectGuid) -> Position {
    obj(w, wo)
        .location()
        .expect("NullReferenceException: Location")
}

/// `session.Network.EnqueueSend(new GameMessageSystemChat(message, type))`.
pub(crate) fn system_chat(
    w: &mut World,
    session: SessionId,
    message: &str,
    chat_message_type: ChatMessageType,
) {
    enqueue_send(
        w,
        session,
        game_message_system_chat(message, chat_message_type),
    );
}

/// `bool.ToString()`.
pub(crate) fn bool_string(b: bool) -> &'static str {
    if b {
        "True"
    } else {
        "False"
    }
}
