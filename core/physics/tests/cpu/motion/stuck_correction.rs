//! A remote body stuck to another object, given a server correction its stick keeps it from
//! closing, is never snapped onto that correction; the same body not stuck gives up on it and
//! snaps. Fixture: nine flat landblocks, the human body, and a motion source that holds the body
//! where it stands every sub-step (a stick whose target is already in reach) and says whether it
//! is stuck.

use dereth_physics::PhysicsWorld;
use dereth_primitives::motion::MotionSource;
use dereth_primitives::{Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};

use crate::common::physics_fixture::{flat_world, player_geometry};

const GROUND: f32 = 20.0;

fn at(x: f32, y: f32) -> Position {
    Position::new(
        LandblockId::new(0xA9, 0xB4).cell(1),
        Frame::new(Vec3::new(x, y, GROUND), Quat::IDENTITY),
    )
}

/// What a stick does to a body whose target is already within reach: it replaces the offset
/// with none, every sub-step, so nothing else moves the body.
#[derive(Debug)]
struct Held {
    stuck: bool,
}

impl MotionSource for Held {
    fn adjust_position_offset(&mut self, offset: &mut Frame, _quantum: f64) {
        offset.origin = Vec3::ZERO;
    }
    fn advance(&mut self, _quantum: f64) -> Frame {
        Frame::default()
    }
    fn tick_movement(&mut self, _now: LocalTime) {}
    fn hit_ground(&mut self) {}
    fn leave_ground(&mut self) {}
    fn process_hooks(&mut self) {}
    fn has_collision_geometry(&self) -> bool {
        true
    }
    fn motion_max_speed(&self, _use_adjusted: bool) -> Option<f32> {
        Some(4.0)
    }
    fn is_sticky(&self) -> bool {
        self.stuck
    }
}

/// The largest distance the held body moves in one frame over two seconds at 30 frames a
/// second, after a correction 3 m away.
fn largest_frame_step(stuck: bool) -> f32 {
    let mut w: PhysicsWorld = flat_world();
    let viewer = w.create(ObjectId(0x5000_0001), player_geometry(), true);
    assert!(w.enter_world(viewer, &at(90.0, 100.0)), "the viewer stands");
    w.set_player(viewer);
    let h = w.create(ObjectId(0x5000_0002), player_geometry(), true);
    assert!(
        w.enter_world(h, &at(100.0, 100.0)),
        "the remote body stands"
    );
    w.get_mut(h)
        .expect("the remote body")
        .set_motion(Box::new(Held { stuck }));
    let mut now = 1.0;
    for _ in 0..10 {
        now += 1.0 / 30.0;
        w.use_time(LocalTime(now), false);
    }
    let corrected = w.move_or_teleport(h, &at(103.0, 100.0), false, true);
    assert!(
        matches!(
            corrected,
            dereth_physics::MoveOrTeleport::Interpolated { .. }
        ),
        "a correction 3 m off within the viewer's reach is walked, not snapped: {corrected:?}"
    );
    let mut last = w.get(h).expect("the remote body").position.frame.origin;
    let mut largest = 0.0_f32;
    for _ in 0..60 {
        now += 1.0 / 30.0;
        w.use_time(LocalTime(now), false);
        let here = w.get(h).expect("the remote body").position.frame.origin;
        let (dx, dy) = (here.x - last.x, here.y - last.y);
        largest = largest.max((dx * dx + dy * dy).sqrt());
        last = here;
    }
    largest
}

/// Behaviour: movement.stick.a-stuck-body-is-not-snapped-onto-a-correction-it-cannot-close
#[test]
fn a_stuck_body_is_not_snapped_onto_a_correction_it_cannot_close() {
    let step = largest_frame_step(true);
    assert!(step < 0.05, "the stuck body jumped {step} m in one frame");
}

/// Behaviour: movement.stick.a-stuck-body-is-not-snapped-onto-a-correction-it-cannot-close
#[test]
fn the_same_body_not_stuck_gives_up_and_snaps_onto_it() {
    let step = largest_frame_step(false);
    assert!(
        step > 2.5,
        "a body that is not stuck and cannot close a correction gives up and snaps onto it; its \
         largest step was {step} m"
    );
}
