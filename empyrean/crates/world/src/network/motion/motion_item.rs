// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Motion/MotionItem.cs
//! Port of `Source/ACE.Server/Network/Motion/MotionItem.cs`.

use std::collections::HashMap;
use std::sync::OnceLock;

use dereth_world_data::command_numbering::{self as numbering, CommandNumbering};
use empyrean_common::dotnet::binary_reader::{BinaryReader, ReadError};
use empyrean_entity::enums::MotionCommand;
use empyrean_entity::ObjectGuid;

use crate::network::game_messages::game_message::BinaryWriter;
use crate::network::sequence::sequence_manager::SequenceManager;
use crate::network::sequence::sequence_type::SequenceType;

// ACE: MotionItem
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MotionItem {
    // ACE: MotionItem.WorldObject
    /// The object whose `Motion` sequence the writer advances.
    pub world_object: ObjectGuid,

    // ACE: MotionItem.MotionCommand
    /// technically this is just a Command, which is MotionCommand & 0xFFFF (ushort); to avoid
    /// redundancy, not creating a separate Command class here
    pub motion_command: MotionCommand,

    // ACE: MotionItem.PackedSequence
    /// write: (motionSequence & 0x7FFF) | (autonomous & 0x1 << 15). sequence of the animation.
    /// Note the MSB appears to be used for autonomous flag, so the max size of this member would
    /// be 1/2 a dword.
    pub packed_sequence: u16,
    // ACE: MotionItem.ServerActionSequence
    /// read: packedSequence & 0x7FFF; Sequence of the animation.
    pub server_action_sequence: u16,
    // ACE: MotionItem.IsAutonomous
    /// read: packedSequence >> 15 == 1; True = client initiated, False = server initiated
    pub is_autonomous: bool,
    // ACE: MotionItem.Speed
    /// the speed at which to perform the animation / movement.
    pub speed: f32,
}

impl MotionItem {
    // ACE: MotionItem.MotionItem
    /// `new MotionItem(WorldObject worldObject, MotionCommand motionCommand, float speed = 1.0f)`.
    #[must_use]
    pub fn new(world_object: ObjectGuid, motion_command: MotionCommand, speed: f32) -> Self {
        Self {
            world_object,
            motion_command,
            speed,
            ..Default::default()
        }
    }

    // ACE: MotionItem.MotionItem
    /// `new MotionItem(WorldObject wo, BinaryReader reader)`: a raw command the enum does not
    /// know leaves `MotionCommand` at 0 (ACE logs it). Not ACE: the raw command is an index in
    /// `numbering`, the world's files' (ACE's is always the final one).
    pub fn read(
        wo: ObjectGuid,
        reader: &mut BinaryReader<'_>,
        numbering: CommandNumbering,
    ) -> Result<Self, ReadError> {
        let raw_command = numbering::final_index_from_wire(numbering, reader.read_u16()?);

        let motion_command = match raw_to_interpreted().get(&raw_command) {
            Some(c) => *c,
            None => {
                log::info!(
                    "MotionPack: couldn't find interpreted command for raw command {raw_command}"
                );
                MotionCommand(0)
            }
        };
        let packed_sequence = reader.read_u16()?;
        Ok(MotionItem {
            world_object: wo,
            motion_command,
            packed_sequence,
            server_action_sequence: packed_sequence & 0x7FFF,
            is_autonomous: (packed_sequence >> 15) == 1,
            speed: reader.read_f32()?,
        })
    }
}

impl std::fmt::Display for MotionItem {
    // ACE: MotionItem.ToString
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "MotionCommand: {}\r\nIsAutonomous: {}\r\nSpeed: {}\r\n",
            self.motion_command,
            if self.is_autonomous { "True" } else { "False" },
            empyrean_common::dotnet::to_string(self.speed)
        )
    }
}

// ACE: PackedCommandExtensions.RawToInterpreted, PackedCommandExtensions.PackedCommandExtensions
/// `PackedCommandExtensions.RawToInterpreted`: every `MotionCommand` by its low 16 bits (the
/// static constructor adds `Enum.GetValues` in order; the enum has no two members with the same
/// low word, which `Dictionary.Add` would reject). The enum's UI target-selection block is the
/// retail client's (V331), so raw 0x110..0x117 read as `CombatEat` .. `ClosestPlayer` as the client
/// numbers them.
pub fn raw_to_interpreted() -> &'static HashMap<u16, MotionCommand> {
    static MAP: OnceLock<HashMap<u16, MotionCommand>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut m = HashMap::new();
        for c in MotionCommand::ALL {
            #[allow(clippy::cast_possible_truncation)] // `(ushort)(uint)interpretedCommand`
            m.entry(c.0 as u16).or_insert(*c);
        }
        m
    })
}

// ACE: PackedCommandExtensions.Write
/// `writer.Write(MotionItem mc)`: the command, the object's next `Motion` sequence (with the MSB
/// set for a client-initiated motion), the speed. `sequences` is `mc.WorldObject.Sequences`.
/// Not ACE: the command goes out in `numbering`, the world's files'; the caller leaves out a
/// command that numbering lacks.
#[allow(clippy::cast_possible_truncation)] // `(ushort)mc.MotionCommand`
pub fn write(
    writer: &mut Vec<u8>,
    mc: &MotionItem,
    sequences: &mut SequenceManager,
    numbering: CommandNumbering,
) {
    let index =
        numbering::wire_index(numbering, mc.motion_command.0).unwrap_or(mc.motion_command.0 as u16);
    writer.write_u16(index); // verified

    // should already be masked with 0x7FFF
    let mut next_sequence = sequences.get_next_sequence(SequenceType::Motion);

    if mc.is_autonomous {
        next_sequence[1] |= 0x80; // if client-initiated motion, set upper bit
    }

    writer.write_bytes(&next_sequence);

    writer.write_f32(mc.speed);
}
