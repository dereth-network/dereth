// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/PositionPack.cs
//! Port of `Source/ACE.Server/Network/Structure/PositionPack.cs`.

use empyrean_entity::enums::{Placement, PositionFlags};
use empyrean_entity::{ObjectGuid, Quaternion, Vector3};

use super::origin::{self, Origin};
use crate::network::game_messages::game_message::write_record;
use crate::network::sequence::sequence_type::SequenceType;
use crate::physics::phys_ext;
use crate::World;

// ACE: PositionPack
/// A position with sequences.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PositionPack {
    // ACE: PositionPack.WorldObject
    pub world_object: ObjectGuid,

    // ACE: PositionPack.Flags
    pub flags: PositionFlags,
    // ACE: PositionPack.Origin
    /// the location of the object in the world
    pub origin: Origin,
    // ACE: PositionPack.Rotation
    pub rotation: Quaternion,

    // ACE: PositionPack.Velocity
    pub velocity: Vector3,
    // ACE: PositionPack.PlacementID
    pub placement_id: Option<Placement>,

    // really just a bunch of ushorts for these particular sequences, but for type safety, there
    // appear to be some other sequences that could be either uints or ulongs, so
    // GetCurrentSequence/GetNextSequence returns byte arrays here...
    // ACE: PositionPack.InstanceSequence
    pub instance_sequence: Vec<u8>,
    // ACE: PositionPack.PositionSequence
    pub position_sequence: Vec<u8>,
    // ACE: PositionPack.TeleportSequence
    pub teleport_sequence: Vec<u8>,
    // ACE: PositionPack.ForcePositionSequence
    pub force_position_sequence: Vec<u8>,
}

// ACE: PositionPack.PositionPack
/// `new PositionPack(WorldObject wo, bool adminMove = false)`. Note that this constructor
/// increments the object's position sequence (and its teleport sequence for an admin move).
pub fn position_pack_new(w: &mut World, wo: ObjectGuid, admin_move: bool) -> PositionPack {
    let phys = w.objects.get(wo).and_then(|o| o.phys);
    let velocity = phys.map_or(Vector3::ZERO, |h| phys_ext::velocity(w, h)); // average or instantaneous?
    let grounded = phys.is_some_and(|h| phys_ext::on_walkable(w, h));

    let o = w.objects.get_mut(wo).expect("ACE: wo is null");
    let location = o
        .location()
        .expect("ACE: wo.Location is null (NullReferenceException)");

    let mut p = PositionPack {
        world_object: wo,
        origin: Origin::new(location.cell(), location.pos()),
        rotation: location.rotation(),
        velocity,
        placement_id: o.placement(),
        ..Default::default()
    };

    p.instance_sequence = o
        .sequences
        .get_current_sequence(SequenceType::ObjectInstance);
    p.position_sequence = o.sequences.get_next_sequence(SequenceType::ObjectPosition);

    p.teleport_sequence = if admin_move {
        o.sequences.get_next_sequence(SequenceType::ObjectTeleport)
    } else {
        o.sequences
            .get_current_sequence(SequenceType::ObjectTeleport)
    };

    p.force_position_sequence = o
        .sequences
        .get_current_sequence(SequenceType::ObjectForcePosition);

    p.flags = p.build_flags(grounded);
    p
}

impl PositionPack {
    // ACE: PositionPack.BuildFlags
    /// The PositionFlags based on the current state. `on_walkable` is
    /// `WorldObject.PhysicsObj != null && (PhysicsObj.TransientState & OnWalkable) != 0`.
    #[must_use]
    pub fn build_flags(&self, on_walkable: bool) -> PositionFlags {
        let mut flags = PositionFlags::None;

        if self.velocity != Vector3::ZERO {
            flags |= PositionFlags::HasVelocity;
        }

        if self.placement_id.is_some() {
            flags |= PositionFlags::HasPlacementID;
        }

        if on_walkable {
            flags |= PositionFlags::IsGrounded;
        }

        if self.rotation.w == 0.0 {
            flags |= PositionFlags::OrientationHasNoW;
        }
        if self.rotation.x == 0.0 {
            flags |= PositionFlags::OrientationHasNoX;
        }
        if self.rotation.y == 0.0 {
            flags |= PositionFlags::OrientationHasNoY;
        }
        if self.rotation.z == 0.0 {
            flags |= PositionFlags::OrientationHasNoZ;
        }

        flags
    }
}

// ACE: PositionPackExtensions.Write
/// `writer.Write(PositionPack position)`.
pub fn write(writer: &mut Vec<u8>, position: &PositionPack) {
    write_record(writer, &[], |w| record(position).write(w));
}

/// The dereth-protocol record the `Write` extension below writes, field for field. The four sequences are two bytes each; the sections the flags leave out are left out.
#[must_use]
pub fn record(position: &PositionPack) -> dereth_protocol::movement::PositionPack {
    use crate::network::game_messages::game_message::ushort_sequence;
    use empyrean_entity::shared_types::{wire_quat, wire_vec3};
    dereth_protocol::movement::PositionPack {
        flags: position.flags.0,
        origin: origin::record(&position.origin),
        // choose valid sections by masking against flags
        orientation: wire_quat(position.rotation),
        velocity: ((position.flags & PositionFlags::HasVelocity).0 != 0)
            .then(|| wire_vec3(position.velocity)),
        placement_id: ((position.flags & PositionFlags::HasPlacementID).0 != 0).then(|| {
            position
                .placement_id
                .expect("ACE: PlacementID is null (InvalidOperationException)")
                .0
        }),
        instance_timestamp: ushort_sequence(&position.instance_sequence),
        position_timestamp: ushort_sequence(&position.position_sequence),
        teleport_timestamp: ushort_sequence(&position.teleport_sequence),
        force_position_timestamp: ushort_sequence(&position.force_position_sequence),
    }
}
