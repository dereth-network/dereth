// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Motion/MoveToParameters.cs
//! Port of `Source/ACE.Server/Network/Motion/MoveToParameters.cs`.

use empyrean_entity::enums::MovementParams;

use crate::network::game_messages::game_message::BinaryWriter;

// ACE: MoveToParameters
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoveToParameters {
    // ACE: MoveToParameters.MovementParameters
    pub movement_parameters: MovementParams,
    // ACE: MoveToParameters.DistanceToObject
    /// move within this distance to the object
    pub distance_to_object: f32,
    // ACE: MoveToParameters.MinDistance
    /// the minimum distance to move
    pub min_distance: f32,
    // ACE: MoveToParameters.FailDistance
    /// the distance at which the movement will fail
    pub fail_distance: f32,
    // ACE: MoveToParameters.Speed
    /// walk/run speed multiplier
    pub speed: f32,
    // ACE: MoveToParameters.WalkRunThreshold
    /// if below this distance, walk instead of run
    pub walk_run_threshold: f32,
    // ACE: MoveToParameters.DesiredHeading
    /// heading to turn to
    pub desired_heading: f32,
}

impl Default for MoveToParameters {
    // ACE: MoveToParameters.MoveToParameters
    /// `new MoveToParameters()`: [`MoveToParameters::set_defaults`]. The copy constructor is
    /// `Clone`.
    fn default() -> Self {
        let mut p = MoveToParameters {
            movement_parameters: MovementParams(0),
            distance_to_object: 0.0,
            min_distance: 0.0,
            fail_distance: 0.0,
            speed: 0.0,
            walk_run_threshold: 0.0,
            desired_heading: 0.0,
        };
        p.set_defaults();
        p
    }
}

impl MoveToParameters {
    // ACE: MoveToParameters.SetDefaults
    pub fn set_defaults(&mut self) {
        self.movement_parameters = MovementParams::CanWalk
            | MovementParams::CanRun
            | MovementParams::CanSideStep
            | MovementParams::CanWalkBackwards
            | MovementParams::MoveTowards
            | MovementParams::UseSpheres
            | MovementParams::SetHoldKey
            | MovementParams::ModifyRawState
            | MovementParams::ModifyInterpretedState
            | MovementParams::CancelMoveTo
            | MovementParams::StopCompletely;

        self.min_distance = 0.0;
        self.fail_distance = f32::MAX;
        self.speed = 1.0;
        self.walk_run_threshold = 15.0;
        self.desired_heading = 0.0;
        self.distance_to_object = 0.6;
    }
}

/// Not ACE's (retail, V257; the retail captures): the MoveTo flag words Turbine's server sent, by kind of
/// move. Every retail MoveTo carried a walk/run threshold of 15.0 ([`RETAIL_WALK_RUN_THRESHOLD`]);
/// bits 18-31 held server junk and are sent as zero. ACE's gameplay moves keep CanWalk and CanRun
/// on its attack chases, add CanCharge to use-moves of 7.5 m or more, and use a threshold of 1.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetailMoveTo {
    /// An attack chase (a monster or pet after its target, the player's melee charge and melee
    /// approach): CanCharge set; CanWalk, CanRun, CanSideStep and CanWalkBackwards cleared; fails
    /// on walk, uses the final heading, sticks and may move away. `0x1EFF0`.
    AttackChase,
    /// [`Self::AttackChase`] without Sticky. `0x1EF70`. Retail monsters sent it while closing to
    /// a distance from the player before a spell or missile attack; sent by ranged and casting
    /// monsters with the `monster_ranged_closing` option on (V336). ACE's ranged monsters turn
    /// instead of moving, and every ACE chase is sticky.
    AttackChaseUnstuck,
    /// A plain move and an item pickup: the client's defaults. `0x1EE0F`.
    Plain,
    /// Using a door, corpse, chest or vendor, and every NPC move: the defaults plus
    /// UseFinalHeading. `0x1EE4F`.
    Use,
    /// Using a portal: [`Self::Use`] without UseSpheres. `0x1EA4F`.
    Portal,
}

/// The walk/run threshold of every retail MoveTo (V257), the client's own default.
pub const RETAIL_WALK_RUN_THRESHOLD: f32 = dereth_animation::motion::DEFAULT_WALK_RUN_THRESHOLD;

impl RetailMoveTo {
    /// The flag word.
    #[must_use]
    pub const fn flags(self) -> u32 {
        use dereth_animation::motion::{flags as f, DEFAULT_FLAGS};
        const CAN_MOVE: u32 = f::CAN_WALK | f::CAN_RUN | f::CAN_SIDESTEP | f::CAN_WALK_BACKWARDS;
        const CHASE: u32 = (DEFAULT_FLAGS & !CAN_MOVE)
            | f::CAN_CHARGE
            | f::FAIL_WALK
            | f::USE_FINAL_HEADING
            | f::MOVE_AWAY;
        match self {
            Self::AttackChase => CHASE | f::STICKY,
            Self::AttackChaseUnstuck => CHASE,
            Self::Plain => DEFAULT_FLAGS,
            Self::Use => DEFAULT_FLAGS | f::USE_FINAL_HEADING,
            Self::Portal => (DEFAULT_FLAGS | f::USE_FINAL_HEADING) & !f::USE_SPHERES,
        }
    }

    /// The flag word as the network parameters carry it.
    #[must_use]
    pub const fn movement_params(self) -> MovementParams {
        MovementParams(self.flags())
    }

    /// Sets the flag word and the walk/run threshold of `mvp`.
    pub fn apply(self, mvp: &mut MoveToParameters) {
        mvp.movement_parameters = self.movement_params();
        mvp.walk_run_threshold = RETAIL_WALK_RUN_THRESHOLD;
    }

    /// The server body's parameters for the same move: the client's constructor with this flag
    /// word (the threshold is already 15.0).
    #[must_use]
    pub fn movement_parameters(self) -> dereth_animation::motion::MovementParameters {
        dereth_animation::motion::MovementParameters {
            flags: self.flags(),
            ..dereth_animation::motion::MovementParameters::default()
        }
    }
}

// ACE: MoveToParametersExtensions.Write
pub fn write(writer: &mut Vec<u8>, mvp: &MoveToParameters) {
    writer.write_u32(mvp.movement_parameters.0);
    writer.write_f32(mvp.distance_to_object);
    writer.write_f32(mvp.min_distance);
    writer.write_f32(mvp.fail_distance);
    writer.write_f32(mvp.speed);
    writer.write_f32(mvp.walk_run_threshold);
    writer.write_f32(mvp.desired_heading);
}
