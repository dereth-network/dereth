//! Behaviour: none (tooling: the world editor on an Infiltration world, over the February 2005 files)
//! The editor opened on a world built on the February 2005 files (before Throne of Destiny): the
//! world is drawn from them, and each tool (a static placed, the ground raised, an NPC spawned, a
//! building of those files dropped, a dungeon of their pieces built) writes records in their
//! layout that the checks pass, and the world is drawn again with them.
//! Fixture: the end-of-retail files in `$DERETH_TEST_DAT_DIR` beside the February 2005 files in
//! `$DERETH_TEST_PRETOD_DAT_DIR`, and a device; run with `--test-threads=1`.

use std::path::{Path, PathBuf};

use dereth_dat::testing::ScratchDir;
use dereth_primitives::{DataId, EraId, Vec3};
use lyceum_editor::app::EditorApp;
use lyceum_foundry::brush::Brush;
use lyceum_foundry::pick;
use lyceum_foundry::script::Action as Edit;
use lyceum_foundry::source::{Furnished, Place, Rotation};

use super::{capture, gpu_lock, sample, shots, HOLTBURG};

const ERA: EraId = EraId::Infiltration;

/// The sample world, built on Infiltration's era as `lyceum new --era infiltration` makes it.
fn infiltration(tag: &str) -> (ScratchDir, PathBuf) {
    let (dir, world) = sample(tag);
    let text = std::fs::read_to_string(&world).unwrap();
    std::fs::write(&world, format!("era = \"infiltration\"\n{text}")).unwrap();
    for (rel, bytes) in lyceum_foundry::sample::FILES {
        let path = dir.path().join(rel);
        if path.is_file() {
            std::fs::write(&path, lyceum_foundry::sample::file_for(ERA, rel, bytes)).unwrap();
        }
    }
    (dir, world)
}

/// The editor on `world`, the February 2005 files beside the player's later ones.
fn editor(world: &Path, more: &[&str]) -> EditorApp {
    let classic = dereth_dat::testing::classic_dat_dir().unwrap_or_else(|| {
        panic!(
            "{}",
            dereth_dat::testing::classic_shortfall().unwrap_or_default()
        )
    });
    let cache = Path::new(env!("CARGO_TARGET_TMPDIR")).join("editor-cache");
    let argv: Vec<String> = [
        world.to_str().unwrap(),
        "--dats",
        dereth_dat::testing::dat_dir().to_str().unwrap(),
        "--classic-dats",
        classic.to_str().unwrap(),
        "--headless",
        "--size",
        "1280x900",
        "--landblock",
        "A9B4",
        "--no-weenies",
        "--thumbnails",
        cache.to_str().unwrap(),
    ]
    .iter()
    .chain(more)
    .map(|s| (*s).to_owned())
    .collect();
    let args = lyceum_editor::args::parse(&argv).unwrap();
    assert_eq!(args.world_era(), ERA);
    let cfg = lyceum_editor::args::runtime_config(&args).unwrap();
    EditorApp::open(&args, cfg).unwrap()
}

fn keep(ed: &mut EditorApp, name: &str) {
    ed.capture(&shots("classic").join(name)).unwrap();
}

/// How much `img` varies: the spread of its pixels' brightness.
fn spread(img: &(usize, usize, Vec<u8>)) -> f32 {
    let b: Vec<f32> = img
        .2
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| f32::from(p[0]) + f32::from(p[1]) + f32::from(p[2]))
        .collect();
    #[allow(clippy::cast_precision_loss)]
    let n = b.len() as f32;
    let mean = b.iter().sum::<f32>() / n;
    (b.iter().map(|x| (x - mean) * (x - mean)).sum::<f32>() / n).sqrt()
}

/// The mean difference of two frames' pixels within `r` pixels of `at`.
fn difference(
    a: &(usize, usize, Vec<u8>),
    b: &(usize, usize, Vec<u8>),
    at: (f32, f32),
    r: f32,
) -> f32 {
    let (w, h, _) = a;
    let mut sum = 0.0;
    let mut n = 0.0;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    for y in (at.1 - r).max(0.0) as usize..((at.1 + r) as usize).min(h - 1) {
        for x in (at.0 - r).max(0.0) as usize..((at.0 + r) as usize).min(w - 1) {
            let i = (y * w + x) * 4;
            for k in 0..3 {
                sum += (f32::from(a.2[i + k]) - f32::from(b.2[i + k])).abs();
            }
            n += 3.0;
        }
    }
    if n == 0.0 {
        0.0
    } else {
        sum / n
    }
}

/// Every record the session compiled, written as a build does, passes the checks over the
/// February 2005 files: in their layout.
fn records_pass(ed: &EditorApp, dir: &Path) {
    let out = dir.join("check-overlay");
    let s = &ed.session;
    lyceum_foundry::compile::write_overlay(s.compiled(), s.base(), &s.world().key, &out).unwrap();
    let report = s.check(&out).unwrap();
    assert!(report.is_empty(), "{report:?}");
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn an_infiltration_world_is_drawn_from_the_february_2005_files() {
    let _gpu = gpu_lock();
    let (dir, world) = infiltration("editor-classic-open");
    let mut ed = editor(&world, &[]);
    assert_eq!(ed.session.base().era, ERA);
    assert!(ed.faults.is_empty(), "{:?}", ed.faults);
    let z = ed.session.ground(HOLTBURG, 84.0, 84.0).unwrap();
    ed.look_at(HOLTBURG, Vec3::new(84.0, 84.0, z), 160.0, 30.0, -35.0);
    for _ in 0..4 {
        assert!(ed.frame());
    }
    let img = capture(&mut ed, dir.path());
    assert!(spread(&img) > 20.0, "Holtburg is drawn: {}", spread(&img));
    keep(&mut ed, "01-holtburg.png");
}

#[test]
fn each_tool_builds_an_infiltration_world_in_the_layout_of_its_files() {
    let _gpu = gpu_lock();
    let (dir, world) = infiltration("editor-classic-tools");
    let mut ed = editor(&world, &[]);
    let o = pick::block_origin(HOLTBURG);

    // A static placed on Holtburg's open west side is drawn there.
    let (x, y) = (40.0, 60.0);
    let z = ed.session.ground(HOLTBURG, x, y).unwrap();
    ed.look_at(HOLTBURG, Vec3::new(x, y, z + 1.0), 14.0, -30.0, -20.0);
    for _ in 0..3 {
        ed.frame();
    }
    let at = ed
        .view()
        .unwrap()
        .project(Vec3::new(o.x + x, o.y + y, o.z + z + 1.0))
        .unwrap();
    let before = capture(&mut ed, dir.path());
    let statics = ed.session.statics(HOLTBURG).unwrap().objects.len();
    ed.edit(&Edit::Op(lyceum_foundry::edit::Op::PlaceStatic {
        landblock: HOLTBURG,
        model: DataId(0x0200_040B),
        at: Place {
            x,
            y,
            z: lyceum_foundry::source::Height::Ground,
        },
        rotation: Rotation::Heading(30.0),
        name: None,
    }))
    .unwrap();
    ed.frame();
    assert_eq!(
        ed.session.statics(HOLTBURG).unwrap().objects.len(),
        statics + 1
    );
    let after = capture(&mut ed, dir.path());
    assert!(
        difference(&before, &after, at, 20.0) > 4.0,
        "the stone stands there now"
    );
    keep(&mut ed, "02-static.png");

    // The ground raised: Holtburg is built again, and its ground stands higher.
    let reread = ed.stats.blocks_reread;
    let was = ed.session.ground(HOLTBURG, 96.0, 150.0).unwrap();
    ed.edit(&Edit::Brush {
        brush: Brush::Raise(20),
        landblock: HOLTBURG,
        x: 96.0,
        y: 150.0,
        radius: 20.0,
    })
    .unwrap();
    ed.frame();
    assert_eq!(ed.stats.blocks_reread - reread, 1);
    assert!(ed.session.ground(HOLTBURG, 96.0, 150.0).unwrap() > was + 5.0);
    ed.look_at(HOLTBURG, Vec3::new(96.0, 150.0, was), 90.0, 0.0, -30.0);
    for _ in 0..3 {
        ed.frame();
    }
    keep(&mut ed, "03-terrain.png");

    // The sample's NPC spawned again, drawn with its model from these files.
    let spawns = ed.session.spawns().len();
    let z = ed.session.ground(HOLTBURG, 60.0, 40.0).unwrap();
    ed.edit(&Edit::Op(lyceum_foundry::edit::Op::PlaceSpawn {
        landblock: HOLTBURG,
        weenie: lyceum_foundry::edit::Weenie::Symbol("holtburg:guide".into()),
        origin: Vec3::new(60.0, 40.0, z),
        heading: 180.0,
    }))
    .unwrap();
    assert_eq!(ed.session.spawns().len(), spawns + 1);
    assert!(ed
        .session
        .spawns()
        .iter()
        .all(|s| s.setup == Some(DataId(0x0200_07DD))));
    ed.look_at(HOLTBURG, Vec3::new(60.0, 40.0, z + 1.0), 8.0, 0.0, -15.0);
    for _ in 0..3 {
        ed.frame();
    }
    keep(&mut ed, "04-spawn.png");

    // A building of these files, the most placed that stands alone, dropped west of Holtburg.
    ed.wait_for_names().unwrap();
    let catalog = ed.session.buildings().expect("the files' buildings");
    let model = catalog
        .gallery()
        .into_iter()
        .find(|e| e.set.is_empty() && !e.layouts.is_empty())
        .map(|e| e.model)
        .unwrap();
    let west = 0xA8B4;
    let (op, _) = ed
        .session
        .drop_building_as(model, 0, &Furnished::Template, false, west, 84.0, 84.0, 0.0)
        .unwrap();
    ed.edit(&Edit::Op(op)).unwrap();
    let info = ed.session.statics(west).unwrap();
    assert_eq!(info.buildings.len(), 1);
    assert!(!ed.session.cells(west).is_empty(), "its rooms");
    let gz = ed.session.ground(west, 84.0, 84.0).unwrap();
    ed.look_at(west, Vec3::new(84.0, 84.0, gz + 3.0), 40.0, 30.0, -25.0);
    for _ in 0..3 {
        ed.frame();
    }
    keep(&mut ed, "05-building.png");

    // A dungeon of these files' pieces, built with what fits.
    ed.wait_for_pieces();
    let (lb, set) = super::dungeon::build_with_fits(&mut ed, "classic-dungeon");
    assert!(set.len() >= 8, "{} pieces", set.len());
    assert!(ed.free_portals(lb, &[]).is_empty());
    let walk = ed.session.walk(lb);
    assert!(walk.is_empty(), "{walk:#?}");
    keep(&mut ed, "06-dungeon.png");
    ed.leave_dungeon();

    assert!(ed.faults.is_empty(), "{:?}", ed.faults);
    records_pass(&ed, dir.path());
}
