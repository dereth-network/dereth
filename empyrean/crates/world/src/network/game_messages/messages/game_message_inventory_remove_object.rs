// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageInventoryRemoveObject.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageInventoryRemoveObject.cs`.

use dereth_protocol::objects as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::world_objects::world_object::WorldObject;

// ACE: GameMessageInventoryRemoveObject.GameMessageInventoryRemoveObject
#[must_use]
pub fn game_message_inventory_remove_object(world_object: &WorldObject) -> GameMessage {
    GameMessage::from_proto(
        GameMessageOpcode::InventoryRemoveObject,
        GameMessageGroup::UIQueue,
        &proto::ItemServerSaysRemove {
            object: world_object.guid.into(),
        },
    )
}
