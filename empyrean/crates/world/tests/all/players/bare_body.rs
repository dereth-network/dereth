//! Divergence: V430
//! A body of the character-creation table wears that table's bare parts where nothing covers it:
//! on the February 2005 dats the human setup's own arms and hands are armoured and the table's
//! base description gives every new body bare ones, so an uncovered arm is sent as the bare arm.
//! On the end-of-retail dats the table's bare parts are the setup's own but for the female
//! abdomen. An object of no heritage keeps the setup's parts.
//! Fixture: the February 2005 portal and cell files (`DERETH_TEST_PRETOD_DAT_DIR`) and the retail
//! dats.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_dat::{DatManager, RealDats};
use empyrean_entity::enums::{PropertyDataId, PropertyInt};
use empyrean_entity::ObjectGuid;
use empyrean_world::dispatch::Class;
use empyrean_world::world_objects::creature_networking::creature_calculate_obj_desc;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::World;

const BODY: ObjectGuid = ObjectGuid::new(0x8000_0100);

fn open(dir: &std::path::Path) -> Arc<DatManager> {
    let real =
        RealDats::open(dir).unwrap_or_else(|e| panic!("the dats under {}: {e}", dir.display()));
    DatManager::initialize(Arc::new(real)).expect("the dats initialize")
}

fn world(dats: Arc<DatManager>) -> World {
    let now = ClockSnapshot {
        portal_year_ticks: 0.0,
        unix_time: 0.0,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    };
    World::new(now, dats)
}

fn february_2005() -> World {
    static DATS: OnceLock<Arc<DatManager>> = OnceLock::new();
    let dats = DATS.get_or_init(|| {
        if let Some(msg) = dereth_dat::testing::pre_tod_shortfall() {
            panic!("{msg}");
        }
        open(&dereth_dat::testing::pre_tod_dat_dir().unwrap_or_default())
    });
    world(Arc::clone(dats))
}

fn end_of_retail() -> World {
    static DATS: OnceLock<Arc<DatManager>> = OnceLock::new();
    world(Arc::clone(
        DATS.get_or_init(|| open(&dereth_dat::testing::dat_dir())),
    ))
}

/// The parts sent for a body on `setup` with nothing worn, of `heritage` and `sex` when given.
fn bare_body(w: &mut World, setup: u32, who: Option<(i32, i32)>) -> Vec<(u8, u32)> {
    let mut o = WorldObject::allocate(Class::Creature);
    o.guid = BODY;
    o.biota.id = BODY.full();
    o.set_property(PropertyDataId::Setup, setup);
    if let Some((heritage, sex)) = who {
        o.set_property(PropertyInt::HeritageGroup, heritage);
        o.set_property(PropertyInt::Gender, sex);
    }
    w.objects.insert(o).expect("fresh guid");
    let d = creature_calculate_obj_desc(w, BODY);
    d.anim_part_changes
        .iter()
        .map(|p| (p.index, p.animation_id))
        .collect()
}

fn part(parts: &[(u8, u32)], index: u8) -> Option<u32> {
    parts.iter().find(|(p, _)| *p == index).map(|(_, m)| *m)
}

#[test]
fn on_the_february_2005_dats_an_uncovered_arm_is_the_bare_arm() {
    let mut w = february_2005();
    let man = bare_body(&mut w, 0x0200_0001, Some((1, 1)));
    // The upper arms, forearms and hands of the table's base description, not the setup's
    // armoured `0x01000055`... .
    for (index, bare) in [
        (10, 0x0100_0497),
        (11, 0x0100_0495),
        (12, 0x0100_0076),
        (13, 0x0100_04AD),
        (14, 0x0100_0496),
        (15, 0x0100_0077),
    ] {
        assert_eq!(part(&man, index), Some(bare), "part {index}");
    }
    // The parts the table leaves as the setup's own are the setup's.
    assert_eq!(part(&man, 0), Some(0x0100_004E));
    assert_eq!(part(&man, 0x10), None, "the head is the hair's");

    let mut w = february_2005();
    let woman = bare_body(&mut w, 0x0200_004E, Some((1, 2)));
    assert_eq!(part(&woman, 9), Some(0x0100_04B6));
    assert_eq!(part(&woman, 10), Some(0x0100_04CF));
}

#[test]
fn an_object_of_no_heritage_or_on_another_setup_keeps_the_setups_parts() {
    let mut w = february_2005();
    let statue = bare_body(&mut w, 0x0200_0001, None);
    assert_eq!(part(&statue, 10), Some(0x0100_0055), "the setup's own arm");
    // A heritage and sex whose table body is another setup.
    let mut w = february_2005();
    let other = bare_body(&mut w, 0x0200_0001, Some((1, 2)));
    assert_eq!(part(&other, 10), Some(0x0100_0055));
}

#[test]
fn on_the_end_of_retail_dats_only_the_female_abdomen_differs_from_the_setup() {
    let mut w = end_of_retail();
    let man = bare_body(&mut w, 0x0200_0001, Some((1, 1)));
    let mut w = end_of_retail();
    let statue = bare_body(&mut w, 0x0200_0001, None);
    assert_eq!(man, statue);
    assert_eq!(part(&man, 10), Some(0x0100_0055));

    let mut w = end_of_retail();
    let woman = bare_body(&mut w, 0x0200_004E, Some((1, 2)));
    let mut w = end_of_retail();
    let female_statue = bare_body(&mut w, 0x0200_004E, None);
    assert_eq!(part(&woman, 9), Some(0x0100_04B6));
    assert_eq!(part(&female_statue, 9), Some(0x0100_04B9));
    let differ: Vec<u8> = woman
        .iter()
        .zip(&female_statue)
        .filter(|(a, b)| a != b)
        .map(|(a, _)| a.0)
        .collect();
    assert_eq!(differ, [9]);
}
