//! ACE: Source/ACE.Server/WorldObjects/WorldObject.cs::WorldObject
//! Virtual calls route to the most-derived override, unported overrides fall through, out-of-
//! subtree/missing object panics.
//! Fixture: synthetic dats, isolated world state.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Duration;

use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::not_ported::take_local;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::MotionCommand;
use empyrean_entity::ObjectGuid;
use empyrean_world::dispatch::{self, class_of, Class};
use empyrean_world::world_objects::kinds::KindData;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::World;

const DOOR: u32 = 0x7000_0001;
const PORTAL: u32 = 0x7000_0002;
const CHEST: u32 = 0x7000_0003;
const STORAGE: u32 = 0x7000_0004;
const COMBAT_PET: u32 = 0x8000_0005;
const PLAYER: u32 = 0x5000_0006;
const PLAIN: u32 = 0x7000_0007;
const ACTIVATOR: u32 = 0x5000_0099;

fn g(v: u32) -> ObjectGuid {
    ObjectGuid::new(v)
}

fn world() -> World {
    let now = ClockSnapshot {
        portal_year_ticks: 0.0,
        unix_time: 0.0,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    };
    let mut w = World::new(now, FakeDats::new().build().expect("empty fake dats"));
    let objs = [
        (DOOR, KindData::Door(Box::default()), 0),
        (PORTAL, KindData::Portal(Box::default()), 0),
        (CHEST, KindData::Chest(Box::default()), 1),
        (STORAGE, KindData::Storage(Box::default()), 1),
        (COMBAT_PET, KindData::CombatPet(Box::default()), 2),
        (PLAYER, KindData::Player, 3),
        (PLAIN, KindData::WorldObject, 0),
    ];
    for (guid, kind, depth) in objs {
        let o = WorldObject {
            guid: g(guid),
            container: (depth >= 1).then(Box::default),
            creature: (depth >= 2).then(Box::default),
            player: (depth >= 3).then(Box::default),
            kind,
            ..Default::default()
        };
        w.objects.insert(o).expect("fresh guid");
    }
    take_local();
    w
}

/// The `not_ported!` members hit since the last call, sorted by name.
fn hits() -> Vec<&'static str> {
    take_local().into_keys().collect()
}

#[test]
fn class_of_reads_the_most_derived_class() {
    let mut w = world();
    assert_eq!(class_of(&w, g(STORAGE)), Class::Storage);
    assert_eq!(class_of(&w, g(COMBAT_PET)), Class::CombatPet);
    assert_eq!(class_of(&w, g(PLAIN)), Class::WorldObject);
    // A plain kind carrying components reads as its most derived component class.
    let o = WorldObject {
        guid: g(0x5000_0100),
        player: Some(Box::default()),
        ..Default::default()
    };
    w.objects.insert(o).unwrap();
    assert_eq!(class_of(&w, g(0x5000_0100)), Class::Player);
    assert_eq!(Class::HousePortal.name(), "HousePortal");
}

#[test]
fn act_on_use_routes_to_the_nearest_override() {
    let mut w = world();
    let a = g(ACTIVATOR);
    let cases: [(u32, &[&str]); 6] = [
        (DOOR, &[]),
        (PORTAL, &[]),
        (CHEST, &[]),
        // Storage does not override ActOnUse: it runs Chest's.
        (STORAGE, &[]),
        (COMBAT_PET, &[]),
        (PLAYER, &[]),
    ];
    for (guid, want) in cases {
        dispatch::act_on_use::act_on_use(&mut w, g(guid), a);
        assert_eq!(hits(), want, "ActOnUse on 0x{guid:08X}");
    }
    dispatch::act_on_use::act_on_use(&mut w, g(PLAIN), g(PORTAL));
    assert!(hits().is_empty());
    assert!(
        w.objects.get(g(DOOR)).unwrap().is_open(),
        "Door.ActOnUse opened the door"
    );
    assert!(
        !w.objects.get(g(CHEST)).unwrap().is_open()
            && !w.objects.get(g(STORAGE)).unwrap().is_open()
    );
}

#[test]
fn check_use_requirements_and_collisions_route_by_class() {
    let mut w = world();
    let a = g(ACTIVATOR);
    let r = dispatch::check_use_requirements::check_use_requirements(&mut w, g(STORAGE), a);
    assert!(hits().is_empty());
    assert!(!r.success);
    // Door does not override it: WorldObject's (ported) refuses a null activator.
    let r = dispatch::check_use_requirements::check_use_requirements(&mut w, g(DOOR), a);
    assert!(hits().is_empty());
    assert!(!r.success);
    // an activator that is not a player passes WorldObject's checks
    let r = dispatch::check_use_requirements::check_use_requirements(&mut w, g(DOOR), g(PLAIN));
    assert!(r.success);

    dispatch::on_collide_object::on_collide_object(&mut w, g(PLAYER), a);
    assert!(hits().is_empty());
    dispatch::on_collide_object::on_collide_object(&mut w, g(COMBAT_PET), a);
    assert!(hits().is_empty());
    // Portal's own `OnCollideObject(Player)` is a separate virtual, not an override.
    dispatch::on_collide_object::on_collide_object(&mut w, g(PORTAL), a);
    assert!(hits().is_empty());
    dispatch::on_collide_object::on_collide_object_player(&mut w, g(PORTAL), a);
    assert!(hits().is_empty());
    // An empty ACE base is ported as such: no stub is hit.
    dispatch::on_collide_object_end::on_collide_object_end(&mut w, g(DOOR), a);
    assert!(hits().is_empty());
}

#[test]
fn properties_and_trivial_bases_return_ace_values() {
    let mut w = world();
    let chest = dispatch::default_chest_reset_interval::default_chest_reset_interval(&w, g(CHEST));
    assert_eq!((chest, hits()), (120.0, vec![]));
    let storage =
        dispatch::default_chest_reset_interval::default_chest_reset_interval(&w, g(STORAGE));
    assert_eq!((storage, hits()), (f64::INFINITY, vec![]));
    let pickup = dispatch::motion_pickup::motion_pickup(&w, g(STORAGE));
    assert_eq!((pickup, hits()), (MotionCommand::Pickup, vec![]));

    assert_eq!(
        dispatch::get_burden_mod::get_burden_mod(&mut w, g(COMBAT_PET)),
        1.0
    );
    assert!(hits().is_empty());

    let _ = dispatch::name::name(&w, g(PLAYER));
    assert!(
        hits().is_empty(),
        "The player name is read from its properties"
    );
    dispatch::name::set_name(&mut w, g(DOOR), "Door".into());
    assert!(hits().is_empty());
}

#[test]
fn creature_members_route_within_the_creature_subtree() {
    let mut w = world();
    dispatch::sleep::sleep(&mut w, g(COMBAT_PET));
    assert!(hits().is_empty());
    assert_eq!(
        dispatch::get_burden_mod::get_burden_mod(&mut w, g(COMBAT_PET)),
        1.0
    );
    assert!(hits().is_empty());
    let _ = (g(PLAYER), g(ACTIVATOR));
}

#[test]
fn a_member_outside_its_declaring_subtree_panics() {
    let mut w = world();
    let r = catch_unwind(AssertUnwindSafe(|| {
        dispatch::get_burden_mod::get_burden_mod(&mut w, g(DOOR))
    }));
    let msg = *r
        .expect_err("Door is not a Creature")
        .downcast::<String>()
        .unwrap();
    assert!(
        msg.contains("Creature.GetBurdenMod") && msg.contains("Door"),
        "{msg}"
    );
}

#[test]
#[should_panic(expected = "virtual call on missing object")]
fn a_virtual_call_on_a_missing_object_panics() {
    let mut w = world();
    dispatch::act_on_use::act_on_use(&mut w, g(0x7000_0FFF), g(ACTIVATOR));
}
