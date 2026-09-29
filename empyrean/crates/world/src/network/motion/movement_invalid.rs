// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Motion/MovementInvalid.cs
//! Port of `Source/ACE.Server/Network/Motion/MovementInvalid.cs`.
//!
//! ACE keeps a back reference to the owning `MovementData` and reads its `MotionFlags` when
//! writing; the writer here takes those flags from the parent's writer.

use empyrean_entity::enums::MotionFlags;
use empyrean_entity::ObjectGuid;

use super::interpreted_motion_state::{self, InterpretedMotionState};
use super::movement_data::{Motion, MovementData};
use crate::network::game_messages::game_message::BinaryWriter;
use crate::network::sequence::sequence_manager::SequenceManager;

// ACE: MovementInvalid
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MovementInvalid {
    // ACE: MovementInvalid.State
    /// set of movement / animation data
    pub state: InterpretedMotionState,
    // ACE: MovementInvalid.StickyObject
    /// choose valid sections by masking against MotionFlags: 0x1 - object to stick to
    pub sticky_object: ObjectGuid,
}

impl MovementInvalid {
    // ACE: MovementInvalid.MovementInvalid
    /// `new MovementInvalid(MovementData movementData)`.
    #[must_use]
    pub fn new(movement_data: &MovementData) -> Self {
        Self {
            state: InterpretedMotionState::from_movement_data(movement_data),
            sticky_object: ObjectGuid::default(),
        }
    }

    // ACE: MovementInvalid.MovementInvalid
    /// `new MovementInvalid(MovementData movementData, Motion motion)`.
    #[must_use]
    pub fn from_motion(movement_data: &MovementData, motion: &Motion) -> Self {
        let mut m = Self {
            state: InterpretedMotionState::from_motion(movement_data, motion),
            sticky_object: ObjectGuid::default(),
        };

        if (motion.motion_flags & MotionFlags::StickToObject).0 != 0 {
            m.sticky_object = motion.target_guid;
        }
        m
    }

    // ACE: MovementInvalid.MovementInvalid
    /// `new MovementInvalid(MovementData movementData, InterpretedMotionState state)`.
    // ACE-BUG: `state.BuildMovementFlags()` is called and its result discarded, so the state keeps
    // whatever flags the caller set (its only caller sets them just before).
    #[must_use]
    pub fn from_state(state: InterpretedMotionState) -> Self {
        let _ = state.build_movement_flags();
        Self {
            state,
            sticky_object: ObjectGuid::default(),
        }
    }
}

// ACE: MovementInvalidExtensions.Write
/// `writer.Write(MovementInvalid movement)`; `motion_flags` is `movement.MovementData.MotionFlags`.
pub fn write(
    writer: &mut Vec<u8>,
    movement: &MovementInvalid,
    motion_flags: MotionFlags,
    sequences: &mut SequenceManager,
) {
    interpreted_motion_state::write(writer, &movement.state, sequences);

    if (motion_flags & MotionFlags::StickToObject).0 != 0 {
        writer.write_guid(movement.sticky_object);
    }
}
