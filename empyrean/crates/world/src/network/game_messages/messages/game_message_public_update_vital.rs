// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessagePublicUpdateVital.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessagePublicUpdateVital.cs`.

use dereth_protocol::qualities::{self as proto, PublicUpdate};
use dereth_protocol::types::qualities::{Attribute, SecondaryAttribute};
use empyrean_entity::enums::PropertyAttribute2nd;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::{byte_sequence, GameMessage};
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::network::sequence::sequence_manager::HasSequences;
use crate::network::sequence::sequence_type::SequenceType;

// ACE: GameMessagePublicUpdateVital.GameMessagePublicUpdateVital
#[must_use]
pub fn game_message_public_update_vital(
    world_object: &mut impl HasSequences,
    attribute: PropertyAttribute2nd,
    ranks: u32,
    base_value: u32,
    total_investment: u32,
    current_value: u32,
) -> GameMessage {
    let sequence = byte_sequence(
        &world_object
            .sequences()
            .get_next_sequence_of(SequenceType::UpdateAttribute2ndLevel, attribute),
    );
    GameMessage::from_proto(
        GameMessageOpcode::PublicUpdateVital,
        GameMessageGroup::UIQueue,
        &proto::QualitiesUpdateAttribute2nd(PublicUpdate {
            sequence,
            object: world_object.world_object().guid.into(),
            property_id: attribute.into(),
            value: SecondaryAttribute {
                attribute: Attribute {
                    level_from_cp: ranks,
                    init_level: base_value,
                    cp_spent: total_investment,
                },
                current_level: current_value,
            },
        }),
    )
}
