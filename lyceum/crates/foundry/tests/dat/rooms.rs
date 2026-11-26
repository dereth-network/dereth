//! Behaviour: none (tooling: which indoor cell a point is in, and a spawn in a room checked against the room's floor and a new one set on it, over the retail files and AC2's kits)
//! Fixture: the retail files, and for AC2's kits the Asheron's Call 2 install they are made from
//! (`AUBEREAN_TEST_DAT_DIR`).

use dereth_assets::world::CellLandblock;
use dereth_assets::Decode;
use dereth_primitives::frame::V3;
use dereth_primitives::{DataId, Frame, Quat, Vec3};
use lyceum_foundry::edit::{Op, Session, Weenie};
use lyceum_foundry::floor::{Floor, Stand};
use lyceum_foundry::source::{Link, Rotation};

const HOLTBURG: u16 = 0xA9B4;

#[test]
fn every_holtburg_room_holds_its_own_middle_and_no_room_holds_the_street() {
    let (_dir, world) = super::sample::sample("pack-rooms");
    let s = Session::open(&world, &dereth_dat::testing::dat_dir(), None).unwrap();
    let shapes = s.cell_shapes(HOLTBURG);
    let cells = s.cells(HOLTBURG);
    assert_eq!(shapes.len(), cells.len());
    for (index, cell) in &cells {
        let env = s.environment(cell.environment).unwrap();
        let cs = &env.cells[usize::from(cell.cell_struct)];
        #[allow(clippy::cast_precision_loss)]
        let n = cs.vertex_array.vertices.len() as f32;
        let sum = cs.vertex_array.vertices.iter().fold(Vec3::ZERO, |a, v| {
            a.add(dereth_primitives::frame::localtoglobal(
                &cell.frame,
                v.position,
            ))
        });
        let middle = sum.mul(1.0 / n);
        let holding: Vec<u16> = shapes
            .iter()
            .filter(|r| r.holds(middle))
            .map(|r| r.index)
            .collect();
        assert!(holding.contains(index), "0x{index:04X}: {holding:04X?}");
        // Holtburg's rooms are a town's: every one is seen from outside.
        assert!(shapes.iter().any(|r| r.index == *index && r.seen_outside));
    }
    // The middle of the landblock's south edge, at the ground: the street, in no room.
    let street = Vec3::new(96.0, 1.0, s.ground(HOLTBURG, 96.0, 1.0).unwrap() + 1.0);
    assert!(shapes.iter().all(|r| !r.holds(street)));
}

#[test]
fn the_worlds_dungeons_are_the_landblocks_whose_rooms_are_not_seen_from_outside() {
    let (_dir, world) = super::sample::sample("pack-dungeons");
    let s = Session::open(&world, &dereth_dat::testing::dat_dir(), None).unwrap();
    let started = std::time::Instant::now();
    let retail = s.retail_dungeons();
    println!("{} dungeons in {:?}", retail.len(), started.elapsed());
    // The Drudge Hideout is one; Holtburg's houses are a town's.
    assert!(retail.contains_key(&0x019E));
    assert!(!retail.contains_key(&HOLTBURG));
    assert!(retail.len() > 1_000, "{}", retail.len());
    // With no server read, none is named.
    let all = s.dungeons(&retail);
    assert_eq!(all.len(), retail.len());
    assert!(all.iter().all(|d| d.name.is_none() && !d.pack));
}

/// The player's Asheron's Call 2 install the AC2 kits are made from: `AUBEREAN_TEST_DAT_DIR`.
fn ac2_install() -> lyceum_foundry::ac2_kit::Ac2Install {
    let dir = std::env::var_os("AUBEREAN_TEST_DAT_DIR")
        .filter(|v| !v.is_empty())
        .expect("AUBEREAN_TEST_DAT_DIR: the Asheron's Call 2 install the AC2 kits are made from");
    lyceum_foundry::ac2_kit::open(std::path::Path::new(&dir)).expect("the AC2 install opens")
}

/// The first landblock of the open sea floor: heights all 0, and no statics record or cells.
fn empty_landblock() -> u16 {
    let store = dereth_dat::testing::open_store_or_fail();
    (0x0101u16..0xFEFE)
        .find(|lb| {
            let id = DataId((u32::from(*lb) << 16) | 0xFFFF);
            let Ok(b) = store.read_cell(id) else {
                return false;
            };
            let land = CellLandblock::decode_payload(id, &b).unwrap();
            land.lbi_exists == 0 && land.height.iter().all(|h| *h == 0)
        })
        .expect("an empty landblock")
}

/// The checks' faults of a spawn in a room ([`lyceum_foundry::floor::check_packs`]).
fn floor_faults(s: &Session) -> Vec<String> {
    let scratch = dereth_dat::testing::ScratchDir::new("floors-check").unwrap();
    s.write_scratch(scratch.path()).unwrap();
    s.check(scratch.path())
        .unwrap()
        .into_iter()
        .filter(|f| !f.warning && f.text.contains(" in room 0x"))
        .map(|f| f.text)
        .collect()
}

/// AC2's halls and rooms have their floor a meter or more over their cell, and its rooms' side
/// and corner tiles have no floor in the middle at all: a spawn written at such a room's middle,
/// half a meter up, is a fault naming the room and its floor; one placed there lands on the
/// floor (the one straight over it, or else the middle of the room's floor), and stands.
#[test]
fn a_spawn_at_an_ac2_rooms_middle_is_a_fault_and_one_placed_there_lands_on_its_floor() {
    let (dir, world) = super::sample::sample("floors-ac2");
    std::fs::write(
        dir.path().join("holtburg/records/ac2.toml"),
        "[[ac2-kit]]\nkit = \"basic\"\n",
    )
    .unwrap();
    let dats = dereth_dat::testing::dat_dir();
    let mut s = Session::open_with(&world, &dats, None, Some(ac2_install())).unwrap();
    s.set_today("2026-11-26 00:00:00");
    let lb = empty_landblock();
    // A hall, its floor over its middle a meter up; and a room's side tile, a ledge along one of
    // its sides with nothing at its middle.
    let mut rooms = Vec::new();
    for (k, label) in ["straight", "room floor side"].into_iter().enumerate() {
        let p = s
            .ac2_pieces()
            .iter()
            .find(|p| p.label == label)
            .unwrap_or_else(|| panic!("{label} is a piece"))
            .clone();
        let env = s.environment(p.environment).unwrap();
        #[allow(clippy::cast_precision_loss)]
        let at = Vec3::new(40.0 + 60.0 * k as f32, 96.0, -40.0);
        let f = Floor::of_piece(&env, 0, &Frame::new(at, Quat::IDENTITY)).unwrap();
        assert!(!f.is_empty(), "{label} has a floor");
        s.apply(&Op::PlaceCell {
            landblock: lb,
            environment: p.environment,
            piece: 0,
            surfaces: p.looks[0].1.clone(),
            at: [at.x, at.y, at.z],
            rotation: Rotation::Heading(0.0),
            links: vec![Link::Shut; env.cells[0].portals.len()],
        })
        .unwrap();
        rooms.push((label, at));
    }
    let cells: Vec<u16> = s.cells(lb).keys().copied().collect();
    assert_eq!(cells, [0x0100, 0x0101]);
    let floor = |s: &Session, c: u16| s.room_floor(lb, c).expect("the room's floor");
    // The hall's middle is under its floor; the side tile's is over none.
    let (hall, side) = (rooms[0].1, rooms[1].1);
    let up = Vec3::new(0.0, 0.0, 0.5);
    assert!(
        matches!(floor(&s, 0x0100).stand(hall.add(up)), Stand::Under(d) if d > 0.3),
        "{:?}",
        floor(&s, 0x0100).stand(hall.add(up))
    );
    assert_eq!(floor(&s, 0x0101).stand(side.add(up)), Stand::Over);
    // Written there as rows are written, both are faults, each naming its room and the nearest
    // point of its floor.
    let pack = s.packs()[s.target()].clone();
    let mut text = String::new();
    for (k, at) in [hall, side].into_iter().enumerate() {
        let mut row = toml::Table::new();
        let guid = lyceum_foundry::placement_sql::guid_of(lb, u16::try_from(k).unwrap());
        row.insert("guid".into(), toml::Value::Integer(guid.into()));
        row.insert("class".into(), toml::Value::Integer(1));
        let cell = (u32::from(lb) << 16) | u32::from(cells[k]);
        row.insert("cell".into(), toml::Value::Integer(cell.into()));
        let p = at.add(up);
        row.insert(
            "at".into(),
            toml::Value::Array(
                [p.x, p.y, p.z]
                    .iter()
                    .map(|v| toml::Value::Float(f64::from(*v)))
                    .collect(),
            ),
        );
        text.push_str(&lyceum_foundry::placement_sql::to_sql(&row, "2026-11-26 00:00:00").unwrap());
    }
    let file = pack.dir.join("server/sql/camps.sql");
    s.apply(&Op::WriteSql {
        file: file.clone(),
        text: Some(text),
    })
    .unwrap();
    let found = floor_faults(&s);
    assert_eq!(found.len(), 2, "{found:#?}");
    let hall_cell = format!("room 0x{:08X}", (u32::from(lb) << 16) | 0x0100);
    let side_cell = format!("room 0x{:08X}", (u32::from(lb) << 16) | 0x0101);
    assert!(
        found[0].contains(&hall_cell)
            && found[0].contains("under its room's floor")
            && found[0].contains("the nearest floor is at ["),
        "{found:#?}"
    );
    assert!(
        found[1].contains(&side_cell)
            && found[1].contains("over no floor")
            && found[1].contains("the nearest floor is at ["),
        "{found:#?}"
    );
    assert!(found
        .iter()
        .all(|f| f.starts_with(&format!("{}:", file.display()))));
    s.apply(&Op::WriteSql { file, text: None }).unwrap();
    assert!(floor_faults(&s).is_empty());
    // Placed at those same places, each lands on its room's floor: the hall's on the floor
    // straight over it, the side tile's at the point of its ledge farthest from the edges.
    for (k, at) in [hall, side].into_iter().enumerate() {
        let applied = s
            .apply(&Op::PlaceSpawnInCell {
                landblock: lb,
                cell: cells[k],
                weenie: Weenie::Class(1),
                origin: at.add(up),
                heading: 0.0,
            })
            .unwrap();
        let file = applied.spawned.unwrap();
        let spawn = s.spawns().into_iter().find(|x| x.file == file).unwrap();
        let o = spawn.frame.origin;
        let f = floor(&s, cells[k]);
        assert_eq!(f.stand(o), Stand::On(0.0), "{o:?}");
        assert!(
            f.under(o).is_some_and(|q| (q.z - o.z).abs() < 1e-3),
            "{o:?}"
        );
        if k == 0 {
            // Under the floor, it goes up onto the floor straight over it.
            assert!((o.x - at.x).abs() < 1e-3 && (o.y - at.y).abs() < 1e-3);
            assert!(o.z > at.z + 0.5, "the floor is over the middle: {o:?}");
        } else {
            // Written to the millimeter.
            let m = f.middle().unwrap();
            assert!(m.sub(o).dot(m.sub(o)).sqrt() < 0.01, "{m:?} {o:?}");
            assert!((o.x - at.x).abs() > 1.0 || (o.y - at.y).abs() > 1.0);
        }
    }
    // Written as an edit naming only its room, a spawn goes to the middle of the room's floor.
    let edit = format!(
        "[[edit]]
do = \"spawn\"
weenie = 1
landblock = \"{lb:04X}\"
cell = \"0101\"
"
    );
    let pack = s.packs()[s.target()].name.clone();
    for a in lyceum_foundry::script::actions(&edit, &pack).unwrap() {
        let file = s.act(&a).unwrap().spawned.unwrap();
        let o = s
            .spawns()
            .into_iter()
            .find(|x| x.file == file)
            .unwrap()
            .frame
            .origin;
        let m = floor(&s, 0x0101).middle().unwrap();
        assert!(m.sub(o).dot(m.sub(o)).sqrt() < 0.01, "{m:?} {o:?}");
    }
    // A spawn that stands passes.
    assert!(floor_faults(&s).is_empty(), "{:#?}", floor_faults(&s));
}

/// The same rule in AC1's rooms: a spawn on a retail corridor's floor, or over it, stands, and
/// one under it, or beside it over no floor, is a fault.
#[test]
fn a_spawn_in_an_ac1_room_stands_on_its_floor_and_not_over_or_under_it() {
    let (_dir, world) = super::sample::sample("floors-ac1");
    let mut s = Session::open(&world, &dereth_dat::testing::dat_dir(), None).unwrap();
    s.set_today("2026-11-26 00:00:00");
    let lb = empty_landblock();
    let piece = lyceum_foundry::pieces::catalog(s.base())
        .pieces
        .iter()
        .filter(|p| p.portals == 2)
        .max_by_key(|p| p.cells)
        .cloned()
        .expect("a corridor");
    let env = s.environment(piece.environment).unwrap();
    s.apply(&Op::PlaceCell {
        landblock: lb,
        environment: piece.environment,
        piece: piece.piece,
        surfaces: piece.surfaces.clone(),
        at: [96.0, 96.0, -40.0],
        rotation: Rotation::Heading(0.0),
        links: vec![Link::Shut; env.cells[usize::from(piece.piece)].portals.len()],
    })
    .unwrap();
    let f = s.room_floor(lb, 0x0100).unwrap();
    // Its floor is level with its frame, as AC1's are.
    let floor = f.middle().expect("a floor");
    assert!((floor.z + 40.0).abs() < 0.05, "{floor:?}");
    assert_eq!(f.stand(floor), Stand::On(0.0));
    assert!(matches!(
        f.stand(floor.add(Vec3::new(0.0, 0.0, 1.0))),
        Stand::On(_)
    ));
    assert_eq!(f.stand(floor.add(Vec3::new(50.0, 50.0, 1.0))), Stand::Over);
    assert!(matches!(
        f.stand(floor.add(Vec3::new(0.0, 0.0, -0.5))),
        Stand::Under(_) | Stand::Over
    ));
    assert!(f.fault(floor).is_none());
    assert!(f.fault(floor.add(Vec3::new(0.0, 0.0, -0.5))).is_some());
    // Placed in the air over it, a spawn lands on it.
    let file = s
        .apply(&Op::PlaceSpawnInCell {
            landblock: lb,
            cell: 0x0100,
            weenie: Weenie::Class(1),
            origin: floor.add(Vec3::new(0.0, 0.0, 1.5)),
            heading: 0.0,
        })
        .unwrap()
        .spawned
        .unwrap();
    let o = s
        .spawns()
        .into_iter()
        .find(|x| x.file == file)
        .unwrap()
        .frame
        .origin;
    assert!((o.z - floor.z).abs() < 1e-3, "{o:?} {floor:?}");
    assert!(floor_faults(&s).is_empty(), "{:#?}", floor_faults(&s));
}
