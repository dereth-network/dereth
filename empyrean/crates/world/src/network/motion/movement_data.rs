// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Motion/MovementData.cs
//! Port of `Source/ACE.Server/Network/Motion/MovementData.cs`.
//!
//! The movement structures read `ACE.Server.Entity.Motion` (`entity/motion.rs`, re-exported here).
//!
//! ACE's writers reach the sequences through each structure's `WorldObject` back reference; here
//! the writer is given that object's `SequenceManager`.

use empyrean_entity::enums::{HoldKey, MotionCommand, MotionFlags, MotionStance, MovementType};
use empyrean_entity::ObjectGuid;

use super::interpreted_motion_state::InterpretedMotionState;
use super::move_to_object::{self, MoveToObject};
use super::move_to_position::{self, MoveToPosition};
use super::move_to_state::MoveToState;
use super::movement_invalid::{self, MovementInvalid};
use super::raw_motion_state::RawMotionFlags;
use super::turn_to_heading::{self, TurnToHeading};
use super::turn_to_object::{self, TurnToObject};
use crate::network::game_messages::game_message::BinaryWriter;
use crate::network::sequence::sequence_manager::SequenceManager;
use crate::network::sequence::sequence_type::SequenceType;
use crate::World;

// ACE: MovementData
/// The movement and animation for an object.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MovementData {
    // ACE: MovementData.WorldObject
    pub world_object: ObjectGuid,

    // ACE: MovementData.MovementSequence
    pub movement_sequence: u16,
    // ACE: MovementData.ServerControlSequence
    pub server_control_sequence: u16,
    // ACE: MovementData.IsAutonomous
    /// true = client initiated, false = server initiated
    pub is_autonomous: bool,
    // ACE: MovementData.MovementType
    pub movement_type: MovementType,
    // ACE: MovementData.MotionFlags
    pub motion_flags: MotionFlags,
    // ACE: MovementData.CurrentStyle
    pub current_style: MotionStance,

    // select one section based on MovementType:
    // ACE: MovementData.Invalid
    pub invalid: Option<MovementInvalid>,
    // ACE: MovementData.MoveToObject
    pub move_to_object: Option<MoveToObject>,
    // ACE: MovementData.MoveToPosition
    pub move_to_position: Option<MoveToPosition>,
    // ACE: MovementData.TurnToObject
    pub turn_to_object: Option<TurnToObject>,
    // ACE: MovementData.TurnToHeading
    pub turn_to_heading: Option<TurnToHeading>,
}

impl MovementData {
    // ACE: MovementData.MovementData
    /// `new MovementData(WorldObject wo)`.
    #[must_use]
    pub fn new(wo: ObjectGuid) -> Self {
        Self {
            world_object: wo,
            ..Default::default()
        }
    }

    // ACE: MovementData.MovementData
    /// `new MovementData(WorldObject wo, Motion motion)`.
    #[must_use]
    pub fn from_motion(wo: ObjectGuid, motion: &Motion) -> Self {
        let mut d = MovementData {
            world_object: wo,
            ..Default::default()
        };
        //var sequence = wo.Sequences;

        // do this here, or in network writer?
        d.is_autonomous = motion.is_autonomous;
        //MovementSequence = BitConverter.ToUInt16(sequence.GetNextSequence(SequenceType.ObjectMovement));

        //if (IsAutonomous)
        //ServerControlSequence = BitConverter.ToUInt16(sequence.GetCurrentSequence(SequenceType.ObjectServerControl));
        //else
        //ServerControlSequence = BitConverter.ToUInt16(sequence.GetNextSequence(SequenceType.ObjectServerControl));

        d.movement_type = motion.movement_type;
        d.motion_flags = motion.motion_flags;

        //if (motion.HasTarget)
        //MotionFlags |= MotionFlags.StickToObject;
        //if (motion.StandingLongJump)
        //MotionFlags |= MotionFlags.StandingLongJump;    // indicates if player started charging jump bar while standing still

        d.current_style = motion.stance;

        match d.movement_type {
            MovementType::Invalid => d.invalid = Some(MovementInvalid::from_motion(&d, motion)),
            MovementType::MoveToObject => d.move_to_object = Some(MoveToObject::new(motion)),
            MovementType::MoveToPosition => d.move_to_position = Some(MoveToPosition::new(motion)),
            MovementType::TurnToObject => d.turn_to_object = Some(TurnToObject::new(motion)),
            MovementType::TurnToHeading => d.turn_to_heading = Some(TurnToHeading::new(motion)),
            _ => {}
        }
        d
    }

    // ACE: MovementData.MovementData
    /// `new MovementData(Creature creature, MoveToState state)`: converts a MoveToState packet
    /// from the client to a MovementData packet to send to other clients. This is effectively a
    /// shortcut for converting RawMotionState -> InterpretedMotionState.
    #[must_use]
    pub fn from_move_to_state(w: &mut World, creature: ObjectGuid, state: &MoveToState) -> Self {
        let mut d = MovementData {
            world_object: creature,
            ..Default::default()
        };

        let raw_state = &state.raw_motion_state;

        // keeping most of this existing logic, ported from ConvertToClientAccepted
        if (raw_state.flags & RawMotionFlags::CurrentStyle) != 0 {
            d.current_style = raw_state.current_style;
        }

        // only using primary hold key?
        let hold_key = raw_state.current_hold_key;
        let speed = if hold_key == HoldKey::Run {
            crate::world_objects::world_object_networking::shims::creature_get_run_rate(w, creature)
        } else {
            1.0
        };

        let mut interp_state = InterpretedMotionState::from_movement_data(&d);

        // move forwards / backwards / animation
        if (raw_state.flags & RawMotionFlags::ForwardCommand) != 0 && !state.standing_long_jump {
            if raw_state.forward_command == MotionCommand::WalkForward
                || raw_state.forward_command == MotionCommand::WalkBackwards
            {
                interp_state.forward_command = MotionCommand::WalkForward;

                if raw_state.forward_command == MotionCommand::WalkForward
                    && hold_key == HoldKey::Run
                {
                    interp_state.forward_command = MotionCommand::RunForward;
                }

                interp_state.forward_speed = speed;

                if raw_state.forward_command == MotionCommand::WalkBackwards {
                    interp_state.forward_speed *= -0.65;
                }
            } else {
                interp_state.forward_command = raw_state.forward_command;
            }
        }

        // sidestep
        if (raw_state.flags & RawMotionFlags::SideStepCommand) != 0 && !state.standing_long_jump {
            interp_state.sidestep_command = MotionCommand::SideStepRight;
            interp_state.sidestep_speed = speed * 3.12 / 1.25 * 0.5;

            if raw_state.sidestep_command == MotionCommand::SideStepLeft {
                interp_state.sidestep_speed *= -1.0;
            }

            interp_state.sidestep_speed = math_clamp(interp_state.sidestep_speed, -3.0, 3.0);
        }

        // rotate
        if (raw_state.flags & RawMotionFlags::TurnCommand) != 0 {
            interp_state.turn_command = MotionCommand::TurnRight;
            interp_state.turn_speed = if hold_key == HoldKey::Run { 1.5 } else { 1.0 };

            // mouselook
            if raw_state.turn_speed != 0.0 && raw_state.turn_speed <= 1.5 {
                interp_state.turn_speed = raw_state.turn_speed;
            }

            if raw_state.turn_command == MotionCommand::TurnLeft {
                interp_state.turn_speed *= -1.0;
            }
        }

        // contact/sticky?
        // this alone isn't enough for standing long jump,
        // and observing clients seems to show a buggy shallow arc jump
        // without the above exclusions of ForwardCommand / SidestepCommand
        if state.standing_long_jump {
            d.motion_flags |= MotionFlags::StandingLongJump;
        }

        interp_state.commands.clone_from(&raw_state.commands);
        interp_state.flags = interp_state.build_movement_flags();

        // this is a hack to make walking work correctly - investigate this
        // wouldn't all of these be autonomous?
        // walk backwards?
        //if (holdKey != HoldKey.Invalid || rawState.ForwardCommand == MotionCommand.WalkForward)
        d.is_autonomous = true;

        d.invalid = Some(MovementInvalid::from_state(interp_state));
        d
    }

    // ACE: MovementData.Serialize
    /// Serializes this movement data to a byte array (no header). Currently only used for
    /// CreateObject messages in ACE's old method.
    #[must_use]
    pub fn serialize(&self, sequences: &mut SequenceManager) -> Vec<u8> {
        let mut writer = Vec::new();
        write(&mut writer, self, false, sequences);
        writer
    }
}

/// `Math.Clamp(float, float, float)`.
fn math_clamp(value: f32, min: f32, max: f32) -> f32 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

// ACE: MovementDataExtensions.Write
/// `writer.Write(MovementData motion, bool header = true)`. `sequences` is
/// `motion.WorldObject.Sequences`: the header advances its movement sequence (and its server
/// control sequence for a server-initiated motion).
#[allow(clippy::cast_possible_truncation)] // `(ushort)motion.CurrentStyle`
pub fn write(
    writer: &mut Vec<u8>,
    motion: &MovementData,
    header: bool,
    sequences: &mut SequenceManager,
) {
    if header {
        writer.write_bytes(&sequences.get_next_sequence(SequenceType::ObjectMovement));

        if motion.is_autonomous {
            writer.write_bytes(&sequences.get_current_sequence(SequenceType::ObjectServerControl));
        } else {
            writer.write_bytes(&sequences.get_next_sequence(SequenceType::ObjectServerControl));
        }

        writer.write_u8(u8::from(motion.is_autonomous));
        writer.align();
    }

    writer.write_u8(motion.movement_type.0);
    writer.write_u8(motion.motion_flags.0);

    writer.write_u16(motion.current_style.0 as u16); // send MotionStance as ushort

    // A null section for its movement type throws in ACE.
    match motion.movement_type {
        MovementType::Invalid => {
            let invalid = motion
                .invalid
                .as_ref()
                .expect("ACE: motion.Invalid is null");
            movement_invalid::write(writer, invalid, motion.motion_flags, sequences);
        }
        MovementType::MoveToObject => {
            move_to_object::write(
                writer,
                motion
                    .move_to_object
                    .as_ref()
                    .expect("ACE: motion.MoveToObject is null"),
            );
        }
        MovementType::MoveToPosition => {
            move_to_position::write(
                writer,
                motion
                    .move_to_position
                    .as_ref()
                    .expect("ACE: motion.MoveToPosition is null"),
            );
        }
        MovementType::TurnToObject => {
            turn_to_object::write(
                writer,
                motion
                    .turn_to_object
                    .as_ref()
                    .expect("ACE: motion.TurnToObject is null"),
            );
        }
        MovementType::TurnToHeading => {
            turn_to_heading::write(
                writer,
                motion
                    .turn_to_heading
                    .as_ref()
                    .expect("ACE: motion.TurnToHeading is null"),
            );
        }
        _ => {}
    }
}

/// `ACE.Server.Entity.Motion`, ported in `entity/motion.rs`.
pub use crate::entity::motion::Motion;
