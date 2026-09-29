//! ACE: Source/ACE.Server/Physics/PhysicsObj.cs::SetMotionTableID
//! Shared animation motion stack on server physics bodies: SetMotionTableID, update_object,
//! PhysicsObj motion calls, MoveTo at run rate.
//! Fixture: Synthetic motion tables and server physics; real-content checks use retail dats.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use dereth_animation::motion::{flags, HoldKey, MovementParameters, RawMotionState};
use dereth_animation::MotionCommand;
use dereth_assets::geometry::AnimFrame;
use dereth_assets::motion::{AnimData, MotionData};
use dereth_assets::{Animation, MotionTable};
use dereth_physics::geom::Sphere;
use dereth_physics::{PhysHandle, SetupGeometry};
use dereth_primitives::{DataId, LandblockId, Position as PPosition, Vec3};
use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::not_ported::take_local;
use empyrean_dat::physics::flat_land_source;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    MotionCommand as AceMotion, MotionStance, PhysicsState, PropertyDataId, WeenieError,
};
use empyrean_entity::{ObjectGuid, Position};
use empyrean_world::physics::{motion, motion_table as mt, phys_ext};
use empyrean_world::world_objects::kinds::KindData;
use empyrean_world::world_objects::monster_navigation as nav;
use empyrean_world::world_objects::world_object::{self, WorldObject};
use empyrean_world::World;

// ---------------------------------------------------------------------------------- fixtures

const SETUP: u32 = 0x0200_1000;
const HOME: u16 = 0xA9B4;
const MT: u32 = 0x0900_0200;

const NONCOMBAT: u32 = 0x8000_003D;
const READY: u32 = 0x4100_0003;
const WALK: u32 = 0x4500_0005;
const RUN: u32 = 0x4400_0007;
const TURN_RIGHT: u32 = 0x6500_000D;
/// `AttackHigh1`, an action-class command.
const ACTION: u32 = 0x1000_0062;

/// Every cycle's animation: 10 frames, no root motion (the cycles move by their velocity).
const CYCLE_ANIM: u32 = 0x0300_0100;
/// The action: 15 frames at 30/s from Ready, 0.5 s.
const ACTION_ANIM: u32 = 0x0300_0101;

const QUANTUM: f64 = 1.0 / 30.0;
const WALK_VELOCITY: f32 = 3.12;
const RUN_VELOCITY: f32 = 4.0;
const TURN_OMEGA: f32 = -std::f32::consts::FRAC_PI_2;

fn snapshot(t: f64) -> ClockSnapshot {
    ClockSnapshot {
        portal_year_ticks: t,
        unix_time: 0.0,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    }
}

fn anim(anim_id: u32) -> AnimData {
    AnimData {
        anim_id: DataId(anim_id),
        low_frame: 0,
        high_frame: -1,
        framerate: 30.0,
    }
}

fn data(key: u32, anims: Vec<AnimData>, velocity: Option<Vec3>, omega: Option<Vec3>) -> MotionData {
    MotionData {
        key,
        bitfield: 0,
        flags: 0,
        anims,
        velocity,
        omega,
    }
}

fn animation(id: u32, n: u32) -> Animation {
    let part_frames = (0..n)
        .map(|_| AnimFrame {
            frames: Vec::new(),
            hooks: Vec::new(),
        })
        .collect();
    Animation {
        id: DataId(id),
        flags: 0,
        num_parts: 0,
        num_frames: n,
        has_hooks: false,
        pos_frames: None,
        part_frames,
    }
}

/// `(style << 16) | (motion & 0xFFFFFF)`, the cycle and link key.
fn key(motion: u32) -> u32 {
    NONCOMBAT.wrapping_shl(16) | (motion & 0xFF_FFFF)
}

fn motion_table() -> MotionTable {
    let cycles = vec![
        data(key(READY), vec![anim(CYCLE_ANIM)], None, None),
        data(
            key(WALK),
            vec![anim(CYCLE_ANIM)],
            Some(Vec3::new(0.0, WALK_VELOCITY, 0.0)),
            None,
        ),
        data(
            key(RUN),
            vec![anim(CYCLE_ANIM)],
            Some(Vec3::new(0.0, RUN_VELOCITY, 0.0)),
            None,
        ),
        // bitfield 2: the turn cycle is only entered from the style default (Ready); from a walk
        // the turn is the modifier below, as in the retail tables
        MotionData {
            bitfield: 2,
            ..data(
                key(TURN_RIGHT),
                vec![anim(CYCLE_ANIM)],
                None,
                Some(Vec3::new(0.0, 0.0, TURN_OMEGA)),
            )
        },
    ];
    // turning while walking is a modifier on the walk (omega only), as in the retail tables
    let modifiers = vec![data(
        key(TURN_RIGHT),
        Vec::new(),
        None,
        Some(Vec3::new(0.0, 0.0, TURN_OMEGA)),
    )];
    let mut links = BTreeMap::new();
    links.insert(
        key(READY),
        vec![data(ACTION, vec![anim(ACTION_ANIM)], None, None)],
    );
    MotionTable {
        id: DataId(MT),
        default_style: NONCOMBAT,
        style_defaults: BTreeMap::from([(NONCOMBAT, READY)]),
        cycles,
        modifiers,
        links,
    }
}

/// A world on `FakeDats` with the motion table, physics over flat landblocks at 20 m and one
/// synthetic setup (a 0.5 m sphere standing on its base).
fn world() -> World {
    world_on(empyrean_testkit::dats::with_stat_tables(FakeDats::new()))
}

/// [`world`] over `dats` (plus the motion table and its animations).
fn world_on(dats: FakeDats) -> World {
    let dats = dats
        .with_portal(MT, motion_table())
        .with_portal(CYCLE_ANIM, animation(CYCLE_ANIM, 10))
        .with_portal(ACTION_ANIM, animation(ACTION_ANIM, 15))
        .build()
        .expect("fake dats");
    let mut w = World::new(snapshot(0.0), dats);
    let blocks = [HOME, 0xAAB4, 0xA9B5];
    let ids: Vec<LandblockId> = blocks.iter().map(|b| LandblockId(*b)).collect();
    phys_ext::use_land_source(&mut w, Arc::new(flat_land_source(&ids, 10)));
    for &b in &blocks {
        phys_ext::load_landblock(&mut w, b);
        let adj: Vec<u16> = blocks.iter().copied().filter(|&o| o != b).collect();
        phys_ext::set_adjacents(&mut w, b, adj);
    }
    phys_ext::register_setup(
        &mut w,
        SETUP,
        SetupGeometry {
            spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)],
            sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, 0.5), 1.0),
            step_up_height: 0.3,
            step_down_height: 0.3,
            radius: 0.5,
            height: 1.0,
            ..SetupGeometry::default()
        },
    );
    w
}

fn at(x: f32, y: f32) -> Position {
    Position::from_components(
        u32::from(HOME) << 16 | 1,
        x,
        y,
        20.0,
        0.0,
        0.0,
        0.0,
        1.0,
        false,
    )
}

/// An object of `kind` (`depth`: 0 plain, 2 creature, 3 player) with the synthetic setup and
/// `table`, entered into the world at `pos`.
fn spawn(
    w: &mut World,
    guid: u32,
    kind: KindData,
    depth: u8,
    table: u32,
    pos: Position,
) -> (ObjectGuid, PhysHandle) {
    let g = ObjectGuid::new(guid);
    let mut o = WorldObject {
        guid: g,
        container: (depth >= 1).then(Box::default),
        creature: (depth >= 2).then(Box::default),
        player: (depth >= 3).then(Box::default),
        kind,
        ..Default::default()
    };
    o.biota.properties_enchantment_registry = Some(Vec::new());
    o.set_property(PropertyDataId::Setup, SETUP);
    if table != 0 {
        o.set_property(PropertyDataId::MotionTable, table);
    }
    o.set_location(Some(pos));
    o.current_landblock = Some(empyrean_entity::LandblockId::new(pos.cell()));
    if depth >= 2 {
        // as the Creature constructor leaves it: vitals (`UpdateObjectPhysics` reads `IsDead`)
        // and the cached `IsMonster` (`SetMonsterState`)
        with_vitals(&mut o);
        empyrean_world::world_objects::monster::set_monster_state(&mut o);
    }
    w.objects.insert(o).expect("fresh guid");
    assert!(
        phys_ext::add_world_object_physics(w, g),
        "{g:?} entered the world"
    );
    (g, phys_ext::physics_obj(w, g).expect("a body"))
}

/// The six attributes at 100 (a player's run rate reads Strength for its burden, V332), and
/// health, stamina and mana at 100 of 100.
fn with_vitals(o: &mut WorldObject) {
    use empyrean_entity::enums::properties::PropertyAttribute;
    use empyrean_entity::enums::PropertyAttribute2nd as V;
    use empyrean_entity::models::properties_attribute::PropertiesAttribute;
    use empyrean_entity::models::properties_attribute_2nd::PropertiesAttribute2nd;
    let mut attributes = empyrean_common::dotnet::DotNetDict::new();
    for a in 1..=6u16 {
        attributes.insert(
            PropertyAttribute(a),
            PropertiesAttribute {
                init_level: 100,
                ..Default::default()
            },
        );
    }
    o.biota.properties_attribute = Some(attributes);
    for a in 1..=6u16 {
        let ca = empyrean_world::world_objects::entity::creature_attribute::CreatureAttribute::new(
            o,
            PropertyAttribute(a),
        );
        o.attributes_mut().insert(PropertyAttribute(a), ca);
    }
    let vitals = [V::MaxHealth, V::MaxStamina, V::MaxMana];
    let mut v2 = empyrean_common::dotnet::DotNetDict::new();
    for v in vitals {
        v2.insert(
            v,
            PropertiesAttribute2nd {
                init_level: 100,
                level_from_cp: 0,
                cp_spent: 0,
                current_level: 100,
            },
        );
    }
    o.biota.properties_attribute_2nd = Some(v2);
    for v in vitals {
        let cv = empyrean_world::world_objects::entity::creature_vital::CreatureVital::new(o, v);
        o.vitals_mut().insert(v, cv);
    }
}

fn creature(w: &mut World, guid: u32, x: f32, y: f32) -> (ObjectGuid, PhysHandle) {
    spawn(w, guid, KindData::Creature, 2, MT, at(x, y))
}

/// A player past login: out of the pink bubble (`Hidden` cleared, as ACE's login completion
/// does), so its body animates.
fn player(w: &mut World, x: f32, y: f32) -> PhysHandle {
    let (g, h) = spawn(w, 0x5000_0001, KindData::Player, 3, MT, at(x, y));
    phys_ext::set_physics_state(w, g, PhysicsState::Hidden, Some(false));
    h
}

/// One physics quantum of world time, then `update_object` for each body.
fn step(w: &mut World, bodies: &[PhysHandle]) {
    w.now.portal_year_ticks += QUANTUM;
    for &h in bodies {
        phys_ext::update_object(w, h);
    }
}

/// Whole quanta in `seconds`.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a few seconds of quanta
fn quanta(seconds: f64) -> u32 {
    (seconds / QUANTUM).round() as u32
}

fn run(w: &mut World, bodies: &[PhysHandle], seconds: f64) {
    for _ in 0..quanta(seconds) {
        step(w, bodies);
    }
}

/// Steps until `done`, for at most `seconds`; the time it took, or `None`.
fn run_until(
    w: &mut World,
    bodies: &[PhysHandle],
    seconds: f64,
    mut done: impl FnMut(&World) -> bool,
) -> Option<f64> {
    let start = w.now.portal_year_ticks;
    for _ in 0..quanta(seconds) {
        step(w, bodies);
        if done(w) {
            return Some(w.now.portal_year_ticks - start);
        }
    }
    None
}

fn pos(w: &World, h: PhysHandle) -> PPosition {
    phys_ext::position(w, h).expect("a placed body")
}

fn dist(a: &PPosition, b: &PPosition) -> f32 {
    let (dx, dy) = (
        b.frame.origin.x - a.frame.origin.x,
        b.frame.origin.y - a.frame.origin.y,
    );
    (dx * dx + dy * dy).sqrt()
}

fn heading(w: &World, h: PhysHandle) -> f32 {
    dereth_animation::frame::get_heading(&pos(w, h).frame)
}

fn moved_to(w: &World, h: PhysHandle, x: f32, y: f32) -> PPosition {
    let mut p = pos(w, h);
    p.frame.origin = Vec3::new(x, y, p.frame.origin.z);
    p
}

/// A creature entered and settled: on the ground, idle in its default state.
fn settled(w: &mut World, bodies: &[PhysHandle]) {
    run(w, bodies, 0.5);
    let _ = take_local();
}

// ---------------------------------------------------------------------------------- attachment

#[test]
fn set_motion_table_id_attaches_a_driver_in_its_default_state() {
    let mut w = world();
    let (_, h) = creature(&mut w, 0x7000_0001, 100.0, 100.0);
    let (_, plain) = spawn(
        &mut w,
        0x7000_0002,
        KindData::WorldObject,
        0,
        0,
        at(104.0, 100.0),
    );

    assert!(
        motion::has_movement_manager(&w, h),
        "a creature with a table has a movement manager"
    );
    assert!(!motion::has_movement_manager(&w, plain), "table 0: none");
    let state = phys_ext::interpreted_state(&w, h).expect("an interpreter");
    assert_eq!(state.current_style, MotionCommand(NONCOMBAT));
    assert_eq!(state.forward_command, MotionCommand(READY));

    settled(&mut w, &[h]);
    assert!(
        !phys_ext::is_animating(&w, h),
        "the default state's pending motion completed"
    );
    assert!(
        !phys_ext::is_moving_or_animating(&w, h),
        "idle on its cycle"
    );
    assert!(!world_object::is_animating(
        &w,
        ObjectGuid::new(0x7000_0001)
    ));

    // DoMotion on a body without a table is ACE's NoAnimationTable; get_minterp makes one.
    let mvp = phys_ext::ace_movement_parameters();
    assert_eq!(
        phys_ext::do_motion(&mut w, plain, WALK, &mvp),
        WeenieError::NoAnimationTable
    );
    assert!(phys_ext::get_minterp(&mut w, plain, |m| m.initted).expect("made lazily"));
    assert!(motion::has_movement_manager(&w, plain));
}

#[test]
fn a_motion_that_completes_at_once_is_done_inside_the_call() {
    let mut w = world();
    let (_, h) = creature(&mut w, 0x7000_0001, 100.0, 100.0);
    settled(&mut w, &[h]);
    // Ready while in Ready queues nothing to play: every movement call ends with the
    // completed-motion check, so it is not pending when the call returns
    assert_eq!(
        phys_ext::do_motion(&mut w, h, READY, &MovementParameters::default()),
        WeenieError::None
    );
    assert!(!phys_ext::is_animating(&w, h));
}

#[test]
fn a_new_motion_table_wakes_an_idle_body() {
    let mut w = world();
    let (_, h) = creature(&mut w, 0x7000_0001, 100.0, 100.0);
    settled(&mut w, &[h]);
    assert!(phys_ext::set_active(&mut w, h, false));
    assert!(!phys_ext::is_active(&w, h));
    // SetMotionTableID -> MakeMovementManager(true): a non-static body goes active
    assert!(phys_ext::set_motion_table_id(&mut w, h, MT));
    assert!(phys_ext::is_active(&w, h));
    assert!(motion::has_movement_manager(&w, h));
    assert!(phys_ext::set_motion_table_id(&mut w, h, 0));
    assert!(
        !motion::has_movement_manager(&w, h),
        "table 0 drops the manager"
    );
}

#[test]
fn a_dropped_creature_lands_standing_and_leaving_the_ground_keeps_its_walk() {
    let mut w = world();
    let (_, h) = spawn(
        &mut w,
        0x7000_0001,
        KindData::Creature,
        2,
        MT,
        Position::from_components(
            u32::from(HOME) << 16 | 1,
            100.0,
            100.0,
            23.0,
            0.0,
            0.0,
            0.0,
            1.0,
            false,
        ),
    );
    run(&mut w, &[h], 2.0);
    assert!(
        (pos(&w, h).frame.origin.z - 20.0).abs() < 0.01,
        "landed: {:?}",
        pos(&w, h).frame.origin
    );
    let state = phys_ext::interpreted_state(&w, h).expect("interp");
    assert_eq!(
        state.forward_command,
        MotionCommand(READY),
        "HitGround re-applied the standing state"
    );
    assert!(!phys_ext::is_animating(&w, h));

    // walking north, it leaves the ground: LeaveGround's velocity is the walk, in world space,
    // capped at run rate x 4 (InqRunRate stood in at ACE's answer for Run skill 0: 1.0)
    phys_ext::set_run_rate_stand_in(&mut w, h, Some(1.0));
    phys_ext::do_motion(&mut w, h, WALK, &MovementParameters::default());
    run(&mut w, &[h], 0.2);
    phys_ext::set_on_walkable(&mut w, h, false);
    let v = phys_ext::velocity(&w, h);
    assert!(
        (v.y - WALK_VELOCITY).abs() < 1e-3 && v.x.abs() < 1e-3,
        "leave-ground velocity {v:?}"
    );
}

#[test]
fn ace_movement_parameters_are_aces_constructor() {
    let p = phys_ext::ace_movement_parameters();
    assert_eq!(p.flags, 0x1_EE1F, "the client's 0x1EE0F plus CanCharge");
    assert_eq!(p.walk_run_threshold, 1.0);
    assert_eq!(p.distance_to_object, 0.6);
    assert_eq!(p.fail_distance, f32::MAX);
    assert_eq!(p.speed, 1.0);
    assert_eq!(p.hold_key_to_apply, HoldKey::Invalid);
}

// ---------------------------------------------------------------------------------- MoveTo

#[test]
fn a_creature_walks_to_a_position_arrives_and_the_completion_notice_fires() {
    let mut w = world();
    let (g, h) = creature(&mut w, 0x7000_0001, 100.0, 100.0);
    settled(&mut w, &[h]);

    // 10 m due east: a turn to 90 degrees, then a walk (within the client's 15 m threshold)
    let target = moved_to(&w, h, 110.0, 100.0);
    let mvp = MovementParameters::default();
    phys_ext::move_to_position(&mut w, h, &target, &mvp);
    assert!(phys_ext::is_moving_to(&w, h));
    // Creature.OnMoveComplete (Monster_Navigation.cs) clears IsMoving on a success: raised here, it
    // shows each call of the real completion
    nav::fields_mut(&mut w, g).is_moving = true;
    assert_eq!(
        phys_ext::last_move_complete(&w, h),
        None,
        "nothing completes on the call"
    );
    assert!(
        nav::fields(&w, g).is_moving,
        "OnMoveComplete not called yet"
    );

    let took =
        run_until(&mut w, &[h], 8.0, |w| !phys_ext::is_moving_to(w, h)).expect("the MoveTo ends");
    let end = pos(&w, h);
    // cylinder distance with the body's 0.5 m radius: within 0.6 of the target's centre
    assert!(
        dist(&end, &target) <= 0.6 + 0.5 + 0.01,
        "arrived: {:?} vs {:?}",
        end.frame.origin,
        target.frame.origin
    );
    assert!(
        (heading(&w, h) - 90.0).abs() < 1.0,
        "facing east: {}",
        heading(&w, h)
    );
    // 90 degrees at pi/2 rad/s is 1 s; ~8.9 m at 3.12 m/s is ~2.9 s
    assert!(took > 3.0 && took < 5.0, "took {took}");

    assert_eq!(phys_ext::last_move_complete(&w, h), Some(WeenieError::None));
    assert!(
        !nav::fields(&w, g).is_moving,
        "OnMoveComplete(None) reached the creature"
    );
    let state = phys_ext::interpreted_state(&w, h).expect("interp");
    assert_eq!(state.forward_command, MotionCommand(READY), "stopped");

    // standing still afterwards, and the completion does not fire again
    nav::fields_mut(&mut w, g).is_moving = true;
    let rest = pos(&w, h);
    run(&mut w, &[h], 1.0);
    assert!(dist(&rest, &pos(&w, h)) < 1e-3);
    assert!(
        nav::fields(&w, g).is_moving,
        "OnMoveComplete fired exactly once"
    );
}

#[test]
fn a_new_move_to_cancels_the_old_one_with_action_cancelled() {
    let mut w = world();
    let (_, h) = creature(&mut w, 0x7000_0001, 100.0, 100.0);
    settled(&mut w, &[h]);

    let mvp = MovementParameters::default();
    let north = moved_to(&w, h, 100.0, 110.0);
    phys_ext::move_to_position(&mut w, h, &north, &mvp);
    run(&mut w, &[h], 0.5);
    let south = moved_to(&w, h, 100.0, 90.0);
    phys_ext::move_to_position(&mut w, h, &south, &mvp);
    assert_eq!(
        phys_ext::last_move_complete(&w, h),
        Some(WeenieError::ActionCancelled)
    );
    assert!(phys_ext::is_moving_to(&w, h), "the new one runs");

    phys_ext::cancel_moveto(&mut w, h);
    assert!(!phys_ext::is_moving_to(&w, h));
    assert_eq!(
        phys_ext::last_move_complete(&w, h),
        Some(WeenieError::ActionCancelled)
    );
}

#[test]
fn a_replacing_turn_that_is_already_done_reports_the_cancel_then_its_own_success() {
    let mut w = world();
    let (_, h) = creature(&mut w, 0x7000_0001, 100.0, 100.0);
    settled(&mut w, &[h]);

    let north = moved_to(&w, h, 100.0, 110.0);
    phys_ext::move_to_position(&mut w, h, &north, &MovementParameters::default());
    run(&mut w, &[h], 0.5);
    // a turn to the heading it already has ends inside the call
    let mvp = MovementParameters {
        desired_heading: heading(&w, h),
        ..MovementParameters::default()
    };
    phys_ext::turn_to_heading(&mut w, h, &mvp);
    assert!(!phys_ext::is_moving_to(&w, h));
    assert_eq!(phys_ext::last_move_complete(&w, h), Some(WeenieError::None));
}

#[test]
fn move_to_object_follows_a_moving_target() {
    let mut w = world();
    let (g, h) = creature(&mut w, 0x7000_0001, 100.0, 100.0);
    let (_, t) = spawn(
        &mut w,
        0x7000_0002,
        KindData::WorldObject,
        0,
        0,
        at(100.0, 112.0),
    );
    settled(&mut w, &[h, t]);

    let mvp = MovementParameters::default();
    phys_ext::move_to_object(&mut w, h, t, &mvp);
    assert!(phys_ext::is_moving_to(&w, h));
    assert!(
        phys_ext::move_to_initialized(&w, h),
        "the first target update arrived with the call"
    );
    nav::fields_mut(&mut w, g).is_moving = true;

    // after a second the target moves 6 m east
    run(&mut w, &[h, t], 1.0);
    let moved = moved_to(&w, t, 106.0, 112.0);
    assert!(phys_ext::set_position(&mut w, t, &moved));

    run_until(&mut w, &[h, t], 10.0, |w| !phys_ext::is_moving_to(w, h)).expect("arrives");
    let (end, target) = (pos(&w, h), pos(&w, t));
    // arrival is at distance_to_object between the cylinders (both 0.5 m)
    assert!(
        dist(&end, &target) <= 0.6 + 1.0 + 0.05,
        "near the moved target: {:?} vs {:?}",
        end.frame.origin,
        target.frame.origin
    );
    assert!(
        end.frame.origin.x > 102.0,
        "it followed the target east: {:?}",
        end.frame.origin
    );
    assert_eq!(phys_ext::last_move_complete(&w, h), Some(WeenieError::None));
    assert!(
        !nav::fields(&w, g).is_moving,
        "Creature.OnMoveComplete(None) ran"
    );
    nav::fields_mut(&mut w, g).is_moving = true;
    run(&mut w, &[h, t], 1.0);
    assert!(nav::fields(&w, g).is_moving, "exactly once");
}

#[test]
fn a_target_update_carries_where_the_target_is_at_that_update() {
    let mut w = world();
    let (_, h) = creature(&mut w, 0x7000_0001, 100.0, 100.0);
    let (_, t) = spawn(
        &mut w,
        0x7000_0002,
        KindData::WorldObject,
        0,
        0,
        at(100.0, 120.0),
    );
    settled(&mut w, &[h, t]);
    phys_ext::move_to_object(&mut w, h, t, &MovementParameters::default());

    // step to just before the next 0.5 s target tick
    let tick = |w: &World| {
        motion::with_driver(w, h, |d| d.target.expect("a target").tick_time).expect("driver")
    };
    while w.now.portal_year_ticks + QUANTUM - tick(&w) < 0.5 {
        step(&mut w, &[h, t]);
    }
    let updates = motion::with_driver(&w, h, |d| d.target_updates).expect("driver");

    // the target is moved by gameplay between updates; the tick at the next update sees it
    let moved = moved_to(&w, t, 103.0, 120.0);
    assert!(phys_ext::set_position(&mut w, t, &moved));
    step(&mut w, &[h, t]);
    let (n, info) =
        motion::with_driver(&w, h, |d| (d.target_updates, d.last_target_info)).expect("driver");
    assert_eq!(n, updates + 1, "the tick sent an update");
    assert_eq!(
        info.expect("an update").target_position.frame.origin.x,
        103.0,
        "from the target's position at this update"
    );
}

#[test]
fn a_sticky_move_to_object_sticks_follows_and_expires_after_a_second() {
    let mut w = world();
    let (_, h) = creature(&mut w, 0x7000_0001, 100.0, 100.0);
    let (tg, t) = spawn(
        &mut w,
        0x7000_0002,
        KindData::WorldObject,
        0,
        0,
        at(100.0, 104.0),
    );
    settled(&mut w, &[h, t]);

    let mvp = MovementParameters {
        flags: MovementParameters::default().flags | flags::STICKY,
        ..MovementParameters::default()
    };
    phys_ext::move_to_object(&mut w, h, t, &mvp);
    run_until(&mut w, &[h, t], 5.0, |w| !phys_ext::is_moving_to(w, h)).expect("arrives");
    let stuck = motion::with_driver(&w, h, |d| d.movement.sticky.target_id).expect("driver");
    assert_eq!(stuck.0, tg.full(), "stuck to the target on arrival");

    // at once the target slides 1 m north; within the stick's second (target updates come every
    // 0.5 s) the stuck body closes up behind it
    let before = pos(&w, h);
    let moved = moved_to(&w, t, 100.0, 105.0);
    assert!(phys_ext::set_position(&mut w, t, &moved));
    run(&mut w, &[h, t], 0.9);
    let after = pos(&w, h);
    assert!(
        after.frame.origin.y - before.frame.origin.y > 0.7,
        "followed north: {:?} -> {:?}",
        before.frame.origin,
        after.frame.origin
    );

    // StickTo's deadline is one second
    run(&mut w, &[h, t], 0.5);
    let stuck = motion::with_driver(&w, h, |d| d.movement.sticky.target_id).expect("driver");
    assert_eq!(stuck.0, 0, "unstuck after the timeout");
}

#[test]
fn move_to_object_on_a_gone_target_fails_with_no_object() {
    let mut w = world();
    let (_, h) = creature(&mut w, 0x7000_0001, 100.0, 100.0);
    let (_, t) = spawn(
        &mut w,
        0x7000_0002,
        KindData::WorldObject,
        0,
        0,
        at(100.0, 110.0),
    );
    settled(&mut w, &[h, t]);

    phys_ext::move_to_object(&mut w, h, t, &MovementParameters::default());
    run(&mut w, &[h], 0.5);
    phys_ext::destroy_object(&mut w, t);
    run_until(&mut w, &[h], 2.0, |w| !phys_ext::is_moving_to(w, h)).expect("the MoveTo ends");
    assert_eq!(
        phys_ext::last_move_complete(&w, h),
        Some(WeenieError::ObjectGone),
        "gone after the first update"
    );
}

// ---------------------------------------------------------------------------------- TurnTo

#[test]
fn turn_to_heading_completes_on_the_heading() {
    let mut w = world();
    let (_, h) = creature(&mut w, 0x7000_0001, 100.0, 100.0);
    settled(&mut w, &[h]);
    assert!(heading(&w, h).abs() < 1e-3, "starts facing north");

    let mvp = MovementParameters {
        desired_heading: 90.0,
        ..MovementParameters::default()
    };
    phys_ext::turn_to_heading(&mut w, h, &mvp);
    let took =
        run_until(&mut w, &[h], 3.0, |w| !phys_ext::is_moving_to(w, h)).expect("the turn ends");
    assert!(
        (heading(&w, h) - 90.0).abs() < 1e-3,
        "snapped exactly to 90: {}",
        heading(&w, h)
    );
    assert!(
        (took - 1.0).abs() <= 2.0 * QUANTUM,
        "90 degrees at pi/2 rad/s: {took}"
    );
    assert_eq!(phys_ext::last_move_complete(&w, h), Some(WeenieError::None));

    // turning left (270) takes the short way
    let mvp = MovementParameters {
        desired_heading: 45.0,
        ..MovementParameters::default()
    };
    phys_ext::turn_to_heading(&mut w, h, &mvp);
    run_until(&mut w, &[h], 3.0, |w| !phys_ext::is_moving_to(w, h)).expect("the turn ends");
    assert!((heading(&w, h) - 45.0).abs() < 1e-3, "{}", heading(&w, h));
    assert_eq!(phys_ext::last_move_complete(&w, h), Some(WeenieError::None));
}

#[test]
fn turn_to_object_faces_the_object() {
    let mut w = world();
    let (_, h) = creature(&mut w, 0x7000_0001, 100.0, 100.0);
    let (_, t) = spawn(
        &mut w,
        0x7000_0002,
        KindData::WorldObject,
        0,
        0,
        at(90.0, 100.0),
    );
    settled(&mut w, &[h, t]);

    assert!(phys_ext::turn_to_object(
        &mut w,
        h,
        Some(t),
        &MovementParameters::default()
    ));
    assert!(
        !phys_ext::turn_to_object(&mut w, h, None, &MovementParameters::default()),
        "no object: false"
    );
    run_until(&mut w, &[h, t], 4.0, |w| !phys_ext::is_moving_to(w, h)).expect("the turn ends");
    assert!(
        (heading(&w, h) - 270.0).abs() < 1e-3,
        "facing west: {}",
        heading(&w, h)
    );
    assert_eq!(phys_ext::last_move_complete(&w, h), Some(WeenieError::None));
    assert!(
        dist(&pos(&w, h), &at_pos(100.0, 100.0)) < 1e-3,
        "turned in place"
    );
}

fn at_pos(x: f32, y: f32) -> PPosition {
    phys_ext::to_physics_position(&at(x, y))
}

// ---------------------------------------------------------------------------------- timing

#[test]
fn an_action_animates_for_exactly_its_animation_length() {
    let mut w = world();
    let (g, h) = creature(&mut w, 0x7000_0001, 100.0, 100.0);
    settled(&mut w, &[h]);

    let length = mt::get_animation_length(&w, MT, MotionStance(NONCOMBAT), AceMotion(ACTION), 1.0);
    assert_eq!(length, 0.5, "15 frames at 30/s from Ready");

    let err = phys_ext::do_motion(&mut w, h, ACTION, &phys_ext::ace_movement_parameters());
    assert_eq!(err, WeenieError::None);
    assert!(phys_ext::is_animating(&w, h), "the action is pending");
    assert!(world_object::is_animating(&w, g));
    assert!(phys_ext::is_moving_or_animating(&w, h));

    let took = run_until(&mut w, &[h], 2.0, |w| !phys_ext::is_animating(w, h))
        .expect("the action completes");
    assert!(
        (took - f64::from(length)).abs() <= QUANTUM + 1e-9,
        "not animating after {took}, length {length}"
    );
    assert!(!world_object::is_animating(&w, g));
}

#[test]
fn the_update_gate_ticks_an_animating_creature_and_stops_when_it_is_done() {
    let mut w = world();
    let (g, h) = creature(&mut w, 0x7000_0001, 100.0, 100.0);
    // through ACE's UpdateObjectPhysics: past its first updates an idle creature does not tick
    for _ in 0..30 {
        w.now.portal_year_ticks += QUANTUM;
        empyrean_world::dispatch::update_object_physics::update_object_physics(&mut w, g);
    }
    let _ = take_local();
    let idle = phys_ext::server_record(&w, h)
        .expect("record")
        .last_physics_update;
    w.now.portal_year_ticks += 1.0;
    empyrean_world::dispatch::update_object_physics::update_object_physics(&mut w, g);
    // the gate stamps LastPhysicsUpdate before it decides; the body itself did not step
    assert!(
        phys_ext::server_record(&w, h)
            .expect("record")
            .last_physics_update
            > idle
    );

    // as ACE's callers do (ExecuteMotion): a body at rest restarts its clock first
    phys_ext::restart_clock_if_idle(&mut w, h);
    phys_ext::do_motion(&mut w, h, ACTION, &phys_ext::ace_movement_parameters());
    let mut ticks = 0;
    for _ in 0..60 {
        w.now.portal_year_ticks += QUANTUM;
        empyrean_world::dispatch::update_object_physics::update_object_physics(&mut w, g);
        if !phys_ext::is_animating(&w, h) {
            break;
        }
        ticks += 1;
    }
    assert!(
        !phys_ext::is_animating(&w, h),
        "the action finished through the gated updates"
    );
    assert!(
        ticks >= 14,
        "it took the gated updates at the creature rate: {ticks}"
    );
}

// ---------------------------------------------------------------------------------- players

#[test]
fn a_players_raw_motion_state_run_forward_moves_the_body_at_the_run_rate() {
    let mut w = world();
    let h = player(&mut w, 100.0, 100.0);
    settled(&mut w, &[h]);
    // Use a fixed run rate for the motion fixture
    phys_ext::set_run_rate_stand_in(&mut w, h, Some(2.0));

    let raw = RawMotionState {
        current_holdkey: HoldKey::Run,
        forward_command: MotionCommand(WALK),
        forward_holdkey: HoldKey::Invalid,
        forward_speed: 1.0,
        ..RawMotionState::default()
    };
    phys_ext::apply_raw_motion_state(&mut w, h, &raw, false);
    let state = phys_ext::interpreted_state(&w, h).expect("interp");
    assert_eq!(
        state.forward_command,
        MotionCommand(RUN),
        "walk + Run hold key = run"
    );
    assert_eq!(state.forward_speed, 2.0, "at the run rate");

    let start = pos(&w, h);
    run(&mut w, &[h], 1.0);
    let end = pos(&w, h);
    let covered = dist(&start, &end);
    assert!(
        (covered - RUN_VELOCITY * 2.0).abs() < 0.3,
        "8 m/s for a second: {covered}"
    );
    assert!(
        (end.frame.origin.x - start.frame.origin.x).abs() < 1e-3,
        "straight ahead (north)"
    );

    // releasing the key: Ready again, and the body stops
    let stop = RawMotionState {
        current_holdkey: HoldKey::Run,
        ..RawMotionState::default()
    };
    phys_ext::apply_raw_motion_state(&mut w, h, &stop, false);
    run(&mut w, &[h], 0.2);
    let rest = pos(&w, h);
    run(&mut w, &[h], 0.5);
    assert!(dist(&rest, &pos(&w, h)) < 1e-3, "stopped");
    assert_eq!(
        phys_ext::interpreted_state(&w, h)
            .expect("interp")
            .forward_command,
        MotionCommand(READY)
    );
}

#[test]
fn a_players_motion_done_reaches_handle_motion_done() {
    let mut w = world();
    let h = player(&mut w, 100.0, 100.0);
    settled(&mut w, &[h]);

    use std::sync::atomic::{AtomicBool, Ordering};
    static APPLIED: AtomicBool = AtomicBool::new(false);
    let g = ObjectGuid::new(0x5000_0001);
    let o = w.objects.get_mut(g).unwrap();
    o.set_player_killer_status_prop(empyrean_entity::enums::PlayerKillerStatus::PK);
    o.wo.world_object_properties.current_motion_state =
        Some(empyrean_world::network::motion::movement_data::Motion::new(
            MotionStance::NonCombat,
            AceMotion::Ready,
            1.0,
        ));
    o.player
        .as_mut()
        .unwrap()
        .player_use
        .food_state
        .start_chugging(
            AceMotion(ACTION),
            Box::new(|_w: &mut World| APPLIED.store(true, Ordering::SeqCst)),
            0.0,
            MotionStance::NonCombat,
        );
    phys_ext::do_motion(&mut w, h, ACTION, &phys_ext::ace_movement_parameters());
    run_until(&mut w, &[h], 2.0, |w| !phys_ext::is_animating(w, h)).expect("completes");
    assert!(
        APPLIED.load(Ordering::SeqCst),
        "WeenieObject.OnMotionDone -> Player.HandleMotionDone -> HandleMotionDone_UseConsumable"
    );
    let use_state = &w
        .objects
        .get(g)
        .unwrap()
        .player
        .as_ref()
        .unwrap()
        .player_use
        .food_state;
    assert_eq!(
        use_state.use_motion,
        AceMotion::Ready,
        "waiting for the return to Ready"
    );
}

#[test]
fn raw_motion_state_set_state_defaults_zeroes_as_ace_does() {
    let mut raw = RawMotionState::default();
    let input = RawMotionState {
        current_style: MotionCommand::NONE,
        forward_command: MotionCommand::NONE,
        forward_speed: 0.0,
        sidestep_speed: 0.0,
        turn_speed: 0.0,
        turn_command: MotionCommand(TURN_RIGHT),
        ..RawMotionState::default()
    };
    phys_ext::raw_motion_state_set_state(&mut raw, &input);
    assert_eq!(raw.current_style, MotionCommand(NONCOMBAT));
    assert_eq!(raw.forward_command, MotionCommand(READY));
    assert_eq!(
        (raw.forward_speed, raw.sidestep_speed, raw.turn_speed),
        (1.0, 1.0, 1.0)
    );
    assert_eq!(raw.turn_command, MotionCommand(TURN_RIGHT));
}

#[test]
fn stop_completely_cancels_the_move_to_and_stops() {
    let mut w = world();
    let (_, h) = creature(&mut w, 0x7000_0001, 100.0, 100.0);
    settled(&mut w, &[h]);

    let north = moved_to(&w, h, 100.0, 110.0);
    phys_ext::move_to_position(&mut w, h, &north, &MovementParameters::default());
    run(&mut w, &[h], 1.0);
    phys_ext::stop_completely(&mut w, h, true);
    assert!(!phys_ext::is_moving_to(&w, h));
    assert_eq!(
        phys_ext::last_move_complete(&w, h),
        Some(WeenieError::ActionCancelled)
    );
    run(&mut w, &[h], 0.2);
    let rest = pos(&w, h);
    run(&mut w, &[h], 0.5);
    assert!(dist(&rest, &pos(&w, h)) < 1e-3, "stopped");
}

#[cfg(feature = "real-content")]
fn wrap_stats(w: &World, o: &mut WorldObject) {
    use empyrean_common::dotnet::DotNetDict;
    o.biota.properties_enchantment_registry = Some(Vec::new());
    o.biota
        .properties_attribute
        .get_or_insert_with(DotNetDict::new);
    o.biota
        .properties_attribute_2nd
        .get_or_insert_with(DotNetDict::new);
    o.biota.properties_skill.get_or_insert_with(DotNetDict::new);
    empyrean_world::world_objects::creature_vitals::set_ephemeral_stat_values(w, o);
}

// ---------------------------------------------------------------------------------- real content

#[cfg(feature = "real-content")]
mod real_content {
    //! The retail dats under `DERETH_TEST_DAT_DIR`; fails, never skips,
    //! when they are absent. A drudge (setup `0x020007DD`, motion table `0x09000008`, from the
    //! local ACE world DB) on flat land.

    use super::*;
    use empyrean_dat::{DatManager, RealDats};
    use std::sync::OnceLock;

    const DRUDGE_SETUP: u32 = 0x0200_07DD;
    const DRUDGE_TABLE: u32 = 0x0900_0008;
    const HAND_COMBAT: u32 = 0x8000_003C;

    fn dats() -> Arc<DatManager> {
        static DATS: OnceLock<Arc<DatManager>> = OnceLock::new();
        Arc::clone(DATS.get_or_init(|| {
            let dir = dereth_dat::testing::dat_dir();
            let real = RealDats::open(&dir).unwrap_or_else(|e| {
                panic!(
                    "the real-content tier needs the retail dats under {} (DERETH_TEST_DAT_DIR): {e}",
                    dir.display()
                )
            });
            DatManager::initialize(Arc::new(real)).expect("the retail dats initialize")
        }))
    }

    fn drudge_world() -> (World, PhysHandle) {
        let mut w = World::new(snapshot(0.0), dats());
        phys_ext::use_land_source(&mut w, Arc::new(flat_land_source(&[LandblockId(HOME)], 10)));
        phys_ext::load_landblock(&mut w, HOME);
        phys_ext::set_adjacents(&mut w, HOME, Vec::new());
        let g = ObjectGuid::new(0x7000_0001);
        let pos = at(100.0, 100.0);
        let mut o = WorldObject {
            guid: g,
            container: Some(Box::default()),
            creature: Some(Box::default()),
            kind: KindData::Creature,
            ..Default::default()
        };
        wrap_stats(&w, &mut o);
        o.set_property(PropertyDataId::Setup, DRUDGE_SETUP);
        o.set_property(PropertyDataId::MotionTable, DRUDGE_TABLE);
        o.set_location(Some(pos));
        o.current_landblock = Some(empyrean_entity::LandblockId::new(pos.cell()));
        w.objects.insert(o).expect("fresh guid");
        assert!(phys_ext::add_world_object_physics(&mut w, g));
        let h = phys_ext::physics_obj(&w, g).expect("a body");
        run(&mut w, &[h], 1.0);
        (w, h)
    }

    /// The drudge's real table drives the body: it enters its default style and idles, walks to a
    /// position and arrives, all through the real animations.
    #[test]
    fn a_drudge_walks_on_its_real_motion_table() {
        let (mut w, h) = drudge_world();
        assert!(motion::has_movement_manager(&w, h));
        assert!(!phys_ext::is_animating(&w, h), "idle");
        let target = moved_to(&w, h, 100.0, 108.0);
        phys_ext::move_to_position(&mut w, h, &target, &MovementParameters::default());
        run_until(&mut w, &[h], 10.0, |w| !phys_ext::is_moving_to(w, h)).expect("arrives");
        assert_eq!(phys_ext::last_move_complete(&w, h), Some(WeenieError::None));
        let d = dist(&pos(&w, h), &target);
        assert!(d < 2.0, "arrived within reach: {d}");
    }

    /// In HandCombat, each attack the drudge's table has stops animating after ACE's
    /// `GetAnimationLength` for it (within one physics quantum).
    #[test]
    fn a_drudges_attack_animations_last_their_animation_length() {
        let (mut w, h) = drudge_world();
        let mvp = phys_ext::ace_movement_parameters();
        assert_eq!(
            phys_ext::do_motion(&mut w, h, HAND_COMBAT, &mvp),
            WeenieError::None
        );
        run_until(&mut w, &[h], 5.0, |w| !phys_ext::is_animating(w, h))
            .expect("the stance change completes");
        assert_eq!(
            phys_ext::interpreted_state(&w, h)
                .expect("interp")
                .current_style,
            MotionCommand(HAND_COMBAT)
        );

        let attacks = [
            0x1000_0062_u32,
            0x1000_0063,
            0x1000_0064,
            0x1000_0065,
            0x1000_0066,
            0x1000_0067,
        ];
        let mut checked = 0;
        for attack in attacks {
            let length = mt::get_animation_length(
                &w,
                DRUDGE_TABLE,
                MotionStance(HAND_COMBAT),
                AceMotion(attack),
                1.0,
            );
            if length == 0.0 {
                continue; // the drudge has no such attack
            }
            run(&mut w, &[h], 0.5);
            assert_eq!(
                phys_ext::do_motion(&mut w, h, attack, &mvp),
                WeenieError::None,
                "{attack:08X}"
            );
            assert!(phys_ext::is_animating(&w, h));
            let took = run_until(&mut w, &[h], 5.0, |w| !phys_ext::is_animating(w, h))
                .expect("the attack completes");
            eprintln!("{attack:08X}: GetAnimationLength {length:.4} s, animating for {took:.4} s");
            assert!(
                (took - f64::from(length)).abs() <= QUANTUM + 1e-6,
                "{attack:08X}: not animating after {took}, length {length}"
            );
            checked += 1;
        }
        assert!(checked >= 3, "the drudge has attacks: {checked}");
    }
}

/// A creature whose stats are wrapped as its constructor wraps them (`Creature.SetEphemeralValues`):
/// every attribute at `quickness`, Run trained, `CurrentMotionState` NonCombat/Ready.
fn runner(w: &mut World, guid: u32, quickness: u32, pos: Position) -> (ObjectGuid, PhysHandle) {
    use empyrean_common::dotnet::DotNetDict;
    use empyrean_entity::enums::{PropertyAttribute, Skill, SkillAdvancementClass};
    use empyrean_entity::models::properties_attribute::PropertiesAttribute;
    use empyrean_entity::models::properties_skill::PropertiesSkill;
    use empyrean_world::network::motion::movement_data::Motion;

    let g = ObjectGuid::new(guid);
    let mut o = WorldObject::allocate(empyrean_world::dispatch::Class::Creature);
    o.guid = g;
    o.biota.properties_enchantment_registry = Some(Vec::new());
    let mut attributes = DotNetDict::new();
    for a in 1..=6u16 {
        attributes.insert(
            PropertyAttribute(a),
            PropertiesAttribute {
                init_level: quickness,
                ..Default::default()
            },
        );
    }
    o.biota.properties_attribute = Some(attributes);
    o.biota.properties_attribute_2nd = Some(DotNetDict::new());
    let mut skills = DotNetDict::new();
    skills.insert(
        Skill::Run,
        PropertiesSkill {
            sac: SkillAdvancementClass::Trained,
            ..Default::default()
        },
    );
    o.biota.properties_skill = Some(skills);
    empyrean_world::world_objects::creature_vitals::set_ephemeral_stat_values(w, &mut o);
    o.wo.world_object_properties.current_motion_state =
        Some(Motion::new(MotionStance::NonCombat, AceMotion::Ready, 1.0));
    o.set_property(PropertyDataId::Setup, SETUP);
    o.set_property(PropertyDataId::MotionTable, MT);
    o.set_location(Some(pos));
    o.current_landblock = Some(empyrean_entity::LandblockId::new(pos.cell()));
    w.objects.insert(o).expect("fresh guid");
    assert!(
        phys_ext::add_world_object_physics(w, g),
        "{g:?} entered the world"
    );
    (g, phys_ext::physics_obj(w, g).expect("a body"))
}

/// A running move to moves at the creatures run rate.
#[test]
fn a_running_move_to_moves_at_the_creatures_run_rate() {
    use dereth_assets::tables::{SkillBase, SkillFormula};
    use dereth_assets::SkillTable;
    use empyrean_world::world_objects::creature_navigation;

    // Run = Quickness (formula X = 1, Z = 1, attr1 = Quickness)
    let run_skill = SkillBase {
        description: String::new(),
        name: "Run".into(),
        icon: 0,
        trained_cost: 4,
        specialized_cost: 4,
        category: 3,
        chargen_use: 1,
        min_level: 1,
        formula: SkillFormula {
            w: 0,
            x: 1,
            y: 0,
            z: 1,
            attr1: 3,
            attr2: 0,
        },
        upper_bound: 0.0,
        lower_bound: 0.0,
        learn_mod: 0.0,
    };
    let table = SkillTable {
        id: DataId(0x0E00_0004),
        buckets: 64,
        skills: BTreeMap::from([(24, run_skill)]),
    };
    let mut w =
        world_on(empyrean_testkit::dats::with_stat_tables(FakeDats::new()).with_skill_table(table));
    // Quickness 200: Run 200, GetRunRate(0, 200, 1) = (200 / 400 * 11 + 4) / 4 = 2.375
    let (g, h) = runner(&mut w, 0x7000_0001, 200, at(100.0, 100.0));
    settled(&mut w, &[h]);
    let rate = 2.375f32;

    // 40 m due north (the way it faces): past the 15 m walk/run threshold, so it runs
    let target = at(100.0, 140.0);
    creature_navigation::creature_move_to_position(&mut w, g, &target, 1.0, true, None, None);
    assert!(
        phys_ext::is_moving_to(&w, h),
        "the MoveTo started on the body"
    );
    run(&mut w, &[h], 0.5);
    let state = phys_ext::interpreted_state(&w, h).expect("interp");
    assert_eq!(state.forward_command, MotionCommand(RUN), "running");
    assert_eq!(state.forward_speed, rate, "at the creature's run rate");

    let a = pos(&w, h);
    run(&mut w, &[h], 1.0);
    let covered = dist(&a, &pos(&w, h));
    assert!(
        (covered - RUN_VELOCITY * rate).abs() < 0.3,
        "9.5 m/s for a second: {covered}"
    );
    let o = w.objects.get(g).expect("the runner");
    let m =
        o.wo.world_object_properties
            .current_motion_state
            .as_ref()
            .expect("a motion state");
    assert_eq!(
        m.stance,
        MotionStance::NonCombat,
        "MoveTo(Position) broadcasts without replacing the state"
    );
}

// ---------------------------------------------------------------------------------- AlwaysTurn (A14)

/// `MoveToManager.AlwaysTurn` (ACE's `BeginTurnToHeading`: `if (IsAnimating && !AlwaysTurn)
/// return;`, the shared manager's host option since A14): a turn requested while an action
/// animates waits for the action by default, and starts at once with `AlwaysTurn`.
#[test]
fn always_turn_starts_a_turn_while_an_action_animates() {
    let turned_after = |always_turn: bool| {
        let mut w = world();
        let (_, h) = creature(&mut w, 0x7000_0001, 100.0, 100.0);
        settled(&mut w, &[h]);
        assert_eq!(
            phys_ext::do_motion(&mut w, h, ACTION, &phys_ext::ace_movement_parameters()),
            WeenieError::None
        );
        assert!(phys_ext::is_animating(&w, h), "the action is pending");

        phys_ext::set_move_to_always_turn(&w, h, always_turn);
        assert_eq!(phys_ext::move_to_always_turn(&w, h), always_turn);
        let mvp = MovementParameters {
            desired_heading: 90.0,
            ..MovementParameters::default()
        };
        phys_ext::turn_to_heading(&mut w, h, &mvp);
        // a quarter of the action's 0.5 s
        run(&mut w, &[h], 0.125);
        phys_ext::set_move_to_always_turn(&w, h, false);
        heading(&w, h)
    };
    assert!(
        turned_after(false).abs() < 1e-3,
        "without AlwaysTurn the turn waits for the action: {}",
        turned_after(false)
    );
    let h = turned_after(true);
    assert!(
        h > 1.0 && h < 90.0,
        "with AlwaysTurn the turn is under way: {h}"
    );
}
