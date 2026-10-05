//! What happens when a movement operation finishes: an approach that arrives sticks to its target
//! before the next operation starts, an action's animation-done callback unsticks the body and
//! clears its target subscription, and a sticky timeout cancels the approach and drains both motion
//! queues in the same phase. The body then walks and stops under manual input again.
//! Fixture: the production `Character` on real terrain with the retail dats; no device.

use std::sync::Arc;

use dereth_animation::motion::{flags, MoveToRequest, MovementParameters};
use dereth_animation::MotionCommand;
use dereth_primitives::{LocalTime, MotionSource, ObjectId, Vec3};
use {
    dereth_client_runtime::character::Character, dereth_client_runtime::character::CharacterInput,
};
use {dereth_world_data::landblock::load_region, dereth_world_data::landblock::DEFAULT_LANDBLOCK};

struct Body {
    c: Character,
    frame: u32,
}

impl Body {
    fn new() -> Self {
        let store = Arc::new(dereth_dat::testing::open_store().expect("required retail DATs"));
        let region = load_region(&store).expect("real region");
        let c = Character::new(&store, &region, DEFAULT_LANDBLOCK, (96.0, 96.0))
            .expect("real DAT body and terrain");
        let mut body = Self { c, frame: 0 };
        body.frames(60);
        assert!(body.c.on_ground());
        body
    }

    fn frames(&mut self, count: u32) {
        for _ in 0..count {
            self.frame += 1;
            self.c.update(LocalTime(f64::from(self.frame) / 30.0));
        }
    }

    fn next_manual_move_and_release(&mut self) {
        self.c.input = CharacterInput::default();
        self.frames(60);
        assert!(!self.c.driver().movement.motions_pending());
        assert!(self.c.driver().motion_table.pending().is_empty());
        let before = self.c.position();
        self.c.input.forward = true;
        self.frames(45);
        assert!(dereth_animation::motion::moveto::distance(&before, &self.c.position()) > 0.5);
        assert_eq!(
            self.c.driver().movement.interp.raw_state.forward_command,
            MotionCommand::WALK_FORWARD
        );
        self.c.input = CharacterInput::default();
        self.frames(60);
        let stopped = self.c.position();
        assert_eq!(
            self.c
                .driver()
                .movement
                .interp
                .interpreted_state
                .forward_command,
            MotionCommand::READY
        );
        assert!(!self.c.is_moving_to());
        self.frames(30);
        assert!(dereth_animation::motion::moveto::distance(&stopped, &self.c.position()) < 0.05);
    }
}

/// Behaviour: movement.sticky.arrival-sticks-to-the-target-before-the-next-operation
#[test]
fn arrival_sticks_to_the_captured_target_before_the_next_operation() {
    let mut b = Body::new();
    let target = ObjectId(0x7000_1011);
    let params = MovementParameters {
        flags: MovementParameters::default().flags | flags::STICKY,
        ..MovementParameters::default()
    };
    b.c.perform_move_to(
        &MoveToRequest::MoveToObject {
            object_id: target,
            top_level_id: target,
            radius: 0.73,
            height: 1.61,
        },
        &params,
        None,
    );
    assert!(b.c.is_moving_to());
    assert_eq!(b.c.wanted_target().map(|t| t.id), Some(target));
    // Already in range: the target update reaches the next-node path's empty arrival arm.
    b.c.update_target(b.c.position(), Vec3::ZERO, true);
    assert!(
        !b.c.is_moving_to(),
        "arrival cleanup must finish before the body sticks to the target"
    );
    assert_eq!(
        b.c.sticky_target(),
        Some(target),
        "arrival reaches the position owner"
    );
    assert_eq!(b.c.driver().movement.sticky.target_radius, 0.73);
    assert_eq!(
        b.c.wanted_target().map(|t| t.id),
        Some(target),
        "cleanup's target clear must precede the fresh sticky subscription"
    );
    assert_eq!(
        b.c.driver().movement.moveto.top_level_object_id,
        ObjectId(0)
    );
    b.c.unstick_from_object();
    assert_eq!(b.c.wanted_target(), None);
    b.next_manual_move_and_release();
}

/// Behaviour: movement.sticky.animation-done-unsticks-before-manual-motion
#[test]
fn real_animation_done_unsticks_before_returning_to_manual_motion() {
    let mut b = Body::new();
    let target = ObjectId(0x7000_1012);
    let command = MotionCommand::WAVE;
    assert_eq!(
        b.c.driver_mut()
            .do_interpreted_motion(command, &MovementParameters::default()),
        0
    );
    assert!(b
        .c
        .driver()
        .movement
        .interp
        .pending_motions
        .iter()
        .any(|n| n.motion == command));
    assert!(b
        .c
        .driver()
        .motion_table
        .pending()
        .iter()
        .any(|n| n.motion == command));
    let mut completed = false;
    for _ in 0..300 {
        // An explicit sticky-target refresh before each physics observation keeps the deadline
        // fresh without injecting a completion hook or editing either motion ledger.
        b.c.stick_to_object(target, 0.4, 1.0);
        assert_eq!(b.c.sticky_target(), Some(target));
        b.frames(1);
        if !b
            .c
            .driver()
            .movement
            .interp
            .pending_motions
            .iter()
            .any(|n| n.motion == command)
        {
            completed = true;
            break;
        }
    }
    assert!(
        completed,
        "the retail action must actually reach its animation-done hook"
    );
    assert!(b
        .c
        .driver()
        .movement
        .interp
        .interpreted_state
        .actions
        .is_empty());
    assert!(!b
        .c
        .driver()
        .motion_table
        .pending()
        .iter()
        .any(|n| n.motion == command));
    assert_eq!(
        b.c.sticky_target(),
        None,
        "the completed motion must unstick the body, not only queue an effect"
    );
    assert_eq!(
        b.c.wanted_target(),
        None,
        "the owner callback clears the subscription too"
    );
    b.next_manual_move_and_release();
}

#[test]
fn internal_link_removal_reenters_the_current_owner_before_dead_continues() {
    let mut b = Body::new();
    let target = ObjectId(0x7000_1013);
    b.c.perform_move_to(
        &MoveToRequest::TurnToObject {
            object_id: target,
            top_level_id: target,
        },
        &MovementParameters::default(),
        None,
    );
    assert!(b.c.is_moving_to());
    assert_eq!(
        b.c.driver_mut()
            .do_interpreted_motion(MotionCommand::WAVE, &MovementParameters::default()),
        0
    );
    b.c.stick_to_object(target, 0.4, 1.0);
    assert!(
        b.c.is_moving_to(),
        "the first stick does not cancel a preexisting approach"
    );
    assert!(b
        .c
        .driver()
        .movement
        .interp
        .pending_motions
        .iter()
        .any(|n| n.motion.is_action()));
    // Outside a cell, DEAD removes link animations before its own table operation. Releasing the
    // sticky target re-enters move-to cancellation; stopping then removes link animations again
    // with no cell, before the outer completion pop.
    let err = {
        let mut driver = b.c.driver_mut();
        driver.env.in_cell = false;
        let err = driver.do_interpreted_motion(MotionCommand::DEAD, &MovementParameters::default());
        driver.env.in_cell = true;
        err
    };
    assert_eq!(err, 0);
    assert_eq!(
        b.c.sticky_target(),
        None,
        "internal removal must unstick the body inline"
    );
    assert!(
        !b.c.is_moving_to(),
        "unsticking must cancel through the currently borrowed owner"
    );
    assert!(b.c.driver().movement.interp.pending_motions.is_empty());
    assert!(b.c.driver().motion_table.pending().is_empty());
    assert!(b
        .c
        .driver()
        .movement
        .interp
        .interpreted_state
        .actions
        .is_empty());
    assert_eq!(
        b.c.driver().target,
        None,
        "subscription is cleared inside the driver call"
    );
    let next = ObjectId(0x7000_1014);
    b.c.perform_move_to(
        &MoveToRequest::TurnToObject {
            object_id: next,
            top_level_id: next,
        },
        &MovementParameters::default(),
        None,
    );
    assert_eq!(b.c.driver().target.map(|t| t.id), Some(next));
    b.frames(1);
    assert_eq!(
        b.c.driver().target.map(|t| t.id),
        Some(next),
        "the old callback's target clear cannot arrive after the next operation"
    );
    b.c.move_to_interpreted_state(&Default::default());
    b.next_manual_move_and_release();
}

#[test]
fn driver_position_expiry_runs_after_the_part_array_completion_phase() {
    let mut b = Body::new();
    let target = ObjectId(0x7000_1021);
    b.c.perform_move_to(
        &MoveToRequest::TurnToObject {
            object_id: target,
            top_level_id: target,
        },
        &MovementParameters::default(),
        None,
    );
    b.c.stick_to_object(target, 0.4, 1.0);
    let deadline = b.c.driver().movement.sticky.sticky_timeout_time;
    {
        let mut driver = b.c.driver_mut();
        assert!(driver.motion_table.pending().is_empty());
        assert!(driver.movement.interp.pending_motions.is_empty());
        // One explicit production driver phase, not an App frame that may execute many quanta.
        driver.tick_movement(LocalTime(deadline + 0.1));
        assert!(!driver.movement.is_moving_to());
        assert!(!driver.movement.sticky.is_sticky());
        assert_eq!(driver.target, None);
        // Position-phase cancellation queues READY after the ordinary movement
        // drain, but stopping runs movement completion again. A READY with nothing to play
        // therefore pops inside that same call. Both ledgers are empty when the phase returns,
        // not one tick later.
        assert!(
            driver.movement.interp.pending_motions.is_empty(),
            "stopping completes its own pending READY motion: {:?}",
            driver
                .movement
                .interp
                .pending_motions
                .iter()
                .map(|n| n.motion)
                .collect::<Vec<_>>()
        );
        assert!(driver.motion_table.pending().is_empty());
        driver.tick_movement(LocalTime(deadline + 0.2));
        assert!(driver.movement.interp.pending_motions.is_empty());
        assert!(driver.motion_table.pending().is_empty());
    }
    b.next_manual_move_and_release();
}
