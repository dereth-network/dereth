//! Shared isolated-world fixture for the area tests.

#![allow(unused_imports)]

pub(crate) use dereth_primitives::ObjectId;
pub(crate) use dereth_protocol::actions::{pack_action, pack_action_raw};
pub(crate) use dereth_protocol::admin::{
    CharacterQueryAge, CharacterQueryBirth, CharacterRequestPing,
};
pub(crate) use dereth_protocol::login::CharacterPlayerOptionChangedEvent;
pub(crate) use dereth_protocol::{Message, Opcode};
pub(crate) use empyrean_common::dotnet::datetime::TimeSpan;
pub(crate) use empyrean_entity::enums::{
    CharacterOption, PropertyAttribute2nd, PropertyFloat, PropertyInt, Tolerance,
};
pub(crate) use empyrean_entity::ObjectGuid;
pub(crate) use empyrean_net::{ClientMessage, SessionId};
pub(crate) use empyrean_world::dispatch::Class;
pub(crate) use empyrean_world::managers::landblock_manager as lm;
pub(crate) use empyrean_world::network::game_messages::game_message::start_capture;
pub(crate) use empyrean_world::network::managers::inbound_message_manager::{
    handle_client_message, run_inbound_message_queue,
};
pub(crate) use empyrean_world::world_objects::world_object::WorldObject;
pub(crate) use empyrean_world::world_objects::{
    creature_tick, monster_awareness, monster_combat, player_character, player_monster,
    world_object_decay,
};
pub(crate) use empyrean_world::World;

pub(crate) use crate::monsters::monster_ai::{lb_id, H, MONSTER, PLAYER};
pub(crate) use crate::social::fellowship::{chats_to, events_to, sent};

pub(crate) fn health(w: &World, g: ObjectGuid) -> u32 {
    let o = w.objects.get(g).unwrap();
    o.health().current(o)
}

pub(crate) const ITEM: ObjectGuid = ObjectGuid::new(0x8000_0200);

/// A dynamic item lying on the landblock.
pub(crate) fn item(h: &mut H) {
    let mut o = WorldObject::allocate(Class::GenericObject);
    o.guid = ITEM;
    o.biota.id = ITEM.full();
    o.set_location(Some(h.pos(MONSTER)));
    h.w.objects.insert(o).unwrap();
}

pub(crate) const A: ObjectGuid = ObjectGuid::new(0x5000_0001);

/// The player's session is in the world (game actions are `SessionState.WorldConnected`).
pub(crate) fn in_world(h: &mut crate::social::fellowship::H, session: SessionId) {
    h.w.sessions.get_mut(session).unwrap().state = empyrean_net::SessionState::WorldConnected;
}

pub(crate) fn action<M: Message>(w: &mut World, session: SessionId, m: &M) {
    handle_client_message(
        w,
        ClientMessage::new(pack_action(0x10, m).expect("encode")).expect("opcode"),
        session,
    );
    run_inbound_message_queue(w);
}
