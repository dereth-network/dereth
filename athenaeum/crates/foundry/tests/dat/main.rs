//! Behaviour: none (checks the world builder end to end on the world database and the end-of-retail install: a world over the database, edited, saved and compiled into the server's tables)
//!
//! The install is the end-of-retail data files the tests read everywhere; the database is read from
//! `YALAIN_TEST_WORLD_DB` (a checkout), else from the release this build pins in the per-user cache
//! (`yalain world fetch`); without either, a test fails saying so.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use athenaeum_foundry::edit::{NewKind, NewRow};
use athenaeum_foundry::{build, new, overlay, world, Layer, Session};
use auberean_assets::landblock::Landblock;
use auberean_assets::Context;
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::overlay::WorldOverlay;
use auberean_dat::testing::{open_wrong_once, require_install, TARGET};
use auberean_dat::DatFile;
use auberean_world::records::{RegionSet, WorldRecords};
use auberean_world::scenery::{block_scenery, SceneryInput, SceneryObject};
use auberean_world::terrain::build_block;
use dereth_primitives::num::math;
use yalain_content::{ContentObject, Kind};
use yalain_world_db::compile::{outdoor_cell, Tables};
use yalain_world_db::release::{pinned_in_cache, PINNED_TAG};

/// The variable naming a world database checkout for the tests.
const WORLD_DB_VAR: &str = "YALAIN_TEST_WORLD_DB";

fn database() -> PathBuf {
    if let Some(v) = std::env::var_os(WORLD_DB_VAR).filter(|v| !v.is_empty()) {
        return PathBuf::from(v);
    }
    pinned_in_cache().unwrap_or_else(|why| {
        panic!(
            "no world database: {why}. Set {WORLD_DB_VAR} to a checkout, or fetch the pinned release ({PINNED_TAG}) with `yalain world fetch`"
        )
    })
}

/// A folder of the test's own, empty.
fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("athenaeum-dat-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn compiled(dir: &Path, base: &Path) -> Tables {
    let opened = world::open(dir, Some(base)).expect("the world opens over the database");
    let files = build::InstallFiles::open(&require_install(TARGET)).expect("the install");
    let install = files.install().expect("the install's script library");
    build::compile_world(&opened, &install).expect("the world compiles")
}

fn at(t: &Tables, template: u32, cell: u32) -> Vec<&ContentObject> {
    t.content
        .objects
        .iter()
        .filter(|o| o.template == template && o.cell == cell)
        .collect()
}

/// A storekeeper of Arwic's, placed in a new world over the database, saved and compiled, is a
/// person of the server's content where it was placed, and a person of the database moved in the
/// world stands where it was moved to and no longer where it stood.
#[test]
fn a_person_placed_and_one_moved_in_a_world_are_where_the_server_puts_them() {
    let base = database();
    let root = scratch("place");
    let dir = root.join("arwic-plus");
    new::new_world(&dir, None).unwrap();
    let opened = world::open(&dir, Some(&base)).unwrap();
    let base_objects = {
        let mut s = Session::open(opened.clone(), None).unwrap();
        assert!(s.problems().is_empty(), "{:?}", s.problems());
        // An Arwic person of the database, moved a few meters.
        let moved = "object n-334c@77B8";
        let was = s
            .entry(moved)
            .expect("the database places this person")
            .clone();
        assert_eq!(was.layer, Layer::Base);
        s.move_to(moved, 0x77B8, [3.0, 104.0, 182.5]).unwrap();
        // A weapon vendor placed beside them.
        let placed = s
            .place(&NewRow {
                kind: NewKind::Person,
                template: 0x4700_33D0,
                landblock: 0x77B8,
                at: [7.0, 105.0, 182.5],
                heading: 180.0,
            })
            .unwrap();
        assert_eq!(placed, "object person-1@77B8");
        assert!(s.problems().is_empty(), "{:?}", s.problems());
        s.save().unwrap();
        opened.base.merged.rows.len()
    };
    let t = compiled(&dir, &base);
    let vendor = at(&t, 0x4700_33D0, outdoor_cell(0x77B8, 7.0, 105.0));
    let vendor = vendor
        .iter()
        .find(|o| o.origin == [7.0, 105.0, 182.5])
        .expect("the vendor is in the server's content");
    assert_eq!(vendor.kind, Kind::Npc);
    assert!(vendor.placed.starts_with("authored"), "{}", vendor.placed);
    assert_ne!(vendor.description.look, 0, "described from its template");
    let orven: Vec<&ContentObject> = t
        .content
        .objects
        .iter()
        .filter(|o| o.template == 0x4700_334C)
        .collect();
    assert!(orven.iter().any(|o| o.origin == [3.0, 104.0, 182.5]));
    assert!(
        !orven
            .iter()
            .any(|o| (o.origin[0] - 1.3796).abs() < 1e-3 && (o.origin[1] - 100.7791).abs() < 1e-3),
        "no longer where the database put them"
    );
    // The database's own content, and one more.
    let plain = {
        let empty = root.join("empty");
        new::new_world(&empty, None).unwrap();
        compiled(&empty, &base)
    };
    assert_eq!(t.content.objects.len(), plain.content.objects.len() + 1);
    assert!(base_objects > 0);
    std::fs::remove_dir_all(&root).ok();
}

/// A script library read wrong once, over a file that did not change, is read again and a compile
/// reads the install with it.
/// Behaviour: data.read.script-library-reads-are-checked
#[test]
fn a_script_library_read_wrong_once_is_read_again_for_a_compile() {
    let dir = require_install(TARGET);
    let found = install_dats(&dir).expect("the install");
    let (_, p) = found
        .iter()
        .find(|(n, _)| *n == DatName::Portal)
        .expect("a portal file");
    let portal = open_wrong_once(p, auberean_script::library::RECORD);
    let files = build::InstallFiles::of(portal, None).expect("the install");
    files.install().expect("the second read decodes");
}

/// A world's rows, in a pack file of their own.
fn with_rows(dir: &Path, file: &str, rows: &str) {
    let pack = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .find(|e| e.path().join("pack.toml").is_file())
        .expect("the world's pack")
        .path();
    let path = pack.join("server").join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, rows).unwrap();
}

/// A landblock's scenery, drawn through `vertices`, frills apart.
fn grown(
    w: &WorldRecords,
    region: &RegionSet,
    land: &HashMap<(i32, i32), Landblock>,
    (x, y): (i32, i32),
    vertices: &[u32],
) -> Vec<SceneryObject> {
    let tb = build_block(&region.land, region.surfaces(), land, x, y).expect("the block");
    let input = SceneryInput {
        land: &region.land,
        terrain: &region.terrain,
        scenes: region.scenes.as_ref().expect("the scenes"),
        vertices,
        block: &tb,
        obstacles: &[],
        degrade: 1.0,
        water: None,
    };
    block_scenery(&input, w)
}

/// A clearing takes everything the land grows out of its shape, frills too, in its own block and the
/// next one over, through the folder the build writes; it leaves the land's height, what covers it
/// and every other record as the install has them. A rebuild makes the same bytes, and a world with
/// no clearing makes no overlay.
#[test]
fn a_clearing_takes_what_the_land_grows_out_of_its_shape() {
    let base = database();
    let root = scratch("clearing");
    let dir = root.join("cleared");
    new::new_world(&dir, None).unwrap();
    let dats = require_install(TARGET);
    let out = root.join("built");
    let opened = world::open(&dir, Some(&base)).unwrap();

    // No clearing, no overlay.
    let (folder, built) = overlay::write(&opened, &dats, &out).unwrap();
    assert_eq!(built, overlay::Built::default());
    assert!(
        WorldOverlay::open(&folder).is_err(),
        "a folder with nothing in it"
    );

    // The install as it stands, and two blocks that grow plenty about the places below.
    let open = |n: DatName| DatFile::open(&n.in_dir(&dats)).unwrap();
    let portal = open(DatName::Portal);
    let ctx = Context::from_portal(&portal).unwrap();
    let plain = WorldRecords::new(portal, open(DatName::Cell(1)), None, ctx.clone());
    let region = plain.region().unwrap();
    let land: HashMap<(i32, i32), Landblock> = plain
        .landblock_coordinates()
        .into_iter()
        .map(|(x, y)| ((x, y), plain.landblock(x, y).unwrap()))
        .collect();
    let in_circle = |o: &SceneryObject| math::hypotf(o.origin.x - 80.0, o.origin.y - 80.0) <= 20.0;
    // A rectangle 30 long and 20 wide, turned a quarter, across the block's east edge: x within 10 of
    // 155, y within 15 of 80.
    let in_rectangle = |x: f32, o: &SceneryObject| {
        (o.origin.x + x - 155.0).abs() <= 10.0 && (o.origin.y - 80.0).abs() <= 15.0
    };
    let mut keys: Vec<_> = land.keys().copied().collect();
    keys.sort_unstable();
    let plenty = |(x, y): (i32, i32)| {
        let g = grown(&plain, &region, &land, (x, y), &land[&(x, y)].vertices);
        let east = land.contains_key(&(x + 1, y)).then(|| {
            grown(
                &plain,
                &region,
                &land,
                (x + 1, y),
                &land[&(x + 1, y)].vertices,
            )
        });
        g.iter().filter(|o| in_circle(o)).count() >= 10
            && g.iter().filter(|o| in_rectangle(0.0, o)).count() >= 3
            && east.is_some_and(|e| e.iter().filter(|o| in_rectangle(160.0, o)).count() >= 1)
    };
    let (circle, rectangle) = {
        let mut found = keys.iter().step_by(13).copied().filter(|k| plenty(*k));
        (
            found.next().expect("a block"),
            found.next().expect("a block"),
        )
    };
    let name = |(x, y): (i32, i32)| format!("{x:02X}{y:02X}");
    with_rows(
        &dir,
        &format!("landblocks/{}.toml", name(circle)),
        "[[clearing]]\nkey = \"yard\"\nat = [80.0, 80.0, \"ground\"]\nradius = 20.0\nprov = { from = \"authored\" }\n",
    );
    with_rows(
        &dir,
        &format!("landblocks/{}.toml", name(rectangle)),
        "[[clearing]]\nkey = \"lane\"\nat = [155.0, 80.0, \"ground\"]\nsize = [30.0, 20.0]\nheading = 90.0\nprov = { from = \"authored\" }\n",
    );
    let opened = world::open(&dir, Some(&base)).unwrap();
    assert!(opened.check().is_empty(), "{:?}", opened.check());
    let (folder, built) = overlay::write(&opened, &dats, &out).unwrap();
    assert!(built.blocks >= 3 && built.vertices > 10, "{built:?}");

    // Read through the folder as a client reads it.
    let w = WorldOverlay::open(&folder).unwrap();
    assert_eq!(w.world(), "cleared");
    let cell = w.over(open(DatName::Cell(1))).unwrap();
    let portal = w.over(open(DatName::Portal)).unwrap();
    let through = WorldRecords::new(portal, cell.clone(), None, ctx);
    let read = |(x, y): (i32, i32)| through.landblock(x, y).unwrap();

    let after = read(circle);
    let g = grown(&plain, &region, &land, circle, &after.vertices);
    assert!(!g.iter().any(in_circle), "{g:?}");
    assert!(!g.is_empty(), "the rest of the block still grows");

    let after = read(rectangle);
    let g = grown(&plain, &region, &land, rectangle, &after.vertices);
    assert!(!g.iter().any(|o| in_rectangle(0.0, o)), "{g:?}");
    let east = (rectangle.0 + 1, rectangle.1);
    let after_east = read(east);
    let g = grown(&plain, &region, &land, east, &after_east.vertices);
    assert!(!g.iter().any(|o| in_rectangle(160.0, o)), "{g:?}");

    // Only the scene field of a vertex changed, and only blocks a shape reaches.
    let scene = region.land.word.scene;
    for block in [circle, rectangle, east] {
        let (old, new) = (&land[&block], read(block));
        assert_eq!(old.heights, new.heights);
        assert_eq!(old.info, new.info);
        for (a, b) in old.vertices.iter().zip(&new.vertices) {
            assert_eq!(scene.with(*a, 0), scene.with(*b, 0));
        }
    }
    let far = keys
        .iter()
        .copied()
        .find(|(x, y)| {
            (x - circle.0).abs() > 2 && (x - rectangle.0).abs() > 2 && (y - circle.1).abs() > 2
        })
        .expect("a block far away");
    assert_eq!(land[&far], read(far));
    let layer = cell.shared().layer().expect("the overlay");
    assert_eq!(layer.len() - cell.base().len(), 0, "it adds no record");
    assert_eq!(layer.records().count(), built.blocks);

    // The same rows make the same bytes, over what was there.
    let bytes = |d: &Path| std::fs::read(d.join("overlay_cell.dat")).unwrap();
    let first = bytes(&folder);
    let (again, rebuilt) = overlay::write(&opened, &dats, &out).unwrap();
    assert_eq!(rebuilt, built);
    assert_eq!(bytes(&again), first);
}

/// Built again, a world's overlay is the revision after its last: a clearing changed goes into the
/// next revision, the same rows change nothing, and a clearing taken away gives the land back. The
/// folder is replaced whole under a reader that holds it open, as a running server does, which
/// keeps what it read until it opens the folder again.
#[test]
fn a_world_built_again_is_the_next_revision_even_with_a_server_reading_the_folder() {
    let root = scratch("revisions");
    let dir = root.join("cleared");
    let base = database();
    new::new_world(&dir, None).unwrap();
    let dats = require_install(TARGET);
    let row = |radius: f32| {
        format!("[[clearing]]\nkey = \"yard\"\nat = [75.0, 45.0, \"ground\"]\nradius = {radius:.1}\nprov = {{ from = \"authored\" }}\n")
    };
    let cell_file = DatFile::open(&DatName::Cell(1).in_dir(&dats)).unwrap();
    let block = 0xB590_FFFF;
    let in_block = format!("landblocks/{:04X}.toml", block >> 16);

    with_rows(&dir, &in_block, &row(25.0));
    let opened = world::open(&dir, Some(&base)).unwrap();
    let (folder, _) = overlay::write(&opened, &dats, &dir).unwrap();
    assert_eq!(folder, dir.join(overlay::FOLDER));
    let over = |f: &Path| {
        WorldOverlay::open(f)
            .unwrap()
            .over(DatFile::open(&DatName::Cell(1).in_dir(&dats)).unwrap())
            .unwrap()
    };
    let held = over(&folder);
    let small = held.read(block).unwrap();
    assert_ne!(small, cell_file.read(block).unwrap());
    let first = held.shared().layer().unwrap().own_iterations().to_vec();
    assert_eq!(first.len(), 1, "{first:?}");

    // The same rows leave the file as it is.
    let path = folder.join("overlay_cell.dat");
    let bytes = std::fs::read(&path).unwrap();
    overlay::write(&opened, &dats, &dir).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);

    // A bigger clearing: the next revision, with the file still held open here.
    with_rows(&dir, &in_block, &row(40.0));
    let opened = world::open(&dir, Some(&base)).unwrap();
    overlay::write(&opened, &dats, &dir).unwrap();
    assert_eq!(held.read(block).unwrap(), small, "what it has, it keeps");
    let next = over(&folder);
    assert_ne!(next.read(block).unwrap(), small);
    let revisions = next.shared().layer().unwrap().own_iterations().to_vec();
    assert_eq!(revisions, [first[0], first[0] + 1]);

    // Taken away: the install's record again, in a revision of its own for who held the last.
    let file = dir.join("fill-cleared/server").join(&in_block);
    std::fs::remove_file(file).unwrap();
    let opened = world::open(&dir, Some(&base)).unwrap();
    overlay::write(&opened, &dats, &dir).unwrap();
    let last = over(&folder);
    assert_eq!(last.read(block).unwrap(), cell_file.read(block).unwrap());
    assert_eq!(last.shared().layer().unwrap().own_iterations().len(), 3);

    drop((held, next, last));
    std::fs::remove_dir_all(&root).ok();
}
