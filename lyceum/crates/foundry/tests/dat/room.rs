//! Behaviour: none (tooling: where a new dungeon has room, the check run over every retail dungeon)

use std::collections::{BTreeMap, BTreeSet};

use dereth_primitives::DataId;
use lyceum_foundry::edit::Session;
use lyceum_foundry::room::{clashes, room_for_dungeon, sea_floor, Clash, Solid, Taken};

/// A building's rooms: those its doorways lead to, and every room joined to one of them.
fn building_rooms(
    s: &Session,
    lb: u16,
    cells: &BTreeMap<u16, dereth_assets::world::EnvCell>,
) -> BTreeSet<u16> {
    let mut rooms = BTreeSet::new();
    let mut todo: Vec<u16> = s
        .statics(lb)
        .map(|info| {
            info.buildings
                .iter()
                .flat_map(|b| &b.portals)
                .flat_map(|p| std::iter::once(p.other_cell_id).chain(p.stab_list.iter().copied()))
                .collect()
        })
        .unwrap_or_default();
    while let Some(n) = todo.pop() {
        if !rooms.insert(n) {
            continue;
        }
        if let Some(c) = cells.get(&n) {
            todo.extend(
                c.portals
                    .iter()
                    .map(|p| (p.other_cell_id & 0xFFFF) as u16)
                    .filter(|o| *o != 0xFFFF && *o >= 0x100),
            );
        }
    }
    rooms
}

#[test]
fn every_retail_dungeon_stands_where_the_room_check_finds_room_for_it() {
    let (_dir, world) = super::sample::sample("room-retail");
    let s = Session::open(&world, &dereth_dat::testing::dat_dir(), None).unwrap();
    let dungeons = s.retail_dungeons();
    assert_eq!(dungeons.len(), 1970, "retail's dungeon landblocks");
    let mut envs: BTreeMap<DataId, Option<dereth_assets::Environment>> = BTreeMap::new();
    let (mut on_sea_floor, mut sea_floor_with_statics, mut under_land) = (0, 0, 0);
    let (mut rooms_checked, mut crossing) = (0, 0);
    let mut shared: Vec<(u16, u16, u16)> = Vec::new();
    for &lb in dungeons.keys() {
        let terrain = s.terrain(lb);
        // The landblock as a new dungeon would find it with this one's rooms taken away: the sea
        // floor has room, but for statics standing there; land is refused.
        let mut statics = s.statics(lb);
        if let Some(info) = statics.as_mut() {
            info.num_cells = 0;
        }
        match room_for_dungeon(lb, terrain.as_ref(), statics.as_ref(), 0) {
            Ok(()) => on_sea_floor += 1,
            Err(Taken::Statics) => sea_floor_with_statics += 1,
            Err(Taken::Land) => under_land += 1,
            Err(other) => panic!("{lb:04X}: {other:?}"),
        }
        // Its rooms against the rest of its landblock: the ground and any building's rooms.
        let cells = s.cells(lb);
        let buildings = building_rooms(&s, lb, &cells);
        let mut solids: BTreeMap<u16, Solid> = BTreeMap::new();
        for (n, c) in &cells {
            let e = envs
                .entry(c.environment)
                .or_insert_with(|| s.environment(c.environment));
            if let Some(sol) = e.as_ref().and_then(|e| Solid::of_cell(c, e)) {
                solids.insert(*n, sol);
            }
        }
        let others: BTreeMap<u16, Solid> = solids
            .iter()
            .filter(|(n, _)| buildings.contains(n))
            .map(|(n, s)| (*n, s.clone()))
            .collect();
        let land = terrain.as_ref().is_some_and(|t| !sea_floor(t));
        let ground = |x: f32, y: f32| s.ground(lb, x, y);
        let ground: Option<&dyn Fn(f32, f32) -> Option<f32>> = land.then_some(&ground);
        for (n, room) in solids.iter().filter(|(n, _)| !buildings.contains(n)) {
            rooms_checked += 1;
            for c in clashes(room, &others, &[], ground) {
                match c {
                    Clash::Ground => crossing += 1,
                    Clash::Room(o) => shared.push((lb, *n, o)),
                }
            }
        }
    }
    println!(
        "sea floor {on_sea_floor} (with statics {sea_floor_with_statics}), under land {under_land}; \
         {rooms_checked} rooms, {crossing} across the ground, {} sharing a building's room",
        shared.len()
    );
    // Retail's own: nearly all on the sea floor, the rest under land, which a new one is kept off.
    assert_eq!(on_sea_floor + sea_floor_with_statics, 1751);
    assert_eq!(under_land, 219);
    // No room of a retail dungeon crosses the ground.
    assert!(rooms_checked > 600_000);
    assert_eq!(crossing, 0);
    // The few rooms found sharing a building's room share space well inside both rooms' walls,
    // seen from each side: retail stacks a dungeon's room on a building's so in five landblocks.
    for (lb, n, o) in &shared {
        let cells = s.cells(*lb);
        let solid = |i: u16| {
            let c = &cells[&i];
            Solid::of_cell(c, &s.environment(c.environment).unwrap()).unwrap()
        };
        let (a, b) = (solid(*n), solid(*o));
        assert!(
            a.samples.iter().any(|p| b.holds(*p)) && b.samples.iter().any(|p| a.holds(*p)),
            "{lb:04X}: room {n:04X} and {o:04X}"
        );
    }
    let blocks: BTreeSet<u16> = shared.iter().map(|(lb, ..)| *lb).collect();
    assert_eq!((shared.len(), blocks.len()), (10, 5), "{shared:04X?}");
}

#[test]
fn rooms_joined_at_a_doorway_fit_and_a_room_placed_on_another_does_not() {
    let (_dir, world) = super::sample::sample("room-fit");
    let s = Session::open(&world, &dereth_dat::testing::dat_dir(), None).unwrap();
    // A retail dungeon's rooms: each meets its neighbors at doorways and shares no space with
    // them; the same room again where it stands shares all of its own.
    let lb = *s
        .retail_dungeons()
        .iter()
        .find(|(_, n)| **n > 10)
        .unwrap()
        .0;
    let cells = s.cells(lb);
    assert!(cells.len() > 10);
    let solids: BTreeMap<u16, Solid> = cells
        .iter()
        .filter_map(|(n, c)| Some((*n, Solid::of_cell(c, &s.environment(c.environment)?)?)))
        .collect();
    let (n, c) = cells.iter().find(|(_, c)| c.portals.len() >= 2).unwrap();
    let joined: Vec<u16> = c
        .portals
        .iter()
        .map(|p| (p.other_cell_id & 0xFFFF) as u16)
        .filter(|o| solids.contains_key(o))
        .collect();
    assert!(!joined.is_empty());
    for o in &joined {
        assert!(
            !solids[n].overlaps(&solids[o]),
            "{n:04X} and {o:04X} meet at a doorway"
        );
    }
    let again = solids[n].clone();
    assert!(clashes(&again, &solids, &[], None).contains(&Clash::Room(*n)));
}
