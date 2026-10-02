// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Motion/MoveToState.cs
//! Port of `Source/ACE.Server/Network/Motion/MoveToState.cs`.

use empyrean_common::dotnet::binary_reader::{BinaryReader, ReadError};
use empyrean_entity::{ObjectGuid, Position};

use super::raw_motion_state::RawMotionState;

// ACE: MoveToState
/// Client sends this structure in F61C - MoveToState.
#[derive(Debug, Clone, Default)]
pub struct MoveToState {
    // ACE: MoveToState.WorldObject
    pub world_object: ObjectGuid,

    // ACE: MoveToState.RawMotionState
    /// the raw movement commands sent by the client; these are in turn translated to an
    /// InterpretedMotionState
    pub raw_motion_state: RawMotionState,
    // ACE: MoveToState.Position
    pub position: Option<Position>,

    // ACE: MoveToState.InstanceSequence
    pub instance_sequence: u16,
    // ACE: MoveToState.ServerControlSequence
    pub server_control_sequence: u16,
    // ACE: MoveToState.TeleportSequence
    pub teleport_sequence: u16,
    // ACE: MoveToState.ForcePositionSequence
    pub force_position_sequence: u16,

    // ACE: MoveToState.ContactLongJump
    pub contact_long_jump: u8,

    // not sent in packet directly as bools, parsed from above
    // ACE: MoveToState.Contact
    /// verify: contact (indicates if player is on ground), or sticky bit?
    pub contact: bool,
    // ACE: MoveToState.StandingLongJump
    pub standing_long_jump: bool,
}

impl MoveToState {
    // ACE: MoveToState.MoveToState
    /// `new MoveToState(WorldObject wo, BinaryReader reader)`. Not ACE: `numbering` is the world's
    /// files' command numbering, which the client wrote its motion state in.
    pub fn read(
        wo: ObjectGuid,
        reader: &mut BinaryReader<'_>,
        numbering: dereth_world_data::command_numbering::CommandNumbering,
    ) -> Result<Self, ReadError> {
        let raw_motion_state = RawMotionState::read(wo, reader, numbering)?;
        let at = reader.position();
        let position = Position::from_reader(reader).ok_or(ReadError::EndOfStream {
            at,
            needed: 32,
            available: 0,
        })?;

        let mut s = MoveToState {
            world_object: wo,
            raw_motion_state,
            position: Some(position),
            instance_sequence: reader.read_u16()?,
            server_control_sequence: reader.read_u16()?,
            teleport_sequence: reader.read_u16()?,
            force_position_sequence: reader.read_u16()?,
            contact_long_jump: reader.read_byte()?,
            contact: false,
            standing_long_jump: false,
        };
        if (s.contact_long_jump & 0x1) != 0 {
            s.contact = true;
        }
        if (s.contact_long_jump & 0x2) != 0 {
            s.standing_long_jump = true;
        }

        // align to DWORD boundary
        let pos = u32::try_from(reader.position()).unwrap_or(u32::MAX);
        reader.skip(empyrean_net::extensions::align_reader_skip(pos) as usize);
        Ok(s)
    }
}
