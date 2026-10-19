//! Behaviour: none (tooling: the world editor, off screen over the retail dats)
//! The editor opened on the sample world over the retail dats, drawn off screen on a device: a
//! click selects what it is over and the handles are drawn where they project, a drag on an arrow
//! moves the static (or a spawn) by what the pointer swept, an edit redraws only the landblocks it
//! reaches, a spawn is drawn where it will stand, the body walks, stops at a static placed in its
//! way and flies again, and the model browser draws and names its models.
//! Fixture: the retail dats in `$DERETH_TEST_DAT_DIR` and a device; run with `--test-threads=1`.

#![cfg(gpu)]

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use dereth_dat::testing::ScratchDir;
use dereth_input::host::HostEvent;
use dereth_input::keys::MouseButton;
use dereth_primitives::Vec3;
use lyceum_editor::app::{EditorApp, Selection};
use lyceum_editor::gizmo::{self, Handle};
use lyceum_foundry::brush::Brush;
use lyceum_foundry::models::Category;
use lyceum_foundry::pick;
use lyceum_foundry::script::Action as Edit;

const HOLTBURG: u16 = 0xA9B4;

/// One device at a time in this binary.
fn gpu_lock() -> MutexGuard<'static, ()> {
    static GPU: Mutex<()> = Mutex::new(());
    GPU.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let p = e.unwrap().path();
        let dest = to.join(p.file_name().unwrap());
        if p.is_dir() {
            copy_tree(&p, &dest);
        } else {
            std::fs::copy(&p, &dest).unwrap();
        }
    }
}

/// The pack tool's sample world, copied into a scratch folder.
fn sample(tag: &str) -> (ScratchDir, PathBuf) {
    dereth_dat::testing::require_dats();
    let dir = ScratchDir::new(tag).unwrap();
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sample");
    copy_tree(&from.join("holtburg"), &dir.path().join("holtburg"));
    std::fs::copy(from.join("world.lock"), dir.path().join("world.lock")).unwrap();
    std::fs::write(
        dir.path().join("world.toml"),
        "key = \"sample-world\"\npacks = [\"holtburg\"]\nout = \"build\"\n",
    )
    .unwrap();
    let world = dir.path().join("world.toml");
    (dir, world)
}

fn editor(world: &Path) -> EditorApp {
    editor_with(world, &[])
}

/// The editor on `world`, with `more` on its command line.
fn editor_with(world: &Path, more: &[&str]) -> EditorApp {
    // The model browser's pictures and placements are kept in cargo's own folder, and read no
    // server database from the user's cache; a test may name its own.
    let cache = Path::new(env!("CARGO_TARGET_TMPDIR")).join("editor-cache");
    let argv: Vec<String> = [
        world.to_str().unwrap(),
        "--dats",
        dereth_dat::testing::dat_dir().to_str().unwrap(),
        "--headless",
        "--size",
        "800x600",
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
    let cfg = lyceum_editor::args::runtime_config(&args).unwrap();
    EditorApp::open(&args, cfg).unwrap()
}

/// The frame drawn last: its width, height and pixels, red, green, blue, alpha.
fn capture(ed: &mut EditorApp, dir: &Path) -> (usize, usize, Vec<u8>) {
    let path = dir.join("frame.png");
    ed.capture(&path).unwrap();
    let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&path).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    let channels = info.line_size / info.width as usize;
    let mut rgba = Vec::with_capacity(info.width as usize * info.height as usize * 4);
    for px in buf[..info.buffer_size()].chunks_exact(channels) {
        rgba.extend_from_slice(&[px[0], px[1], px[2], if channels == 4 { px[3] } else { 255 }]);
    }
    (info.width as usize, info.height as usize, rgba)
}

/// Where a test leaves the frames worth looking at: cargo's own temporary folder.
fn shots(name: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("editor-shots")
        .join(name);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn pixel(img: &(usize, usize, Vec<u8>), x: f32, y: f32) -> [u8; 3] {
    let (w, h, px) = img;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (x, y) = (
        (x.round().max(0.0) as usize).min(w - 1),
        (y.round().max(0.0) as usize).min(h - 1),
    );
    let i = (y * w + x) * 4;
    [px[i], px[i + 1], px[i + 2]]
}

fn click(ed: &mut EditorApp, at: (f32, f32)) {
    ed.queue(HostEvent::CursorMoved {
        x: f64::from(at.0),
        y: f64::from(at.1),
    });
    ed.queue(HostEvent::MouseInput {
        button: MouseButton::Left,
        pressed: true,
    });
    ed.frame();
    ed.queue(HostEvent::MouseInput {
        button: MouseButton::Left,
        pressed: false,
    });
    ed.frame();
}

/// The sample's west stone, `0x0200040B` placed by the pack at (24, 24) on the ground: the first
/// static after Holtburg's own 114. The camera is put looking at it; answers its place.
fn stone_west(ed: &mut EditorApp) -> Vec3 {
    let z = ed.session.ground(HOLTBURG, 24.0, 24.0).unwrap();
    // From the south-west, so the north arrow is not drawn along the up one.
    ed.look_at(HOLTBURG, Vec3::new(24.0, 24.0, z + 1.5), 22.0, -40.0, -25.0);
    for _ in 0..3 {
        ed.frame();
    }
    let o = pick::block_origin(HOLTBURG);
    Vec3::new(o.x + 24.0, o.y + 24.0, z)
}

#[test]
fn a_click_on_a_static_selects_it_and_its_handles_are_drawn_where_they_project() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-select");
    let mut ed = editor(&world);
    let stone = stone_west(&mut ed);
    let at = ed
        .view()
        .unwrap()
        .project(Vec3::new(stone.x, stone.y, stone.z + 1.5))
        .unwrap();
    click(&mut ed, at);
    assert_eq!(
        ed.selection,
        Some(Selection::Static {
            landblock: HOLTBURG,
            index: 114
        })
    );
    let h = ed.handles().expect("a selected static has handles");
    ed.frame();
    let img = capture(&mut ed, dir.path());
    // The east arrow is drawn in its color all along the segment it projects to, and the north
    // arrow in its own.
    for (handle, rgb) in [
        (Handle::X, [0xE8, 0x4A, 0x3C]),
        (Handle::Y, [0x5C, 0xD0, 0x5A]),
    ] {
        let tip = h.arrows.iter().find(|(k, _)| *k == handle).unwrap().1;
        let on = (0..20)
            .filter(|i| {
                #[allow(clippy::cast_precision_loss)]
                let t = 0.3 + 0.03 * *i as f32;
                let p = pixel(
                    &img,
                    h.origin.0 + (tip.0 - h.origin.0) * t,
                    h.origin.1 + (tip.1 - h.origin.1) * t,
                );
                p.iter()
                    .zip(rgb)
                    .all(|(a, b)| (i32::from(*a) - b).abs() <= 12)
            })
            .count();
        assert!(on >= 16, "{handle:?}: {on} of 20 points in its color");
    }
}

#[test]
fn a_drag_along_the_east_arrow_moves_the_static_east_by_what_the_pointer_swept() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-drag");
    let mut ed = editor(&world);
    let stone = stone_west(&mut ed);
    let at = ed
        .view()
        .unwrap()
        .project(Vec3::new(stone.x, stone.y, stone.z + 1.5))
        .unwrap();
    click(&mut ed, at);
    let h = ed.handles().unwrap();
    let tip = h.arrows.iter().find(|(k, _)| *k == Handle::X).unwrap().1;
    let along = |t: f32| {
        (
            h.origin.0 + (tip.0 - h.origin.0) * t,
            h.origin.1 + (tip.1 - h.origin.1) * t,
        )
    };
    let (from, to) = (along(0.6), along(1.4));
    let view = ed.view().unwrap();
    let want = gizmo::drag_along(
        Handle::X,
        stone,
        &view.ray(from.0, from.1),
        &view.ray(to.0, to.1),
    )
    .unwrap();
    assert!(want > 0.5, "the drag sweeps east: {want}");
    ed.queue(HostEvent::CursorMoved {
        x: f64::from(from.0),
        y: f64::from(from.1),
    });
    ed.queue(HostEvent::MouseInput {
        button: MouseButton::Left,
        pressed: true,
    });
    ed.frame();
    // Through a point half way, then to the end: two steps of one drag.
    for p in [along(1.0), to] {
        ed.queue(HostEvent::CursorMoved {
            x: f64::from(p.0),
            y: f64::from(p.1),
        });
        ed.frame();
    }
    ed.queue(HostEvent::MouseInput {
        button: MouseButton::Left,
        pressed: false,
    });
    ed.frame();
    let o = ed.session.statics(HOLTBURG).unwrap().objects[114]
        .frame
        .origin;
    assert!((o.x - (24.0 + want)).abs() < 0.01, "{o:?}, {want}");
    assert!((o.y - 24.0).abs() < 1e-4, "{o:?}");
    assert_eq!(ed.session.history(), (1, 0), "one drag is one step");
    // It still stands on the ground, where it was placed to stand.
    let ground = ed.session.ground(HOLTBURG, o.x, o.y).unwrap();
    assert!((o.z - ground).abs() < 0.01, "{o:?} over {ground}");
    // Ctrl+S saves it: its own entry moves, on the ground still, and the landblock the drag did
    // not reach keeps its file byte for byte.
    let west = std::fs::read(dir.path().join("holtburg/edits/landblocks/A8B4.toml")).unwrap();
    ed.step(&lyceum_editor::script::Step {
        key: Some("save".into()),
        ..Default::default()
    })
    .unwrap();
    ed.frame();
    let text =
        std::fs::read_to_string(dir.path().join("holtburg/edits/landblocks/A9B4.toml")).unwrap();
    let moved = format!(
        "at = [{}, 24.0, \"ground\"]",
        lyceum_foundry::edit::meters(o.x)
    );
    assert!(text.contains(&moved), "{moved} in\n{text}");
    assert_eq!(
        std::fs::read(dir.path().join("holtburg/edits/landblocks/A8B4.toml")).unwrap(),
        west
    );
    assert!(!ed.session.unsaved());
}

/// The mean difference of the two frames' pixels in the square of side `2 * r` around `c`.
fn difference(
    a: &(usize, usize, Vec<u8>),
    b: &(usize, usize, Vec<u8>),
    c: (f32, f32),
    r: f32,
) -> f32 {
    let mut sum = 0.0;
    let mut n = 0.0;
    #[allow(clippy::cast_possible_truncation)]
    let r = r as i32;
    for dy in -r..=r {
        for dx in -r..=r {
            #[allow(clippy::cast_precision_loss)]
            let (x, y) = (c.0 + dx as f32, c.1 + dy as f32);
            let (p, q) = (pixel(a, x, y), pixel(b, x, y));
            sum += p
                .iter()
                .zip(q)
                .map(|(m, n)| (f32::from(*m) - f32::from(n)).abs())
                .sum::<f32>();
            n += 3.0;
        }
    }
    sum / n
}

#[test]
fn a_terrain_raise_rebuilds_only_its_landblock_and_changes_only_the_ground_it_covers() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-raise");
    let mut ed = editor(&world);
    let center = Vec3::new(
        96.0,
        120.0,
        ed.session.ground(HOLTBURG, 96.0, 120.0).unwrap(),
    );
    let far = Vec3::new(
        160.0,
        40.0,
        ed.session.ground(HOLTBURG, 160.0, 40.0).unwrap(),
    );
    ed.look_at(
        HOLTBURG,
        Vec3::new(120.0, 80.0, center.z),
        230.0,
        0.0,
        -75.0,
    );
    for _ in 0..4 {
        ed.frame();
    }
    let view = ed.view().unwrap();
    let o = pick::block_origin(HOLTBURG);
    let world_of = |p: Vec3| Vec3::new(o.x + p.x, o.y + p.y, p.z);
    let (c, f) = (
        view.project(world_of(center)).unwrap(),
        view.project(world_of(far)).unwrap(),
    );
    let before = capture(&mut ed, dir.path());
    let reread = ed.stats.blocks_reread;
    ed.edit(&Edit::Brush {
        brush: Brush::Raise(30),
        landblock: HOLTBURG,
        x: center.x,
        y: center.y,
        radius: 30.0,
    })
    .unwrap();
    assert_eq!(
        ed.stats.blocks_reread - reread,
        1,
        "the brush stays inside Holtburg"
    );
    ed.frame();
    let after = capture(&mut ed, dir.path());
    let near = difference(&before, &after, c, 12.0);
    let away = difference(&before, &after, f, 12.0);
    assert!(near > 8.0, "the raised ground is drawn anew: {near}");
    assert!(away < 1.0, "the ground it does not cover is not: {away}");
}

#[test]
fn a_spawn_is_drawn_where_it_will_stand_and_is_selected() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-spawn");
    let mut ed = editor(&world);
    let z = ed.session.ground(HOLTBURG, 60.0, 60.0).unwrap();
    ed.look_at(HOLTBURG, Vec3::new(60.0, 60.0, z + 1.0), 12.0, 0.0, -20.0);
    for _ in 0..3 {
        ed.frame();
    }
    let o = pick::block_origin(HOLTBURG);
    let at = ed
        .view()
        .unwrap()
        .project(Vec3::new(o.x + 60.0, o.y + 60.0, z + 1.0))
        .unwrap();
    let before = capture(&mut ed, dir.path());
    ed.mode = lyceum_editor::app::Mode::Spawn;
    ed.panels.weenie = Some(lyceum_foundry::edit::Weenie::Symbol(
        "holtburg:guide".into(),
    ));
    let ground = ed
        .view()
        .unwrap()
        .project(Vec3::new(o.x + 60.0, o.y + 60.0, z))
        .unwrap();
    click(&mut ed, ground);
    let selected = ed.selection.clone();
    // The selection's mark is not what is looked at.
    ed.selection = None;
    ed.frame();
    let spawns = ed.session.spawns();
    assert_eq!(spawns.len(), 2, "the sample's and this one");
    let s = spawns
        .iter()
        .find(|s| s.file.ends_with("A9B4 guide-2.sql"))
        .expect("the new spawn's own file");
    assert_eq!(s.setup, Some(dereth_primitives::DataId(0x0200_07DD)));
    assert!(
        matches!(&selected, Some(Selection::Spawn { file, guid }) if *file == s.file && *guid == s.guid)
    );
    let after = capture(&mut ed, dir.path());
    assert!(
        difference(&before, &after, at, 6.0) > 10.0,
        "the drudge stands there now"
    );
}

#[test]
fn the_body_walks_from_where_the_camera_is_and_the_camera_flies_again() {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-walk");
    let mut ed = editor(&world);
    stone_west(&mut ed);
    ed.set_walking(true).unwrap();
    for _ in 0..3 {
        ed.frame();
    }
    let body = ed
        .core
        .world
        .as_ref()
        .and_then(|w| w.character.as_ref())
        .map(dereth_client_runtime::character::Character::position)
        .expect("a body");
    let (under, x, y) = lyceum_editor::camera::landblock_of(ed.camera.position).unwrap();
    assert_eq!(body.cell.0 >> 16, u32::from(under), "under the camera");
    assert!(body.cell.0 & 0xFFFF <= 0x40, "outdoors");
    let o = body.frame.origin;
    assert!(
        (o.x - x).abs() < 0.5 && (o.y - y).abs() < 0.5,
        "{o:?} stands where the camera was, ({x}, {y})"
    );
    ed.set_walking(false).unwrap();
    ed.frame();
    assert!(ed.core.world.as_ref().unwrap().character.is_none());
    assert!(!ed.walking);
}

#[test]
fn a_drag_on_the_ring_turns_the_static_by_the_angle_the_pointer_swept() {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-turn");
    let mut ed = editor(&world);
    let stone = stone_west(&mut ed);
    let at = ed
        .view()
        .unwrap()
        .project(Vec3::new(stone.x, stone.y, stone.z + 1.5))
        .unwrap();
    click(&mut ed, at);
    let h = ed.handles().unwrap();
    // A point of the ring away from the arrows, and one a quarter of the ring on.
    let n = h.ring.len();
    let (from, to) = (h.ring[n * 5 / 8], h.ring[n * 7 / 8]);
    let view = ed.view().unwrap();
    // The bearing from the static of the level point under a pixel, clockwise from north.
    let bearing = |p: (f32, f32)| {
        let r = view.ray(p.0, p.1);
        let t = (stone.z - r.origin.z) / r.dir.z;
        let (x, y) = (r.origin.x + r.dir.x * t, r.origin.y + r.dir.y * t);
        dereth_primitives::num::math::atan2f(x - stone.x, y - stone.y).to_degrees()
    };
    let want = (bearing(to) - bearing(from) + 540.0) % 360.0 - 180.0;
    assert!(want.abs() > 30.0, "the drag sweeps round: {want}");
    let before = ed.session.statics(HOLTBURG).unwrap().objects[114].frame;
    for (p, press) in [(from, Some(true)), (to, None), (to, Some(false))] {
        ed.queue(HostEvent::CursorMoved {
            x: f64::from(p.0),
            y: f64::from(p.1),
        });
        if let Some(pressed) = press {
            ed.queue(HostEvent::MouseInput {
                button: MouseButton::Left,
                pressed,
            });
        }
        ed.frame();
    }
    let after = ed.session.statics(HOLTBURG).unwrap().objects[114].frame;
    let turned = gizmo::heading_of(after.rotation) - gizmo::heading_of(before.rotation);
    let turned = (turned + 540.0) % 360.0 - 180.0;
    assert!((turned - want).abs() < 0.1, "{turned} for {want}");
    assert_eq!(after.origin, before.origin, "a turn moves nothing");
}

#[test]
fn a_static_placed_inside_a_house_is_warned_of_and_a_click_on_the_warning_goes_there() {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-warn");
    let mut ed = editor(&world);
    stone_west(&mut ed);
    assert!(ed.faults.is_empty(), "{:?}", ed.faults);
    // In the middle of the house at (79.5, 37.5), as the sample's first stones stood.
    ed.edit(&Edit::Op(lyceum_foundry::edit::Op::PlaceStatic {
        landblock: HOLTBURG,
        model: dereth_primitives::DataId(0x0200_040B),
        at: lyceum_foundry::source::Place {
            x: 79.5,
            y: 37.5,
            z: lyceum_foundry::source::Height::Ground,
        },
        rotation: lyceum_foundry::source::Rotation::Heading(0.0),
    }))
    .unwrap();
    assert_eq!(ed.faults.len(), 1, "{:?}", ed.faults);
    let f = ed.faults[0].clone();
    assert!(
        f.warning && f.text.contains("stands inside building 8 (0x01000BC3)"),
        "{f:?}"
    );
    let (lb, at) = f.at.expect("a place");
    assert_eq!(lb, HOLTBURG);
    assert!((at.x - 79.5).abs() < 1e-3 && (at.y - 37.5).abs() < 1e-3);
    ed.frame();
    // The finding is listed with the checks; a click on its row.
    let row = ed.fault_rect(0).expect("the finding listed");
    click(&mut ed, (row.center().x, row.center().y));
    let target = Vec3::new(
        pick::block_origin(HOLTBURG).x + 79.5,
        pick::block_origin(HOLTBURG).y + 37.5,
        at.z,
    );
    let c = ed.camera.position;
    let d = Vec3::new(c.x - target.x, c.y - target.y, c.z - target.z).magnitude();
    assert!(
        (d - 40.0).abs() < 0.5,
        "the camera looks at it from 40 m: {d}"
    );
}

#[test]
fn a_building_moved_off_the_ground_is_warned_of_its_doors_and_a_click_on_the_warning_goes_there() {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-building-warn");
    let mut ed = editor(&world);
    stone_west(&mut ed);
    assert!(ed.faults.is_empty(), "{:?}", ed.faults);
    // The cottage at (79.5, 37.5, 94.0), raised 3 m.
    ed.edit(&Edit::MoveBuilding {
        landblock: HOLTBURG,
        index: 8,
        at: [79.5, 37.5, 97.0],
        rotation: lyceum_foundry::source::Rotation::Heading(45.0),
    })
    .unwrap();
    let doors: Vec<_> = ed
        .faults
        .iter()
        .filter(|f| f.text.starts_with("0xA9B4FFFE: building 8: its door"))
        .cloned()
        .collect();
    assert!(!doors.is_empty(), "{:?}", ed.faults);
    let f = &doors[0];
    assert!(
        f.warning && f.text.contains("3.0 m over the ground"),
        "{f:?}"
    );
    let (lb, at) = f.at.expect("a place");
    assert_eq!(lb, HOLTBURG);
    assert!(
        (at.x - 79.5).abs() < 1e-3 && (at.y - 37.5).abs() < 1e-3,
        "{at:?}"
    );
    ed.frame();
    let i = ed.faults.iter().position(|x| x == f).unwrap();
    let row = ed.fault_rect(i).expect("the finding listed");
    click(&mut ed, (row.center().x, row.center().y));
    let target = Vec3::new(
        pick::block_origin(HOLTBURG).x + 79.5,
        pick::block_origin(HOLTBURG).y + 37.5,
        at.z,
    );
    let c = ed.camera.position;
    let d = Vec3::new(c.x - target.x, c.y - target.y, c.z - target.z).magnitude();
    assert!(
        (d - 40.0).abs() < 0.5,
        "the camera looks at it from 40 m: {d}"
    );
    // Put back: no warning of it.
    ed.edit(&Edit::Undo).unwrap();
    assert!(
        ed.faults.iter().all(|f| !f.text.contains(": building 8:")),
        "{:?}",
        ed.faults
    );
}

/// Drag along `handle`'s arrow from 60% of it to 140%, as the pointer would over four frames;
/// answers how far that moves along the arrow, worked out from the pointer's rays.
fn drag_arrow(ed: &mut EditorApp, handle: Handle, origin: Vec3) -> f32 {
    let h = ed.handles().unwrap();
    let tip = h.arrows.iter().find(|(k, _)| *k == handle).unwrap().1;
    let along = |t: f32| {
        (
            h.origin.0 + (tip.0 - h.origin.0) * t,
            h.origin.1 + (tip.1 - h.origin.1) * t,
        )
    };
    let (from, to) = (along(0.6), along(1.4));
    let view = ed.view().unwrap();
    let want = gizmo::drag_along(
        handle,
        origin,
        &view.ray(from.0, from.1),
        &view.ray(to.0, to.1),
    )
    .unwrap();
    for (p, press) in [
        (from, Some(true)),
        (along(1.0), None),
        (to, None),
        (to, Some(false)),
    ] {
        ed.queue(HostEvent::CursorMoved {
            x: f64::from(p.0),
            y: f64::from(p.1),
        });
        if let Some(pressed) = press {
            ed.queue(HostEvent::MouseInput {
                button: MouseButton::Left,
                pressed,
            });
        }
        ed.frame();
    }
    want
}

#[test]
fn a_spawn_is_moved_and_turned_by_its_handles_and_its_sql_is_rewritten() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-spawn-drag");
    let mut ed = editor(&world);
    // By the east edge of its outdoor cell (48 to 72 m), so the drag carries it into the next.
    let x0 = 70.5;
    let z = ed.session.ground(HOLTBURG, x0, 60.0).unwrap();
    ed.edit(&Edit::Op(lyceum_foundry::edit::Op::PlaceSpawn {
        landblock: HOLTBURG,
        weenie: lyceum_foundry::edit::Weenie::Symbol("holtburg:guide".into()),
        origin: Vec3::new(x0, 60.0, z),
        heading: 0.0,
    }))
    .unwrap();
    let placed = ed
        .session
        .spawns()
        .into_iter()
        .find(|s| s.file.ends_with("A9B4 guide-2.sql"))
        .unwrap();
    ed.selection = Some(Selection::Spawn {
        file: placed.file.clone(),
        guid: placed.guid.clone(),
    });
    ed.look_at(HOLTBURG, Vec3::new(x0, 60.0, z + 1.0), 18.0, -40.0, -25.0);
    for _ in 0..3 {
        ed.frame();
    }
    let o = pick::block_origin(HOLTBURG);
    let want = drag_arrow(&mut ed, Handle::X, Vec3::new(o.x + x0, o.y + 60.0, z));
    assert!(want > 1.5, "the drag sweeps east, out of the cell: {want}");
    let moved = ed
        .session
        .spawns()
        .into_iter()
        .find(|s| s.guid == placed.guid)
        .unwrap();
    let m = moved.frame.origin;
    assert!((m.x - (x0 + want)).abs() < 0.01, "{m:?}, {want}");
    assert!((m.y - 60.0).abs() < 1e-3, "{m:?}");
    let ground = ed.session.ground(HOLTBURG, m.x, m.y).unwrap();
    assert!(
        (m.z - ground).abs() < 0.01,
        "on the ground still: {m:?} over {ground}"
    );
    assert_eq!(
        moved.cell,
        (u32::from(HOLTBURG) << 16) | lyceum_foundry::edit::outdoor_cell(m.x, m.y)
    );
    assert_eq!(ed.session.history(), (2, 0), "the spawn, and one drag");
    // The ring turns it, where it stands.
    let h = ed.handles().unwrap();
    let n = h.ring.len();
    let (from, to) = (h.ring[n * 5 / 8], h.ring[n * 7 / 8]);
    let center = Vec3::new(o.x + m.x, o.y + m.y, m.z);
    let view = ed.view().unwrap();
    let bearing = |p: (f32, f32)| {
        let r = view.ray(p.0, p.1);
        let t = (center.z - r.origin.z) / r.dir.z;
        let (x, y) = (r.origin.x + r.dir.x * t, r.origin.y + r.dir.y * t);
        dereth_primitives::num::math::atan2f(x - center.x, y - center.y).to_degrees()
    };
    let swept = (bearing(to) - bearing(from) + 540.0) % 360.0 - 180.0;
    for (p, press) in [(from, Some(true)), (to, None), (to, Some(false))] {
        ed.queue(HostEvent::CursorMoved {
            x: f64::from(p.0),
            y: f64::from(p.1),
        });
        if let Some(pressed) = press {
            ed.queue(HostEvent::MouseInput {
                button: MouseButton::Left,
                pressed,
            });
        }
        ed.frame();
    }
    let turned = ed
        .session
        .spawns()
        .into_iter()
        .find(|s| s.guid == placed.guid)
        .unwrap();
    let heading = gizmo::heading_of(turned.frame.rotation);
    let d = (heading - swept + 540.0) % 360.0 - 180.0;
    assert!(d.abs() < 0.1, "turned to {heading} for {swept}");
    assert_eq!(turned.frame.origin, m, "a turn moves nothing");
    // Saved, the spawn's file says where it stands now.
    ed.session.save().unwrap();
    let sql = std::fs::read_to_string(
        dir.path()
            .join("holtburg/server/sql/landblocks/A9B4 guide-2.sql"),
    )
    .unwrap();
    assert!(
        sql.contains(&format!(", {}, 60.0, ", lyceum_foundry::edit::meters(m.x))),
        "{sql}"
    );
    // Each edit wrote only the records it reached, after the scratch overlay's first two
    // folders were written whole.
    assert_eq!(ed.stats.whole_writes, 1, "{:?}", ed.stats);
    assert!(
        ed.stats.records_written <= 2 * 2 * ed.stats.reloads,
        "{:?}",
        ed.stats
    );
}

/// Where the walking body stands, in world meters.
fn body_at(ed: &EditorApp) -> Vec3 {
    let p = ed
        .core
        .world
        .as_ref()
        .and_then(|w| w.character.as_ref())
        .map(dereth_client_runtime::character::Character::position)
        .expect("a body");
    let lb = u16::try_from(p.cell.0 >> 16).unwrap();
    let o = pick::block_origin(lb);
    Vec3::new(
        o.x + p.frame.origin.x,
        o.y + p.frame.origin.y,
        p.frame.origin.z,
    )
}

/// Walk north from the camera's place for `frames` frames, after doing `meanwhile` with the body
/// standing there; answers how far north the body went.
fn walk_north(
    ed: &mut EditorApp,
    start: (f32, f32),
    frames: u32,
    meanwhile: impl FnOnce(&mut EditorApp),
) -> f32 {
    let z = ed.session.ground(HOLTBURG, start.0, start.1).unwrap();
    ed.step(&lyceum_editor::script::Step {
        camera: Some(lyceum_editor::script::Camera::At {
            landblock: HOLTBURG,
            at: Vec3::new(start.0, start.1, z + 1.8),
            yaw: 0.0,
            pitch: -10.0,
        }),
        ..Default::default()
    })
    .unwrap();
    ed.frame();
    ed.set_walking(true).unwrap();
    for _ in 0..3 {
        ed.frame();
    }
    meanwhile(ed);
    ed.frame();
    let from = body_at(ed);
    let w = |pressed| HostEvent::KeyboardInput {
        key: dereth_input::keys::Key::new(0x57, 0),
        pressed,
        text: None,
    };
    ed.queue(w(true));
    for _ in 0..frames {
        ed.frame();
    }
    ed.queue(w(false));
    ed.frame();
    let to = body_at(ed);
    ed.set_walking(false).unwrap();
    ed.frame();
    to.y - from.y
}

#[test]
fn the_walking_body_stops_at_a_static_placed_in_its_way() {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-walk-block");
    let mut ed = editor(&world);
    // Open meadow north of Holtburg's houses.
    let start = (120.0, 110.0);
    let free = walk_north(&mut ed, start, 150, |_| {});
    assert!(free > 12.0, "the way north is open: {free} m");
    // A house of the sample's, placed 7 m ahead while the body stands there.
    let blocked = walk_north(&mut ed, start, 150, |ed| {
        ed.edit(&Edit::Op(lyceum_foundry::edit::Op::PlaceStatic {
            landblock: HOLTBURG,
            model: dereth_primitives::DataId(0x0100_0A2B),
            at: lyceum_foundry::source::Place {
                x: start.0,
                y: start.1 + 7.0,
                z: lyceum_foundry::source::Height::Ground,
            },
            rotation: lyceum_foundry::source::Rotation::Heading(0.0),
        }))
        .unwrap();
    });
    assert!(blocked > 0.5, "the body walks up to it: {blocked} m");
    assert!(
        blocked < 7.0,
        "and stops short of its middle: {blocked} m, {free} m when open"
    );
}

#[test]
fn the_walking_body_walks_on_ground_raised_while_it_stands_there() {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-walk-raise");
    let mut ed = editor(&world);
    // Slowly, so the few steps stay on the raised ground.
    ed.camera.speed = 0;
    let at = (120.0, 110.0);
    let z = ed.session.ground(HOLTBURG, at.0, at.1).unwrap();
    ed.look_at(HOLTBURG, Vec3::new(at.0, at.1, z), 6.0, 0.0, -30.0);
    ed.frame();
    ed.set_walking(true).unwrap();
    for _ in 0..5 {
        ed.frame();
    }
    let before = body_at(&ed);
    ed.edit(&Edit::Brush {
        brush: Brush::Raise(3),
        landblock: HOLTBURG,
        x: at.0,
        y: at.1,
        radius: 30.0,
    })
    .unwrap();
    // A body at rest keeps its contact; a few steps on, it walks on the ground as it is now.
    let w = |pressed| HostEvent::KeyboardInput {
        key: dereth_input::keys::Key::new(0x57, 0),
        pressed,
        text: None,
    };
    ed.queue(w(true));
    for _ in 0..30 {
        ed.frame();
    }
    ed.queue(w(false));
    for _ in 0..5 {
        ed.frame();
    }
    let after = body_at(&ed);
    let o = pick::block_origin(HOLTBURG);
    let ground = ed
        .session
        .ground(HOLTBURG, after.x - o.x, after.y - o.y)
        .unwrap();
    assert!(
        after.z - before.z > 2.0,
        "lifted with the ground: {} to {}",
        before.z,
        after.z
    );
    assert!(
        (after.z - ground).abs() < 0.2,
        "standing on it: {} over {ground}",
        after.z
    );
}

#[test]
fn the_model_browser_draws_a_models_picture_once_names_it_and_finds_it_by_name() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-browser");
    let pictures = dir.path().join("pictures");
    let pictures_arg = pictures.to_str().unwrap().to_owned();
    let gribble = dereth_primitives::DataId(0x0200_07DD);
    let house = dereth_primitives::DataId(0x0100_0A2B);
    let path;
    {
        let mut ed = editor_with(&world, &["--thumbnails", &pictures_arg]);
        ed.mode = lyceum_editor::app::Mode::Place;
        // By a weenie's name, whatever its case: the sample's guide is drawn with this setup.
        ed.wait_for_names().unwrap();
        ed.panels.filter = "GRIBBLE".into();
        assert_eq!(ed.browser_list(), [gribble]);
        assert_eq!(
            ed.names.label(gribble).as_deref(),
            Some("Gribble the Painted")
        );
        ed.frame();
        ed.frame();
        assert_eq!(ed.pictures(), (1, 0), "drawn once, and held");
        path = ed.picture_path(gribble).unwrap();
        assert!(
            path.starts_with(&pictures),
            "kept where it was told: {path:?}"
        );
        let img = capture(&mut ed, dir.path());
        // The picture is drawn in the list's row: the model, not the empty slot's flat color.
        let (shown, rect) = ed.picture_rects[0];
        assert_eq!(shown, gribble);
        let colors: std::collections::BTreeSet<[u8; 3]> = (0..40)
            .flat_map(|y| (0..40).map(move |x| (x, y)))
            .map(|(x, y)| {
                pixel(
                    &img,
                    rect.min.x + 4.0 + x as f32,
                    rect.min.y + 4.0 + y as f32,
                )
            })
            .collect();
        assert!(colors.len() > 20, "{} colors in the picture", colors.len());
        // A graphics object draws too, found by its id.
        ed.panels.filter = "01000a2b".into();
        assert_eq!(ed.browser_list(), [house]);
        ed.frame();
        assert_eq!(ed.pictures(), (2, 0));
    }
    // A picture already drawn is read from the cache, not drawn again.
    let mut ed = editor_with(&world, &["--thumbnails", &pictures_arg]);
    ed.wait_for_names().unwrap();
    ed.mode = lyceum_editor::app::Mode::Place;
    ed.panels.filter = "gribble".into();
    ed.frame();
    assert_eq!(ed.pictures(), (0, 1));
    assert_eq!(ed.picture_path(gribble).unwrap(), path);
}

#[test]
fn the_body_walks_at_the_speed_the_camera_flies_at() {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-walk-speed");
    let mut ed = editor(&world);
    let start = (120.0, 110.0);
    ed.camera.speed = 2; // 5 m/s
    let slow = walk_north(&mut ed, start, 60, |_| {});
    ed.camera.speed = 4; // 25 m/s
    let fast = walk_north(&mut ed, start, 60, |_| {});
    assert!(slow > 1.0, "{slow} m");
    assert!(fast > 3.0 * slow, "{fast} m against {slow} m");
}

/// Hold key `vk` down for `frames` frames, then let it go.
fn hold_key(ed: &mut EditorApp, vk: usize, frames: u32) {
    let k = |pressed| HostEvent::KeyboardInput {
        key: dereth_input::keys::Key::new(vk, 0),
        pressed,
        text: None,
    };
    ed.queue(k(true));
    for _ in 0..frames {
        ed.frame();
    }
    ed.queue(k(false));
    ed.frame();
}

#[test]
fn space_raises_the_flying_camera_and_a_held_space_jumps_the_body() {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-space");
    let mut ed = editor(&world);
    let at = (120.0, 110.0);
    let z = ed.session.ground(HOLTBURG, at.0, at.1).unwrap();
    ed.look_at(HOLTBURG, Vec3::new(at.0, at.1, z), 6.0, 0.0, -30.0);
    ed.frame();
    let before = ed.camera.position.z;
    hold_key(&mut ed, 0x20, 30);
    let rose = ed.camera.position.z - before;
    assert!(rose > 1.0, "the camera rose {rose} m");
    // Walking, a held Space charges a jump and letting it go jumps: the body leaves the ground.
    ed.set_walking(true).unwrap();
    for _ in 0..5 {
        ed.frame();
    }
    // How high a jump charged for `frames` frames takes the body.
    let jump = |ed: &mut EditorApp, frames: u32| {
        let ground = body_at(ed).z;
        let k = |pressed| HostEvent::KeyboardInput {
            key: dereth_input::keys::Key::new(0x20, 0),
            pressed,
            text: None,
        };
        ed.queue(k(true));
        for _ in 0..frames {
            ed.frame();
        }
        ed.queue(k(false));
        let mut highest = ground;
        for _ in 0..60 {
            ed.frame();
            highest = highest.max(body_at(ed).z);
        }
        highest - ground
    };
    let short = jump(&mut ed, 2);
    let long = jump(&mut ed, 25);
    assert!(long > 0.3, "the body rose {long} m");
    assert!(
        long > short + 0.1,
        "a longer charge jumps higher: {short} m, then {long} m"
    );
}

#[test]
fn shift_lowers_the_flying_camera_and_walks_the_body() {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-shift");
    let mut ed = editor(&world);
    let at = (120.0, 110.0);
    let z = ed.session.ground(HOLTBURG, at.0, at.1).unwrap();
    ed.look_at(HOLTBURG, Vec3::new(at.0, at.1, z), 30.0, 0.0, -30.0);
    ed.frame();
    let before = ed.camera.position.z;
    hold_key(&mut ed, 0x10, 30);
    let fell = before - ed.camera.position.z;
    assert!(fell > 1.0, "the camera came down {fell} m");
    // Walking, the body runs; with Shift held it walks, and goes less far.
    let start = (120.0, 110.0);
    let ran = walk_north(&mut ed, start, 60, |_| {});
    let shift = |pressed| HostEvent::KeyboardInput {
        key: dereth_input::keys::Key::new(0x10, 0),
        pressed,
        text: None,
    };
    let walked = walk_north(&mut ed, start, 60, |ed| ed.queue(shift(true)));
    ed.queue(shift(false));
    ed.frame();
    assert!(
        walked > 1.0 && walked < 0.8 * ran,
        "walked {walked} m where it ran {ran} m"
    );
}

/// Where the sample's stone at `(x, y)` of Holtburg is drawn, a meter and a half up.
fn stone_pixel(ed: &EditorApp, x: f32, y: f32) -> (f32, f32) {
    let z = ed.session.ground(HOLTBURG, x, y).unwrap();
    let o = pick::block_origin(HOLTBURG);
    ed.view()
        .unwrap()
        .project(Vec3::new(o.x + x, o.y + y, z + 1.5))
        .unwrap()
}

#[test]
fn statics_selected_together_move_and_go_together_each_as_one_step() {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-multi");
    let mut ed = editor(&world);
    let z = ed.session.ground(HOLTBURG, 36.0, 28.0).unwrap();
    ed.look_at(HOLTBURG, Vec3::new(36.0, 28.0, z + 1.0), 40.0, -40.0, -35.0);
    for _ in 0..3 {
        ed.frame();
    }
    let (west, east) = (stone_pixel(&ed, 24.0, 24.0), stone_pixel(&ed, 48.0, 24.0));
    // A click, then a Ctrl-click: both selected, the second the one the handles show.
    click(&mut ed, west);
    let ctrl = |pressed| HostEvent::KeyboardInput {
        key: dereth_input::keys::Key::new(0x11, 0),
        pressed,
        text: None,
    };
    ed.queue(ctrl(true));
    click(&mut ed, east);
    ed.queue(ctrl(false));
    ed.frame();
    let stone = |i: usize| Selection::Static {
        landblock: HOLTBURG,
        index: i,
    };
    assert_eq!(ed.selection, Some(stone(115)));
    assert_eq!(ed.also, [stone(114)]);
    let frame_of =
        |ed: &EditorApp, i: usize| ed.session.statics(HOLTBURG).unwrap().objects[i].frame;
    let (w0, e0) = (frame_of(&ed, 114), frame_of(&ed, 115));
    // The east arrow drags both the same way, as one step.
    let o = pick::block_origin(HOLTBURG);
    let zz = ed.session.ground(HOLTBURG, 48.0, 24.0).unwrap();
    let want = drag_arrow(&mut ed, Handle::X, Vec3::new(o.x + 48.0, o.y + 24.0, zz));
    let (w1, e1) = (frame_of(&ed, 114), frame_of(&ed, 115));
    assert!(
        (e1.origin.x - e0.origin.x - want).abs() < 0.01,
        "{e1:?} {want}"
    );
    assert!(
        (w1.origin.x - w0.origin.x - want).abs() < 0.01,
        "{w1:?} {want}"
    );
    assert_eq!(ed.session.history(), (1, 0), "one drag of two is one step");
    // Delete takes both away, as one step; undo puts both back.
    let n = ed.session.statics(HOLTBURG).unwrap().objects.len();
    hold_key(&mut ed, 0x2E, 1);
    assert_eq!(ed.session.statics(HOLTBURG).unwrap().objects.len(), n - 2);
    assert_eq!(ed.session.history(), (2, 0));
    assert!(ed.selection.is_none() && ed.also.is_empty());
    ed.edit(&Edit::Undo).unwrap();
    assert_eq!(ed.session.statics(HOLTBURG).unwrap().objects.len(), n);
    // A box drawn round the three stones selects them all.
    ed.edit(&Edit::Undo).unwrap();
    ed.frame();
    let pts = [
        stone_pixel(&ed, 24.0, 24.0),
        stone_pixel(&ed, 48.0, 24.0),
        stone_pixel(&ed, 36.0, 34.0),
    ];
    let lo = (
        pts.iter().map(|p| p.0).fold(f32::INFINITY, f32::min) - 30.0,
        pts.iter().map(|p| p.1).fold(f32::INFINITY, f32::min) - 30.0,
    );
    let hi = (
        pts.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max) + 30.0,
        pts.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max) + 30.0,
    );
    for (p, press) in [(lo, Some(true)), (hi, None), (hi, Some(false))] {
        ed.queue(HostEvent::CursorMoved {
            x: f64::from(p.0),
            y: f64::from(p.1),
        });
        if let Some(pressed) = press {
            ed.queue(HostEvent::MouseInput {
                button: MouseButton::Left,
                pressed,
            });
        }
        ed.frame();
    }
    let mut got: Vec<Selection> = ed.selection.iter().chain(&ed.also).cloned().collect();
    got.sort_by_key(|s| match s {
        Selection::Static { index, .. } => *index,
        _ => usize::MAX,
    });
    let statics: Vec<&Selection> = got
        .iter()
        .filter(|s| matches!(s, Selection::Static { index, .. } if (114..=116).contains(index)))
        .collect();
    assert_eq!(statics.len(), 3, "{got:?}");
}

#[test]
fn the_browser_shows_one_category_as_a_grid_and_a_tag_finds_its_models_again() {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-shelves");
    let gribble = dereth_primitives::DataId(0x0200_07DD);
    {
        let mut ed = editor(&world);
        ed.wait_for_names().unwrap();
        assert!(!ed.naming());
        ed.mode = lyceum_editor::app::Mode::Place;
        let counts = ed.category_counts();
        // The guide, who cannot be attacked, is the one NPC the sample's SQL names.
        for c in [
            Category::Tree,
            Category::Building,
            Category::Npc,
            Category::Part,
        ] {
            assert!(counts.get(&c).is_some_and(|n| *n > 0), "{c:?}: {counts:?}");
        }
        // One shelf: every model on it is a tree, drawn in rows of several.
        ed.panels.category = Some(Category::Tree);
        let list = ed.browser_list();
        assert_eq!(list.len(), counts[&Category::Tree]);
        assert!(list.iter().all(|m| ed.names.category(*m) == Category::Tree));
        ed.frame();
        let rects = ed.picture_rects.clone();
        assert!(rects.len() > 4, "{} pictures shown", rects.len());
        let first_row: Vec<_> = rects
            .iter()
            .filter(|(_, r)| (r.min.y - rects[0].1.min.y).abs() < 1.0)
            .collect();
        assert!(first_row.len() > 1, "a row of {}", first_row.len());
        assert!(
            rects.iter().any(|(_, r)| r.min.y > rects[0].1.max.y),
            "a second row"
        );
        assert!(rects.iter().all(|(m, _)| list.contains(m)));
        // A click on a picture chooses its model.
        let (pick, r) = rects[2];
        click(&mut ed, (r.center().x, r.center().y));
        assert_eq!(ed.model, Some(pick));
        // Those named by the filter come before those only one of whose uses it names.
        ed.panels.filter = "grassland".into();
        let named: Vec<bool> = ed
            .browser_list()
            .iter()
            .map(|m| {
                ed.names
                    .label(*m)
                    .is_some_and(|l| l.to_lowercase().contains("grassland"))
            })
            .collect();
        let first_other = named.iter().position(|n| !n).expect("one grows there too");
        assert!(first_other > 0 && named[first_other..].iter().all(|n| !n));
        ed.panels.filter.clear();
        // A tag, kept beside the world, finds the guide's model on its own shelf.
        ed.panels.category = None;
        ed.tag_model(gribble, "guide", true).unwrap();
        let file = world
            .parent()
            .unwrap()
            .join(lyceum_foundry::models::Tags::FILE);
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains(r#"0x020007DD = ["guide"]"#), "{text}");
        ed.panels.tag = Some("guide".into());
        assert_eq!(ed.browser_list(), [gribble]);
        ed.panels.tag = None;
        ed.panels.filter = "GUIDE".into();
        assert!(ed.browser_list().contains(&gribble));
    }
    let ed = editor(&world);
    assert_eq!(ed.tags.of(gribble), ["guide"]);
}

#[test]
fn the_placements_gathered_once_are_read_from_the_cache_the_next_time() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-placements");
    let cache = dir.path().join("cache");
    let cache_arg = cache.to_str().unwrap().to_owned();
    let building = dereth_primitives::DataId(0x0100_0C1E);
    let set = lyceum_foundry::dats::DatSet::open(&dereth_dat::testing::dat_dir()).unwrap();
    let file = lyceum_editor::catalog::placements_file(&cache, &set);
    let town = {
        let mut ed = editor_with(&world, &["--thumbnails", &cache_arg]);
        ed.wait_for_names().unwrap();
        let label = ed.names.label(building).unwrap();
        label.strip_suffix(" building").unwrap().to_owned()
    };
    // What the next start reads is the file: a town renamed in it is the label's.
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, text.replace(&town, "Testburg")).unwrap();
    let mut ed = editor_with(&world, &["--thumbnails", &cache_arg]);
    ed.wait_for_names().unwrap();
    assert_eq!(
        ed.names.label(building).as_deref(),
        Some("Testburg building")
    );
}

#[test]
fn the_road_brush_marks_the_vertices_it_covers_and_paints_and_erases_just_those() {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-road");
    let mut ed = editor(&world);
    let z = ed.session.ground(HOLTBURG, 120.0, 120.0).unwrap();
    ed.look_at(HOLTBURG, Vec3::new(120.0, 120.0, z), 40.0, 0.0, -60.0);
    ed.frame();
    ed.mode = lyceum_editor::app::Mode::Terrain;
    ed.panels.brush = Brush::Road(1);
    ed.panels.radius = 24.0;
    // The pointer a few meters off the vertex at (120, 120): the brush is centered on it.
    let o = pick::block_origin(HOLTBURG);
    let near = ed.session.ground(HOLTBURG, 117.0, 123.0).unwrap();
    let p = ed
        .view()
        .unwrap()
        .project(Vec3::new(o.x + 117.0, o.y + 123.0, near))
        .unwrap();
    let (gx, gy) = (0xA9 * 8 + 5, 0xB4 * 8 + 5);
    let road = |ed: &EditorApp, v: (u32, u32)| ed.session.vertex(v.0, v.1).unwrap().road;
    // The vertex under it a road of another kind first: a road painted over it keeps its kind.
    ed.panels.brush = Brush::Road(2);
    ed.panels.radius = 0.0;
    click(&mut ed, p);
    assert_eq!(road(&ed, (gx, gy)), 2);
    ed.panels.brush = Brush::Road(1);
    ed.panels.radius = 24.0;
    ed.frame();
    let mut marked: Vec<(u32, u32)> = ed.vertex_marks.iter().map(|m| m.0).collect();
    marked.sort_unstable();
    assert_eq!(
        marked,
        [
            (gx - 1, gy),
            (gx, gy - 1),
            (gx, gy),
            (gx, gy + 1),
            (gx + 1, gy)
        ]
    );
    for (v, change, _) in &ed.vertex_marks {
        assert_eq!(*change, road(&ed, *v) == 0, "{v:?}");
    }
    let beyond = (gx + 1, gy + 1);
    let left = road(&ed, beyond);
    click(&mut ed, p);
    assert!(marked.iter().all(|v| road(&ed, *v) != 0), "painted");
    assert_eq!(road(&ed, (gx, gy)), 2, "its own kind kept");
    assert_eq!(road(&ed, beyond), left, "one vertex beyond is left");
    assert_eq!(ed.session.history(), (2, 0));
    // Painted, nothing more to change; erased, every one of them is plain ground again.
    ed.frame();
    assert!(ed.vertex_marks.iter().all(|m| !m.1));
    ed.panels.brush = Brush::Road(0);
    click(&mut ed, p);
    assert!(marked.iter().all(|v| road(&ed, *v) == 0), "erased");
    assert_eq!(road(&ed, beyond), left);
}

#[test]
fn the_terrain_brush_lists_the_regions_terrains_by_name_and_picture_and_a_click_paints_with_one() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-terrains");
    let mut ed = editor(&world);
    let types = ed.terrain_types();
    assert!(types.len() >= 32, "{} terrain types", types.len());
    assert_eq!(types[1].1, "Grassland");
    assert!(types.iter().all(|(_, _, tex)| tex.is_some()));
    ed.mode = lyceum_editor::app::Mode::Terrain;
    ed.panels.brush = Brush::Terrain(0);
    ed.frame();
    ed.frame();
    assert_eq!(ed.terrain_rects.len(), types.len());
    let img = capture(&mut ed, dir.path());
    // Grassland's picture is drawn in its cell, and it is green.
    let (_, r) = *ed.terrain_rects.iter().find(|(v, _)| *v == 1).unwrap();
    let mut sum = [0u32; 3];
    let mut n = 0;
    for y in 4..40 {
        for x in 4..40 {
            let [red, green, blue] = pixel(&img, r.min.x + x as f32, r.min.y + y as f32);
            sum[0] += u32::from(red);
            sum[1] += u32::from(green);
            sum[2] += u32::from(blue);
            n += 1;
        }
    }
    let [red, green, blue] = sum.map(|s| s / n);
    assert!(
        green > red && green > blue && green > 40,
        "{red} {green} {blue}"
    );
    click(&mut ed, (r.center().x, r.center().y));
    assert_eq!(ed.panels.brush, Brush::Terrain(1));
}

#[test]
fn the_scene_brush_lists_what_each_scene_grows_on_the_terrain_under_it_and_a_click_paints_with_one()
{
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-scenes");
    let mut ed = editor(&world);
    ed.wait_for_names().unwrap();
    let z = ed.session.ground(HOLTBURG, 120.0, 120.0).unwrap();
    ed.look_at(HOLTBURG, Vec3::new(120.0, 120.0, z), 40.0, 0.0, -60.0);
    ed.frame();
    ed.mode = lyceum_editor::app::Mode::Terrain;
    ed.panels.brush = Brush::Scene(0);
    ed.panels.radius = 24.0;
    ed.panels.scene_terrain = 999;
    let o = pick::block_origin(HOLTBURG);
    let p = ed
        .view()
        .unwrap()
        .project(Vec3::new(o.x + 120.0, o.y + 120.0, z))
        .unwrap();
    ed.queue(HostEvent::CursorMoved {
        x: f64::from(p.0),
        y: f64::from(p.1),
    });
    ed.frame();
    // The terrain under the brush's vertex is the one listed.
    let terrain = ed
        .session
        .vertex(0xA9 * 8 + 5, 0xB4 * 8 + 5)
        .unwrap()
        .terrain;
    assert_eq!(ed.panels.scene_terrain, terrain);
    let scenes = ed.scenes_of(terrain).to_vec();
    assert!(scenes.iter().any(|m| !m.is_empty()), "{scenes:?}");
    for _ in 0..12 {
        ed.frame();
    }
    assert_eq!(ed.scene_rects.len(), scenes.len());
    for models in &scenes {
        let words = ed.scene_words(models);
        assert_eq!(models.is_empty(), words == "Nothing grows", "{words}");
    }
    // The first scene that grows something shows its models' pictures.
    let (v, _) = scenes
        .iter()
        .enumerate()
        .find(|(_, m)| !m.is_empty())
        .unwrap();
    let shown: Vec<_> = ed
        .picture_rects
        .iter()
        .filter(|(m, _)| scenes[v].contains(m))
        .copied()
        .collect();
    assert!(!shown.is_empty());
    let img = capture(&mut ed, dir.path());
    let (_, r) = shown[0];
    let colors: std::collections::BTreeSet<[u8; 3]> = (0..30)
        .flat_map(|y| (0..30).map(move |x| (x, y)))
        .map(|(x, y)| pixel(&img, r.min.x + 3.0 + x as f32, r.min.y + 3.0 + y as f32))
        .collect();
    assert!(colors.len() > 10, "{} colors in the picture", colors.len());
    // Over the panels the list stays as it is; a click on a row paints with that scene.
    let (_, row) = ed.scene_rects[v];
    click(&mut ed, (row.min.x + 8.0, row.min.y + 6.0));
    assert_eq!(ed.panels.scene_terrain, terrain);
    assert!(ed.vertex_marks.is_empty());
    assert_eq!(ed.panels.brush, Brush::Scene(u16::try_from(v).unwrap()));
}

#[test]
fn the_map_shows_the_camera_on_the_world_a_click_flies_there_and_an_edit_redraws_its_landblock() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-map");
    let mut ed = editor(&world);
    ed.wait_for_names().unwrap();
    assert!(ed.world_map.is_some());
    ed.frame();
    ed.frame();
    assert!(ed.map_rect.is_some());
    // The camera's spot is marked where it is.
    let cam = ed.camera.position;
    let spot = ed.map_point(cam.x, cam.y).unwrap();
    let img = capture(&mut ed, dir.path());
    let [r, g, b] = pixel(&img, spot.x, spot.y);
    assert!(r > 200 && g < 90 && b < 90, "{r} {g} {b}");
    // A click on Yaraq flies the camera there, over the ground.
    let o = pick::block_origin(0x7D64);
    let target = (o.x + 96.0, o.y + 96.0);
    let p = ed.map_point(target.0, target.1).unwrap();
    click(&mut ed, (p.x, p.y));
    let c = ed.camera.position;
    let off = ((c.x - target.0).powi(2) + (c.y - target.1).powi(2)).sqrt();
    let per_pixel = 24.0 * 2048.0 / ed.map_rect.unwrap().width();
    assert!(off < 2.0 * per_pixel, "{off} m off, {per_pixel} m a pixel");
    let ground = ed.session.ground(0x7D64, c.x - o.x, c.y - o.y).unwrap();
    assert!(c.z >= ground + 10.0, "{} over {ground}", c.z);
    // Zoomed in on Holtburg, a terrain edit there is drawn on the map.
    ed.fly_to(
        pick::block_origin(HOLTBURG).x + 96.0,
        pick::block_origin(HOLTBURG).y + 96.0,
    )
    .unwrap();
    ed.panels.map_zoom = 64.0;
    ed.frame();
    assert!((ed.map_view.width() - 32.0).abs() < 1e-3);
    let h = pick::block_origin(HOLTBURG);
    let spot = ed.map_point(h.x + 48.0, h.y + 48.0).unwrap();
    let before_pixels = ed.world_map.as_ref().unwrap().block_pixels(HOLTBURG);
    let before = pixel(&capture(&mut ed, dir.path()), spot.x, spot.y);
    let water = ed
        .terrain_types()
        .into_iter()
        .find(|(_, n, _)| n == "Water deep sea")
        .unwrap()
        .0;
    ed.edit(&Edit::Brush {
        brush: Brush::Terrain(water),
        landblock: HOLTBURG,
        x: 48.0,
        y: 48.0,
        radius: 30.0,
    })
    .unwrap();
    ed.frame();
    assert_ne!(
        ed.world_map.as_ref().unwrap().block_pixels(HOLTBURG),
        before_pixels
    );
    let after = pixel(&capture(&mut ed, dir.path()), spot.x, spot.y);
    assert!(
        after[2] > after[0] && after != before,
        "{before:?} then {after:?}"
    );
}

#[test]
fn a_selected_building_is_moved_by_its_handles_with_its_cells_and_says_so_beside_it() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-building");
    let mut ed = editor(&world);
    let info = ed.session.statics(HOLTBURG).unwrap();
    let b = info.buildings[0].clone();
    let o = pick::block_origin(HOLTBURG);
    let at = Vec3::new(
        o.x + b.frame.origin.x,
        o.y + b.frame.origin.y,
        o.z + b.frame.origin.z,
    );
    ed.look_at(
        HOLTBURG,
        Vec3::new(b.frame.origin.x, b.frame.origin.y, b.frame.origin.z + 3.0),
        30.0,
        20.0,
        -25.0,
    );
    ed.frame();
    let p = ed
        .view()
        .unwrap()
        .project(Vec3::new(at.x, at.y, at.z + 3.0))
        .unwrap();
    click(&mut ed, p);
    assert_eq!(
        ed.selection,
        Some(Selection::Building {
            landblock: HOLTBURG,
            index: 0
        })
    );
    assert!(ed.handles().is_some(), "a building has handles");
    ed.frame();
    let note = ed.building_note.expect("the note beside it");
    let img = capture(&mut ed, dir.path());
    let colors: std::collections::BTreeSet<[u8; 3]> = (0..400)
        .flat_map(|x| (0..200).map(move |y| (x, y)))
        .map(|(x, y)| egui::pos2(note.min.x + x as f32, note.min.y + y as f32))
        .filter(|p| note.contains(*p))
        .map(|p| pixel(&img, p.x, p.y))
        .collect();
    assert!(
        colors.len() > 6,
        "words in the note: {} colors",
        colors.len()
    );
    // Dragged along its east arrow: the building and every cell inside it go together.
    let before = ed.session.cells(HOLTBURG);
    let doors: Vec<u16> = b.portals.iter().map(|p| p.other_cell_id).collect();
    let inside = lyceum_foundry::cells::building_cells(&doors, &before);
    let moved = drag_arrow(&mut ed, Handle::X, at);
    assert!(moved.abs() > 0.5, "moved {moved} m");
    let now = ed.session.statics(HOLTBURG).unwrap().buildings[0]
        .frame
        .origin;
    assert!(
        (now.x - b.frame.origin.x - moved).abs() < 0.05,
        "the building: {now:?}"
    );
    let after = ed.session.cells(HOLTBURG);
    for c in &inside {
        let dx = after[c].frame.origin.x - before[c].frame.origin.x;
        assert!((dx - moved).abs() < 0.05, "cell {c:04X} moved {dx}");
    }
    // One step of the history, undone as one.
    ed.edit(&Edit::Undo).unwrap();
    assert_eq!(ed.session.cells(HOLTBURG), before);
    // Nothing selected, no note.
    hold_key(&mut ed, 0x1B, 1);
    ed.frame();
    assert!(ed.selection.is_none() && ed.building_note.is_none());
}

#[test]
fn a_piece_is_placed_ahead_the_next_snapped_to_its_free_portal_both_drawn_things_stand_inside_and_a_body_walks_through(
) {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-dungeon");
    let mut ed = editor(&world);
    ed.mode = lyceum_editor::app::Mode::Dungeon;
    ed.wait_for_pieces();
    // The corridor retail builds the most cells of.
    let pieces = ed.kit.pieces.clone().unwrap();
    let i = pieces
        .iter()
        .enumerate()
        .filter(|(_, p)| p.portals == 2)
        .max_by_key(|(_, p)| p.cells)
        .map(|(i, _)| i)
        .unwrap();
    ed.kit.chosen = Some(i);
    ed.look_at(HOLTBURG, Vec3::new(120.0, 120.0, 90.0), 30.0, 0.0, -10.0);
    ed.frame();
    let before = capture(&mut ed, dir.path());
    // A click in the middle of the world: the piece ahead of the camera.
    let world_rect = ed.world_rect.unwrap();
    click(&mut ed, (world_rect.center().x, world_rect.center().y));
    let names = ed.session.cell_names(HOLTBURG);
    assert_eq!(names.len(), 1, "one cell placed: {:?}", ed.panels.message);
    // Its free portals; a click on the first joins the next piece there.
    let free = ed.free_portals(HOLTBURG);
    assert_eq!(free.len(), 2);
    let o = pick::block_origin(HOLTBURG);
    let m = free[0].opening.middle;
    let p = ed
        .view()
        .unwrap()
        .project(Vec3::new(o.x + m.x, o.y + m.y, o.z + m.z))
        .unwrap();
    click(&mut ed, p);
    let names = ed.session.cell_names(HOLTBURG);
    assert_eq!(names.len(), 2, "{:?}", ed.panels.message);
    let cells = ed.session.cells(HOLTBURG);
    let ids: Vec<u16> = names.keys().copied().collect();
    let (a, b) = (&cells[&ids[0]], &cells[&ids[1]]);
    assert!(a
        .portals
        .iter()
        .any(|p| p.flags & 5 == 1 && p.other_cell_id == u32::from(ids[1])));
    assert!(b
        .portals
        .iter()
        .any(|p| p.flags & 5 == 1 && p.other_cell_id == u32::from(ids[0])));
    assert!(a.visible_cells.contains(&ids[1]) && b.visible_cells.contains(&ids[0]));
    let free = ed.free_portals(HOLTBURG);
    assert_eq!(free.len(), 2, "the two ends left free");
    // The walk finds each reached and standing.
    let walk = ed.session.walk(HOLTBURG);
    assert!(
        walk.iter()
            .all(|f| !f.text.contains("falls") && !f.text.contains("reaches")),
        "{walk:#?}"
    );
    // From the flying camera, the free ends outlined in the selection's color.
    ed.frame();
    let after = capture(&mut ed, dir.path());
    let c = &free[1].opening.corners;
    let edge = Vec3::new(
        o.x + (c[0].x + c[1].x) / 2.0,
        o.y + (c[0].y + c[1].y) / 2.0,
        o.z + (c[0].z + c[1].z) / 2.0,
    );
    let q = ed.view().unwrap().project(edge).unwrap();
    assert!(
        difference(&before, &after, q, 3.0) > 10.0,
        "the free end's outline at {q:?}"
    );
    // From inside, as the client draws it: both pieces are in the drawn world, and a body in
    // the first stands in its cell, which is drawn.
    let env = ed.session.environment(a.environment).unwrap();
    let vs = &env.cells[usize::from(a.cell_struct)].vertex_array.vertices;
    #[allow(clippy::cast_precision_loss)]
    let n = vs.len() as f32;
    let mid = vs.iter().fold(Vec3::ZERO, |m, v| {
        let p = dereth_primitives::frame::localtoglobal(&a.frame, v.position);
        Vec3::new(m.x + p.x / n, m.y + p.y / n, m.z + p.z / n)
    });
    ed.camera.position = Vec3::new(o.x + mid.x, o.y + mid.y, o.z + mid.z);
    ed.set_walking(true).unwrap();
    for _ in 0..10 {
        ed.frame();
    }
    assert_eq!(
        ed.body_cell(),
        Some((u32::from(HOLTBURG) << 16) | u32::from(ids[0]))
    );
    let (resident, meshes) = ed.cells_drawn();
    for c in &ids {
        assert!(resident.contains(&((u32::from(HOLTBURG) << 16) | u32::from(*c))));
    }
    assert!(meshes > 0, "the body's cell is drawn");
    // Indoors, a click on a piece's floor stands a static in the cell the pointer's ray meets
    // first, and places a spawn there: the floor under either piece's middle, where it shows in
    // the world.
    let world_rect = ed.world_rect.unwrap();
    let (q, hit) = ids
        .iter()
        .find_map(|c| {
            let e = ed.session.environment(cells[c].environment)?;
            let vs = &e.cells[usize::from(cells[c].cell_struct)]
                .vertex_array
                .vertices;
            #[allow(clippy::cast_precision_loss)]
            let n = vs.len() as f32;
            let m = vs.iter().fold(Vec3::ZERO, |m, v| {
                let p = dereth_primitives::frame::localtoglobal(&cells[c].frame, v.position);
                Vec3::new(m.x + p.x / n, m.y + p.y / n, m.z + p.z / n)
            });
            let (_, f) = ed
                .session
                .cell_hit(HOLTBURG, m, Vec3::new(0.0, 0.0, -1.0))?;
            let q = ed
                .view()?
                .project(Vec3::new(o.x + f.x, o.y + f.y, o.z + f.z))?;
            if !world_rect.shrink(4.0).contains(egui::pos2(q.0, q.1)) {
                return None;
            }
            let r = ed.view()?.ray(q.0, q.1);
            let (hit, _) = ed.session.cell_hit(
                HOLTBURG,
                Vec3::new(r.origin.x - o.x, r.origin.y - o.y, r.origin.z - o.z),
                r.dir,
            )?;
            ids.contains(&hit).then_some((q, hit))
        })
        .expect("a piece's floor in view");
    ed.mode = lyceum_editor::app::Mode::Place;
    ed.model = Some(dereth_primitives::DataId(0x0200_040B));
    click(&mut ed, q);
    let cell = &ed.session.cells(HOLTBURG)[&hit];
    assert_eq!(cell.static_objects.len(), 1, "{:?}", ed.panels.message);
    assert_eq!(cell.static_objects[0].id.raw(), 0x0200_040B);
    ed.mode = lyceum_editor::app::Mode::Spawn;
    ed.panels.weenie = Some(lyceum_foundry::edit::Weenie::Symbol(
        "holtburg:guide".into(),
    ));
    click(&mut ed, q);
    let spawns = ed.session.spawns();
    assert!(
        spawns
            .iter()
            .any(|s| s.cell == (u32::from(HOLTBURG) << 16) | u32::from(hit)),
        "{:?}",
        ed.panels.message
    );
    // The body, turned toward the second piece and walked, goes through the portal between
    // them into it.
    ed.set_walking(false).unwrap();
    let middle = |c: &dereth_assets::world::EnvCell| {
        let e = ed.session.environment(c.environment).unwrap();
        let vs = &e.cells[usize::from(c.cell_struct)].vertex_array.vertices;
        #[allow(clippy::cast_precision_loss)]
        let n = vs.len() as f32;
        vs.iter().fold(Vec3::ZERO, |m, v| {
            let p = dereth_primitives::frame::localtoglobal(&c.frame, v.position);
            Vec3::new(m.x + p.x / n, m.y + p.y / n, m.z + p.z / n)
        })
    };
    let (ma, mb) = (middle(a), middle(b));
    ed.camera.position = Vec3::new(o.x + ma.x, o.y + ma.y, o.z + ma.z);
    ed.camera.yaw = dereth_primitives::num::math::atan2f(-(mb.x - ma.x), mb.y - ma.y);
    ed.set_walking(true).unwrap();
    for _ in 0..3 {
        ed.frame();
    }
    assert_eq!(
        ed.body_cell(),
        Some((u32::from(HOLTBURG) << 16) | u32::from(ids[0]))
    );
    hold_key(&mut ed, 0x57, 240);
    assert_eq!(
        ed.body_cell(),
        Some((u32::from(HOLTBURG) << 16) | u32::from(ids[1])),
        "the body at {:?}",
        body_at(&ed)
    );
}

#[test]
fn the_piece_browser_draws_each_piece_s_floor_plan_finds_one_by_its_label_and_a_click_chooses_it() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-pieces");
    let mut ed = editor(&world);
    ed.mode = lyceum_editor::app::Mode::Dungeon;
    // The steps folded away, for room to show the pieces in the small window.
    ed.dungeon_list.steps = false;
    ed.wait_for_pieces();
    ed.frame();
    ed.frame();
    let rects = ed.kit.piece_rects.clone();
    assert!(rects.len() >= 2, "{} pieces shown", rects.len());
    // Each drawn: its picture (or its floor plan until the picture is drawn), not an empty
    // square.
    let img = capture(&mut ed, dir.path());
    let (_, r) = rects[0];
    let middle = egui::pos2(r.center().x, r.min.y + 4.0 + (r.width() - 16.0) / 2.0);
    let colors: std::collections::BTreeSet<[u8; 3]> = (0..40)
        .flat_map(|x| (0..24).map(move |y| (x, y)))
        .map(|(x, y)| pixel(&img, middle.x - 20.0 + x as f32, middle.y - 12.0 + y as f32))
        .collect();
    assert!(colors.len() > 3, "the piece: {} colors", colors.len());
    // Typed: the corridor alone.
    let pieces = ed.kit.pieces.clone().unwrap();
    let corridor = pieces
        .iter()
        .filter(|p| p.portals == 2)
        .max_by_key(|p| p.cells)
        .unwrap()
        .label();
    ed.kit.filter = corridor.clone();
    ed.frame();
    let shown = ed.kit.piece_rects.clone();
    assert_eq!(shown.len(), 1);
    assert_eq!(pieces[shown[0].0].label(), corridor);
    click(&mut ed, (shown[0].1.center().x, shown[0].1.center().y));
    assert_eq!(ed.kit.chosen, Some(shown[0].0));
}

/// Two of the server's weenies, as its SQL names them: a door and a creature.
const SERVER_WEENIES: &str = "\
INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES (90004, 'doorplain', 19, '2005-02-09 10:00:00')
     , (90005, 'drudgeskulker', 10, '2005-02-09 10:00:00');
INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES (90004, 1, 'Plain Door'), (90005, 1, 'Drudge Skulker');
INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES (90004, 1, 0x02000DD2), (90005, 1, 0x020007DD);
";

#[test]
fn the_spawn_browser_lists_every_weenie_by_name_kind_and_picture_and_places_the_one_clicked() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-weenies");
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, SERVER_WEENIES).unwrap();
    let mut ed = editor_with(&world, &["--weenies", sql.to_str().unwrap()]);
    ed.wait_for_names().unwrap();
    // The pack's guide first, then the server's by name.
    let names: Vec<&str> = ed.weenies.iter().map(|w| w.name.as_str()).collect();
    assert_eq!(
        names,
        ["Gribble the Painted", "Drudge Skulker", "Plain Door"]
    );
    let kinds: Vec<Category> = ed.weenies.iter().map(|w| w.category).collect();
    assert_eq!(kinds, [Category::Npc, Category::Creature, Category::Door]);
    ed.mode = lyceum_editor::app::Mode::Spawn;
    ed.panels.weenie_filter = "DOOR".into();
    assert_eq!(ed.spawn_list(), [2]);
    ed.panels.weenie_filter = "90005".into();
    assert_eq!(ed.spawn_list(), [1]);
    ed.panels.weenie_filter.clear();
    ed.panels.weenie_category = Some(Category::Npc);
    assert_eq!(ed.spawn_list(), [0]);
    ed.panels.weenie_category = None;
    for _ in 0..3 {
        ed.frame();
    }
    // Each is drawn with its setup's picture; a click chooses the door.
    assert_eq!(ed.weenie_rects.len(), 3);
    let img = capture(&mut ed, dir.path());
    let (_, r) = ed.weenie_rects[1].clone();
    let colors: std::collections::BTreeSet<[u8; 3]> = (0..40)
        .flat_map(|y| (0..40).map(move |x| (x, y)))
        .map(|(x, y)| pixel(&img, r.min.x + 4.0 + x as f32, r.min.y + 4.0 + y as f32))
        .collect();
    assert!(colors.len() > 20, "{} colors in the drudge", colors.len());
    let (door, r) = ed.weenie_rects[2].clone();
    click(&mut ed, (r.center().x, r.center().y));
    assert_eq!(door, lyceum_foundry::edit::Weenie::Class(90004));
    assert_eq!(ed.panels.weenie, Some(door));
    // Placed on the ground, it is drawn with the door's setup.
    let z = ed.session.ground(HOLTBURG, 60.0, 60.0).unwrap();
    ed.look_at(HOLTBURG, Vec3::new(60.0, 60.0, z + 1.0), 12.0, 0.0, -20.0);
    ed.frame();
    let o = pick::block_origin(HOLTBURG);
    let ground = ed
        .view()
        .unwrap()
        .project(Vec3::new(o.x + 60.0, o.y + 60.0, z))
        .unwrap();
    click(&mut ed, ground);
    let placed = ed
        .session
        .spawns()
        .into_iter()
        .find(|s| s.weenie == "90004")
        .expect("the door's spawn");
    assert_eq!(placed.setup, Some(dereth_primitives::DataId(0x0200_0DD2)));
}

/// A server drudge standing in Holtburg's open west side, as a dump writes it.
const SERVER_SPAWN: &str = "\
INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES (90005, 'drudgeskulker', 10, '2005-02-09 10:00:00');
INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES (90005, 1, 'Drudge Skulker');
INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES (90005, 1, 0x020007DD);
INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`)
VALUES (0x7A9B4050, 90005, 0xA9B40011, 60, 60, 0, 1, 0, 0, 0, False, '2019-05-16 00:00:00');
";

#[test]
fn the_servers_own_spawn_is_drawn_and_moving_or_deleting_it_writes_the_packs_override_file() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-server-spawn");
    let z = {
        let s = lyceum_foundry::edit::Session::open(&world, &dereth_dat::testing::dat_dir(), None)
            .unwrap();
        s.ground(HOLTBURG, 60.0, 60.0).unwrap()
    };
    let sql = dir.path().join("server.sql");
    std::fs::write(
        &sql,
        SERVER_SPAWN.replace("60, 60, 0,", &format!("60, 60, {z},")),
    )
    .unwrap();
    let mut ed = editor_with(&world, &["--weenies", sql.to_str().unwrap()]);
    ed.look_at(HOLTBURG, Vec3::new(60.0, 60.0, z + 1.0), 12.0, 0.0, -20.0);
    for _ in 0..3 {
        ed.frame();
    }
    let server: Vec<_> = ed
        .session
        .spawns()
        .into_iter()
        .filter(|s| s.server)
        .collect();
    assert_eq!(server.len(), 1, "shown around the camera");
    let o = pick::block_origin(HOLTBURG);
    let at = ed
        .view()
        .unwrap()
        .project(Vec3::new(o.x + 60.0, o.y + 60.0, z + 1.0))
        .unwrap();
    // Drawn: the drudge's pixels are not the ground's.
    let drawn = capture(&mut ed, dir.path());
    let file = ed.session.override_file(HOLTBURG);
    // Selected and dragged east, it is written into the pack's override file.
    click(&mut ed, at);
    assert!(matches!(&ed.selection, Some(Selection::Spawn { file: f, .. }) if *f == file));
    let want = drag_arrow(&mut ed, Handle::X, Vec3::new(o.x + 60.0, o.y + 60.0, z));
    let moved = ed
        .session
        .spawns()
        .into_iter()
        .find(|s| s.guid == "0x7A9B4050")
        .unwrap();
    assert!(!moved.server && moved.file == file);
    assert!((moved.frame.origin.x - 60.0 - want).abs() < 0.01, "{want}");
    let after = capture(&mut ed, dir.path());
    assert!(
        difference(&drawn, &after, at, 6.0) > 10.0,
        "the drudge is no longer where it stood"
    );
    // Deleted, the file takes it away; saved, the file says so.
    hold_key(&mut ed, 0x2E, 1);
    assert!(!ed.session.spawns().iter().any(|s| s.guid == "0x7A9B4050"));
    ed.edit(&Edit::Save).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("DELETE FROM `landblock_instance` WHERE `guid` = 0x7A9B4050;"));
    assert!(
        !text.contains("INSERT INTO `landblock_instance` ("),
        "{text}"
    );
}

/// A few of the server's names, as `empyrean-import schema` writes them.
const SCHEMA: &str = r#"{"properties":{"int":{"25":"Level"},"string":{"1":"Name"},"did":{"1":"Setup","35":"DeathTreasureType"}},"enums":{"EmoteCategory":{"1":"Refuse","7":"Use"},"EmoteType":{"5":"Motion","10":"Tell"}}}"#;

/// Type `text` into the form field `key`: a click into it, everything in it chosen, the text,
/// and Enter.
fn type_into(ed: &mut EditorApp, key: &str, text: &str) {
    let r = ed
        .form_fields
        .iter()
        .find(|(k, _)| k == key)
        .unwrap_or_else(|| panic!("no field {key}"))
        .1;
    click(ed, (r.center().x, r.center().y));
    let k = |vk, pressed, t: Option<&str>| HostEvent::KeyboardInput {
        key: dereth_input::keys::Key::new(vk, 0),
        pressed,
        text: t.map(str::to_owned),
    };
    ed.queue(k(0x11, true, None));
    ed.queue(k(0x41, true, Some("\u{1}")));
    ed.queue(k(0x41, false, None));
    ed.queue(k(0x11, false, None));
    ed.frame();
    for c in text.chars() {
        ed.queue(k(0x30, true, Some(&c.to_string())));
        ed.queue(k(0x30, false, None));
    }
    ed.frame();
    ed.queue(k(0x0D, true, None));
    ed.queue(k(0x0D, false, None));
    ed.frame();
    ed.frame();
}

#[test]
fn the_weenie_form_shows_a_packs_weenie_by_the_servers_names_and_an_edit_is_one_step_that_redraws_it(
) {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-form");
    let schema = dir.path().join("schema.json");
    std::fs::write(&schema, SCHEMA).unwrap();
    let mut ed = editor_with(&world, &["--schema", schema.to_str().unwrap()]);
    // Every property, under Advanced.
    ed.panels.advanced = true;
    let guide = ed
        .session
        .spawns()
        .into_iter()
        .find(|s| s.file.ends_with("A9B4 guide.sql"))
        .unwrap();
    let o = pick::block_origin(HOLTBURG);
    let at = guide.frame.origin;
    ed.look_at(HOLTBURG, Vec3::new(at.x, at.y, at.z + 1.0), 8.0, 0.0, -15.0);
    ed.selection = Some(Selection::Spawn {
        file: guide.file.clone(),
        guid: guide.guid.clone(),
    });
    ed.show_weenie();
    for _ in 0..3 {
        ed.frame();
    }
    assert_eq!(
        ed.form_weenie(),
        Some(lyceum_foundry::edit::Weenie::Symbol(
            "holtburg:guide".into()
        ))
    );
    for key in ["string:1", "int:25", "did:1"] {
        assert!(ed.form_fields.iter().any(|(k, _)| k == key), "{key}");
    }
    let file = ed
        .session
        .weenie_file(&lyceum_foundry::edit::Weenie::Symbol(
            "holtburg:guide".into(),
        ))
        .unwrap()
        .0;
    // Typed into the Name field, the SQL takes it, as one step.
    type_into(&mut ed, "string:1", "Gribble the Bold");
    let text = ed.session.sql_text(&file).unwrap();
    assert!(text.contains("1, 'Gribble the Bold') /* Name */"), "{text}");
    assert_eq!(ed.session.history(), (1, 0));
    // So does the Level, set by the form's own edit.
    ed.form_edit(&lyceum_editor::app::FormEdit::Property {
        kind: lyceum_foundry::content::PropertyKind::Int,
        id: 25,
        value: Some("12".into()),
    })
    .unwrap();
    let text = ed.session.sql_text(&file).unwrap();
    assert!(text.contains("25,         12) /* Level */"), "{text}");
    // A new setup redraws the guide where it stands.
    let p = ed
        .view()
        .unwrap()
        .project(Vec3::new(o.x + at.x, o.y + at.y, at.z + 1.0))
        .unwrap();
    let before = capture(&mut ed, dir.path());
    ed.form_edit(&lyceum_editor::app::FormEdit::Property {
        kind: lyceum_foundry::content::PropertyKind::DataId,
        id: 1,
        value: Some("0x02000DD2".into()),
    })
    .unwrap();
    ed.frame();
    let moved = ed
        .session
        .spawns()
        .into_iter()
        .find(|s| s.guid == guide.guid)
        .unwrap();
    assert_eq!(moved.setup, Some(dereth_primitives::DataId(0x0200_0DD2)));
    let after = capture(&mut ed, dir.path());
    assert!(
        difference(&before, &after, p, 8.0) > 10.0,
        "the guide drawn anew"
    );
    // A value that is not one is refused, and nothing changes.
    assert!(ed
        .form_edit(&lyceum_editor::app::FormEdit::Property {
            kind: lyceum_foundry::content::PropertyKind::Int,
            id: 25,
            value: Some("twelve".into()),
        })
        .is_err());
    assert_eq!(ed.session.history(), (3, 0));
    // A spell added, its chance set, and taken away again.
    let spells = |ed: &mut EditorApp| {
        let text = ed.session.sql_text(&file).unwrap();
        let doc = lyceum_foundry::content::SqlDoc::parse(&text);
        doc.rows("weenie_properties_spell_book")
            .iter()
            .map(|r| {
                (
                    doc.cell(*r, "spell").unwrap(),
                    doc.cell(*r, "probability").unwrap(),
                )
            })
            .collect::<Vec<_>>()
    };
    use lyceum_editor::app::FormEdit;
    ed.form_edit(&FormEdit::AddRow {
        table: "weenie_properties_spell_book".into(),
        values: vec![("spell".into(), "1".into())],
    })
    .unwrap();
    assert_eq!(spells(&mut ed), [("1".to_owned(), "2".to_owned())]);
    ed.form_edit(&FormEdit::Cell {
        table: "weenie_properties_spell_book".into(),
        row: 0,
        column: "probability".into(),
        value: "2.5".into(),
    })
    .unwrap();
    assert_eq!(spells(&mut ed), [("1".to_owned(), "2.5".to_owned())]);
    ed.form_edit(&FormEdit::RemoveRow {
        table: "weenie_properties_spell_book".into(),
        row: 0,
    })
    .unwrap();
    assert!(spells(&mut ed).is_empty());
    for _ in 0..3 {
        ed.edit(&Edit::Undo).unwrap();
    }
    // Undone, it is the drudge again.
    ed.edit(&Edit::Undo).unwrap();
    let back = ed
        .session
        .spawns()
        .into_iter()
        .find(|s| s.guid == guide.guid)
        .unwrap();
    assert_eq!(back.setup, Some(dereth_primitives::DataId(0x0200_07DD)));
}

#[test]
fn a_server_class_copied_into_the_pack_is_edited_there_and_its_spawns_take_the_copy() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-copy-class");
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, SERVER_SPAWN).unwrap();
    let mut ed = editor_with(&world, &["--weenies", sql.to_str().unwrap()]);
    ed.look_at(HOLTBURG, Vec3::new(60.0, 60.0, 80.0), 12.0, 0.0, -20.0);
    ed.frame();
    let spawn = ed
        .session
        .spawns()
        .into_iter()
        .find(|s| s.server)
        .expect("the server's drudge");
    ed.selection = Some(Selection::Spawn {
        file: spawn.file.clone(),
        guid: spawn.guid.clone(),
    });
    let class = lyceum_foundry::edit::Weenie::Class(90005);
    assert_eq!(ed.form_weenie(), Some(class.clone()));
    assert!(ed.session.weenie_file(&class).is_none(), "the server's own");
    ed.copy_class(90005).unwrap();
    let (file, mine) = ed.session.weenie_file(&class).expect("the pack's copy");
    assert!(mine && file.ends_with("server/sql/weenies/90005 Drudge Skulker.sql"));
    let text = ed.session.sql_text(&file).unwrap();
    assert!(
        text.contains("VALUES (90005, 'drudgeskulker', 10,"),
        "{text}"
    );
    // The copy's setup is the spawn's.
    ed.form_edit(&lyceum_editor::app::FormEdit::Property {
        kind: lyceum_foundry::content::PropertyKind::DataId,
        id: 1,
        value: Some("0x02000DD2".into()),
    })
    .unwrap();
    let now = ed
        .session
        .spawns()
        .into_iter()
        .find(|s| s.guid == spawn.guid)
        .unwrap();
    assert!(now.server, "the spawn itself is the server's still");
    assert_eq!(now.setup, Some(dereth_primitives::DataId(0x0200_0DD2)));
    // The spawn browser shows the copy.
    let entry = ed.weenies.iter().find(|w| w.weenie == class).unwrap();
    assert!(entry.pack && entry.setup == Some(0x0200_0DD2));
    assert_eq!(ed.session.history(), (2, 0));
    ed.edit(&Edit::Save).unwrap();
    assert!(std::fs::read_to_string(&file)
        .unwrap()
        .contains("1, 0x02000DD2)"));
}

#[test]
fn a_new_weenie_is_made_from_a_server_class_and_placed_as_the_packs_own() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-new-weenie");
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, SERVER_SPAWN).unwrap();
    let mut ed = editor_with(&world, &["--weenies", sql.to_str().unwrap()]);
    assert!(ed.new_weenie("Bad Name", 90005, false).is_err());
    assert!(ed.new_weenie("skulker", 1, false).is_err(), "no such class");
    ed.new_weenie("skulker", 90005, false).unwrap();
    let w = lyceum_foundry::edit::Weenie::Symbol("holtburg:skulker".into());
    assert_eq!(ed.panels.weenie, Some(w.clone()));
    let (file, mine) = ed.session.weenie_file(&w).unwrap();
    assert!(mine && file.ends_with("server/sql/weenies/skulker.sql"));
    let text = ed.session.sql_text(&file).unwrap();
    assert!(
        text.contains("VALUES ({{wcid:skulker}}, 'skulker', 10,"),
        "{text}"
    );
    let entry = ed.weenies.iter().find(|e| e.weenie == w).unwrap();
    assert!(entry.pack && entry.setup == Some(0x0200_07DD));
    assert!(ed.new_weenie("skulker", 90005, true).is_err(), "taken");
    // Placed on the ground, it is drawn as the drudge it was made from.
    let z = ed.session.ground(HOLTBURG, 70.0, 70.0).unwrap();
    ed.look_at(HOLTBURG, Vec3::new(70.0, 70.0, z + 1.0), 12.0, 0.0, -20.0);
    ed.frame();
    ed.mode = lyceum_editor::app::Mode::Spawn;
    let o = pick::block_origin(HOLTBURG);
    let ground = ed
        .view()
        .unwrap()
        .project(Vec3::new(o.x + 70.0, o.y + 70.0, z))
        .unwrap();
    click(&mut ed, ground);
    let placed = ed
        .session
        .spawns()
        .into_iter()
        .find(|s| s.weenie == "{{wcid:skulker}}")
        .expect("its spawn");
    assert_eq!(placed.setup, Some(dereth_primitives::DataId(0x0200_07DD)));
}

/// A pack generator scattering within ten meters, and making one thing at its own place.
const GENERATOR: &str = "\
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:gen}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:gen}}, 'gen', 1, '2026-10-19 00:00:00');

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:gen}},  43,      10) /* GeneratorRadius */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:gen}},   1, 'Drudge Generator') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:gen}},   1, 0x020007DD) /* Setup */;

INSERT INTO `weenie_properties_generator` (`object_Id`, `probability`, `weenie_Class_Id`, `delay`, `init_Create`, `max_Create`, `when_Create`, `where_Create`, `stack_Size`, `palette_Id`, `shade`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`)
VALUES ({{wcid:gen}}, 1, {{wcid:guide}}, 180, 1, 1, 2, 4, NULL, NULL, NULL, 0xA9B40012, 80, 80, ZZ, 1, 0, 0, 0);
";

#[test]
fn a_selected_generator_shows_where_it_makes_things_and_its_ring_moves_with_it() {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-generator");
    let mut ed = editor(&world);
    let o = pick::block_origin(HOLTBURG);
    let z = ed.session.ground(HOLTBURG, 60.0, 60.0).unwrap();
    let z80 = ed.session.ground(HOLTBURG, 80.0, 80.0).unwrap();
    let file = world
        .parent()
        .unwrap()
        .join("holtburg/server/sql/weenies/gen.sql");
    ed.session
        .apply(&lyceum_foundry::edit::Op::WriteSql {
            file,
            text: Some(GENERATOR.replace("ZZ", &z80.to_string())),
        })
        .unwrap();
    ed.session
        .apply(&lyceum_foundry::edit::Op::PlaceSpawn {
            landblock: HOLTBURG,
            weenie: lyceum_foundry::edit::Weenie::Symbol("holtburg:gen".into()),
            origin: Vec3::new(60.0, 60.0, z),
            heading: 0.0,
        })
        .unwrap();
    ed.reload(&lyceum_foundry::edit::Touched {
        landblocks: [HOLTBURG].into_iter().collect(),
        sql: true,
    })
    .unwrap();
    let s = ed
        .session
        .spawns()
        .into_iter()
        .find(|s| s.weenie == "{{wcid:gen}}")
        .unwrap();
    ed.look_at(HOLTBURG, Vec3::new(65.0, 65.0, z), 45.0, 0.0, -50.0);
    ed.selection = Some(Selection::Spawn {
        file: s.file.clone(),
        guid: s.guid.clone(),
    });
    ed.frame();
    ed.frame();
    let marks = ed.generator_marks.clone();
    let (at, r) = marks.ring.expect("its ring");
    assert!((r - 10.0).abs() < 1e-3);
    let view = ed.view().unwrap();
    let want = view.project(Vec3::new(o.x + 60.0, o.y + 60.0, z)).unwrap();
    assert!(
        (at.0 - want.0).abs() < 1.5 && (at.1 - want.1).abs() < 1.5,
        "{at:?} {want:?}"
    );
    let place = view
        .project(Vec3::new(o.x + 80.0, o.y + 80.0, z80))
        .unwrap();
    assert_eq!(marks.places.len(), 1);
    let p = marks.places[0];
    assert!(
        (p.0 - place.0).abs() < 1.5 && (p.1 - place.1).abs() < 1.5,
        "{p:?} {place:?}"
    );
    // Dragged east, its ring goes with it; the place it makes things at stays.
    let moved = drag_arrow(&mut ed, Handle::X, Vec3::new(o.x + 60.0, o.y + 60.0, z));
    ed.frame();
    let after = ed.generator_marks.clone();
    let view = ed.view().unwrap();
    let z2 = ed.session.ground(HOLTBURG, 60.0 + moved, 60.0).unwrap();
    let want = view
        .project(Vec3::new(o.x + 60.0 + moved, o.y + 60.0, z2))
        .unwrap();
    let (at2, _) = after.ring.unwrap();
    assert!(
        (at2.0 - want.0).abs() < 2.0 && (at2.1 - want.1).abs() < 2.0,
        "{at2:?} {want:?}"
    );
    assert!((at2.0 - at.0).abs() > 5.0, "the ring moved on screen");
    let p2 = after.places[0];
    assert!((p2.0 - place.0).abs() < 1.5 && (p2.1 - place.1).abs() < 1.5);
}

#[test]
fn the_weenie_forms_emote_tree_adds_sets_and_actions_orders_them_and_takes_them_away() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-emotes");
    let schema = dir.path().join("schema.json");
    std::fs::write(&schema, SCHEMA).unwrap();
    let mut ed = editor_with(&world, &["--schema", schema.to_str().unwrap()]);
    let w = lyceum_foundry::edit::Weenie::Symbol("holtburg:guide".into());
    ed.panels.weenie = Some(w.clone());
    let file = ed.session.weenie_file(&w).unwrap().0;
    // Each set: its category, and each action's type and message.
    let tree = |ed: &mut EditorApp| {
        let text = ed.session.sql_text(&file).unwrap();
        let doc = lyceum_foundry::content::SqlDoc::parse(&text);
        doc.emote_sets()
            .iter()
            .map(|s| {
                (
                    doc.cell(s.emote, "category").unwrap(),
                    doc.actions(s)
                        .iter()
                        .map(|a| {
                            (
                                doc.cell(*a, "order").unwrap(),
                                doc.cell(*a, "type").unwrap(),
                                doc.cell(*a, "message").unwrap(),
                            )
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>()
    };
    let t = |o: &str, k: &str, m: &str| (o.to_owned(), k.to_owned(), m.to_owned());
    let start = tree(&mut ed);
    assert_eq!(start.len(), 1);
    assert_eq!(start[0].1.len(), 1);
    use lyceum_editor::app::FormEdit;
    let edit = |ed: &mut EditorApp, e: FormEdit| ed.form_edit(&e).unwrap();
    // An action by the server's name of its type, said, then made the first.
    edit(
        &mut ed,
        FormEdit::AddAction {
            set: 0,
            kind: "motion".into(),
        },
    );
    edit(
        &mut ed,
        FormEdit::Action {
            set: 0,
            action: 1,
            column: "message".into(),
            value: "Ahem".into(),
        },
    );
    edit(
        &mut ed,
        FormEdit::MoveAction {
            set: 0,
            from: 1,
            to: 0,
        },
    );
    let now = tree(&mut ed);
    assert_eq!(now[0].1[0], t("0", "5", "'Ahem'"));
    assert_eq!(now[0].1[1].0, "1");
    assert_eq!(now[0].1[1].1, "10");
    // A set of its own, by number, its chance set; then taken away with its action.
    edit(
        &mut ed,
        FormEdit::AddEmoteSet {
            category: "1".into(),
        },
    );
    edit(
        &mut ed,
        FormEdit::EmoteSet {
            set: 1,
            column: "probability".into(),
            value: "0.5".into(),
        },
    );
    edit(
        &mut ed,
        FormEdit::AddAction {
            set: 1,
            kind: "Tell".into(),
        },
    );
    let now = tree(&mut ed);
    assert_eq!(now.len(), 2);
    assert_eq!(now[1].0, "1");
    assert_eq!(now[1].1, [t("0", "10", "NULL")]);
    let text = ed.session.sql_text(&file).unwrap();
    assert!(text.contains("1 /* Refuse */"), "{text}");
    assert_eq!(
        text.matches("SET @parent_id = LAST_INSERT_ID();").count(),
        2
    );
    edit(&mut ed, FormEdit::RemoveEmoteSet { set: 1 });
    edit(&mut ed, FormEdit::RemoveAction { set: 0, action: 0 });
    assert_eq!(tree(&mut ed), start);
    // A type the server has no name for, by name, is refused.
    assert!(ed
        .form_edit(&FormEdit::AddAction {
            set: 0,
            kind: "Dance".into(),
        })
        .is_err());
}

/// A quest and a death treasure of the server's.
const SERVER_LOOT: &str = "\
INSERT INTO `quest` (`id`, `name`, `min_Delta`, `max_Solves`, `message`, `last_Modified`)
VALUES (1, 'HoltburgWell', 3600, 1, 'Drew from the well.', '2005-02-09 10:00:00');
INSERT INTO `treasure_death` (`id`, `treasure_Type`, `tier`, `loot_Quality_Mod`, `unknown_Chances`, `item_Chance`, `item_Min_Amount`, `item_Max_Amount`, `item_Treasure_Type_Selection_Chances`, `magic_Item_Chance`, `magic_Item_Min_Amount`, `magic_Item_Max_Amount`, `magic_Item_Treasure_Type_Selection_Chances`, `mundane_Item_Chance`, `mundane_Item_Min_Amount`, `mundane_Item_Max_Amount`, `mundane_Item_Type_Selection_Chances`, `last_Modified`)
VALUES (1, 4, 2, 0, 19, 100, 1, 2, 10, 50, 2, 3, 9, 100, 0, 0, 1, '2005-02-09 10:00:00');
";

#[test]
fn the_weenie_forms_quests_say_where_they_are_registered_and_its_treasure_says_what_it_drops() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-quests");
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, SERVER_LOOT).unwrap();
    let mut ed = editor_with(&world, &["--weenies", sql.to_str().unwrap()]);
    let w = lyceum_foundry::edit::Weenie::Symbol("holtburg:guide".into());
    ed.panels.weenie = Some(w.clone());
    let file = ed.session.weenie_file(&w).unwrap().0;
    use lyceum_editor::app::FormEdit;
    // Two quest actions: one names the server's quest, one a quest of no one's.
    for (n, quest) in [(1, "HoltburgWell@A9B4"), (2, "GribbleGreeted")] {
        ed.form_edit(&FormEdit::AddAction {
            set: 0,
            kind: "22".into(),
        })
        .unwrap();
        ed.form_edit(&FormEdit::Action {
            set: 0,
            action: n,
            column: "message".into(),
            value: quest.into(),
        })
        .unwrap();
    }
    let text = ed.session.sql_text(&file).unwrap();
    let named = lyceum_editor::app::quests_named(&lyceum_foundry::content::SqlDoc::parse(&text));
    assert_eq!(named, ["HoltburgWell", "GribbleGreeted"]);
    let (q, at) = ed.session.quest("HoltburgWell").expect("the server's");
    assert!(at.is_none() && q.min_delta == 3600 && q.max_solves == 1);
    assert!(ed.session.quest("GribbleGreeted").is_none());
    // Registered in the pack, then made a daily quest.
    ed.form_edit(&FormEdit::RegisterQuest {
        name: "GribbleGreeted".into(),
    })
    .unwrap();
    let (_, at) = ed.session.quest("GribbleGreeted").expect("the pack's");
    let qfile = at.expect("in the pack's file");
    assert!(qfile.ends_with("server/sql/quests/GribbleGreeted.sql"));
    ed.form_edit(&FormEdit::Quest {
        name: "GribbleGreeted".into(),
        column: "min_Delta".into(),
        value: "86400".into(),
    })
    .unwrap();
    assert_eq!(
        ed.session.quest("GribbleGreeted").unwrap().0.min_delta,
        86400
    );
    assert!(
        ed.form_edit(&FormEdit::Quest {
            name: "HoltburgWell".into(),
            column: "min_Delta".into(),
            value: "1".into(),
        })
        .is_err(),
        "the server's quest is not the pack's to change"
    );
    // Its death treasure, set by number, is the server's tier 2.
    ed.form_edit(&FormEdit::Property {
        kind: lyceum_foundry::content::PropertyKind::DataId,
        id: 35,
        value: Some("4".into()),
    })
    .unwrap();
    let t = ed.session.treasure(4).expect("the server's treasure");
    assert_eq!((t.tier, t.magic), (2, (50, 2, 3)));
    let text = ed.session.sql_text(&file).unwrap();
    assert!(text.contains("35, 0x00000004)"), "{text}");
}

#[test]
fn the_checks_list_what_the_weenie_names_that_the_world_does_not_have() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-content-checks");
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, SERVER_WEENIES).unwrap();
    let schema = dir.path().join("schema.json");
    std::fs::write(&schema, SCHEMA).unwrap();
    let mut ed = editor_with(
        &world,
        &[
            "--weenies",
            sql.to_str().unwrap(),
            "--schema",
            schema.to_str().unwrap(),
        ],
    );
    // Every property, under Advanced.
    ed.panels.advanced = true;
    ed.panels.weenie = Some(lyceum_foundry::edit::Weenie::Symbol(
        "holtburg:guide".into(),
    ));
    ed.frame();
    // Each fault's words, without the line it is on (the lines move as rows come and go).
    let faults = |ed: &EditorApp| -> Vec<String> {
        ed.faults
            .iter()
            .filter(|f| !f.warning)
            .map(|f| match f.text.split_once(".sql:") {
                Some((file, rest)) => format!(
                    "{file}.sql: {}",
                    rest.split_once(": ").map_or(rest, |(_, m)| m)
                ),
                None => f.text.clone(),
            })
            .collect()
    };
    let before = faults(&ed);
    use lyceum_editor::app::FormEdit;
    // A class the world has is fine; one it has not, a spell the table has not, and a
    // property the server has no name for are faults, each naming the guide's file.
    for (table, key, value) in [
        ("weenie_properties_create_list", "weenie_Class_Id", "90004"),
        ("weenie_properties_create_list", "weenie_Class_Id", "999999"),
        ("weenie_properties_spell_book", "spell", "99999"),
    ] {
        ed.form_edit(&FormEdit::AddRow {
            table: table.into(),
            values: vec![(key.into(), value.into())],
        })
        .unwrap();
    }
    ed.form_edit(&FormEdit::Property {
        kind: lyceum_foundry::content::PropertyKind::Int,
        id: 777,
        value: Some("1".into()),
    })
    .unwrap();
    let new: Vec<String> = faults(&ed)
        .into_iter()
        .filter(|f| !before.contains(f))
        .collect();
    assert_eq!(new.len(), 3, "{new:#?}");
    assert!(new.iter().all(|f| f.contains("guide.sql: ")));
    assert!(new.iter().any(|f| f.contains("names class 999999")));
    assert!(new
        .iter()
        .any(|f| f.contains("spell 99999 is not in the spell table")));
    assert!(new
        .iter()
        .any(|f| f.contains("property 777 is not one the server has")));
    assert!(!new.iter().any(|f| f.contains("90004")));
    // A click on one of them opens the guide's form.
    ed.panels.weenie = None;
    ed.frame();
    // (The first of the guide's findings the list shows whole.)
    let r = (0..ed.faults.len())
        .filter(|i| ed.faults[*i].text.contains("guide.sql:"))
        .filter_map(|i| ed.fault_rect(i))
        .find(|r| r.min.y > 0.0 && r.max.y < 570.0)
        .expect("one of the guide's findings shown");
    click(&mut ed, (r.min.x + 20.0, r.center().y));
    assert_eq!(
        ed.panels.weenie,
        Some(lyceum_foundry::edit::Weenie::Symbol(
            "holtburg:guide".into()
        ))
    );
    ed.frame();
    assert!(
        ed.form_fields.iter().any(|(k, _)| k == "string:1"),
        "its form is shown"
    );
    // Undone, they are gone.
    for _ in 0..4 {
        ed.edit(&Edit::Undo).unwrap();
    }
    assert_eq!(faults(&ed), before);
}

#[test]
fn a_resized_window_lays_the_panels_and_the_world_view_out_again_at_its_new_size() {
    let _g = gpu_lock();
    let (dir, world) = sample("editor-resize");
    let mut ed = editor(&world);
    let stone = stone_west(&mut ed);
    let before = ed.world_rect.expect("the world view");
    let small = capture(&mut ed, dir.path());
    let at_before = ed.view().unwrap().project(stone).unwrap();
    ed.resize(1280, 800);
    for _ in 0..3 {
        ed.frame();
    }
    let after = ed.world_rect.expect("the world view");
    let big = capture(&mut ed, dir.path());
    let at_after = ed.view().unwrap().project(stone).unwrap();
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("resize").join("1280x800.png"),
    )
    .unwrap();
    println!("before {before:?} stone {at_before:?}, after {after:?} stone {at_after:?}");
    assert_eq!((small.0, small.1, big.0, big.1), (800, 600, 1280, 800));
    // The panels are docked by share, so each keeps its share of the larger window and the world
    // view grows with it.
    assert!(after.max.x <= 1280.0 && after.max.y <= 800.0);
    assert!(after.width() > 1.5 * before.width() && after.height() > 1.3 * before.height());
    // The view's projection follows the new size: what the camera looks at stays in the middle
    // of the window across, where the world is drawn under the panels.
    assert!((at_before.0 - 400.0).abs() < 2.0 && (at_after.0 - 640.0).abs() < 2.0);
}

/// A Holtburg building's room with a doorway out: the room's cell id, the doorway's middle and
/// the way out of it, in landblock meters.
fn holtburg_doorway() -> (u32, Vec3, Vec3) {
    use dereth_assets::world::EnvCell;
    use dereth_assets::{Decode, Environment};
    use dereth_dat::DbType;
    let store = dereth_dat::testing::open_store_or_fail();
    for index in 0x0100..0x0200u32 {
        let id = dereth_primitives::DataId(0xA9B4_0000 | index);
        let Ok(bytes) = store.read_typed(DbType::Cell, id) else {
            break;
        };
        let cell = EnvCell::decode_payload(id, &bytes).unwrap();
        if cell.flags & 1 == 0 {
            continue;
        }
        let env = Environment::decode_payload(
            cell.environment,
            &store
                .read_typed(DbType::Environment, cell.environment)
                .unwrap(),
        )
        .unwrap();
        for (slot, p) in cell.portals.iter().enumerate() {
            if p.other_cell_id == 0xFFFF_FFFF {
                if let Some(o) =
                    lyceum_foundry::pieces::opening(&env, cell.cell_struct, slot, &cell.frame)
                {
                    if o.out.z.abs() < 0.3 {
                        return (id.0, o.middle, o.out);
                    }
                }
            }
        }
    }
    panic!("no doorway out of a Holtburg room");
}

#[test]
fn a_buildings_room_is_drawn_through_its_doorway_from_outside_and_around_the_camera_inside_it() {
    let _g = gpu_lock();
    let (dir, world) = sample("editor-interiors");
    let mut ed = editor(&world);
    let (room, middle, out) = holtburg_doorway();
    // Looking in at the doorway from 5 m outside it.
    let yaw = dereth_primitives::num::math::atan2f(out.x, -out.y).to_degrees();
    ed.look_at(HOLTBURG, middle, 5.0, yaw, -5.0);
    for _ in 0..4 {
        ed.frame();
    }
    assert_eq!(ed.camera_cell(), None);
    let seen = ed.cells_seen();
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("interiors").join("outside.png"),
    )
    .unwrap();
    let through = seen.get(&room).copied().unwrap_or(0.0);
    assert!(
        through > 2_000.0,
        "the room through its doorway: {through} px; {seen:?}"
    );
    // Flown in, 2.5 m past the doorway, looking on into the room.
    let inside = Vec3::new(middle.x - 2.5 * out.x, middle.y - 2.5 * out.y, middle.z);
    ed.look_at(HOLTBURG, inside, 0.1, yaw, -5.0);
    for _ in 0..4 {
        ed.frame();
    }
    let here = ed.camera_cell().expect("the camera is in a room");
    let seen = ed.cells_seen();
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("interiors").join("inside.png"),
    )
    .unwrap();
    assert!(here.1, "a building's room is seen from outside");
    // The room the camera is in is drawn around it, over the whole screen.
    let around = seen.get(&here.0 .0).copied().unwrap_or(0.0);
    assert!(
        around > 0.9 * 800.0 * 600.0,
        "the room around the camera: {around} px; {seen:?}"
    );
    let (_, meshes) = ed.cells_drawn();
    assert!(meshes > 0, "the room's walls are drawn");
}

#[test]
fn the_nearby_list_names_what_stands_round_the_camera_nearest_first_and_a_click_flies_to_one() {
    use lyceum_editor::app::{NearbyKind, NearbySort};
    let _g = gpu_lock();
    let (dir, world) = sample("editor-nearby");
    let mut ed = editor(&world);
    stone_west(&mut ed);
    ed.wait_for_names().unwrap();
    // The Nearby tab brought forward, and a frame to gather the list and draw it.
    ed.show_tab(lyceum_editor::app::Tab::Nearby);
    let started = std::time::Instant::now();
    let all = ed.nearby_entries();
    println!("gathered {} in {:?}", all.len(), started.elapsed());
    for _ in 0..31 {
        ed.frame();
    }
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("nearby").join("list.png"),
    )
    .unwrap();
    let shown: Vec<_> = ed.nearby.shown().into_iter().cloned().collect();
    // The pack's three stones, Holtburg's own statics and buildings, and the pack's guide.
    let stone = shown
        .iter()
        .find(|e| e.name == "stone-west")
        .expect("the west stone");
    assert_eq!(stone.kind, NearbyKind::Static);
    assert!((stone.distance - 22.0).abs() < 1.0, "{}", stone.distance);
    for k in [NearbyKind::Static, NearbyKind::Building, NearbyKind::Spawn] {
        assert!(shown.iter().any(|e| e.kind == k), "{k:?}");
    }
    assert!(shown.iter().any(|e| e.name == "Gribble the Painted"));
    assert!(
        shown.windows(2).all(|w| w[0].distance <= w[1].distance),
        "nearest first"
    );
    assert!(shown
        .iter()
        .all(|e| e.distance <= lyceum_editor::app::NEARBY_REACH));
    // By name, and only the spawns.
    ed.nearby.sort = NearbySort::Name;
    let names: Vec<String> = ed
        .nearby
        .shown()
        .iter()
        .map(|e| e.name.to_lowercase())
        .collect();
    assert!(names.windows(2).all(|w| w[0] <= w[1]));
    ed.nearby.kinds.insert(NearbyKind::Spawn);
    assert!(ed
        .nearby
        .shown()
        .iter()
        .all(|e| e.kind == NearbyKind::Spawn));
    ed.nearby.kinds.clear();
    ed.nearby.sort = NearbySort::Distance;
    ed.nearby.filter = "stone-east".into();
    ed.frame();
    // A click on it selects it and flies there.
    let (_, row) = ed.nearby.rects[0];
    click(&mut ed, (row.left() + 12.0, row.center().y));
    let east = ed.nearby.shown()[0].clone();
    assert_eq!(ed.selection, east.select);
    let at = pick::block_origin(HOLTBURG);
    let there = Vec3::new(at.x + east.at.x, at.y + east.at.y, at.z + east.at.z);
    let eye = ed.eye();
    let d =
        ((eye.x - there.x).powi(2) + (eye.y - there.y).powi(2) + (eye.z - there.z).powi(2)).sqrt();
    assert!(d < 20.0, "the camera is {d} m from it");
    // The list follows the camera: the stone is now as far as the camera stands off it.
    ed.nearby.filter.clear();
    ed.frame();
    let now = ed
        .nearby
        .shown()
        .into_iter()
        .find(|e| e.name == "stone-east")
        .cloned()
        .unwrap();
    assert!((now.distance - 14.0).abs() < 1.0, "{}", now.distance);
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("nearby").join("flown.png"),
    )
    .unwrap();
}

/// The server's database for the dungeon tests: a blank portal, and a portal into the Drudge
/// Hideout that names it.
const SERVER_PORTALS: &str = "\
INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES (371, 'portalblank', 7, '2005-02-09 10:00:00');
INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES (371, 1, 'Portal');
INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES (371, 1, 0x020001B3);
INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES (90010, 'portaldrudgehideout', 7, '2005-02-09 10:00:00');
INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES (90010, 1, 'Drudge Hideout');
INSERT INTO `weenie_properties_position` (`object_Id`, `position_Type`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`)
VALUES (90010, 2, 0x019E0114, 40, -40, 0, 1, 0, 0, 0);
";

#[test]
fn the_dungeon_list_names_a_dungeon_by_its_portal_and_enter_flies_into_its_first_cell_drawn_round_the_camera(
) {
    let _g = gpu_lock();
    let (dir, world) = sample("editor-dungeon-list");
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, SERVER_PORTALS).unwrap();
    let mut ed = editor_with(&world, &["--weenies", sql.to_str().unwrap()]);
    let all = ed.dungeons();
    assert!(all.len() > 1_000, "{}", all.len());
    let hideout = all
        .iter()
        .find(|d| d.landblock == 0x019E)
        .expect("the hideout");
    assert_eq!(ed.dungeon_name(hideout), "Drudge Hideout");
    let other = all.iter().find(|d| d.name.is_none()).unwrap();
    assert_eq!(
        ed.dungeon_name(other),
        format!("Dungeon {:04X}", other.landblock)
    );
    // Into it: the camera in its first cell, the cell drawn round it, in dungeon mode. Its rooms
    // lie south of its landblock's square.
    ed.enter_dungeon(0x019E, false).unwrap();
    assert_eq!(ed.mode, lyceum_editor::app::Mode::Dungeon);
    for _ in 0..4 {
        ed.frame();
    }
    let (cell, seen) = ed.camera_cell().expect("in a room");
    assert_eq!((cell.0, seen), (0x019E_0100, false));
    let around = ed.cells_seen().get(&0x019E_0100).copied().unwrap_or(0.0);
    assert!(around > 0.9 * 800.0 * 600.0, "{around}");
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("dungeons").join("entered.png"),
    )
    .unwrap();
    // To edit it: select mode, with the Nearby list of its cells, statics and spawns.
    ed.enter_dungeon(0x019E, true).unwrap();
    assert_eq!(ed.mode, lyceum_editor::app::Mode::Select);
    for _ in 0..2 {
        ed.frame();
    }
    use lyceum_editor::app::NearbyKind;
    let here = |k: NearbyKind| {
        ed.nearby
            .entries
            .iter()
            .filter(|e| e.landblock == 0x019E && e.kind == k)
            .count()
    };
    assert!(
        here(NearbyKind::Cell) > 5 && here(NearbyKind::Static) > 5,
        "{:?}",
        ed.nearby.entries.len()
    );
    // A click on a cell of it in the list goes into that cell.
    ed.show_tab(lyceum_editor::app::Tab::Nearby);
    ed.nearby.kinds.insert(NearbyKind::Cell);
    ed.nearby.sort = lyceum_editor::app::NearbySort::Name;
    ed.frame();
    let (i, row) = ed.nearby.rects[1];
    let target = ed.nearby.shown()[i].clone();
    click(&mut ed, (row.left() + 12.0, row.center().y));
    let want = (u32::from(target.landblock) << 16) | u32::from(target.cell.unwrap());
    assert_eq!(ed.camera_cell().map(|c| c.0 .0), Some(want));
    ed.nearby.kinds.clear();
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("dungeons").join("edit.png"),
    )
    .unwrap();
    // The dungeon panel, with its steps, and the list of dungeons.
    ed.mode = lyceum_editor::app::Mode::Dungeon;
    ed.show_tab(lyceum_editor::app::Tab::Dungeons);
    ed.dungeon_list.filter = "drudge".into();
    for _ in 0..2 {
        ed.frame();
    }
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("dungeons").join("panel.png"),
    )
    .unwrap();
    // A click on its Enter button does the same.
    ed.fly_to(0xA9 as f32 * 192.0 + 96.0, 0xB4 as f32 * 192.0 + 96.0)
        .unwrap();
    ed.mode = lyceum_editor::app::Mode::Dungeon;
    ed.frame();
    let (_, enter, _) = ed
        .dungeon_list
        .rects
        .iter()
        .find(|(lb, ..)| *lb == 0x019E)
        .copied()
        .expect("its row");
    click(&mut ed, (enter.center().x, enter.center().y));
    assert_eq!(ed.dungeon_block, Some(0x019E));
    assert_eq!(ed.camera_cell().map(|c| c.0 .0), Some(0x019E_0100));
}

#[test]
fn a_new_dungeon_starts_in_an_empty_landblock_is_walked_and_a_portal_outdoors_leads_into_its_first_cell(
) {
    let _g = gpu_lock();
    let (dir, world) = sample("editor-new-dungeon");
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, SERVER_PORTALS).unwrap();
    let mut ed = editor_with(&world, &["--weenies", sql.to_str().unwrap()]);
    let lb = ed.new_dungeon().unwrap();
    assert!(ed.session.cell_names(lb).is_empty());
    assert!(ed
        .session
        .statics(lb)
        .is_none_or(|i| i.num_cells == 0 && i.buildings.is_empty()));
    assert_eq!(ed.mode, lyceum_editor::app::Mode::Dungeon);
    // The first piece, ahead of the camera, and one more snapped to it.
    ed.wait_for_pieces();
    let pieces = ed.kit.pieces.clone().unwrap();
    let corridor = pieces
        .iter()
        .filter(|p| p.portals == 2)
        .max_by_key(|p| p.cells)
        .unwrap()
        .label();
    ed.step(&lyceum_editor::script::Step {
        piece: Some(corridor),
        extend: Some(2),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(ed.session.cell_names(lb).len(), 2, "{}", ed.panels.message);
    let walk = ed.session.walk(lb);
    assert!(
        walk.iter()
            .all(|f| !f.text.contains("falls") && !f.text.contains("reaches")),
        "{walk:#?}"
    );
    // Back outdoors in Holtburg, a portal in: a pack weenie whose destination is the dungeon's
    // first cell, and its spawn on the ground ahead of the camera.
    let z = ed.session.ground(HOLTBURG, 120.0, 120.0).unwrap();
    ed.look_at(HOLTBURG, Vec3::new(120.0, 120.0, z + 2.0), 10.0, 0.0, -10.0);
    ed.mode = lyceum_editor::app::Mode::Select;
    ed.frame();
    ed.portal_to_dungeon(lb).unwrap();
    let w = lyceum_foundry::edit::Weenie::Symbol(format!("holtburg:portal-{lb:04x}"));
    let (file, mine) = ed.session.weenie_file(&w).unwrap();
    assert!(mine);
    let text = ed.session.sql_text(&file).unwrap();
    let first = ed.session.cell_names(lb).keys().copied().min().unwrap();
    let doc = lyceum_foundry::content::SqlDoc::parse(&text);
    let dest = doc
        .rows("weenie_properties_position")
        .into_iter()
        .find(|r| doc.cell(*r, "position_Type").as_deref() == Some("2"))
        .expect("a destination");
    let cell = lyceum_foundry::sql_ids::number(&doc.cell(dest, "obj_Cell_Id").unwrap()).unwrap();
    assert_eq!(cell, (u32::from(lb) << 16) | u32::from(first), "{text}");
    let at = |c: &str| doc.cell(dest, c).unwrap().parse::<f32>().unwrap();
    let inside = ed
        .session
        .cell_shapes(lb)
        .into_iter()
        .find(|r| r.index == first)
        .unwrap();
    assert!(
        inside.holds(Vec3::new(
            at("origin_X"),
            at("origin_Y"),
            at("origin_Z") + 0.5
        )),
        "{text}"
    );
    assert!(text.contains("Portal'"), "named for the dungeon: {text}");
    let spawn = ed
        .session
        .spawns()
        .into_iter()
        .find(|s| s.weenie == format!("{{{{wcid:portal-{lb:04x}}}}}"))
        .expect("its spawn");
    assert_eq!(spawn.landblock(), HOLTBURG);
    assert!(spawn.cell & 0xFFFF < 0x0100, "outdoors");
    for _ in 0..2 {
        ed.frame();
    }
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("dungeons").join("portal.png"),
    )
    .unwrap();
}

/// The server's blanks the "New ..." flows start from: an NPC, a creature, an item, a door, a
/// portal and a chest, as plain as the server's own.
const SERVER_BLANKS: &str = "\
INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES (5773, 'npcblank', 10, '2005-02-09 10:00:00')
     , (7, 'creatureblank', 10, '2005-02-09 10:00:00')
     , (280, 'itemblank', 1, '2005-02-09 10:00:00')
     , (278, 'doorblank', 19, '2005-02-09 10:00:00')
     , (371, 'portalblank', 7, '2005-02-09 10:00:00')
     , (143, 'chestblank', 20, '2005-02-09 10:00:00');
INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES (5773, 1, 'Townsperson'), (7, 1, 'Drudge'), (280, 1, 'Trinket'), (278, 1, 'Door')
     , (371, 1, 'Portal'), (143, 1, 'Chest');
INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES (5773, 1, 0x02000001), (7, 1, 0x020007DD), (280, 1, 0x02000181), (278, 1, 0x02000DD2)
     , (371, 1, 0x020001B3), (143, 1, 0x0200040B);
INSERT INTO `treasure_death` (`treasure_Type`, `tier`, `loot_Quality_Mod`, `unknown_Chances`, `item_Chance`, `item_Min_Amount`, `item_Max_Amount`, `item_Treasure_Type_Selection_Chances`, `magic_Item_Chance`, `magic_Item_Min_Amount`, `magic_Item_Max_Amount`, `magic_Item_Treasure_Type_Selection_Chances`, `mundane_Item_Chance`, `mundane_Item_Min_Amount`, `mundane_Item_Max_Amount`, `mundane_Item_Type_Selection_Chances`, `last_Modified`)
VALUES (4, 1, 0, 19, 100, 1, 1, 8, 50, 0, 1, 8, 100, 0, 2, 8, '2005-02-09 10:00:00');
";

#[test]
fn each_new_flow_writes_its_kind_of_weenie_whose_sql_passes_the_checks_and_one_is_placed_here() {
    use lyceum_editor::app::Flow;
    let _g = gpu_lock();
    let (dir, world) = sample("editor-flows");
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, SERVER_BLANKS).unwrap();
    let mut ed = editor_with(&world, &["--weenies", sql.to_str().unwrap()]);
    stone_west(&mut ed);
    for flow in Flow::ALL {
        let name = format!("Test {}", flow.key().replace('_', " "));
        let w = ed
            .new_for(flow, &name)
            .unwrap_or_else(|e| panic!("{flow:?}: {e}"));
        let (file, mine) = ed.session.weenie_file(&w).unwrap();
        assert!(mine);
        let text = ed.session.sql_text(&file).unwrap();
        let doc = lyceum_foundry::content::SqlDoc::parse(&text);
        assert_eq!(
            doc.property(lyceum_foundry::content::PropertyKind::String, 1)
                .as_deref()
                .map(lyceum_foundry::content::unquote),
            Some(name.clone())
        );
        assert_eq!(Flow::of(&doc), flow, "{text}");
        // Its SQL passes every check the pack makes of its content.
        let stem = file.file_stem().unwrap().to_string_lossy().into_owned();
        let said: Vec<_> = ed
            .faults
            .iter()
            .filter(|f| f.text.contains(&stem))
            .collect();
        assert!(said.is_empty(), "{flow:?}: {said:#?}\n{text}");
        // The form by what it is for draws, and one is placed where the camera is.
        ed.frame();
        assert!(ed.spawns_of(&w).is_empty());
        ed.place_one_here(&w).unwrap();
        assert_eq!(ed.spawns_of(&w).len(), 1, "{flow:?}");
    }
    // A portal sent into a dungeon; a vendor's trade changed through its form.
    let portal = lyceum_foundry::edit::Weenie::Symbol("holtburg:test-portal".into());
    ed.panels.weenie = Some(portal.clone());
    ed.selection = None;
    ed.destination_dungeon(0x019E).unwrap();
    let (file, _) = ed.session.weenie_file(&portal).unwrap();
    assert!(ed.session.sql_text(&file).unwrap().contains("0x019E0100"));
    let vendor = lyceum_foundry::edit::Weenie::Symbol("holtburg:test-vendor".into());
    ed.panels.weenie = Some(vendor.clone());
    ed.form_edit(&lyceum_editor::app::FormEdit::Property {
        kind: lyceum_foundry::content::PropertyKind::Int,
        id: 74,
        value: Some("8".into()),
    })
    .unwrap();
    ed.show_weenie();
    // Room to see the whole form.
    ed.resize(1500, 1100);
    for _ in 0..3 {
        ed.frame();
    }
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("flows").join("vendor.png"),
    )
    .unwrap();
    let quest = lyceum_foundry::edit::Weenie::Symbol("holtburg:test-quest-giver".into());
    ed.panels.weenie = Some(quest);
    for _ in 0..2 {
        ed.frame();
    }
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("flows").join("quest-giver.png"),
    )
    .unwrap();
}

/// Hold key `vk` until the camera stops moving (no millimeter in ten frames), or for at most
/// `cap` frames: where it ended.
fn fly_until_still(ed: &mut EditorApp, vk: usize, cap: u32) -> Vec3 {
    let k = |pressed| HostEvent::KeyboardInput {
        key: dereth_input::keys::Key::new(vk, 0),
        pressed,
        text: None,
    };
    ed.queue(k(true));
    let mut trail = vec![ed.camera.position];
    for _ in 0..cap {
        ed.frame();
        trail.push(ed.camera.position);
        if trail.len() > 10 {
            let a = trail[trail.len() - 11];
            let b = ed.camera.position;
            if Vec3::new(b.x - a.x, b.y - a.y, b.z - a.z).magnitude() < 1e-3 {
                break;
            }
        }
    }
    ed.queue(k(false));
    ed.frame();
    ed.camera.position
}

#[test]
fn with_collide_on_the_flying_camera_stops_at_a_house_and_the_ground_and_flies_through_with_it_off()
{
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-collide");
    let mut ed = editor(&world);
    let o = pick::block_origin(HOLTBURG);
    // South of the house at (79.5, 37.5), a body's height over the ground, looking north at it.
    let z = ed.session.ground(HOLTBURG, 79.5, 20.0).unwrap();
    let start = Vec3::new(o.x + 79.5, o.y + 20.0, z + 1.5);
    let aim = |ed: &mut EditorApp| {
        ed.camera.position = start;
        ed.camera.yaw = 0.0;
        ed.camera.pitch = 0.0;
        ed.camera.speed = 4; // 25 m/s
        ed.frame();
    };
    aim(&mut ed);
    ed.collide = true;
    let stopped = fly_until_still(&mut ed, 0x57, 600);
    let y = stopped.y - o.y;
    assert!(
        y > 25.0 && y < 37.5 - 1.0,
        "stopped at the house's wall, south of its middle: y {y}"
    );
    assert!(
        (stopped.x - start.x).abs() < 0.5,
        "it slid no way: {stopped:?}"
    );
    let c = ed.collider.as_ref().expect("the camera's world");
    assert!(c.stopped > 0 && c.blocks.contains(&HOLTBURG), "{c:?}");
    // Off, the same flight goes into the house and through it.
    aim(&mut ed);
    ed.collide = false;
    let k = |pressed| HostEvent::KeyboardInput {
        key: dereth_input::keys::Key::new(0x57, 0),
        pressed,
        text: None,
    };
    ed.queue(k(true));
    for _ in 0..600 {
        ed.frame();
        if ed.camera.position.y - o.y > 37.5 + 2.0 {
            break;
        }
    }
    ed.queue(k(false));
    ed.frame();
    assert!(
        ed.camera.position.y - o.y > 37.5 + 2.0,
        "{:?}",
        ed.camera.position
    );
    // Down onto open ground with it on: it stops on the ground, never under it.
    let g = ed.session.ground(HOLTBURG, 120.0, 110.0).unwrap();
    ed.camera.position = Vec3::new(o.x + 120.0, o.y + 110.0, g + 3.0);
    ed.camera.pitch = -0.5;
    ed.collide = true;
    ed.frame();
    let down = fly_until_still(&mut ed, 0x10, 600);
    assert!(
        down.z > g + lyceum_editor::app::FLY_RADIUS - 0.05 && down.z < g + 1.0,
        "on the ground at {g}: {down:?}"
    );
}

/// A server drudge standing in the Drudge Hideout's first room, as a dump writes it; `{x}`,
/// `{y}` and `{z}` are filled with the room's floor.
const DUNGEON_SPAWN: &str = "\
INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES (90005, 'drudgeskulker', 10, '2005-02-09 10:00:00');
INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES (90005, 1, 'Drudge Skulker');
INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES (90005, 1, 0x020007DD);
INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`)
VALUES (0x719E0050, 90005, 0x019E0100, {x}, {y}, {z}, 1, 0, 0, 0, False, '2019-05-16 00:00:00');
";

#[test]
fn a_servers_spawn_in_a_dungeons_room_is_drawn_with_the_room_and_selected_by_a_click() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-dungeon-spawn");
    // The first room's middle, on its floor.
    let (x, y, z) = {
        let s = lyceum_foundry::edit::Session::open(&world, &dereth_dat::testing::dat_dir(), None)
            .unwrap();
        let cell = s.cells(0x019E).remove(&0x0100).unwrap();
        let o = cell.frame.origin;
        let (_, floor) = s
            .cell_at(0x019E, Vec3::new(o.x, o.y, o.z + 1.0))
            .expect("in the room");
        (o.x, o.y, floor)
    };
    let sql = dir.path().join("server.sql");
    std::fs::write(
        &sql,
        DUNGEON_SPAWN
            .replace("{x}", &x.to_string())
            .replace("{y}", &y.to_string())
            .replace("{z}", &z.to_string()),
    )
    .unwrap();
    let mut ed = editor_with(&world, &["--weenies", sql.to_str().unwrap()]);
    ed.enter_dungeon(0x019E, true).unwrap();
    for _ in 0..4 {
        ed.frame();
    }
    // Shown, drawn among the room's statics.
    let spawn = ed
        .session
        .spawns()
        .into_iter()
        .find(|s| s.guid == "0x719E0050")
        .expect("the hideout's spawns shown");
    assert!(spawn.server);
    let records = ed.session.scratch_records_in(Some(&[0x019E].into()));
    let bytes = &records[&dereth_dat::ModernDat::Cell][&0x019E_0100].bytes;
    let cell = <dereth_assets::world::EnvCell as dereth_assets::Decode>::decode_payload(
        dereth_primitives::DataId(0x019E_0100),
        bytes,
    )
    .unwrap();
    assert!(
        cell.static_objects
            .iter()
            .any(|o| o.id == dereth_primitives::DataId(0x0200_07DD)),
        "the drudge among the room's statics"
    );
    // A click on it selects it.
    ed.look_at(0x019E, Vec3::new(x, y, z + 1.0), 4.0, 0.0, -20.0);
    for _ in 0..3 {
        ed.frame();
    }
    let o = pick::block_origin(0x019E);
    let p = ed
        .view()
        .unwrap()
        .project(Vec3::new(o.x + x, o.y + y, o.z + z + 1.0))
        .unwrap();
    click(&mut ed, p);
    assert_eq!(
        ed.selection,
        Some(Selection::Spawn {
            file: spawn.file.clone(),
            guid: spawn.guid.clone()
        })
    );
    ed.frame();
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("dungeon-spawns").join("drudge.png"),
    )
    .unwrap();
}

#[test]
fn a_static_in_a_holtburg_room_is_selected_by_a_click_and_deleted_as_an_edit_of_the_room() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-room-static");
    let mut ed = editor(&world);
    // A furnished room, and of its statics of a hand's size to a body's the one nearest its
    // middle, aimed at at the middle of its shape.
    let (n, cell) = ed
        .session
        .cells(HOLTBURG)
        .into_iter()
        .find(|(_, c)| c.static_objects.len() >= 3)
        .unwrap();
    ed.fly_into_cell(HOLTBURG, n).unwrap();
    for _ in 0..3 {
        ed.frame();
    }
    let o = pick::block_origin(HOLTBURG);
    let middle = Vec3::new(
        ed.camera.position.x - o.x,
        ed.camera.position.y - o.y,
        ed.camera.position.z - o.z,
    );
    let mut models = pick::Models::default();
    let (k, at) = cell
        .static_objects
        .iter()
        .enumerate()
        .filter_map(|(k, s)| {
            let m = models.get(ed.session.base(), s.id)?;
            (0.3..1.5).contains(&m.radius).then(|| {
                (
                    k,
                    dereth_primitives::frame::localtoglobal(&s.frame, m.center),
                )
            })
        })
        .min_by(|a, b| {
            let d = |p: Vec3| Vec3::new(p.x - middle.x, p.y - middle.y, 0.0).magnitude();
            d(a.1).total_cmp(&d(b.1))
        })
        .unwrap();
    ed.look_at(HOLTBURG, at, 1.8, 0.0, -40.0);
    for _ in 0..3 {
        ed.frame();
    }
    let p = ed
        .view()
        .unwrap()
        .project(Vec3::new(o.x + at.x, o.y + at.y, o.z + at.z))
        .unwrap();
    click(&mut ed, p);
    let Some(Selection::CellStatic {
        landblock,
        cell: c,
        index,
    }) = ed.selection.clone()
    else {
        panic!("{:?}", ed.selection)
    };
    assert_eq!((landblock, c), (HOLTBURG, n));
    println!("aimed at static {k}, selected {index}");
    ed.frame();
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("room-static").join("selected.png"),
    )
    .unwrap();
    // Delete takes it away: the room holds one fewer, and the pack's file says which.
    hold_key(&mut ed, 0x2E, 1);
    let now = ed.session.cells(HOLTBURG).remove(&n).unwrap();
    assert_eq!(now.static_objects.len(), cell.static_objects.len() - 1);
    assert_eq!(ed.selection, None);
    ed.session.save().unwrap();
    let text =
        std::fs::read_to_string(dir.path().join("holtburg/edits/landblocks/A9B4.toml")).unwrap();
    assert!(
        text.contains("[[remove-cell-static]]")
            && text.contains(&format!("cell = 0x{n:04X}"))
            && text.contains(&format!("index = {index}")),
        "{text}"
    );
}

#[test]
fn the_piece_browser_groups_pieces_by_look_shows_each_in_a_dungeon_and_filters_by_shape_and_joins()
{
    use lyceum_foundry::pieces::Shape;
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-piece-looks");
    let mut ed = editor(&world);
    ed.resize(1280, 900);
    ed.mode = lyceum_editor::app::Mode::Dungeon;
    ed.dungeon_list.steps = false;
    ed.wait_for_pieces();
    for _ in 0..2 {
        ed.frame();
    }
    // Grouped by look: a header before the pieces, each piece under its own commonest look.
    assert!(!ed.kit.header_rects.is_empty());
    let pieces = ed.kit.pieces.clone().unwrap();
    let list = ed.piece_list();
    let looks: Vec<usize> = list.iter().map(|i| pieces[*i].looks[0].family).collect();
    assert!(looks.windows(2).all(|w| w[0] <= w[1]), "in look order");
    // Each shown piece's picture drawn in a dungeon that uses it.
    ed.wait_for_previews();
    ed.frame();
    let (drawn, read) = ed.previews_made();
    assert!(drawn + read >= 4, "{drawn} drawn, {read} read");
    let img = capture(&mut ed, dir.path());
    let (_, r) = ed.kit.piece_rects[0];
    let colors: std::collections::BTreeSet<[u8; 3]> = (0..60)
        .flat_map(|x| (0..44).map(move |y| (x, y)))
        .map(|(x, y)| pixel(&img, r.min.x + 14.0 + x as f32, r.min.y + 26.0 + y as f32))
        .collect();
    assert!(colors.len() > 40, "the picture: {} colors", colors.len());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("pieces").join("browser.png"),
    )
    .unwrap();
    // The corridor chosen: its card, and with Joins the chosen piece only what retail joins
    // it to.
    let c = pieces
        .iter()
        .enumerate()
        .filter(|(_, p)| p.portals == 2)
        .max_by_key(|(_, p)| p.cells)
        .map(|(i, _)| i)
        .unwrap();
    ed.kit.chosen = Some(c);
    ed.kit.joins_chosen = true;
    for _ in 0..2 {
        ed.frame();
    }
    let joined: std::collections::BTreeSet<usize> =
        pieces[c].joins.iter().map(|(i, _)| *i).collect();
    let shown = ed.piece_list();
    assert!(!shown.is_empty() && shown.iter().all(|i| joined.contains(i)));
    ed.wait_for_previews();
    ed.frame();
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("pieces").join("chosen.png"),
    )
    .unwrap();
    // By shape: the rooms alone, grouped by shape.
    ed.kit.joins_chosen = false;
    ed.kit.shape = Some(Shape::Room);
    ed.kit.group = lyceum_editor::app::PieceGroup::Shape;
    ed.frame();
    let rooms = ed.piece_list();
    assert!(!rooms.is_empty() && rooms.iter().all(|i| pieces[*i].shape == Shape::Room));
    // By look: the pieces drawn in the Drudge Hideout's look.
    ed.kit.shape = None;
    let hideout = ed
        .kit
        .families
        .iter()
        .position(|f| f.landblocks.iter().any(|(lb, _)| *lb == 0x019E))
        .unwrap();
    ed.kit.family = Some(hideout);
    ed.frame();
    let drawn_so = ed.piece_list();
    assert!(drawn_so
        .iter()
        .all(|i| pieces[*i].looks.iter().any(|l| l.family == hideout)));
    assert!(drawn_so.len() >= 10, "{}", drawn_so.len());
}

#[test]
fn inside_a_dungeon_its_cells_are_listed_with_their_pieces_and_a_cell_chosen_or_clicked_shows_its_piece(
) {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-dungeon-pieces");
    let mut ed = editor(&world);
    ed.resize(1280, 900);
    ed.dungeon_list.steps = false;
    ed.wait_for_pieces();
    ed.enter_dungeon(0x019E, false).unwrap();
    for _ in 0..3 {
        ed.frame();
    }
    // Chosen from the list: the cell selected, its piece chosen, the camera in it.
    ed.choose_cell(0x019E, 0x0105).unwrap();
    for _ in 0..3 {
        ed.frame();
    }
    assert_eq!(
        ed.selection,
        Some(Selection::Cell {
            landblock: 0x019E,
            cell: 0x0105
        })
    );
    let cell = ed.session.cells(0x019E).remove(&0x0105).unwrap();
    let pieces = ed.kit.pieces.clone().unwrap();
    let chosen = &pieces[ed.kit.chosen.unwrap()];
    assert_eq!(
        (chosen.environment, chosen.piece),
        (cell.environment, cell.cell_struct)
    );
    assert_eq!(ed.camera_cell().map(|c| c.0 .0), Some(0x019E_0105));
    ed.wait_for_previews();
    ed.frame();
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("pieces").join("dungeon-cells.png"),
    )
    .unwrap();
    // A click on the room's ceiling in the world selects the room and chooses its piece.
    ed.kit.chosen = None;
    ed.selection = None;
    ed.camera.pitch = 1.2;
    ed.frame();
    let w = ed.world_rect.expect("the world drawn");
    click(&mut ed, (w.center().x, w.center().y));
    let Some(Selection::Cell { landblock, cell: n }) = ed.selection.clone() else {
        panic!("{:?}", ed.selection)
    };
    assert_eq!(landblock, 0x019E);
    let c = ed.session.cells(0x019E).remove(&n).unwrap();
    let chosen = &pieces[ed.kit.chosen.expect("its piece chosen")];
    assert_eq!(
        (chosen.environment, chosen.piece),
        (c.environment, c.cell_struct)
    );
    // Reshaped: the dead end 0103 replaced by a piece that keeps its way and opens another, a
    // free portal to build on; then a piece built there.
    let old = ed.session.cells(0x019E).remove(&0x0103).unwrap();
    let old_env = ed.session.environment(old.environment).unwrap();
    let fits = |ed: &EditorApp, p: &lyceum_foundry::pieces::Piece| {
        let env = ed.session.environment(p.environment)?;
        lyceum_foundry::pieces::reshape_fit(&old, &old_env, &env, p.piece, 90.0)
    };
    let i = pieces
        .iter()
        .position(|p| p.portals == 2 && fits(&ed, p).is_some())
        .expect("a piece that takes the dead end's place");
    ed.kit.chosen = Some(i);
    ed.replace_cell(0x019E, 0x0103).unwrap();
    let reshaped = ed.session.cells(0x019E).remove(&0x0103).unwrap();
    assert_eq!(
        (reshaped.environment, reshaped.cell_struct),
        (pieces[i].environment, pieces[i].piece)
    );
    let free = ed.free_portals(0x019E);
    let end = free
        .iter()
        .find(|f| {
            matches!(
                f.link,
                lyceum_foundry::source::Link::Base { cell: 0x0103, .. }
            )
        })
        .expect("its free end")
        .opening
        .clone();
    let j = pieces
        .iter()
        .position(|p| {
            p.portals >= 2
                && ed.session.environment(p.environment).is_some_and(|e| {
                    (0..p.portals).any(|k| {
                        lyceum_foundry::pieces::snap(&e, p.piece, k, &end, 0.0, 90.0)
                            .is_some_and(|(_, m)| m < 0.01)
                    })
                })
        })
        .expect("a piece that meets the free end");
    ed.kit.chosen = Some(j);
    ed.extend(1).unwrap();
    let names = ed.session.cell_names(0x019E);
    assert_eq!(names.len(), 1, "one new cell");
    let new = *names.keys().next().unwrap();
    assert!(ed.session.cells(0x019E)[&0x0103]
        .portals
        .iter()
        .any(|p| p.other_cell_id == u32::from(new) && p.flags & 1 == 1));
    ed.fly_into_cell(0x019E, 0x0103).unwrap();
    for _ in 0..3 {
        ed.frame();
    }
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("pieces").join("reshaped.png"),
    )
    .unwrap();
}

/// The work of `frames` frames after a few to settle: each frame's mean, and the most of each
/// count.
fn measure(
    ed: &mut EditorApp,
    frames: u32,
) -> (lyceum_editor::app::Work, lyceum_editor::app::Work) {
    for _ in 0..10 {
        ed.frame();
    }
    let mut mean = lyceum_editor::app::Work::default();
    let mut most = lyceum_editor::app::Work::default();
    for _ in 0..frames {
        ed.frame();
        let w = ed.work;
        mean.ui_ms += w.ui_ms / frames as f32;
        mean.world_ms += w.world_ms / frames as f32;
        mean.overlay_vertices += w.overlay_vertices / frames as usize;
        mean.draw_calls += w.draw_calls / u64::from(frames);
        most.overlay_vertices = most.overlay_vertices.max(w.overlay_vertices);
        most.draw_calls = most.draw_calls.max(w.draw_calls);
        most.cells_read = most.cells_read.max(w.cells_read);
        most.picks = most.picks.max(w.picks);
    }
    (mean, most)
}

#[test]
fn an_idle_frame_reads_no_cells_and_picks_nothing_in_every_tool_outdoors_and_in_a_dungeon() {
    use lyceum_editor::app::Mode;
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-work");
    let mut ed = editor(&world);
    ed.resize(1600, 900);
    ed.dungeon_list.steps = false;
    ed.wait_for_pieces();
    let mut rows = Vec::new();
    let mut scene = |ed: &mut EditorApp, name: &str| {
        let (mean, most) = measure(ed, 40);
        println!(
            "{name:<34} ui {:6.2} ms  world {:6.2} ms  overlay {:>6} corners  draws {:>5}  cells read {:>3}  picks {:>2}",
            mean.ui_ms, mean.world_ms, mean.overlay_vertices, mean.draw_calls, most.cells_read, most.picks
        );
        rows.push((name.to_owned(), mean, most));
    };
    // Outdoors over Holtburg, in each tool.
    let z = ed.session.ground(HOLTBURG, 96.0, 96.0).unwrap();
    ed.look_at(HOLTBURG, Vec3::new(96.0, 96.0, z), 60.0, 30.0, -30.0);
    for (mode, name) in [
        (Mode::Select, "outdoors, select"),
        (Mode::Place, "outdoors, place scenery"),
        (Mode::Spawn, "outdoors, add object"),
        (Mode::Dungeon, "outdoors, dungeon kit"),
    ] {
        ed.mode = mode;
        scene(&mut ed, name);
    }
    // In the Drudge Hideout, a room selected.
    ed.enter_dungeon(0x019E, false).unwrap();
    scene(&mut ed, "in a dungeon, dungeon kit");
    ed.choose_cell(0x019E, 0x0105).unwrap();
    scene(&mut ed, "in a dungeon, a room selected");
    ed.mode = Mode::Select;
    scene(&mut ed, "in a dungeon, select");
    // Nothing idle reads a landblock's cells (they are read once until the world changes), or
    // picks. The times are printed for the record and asserted on nowhere.
    for (name, _, most) in &rows {
        assert_eq!(most.cells_read, 0, "{name}: {most:?}");
        assert_eq!(most.picks, 0, "{name}: {most:?}");
    }
}

#[test]
fn packs_at_odds_are_listed_with_the_checks_and_a_click_on_one_goes_there() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-conflicts");
    // A second pack that sets one of the vertices the sample sets, otherwise.
    let rival = dir.path().join("rival");
    std::fs::create_dir_all(rival.join("edits/landblocks")).unwrap();
    std::fs::write(
        rival.join("pack.toml"),
        "name = \"rival\"\nversion = \"0.1.0\"\nlicense = \"MIT\"\n",
    )
    .unwrap();
    std::fs::write(
        rival.join("edits/landblocks/A9B4.toml"),
        "[[vertex]]\nat = [0, 1]\nheight = 60\n",
    )
    .unwrap();
    std::fs::write(
        &world,
        "key = \"sample-world\"\npacks = [\"holtburg\", \"rival\"]\nout = \"build\"\n",
    )
    .unwrap();
    let mut ed = editor(&world);
    let i = ed
        .faults
        .iter()
        .position(|f| f.text.contains("vertex"))
        .expect("the conflict listed");
    let f = ed.faults[i].clone();
    assert!(
        !f.warning
            && f.text
                .contains(": vertex 1352,1441 height: holtburg sets it to 50")
            && f.text.contains("; rival sets it to 60 ("),
        "{f:?}"
    );
    // The vertex, on Holtburg's west edge, on the ground.
    let (lb, at) = f.at.expect("a place");
    assert_eq!((lb, at.x, at.y), (HOLTBURG, 0.0, 24.0));
    assert!((at.z - ed.session.ground(HOLTBURG, 0.0, 24.0).unwrap()).abs() < 1e-3);
    ed.frame();
    let row = ed
        .fault_rect(i)
        .expect("the conflict listed with the checks");
    click(&mut ed, (row.center().x, row.center().y));
    let o = pick::block_origin(HOLTBURG);
    let target = Vec3::new(o.x + at.x, o.y + at.y, at.z);
    let c = ed.camera.position;
    let d = Vec3::new(c.x - target.x, c.y - target.y, c.z - target.z).magnitude();
    assert!(
        (d - 40.0).abs() < 0.5,
        "the camera looks at the vertex from 40 m: {d}"
    );
    for _ in 0..3 {
        ed.frame();
    }
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("conflicts").join("listed.png"),
    )
    .unwrap();
}
