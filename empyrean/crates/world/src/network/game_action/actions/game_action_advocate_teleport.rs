// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionAdvocateTeleport.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionAdvocateTeleport.cs`.

use dereth_protocol as proto;
use empyrean_entity::enums::ChatMessageType;
use empyrean_entity::{LandblockId, Position};
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::World;

// ACE: GameActionAdvocateTeleport.Handle
/// The minimap teleport of an admin, arch or PSR: refused for an all-water landblock, else the
/// position's z / indoor cell is adjusted and the player teleported there.
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::trade::AdvocateTeleport>()?;
    let (_target, wire) = (m.target_name, m.destination);
    // `new Position(message.Payload)`: the raw cell, origin and rotation
    let mut position = Position::new();
    position.set_landblock_id(LandblockId::new(wire.objcell_id));
    position.position_x = wire.frame.origin.x;
    position.position_y = wire.frame.origin.y;
    position.position_z = wire.frame.origin.z;
    position.rotation_w = wire.frame.orientation.w;
    position.rotation_x = wire.frame.orientation.x;
    position.rotation_y = wire.frame.orientation.y;
    position.rotation_z = wire.frame.orientation.z;

    let player = session_player(w, session);
    // this check is also done clientside, see: PlayerDesc::PlayerIsPSR
    {
        let p = w
            .objects
            .get(player)
            .expect("ACE: session.Player is null (NullReferenceException)");
        if !p.is_admin_prop() && !p.is_arch() && !p.is_psr() {
            return Ok(());
        }
    }

    //Console.WriteLine($"Handle minimap teleport");
    //Console.WriteLine($"Client sent position: {position}");

    // Check if water block
    if lscape_get_landblock_is_entirely_water(w, position.landblock_id().raw()) {
        crate::network::chat_packet::send_server_message(
            w,
            Some(session),
            &format!(
                "Landblock 0x{:04X} is entirely filled with water, and is impassable",
                position.landblock_id().landblock()
            ),
            ChatMessageType::Broadcast,
        );
        return Ok(());
    }

    // update z / indoor cell
    crate::entity::position_extensions::adjust_map_coords(w, &mut position)
        .unwrap_or_else(|e| panic!("{e}"));

    let coords =
        crate::entity::position_extensions::get_map_coord_str(&position).unwrap_or_default();
    crate::network::chat_packet::send_server_message(
        w,
        Some(session),
        &format!("Teleporting to: ({coords})"),
        ChatMessageType::Broadcast,
    );
    crate::world_objects::player_location::teleport(w, player, &position, false);
    Ok(())
}

/// `LScape.get_landblock(landcell).WaterType == LandDefs.WaterType.EntirelyWater`: the physics
/// landscape's landblock (the shared physics crate's `WaterType`, whose `EntirelyWater` is 2).
///
/// # Panics
/// A landblock the landscape cannot load (ACE dereferences the null landblock).
fn lscape_get_landblock_is_entirely_water(w: &World, landcell: u32) -> bool {
    let landblock_cell = (landcell & 0xFFFF_0000) | 0x0001;
    let cell = crate::physics::phys_ext::get_landcell(w, landblock_cell)
        .expect("NullReferenceException: LScape.get_landblock");
    let landblock = w
        .physics
        .land()
        .landblock(cell.landblock())
        .expect("NullReferenceException: LScape.get_landblock");
    landblock.water_type as u8 == 2
}
