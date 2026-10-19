//! Behaviour: none (tooling: the dungeon kit over the retail files: pieces, placing, walking, and moving a building)

use std::collections::{BTreeMap, BTreeSet};

use dereth_assets::world::{CellLandblock, EnvCell, LandblockInfo};
use dereth_assets::{Decode, Environment};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::frame::localtoglobal;
use dereth_primitives::{DataId, Frame, Quat, Vec3};
use lyceum_foundry::edit::{Op, Session};
use lyceum_foundry::pieces::{catalog, opening, snap, Piece, Shape};
use lyceum_foundry::source::{Link, Rotation};
use lyceum_foundry::walk;

fn session(tag: &str) -> (dereth_dat::testing::ScratchDir, Session) {
    let (dir, world) = super::sample::sample(tag);
    let mut s = Session::open(&world, &dereth_dat::testing::dat_dir(), None).unwrap();
    s.set_today("2026-10-19 00:00:00");
    (dir, s)
}

/// The commonest piece with two portals: a corridor.
fn corridor(pieces: &[Piece]) -> Piece {
    pieces
        .iter()
        .filter(|p| p.portals == 2)
        .max_by_key(|p| p.cells)
        .cloned()
        .expect("a piece with two portals")
}

/// The first landblock of the open sea floor: heights all 0, and no statics record or cells.
fn empty_landblock(s: &RetailDatStore) -> u16 {
    (0x0101u16..)
        .find(|lb| {
            let id = DataId((u32::from(*lb) << 16) | 0xFFFF);
            let Ok(b) = s.read_cell(id) else { return false };
            let land = CellLandblock::decode_payload(id, &b).unwrap();
            land.lbi_exists == 0 && land.height.iter().all(|h| *h == 0)
        })
        .expect("an empty landblock")
}

#[test]
fn the_pieces_are_every_piece_retails_cells_are_built_of() {
    let set = lyceum_foundry::dats::DatSet::open(&dereth_dat::testing::dat_dir()).unwrap();
    let pieces = catalog(&set).pieces;
    let envs: BTreeSet<u32> = pieces.iter().map(|p| p.environment.raw()).collect();
    assert_eq!(envs.len(), 769, "the environments retail's cells use");
    let cells: usize = pieces.iter().map(|p| p.cells).sum();
    assert_eq!(cells, 734_976, "every retail cell counted once");
    let c = corridor(&pieces);
    println!("the corridor: {}", c.label());
    assert!(c.size.iter().all(|x| *x > 0.0) && !c.surfaces.is_empty() && !c.landblocks.is_empty());
}

#[test]
fn a_piece_snapped_to_another_meets_it_portal_to_portal_at_a_quarter_turn() {
    let set = lyceum_foundry::dats::DatSet::open(&dereth_dat::testing::dat_dir()).unwrap();
    let piece = corridor(&catalog(&set).pieces);
    let env = Environment::decode_payload(
        piece.environment,
        &set.read_portal(piece.environment).unwrap(),
    )
    .unwrap();
    let first = Frame::new(Vec3::new(96.0, 96.0, -40.0), Quat::IDENTITY);
    let target = opening(&env, piece.piece, 0, &first).unwrap();
    for slot in 0..2 {
        let (frame, miss) = snap(&env, piece.piece, slot, &target, 0.0, 90.0).unwrap();
        assert!(miss < 0.01, "slot {slot}: corners {miss} m apart");
        let placed = opening(&env, piece.piece, slot, &frame).unwrap();
        assert!(
            placed.out.dot(target.out) < -0.99,
            "the two face each other"
        );
        // A quarter turn about up, nothing else.
        let q = frame.rotation;
        assert!(q.x.abs() < 1e-6 && q.y.abs() < 1e-6);
        let deg = (2.0 * dereth_primitives::num::math::atan2f(q.z, q.w))
            .to_degrees()
            .rem_euclid(90.0);
        assert!(deg < 0.01 || deg > 89.99, "turned {deg}");
    }
}

#[test]
fn a_dungeon_of_two_pieces_compiles_to_cells_that_walk_and_see_each_other() {
    let (_dir, mut s) = session("dungeon-two");
    let store = dereth_dat::testing::open_store_or_fail();
    let lb = empty_landblock(&store);
    let pieces = catalog(s.base()).pieces;
    let piece = corridor(&pieces);
    let env = s.environment(piece.environment).unwrap();
    // The first piece, then the second snapped to its portal 0 by its own portal 1.
    s.apply(&Op::PlaceCell {
        landblock: lb,
        environment: piece.environment,
        piece: piece.piece,
        surfaces: piece.surfaces.clone(),
        at: [96.0, 96.0, -40.0],
        rotation: Rotation::Heading(0.0),
        links: vec![Link::Shut, Link::Shut],
    })
    .unwrap();
    let first = Frame::new(Vec3::new(96.0, 96.0, -40.0), Quat::IDENTITY);
    let target = opening(&env, piece.piece, 0, &first).unwrap();
    let (frame, _) = snap(&env, piece.piece, 1, &target, 0.0, 90.0).unwrap();
    let q = frame.rotation;
    let pack = s.packs()[s.target()].name.clone();
    s.apply(&Op::PlaceCell {
        landblock: lb,
        environment: piece.environment,
        piece: piece.piece,
        surfaces: piece.surfaces.clone(),
        at: [frame.origin.x, frame.origin.y, frame.origin.z],
        rotation: Rotation::Quat([q.w, q.x, q.y, q.z]),
        links: vec![
            Link::Shut,
            Link::Cell {
                cell: format!("{pack}:cell-1"),
                portal: 0,
            },
        ],
    })
    .unwrap();
    let cells = s.cells(lb);
    assert_eq!(cells.keys().copied().collect::<Vec<_>>(), [0x0100, 0x0101]);
    let names = s.cell_names(lb);
    assert_eq!(names[&0x0100], format!("{pack}:cell-1"));
    let (a, b) = (&cells[&0x0100], &cells[&0x0101]);
    // Joined both ways, exactly, each naming the other's portal.
    let ab: Vec<_> = a.portals.iter().filter(|p| p.flags & 4 == 0).collect();
    let ba: Vec<_> = b.portals.iter().filter(|p| p.flags & 4 == 0).collect();
    assert_eq!((ab.len(), ba.len()), (1, 1));
    assert_eq!((ab[0].other_cell_id, ba[0].other_cell_id), (0x0101, 0x0100));
    assert_eq!(ab[0].flags & 1, 1, "an exact match");
    assert_eq!(
        b.portals[usize::try_from(ab[0].other_portal_id).unwrap()].other_cell_id,
        0x0100
    );
    // Each sees the other.
    assert_eq!(a.visible_cells, [0x0101]);
    assert_eq!(b.visible_cells, [0x0100]);
    assert_eq!(a.flags & 1, 0, "not seen from outside");
    // The statics record says two cells and exists.
    let info = s.statics(lb).unwrap();
    assert_eq!(info.num_cells, 2);
    // A point in the first stands in it, over its floor; a ray down from there meets the floor.
    let e = s.environment(a.environment).unwrap();
    let vs = &e.cells[usize::from(a.cell_struct)].vertex_array.vertices;
    #[allow(clippy::cast_precision_loss)]
    let n = vs.len() as f32;
    let mid = vs.iter().fold(Vec3::ZERO, |m, v| {
        let p = localtoglobal(&a.frame, v.position);
        Vec3::new(m.x + p.x / n, m.y + p.y / n, m.z + p.z / n)
    });
    let (c, under) = s.cell_at(lb, mid).expect("the middle stands in a cell");
    assert_eq!(c, 0x0100);
    let (c, floor) = s.cell_hit(lb, mid, Vec3::new(0.0, 0.0, -1.0)).unwrap();
    assert_eq!(c, 0x0100);
    assert!(
        (floor.z - under).abs() < 1e-3,
        "a floor at {under}, met at {floor:?}"
    );
    assert!(mid.z > floor.z);
    // A static stood in it and a spawn placed in it are the cell's.
    s.apply(&Op::PlaceCellStatic {
        landblock: lb,
        cell: format!("{pack}:cell-1"),
        model: DataId(0x0200_040B),
        at: [floor.x, floor.y, floor.z],
        rotation: Rotation::Heading(0.0),
    })
    .unwrap();
    let stood = &s.cells(lb)[&0x0100].static_objects;
    assert_eq!(stood.len(), 1);
    assert!((stood[0].frame.origin.z - floor.z).abs() < 1e-3);
    s.apply(&Op::PlaceSpawnInCell {
        landblock: lb,
        cell: 0x0100,
        weenie: lyceum_foundry::edit::Weenie::Class(1),
        origin: floor,
        heading: 0.0,
    })
    .unwrap();
    assert!(s
        .spawns()
        .iter()
        .any(|x| x.cell == (u32::from(lb) << 16) | 0x0100));
    // The walk: each reached, each with its floor; but the shut ends are named.
    let found = s.walk(lb);
    assert!(
        found
            .iter()
            .all(|f| !f.text.contains("falls") && !f.text.contains("reaches")),
        "{found:#?}"
    );
    // Taken away, the first's portal shuts again.
    s.apply(&Op::RemoveCell {
        landblock: lb,
        name: format!("{pack}:cell-2"),
    })
    .unwrap();
    let cells = s.cells(lb);
    assert_eq!(cells.len(), 1);
    assert!(
        cells[&0x0100].portals.iter().all(|p| p.flags & 4 != 0)
            || cells[&0x0100].portals.is_empty()
    );
    assert!(cells[&0x0100].visible_cells.is_empty());
}

/// The walk checks over every retail dungeon and building: every portal back, every floor found,
/// but for what retail itself holds (counted here).
#[test]
fn the_walk_checks_hold_for_retails_cells() {
    let s: RetailDatStore = dereth_dat::testing::open_store_or_fail();
    let mut by_lb: BTreeMap<u16, BTreeMap<u16, EnvCell>> = BTreeMap::new();
    for id in s.cell().iter_ids() {
        if dereth_dat::classify_cell_id(id) == Some(DbType::Cell) {
            #[allow(clippy::cast_possible_truncation)]
            let (lb, c) = ((id.raw() >> 16) as u16, (id.raw() & 0xFFFF) as u16);
            let cell = EnvCell::decode_payload(id, &s.read_cell(id).unwrap()).unwrap();
            by_lb.entry(lb).or_default().insert(c, cell);
        }
    }
    let mut envs: BTreeMap<u32, Option<Environment>> = BTreeMap::new();
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    for cells in by_lb.values() {
        for c in cells.values() {
            envs.entry(c.environment.raw()).or_insert_with(|| {
                s.read_typed(DbType::Environment, c.environment)
                    .ok()
                    .and_then(|b| Environment::decode_payload(c.environment, &b).ok())
            });
        }
        let which: BTreeSet<u16> = cells.keys().copied().collect();
        // Buildings' cells are reached through the outdoors too: no start is named.
        let found = walk::check(cells, &which, None, &|id| {
            envs.get(&id.raw()).cloned().flatten()
        });
        // A cell some portal of which (its own, or one of a cell it leads to) has no way back.
        let one_way: BTreeSet<u16> = found
            .iter()
            .filter(|f| f.text.contains("does not lead back"))
            .flat_map(|f| {
                std::iter::once(f.cell).chain(
                    cells[&f.cell]
                        .portals
                        .iter()
                        .filter(|p| p.flags & 4 == 0)
                        .map(|p| u16::try_from(p.other_cell_id).unwrap_or(0)),
                )
            })
            .collect();
        for f in &found {
            let kind = if f.text.contains("does not lead back") {
                "no way back"
            } else if f.text.contains("does not meet") {
                "portals apart"
            } else if f.text.contains("falls") && one_way.contains(&f.cell) {
                "no floor, by a portal with no way back"
            } else if f.text.contains("falls further") {
                "a fall deeper than the walk looks"
            } else if f.text.contains("falls") {
                "no floor"
            } else if f.text.contains("reaches") {
                "not reached"
            } else {
                "other"
            };
            *kinds.entry(kind.to_owned()).or_default() += 1;
        }
    }
    println!("{kinds:#?}");
    let n = |k: &str| kinds.get(k).copied().unwrap_or(0);
    // Retail's own, as measured: 1,778 portals with no portal back, and 718 cells whose floor
    // is such a portal or lies beyond one; 244 cells over shafts deeper than the walk looks; and
    // 26 others (rooms of a few towers and a cave) with nothing the walk finds under them.
    assert_eq!(n("no way back"), 1_778, "{kinds:#?}");
    assert_eq!(
        n("no floor, by a portal with no way back"),
        718,
        "{kinds:#?}"
    );
    assert_eq!(n("a fall deeper than the walk looks"), 244, "{kinds:#?}");
    assert_eq!(n("no floor"), 26, "{kinds:#?}");
    assert_eq!(n("portals apart"), 0, "{kinds:#?}");
    assert_eq!(n("not reached") + n("other"), 0, "{kinds:#?}");
}

#[test]
fn a_building_moved_takes_its_cells_and_their_statics_and_back_to_its_place_changes_nothing() {
    let (_dir, mut s) = session("dungeon-building");
    let lb = 0xA9B4;
    let info: LandblockInfo = s.statics(lb).unwrap();
    let b = info.buildings[0].clone();
    let before = s.cells(lb);
    let doorways: Vec<u16> = b.portals.iter().map(|p| p.other_cell_id).collect();
    let inside = lyceum_foundry::cells::building_cells(&doorways, &before);
    assert!(!inside.is_empty());
    let to = Frame::new(
        Vec3::new(
            b.frame.origin.x + 6.0,
            b.frame.origin.y - 4.0,
            b.frame.origin.z,
        ),
        lyceum_foundry::compile::heading_quat(90.0),
    );
    s.apply(&Op::MoveBuilding {
        landblock: lb,
        index: 0,
        at: [to.origin.x, to.origin.y, to.origin.z],
        rotation: Rotation::Heading(90.0),
    })
    .unwrap();
    let after = s.cells(lb);
    let moved = s.statics(lb).unwrap().buildings[0].frame;
    assert!((moved.origin.x - to.origin.x).abs() < 1e-4);
    // Each cell, and each of its statics, where the building took it: the same place relative
    // to the building.
    let rel = |f: &Frame, home: &Frame| dereth_primitives::frame::globaltolocal(home, f.origin);
    for c in &inside {
        let (x, y) = (&before[c], &after[c]);
        let (p, q) = (rel(&x.frame, &b.frame), rel(&y.frame, &moved));
        assert!(
            (p.x - q.x).abs() < 1e-3 && (p.y - q.y).abs() < 1e-3 && (p.z - q.z).abs() < 1e-3,
            "cell {c:04X}"
        );
        for (u, v) in x.static_objects.iter().zip(&y.static_objects) {
            let (p, q) = (rel(&u.frame, &b.frame), rel(&v.frame, &moved));
            assert!(
                (p.x - q.x).abs() < 1e-3 && (p.y - q.y).abs() < 1e-3,
                "a static of {c:04X}"
            );
        }
        // Its portals and lists stay as they were.
        assert_eq!(x.portals, y.portals);
        assert_eq!(x.visible_cells, y.visible_cells);
    }
    // A cell of another building stays.
    for (c, x) in &before {
        if !inside.contains(c) {
            assert_eq!(x, &after[c], "cell {c:04X}");
        }
    }
    // Moved with the spawns that go with it: a spawn in one of its rooms keeps its place in the
    // room.
    let room = *inside.iter().next().unwrap();
    let at = after[&room].frame.origin;
    s.apply(&Op::PlaceSpawnInCell {
        landblock: lb,
        cell: room,
        weenie: lyceum_foundry::edit::Weenie::Class(1),
        origin: at,
        heading: 0.0,
    })
    .unwrap();
    let op = s
        .move_building(
            lb,
            0,
            [moved.origin.x + 3.0, moved.origin.y, moved.origin.z],
            Rotation::Heading(90.0),
        )
        .unwrap();
    s.apply(&op).unwrap();
    let spawn = s
        .spawns()
        .into_iter()
        .find(|x| x.cell == (u32::from(lb) << 16) | u32::from(room))
        .unwrap();
    let room_now = s.cells(lb)[&room].frame.origin;
    assert!(
        (spawn.frame.origin.x - room_now.x).abs() < 1e-2
            && (spawn.frame.origin.y - room_now.y).abs() < 1e-2,
        "{:?} in a room at {room_now:?}",
        spawn.frame.origin
    );
    s.undo().unwrap();
    s.undo().unwrap();
    // Back to its own place: the compiled cells are the files' again.
    let q = b.frame.rotation;
    s.apply(&Op::MoveBuilding {
        landblock: lb,
        index: 0,
        at: [b.frame.origin.x, b.frame.origin.y, b.frame.origin.z],
        rotation: Rotation::Quat([q.w, q.x, q.y, q.z]),
    })
    .unwrap();
    assert_eq!(s.cells(lb), before);
    let _ = localtoglobal;
}

/// Three corridor pieces in a row in landblock `lb`, each joined to the one before: their
/// numbers, in order.
fn three_in_a_row(s: &mut Session, lb: u16, piece: &Piece) -> Vec<u16> {
    let env = s.environment(piece.environment).unwrap();
    let pack = s.packs()[s.target()].name.clone();
    let mut frame = Frame::new(Vec3::new(96.0, 96.0, -40.0), Quat::IDENTITY);
    for i in 1..=3 {
        let links = if i == 1 {
            vec![Link::Shut, Link::Shut]
        } else {
            vec![
                Link::Shut,
                Link::Cell {
                    cell: format!("{pack}:cell-{}", i - 1),
                    portal: 0,
                },
            ]
        };
        let q = frame.rotation;
        s.apply(&Op::PlaceCell {
            landblock: lb,
            environment: piece.environment,
            piece: piece.piece,
            surfaces: piece.surfaces.clone(),
            at: [frame.origin.x, frame.origin.y, frame.origin.z],
            rotation: Rotation::Quat([q.w, q.x, q.y, q.z]),
            links,
        })
        .unwrap();
        // The next joined to this one's portal 0 by its own portal 1.
        let target = opening(&env, piece.piece, 0, &frame).unwrap();
        frame = snap(&env, piece.piece, 1, &target, 0.0, 90.0).unwrap().0;
    }
    let names = s.cell_names(lb);
    (1..=3)
        .map(|i| {
            *names
                .iter()
                .find(|(_, n)| **n == format!("{pack}:cell-{i}"))
                .unwrap()
                .0
        })
        .collect()
}

#[test]
fn a_spawn_in_a_new_cell_stays_in_it_when_a_cell_numbered_before_it_is_taken_away() {
    let (_dir, mut s) = session("dungeon-numbers");
    let store = dereth_dat::testing::open_store_or_fail();
    let lb = empty_landblock(&store);
    let piece = corridor(&catalog(s.base()).pieces);
    let pack = s.packs()[s.target()].name.clone();
    let n = three_in_a_row(&mut s, lb, &piece);
    assert_eq!(n, [0x0100, 0x0101, 0x0102]);
    // A spawn in the third, written with the cell's symbol.
    let floor = s.cells(lb)[&n[2]].frame.origin;
    let applied = s
        .apply(&Op::PlaceSpawnInCell {
            landblock: lb,
            cell: n[2],
            weenie: lyceum_foundry::edit::Weenie::Class(1),
            origin: floor,
            heading: 0.0,
        })
        .unwrap();
    let file = applied.spawned.unwrap();
    let text = s.packs()[s.target()]
        .sql
        .iter()
        .find(|f| f.path == file)
        .map(|f| f.text.clone())
        .unwrap();
    assert!(
        text.contains(&format!("{{{{cell:cell-3@{lb:04X}}}}}")),
        "{text}"
    );
    let third = (u32::from(lb) << 16) | u32::from(n[2]);
    let cell_of_spawn = |s: &Session| {
        s.spawns()
            .into_iter()
            .find(|x| x.file == file)
            .unwrap()
            .cell
    };
    assert_eq!(cell_of_spawn(&s), third);
    // The first taken away: the third keeps its number, and so does the spawn's cell.
    s.apply(&Op::RemoveCell {
        landblock: lb,
        name: format!("{pack}:cell-1"),
    })
    .unwrap();
    assert_eq!(s.cell_names(lb).get(&n[2]), Some(&format!("{pack}:cell-3")));
    assert_eq!(cell_of_spawn(&s), third);
    let compiled = s
        .compiled()
        .sql
        .values()
        .find(|t| t.contains("landblock_instance") && t.contains(&format!("0x{third:08X}")));
    assert!(
        compiled.is_some(),
        "the built SQL places the spawn in {third:08X}"
    );
    // The first's number stands closed: no portal, far under the landblock, no list; the
    // landblock still counts every number.
    let closed = &s.cells(lb)[&n[0]];
    assert!(closed.portals.is_empty() && closed.visible_cells.is_empty());
    assert!((closed.frame.origin.z - lyceum_foundry::cells::CLOSED_DEPTH).abs() < 1e-3);
    assert_eq!(s.statics(lb).unwrap().num_cells, 3);
    assert!(
        s.walk(lb).iter().all(|f| f.cell != n[0]),
        "{:#?}",
        s.walk(lb)
    );
    // A new cell takes a number never held: after the third, not the first's.
    s.apply(&Op::PlaceCell {
        landblock: lb,
        environment: piece.environment,
        piece: piece.piece,
        surfaces: piece.surfaces.clone(),
        at: [20.0, 20.0, -40.0],
        rotation: Rotation::Heading(0.0),
        links: vec![Link::Shut, Link::Shut],
    })
    .unwrap();
    let fourth = s
        .cell_names(lb)
        .into_iter()
        .find(|(_, name)| *name == format!("{pack}:cell-4"))
        .unwrap()
        .0;
    assert_eq!(fourth, n[2] + 1);
    assert_eq!(cell_of_spawn(&s), third);
    // The lockfile keeps every cell's number, the one taken away's among them.
    let lock = &s.compiled().lock;
    assert_eq!(
        lock.get(
            lyceum_foundry::symbol::Kind::Cell,
            &format!("{pack}:cell-1@{lb:04X}")
        ),
        Some((u32::from(lb) << 16) | u32::from(n[0]))
    );
}

#[test]
fn a_spawn_naming_a_cell_no_pack_places_stops_the_build_with_where() {
    let (_dir, mut s) = session("dungeon-no-cell");
    let store = dereth_dat::testing::open_store_or_fail();
    let lb = empty_landblock(&store);
    let piece = corridor(&catalog(s.base()).pieces);
    let pack = s.packs()[s.target()].name.clone();
    let n = three_in_a_row(&mut s, lb, &piece);
    let applied = s
        .apply(&Op::PlaceSpawnInCell {
            landblock: lb,
            cell: n[1],
            weenie: lyceum_foundry::edit::Weenie::Class(1),
            origin: s.cells(lb)[&n[1]].frame.origin,
            heading: 0.0,
        })
        .unwrap();
    assert!(applied.spawned.is_some());
    // Its cell taken away: refused, with the spawn's file named.
    let e = s
        .apply(&Op::RemoveCell {
            landblock: lb,
            name: format!("{pack}:cell-2"),
        })
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("no pack places cell") && e.contains("cell-2"),
        "{e}"
    );
}

#[test]
fn the_pieces_are_sorted_by_shape_look_and_the_pieces_retail_joins_them_to() {
    let set = lyceum_foundry::dats::DatSet::open(&dereth_dat::testing::dat_dir()).unwrap();
    let c = catalog(&set);
    // Every shape has its pieces.
    let mut shapes: BTreeMap<Shape, usize> = BTreeMap::new();
    for p in &c.pieces {
        *shapes.entry(p.shape).or_default() += 1;
    }
    println!("{} pieces: {shapes:?}", c.pieces.len());
    assert_eq!(shapes.values().sum::<usize>(), c.pieces.len());
    assert!(
        Shape::ALL
            .iter()
            .all(|s| shapes.get(s).copied().unwrap_or(0) >= 50),
        "{shapes:?}"
    );
    // The commonest piece with two ways out is a corridor, and retail joins it most to the
    // other pieces of its own environment.
    let corridor = corridor(&c.pieces);
    assert_eq!(corridor.shape, Shape::Corridor);
    let (first, times) = corridor.joins[0];
    assert_eq!(c.pieces[first].environment, corridor.environment);
    assert!(times > 1_000, "{times}");
    // Looks shared by landblocks: families of several, the largest of tens; every piece drawn in
    // at least one, every family with its pieces.
    let shared = c.families.iter().filter(|f| f.landblocks.len() > 1).count();
    println!(
        "{} families, {shared} of several landblocks, the largest {:?}",
        c.families.len(),
        c.families
            .iter()
            .take(5)
            .map(|f| (f.landblocks.len(), f.cells, f.pieces))
            .collect::<Vec<_>>()
    );
    assert!(shared >= 100, "{shared}");
    assert!(c.families[0].landblocks.len() >= 30);
    assert!(c.pieces.iter().all(|p| !p.looks.is_empty()));
    assert!(c
        .families
        .iter()
        .all(|f| f.pieces > 0 && !f.landblocks.is_empty()));
    let cells: usize = c.families.iter().map(|f| f.cells).sum();
    assert_eq!(cells, 734_976, "every retail cell in one family");
    // The Drudge Hideout's look is shared with other landblocks.
    let hideout = c
        .families
        .iter()
        .find(|f| f.landblocks.iter().any(|(lb, _)| *lb == 0x019E))
        .unwrap();
    println!(
        "the hideout's family: {:04X?}",
        hideout
            .landblocks
            .iter()
            .take(8)
            .map(|l| l.0)
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_dungeons_dead_end_reshaped_as_a_corridor_keeps_its_way_and_a_new_piece_joins_its_free_end() {
    const HIDEOUT: u16 = 0x019E;
    let (dir, mut s) = session("dungeon-reshape");
    let pieces = catalog(s.base()).pieces;
    let cells = s.cells(HIDEOUT);
    // Every cell fits its own place with its own piece, every way kept (a piece the same turned
    // half about may fit as well).
    for c in cells.values() {
        let e = s.environment(c.environment).unwrap();
        let (_, links) =
            lyceum_foundry::pieces::reshape_fit(c, &e, &e, c.cell_struct, 90.0).expect("itself");
        assert!(links.iter().all(|l| *l == Link::Keep));
    }
    // A dead end of the hideout, and a piece with a way more that takes its place: of its own
    // environment when one does.
    let (n, old, piece, frame, links) = cells
        .iter()
        .filter(|(_, c)| c.portals.len() == 1 && c.portals[0].flags & 4 == 0)
        .find_map(|(n, old)| {
            let old_env = s.environment(old.environment)?;
            let mut by_kin: Vec<&Piece> = pieces.iter().filter(|p| p.portals == 2).collect();
            by_kin.sort_by_key(|p| (p.environment != old.environment, std::cmp::Reverse(p.cells)));
            by_kin.into_iter().find_map(|p| {
                let env = s.environment(p.environment)?;
                lyceum_foundry::pieces::reshape_fit(old, &old_env, &env, p.piece, 90.0)
                    .map(|(f, l)| (*n, old.clone(), p.clone(), f, l))
            })
        })
        .expect("a dead end a corridor fits");
    println!(
        "dead end {n:04X} ({:04X}/{}) as {}",
        old.environment.raw() & 0xFFFF,
        old.cell_struct,
        piece.label()
    );
    let neighbor = u16::try_from(old.portals[0].other_cell_id).unwrap();
    assert_eq!(links.iter().filter(|l| **l == Link::Keep).count(), 1);
    let free = links.iter().position(|l| *l == Link::Shut).unwrap();
    let q = frame.rotation;
    s.apply(&Op::ReplaceCell {
        landblock: HIDEOUT,
        cell: n,
        environment: piece.environment,
        piece: piece.piece,
        surfaces: old.surfaces.clone(),
        at: [frame.origin.x, frame.origin.y, frame.origin.z],
        rotation: Rotation::Quat([q.w, q.x, q.y, q.z]),
        links: links.clone(),
    })
    .unwrap();
    let after = s.cells(HIDEOUT);
    let reshaped = &after[&n];
    assert_eq!(
        (reshaped.environment, reshaped.cell_struct),
        (piece.environment, piece.piece)
    );
    assert_eq!(
        reshaped.static_objects, old.static_objects,
        "its statics kept"
    );
    // Its way to the neighbor kept, and the neighbor's way back names its new portal.
    assert_eq!(reshaped.portals.len(), 1);
    assert_eq!(reshaped.portals[0].other_cell_id, u32::from(neighbor));
    let back = after[&neighbor]
        .portals
        .iter()
        .find(|p| p.other_cell_id == u32::from(n))
        .unwrap();
    assert_eq!(back.other_portal_id, 0);
    // A new piece joined to its free end: one with a portal that meets it, by that portal.
    let env = s.environment(piece.environment).unwrap();
    let target = opening(&env, piece.piece, free, &frame).unwrap();
    let (next, join, nf) = pieces
        .iter()
        .filter(|p| p.portals >= 2)
        .find_map(|p| {
            let e = s.environment(p.environment)?;
            (0..p.portals).find_map(|k| {
                snap(&e, p.piece, k, &target, 0.0, 90.0)
                    .filter(|(_, m)| *m < 0.01)
                    .map(|(f, _)| (p.clone(), k, f))
            })
        })
        .expect("a piece that meets the free end");
    let nq = nf.rotation;
    s.apply(&Op::PlaceCell {
        landblock: HIDEOUT,
        environment: next.environment,
        piece: next.piece,
        surfaces: next.surfaces.clone(),
        at: [nf.origin.x, nf.origin.y, nf.origin.z],
        rotation: Rotation::Quat([nq.w, nq.x, nq.y, nq.z]),
        links: (0..next.portals)
            .map(|k| {
                if k == join {
                    Link::Base {
                        cell: n,
                        portal: free,
                    }
                } else {
                    Link::Shut
                }
            })
            .collect(),
    })
    .unwrap();
    let after = s.cells(HIDEOUT);
    let new = *s.cell_names(HIDEOUT).keys().next().unwrap();
    assert_eq!(
        new,
        cells.keys().max().unwrap() + 1,
        "numbered after the hideout's"
    );
    let joined = after[&n]
        .portals
        .iter()
        .find(|p| p.other_cell_id == u32::from(new))
        .expect("the reshaped cell leads to the new");
    assert_eq!(joined.flags & 1, 1, "an exact match");
    assert!(after[&new]
        .portals
        .iter()
        .any(|p| p.other_cell_id == u32::from(n) && p.flags & 1 == 1));
    assert!(after[&n].visible_cells.contains(&new) && after[&new].visible_cells.contains(&n));
    // The walk finds every way leading back, and every portal met.
    let envs = |id: DataId| s.environment(id);
    let which: BTreeSet<u16> = [n, new, neighbor].into();
    let found = walk::check(&after, &which, None, &envs);
    assert!(
        found
            .iter()
            .all(|f| !f.text.contains("does not lead back") && !f.text.contains("does not meet")),
        "{found:#?}"
    );
    // Saved and read again, a full build gives the same.
    s.save().unwrap();
    let text = std::fs::read_to_string(
        dir.path()
            .join(format!("holtburg/edits/landblocks/{HIDEOUT:04X}.toml")),
    )
    .unwrap();
    assert!(
        text.contains("[[replace-cell]]") && text.contains("keep"),
        "{text}"
    );
    assert!(text.contains(&format!("0x{n:04X}:{free}")), "{text}");
    let reopened = Session::open(&s.world().path, &dereth_dat::testing::dat_dir(), None).unwrap();
    assert_eq!(reopened.compiled().records, s.compiled().records);
    // Taken away, the new cell leaves the reshaped cell's end free again.
    let mut s = reopened;
    let name = s.cell_names(HIDEOUT)[&new].clone();
    s.apply(&Op::RemoveCell {
        landblock: HIDEOUT,
        name,
    })
    .unwrap();
    assert!(s.cells(HIDEOUT)[&n]
        .portals
        .iter()
        .all(|p| p.other_cell_id != u32::from(new)));
}
