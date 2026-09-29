// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Motion.cs
//! Port of `Source/ACE.Server/Entity/Motion.cs` (`network/motion/movement_data.rs` re-exports
//! it).

use empyrean_entity::enums::{MotionCommand, MotionFlags, MotionStance, MovementType};
use empyrean_entity::{ObjectGuid, Position};

use crate::network::motion::interpreted_motion_state::InterpretedMotionState;
use crate::network::motion::move_to_parameters::MoveToParameters;
use crate::World;

// ACE: Motion
/// Convenience wrapper for transitioning the most common uses of Motion.
#[derive(Debug, Clone)]
pub struct Motion {
    /// previously in MotionState base abstract class, only 1 reference. IsAutonomous TRUE would
    /// indicate a client-initiated movement
    pub is_autonomous: bool,
    /// 5 different modes of operation: Invalid (General), MoveToObject, MoveToPosition,
    /// TurnToObject, TurnToHeading
    pub movement_type: MovementType,
    /// stick to object / standing long jump
    pub motion_flags: MotionFlags,
    /// CurrentStyle
    pub stance: MotionStance,
    /// General - InterpretedMotionState / StickyObject (`new InterpretedMotionState()`)
    pub motion_state: InterpretedMotionState,
    /// this is dual-purposed for both Target and StickyObject (fixme)
    pub target_guid: ObjectGuid,
    /// used for: MoveTo's
    pub position: Option<Position>,
    /// used for MoveTo and TurnTo (`new MoveToParameters()`)
    pub move_to_parameters: MoveToParameters,
    /// used for: MoveTo's
    pub run_rate: f32,
    /// overrides MovementParams version
    pub desired_heading: f32,
}

impl Motion {
    // ACE: Motion.Motion
    /// `new Motion(MotionStance stance)`: used to switch combat stances.
    #[must_use]
    pub fn from_stance(stance: MotionStance) -> Self {
        Motion {
            is_autonomous: false,
            movement_type: MovementType::default(),
            motion_flags: MotionFlags::default(),
            stance,
            motion_state: InterpretedMotionState::new(),
            target_guid: ObjectGuid::default(),
            position: None,
            move_to_parameters: MoveToParameters::default(),
            run_rate: 0.0,
            desired_heading: 0.0,
        }
    }

    // ACE: Motion.Motion
    /// `new Motion(MotionStance stance, MotionCommand motion, float speed = 1.0f)`.
    #[must_use]
    pub fn new(stance: MotionStance, motion: MotionCommand, speed: f32) -> Self {
        let mut m = Motion::from_stance(stance);
        m.set_forward_command(motion, speed);
        m
    }

    /// The stance `new Motion(WorldObject wo, ...)` starts from: the object's current stance, or
    /// NonCombat (with ACE's warning) when it has no `CurrentMotionState`.
    fn stance_of(w: &World, wo: ObjectGuid) -> MotionStance {
        match w.objects.get(wo).and_then(|o| {
            crate::world_objects::world_object_networking::shims::current_motion_state(o)
        }) {
            Some(current) => current.stance,
            None => {
                log::warn!("{wo:?} has a null CurrentMotionState, subbing in new Motion(MotionStance.NonCombat) for it.");
                MotionStance::NonCombat
            }
        }
    }

    // ACE: Motion.Motion
    /// `new Motion(WorldObject wo, WorldObject target, MovementType type)`: a new MoveToObject /
    /// TurnToObject motion.
    #[must_use]
    pub fn to_object(w: &World, wo: ObjectGuid, target: ObjectGuid, r#type: MovementType) -> Self {
        let mut m = Motion::from_stance(Motion::stance_of(w, wo));
        m.movement_type = r#type;
        let location = w
            .objects
            .get(target)
            .and_then(crate::world_objects::world_object::WorldObject::location);
        m.position = Some(Position::from_position(
            &location.expect("ACE: target.Location is null"),
        ));
        m.target_guid = target;
        m
    }

    // ACE: Motion.Motion
    /// `new Motion(WorldObject wo, Position position)`: a new MoveToPosition motion.
    #[must_use]
    pub fn to_position(w: &World, wo: ObjectGuid, position: &Position) -> Self {
        let mut m = Motion::from_stance(Motion::stance_of(w, wo));
        m.movement_type = MovementType::MoveToPosition;
        m.position = Some(Position::from_position(position));
        m
    }

    // ACE: Motion.Motion
    /// `new Motion(WorldObject wo, Position position, float heading)`: a new TurnToHeading motion.
    #[must_use]
    pub fn to_heading(w: &World, wo: ObjectGuid, position: &Position, heading: f32) -> Self {
        let mut m = Motion::from_stance(Motion::stance_of(w, wo));
        m.movement_type = MovementType::TurnToHeading;
        m.position = Some(Position::from_position(position));
        m.desired_heading = heading;
        m
    }

    // ACE: Motion.Motion
    /// `new Motion(WorldObject wo, MotionCommand motion, float speed = 1.0f)`.
    #[must_use]
    pub fn from_world_object(w: &World, wo: ObjectGuid, motion: MotionCommand, speed: f32) -> Self {
        let mut m = Motion::from_stance(Motion::stance_of(w, wo));
        m.set_forward_command(motion, speed);
        m
    }

    // ACE: Motion.SetForwardCommand
    /// `SetForwardCommand(MotionCommand motion, float speed = 1.0f)`.
    pub fn set_forward_command(&mut self, motion: MotionCommand, speed: f32) {
        self.motion_state.forward_command = motion;
        self.motion_state.forward_speed = speed;
    }

    // ACE: Motion.SetSidestepCommand
    /// `SetSidestepCommand(MotionCommand motion, float speed = 1.0f)`.
    pub fn set_sidestep_command(&mut self, motion: MotionCommand, speed: f32) {
        self.motion_state.sidestep_command = motion;
        self.motion_state.sidestep_speed = speed;
    }

    // ACE: Motion.SetTurnCommand
    /// `SetTurnCommand(MotionCommand motion, float speed = 1.0f)`.
    pub fn set_turn_command(&mut self, motion: MotionCommand, speed: f32) {
        self.motion_state.turn_command = motion;
        self.motion_state.turn_speed = speed;
    }

    // ACE: Motion.Persist
    /// `Persist(Motion motion)`: keeps the sidestep and turn of `motion`.
    pub fn persist(&mut self, motion: &Motion) {
        self.set_sidestep_command(
            motion.motion_state.sidestep_command,
            motion.motion_state.sidestep_speed,
        );
        self.set_turn_command(
            motion.motion_state.turn_command,
            motion.motion_state.turn_speed,
        );
    }

    // ACE: Motion.Motion
    /// `new Motion(Motion motion)`: the copy constructor (every reference field is copied deep).
    #[must_use]
    pub fn from_motion(motion: &Motion) -> Self {
        motion.clone()
    }
}
