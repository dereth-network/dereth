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
            .map(|r| r.bytes.clone())
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
