//! Divergence: V194
//! Every weenie-backed body carries its creature answer; projectile target is the transition
//! target id; legacy player move commits the sweep's contact.
//! Fixture: Synthetic terrain and world bodies with explicit positions and motion state.

use std::time::Duration;

use dereth_physics::{PhysHandle, SetupGeometry};
use dereth_primitives::ObjectId;
use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::PropertyDataId;
use empyrean_entity::{ObjectGuid, Position};
use empyrean_testkit::land;
use empyrean_world::physics::phys_ext;
use empyrean_world::world_objects::kinds::KindData;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::World;

const HOME: u16 = 0xA9B4;

fn snapshot(t: f64) -> ClockSnapshot {
    ClockSnapshot {
        portal_year_ticks: t,
        unix_time: 0.0,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    }
}

/// A world on `FakeDats`, physics over one flat landblock at 20 m and the synthetic setup.
fn world() -> World {
    let mut w = World::new(
        snapshot(0.0),
        empyrean_testkit::dats::with_stat_tables(FakeDats::new())
            .build()
            .expect("fake dats"),
    );
    land::use_flat_land_with_setup(
        &mut w,
        &[HOME],
        10,
        land::TEST_SETUP,
        SetupGeometry {
            step_up_height: 0.3,
            step_down_height: 0.3,
            ..land::test_setup_geometry()
        },
    );
    empyrean_world::managers::landblock_manager::get_landblock(
        &mut w,
        empyrean_entity::LandblockId::new(u32::from(HOME) << 16 | 0xFFFF),
        false,
        false,
    );
    phys_ext::load_landblock(&mut w, HOME);
    phys_ext::set_adjacents(&mut w, HOME, Vec::new());
    w
}

fn at(x: f32, y: f32, z: f32) -> Position {
    Position::from_components(
        u32::from(HOME) << 16 | 1,
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

/// An object of `kind` (`depth` 2: a creature) with the synthetic setup, entered at `pos`.
fn spawn(
    w: &mut World,
    guid: u32,
    kind: KindData,
    depth: u8,
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
    o.set_property(PropertyDataId::Setup, land::TEST_SETUP);
    o.set_location(Some(pos));
    o.current_landblock = Some(empyrean_entity::LandblockId::new(pos.cell()));
    if depth == 2 {
        empyrean_world::world_objects::monster::set_monster_state(&mut o);
    }
    w.objects.insert(o).expect("fresh guid");
    assert!(
        phys_ext::add_world_object_physics(w, g),
        "{g:?} entered the world"
    );
    (g, phys_ext::physics_obj(w, g).expect("a body"))
}

// ---------------------------------------------------------------------------------- A11

/// ACE's `FindObjCollisions`/`MissileIgnore` read `WeenieObj != null` and `WeenieObj.IsCreature`
/// of every candidate: each body with a world object carries them in the shared crate.
#[test]
fn every_weenie_backed_body_carries_its_creature_answer() {
    let mut w = world();
    let (_, creature) = spawn(
        &mut w,
        0x7000_0001,
        KindData::Creature,
        2,
        at(100.0, 100.0, 20.0),
    );
    let (_, item) = spawn(
        &mut w,
        0x7000_0002,
        KindData::WorldObject,
        0,
        at(110.0, 100.0, 20.0),
    );
    let record = |h| w.physics.get(h).and_then(|o| o.weenie.clone());
    let c = record(creature).expect("a creature's WeenieObj is not null");
    assert!(c.is_creature && !c.is_player);
    let i = record(item).expect("an item's WeenieObj is not null");
    assert!(!i.is_creature && !i.is_player);
}

/// `ObjectInfo.Init`: `TargetID = obj.ProjectileTarget.ID`, and zero without a target.
#[test]
fn a_projectile_target_is_the_bodys_transition_target_id() {
    let mut w = world();
    let (_, missile) = spawn(
        &mut w,
        0x7000_0001,
        KindData::WorldObject,
        0,
        at(100.0, 100.0, 21.0),
    );
    let (_, target) = spawn(
        &mut w,
        0x7000_0002,
        KindData::Creature,
        2,
        at(110.0, 100.0, 20.0),
    );
    let target_id = |w: &World| w.physics.get(missile).expect("a body").projectile_target_id;
    assert_eq!(target_id(&w), ObjectId(0));
    phys_ext::set_projectile_target(&mut w, missile, Some(target));
    assert_eq!(target_id(&w), ObjectId(0x7000_0002));
    assert_eq!(phys_ext::projectile_target(&w, missile), Some(target));
    phys_ext::set_projectile_target(&mut w, missile, None);
    assert_eq!(target_id(&w), ObjectId(0));
}

// ---------------------------------------------------------------------------------- A13

/// `UpdateObjectInternalServer` -> `SetPositionInternal(transit)`: the legacy movement's sweep
/// commits the transition's contact plane and contact state, which `set_current_pos` then keeps.
/// The body starts with its contact cleared, so only a commit of the sweep sets it again.
#[test]
fn the_legacy_player_move_commits_its_sweeps_contact() {
    use empyrean_common::dotnet::{Quaternion, Vector3};
    use empyrean_world::world_objects::player_tick;

    let mut w = world();
    let (g, h) = spawn(
        &mut w,
        0x5000_0001,
        KindData::Player,
        3,
        at(100.0, 100.0, 20.0),
    );
    assert!(
        !player_tick::fast_tick(&w, g),
        "a non-PK player takes the legacy movement"
    );
    {
        let o = w.physics.get_mut(h).expect("a body");
        o.transient_state.set_contact(false);
        o.set_on_walkable(false);
        o.contact_plane = dereth_physics::geom::plane::Plane::default();
    }
    w.now.portal_year_ticks += 0.1;
    let cell = u32::from(HOME) << 16 | 1;
    player_tick::set_request_pos(
        &mut w,
        g,
        h,
        Vector3::new(102.0, 100.0, 20.0),
        Quaternion::IDENTITY,
        None,
        cell,
    );
    assert!(
        player_tick::update_object_server(&mut w, g, h, true),
        "the move succeeds"
    );

    let o = w.physics.get(h).expect("a body");
    assert!(
        (o.position.frame.origin.x - 102.0).abs() < 1e-4,
        "at the requested position: {:?}",
        o.position
    );
    assert!(
        o.transient_state.in_contact() && o.transient_state.on_walkable(),
        "the sweep's contact is committed"
    );
    assert!(
        o.contact_plane.normal.z > 0.99,
        "the ground's plane: {:?}",
        o.contact_plane
    );
}
