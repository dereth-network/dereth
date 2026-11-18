// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageUpdatePosition.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageUpdatePosition.cs`.

use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::BinaryWriter;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::World;

// ACE: GameMessageUpdatePosition.GameMessageUpdatePosition
/// `adminMove` defaults to false in ACE. `new PositionPack(worldObject, adminMove)` and its writer
/// are in `structure/position_pack.rs`. ACE keeps the pack in a public
/// field that nothing reads; it is not kept here.
#[must_use]
pub fn game_message_update_position(
    w: &mut World,
    world_object: ObjectGuid,
    admin_move: bool,
) -> GameMessage {
    // 68 is the max seen in retail pcaps
    let mut msg = GameMessage::with_capacity(
        GameMessageOpcode::UpdatePosition,
        GameMessageGroup::SmartboxQueue,
        68,
    );
    // todo: avoid create intermediate object
    let position_pack =
        crate::network::structure::position_pack::position_pack_new(w, world_object, admin_move);
    msg.data.write_guid(world_object);
    crate::network::structure::position_pack::write(&mut msg.data, &position_pack);
    msg
}

/// Not ACE's (V550): an UpdatePosition of `world_object` at `location` in place of its
/// `Location` (where the server has moved a player since its last report).
#[must_use]
pub fn game_message_update_position_at(
    w: &mut World,
    world_object: ObjectGuid,
    location: &empyrean_entity::Position,
) -> GameMessage {
    let mut msg = GameMessage::with_capacity(
        GameMessageOpcode::UpdatePosition,
        GameMessageGroup::SmartboxQueue,
        68,
    );
    let position_pack = crate::network::structure::position_pack::position_pack_at(
        w,
        world_object,
        location,
        false,
    );
    msg.data.write_guid(world_object);
    crate::network::structure::position_pack::write(&mut msg.data, &position_pack);
    msg
}
