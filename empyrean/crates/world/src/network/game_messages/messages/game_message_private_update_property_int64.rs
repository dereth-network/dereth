// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessagePrivateUpdatePropertyInt64.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessagePrivateUpdatePropertyInt64.cs`.

use dereth_protocol::qualities::{self as proto, PrivateUpdate};
use empyrean_entity::enums::PropertyInt64;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::{byte_sequence, GameMessage};
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::network::sequence::sequence_manager::HasSequences;
use crate::network::sequence::sequence_type::SequenceType;

// ACE: GameMessagePrivateUpdatePropertyInt64.GameMessagePrivateUpdatePropertyInt64
#[must_use]
pub fn game_message_private_update_property_int64(
    world_object: &mut impl HasSequences,
    property: PropertyInt64,
    value: i64,
) -> GameMessage {
    let sequence = byte_sequence(
        &world_object
            .sequences()
            .get_next_sequence_of(SequenceType::UpdatePropertyInt64, property),
    );
    GameMessage::from_proto(
        GameMessageOpcode::PrivateUpdatePropertyInt64,
        GameMessageGroup::UIQueue,
        &proto::QualitiesPrivateUpdateInt64(PrivateUpdate {
            sequence,
            property_id: property.into(),
            value,
        }),
    )
}
