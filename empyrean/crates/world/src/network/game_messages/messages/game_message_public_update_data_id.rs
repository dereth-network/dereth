// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessagePublicUpdateDataID.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessagePublicUpdateDataID.cs`.

use dereth_protocol::qualities::{self as proto, PublicUpdate};
use empyrean_entity::enums::PropertyDataId;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::{byte_sequence, GameMessage};
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::network::sequence::sequence_manager::HasSequences;
use crate::network::sequence::sequence_type::SequenceType;

// ACE: GameMessagePublicUpdatePropertyDataID.GameMessagePublicUpdatePropertyDataID
#[must_use]
pub fn game_message_public_update_data_id(
    world_object: &mut impl HasSequences,
    property: PropertyDataId,
    value: u32,
) -> GameMessage {
    let sequence = byte_sequence(
        &world_object
            .sequences()
            .get_next_sequence_of(SequenceType::UpdatePropertyDataID, property),
    );
    let object = world_object.world_object().guid.into();
    GameMessage::from_proto(
        GameMessageOpcode::PublicUpdatePropertyDataID,
        GameMessageGroup::UIQueue,
        &proto::QualitiesUpdateDataId(PublicUpdate {
            sequence,
            object,
            property_id: property.into(),
            value,
        }),
    )
}
