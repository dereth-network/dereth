// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Motion/InterpretedMotionState.cs
//! Port of `Source/ACE.Server/Network/Motion/InterpretedMotionState.cs`.
//!
//! ACE keeps a back reference to the owning `MovementData`, which it only assigns (its
//! `CurrentStyle` is copied at construction); the reference is not kept here.

use dereth_world_data::command_numbering::{self as numbering, CommandNumbering};
use empyrean_entity::enums::{MotionCommand, MotionStance, MovementStateFlag};
use empyrean_entity::ObjectGuid;

use super::motion_item::{self, MotionItem};
use super::movement_data::{Motion, MovementData};
use crate::network::game_messages::game_message::BinaryWriter;
use crate::network::sequence::sequence_manager::SequenceManager;

// ACE: InterpretedMotionState
/// Information related to movement and animation.
#[derive(Debug, Clone, PartialEq)]
pub struct InterpretedMotionState {
    // ACE: InterpretedMotionState.Flags
    pub flags: MovementStateFlag,

    // ACE: InterpretedMotionState.CurrentStyle
    pub current_style: MotionStance,

    // ACE: InterpretedMotionState.ForwardCommand
    pub forward_command: MotionCommand,
    // ACE: InterpretedMotionState.SidestepCommand
    pub sidestep_command: MotionCommand,
    // ACE: InterpretedMotionState.TurnCommand
    pub turn_command: MotionCommand,

    // ACE: InterpretedMotionState.ForwardSpeed
    pub forward_speed: f32,
    // ACE: InterpretedMotionState.SidestepSpeed
    pub sidestep_speed: f32,
    // ACE: InterpretedMotionState.TurnSpeed
    pub turn_speed: f32,

    // ACE: InterpretedMotionState.Commands
    /// commands: list of length commandListLength (`null` until the first command)
    pub commands: Option<Vec<MotionItem>>,
}

impl Default for InterpretedMotionState {
    // ACE: InterpretedMotionState.InterpretedMotionState
    /// `new InterpretedMotionState()`: `ForwardCommand = Ready`, the speeds 1.0.
    fn default() -> Self {
        InterpretedMotionState {
            flags: MovementStateFlag::default(),
            current_style: MotionStance::default(),
            forward_command: MotionCommand::Ready,
            sidestep_command: MotionCommand::default(),
            turn_command: MotionCommand::default(),
            forward_speed: 1.0,
            sidestep_speed: 1.0,
            turn_speed: 1.0,
            commands: None,
        }
    }
}

impl InterpretedMotionState {
    // ACE: InterpretedMotionState.InterpretedMotionState
    /// `new InterpretedMotionState()`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    // ACE: InterpretedMotionState.InterpretedMotionState
    /// `new InterpretedMotionState(InterpretedMotionState state)`: the copy constructor (the
    /// commands are copied one by one). `Clone` is the same.
    #[must_use]
    pub fn copy(state: &InterpretedMotionState) -> Self {
        state.clone()
    }

    // ACE: InterpretedMotionState.InterpretedMotionState
    /// `new InterpretedMotionState(MovementData data)`: the data's style, no commands (field
    /// initializers: speeds 1.0, `ForwardCommand` Invalid).
    #[must_use]
    pub fn from_movement_data(data: &MovementData) -> Self {
        let mut s = InterpretedMotionState {
            current_style: data.current_style,
            forward_command: MotionCommand::default(),
            ..Default::default()
        };
        s.flags = s.build_movement_flags();
        s
    }

    // ACE: InterpretedMotionState.InterpretedMotionState
    /// `new InterpretedMotionState(MovementData data, Motion motion)`: the data's style and the
    /// motion's state. ACE shares the motion's `Commands` list; it is copied here (nothing
    /// mutates it after the message is built).
    #[must_use]
    pub fn from_motion(data: &MovementData, motion: &Motion) -> Self {
        let state = &motion.motion_state;
        let mut s = InterpretedMotionState {
            flags: MovementStateFlag::default(),
            current_style: data.current_style,
            forward_command: state.forward_command,
            sidestep_command: state.sidestep_command,
            turn_command: state.turn_command,
            forward_speed: state.forward_speed,
            sidestep_speed: state.sidestep_speed,
            turn_speed: state.turn_speed,
            commands: state.commands.clone(),
        };
        s.flags = s.build_movement_flags();
        s
    }

    // ACE: InterpretedMotionState.BuildMovementFlags
    /// Builds the MovementFlags based on the current state.
    #[must_use]
    pub fn build_movement_flags(&self) -> MovementStateFlag {
        let mut flags = MovementStateFlag::Invalid;

        if self.current_style.0 != 0 && self.current_style != MotionStance::Invalid {
            flags |= MovementStateFlag::CurrentStyle;
        }

        if self.forward_command != MotionCommand::Invalid {
            flags |= MovementStateFlag::ForwardCommand;
        }
        if self.sidestep_command != MotionCommand::Invalid {
            flags |= MovementStateFlag::SideStepCommand;
        }
        if self.turn_command != MotionCommand::Invalid {
            flags |= MovementStateFlag::TurnCommand;
        }

        if self.forward_speed != 1.0 {
            flags |= MovementStateFlag::ForwardSpeed;
        }
        if self.sidestep_speed != 1.0 {
            flags |= MovementStateFlag::SideStepSpeed;
        }
        if self.turn_speed != 1.0 {
            flags |= MovementStateFlag::TurnSpeed;
        }

        flags
    }

    // ACE: InterpretedMotionState.AddCommand
    /// `speed` defaults to 1.0 in ACE.
    pub fn add_command(
        &mut self,
        world_object: ObjectGuid,
        motion_command: MotionCommand,
        speed: f32,
    ) {
        self.commands
            .get_or_insert_with(Vec::new)
            .push(MotionItem::new(world_object, motion_command, speed));
    }

    // ACE: InterpretedMotionState.HasMovement
    #[must_use]
    pub fn has_movement(&self) -> bool {
        (self.forward_command != MotionCommand::Invalid
            && self.forward_command != MotionCommand::Ready)
            || self.turn_command != MotionCommand::Invalid
            || self.sidestep_command != MotionCommand::Invalid
    }
}

// ACE: InterpretedMotionStateExtensions.Write
/// `writer.Write(InterpretedMotionState state)`: the flags with the command count in bits 7+, the
/// flagged fields (stance and commands as `ushort`), the commands, then DWORD alignment.
/// `sequences` is the sequences of the commands' object (see `motion_item::write`).
///
/// Not ACE: the stance and commands go out in `numbering`, the world's files' (ACE's is always the
/// final one). A stance or command that numbering lacks is sent as the field's default
/// (`NonCombat`, `Ready`, none), and a queued command it lacks is left out.
#[allow(clippy::cast_possible_truncation)] // `(ushort)` of the stance and commands
pub fn write(
    writer: &mut Vec<u8>,
    state: &InterpretedMotionState,
    sequences: &mut SequenceManager,
    numbering: CommandNumbering,
) {
    let index = |id: u32, default: u32| {
        numbering::wire_index(numbering, id)
            .or_else(|| numbering::wire_index(numbering, default))
            .unwrap_or(0)
    };
    let commands: Vec<&MotionItem> = state
        .commands
        .iter()
        .flatten()
        .filter(|m| numbering::wire_index(numbering, m.motion_command.0).is_some())
        .collect();
    let num_commands = commands.len();

    // `(uint)state.Flags | (uint)numCommands << 7`
    writer.write_u32(state.flags.0 | (num_commands as u32) << 7);

    // for MotionStance / MotionCommand, write as ushort
    if (state.flags & MovementStateFlag::CurrentStyle).0 != 0 {
        writer.write_u16(index(state.current_style.0, MotionStance::NonCombat.0));
    }

    if (state.flags & MovementStateFlag::ForwardCommand).0 != 0 {
        writer.write_u16(index(state.forward_command.0, MotionCommand::Ready.0));
    }

    if (state.flags & MovementStateFlag::SideStepCommand).0 != 0 {
        writer.write_u16(index(state.sidestep_command.0, 0));
    }

    if (state.flags & MovementStateFlag::TurnCommand).0 != 0 {
        writer.write_u16(index(state.turn_command.0, 0));
    }

    if (state.flags & MovementStateFlag::ForwardSpeed).0 != 0 {
        writer.write_f32(state.forward_speed);
    }

    if (state.flags & MovementStateFlag::SideStepSpeed).0 != 0 {
        writer.write_f32(state.sidestep_speed);
    }

    if (state.flags & MovementStateFlag::TurnSpeed).0 != 0 {
        writer.write_f32(state.turn_speed);
    }

    for motion in commands {
        motion_item::write(writer, motion, sequences, numbering);
    }

    // align to DWORD boundary
    writer.align();
}
