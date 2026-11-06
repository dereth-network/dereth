//! Missiles and combat stories: a missile's guidance (the velocity it wants toward its target,
//! its speed, height, turning and fuel rules, its launch and placement, a step of its flight, what
//! sets it off) and a story played on an attacker's body in step with its animation (hits, missiles
//! made in hand and fired, and the story's binary form).
//! Fixture: made-up bodies, objects and stories behind recording hosts.

use auberean_assets::Context;
use auberean_dat::Reader;
use auberean_physics::collision::{state, CollisionReport, CollisionSettings, ContactRecord};
use auberean_physics::geom::Heading;
use auberean_physics::missile::{
    fail, ground, set, Guidance, GuidanceHost, MissileParameters, MissileTargetingParameters,
    FLIGHT_FX,
};
use auberean_physics::pack::{Pack, Writer};
use auberean_physics::story::{
    kind, Behavior, FxAndBehaviorInfo, MissileInfo, PhysicsStory, StoryHookData, StoryHost,
    StoryPlayer, FADE_SLIDER, STORY_HOOK,
};
use auberean_physics::target::Aim;
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

const CELL: CellId = CellId(0x2020_0011);

fn at(x: f32, y: f32, z: f32) -> Position {
    Position::new(CELL, Frame::new(Vec3::new(x, y, z), Quat::IDENTITY))
}

fn close(a: Vec3, b: Vec3, tol: f32) -> bool {
    (a.x - b.x).abs() <= tol && (a.y - b.y).abs() <= tol && (a.z - b.z).abs() <= tol
}

fn length(v: Vec3) -> f32 {
    (v.x * v.x + v.y * v.y + v.z * v.z).sqrt()
}

/// A missile's body and the objects around it, recording what the guidance asks of them.
#[derive(Default)]
struct Sky {
    now: f64,
    position: Option<Position>,
    gravity: Vec3,
    wind: Vec3,
    objects: Vec<(u64, Position)>,
    creatures: Vec<u64>,
    dynamic: Vec<u64>,
    tracked: u64,
    velocity: Option<Vec3>,
    placed: Option<Position>,
    aligned: bool,
    streak: bool,
    fx: Vec<u32>,
    stopped_fx: Vec<u32>,
    expired: u32,
    out_of_fuel: u32,
    deleted: Option<(f64, f64)>,
    stuck: Option<u64>,
}

impl GuidanceHost for Sky {
    fn now(&self) -> f64 {
        self.now
    }
    fn position(&self) -> Position {
        self.position.unwrap_or_else(|| at(0.0, 0.0, 0.0))
    }
    fn drawing_position(&self) -> Position {
        self.position()
    }
    fn has_parent(&self) -> bool {
        false
    }
    fn is_in_world(&self) -> bool {
        self.placed.is_some()
    }
    fn is_valid(&self, p: &Position) -> bool {
        p.cell.0 != 0
    }
    fn gravity(&self) -> Vec3 {
        self.gravity
    }
    fn wind(&self) -> Vec3 {
        self.wind
    }
    fn height_above_walkable(&self) -> Option<f32> {
        None
    }
    fn set_missile_velocity(&mut self, v: Vec3) {
        self.velocity = Some(v);
    }
    fn align_to_path(&mut self, on: bool) {
        self.aligned = on;
    }
    fn report_out_of_fuel(&mut self) {
        self.out_of_fuel += 1;
    }
    fn report_expired(&mut self) {
        self.expired += 1;
    }
    fn init_end(&mut self, at: &Position) -> bool {
        self.placed = Some(*at);
        true
    }
    fn target_id(&self) -> u64 {
        self.tracked
    }
    fn set_target(&mut self, _context: u32, object: u64, _radius: f32, _quantum: f32) {
        self.tracked = object;
    }
    fn start_streak(&mut self) {
        self.streak = true;
    }
    fn stop_streak(&mut self) {
        self.streak = false;
    }
    fn do_fx(&mut self, fx: u32, _intensity: f32) {
        self.fx.push(fx);
    }
    fn stop_fx(&mut self, fx: u32) {
        self.stopped_fx.push(fx);
    }
    fn delete_self(&mut self, fade: f64, delay: f64) {
        self.deleted = Some((fade, delay));
    }
    fn stick_to(&mut self, object: u64) {
        self.stuck = Some(object);
    }
    fn random_check(&mut self, _chance: f32) -> bool {
        false
    }
    fn object_exists(&self, object: u64) -> bool {
        self.objects.iter().any(|(id, _)| *id == object)
    }
    fn object_position(&self, object: u64) -> Option<Position> {
        self.objects
            .iter()
            .find(|(id, _)| *id == object)
            .map(|(_, p)| *p)
    }
    fn object_heading(&self, _object: u64) -> Option<Heading> {
        None
    }
    fn object_offset_position(
        &self,
        object: u64,
        _height: f32,
        _scaled: bool,
        _offset: Vec3,
    ) -> Option<Position> {
        self.object_position(object)
    }
    fn object_velocity(&self, _object: u64) -> Vec3 {
        Vec3::ZERO
    }
    fn object_is_dynamic(&self, object: u64) -> bool {
        self.dynamic.contains(&object)
    }
    fn object_has_guidance(&self, _object: u64) -> bool {
        false
    }
    fn object_is_player_or_creature(&self, object: u64) -> bool {
        self.creatures.contains(&object)
    }
}

/// Guidance aimed at a target heard of at `target`, moving at `velocity`.
fn aimed(target: Position, velocity: Vec3) -> Guidance {
    let mut g = Guidance::new();
    g.target = Aim {
        object: 0,
        position: target,
        offset: velocity,
        valid: true,
    };
    g
}

/// A missile with no target velocity, 10 m south of a still target, wants to fly straight at it
/// at its top speed (50 m/s).
///
/// Behaviour: missile.guidance.a-missile-meets-a-still-target-head-on
#[test]
fn a_missile_meets_a_still_target_head_on() {
    let g = Guidance::new();
    let mut v = Vec3::ZERO;
    g.collide_velocity(
        0.0,
        Vec3::new(0.0, -10.0, 0.0),
        Vec3::ZERO,
        false,
        &mut v,
        false,
    );
    assert!(close(v, Vec3::new(0.0, 50.0, 0.0), 1e-3), "{v:?}");
}

/// Against a target crossing its path the missile aims where the target will be: at its own
/// speed, with the target's sideways speed, so both arrive at the same place at the same time.
///
/// Behaviour: missile.guidance.a-missile-leads-a-moving-target
#[test]
fn a_missile_leads_a_moving_target() {
    let g = Guidance::new();
    let (offset, target_v) = (Vec3::new(0.0, -100.0, 0.0), Vec3::new(10.0, 0.0, 0.0));
    let mut v = Vec3::ZERO;
    g.collide_velocity(0.0, offset, target_v, false, &mut v, false);
    assert!((length(v) - 50.0).abs() < 1e-2, "{v:?}");
    assert!((v.x - 10.0).abs() < 1e-4);
    // Where the missile and the target are after the time it takes to close the distance.
    let t = 100.0 / v.y;
    let missile = Vec3::new(v.x * t, v.y * t - 100.0, 0.0);
    let target = Vec3::new(target_v.x * t, 0.0, 0.0);
    assert!(close(missile, target, 1e-3), "{missile:?} {target:?}");
}

/// A target fleeing faster than the missile can fly gives no time to meet it: the missile keeps
/// the velocity it wanted before (only the speed rules apply), rather than chasing.
///
/// Behaviour: missile.guidance.a-missile-that-cannot-catch-its-target-keeps-the-velocity-it-wanted
#[test]
fn a_missile_that_cannot_catch_its_target_keeps_the_velocity_it_wanted() {
    let g = Guidance::new();
    let mut v = Vec3::new(3.0, 4.0, 0.0);
    g.collide_velocity(
        0.0,
        Vec3::new(0.0, -10.0, 0.0),
        Vec3::new(0.0, 100.0, 0.0),
        false,
        &mut v,
        false,
    );
    assert_eq!(v, Vec3::new(3.0, 4.0, 0.0));
}

/// A missile keeping 5 m from a still target 20 m away flies sideways around it at the distance
/// it keeps and toward it by the gap still to close; from the target's own place it flies east.
///
/// Behaviour: missile.guidance.a-missile-keeping-its-distance-circles-and-closes-the-gap
#[test]
fn a_missile_keeping_its_distance_circles_and_closes_the_gap() {
    let mut g = Guidance::new();
    g.params.set_maintain_distance(5.0);
    g.params.set_turn_method(1);
    let mut host = Sky::default();
    let mut v = Vec3::ZERO;
    g.maint_dist_velocity(
        &mut host,
        Vec3::new(20.0, 0.0, 0.0),
        Vec3::ZERO,
        false,
        &mut v,
        true,
    );
    assert!(close(v, Vec3::new(-15.0, 5.0, 0.0), 1e-5), "{v:?}");
    g.params.set_turn_method(0);
    g.maint_dist_velocity(
        &mut host,
        Vec3::new(20.0, 0.0, 0.0),
        Vec3::ZERO,
        false,
        &mut v,
        true,
    );
    assert!(
        close(v, Vec3::new(-15.0, -5.0, 0.0), 1e-5),
        "the other way round: {v:?}"
    );
    g.maint_dist_velocity(&mut host, Vec3::ZERO, Vec3::ZERO, false, &mut v, true);
    assert!(close(v, Vec3::new(1.0, 0.0, 0.0), 1e-6), "{v:?}");
}

/// Every wanted velocity is kept between the least and greatest speeds (no length becomes due
/// north), and a missile following the ground climbs at least to its height over it, or holds it.
///
/// Behaviour: missile.guidance.speed-and-height-rules-shape-every-wanted-velocity
#[test]
fn speed_and_height_rules_shape_every_wanted_velocity() {
    let mut g = Guidance::new();
    g.params.set_speed_limits(20.0, 2.0);
    let mut v = Vec3::new(0.0, 0.0, 0.5);
    g.adjust_speed(&mut v);
    assert!(close(v, Vec3::new(0.0, 0.0, 2.0), 1e-6));
    let mut v = Vec3::ZERO;
    g.adjust_speed(&mut v);
    assert_eq!(v, Vec3::new(0.0, 2.0, 0.0));
    let mut v = Vec3::new(30.0, 40.0, 0.0);
    g.adjust_speed(&mut v);
    assert!(close(v, Vec3::new(12.0, 16.0, 0.0), 1e-5));

    g.params.set_follow_ground(ground::AT_LEAST, 1.5);
    g.height = 0.5;
    let mut v = Vec3::new(5.0, 0.0, -3.0);
    g.adjust_height(&mut v);
    assert_eq!(v.z, 1.0, "climbs to its height");
    let mut v = Vec3::new(5.0, 0.0, 3.0);
    g.adjust_height(&mut v);
    assert_eq!(v.z, 3.0, "already climbing faster");
    g.params.set_follow_ground(ground::HOLD, 1.5);
    g.adjust_height(&mut v);
    assert_eq!(v.z, 1.0, "holds its height");
}

/// A missile flying north that wants to fly east pushes right and brakes, each no harder than its
/// greatest accelerations; within them it pushes exactly as hard as its reaction time asks.
///
/// Behaviour: missile.guidance.a-missile-turns-and-brakes-within-its-own-limits
#[test]
fn a_missile_turns_and_brakes_within_its_own_limits() {
    let mut g = Guidance::new();
    g.velocity = Vec3::new(0.0, 10.0, 0.0);
    g.desired_velocity = Vec3::new(10.0, 0.0, 0.0);
    g.calc_acceleration();
    assert!(
        close(g.acceleration, Vec3::new(1.0, -1.0, 0.0), 1e-6),
        "{:?}",
        g.acceleration
    );
    g.desired_velocity = Vec3::new(0.5, 10.0, 0.0);
    g.calc_acceleration();
    assert_eq!(g.acceleration, Vec3::new(0.5, 0.0, 0.0));
}

/// Fuel burns at the missile's acceleration times the time passed, down to none; out of fuel it
/// keeps the acceleration it had, and its guidance reports it once.
///
/// Behaviour: missile.guidance.fuel-burns-with-acceleration-and-an-empty-missile-coasts
#[test]
fn fuel_burns_with_acceleration_and_an_empty_missile_coasts() {
    let mut g = Guidance::new();
    g.params.set_total_fuel(10.0, true);
    g.fuel = 10.0;
    g.acceleration = Vec3::new(3.0, 4.0, 0.0);
    g.update_fuel_use(1.5);
    assert_eq!(g.fuel, 2.5);
    g.update_fuel_use(1.0);
    assert_eq!(g.fuel, 0.0);
    assert!(g.out_of_fuel());
    g.velocity = Vec3::new(0.0, 10.0, 0.0);
    g.desired_velocity = Vec3::new(10.0, 0.0, 0.0);
    g.calc_acceleration();
    assert_eq!(g.acceleration, Vec3::new(3.0, 4.0, 0.0));

    let mut host = Sky {
        position: Some(at(10.0, 10.0, 0.0)),
        placed: Some(at(10.0, 10.0, 0.0)),
        objects: vec![(7, at(10.0, 60.0, 0.0))],
        tracked: 7,
        now: 5.0,
        ..Sky::default()
    };
    g.targeting.set_target_object(7, 0.5, false, Vec3::ZERO);
    g.target.object = 7;
    g.moving = true;
    for _ in 0..2 {
        host.now += 1.0;
        assert_eq!(g.update(&mut host), fail::NONE);
    }
    assert_eq!(host.out_of_fuel, 1);
}

/// A step of flight moves the missile by its velocity and half its acceleration (with gravity, and
/// drag taken from its velocity before the wind is added) over the step, and sets its new
/// velocity; its lifespan running out is reported once. A missile not flying does not move.
///
/// Behaviour: missile.flight.a-missile-moves-under-gravity-drag-and-wind-and-expires-once
#[test]
fn a_missile_moves_under_gravity_drag_and_wind_and_expires_once() {
    let mut g = Guidance::new();
    g.moving = true;
    g.params.set_affected_by_gravity(true);
    let mut host = Sky {
        gravity: Vec3::new(0.0, 0.0, -9.8),
        ..Sky::default()
    };
    let mut f = Frame::new(Vec3::ZERO, Quat::IDENTITY);
    g.update_missile_physics(&mut host, Vec3::new(10.0, 0.0, 0.0), 0.5, &mut f);
    assert!(
        close(f.origin, Vec3::new(5.0, 0.0, -1.225), 1e-5),
        "{:?}",
        f.origin
    );
    assert!(close(g.velocity, Vec3::new(10.0, 0.0, -4.9), 1e-5));
    assert_eq!(host.velocity, Some(g.velocity));

    let mut g = Guidance::new();
    g.moving = true;
    g.params.set_drag(0.5);
    g.params.bits |= set::AFFECTED_WIND;
    g.params.wind = true;
    g.params.set_total_lifespan(10.0);
    g.expire_time = 10.0;
    host.wind = Vec3::new(0.0, 2.0, 0.0);
    host.now = 11.0;
    let mut f = Frame::new(Vec3::ZERO, Quat::IDENTITY);
    g.update_missile_physics(&mut host, Vec3::new(10.0, 0.0, 0.0), 0.5, &mut f);
    assert!(
        close(f.origin, Vec3::new(4.375, 1.0, 0.0), 1e-5),
        "{:?}",
        f.origin
    );
    assert!(close(g.velocity, Vec3::new(7.5, 2.0, 0.0), 1e-5));
    g.update_missile_physics(&mut host, g.velocity, 0.5, &mut f);
    assert_eq!(host.expired, 1);

    g.moving = false;
    let mut f = Frame::new(Vec3::new(1.0, 2.0, 3.0), Quat::IDENTITY);
    g.update_missile_physics(&mut host, Vec3::new(10.0, 0.0, 0.0), 0.5, &mut f);
    assert_eq!(f.origin, Vec3::ZERO);
}

/// A missile pulled by gravity is launched at its speed on the flattest course that comes down
/// on a still target 20 m away on level ground.
///
/// Behaviour: missile.flight.a-lobbed-missile-comes-down-on-its-target
#[test]
fn a_lobbed_missile_comes_down_on_its_target() {
    let g = Guidance::new();
    let host = Sky {
        gravity: Vec3::new(0.0, 0.0, -9.8),
        ..Sky::default()
    };
    let v = g.gravitational_launch(&host, 50.0, Vec3::new(0.0, 20.0, 0.0), Vec3::ZERO, false);
    assert!(length(v) <= 50.0 && length(v) > 49.8, "{v:?}");
    assert!(
        v.x.abs() < 1e-6 && v.z > 0.0 && v.z < 2.5,
        "a flat arc: {v:?}"
    );
    let t = 20.0 / v.y;
    let z = v.z * t - 4.9 * t * t;
    assert!(z.abs() < 1e-3, "lands at {z}");
}

/// A missile launches straight ahead at its starting speed (else its top speed); one that starts
/// toward its target does so only when the target is within 45 degrees of where it faces.
///
/// Behaviour: missile.launch.a-missile-launches-straight-ahead-or-toward-a-target-within-45-degrees
#[test]
fn a_missile_launches_straight_ahead_or_toward_a_target_within_45_degrees() {
    let from = at(10.0, 10.0, 0.0);
    let mut host = Sky::default();
    let mut g = aimed(at(110.0, 10.0, 0.0), Vec3::ZERO);
    let mut v = Vec3::ZERO;
    assert_eq!(
        g.calc_initial_velocity(&mut host, &from, &mut v),
        fail::NONE
    );
    assert!(close(v, Vec3::new(0.0, 50.0, 0.0), 1e-5), "{v:?}");
    g.targeting.set_initial_speed(20.0);
    g.calc_initial_velocity(&mut host, &from, &mut v);
    assert!(close(v, Vec3::new(0.0, 20.0, 0.0), 1e-5));
    g.targeting.set_start_toward_target();
    g.calc_initial_velocity(&mut host, &from, &mut v);
    assert!(
        close(v, Vec3::new(0.0, 20.0, 0.0), 1e-5),
        "east is 90 degrees off: {v:?}"
    );
    g.target.position = at(15.0, 110.0, 0.0);
    g.calc_initial_velocity(&mut host, &from, &mut v);
    assert!((length(v) - 20.0).abs() < 1e-3, "{v:?}");
    assert!((v.x / v.y - 0.05).abs() < 1e-4, "toward the target: {v:?}");
}

/// A missile is not launched at a target past its range, nor before its target has been heard of,
/// nor from a place outside the world.
///
/// Behaviour: missile.launch.a-missile-is-not-launched-past-its-range-or-at-an-unheard-target
#[test]
fn a_missile_is_not_launched_past_its_range_or_at_an_unheard_target() {
    let from = at(10.0, 10.0, 0.0);
    let mut host = Sky::default();
    let mut g = aimed(at(10.0, 110.0, 0.0), Vec3::ZERO);
    let mut v = Vec3::ZERO;
    g.targeting.set_max_range(150.0);
    assert_eq!(
        g.calc_initial_velocity(&mut host, &from, &mut v),
        fail::NONE
    );
    g.targeting.set_max_range(50.0);
    assert_eq!(
        g.calc_initial_velocity(&mut host, &from, &mut v),
        fail::NO_COURSE
    );
    g.target.valid = false;
    assert_eq!(
        g.calc_initial_velocity(&mut host, &from, &mut v),
        fail::TARGET_UNHEARD
    );
    let nowhere = Position::new(CellId(0), from.frame);
    assert_eq!(
        g.calc_initial_velocity(&mut host, &nowhere, &mut v),
        fail::NO_PLACE
    );
}

/// Asked which way it would launch, a missile works its launch out but answers north and level.
///
/// Behaviour: missile.launch.a-missile-asked-its-direction-answers-north-and-level
#[test]
fn a_missile_asked_its_direction_answers_north_and_level() {
    let mut host = Sky {
        position: Some(at(10.0, 10.0, 0.0)),
        ..Sky::default()
    };
    let mut g = aimed(at(110.0, 10.0, 0.0), Vec3::ZERO);
    g.targeting.set_start_toward_target();
    g.targeting.set_always_start_toward_target();
    let mut v = Vec3::ZERO;
    let from = host.position();
    g.calc_initial_velocity(&mut host, &from, &mut v);
    assert!(v.x > 49.0, "it would fly east: {v:?}");
    assert_eq!(
        g.inquire_direction(&mut host),
        (Heading::ZERO, 0.0, fail::NONE)
    );
}

/// A missile aimed at an object finds it and tracks it; placed and fired, it is put in the world
/// at its base place and launched (facing along its path, its streak and flight effect on), then
/// steers toward the target once a reaction interval has passed.
///
/// Behaviour: missile.launch.a-missile-is-placed-then-launched-once-its-target-is-known
#[test]
fn a_missile_is_placed_then_launched_once_its_target_is_known() {
    let base = at(10.0, 10.0, 1.0);
    let mut host = Sky {
        position: Some(base),
        objects: vec![(7, at(10.0, 60.0, 1.0))],
        now: 3.0,
        ..Sky::default()
    };
    let mut t = MissileTargetingParameters::default();
    t.set_target_object(7, 0.5, false, Vec3::ZERO);
    t.set_base_position(base);
    let mut g = Guidance::new();
    assert_eq!(
        g.initialize(&mut host, &MissileParameters::default(), &t),
        fail::NONE
    );
    assert_eq!(host.tracked, 7);
    assert!(g.target.valid && g.target_status == 1);

    g.place_and_fire();
    assert_eq!(g.update(&mut host), fail::NONE);
    assert_eq!(host.placed, Some(base));
    assert_eq!(host.velocity, Some(Vec3::new(0.0, 50.0, 0.0)));
    assert!(host.aligned && host.streak && g.moving);
    assert_eq!(host.fx, vec![FLIGHT_FX]);
    assert!(!g.ready_to_place && !g.ready_to_fire);

    host.objects = vec![(7, at(60.0, 10.0, 1.0))];
    host.now += 1.0;
    assert_eq!(g.update(&mut host), fail::NONE);
    assert!(close(g.desired_velocity, Vec3::new(50.0, 0.0, 0.0), 1e-3));
}

/// A missile aimed at a bare place takes the place its aim already held, not the place it is
/// given: a new missile so aimed has no target.
///
/// Behaviour: missile.launch.a-missile-aimed-at-a-place-keeps-the-place-it-already-had
#[test]
fn a_missile_aimed_at_a_place_keeps_the_place_it_already_had() {
    let mut host = Sky::default();
    let mut t = MissileTargetingParameters::default();
    t.set_target_position(at(50.0, 50.0, 0.0));
    let mut g = Guidance::new();
    assert_eq!(
        g.initialize(&mut host, &MissileParameters::default(), &t),
        fail::NO_TARGET
    );
    assert_eq!(g.target.position.cell, CellId(0));
    let mut g = aimed(at(20.0, 20.0, 0.0), Vec3::new(1.0, 0.0, 0.0));
    assert_eq!(
        g.initialize(&mut host, &MissileParameters::default(), &t),
        fail::NONE
    );
    assert_eq!(g.target.position, at(20.0, 20.0, 0.0));
    assert_eq!(g.target.offset, Vec3::ZERO);
}

fn report(records: &[(u64, bool, bool)]) -> CollisionReport {
    CollisionReport {
        records: records
            .iter()
            .map(|&(object, begin, ethereal)| ContactRecord {
                object,
                begin,
                ethereal,
            })
            .collect(),
        ..CollisionReport::default()
    }
}

/// A missile goes off on the first thing it begins touching that it does not pass through; of
/// what it passes through, on its target, or (going off on a miss) on any player or creature, but
/// never on its source; and on a collision normal with nothing struck.
///
/// Behaviour: missile.detonate.a-collision-sets-a-missile-off-on-what-it-strikes
#[test]
fn a_collision_sets_a_missile_off_on_what_it_strikes() {
    let host = Sky {
        objects: vec![
            (3, at(0.0, 0.0, 0.0)),
            (7, at(0.0, 0.0, 0.0)),
            (8, at(0.0, 0.0, 0.0)),
        ],
        creatures: vec![3, 8],
        ..Sky::default()
    };
    let mut g = Guidance::new();
    g.targeting.set_target_object(7, 0.5, false, Vec3::ZERO);
    g.targeting.use_source_for_base(3, 0.5, false, Vec3::ZERO);
    assert_eq!(
        g.should_detonate(&host, &report(&[(9, true, false)])),
        Some(None)
    );
    assert_eq!(
        g.should_detonate(&host, &report(&[(8, true, false)])),
        Some(Some(8))
    );
    assert_eq!(
        g.should_detonate(&host, &report(&[(8, false, false)])),
        None
    );
    assert_eq!(g.should_detonate(&host, &report(&[(8, true, true)])), None);
    assert_eq!(
        g.should_detonate(&host, &report(&[(8, true, true), (7, true, true)])),
        Some(Some(7))
    );
    g.params.set_detonates_on_miss(true);
    assert_eq!(g.should_detonate(&host, &report(&[(3, true, true)])), None);
    assert_eq!(
        g.should_detonate(&host, &report(&[(8, true, true)])),
        Some(Some(8))
    );
    assert_eq!(
        g.should_detonate(&host, &report(&[(7, true, true)])),
        None,
        "going off on a miss, its target is no more than any other object"
    );
    let mut hit = report(&[]);
    hit.normal_state = state::BEGIN;
    assert_eq!(g.should_detonate(&host, &hit), Some(None));
    hit.normal_state = state::END;
    assert_eq!(g.should_detonate(&host, &hit), None);
}

/// What a missile knows of another object says whether its game object is a player or a
/// creature, as the host noted it, for an object that moves and one that does not.
///
/// Behaviour: missile.detonate.an-object-noted-says-whether-it-is-a-player-or-a-creature
#[test]
fn an_object_noted_says_whether_it_is_a_player_or_a_creature() {
    use auberean_physics::body::missile::{ObjectTable, Objects};
    let body = crate::contact::objects::body(1.0, 1.0);
    let one = Vec3::new(1.0, 1.0, 1.0);
    let mut table = ObjectTable::default();
    table.note(1, &body, true);
    table.note(2, &body, false);
    table.note_still(3, &body, one, true);
    table.note_still(4, &body, one, false);
    let asked = |id| table.object(id).map(|o| o.player_or_creature);
    assert_eq!(asked(1), Some(true));
    assert_eq!(asked(2), Some(false));
    assert_eq!(asked(3), Some(true));
    assert_eq!(asked(4), Some(false));
}

/// An object that stands where it is put, with no collision spheres of its own, is as dynamic as
/// its state says: only a static one turns a sticky missile away.
///
/// Behaviour: missile.detonate.an-object-standing-still-is-dynamic-unless-static
#[test]
fn an_object_standing_still_is_dynamic_unless_static() {
    use auberean_physics::body::missile::{ObjectTable, Objects};
    let mut body = crate::contact::objects::body(1.0, 1.0);
    let one = Vec3::new(1.0, 1.0, 1.0);
    let mut table = ObjectTable::default();
    table.note_still(1, &body, one, false);
    assert_eq!(table.object(1).map(|o| o.dynamic), Some(true));
    body.set_state(body.state() | 0x1);
    assert!(body.is_static());
    table.note_still(2, &body, one, false);
    assert_eq!(table.object(2).map(|o| o.dynamic), Some(false));
}

/// A missile that goes off stops flying (its flight effect and streak stop) and fades out over
/// half a second; a sticky one instead sticks to a moving object it struck, and stays put on
/// anything else.
///
/// Behaviour: missile.detonate.a-missile-that-goes-off-fades-out-unless-it-sticks
#[test]
fn a_missile_that_goes_off_fades_out_unless_it_sticks() {
    let mut host = Sky {
        objects: vec![(8, at(0.0, 0.0, 0.0))],
        dynamic: vec![8],
        ..Sky::default()
    };
    let mut g = Guidance::new();
    g.moving = true;
    g.handle_collision(&mut host, &report(&[(8, true, false)]));
    assert_eq!(host.deleted, Some((0.5, 0.0)));
    assert!(!g.moving && !g.activated);
    assert_eq!(host.stopped_fx, vec![FLIGHT_FX]);

    let mut host = Sky {
        objects: vec![(8, at(0.0, 0.0, 0.0))],
        dynamic: vec![8],
        ..Sky::default()
    };
    let mut g = Guidance::new();
    g.params.set_sticky(true);
    let mut ground = report(&[]);
    ground.normal_state = state::BEGIN;
    g.handle_collision(&mut host, &ground);
    assert_eq!((host.deleted, host.stuck), (None, None));
    g.handle_collision(&mut host, &report(&[(8, true, false)]));
    assert_eq!((host.deleted, host.stuck), (None, Some(8)));
}

/// Conserving fuel marks the same bit as taking the surroundings' top speeds, so setting one sends
/// and copies both; nothing uses the bit between following the ground and keeping a distance.
///
/// Behaviour: missile.settings.conserving-fuel-and-the-surroundings-top-speeds-share-a-bit
#[test]
fn conserving_fuel_and_the_surroundings_top_speeds_share_a_bit() {
    let mut p = MissileParameters::default();
    p.set_conserve_fuel(true, 3.0, false);
    assert_eq!(p.bits, 0x1000);
    let mut w = Writer::new();
    p.pack(&mut w);
    let b = w.finish();
    assert_eq!(b.len(), 20);
    let word = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    assert_eq!((word(0), word(4), word(8), word(16)), (0x1000, 1, 0, 1));
    let back = MissileParameters::unpack(&mut Reader::new(&b), &Context::empty()).expect("reads");
    assert_eq!(back, p);

    let mut to = MissileParameters::default();
    to.use_env_max_speeds(false);
    to.bits = 0;
    p.copy_into(&mut to);
    assert!(to.env_max_speeds && to.conserve_fuel && to.min_catchup_speed == 3.0);
}

/// A behavior as stories carry it here: an id and a context.
#[derive(Clone, Debug, Default, PartialEq)]
struct Act(u32, u32);

impl Pack for Act {
    fn pack(&self, w: &mut Writer) {
        let bits = u32::from(self.0 != 0) | if self.1 != 0 { 0x40_0000 } else { 0 };
        w.u32(bits);
        if self.0 != 0 {
            w.u32(self.0);
        }
        if self.1 != 0 {
            w.u32(self.1);
        }
    }

    fn unpack(r: &mut Reader<'_>, _ctx: &Context) -> auberean_dat::reader::Result<Self> {
        let bits = r.u32()?;
        let id = if bits & 1 != 0 { r.u32()? } else { 0 };
        let ctx = if bits & 0x40_0000 != 0 { r.u32()? } else { 0 };
        Ok(Self(id, ctx))
    }
}

impl Behavior for Act {
    fn is_valid(&self) -> bool {
        self.0 != 0
    }
    fn set_context(&mut self, context: u32) {
        self.1 = context;
    }
}

/// An attacker's body and the world a story plays in, recording each call.
#[derive(Default)]
struct Stage {
    player: bool,
    player_id: u64,
    objects: Vec<u64>,
    behavior_fails: bool,
    effects: Option<FxAndBehaviorInfo<Act>>,
    missile: Option<MissileInfo>,
    next_object: u64,
    log: Vec<String>,
}

impl StoryHost<Act> for Stage {
    fn is_the_player(&self) -> bool {
        self.player
    }
    fn has_game_object(&self) -> bool {
        true
    }
    fn attack_started(&mut self, context: u32) {
        self.log.push(format!("started {context}"));
    }
    fn attack_ended(&mut self, context: u32) {
        self.log.push(format!("ended {context}"));
    }
    fn hit_effects(
        &mut self,
        _skill: u32,
        _attack_hook: u32,
        _result: u32,
        info: &mut FxAndBehaviorInfo<Act>,
    ) -> bool {
        match &self.effects {
            Some(e) => {
                *info = e.clone();
                true
            }
            None => false,
        }
    }
    fn missile_info(
        &mut self,
        _skill: u32,
        _attack_hook: u32,
        _attacker: u64,
        _target: u64,
        info: &mut MissileInfo,
    ) -> bool {
        match &self.missile {
            Some(m) => {
                *info = m.clone();
                true
            }
            None => false,
        }
    }
    fn is_skill_harmful(&mut self, _skill: u32) -> bool {
        true
    }
    fn do_behavior(&mut self, behavior: &Act) -> bool {
        self.log
            .push(format!("behavior {} {}", behavior.0, behavior.1));
        !self.behavior_fails
    }
    fn do_fx(&mut self, fx: u32, _intensity: f32) {
        self.log.push(format!("fx {fx}"));
    }
    fn object_exists(&self, object: u64) -> bool {
        self.objects.contains(&object)
    }
    fn object_behavior(&mut self, object: u64, behavior: &Act) {
        self.log.push(format!("behavior {object}: {}", behavior.0));
    }
    fn object_fx(&mut self, object: u64, fx: u32, _intensity: f32) {
        self.log.push(format!("fx {object}: {fx}"));
    }
    fn object_slider(&mut self, object: u64, slider: u32, value: f32, seconds: f32) {
        assert_eq!(slider, FADE_SLIDER);
        self.log
            .push(format!("fade {object} to {value} over {seconds}"));
    }
    fn object_has_game_object(&self, object: u64) -> bool {
        self.objects.contains(&object)
    }
    fn show_damage(
        &mut self,
        attacker: u64,
        target: u64,
        skill: u32,
        result: u32,
        target_health: i32,
        attacker_health: i32,
        _target_pk: i32,
        _attacker_pk: i32,
    ) {
        self.log.push(format!(
            "damage {attacker}>{target} skill {skill} result {result} {target_health}/{attacker_health}"
        ));
    }
    fn player_id(&self) -> u64 {
        self.player_id
    }
    fn notice_last_attacker(&mut self, target: u64, attacker: u64) {
        self.log
            .push(format!("last attacker of {target}: {attacker}"));
    }
    fn destroy_object(&mut self, object: u64) {
        self.log.push(format!("destroy {object}"));
    }
    fn object_data_id(&self, _object: u64) -> u32 {
        0
    }
    fn create_temporary(&mut self, data_id: u32, seconds: f64) -> Option<u64> {
        self.next_object += 1;
        let id = 1000 + self.next_object;
        self.objects.push(id);
        self.log
            .push(format!("make {id} from {data_id:#x} for {seconds}"));
        Some(id)
    }
    fn copy_look(&mut self, _from: u64, _to: u64) {}
    fn set_collision_state(&mut self, _object: u64, _c: &CollisionSettings) {}
    fn missile_initialize(
        &mut self,
        object: u64,
        _params: &MissileParameters,
        _targeting: &MissileTargetingParameters,
    ) {
        self.log.push(format!("guide {object}"));
    }
    fn hold(&mut self, object: u64, location: u32) {
        self.log.push(format!("hold {object} at {location}"));
    }
    fn missile_place_and_fire(&mut self, object: u64) {
        self.log.push(format!("fire {object}"));
    }
}

fn hook(kind: u32, anim_hook: u32, target: u64) -> StoryHookData {
    StoryHookData {
        kind,
        anim_hook,
        target,
        ..StoryHookData::default()
    }
}

fn story(hooks: &[StoryHookData]) -> PhysicsStory<Act> {
    let mut s = PhysicsStory::<Act> {
        attacker: 100,
        attacker_behavior: Act(5, 0),
        client_context: 42,
        skill: 9,
        ..PhysicsStory::default()
    };
    for h in hooks {
        s.add_hook(*h);
    }
    s
}

fn damages(stage: &Stage) -> Vec<&str> {
    stage
        .log
        .iter()
        .filter(|l| l.starts_with("damage"))
        .map(String::as_str)
        .collect()
}

/// A story plays the attacker's behavior under a fresh context and runs the hooks waiting for no
/// animation hook at once; each story animation hook of the attack then lets the next hooks run,
/// in the order of the animation hooks they wait for (and, waiting for the same, as they were
/// added). The player's attack begins with its client context.
///
/// Behaviour: missile.story.hooks-run-in-step-with-the-attack-animation
#[test]
fn hooks_run_in_step_with_the_attack_animation() {
    let mut stage = Stage {
        player: true,
        objects: vec![201, 202, 203, 204],
        ..Stage::default()
    };
    let s = story(&[
        hook(kind::DAMAGE, 2, 204),
        hook(kind::DAMAGE, 1, 202),
        hook(kind::DAMAGE, 0, 201),
        hook(kind::DAMAGE, 1, 203),
    ]);
    let mut p = StoryPlayer::new();
    p.do_story(&mut stage, &s);
    assert_eq!(&stage.log[..2], ["started 42", "behavior 5 2"]);
    assert_eq!(damages(&stage), ["damage 100>201 skill 9 result 0 0/0"]);
    p.handle_anim_hook(&mut stage, STORY_HOOK + 1);
    assert_eq!(damages(&stage).len(), 1, "another kind of animation hook");
    p.handle_anim_hook(&mut stage, STORY_HOOK);
    assert_eq!(
        damages(&stage)[1..],
        [
            "damage 100>202 skill 9 result 0 0/0",
            "damage 100>203 skill 9 result 0 0/0"
        ]
    );
    p.handle_anim_hook(&mut stage, STORY_HOOK);
    assert_eq!(damages(&stage).len(), 4);
    assert_eq!(p.context, 2);
}

/// The attacker's behavior ending runs every hook left and empties the story (the player's attack
/// ends); its failing empties it without running them. A story with no attacker behavior runs all
/// its hooks at once. A new story finishes the one playing first.
///
/// Behaviour: missile.story.an-ended-behavior-runs-the-remaining-hooks-and-a-failed-one-drops-them
#[test]
fn an_ended_behavior_runs_the_remaining_hooks_and_a_failed_one_drops_them() {
    let mut stage = Stage {
        player: true,
        objects: vec![201, 202, 203],
        ..Stage::default()
    };
    let hooks = [
        hook(kind::DAMAGE, 0, 201),
        hook(kind::DAMAGE, 1, 202),
        hook(kind::DAMAGE, 3, 203),
    ];
    let mut p = StoryPlayer::new();
    p.do_story(&mut stage, &story(&hooks));
    p.handle_behavior_done(&mut stage, p.context + 1, 0);
    assert_eq!(damages(&stage).len(), 1, "another behavior's end");
    p.handle_behavior_done(&mut stage, p.context, 0);
    assert_eq!(damages(&stage).len(), 3);
    assert!(p.story.as_ref().is_some_and(PhysicsStory::is_null));
    assert_eq!(stage.log.last().map(String::as_str), Some("ended 42"));

    let mut stage = Stage {
        objects: vec![201, 202, 203],
        ..Stage::default()
    };
    p.do_story(&mut stage, &story(&hooks));
    p.handle_behavior_done(&mut stage, p.context, 1);
    assert_eq!(damages(&stage).len(), 1);

    let mut stage = Stage {
        objects: vec![201, 202, 203],
        ..Stage::default()
    };
    let mut bare = story(&hooks);
    bare.attacker_behavior = Act::default();
    p.do_story(&mut stage, &bare);
    assert_eq!(damages(&stage).len(), 3);

    let mut stage = Stage {
        objects: vec![201, 202, 203],
        ..Stage::default()
    };
    p.do_story(&mut stage, &story(&hooks));
    p.do_story(&mut stage, &story(&hooks[..1]));
    assert_eq!(damages(&stage).len(), 4);
}

/// A hit plays the rules' effect on the attacker and their behaviors and effects on the target and
/// the weapon, shows the damage on the target (with no skill for a basic attack), and tells a
/// target that is the player who attacked it when the skill harms.
///
/// Behaviour: missile.story.a-hit-shows-its-effects-and-damage
#[test]
fn a_hit_shows_its_effects_and_damage() {
    let effects = FxAndBehaviorInfo {
        attacker_fx: 11,
        target_fx: 12,
        weapon_fx: 13,
        target_behavior: Act(21, 0),
        weapon_behavior: Act(22, 0),
    };
    let mut stage = Stage {
        objects: vec![200, 300],
        player_id: 200,
        effects: Some(effects),
        ..Stage::default()
    };
    let mut h = hook(kind::DAMAGE, 0, 200);
    h.weapon = 300;
    h.attack_result = 3;
    h.target_health_change = -40;
    h.attacker_health_change = 5;
    let mut p = StoryPlayer::new();
    p.do_story(&mut stage, &story(&[h]));
    assert_eq!(
        stage.log[1..],
        [
            "fx 11",
            "behavior 200: 21",
            "fx 200: 12",
            "behavior 300: 22",
            "fx 300: 13",
            "damage 100>200 skill 9 result 3 -40/5",
            "last attacker of 200: 100",
        ]
    );
    let mut basic = story(&[h]);
    basic.basic_attack = true;
    stage.log.clear();
    p.do_story(&mut stage, &basic);
    assert_eq!(damages(&stage), ["damage 100>200 skill 0 result 3 -40/5"]);
}

/// A make-missiles hook makes the missile of every fire hook that is held before firing, puts each
/// in the attacker's hand and fades a thrown weapon out; each fire hook then fires the first made.
/// One not held is made at the moment of firing. When the story ends early the unfired missiles are
/// removed and the weapon fades back in over half a second.
///
/// Behaviour: missile.story.missiles-are-made-in-hand-fired-in-order-and-the-unfired-removed
#[test]
fn missiles_are_made_in_hand_fired_in_order_and_the_unfired_removed() {
    let held = MissileInfo {
        entity: 0x77,
        holding_location: 4,
        thrown: true,
        ..MissileInfo::default()
    };
    let mut stage = Stage {
        objects: vec![200, 300],
        missile: Some(held),
        ..Stage::default()
    };
    let mut fire = hook(kind::FIRE_MISSILE, 1, 200);
    fire.weapon = 300;
    let mut fire2 = fire;
    fire2.anim_hook = 2;
    let s = story(&[hook(kind::CREATE_MISSILES, 0, 200), fire, fire2]);
    let mut p = StoryPlayer::new();
    p.do_story(&mut stage, &s);
    assert_eq!(p.pending_missiles, [1001, 1002]);
    assert!(stage.log.contains(&"make 1001 from 0x77 for 8".to_owned()));
    assert!(stage.log.contains(&"hold 1002 at 4".to_owned()));
    assert!(stage.log.contains(&"fade 300 to 0 over 0".to_owned()));
    p.handle_anim_hook(&mut stage, STORY_HOOK);
    assert_eq!(stage.log.last().map(String::as_str), Some("fire 1001"));
    p.handle_behavior_done(&mut stage, p.context, 1);
    let tail: Vec<_> = stage.log.iter().rev().take(3).rev().cloned().collect();
    assert_eq!(
        tail,
        [
            "destroy 1002",
            "fade 300 to 1 over 0.5",
            "fade 300 to 1 over 0.5"
        ]
    );

    let mut stage = Stage {
        objects: vec![200],
        missile: Some(MissileInfo {
            entity: 0x78,
            ..MissileInfo::default()
        }),
        ..Stage::default()
    };
    p.do_story(&mut stage, &story(&[hook(kind::FIRE_MISSILE, 0, 200)]));
    assert_eq!(
        stage.log[1..],
        ["make 1001 from 0x78 for 8", "guide 1001", "fire 1001"]
    );
}

/// A story's binary form is the one observers are sent: the protocol reads back its attacker, its
/// behavior, its context, its skill and its hooks (kind and fields), in hook order.
///
/// Behaviour: missile.story.a-story-packs-as-the-protocol-reads-it
#[test]
fn a_story_packs_as_the_protocol_reads_it() {
    use auberean_protocol::types::PhysicsStory as Wire;
    use auberean_protocol::wire::{Field, Reader as WireReader};

    let mut late = hook(kind::DAMAGE, 3, 200);
    late.weapon = 300;
    late.attack_result = 2;
    late.target_health_change = -7;
    late.attacker_pk_damage = 1;
    let early = hook(kind::FIRE_MISSILE, 1, 201);
    let mut s = story(&[late, early]);
    s.basic_attack = true;
    let mut w = Writer::new();
    s.pack(&mut w);
    let b = w.finish();

    let mut r = WireReader::new(&b);
    let wire = Wire::read(&mut r).expect("the protocol reads it");
    r.finish().expect("all of it");
    assert_eq!(
        (wire.attacker, wire.attack_context, wire.skill),
        (100, 42, 9)
    );
    assert_eq!(wire.attacker_behavior.behavior, 5);
    assert_eq!(wire.bits & 0x20, 0x20, "a basic attack");
    let hooks: Vec<_> = wire
        .hooks
        .iter()
        .map(|h| (h.bits & 0xF, h.target))
        .collect();
    assert_eq!(hooks, [(kind::FIRE_MISSILE, 201), (kind::DAMAGE, 200)]);
    assert_eq!(wire.hooks[1].weapon, 300);
    assert_eq!(wire.hooks[1].numbers, [2, 3, 0, -7, 0, 0, 1]);

    let back = PhysicsStory::<Act>::unpack(&mut Reader::new(&b), &Context::empty()).expect("reads");
    assert_eq!(back, s);
}
