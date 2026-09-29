// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessagePrivateUpdateDataID.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessagePrivateUpdateDataID.cs`.

use dereth_protocol::qualities::{self as proto, PrivateUpdate};
use empyrean_entity::enums::PropertyDataId;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::{byte_sequence, GameMessage};
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::network::sequence::sequence_manager::HasSequences;
use crate::network::sequence::sequence_type::SequenceType;

// ACE: GameMessagePrivateUpdateDataID.GameMessagePrivateUpdateDataID
#[must_use]
pub fn game_message_private_update_data_id(
    world_object: &mut impl HasSequences,
    property: PropertyDataId,
    value: u32,
) -> GameMessage {
    let sequence = byte_sequence(
        &world_object
            .sequences()
            .get_next_sequence_of(SequenceType::UpdatePropertyDataID, property),
    );
    GameMessage::from_proto(
        GameMessageOpcode::PrivateUpdatePropertyDataID,
        GameMessageGroup::UIQueue,
        &proto::QualitiesPrivateUpdateDataId(PrivateUpdate {
            sequence,
            property_id: property.into(),
            value,
        }),
    )
}
