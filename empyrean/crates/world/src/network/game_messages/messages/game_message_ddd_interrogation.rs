// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageDDDInterrogation.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageDDDInterrogation.cs`.

use dereth_protocol::admin as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::World;

/// `PropertyManager.GetBool("allow_highres_dat").Item`.
fn allow_highres_dat(w: &World) -> bool {
    crate::managers::property_manager::get_bool(w, "allow_highres_dat", false, true).item
}

// ACE: GameMessageDDDInterrogation.GameMessageDDDInterrogation
#[must_use]
pub fn game_message_ddd_interrogation(w: &World) -> GameMessage {
    let mut product_id: u32 = 0x1;
    if allow_highres_dat(w) {
        product_id |= 0x4;
    }

    let body = proto::DddInterrogation {
        servers_region: 1,     // the server's region
        name_rule_language: 1, // the name-rule language
        product_id,            // the product id
        // the supported-language count, then Invalid and English
        supported_languages: vec![0, 1],
    };
    GameMessage::from_proto(
        GameMessageOpcode::DDD_Interrogation,
        GameMessageGroup::DatabaseQueue,
        &body,
    )
}
