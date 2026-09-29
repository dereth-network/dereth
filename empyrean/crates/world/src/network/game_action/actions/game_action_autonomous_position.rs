// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionAutonomousPosition.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionAutonomousPosition.cs`.

use dereth_protocol as proto;
use empyrean_entity::{LandblockId, Position};
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_networking;
use crate::World;

// ACE: GameActionAutonomousPosition.Handle
/// Sent every ~1 second by the client when a player is moving, with the latest position from the
/// client. `dereth-protocol` reads ACE's fields in ACE's order, including the trailing `Align()`,
/// which ACE makes after the player updates (no observable difference). The contact flag and a
/// grounded position are recorded; unless teleporting, the position is requested (with a
/// broadcast).
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    //Console.WriteLine($"{session.Player.Name}.AutoPos");

    let m = message.decode::<proto::movement::MovementAutonomousPosition>()?;

    // `new Position(message.Payload)`: the cell, the origin and the rotation (w, x, y, z)
    let wire = m.0.position;
    let mut position = Position::new();
    position.set_landblock_id(LandblockId::new(wire.objcell_id));
    position.position_x = wire.frame.origin.x;
    position.position_y = wire.frame.origin.y;
    position.position_z = wire.frame.origin.z;
    position.rotation_w = wire.frame.orientation.w;
    position.rotation_x = wire.frame.orientation.x;
    position.rotation_y = wire.frame.orientation.y;
    position.rotation_z = wire.frame.orientation.z;

    // the instance, server control, teleport and force position timestamps: read and unused
    let _timestamps = m.0.timestamps;

    let player = session_player(w, session);

    let last_contact = m.0.contact != 0; // TRUE if player is currently on ground
    let fields = crate::world_objects::player::fields_mut(w, player);
    fields.last_contact = last_contact;

    if last_contact {
        fields.last_ground_pos = Some(position);
    }

    let teleporting = w
        .objects
        .get(player)
        .is_some_and(|o| o.wo.world_object.teleporting);
    if !teleporting {
        player_networking::set_requested_location(w, player, position, true);
    }

    Ok(())
}
