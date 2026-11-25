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
    empty_landblocks(s).next().expect("an empty landblock")
}

/// The landblocks of the open sea floor, in order.
fn empty_landblocks(s: &RetailDatStore) -> impl Iterator<Item = u16> + '_ {
    (0x0101u16..0xFEFE).filter(|lb| {
        let id = DataId((u32::from(*lb) << 16) | 0xFFFF);
        let Ok(b) = s.read_cell(id) else { return false };
        let land = CellLandblock::decode_payload(id, &b).unwrap();
        land.lbi_exists == 0 && land.height.iter().all(|h| *h == 0)
    })
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
            Some((
                *n,
                s.ac2_shape(c.environment, c.cell_struct)?.placed(&c.frame),
            ))
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
        pitch: PITCH,
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

/// The player's Asheron's Call 2 install the AC2 kits are made from: `AUBEREAN_TEST_DAT_DIR`.
fn ac2_install() -> lyceum_foundry::ac2_kit::Ac2Install {
    let dir = std::env::var_os("AUBEREAN_TEST_DAT_DIR")
        .filter(|v| !v.is_empty())
        .expect("AUBEREAN_TEST_DAT_DIR: the Asheron's Call 2 install the AC2 kits are made from");
    lyceum_foundry::ac2_kit::open(std::path::Path::new(&dir)).expect("the AC2 install opens")
}

/// A world asking for AC2's `basic` kit: every piece is made in every look, at full size; two of
/// its halls snap doorway to doorway and walk; and the same install gives the same ids again.
#[test]
fn an_ac2_kits_pieces_are_made_in_every_look_and_two_of_its_halls_join_and_walk() {
    let (dir, world) = super::sample::sample("dungeon-ac2-kit");
    std::fs::write(
        dir.path().join("holtburg/records/ac2.toml"),
        "[[ac2-kit]]\nkit = \"basic\"\n",
    )
    .unwrap();
    let dats = dereth_dat::testing::dat_dir();
    // Without the install, the world says what it needs.
    let e = Session::open(&world, &dats, None).unwrap_err().to_string();
    assert!(e.contains("--ac2"), "{e}");
    let mut s = Session::open_with(&world, &dats, None, Some(ac2_install())).unwrap();
    s.set_today("2026-11-21 00:00:00");
    let pieces = s.ac2_pieces().to_vec();
    assert_eq!(pieces.len(), 40, "every piece of the basic kit");
    for p in &pieces {
        let names: Vec<&str> = p.looks.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, ["own", "plain", "haunted", "emplite"], "{}", p.label);
        let env = s
            .environment(p.environment)
            .expect("its environment is the world's");
        // A surface for each slot its drawn polygons name, then the clear one its doorways draw
        // with, in every look alike.
        let slots = surface_slots(&env, 0);
        for (name, surfaces) in &p.looks {
            assert!(surfaces.len() > slots, "{} in {name}", p.label);
            assert_eq!(surfaces.len(), p.looks[0].1.len(), "{} in {name}", p.label);
        }
    }
    // What each is on the grid: the halls two squares a side by their doorways, the room's
    // squares as tiles, and its flat lids as ceilings; and none of the world's own pieces a tile.
    let kind = |label: &str| {
        let p = pieces.iter().find(|p| p.label == label).unwrap();
        lyceum_foundry::kit::form(&s.environment(p.environment).unwrap())
            .unwrap()
            .kind
    };
    for (label, k) in [
        ("straight", Kind::Corridor),
        ("turn", Kind::Corner),
        ("tee", Kind::Tee),
        ("cross", Kind::Cross),
        ("end", Kind::DeadEnd),
        ("room floor corner", Kind::Tile),
        ("room ceiling side", Kind::Tile),
        ("room middle open", Kind::Tile),
        ("room flat ceiling", Kind::Ceiling),
    ] {
        assert_eq!(kind(label), k, "{label}");
    }
    let retail = catalog(s.base()).kit;
    assert!(retail.iter().all(|p| p.form.kind != Kind::Tile));
    // A look is another list of surfaces for the same environment.
    let straight = pieces.iter().find(|p| p.label == "straight").unwrap();
    assert_ne!(
        straight.looks[0].1, straight.looks[2].1,
        "haunted is not its own look"
    );
    // Full size: AC2's 20 m hall, 10 m doorways.
    let env = s.environment(straight.environment).unwrap();
    let form = lyceum_foundry::kit::form(&env).unwrap();
    assert!(
        (form.hi.y - form.lo.y - 20.0).abs() < 1.0,
        "{:?} {:?}",
        form.lo,
        form.hi
    );
    // Two halls, the second snapped to the first's doorway 0 by its own doorway 1.
    let store = dereth_dat::testing::open_store_or_fail();
    let lb = empty_landblock(&store);
    let surfaces = straight.looks[2].1.clone();
    s.apply(&Op::PlaceCell {
        landblock: lb,
        environment: straight.environment,
        piece: 0,
        surfaces: surfaces.clone(),
        at: [96.0, 96.0, -40.0],
        rotation: Rotation::Heading(0.0),
        links: vec![Link::Shut, Link::Shut],
    })
    .unwrap();
    let first = Frame::new(Vec3::new(96.0, 96.0, -40.0), Quat::IDENTITY);
    let target = opening(&env, 0, 0, &first).unwrap();
    let (frame, _) = snap(&env, 0, 1, &target, 0.0, 90.0).unwrap();
    let q = frame.rotation;
    let pack = s.packs()[s.target()].name.clone();
    s.apply(&Op::PlaceCell {
        landblock: lb,
        environment: straight.environment,
        piece: 0,
        surfaces,
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
    assert_eq!(cells.len(), 2);
    let joined: Vec<_> = cells[&0x0100]
        .portals
        .iter()
        .filter(|p| p.flags & 4 == 0)
        .collect();
    assert_eq!(joined.len(), 1);
    assert_eq!(joined[0].other_cell_id, 0x0101);
    let found = s.walk(lb);
    assert!(
        found.iter().all(|f| !f.text.contains("falls")),
        "{found:#?}"
    );
    // Saved and built, then built again: the same records, the lockfile keeping the kit's ids.
    s.save().unwrap();
    let ac2 = ac2_install();
    lyceum_foundry::compile::build_with(&world, &dats, None, false, Some(&ac2)).unwrap();
    let overlay = dir.path().join("build/overlay");
    let first: Vec<u8> = std::fs::read(overlay.join("overlay_portal.dat")).unwrap();
    let lock = std::fs::read_to_string(dir.path().join("world.lock")).unwrap();
    assert!(
        lock.contains("ac2-environment-0100"),
        "the kit's environments are in the lockfile"
    );
    lyceum_foundry::compile::build_with(&world, &dats, None, false, Some(&ac2)).unwrap();
    assert_eq!(
        std::fs::read(overlay.join("overlay_portal.dat")).unwrap(),
        first
    );
}

/// An editor given the install asks for every AC2 kit in the pack being edited, once, and the
/// session makes them at once; saved, the pack's `records/ac2.toml` asks for them from then on.
#[test]
fn an_editor_given_the_ac2_install_adds_its_kits_to_the_pack_once_and_saves_the_row() {
    let (dir, world) = super::sample::sample("dungeon-ac2-add");
    let dats = dereth_dat::testing::dat_dir();
    let mut bare = Session::open(&world, &dats, None).unwrap();
    assert!(bare
        .add_ac2_kits()
        .unwrap_err()
        .to_string()
        .contains("--ac2"));
    let mut s = Session::open_with(&world, &dats, None, Some(ac2_install())).unwrap();
    assert!(s.ac2_pieces().is_empty(), "no pack asks for a kit yet");
    assert!(s.add_ac2_kits().unwrap());
    assert_eq!(
        s.ac2_pieces().len(),
        287,
        "basic's 40, fancy's 35, the tomb's 43, the mine's 14, the castle's 47 and the cave's 108"
    );
    let env = s.ac2_pieces()[0].environment;
    assert!(
        s.environment(env).is_some(),
        "its records are the world's at once"
    );
    assert!(!s.add_ac2_kits().unwrap(), "asked for already");
    s.save().unwrap();
    let row = std::fs::read_to_string(dir.path().join("holtburg/records/ac2.toml")).unwrap();
    assert_eq!(
        row,
        "[[ac2-kit]]\nkit = \"basic\"\n\n[[ac2-kit]]\nkit = \"fancy\"\n\n[[ac2-kit]]\nkit = \"tomb\"\n\n[[ac2-kit]]\nkit = \"mine\"\n\n[[ac2-kit]]\nkit = \"castle\"\n\n[[ac2-kit]]\nkit = \"cave\"\n"
    );
    // Opened again, the pack asks for them itself, with the same ids.
    let again = Session::open_with(&world, &dats, None, Some(ac2_install())).unwrap();
    assert_eq!(again.ac2_pieces()[0].environment, env);
}

/// The sides a piece's doorways open on at `frame`, as compass bearings in landblock space
/// (`N`, `E`, `S`, `W`), its openings up and down left out.
fn ac2_sides(env: &Environment, frame: &Frame, form: &lyceum_foundry::kit::Form) -> BTreeSet<char> {
    (0..form.doors.len())
        .filter_map(|slot| {
            let o = opening(env, 0, slot, frame)?.out;
            if o.z.abs() > 0.5 {
                None
            } else if o.y > 0.5 {
                Some('N')
            } else if o.y < -0.5 {
                Some('S')
            } else if o.x > 0.5 {
                Some('E')
            } else {
                Some('W')
            }
        })
        .collect()
}

/// AC2 kit piece `label` set at `at` in landblock `lb` in look `look`: turned the first quarter
/// turn whose doorways open on `sides` (any, when empty) and that fits the dungeon as it stands,
/// joining every doorway it meets. Answers the new cell's number.
fn place_ac2(s: &mut Session, lb: u16, label: &str, look: &str, at: Vec3, sides: &str) -> u16 {
    let p = s
        .ac2_pieces()
        .iter()
        .find(|p| match label.split_once('/') {
            Some((kit, l)) => p.kit.name() == kit && p.label == l,
            None => p.label == label,
        })
        .unwrap_or_else(|| panic!("{label} is a piece"))
        .clone();
    let env = s.environment(p.environment).unwrap();
    let form = lyceum_foundry::kit::form_on(&env, &p.kit.grid()).unwrap();
    let parts: Vec<lyceum_foundry::room::Shape> = (0..form.parts())
        .map(|k| s.ac2_shape(p.environment, k).unwrap())
        .collect();
    let want: BTreeSet<char> = sides.chars().collect();
    let surfaces = p.looks.iter().find(|(n, _)| *n == look).unwrap().1.clone();
    // Its quarter turns at `at`; with no sides asked for, also each of its doorways snapped to
    // each of the dungeon's free ones.
    let mut frames: Vec<Frame> = (0..4u8)
        .map(|turn| {
            Frame::new(
                at,
                lyceum_foundry::compile::heading_quat(90.0 * f32::from(turn)),
            )
        })
        .collect();
    if want.is_empty() {
        for d in s.free_doorways(lb, &[]) {
            for slot in 0..form.doors.len() {
                frames.extend(snap(&env, 0, slot, &d.opening, 0.0, 90.0).map(|(f, _)| f));
            }
        }
    }
    for frame in frames {
        if !want.is_empty() && ac2_sides(&env, &frame, &form) != want {
            continue;
        }
        let cells = s.cells(lb);
        let rooms = spaces(s, &cells, &[]);
        let free = s.free_doorways(lb, &[]);
        let dungeon = Dungeon {
            rooms: &rooms,
            doorways: &free,
            ground: None,
            pitch: p.kit.grid().pitch,
        };
        let Ok(fit) = fit_piece(&form, &parts, &frame, &dungeon) else {
            continue;
        };
        if !cells.is_empty() && fit.joins() == 0 {
            continue;
        }
        let (q, o) = (frame.rotation, frame.origin);
        s.apply(&Op::PlacePieces {
            landblock: lb,
            pieces: vec![NewPiece {
                environment: p.environment,
                surfaces,
                at: [o.x, o.y, o.z],
                rotation: Rotation::Quat([q.w, q.x, q.y, q.z]),
                parts: fit
                    .links
                    .iter()
                    .enumerate()
                    .map(|(k, l)| (u16::try_from(k).unwrap(), l.clone()))
                    .collect(),
            }],
            joins: Vec::new(),
        })
        .unwrap();
        return *s.cells(lb).keys().max().unwrap();
    }
    panic!("{label} fits at {at:?} on no quarter turn opening {sides}");
}

/// A room of AC2's basic kit two tiles by three under its ceiling, a hall from its doorway, the
/// kit's entrance hall at the hall's far end, and a portal on Holtburg's green that sends who
/// uses it into the entrance hall: who arrives walks from there through the hall into the room.
#[test]
fn an_ac2_room_and_hall_reached_by_a_portal_walk_end_to_end() {
    let (dir, world) = super::sample::sample("dungeon-ac2-layout");
    std::fs::write(
        dir.path().join("holtburg/records/ac2.toml"),
        "[[ac2-kit]]\nkit = \"basic\"\n",
    )
    .unwrap();
    let dats = dereth_dat::testing::dat_dir();
    let mut s = Session::open_with(&world, &dats, None, Some(ac2_install())).unwrap();
    s.set_today("2026-11-21 00:00:00");
    let store = dereth_dat::testing::open_store_or_fail();
    let lb = empty_landblock(&store);
    let z = -40.0;
    let look = "haunted";
    // The hall first, then the room laid west of it in one step: its floor tiles and its ceiling
    // tiles over them, the doorway tile where the hall meets it.
    let hall = place_ac2(
        &mut s,
        lb,
        "straight",
        look,
        Vec3::new(105.0, 90.0, z),
        "EW",
    );
    let room = lyceum_foundry::edit::Ac2Room {
        landblock: lb,
        kit: lyceum_foundry::ac2_kit::Kit::Basic,
        look: look.to_owned(),
        south_west: (80.0, 80.0),
        floor: z,
        east: 2,
        north: 3,
    };
    let laid = s.lay_ac2_room(&room).unwrap();
    assert_eq!(laid.len(), 12, "six floor tiles and six ceiling tiles");
    assert!(
        s.free_doorways(lb, &[])
            .iter()
            .all(|d| d.opening.middle.x > 110.0),
        "the room's every doorway joined; the hall's far end free"
    );
    // One undo takes the whole room back, and redo lays it again.
    s.undo().unwrap();
    assert_eq!(s.cells(lb).len(), 1);
    s.redo().unwrap();
    assert_eq!(s.cells(lb).len(), 13);
    // A room too narrow, or one whose doorway falls at a corner, is refused and lays nothing.
    let narrow = lyceum_foundry::edit::Ac2Room {
        east: 1,
        ..room.clone()
    };
    assert!(s.lay_ac2_room(&narrow).is_err());
    assert_eq!(s.cells(lb).len(), 13);
    let entry = place_ac2(
        &mut s,
        lb,
        "entrance hall",
        look,
        Vec3::new(115.0, 90.0, z),
        "",
    );
    assert_eq!(s.cells(lb).len(), 14);
    // Every doorway between them is joined: what stays free is the entrance hall's front door.
    let free = s.free_doorways(lb, &[]);
    assert_eq!(free.len(), 1, "{free:?}");
    // The portal on the green, sending who uses it to the entrance hall's floor.
    let inside = s.cells(lb)[&entry].frame.origin;
    let (c, floor) = s
        .cell_hit(
            lb,
            Vec3::new(inside.x, inside.y, inside.z + 3.0),
            Vec3::new(0.0, 0.0, -1.0),
        )
        .expect("the entrance hall has a floor under its middle");
    assert_eq!(c, entry);
    let dest = (u32::from(lb) << 16) | u32::from(entry);
    let pack = s.packs()[s.target()].name.clone();
    let sql = format!(
        "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\n\
         VALUES ({{{{wcid:ac2-door}}}}, 'ac2-door', 7, '2026-11-21 00:00:00');\n\
         INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)\n\
         VALUES ({{{{wcid:ac2-door}}}}, 1, 'Portal');\n\
         INSERT INTO `weenie_properties_position` (`object_Id`, `position_Type`, `obj_Cell_Id`, \
         `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`)\n\
         VALUES ({{{{wcid:ac2-door}}}}, 2, {dest:#010X}, {:.3}, {:.3}, {:.3}, 1, 0, 0, 0);\n",
        floor.x,
        floor.y,
        floor.z + 0.5
    );
    let file = s.packs()[s.target()]
        .dir
        .join("server/sql/weenies/ac2-door.sql");
    s.apply(&Op::WriteSql {
        file,
        text: Some(sql),
    })
    .unwrap();
    s.apply(&Op::PlaceSpawn {
        landblock: 0xA9B4,
        weenie: lyceum_foundry::edit::Weenie::Symbol(format!("{pack}:ac2-door")),
        origin: Vec3::new(100.0, 100.0, 70.0),
        heading: 0.0,
    })
    .unwrap();
    let doors = s.entrances(lb);
    assert_eq!(doors.len(), 1, "{doors:?}");
    let (cell, at) = s
        .drop_point(lb)
        .expect("who uses the portal arrives somewhere");
    assert_eq!(cell, dest);
    assert!((at.z - floor.z - 0.5).abs() < 1e-3);
    // From there, every cell is reached and stood in: nothing falls, nothing is out of reach.
    let found = s.walk(lb);
    assert!(found.is_empty(), "{found:#?}");
    assert_ne!(hall, entry);
}

/// The basic kit's lights are its dungeons' commonest, each a light a room's static stands for;
/// a room lit by more than the client's pool of 40 is warned of at the build, and one of 40 is
/// not.
#[test]
fn an_ac2_kits_lights_stand_in_a_room_and_a_room_seeing_more_than_forty_is_warned_of() {
    let (dir, world) = super::sample::sample("dungeon-ac2-lights");
    std::fs::write(
        dir.path().join("holtburg/records/ac2.toml"),
        "[[ac2-kit]]\nkit = \"basic\"\n",
    )
    .unwrap();
    let dats = dereth_dat::testing::dat_dir();
    let mut s = Session::open_with(&world, &dats, None, Some(ac2_install())).unwrap();
    s.set_today("2026-11-21 00:00:00");
    let lights = s.ac2_lights().to_vec();
    assert!(
        !lights.is_empty() && lights.len() <= lyceum_foundry::ac2_kit::LIGHTS,
        "{lights:?}"
    );
    assert!(
        lights.windows(2).all(|w| w[0].uses >= w[1].uses),
        "most used first"
    );
    for l in &lights {
        let bytes = s.base().read_portal(l.setup);
        let bytes = bytes.or_else(|| {
            s.compiled()
                .records
                .get(&ModernDat::Portal)
                .and_then(|r| r.get(&l.setup.raw()))
                .map(|st| st.bytes.to_vec())
        });
        let setup = s
            .base()
            .decode::<dereth_assets::Setup>(l.setup, &bytes.unwrap())
            .unwrap();
        assert_eq!(setup.lights.len(), 1, "{}", l.label());
        assert_eq!(setup.lights[&0].color_argb, l.color);
    }
    // A hall, and 40 of a light standing in it: within the pool.
    let store = dereth_dat::testing::open_store_or_fail();
    let lb = empty_landblock(&store);
    let hall = place_ac2(
        &mut s,
        lb,
        "straight",
        "own",
        Vec3::new(100.0, 100.0, -40.0),
        "NS",
    );
    let name = s.cell_names(lb)[&hall].clone();
    let stand = |s: &mut Session, k: usize| {
        #[allow(clippy::cast_precision_loss)]
        let y = 92.0 + 0.4 * k as f32;
        s.apply(&Op::PlaceCellStatic {
            landblock: lb,
            cell: name.clone(),
            model: lights[0].setup,
            at: [100.0, y, -35.0],
            rotation: Rotation::Heading(0.0),
        })
        .unwrap();
    };
    for k in 0..40 {
        stand(&mut s, k);
    }
    s.save().unwrap();
    let ac2 = ac2_install();
    let crowded = |w: &[String]| w.iter().any(|w| w.contains("lights stand in this room"));
    let built =
        lyceum_foundry::compile::build_with(&world, &dats, None, false, Some(&ac2)).unwrap();
    assert!(!crowded(&built.warnings), "{:?}", built.warnings);
    // One more: the room's pool is past the client's.
    stand(&mut s, 40);
    s.save().unwrap();
    let built =
        lyceum_foundry::compile::build_with(&world, &dats, None, false, Some(&ac2)).unwrap();
    assert!(crowded(&built.warnings), "{:?}", built.warnings);
}

/// AC2's tomb on its own 15 m grid: its halls a square each, its ramps stairs, its rooms' floors,
/// middles and roofs tiles; a room of them laid through a room entrance off a hall, the roof over
/// the floor, walks.
#[test]
fn the_ac2_tomb_stands_on_fifteen_meter_squares_and_its_room_laid_through_an_entrance_walks() {
    use lyceum_foundry::ac2_kit::Kit;
    let (dir, world) = super::sample::sample("dungeon-ac2-tomb");
    std::fs::write(
        dir.path().join("holtburg/records/ac2.toml"),
        "[[ac2-kit]]\nkit = \"tomb\"\n",
    )
    .unwrap();
    let dats = dereth_dat::testing::dat_dir();
    let mut s = Session::open_with(&world, &dats, None, Some(ac2_install())).unwrap();
    s.set_today("2026-11-21 00:00:00");
    let pieces = s.ac2_pieces().to_vec();
    assert_eq!(pieces.len(), 43);
    assert!(pieces
        .iter()
        .all(|p| p.kit == Kit::Tomb && p.looks.len() == 1));
    let kind = |label: &str| {
        let p = pieces.iter().find(|p| p.label == label).unwrap();
        lyceum_foundry::kit::form_on(&s.environment(p.environment).unwrap(), &Kit::Tomb.grid())
            .unwrap()
            .kind
    };
    for (label, k) in [
        ("straight", Kind::Corridor),
        ("corner", Kind::Corner),
        ("tee", Kind::Tee),
        ("cross", Kind::Cross),
        ("end", Kind::DeadEnd),
        ("ramp", Kind::Stairs),
        ("room floor corner", Kind::Tile),
        ("room middle open", Kind::Tile),
        ("room roof side", Kind::Tile),
    ] {
        assert_eq!(kind(label), k, "{label}");
    }
    assert!(
        !s.ac2_lights().is_empty(),
        "the tomb's own dungeons' lights"
    );
    // A hall, then a room of three squares by three laid south of it against its doorway.
    let store = dereth_dat::testing::open_store_or_fail();
    let lb = empty_landblock(&store);
    let z = -40.0;
    place_ac2(
        &mut s,
        lb,
        "straight",
        "own",
        Vec3::new(105.0, 105.0, z),
        "NS",
    );
    assert_eq!(s.grid_pitch(lb), 15.0);
    assert_eq!(s.floor_rise(lb), 12.0);
    let room = lyceum_foundry::edit::Ac2Room {
        landblock: lb,
        kit: Kit::Tomb,
        look: "own".to_owned(),
        south_west: (90.0, 60.0),
        floor: z,
        east: 3,
        north: 3,
    };
    // As in AC2's own tombs, a hall reaches a room through a room entrance, never straight: the
    // entrance stands on the room's square against the hall, its second doorway to the hall, and
    // the room's tiles fill the rest but for the row the hall meets.
    let laid = s.lay_ac2_room(&room).unwrap();
    assert_eq!(
        laid.len(),
        13,
        "the entrance, six floor tiles and six roof tiles"
    );
    let cells = s.cells(lb);
    let entrance = pieces
        .iter()
        .find(|p| p.label == "room floor entrance")
        .unwrap()
        .environment;
    let (_, cell) = cells
        .iter()
        .find(|(_, c)| c.environment == entrance)
        .expect("an entrance laid");
    assert!(
        (cell.frame.origin.x - 105.0).abs() < 0.01 && (cell.frame.origin.y - 90.0).abs() < 0.01
    );
    let env = s.environment(entrance).unwrap();
    let to_hall = lyceum_foundry::pieces::opening(&env, 0, 1, &cell.frame).unwrap();
    assert!(
        to_hall.out.y > 0.99,
        "its second doorway faces the hall: {:?}",
        to_hall.out
    );
    let at = |x: f32, y: f32| {
        cells
            .values()
            .any(|c| (c.frame.origin.x - x).abs() < 0.01 && (c.frame.origin.y - y).abs() < 0.01)
    };
    assert!(at(90.0, 60.0), "the room where it was asked");
    assert!(!at(90.0, 45.0), "not a square back");
    assert!(
        !at(90.0, 90.0) && !at(120.0, 90.0),
        "nothing beside the entrance, which narrows to the hall"
    );
    s.undo().unwrap();
    assert_eq!(
        s.cells(lb).len(),
        1,
        "one undo takes the entrance and the room"
    );
    s.redo().unwrap();
    assert_eq!(s.cells(lb).len(), 14);
    let free = s.free_doorways(lb, &[]);
    assert_eq!(free.len(), 1, "the hall's far end: {free:?}");
    let found = s.walk(lb);
    assert!(found.is_empty(), "{found:#?}");
}

/// AC2's mine on its 20 m grid: its tunnels read as their kinds, join one another and, through
/// its piece for the purpose, one of basic's halls, and walk; it has no room tiles to lay.
#[test]
fn the_ac2_mine_stands_on_twenty_meter_squares_and_joins_a_basic_hall() {
    use lyceum_foundry::ac2_kit::Kit;
    let (dir, world) = super::sample::sample("dungeon-ac2-mine");
    std::fs::write(
        dir.path().join("holtburg/records/ac2.toml"),
        "[[ac2-kit]]\nkit = \"mine\"\n\n[[ac2-kit]]\nkit = \"basic\"\n",
    )
    .unwrap();
    let dats = dereth_dat::testing::dat_dir();
    let mut s = Session::open_with(&world, &dats, None, Some(ac2_install())).unwrap();
    s.set_today("2026-11-21 00:00:00");
    let mine: Vec<_> = s
        .ac2_pieces()
        .iter()
        .filter(|p| p.kit == Kit::Mine)
        .cloned()
        .collect();
    assert_eq!(mine.len(), 14);
    let kind = |label: &str| {
        let p = mine.iter().find(|p| p.label == label).unwrap();
        lyceum_foundry::kit::form_on(&s.environment(p.environment).unwrap(), &Kit::Mine.grid())
            .unwrap()
            .kind
    };
    for (label, k) in [
        ("straight", Kind::Corridor),
        ("turn", Kind::Corner),
        ("tee", Kind::Tee),
        ("cross", Kind::Cross),
        ("end", Kind::DeadEnd),
        ("down half", Kind::Stairs),
        ("to basic", Kind::Corridor),
    ] {
        assert_eq!(kind(label), k, "{label}");
    }
    let store = dereth_dat::testing::open_store_or_fail();
    let lb = empty_landblock(&store);
    let z = -40.0;
    place_ac2(
        &mut s,
        lb,
        "mine/straight",
        "own",
        Vec3::new(100.0, 100.0, z),
        "NS",
    );
    assert_eq!(s.grid_pitch(lb), 20.0);
    assert_eq!(s.floor_rise(lb), 10.0);
    place_ac2(
        &mut s,
        lb,
        "mine/cross",
        "own",
        Vec3::new(100.0, 120.0, z),
        "NESW",
    );
    place_ac2(
        &mut s,
        lb,
        "mine/end",
        "own",
        Vec3::new(120.0, 120.0, z),
        "W",
    );
    place_ac2(
        &mut s,
        lb,
        "mine/to basic",
        "own",
        Vec3::new(80.0, 120.0, z),
        "EW",
    );
    // Basic's hall, two of its 10 m squares, against the mine's piece for it.
    place_ac2(
        &mut s,
        lb,
        "basic/straight",
        "own",
        Vec3::new(60.0, 120.0, z),
        "",
    );
    let refused = s
        .lay_ac2_room(&lyceum_foundry::edit::Ac2Room {
            landblock: lb,
            kit: Kit::Mine,
            look: "own".to_owned(),
            south_west: (100.0, 140.0),
            floor: z,
            east: 2,
            north: 2,
        })
        .unwrap_err()
        .to_string();
    assert!(refused.contains("no room tiles"), "{refused}");
    let found = s.walk(lb);
    assert!(found.is_empty(), "{found:#?}");
}

/// A refused AC2 room leaves the history as it was: what undo and redo can take is unchanged, and
/// redo after the refusal does again the step that was taken back before it, never a part of the
/// refused room. The tomb's room is refused after its entrance is laid (a room a square wide),
/// basic's partway through its tiles (a loop of halls comes back into its west squares).
#[test]
fn a_refused_ac2_room_leaves_undo_and_redo_as_they_were() {
    use lyceum_foundry::ac2_kit::Kit;
    let dats = dereth_dat::testing::dat_dir();
    let store = dereth_dat::testing::open_store_or_fail();
    for kit in [Kit::Tomb, Kit::Basic] {
        let (dir, world) = super::sample::sample("dungeon-ac2-refused");
        std::fs::write(
            dir.path().join("holtburg/records/ac2.toml"),
            format!("[[ac2-kit]]\nkit = \"{}\"\n", kit.name()),
        )
        .unwrap();
        let mut s = Session::open_with(&world, &dats, None, Some(ac2_install())).unwrap();
        s.set_today("2026-11-22 00:00:00");
        let lb = empty_landblock(&store);
        let z = -40.0;
        let look = "own";
        let room = match kit {
            // A hall, north-south; the room south of it, one square wide.
            Kit::Tomb => {
                place_ac2(
                    &mut s,
                    lb,
                    "straight",
                    look,
                    Vec3::new(105.0, 105.0, z),
                    "NS",
                );
                lyceum_foundry::edit::Ac2Room {
                    landblock: lb,
                    kit,
                    look: look.to_owned(),
                    south_west: (105.0, 60.0),
                    floor: z,
                    east: 1,
                    north: 3,
                }
            }
            // A hall north-south with the room south of it, against its doorway; from the hall's
            // north end a loop of halls comes back down into the room's west squares, so a tile
            // there does not fit once the room's first tiles are laid.
            _ => {
                place_ac2(
                    &mut s,
                    lb,
                    "straight",
                    look,
                    Vec3::new(105.0, 105.0, z),
                    "NS",
                );
                place_ac2(&mut s, lb, "tee", look, Vec3::new(105.0, 125.0, z), "NSW");
                place_ac2(
                    &mut s,
                    lb,
                    "straight",
                    look,
                    Vec3::new(85.0, 125.0, z),
                    "EW",
                );
                place_ac2(&mut s, lb, "turn", look, Vec3::new(65.0, 125.0, z), "ES");
                place_ac2(
                    &mut s,
                    lb,
                    "straight",
                    look,
                    Vec3::new(65.0, 105.0, z),
                    "NS",
                );
                place_ac2(&mut s, lb, "end", look, Vec3::new(65.0, 85.0, z), "N");
                lyceum_foundry::edit::Ac2Room {
                    landblock: lb,
                    kit,
                    look: look.to_owned(),
                    south_west: (65.0, 70.0),
                    floor: z,
                    east: 6,
                    north: 3,
                }
            }
        };
        // A step taken back before the room: a second hall placed on the first's far end, then
        // undone.
        let (second, sides) = match kit {
            Kit::Tomb => (Vec3::new(105.0, 120.0, z), "NS"),
            _ => (Vec3::new(105.0, 145.0, z), "NS"),
        };
        place_ac2(&mut s, lb, "straight", look, second, sides);
        s.undo().unwrap();
        let cells = s.cells(lb).len();
        let history = s.history();
        assert_eq!(history.1, 1, "{kit:?}: one step to do again");
        let refusal = s
            .lay_ac2_room(&room)
            .expect_err("the room is refused")
            .to_string();
        // The tomb's after its entrance is laid, basic's after some of its tiles are.
        let why = match kit {
            Kit::Tomb => "two squares or more",
            _ => "does not fit",
        };
        assert!(refusal.contains(why), "{kit:?}: {refusal}");
        assert_eq!(s.cells(lb).len(), cells, "{kit:?}: nothing laid");
        assert_eq!(s.history(), history, "{kit:?}: undo and redo as they were");
        s.redo().unwrap();
        assert_eq!(
            s.cells(lb).len(),
            cells + 1,
            "{kit:?}: redo places the second hall again"
        );
        assert!(
            s.cells(lb)
                .values()
                .any(|c| (c.frame.origin.x - second.x).abs() < 0.5
                    && (c.frame.origin.y - second.y).abs() < 0.5),
            "{kit:?}: the second hall is what came back"
        );
        assert_eq!(s.history().1, 0, "{kit:?}: and nothing more to do again");
    }
}

/// A hall of a test layout: its piece, where it stands (x, y) and the sides it opens on.
type Hall = (&'static str, f32, f32, &'static str);

/// Where a test room's entrances stand (x, y), or what its refusal says.
type RoomOutcome = Result<(&'static [(i32, i32)], usize, [f32; 4]), &'static str>;

/// A tomb room laid against two halls gets an entrance at each, on the room's squares against them,
/// whichever hall the room comes to first, with both on one side or round a corner; as in AC2's own
/// tombs nothing stands beside an entrance: the room's tiles fill the rectangle inside the rows and
/// columns the halls meet, and it walks. A square two halls meet is refused before anything is
/// laid, the history as it was.
#[test]
fn a_tomb_room_meets_each_hall_through_an_entrance_of_its_own() {
    use lyceum_foundry::ac2_kit::Kit;
    let dats = dereth_dat::testing::dat_dir();
    let store = dereth_dat::testing::open_store_or_fail();
    // Each layout's halls, joined into one dungeon, around a room of three squares by three whose
    // south-west square is at 90, 60; and where its entrances stand, or why it is refused.
    let layouts: [(&str, &[Hall], RoomOutcome); 5] = [
        (
            "north and east, the east one met first",
            &[
                ("straight", 105.0, 105.0, "NS"),
                ("corner", 105.0, 120.0, "SE"),
                ("straight", 120.0, 120.0, "EW"),
                ("corner", 135.0, 120.0, "WS"),
                ("straight", 135.0, 105.0, "NS"),
                ("straight", 135.0, 90.0, "NS"),
                ("tee", 135.0, 75.0, "NSW"),
            ],
            Ok((&[(105, 90), (120, 75)], 10, [90.0, 60.0, 105.0, 75.0])),
        ),
        (
            "south and east, the south one met first",
            &[
                ("straight", 105.0, 45.0, "NS"),
                ("corner", 105.0, 30.0, "NE"),
                ("straight", 120.0, 30.0, "EW"),
                ("corner", 135.0, 30.0, "WN"),
                ("straight", 135.0, 45.0, "NS"),
                ("straight", 135.0, 60.0, "NS"),
                ("tee", 135.0, 75.0, "NSW"),
            ],
            Ok((&[(105, 60), (120, 75)], 10, [90.0, 75.0, 105.0, 90.0])),
        ),
        (
            "two on the north side",
            &[
                ("straight", 105.0, 105.0, "NS"),
                ("corner", 105.0, 120.0, "SE"),
                ("tee", 120.0, 120.0, "WES"),
                ("straight", 120.0, 105.0, "NS"),
            ],
            Ok((&[(105, 90), (120, 90)], 14, [90.0, 60.0, 120.0, 75.0])),
        ),
        (
            "north and west, round the room's north-west corner",
            &[
                ("corner", 105.0, 105.0, "SW"),
                ("straight", 90.0, 105.0, "EW"),
                ("corner", 75.0, 105.0, "ES"),
                ("straight", 75.0, 90.0, "NS"),
                ("straight", 75.0, 75.0, "NS"),
                ("corner", 75.0, 60.0, "NE"),
            ],
            Ok((&[(90, 60), (105, 90)], 10, [105.0, 60.0, 120.0, 75.0])),
        ),
        (
            "north and east at the north-east corner square",
            &[
                ("straight", 120.0, 105.0, "NS"),
                ("corner", 120.0, 120.0, "SE"),
                ("corner", 135.0, 120.0, "WS"),
                ("straight", 135.0, 105.0, "NS"),
                ("corner", 135.0, 90.0, "NW"),
            ],
            Err("two halls meet the room's square 3, 3"),
        ),
    ];
    for (name, halls, want) in layouts {
        let (dir, world) = super::sample::sample("dungeon-ac2-two-halls");
        std::fs::write(
            dir.path().join("holtburg/records/ac2.toml"),
            "[[ac2-kit]]\nkit = \"tomb\"\n",
        )
        .unwrap();
        let mut s = Session::open_with(&world, &dats, None, Some(ac2_install())).unwrap();
        s.set_today("2026-11-24 00:00:00");
        let lb = empty_landblock(&store);
        let z = -40.0;
        for &(label, x, y, sides) in halls {
            place_ac2(&mut s, lb, label, "own", Vec3::new(x, y, z), sides);
        }
        let room = lyceum_foundry::edit::Ac2Room {
            landblock: lb,
            kit: Kit::Tomb,
            look: "own".to_owned(),
            south_west: (90.0, 60.0),
            floor: z,
            east: 3,
            north: 3,
        };
        let cells = s.cells(lb).len();
        let history = s.history();
        let (want, count, [x0, y0, x1, y1]) = match want {
            Ok(w) => w,
            Err(why) => {
                let refusal = s.lay_ac2_room(&room).expect_err(name).to_string();
                assert!(refusal.contains(why), "{name}: {refusal}");
                assert_eq!(s.cells(lb).len(), cells, "{name}: nothing laid");
                assert_eq!(s.history(), history, "{name}: the history as it was");
                continue;
            }
        };
        let laid = s
            .lay_ac2_room(&room)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(laid.len(), count, "{name}: two entrances and the tiles");
        let entrance = s
            .ac2_pieces()
            .iter()
            .find(|p| p.label == "room floor entrance")
            .unwrap()
            .environment;
        let mut entrances: Vec<(i32, i32)> = s
            .cells(lb)
            .values()
            .filter(|c| c.environment == entrance)
            .map(|c| {
                (
                    dereth_primitives::num::floor_to_i32(c.frame.origin.x + 0.5),
                    dereth_primitives::num::floor_to_i32(c.frame.origin.y + 0.5),
                )
            })
            .collect();
        entrances.sort_unstable();
        assert_eq!(entrances, want, "{name}: on the squares against the halls");
        // Every tile in the rectangle inside the halls' rows and columns: none beside an entrance.
        let now = s.cells(lb);
        for n in &laid {
            let c = &now[n];
            let (x, y) = (c.frame.origin.x, c.frame.origin.y);
            assert!(
                c.environment == entrance
                    || (x0 - 0.5..=x1 + 0.5).contains(&x) && (y0 - 0.5..=y1 + 0.5).contains(&y),
                "{name}: a tile at {x}, {y}, outside {x0}..{x1}, {y0}..{y1}"
            );
        }
        let found = s.walk(lb);
        assert!(found.is_empty(), "{name}: {found:#?}");
        s.undo().unwrap();
        assert_eq!(s.cells(lb).len(), cells, "{name}: one undo takes the room");
    }
}

/// At a tomb hall's free doorway, what is offered is only what AC2's own tombs join: each room
/// entrance by its second doorway (never its first, the room's, backwards), no room tile (a hall
/// never meets one directly), and the halls as before.
#[test]
fn a_tomb_hall_is_offered_only_the_joins_ac2s_tombs_make() {
    let (dir, world) = super::sample::sample("dungeon-ac2-offered");
    std::fs::write(
        dir.path().join("holtburg/records/ac2.toml"),
        "[[ac2-kit]]\nkit = \"tomb\"\n",
    )
    .unwrap();
    let dats = dereth_dat::testing::dat_dir();
    let mut s = Session::open_with(&world, &dats, None, Some(ac2_install())).unwrap();
    s.set_today("2026-11-22 00:00:00");
    let store = dereth_dat::testing::open_store_or_fail();
    let lb = empty_landblock(&store);
    let z = -40.0;
    place_ac2(
        &mut s,
        lb,
        "straight",
        "own",
        Vec3::new(105.0, 105.0, z),
        "NS",
    );
    let rooms: BTreeMap<u16, lyceum_foundry::room::Solid> = s
        .cells(lb)
        .iter()
        .filter_map(|(n, c)| {
            Some((
                *n,
                lyceum_foundry::room::Solid::of_cell(c, &s.environment(c.environment)?)?,
            ))
        })
        .collect();
    let free = s.free_doorways(lb, &[]);
    let among: Vec<usize> = (0..free.len()).collect();
    // Each piece: what fits there, and of that what is offered, with the doorway of the piece
    // that meets the hall in each.
    let offered = |label: &str| {
        let p = s
            .ac2_pieces()
            .iter()
            .find(|p| p.label == label)
            .unwrap()
            .clone();
        let env = s.environment(p.environment).unwrap();
        let form = lyceum_foundry::kit::form_on(&env, &p.kit.grid()).unwrap();
        let parts: Vec<lyceum_foundry::room::Shape> = (0..form.parts())
            .filter_map(|k| lyceum_foundry::room::Shape::of(&env, k))
            .collect();
        let d = lyceum_foundry::grid::Dungeon {
            rooms: &rooms,
            doorways: &free,
            ground: None,
            pitch: p.kit.grid().pitch,
        };
        let fits = lyceum_foundry::grid::piece_placements(&form, &parts, &d, &among);
        let n = fits.len();
        let kept = s.ac2_offered(lb, p.environment, &free, fits);
        let meeting: Vec<usize> = kept
            .iter()
            .flat_map(|pl| {
                (0..form.doors.len()).filter(|&slot| {
                    lyceum_foundry::pieces::opening(&env, 0, slot, &pl.frame).is_some_and(|o| {
                        free.iter().any(|f| {
                            (f.opening.middle.x - o.middle.x).abs() < 0.1
                                && (f.opening.middle.y - o.middle.y).abs() < 0.1
                                && (f.opening.middle.z - o.middle.z).abs() < 0.1
                        })
                    })
                })
            })
            .collect();
        (n, meeting)
    };
    for entrance in ["room floor entrance", "room floor entrance 2"] {
        let (fits, meeting) = offered(entrance);
        assert_eq!(fits, 4, "{entrance}: it fits either way round");
        assert_eq!(
            meeting,
            [1, 1],
            "{entrance}: offered by its second doorway only"
        );
    }
    for tile in [
        "room floor side",
        "room floor corner",
        "room floor two sides",
        "room floor three sides",
    ] {
        let (fits, meeting) = offered(tile);
        assert!(fits > 0, "{tile} fits against the hall");
        assert!(meeting.is_empty(), "{tile} is not offered: {meeting:?}");
    }
    let (fits, meeting) = offered("straight");
    assert_eq!((fits, meeting.len()), (4, 4), "halls as before");
}

/// AC2's castle on its 20 m grid, in both its looks: halls, a 60 m room and a tower of three
/// floors join and walk, and its piece for the purpose joins one of the tomb's halls.
#[test]
fn the_ac2_castle_stands_on_twenty_meter_squares_with_its_rooms_and_a_tower() {
    use lyceum_foundry::ac2_kit::Kit;
    let (dir, world) = super::sample::sample("dungeon-ac2-castle");
    std::fs::write(
        dir.path().join("holtburg/records/ac2.toml"),
        "[[ac2-kit]]\nkit = \"castle\"\n\n[[ac2-kit]]\nkit = \"tomb\"\n\n[[ac2-kit]]\nkit = \"fancy\"\n",
    )
    .unwrap();
    let dats = dereth_dat::testing::dat_dir();
    let mut s = Session::open_with(&world, &dats, None, Some(ac2_install())).unwrap();
    s.set_today("2026-11-21 00:00:00");
    let castle: Vec<_> = s
        .ac2_pieces()
        .iter()
        .filter(|p| p.kit == Kit::Castle)
        .cloned()
        .collect();
    assert_eq!(castle.len(), 47);
    assert!(
        castle
            .iter()
            .all(|p| p.looks.len() == 2 && p.looks[0].1 != p.looks[1].1),
        "the human look draws every piece in its own materials"
    );
    let kind = |label: &str| {
        let p = castle.iter().find(|p| p.label == label).unwrap();
        lyceum_foundry::kit::form_on(&s.environment(p.environment).unwrap(), &Kit::Castle.grid())
            .unwrap()
            .kind
    };
    for (label, k) in [
        ("straight", Kind::Corridor),
        ("corner", Kind::Corner),
        ("tee", Kind::Tee),
        ("cross", Kind::Cross),
        ("end c", Kind::DeadEnd),
        ("ramp", Kind::Stairs),
        ("room a", Kind::Room { long: 3, short: 3 }),
        // Its halls are a square across: a piece two squares across is a room.
        ("room b", Kind::Room { long: 2, short: 2 }),
        ("to tomb", Kind::Corridor),
        ("to fancy", Kind::Corridor),
    ] {
        assert_eq!(kind(label), k, "{label}");
    }
    let store = dereth_dat::testing::open_store_or_fail();
    let lb = empty_landblock(&store);
    let z = -40.0;
    place_ac2(
        &mut s,
        lb,
        "castle/straight",
        "human",
        Vec3::new(100.0, 100.0, z),
        "NS",
    );
    assert_eq!(s.grid_pitch(lb), 20.0);
    place_ac2(
        &mut s,
        lb,
        "castle/cross",
        "human",
        Vec3::new(100.0, 120.0, z),
        "NESW",
    );
    // The 60 m room south of the hall, a tower to the east, the tomb's hall to the west.
    place_ac2(
        &mut s,
        lb,
        "castle/room a",
        "human",
        Vec3::new(100.0, 60.0, z),
        "",
    );
    let bottom = place_ac2(
        &mut s,
        lb,
        "castle/tower bottom a",
        "human",
        Vec3::new(120.0, 120.0, z),
        "",
    );
    // The tower's floors over its bottom, each joined to the one under it.
    let middle = place_ac2(
        &mut s,
        lb,
        "castle/tower mid a",
        "human",
        Vec3::new(120.0, 120.0, z + 10.0),
        "",
    );
    let top = place_ac2(
        &mut s,
        lb,
        "castle/tower top a",
        "human",
        Vec3::new(120.0, 120.0, z + 20.0),
        "",
    );
    let cells = s.cells(lb);
    for (n, floor) in [(bottom, z), (middle, z + 10.0), (top, z + 20.0)] {
        let c = &cells[&n];
        assert!(
            (c.frame.origin.z - floor).abs() < 0.01
                && (c.frame.origin.x - 120.0).abs() < 0.01
                && (c.frame.origin.y - 120.0).abs() < 0.01,
            "a tower floor at {:?}",
            c.frame.origin
        );
    }
    place_ac2(
        &mut s,
        lb,
        "castle/to tomb",
        "own",
        Vec3::new(80.0, 120.0, z),
        "EW",
    );
    place_ac2(
        &mut s,
        lb,
        "tomb/straight",
        "own",
        Vec3::new(62.5, 120.0, z),
        "",
    );
    // North of the cross, its piece to a fancy hall, and the fancy hall.
    place_ac2(
        &mut s,
        lb,
        "castle/to fancy",
        "own",
        Vec3::new(100.0, 140.0, z),
        "NS",
    );
    let fancy = place_ac2(
        &mut s,
        lb,
        "fancy/hall straight",
        "own",
        Vec3::new(100.0, 160.0, z),
        "",
    );
    let at = s.cells(lb)[&fancy].frame.origin;
    assert!(
        (at.x - 100.0).abs() < 0.01 && (at.y - 160.0).abs() < 0.01,
        "the fancy hall against the castle's piece for it: {at:?}"
    );
    let found = s.walk(lb);
    assert!(found.is_empty(), "{found:#?}");
}

/// Every one of the world's own dungeon pieces reaches no further than its doorways, so retail's
/// fits are as they were; of AC2's kits' pieces only the castle's piece to a tomb's hall reaches
/// past them, as its catalog says.
#[test]
fn the_worlds_own_pieces_reach_none_and_of_ac2s_only_the_castles_to_tomb() {
    let (dir, world) = super::sample::sample("dungeon-ac2-reach");
    let kits: String = lyceum_foundry::ac2_kit::Kit::ALL
        .iter()
        .map(|k| format!("[[ac2-kit]]\nkit = \"{}\"\n\n", k.name()))
        .collect();
    std::fs::write(dir.path().join("holtburg/records/ac2.toml"), kits).unwrap();
    let dats = dereth_dat::testing::dat_dir();
    let s = Session::open_with(&world, &dats, None, Some(ac2_install())).unwrap();
    let set = lyceum_foundry::dats::DatSet::open(&dats).unwrap();
    let catalog = lyceum_foundry::pieces::catalog(&set);
    let mut retail = 0usize;
    for p in &catalog.kit {
        for part in 0..p.form.parts() {
            if let Some(shape) = s.ac2_shape(p.environment, part) {
                retail += 1;
                assert_eq!(shape.reach(), 0.0, "{:?} part {part}", p.environment);
            }
        }
    }
    assert!(retail > 100, "the world's own pieces: {retail}");
    let reaching: Vec<(&str, &str, f32)> = s
        .ac2_pieces()
        .iter()
        .filter_map(|p| {
            let r = s.ac2_shape(p.environment, 0)?.reach();
            (r != 0.0).then_some((p.kit.name(), p.label, r))
        })
        .collect();
    assert_eq!(reaching, [("castle", "to tomb", 0.84)]);
}

/// Every cell of AC2's dungeons in the install: by block, each cell's mesh, frame and doorways
/// (its mesh's doorway, the cell it leads to and that cell's doorway).
struct Ac2Cell {
    mesh: u32,
    frame: Frame,
    doors: Vec<(usize, u16, usize)>,
}

fn ac2_dungeon_cells(ac2: &lyceum_foundry::ac2_kit::Ac2Install) -> BTreeMap<(u16, u16), Ac2Cell> {
    let wr = ac2.records();
    let mut out = BTreeMap::new();
    for x in 0..=255i32 {
        for y in 0..=255i32 {
            let block = u16::try_from((x << 8) | y).unwrap();
            for low in 0x0100u16.. {
                let Some(c) = wr.cell(x, y, low) else { break };
                let (o, q) = (c.frame.origin, c.frame.rotation);
                out.insert(
                    (block, low),
                    Ac2Cell {
                        mesh: c.mesh.0,
                        frame: Frame::new(
                            Vec3::new(o.x, o.y, o.z),
                            dereth_primitives::Quat::new(q.w, q.x, q.y, q.z),
                        ),
                        doors: c
                            .portals
                            .iter()
                            .map(|p| {
                                (
                                    usize::from(p.portal),
                                    p.other_cell,
                                    usize::from(p.other_portal),
                                )
                            })
                            .collect(),
                    },
                );
            }
        }
    }
    out
}

/// Each cell mesh of `meshes` as an AC1 environment, made as an AC2 kit's pieces are.
fn ac2_environments(
    ac2: &lyceum_foundry::ac2_kit::Ac2Install,
    meshes: &BTreeSet<u32>,
) -> BTreeMap<u32, dereth_assets::Environment> {
    use dereth_assets::Decode;
    let mut pictures = dereth_ac2_convert::records::Pictures::default();
    let mut ids = dereth_ac2_convert::records::Ids::default();
    let mut out = dereth_ac2_convert::records::Records::default();
    let mut made = BTreeMap::new();
    let mut envs = BTreeMap::new();
    for &m in meshes {
        let Some((env, ..)) = dereth_ac2_convert::cell::environment_by_material(
            ac2.records(),
            ac2,
            m,
            &mut pictures,
            &mut ids,
            &mut out,
            &mut made,
        )
        .unwrap() else {
            continue;
        };
        envs.insert(
            m,
            dereth_assets::Environment::decode_payload(env, &out.0[&env]).unwrap(),
        );
    }
    envs
}

/// AC2's cave pieces, by cell mesh: the cave kit's, and those of AC2's caves the kit leaves for
/// later (its entrance from the land and its two great rooms).
fn ac2_caves() -> BTreeSet<u32> {
    lyceum_foundry::ac2_kit::PIECES
        .iter()
        .filter(|(k, _, _)| *k == lyceum_foundry::ac2_kit::Kit::Cave)
        .map(|(_, m, _)| *m)
        .chain([
            0x0100_006A,
            0x0100_0170,
            0x0100_0171,
            0x0100_00E1,
            0x0100_0135,
        ])
        .collect()
}

/// AC2's caves as spaces of voxels: no two cells joined in AC2's own caves share space, where by
/// their boxes thousands would; each pair a meter closer through the doorway joining them does,
/// and so does each cave piece set a meter into a copy of itself beside it. AC2's basic halls and
/// the castle's piece to a tomb's hall clash by voxels as by their boxes.
#[test]
fn the_caves_of_ac2_as_voxels_share_no_space_where_they_join_and_a_meter_closer_they_do() {
    use dereth_primitives::frame::V3;
    use lyceum_foundry::ac2_kit::{Kit, PIECES};
    use lyceum_foundry::room::Shape as Space;
    let ac2 = ac2_install();
    let cells = ac2_dungeon_cells(&ac2);
    assert!(cells.len() > 54_000, "AC2's dungeon cells: {}", cells.len());
    let caves: BTreeSet<u32> = ac2_caves();
    let meshes: BTreeSet<u32> = cells.values().map(|c| c.mesh).collect();
    let envs = ac2_environments(&ac2, &meshes);
    let kit_of = |m: u32, kit: Kit| PIECES.iter().any(|(k, mesh, _)| *k == kit && *mesh == m);
    let to_tomb = PIECES
        .iter()
        .find(|(k, _, l)| *k == Kit::Castle && *l == "to tomb")
        .unwrap()
        .1;
    // Voxels for the caves, and for basic's halls and the castle's to tomb and the tomb's halls,
    // to compare with their boxes.
    let voxel_of: BTreeMap<u32, Space> = {
        let started = std::time::Instant::now();
        let v: BTreeMap<u32, Space> = envs
            .iter()
            .filter(|(m, _)| {
                caves.contains(m)
                    || kit_of(**m, Kit::Basic)
                    || kit_of(**m, Kit::Tomb)
                    || **m == to_tomb
            })
            .filter_map(|(m, e)| Some((*m, Space::of_voxels(e, 0)?)))
            .collect();
        #[allow(clippy::cast_precision_loss)]
        let each = started.elapsed().as_secs_f64() * 1e3 / v.len() as f64;
        eprintln!("{} meshes as voxels, {each:.1} ms each", v.len());
        v
    };
    let box_of: BTreeMap<u32, Space> = envs
        .iter()
        .filter_map(|(m, e)| Some((*m, Space::of(e, 0)?)))
        .collect();
    let mut leaky: Vec<String> = caves
        .iter()
        .filter_map(|m| Some((m, voxel_of.get(m)?.filled_share()?)))
        .filter(|(_, f)| *f > 0.9)
        .map(|(m, f)| format!("{m:08X} {f:.2}"))
        .collect();
    leaky.sort();
    eprintln!("cave pieces whose space fills over 90% of their box: {leaky:?}");
    // Each piece's space as a world judges it: a cave's its voxels, any other's its box, reaching
    // as far as its kit's catalog says.
    let space = |m: u32| {
        if caves.contains(&m) {
            voxel_of.get(&m).cloned()
        } else {
            box_of
                .get(&m)
                .map(|s| s.clone().reaching(lyceum_foundry::ac2_kit::reach(m)))
        }
    };
    let (mut pairs, mut boxed, mut basic, mut basic_same, mut castle, mut castle_same) =
        (0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
    let (mut shared, mut missed) = (Vec::new(), Vec::new());
    let (mut by_voxels, mut by_box) = (std::time::Duration::ZERO, std::time::Duration::ZERO);
    let mut seen = BTreeSet::new();
    for (&(block, low), a) in &cells {
        for &(slot, other, other_slot) in &a.doors {
            let Some(b) = cells.get(&(block, other)) else {
                continue;
            };
            if low == other || !seen.insert((block, low.min(other), low.max(other))) {
                continue;
            }
            let Some(door) = lyceum_foundry::pieces::opening(&envs[&a.mesh], 0, slot, &a.frame)
            else {
                continue;
            };
            let back = lyceum_foundry::pieces::opening(&envs[&b.mesh], 0, other_slot, &b.frame)
                .expect("the other's doorway");
            let gap = door.middle.sub(back.middle);
            assert!(
                gap.dot(gap) < 0.05 * 0.05 && door.out.dot(back.out) < -0.99,
                "{block:04X} {low:04X}-{other:04X}: doorways that do not meet"
            );
            let name = format!("{block:04X} {low:04X}-{other:04X}");
            let both_basic = kit_of(a.mesh, Kit::Basic) && kit_of(b.mesh, Kit::Basic);
            let castles = a.mesh == to_tomb || b.mesh == to_tomb;
            if both_basic || castles {
                let (Some(va), Some(vb)) = (voxel_of.get(&a.mesh), voxel_of.get(&b.mesh)) else {
                    continue;
                };
                let by_boxes = box_of[&a.mesh]
                    .placed(&a.frame)
                    .overlaps(&box_of[&b.mesh].placed(&b.frame));
                let by_voxel = va.placed(&a.frame).overlaps(&vb.placed(&b.frame));
                if both_basic {
                    basic += 1;
                    basic_same += usize::from(by_boxes == by_voxel);
                }
                if castles && by_boxes {
                    castle += 1;
                    castle_same += usize::from(by_voxel);
                }
            }
            if !caves.contains(&a.mesh) && !caves.contains(&b.mesh) {
                continue;
            }
            let (Some(ma), Some(mb)) = (space(a.mesh), space(b.mesh)) else {
                continue;
            };
            pairs += 1;
            let (sa, sb) = (ma.placed(&a.frame), mb.placed(&b.frame));
            let t = std::time::Instant::now();
            if sa.overlaps(&sb) {
                shared.push(name.clone());
            }
            by_voxels += t.elapsed();
            let t = std::time::Instant::now();
            boxed += usize::from(
                box_of[&a.mesh]
                    .placed(&a.frame)
                    .overlaps(&box_of[&b.mesh].placed(&b.frame)),
            );
            by_box += t.elapsed();
            // A meter closer, through the doorway joining them, than either may reach.
            let reach =
                lyceum_foundry::ac2_kit::reach(a.mesh).max(lyceum_foundry::ac2_kit::reach(b.mesh));
            let closer = Frame::new(
                b.frame.origin.sub(door.out.mul(1.0 + reach)),
                b.frame.rotation,
            );
            if !sa.overlaps(&mb.placed(&closer)) {
                missed.push(name);
            }
        }
    }
    // Each cave piece beside a copy of itself, turned the same way: moved across its first
    // doorway's way until they share no space, then a meter back.
    let mut apart_missed = Vec::new();
    for m in &caves {
        let (Some(v), Some(env)) = (voxel_of.get(m), envs.get(m)) else {
            continue;
        };
        let here = Frame::new(Vec3::ZERO, Quat::IDENTITY);
        let Some(door) = lyceum_foundry::pieces::opening(env, 0, 0, &here) else {
            continue;
        };
        let across = Vec3::new(-door.out.y, door.out.x, 0.0);
        let one = v.placed(&here);
        let Some(clear) = (0..400u16)
            .map(|k| f32::from(k) * 0.25)
            .find(|d| !one.overlaps(&v.placed(&Frame::new(across.mul(*d), Quat::IDENTITY))))
        else {
            continue;
        };
        if clear >= 1.0
            && !one.overlaps(&v.placed(&Frame::new(across.mul(clear - 1.0), Quat::IDENTITY)))
        {
            apart_missed.push(format!("{m:08X}"));
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let each = |d: std::time::Duration| d.as_secs_f64() * 1e6 / pairs as f64;
    eprintln!(
        "{pairs} joined pairs with a cave piece: by their boxes {boxed} share space, by voxels {}; \
         {} not caught a meter closer, {} cave pieces not caught a meter into a copy beside \
         them; a pair judged in {:.1} us by voxels, {:.1} us by box; basic's {basic} pairs \
         {basic_same} the same by voxels; the castle's to tomb {castle}, {castle_same} of them \
         clashing by voxels too",
        shared.len(),
        missed.len(),
        apart_missed.len(),
        each(by_voxels),
        each(by_box)
    );
    assert!(shared.is_empty(), "{} of {pairs}: {shared:?}", shared.len());
    assert!(missed.is_empty(), "{} of {pairs}: {missed:?}", missed.len());
    assert!(apart_missed.is_empty(), "{apart_missed:?}");
    assert!(pairs > 12_000, "{pairs}");
    assert!(boxed > 2_000, "{boxed}");
}

/// The placements of `form` (its parts' spaces `spaces`) that join one of `dungeon`'s open
/// doorways `among`, found as they always were for the world's own kits and AC2's: each of its
/// doorways set against each open one at each quarter turn, met whole or on the square under it.
fn by_quarter_turns(
    form: &lyceum_foundry::kit::Form,
    spaces: &[lyceum_foundry::room::Shape],
    dungeon: &Dungeon<'_>,
    among: &[usize],
) -> Vec<lyceum_foundry::grid::PiecePlacement> {
    use dereth_primitives::frame::V3;
    use lyceum_foundry::grid::{placed, square_middle_at, square_of_at};
    let mut out = Vec::new();
    let mut tried: Vec<Frame> = Vec::new();
    for &i in among {
        let d = &dungeon.doorways[i];
        for door in &form.doors {
            for turn in 0..4u8 {
                let q = lyceum_foundry::compile::heading_quat(f32::from(turn) * 90.0);
                let local = placed(&door.opening, &Frame::new(Vec3::ZERO, q));
                if local.out.dot(d.opening.out) > -0.99 {
                    continue;
                }
                let whole = Frame::new(d.opening.middle.sub(local.middle), q);
                let o = whole.origin;
                let (sx, sy) =
                    square_middle_at(square_of_at(o.x, o.y, dungeon.pitch), dungeon.pitch);
                for frame in [whole, Frame::new(Vec3::new(sx, sy, o.z), q)] {
                    let same = |f: &Frame| {
                        let gap = f.origin.sub(frame.origin);
                        gap.dot(gap) < 1e-4 && quarter_turns(f) == quarter_turns(&frame)
                    };
                    if tried.iter().any(same) {
                        continue;
                    }
                    tried.push(frame);
                    if let Ok(p) = fit_piece(form, spaces, &frame, dungeon) {
                        if p.joined.contains(&i) {
                            out.push(p);
                        }
                    }
                }
            }
        }
    }
    out
}

/// Whether `a` and `b` are the same placements in the same order: frames to a millimeter and a
/// thousandth of a turn, and the same links.
fn same_placements(
    a: &[lyceum_foundry::grid::PiecePlacement],
    b: &[lyceum_foundry::grid::PiecePlacement],
) -> bool {
    use dereth_primitives::frame::V3;
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            let gap = x.frame.origin.sub(y.frame.origin);
            let (p, q) = (x.frame.rotation, y.frame.rotation);
            let turn = (p.w * q.w + p.x * q.x + p.y * q.y + p.z * q.z).abs();
            gap.dot(gap) < 1e-6 && turn > 0.999_999 && x.links == y.links && x.joined == y.joined
        })
}

/// Every piece of the world's own kit, at Holtburg Redoubt's doorways into a room taken away, and
/// every piece of each of AC2's kits on squares, at the doorways of a hall of its kit, is offered
/// exactly the placements it was when pieces were turned only by quarter turns.
#[test]
fn every_kit_piece_is_offered_the_placements_quarter_turns_gave_it() {
    // The world's own kit, in Holtburg Redoubt with a room taken away.
    let (_dir, mut s) = session("kit-turns");
    let before = s.cells(REDOUBT);
    let room = placed_pieces(&before)
        .into_iter()
        .find(|p| p.holds(0x017D))
        .expect("its piece");
    s.apply(&Op::TakeCells {
        landblock: REDOUBT,
        cells: room.cells.clone(),
    })
    .unwrap();
    let after = s.cells(REDOUBT);
    let free = s.free_doorways(REDOUBT, &[]);
    assert_eq!(free.len(), 3);
    let among: Vec<usize> = (0..free.len()).collect();
    let rooms = spaces(&s, &after, &[]);
    let dungeon = Dungeon {
        rooms: &rooms,
        doorways: &free,
        ground: None,
        pitch: PITCH,
    };
    let set = lyceum_foundry::dats::DatSet::open(&dereth_dat::testing::dat_dir()).unwrap();
    let mut offered = 0usize;
    for p in &catalog(&set).kit {
        let env = s.environment(p.environment).unwrap();
        let parts: Vec<lyceum_foundry::room::Shape> = (0..p.form.parts())
            .filter_map(|k| lyceum_foundry::room::Shape::of(&env, k))
            .collect();
        let now = lyceum_foundry::grid::piece_placements(&p.form, &parts, &dungeon, &among);
        assert!(
            same_placements(&now, &by_quarter_turns(&p.form, &parts, &dungeon, &among)),
            "{:?}",
            p.environment
        );
        offered += now.len();
    }
    assert!(offered > 50, "the world's own: {offered}");
    // AC2's kits, each at a hall of its own.
    let (dir, world) = super::sample::sample("kit-turns-ac2");
    let kits: String = lyceum_foundry::ac2_kit::Kit::ALL
        .iter()
        .map(|k| format!("[[ac2-kit]]\nkit = \"{}\"\n\n", k.name()))
        .collect();
    std::fs::write(dir.path().join("holtburg/records/ac2.toml"), kits).unwrap();
    let dats = dereth_dat::testing::dat_dir();
    let mut s = Session::open_with(&world, &dats, None, Some(ac2_install())).unwrap();
    s.set_today("2026-11-22 00:00:00");
    let store = dereth_dat::testing::open_store_or_fail();
    let mut offered = 0usize;
    // The kits on squares: the cave's, on none, is turned by eighths as its doorways ask.
    for (kit, lb) in lyceum_foundry::ac2_kit::Kit::ALL
        .iter()
        .filter(|k| k.grid().squares)
        .zip(empty_landblocks(&store))
    {
        let hall = match kit.name() {
            "fancy" => "fancy/hall straight".to_owned(),
            name => format!("{name}/straight"),
        };
        let at = Vec3::new(100.0, 100.0, -40.0);
        let first = place_ac2(&mut s, lb, &hall, "own", at, "NS");
        let cells = s.cells(lb);
        let rooms = spaces(&s, &cells, &[]);
        let free: Vec<lyceum_foundry::grid::Doorway> = s
            .free_doorways(lb, &[])
            .into_iter()
            .filter(|d| (d.opening.middle.z - at.z).abs() < 30.0)
            .collect();
        assert!(!free.is_empty(), "{hall} {first:04X}");
        let among: Vec<usize> = (0..free.len()).collect();
        let dungeon = Dungeon {
            rooms: &rooms,
            doorways: &free,
            ground: None,
            pitch: kit.grid().pitch,
        };
        for p in s.ac2_pieces().iter().filter(|p| p.kit == *kit) {
            let env = s.environment(p.environment).unwrap();
            let Some(form) = lyceum_foundry::kit::form_on(&env, &kit.grid()) else {
                continue;
            };
            let parts: Vec<lyceum_foundry::room::Shape> = (0..form.parts())
                .filter_map(|k| s.ac2_shape(p.environment, k))
                .collect();
            let now = lyceum_foundry::grid::piece_placements(&form, &parts, &dungeon, &among);
            assert!(
                same_placements(&now, &by_quarter_turns(&form, &parts, &dungeon, &among)),
                "{}/{}",
                kit.name(),
                p.label
            );
            offered += now.len();
        }
    }
    assert!(offered > 100, "AC2's: {offered}");
}

/// AC2's caves rebuilt by fit alone: of every two cave cells joined in AC2's own caves, the second
/// set against the first one's doorways lands on the frame AC2 gives it (as near as AC2's two
/// doorways are to each other), an eighth of a turn included, its space its voxels.
#[test]
fn the_caves_of_ac2_set_piece_by_piece_land_where_ac2_lays_them_eighth_turns_and_all() {
    use dereth_primitives::frame::V3;
    use lyceum_foundry::room::Shape as Space;
    let ac2 = ac2_install();
    let cells = ac2_dungeon_cells(&ac2);
    let caves: BTreeSet<u32> = ac2_caves();
    let meshes: BTreeSet<u32> = cells
        .values()
        .map(|c| c.mesh)
        .filter(|m| caves.contains(m))
        .collect();
    let envs = ac2_environments(&ac2, &meshes);
    let grid = lyceum_foundry::kit::Grid {
        pitch: 20.0,
        tile_height: 20.0,
        two_square_halls: false,
        squares: false,
    };
    let voxels: BTreeMap<u32, Space> = envs
        .iter()
        .filter_map(|(m, e)| Some((*m, Space::of_voxels(e, 0)?)))
        .collect();
    let forms: BTreeMap<u32, lyceum_foundry::kit::Form> = envs
        .iter()
        .filter_map(|(m, e)| Some((*m, lyceum_foundry::kit::form_on(e, &grid)?)))
        .collect();
    let (mut tried, mut eighths, mut placing) = (0usize, 0usize, std::time::Duration::ZERO);
    let mut lost = Vec::new();
    for (&(block, low), a) in &cells {
        for &(slot, other, other_slot) in &a.doors {
            let Some(b) = cells.get(&(block, other)) else {
                continue;
            };
            let (Some(sa), Some(sb), Some(form)) =
                (voxels.get(&a.mesh), voxels.get(&b.mesh), forms.get(&b.mesh))
            else {
                continue;
            };
            if low == other || form.doors.iter().all(|d| d.slot != other_slot) {
                continue;
            }
            let Some(open) = lyceum_foundry::pieces::opening(&envs[&a.mesh], 0, slot, &a.frame)
            else {
                continue;
            };
            let rooms = BTreeMap::from([(0u16, sa.placed(&a.frame))]);
            // Every doorway of the first that leads to the second (some forks meet twice), this one
            // first.
            let mut doorways = vec![lyceum_foundry::grid::Doorway {
                link: Link::Cell {
                    cell: "first".to_owned(),
                    portal: slot,
                },
                opening: open,
            }];
            for &(s2, o2, _) in &a.doors {
                if o2 == other && s2 != slot {
                    if let Some(o) =
                        lyceum_foundry::pieces::opening(&envs[&a.mesh], 0, s2, &a.frame)
                    {
                        doorways.push(lyceum_foundry::grid::Doorway {
                            link: Link::Cell {
                                cell: "first".to_owned(),
                                portal: s2,
                            },
                            opening: o,
                        });
                    }
                }
            }
            let dungeon = Dungeon {
                rooms: &rooms,
                doorways: &doorways,
                ground: None,
                pitch: grid.pitch,
            };
            tried += 1;
            eighths += usize::from(quarter_turns(&b.frame).is_none());
            let t = std::time::Instant::now();
            let found = lyceum_foundry::grid::piece_placements(
                form,
                std::slice::from_ref(sb),
                &dungeon,
                &[0],
            );
            placing += t.elapsed();
            // As near as AC2's own two doorways there are to each other (a centimeter apart in a
            // few of its caves), and a millimeter.
            let back = lyceum_foundry::pieces::opening(&envs[&b.mesh], 0, other_slot, &b.frame)
                .expect("the other's doorway");
            let apart = back.middle.sub(doorways[0].opening.middle);
            let near = apart.dot(apart).sqrt() + 0.001;
            let lands = found.iter().any(|p| {
                let gap = p.frame.origin.sub(b.frame.origin);
                let (x, y) = (p.frame.rotation, b.frame.rotation);
                gap.dot(gap) <= near * near
                    && (x.w * y.w + x.x * y.x + x.y * y.y + x.z * y.z).abs() > 0.999_9
            });
            if !lands {
                lost.push(format!(
                    "{block:04X} {low:04X}-{other:04X} {:08X}: {} placements",
                    b.mesh,
                    found.len()
                ));
            }
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let each = placing.as_secs_f64() * 1e3 / tried as f64;
    eprintln!(
        "{tried} cave pieces set against the doorway of the one before, {eighths} of them at an \
         eighth of a turn; {} not where AC2 lays them; {each:.2} ms a placement",
        lost.len()
    );
    assert!(tried > 20_000 && eighths > 5_000, "{tried} {eighths}");
    assert!(
        lost.is_empty(),
        "{} of {tried}: {:?}",
        lost.len(),
        &lost[..lost.len().min(20)]
    );
}

/// On a grid with no squares, each of AC2's cave pieces is what its doorways make it: none is
/// left over, a 45-degree bend is a corner and a Y a T.
#[test]
fn the_cave_pieces_of_ac2_are_read_by_their_doorways_off_the_square() {
    let ac2 = ac2_install();
    let caves: BTreeSet<u32> = ac2_caves();
    let envs = ac2_environments(&ac2, &caves);
    let grid = lyceum_foundry::kit::Grid {
        pitch: 20.0,
        tile_height: 20.0,
        two_square_halls: false,
        squares: false,
    };
    let kind = |m: u32| lyceum_foundry::kit::form_on(&envs[&m], &grid).map(|f| f.kind);
    let other: Vec<String> = envs
        .keys()
        .filter(|m| kind(**m).is_none_or(|k| k == Kind::Other))
        .map(|m| format!("{m:08X}"))
        .collect();
    assert!(other.is_empty(), "{other:?}");
    for (mesh, want) in [
        (0x0100_001C, Kind::Corridor), // straight
        (0x0100_0021, Kind::DeadEnd),  // end
        (0x0100_001E, Kind::Corner),   // turn
        (0x0100_0028, Kind::Corner),   // bend 45
        (0x0100_0065, Kind::Tee),      // t
        (0x0100_0067, Kind::Tee),      // y
        (0x0100_001D, Kind::Cross),    // cross
        (0x0100_0020, Kind::Stairs),   // straight up
    ] {
        assert_eq!(kind(mesh), Some(want), "{mesh:08X}");
    }
    // On a grid of squares, a 45-degree bend's diagonal doorway is off the grid.
    let squared = lyceum_foundry::kit::Grid {
        squares: true,
        ..grid
    };
    assert_eq!(
        lyceum_foundry::kit::form_on(&envs[&0x0100_0028], &squared).map(|f| f.kind),
        Some(Kind::Other)
    );
}

/// Kit piece `label` (`kit/label`) set in look `look` at one of the free doorways of cell `from`
/// of landblock `lb`, as the fits at a doorway offer it: the `pick`th of them. Its new cell.
fn place_ac2_at(s: &mut Session, lb: u16, label: &str, look: &str, from: u16, pick: usize) -> u16 {
    let p = s
        .ac2_pieces()
        .iter()
        .find(|p| {
            label
                .split_once('/')
                .is_some_and(|(k, l)| p.kit.name() == k && p.label == l)
        })
        .unwrap_or_else(|| panic!("{label} is a piece"))
        .clone();
    let env = s.environment(p.environment).unwrap();
    let form = lyceum_foundry::kit::form_on(&env, &p.kit.grid()).unwrap();
    let parts: Vec<lyceum_foundry::room::Shape> = (0..form.parts())
        .map(|k| s.ac2_shape(p.environment, k).unwrap())
        .collect();
    let cells = s.cells(lb);
    let rooms = spaces(s, &cells, &[]);
    let free = s.free_doorways(lb, &[]);
    let name = s.cell_names(lb)[&from].clone();
    let bare = |n: &str| n.rsplit(':').next().unwrap_or(n).to_owned();
    let among: Vec<usize> = free
        .iter()
        .enumerate()
        .filter(|(_, d)| matches!(&d.link, Link::Cell { cell, .. } if bare(cell) == bare(&name)))
        .map(|(i, _)| i)
        .collect();
    assert!(!among.is_empty(), "{from:04X} has no free doorway");
    let dungeon = Dungeon {
        rooms: &rooms,
        doorways: &free,
        ground: None,
        pitch: p.kit.grid().pitch,
    };
    let fits = s.ac2_offered(
        lb,
        p.environment,
        &free,
        lyceum_foundry::grid::piece_placements(&form, &parts, &dungeon, &among),
    );
    let fit = fits
        .get(pick)
        .unwrap_or_else(|| panic!("{label} at {from:04X}: {} fits", fits.len()))
        .clone();
    let surfaces = p.looks.iter().find(|(n, _)| *n == look).unwrap().1.clone();
    let (q, o) = (fit.frame.rotation, fit.frame.origin);
    s.apply(&Op::PlacePieces {
        landblock: lb,
        pieces: vec![NewPiece {
            environment: p.environment,
            surfaces,
            at: [o.x, o.y, o.z],
            rotation: Rotation::Quat([q.w, q.x, q.y, q.z]),
            parts: fit
                .links
                .iter()
                .enumerate()
                .map(|(k, l)| (u16::try_from(k).unwrap(), l.clone()))
                .collect(),
        }],
        joins: Vec::new(),
    })
    .unwrap();
    *s.cells(lb).keys().max().unwrap()
}

/// A sample cave of AC2's cave kit, set piece by piece at its doorways: a 20 m tunnel, a bend of
/// an eighth of a turn and a fork; down one arm to the small tunnels, out the other to the large;
/// back the other way a stair up to the wide tunnels, and a transition. Every family of the kit
/// is in it, eighth turns among its frames, and a walk through it finds nothing.
#[test]
fn a_sample_cave_of_every_family_is_set_at_its_doorways_and_walks_clean() {
    use lyceum_foundry::ac2_kit::Kit;
    let (dir, world) = super::sample::sample("dungeon-ac2-cave");
    std::fs::write(
        dir.path().join("holtburg/records/ac2.toml"),
        "[[ac2-kit]]\nkit = \"cave\"\n",
    )
    .unwrap();
    let dats = dereth_dat::testing::dat_dir();
    let mut s = Session::open_with(&world, &dats, None, Some(ac2_install())).unwrap();
    s.set_today("2026-11-22 00:00:00");
    let cave: Vec<_> = s
        .ac2_pieces()
        .iter()
        .filter(|p| p.kit == Kit::Cave)
        .cloned()
        .collect();
    assert_eq!(cave.len(), 108);
    // Each look of the kit draws the pieces of the set it is drawn over in other stones: of its
    // tunnels (88 pieces) and of its second set (`a`, 19), how many each look changes.
    let mut changed: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for p in &cave {
        for (n, surfaces) in &p.looks[1..] {
            let e = changed.entry(n).or_default();
            if *surfaces != p.looks[0].1 {
                if p.label.ends_with(" a") {
                    e.1 += 1;
                } else {
                    e.0 += 1;
                }
            }
        }
    }
    assert_eq!(
        changed,
        BTreeMap::from([
            ("coldstone", (88, 19)),
            ("dark", (0, 19)),
            ("greystone", (88, 19)),
            ("ice", (88, 0)),
            ("olthoi", (88, 19)),
            ("red", (0, 19)),
            ("seashore", (88, 0)),
        ])
    );
    let store = dereth_dat::testing::open_store_or_fail();
    let lb = empty_landblock(&store);
    let z = -60.0;
    let first = place_ac2(
        &mut s,
        lb,
        "cave/straight",
        "own",
        Vec3::new(100.0, 100.0, z),
        "NS",
    );
    let bend = place_ac2_at(&mut s, lb, "cave/bend 45", "own", first, 0);
    let fork = place_ac2_at(&mut s, lb, "cave/y", "own", bend, 0);
    // One arm: down to the small tunnels.
    let small = place_ac2_at(&mut s, lb, "cave/to small straight", "own", fork, 0);
    let small = place_ac2_at(&mut s, lb, "cave/small straight", "own", small, 0);
    place_ac2_at(&mut s, lb, "cave/small end", "own", small, 0);
    // The other: out to the large.
    let large = place_ac2_at(&mut s, lb, "cave/bend 45", "own", fork, 0);
    let large = place_ac2_at(&mut s, lb, "cave/to large straight", "own", large, 0);
    let large = place_ac2_at(&mut s, lb, "cave/large straight", "own", large, 0);
    place_ac2_at(&mut s, lb, "cave/large end", "own", large, 0);
    // Back the other way: a stair up to the wide tunnels.
    let up = place_ac2_at(&mut s, lb, "cave/straight up", "own", first, 0);
    let wide = place_ac2_at(&mut s, lb, "cave/transition straight", "own", up, 0);
    let wide = place_ac2_at(&mut s, lb, "cave/wide straight", "own", wide, 0);
    place_ac2_at(&mut s, lb, "cave/wide end", "own", wide, 0);
    let cells = s.cells(lb);
    let label_of = |env: DataId| {
        cave.iter()
            .find(|p| p.environment == env)
            .map_or("?", |p| p.label)
    };
    let labels: BTreeSet<&str> = cells.values().map(|c| label_of(c.environment)).collect();
    for family in [
        "small ",
        "large ",
        "wide ",
        "to small",
        "to large",
        "transition",
        "bend 45",
        "y",
        "straight up",
    ] {
        assert!(
            labels.iter().any(|l| l.starts_with(family) || *l == family),
            "{family}: {labels:?}"
        );
    }
    assert!(
        cells.values().any(|c| quarter_turns(&c.frame).is_none()),
        "an eighth of a turn among its frames"
    );
    let found = s.walk(lb);
    assert!(found.is_empty(), "{found:#?}");
}

mod pull {
    //! Behaviour: none (tooling: a retail dungeon pulled from a world over the retail files into a world repository)
    use std::path::{Path, PathBuf};

    use lyceum_foundry::pull::{pull, Entrance, Request};
    use lyceum_foundry::repo::RowSchema;

    use crate::sample::sample;

    /// The dungeon pulled: Holtburg's, and another to copy into it.
    const DUNGEON: u16 = 0x01F6;
    const OTHER: u16 = 0x01F7;

    /// A server's world around the dungeon: a generator in it with a creature it links, a portal out
    /// onto the land, and the portal into it, placed on the land.
    const DATABASE: &str = "\
    INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`) VALUES (1154,'gen',1,'2005-02-09 10:00:00'),(7924,'drudge',10,'2005-02-09 10:00:00'),(3631,'surface',7,'2005-02-09 10:00:00'),(3630,'way-in',7,'2005-02-09 10:00:00');
    INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`) VALUES (3631,1,'Surface'),(3630,1,'Holtburg Dungeon');
    INSERT INTO `weenie_properties_position` (`object_Id`, `position_Type`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`) VALUES (3631,2,0xA9B40031,84,141,42,1,0,0,0),(3630,2,0x01F60100,10,-30,0,1,0,0,0);
    INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`) VALUES (0x701F6000,1154,0x01F60100,10,-30,0,1,0,0,0,'\\0','2019-05-16 00:00:00'),(0x701F6001,7924,0x01F60101,12,-31,0,1,0,0,0,'\u{1}','2019-05-16 00:00:00'),(0x701F6002,3631,0x01F60102,14,-32,0,1,0,0,0,'\\0','2019-05-16 00:00:00'),(0x7A9B4000,3630,0xA9B40031,84,141,42,1,0,0,0,'\\0','2019-05-16 00:00:00');
    INSERT INTO `landblock_instance_link` (`parent_GUID`, `child_GUID`, `last_Modified`) VALUES (0x701F6000,0x701F6001,'2019-05-16 00:00:00');
    ";

    /// The server's vocabulary for the rows written, as `empyrean-import schema` writes it.
    fn schema() -> RowSchema {
        RowSchema::of_schema(&serde_json::json!({ "rows": {
            "placement": {
                "file": "landblock/{cell >> 16:04X}.toml",
                "item": "spawn {guid:#010X}",
                "key": ["guid"],
                "fields": {
                    "guid": "hex", "class": "int", "cell": "hex",
                    "at": { "list": "float" }, "rotation": { "list": "float" },
                    "link_child": "bool", "last_modified": "time",
                    "links": { "list": { "either": ["hex", { "fields": { "child": "hex", "last_modified": "time" } }] } },
                },
            },
            "weenie": { "file": "weenie/{class:05}.toml", "item": "weenie {class}", "key": ["class"], "fields": { "class": "int" } },
        } }))
        .unwrap()
        .unwrap()
    }

    /// A world repository of one layer and one pack in `root`, over the retail files with no land:
    /// the pack's Lyceum edits `edits` (landblock, text), if any.
    fn repository(root: &Path, edits: Option<(u16, &str)>) -> PathBuf {
        let write = |rel: &str, text: &str| {
            let p = root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, text).unwrap();
        };
        write(
            "base/emergence/pack.toml",
            "name = \"emergence\"\nversion = \"1\"\n",
        );
        write("base/library.txt", "# The way in.\nweenie 3630\n");
        if let Some((lb, text)) = edits {
            write(
                &format!("base/emergence/edits/landblocks/{lb:04X}.toml"),
                text,
            );
        }
        write(
            "world.toml",
            "format_version = 1\ngame = \"ac1\"\nkey = \"test-repo\"\nera = \"newdereth\"\npacks = [\"base/emergence\"]\nlineage = []\n[base]\nmonth = \"2026-12\"\n",
        );
        root.join("world.toml")
    }

    /// A retail dungeon pulled from a world over the retail files into a repository whose world holds
    /// its cells alike: what the database places in it as the pack's rows, linked as it links them; a
    /// portal into it where asked, of the class that leads there; its exit onto the land, and every
    /// class the repository lacks, reported. Pulled again, it is refused for what is there now; a
    /// repository whose world holds the dungeon's cells otherwise, or a landblock with no dungeon, are
    /// refused before anything is written.
    #[test]
    fn a_retail_dungeon_is_pulled_into_a_repository_as_its_rows() {
        let (dir, world) = sample("pull-dungeon");
        let dats = dereth_dat::testing::dat_dir();
        let database = dir.path().join("ACE-World-Database-v0.9.296.sql");
        std::fs::write(&database, DATABASE).unwrap();
        let vocabulary = schema();
        let target = repository(&dir.path().join("repo"), None);
        let run = |target: &Path, lb: u16, entrance: Option<Entrance>, alongside: bool| {
            pull(
                &Request {
                    source: &world,
                    database: &database,
                    landblock: lb,
                    target,
                    schema: &vocabulary,
                    land: None,
                    pack: None,
                    entrance,
                    portal: None,
                    alongside,
                    dry_run: false,
                    library: true,
                },
                &dats,
            )
        };
        let at_holtburg = Entrance {
            cell: 0xA9B4_0031,
            at: [90.0, 140.0, 42.0],
            heading: 90.0,
        };
        let done = run(&target, DUNGEON, Some(at_holtburg), false).unwrap();
        assert_eq!(done.plan.rows.len(), 3);
        assert_eq!(done.already, 0);
        assert_eq!(
            done.entrance,
            Some((0x7A9B_4800, 3630)),
            "the portal that leads in"
        );
        assert_eq!(done.plan.exits.len(), 1);
        assert!(done.plan.exits[0].outdoors());
        let missing: Vec<u32> = done.missing.iter().map(|(c, _)| *c).collect();
        assert_eq!(
            missing,
            [1154, 3630, 3631, 7924],
            "the repository defines none"
        );
        let (list, added) = done.library.clone().expect("its library list");
        assert_eq!(
            added,
            [1154, 3631, 7924],
            "all but the one it names already"
        );
        let text = std::fs::read_to_string(&list).unwrap();
        assert!(text.contains("weenie 3631  # Surface\n"), "{text}");
        let repo = dir.path().join("repo/base/emergence");
        let rows = std::fs::read_to_string(repo.join("landblock/01F6.toml")).unwrap();
        assert!(rows.contains("guid = 0x701F6000"), "{rows}");
        assert!(rows.contains("links = [0x701F6001]"), "{rows}");
        for part in ["source = \"ACE-World-Database\"", "revision = \"v0.9.296\""] {
            assert!(rows.contains(part), "{part}: {rows}");
        }
        let way_in = std::fs::read_to_string(repo.join("landblock/A9B4.toml")).unwrap();
        assert!(
            way_in.contains("guid = 0x7A9B4800") && way_in.contains("class = 3630"),
            "{way_in}"
        );

        // Pulled again: what the repository places there now stands in the way.
        let again = run(&target, DUNGEON, None, false).unwrap_err().to_string();
        assert!(again.contains("already places 3"), "{again}");
        let beside = run(&target, DUNGEON, None, true).unwrap_err().to_string();
        assert!(beside.contains("already uses 3 of the guids"), "{beside}");

        // A repository whose world copies another dungeon into this one holds its cells otherwise.
        let copied = repository(
            &dir.path().join("copied"),
            Some((
                DUNGEON,
                &format!("[[add-dungeon]]\nname = \"copy\"\ntemplate = 0x{OTHER:04X}\n"),
            )),
        );
        let differ = run(&copied, DUNGEON, None, false).unwrap_err().to_string();
        assert!(differ.contains("otherwise than"), "{differ}");
        assert!(
            !dir.path().join("copied/base/emergence/landblock").exists(),
            "nothing written"
        );

        // Holtburg's landblock holds buildings' rooms, not a dungeon.
        let town = run(&target, 0xA9B4, None, false).unwrap_err().to_string();
        assert!(town.contains("not a dungeon"), "{town}");
    }
}
