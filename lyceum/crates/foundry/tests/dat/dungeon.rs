//! Behaviour: none (tooling: the dungeon kit over the retail files: pieces, placing, walking, and moving a building; pieces placed whole, taken away and dressed)
//! Fixture: the retail files; for the dungeons' floors, the world database too
//! (`EMPYREAN_WORLD_SQL`, else the newest in the importer's cache).

use std::collections::{BTreeMap, BTreeSet};

use dereth_assets::world::{CellLandblock, EnvCell, LandblockInfo};
use dereth_assets::{Decode, Environment};
use dereth_dat::{DbType, ModernDat, RetailDatStore};
use dereth_primitives::frame::localtoglobal;
use dereth_primitives::{DataId, Frame, Quat, Vec3};
use lyceum_foundry::edit::{NewPiece, Op, Session};
use lyceum_foundry::grid::{fit_piece, on_lattice, quarter_turns, Dungeon, PITCH, RISE};
use lyceum_foundry::kit::{placed_pieces, Kind};
use lyceum_foundry::pieces::{catalog, opening, snap, Piece, Shape};
use lyceum_foundry::room::Solid;
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
fn a_cell_taken_away_takes_its_spawns_and_a_spawn_naming_a_cell_no_pack_places_stops_the_build() {
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
    let file = applied.spawned.expect("the spawn's file");
    let text = s.sql_text(&file).expect("its text");
    // Its cell taken away: the spawn goes with it, in the same step.
    s.apply(&Op::RemoveCell {
        landblock: lb,
        name: format!("{pack}:cell-2"),
    })
    .unwrap();
    assert!(s.sql_text(&file).is_none());
    assert!(!s.spawns().iter().any(|x| x.file == file));
    s.undo().unwrap();
    assert!(s.sql_text(&file).is_some());
    // A spawn written to name a cell no pack places: refused, with the spawn's file named.
    let e = s
        .apply(&Op::WriteSql {
            file: file.clone(),
            text: Some(text.replace("cell-2", "cell-9")),
        })
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("no pack places cell") && e.contains("cell-9"),
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
                    .filter(|(_, l)| {
                        l.iter().filter(|link| **link == Link::Keep).count() == old.portals.len()
                    })
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

#[test]
fn replacing_a_junction_with_a_dead_end_warns_and_repairs_neighbor_portal_indices() {
    const LB: u16 = 0x019E;
    let (dir, mut s) = session("dungeon-drop-doorway");
    let before = s.cells(LB);
    let pieces = catalog(s.base()).pieces;
    let (n, old, piece, frame, links) = before
        .iter()
        .filter(|(_, c)| c.portals.len() >= 2)
        .find_map(|(n, old)| {
            let old_env = s.environment(old.environment)?;
            pieces
                .iter()
                .filter(|p| p.portals == 1 && p.environment == old.environment)
                .find_map(|p| {
                    let env = s.environment(p.environment)?;
                    let (f, links) =
                        lyceum_foundry::pieces::reshape_fit(old, &old_env, &env, p.piece, 90.0)?;
                    links
                        .contains(&Link::Keep)
                        .then(|| (*n, old.clone(), p.clone(), f, links))
                })
        })
        .expect("a junction with a matching dead end");
    let q = frame.rotation;
    s.apply(&Op::ReplaceCell {
        landblock: LB,
        cell: n,
        environment: piece.environment,
        piece: piece.piece,
        surfaces: old.surfaces.clone(),
        at: [frame.origin.x, frame.origin.y, frame.origin.z],
        rotation: Rotation::Quat([q.w, q.x, q.y, q.z]),
        links,
    })
    .unwrap();
    let after = s.cells(LB);
    assert_eq!(after[&n].portals.len(), 1);
    assert_eq!(s.compiled().warnings.len(), 1);
    assert!(s.compiled().warnings[&LB][0]
        .contains(&format!("drops {} doorway(s)", old.portals.len() - 1)));
    for (cell_id, cell) in &after {
        for (index, portal) in cell.portals.iter().enumerate() {
            if portal.other_cell_id == u32::MAX {
                continue;
            }
            let neighbor = &after[&u16::try_from(portal.other_cell_id).unwrap()];
            let back = &neighbor.portals[usize::try_from(portal.other_portal_id).unwrap()];
            assert_eq!(back.other_cell_id, u32::from(*cell_id));
            assert_eq!(usize::try_from(back.other_portal_id).unwrap(), index);
        }
    }
    s.undo().unwrap();
    assert_eq!(s.cells(LB), before);
    assert!(s.compiled().warnings.is_empty());
    s.redo().unwrap();
    assert_eq!(s.cells(LB), after);
    s.save().unwrap();
    let reopened = Session::open(&s.world().path, &dereth_dat::testing::dat_dir(), None).unwrap();
    assert_eq!(reopened.cells(LB), after);
    assert_eq!(reopened.compiled().warnings, s.compiled().warnings);
    let built = lyceum_foundry::compile::build(
        &s.world().path,
        &dereth_dat::testing::dat_dir(),
        Some(&dir.path().join("baked")),
        false,
    )
    .unwrap();
    assert!(built
        .warnings
        .iter()
        .any(|w| w.contains("replacement drops")));
}

#[test]
fn dropping_all_of_a_buildings_room_doorways_removes_its_outdoor_entrance() {
    let (_dir, mut s) = session("dungeon-drop-entrance");
    let lb = 0xA9B4;
    let info = s.statics(lb).unwrap();
    let n = info.buildings[0].portals[0].other_cell_id;
    let old = s.cells(lb)[&n].clone();
    let env = s.environment(old.environment).unwrap();
    let q = old.frame.rotation;
    s.apply(&Op::ReplaceCell {
        landblock: lb,
        cell: n,
        environment: old.environment,
        piece: old.cell_struct,
        surfaces: old.surfaces.clone(),
        at: [old.frame.origin.x, old.frame.origin.y, old.frame.origin.z],
        rotation: Rotation::Quat([q.w, q.x, q.y, q.z]),
        links: vec![Link::Shut; env.cells[usize::from(old.cell_struct)].portals.len()],
    })
    .unwrap();
    assert!(s.cells(lb)[&n].portals.is_empty());
    assert!(s
        .statics(lb)
        .unwrap()
        .buildings
        .iter()
        .flat_map(|b| &b.portals)
        .all(|p| p.other_cell_id != n));
    assert_eq!(s.compiled().warnings[&lb].len(), 1);
    s.undo().unwrap();
    assert_eq!(s.statics(lb).unwrap(), info);
}

#[test]
fn retails_dungeon_rooms_stand_on_a_ten_meter_grid_at_quarter_turns_with_floors_six_meters_apart() {
    let (_dir, world) = super::sample::sample("lattice-retail");
    let s = Session::open(&world, &dereth_dat::testing::dat_dir(), None).unwrap();
    let dungeons = s.retail_dungeons();
    let mut envs: BTreeMap<DataId, Option<dereth_assets::Environment>> = BTreeMap::new();
    let (mut cells, mut on, mut turned, mut floored) = (0usize, 0usize, 0usize, 0usize);
    let (mut whole, mut most) = (0usize, 0usize);
    // Each doorway of a room on the grid: at the middle of a square's side, or elsewhere.
    let (mut doorways, mut on_sides) = (0usize, 0usize);
    let side = |v: f32| {
        let r = v.rem_euclid(PITCH);
        (r - PITCH / 2.0).abs() < 0.05
    };
    let mid = |v: f32| {
        let r = v.rem_euclid(PITCH);
        r < 0.05 || r > PITCH - 0.05
    };
    for &lb in dungeons.keys() {
        let here = s.cells(lb);
        let mut here_on = 0;
        for c in here.values() {
            cells += 1;
            let grid = on_lattice(&c.frame);
            let turn = quarter_turns(&c.frame).is_some();
            here_on += usize::from(grid && turn);
            on += usize::from(grid);
            turned += usize::from(turn);
            floored += usize::from(
                (c.frame.origin.z / RISE - (c.frame.origin.z / RISE).round()).abs() < 0.01,
            );
            if !(grid && turn) {
                continue;
            }
            let e = envs
                .entry(c.environment)
                .or_insert_with(|| s.environment(c.environment));
            let Some(env) = e.as_ref() else { continue };
            let slots = env
                .cells
                .get(usize::from(c.cell_struct))
                .map_or(0, |cs| cs.portals.len());
            for slot in 0..slots {
                let Some(o) = opening(env, c.cell_struct, slot, &c.frame) else {
                    continue;
                };
                if o.out.z.abs() > 0.5 {
                    continue;
                }
                doorways += 1;
                let m = o.middle;
                on_sides += usize::from((side(m.x) && mid(m.y)) || (mid(m.x) && side(m.y)));
            }
        }
        whole += usize::from(here_on == here.len());
        most += usize::from(here_on * 10 >= here.len() * 9);
    }
    #[allow(clippy::cast_precision_loss)]
    let pc = |n: usize, of: usize| 100.0 * n as f32 / of as f32;
    println!(
        "{} dungeons, {cells} rooms: on the {PITCH} m grid {on} ({:.2}%), at quarter turns {turned} \
         ({:.2}%), floors on {RISE} m steps {floored} ({:.2}%); dungeons wholly on it {whole} \
         ({:.1}%), nine tenths on it {most} ({:.1}%); level doorways {doorways}, at a square's side \
         {on_sides} ({:.2}%)",
        dungeons.len(),
        pc(on, cells),
        pc(turned, cells),
        pc(floored, cells),
        pc(whole, dungeons.len()),
        pc(most, dungeons.len()),
        pc(on_sides, doorways),
    );
    assert_eq!(dungeons.len(), 1970);
    assert!(on * 100 >= cells * 99, "{on} of {cells}");
    assert!(turned * 1000 >= cells * 998, "{turned} of {cells}");
    assert!(floored * 100 >= cells * 99, "{floored} of {cells}");
    assert!(whole * 10 >= dungeons.len() * 9, "{whole}");
}

/// How many surfaces a piece's polygons name: one past the highest surface any of them uses.
fn surface_slots(env: &dereth_assets::Environment, piece: u16) -> usize {
    env.cells.get(usize::from(piece)).map_or(0, |cs| {
        let portals: std::collections::BTreeSet<u16> = cs.portals.iter().copied().collect();
        cs.polygons
            .iter()
            .enumerate()
            .filter(|(i, _)| !u16::try_from(*i).is_ok_and(|i| portals.contains(&i)))
            .map(|(_, p)| usize::from(p.pos_surface) + 1)
            .max()
            .unwrap_or(0)
    })
}

#[test]
fn a_looks_surfaces_fit_the_pieces_retail_drew_in_it_and_every_piece_of_their_environment() {
    let set = lyceum_foundry::dats::DatSet::open(&dereth_dat::testing::dat_dir()).unwrap();
    let catalog = lyceum_foundry::pieces::catalog(&set);
    let mut envs: BTreeMap<DataId, Option<dereth_assets::Environment>> = BTreeMap::new();
    let mut env = |id: DataId| -> Option<dereth_assets::Environment> {
        envs.entry(id)
            .or_insert_with(|| {
                use dereth_assets::Decode;
                set.read_portal(id)
                    .and_then(|b| dereth_assets::Environment::decode_payload(id, &b).ok())
            })
            .clone()
    };
    // Each retail piece's look: its surface list as long as the surfaces its polygons name.
    let (mut looks, mut fitting) = (0usize, 0usize);
    // Pieces of one environment name as many surfaces as each other.
    let mut slots_of_env: BTreeMap<DataId, std::collections::BTreeSet<usize>> = BTreeMap::new();
    for p in &catalog.pieces {
        let Some(e) = env(p.environment) else {
            continue;
        };
        let slots = surface_slots(&e, p.piece);
        slots_of_env.entry(p.environment).or_default().insert(slots);
        for l in &p.looks {
            looks += 1;
            fitting += usize::from(l.surfaces.len() >= slots);
        }
    }
    let alike = slots_of_env.values().filter(|s| s.len() == 1).count();
    // The pieces a look could draw: those retail drew in it, and those whose environment it drew
    // another piece of, with as many surfaces.
    let (mut drawn, mut by_env) = (0usize, 0usize);
    for f in 0..catalog.families.len() {
        let mut lists: BTreeMap<DataId, Vec<usize>> = BTreeMap::new();
        for p in &catalog.pieces {
            if let Some(l) = p.looks.iter().find(|l| l.family == f) {
                lists
                    .entry(p.environment)
                    .or_default()
                    .push(l.surfaces.len());
            }
        }
        for p in &catalog.pieces {
            if p.looks.iter().any(|l| l.family == f) {
                drawn += 1;
            } else if lists.contains_key(&p.environment) {
                by_env += 1;
            }
        }
    }
    println!(
        "{} pieces, {} environments, {} looks; surface lists as long as their pieces' surfaces: \
         {fitting} of {looks}; environments whose pieces all name as many surfaces: {alike} of {}; \
         piece-and-look pairs retail drew {drawn}, more drawn by another piece of the same \
         environment {by_env}",
        catalog.pieces.len(),
        slots_of_env.len(),
        catalog.families.len(),
        slots_of_env.len(),
    );
    assert_eq!(fitting, looks);
}

#[test]
fn a_static_added_to_one_of_the_worlds_rooms_stands_in_it_after_its_own_and_moves_and_goes_by_its_row(
) {
    let (_dir, mut s) = session("dungeon-add-cell-static");
    let (lb, cell) = (0x019E, 0x0105);
    let own = s.cells(lb)[&cell].static_objects.len();
    let at = s.cells(lb)[&cell].frame.origin;
    s.apply(&Op::AddCellStatic {
        landblock: lb,
        cell,
        model: DataId(0x0200_040B),
        at: [at.x, at.y, at.z],
        rotation: Rotation::Heading(90.0),
    })
    .unwrap();
    let c = &s.cells(lb)[&cell];
    assert_eq!(c.static_objects.len(), own + 1);
    assert_eq!(c.flags & 2, 2, "the cell says it holds statics");
    let added = c.static_objects[own];
    assert_eq!(added.id, DataId(0x0200_040B));
    assert!((added.frame.origin.x - at.x).abs() < 1e-3);
    // Moved by its place among the room's statics: its own row changes.
    s.apply(&Op::MoveCellStatic {
        landblock: lb,
        cell,
        index: own,
        at: [at.x + 1.0, at.y, at.z],
        rotation: None,
    })
    .unwrap();
    let moved = s.cells(lb)[&cell].static_objects[own];
    assert!((moved.frame.origin.x - at.x - 1.0).abs() < 1e-3);
    // The room's own statics stand as they were.
    let id = DataId((u32::from(lb) << 16) | u32::from(cell));
    let base = EnvCell::decode_payload(id, &s.base().cell.read(id).unwrap()).unwrap();
    assert_eq!(
        s.cells(lb)[&cell].static_objects[..own],
        base.static_objects[..]
    );
    // Taken away: the room as the world has it.
    s.apply(&Op::RemoveCellStatic {
        landblock: lb,
        cell,
        index: own,
    })
    .unwrap();
    assert_eq!(s.cells(lb)[&cell].static_objects.len(), own);
}

/// Holtburg Redoubt.
const REDOUBT: u16 = 0x0163;

#[test]
fn the_worlds_dungeons_place_each_environment_whole_and_holtburg_redoubts_pieces_all_stand_in_its_look(
) {
    let set = lyceum_foundry::dats::DatSet::open(&dereth_dat::testing::dat_dir()).unwrap();
    let cells = set.file(ModernDat::Cell).unwrap();
    // Every dungeon room, grouped by its landblock, environment and frame: a placement.
    let mut dungeons: BTreeSet<u16> = BTreeSet::new();
    let mut by_block: BTreeMap<u16, BTreeMap<u16, EnvCell>> = BTreeMap::new();
    for id in cells.iter_ids() {
        if dereth_dat::classify_cell_id(id) != Some(DbType::Cell) {
            continue;
        }
        let Some(c) = cells
            .read(id)
            .ok()
            .and_then(|b| EnvCell::decode_payload(id, &b).ok())
        else {
            continue;
        };
        let (lb, n) = (
            u16::try_from(id.raw() >> 16).unwrap(),
            u16::try_from(id.raw() & 0xFFFF).unwrap(),
        );
        if n == 0x0100 && c.flags & 1 == 0 {
            dungeons.insert(lb);
        }
        by_block.entry(lb).or_default().insert(n, c);
    }
    let mut parts_of: BTreeMap<DataId, usize> = BTreeMap::new();
    let (mut placements, mut whole, mut rooms, mut in_whole) = (0usize, 0usize, 0usize, 0usize);
    for lb in &dungeons {
        for p in placed_pieces(&by_block[lb]) {
            let n = *parts_of.entry(p.environment).or_insert_with(|| {
                set.read_portal(p.environment)
                    .and_then(|b| {
                        dereth_assets::Environment::decode_payload(p.environment, &b).ok()
                    })
                    .map_or(0, |e| e.cells.len())
            });
            placements += 1;
            rooms += p.cells.len();
            let parts: BTreeSet<u16> = p
                .cells
                .iter()
                .map(|c| by_block[lb][c].cell_struct)
                .collect();
            if parts.len() == n {
                whole += 1;
                in_whole += p.cells.len();
            }
        }
    }
    println!(
        "{} dungeons: {placements} placements, {whole} whole; {rooms} rooms, {in_whole} in whole \
         placements",
        dungeons.len()
    );
    assert!(
        whole * 10_000 >= placements * 9_999,
        "{whole} of {placements}"
    );
    // Holtburg Redoubt's pieces: each drawn in its look, each in a group a builder knows.
    let catalog = lyceum_foundry::pieces::catalog(&set);
    let look = catalog
        .families
        .iter()
        .position(|f| f.landblocks.iter().any(|(l, _)| *l == REDOUBT))
        .expect("the redoubt's look");
    let used: Vec<_> = catalog
        .kit
        .iter()
        .filter(|p| p.landblocks.contains_key(&REDOUBT))
        .collect();
    for p in &used {
        println!("{} {:?}", p.label(), p.form.kind);
    }
    assert_eq!(used.len(), 25, "the environments the redoubt is built of");
    assert!(used.iter().all(|p| p.in_look(look)));
    assert!(used.iter().all(|p| p.form.kind != Kind::Other));
}

/// The rooms of `cells` as spaces, but those of `but`.
fn spaces(s: &Session, cells: &BTreeMap<u16, EnvCell>, but: &[u16]) -> BTreeMap<u16, Solid> {
    cells
        .iter()
        .filter(|(n, _)| !but.contains(n))
        .filter(|(_, c)| c.frame.origin.z > lyceum_foundry::cells::CLOSED_DEPTH + 1.0)
        .filter_map(|(n, c)| {
            let e = s.environment(c.environment)?;
            Some((*n, Solid::of_cell(c, &e)?))
        })
        .collect()
}

#[test]
fn a_room_of_holtburg_redoubt_taken_away_frees_the_doorways_into_it_and_its_piece_set_back_rejoins_them(
) {
    let (_dir, mut s) = session("kit-take");
    let before = s.cells(REDOUBT);
    // The room whose ceiling stands over the entry: the piece cell 017D is part of.
    let room = placed_pieces(&before)
        .into_iter()
        .find(|p| p.holds(0x017D))
        .expect("its piece");
    assert_eq!(room.cells.len(), 10, "{room:?}");
    // The openings of the rooms around it that lead in: the room above opens down into its
    // seven upper parts through one opening.
    let ways_in = before
        .iter()
        .filter(|(n, _)| !room.holds(**n))
        .flat_map(|(n, c)| c.portals.iter().map(move |p| (*n, p)))
        .filter(|(_, p)| u16::try_from(p.other_cell_id).is_ok_and(|o| room.holds(o)))
        .map(|(n, p)| (n, p.polygon_id))
        .collect::<BTreeSet<_>>()
        .len();
    assert_eq!(ways_in, 3);
    s.apply(&Op::TakeCells {
        landblock: REDOUBT,
        cells: room.cells.clone(),
    })
    .unwrap();
    let after = s.cells(REDOUBT);
    for n in &room.cells {
        let c = &after[n];
        assert!(
            c.frame.origin.z <= lyceum_foundry::cells::CLOSED_DEPTH + 1.0
                && c.portals.is_empty()
                && c.static_objects.is_empty(),
            "{n:04X} closed: {c:?}"
        );
    }
    assert!(after.iter().all(|(_, c)| c
        .portals
        .iter()
        .all(|p| !u16::try_from(p.other_cell_id).is_ok_and(|o| room.holds(o)))));
    // Every way that led in is a free doorway now.
    let free = s.free_doorways(REDOUBT, &[]);
    assert_eq!(free.len(), ways_in, "{free:?}");
    // The same piece set back where it stood joins every one of them.
    let env = s.environment(room.environment).unwrap();
    let form = lyceum_foundry::kit::form(&env).unwrap();
    let parts: Vec<lyceum_foundry::room::Shape> = (0..form.parts())
        .map(|p| lyceum_foundry::room::Shape::of(&env, p).unwrap())
        .collect();
    let rooms = spaces(&s, &after, &[]);
    let dungeon = Dungeon {
        rooms: &rooms,
        doorways: &free,
        ground: None,
    };
    let fit = fit_piece(&form, &parts, &room.frame, &dungeon).unwrap();
    assert_eq!(fit.joins(), ways_in);
    let q = room.frame.rotation;
    let o = room.frame.origin;
    s.apply(&Op::PlacePieces {
        landblock: REDOUBT,
        pieces: vec![NewPiece {
            environment: room.environment,
            surfaces: before[&room.cells[0]].surfaces.clone(),
            at: [o.x, o.y, o.z],
            rotation: Rotation::Quat([q.w, q.x, q.y, q.z]),
            parts: fit
                .links
                .iter()
                .enumerate()
                .map(|(k, l)| (u16::try_from(k).unwrap(), l.clone()))
                .collect(),
        }],
        joins: form
            .inner
            .iter()
            .map(|((pa, sa), (pb, sb))| ((0, usize::from(*pa), *sa), (0, usize::from(*pb), *sb)))
            .collect(),
    })
    .unwrap();
    assert!(s.free_doorways(REDOUBT, &[]).is_empty());
    let walk = s.walk(REDOUBT);
    assert!(walk.is_empty(), "{walk:#?}");
    let new = s.cells(REDOUBT);
    let names = s.cell_names(REDOUBT);
    assert_eq!(names.len(), 10);
    for n in names.keys() {
        let c = &new[n];
        assert!(c.portals.iter().all(
            |p| p.flags & 4 != 0 || new.contains_key(&u16::try_from(p.other_cell_id).unwrap())
        ));
    }
    // Both steps undone: the world's own rooms as they were, byte for byte.
    s.undo().unwrap();
    s.undo().unwrap();
    assert_eq!(s.cells(REDOUBT), before);
}

#[test]
fn a_look_dresses_every_room_of_another_dungeon_by_its_own_list_where_it_has_one_and_by_role_elsewhere(
) {
    let set = lyceum_foundry::dats::DatSet::open(&dereth_dat::testing::dat_dir()).unwrap();
    let catalog = lyceum_foundry::pieces::catalog(&set);
    let kit: BTreeMap<DataId, usize> = catalog
        .kit
        .iter()
        .enumerate()
        .map(|(i, p)| (p.environment, i))
        .collect();
    let family_of: BTreeMap<u16, usize> = catalog
        .families
        .iter()
        .enumerate()
        .flat_map(|(i, f)| f.landblocks.iter().map(move |(lb, _)| (*lb, i)))
        .collect();
    // The three biggest looks of many dungeons.
    let mut big: Vec<usize> = (0..catalog.families.len())
        .filter(|f| catalog.families[*f].landblocks.len() > 1)
        .collect();
    big.sort_by_key(|f| std::cmp::Reverse(catalog.families[*f].cells));
    big.truncate(3);
    // A sample of the world's dungeons: every twentieth, each restyled in each of those looks it
    // is not drawn in already.
    let cells = set.file(ModernDat::Cell).unwrap();
    let mut blocks: BTreeMap<u16, BTreeMap<u16, EnvCell>> = BTreeMap::new();
    for id in cells.iter_ids() {
        if dereth_dat::classify_cell_id(id) != Some(DbType::Cell) {
            continue;
        }
        let Some(c) = cells
            .read(id)
            .ok()
            .and_then(|b| EnvCell::decode_payload(id, &b).ok())
        else {
            continue;
        };
        let (lb, n) = (
            u16::try_from(id.raw() >> 16).unwrap(),
            u16::try_from(id.raw() & 0xFFFF).unwrap(),
        );
        blocks.entry(lb).or_default().insert(n, c);
    }
    let dungeons: Vec<u16> = blocks
        .iter()
        .filter(|(_, cs)| cs.get(&0x0100).is_some_and(|c| c.flags & 1 == 0))
        .map(|(lb, _)| *lb)
        .step_by(20)
        .collect();
    let (mut exact, mut by_role, mut rooms) = (0usize, 0usize, 0usize);
    for lb in &dungeons {
        for look in &big {
            if family_of.get(lb) == Some(look) {
                continue;
            }
            for p in placed_pieces(&blocks[lb]) {
                let Some(piece) = kit.get(&p.environment).map(|i| &catalog.kit[*i]) else {
                    continue;
                };
                let (list, how) =
                    lyceum_foundry::kit::dress(piece, *look, catalog.dress.get(*look));
                assert!(list.len() >= piece.form.roles.len(), "{}", piece.label());
                rooms += p.cells.len();
                match how {
                    lyceum_foundry::kit::Dressed::Exact => exact += p.cells.len(),
                    lyceum_foundry::kit::Dressed::ByRole => by_role += p.cells.len(),
                    lyceum_foundry::kit::Dressed::Kept => panic!("a big look draws every role"),
                }
            }
        }
    }
    println!(
        "{} dungeons restyled in {} looks: {rooms} rooms, {exact} by the look's own list, \
         {by_role} by role",
        dungeons.len(),
        big.len()
    );
    assert_eq!(exact + by_role, rooms);
    assert!(exact > 0 && by_role > 0);
}

/// A dungeon of two February 2005 corridors, joined, on an empty landblock: its cells are of that
/// layout, see each other, and pass the checks.
#[test]
fn a_dungeon_of_february_2005_pieces_compiles_in_their_layout() {
    let (_scratch, mut s) = super::pretod::session("classic-dungeon");
    let pieces = catalog(s.base()).pieces;
    let envs: std::collections::BTreeSet<u32> =
        pieces.iter().map(|p| p.environment.raw()).collect();
    assert!(envs.len() > 600, "{} environments", envs.len());
    let piece = super::pretod::corridor(&pieces);
    let lb = super::pretod::empty_landblock(s.base());
    let env = s.environment(piece.environment).unwrap();
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
    let (a, b) = (&cells[&0x0100], &cells[&0x0101]);
    assert!(a.visible_cells.contains(&0x0101) && b.visible_cells.contains(&0x0100));
    let joined = |c: &EnvCell, to: u32| c.portals.iter().any(|p| p.other_cell_id == to);
    assert!(joined(a, 0x0101) && joined(b, 0x0100), "joined both ways");
    super::pretod::check(&s);
}

/// The end-of-retail world database: `EMPYREAN_WORLD_SQL`, else the newest in the importer's
/// cache.
fn world_sql() -> std::path::PathBuf {
    std::env::var_os("EMPYREAN_WORLD_SQL")
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            lyceum_foundry::export::database_cache().and_then(|d| {
                lyceum_foundry::export::newest_database_for(&d, lyceum_foundry::export::Era::Eor)
            })
        })
        .expect("the world database: set EMPYREAN_WORLD_SQL, or fetch it")
}

/// How much of a piece's footprint its own floor covers at its bottom: the area of its physics
/// polygons lying within half a meter of its lowest point, over its squares'.
fn floor_cover(env: &Environment, form: &lyceum_foundry::kit::Form) -> f32 {
    let mut area = 0.0f32;
    for cs in &env.cells {
        let vs = &cs.vertex_array.vertices;
        for p in &cs.physics_polygons {
            let pts: Vec<Vec3> = p
                .vertex_ids
                .iter()
                .filter_map(|k| vs.get(usize::from(*k)).map(|v| v.position))
                .collect();
            if pts.len() < 3 || pts.iter().any(|q| q.z > form.lo.z + 0.5) {
                continue;
            }
            let twice: f32 = (0..pts.len())
                .map(|k| {
                    let (u, w) = (pts[k], pts[(k + 1) % pts.len()]);
                    u.x * w.y - w.x * u.y
                })
                .sum();
            area += twice.abs() / 2.0;
        }
    }
    area / (f32::from(form.footprint.0) * f32::from(form.footprint.1) * PITCH * PITCH)
}

/// Retail's dungeons floor by floor, counted from the floor their portals drop players on: a
/// room's floor is the bottom of its piece, a room with a floor of its own is one whose piece
/// does not open down, and floors stand on 6 m steps from the arrival. Every opening up or down
/// is closed: a top by a ceiling piece or the piece over it, a bottom by the piece under it. A
/// piece that opens down has no floor of its own: it is the upper part of a taller room or of a
/// shaft, so players go between floors by stairs or by dropping through one.
#[test]
fn retails_dungeons_stack_floors_six_meters_apart_from_the_arrival_and_close_every_opening_up_or_down(
) {
    let set = lyceum_foundry::dats::DatSet::open(&dereth_dat::testing::dat_dir()).unwrap();
    let catalog = catalog(&set);
    let kit: BTreeMap<DataId, usize> = catalog
        .kit
        .iter()
        .enumerate()
        .map(|(i, p)| (p.environment, i))
        .collect();
    let (_dir, world) = super::sample::sample("floors-retail");
    let mut s = Session::open(&world, &dereth_dat::testing::dat_dir(), None).unwrap();
    s.learn_weenies(&std::fs::read_to_string(world_sql()).unwrap());
    let dungeons = s.retail_dungeons();
    let file = set.file(ModernDat::Cell).unwrap();
    let mut by_block: BTreeMap<u16, BTreeMap<u16, EnvCell>> = BTreeMap::new();
    for id in file.iter_ids() {
        let lb = u16::try_from(id.raw() >> 16).unwrap();
        if dereth_dat::classify_cell_id(id) != Some(DbType::Cell) || !dungeons.contains_key(&lb) {
            continue;
        }
        let c = EnvCell::decode_payload(id, &file.read(id).unwrap()).unwrap();
        by_block
            .entry(lb)
            .or_default()
            .insert(u16::try_from(id.raw() & 0xFFFF).unwrap(), c);
    }
    let mut envs: BTreeMap<DataId, Option<Environment>> = BTreeMap::new();
    let mut env = |id: DataId| -> Option<Environment> {
        envs.entry(id)
            .or_insert_with(|| {
                set.read_portal(id)
                    .and_then(|b| Environment::decode_payload(id, &b).ok())
            })
            .clone()
    };
    // Openings up and down, by what closes them; pieces that open down, and those of them with
    // no floor of their own.
    let (mut tops, mut bottoms): (BTreeMap<&str, usize>, BTreeMap<&str, usize>) =
        Default::default();
    let (mut open_down, mut floorless) = (0usize, 0usize);
    // Floors from the arrival: per dungeon, the floors of its rooms with a floor of their own.
    let mut arrived = 0usize;
    let mut floors: BTreeMap<usize, usize> = BTreeMap::new();
    let (mut above, mut below) = (0usize, 0usize);
    let (mut deepest, mut highest) = (0i32, 0i32);
    let (mut standing, mut on_steps) = (0usize, 0usize);
    let (mut lowest_z, mut highest_z) = (f32::MAX, f32::MIN);
    // How floors join: stairs (a side doorway between rooms on different floors), and drops (a
    // floorless piece walked into from a floor).
    let mut joined_by: BTreeMap<&str, usize> = BTreeMap::new();
    for (lb, cells) in &by_block {
        let placed = placed_pieces(cells);
        let of_cell: BTreeMap<u16, usize> = placed
            .iter()
            .enumerate()
            .flat_map(|(i, p)| p.cells.iter().map(move |c| (*c, i)))
            .collect();
        let forms: Vec<Option<&lyceum_foundry::kit::Form>> = placed
            .iter()
            .map(|p| kit.get(&p.environment).map(|i| &catalog.kit[*i].form))
            .collect();
        let ceiling = |i: usize| forms[i].is_some_and(|f| f.kind == Kind::Ceiling);
        let floor_z = |i: usize| placed[i].frame.origin.z + forms[i].map_or(0.0, |f| f.lo.z);
        // Each piece's doorways: whether each opens on a side, or up, and the piece it joins.
        let mut ways: Vec<Vec<(bool, bool, Option<usize>)>> = vec![Vec::new(); placed.len()];
        for (i, p) in placed.iter().enumerate() {
            let Some(f) = forms[i] else { continue };
            lowest_z = lowest_z.min(p.frame.origin.z + f.lo.z);
            highest_z = highest_z.max(p.frame.origin.z + f.hi.z);
            let Some(e) = env(p.environment) else {
                continue;
            };
            if f.below && !ceiling(i) {
                open_down += 1;
                floorless += usize::from(floor_cover(&e, f) < 0.05);
            }
            for d in &f.doors {
                let Some(c) = p.cells.iter().find(|c| cells[c].cell_struct == d.part) else {
                    continue;
                };
                let polygon = e.cells[usize::from(d.part)].portals[d.slot];
                let to = cells[c].portals.iter().find(|q| q.polygon_id == polygon);
                let j = to
                    .filter(|q| q.flags & 4 == 0)
                    .and_then(|q| u16::try_from(q.other_cell_id).ok())
                    .and_then(|o| of_cell.get(&o).copied());
                ways[i].push((d.side.is_some(), d.up, j));
                if d.side.is_some() || ceiling(i) {
                    continue;
                }
                let what = match (to, j) {
                    (None, _) => "shut",
                    (Some(q), _) if q.flags & 4 != 0 => "outside",
                    (_, Some(j)) if ceiling(j) => "a ceiling",
                    (_, Some(_)) => "a piece",
                    (_, None) => "nothing",
                };
                *if d.up { &mut tops } else { &mut bottoms }
                    .entry(what)
                    .or_default() += 1;
            }
        }
        let Some((_, at)) = s.drop_point(*lb).filter(|(c, _)| c >> 16 == u32::from(*lb)) else {
            continue;
        };
        arrived += 1;
        #[allow(clippy::cast_possible_truncation)]
        let level = |i: usize| ((floor_z(i) - at.z) / RISE).round() as i32;
        let stands = |i: usize| forms[i].is_some_and(|f| f.kind != Kind::Ceiling && !f.below);
        let mut levels = BTreeSet::new();
        for i in (0..placed.len()).filter(|i| stands(*i)) {
            standing += 1;
            let steps = (floor_z(i) - at.z) / RISE;
            on_steps += usize::from((steps - steps.round()).abs() * RISE < 0.5);
            levels.insert(level(i));
        }
        let (Some(lo), Some(hi)) = (levels.first().copied(), levels.last().copied()) else {
            continue;
        };
        *floors.entry(levels.len()).or_default() += 1;
        above += usize::from(hi > 0);
        below += usize::from(lo < 0);
        deepest = deepest.min(lo);
        highest = highest.max(hi);
        if levels.len() < 2 {
            continue;
        }
        let (mut stairs, mut drops) = (false, false);
        for (i, w) in ways.iter().enumerate() {
            for (side, _, j) in w {
                let Some(j) = j.filter(|j| *side && *j != i) else {
                    continue;
                };
                stairs |= stands(i) && stands(j) && level(i) != level(j);
                drops |= forms[i].is_some_and(|f| f.below) && stands(j) && level(i) == level(j);
            }
        }
        *joined_by
            .entry(match (stairs, drops) {
                (true, true) => "stairs and drops",
                (true, false) => "stairs",
                (false, true) => "drops",
                (false, false) => "neither",
            })
            .or_default() += 1;
    }
    println!(
        "{} dungeons, {arrived} with a portal in. Floors with rooms, from the arrival: {floors:?}; \
         {above} have floors above it, {below} below; the deepest {deepest}, the highest {highest}; \
         rooms on 6 m steps from the arrival {on_steps} of {standing}; rooms from {lowest_z} m to \
         {highest_z} m. Openings up closed by {tops:?}; down by {bottoms:?}; pieces that open down \
         {open_down}, with no floor of their own {floorless}. Dungeons of several floors joined by \
         {joined_by:?}",
        dungeons.len()
    );
    assert_eq!(dungeons.len(), 1970);
    assert!(on_steps * 100 >= standing * 95, "{on_steps} of {standing}");
    assert!(above > 0 && below > 0 && deepest < -20 && highest > 20);
    assert!(lowest_z < -200.0 && highest_z > 600.0);
    // Nothing is left open up or down.
    assert_eq!(tops.get("shut"), None, "{tops:?}");
    assert_eq!(bottoms.get("shut"), None, "{bottoms:?}");
    // Two in three tops go on into the piece over them; the rest are closed by a ceiling.
    assert_eq!(
        (tops["a piece"], tops["a ceiling"]),
        (95_468, 45_162),
        "{tops:?}"
    );
    assert_eq!(bottoms["a piece"], 86_920, "{bottoms:?}");
    assert_eq!((floorless, open_down), (84_033, 84_722));
    // Most dungeons of several floors join them by stairs, many by drops as well.
    assert!(joined_by["stairs"] + joined_by["stairs and drops"] > joined_by["drops"] * 5);
}

/// Sand Caves: its portals drop players over an open shaft, twelve meters above its floor.
const SAND_CAVES: u16 = 0x5760;

/// How far over a dungeon's drop point the editor's camera goes in, in meters.
const EYE: f32 = 1.6;

/// From the camera at the drop point of every retail dungeon whose portals drop into a room, a
/// body let go lands on a floor in the dungeon: under a room with a floor of its own, that floor;
/// down an open shaft, the floor at its bottom. Sand Caves drops players into a shaft, so its
/// drop point's own room has no floor under it, and nothing there is the landscape.
#[test]
fn from_the_camera_at_every_retail_dungeons_drop_point_a_body_lands_on_a_floor_down_open_shafts_too(
) {
    let (_dir, world) = super::sample::sample("drop-point-landings");
    let mut s = Session::open(&world, &dereth_dat::testing::dat_dir(), None).unwrap();
    s.learn_weenies(&std::fs::read_to_string(world_sql()).unwrap());
    let (cell, at) = s.drop_point(SAND_CAVES).expect("Sand Caves' portals");
    assert_eq!(
        cell, 0x5760_0133,
        "the room its portals name, at the shaft's bottom"
    );
    let eye = Vec3::new(at.x, at.y, at.z + EYE);
    assert_eq!(
        s.cell_at(SAND_CAVES, eye),
        None,
        "no floor in the camera's own room"
    );
    let (room, floor) = s.landing(SAND_CAVES, eye).expect("a floor down the shaft");
    assert_eq!(room, 0x0133);
    assert!((floor + 18.0).abs() < 1e-3, "{floor}");

    let (mut rooms, mut shafts) = (0, Vec::new());
    for lb in s.retail_dungeons().keys() {
        let Some((cell, at)) = s.drop_point(*lb) else {
            continue;
        };
        if cell >> 16 != u32::from(*lb) || cell & 0xFFFF < 0x100 {
            continue;
        }
        rooms += 1;
        let eye = Vec3::new(at.x, at.y, at.z + EYE);
        let landed = s.landing(*lb, eye);
        assert!(
            landed.is_some(),
            "{lb:04X}: nothing to land on under {at:?}"
        );
        match s.cell_at(*lb, eye) {
            Some((_, own)) => assert_eq!(landed.map(|(_, z)| z), Some(own), "{lb:04X}"),
            None => shafts.push(*lb),
        }
    }
    println!(
        "{rooms} retail dungeons drop into a room; at {} the camera there is over an open \
         shaft: {shafts:04X?}",
        shafts.len()
    );
    assert!(rooms > 800, "{rooms}");
    assert!(shafts.contains(&SAND_CAVES), "{shafts:04X?}");
}
