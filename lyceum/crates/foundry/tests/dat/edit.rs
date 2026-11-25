//! Behaviour: none (tooling: the world editor's edit session and picks over the sample world on the retail files)

use dereth_dat::ModernDat;
use dereth_primitives::{DataId, Vec3};
use lyceum_foundry::compile;
use lyceum_foundry::edit::Session;
use lyceum_foundry::pick::{self, Hit, Mask, Models, Ray, BLOCK};
use lyceum_foundry::world::World;

const EDITS: &str = r#"
[[edit]]
do = "place-static"
landblock = "A9B4"
model = 0x0200040B
at = [30.0, 60.0, "ground"]
heading = 45.0

[[edit]]
do = "move-static"
landblock = "A9B4"
static = "stone-west"
at = [20.0, 26.0, "ground"]

[[edit]]
do = "move-static"
landblock = "A9B4"
index = 0
heading = 15.0

[[edit]]
do = "brush"
brush = "raise"
amount = 3
landblock = "A9B4"
at = [0.0, 72.0]
radius = 30.0

[[edit]]
do = "brush"
brush = "terrain"
value = 4
landblock = "A9B4"
at = [12.0, 96.0]
radius = 20.0

[[edit]]
do = "spawn"
weenie = "guide"
landblock = "A8B4"
at = [170.0, 36.0, "ground"]
heading = 270.0

[[edit]]
do = "remove-static"
landblock = "A8B4"
static = "house-stone-2"
"#;

fn session(tag: &str) -> (dereth_dat::testing::ScratchDir, Session) {
    let (dir, world) = super::sample::sample(tag);
    let mut s = Session::open(&world, &dereth_dat::testing::dat_dir(), None).unwrap();
    s.set_today("2026-10-18 00:00:00");
    (dir, s)
}

fn full(s: &Session) -> compile::Compiled {
    let world = World::read(&s.world().path).unwrap();
    let lock = lyceum_foundry::lock::Lock::read(&world.lock_path()).unwrap();
    compile::compile(&world, s.packs(), s.base(), &lock).unwrap()
}

#[test]
fn an_edit_session_over_the_sample_world_compiles_the_bytes_a_build_does() {
    let (dir, mut s) = session("edit-sample");
    let actions = lyceum_foundry::script::actions(EDITS, "holtburg").unwrap();
    assert_eq!(actions.len(), 7);
    for (i, a) in actions.iter().enumerate() {
        let done = s.act(a).unwrap();
        assert!(!done.touched.landblocks.is_empty(), "edit {}", i + 1);
        let f = full(&s);
        assert_eq!(s.compiled().records, f.records, "edit {}", i + 1);
        assert_eq!(s.compiled().sql, f.sql, "edit {}", i + 1);
    }
    // The raise on the A9B4/A8B4 edge reached both landblocks' copies of x = 0 / x = 8.
    let (east, west) = (s.terrain(0xA9B4).unwrap(), s.terrain(0xA8B4).unwrap());
    for y in 2..=4 {
        assert_eq!(east.height[y], west.height[8 * 9 + y], "row {y}");
    }
    s.save().unwrap();
    compile::build(&s.world().path, s.base().dir.as_path(), None, false).unwrap();
    let mine = dir.path().join("mine");
    compile::write_overlay(s.compiled(), s.base(), "sample-world", &mine).unwrap();
    for t in [ModernDat::Portal, ModernDat::Cell] {
        let name = dereth_dat::overlay::container_name(t);
        assert_eq!(
            std::fs::read(mine.join(name)).unwrap(),
            std::fs::read(dir.path().join("build/overlay").join(name)).unwrap(),
            "{name}"
        );
    }
    // Undo all seven: the sources and the records are the sample's again.
    for _ in 0..7 {
        s.undo().unwrap();
    }
    let (_, fresh) = session("edit-sample-fresh");
    assert_eq!(s.compiled().records, fresh.compiled().records);
}

fn down(lb: u16, x: f32, y: f32) -> Ray {
    let o = pick::block_origin(lb);
    Ray {
        origin: Vec3::new(o.x + x, o.y + y, 300.0),
        dir: Vec3::new(0.0, 0.0, -1.0),
    }
}

#[test]
fn a_pick_names_the_static_the_building_and_the_ground_vertex_under_it() {
    let (_dir, s) = session("edit-pick");
    let mut models = Models::default();
    let near = [0xA9B4, 0xA8B4];
    // The sample's west stone, placed at (24, 24) on the ground: the first static after
    // Holtburg's own 114.
    let hit = pick::pick(&s, &mut models, &down(0xA9B4, 24.0, 24.0), &near, Mask::ALL).unwrap();
    assert_eq!(
        hit.hit,
        Hit::Static {
            landblock: 0xA9B4,
            index: 114,
            model: DataId(0x0200_040B)
        }
    );
    // The house at (79.5, 37.5).
    let buildings = Mask {
        statics: false,
        spawns: false,
        ground: false,
        buildings: true,
        cell_statics: false,
        cells: false,
    };
    let hit = pick::pick(&s, &mut models, &down(0xA9B4, 79.5, 37.5), &near, buildings).unwrap();
    assert_eq!(
        hit.hit,
        Hit::Building {
            landblock: 0xA9B4,
            index: 8,
            model: DataId(0x0100_0BC3)
        }
    );
    // The ground just off a vertex names it, and the point is on the compiled terrain.
    let ground = Mask::GROUND;
    let hit = pick::pick(&s, &mut models, &down(0xA8B4, 100.0, 50.0), &near, ground).unwrap();
    let Hit::Ground { point, vertex } = hit.hit else {
        panic!("{hit:?}")
    };
    assert_eq!(vertex, (0xA8 * 8 + 4, 0xB4 * 8 + 2));
    let z = s.ground(0xA8B4, 100.0, 50.0).unwrap();
    assert!((point.z - z).abs() < 0.01, "{point:?} {z}");
    assert!((point.x - (f32::from(0xA8u8) * BLOCK + 100.0)).abs() < 0.01);
}

/// A static a world's import places with a model the import adds (as a converted land places its
/// buildings and statics): the session picks it, and the files with the import over them list its
/// model where the world places it.
#[test]
fn a_static_an_import_places_with_its_own_model_is_picked_and_listed() {
    use dereth_assets::world::{LandblockInfo, ObjectEntry};
    use dereth_primitives::{Frame, Quat};
    use lyceum_foundry::compile::{Records, Staged};
    use lyceum_foundry::dats::{files_key, DatSet};
    use lyceum_foundry::models::{Category, ModelNames, Placements};

    let (dir, world) = super::sample::sample("edit-import-static");
    let base = DatSet::open(&dereth_dat::testing::dat_dir()).unwrap();
    // The sample's stone, under an id the files don't hold.
    let (stone, new) = (DataId(0x0200_040B), DataId(0x02FF_F001));
    assert!(!base.has_portal(new));
    let mut setup = base.read_portal(stone).unwrap();
    setup[..4].copy_from_slice(&new.raw().to_le_bytes());
    // Placed over the open sea, where the files place nothing.
    let lb = 0x0101u16;
    let lid = DataId(0x0101_FFFE);
    let info = LandblockInfo {
        id: lid,
        num_cells: 0,
        objects: vec![ObjectEntry {
            id: new,
            frame: Frame::new(Vec3::new(100.0, 100.0, 20.0), Quat::IDENTITY),
        }],
        pack_mask: 0,
        buildings: Vec::new(),
        restrictions: None,
    };
    let mut records = Records::new();
    records.entry(ModernDat::Portal).or_default().insert(
        new.raw(),
        Staged {
            bytes: setup.into(),
            version: 3,
        },
    );
    records.entry(ModernDat::Cell).or_default().insert(
        lid.raw(),
        Staged {
            bytes: info.encode().into(),
            version: 3,
        },
    );
    let overlay = dir.path().join("import");
    compile::write_records(
        &records,
        &std::collections::BTreeMap::new(),
        &base,
        "an-import",
        &overlay,
    )
    .unwrap();
    let text = std::fs::read_to_string(&world).unwrap();
    let imports = format!(
        "imports = [{:?}]\npacks",
        overlay.display().to_string().replace('\\', "/")
    );
    std::fs::write(&world, text.replacen("packs", &imports, 1)).unwrap();

    let s = Session::open(&world, &dereth_dat::testing::dat_dir(), None).unwrap();
    let statics = Mask {
        statics: true,
        buildings: false,
        spawns: false,
        ground: false,
        cell_statics: false,
        cells: false,
    };
    let hit = pick::pick(
        &s,
        &mut Models::default(),
        &down(lb, 100.0, 100.0),
        &[lb],
        statics,
    )
    .expect("the import's static is met");
    assert_eq!(
        hit.hit,
        Hit::Static {
            landblock: lb,
            index: 0,
            model: new
        }
    );

    let under = base.with_imports(&s.world().imports).unwrap();
    assert!(under.has_portal(new));
    assert_ne!(files_key(&under.portal), files_key(&base.portal));
    assert_eq!(
        files_key(&base.portal),
        dereth_dat::overlay::fingerprint(&base.portal)
    );
    let names = ModelNames::new(
        &Placements::read(&under, None, &|_| None),
        &std::collections::BTreeMap::new(),
    );
    assert_eq!(names.get(new).map(|u| u.outdoors), Some(1));
    assert_eq!(names.category(new), Category::BuildingPiece);
    // Two imports are more than the files can hold.
    assert!(base.with_imports(&[overlay.clone(), overlay]).is_err());
}

/// A world on a finer land than its files', built and edited over the retail files.
mod land {
    use std::path::PathBuf;

    use dereth_assets::world::CellLandblock;
    use dereth_assets::Region;
    use dereth_dat::testing::ScratchDir;
    use dereth_dat::ModernDat;
    use dereth_primitives::land_grid::{on_road, LandGrid};
    use dereth_primitives::{DataId, EraId};
    use lyceum_foundry::compile;
    use lyceum_foundry::edit::Session;
    use lyceum_foundry::world::World;

    /// A world made from the sample, on a land `spacing` meters apart.
    fn world(tag: &str, spacing: u32, empty: bool) -> (ScratchDir, PathBuf) {
        dereth_dat::testing::require_dats();
        let dir = ScratchDir::new(tag).unwrap();
        let w = lyceum_foundry::sample::make(&dir.path().join("w"), empty, EraId::Eor, spacing)
            .unwrap();
        (dir, w)
    }

    fn open(world: &std::path::Path) -> Session {
        let mut s = Session::open(world, &dereth_dat::testing::dat_dir(), None).unwrap();
        s.set_today("2026-11-21 00:00:00");
        s
    }

    fn full(s: &Session) -> compile::Compiled {
        let world = World::read(&s.world().path).unwrap();
        let lock = lyceum_foundry::lock::Lock::read(&world.lock_path()).unwrap();
        compile::compile(&world, s.packs(), s.base(), &lock).unwrap()
    }

    /// The files' own landblock `lb` and region.
    fn files_block(s: &Session, lb: u16) -> (CellLandblock, Region) {
        let base = s.base();
        let id = DataId((u32::from(lb) << 16) | 0xFFFF);
        let bytes = base.file(ModernDat::Cell).unwrap().read(id).unwrap();
        let land = base.decode::<CellLandblock>(id, &bytes).unwrap();
        let region = base.read_portal(compile::REGION).unwrap();
        (
            land,
            base.decode::<Region>(compile::REGION, &region).unwrap(),
        )
    }

    /// Landblocks the sample does not edit: Holtburg's north and south neighbors, a road crossing
    /// west of Yaraq, and the open sea.
    const UNEDITED: [u16; 4] = [0xA9B5, 0xA9B3, 0x7D64, 0x0101];

    #[test]
    fn a_finer_land_lays_every_landblock_on_its_grid_with_the_files_corners_and_ground() {
        let (_dir, w) = world("land-fine", 8, false);
        let s = open(&w);
        assert_eq!(s.grid(), lyceum_foundry::land::grid_of_steps(3));
        let region = s.region().unwrap();
        assert_eq!(region.land_defs.land_grid().vertices_per_cell, 3);
        assert_eq!(region.land_defs.road_width, 4.0);
        // Every landblock of the files is in the overlay, on the grid.
        let base_blocks = s
            .base()
            .file(ModernDat::Cell)
            .unwrap()
            .base_entries()
            .filter(|(id, _)| id.raw() & 0xFFFF == 0xFFFF)
            .count();
        let cells = &s.compiled().records[&ModernDat::Cell];
        let fine_blocks = cells.keys().filter(|id| *id & 0xFFFF == 0xFFFF).count();
        assert_eq!(fine_blocks, base_blocks);
        for lb in UNEDITED {
            let (files, files_region) = files_block(&s, lb);
            let fine = s.terrain(lb).unwrap();
            assert_eq!(fine.vertex_steps(), 3, "{lb:04X}");
            assert_eq!(fine.grid_height().len(), 25 * 25, "{lb:04X}");
            // The corners are the files', heights and words alike.
            assert_eq!(
                (fine.height, fine.terrain),
                (files.height, files.terrain),
                "{lb:04X}"
            );
            // The ground keeps its shape to within half a step of the height table (one meter).
            let mut worst = 0.0f32;
            for i in 0..64 {
                for j in 0..64 {
                    #[allow(clippy::cast_precision_loss)]
                    let (x, y) = (1.5 + i as f32 * 2.97, 1.5 + j as f32 * 2.97);
                    let a = compile::ground(&files, &files_region, lb, x, y);
                    let b = s.ground(lb, x, y).unwrap();
                    worst = worst.max((a - b).abs());
                }
            }
            assert!(worst <= 1.0 + 1e-3, "{lb:04X}: the ground moved {worst} m");
            // A road is where the files' road is, to within a vertex step.
            let fine_grid = s.grid();
            let (mut both, mut either) = (0, 0);
            for i in 0..192 {
                for j in 0..192 {
                    #[allow(clippy::cast_precision_loss)]
                    let (x, y) = (i as f32 + 0.5, j as f32 + 0.5);
                    let was = on_road(&files.terrain, 9, 24.0, 5.0, x, y);
                    let is = on_road(
                        fine.grid_terrain(),
                        25,
                        fine_grid.vertex_spacing(),
                        fine_grid.road_width,
                        x,
                        y,
                    );
                    both += usize::from(was && is);
                    either += usize::from(was || is);
                }
            }
            if either > 0 {
                #[allow(clippy::cast_precision_loss)]
                let overlap = both as f32 / either as f32;
                assert!(overlap > 0.6, "{lb:04X}: the roads overlap {overlap}");
            }
        }
        // The sample's hill, three times the vertices, stands where it stood.
        let coarse_dir = ScratchDir::new("land-coarse").unwrap();
        let coarse =
            lyceum_foundry::sample::make(&coarse_dir.path().join("w"), false, EraId::Eor, 24)
                .unwrap();
        let coarse = open(&coarse);
        let (a, b) = (coarse.terrain(0xA9B4).unwrap(), s.terrain(0xA9B4).unwrap());
        assert_eq!(
            b.height, a.height,
            "the corners the sample sets are the same corners"
        );
    }

    #[test]
    fn an_edit_on_a_finer_land_sets_the_vertices_between_the_corners_and_compiles_as_a_build_does()
    {
        let (_dir, w) = world("land-fine-edit", 8, true);
        let mut s = open(&w);
        let fresh = s.compiled().records.clone();
        // Holtburg's south neighbor: (100, 100) is between corners, and the four vertices within
        // 9 m are 8 m apart: (12, 12), (12, 13), (13, 12) and (13, 13).
        let lb: u16 = 0xA9B3;
        let (bx, by) = (u32::from(lb >> 8) * 24, u32::from(lb & 0xFF) * 24);
        let before: Vec<u8> = [(12, 12), (12, 13), (13, 12), (13, 13), (14, 12)]
            .iter()
            .map(|(x, y)| s.vertex(bx + x, by + y).unwrap().height)
            .collect();
        let edits = r#"
    [[edit]]
    do = "brush"
    brush = "raise"
    amount = 3
    landblock = "A9B3"
    at = [100.0, 100.0]
    radius = 9.0

    [[edit]]
    do = "brush"
    brush = "raise"
    amount = 2
    landblock = "A9B3"
    at = [0.0, 96.0]
    radius = 1.0
    "#;
        for a in lyceum_foundry::script::actions(edits, "w").unwrap() {
            s.act(&a).unwrap();
            assert_eq!(s.compiled().records, full(&s).records);
        }
        let after: Vec<u8> = [(12, 12), (12, 13), (13, 12), (13, 13), (14, 12)]
            .iter()
            .map(|(x, y)| s.vertex(bx + x, by + y).unwrap().height)
            .collect();
        assert_eq!(
            &after[..4],
            &before[..4].iter().map(|h| h + 3).collect::<Vec<_>>()[..]
        );
        assert_eq!(after[4], before[4], "(14, 12) is 10 m away");
        // The pack says it vertex by vertex, in the block's own 0 to 24.
        s.save().unwrap();
        let file = s.packs()[0].dir.join("edits/landblocks/A9B3.toml");
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("at = [13, 12]"), "{text}");
        // On the block's west edge, both copies: x = 0 here, x = 24 in the block west of it.
        let (here, west) = (s.terrain(lb).unwrap(), s.terrain(0xA8B3).unwrap());
        let (h, _) = lyceum_foundry::land::vertex(&here, 0, 12).unwrap();
        let (w, _) = lyceum_foundry::land::vertex(&west, 24, 12).unwrap();
        assert_eq!(h, w);
        s.undo().unwrap();
        s.undo().unwrap();
        assert_eq!(s.compiled().records, fresh);
    }

    #[test]
    fn a_world_over_an_import_on_a_finer_land_takes_its_grid_from_the_import_s_region() {
        // A finer world's overlay, made the way a world builder's import is laid: under the packs.
        let (dir, fine) = world("land-import-source", 8, true);
        compile::build(&fine, &dereth_dat::testing::dat_dir(), None, false).unwrap();
        let overlay = dir.path().join("w/build/overlay");
        let (_d2, w) = world("land-import", 24, true);
        let text = std::fs::read_to_string(&w).unwrap();
        let imports = format!(
            "imports = [{:?}]\npacks",
            overlay.display().to_string().replace('\\', "/")
        );
        std::fs::write(&w, text.replacen("packs", &imports, 1)).unwrap();
        let mut s = open(&w);
        assert_eq!(
            s.grid(),
            lyceum_foundry::land::grid_of_steps(3),
            "the import's region says 3 steps"
        );
        let lb: u16 = 0xA9B3;
        let (bx, by) = (u32::from(lb >> 8) * 24, u32::from(lb & 0xFF) * 24);
        let before = s.vertex(bx + 13, by + 12).unwrap().height;
        let edit = r#"
    [[edit]]
    do = "brush"
    brush = "lower"
    amount = 1
    landblock = "A9B3"
    at = [104.0, 96.0]
    radius = 1.0
    "#;
        for a in lyceum_foundry::script::actions(edit, "w").unwrap() {
            s.act(&a).unwrap();
        }
        assert_eq!(s.vertex(bx + 13, by + 12).unwrap().height, before - 1);
        assert_eq!(s.compiled().records, full(&s).records);
        // A spacing other than the import's own is refused.
        let text = std::fs::read_to_string(&w).unwrap();
        std::fs::write(&w, format!("{text}\n[land]\nspacing = 12\n")).unwrap();
        let e = Session::open(&w, &dereth_dat::testing::dat_dir(), None)
            .unwrap_err()
            .to_string();
        assert!(e.contains("its land spacing is theirs"), "{e}");
    }

    #[test]
    fn a_world_on_its_files_land_lays_no_land_and_saying_24_meters_is_the_same() {
        let (dir, w) = world("land-files", 24, true);
        let s = open(&w);
        assert!(s.grid().is_default());
        assert!(
            s.compiled().records.is_empty(),
            "an empty world changes nothing"
        );
        let text = std::fs::read_to_string(&w).unwrap();
        std::fs::write(&w, format!("{text}\n[land]\nspacing = 24\n")).unwrap();
        let again = open(&w);
        assert_eq!(World::read(&w).unwrap().land_steps, Some(1));
        assert!(again.grid().is_default());
        assert!(again.compiled().records.is_empty());
        // A vertex past 8 is off a landblock of the files' land.
        let edits = dir.path().join("w/w/edits/landblocks/A9B3.toml");
        std::fs::create_dir_all(edits.parent().unwrap()).unwrap();
        std::fs::write(&edits, "[[vertex]]\nat = [9, 0]\nheight = 3\n").unwrap();
        let e = Session::open(&w, &dereth_dat::testing::dat_dir(), None)
            .unwrap_err()
            .to_string();
        assert!(e.contains("each 0 to 8"), "{e}");
        let _ = LandGrid::DEFAULT;
    }
}

/// A starter area written into the pack's records by the table tabs' edit is compiled into the
/// character-creation table as a full build compiles it, a second change of it too, and undone
/// the table is the sample's again; a records file holding anything but the tables' rows is
/// refused.
#[test]
fn a_starter_areas_row_written_in_the_session_compiles_as_a_build_does_and_undoes() {
    use dereth_assets::tables::CharGen;
    use dereth_assets::Decode;
    use lyceum_foundry::edit::Op;
    use lyceum_foundry::starter_areas::{self, FILE, KIND};
    use lyceum_foundry::table_edit;
    use lyceum_foundry::tables::CHARGEN;

    let (_dir, mut s) = session("edit-starter-area");
    let file = s.packs()[s.target()].dir.join("records").join(FILE);
    let areas = |s: &Session| {
        let bytes = s
            .compiled()
            .records
            .get(&ModernDat::Portal)
            .and_then(|r| r.get(&CHARGEN))
            .map(|r| r.bytes.to_vec())
            .unwrap_or_else(|| s.base().read_portal(DataId(CHARGEN)).unwrap());
        starter_areas::areas(&CharGen::decode_payload(DataId(CHARGEN), &bytes).unwrap())
    };
    let before = areas(&s);
    let key = [("name", toml::Value::String("Holtburg Gate".into()))];
    let place = starter_areas::place_row(0xA9B4_0025, 100.0, 100.0, None, 90.0);
    let text = table_edit::set_fields(
        None,
        KIND,
        &key,
        &[("at", Some(toml::Value::Array(vec![place])))],
    )
    .unwrap();
    let done = s
        .apply(&Op::WriteTables {
            file: file.clone(),
            text: text.clone(),
        })
        .unwrap();
    assert!(done.touched.tables);
    assert_eq!(s.compiled().records, full(&s).records);
    let gate = areas(&s).pop().unwrap();
    assert_eq!(
        (gate.name.as_str(), gate.places.len()),
        ("Holtburg Gate", 1)
    );
    // Named no heritages, a new area is every heritage's.
    let every = starter_areas::heritages(
        &CharGen::decode_payload(
            DataId(CHARGEN),
            &s.base().read_portal(DataId(CHARGEN)).unwrap(),
        )
        .unwrap(),
    );
    assert_eq!(gate.heritages, every);

    // Offered to one heritage only.
    let text = table_edit::set_fields(
        text.as_deref(),
        KIND,
        &key,
        &[(
            "heritages",
            Some(toml::Value::Array(vec!["Aluvian".into()])),
        )],
    )
    .unwrap();
    s.apply(&Op::WriteTables {
        file: file.clone(),
        text,
    })
    .unwrap();
    assert_eq!(s.compiled().records, full(&s).records);
    assert_eq!(areas(&s).pop().unwrap().heritages, ["Aluvian"]);

    // A records file of other rows is no table file.
    assert!(s
        .apply(&Op::WriteTables {
            file: file.clone(),
            text: Some("[[surface-texture]]\nname = \"stone\"\nlevels = [0x06000001]\n".into()),
        })
        .is_err());

    s.undo().unwrap();
    s.undo().unwrap();
    assert_eq!(areas(&s), before);
    let (_, fresh) = session("edit-starter-area-fresh");
    assert_eq!(s.compiled().records, fresh.compiled().records);
}

/// After a table's rows change, the checks run again over what the change wrote find what a
/// session opened afresh on the saved world finds, and again once the change is undone.
#[test]
fn a_table_edits_check_finds_what_a_fresh_sessions_check_finds() {
    use lyceum_foundry::edit::Op;
    use lyceum_foundry::scratch::Scratch;
    use lyceum_foundry::starter_areas::{self, FILE, KIND};
    use lyceum_foundry::table_edit;

    let (dir, mut s) = session("check-table-edit");
    let found = |s: &Session, d: &std::path::Path| {
        let mut v: Vec<(String, bool)> = s
            .check(d)
            .unwrap()
            .into_iter()
            .map(|f| (f.text, f.warning))
            .collect();
        v.sort();
        v
    };
    let fresh = |s: &mut Session, n: &str| {
        s.save().unwrap();
        let mut again =
            Session::open(&s.world().path, &dereth_dat::testing::dat_dir(), None).unwrap();
        again.set_today("2026-10-18 00:00:00");
        let (scratch, _) = Scratch::create(&dir.path().join(n), &again).unwrap();
        found(&again, scratch.dir())
    };
    let (mut scratch, _) = Scratch::create(&dir.path().join("scratch"), &s).unwrap();
    let clean = found(&s, scratch.dir());
    let file = s.packs()[s.target()].dir.join("records").join(FILE);
    let place = starter_areas::place_row(0xA9B4_0025, 100.0, 100.0, None, 90.0);
    let text = table_edit::set_fields(
        None,
        KIND,
        &[("name", toml::Value::String("Holtburg Gate".into()))],
        &[("at", Some(toml::Value::Array(vec![place])))],
    )
    .unwrap();
    let applied = s.apply(&Op::WriteTables { file, text }).unwrap();
    assert!(applied.touched.tables);
    scratch.update(&s, &applied.touched.landblocks).unwrap();
    assert_eq!(found(&s, scratch.dir()), fresh(&mut s, "fresh-1"));
    let touched = s.undo().unwrap();
    scratch.update(&s, &touched.landblocks).unwrap();
    assert_eq!(found(&s, scratch.dir()), fresh(&mut s, "fresh-2"));
    assert_eq!(found(&s, scratch.dir()), clean);
}

/// The rows the Tables tab writes compile as a full build compiles them: a skill's cost and
/// formula, a heritage's credits and a skill it now offers (both its costs), and a higher level
/// cap whose added levels continue the curve's last step and never fall.
#[test]
fn the_table_tabs_rows_compile_as_a_build_does() {
    use dereth_assets::tables::{CharGen, SkillTable, XpTable};
    use dereth_assets::Decode;
    use lyceum_foundry::edit::Op;
    use lyceum_foundry::table_edit::set_fields;
    use lyceum_foundry::table_views::{self as v, EXPERIENCE_FILE, HERITAGES_FILE, SKILLS_FILE};
    use lyceum_foundry::tables::{CHARGEN, EXPERIENCE, SKILLS};
    use toml::Value as T;

    let (_dir, mut s) = session("table-tab-rows");
    let read = |s: &Session, id: u32| {
        s.compiled()
            .records
            .get(&ModernDat::Portal)
            .and_then(|r| r.get(&id))
            .map(|r| r.bytes.to_vec())
            .unwrap_or_else(|| s.base().read_portal(DataId(id)).unwrap())
    };
    let skills = SkillTable::decode_payload(DataId(SKILLS), &read(&s, SKILLS)).unwrap();
    assert_eq!(skills.skills[&6].name, "Melee Defense");
    let xp = XpTable::decode_payload(DataId(EXPERIENCE), &read(&s, EXPERIENCE)).unwrap();
    let levels: Vec<u64> = v::curve(&xp).iter().map(|(x, _)| *x).collect();
    let top = levels.len() - 1;
    let dir = s.packs()[s.target()].dir.join("records");
    let s_name = || T::String("Melee Defense".into());

    let skills_text = set_fields(
        None,
        "skill",
        &v::skill_key(None, 6, "Melee Defense"),
        &[
            ("trained_cost", Some(T::Integer(5))),
            (
                "formula",
                Some(v::formula_row(
                    &["Coordination", "Quickness"],
                    &[1, 1],
                    0,
                    3,
                )),
            ),
        ],
    )
    .unwrap();
    let mut cg = set_fields(
        None,
        "heritage",
        &[("heritage", T::String("Aluvian".into()))],
        &[("skill_credits", Some(T::Integer(52)))],
    )
    .unwrap();
    cg = set_fields(
        cg.as_deref(),
        "heritage-skill",
        &[
            ("heritage", T::String("Aluvian".into())),
            ("skill", s_name()),
        ],
        &[
            ("trained_cost", Some(T::Integer(7))),
            ("specialized_cost", Some(T::Integer(12))),
        ],
    )
    .unwrap();
    let mut xp_text = set_fields(
        None,
        "experience",
        &[],
        &[(
            "max_level",
            Some(T::Integer(i64::try_from(top + 2).unwrap())),
        )],
    )
    .unwrap();
    let added = v::raised(&levels, top + 2);
    assert_eq!(added.len(), 2);
    for (n, x) in &added {
        xp_text = set_fields(
            xp_text.as_deref(),
            "level",
            &[("level", T::Integer(i64::try_from(*n).unwrap()))],
            &[("xp", Some(T::Integer(i64::try_from(*x).unwrap())))],
        )
        .unwrap();
    }
    s.apply(&Op::Batch(vec![
        Op::WriteTables {
            file: dir.join(SKILLS_FILE),
            text: skills_text,
        },
        Op::WriteTables {
            file: dir.join(HERITAGES_FILE),
            text: cg,
        },
        Op::WriteTables {
            file: dir.join(EXPERIENCE_FILE),
            text: xp_text,
        },
    ]))
    .unwrap();
    assert_eq!(s.compiled().records, full(&s).records);

    let skills = SkillTable::decode_payload(DataId(SKILLS), &read(&s, SKILLS)).unwrap();
    let md = &skills.skills[&6];
    assert_eq!(md.trained_cost, 5);
    assert_eq!(
        v::formula_words(&md.formula),
        "(Coordination + Quickness) / 3"
    );
    let cg = CharGen::decode_payload(DataId(CHARGEN), &read(&s, CHARGEN)).unwrap();
    let aluvian = v::heritages(&cg, &skills)
        .into_iter()
        .find(|h| h.name == "Aluvian")
        .unwrap();
    assert_eq!(aluvian.skill_credits, 52);
    assert!(
        aluvian.skills.contains(&("Melee Defense".into(), 7, 12)),
        "{:?}",
        aluvian.skills
    );
    let xp = XpTable::decode_payload(DataId(EXPERIENCE), &read(&s, EXPERIENCE)).unwrap();
    let now: Vec<u64> = v::curve(&xp).iter().map(|(x, _)| *x).collect();
    assert_eq!(now.len(), top + 3);
    assert!(
        now.windows(2).skip(1).all(|w| w[1] >= w[0]),
        "the curve never falls"
    );
    assert_eq!(
        now[top + 1] - now[top],
        now[top] - now[top - 1],
        "the last step continued"
    );
}

/// The creature types the Tables tab writes compile as a full build compiles them: a type added
/// past the highest, one renamed, one taken out; taking the pack's removal away again brings the
/// type back as the files have it.
#[test]
fn the_table_tabs_creature_types_compile_as_a_build_does() {
    use dereth_assets::tables::EnumMapper;
    use dereth_assets::Decode;
    use lyceum_foundry::edit::Op;
    use lyceum_foundry::table_edit::set_fields;
    use lyceum_foundry::table_views::{self as v, CREATURE_TYPES_FILE};
    use lyceum_foundry::tables::CREATURE_TYPES;
    use toml::Value as T;

    let (_dir, mut s) = session("table-tab-creature-types");
    let read = |s: &Session| {
        let bytes = s
            .compiled()
            .records
            .get(&ModernDat::Portal)
            .and_then(|r| r.get(&CREATURE_TYPES))
            .map(|r| r.bytes.to_vec())
            .unwrap_or_else(|| s.base().read_portal(DataId(CREATURE_TYPES)).unwrap());
        EnumMapper::decode_payload(DataId(CREATURE_TYPES), &bytes).unwrap()
    };
    let was = read(&s);
    let types = v::creature_types(&was);
    let (renamed, old_name) = types[1].clone();
    let (gone, gone_name) = types[2].clone();
    let new = v::free_creature_type(&was);
    let file = s.packs()[s.target()]
        .dir
        .join("records")
        .join(CREATURE_TYPES_FILE);
    let id = |n: u32| vec![("id", T::Integer(i64::from(n)))];
    let mut text = set_fields(
        None,
        "creature-type",
        &id(new),
        &[("name", Some(T::String("Tusker Kin".into())))],
    )
    .unwrap();
    text = set_fields(
        text.as_deref(),
        "creature-type",
        &v::creature_type_key(text.as_deref(), renamed, &old_name),
        &[("name", Some(T::String("Old Ones".into())))],
    )
    .unwrap();
    text = set_fields(
        text.as_deref(),
        "creature-type",
        &id(gone),
        &[("remove", Some(T::Boolean(true)))],
    )
    .unwrap();
    s.apply(&Op::WriteTables {
        file: file.clone(),
        text: text.clone(),
    })
    .unwrap();
    assert_eq!(s.compiled().records, full(&s).records);
    let now = v::creature_types(&read(&s));
    assert!(now.contains(&(new, "Tusker Kin".into())), "{now:?}");
    assert!(now.contains(&(renamed, "Old Ones".into())));
    assert!(!now.iter().any(|(k, _)| *k == gone));
    assert_eq!(now.len(), types.len());
    assert_eq!(v::removed_creature_types(text.as_deref()), [gone]);

    let back = set_fields(
        text.as_deref(),
        "creature-type",
        &id(gone),
        &[("remove", None)],
    )
    .unwrap();
    s.apply(&Op::WriteTables { file, text: back }).unwrap();
    assert!(v::creature_types(&read(&s)).contains(&(gone, gone_name)));
}
