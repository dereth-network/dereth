// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessagePrivateUpdatePropertyBool.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessagePrivateUpdatePropertyBool.cs`.

use dereth_protocol::qualities::{self as proto, PrivateUpdate};
use empyrean_entity::enums::PropertyBool;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::{byte_sequence, GameMessage};
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::network::sequence::sequence_manager::HasSequences;
use crate::network::sequence::sequence_type::SequenceType;

// ACE: GameMessagePrivateUpdatePropertyBool.GameMessagePrivateUpdatePropertyBool
#[must_use]
pub fn game_message_private_update_property_bool(
    world_object: &mut impl HasSequences,
    property: PropertyBool,
    value: bool,
) -> GameMessage {
    let sequence = byte_sequence(
        &world_object
            .sequences()
            .get_next_sequence_of(SequenceType::UpdatePropertyBool, property),
    );
    GameMessage::from_proto(
        GameMessageOpcode::PrivateUpdatePropertyBool,
        GameMessageGroup::UIQueue,
        &proto::QualitiesPrivateUpdateBool(PrivateUpdate {
            sequence,
            property_id: property.into(),
            value: i32::from(value),
        }),
    )
}
