//! ACE: Source/ACE.Server/Physics/Common/ObjectMaint.cs::GetKnownObject
//! Phys_ext over shared PhysicsWorld, WeenieObject collision routing, ObjectMaint
//! visibility/forget, ServerObjectManager, trajectory solvers vs vectors; ForceObjDescSend
//! answered only for known/held items.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state, retail dats or world.pack in the real-content tier.

// V6.
#![allow(clippy::disallowed_methods)]

use std::time::Duration;

use dereth_physics::{PhysHandle, SetupGeometry};
use dereth_primitives::{CellId, Vec3};
use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::dotnet::Vector3;
use empyrean_common::not_ported::take_local;
use empyrean_common::vectors::{self, f32_of, f64_of, i64_of, same_f32, same_f64, Case};
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    MotionCommand, MotionStance, PositionType, PropertyBool, PropertyDataId,
};
use empyrean_entity::{ObjectGuid, Position};
use empyrean_testkit::land;
use empyrean_world::dispatch;
use empyrean_world::physics::object_maint as om;
use empyrean_world::physics::phys_ext;
use empyrean_world::physics::{motion_table as mt, server_object_manager, trajectory, trajectory2};
use empyrean_world::world_objects::kinds::KindData;
use empyrean_world::world_objects::world_object::{self, WorldObject};
use empyrean_world::World;
use serde_json::Value;

// ---------------------------------------------------------------------------------- fixtures

const SETUP: u32 = land::TEST_SETUP;
const HOME: u16 = 0xA9B4;

fn snapshot(t: f64) -> ClockSnapshot {
    ClockSnapshot {
        portal_year_ticks: t,
        unix_time: 0.0,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    }
}

/// A world on `FakeDats`, physics over flat landblocks (height index 10: 20 m), one synthetic
/// setup: a 0.5 m sphere standing on its base.
fn world_with(blocks: &[u16]) -> World {
    let mut w = World::new(
        snapshot(0.0),
        empyrean_testkit::dats::with_stat_tables(FakeDats::new())
            .build()
            .expect("fake dats"),
    );
    land::use_flat_land_with_setup(
        &mut w,
        blocks,
        10,
        SETUP,
        SetupGeometry {
            step_up_height: 0.3,
            step_down_height: 0.3,
            ..land::test_setup_geometry()
        },
    );
    // Each physics landblock belongs to a loaded server landblock, as in ACE (`AddPhysicsObj`'s
    // `AdjustDungeon` reaches `LandblockManager.GetLandblock`, which would otherwise load one and
    // re-derive the adjacents from the loaded landblocks alone).
    for &b in blocks {
        empyrean_world::managers::landblock_manager::get_landblock(
            &mut w,
            empyrean_entity::LandblockId::new(u32::from(b) << 16 | 0xFFFF),
            false,
            false,
        );
    }
    for &b in blocks {
        phys_ext::load_landblock(&mut w, b);
        let adj: Vec<u16> = blocks
            .iter()
            .copied()
            .filter(|&o| {
                o != b && phys_ext::get_block_dist(u32::from(o) << 16, u32::from(b) << 16) <= 1
            })
            .collect();
        phys_ext::set_adjacents(&mut w, b, adj);
    }
    w
}

fn world() -> World {
    world_with(&[HOME, 0xAAB4, 0xA9B5])
}

fn at(landblock: u16, x: f32, y: f32, z: f32) -> Position {
    Position::from_components(
        u32::from(landblock) << 16 | 1,
        x,
        y,
        z,
        0.0,
        0.0,
        0.0,
        1.0,
        false,
    )
}

/// An object of `kind` with the synthetic setup at `pos`. `depth`: 1 container, 2 creature, 3 player.
fn spawn(w: &mut World, guid: u32, kind: KindData, depth: u8, pos: Position) -> ObjectGuid {
    let g = ObjectGuid::new(guid);
    let mut o = WorldObject {
        guid: g,
        container: (depth >= 1).then(Box::default),
        creature: (depth >= 2).then(Box::default),
        player: (depth >= 3).then(Box::default),
        kind,
        ..Default::default()
    };
    o.set_property(PropertyDataId::Setup, SETUP);
    o.set_location(Some(pos));
    o.current_landblock = Some(empyrean_entity::LandblockId::new(pos.cell()));
    if depth == 2 {
        // the Creature constructor's `SetMonsterState` caches `IsMonster`
        empyrean_world::world_objects::monster::set_monster_state(&mut o);
    }
    w.objects.insert(o).expect("fresh guid");
    if depth >= 3 {
        let session = empyrean_net::SessionId {
            client_id: 1,
            generation: 1,
        };
        w.sessions.insert(
            session,
            empyrean_world::sessions::SessionData {
                player: Some(g),
                ..Default::default()
            },
        );
        empyrean_world::network::game_messages::game_message::start_capture();
    }
    g
}

/// `Landblock.AddWorldObjectInternal`'s physics part, and the body.
fn enter(w: &mut World, g: ObjectGuid) -> PhysHandle {
    assert!(
        phys_ext::add_world_object_physics(w, g),
        "{g:?} entered the world"
    );
    phys_ext::physics_obj(w, g).expect("a body")
}

/// `LandblockManager.AddObject`: the landblock loads and `AddWorldObjectInternal` makes the body.
fn enter_landblock(w: &mut World, g: ObjectGuid) -> PhysHandle {
    assert!(
        empyrean_world::managers::landblock_manager::add_object(w, g, false),
        "{g:?} joined its landblock"
    );
    phys_ext::physics_obj(w, g).expect("a body")
}

fn tick(w: &mut World, dt: f64) {
    w.now.portal_year_ticks += dt;
}

fn hits(name: &str) -> u64 {
    take_local().get(name).copied().unwrap_or(0)
}

fn creates() -> usize {
    empyrean_world::network::game_messages::game_message::take_sent()
        .iter()
        .filter(|(_, _, b)| b.starts_with(&0xF745u32.to_le_bytes()))
        .count()
}

// ---------------------------------------------------------------------------------- phys_ext

/// ACE: WorldObject.InitPhysicsObj
///
/// An object that bumps on creation (`BumpVelocity`, which the corpse and storage constructors
/// set) is made with a velocity of `(0, 0, 0.5)`, written as a field (no activation, no clamp);
/// any other starts at rest. The velocity is what the object's physics description then carries.
#[test]
fn a_bumped_object_is_made_moving_up_at_half_a_metre_a_second_and_others_at_rest() {
    for (bump, want) in [(true, Vector3::new(0.0, 0.0, 0.5)), (false, Vector3::ZERO)] {
        let mut w = world();
        let g = spawn(
            &mut w,
            0x7000_0001,
            KindData::WorldObject,
            0,
            at(HOME, 100.0, 100.0, 25.0),
        );
        w.objects
            .get_mut(g)
            .expect("spawned")
            .wo
            .world_object
            .bump_velocity = bump;
        let _ = take_local();

        dispatch::init_physics_obj::init_physics_obj(&mut w, g);
        let h = phys_ext::physics_obj(&w, g).expect("InitPhysicsObj made a body");
        assert_eq!(phys_ext::velocity(&w, h), want, "bump_velocity = {bump}");
        assert_eq!(
            empyrean_world::world_objects::world_object_properties::velocity(&w, g),
            want,
            "and WorldObject.Velocity reads it"
        );
    }
}

#[test]
fn an_object_is_made_enters_the_world_and_falls_to_the_ground() {
    let mut w = world();
    let g = spawn(
        &mut w,
        0x7000_0001,
        KindData::WorldObject,
        0,
        at(HOME, 100.0, 100.0, 25.0),
    );
    let _ = take_local();

    dispatch::init_physics_obj::init_physics_obj(&mut w, g);
    let h = phys_ext::physics_obj(&w, g).expect("InitPhysicsObj made a body");
    assert_eq!(
        phys_ext::id(&w, h),
        Some(g.full()),
        "set_object_guid named it"
    );
    assert_eq!(
        server_object_manager::get_object_a(&w, g.full()),
        Some(h),
        "and registered it"
    );
    assert_eq!(
        phys_ext::weenie_obj(&w, h).world_object(&w),
        Some(g),
        "the WeenieObject links back"
    );
    assert_eq!(
        phys_ext::state(&w, h).0,
        0x0040_0C08,
        "the default state (no PhysicsState property)"
    );
    assert_eq!(
        hits("ACE: WorldObject.CalculatedPhysicsState"),
        0,
        "Physics state is applied to the body"
    );
    // CalculatedPhysicsState set the null PropertyBool counterparts of the default state's flags
    let o = w.objects.get(g).unwrap();
    assert_eq!(
        (
            o.report_collisions(),
            o.gravity_status(),
            o.lights_status(),
            o.allow_edge_slide()
        ),
        (Some(true), Some(true), Some(true), Some(true))
    );
    assert_eq!(o.ethereal(), None);

    assert!(world_object::add_physics_obj(&mut w, g));
    let cell = phys_ext::cur_cell(&w, h).expect("in a cell");
    assert_eq!(
        cell,
        CellId(0xA9B4_0025),
        "outdoor cell (4, 4) of the landblock"
    );
    assert_eq!(phys_ext::cur_landblock(&w, h), Some(HOME));
    assert_eq!(phys_ext::get_server_objects(&w, HOME, false), vec![h]);
    let home = w
        .objects
        .get(g)
        .and_then(|o| o.get_position(PositionType::Home))
        .expect("Home set");
    assert_eq!(home.cell(), 0xA9B4_0025);
    assert!(phys_ext::is_active(&w, h));

    // Fall under gravity through the ported UpdateObjectPhysics.
    let z0 = w
        .objects
        .get(g)
        .and_then(|o| o.location())
        .expect("location")
        .position_z;
    for _ in 0..60 {
        tick(&mut w, 1.0 / 30.0);
        dispatch::update_object_physics::update_object_physics(&mut w, g);
    }
    let loc = w
        .objects
        .get(g)
        .and_then(|o| o.location())
        .expect("location");
    assert!(loc.position_z < z0, "fell: {} -> {}", z0, loc.position_z);
    assert!(
        (loc.position_z - 20.0).abs() < 0.05,
        "rests on the 20 m ground, at {}",
        loc.position_z
    );
    let body = phys_ext::position(&w, h).expect("body");
    assert_eq!(
        body.frame.origin.z, loc.position_z,
        "Location is synced from the body"
    );

    // Once it rests, InitialUpdates passes 1 and a non-missile, non-animating object stops ticking.
    let rec = phys_ext::server_record(&w, h).expect("record");
    assert!(
        rec.initial_updates > 1,
        "initial updates: {}",
        rec.initial_updates
    );
}

#[test]
fn a_moved_object_changes_cell_and_landblock_lists() {
    let mut w = world();
    let g = spawn(
        &mut w,
        0x7000_0001,
        KindData::WorldObject,
        0,
        at(HOME, 100.0, 100.0, 20.0),
    );
    let h = enter(&mut w, g);

    let target = phys_ext::to_physics_position(&at(0xAAB4, 10.0, 100.0, 20.0));
    assert!(phys_ext::set_position(&mut w, h, &target));
    assert_eq!(phys_ext::cur_landblock(&w, h), Some(0xAAB4));
    assert!(
        phys_ext::get_server_objects(&w, HOME, false).is_empty(),
        "left the old landblock's list"
    );
    assert_eq!(phys_ext::get_server_objects(&w, 0xAAB4, false), vec![h]);
    assert_eq!(
        phys_ext::get_server_objects(&w, HOME, true),
        vec![h],
        "seen through the adjacents"
    );

    phys_ext::destroy_object(&mut w, h);
    assert!(phys_ext::get_server_objects(&w, 0xAAB4, false).is_empty());
    assert_eq!(server_object_manager::get_object_a(&w, g.full()), None);
    assert!(w.physics.get(h).is_none());
}

#[test]
fn an_object_outside_the_loaded_landblocks_is_not_placed() {
    let mut w = world();
    let g = spawn(
        &mut w,
        0x7000_0001,
        KindData::WorldObject,
        0,
        at(0x1010, 100.0, 100.0, 20.0),
    );
    dispatch::init_physics_obj::init_physics_obj(&mut w, g);
    assert!(
        !world_object::add_physics_obj(&mut w, g),
        "LScape.get_landcell found no cell"
    );
    assert!(
        w.objects.get(g).and_then(|o| o.phys).is_none(),
        "the body was destroyed and dropped"
    );
    assert!(server_object_manager::get_object_a(&w, g.full()).is_none());
}

#[test]
fn set_physics_property_state_writes_the_property_and_the_body() {
    let mut w = world();
    let g = spawn(
        &mut w,
        0x5000_0001,
        KindData::Player,
        3,
        at(HOME, 100.0, 100.0, 20.0),
    );
    dispatch::init_physics_obj::init_physics_obj(&mut w, g);
    let h = phys_ext::physics_obj(&w, g).expect("body");
    let s = phys_ext::state(&w, h).0;
    assert_ne!(s & 0x10, 0, "Player.InitPhysicsObj: IgnoreCollisions");
    assert_eq!(s & 0x08, 0, "ReportCollisions cleared");
    assert_ne!(s & 0x4000, 0, "Hidden");
    let o = w.objects.get(g).expect("player");
    assert_eq!(o.get_property(PropertyBool::IgnoreCollisions), Some(true));
    assert_eq!(o.get_property(PropertyBool::ReportCollisions), Some(false));

    phys_ext::set_physics_property_state(
        &mut w,
        g,
        PropertyBool::IgnoreCollisions,
        empyrean_entity::enums::PhysicsState::IgnoreCollisions,
        None,
    );
    assert_eq!(phys_ext::state(&w, h).0 & 0x10, 0, "null clears the flag");
    assert_eq!(
        w.objects
            .get(g)
            .and_then(|o| o.get_property(PropertyBool::IgnoreCollisions)),
        None
    );
}

// ---------------------------------------------------------------------------------- collisions (V6)

#[test]
fn a_collision_notice_reaches_the_movers_dispatch_and_ends_at_the_target() {
    let mut w = world();
    // the mover is a creature: Creature.OnCollideObject hands a hotspot target to
    // Hotspot.OnCollideObject, which records the creature (AffectsAis lets a monster count)
    let door = spawn(
        &mut w,
        0x8000_0010,
        KindData::Creature,
        2,
        at(HOME, 100.0, 100.0, 20.0),
    );
    let hot = spawn(
        &mut w,
        0x7000_0011,
        KindData::Hotspot(Box::default()),
        0,
        at(HOME, 102.5, 100.0, 20.0),
    );
    w.objects
        .get_mut(hot)
        .expect("the hotspot")
        .set_property(PropertyBool::AffectsAis, true);
    w.objects
        .get_mut(hot)
        .expect("the hotspot")
        .set_property(empyrean_entity::enums::PropertyFloat::HotspotCycleTime, 1.0);
    let hd = enter_landblock(&mut w, door);
    let _ht = enter_landblock(&mut w, hot);
    let _ = take_local();
    let touching = |w: &World| match &w.objects.get(hot).expect("the hotspot").kind {
        KindData::Hotspot(d) => d.hotspot.creatures.contains(&door),
        _ => unreachable!("a hotspot"),
    };
    assert!(!touching(&w));

    phys_ext::set_velocity(&mut w, hd, Vector3::new(5.0, 0.0, 0.0), false);
    for _ in 0..30 {
        tick(&mut w, 1.0 / 30.0);
        phys_ext::update_object(&mut w, hd);
    }
    assert!(
        touching(&w),
        "WeenieObject.DoCollision routed to Creature.OnCollideObject, then Hotspot.OnCollideObject"
    );
    let rec = phys_ext::server_record(&w, hd).expect("record");
    assert!(
        rec.collision_table.contains_key(&hot.full()),
        "track_object_collision recorded the hotspot"
    );

    // Stop, move away, and let a second pass: report_collision_end reaches the hotspot.
    phys_ext::set_velocity_field(&mut w, hd, Vector3::ZERO);
    let away = phys_ext::to_physics_position(&at(HOME, 90.0, 100.0, 20.0));
    assert!(phys_ext::set_position(&mut w, hd, &away));
    let _ = take_local();
    tick(&mut w, 1.5);
    phys_ext::update_object(&mut w, hd);
    let rec = phys_ext::server_record(&w, hd).expect("record");
    assert!(rec.collision_table.is_empty());
}

#[test]
fn an_environment_collision_reaches_on_collide_environment() {
    let mut w = world();
    let ammo = spawn(
        &mut w,
        0x7000_0020,
        KindData::Ammunition(Box::default()),
        0,
        at(HOME, 100.0, 100.0, 23.0),
    );
    let control = spawn(
        &mut w,
        0x7000_0021,
        KindData::WorldObject,
        0,
        at(HOME, 104.0, 100.0, 23.0),
    );
    let h = enter_landblock(&mut w, ammo);
    let hc = enter_landblock(&mut w, control);
    let _ = take_local();
    let (mut removed, mut landed) = (None, None);
    for step in 0..60 {
        tick(&mut w, 1.0 / 30.0);
        if removed.is_none() {
            phys_ext::update_object(&mut w, h);
            if w.objects.get(ammo).is_none() {
                removed = Some(step);
            }
        }
        phys_ext::update_object(&mut w, hc);
        #[allow(clippy::float_cmp)]
        if landed.is_none() && phys_ext::position(&w, hc).is_some_and(|p| p.frame.origin.z == 20.0)
        {
            landed = Some(step);
        }
    }
    let removed =
        removed.expect("an environment collision reached Ammunition.OnCollideEnvironment");
    assert_eq!(
        Some(removed),
        landed,
        "reported on reaching the ground, not while falling"
    );
}

// ---------------------------------------------------------------------------------- ObjectMaint

const P: u32 = 0x5000_0001;

#[test]
fn a_player_sees_objects_in_ace_order_and_the_clamp_holds_back_far_ones() {
    let mut w = world();
    // server-object order: the adjacent block's object first, then two in the home block
    let x1 = spawn(
        &mut w,
        0x7000_0001,
        KindData::WorldObject,
        0,
        at(0xAAB4, 5.0, 100.0, 20.0),
    );
    let x2 = spawn(
        &mut w,
        0x7000_0002,
        KindData::WorldObject,
        0,
        at(HOME, 110.0, 100.0, 20.0),
    );
    let x3 = spawn(
        &mut w,
        0x7000_0003,
        KindData::WorldObject,
        0,
        at(HOME, 90.0, 120.0, 20.0),
    );
    let far = spawn(
        &mut w,
        0x7000_0004,
        KindData::WorldObject,
        0,
        at(0xA9B5, 100.0, 40.0, 20.0),
    );
    let h1 = enter(&mut w, x1);
    let h2 = enter(&mut w, x2);
    let h3 = enter(&mut w, x3);
    let hf = enter(&mut w, far);
    let _ = take_local();

    let p = spawn(&mut w, P, KindData::Player, 3, at(HOME, 100.0, 100.0, 20.0));
    let hp = enter(&mut w, p);
    // Re-pinned for V260/V278/V286/V287/V308 stage 2b (the retail pcaps): ACE created all three; the create set is the
    // round cell window, and `x1` is 4 cells east of the player's cell (dx² = 16 > 13).
    assert_eq!(creates(), 2, "a create for each object in the create set");

    // GetServerObjects(true): the home block's list, then the adjacents' (AAB4, then A9B5). The
    // visible objects stay ACE's landblock view (the monsters read it), clamp included.
    assert_eq!(
        om::get_visible_objects_values(&w, hp),
        vec![h2, h3, h1],
        "InitialClamp: `far` is 2D 132 m away (beyond 112.5 m)"
    );
    assert_eq!(
        om::get_known_objects_values(&w, hp),
        vec![h2, h3],
        "the create set, in ACE's order"
    );
    assert!(
        !om::known_objects_contains_key(&w, hp, x1.full()),
        "4 cells east: outside the window"
    );
    assert!(!om::known_objects_contains_key(&w, hp, far.full()));
    for h in [h2, h3] {
        assert_eq!(
            om::get_known_players_values(&w, h),
            vec![hp],
            "each object created knows the player"
        );
    }
    assert!(om::get_known_players_values(&w, h1).is_empty());
    assert!(om::get_known_players_values(&w, hf).is_empty());
}

#[test]
fn objects_leaving_view_are_queued_then_forgotten_after_21_seconds() {
    let mut w = world_with(&[HOME, 0xAAB4, 0xACB4]);
    let x = spawn(
        &mut w,
        0x7000_0001,
        KindData::WorldObject,
        0,
        at(HOME, 110.0, 100.0, 20.0),
    );
    let hx = enter(&mut w, x);
    let p = spawn(&mut w, P, KindData::Player, 3, at(HOME, 100.0, 100.0, 20.0));
    let hp = enter(&mut w, p);
    assert_eq!(om::get_visible_objects_count(&w, hp), 1);

    // three blocks east: nothing of HOME is visible from there
    let there = phys_ext::to_physics_position(&at(0xACB4, 100.0, 100.0, 20.0));
    assert!(phys_ext::set_position(&mut w, hp, &there));
    assert_eq!(om::get_visible_objects_count(&w, hp), 0, "occluded");
    assert_eq!(om::get_known_objects_count(&w, hp), 1, "still known");
    assert_eq!(
        om::get_destruction_queue_copy(&w, hp),
        vec![(hx, 21.0)],
        "queued until now + DestructionTime (retail's 21 s)"
    );

    // back inside 21 s: visible again, dequeued, no second create
    let _ = take_local();
    let _ = creates();
    tick(&mut w, 10.0);
    let back = phys_ext::to_physics_position(&at(HOME, 100.0, 100.0, 20.0));
    assert!(phys_ext::set_position(&mut w, hp, &back));
    assert_eq!(om::get_visible_objects_count(&w, hp), 1);
    assert_eq!(om::get_destruction_queue_count(&w, hp), 0);
    assert_eq!(creates(), 0, "already known: no create");

    // away for more than 21 s, then any cell change forgets it
    assert!(phys_ext::set_position(&mut w, hp, &there));
    tick(&mut w, 22.0);
    let there2 = phys_ext::to_physics_position(&at(0xACB4, 150.0, 100.0, 20.0));
    assert!(phys_ext::set_position(&mut w, hp, &there2));
    assert_eq!(
        om::get_known_objects_count(&w, hp),
        0,
        "DestroyObjects expired it"
    );
    assert!(
        om::get_known_players_values(&w, hx).is_empty(),
        "the inverse is removed too"
    );
}

/// V260/V278/V286/V287/V308 stage 1: an object that walks out of a player's view and back is not forgotten while in
/// view, whether it returns inside 21 s (kept, no create) or after its deadline but before the next
/// sweep (stage 2a: forgotten and created afresh on the spot, then kept).
#[test]
fn an_object_back_in_view_is_taken_off_the_forget_queue() {
    for (away, recreated, label) in [
        (10.0, 0, "back inside 21 s"),
        (22.0, 1, "back after the deadline, before the sweep"),
    ] {
        let mut w = world_with(&[HOME, 0xAAB4, 0xACB4]);
        let p = spawn(&mut w, P, KindData::Player, 3, at(HOME, 100.0, 100.0, 20.0));
        let hp = enter(&mut w, p);
        let x = spawn(
            &mut w,
            0x7000_0001,
            KindData::WorldObject,
            0,
            at(HOME, 110.0, 100.0, 20.0),
        );
        let hx = enter(&mut w, x);
        assert_eq!(om::get_known_players_values(&w, hx), vec![hp], "{label}");

        // the object moves three blocks east, out of the player's view
        let there = phys_ext::to_physics_position(&at(0xACB4, 100.0, 100.0, 20.0));
        assert!(phys_ext::set_position(&mut w, hx, &there));
        assert_eq!(
            om::get_visible_objects_count(&w, hp),
            0,
            "{label}: occluded"
        );
        assert_eq!(
            om::get_destruction_queue_count(&w, hp),
            1,
            "{label}: queued"
        );

        // and comes back
        tick(&mut w, away);
        let _ = take_local();
        let _ = creates();
        let back = phys_ext::to_physics_position(&at(HOME, 110.0, 100.0, 20.0));
        assert!(phys_ext::set_position(&mut w, hx, &back));
        assert_eq!(
            creates(),
            recreated,
            "{label}: created again only after the deadline"
        );
        assert_eq!(
            om::get_visible_objects_values(&w, hp),
            vec![hx],
            "{label}: visible again"
        );
        assert_eq!(
            om::get_destruction_queue_count(&w, hp),
            0,
            "{label}: no pending forget"
        );

        // past the original deadline, the player's sweep keeps it
        tick(&mut w, 30.0);
        assert!(
            om::destroy_objects(&mut w, hp).is_empty(),
            "{label}: nothing expired"
        );
        assert!(
            om::known_objects_contains_key(&w, hp, x.full()),
            "{label}: still known"
        );
        assert_eq!(
            om::get_known_players_values(&w, hx),
            vec![hp],
            "{label}: still sent its broadcasts"
        );
    }
}

/// V260/V278/V286/V287/V308 stage 2a (retail's 21 s): an object out of a player's view stays known until 21 s have
/// passed and is forgotten by the first sweep after that; one coming back just before 21 s is not
/// created again, one coming back just after is, even before any sweep has run.
#[test]
fn an_object_out_of_view_is_forgotten_at_21_seconds_not_before() {
    // the player walks away and the object stays put: the sweep decides
    let mut w = world_with(&[HOME, 0xAAB4, 0xACB4]);
    let x = spawn(
        &mut w,
        0x7000_0001,
        KindData::WorldObject,
        0,
        at(HOME, 110.0, 100.0, 20.0),
    );
    let hx = enter(&mut w, x);
    let p = spawn(&mut w, P, KindData::Player, 3, at(HOME, 100.0, 100.0, 20.0));
    let hp = enter(&mut w, p);
    let there = phys_ext::to_physics_position(&at(0xACB4, 100.0, 100.0, 20.0));
    assert!(phys_ext::set_position(&mut w, hp, &there));
    tick(&mut w, 20.9);
    assert!(
        om::destroy_objects(&mut w, hp).is_empty(),
        "20.9 s: not yet"
    );
    assert!(
        om::known_objects_contains_key(&w, hp, x.full()),
        "20.9 s: still known"
    );
    tick(&mut w, 0.2);
    assert_eq!(
        om::destroy_objects(&mut w, hp),
        vec![hx],
        "21.1 s: the sweep forgets it"
    );
    assert!(!om::known_objects_contains_key(&w, hp, x.full()));
    assert!(
        om::get_known_players_values(&w, hx).is_empty(),
        "and it no longer knows the player"
    );

    // the object walks away and back, with no sweep in between: the return decides
    for (away, recreated, label) in [(20.9, 0, "back at 20.9 s"), (21.1, 1, "back at 21.1 s")] {
        let mut w = world_with(&[HOME, 0xAAB4, 0xACB4]);
        let p = spawn(&mut w, P, KindData::Player, 3, at(HOME, 100.0, 100.0, 20.0));
        let hp = enter(&mut w, p);
        let x = spawn(
            &mut w,
            0x7000_0001,
            KindData::WorldObject,
            0,
            at(HOME, 110.0, 100.0, 20.0),
        );
        let hx = enter(&mut w, x);
        let there = phys_ext::to_physics_position(&at(0xACB4, 100.0, 100.0, 20.0));
        assert!(phys_ext::set_position(&mut w, hx, &there));
        tick(&mut w, away);
        let _ = take_local();
        let _ = creates();
        let back = phys_ext::to_physics_position(&at(HOME, 110.0, 100.0, 20.0));
        assert!(phys_ext::set_position(&mut w, hx, &back));
        assert_eq!(creates(), recreated, "{label}");
        assert!(
            om::known_objects_contains_key(&w, hp, x.full()),
            "{label}: known"
        );
        assert_eq!(
            om::get_known_players_values(&w, hx),
            vec![hp],
            "{label}: sent its broadcasts"
        );
        assert_eq!(
            om::get_destruction_queue_count(&w, hp),
            0,
            "{label}: nothing pending"
        );
    }
}

/// A player with a session of its own (`client`), so each player's sends can be told apart.
fn spawn_player(w: &mut World, guid: u32, client: u16, pos: Position) -> ObjectGuid {
    // `spawn` gives every player the one session; give this one its own
    let shared = empyrean_net::SessionId {
        client_id: 1,
        generation: 1,
    };
    let before = w.sessions.get_mut(shared).and_then(|s| s.player);
    let g = spawn(w, guid, KindData::Player, 3, pos);
    if client != 1 {
        if let Some(s) = w.sessions.get_mut(shared) {
            s.player = before;
        }
        let session = empyrean_net::SessionId {
            client_id: client,
            generation: 1,
        };
        w.sessions.insert(
            session,
            empyrean_world::sessions::SessionData {
                player: Some(g),
                ..Default::default()
            },
        );
    }
    g
}

/// The messages sent since the last call, by client id, as (client, opcode).
fn sent_opcodes() -> Vec<(u16, u32)> {
    empyrean_world::network::game_messages::game_message::take_sent()
        .iter()
        .filter(|(_, _, b)| b.len() >= 4)
        .map(|(s, _, b)| (s.client_id, u32::from_le_bytes([b[0], b[1], b[2], b[3]])))
        .collect()
}

fn received(sent: &[(u16, u32)], client: u16, opcode: u32) -> usize {
    sent.iter()
        .filter(|&&(c, o)| c == client && o == opcode)
        .count()
}

/// Three players around an object: A beside it (knows it), B one landblock east and 360 m away
/// (inside the 3×3, never sent the object: ACE's 112.5 m first-add clamp skips it), C three
/// landblocks east (outside the 3×3).
fn reach_scene() -> (World, ObjectGuid, [ObjectGuid; 3]) {
    let mut w = world_with(&[HOME, 0xAAB4, 0xACB4]);
    let x = spawn(
        &mut w,
        0x7000_0001,
        KindData::WorldObject,
        0,
        at(HOME, 10.0, 100.0, 20.0),
    );
    let a = spawn_player(&mut w, P, 1, at(HOME, 12.0, 100.0, 20.0));
    let b = spawn_player(&mut w, P + 1, 2, at(0xAAB4, 180.0, 100.0, 20.0));
    let c = spawn_player(&mut w, P + 2, 3, at(0xACB4, 100.0, 100.0, 20.0));
    let hx = enter_landblock(&mut w, x);
    let ha = enter_landblock(&mut w, a);
    let hb = enter_landblock(&mut w, b);
    let hc = enter_landblock(&mut w, c);
    let known = om::get_known_players_values(&w, hx);
    assert_eq!(known, vec![ha], "only A has been sent the object");
    assert!(!known.contains(&hb) && !known.contains(&hc));
    let _ = sent_opcodes();
    (w, x, [a, b, c])
}

/// V260/V278/V286/V287/V308 stage 2c (the retail pcaps): an object's updates reach every player in its 3×3 landblocks,
/// known or not, and nobody past them; who knows the object is unchanged.
#[test]
fn an_update_reaches_every_player_in_the_3x3_landblocks_known_or_not() {
    let (mut w, x, [a, b, c]) = reach_scene();
    let hx = phys_ext::physics_obj(&w, x).expect("a body");
    let ha = phys_ext::physics_obj(&w, a).expect("a body");
    assert_eq!(
        empyrean_world::world_objects::world_object_networking::reach_players(&w, x),
        vec![a, b],
        "the reach: A and B, not C"
    );

    empyrean_world::world_objects::world_object::enqueue_broadcast_physics_state(&mut w, x);
    let sent = sent_opcodes();
    assert_eq!(
        received(&sent, 1, 0xF74B),
        1,
        "A (knows it) gets the SetState, as before"
    );
    assert_eq!(
        received(&sent, 2, 0xF74B),
        1,
        "B (in the reach, never sent it) gets it too"
    );
    assert_eq!(
        received(&sent, 3, 0xF74B),
        0,
        "C (outside the 3×3) gets nothing"
    );
    assert_eq!(sent.iter().filter(|&&(c, _)| c == 3).count(), 0);
    assert_eq!(
        om::get_known_players_values(&w, hx),
        vec![ha],
        "knowledge is unchanged"
    );
    let hb = phys_ext::physics_obj(&w, b).expect("a body");
    assert!(
        !om::known_objects_contains_key(&w, hb, x.full()),
        "B still does not know it"
    );
    let _ = c;
}

/// V260/V278/V286/V287/V308 stage 2c: a CreateObject or UpdateObject broadcast stays with the players who know the
/// object (either one creates it on a client that lacks it).
#[test]
fn a_create_broadcast_stays_with_the_players_who_know_the_object() {
    let (mut w, x, _) = reach_scene();
    let create = empyrean_world::network::game_messages::messages::game_message_create_object::game_message_create_object(&mut w, x, false, false);
    let update = empyrean_world::network::game_messages::messages::game_message_update_object::game_message_update_object(&mut w, x, false, false);
    empyrean_world::world_objects::world_object_networking::enqueue_broadcast(
        &mut w,
        x,
        false,
        &[create],
    );
    empyrean_world::world_objects::world_object_networking::enqueue_broadcast(
        &mut w,
        x,
        false,
        &[update],
    );
    let sent = sent_opcodes();
    assert_eq!(received(&sent, 1, 0xF745), 1, "A");
    assert_eq!(received(&sent, 1, 0xF7DB), 1, "A");
    assert_eq!(
        received(&sent, 2, 0xF745) + received(&sent, 2, 0xF7DB),
        0,
        "not B"
    );
    assert_eq!(
        received(&sent, 3, 0xF745) + received(&sent, 3, 0xF7DB),
        0,
        "not C"
    );
}

/// V260/V278/V286/V287/V308 stage 2c: an object's delete reaches every player in its 3×3 landblocks, including one
/// who never knew it or has forgotten it (retail's corpse decay deletes); nobody past them.
#[test]
fn a_delete_reaches_players_in_the_reach_who_never_knew_or_forgot_the_object() {
    let (mut w, x, [a, b, c]) = reach_scene();
    let hx = phys_ext::physics_obj(&w, x).expect("a body");
    let ha = phys_ext::physics_obj(&w, a).expect("a body");
    // A forgets it (the 21 s forget, silently)
    om::remove_object(&mut w, ha, hx, true);
    assert!(
        om::get_known_players_values(&w, hx).is_empty(),
        "nobody knows it now"
    );

    empyrean_world::world_objects::player_tracking::enqueue_action_broadcast_remove_tracked_object(
        &mut w, x, false,
    );
    for p in [a, b, c] {
        run_due(&mut w, p);
    }
    let sent = sent_opcodes();
    assert_eq!(
        received(&sent, 1, 0xF747),
        1,
        "A (forgot it) gets the DeleteObject"
    );
    assert_eq!(
        received(&sent, 2, 0xF747),
        1,
        "B (never knew it) gets the DeleteObject"
    );
    assert_eq!(
        received(&sent, 3, 0xF747),
        0,
        "C (outside the 3×3) gets nothing"
    );
    let hb = phys_ext::physics_obj(&w, b).expect("a body");
    assert!(
        !om::known_objects_contains_key(&w, hb, x.full()),
        "B's tables are untouched"
    );
}

/// The chat message every ranged broadcast below sends, as speech, emotes and local chat do.
fn chat_message() -> empyrean_world::network::game_messages::game_message::GameMessage {
    empyrean_world::network::game_messages::game_message::GameMessage::new(
        empyrean_world::network::game_messages::game_message_opcode::GameMessageOpcode(0xF7E0),
        empyrean_net::GameMessageGroup::UIQueue,
    )
}

/// A ranged broadcast (speech, emotes, local chat) goes to the players who know the object and
/// are within range, as ACE sends it: a listener out of range is not sent it, and nor is one in
/// range who has forgotten the object. V286's reach is for object updates.
#[test]
fn a_ranged_broadcast_goes_to_the_known_players_within_range() {
    let (mut w, x, [a, _b, _c]) = reach_scene();
    let hx = phys_ext::physics_obj(&w, x).expect("a body");
    let ha = phys_ext::physics_obj(&w, a).expect("a body");
    let msg = chat_message();
    // within 20 m: A, known
    empyrean_world::world_objects::world_object_networking::enqueue_broadcast_range(
        &mut w, x, &msg, 20.0, None,
    );
    let sent = sent_opcodes();
    assert_eq!(received(&sent, 1, 0xF7E0), 1, "A, 2 m away");
    assert_eq!(
        received(&sent, 2, 0xF7E0),
        0,
        "B, in the reach but 360 m away"
    );
    assert_eq!(received(&sent, 3, 0xF7E0), 0, "C");

    // A forgets the object and is still in range: no longer sent it
    om::remove_object(&mut w, ha, hx, true);
    empyrean_world::world_objects::world_object_networking::enqueue_broadcast_range(
        &mut w, x, &msg, 20.0, None,
    );
    let sent = sent_opcodes();
    assert_eq!(
        received(&sent, 1, 0xF7E0),
        0,
        "A, in range but no longer knowing the speaker"
    );
    assert_eq!(received(&sent, 2, 0xF7E0) + received(&sent, 3, 0xF7E0), 0);
}

/// A speaker the listener was never sent is not heard, however wide the range: B is in the reach
/// and within a 400 m range, but has never been sent the object.
#[test]
fn a_ranged_broadcast_does_not_reach_a_player_in_range_who_was_never_sent_the_speaker() {
    let (mut w, x, _) = reach_scene();
    empyrean_world::world_objects::world_object_networking::enqueue_broadcast_range(
        &mut w,
        x,
        &chat_message(),
        400.0,
        None,
    );
    let sent = sent_opcodes();
    assert_eq!(received(&sent, 1, 0xF7E0), 1, "A, who knows it");
    assert_eq!(
        received(&sent, 2, 0xF7E0),
        0,
        "B, 360 m away and in range, never sent the speaker"
    );
    assert_eq!(received(&sent, 3, 0xF7E0), 0, "C, outside the 3x3");
}

/// V286 stage 2c: a motion broadcast with a maximum range is an object update, so it reaches the
/// players in the 3x3 within that range whether or not they know the object.
#[test]
fn a_ranged_motion_broadcast_reaches_players_in_range_who_do_not_know_the_object() {
    let (mut w, x, _) = reach_scene();
    let motion = empyrean_world::network::motion::movement_data::Motion::new(
        MotionStance::NonCombat,
        MotionCommand::Ready,
        1.0,
    );
    empyrean_world::world_objects::world_object_networking::enqueue_broadcast_motion(
        &mut w,
        x,
        &motion,
        Some(400.0),
        Some(false),
    );
    let sent = sent_opcodes();
    assert_eq!(received(&sent, 1, 0xF74C), 1, "A, who knows it");
    assert_eq!(
        received(&sent, 2, 0xF74C),
        1,
        "B, in the reach and in range, never sent the object"
    );
    assert_eq!(received(&sent, 3, 0xF74C), 0, "C, outside the 3x3");
}

// ---------------------------------------------------------------------------------- V288

/// The client's ask to describe an object it has no object for (`0xF6EA`), from `p`.
fn ask(w: &mut World, p: ObjectGuid, guid: ObjectGuid) {
    empyrean_world::world_objects::player::handle_action_force_obj_desc_send(w, p, guid.full());
}

/// An item with the synthetic setup and no location (not in the world).
fn loose_item(w: &mut World, guid: u32, depth: u8) -> ObjectGuid {
    let g = ObjectGuid::new(guid);
    let mut o = WorldObject {
        guid: g,
        container: (depth >= 1).then(Box::default),
        kind: KindData::WorldObject,
        ..Default::default()
    };
    o.set_property(PropertyDataId::Setup, SETUP);
    w.objects.insert(o).expect("fresh guid");
    g
}

/// V288 (the retail pcaps): a known object is answered with one fresh CreateObject to the asking
/// client only, never an appearance update; the known objects and the forget queue are untouched.
#[test]
fn a_known_object_is_answered_with_a_create_to_the_asker_only() {
    let (mut w, x, [a, _b, _c]) = reach_scene();
    let hx = phys_ext::physics_obj(&w, x).expect("a body");
    let ha = phys_ext::physics_obj(&w, a).expect("a body");
    let known_before = om::get_known_objects_values(&w, ha);
    ask(&mut w, a, x);
    let sent = sent_opcodes();
    assert_eq!(received(&sent, 1, 0xF745), 1, "one CreateObject to A");
    assert_eq!(sent.len(), 1, "nothing else, to nobody else: {sent:?}");
    assert_eq!(received(&sent, 1, 0xF625), 0, "never the appearance update");
    assert_eq!(om::get_known_objects_values(&w, ha), known_before);
    assert_eq!(om::get_known_players_values(&w, hx), vec![ha]);
    assert_eq!(
        om::get_destruction_queue_count(&w, ha),
        0,
        "no forget queued"
    );

    // the create is the normal one, byte for byte
    let normal = empyrean_world::network::game_messages::messages::game_message_create_object::game_message_create_object(&mut w, x, false, false);
    let _ = sent_opcodes();
    ask(&mut w, a, x);
    let answer = empyrean_world::network::game_messages::game_message::take_sent();
    assert_eq!(answer.len(), 1);
    assert_eq!(
        answer[0].2, normal.data,
        "byte-identical to a normal create"
    );
}

/// V288: an object the player was never sent is not answered, even in the player's own landblock
/// (ACE found it there and answered) or the next one.
#[test]
fn an_object_never_sent_is_not_answered_even_in_the_players_landblock() {
    let (mut w, x, [_a, b, _c]) = reach_scene();
    // D stands in x's landblock, 190 m from it: outside the create window
    let d = spawn_player(&mut w, P + 3, 4, at(HOME, 200.0, 100.0, 20.0));
    let hd = enter_landblock(&mut w, d);
    assert!(
        !om::known_objects_contains_key(&w, hd, x.full()),
        "D was never sent x"
    );
    let _ = sent_opcodes();
    ask(&mut w, d, x);
    ask(&mut w, b, x);
    ask(&mut w, d, ObjectGuid::new(0x7FFF_FFF0));
    assert!(sent_opcodes().is_empty(), "no answer at all");
    assert!(
        !om::known_objects_contains_key(&w, hd, x.full()),
        "and D still does not know it"
    );
}

/// V288: an object out of view is answered while still known (on the forget queue, which the
/// answer leaves as it was) and not once forgotten past 21 s.
#[test]
fn a_forgotten_object_is_not_answered() {
    let mut w = world_with(&[HOME, 0xAAB4, 0xACB4]);
    let x = spawn(
        &mut w,
        0x7000_0001,
        KindData::WorldObject,
        0,
        at(HOME, 110.0, 100.0, 20.0),
    );
    let hx = enter(&mut w, x);
    let p = spawn(&mut w, P, KindData::Player, 3, at(HOME, 100.0, 100.0, 20.0));
    let hp = enter(&mut w, p);
    let there = phys_ext::to_physics_position(&at(0xACB4, 100.0, 100.0, 20.0));
    assert!(phys_ext::set_position(&mut w, hp, &there));
    let queue = om::get_destruction_queue_copy(&w, hp);
    assert_eq!(queue, vec![(hx, 21.0)]);

    tick(&mut w, 10.0);
    let _ = creates();
    ask(&mut w, p, x);
    assert_eq!(creates(), 1, "10 s out of view: still known, answered");
    assert_eq!(
        om::get_destruction_queue_copy(&w, hp),
        queue,
        "the forget clock is untouched"
    );

    tick(&mut w, 12.0);
    assert_eq!(
        om::destroy_objects(&mut w, hp),
        vec![hx],
        "forgotten at 22 s"
    );
    let _ = creates();
    ask(&mut w, p, x);
    assert_eq!(creates(), 0, "forgotten: no answer");
    assert!(
        !om::known_objects_contains_key(&w, hp, x.full()),
        "and not known again"
    );
}

/// V288: the player's inventory and equipped items, the open container's contents, a known
/// creature's wielded item and a trade partner's offered item are answered; a wielded item of a
/// creature the player does not know is not.
#[test]
fn held_contained_wielded_and_traded_items_are_answered() {
    use empyrean_entity::enums::EquipMask;
    use empyrean_world::world_objects::{container, creature_equipment as ce};

    let mut w = world_with(&[HOME, 0xAAB4, 0xACB4]);
    let p = spawn_player(&mut w, P, 1, at(HOME, 100.0, 100.0, 20.0));
    let partner = spawn_player(&mut w, P + 1, 2, at(HOME, 20.0, 190.0, 20.0));
    let chest = spawn(
        &mut w,
        0x7000_0010,
        KindData::WorldObject,
        1,
        at(HOME, 101.0, 101.0, 20.0),
    );
    let near = spawn(
        &mut w,
        0x7000_0011,
        KindData::WorldObject,
        2,
        at(HOME, 99.0, 99.0, 20.0),
    );
    let far = spawn(
        &mut w,
        0x7000_0012,
        KindData::WorldObject,
        2,
        at(HOME, 180.0, 20.0, 20.0),
    );
    for g in [chest, near, far, p, partner] {
        enter_landblock(&mut w, g);
    }
    let hp = phys_ext::physics_obj(&w, p).expect("a body");
    assert!(om::known_objects_contains_key(&w, hp, near.full()));
    assert!(
        !om::known_objects_contains_key(&w, hp, far.full()),
        "far is outside the window"
    );
    for g in [p, partner, chest] {
        w.objects
            .get_mut(g)
            .expect("spawned")
            .set_item_capacity(Some(10));
    }

    let pack_item = loose_item(&mut w, 0x7000_0020, 0);
    assert!(container::try_add_to_inventory(
        &mut w, p, pack_item, 0, false, false
    ));
    let worn = loose_item(&mut w, 0x7000_0021, 0);
    assert!(ce::try_equip_object(
        &mut w,
        p,
        worn,
        EquipMask::MeleeWeapon
    ));
    let in_chest = loose_item(&mut w, 0x7000_0022, 0);
    assert!(container::try_add_to_inventory(
        &mut w, chest, in_chest, 0, false, false
    ));
    {
        let c = w.objects.get_mut(chest).expect("chest");
        c.set_is_open(true);
        c.set_viewer(p.full());
    }
    w.objects
        .get_mut(p)
        .and_then(|o| o.player.as_mut())
        .expect("a player")
        .player_use
        .last_opened_container_id = chest;
    let near_sword = loose_item(&mut w, 0x7000_0023, 0);
    assert!(ce::try_equip_object(
        &mut w,
        near,
        near_sword,
        EquipMask::MeleeWeapon
    ));
    let far_sword = loose_item(&mut w, 0x7000_0024, 0);
    assert!(ce::try_equip_object(
        &mut w,
        far,
        far_sword,
        EquipMask::MeleeWeapon
    ));
    let offered = loose_item(&mut w, 0x7000_0025, 0);
    assert!(container::try_add_to_inventory(
        &mut w, partner, offered, 0, false, false
    ));
    w.objects
        .get_mut(partner)
        .and_then(|o| o.player.as_mut())
        .expect("a player")
        .player_trade
        .items_in_trade_window
        .insert(offered);
    {
        let t = &mut w
            .objects
            .get_mut(p)
            .and_then(|o| o.player.as_mut())
            .expect("a player")
            .player_trade;
        t.is_trading = true;
        t.trade_partner = partner;
    }
    let queue = om::get_destruction_queue_copy(&w, hp);
    let known = om::get_known_objects_values(&w, hp);
    let _ = sent_opcodes();

    for (item, label) in [
        (pack_item, "inventory"),
        (worn, "equipped"),
        (in_chest, "the open container"),
        (near_sword, "wielded by a known creature"),
        (offered, "the trade partner's offer"),
    ] {
        ask(&mut w, p, item);
        let sent = sent_opcodes();
        assert_eq!(sent, vec![(1, 0xF745)], "{label}: one create to the asker");
    }
    ask(&mut w, p, far_sword);
    assert!(
        sent_opcodes().is_empty(),
        "wielded by an unknown creature: nothing"
    );
    assert_eq!(
        om::get_destruction_queue_copy(&w, hp),
        queue,
        "the forget queue is untouched"
    );
    assert_eq!(
        om::get_known_objects_values(&w, hp),
        known,
        "the known objects are untouched"
    );
}

#[test]
fn an_object_entering_near_a_player_is_created_for_that_player() {
    let mut w = world();
    let p = spawn(&mut w, P, KindData::Player, 3, at(HOME, 100.0, 100.0, 20.0));
    let hp = enter(&mut w, p);
    let _ = take_local();
    let _ = creates();

    let y = spawn(
        &mut w,
        0x7000_0005,
        KindData::WorldObject,
        0,
        at(HOME, 105.0, 100.0, 20.0),
    );
    let hy = enter(&mut w, y);
    assert_eq!(
        creates(),
        1,
        "enter_cell_server: the known player's handle_visible_obj"
    );
    assert_eq!(om::get_known_players_values(&w, hy), vec![hp]);
    assert_eq!(om::get_visible_objects_values(&w, hp), vec![hy]);

    // Destroying it removes it from the player's tables.
    phys_ext::destroy_object(&mut w, hy);
    assert_eq!(om::get_visible_objects_count(&w, hp), 0);
    assert_eq!(om::get_known_objects_count(&w, hp), 0);
}

/// `Player.Teleport` stamps `LastTeleportTime`; the player's `LastTeleportTime` within
/// `TeleportCreateObjectDelay` (1 s) of now.
fn just_teleported(w: &mut World, p: ObjectGuid) {
    let now = w.now.utc;
    w.objects
        .get_mut(p)
        .and_then(|o| o.player.as_mut())
        .expect("a player")
        .player_location
        .last_teleport_time = now;
}

/// The delay manager's due chains, then the player's queue (what a world tick runs).
fn run_due(w: &mut World, p: ObjectGuid) {
    empyrean_world::entity::actions::delay_manager::run_actions(w);
    empyrean_world::entity::actions::action_queue::run_actions(
        w,
        empyrean_world::entity::actions::i_actor::Actor::Object(p),
    );
}

#[test]
fn objects_seen_within_a_second_of_a_teleport_are_sent_a_second_later() {
    // enqueue_obj: an object entering near a player who teleported 0 s ago
    let mut w = world();
    let p = spawn(&mut w, P, KindData::Player, 3, at(HOME, 100.0, 100.0, 20.0));
    let hp = enter(&mut w, p);
    let _ = (take_local(), creates());
    just_teleported(&mut w, p);
    let y = spawn(
        &mut w,
        0x7000_0005,
        KindData::WorldObject,
        0,
        at(HOME, 105.0, 100.0, 20.0),
    );
    let hy = enter(&mut w, y);
    assert_eq!(
        om::get_visible_objects_values(&w, hp),
        vec![hy],
        "visible at once"
    );
    assert_eq!(creates(), 0, "the create waits TeleportCreateObjectDelay");
    tick(&mut w, 0.999);
    run_due(&mut w, p);
    assert_eq!(creates(), 0, "not before a second");
    tick(&mut w, 0.001);
    run_due(&mut w, p);
    assert_eq!(creates(), 1, "TrackObject(wo, true) a second later");

    // a second after the teleport, creates are immediate again
    w.now.utc = w.now.utc + empyrean_common::dotnet::datetime::TimeSpan::from_seconds(1.0);
    let z = spawn(
        &mut w,
        0x7000_0006,
        KindData::WorldObject,
        0,
        at(HOME, 95.0, 100.0, 20.0),
    );
    enter(&mut w, z);
    assert_eq!(
        creates(),
        1,
        "DateTime.UtcNow - LastTeleportTime == 1 s is not within the delay"
    );

    // enqueue_objs: a player arriving (its own cell change) among objects, just teleported
    let mut w = world();
    let x1 = spawn(
        &mut w,
        0x7000_0001,
        KindData::WorldObject,
        0,
        at(HOME, 110.0, 100.0, 20.0),
    );
    let x2 = spawn(
        &mut w,
        0x7000_0002,
        KindData::WorldObject,
        0,
        at(HOME, 90.0, 120.0, 20.0),
    );
    enter(&mut w, x1);
    enter(&mut w, x2);
    let p = spawn(&mut w, P, KindData::Player, 3, at(HOME, 100.0, 100.0, 20.0));
    just_teleported(&mut w, p);
    let _ = (take_local(), creates());
    enter(&mut w, p);
    assert_eq!(creates(), 0, "both wait");
    tick(&mut w, 1.0);
    run_due(&mut w, p);
    assert_eq!(creates(), 2, "one chain sends both a second later");
}

#[test]
fn a_teleporting_object_newly_visible_is_sent_one_tick_later() {
    let mut w = world();
    let p = spawn(&mut w, P, KindData::Player, 3, at(HOME, 100.0, 100.0, 20.0));
    enter(&mut w, p);
    let _ = (take_local(), creates());
    let y = spawn(
        &mut w,
        0x7000_0005,
        KindData::WorldObject,
        0,
        at(HOME, 105.0, 100.0, 20.0),
    );
    w.objects
        .get_mut(y)
        .expect("spawned")
        .wo
        .world_object
        .teleporting = true;
    enter(&mut w, y);
    assert_eq!(
        creates(),
        0,
        "ensure post-teleport position is sent: AddDelayForOneTick"
    );
    tick(&mut w, f64::from(0.001_f32));
    run_due(&mut w, p);
    assert_eq!(creates(), 1);

    // enqueue_objs: a player entering beside a teleporting object and a still one
    let mut w = world();
    let still = spawn(
        &mut w,
        0x7000_0001,
        KindData::WorldObject,
        0,
        at(HOME, 110.0, 100.0, 20.0),
    );
    let moving = spawn(
        &mut w,
        0x7000_0002,
        KindData::WorldObject,
        0,
        at(HOME, 90.0, 100.0, 20.0),
    );
    enter(&mut w, still);
    enter(&mut w, moving);
    w.objects
        .get_mut(moving)
        .expect("spawned")
        .wo
        .world_object
        .teleporting = true;
    let p = spawn(&mut w, P, KindData::Player, 3, at(HOME, 100.0, 100.0, 20.0));
    let _ = (take_local(), creates());
    enter(&mut w, p);
    assert_eq!(
        creates(),
        1,
        "the still object at once, the teleporting one next tick"
    );
    tick(&mut w, f64::from(0.001_f32));
    run_due(&mut w, p);
    assert_eq!(creates(), 1);
}

#[test]
fn a_monster_tracks_a_nearby_player_as_a_visible_target() {
    let mut w = world();
    let p = spawn(&mut w, P, KindData::Player, 3, at(HOME, 100.0, 100.0, 20.0));
    let hp = enter(&mut w, p);
    let m = spawn(
        &mut w,
        0x8000_0001,
        KindData::Creature,
        2,
        at(HOME, 108.0, 100.0, 20.0),
    );
    let hm = enter(&mut w, m);
    assert!(
        phys_ext::weenie_obj(&w, hm).is_monster,
        "attackable creature"
    );
    assert_eq!(om::get_visible_targets_values(&w, hm), vec![hp]);
    assert_eq!(om::get_known_players_values(&w, hm), vec![hp]);
    assert_eq!(
        om::get_visible_objects_values(&w, hp),
        vec![hm],
        "and the player sees the monster"
    );

    om::add_retaliate_target(&mut w, hm, hp);
    assert_eq!(om::get_retaliate_targets_count(&w, hm), 1);
    om::clear_retaliate_targets(&mut w, hm);
    assert_eq!(om::get_retaliate_targets_count(&w, hm), 0);
    assert_eq!(
        om::get_visible_targets_count(&w, hm),
        0,
        "ClearRetaliateTargets also removes them from VisibleTargets"
    );
}

/// Not ACE's (V308, owner 2026-09-24, a fix): past 112.5 m a monster targets a player that
/// already knows it (it was created for the player), and not one that does not.
#[test]
fn past_the_initial_clamp_a_monster_targets_only_a_player_that_knows_it() {
    let mut w = world();
    let p = spawn(&mut w, P, KindData::Player, 3, at(HOME, 96.5, 96.5, 20.0));
    let hp = enter(&mut w, p);

    // in the player's window past 112.5 m (the geometry of the create-set test below): created
    let known = spawn(
        &mut w,
        0x8000_0001,
        KindData::Creature,
        2,
        at(HOME, 191.5, 167.5, 20.0),
    );
    let hk = enter(&mut w, known);
    // outside the window, 122 m east: never created
    let unknown = spawn(
        &mut w,
        0x8000_0002,
        KindData::Creature,
        2,
        at(0xAAB4, 26.5, 96.5, 20.0),
    );
    let hu = enter(&mut w, unknown);
    for h in [hk, hu] {
        let d = phys_ext::distance_2d_squared(
            &phys_ext::position(&w, hp).unwrap(),
            &phys_ext::position(&w, h).unwrap(),
        )
        .sqrt();
        assert!(d > 112.5, "{d} m");
    }
    assert!(
        om::known_objects_contains_key(&w, hp, known.full()),
        "created for the player"
    );
    assert!(
        !om::known_objects_contains_key(&w, hp, unknown.full()),
        "not created"
    );

    assert!(
        om::get_visible_targets_values(&w, hk).is_empty(),
        "the player learns of it after it looks for targets"
    );
    assert_eq!(om::add_visible_targets(&mut w, hk, &[hp]), vec![hp]);
    assert_eq!(
        om::get_visible_targets_values(&w, hk),
        vec![hp],
        "known: past the clamp"
    );
    assert_eq!(
        om::add_visible_targets(&mut w, hu, &[hp]),
        Vec::<PhysHandle>::new()
    );
    assert!(
        om::get_visible_targets_values(&w, hu).is_empty(),
        "unknown: held back by the clamp"
    );
}

#[test]
fn outdoor_visibility_is_one_landblock() {
    let w = world();
    assert!(phys_ext::is_visible(
        &w,
        CellId(0xA9B4_0001),
        CellId(0xAAB5_0010)
    ));
    assert!(!phys_ext::is_visible(
        &w,
        CellId(0xA9B4_0001),
        CellId(0xABB4_0010)
    ));
    assert_eq!(phys_ext::get_block_dist(0xA9B4_0001, 0xA7B6_0001), 2);
}

// ------------------------------------------------------------ the create set (V260/V278/V286/V287/V308 stage 2b)

/// A physics position in outdoor cell `(x / 24, y / 24)` of `block`, at landblock-relative (x, y).
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn land_pos(block: u16, x: f32, y: f32) -> dereth_primitives::Position {
    let index = (x / 24.0) as u32 * 8 + (y / 24.0) as u32 + 1;
    dereth_primitives::Position::new(
        CellId(u32::from(block) << 16 | index),
        dereth_primitives::Frame::new(Vec3::new(x, y, 20.0), dereth_primitives::Quat::IDENTITY),
    )
}

/// A physics position in interior cell `index` of `block`, at landblock-relative (x, y).
fn cell_pos(block: u16, index: u32, x: f32, y: f32) -> dereth_primitives::Position {
    dereth_primitives::Position::new(
        CellId(u32::from(block) << 16 | index),
        dereth_primitives::Frame::new(Vec3::new(x, y, 20.0), dereth_primitives::Quat::IDENTITY),
    )
}

/// The global outdoor cell (x, y) as a position at the cell's centre.
#[allow(clippy::cast_precision_loss)]
fn global_cell_pos(gx: i32, gy: i32) -> dereth_primitives::Position {
    let (bx, by) = (gx.div_euclid(8), gy.div_euclid(8));
    let block = u16::try_from(bx * 256 + by).expect("in the map");
    land_pos(
        block,
        (gx.rem_euclid(8) * 24 + 12) as f32,
        (gy.rem_euclid(8) * 24 + 12) as f32,
    )
}

/// V260/V278/V286/V287/V308 stage 2b (the retail pcaps): outdoors, the create set is the round cell window
/// dx² + dy² ≤ 13 over global cell coordinates: 45 cells, the (±3, ±2) cells in, the (±3, ±3)
/// corners and (0, ±4) out, the same across a landblock edge as inside one.
#[test]
fn the_outdoor_create_set_is_the_round_cell_window_across_landblock_edges() {
    let land = dereth_physics::StaticLandSource::linear();
    // observers in the middle of a landblock and in the corner cell next to three others
    for (gx, gy) in [
        (0xA9 * 8 + 4, 0xB4 * 8 + 4),
        (0xA9 * 8 + 7, 0xB4 * 8 + 7),
        (0xAA * 8, 0xB4 * 8),
    ] {
        let me = global_cell_pos(gx, gy);
        let mut inside = 0;
        for dx in -5..=5 {
            for dy in -5..=5 {
                let there = global_cell_pos(gx + dx, gy + dy);
                let want = dx * dx + dy * dy <= 13;
                assert_eq!(
                    om::create_set_contains(&land, &me, &there),
                    want,
                    "({dx}, {dy}) from ({gx}, {gy})"
                );
                inside += usize::from(want);
            }
        }
        assert_eq!(inside, 45);
    }
    // named cases from the east edge cell (7, 4) of A9B4
    let me = land_pos(HOME, 180.0, 100.0);
    assert!(
        om::create_set_contains(&land, &me, &land_pos(0xAAB4, 60.0, 100.0)),
        "3 cells east, across the edge: in"
    );
    assert!(
        !om::create_set_contains(&land, &me, &land_pos(0xAAB4, 84.0, 100.0)),
        "4 cells east: out"
    );
    assert!(
        !om::create_set_contains(&land, &me, &land_pos(0xAAB4, 60.0, 172.0)),
        "the (3, 3) corner across the edge: out"
    );
    assert!(
        om::create_set_contains(&land, &me, &land_pos(0xAAB4, 60.0, 148.0)),
        "(3, 2): in"
    );
    assert!(
        !om::create_set_contains(&land, &me, &land_pos(HOME, 180.0, 4.0)),
        "(0, -4), 96 m: out"
    );
}

/// A test land with interiors: in HOME a room seen from outside (0x100, whose visible cells
/// include the cellar), a cellar under it (0x101, not seen outside, seeing the room), and a
/// dungeon run (0x102 sees 0x103, not 0x104); rooms seen from outside in the four neighbours of
/// HOME, its four diagonals and a landblock two east.
fn interiors() -> dereth_physics::StaticLandSource {
    let mut land = dereth_physics::StaticLandSource::linear();
    let cell = |id: u32, seen_outside: bool, stab: &[u32]| dereth_physics::EnvCellGeometry {
        id: CellId(id),
        seen_outside,
        stab_list: stab.iter().map(|&s| CellId(s)).collect(),
        ..Default::default()
    };
    land.add_cell(cell(0xA9B4_0100, true, &[0xA9B4_0101]));
    land.add_cell(cell(0xA9B4_0101, false, &[0xA9B4_0100]));
    land.add_cell(cell(0xA9B4_0102, false, &[0xA9B4_0103]));
    land.add_cell(cell(0xA9B4_0103, false, &[0xA9B4_0102]));
    land.add_cell(cell(0xA9B4_0104, false, &[]));
    for block in [
        0xAAB4_u32, 0xA8B4, 0xA9B5, 0xA9B3, 0xAAB5, 0xA8B3, 0xAAB3, 0xA8B5, 0xABB4,
    ] {
        land.add_cell(cell(block << 16 | 0x100, true, &[]));
    }
    land
}

/// V260/V278/V286/V287/V308 stage 2b (V287, the retail pcaps): a room seen from outside is created from its own
/// landblock and the four edge neighbours from anywhere, from a diagonal only when the observer's
/// cell index faces it on at least one axis (4 or more toward +, 4 or less toward −), never from
/// two landblocks away.
#[test]
fn rooms_seen_outside_are_created_at_landblock_reach_with_the_diagonal_near_side_test() {
    let land = interiors();
    let room = |block: u16| cell_pos(block, 0x100, 96.0, 96.0);
    // the far south-west corner cell (0, 0) of HOME: own block and the edge neighbours
    let corner = land_pos(HOME, 1.0, 1.0);
    for block in [HOME, 0xAAB4, 0xA8B4, 0xA9B5, 0xA9B3] {
        assert!(
            om::create_set_contains(&land, &corner, &room(block)),
            "{block:04X} from anywhere"
        );
    }
    assert!(
        !om::create_set_contains(&land, &corner, &room(0xABB4)),
        "two landblocks east: never"
    );

    // the +x +y diagonal (AAB5), by the observer's cell index (cx, cy)
    for (cx, cy, want) in [
        (4, 0, true),
        (0, 4, true),
        (7, 7, true),
        (3, 3, false),
        (0, 0, false),
        (3, 7, true),
        (7, 3, true),
    ] {
        #[allow(clippy::cast_precision_loss)]
        let me = land_pos(HOME, (cx * 24 + 12) as f32, (cy * 24 + 12) as f32);
        assert_eq!(
            om::create_set_contains(&land, &me, &room(0xAAB5)),
            want,
            "+x +y diagonal from cell ({cx}, {cy})"
        );
    }
    // the −x −y diagonal (A8B3): 4 or less faces it
    for (cx, cy, want) in [
        (4, 7, true),
        (7, 4, true),
        (5, 5, false),
        (7, 7, false),
        (0, 0, true),
    ] {
        #[allow(clippy::cast_precision_loss)]
        let me = land_pos(HOME, (cx * 24 + 12) as f32, (cy * 24 + 12) as f32);
        assert_eq!(
            om::create_set_contains(&land, &me, &room(0xA8B3)),
            want,
            "−x −y diagonal from cell ({cx}, {cy})"
        );
    }
    // mixed: the +x −y diagonal (AAB3) from (5, 5): faces it on x
    assert!(om::create_set_contains(
        &land,
        &land_pos(HOME, 132.0, 132.0),
        &room(0xAAB3)
    ));
    assert!(
        !om::create_set_contains(&land, &land_pos(HOME, 84.0, 132.0), &room(0xAAB3)),
        "(3, 5): neither axis"
    );

    // an observer in a room seen from outside sees rooms at the same reach, and the outdoors
    // through the round window
    let in_room = cell_pos(HOME, 0x100, 100.0, 100.0);
    assert!(om::create_set_contains(&land, &in_room, &room(0xA9B5)));
    assert!(
        om::create_set_contains(&land, &in_room, &land_pos(HOME, 148.0, 100.0)),
        "outdoors, 2 cells east"
    );
    assert!(
        !om::create_set_contains(&land, &in_room, &land_pos(HOME, 4.0, 100.0)),
        "outdoors, 4 cells west"
    );
}

/// V260/V278/V286/V287/V308 stage 2b (V287, the retail pcaps): a cellar (a room not seen from outside) is never
/// created for an observer outdoors, however near (retail: none at 10-54 m); from inside, a cellar
/// or dungeon cell sees its own cell and its visible cells at any distance, and nothing else.
#[test]
fn cellars_and_dungeons_follow_the_visible_cell_list() {
    let land = interiors();
    let cellar_npc = cell_pos(HOME, 0x101, 100.0, 100.0);
    for x in [110.0, 125.0, 154.0] {
        assert!(
            !om::create_set_contains(&land, &land_pos(HOME, x, 100.0), &cellar_npc),
            "outdoors {} m away",
            x - 100.0
        );
    }
    assert!(
        om::create_set_contains(&land, &cell_pos(HOME, 0x100, 110.0, 100.0), &cellar_npc),
        "from the room above: on its list"
    );
    // from the cellar: the room (on its list), not the outdoors 10 m away
    let in_cellar = cell_pos(HOME, 0x101, 110.0, 100.0);
    assert!(om::create_set_contains(
        &land,
        &in_cellar,
        &cell_pos(HOME, 0x100, 100.0, 100.0)
    ));
    assert!(!om::create_set_contains(
        &land,
        &in_cellar,
        &land_pos(HOME, 120.0, 100.0)
    ));
    assert!(
        !om::create_set_contains(&land, &in_cellar, &cell_pos(0xAAB4, 0x100, 96.0, 96.0)),
        "not a neighbour's room"
    );

    // a dungeon observer: its own cell and the visible cells, with no distance cap
    let me = cell_pos(HOME, 0x102, 10.0, 10.0);
    assert!(
        om::create_set_contains(&land, &me, &cell_pos(HOME, 0x102, 180.0, 180.0)),
        "own cell"
    );
    assert!(
        om::create_set_contains(&land, &me, &cell_pos(HOME, 0x103, 110.0, 110.0)),
        "on the list, 141 m away: no cap"
    );
    assert!(
        !om::create_set_contains(&land, &me, &cell_pos(HOME, 0x104, 12.0, 10.0)),
        "off the list, 2 m away"
    );
    assert!(
        !om::create_set_contains(&land, &me, &land_pos(HOME, 12.0, 10.0)),
        "not the outdoors"
    );
}

/// V260/V278/V286/V287/V308 stage 2b (the retail pcaps): ACE's 112.5 m first-create clamp is gone for players; an
/// outdoor object in the window 120 m away is created (retail created (3, 2) cells at 104-112 m).
/// The visible objects (ACE's landblock view, which the monsters read) keep the clamp.
#[test]
fn an_object_in_the_window_beyond_112_5_m_is_created() {
    let mut w = world();
    // the player at the south-west corner of cell (4, 4), the object at the far corner of (7, 6)
    let x = spawn(
        &mut w,
        0x7000_0001,
        KindData::WorldObject,
        0,
        at(HOME, 191.5, 167.5, 20.0),
    );
    let hx = enter(&mut w, x);
    let _ = take_local();
    let p = spawn(&mut w, P, KindData::Player, 3, at(HOME, 96.5, 96.5, 20.0));
    let hp = enter(&mut w, p);
    let d = phys_ext::distance_2d_squared(
        &phys_ext::position(&w, hp).unwrap(),
        &phys_ext::position(&w, hx).unwrap(),
    )
    .sqrt();
    assert!(d > 115.0, "{d} m");
    assert_eq!(creates(), 1, "created at {d} m");
    assert_eq!(om::get_known_players_values(&w, hx), vec![hp]);
    assert!(
        om::get_visible_objects_values(&w, hp).is_empty(),
        "ACE's view: held back by the clamp"
    );
}

/// V260/V278/V286/V287/V308 stage 2b (the retail pcaps): an object walking into a standing player's window is
/// created for it, whether or not it knew the player; walking out starts the 21 s forget.
#[test]
fn an_object_walking_into_a_standing_players_window_is_created() {
    let mut w = world();
    let p = spawn(&mut w, P, KindData::Player, 3, at(HOME, 100.0, 100.0, 20.0));
    let hp = enter(&mut w, p);
    // 4 cells west of the player's cell (4, 4): out of the window, never created
    let x = spawn(
        &mut w,
        0x7000_0001,
        KindData::WorldObject,
        0,
        at(HOME, 4.0, 100.0, 20.0),
    );
    let hx = enter(&mut w, x);
    let _ = take_local();
    assert_eq!(creates(), 0);
    assert!(
        om::get_known_players_values(&w, hx).is_empty(),
        "nobody knows it"
    );

    // it steps one cell east: (-3, 0), in
    assert!(phys_ext::set_position(
        &mut w,
        hx,
        &phys_ext::to_physics_position(&at(HOME, 28.0, 100.0, 20.0))
    ));
    assert_eq!(creates(), 1, "created as it enters the window");
    assert_eq!(om::get_known_players_values(&w, hx), vec![hp]);

    // and back out: queued, not forgotten yet; back in before 21 s: kept, no create
    assert!(phys_ext::set_position(
        &mut w,
        hx,
        &phys_ext::to_physics_position(&at(HOME, 4.0, 100.0, 20.0))
    ));
    assert_eq!(
        om::get_destruction_queue_copy(&w, hp),
        vec![(hx, 21.0)],
        "the forget clock starts at the window edge"
    );
    tick(&mut w, 10.0);
    assert!(phys_ext::set_position(
        &mut w,
        hx,
        &phys_ext::to_physics_position(&at(HOME, 28.0, 100.0, 20.0))
    ));
    assert_eq!(creates(), 0);
    assert_eq!(om::get_destruction_queue_count(&w, hp), 0);

    // out for more than 21 s: forgotten, and a return is a create
    assert!(phys_ext::set_position(
        &mut w,
        hx,
        &phys_ext::to_physics_position(&at(HOME, 4.0, 100.0, 20.0))
    ));
    tick(&mut w, 21.5);
    assert!(phys_ext::set_position(
        &mut w,
        hx,
        &phys_ext::to_physics_position(&at(HOME, 28.0, 100.0, 20.0))
    ));
    assert_eq!(creates(), 1, "created again");
}

/// V260/V278/V286/V287/V308 stage 2b: the monsters' side is ACE's. A monster 100 m from a player (inside ACE's
/// 112.5 m and the 3×3, outside the player's window) still targets the player, and the player's
/// landblock view holds it for the wake-up check; the player is not sent it.
#[test]
fn a_monster_outside_the_window_still_targets_and_is_in_the_players_view() {
    let mut w = world();
    let p = spawn(&mut w, P, KindData::Player, 3, at(HOME, 100.0, 100.0, 20.0));
    let hp = enter(&mut w, p);
    let _ = (take_local(), creates());
    let m = spawn(
        &mut w,
        0x8000_0001,
        KindData::Creature,
        2,
        at(0xAAB4, 8.0, 100.0, 20.0),
    );
    let hm = enter(&mut w, m);
    assert!(phys_ext::weenie_obj(&w, hm).is_monster);
    assert_eq!(
        om::get_visible_targets_values(&w, hm),
        vec![hp],
        "ACE's targeting"
    );
    assert_eq!(
        om::get_visible_objects_values(&w, hp),
        vec![hm],
        "ACE's view (wake-ups, cleave)"
    );
    assert!(
        !om::known_objects_contains_key(&w, hp, m.full()),
        "4 cells east: not created"
    );
    assert!(
        om::get_known_players_values(&w, hm).is_empty(),
        "no known player without a create"
    );
    assert_eq!(creates(), 0);
}

#[test]
fn server_object_manager_adds_and_removes_by_id() {
    let mut w = world();
    let g = spawn(
        &mut w,
        0x7000_0001,
        KindData::WorldObject,
        0,
        at(HOME, 100.0, 100.0, 20.0),
    );
    dispatch::init_physics_obj::init_physics_obj(&mut w, g);
    let h = phys_ext::physics_obj(&w, g).expect("body");
    assert_eq!(server_object_manager::get_object_a(&w, g.full()), Some(h));
    server_object_manager::remove_server_object(&mut w, h);
    assert_eq!(server_object_manager::get_object_a(&w, g.full()), None);
    server_object_manager::add_server_object(&mut w, h);
    assert_eq!(server_object_manager::get_object_a(&w, g.full()), Some(h));
}

// ---------------------------------------------------------------------------------- Trajectory (ACE vectors)

fn f64_in(v: &Value, k: &str) -> f64 {
    f64_of(&v[k]).unwrap_or_else(|| panic!("{k} in {v}"))
}
fn f32_in(v: &Value, k: &str) -> f32 {
    f32_of(&v[k]).unwrap_or_else(|| panic!("{k} in {v}"))
}
fn v3(v: &Value) -> Vector3 {
    let a = v.as_array().expect("[x, y, z]");
    Vector3::new(
        f32_of(&a[0]).expect("x"),
        f32_of(&a[1]).expect("y"),
        f32_of(&a[2]).expect("z"),
    )
}
fn same_v3(a: Vector3, b: Vector3) -> bool {
    same_f32(a.x, b.x) && same_f32(a.y, b.y) && same_f32(a.z, b.z)
}

/// Replays a vector file, collecting every mismatch.
fn replay(name: &str, mut check: impl FnMut(&Case) -> Option<String>) {
    let file = vectors::load_named("physics", name);
    let bad: Vec<String> = file.cases.iter().filter_map(&mut check).collect();
    assert!(
        bad.is_empty(),
        "physics/{name}: {} of {} cases differ; first: {}",
        bad.len(),
        file.cases.len(),
        bad[0]
    );
}

/// V317: a quartic with two real roots (a target jumping up below the
/// shooter) answers a real launch velocity and flight time; ACE sorted the NaN placeholders of the
/// missing roots first and answered NaN.
#[test]
fn the_moving_target_solver_reads_only_real_roots() {
    let arc = trajectory::solve_ballistic_arc_moving(
        Vector3::new(0.0, 0.0, 1.0),
        16.440775,
        Vector3::new(-2.44293, -4.5257187, -3.4672399),
        Vector3::new(-5.0403285, 7.183382, 7.67848),
        9.8,
    );
    assert!(arc.num > 0, "{arc:?}");
    assert!(arc.time.is_finite() && arc.time > 0.0, "{arc:?}");
    assert!(
        arc.s0.x.is_finite() && arc.s0.y.is_finite() && arc.s0.z.is_finite(),
        "{arc:?}"
    );
    // the answer hits the target: where the shot is at `time` is where the target is
    let t = arc.time;
    let shot = Vector3::new(0.0, 0.0, 1.0) + arc.s0 * t - Vector3::new(0.0, 0.0, 0.5 * 9.8 * t * t);
    let target = Vector3::new(-2.44293, -4.5257187, -3.4672399)
        + Vector3::new(-5.0403285, 7.183382, 7.67848) * t;
    let miss = shot - target;
    assert!(
        miss.x.abs() < 0.01 && miss.y.abs() < 0.01 && miss.z.abs() < 0.01,
        "{shot:?} vs {target:?}"
    );
}

#[test]
fn trajectory_root_solvers_match_ace() {
    replay("trajectory_solve_quadric", |c| {
        let i = &c.input;
        let r = trajectory::solve_quadric(f64_in(i, "c0"), f64_in(i, "c1"), f64_in(i, "c2"));
        let o = &c.output;
        let ok = i64_of(&o["num"]) == Some(i64::from(r.num))
            && same_f64(r.s0, f64_in(o, "s0"))
            && same_f64(r.s1, f64_in(o, "s1"));
        (!ok).then(|| format!("{i} -> {r:?}, ACE {o}"))
    });
    replay("trajectory_solve_cubic", |c| {
        let i = &c.input;
        let cs = [
            f64_in(i, "c0"),
            f64_in(i, "c1"),
            f64_in(i, "c2"),
            f64_in(i, "c3"),
        ];
        let r = trajectory::solve_cubic(cs[0], cs[1], cs[2], cs[3]);
        let o = &c.output;
        let root = |got: f64, k: &str| same_root_on_host(got, f64_in(o, k), &cs);
        let ok = i64_of(&o["num"]) == Some(i64::from(r.num))
            && root(r.s0, "s0")
            && root(r.s1, "s1")
            && root(r.s2, "s2");
        (!ok).then(|| format!("{i} -> {r:?}, ACE {o}"))
    });
    replay("trajectory_solve_quartic", |c| {
        let i = &c.input;
        let cs = [
            f64_in(i, "c0"),
            f64_in(i, "c1"),
            f64_in(i, "c2"),
            f64_in(i, "c3"),
            f64_in(i, "c4"),
        ];
        let r = trajectory::solve_quartic(cs[0], cs[1], cs[2], cs[3], cs[4]);
        let o = &c.output;
        let root = |got: f64, k: &str| same_root_on_host(got, f64_in(o, k), &cs);
        let ok = i64_of(&o["num"]) == Some(i64::from(r.num))
            && root(r.s0, "s0")
            && root(r.s1, "s1")
            && root(r.s2, "s2")
            && root(r.s3, "s3");
        (!ok).then(|| format!("{i} -> {r:?}, ACE {o}"))
    });
}

/// Same root on host.
fn same_root_on_host(got: f64, want: f64, coefficients: &[f64]) -> bool {
    if same_f64(got, want) {
        return true;
    }
    if (cfg!(windows) && !empyrean_common::math::PORTABLE) || got.is_nan() || want.is_nan() {
        return false;
    }
    let scale = (1..coefficients.len())
        .map(|k| {
            (coefficients[k] / coefficients[0])
                .abs()
                .powf(1.0 / k as f64)
        })
        .fold(got.abs().max(want.abs()), f64::max);
    (got - want).abs() <= 16.0 * f64::EPSILON * scale
}

#[test]
fn trajectory_ballistic_solvers_match_ace() {
    replay("trajectory_ballistic_range", |c| {
        let i = &c.input;
        let r = trajectory::ballistic_range(
            f32_in(i, "speed"),
            f32_in(i, "gravity"),
            f32_in(i, "initial_height"),
        );
        (!same_f32(r, f32_of(&c.output).expect("float")))
            .then(|| format!("{i} -> {r}, ACE {}", c.output))
    });
    replay("trajectory_solve_ballistic_arc", |c| {
        let i = &c.input;
        let r = trajectory::solve_ballistic_arc(
            v3(&i["proj_pos"]),
            f32_in(i, "proj_speed"),
            v3(&i["target"]),
            f32_in(i, "gravity"),
        );
        let o = &c.output;
        let ok = i64_of(&o["num"]) == Some(i64::from(r.num))
            && same_v3(r.s0, v3(&o["s0"]))
            && same_v3(r.s1, v3(&o["s1"]))
            && same_f32(r.t0, f32_in(o, "t0"))
            && same_f32(r.t1, f32_in(o, "t1"));
        (!ok).then(|| format!("{i} -> {r:?}, ACE {o}"))
    });
    replay("trajectory_solve_ballistic_arc_moving", |c| {
        let i = &c.input;
        let r = trajectory::solve_ballistic_arc_moving(
            v3(&i["proj_pos"]),
            f32_in(i, "proj_speed"),
            v3(&i["target_pos"]),
            v3(&i["target_velocity"]),
            f32_in(i, "gravity"),
        );
        let o = &c.output;
        // V317 (V317): where ACE answered its NaN placeholder (a target on the shooter's own
        // point), the real positive root is answered instead, or no solution when the only root
        // is t = 0 (a target outrunning the projectile)
        if f32_in(o, "time").is_nan() {
            let ok = (r.num == 0 && r.time == 0.0)
                || (r.num == 1
                    && r.time.is_finite()
                    && r.time > 0.0
                    && r.s0.x.is_finite()
                    && r.s0.z.is_finite());
            return (!ok).then(|| format!("{i} -> {r:?}, ACE {o} (a real root or none expected)"));
        }
        let ok = i64_of(&o["num"]) == Some(i64::from(r.num))
            && same_v3(r.s0, v3(&o["s0"]))
            && same_v3(r.s1, v3(&o["s1"]))
            && same_f32(r.time, f32_in(o, "time"));
        (!ok).then(|| format!("{i} -> {r:?}, ACE {o}"))
    });
    replay("trajectory_solve_ballistic_arc_fixed", |c| {
        let i = &c.input;
        let r = trajectory::solve_ballistic_arc_fixed(
            v3(&i["projectile_position"]),
            f32_in(i, "lateral_speed"),
            v3(&i["target_position"]),
        );
        let o = &c.output;
        let ok = o["ok"].as_bool() == Some(r.ok)
            && same_v3(r.velocity_vector, v3(&o["velocity_vector"]))
            && same_f32(r.time, f32_in(o, "time"));
        (!ok).then(|| format!("{i} -> {r:?}, ACE {o}"))
    });
    replay("trajectory_solve_ballistic_arc_lateral", |c| {
        let i = &c.input;
        let r = trajectory::solve_ballistic_arc_lateral(
            v3(&i["proj_pos"]),
            f32_in(i, "lateral_speed"),
            v3(&i["target_pos"]),
            f32_in(i, "max_height"),
        );
        let o = &c.output;
        let ok = o["ok"].as_bool() == Some(r.ok)
            && same_v3(r.fire_velocity, v3(&o["fire_velocity"]))
            && same_f32(r.gravity, f32_in(o, "gravity"));
        (!ok).then(|| format!("{i} -> {r:?}, ACE {o}"))
    });
    replay("trajectory_solve_ballistic_arc_lateral_moving", |c| {
        let i = &c.input;
        let r = trajectory::solve_ballistic_arc_lateral_moving(
            v3(&i["proj_pos"]),
            f32_in(i, "lateral_speed"),
            v3(&i["target"]),
            v3(&i["target_velocity"]),
            f32_in(i, "gravity"),
        );
        let o = &c.output;
        let ok = o["ok"].as_bool() == Some(r.ok)
            && same_v3(r.fire_velocity, v3(&o["fire_velocity"]))
            && same_v3(r.impact_point, v3(&o["impact_point"]))
            && same_f32(r.time, f32_in(o, "time"));
        (!ok).then(|| format!("{i} -> {r:?}, ACE {o}"))
    });
}

#[test]
fn trajectory2_matches_ace() {
    replay("trajectory2_calculate_trajectory", |c| {
        let i = &c.input;
        let r = trajectory2::calculate_trajectory(
            v3(&i["start_pos"]),
            v3(&i["end_pos"]),
            v3(&i["target_velocity"]),
            f32_in(i, "speed"),
            i["gravity"].as_bool().expect("bool"),
        );
        (!same_v3(r, v3(&c.output))).then(|| format!("{i} -> {r:?}, ACE {}", c.output))
    });
}

mod motion {
    use super::*;
    use dereth_assets::geometry::AnimFrame;
    use dereth_assets::motion::{AnimData, MotionData};
    use dereth_assets::{AnimHook, Animation, HookData, MotionTable};
    use dereth_primitives::{DataId, Frame, Quat};
    use std::collections::BTreeMap;

    pub const MT: u32 = 0x0900_0100;
    const NONCOMBAT: u32 = 0x8000_003D;
    const READY: u32 = 0x4100_0003;
    const WALK: u32 = 0x4500_0005;
    const RUN: u32 = 0x4400_0007;
    const SWORD: u32 = 0x8000_0040;
    const ATTACK: u32 = 0x1000_0062;
    const A: u32 = 0x0300_0001; // 15 frames, attack hook at frame 5
    const B: u32 = 0x0300_0002; // 10 frames
    const C: u32 = 0x0300_0003; // 10 position frames of 1 m along +y

    fn anim(anim_id: u32, low_frame: i32, high_frame: i32, framerate: f32) -> AnimData {
        AnimData {
            anim_id: DataId(anim_id),
            low_frame,
            high_frame,
            framerate,
        }
    }

    fn data(key: u32, anims: Vec<AnimData>) -> MotionData {
        MotionData {
            key,
            bitfield: 0,
            flags: 0,
            anims,
            velocity: None,
            omega: None,
        }
    }

    fn animation(id: u32, n: u32, hook_at: Option<usize>, pos: bool) -> Animation {
        let part_frames = (0..n as usize)
            .map(|i| AnimFrame {
                frames: Vec::new(),
                hooks: if hook_at == Some(i) {
                    vec![AnimHook {
                        hook_type: 3,
                        direction: 0,
                        data: HookData::Attack {
                            part_index: 7,
                            left: (0.0, 0.0),
                            right: (0.0, 0.0),
                            radius: 1.0,
                            height: 1.0,
                        },
                    }]
                } else {
                    Vec::new()
                },
            })
            .collect();
        let pos_frames =
            pos.then(|| vec![Frame::new(Vec3::new(0.0, 1.0, 0.0), Quat::IDENTITY); n as usize]);
        Animation {
            id: DataId(id),
            flags: u32::from(pos),
            num_parts: 0,
            num_frames: n,
            has_hooks: hook_at.is_some(),
            pos_frames,
            part_frames,
        }
    }

    pub fn world() -> World {
        let nc = NONCOMBAT << 16;
        let mut links = BTreeMap::new();
        // from Ready in NonCombat
        links.insert(
            nc | (READY & 0xFFFFF),
            vec![
                data(WALK, vec![anim(A, 0, -1, 30.0), anim(B, 2, 12, -20.0)]),
                data(ATTACK, vec![anim(A, 0, -1, 30.0)]),
                data(SWORD, vec![anim(B, 0, -1, 10.0)]),
            ],
        );
        // from WalkForward back to Ready
        links.insert(
            nc | (WALK & 0xFFFFF),
            vec![data(READY, vec![anim(B, 0, 5, 10.0)])],
        );
        // the stance's catch-all link
        links.insert(nc, vec![data(0x1300_0080, vec![anim(B, 0, -1, 40.0)])]);
        let table = MotionTable {
            id: DataId(MT),
            default_style: NONCOMBAT,
            style_defaults: BTreeMap::from([(NONCOMBAT, READY), (SWORD, READY)]),
            cycles: vec![
                data(nc | (READY & 0xFFFFFF), vec![anim(B, 0, -1, 30.0)]),
                data(nc | (WALK & 0xFFFFF), vec![anim(B, 0, -1, 20.0)]),
                data(nc | (RUN & 0xFFFFFF), vec![anim(C, 0, -1, 30.0)]),
                data(
                    (SWORD << 16) | (READY & 0xFFFFFF),
                    vec![anim(B, 0, -1, 30.0)],
                ),
            ],
            modifiers: Vec::new(),
            links,
        };
        let dats = empyrean_testkit::dats::with_stat_tables(FakeDats::new())
            .with_portal(MT, table)
            .with_portal(A, animation(A, 15, Some(5), false))
            .with_portal(B, animation(B, 10, None, false))
            .with_portal(C, animation(C, 10, None, true))
            .build()
            .expect("fake dats");
        World::new(snapshot(0.0), dats)
    }

    fn stance() -> MotionStance {
        MotionStance(NONCOMBAT)
    }

    /// V352 (retail): a motion lasts as long as the client plays it. B, frames 2..12 at -20/s, is
    /// clamped to its last frame (9) and played backwards from just under 10 down to 2 (ACE: 8
    /// frames, 0.4 s).
    #[test]
    fn animation_length_is_the_clients_playback_with_its_clamp_and_reverse_rates() {
        let w = world();
        // A: 15 frames at 30/s = 0.5; B: 7.9998 frames at 20/s
        let walk = 0.5 + (9.9998 - 2.0) / 20.0;
        let len = mt::get_animation_length(&w, MT, stance(), MotionCommand(WALK), 1.0);
        assert!((len - walk).abs() < 1e-5, "{len}");
        let len = mt::get_animation_length(&w, MT, stance(), MotionCommand(WALK), 2.0);
        assert!((len - walk / 2.0).abs() < 1e-5, "{len}");
        assert_eq!(
            mt::get_animation_length(&w, 0, stance(), MotionCommand(WALK), 1.0),
            0.0,
            "table 0"
        );
        assert_eq!(
            mt::get_animation_length(&w, 0x0900_0999, stance(), MotionCommand(WALK), 1.0),
            0.0,
            "a missing table is empty"
        );
        // not in the Ready link: the stance's catch-all link, B whole at 40/s
        assert_eq!(
            mt::get_animation_length(&w, MT, stance(), MotionCommand(0x1300_0080), 1.0),
            0.25
        );
    }

    /// V352 (retail): a style change from a motion other than the style's default goes through
    /// the default first, as the client plays it.
    #[test]
    fn a_style_change_from_a_non_ready_motion_goes_through_ready() {
        let w = world();
        // WalkForward -> Ready: B frames 0..5 at 10/s = 0.6 (six frames; ACE 0.5); Ready -> Sword: B
        // whole at 10/s = 1.0
        let len = mt::get_animation_length_between(
            &w,
            MT,
            stance(),
            MotionCommand(WALK),
            MotionCommand(SWORD),
            1.0,
        );
        assert!((len - 1.6).abs() < 1e-6, "{len}");
        let from_ready = mt::get_animation_length_between(
            &w,
            MT,
            stance(),
            MotionCommand::Ready,
            MotionCommand(SWORD),
            1.0,
        );
        assert_eq!(from_ready, 1.0);
        // not a style: no detour
        let len = mt::get_animation_length_between(
            &w,
            MT,
            stance(),
            MotionCommand::Ready,
            MotionCommand(WALK),
            1.0,
        );
        assert!((len - (0.5 + (9.9998 - 2.0) / 20.0)).abs() < 1e-5, "{len}");
    }

    /// V352 (retail): the attack hook on frame 5 of A fires as the client leaves that frame, 6
    /// frames into the 15 (ACE: 5 of 15).
    #[test]
    fn cycle_attack_frames_run_and_turn_speed() {
        let w = world();
        assert_eq!(
            mt::get_cycle_length(&w, MT, stance(), MotionCommand(WALK), 1.0),
            0.5,
            "B, 10 frames at 20/s"
        );
        let frames = mt::get_attack_frames(&w, MT, stance(), MotionCommand(ATTACK));
        assert_eq!(frames.len(), 1);
        assert!((frames[0].0 - 6.0 / 15.0).abs() < 1e-6, "{}", frames[0].0);
        assert!(matches!(
            frames[0].1.data,
            HookData::Attack { part_index: 7, .. }
        ));
        // C: ten 1 m frames, 10 m over 10 frames at 30 frames/s
        assert_eq!(mt::get_run_speed(&w, MT), 30.0);
        assert_eq!(mt::get_turn_speed(&w, MT), 0.0, "no TurnRight cycle");
    }

    #[test]
    fn final_position_moves_the_callers_position_ace_bug() {
        let w = world();
        let table = w
            .dats
            .portal_dat()
            .read_from_dat::<MotionTable>(MT)
            .expect("table");
        let mut start =
            Position::from_components(0xA9B4_0001, 10.0, 10.0, 0.0, 0.0, 0.0, 0.0, 1.0, false);
        // Ready -> Walk plays A (whole, no position frames) first: ACE returns the start unchanged
        let r = mt::get_animation_final_position_from_start_of(
            &w.dats,
            Some(&table),
            &mut start,
            1.0,
            MotionCommand(WALK),
        );
        assert_eq!(r.position_y, 10.0);
    }
}

// ---------------------------------------------------------------------------------- real content

#[cfg(feature = "real-content")]
mod real_content {
    //! The retail dats under `DERETH_TEST_DAT_DIR`; fails, never skips,
    //! when they are absent.

    use super::*;
    use empyrean_dat::{DatManager, RealDats};
    use empyrean_entity::enums::PropertyInt;
    use std::sync::{Arc, OnceLock};

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

    fn real_world() -> World {
        World::new(snapshot(0.0), dats())
    }

    /// A drudge (setup 0x020007DD, motion table 0x09000008 in ACE's world DB) and a human player body
    /// enter the world at the first outdoor chargen start location.
    #[test]
    fn a_drudge_and_a_player_body_enter_the_world_at_a_chargen_spawn() {
        let mut w = real_world();
        let char_gen = w.dats.portal_dat().char_gen();
        let start = *char_gen
            .starter_areas
            .iter()
            .flat_map(|a| &a.locations)
            .find(|p| p.cell.is_outdoor())
            .expect("outdoor start");
        let human = char_gen
            .heritage_groups
            .values()
            .next()
            .and_then(|h| h.sexes.values().next())
            .map(|s| s.setup.0)
            .expect("a body");
        assert!(phys_ext::load_landblock(&mut w, start.cell.landblock().0));

        let r = start.frame.rotation;
        let pos = Position::from_components(
            start.cell.0,
            start.frame.origin.x,
            start.frame.origin.y,
            start.frame.origin.z,
            r.x,
            r.y,
            r.z,
            r.w,
            false,
        );
        let mk = |w: &mut World,
                  guid: u32,
                  kind: KindData,
                  depth: u8,
                  setup: u32,
                  mtable: u32,
                  dx: f32| {
            let g = ObjectGuid::new(guid);
            let mut p = pos;
            p.position_x += dx;
            let mut o = WorldObject {
                guid: g,
                container: (depth >= 1).then(Box::default),
                creature: (depth >= 2).then(Box::default),
                player: (depth >= 3).then(Box::default),
                kind,
                ..Default::default()
            };
            if depth >= 2 {
                use empyrean_common::dotnet::DotNetDict;
                o.biota.properties_enchantment_registry = Some(Vec::new());
                o.biota.properties_attribute = Some(DotNetDict::new());
                o.biota.properties_attribute_2nd = Some(DotNetDict::new());
                o.biota.properties_skill = Some(DotNetDict::new());
                empyrean_world::world_objects::creature_vitals::set_ephemeral_stat_values(
                    w, &mut o,
                );
            }
            o.set_property(PropertyDataId::Setup, setup);
            o.set_property(PropertyDataId::MotionTable, mtable);
            o.set_property(PropertyInt::PhysicsState, 0x0040_0C08);
            o.set_location(Some(p));
            if depth >= 2 {
                empyrean_world::world_objects::monster::set_monster_state(&mut o);
            }
            w.objects.insert(o).expect("fresh");
            g
        };
        let drudge = mk(
            &mut w,
            0x8000_0001,
            KindData::Creature,
            2,
            0x0200_07DD,
            0x0900_0008,
            3.0,
        );
        let player = mk(
            &mut w,
            0x5000_0001,
            KindData::Player,
            3,
            human,
            0x0900_0001,
            0.0,
        );
        w.sessions.insert(
            empyrean_net::SessionId {
                client_id: 1,
                generation: 1,
            },
            empyrean_world::sessions::SessionData {
                player: Some(player),
                ..Default::default()
            },
        );
        for g in [drudge, player] {
            assert!(
                phys_ext::add_world_object_physics(&mut w, g),
                "{g:?} entered at {start:?}"
            );
            let h = phys_ext::physics_obj(&w, g).expect("body");
            let cell = phys_ext::cur_cell(&w, h).expect("cell");
            assert_eq!(cell.landblock(), start.cell.landblock());
            let loc = w
                .objects
                .get(g)
                .and_then(|o| o.location())
                .expect("location");
            assert_eq!(loc.cell(), cell.0, "Location synced to the placed cell");
        }
        let hp = phys_ext::physics_obj(&w, player).expect("player body");
        let hd = phys_ext::physics_obj(&w, drudge).expect("drudge body");
        assert_eq!(
            om::get_visible_objects_values(&w, hp),
            vec![hd],
            "the player sees the drudge"
        );
        assert_eq!(
            om::get_visible_targets_values(&w, hd),
            vec![hp],
            "the drudge targets the player"
        );
    }

    fn u32_in(v: &Value, k: &str) -> u32 {
        u32::try_from(v[k].as_u64().unwrap_or_else(|| panic!("{k} in {v}"))).expect("u32")
    }

    fn anim_replay(name: &str, mut check: impl FnMut(&World, &Case) -> Option<String>) {
        let w = real_world();
        let file = vectors::load_named("anim", name);
        assert!(!file.cases.is_empty());
        let bad: Vec<String> = file.cases.iter().filter_map(|c| check(&w, c)).collect();
        assert!(
            bad.is_empty(),
            "anim/{name}: {} of {} differ; first: {}",
            bad.len(),
            file.cases.len(),
            bad[0]
        );
    }

    #[test]
    #[allow(clippy::approx_constant)] // the harness's literal 0.7071068f, not 1/sqrt(2)
    fn motion_table_speeds_and_final_positions_match_ace() {
        // The lengths, cycle lengths and attack frames are the client's playback since V352, so
        // ACE's vectors for them are no longer replayed; `anim_timing` checks them against the
        // client's sequence on every retail motion table.
        anim_replay("motion_table_speeds", |w, c| {
            let id = u32_in(&c.input, "motion_table_id");
            let (run, turn) = (mt::get_run_speed(w, id), mt::get_turn_speed(w, id));
            let ok = same_f32(run, f32_in(&c.output, "run_speed"))
                && same_f32(turn, f32_in(&c.output, "turn_speed"));
            (!ok).then(|| format!("{id:08X} -> {run} {turn}, ACE {}", c.output))
        });
        anim_replay(
            "motion_table_get_animation_final_position_from_start",
            |w, c| {
                let i = &c.input;
                let id = u32_in(i, "motion_table_id");
                let table = w
                    .dats
                    .portal_dat()
                    .read_from_dat::<empyrean_dat::file_types::MotionTable>(id);
                let mut start = Position::from_components(
                    0xA9B4_0019,
                    84.5,
                    101.25,
                    10.0,
                    0.0,
                    0.0,
                    0.707_106_8,
                    0.707_106_8,
                    false,
                );
                let p = mt::get_animation_final_position_from_start_of(
                    &w.dats,
                    table.as_deref(),
                    &mut start,
                    f32_in(i, "obj_scale"),
                    MotionCommand(u32_in(i, "motion")),
                );
                let o = c.output.as_array().expect("array");
                let got = [
                    p.position_x,
                    p.position_y,
                    p.position_z,
                    p.rotation_w,
                    p.rotation_x,
                    p.rotation_y,
                    p.rotation_z,
                ];
                let ok = o[0].as_u64() == Some(u64::from(p.cell()))
                    && got
                        .iter()
                        .zip(&o[1..])
                        .all(|(g, e)| same_f32(*g, f32_of(e).expect("float")));
                (!ok).then(|| format!("{i} -> {got:?} in {:08X}, ACE {}", p.cell(), c.output))
            },
        );
    }
}

mod body_state {
    use crate::support::creature_world::*;

    /// `Door.Open` sets `Ethereal = true` (SetPhysicsPropertyState: the property and the body's
    /// state bit) and broadcasts the state; `Door.Close` then `FinalizeClose` (after the close
    /// animation, none here) clears both. Neither `SetPhysicsState` nor `EnqueueBroadcastPhysicsState`
    /// is a pointer any more.
    #[test]
    fn a_door_opening_and_closing_writes_its_bodys_ethereal_state() {
        let mut h = H::new();
        let d = h.place_new(DOOR_WCID, 0x7A9B_4400);
        assert!(
            !body_has(&h, d, PhysicsState::Ethereal),
            "a closed door is solid"
        );
        let _ = not_ported::take_local();

        door::open(&mut h.w, d, ObjectGuid::INVALID);
        assert_eq!(h.o(d).ethereal(), Some(true));
        assert!(
            body_has(&h, d, PhysicsState::Ethereal),
            "open: the body is ethereal"
        );

        door::close(&mut h.w, d, ObjectGuid::INVALID);
        h.advance(0.1);
        h.tick();
        assert_eq!(h.o(d).ethereal(), Some(false));
        assert!(
            !body_has(&h, d, PhysicsState::Ethereal),
            "FinalizeClose: solid again"
        );

        let hits = not_ported::take_local();
        for m in [
            "ACE: WorldObject.SetPhysicsState",
            "ACE: WorldObject.EnqueueBroadcastPhysicsState",
        ] {
            assert!(!hits.contains_key(m), "{m} is ported: {hits:?}");
        }
    }

    /// The world-level setter writes the body; the setter on `WorldObject` is for objects with no
    /// body, and refuses one that has a body rather than lose the change.
    #[test]
    fn the_world_level_setter_writes_the_body_and_the_detached_one_refuses_a_body() {
        let mut h = H::new();
        let d = h.place_new(DOOR_WCID, 0x7A9B_4401);
        world_object_properties::set_lights_status(&mut h.w, d, Some(false));
        assert!(!body_has(&h, d, PhysicsState::LightingOn));
        world_object_properties::set_lights_status(&mut h.w, d, Some(true));
        assert!(body_has(&h, d, PhysicsState::LightingOn));
        assert_eq!(h.o(d).lights_status(), Some(true));

        let refused = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            h.w.objects.get_mut(d).unwrap().set_ethereal(Some(true))
        }));
        assert!(
            refused.is_err(),
            "the detached setter panics on an object with a body"
        );
    }

    /// `InitPhysicsObj`'s `CalculatedPhysicsState`: the weenie's PhysicsState (0x40D) sets the null
    /// PropertyBool counterparts (Ethereal, ReportCollisions, GravityStatus) and the body gets those
    /// bits; `Static` has no property and no body yet, so it is the weenie's bit and the body is
    /// static (V305; ACE dropped it and made the body dynamic).
    #[test]
    fn a_new_body_gets_the_calculated_physics_state() {
        let mut h = H::new();
        let g = h.place_new(GHOST_WCID, 0x7A9B_4402);
        let o = h.o(g);
        assert_eq!(
            (o.ethereal(), o.report_collisions(), o.gravity_status()),
            (Some(true), Some(true), Some(true))
        );
        assert_eq!(
            o.lights_status(),
            None,
            "LightingOn was not in the weenie's state"
        );
        let body = phys_ext::physics_obj(&h.w, g).unwrap();
        let state = phys_ext::state(&h.w, body);
        assert_eq!(
            state.0 & 0x40D,
            0x40D,
            "Static | Ethereal | ReportCollisions | Gravity: {:#X}",
            state.0
        );
        assert!(
            h.w.physics.get(body).unwrap().state.is_static(),
            "a static body"
        );
    }
}

mod falling_damage {
    use crate::support::navigation_world::*;

    /// Landing hard: `WeenieObject.DoCollision(EnvCollisionProfile)` gives a player
    /// `HandleFallingDamage` (110 at -30 m/s).
    #[test]
    fn a_hard_landing_reaches_falling_damage_through_the_weenie_object() {
        let mut w = world();
        let h = body(&w);
        w.physics.get_mut(h).unwrap().velocity_vector = Vec3::new(0.0, 0.0, -30.0);
        let before = {
            let o = obj(&w);
            o.health().current(o)
        };
        let weenie = phys_ext::weenie_obj(&w, h);
        assert!(weenie.is_player);
        assert_eq!(weenie.do_collision_environment(&mut w), 0);
        let after = {
            let o = obj(&w);
            o.health().current(o)
        };
        assert_eq!(before - after, 110);
    }
}

#[cfg(feature = "real-content")]
mod retail_projectiles {
    use crate::support::creature_world::real_content::*;

    /// V305/V313/V314 on real content (V313, V314): a projectile of each spell below, launched from a
    /// player on the retail dats and world.pack, has retail's in-flight CreateObject state
    /// (as the retail captures show). Every correction
    /// still matches what the pack stores.
    #[test]
    fn launched_projectiles_carry_retails_in_flight_state() {
        use dereth_protocol::objects::ItemCreateObject;
        use empyrean_world::entity::spell::Spell;
        use empyrean_world::network::game_messages::messages::game_message_create_object::game_message_create_object;
        use empyrean_world::world_objects::{spell_projectile, world_object_magic};

        let stored = pack();
        for c in empyrean_content::corrections::WEENIE_CORRECTIONS {
            let w = stored
                .base()
                .get_stored_weenie(c.weenie_class_id)
                .unwrap_or_else(|| panic!("{} is in the pack", c.weenie_class_id));
            assert!(
                empyrean_content::corrections::stale_corrections(&w).is_empty(),
                "{c:?} no longer matches the pack"
            );
        }
        for c in empyrean_content::corrections::SPELL_CORRECTIONS {
            let s = stored
                .get_cached_spell(c.spell_id)
                .unwrap_or_else(|| panic!("spell {} is in the pack", c.spell_id));
            let empyrean_content::corrections::SpellValue::Wcid(want) = c.corrected;
            assert_eq!(s.wcid, want, "{c:?} applies to the pack");
        }

        let mut h = world();
        lm::get_landblock(
            &mut h.w,
            LandblockId::new(u32::from(HOME) << 16 | 0xFFFF),
            false,
            false,
        );
        let p = player(&mut h, 48.0, 32.0);
        // (spell, its projectile weenie, retail's CreateObject state)
        let cases: [(u32, u32, u32); 18] = [
            (3948, 33862, 0x2_0B48), // Flame Wave (V313)
            (3952, 33866, 0x2_0B48), // Shock Waves (V313)
            (3950, 33864, 0x2_0B48), // Frost Wave (V313)
            (3935, 33727, 0x2_0B48), // Heavy Blade Ring (V313)
            (3914, 33498, 0x2_0348), // Dark Vortex (V313)
            (6169, 52621, 0x2_8F48), // Deadly Lightning Volley (V313)
            (5357, 43232, 0x2_8B48), // Nether Streak I: the streak's projectile (V313)
            (5362, 43231, 0x2_8F48), // Nether Arc II: the arc's projectile (V313)
            (5367, 43231, 0x2_8F48), // Nether Arc VII, the monsters' (V313)
            (3902, 33040, 0x2_8B48), // Ring around the Rabbit (V313, V314)
            (3458, 29030, 0x2_8B48), // Mana Purge's cloud (V314)
            (3459, 29031, 0x2_8B48), // Mucor Cloud (V314)
            (3806, 7270, 0x2_0B48),  // Flame Ring: no ScriptedCollision, as before
            (3901, 33039, 0x2_8E48), // Egg Bomb (V314)
            (4028, 34434, 0x2_8A48), // Snowball (V314)
            (4124, 35960, 0x2_0A48), // Dark Nanners (V314)
            (4264, 37159, 0x2_8A48), // Arcane Death (V314)
            (92, 1636, 0x2_8A48),    // Whirling Blade I (spins): unchanged
        ];
        let mut wrong = Vec::new();
        for (spell_id, wcid, want) in cases {
            let spell = Spell::new(&h.w, spell_id, true);
            assert_eq!(spell.wcid(), wcid, "spell {spell_id}'s projectile");
            let spell_type = spell_projectile::get_projectile_spell_type(&h.w, spell_id);
            let origins = [empyrean_common::dotnet::Vector3::new(0.0, 1.0, 1.5)];
            let velocity = empyrean_common::dotnet::Vector3::new(0.0, 20.0, 0.0);
            let sps = world_object_magic::launch_spell_projectiles(
                &mut h.w, p, &spell, None, spell_type, None, false, false, &origins, velocity, 0,
            );
            let Some(&sp) = sps.first() else {
                wrong.push(format!(
                    "spell {spell_id} ({wcid}, {spell_type:?}): no projectile in flight"
                ));
                continue;
            };
            let m = game_message_create_object(&mut h.w, sp, false, false);
            let state = dereth_protocol::read_body_padded::<ItemCreateObject>(&m.data[4..])
                .expect("the CreateObject decodes")
                .0
                .physicsdesc
                .state;
            if state != want {
                wrong.push(format!(
                    "spell {spell_id} ({wcid}, {spell_type:?}): {state:#X}, retail {want:#X}"
                ));
            }
            for g in sps {
                empyrean_world::world_objects::world_object::destroy(&mut h.w, g, true, false);
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }
}
