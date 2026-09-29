//! Shared isolated-world fixture for the area tests.

#![allow(unused_imports)]

pub(crate) use empyrean_entity::enums::{PropertyInt64, ShareType, XpType};
pub(crate) use empyrean_entity::ObjectGuid;
pub(crate) use empyrean_world::network::game_messages::game_message::start_capture;
pub(crate) use empyrean_world::network::game_messages::game_message_opcode::GameMessageOpcode;
pub(crate) use empyrean_world::world_objects::player_luminance;

pub(crate) use crate::social::fellowship::{chats_to, sent, H};

pub(crate) const A: ObjectGuid = ObjectGuid::new(0x5000_0001);

pub(crate) fn opcodes_to(
    msgs: &[(empyrean_net::SessionId, u32, u32, Vec<u8>)],
    session: empyrean_net::SessionId,
) -> Vec<u32> {
    msgs.iter()
        .filter(|m| m.0 == session)
        .map(|m| m.1)
        .collect()
}

pub(crate) fn available_luminance(h: &H) -> Option<i64> {
    h.w.objects
        .get(A)
        .unwrap()
        .get_property(PropertyInt64::AvailableLuminance)
}

pub(crate) const B: ObjectGuid = ObjectGuid::new(0x5000_0002);
