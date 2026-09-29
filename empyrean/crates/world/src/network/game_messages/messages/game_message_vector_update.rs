// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageVectorUpdate.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageVectorUpdate.cs`.

use empyrean_entity::{ObjectGuid, Vector3};
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::BinaryWriter;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::network::sequence::sequence_manager::HasSequences;
use crate::network::sequence::sequence_type::SequenceType;
use crate::world_objects::world_object_properties;
use crate::World;

// ACE: GameMessageVectorUpdate.GameMessageVectorUpdate
/// `new GameMessageVectorUpdate(worldObject)`: `worldObject.PhysicsObj?.Velocity ?? Vector3.Zero`
/// and `PhysicsObj?.Omega ?? Vector3.Zero` (the fields; an object without a physics body sends
/// zeros), then the sequences.
///
/// # Panics
/// When `this` is not in `World.objects` (ACE: `NullReferenceException`).
#[must_use]
pub fn game_message_vector_update(w: &mut World, this: ObjectGuid) -> GameMessage {
    let velocity = world_object_properties::velocity(w, this);
    let omega = world_object_properties::omega(w, this);
    game_message_vector_update_of(
        w.objects.get_mut(this).expect("ACE: worldObject is null"),
        velocity,
        omega,
    )
}

/// The message body over an object's guid and sequences, with its velocity and omega already read.
#[must_use]
pub fn game_message_vector_update_of(
    world_object: &mut impl HasSequences,
    velocity: Vector3,
    omega: Vector3,
) -> GameMessage {
    let mut msg = GameMessage::with_capacity(
        GameMessageOpcode::VectorUpdate,
        GameMessageGroup::SmartboxQueue,
        36,
    );
    // object guid
    // velocity - Vector3
    // omega - Vector3
    // instance sequence - ushort
    // vector sequence - ushort

    msg.data.write_guid(world_object.world_object().guid);
    for v in [velocity.x, velocity.y, velocity.z] {
        msg.data.write_f32(v);
    }
    for v in [omega.x, omega.y, omega.z] {
        msg.data.write_f32(v);
    }
    msg.data.write_bytes(
        &world_object
            .sequences()
            .get_current_sequence(SequenceType::ObjectInstance),
    );
    msg.data.write_bytes(
        &world_object
            .sequences()
            .get_next_sequence(SequenceType::ObjectVector),
    );
    msg
}
