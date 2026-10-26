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

mod characters;
mod weenies;
mod world;

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
fn the_marks_on_the_world_are_drawn_from_the_view_the_world_is_drawn_with_while_it_resizes_and_moves(
) {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-marks-in-step");
    let mut ed = editor(&world);
    let stone = stone_west(&mut ed);
    let at = ed
        .view()
        .unwrap()
        .project(Vec3::new(stone.x, stone.y, stone.z + 1.5))
        .unwrap();
    click(&mut ed, at);
    assert!(ed.handles().is_some(), "the stone selected");
    let key = |vk: usize, pressed| HostEvent::KeyboardInput {
        key: dereth_input::keys::Key::new(vk, 0),
        pressed,
        text: None,
    };
    let probe = Vec3::new(stone.x + 2.0, stone.y + 1.0, stone.z + 0.5);
    ed.set_speed(1); // 2.5 m/s
                     // Each frame the window is a new size, so the world view is too, and the camera flies back
                     // and turns with a right drag as it does: the marks are drawn from the view the world is.
    ed.queue(key(0x53, true));
    ed.queue(HostEvent::MouseInput {
        button: MouseButton::Right,
        pressed: true,
    });
    for i in 0..8_u32 {
        ed.resize(800 + i * 40, 600 + i * 25);
        ed.queue(HostEvent::CursorMoved {
            x: f64::from(at.0 + i as f32),
            y: f64::from(at.1),
        });
        let before = ed.camera.position;
        ed.frame();
        assert_ne!(ed.camera.position, before, "it flies");
        let marks = ed.marks_view.expect("the marks drawn");
        let drawn = ed.view().expect("the world drawn");
        assert_eq!(marks.viewport(), drawn.viewport(), "frame {i}");
        for p in [probe, stone] {
            let (a, b) = (marks.project(p).unwrap(), drawn.project(p).unwrap());
            assert!(
                (a.0 - b.0).abs() < 0.05 && (a.1 - b.1).abs() < 0.05,
                "frame {i}: {a:?} {b:?}"
            );
        }
    }
    ed.queue(key(0x53, false));
    ed.queue(HostEvent::MouseInput {
        button: MouseButton::Right,
        pressed: false,
    });
    // Still, at a size the window has taken: the arrows are drawn where the world's view puts
    // them.
    ed.resize(1100, 760);
    ed.frame();
    ed.frame();
    let h = ed.handles().expect("its handles");
    let img = capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("marks-in-step").join("after.png"),
    )
    .unwrap();
    let tip = h.arrows.iter().find(|(k, _)| *k == Handle::X).unwrap().1;
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
                .zip([0xE8, 0x4A, 0x3C])
                .all(|(a, b)| (i32::from(*a) - b).abs() <= 12)
        })
        .count();
    assert!(
        on >= 16,
        "{on} of 20 points in the east arrow's color: {h:?}"
    );
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
    // The spawn tool's panels laid out from the first.
    ed.mode = lyceum_editor::app::Mode::Spawn;
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
        name: None,
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
    ed.show_findings();
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
    ed.show_findings();
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
            name: None,
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
    ed.set_speed(0);
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
        // Drawn off the frame: shown in the frame after the one that asked for it.
        ed.frame();
        ed.wait_for_pictures();
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
        ed.wait_for_pictures();
        assert_eq!(ed.pictures(), (2, 0));
    }
    // A picture already drawn is read from the cache, not drawn again.
    let mut ed = editor_with(&world, &["--thumbnails", &pictures_arg]);
    ed.wait_for_names().unwrap();
    ed.mode = lyceum_editor::app::Mode::Place;
    ed.panels.filter = "gribble".into();
    ed.frame();
    ed.wait_for_pictures();
    assert_eq!(ed.pictures(), (0, 1));
    assert_eq!(ed.picture_path(gribble).unwrap(), path);
}

#[test]
fn the_body_walks_at_the_speed_the_camera_flies_at() {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-walk-speed");
    let mut ed = editor(&world);
    let start = (120.0, 110.0);
    ed.set_speed(2); // 5 m/s
    let slow = walk_north(&mut ed, start, 60, |_| {});
    ed.set_speed(4); // 25 m/s
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
        let mut ed = editor_with(&world, &["--size", "1280x900"]);
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
        ed.panels.kinds = [Category::Tree].into_iter().collect();
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
        ed.panels.kinds.clear();
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
    ed.mode = lyceum_editor::app::Mode::Terrain;
    ed.frame();
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
fn the_terrain_brush_lists_the_regions_terrains_by_name_and_picture_in_a_dropdown_and_one_chosen_paints(
) {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-terrains");
    let mut ed = editor_with(&world, &["--size", "1280x900"]);
    let types = ed.terrain_types();
    assert!(types.len() >= 32, "{} terrain types", types.len());
    assert_eq!(types[1].1, "Grassland");
    assert!(types.iter().all(|(_, _, tex)| tex.is_some()));
    ed.mode = lyceum_editor::app::Mode::Terrain;
    ed.frame();
    // The ground's brushes are icons: Lower clicked, the brush lowers.
    let (_, lower) = *ed
        .brush_rects
        .iter()
        .find(|(b, _)| matches!(b, Brush::Lower(_)))
        .expect("Lower's icon");
    assert!(lower.width() < 40.0 && lower.height() < 40.0, "{lower:?}");
    click(&mut ed, (lower.center().x, lower.center().y));
    assert!(matches!(ed.panels.brush, Brush::Lower(_)));
    ed.panels.brush = Brush::Terrain(0);
    ed.frame();
    ed.frame();
    // One dropdown; opened, it lists every terrain.
    let combo = ed.panels.brush_combo.expect("the terrain's dropdown");
    click(&mut ed, (combo.center().x, combo.center().y));
    ed.frame();
    assert_eq!(ed.terrain_rects.len(), types.len());
    let img = capture(&mut ed, dir.path());
    // Grassland's picture is drawn in its cell, and it is green.
    let (_, r) = *ed.terrain_rects.iter().find(|(v, _)| *v == 1).unwrap();
    let mut sum = [0u32; 3];
    let mut n = 0;
    for y in 4..36 {
        for x in 4..36 {
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
fn the_scene_brush_lists_what_each_scene_grows_on_the_terrain_under_it_in_a_dropdown_and_one_chosen_paints(
) {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-scenes");
    let mut ed = editor_with(&world, &["--size", "1280x900"]);
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
    // One dropdown; opened, it lists every scene with what it grows.
    ed.frame();
    let combo = ed.panels.brush_combo.expect("the scene's dropdown");
    click(&mut ed, (combo.center().x, combo.center().y));
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
    // The pictures are drawn off the frame: shown once they have come back.
    ed.wait_for_pictures();
    ed.frame();
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
    ed.show_tab(lyceum_editor::app::Tab::Map);
    ed.frame();
    ed.frame();
    assert!(ed.map_rect.is_some());
    // The camera's spot is marked where it is.
    let cam = ed.camera.position;
    let spot = ed.map_point(cam.x, cam.y).unwrap();
    let img = capture(&mut ed, dir.path());
    let [r, g, b] = pixel(&img, spot.x, spot.y);
    assert!(r > 200 && g < 90 && b < 90, "{r} {g} {b}");
    // At zoom 1, as it opens: the whole world, square, as large as its panel allows and in its
    // middle; zooming out goes no further.
    assert_eq!(ed.panels.map_zoom, 1.0);
    let whole = |ed: &EditorApp| {
        (ed.map_view.width() - 2048.0).abs() < 1.0 && (ed.map_view.height() - 2048.0).abs() < 1.0
    };
    assert!(whole(&ed), "{:?}", ed.map_view);
    let (shown, panel) = (ed.map_rect.unwrap(), ed.map_panel.unwrap());
    assert!((shown.width() - shown.height()).abs() < 1.0, "{shown:?}");
    assert!((shown.width() - panel.width().min(panel.height())).abs() < 1.0);
    assert!(
        (shown.center() - panel.center()).length() < 1.0,
        "{shown:?} in {panel:?}"
    );
    let img = capture(&mut ed, dir.path());
    let inside = pixel(&img, shown.center().x, shown.top() + 2.0);
    // Beside it in the panel: to its side in a wide panel, over it in a tall one.
    let beside = if panel.width() > panel.height() {
        pixel(&img, panel.left() + 2.0, panel.center().y)
    } else {
        pixel(&img, panel.center().x, panel.top() + 2.0)
    };
    assert_ne!(inside, beside, "the map drawn in its square alone");
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("map").join("zoom-1.png"),
    )
    .unwrap();
    hover(&mut ed, (shown.center().x, shown.center().y));
    ed.queue(HostEvent::MouseWheel { notches: 1.0 });
    ed.frame();
    assert!(ed.panels.map_zoom > 1.0);
    for _ in 0..3 {
        ed.queue(HostEvent::MouseWheel { notches: -1.0 });
        ed.frame();
    }
    assert_eq!(ed.panels.map_zoom, 1.0);
    assert!(whole(&ed));
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
    assert!((ed.map_view.width().min(ed.map_view.height()) - 32.0).abs() < 1e-3);
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
fn a_selected_building_is_moved_by_its_handles_with_its_cells() {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-building");
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
    // Esc: nothing selected.
    hold_key(&mut ed, 0x1B, 1);
    ed.frame();
    assert!(ed.selection.is_none());
}

#[test]
fn the_place_tool_filters_by_several_kinds_at_once_stars_favorites_tags_by_right_click_and_sets_things_on_what_is_under_it(
) {
    use lyceum_editor::app::Mode;
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-place-tool");
    let mut ed = editor_with(&world, &["--size", "1280x900"]);
    ed.wait_for_names().unwrap();
    let stone = stone_west(&mut ed);
    ed.mode = Mode::Place;
    for _ in 0..3 {
        ed.frame();
    }
    // The kinds in one dropdown, each a check box: Tree and Rock checked there, the dropdown
    // staying open between; then its Clear shows every kind again.
    let combo = ed.panels.category_combo.expect("the kind's dropdown");
    click(&mut ed, (combo.center().x, combo.center().y));
    ed.frame();
    let row = |ed: &EditorApp, k: Category| {
        ed.panels
            .category_rows
            .iter()
            .find(|(c, _)| *c == Some(k))
            .map(|(_, r)| *r)
            .expect("the kind listed")
    };
    let tree = row(&ed, Category::Tree);
    click(&mut ed, (tree.center().x, tree.center().y));
    ed.frame();
    let rock = row(&ed, Category::Rock);
    click(&mut ed, (rock.center().x, rock.center().y));
    assert_eq!(
        ed.panels.kinds,
        [Category::Tree, Category::Rock].into_iter().collect()
    );
    let list = ed.browser_list();
    let kinds: std::collections::BTreeSet<Category> =
        list.iter().map(|m| ed.names.category(*m)).collect();
    assert_eq!(
        kinds,
        [Category::Tree, Category::Rock].into_iter().collect()
    );
    ed.frame();
    let clear = ed
        .panels
        .category_rows
        .iter()
        .find(|(c, _)| c.is_none())
        .map(|(_, r)| *r)
        .expect("the dropdown's Clear");
    click(&mut ed, (clear.center().x, clear.center().y));
    assert!(ed.panels.kinds.is_empty());
    // Closed again by a click outside it.
    let w = ed.world_rect.unwrap();
    click(&mut ed, (w.center().x, w.max.y - 20.0));
    ed.frame();
    // A star in each tile's corner keeps it among the favorites, and lets it go; the tile is
    // not chosen by it.
    ed.frame();
    let (m, star) = ed.picture_stars[0];
    click(&mut ed, (star.center().x, star.center().y));
    assert!(ed.browser_state.favorites.contains(&m.raw()));
    assert_eq!(ed.model, None);
    ed.frame();
    let (_, star) = *ed.picture_stars.iter().find(|(k, _)| *k == m).unwrap();
    click(&mut ed, (star.center().x, star.center().y));
    assert!(!ed.browser_state.favorites.contains(&m.raw()));
    // A tag given by a right click on a tile.
    let (other, tile) = ed.picture_rects[1];
    ed.tag_model(m, "standing", true).unwrap();
    ed.frame();
    let right = |ed: &mut EditorApp, pressed| {
        ed.queue(HostEvent::MouseInput {
            button: MouseButton::Right,
            pressed,
        });
    };
    ed.queue(HostEvent::CursorMoved {
        x: f64::from(tile.center().x),
        y: f64::from(tile.center().y),
    });
    right(&mut ed, true);
    ed.frame();
    right(&mut ed, false);
    ed.frame();
    ed.frame();
    let (_, check) = ed
        .panels
        .tag_menu
        .iter()
        .find(|(g, _)| g == "standing")
        .cloned()
        .expect("the tag in the menu");
    click(&mut ed, (check.center().x, check.center().y));
    assert!(ed.tags.of(other).iter().any(|g| g == "standing"));
    // The menu stays open: a new tag typed into it, and Enter gives it and closes the menu.
    ed.frame();
    let field = ed
        .panels
        .tag_field
        .expect("the menu still open, with its new tag's field");
    type_in(&mut ed, field, "mossy", true);
    assert!(ed.tags.of(other).iter().any(|g| g == "mossy"));
    assert!(
        ed.panels.tag_field.is_none(),
        "closed once the tag is added"
    );
    // Set on the stone where the pointer meets it: off the ground, on the stone.
    ed.choose_model(dereth_primitives::DataId(0x0200_040B))
        .unwrap();
    let ground = ed.session.ground(HOLTBURG, 24.0, 24.0).unwrap();
    for _ in 0..2 {
        ed.frame();
    }
    let at = ed
        .view()
        .unwrap()
        .project(Vec3::new(stone.x, stone.y, stone.z + 1.5))
        .unwrap();
    click(&mut ed, at);
    let Some(Selection::Static { index, .. }) = ed.selection.clone() else {
        panic!("placed: {:?} {}", ed.selection, ed.panels.message);
    };
    let placed = ed.session.statics(HOLTBURG).unwrap().objects[index]
        .frame
        .origin;
    assert!(
        placed.z > ground + 0.5 && placed.z < ground + 6.0,
        "{placed:?} over {ground}"
    );
    // On open ground: on the ground.
    let z = ed.session.ground(HOLTBURG, 40.0, 10.0).unwrap();
    ed.look_at(HOLTBURG, Vec3::new(40.0, 10.0, z), 22.0, -40.0, -25.0);
    for _ in 0..2 {
        ed.frame();
    }
    let at = ed
        .view()
        .unwrap()
        .project(Vec3::new(
            pick::block_origin(HOLTBURG).x + 40.0,
            pick::block_origin(HOLTBURG).y + 10.0,
            z,
        ))
        .unwrap();
    click(&mut ed, at);
    let Some(Selection::Static { index, .. }) = ed.selection.clone() else {
        panic!("placed: {:?}", ed.selection);
    };
    let o = ed.session.statics(HOLTBURG).unwrap().objects[index]
        .frame
        .origin;
    let g = ed.session.ground(HOLTBURG, o.x, o.y).unwrap();
    assert!((o.z - g).abs() < 0.01, "{o:?} over {g}");
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
    // A window tall enough for the browser's tiles under its search and kinds.
    let mut ed = editor_with(
        &world,
        &["--weenies", sql.to_str().unwrap(), "--size", "1280x900"],
    );
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
    type_in(ed, r, text, true);
}

/// A spawn of weenie `w` set down on the ground in Holtburg, as the spawn tool sets one down, as
/// one step.
fn spawn_of(ed: &mut EditorApp, w: &lyceum_foundry::edit::Weenie) {
    let z = ed.session.ground(HOLTBURG, 84.0, 84.0).unwrap();
    ed.edit(&Edit::Op(lyceum_foundry::edit::Op::PlaceSpawn {
        landblock: HOLTBURG,
        weenie: w.clone(),
        origin: Vec3::new(84.0, 84.0, z),
        heading: 0.0,
    }))
    .unwrap();
}

/// Server class `class` written again by the pack, as its first change in the editor writes it,
/// and the editor opened on it.
fn change_in_world(ed: &mut EditorApp, class: u32) {
    ed.copy_class(class).unwrap();
    ed.open_editor(lyceum_foundry::edit::Weenie::Class(class));
}

/// The rect drawn for `key` among `list`.
fn drawn(list: &[(String, egui::Rect)], key: &str) -> egui::Rect {
    list.iter()
        .find(|(k, _)| k == key)
        .map(|(_, r)| *r)
        .unwrap_or_else(|| panic!("{key} drawn"))
}

/// A right click on the mark of field `key` changed from the server's, and its menu's Put back.
fn put_back_by_mark(ed: &mut EditorApp, key: &str) {
    let mark = drawn(&ed.editor.changed_marks, key);
    ed.queue(HostEvent::CursorMoved {
        x: f64::from(mark.center().x),
        y: f64::from(mark.center().y),
    });
    for pressed in [true, false] {
        ed.queue(HostEvent::MouseInput {
            button: MouseButton::Right,
            pressed,
        });
        ed.frame();
    }
    for _ in 0..4 {
        ed.frame();
    }
    let button = ed.editor.put_back_button.expect("its menu's Put back");
    click(ed, (button.center().x, button.center().y));
    ed.frame();
}

/// A new portal named `name`, made from the spawn tool's New... as a plain portal: the spawn tool
/// places it next. The editor that opens on it is closed again, clear of the world view.
fn new_portal(ed: &mut EditorApp, name: &str) {
    let new = ed.panels.new_button.expect("the spawn tool's New...");
    click(ed, (new.center().x, new.center().y));
    ed.frame();
    assert!(ed.editor.creator.open, "the dialog");
    ed.editor.creator.kind = lyceum_editor::app::NewKind::Portal;
    ed.editor.creator.copy = false;
    ed.frame();
    let field = ed.editor.creator.name_field.expect("its name");
    type_in(ed, field, name, false);
    let make = ed.editor.creator.make_button.expect("Make it");
    click(ed, (make.center().x, make.center().y));
    assert!(!ed.editor.creator.open, "{:?}", ed.editor.creator.problem);
    ed.editor.open = false;
    for _ in 0..2 {
        ed.frame();
    }
}

/// Type `text` into the field drawn at `r`: a click into it, everything in it chosen, the text,
/// and Enter when `enter`.
fn type_in(ed: &mut EditorApp, r: egui::Rect, text: &str, enter: bool) {
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
    if enter {
        ed.queue(k(0x0D, true, None));
        ed.queue(k(0x0D, false, None));
        ed.frame();
    }
    ed.frame();
}

#[test]
fn the_weenie_form_shows_a_packs_weenie_by_the_servers_names_and_an_edit_is_one_step_that_redraws_it(
) {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-form");
    let schema = dir.path().join("schema.json");
    std::fs::write(&schema, SCHEMA).unwrap();
    let mut ed = editor_with(
        &world,
        &["--schema", schema.to_str().unwrap(), "--size", "1280x1000"],
    );
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
    let guide_w = lyceum_foundry::edit::Weenie::Symbol("holtburg:guide".into());
    assert_eq!(ed.form_weenie(), Some(guide_w.clone()));
    // The Definition pane changes nothing: its fields are the editor's.
    assert!(ed.form_fields.is_empty());
    ed.open_editor(guide_w.clone());
    for _ in 0..2 {
        ed.frame();
    }
    // Its pages: Basic first; the level on its creature's page, as it sets one; every property
    // under Advanced.
    use lyceum_editor::app::{Page, Section};
    let has = |ed: &EditorApp, key: &str| ed.form_fields.iter().any(|(k, _)| k == key);
    assert_eq!(ed.editor.page, Page::Section(Section::Basic));
    assert!(has(&ed, "string:1") && has(&ed, "did:1") && !has(&ed, "int:25"));
    for (page, key) in [
        (Page::Section(Section::Creature), "int:25"),
        (Page::Advanced, "int:93"),
        (Page::Section(Section::Basic), "string:1"),
    ] {
        let r = ed
            .editor
            .page_rects
            .iter()
            .find(|(p, _)| *p == page)
            .map(|(_, r)| *r)
            .unwrap_or_else(|| panic!("{page:?} listed"));
        click(&mut ed, (r.center().x, r.center().y));
        ed.frame();
        assert_eq!(ed.editor.page, page);
        assert!(has(&ed, key), "{page:?}: {key}");
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
    // Closed, the editor shows the world view under it again.
    ed.editor.open = false;
    ed.frame();
    let text = ed.session.sql_text(&file).unwrap();
    assert!(text.contains("1, 'Gribble the Bold') /* Name */"), "{text}");
    assert_eq!(ed.session.history(), (1, 0));
    // So does the Level, set by the form's own edit.
    ed.form_edit(
        &guide_w,
        &lyceum_editor::app::FormEdit::Property {
            kind: lyceum_foundry::content::PropertyKind::Int,
            id: 25,
            value: Some("12".into()),
        },
    )
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
    ed.form_edit(
        &guide_w,
        &lyceum_editor::app::FormEdit::Property {
            kind: lyceum_foundry::content::PropertyKind::DataId,
            id: 1,
            value: Some("0x02000DD2".into()),
        },
    )
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
        .form_edit(
            &guide_w,
            &lyceum_editor::app::FormEdit::Property {
                kind: lyceum_foundry::content::PropertyKind::Int,
                id: 25,
                value: Some("twelve".into()),
            }
        )
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
    ed.form_edit(
        &guide_w,
        &FormEdit::AddRow {
            table: "weenie_properties_spell_book".into(),
            values: vec![("spell".into(), "1".into())],
        },
    )
    .unwrap();
    assert_eq!(spells(&mut ed), [("1".to_owned(), "2".to_owned())]);
    ed.form_edit(
        &guide_w,
        &FormEdit::Cell {
            table: "weenie_properties_spell_book".into(),
            row: 0,
            column: "probability".into(),
            value: "2.5".into(),
        },
    )
    .unwrap();
    assert_eq!(spells(&mut ed), [("1".to_owned(), "2.5".to_owned())]);
    ed.form_edit(
        &guide_w,
        &FormEdit::RemoveRow {
            table: "weenie_properties_spell_book".into(),
            row: 0,
        },
    )
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
fn the_editor_stays_on_its_weenie_as_the_selection_moves_and_edit_opens_a_servers_weenie_at_once() {
    use lyceum_foundry::edit::Weenie;
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-names-its-weenie");
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, SERVER_SPAWN).unwrap();
    let mut ed = editor_with(
        &world,
        &["--weenies", sql.to_str().unwrap(), "--size", "1280x1000"],
    );
    let guide = Weenie::Symbol("holtburg:guide".into());
    let drudge = Weenie::Class(90005);
    let guide_file = ed.session.weenie_file(&guide).unwrap().0;
    ed.open_editor(guide.clone());
    for _ in 0..2 {
        ed.frame();
    }
    assert!(ed.editor.open);
    // The server's drudge selected: the Definition pane follows it, the editor stays.
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
    ed.show_tab(lyceum_editor::app::Tab::Definition);
    ed.frame();
    assert_eq!(ed.form_weenie(), Some(drudge.clone()));
    assert_eq!(ed.editor.weenie(), Some(&guide));
    // Typed into the editor's Name field and left: the guide takes it, the drudge nothing.
    type_into(&mut ed, "string:1", "Gribble the Bold");
    let text = ed.session.sql_text(&guide_file).unwrap();
    assert!(text.contains("1, 'Gribble the Bold') /* Name */"), "{text}");
    assert!(
        ed.session.weenie_file(&drudge).is_none(),
        "the drudge unchanged"
    );
    assert_eq!(ed.session.history(), (1, 0));
    // Edit on the server's drudge opens it in the editor at once, after the guide, as the
    // server has it: nothing is written yet.
    ed.edit_weenie(&drudge, None);
    for _ in 0..2 {
        ed.frame();
    }
    assert!(ed.editor.open && ed.tab_in_front(lyceum_editor::app::Tab::Editor));
    assert_eq!(ed.editor.weenie(), Some(&drudge));
    assert!(ed.session.weenie_file(&drudge).is_none(), "nothing written");
    assert_eq!(ed.session.history(), (1, 0));
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("weenie-editor").join("opens-at-once.png"),
    )
    .unwrap();
    // Its first change: the pack writes the server's class again with it, as one step.
    type_into(&mut ed, "string:1", "Drudge Lurker");
    let (file, mine) = ed
        .session
        .weenie_file(&drudge)
        .unwrap_or_else(|| panic!("the pack's change: {}", ed.panels.message));
    assert!(mine && file.ends_with("server/sql/weenies/90005 Drudge Skulker.sql"));
    assert!(ed
        .session
        .sql_text(&file)
        .unwrap()
        .contains("'Drudge Lurker'"));
    assert_eq!(ed.session.history(), (2, 0));
    assert_eq!(ed.editor.weenie(), Some(&drudge));
    assert_eq!(
        ed.owner_of(&drudge),
        lyceum_editor::app::Owner::ChangedIn {
            pack: "holtburg".into(),
            mine: true
        }
    );
    ed.frame();
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("weenie-editor").join("changed-in-world.png"),
    )
    .unwrap();
    // Back to the guide, and forward again.
    let back = ed.editor.back_button.expect("Back");
    click(&mut ed, (back.center().x, back.center().y));
    assert_eq!(ed.editor.weenie(), Some(&guide));
    ed.frame();
    let fwd = ed.editor.forward_button.expect("Forward");
    click(&mut ed, (fwd.center().x, fwd.center().y));
    assert_eq!(ed.editor.weenie(), Some(&drudge));
    // The server's put back: the pack's file goes, as one step, and comes again on undo.
    ed.put_back_server(90005).unwrap();
    assert!(ed.session.weenie_file(&drudge).is_none());
    assert_eq!(ed.owner_of(&drudge), lyceum_editor::app::Owner::Server);
    assert_eq!(ed.session.history(), (3, 0));
    ed.edit(&Edit::Undo).unwrap();
    assert!(ed.session.weenie_file(&drudge).is_some());
    assert!(ed.put_back_server(1).is_err(), "not changed in this world");
}

#[test]
fn the_definition_heads_words_end_before_its_buttons_at_any_pane_width_and_scale() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-definition-head");
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, SERVER_SPAWN).unwrap();
    let mut ed = editor_with(
        &world,
        &["--weenies", sql.to_str().unwrap(), "--size", "1280x1000"],
    );
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
    ed.show_tab(lyceum_editor::app::Tab::Definition);
    // Wide at 100%, then a narrow pane at 150%: "Creature · class 90005 · the server's" is
    // longer than the narrow pane has room for.
    for (name, (w, h), scale) in [("wide", (1280, 1000), 1.0), ("narrow", (900, 800), 1.5)] {
        ed.resize(w, h);
        ed.set_ui_scale(scale);
        for _ in 0..3 {
            ed.frame();
        }
        let words = ed.definition.head_words.expect("the head's words");
        let from = ed.definition.head_words_from.expect("where they start");
        let edit = ed.definition.edit_button.expect("Edit");
        assert!(
            words.max.x <= edit.min.x,
            "{name}: the words {words:?} run under Edit {edit:?}"
        );
        assert!(
            words.min.x >= from - 0.5,
            "{name}: the words {words:?} start before {from}, over the picture"
        );
        #[allow(clippy::cast_precision_loss)] // a window's width
        let window = w as f32 / ed.ui_scale();
        assert!(edit.max.x <= window, "{name}: Edit {edit:?} in the window");
        capture(&mut ed, dir.path());
        std::fs::copy(
            dir.path().join("frame.png"),
            shots("weenie-editor").join(format!("head-{name}.png")),
        )
        .unwrap();
    }
}

#[test]
fn plays_menu_is_an_arrow_drawn_beside_play_in_the_bars_icon_color() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-play-menu");
    let mut ed = editor(&world);
    for _ in 0..2 {
        ed.frame();
    }
    let play = ed.play_button.expect("Play");
    let arrow = ed.play_menu_button.expect("Play's menu");
    assert!(arrow.min.x >= play.max.x - 1.0, "{arrow:?} beside {play:?}");
    assert!((arrow.center().y - play.center().y).abs() < 1.0);
    assert!(arrow.width() < play.width(), "a narrow button: {arrow:?}");
    // Its triangle is drawn in the middle, in the icons' light color over the dark bar.
    let s = ed.ui_scale();
    let shot = capture(&mut ed, dir.path());
    let lit = |p: [u8; 3]| p.iter().copied().max().unwrap_or(0);
    let middle = pixel(&shot, arrow.center().x * s, arrow.center().y * s);
    let side = pixel(&shot, (arrow.min.x + 1.5) * s, arrow.center().y * s);
    assert!(lit(middle) > 150, "the triangle: {middle:?}");
    assert!(lit(side) < 100, "the bar beside it: {side:?}");
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("play-menu").join("bar.png"),
    )
    .unwrap();
}

#[test]
fn the_editors_head_words_end_before_its_buttons_at_any_width_and_scale() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-editor-head");
    let sql = dir.path().join("server.sql");
    // A name longer than the editor is wide.
    std::fs::write(
        &sql,
        SERVER_SPAWN.replace(
            "'Drudge Skulker'",
            "'Drudge Skulker, Who Lurks in the Hills West of Holtburg and Waits for the Unwary'",
        ),
    )
    .unwrap();
    let mut ed = editor_with(
        &world,
        &["--weenies", sql.to_str().unwrap(), "--size", "1280x1000"],
    );
    let drudge = lyceum_foundry::edit::Weenie::Class(90005);
    ed.server_class_now(90005).unwrap().expect("its rows");
    ed.open_editor(drudge);
    // Wide at 100%, then narrow at 150%: the name is cut short before Back either way.
    for (name, (w, h), scale) in [("wide", (1280, 1000), 1.0), ("narrow", (900, 800), 1.5)] {
        ed.resize(w, h);
        ed.set_ui_scale(scale);
        for _ in 0..3 {
            ed.frame();
        }
        let words = ed.editor.head_words.expect("the head's words");
        let back = ed.editor.back_button.expect("Back");
        assert!(
            words.max.x <= back.min.x,
            "{name}: the words {words:?} run under Back {back:?}"
        );
        let forward = ed.editor.forward_button.expect("Forward");
        assert!(back.max.x <= forward.min.x, "{name}: Back before Forward");
        capture(&mut ed, dir.path());
        std::fs::copy(
            dir.path().join("frame.png"),
            shots("weenie-editor").join(format!("editor-head-{name}.png")),
        )
        .unwrap();
    }
}

#[test]
fn the_editor_keeps_its_place_across_dungeon_mode_and_puts_a_weenie_back_as_saved() {
    use lyceum_foundry::edit::Weenie;
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-dungeon-mode");
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, SERVER_SPAWN).unwrap();
    let mut ed = editor_with(
        &world,
        &["--weenies", sql.to_str().unwrap(), "--size", "1280x1000"],
    );
    let guide = Weenie::Symbol("holtburg:guide".into());
    let file = ed.session.weenie_file(&guide).unwrap().0;
    let saved = ed.session.sql_text(&file).unwrap();
    ed.edit_weenie(&guide, Some("int:25".into()));
    for _ in 0..2 {
        ed.frame();
    }
    assert!(ed.editor.open && ed.tab_in_front(lyceum_editor::app::Tab::Editor));
    // Opened at the level: its page shown.
    assert_eq!(
        ed.editor.page,
        lyceum_editor::app::Page::Section(lyceum_editor::app::Section::Creature)
    );
    assert!(ed.form_fields.iter().any(|(k, _)| k == "int:25"));
    // Into a dungeon and out: the editor is drawn in both layouts, on its weenie and page.
    ed.enter_dungeon(0x019E).unwrap();
    for _ in 0..2 {
        ed.frame();
    }
    assert!(ed.editor.open);
    assert!(ed.form_fields.iter().any(|(k, _)| k == "int:25"));
    ed.leave_dungeon();
    ed.form_fields.clear();
    for _ in 0..2 {
        ed.frame();
    }
    assert!(ed.editor.open && ed.editor.weenie() == Some(&guide));
    assert!(ed.form_fields.iter().any(|(k, _)| k == "int:25"));
    // Changed, then put back as saved: one step, undone like any other.
    ed.form_edit(
        &guide,
        &lyceum_editor::app::FormEdit::Property {
            kind: lyceum_foundry::content::PropertyKind::Int,
            id: 25,
            value: Some("12".into()),
        },
    )
    .unwrap();
    assert!(ed.session.sql_unsaved(&file));
    ed.put_back_saved(&guide).unwrap();
    assert_eq!(ed.session.sql_text(&file).unwrap(), saved);
    assert!(!ed.session.sql_unsaved(&file));
    assert_eq!(ed.session.history(), (2, 0));
    ed.edit(&Edit::Undo).unwrap();
    assert!(ed
        .session
        .sql_text(&file)
        .unwrap()
        .contains("25,         12)"));
    // A copy of the server's drudge, from the menus' Make a copy, named: a new weenie of the
    // pack, in the editor.
    ed.panels.weenie = Some(Weenie::Class(90005));
    ed.open_creator(Some(Weenie::Class(90005)));
    ed.editor.creator.name = "Drudge Sneak".into();
    ed.make_weenie().unwrap();
    let made = Weenie::Symbol("holtburg:drudge-sneak".into());
    assert_eq!(ed.editor.weenie(), Some(&made), "{}", ed.panels.message);
    assert!(ed.session.weenie_file(&made).is_some_and(|(_, mine)| mine));
    assert!(ed.session.weenie_file(&Weenie::Class(90005)).is_none());
}

#[test]
fn a_server_class_is_shown_as_it_is_and_its_first_edit_makes_the_packs_copy_which_its_spawns_take()
{
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
    // Shown as the server has it, every row, with nothing written to the pack.
    let shown = ed
        .server_class_now(90005)
        .unwrap()
        .expect("the server's class");
    assert!(
        shown.contains("VALUES (90005, 'drudgeskulker', 10,"),
        "{shown}"
    );
    ed.show_tab(lyceum_editor::app::Tab::Definition);
    for _ in 0..2 {
        ed.frame();
    }
    assert!(
        ed.session.weenie_file(&class).is_none(),
        "reading it copies nothing"
    );
    assert_eq!(ed.session.history(), (0, 0));
    // The first edit makes the pack's copy, with the edit in it: one step.
    ed.form_edit(
        &class,
        &lyceum_editor::app::FormEdit::Property {
            kind: lyceum_foundry::content::PropertyKind::DataId,
            id: 1,
            value: Some("0x02000DD2".into()),
        },
    )
    .unwrap();
    let (file, mine) = ed.session.weenie_file(&class).expect("the pack's copy");
    assert!(mine && file.ends_with("server/sql/weenies/90005 Drudge Skulker.sql"));
    let text = ed.session.sql_text(&file).unwrap();
    assert!(
        text.contains("VALUES (90005, 'drudgeskulker', 10,"),
        "{text}"
    );
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
    assert_eq!(ed.session.history(), (1, 0));
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
    let edit = |ed: &mut EditorApp, e: FormEdit| ed.form_edit(&w, &e).unwrap();
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
            after: None,
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
        .form_edit(
            &w,
            &FormEdit::AddAction {
                set: 0,
                kind: "Dance".into(),
            }
        )
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
        ed.form_edit(
            &w,
            &FormEdit::AddAction {
                set: 0,
                kind: "22".into(),
            },
        )
        .unwrap();
        ed.form_edit(
            &w,
            &FormEdit::Action {
                set: 0,
                action: n,
                column: "message".into(),
                value: quest.into(),
            },
        )
        .unwrap();
    }
    let text = ed.session.sql_text(&file).unwrap();
    let named = lyceum_editor::app::quests_named(&lyceum_foundry::content::SqlDoc::parse(&text));
    assert_eq!(named, ["HoltburgWell", "GribbleGreeted"]);
    let (q, at) = ed.session.quest("HoltburgWell").expect("the server's");
    assert!(at.is_none() && q.min_delta == 3600 && q.max_solves == 1);
    assert!(ed.session.quest("GribbleGreeted").is_none());
    // Registered in the pack, then made a daily quest.
    ed.form_edit(
        &w,
        &FormEdit::RegisterQuest {
            name: "GribbleGreeted".into(),
        },
    )
    .unwrap();
    let (_, at) = ed.session.quest("GribbleGreeted").expect("the pack's");
    let qfile = at.expect("in the pack's file");
    assert!(qfile.ends_with("server/sql/quests/GribbleGreeted.sql"));
    ed.form_edit(
        &w,
        &FormEdit::Quest {
            name: "GribbleGreeted".into(),
            column: "min_Delta".into(),
            value: "86400".into(),
        },
    )
    .unwrap();
    assert_eq!(
        ed.session.quest("GribbleGreeted").unwrap().0.min_delta,
        86400
    );
    assert!(
        ed.form_edit(
            &w,
            &FormEdit::Quest {
                name: "HoltburgWell".into(),
                column: "min_Delta".into(),
                value: "1".into(),
            }
        )
        .is_err(),
        "the server's quest is not the pack's to change"
    );
    // Its death treasure, set by number, is the server's tier 2.
    ed.form_edit(
        &w,
        &FormEdit::Property {
            kind: lyceum_foundry::content::PropertyKind::DataId,
            id: 35,
            value: Some("4".into()),
        },
    )
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
    let guide = lyceum_foundry::edit::Weenie::Symbol("holtburg:guide".into());
    // A class the world has is fine; one it has not, a spell the table has not, and a
    // property the server has no name for are faults, each naming the guide's file.
    for (table, key, value) in [
        ("weenie_properties_create_list", "weenie_Class_Id", "90004"),
        ("weenie_properties_create_list", "weenie_Class_Id", "999999"),
        ("weenie_properties_spell_book", "spell", "99999"),
    ] {
        ed.form_edit(
            &guide,
            &FormEdit::AddRow {
                table: table.into(),
                values: vec![(key.into(), value.into())],
            },
        )
        .unwrap();
    }
    ed.form_edit(
        &guide,
        &FormEdit::Property {
            kind: lyceum_foundry::content::PropertyKind::Int,
            id: 777,
            value: Some("1".into()),
        },
    )
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
    ed.show_findings();
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
    // of the world view across.
    let middle = |r: egui::Rect| r.center().x * ed.ui_scale();
    assert!(
        (at_before.0 - middle(before)).abs() < 2.0 && (at_after.0 - middle(after)).abs() < 2.0,
        "{at_before:?} {at_after:?}"
    );
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
    // The room the camera is in is drawn around it, over the whole world view.
    let around = seen.get(&here.0 .0).copied().unwrap_or(0.0);
    assert!(
        around > 0.9 * view_area(&ed),
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
fn the_dungeon_list_names_a_dungeon_and_one_click_goes_in_at_its_portals_drop_point_and_leave_comes_back(
) {
    let _g = gpu_lock();
    let (dir, world) = sample("editor-dungeon-list");
    // Two portals into the hideout: one standing outdoors in Holtburg, and two of another that
    // stand in another dungeon's rooms and send who uses them elsewhere in it.
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, format!("{SERVER_PORTALS}{HIDEOUT_ENTRANCES}")).unwrap();
    let mut ed = editor_with(
        &world,
        &["--weenies", sql.to_str().unwrap(), "--size", "1200x900"],
    );
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
    // The drop point: the outdoor portal's destination, though the others are more.
    let (cell, at) = ed.drop_point(0x019E).unwrap();
    assert_eq!(cell, 0x019E_0114);
    assert!(
        (at.x - 40.0).abs() < 1e-3 && (at.y + 40.0).abs() < 1e-3,
        "{at:?}"
    );
    // One click on its row goes in, in dungeon mode, at the drop point.
    ed.show_tab(lyceum_editor::app::Tab::Dungeons);
    ed.dungeon_list.filter = "drudge".into();
    for _ in 0..2 {
        ed.frame();
    }
    let before = ed.camera;
    let (_, row) = ed
        .dungeon_list
        .rects
        .iter()
        .find(|(lb, _)| *lb == 0x019E)
        .copied()
        .expect("its row");
    click(&mut ed, (row.center().x, row.center().y));
    assert_eq!(ed.dungeon_mode(), Some(0x019E));
    let o = pick::block_origin(0x019E);
    let p = ed.camera.position;
    assert!(
        (p.x - o.x - 40.0).abs() < 1e-3 && (p.y - o.y + 40.0).abs() < 1e-3,
        "{p:?}"
    );
    // The grid in its tab, and the world view drawing the room round the camera.
    for _ in 0..3 {
        ed.frame();
    }
    assert!(ed.grid_shown() && ed.layout.rect.is_some());
    let (here, _) = ed.camera_cell().expect("in a room");
    assert_eq!(here.0 >> 16, 0x019E);
    let around = ed.cells_seen().get(&here.0).copied().unwrap_or(0.0);
    assert!(around > 0.9 * view_area(&ed), "{around}");
    let grid = ed.layout.rect.unwrap();
    let view = ed.world_rect.unwrap();
    assert!(
        grid.left() >= view.right() - 1.0,
        "the grid beside the world view"
    );
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("dungeons").join("entered.png"),
    )
    .unwrap();
    // Named in the pack: the list and the card call it so.
    ed.name_dungeon(0x019E, "Warren of Tests").unwrap();
    assert_eq!(ed.dungeon_name_of(0x019E), "Warren of Tests");
    ed.session.save().unwrap();
    let text =
        std::fs::read_to_string(dir.path().join("holtburg/edits/landblocks/019E.toml")).unwrap();
    assert!(text.contains("name = \"Warren of Tests\""), "{text}");
    // Leave: back where the camera was.
    ed.frame();
    let leave = ed.dungeon_list.leave_button.expect("the card's Leave");
    click(&mut ed, (leave.center().x, leave.center().y));
    assert_eq!(ed.dungeon_mode(), None);
    assert_eq!(ed.camera, before);
    assert!(!ed.grid_shown());
}

/// More portals into the hideout: the outdoor one's spawn in Holtburg, and two of another that
/// stand in another dungeon's rooms and send who uses them to its first room.
const HIDEOUT_ENTRANCES: &str = "\
INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES (90011, 'portalhideoutback', 7, '2005-02-09 10:00:00');
INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES (90011, 1, 'Hideout Back Door');
INSERT INTO `weenie_properties_position` (`object_Id`, `position_Type`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`)
VALUES (90011, 2, 0x019E0100, 10, -10, 0, 1, 0, 0, 0);
INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`)
VALUES (0x7A9B4070, 90010, 0xA9B40011, 60, 60, 94, 1, 0, 0, 0, False, '2019-05-16 00:00:00')
     , (0x701FD070, 90011, 0x01FD0101, 10, 10, 0, 1, 0, 0, 0, False, '2019-05-16 00:00:00')
     , (0x701FD071, 90011, 0x01FD0102, 12, 10, 0, 1, 0, 0, 0, False, '2019-05-16 00:00:00');
";

/// A point of the grid, in landblock meters, as the window's pixels.
fn grid_point(ed: &EditorApp, at: (f32, f32)) -> (f32, f32) {
    let p = ed.layout.to_screen(at).expect("the grid drawn");
    let s = ed.ui_scale();
    (p.x * s, p.y * s)
}

/// The pointer moved to `at` (pixels) and a frame drawn.
fn hover(ed: &mut EditorApp, at: (f32, f32)) {
    ed.queue(HostEvent::CursorMoved {
        x: f64::from(at.0),
        y: f64::from(at.1),
    });
    assert!(ed.frame());
}

/// Choose piece `i` as a builder does: typed into the browser's filter, its tile clicked.
fn choose_piece(ed: &mut EditorApp, i: usize) {
    let label = ed.kit.pieces.as_ref().unwrap()[i].label();
    ed.kit.filter = label;
    ed.kit.every = true;
    for _ in 0..2 {
        ed.frame();
    }
    let (_, r) = ed
        .kit
        .piece_rects
        .iter()
        .find(|(k, _)| *k == i)
        .copied()
        .expect("its tile shown");
    click(ed, (r.center().x, r.center().y));
    assert_eq!(ed.kit.chosen, Some(i));
    ed.kit.filter.clear();
}

/// The first square of the grid where piece `i` fits on the floor shown, by its middle.
fn fitting_square(ed: &mut EditorApp, lb: u16, i: usize) -> Option<(f32, f32)> {
    let marks = ed.piece_marks(lb, i);
    let p = marks.iter().find(|p| p.square().is_some())?;
    let (s, _) = p.square()?;
    Some(lyceum_foundry::grid::square_middle(s))
}

/// The server's blanks New Weenie starts from: an NPC, a creature, an item, a door, a portal, a
/// chest, a long sword, a cuirass, a shirt, a generator, a sign and a lever, as plain as the
/// server's own.
const SERVER_BLANKS: &str = "\
INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES (5773, 'npcblank', 10, '2005-02-09 10:00:00')
     , (7, 'creatureblank', 10, '2005-02-09 10:00:00')
     , (280, 'itemblank', 1, '2005-02-09 10:00:00')
     , (278, 'doorblank', 19, '2005-02-09 10:00:00')
     , (371, 'portalblank', 7, '2005-02-09 10:00:00')
     , (143, 'chestblank', 20, '2005-02-09 10:00:00')
     , (351, 'swordblank', 6, '2005-02-09 10:00:00')
     , (50, 'armorblank', 2, '2005-02-09 10:00:00')
     , (130, 'clothingblank', 2, '2005-02-09 10:00:00')
     , (381, 'generatorblank', 1, '2005-02-09 10:00:00')
     , (459, 'signblank', 1, '2005-02-09 10:00:00')
     , (285, 'switchblank', 26, '2005-02-09 10:00:00');
INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES (351, 1, 1), (50, 1, 2), (130, 1, 4), (381, 81, 1), (381, 82, 1);
INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES (5773, 1, 'Townsperson'), (7, 1, 'Drudge'), (280, 1, 'Trinket'), (278, 1, 'Door')
     , (371, 1, 'Portal'), (143, 1, 'Chest'), (351, 1, 'Long Sword'), (50, 1, 'Cuirass')
     , (130, 1, 'Shirt'), (381, 1, 'Generator'), (459, 1, 'Sign'), (459, 16, 'A sign.')
     , (285, 1, 'Lever');
INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES (5773, 1, 0x02000001), (7, 1, 0x020007DD), (280, 1, 0x02000181), (278, 1, 0x02000DD2)
     , (371, 1, 0x020001B3), (143, 1, 0x0200040B), (351, 1, 0x02000065), (50, 1, 0x020001A6)
     , (130, 1, 0x020000D4), (381, 1, 0x0200026B), (459, 1, 0x02000610), (285, 1, 0x020004B5);
INSERT INTO `treasure_death` (`treasure_Type`, `tier`, `loot_Quality_Mod`, `unknown_Chances`, `item_Chance`, `item_Min_Amount`, `item_Max_Amount`, `item_Treasure_Type_Selection_Chances`, `magic_Item_Chance`, `magic_Item_Min_Amount`, `magic_Item_Max_Amount`, `magic_Item_Treasure_Type_Selection_Chances`, `mundane_Item_Chance`, `mundane_Item_Min_Amount`, `mundane_Item_Max_Amount`, `mundane_Item_Type_Selection_Chances`, `last_Modified`)
VALUES (4, 1, 0, 19, 100, 1, 1, 8, 50, 0, 1, 8, 100, 0, 2, 8, '2005-02-09 10:00:00');
";

#[test]
fn each_new_kind_writes_its_kind_of_weenie_whose_sql_passes_the_checks_and_one_is_placed_here() {
    use lyceum_editor::app::{Kind, NewKind, Page, Section};
    let _g = gpu_lock();
    let (dir, world) = sample("editor-flows");
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, SERVER_BLANKS).unwrap();
    let mut ed = editor_with(&world, &["--weenies", sql.to_str().unwrap()]);
    stone_west(&mut ed);
    for (n, kind) in NewKind::ALL.into_iter().enumerate() {
        let name = format!("Test {}", kind.key().replace('_', " "));
        let steps = ed.session.history().0;
        let w = ed
            .new_plain(kind, &name)
            .unwrap_or_else(|e| panic!("{kind:?}: {e}"));
        // The server's database is read for the first and kept: the rest are made without its
        // file.
        if n == 0 {
            std::fs::remove_file(&sql).unwrap();
        }
        // One step, a quest giver's quest registered in it.
        assert_eq!(ed.session.history().0, steps + 1, "{kind:?}");
        assert!(
            ed.editor.open && ed.editor.weenie() == Some(&w),
            "the editor on it"
        );
        assert_eq!(ed.panels.weenie.as_ref(), Some(&w), "the spawn tool's");
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
        assert_eq!(Kind::of(&doc), kind.kind(), "{text}");
        if kind == NewKind::Sign {
            assert_eq!(
                doc.property(lyceum_foundry::content::PropertyKind::String, 16)
                    .as_deref()
                    .map(lyceum_foundry::content::unquote),
                Some(name.clone()),
                "a sign reads its name"
            );
        }
        // Its SQL passes every check the pack makes of its content, but that a plain portal leads
        // nowhere until its card says where.
        let stem = file.file_stem().unwrap().to_string_lossy().into_owned();
        let said: Vec<_> = ed
            .faults
            .iter()
            .filter(|f| f.text.contains(&stem))
            .filter(|f| {
                !(kind == NewKind::Portal && f.warning && f.text.contains("no destination"))
            })
            .collect();
        assert!(said.is_empty(), "{kind:?}: {said:#?}\n{text}");
        // The form by what it is for draws, and one is set down in the world.
        ed.frame();
        // What an NPC and a quest giver say is on their Emotes page, a line each: the greeting;
        // the ask after the quest, the thanks, the stamp and what it asks.
        let lines: &[&str] = match kind {
            NewKind::Talker => &["action:0:0"],
            NewKind::QuestGiver => &["action:0:0", "action:1:0", "action:2:0", "action:2:1"],
            _ => &[],
        };
        if !lines.is_empty() {
            ed.editor.page = Page::Section(Section::Emotes);
            for _ in 0..2 {
                ed.frame();
            }
            for k in lines {
                assert!(
                    ed.editor.row_buttons.iter().any(|(r, _)| r == k),
                    "{kind:?}: {k}"
                );
            }
            capture(&mut ed, dir.path());
            std::fs::copy(
                dir.path().join("frame.png"),
                shots("new-weenies").join(format!("{}-emotes.png", kind.key())),
            )
            .unwrap();
            ed.editor.page = Page::default();
        }
        assert!(ed.spawns_of(&w).is_empty());
        spawn_of(&mut ed, &w);
        assert_eq!(ed.spawns_of(&w).len(), 1, "{kind:?}");
    }
    assert!(matches!(ed.session.quest("a-favor"), Some((_, Some(_)))));
    // A copy of the server's portal, named as typed: it is listed by that name, not its symbol.
    let copy = ed
        .new_copy(&lyceum_foundry::edit::Weenie::Class(371), "Coolest Dungeon")
        .unwrap();
    assert_eq!(
        ed.server_database_reads(),
        1,
        "the server's database read once"
    );
    assert_eq!(
        copy,
        lyceum_foundry::edit::Weenie::Symbol("holtburg:coolest-dungeon".into())
    );
    let entry = ed.weenies.iter().find(|e| e.weenie == copy).unwrap();
    assert_eq!(entry.name, "Coolest Dungeon");
    // A portal sent into a dungeon; a vendor's trade changed through its form.
    let portal = lyceum_foundry::edit::Weenie::Symbol("holtburg:test-portal".into());
    ed.panels.weenie = Some(portal.clone());
    ed.selection = None;
    ed.destination_dungeon(&portal, 0x019E).unwrap();
    let (file, _) = ed.session.weenie_file(&portal).unwrap();
    assert!(ed.session.sql_text(&file).unwrap().contains("0x019E0100"));
    let vendor = lyceum_foundry::edit::Weenie::Symbol("holtburg:test-vendor".into());
    ed.panels.weenie = Some(vendor.clone());
    ed.form_edit(
        &vendor,
        &lyceum_editor::app::FormEdit::Property {
            kind: lyceum_foundry::content::PropertyKind::Int,
            id: 74,
            value: Some("8".into()),
        },
    )
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

/// The server's weenies of several kinds: a drudge placed in Holtburg, a sword, a hauberk and a
/// chest that spawns.
const SERVER_KINDS: &str = "\
INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES (90005, 'drudgeskulker', 10, '2005-02-09 10:00:00')
     , (90011, 'longsword', 6, '2005-02-09 10:00:00')
     , (90012, 'hauberk', 2, '2005-02-09 10:00:00')
     , (90013, 'chest', 20, '2005-02-09 10:00:00');
INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES (90005, 1, 'Drudge Skulker'), (90011, 1, 'Longsword'), (90012, 1, 'Chainmail Hauberk')
     , (90013, 1, 'Chest');
INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES (90005, 1, 0x020007DD), (90011, 1, 0x02000181), (90012, 1, 0x02000181)
     , (90013, 1, 0x0200040B);
INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES (90005, 25, 5), (90005, 2, 3), (90005, 146, 120)
     , (90011, 1, 1), (90011, 9, 1048576), (90011, 44, 14), (90011, 45, 1), (90011, 48, 44)
     , (90011, 49, 40), (90011, 158, 7), (90011, 159, 1), (90011, 160, 15), (90011, 19, 450)
     , (90011, 5, 270)
     , (90012, 1, 2), (90012, 9, 1536), (90012, 4, 3072), (90012, 28, 120), (90012, 19, 900)
     , (90013, 81, 3), (90013, 82, 1), (90013, 6, 120), (90013, 38, 100);
INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES (90005, 31, 20), (90005, 64, 1), (90005, 65, 0.5), (90005, 66, 0.75), (90005, 13, 0.9)
     , (90011, 22, 0.5), (90011, 62, 1.1), (90011, 29, 1.05)
     , (90012, 13, 1), (90012, 14, 0.8), (90012, 15, 1.2), (90012, 16, 0.6), (90012, 17, 0.5)
     , (90012, 18, 0.6), (90012, 19, 0.4)
     , (90013, 41, 300), (90013, 43, 5);
INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES (90005, 19, True), (90013, 3, True), (90013, 35, True);
INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`)
VALUES (0x7A9B4050, 90005, 0xA9B40011, 60, 60, 0, 1, 0, 0, 0, False, '2019-05-16 00:00:00');
";

/// The server's names for what the kinds above set, as `empyrean-import schema` writes them.
const SCHEMA_KINDS: &str = r#"{"properties":{"int":{"25":"Level","2":"CreatureType","44":"Damage","158":"WieldRequirements","160":"WieldDifficulty"},"float":{"13":"ArmorModVsSlash","64":"ResistSlash"},"string":{"1":"Name"},"did":{"1":"Setup"}},"enums":{"Skill":{"44":"HeavyWeapons","45":"LightWeapons"},"CreatureType":{"3":"Drudge"},"PaletteTemplate":{"39":"Black"}}}"#;

#[test]
fn each_kind_of_weenie_shows_its_own_sections_and_a_change_from_the_servers_is_marked_and_put_back()
{
    use lyceum_editor::app::{Drill, FormEdit, Page, Section, Tab};
    use lyceum_foundry::content::{PropertyKind, SqlDoc};
    use lyceum_foundry::edit::Weenie;
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-kinds");
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, SERVER_KINDS).unwrap();
    let schema = dir.path().join("schema.json");
    std::fs::write(&schema, SCHEMA_KINDS).unwrap();
    let mut ed = editor_with(
        &world,
        &[
            "--weenies",
            sql.to_str().unwrap(),
            "--schema",
            schema.to_str().unwrap(),
            "--size",
            "1600x1000",
        ],
    );
    let shots = shots("weenie-kinds");
    let page_at = |ed: &EditorApp, page: Page| {
        ed.editor
            .page_rects
            .iter()
            .find(|(p, _)| *p == page)
            .map(|(_, r)| *r)
            .unwrap_or_else(|| panic!("{page:?} listed"))
    };
    let has = |ed: &EditorApp, key: &str| ed.form_fields.iter().any(|(k, _)| k == key);
    // Each kind: the Definition pane's lists, the editor's pages, and the page that is its own.
    let wielding = Page::Section(Section::Wearing);
    for (class, name, lists, pages, page, keys) in [
        (
            90011,
            "sword",
            &[Drill::Spells, Drill::Appears, Drill::UsedBy][..],
            &[
                Section::Basic,
                Section::Appearance,
                Section::Wearing,
                Section::Weapon,
                Section::Magic,
                Section::Emotes,
            ][..],
            Page::Section(Section::Weapon),
            &["int:44", "float:22", "int:49", "float:62", "float:29"][..],
        ),
        (
            90012,
            "armor",
            &[Drill::Spells, Drill::Appears, Drill::UsedBy][..],
            &[
                Section::Basic,
                Section::Appearance,
                Section::Wearing,
                Section::Magic,
                Section::Emotes,
            ][..],
            Page::Section(Section::Wearing),
            &[
                "int:28",
                "float:13",
                "float:14",
                "float:15",
                "float:16",
                "float:17",
                "float:18",
                "float:19",
                "float:165",
            ][..],
        ),
        (
            90013,
            "chest",
            &[Drill::Holds, Drill::Spawns, Drill::Appears, Drill::UsedBy][..],
            &[
                Section::Basic,
                Section::Lock,
                Section::Holds,
                Section::Spawns,
                Section::Emotes,
            ][..],
            Page::Section(Section::Spawns),
            &["int:81", "int:82", "float:41", "float:43"][..],
        ),
        (
            90005,
            "creature",
            &[
                Drill::Wields,
                Drill::Skills,
                Drill::BodyParts,
                Drill::Spells,
                Drill::Emotes,
                Drill::Appears,
                Drill::UsedBy,
            ][..],
            &[
                Section::Basic,
                Section::Appearance,
                Section::Creature,
                Section::Attributes,
                Section::Skills,
                Section::BodyParts,
                Section::Protection,
                Section::Magic,
                Section::Holds,
                Section::Emotes,
            ][..],
            Page::Section(Section::Creature),
            &["int:25", "int:146", "float:31", "float:64", "float:65"][..],
        ),
    ] {
        let w = Weenie::Class(class);
        ed.server_class_now(class).unwrap().expect("its rows");
        ed.selection = None;
        ed.panels.weenie = Some(w.clone());
        ed.editor.open = false;
        ed.show_tab(Tab::Definition);
        for _ in 0..3 {
            ed.frame();
        }
        let drawn: Vec<Drill> = ed.definition.drill_rects.iter().map(|(d, _)| *d).collect();
        assert_eq!(drawn, lists, "{name}: the pane's lists");
        capture(&mut ed, dir.path());
        std::fs::copy(
            dir.path().join("frame.png"),
            shots.join(format!("{name}-definition.png")),
        )
        .unwrap();
        // The editor lists its pages, then Advanced; its own page shows its fields.
        ed.open_editor(w.clone());
        for _ in 0..2 {
            ed.frame();
        }
        let listed: Vec<Page> = ed.editor.page_rects.iter().map(|(p, _)| *p).collect();
        let want: Vec<Page> = pages
            .iter()
            .map(|s| Page::Section(*s))
            .chain([Page::Advanced])
            .collect();
        assert_eq!(listed, want, "{name}: the editor's pages");
        let r = page_at(&ed, page);
        click(&mut ed, (r.center().x, r.center().y));
        ed.frame();
        for key in keys {
            assert!(has(&ed, key), "{name}: {key}");
        }
        capture(&mut ed, dir.path());
        std::fs::copy(
            dir.path().join("frame.png"),
            shots.join(format!("{name}-editor.png")),
        )
        .unwrap();
        // A sword's wielding needs a level: the level asked for, no skill.
        if class == 90011 {
            let r = page_at(&ed, wielding);
            click(&mut ed, (r.center().x, r.center().y));
            ed.frame();
            assert!(has(&ed, "int:160") && !has(&ed, "int:159"));
        }
        assert_eq!(
            ed.session.history(),
            (0, 0),
            "{name}: looking changes nothing"
        );
    }
    // The drudge changed in this world: its level differs from the server's, and is marked on
    // its field and its page.
    let drudge = Weenie::Class(90005);
    change_in_world(&mut ed, 90005);
    ed.form_edit(
        &drudge,
        &FormEdit::Property {
            kind: PropertyKind::Int,
            id: 25,
            value: Some("7".into()),
        },
    )
    .unwrap();
    for _ in 0..3 {
        ed.frame();
    }
    assert_eq!(ed.editor.page, Page::Section(Section::Creature));
    let marked: Vec<&str> = ed
        .editor
        .changed_marks
        .iter()
        .map(|(k, _)| k.as_str())
        .collect();
    assert_eq!(marked, ["int:25"]);
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots.join("creature-changed.png"),
    )
    .unwrap();
    // Put back by a right click on its mark: the server's level, as one step.
    let mark = ed.editor.changed_marks[0].1;
    ed.queue(HostEvent::CursorMoved {
        x: f64::from(mark.center().x),
        y: f64::from(mark.center().y),
    });
    for pressed in [true, false] {
        ed.queue(HostEvent::MouseInput {
            button: MouseButton::Right,
            pressed,
        });
        ed.frame();
    }
    for _ in 0..4 {
        ed.frame();
    }
    let button = ed.editor.put_back_button.expect("its menu's Put back");
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots.join("creature-put-back.png"),
    )
    .unwrap();
    click(&mut ed, (button.center().x, button.center().y));
    ed.frame();
    let (file, _) = ed.session.weenie_file(&drudge).unwrap();
    let doc = SqlDoc::parse(&ed.session.sql_text(&file).unwrap());
    assert_eq!(doc.property(PropertyKind::Int, 25).as_deref(), Some("5"));
    assert_eq!(ed.session.history(), (3, 0), "copied, changed, put back");
    assert!(ed.editor.changed_marks.is_empty());
    // The pack's guide, an NPC: its emotes' page opens on its rows.
    ed.open_editor(Weenie::Symbol("holtburg:guide".into()));
    ed.frame();
    let r = page_at(&ed, Page::Section(Section::Emotes));
    click(&mut ed, (r.center().x, r.center().y));
    for _ in 0..2 {
        ed.frame();
    }
    capture(&mut ed, dir.path());
    std::fs::copy(dir.path().join("frame.png"), shots.join("npc-emotes.png")).unwrap();
    assert!(has(&ed, "emote:0:chance"), "its first set, with its chance");
}

#[test]
fn an_armors_equipment_page_ticks_its_dropdowns_a_step_each_and_puts_a_wield_need_back_unset() {
    use lyceum_editor::app::{FormEdit, Page, Section};
    use lyceum_foundry::content::{bare, PropertyKind, SqlDoc};
    use lyceum_foundry::edit::Weenie;
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-equipment");
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, SERVER_KINDS).unwrap();
    let schema = dir.path().join("schema.json");
    std::fs::write(&schema, SCHEMA_KINDS).unwrap();
    let mut ed = editor_with(
        &world,
        &[
            "--weenies",
            sql.to_str().unwrap(),
            "--schema",
            schema.to_str().unwrap(),
            "--size",
            "1600x1000",
        ],
    );
    let shots = shots("weenie-kinds");
    let armor = Weenie::Class(90012);
    ed.server_class_now(90012).unwrap().expect("its rows");
    change_in_world(&mut ed, 90012);
    ed.editor.page = Page::Section(Section::Wearing);
    for _ in 0..3 {
        ed.frame();
    }
    let int = |ed: &mut EditorApp, id: u32| -> Option<i64> {
        let (file, _) = ed.session.weenie_file(&armor).unwrap();
        SqlDoc::parse(&ed.session.sql_text(&file).unwrap())
            .property(PropertyKind::Int, id)
            .and_then(|v| bare(&v).trim().parse().ok())
    };
    // Its protection is on its equipment's page, and what it covers is a dropdown too.
    let has = |ed: &EditorApp, key: &str| ed.form_fields.iter().any(|(k, _)| k == key);
    assert!(has(&ed, "int:28") && has(&ed, "float:13"));
    drawn(&ed.editor.add_buttons, "int:4");
    // Where it is worn: a dropdown of check boxes that stays open while two are ticked, a step
    // each.
    assert_eq!(int(&mut ed, 9), Some(1536));
    let steps = ed.session.history().0;
    let combo = drawn(&ed.editor.add_buttons, "int:9");
    click(&mut ed, (combo.center().x, combo.center().y));
    for _ in 0..2 {
        ed.frame();
    }
    for bit in [0x0000_0001_u32, 0x0000_0100] {
        let r = drawn(&ed.editor.picker_rows, &format!("int:9:{bit:08X}"));
        click(&mut ed, (r.center().x, r.center().y));
        for _ in 0..2 {
            ed.frame();
        }
    }
    assert_eq!(int(&mut ed, 9), Some(1536 | 0x101));
    assert_eq!(ed.session.history().0, steps + 2, "a step each");
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots.join("armor-worn-on.png"),
    )
    .unwrap();
    // Closed by a click beside it, it counts what is ticked.
    let page = ed
        .editor
        .page_rects
        .iter()
        .find(|(p, _)| *p == Page::Section(Section::Wearing))
        .map(|(_, r)| *r)
        .expect("its page listed");
    click(&mut ed, (page.center().x, page.center().y));
    for _ in 0..2 {
        ed.frame();
    }
    assert!(
        !ed.editor
            .picker_rows
            .iter()
            .any(|(k, _)| k.starts_with("int:9:")),
        "closed"
    );
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots.join("armor-equipment.png"),
    )
    .unwrap();
    // Needing level 3 to wield, which the server's armor leaves unset: put back by its mark's
    // menu, the need and its level taken away as one step, its "of" left as it was, unset.
    ed.form_edits(
        &armor,
        &[
            FormEdit::Property {
                kind: PropertyKind::Int,
                id: 158,
                value: Some("7".into()),
            },
            FormEdit::Property {
                kind: PropertyKind::Int,
                id: 160,
                value: Some("3".into()),
            },
        ],
    )
    .unwrap();
    for _ in 0..3 {
        ed.frame();
    }
    let steps = ed.session.history().0;
    put_back_by_mark(&mut ed, "flow:wield_needs");
    assert_eq!(ed.session.history().0, steps + 1, "{}", ed.panels.message);
    assert_eq!((int(&mut ed, 158), int(&mut ed, 160)), (None, None));
    ed.frame();
    assert!(!ed
        .editor
        .changed_marks
        .iter()
        .any(|(k, _)| k == "flow:wield_needs"));
}

#[test]
fn advanced_adds_a_property_and_a_tables_row_by_its_boxes_and_add_each_as_one_step() {
    use lyceum_editor::app::Page;
    use lyceum_foundry::content::{PropertyKind, SqlDoc};
    use lyceum_foundry::edit::Weenie;
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-advanced-add");
    let schema = dir.path().join("schema.json");
    std::fs::write(&schema, SCHEMA).unwrap();
    let mut ed = editor_with(
        &world,
        &["--schema", schema.to_str().unwrap(), "--size", "1600x1100"],
    );
    let guide = Weenie::Symbol("holtburg:guide".into());
    ed.open_editor(guide.clone());
    ed.editor.page = Page::Advanced;
    // Every property and table, those its other pages show too.
    ed.editor.advanced_all = true;
    for _ in 0..3 {
        ed.frame();
    }
    let doc = |ed: &mut EditorApp| {
        let (file, _) = ed.session.weenie_file(&guide).unwrap();
        SqlDoc::parse(&ed.session.sql_text(&file).unwrap())
    };
    let into = |ed: &mut EditorApp, key: &str, text: &str, enter: bool| {
        let r = drawn(&ed.form_fields, key);
        type_in(ed, r, text, enter);
    };
    // A fold opened or closed by a click on its heading, drawn until it has finished opening
    // or closing, so what is under it stands still.
    let fold = |ed: &mut EditorApp, key: &str| {
        let (r, was) = ed
            .editor
            .folds
            .iter()
            .find(|(k, ..)| k == key)
            .map(|(_, r, o)| (*r, *o))
            .unwrap_or_else(|| panic!("{key} drawn"));
        click(ed, (r.center().x, r.center().y));
        let done = if was > 0.5 { 0.0 } else { 1.0 };
        for _ in 0..10_000 {
            if ed
                .editor
                .folds
                .iter()
                .any(|(k, _, o)| k == key && (*o - done).abs() < f32::EPSILON)
            {
                return;
            }
            ed.frame();
        }
        panic!("{key} never finished");
    };
    // A property by its number, then one by the server's name of it, Enter in its value adding
    // each.
    into(&mut ed, "add:int:name", "146", false);
    into(&mut ed, "add:int:value", "120", true);
    assert_eq!(
        doc(&mut ed).property(PropertyKind::Int, 146).as_deref(),
        Some("120")
    );
    into(&mut ed, "add:int:name", "Level", false);
    into(&mut ed, "add:int:value", "12", true);
    assert_eq!(
        doc(&mut ed).property(PropertyKind::Int, 25).as_deref(),
        Some("12")
    );
    assert_eq!(ed.session.history(), (2, 0));
    // A name the server has no property of is said so, and nothing changes.
    into(&mut ed, "add:int:name", "Bogus", false);
    into(&mut ed, "add:int:value", "1", true);
    assert!(ed.panels.message.contains("Bogus"), "{}", ed.panels.message);
    assert_eq!(ed.session.history(), (2, 0));
    // Folded away, the tables' rows come into view.
    for kind in ["int", "did"] {
        fold(&mut ed, kind);
    }
    // A spell by its id and chance.
    fold(&mut ed, "weenie_properties_spell_book");
    into(
        &mut ed,
        "add:weenie_properties_spell_book:spell",
        "1",
        false,
    );
    into(
        &mut ed,
        "add:weenie_properties_spell_book:probability",
        "2.5",
        true,
    );
    let d = doc(&mut ed);
    let book = d.rows("weenie_properties_spell_book");
    assert_eq!(book.len(), 1);
    assert_eq!(
        (d.cell(book[0], "spell"), d.cell(book[0], "probability")),
        (Some("1".into()), Some("2.5".into()))
    );
    // What it carries, by its wcid and Add; a wcid that is none is refused.
    fold(&mut ed, "weenie_properties_spell_book");
    fold(&mut ed, "weenie_properties_create_list");
    into(
        &mut ed,
        "add:weenie_properties_create_list:weenie_Class_Id",
        "bogus",
        true,
    );
    assert_eq!(ed.session.history(), (3, 0), "{}", ed.panels.message);
    into(
        &mut ed,
        "add:weenie_properties_create_list:weenie_Class_Id",
        "{{wcid:guide}}",
        false,
    );
    let add = drawn(&ed.editor.add_buttons, "add:weenie_properties_create_list");
    click(&mut ed, (add.center().x, add.center().y));
    ed.frame();
    let d = doc(&mut ed);
    let list = d.rows("weenie_properties_create_list");
    assert_eq!(
        d.cell(*list.last().unwrap(), "weenie_Class_Id").as_deref(),
        Some("{{wcid:guide}}")
    );
    // A table with no editor of its own: a palette, every column by its own name.
    fold(&mut ed, "weenie_properties_create_list");
    fold(&mut ed, "weenie_properties_palette");
    into(
        &mut ed,
        "add:weenie_properties_palette:sub_Palette_Id",
        "0x04000BEF",
        false,
    );
    into(&mut ed, "add:weenie_properties_palette:length", "12", true);
    let d = doc(&mut ed);
    let palette = d.rows("weenie_properties_palette");
    assert_eq!(palette.len(), 1);
    assert_eq!(
        ["sub_Palette_Id", "offset", "length"].map(|c| d.cell(palette[0], c)),
        [
            Some("0x04000BEF".into()),
            Some("0".into()),
            Some("12".into())
        ]
    );
    assert_eq!(ed.session.history(), (5, 0));
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("weenie-editor").join("advanced-add.png"),
    )
    .unwrap();
}

#[test]
fn new_weenie_makes_a_plain_one_or_a_copy_of_any_weenie_as_typed_and_the_editor_opens_on_it() {
    use lyceum_editor::app::NewKind;
    use lyceum_foundry::content::{unquote, PropertyKind, SqlDoc};
    use lyceum_foundry::edit::Weenie;
    let _g = gpu_lock();
    let (dir, world) = sample("editor-creator");
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, SERVER_BLANKS).unwrap();
    let mut ed = editor_with(
        &world,
        &["--weenies", sql.to_str().unwrap(), "--size", "1280x1000"],
    );
    let guide = Weenie::Symbol("holtburg:guide".into());
    let shot = |ed: &mut EditorApp, name: &str| {
        capture(ed, dir.path());
        std::fs::copy(dir.path().join("frame.png"), shots("new-weenie").join(name)).unwrap();
    };
    let center = |r: Option<egui::Rect>| {
        let r = r.expect("drawn");
        (r.center().x, r.center().y)
    };
    let text_of = |ed: &mut EditorApp, w: &Weenie| {
        let file = ed.session.weenie_file(w).expect("its file").0;
        ed.session.sql_text(&file).unwrap()
    };
    // Plain: a sign, named as typed.
    // Laid out over the window once it is measured: a frame to size it, one to draw it.
    let open = |ed: &mut EditorApp, copy_of: Option<Weenie>| {
        ed.open_creator(copy_of);
        for _ in 0..3 {
            ed.frame();
        }
        assert!(ed.editor.creator.open);
    };
    open(&mut ed, None);
    assert!(!ed.editor.creator.copy);
    ed.editor.creator.kind = NewKind::Sign;
    let name = ed.editor.creator.name_field.unwrap();
    type_in(&mut ed, name, "Holtburg: 2 miles", false);
    assert_eq!(ed.editor.creator.name, "Holtburg: 2 miles");
    shot(&mut ed, "plain.png");
    let steps = ed.session.history().0;
    let at = center(ed.editor.creator.make_button);
    click(&mut ed, at);
    let sign = Weenie::Symbol("holtburg:holtburg-2-miles".into());
    assert!(!ed.editor.creator.open, "{:?}", ed.editor.creator.problem);
    assert_eq!(ed.session.history().0, steps + 1);
    assert_eq!(ed.editor.weenie(), Some(&sign));
    assert_eq!(ed.panels.weenie.as_ref(), Some(&sign), "the spawn tool's");
    let doc = SqlDoc::parse(&text_of(&mut ed, &sign));
    assert_eq!(
        doc.property(PropertyKind::String, 16).map(|v| unquote(&v)),
        Some("Holtburg: 2 miles".into())
    );
    // The name taken: said in the dialog, which stays open, and nothing is written.
    open(&mut ed, None);
    let name = ed.editor.creator.name_field.unwrap();
    type_in(&mut ed, name, "Holtburg: 2 miles", true);
    assert!(ed.editor.creator.open && ed.editor.creator.problem.is_some());
    shot(&mut ed, "taken.png");
    assert_eq!(ed.session.history().0, steps + 1);
    let at = center(ed.editor.creator.cancel_button);
    click(&mut ed, at);
    assert!(!ed.editor.creator.open);
    // A copy of the pack's guide, from its menu: listed first, named as it is until renamed.
    open(&mut ed, Some(guide.clone()));
    let c = &ed.editor.creator;
    assert!(c.copy && c.of.as_ref() == Some(&guide));
    assert_eq!(c.kind, NewKind::Talker, "listed among its own kind, an NPC");
    assert_eq!(c.name, "Gribble the Painted");
    assert_eq!(c.rows.first().map(|(w, _)| w), Some(&guide), "shown first");
    let name = ed.editor.creator.name_field.unwrap();
    type_in(&mut ed, name, "Gribble Twin", false);
    shot(&mut ed, "copy.png");
    let at = center(ed.editor.creator.make_button);
    click(&mut ed, at);
    let twin = Weenie::Symbol("holtburg:gribble-twin".into());
    assert_eq!(
        ed.editor.weenie(),
        Some(&twin),
        "{:?}",
        ed.editor.creator.problem
    );
    let text = text_of(&mut ed, &twin);
    let doc = SqlDoc::parse(&text);
    assert_eq!(doc.class().as_deref(), Some("{{wcid:gribble-twin}}"));
    assert_eq!(
        doc.property(PropertyKind::String, 1).map(|v| unquote(&v)),
        Some("Gribble Twin".into())
    );
    assert_eq!(doc.emote_sets().len(), 1, "its emotes copied");
    assert!(text.contains("{{surface-texture:stripes}}"), "{text}");
    assert!(
        !text.contains("Gribble, a drudge"),
        "the guide's own comment"
    );
    let entry = ed.weenies.iter().find(|e| e.weenie == twin).unwrap();
    assert_eq!(entry.name, "Gribble Twin");
    // A copy of a server's class, chosen from the list by its kind and a search.
    open(&mut ed, None);
    ed.editor.creator.kind = NewKind::MeleeWeapon;
    let at = center(ed.editor.creator.copy_button);
    click(&mut ed, at);
    assert!(ed.editor.creator.copy);
    for _ in 0..2 {
        ed.frame();
    }
    shot(&mut ed, "copy-list.png");
    ed.editor.creator.search = "long".into();
    for _ in 0..2 {
        ed.frame();
    }
    let sword = Weenie::Class(351);
    let row = ed
        .editor
        .creator
        .rows
        .iter()
        .find(|(w, _)| *w == sword)
        .map(|(_, r)| *r);
    let at = center(row);
    click(&mut ed, at);
    assert_eq!(ed.editor.creator.of.as_ref(), Some(&sword));
    let name = ed.editor.creator.name_field.unwrap();
    type_in(&mut ed, name, "Fine Sword", true);
    let fine = Weenie::Symbol("holtburg:fine-sword".into());
    assert_eq!(
        ed.editor.weenie(),
        Some(&fine),
        "{:?}",
        ed.editor.creator.problem
    );
    assert!(text_of(&mut ed, &fine).contains("'fine-sword', 6,"));
    assert!(
        ed.session.weenie_file(&sword).is_none(),
        "the server's own unchanged"
    );
    assert_eq!(
        ed.server_database_reads(),
        1,
        "the server's database read once"
    );
    ed.frame();
    shot(&mut ed, "made.png");
}

/// The server's drudge with lists of every kind (a wield made always, a set drawn by chance, a
/// spell, attributes, vitals, a skill and two body parts), a camp spawning three things, and the
/// things they name.
const SERVER_LISTS: &str = "\
INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES (90020, 'drudgelister', 10, '2005-02-09 10:00:00')
     , (90021, 'camp', 1, '2005-02-09 10:00:00')
     , (90011, 'longsword', 6, '2005-02-09 10:00:00')
     , (90012, 'hauberk', 2, '2005-02-09 10:00:00');
INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES (90020, 1, 'Drudge Lister'), (90021, 1, 'Camp'), (90011, 1, 'Longsword')
     , (90012, 1, 'Chainmail Hauberk');
INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES (90020, 1, 0x020007DD), (90011, 1, 0x02000181), (90012, 1, 0x02000181);
INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES (90020, 25, 5), (90021, 81, 3), (90021, 82, 1);
INSERT INTO `weenie_properties_create_list` (`object_Id`, `destination_Type`, `weenie_Class_Id`, `stack_Size`, `palette`, `shade`, `try_To_Bond`)
VALUES (90020, 2, 90011, 1, 0, 0, False)
     , (90020, 10, 90012, 1, 0, 0.5, False)
     , (90020, 10, 0, 1, 0, 0.5, False);
INSERT INTO `weenie_properties_spell_book` (`object_Id`, `spell`, `probability`)
VALUES (90020, 1, 2.05);
INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES (90020, 1, 40, 0, 0), (90020, 2, 40, 0, 0);
INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES (90020, 1, 30, 0, 0, 50);
INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES (90020, 6, 0, 2, 0, 50, 0, 0);
INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES (90020, 0, 4, 2, 0.75, 10, 10, 10, 10, 10, 10, 10, 10, 0, 1, 0.33, 0, 0, 0.33, 0, 0, 0.33, 0, 0, 0.33, 0, 0)
     , (90020, 1, 4, 3, 0.75, 20, 20, 20, 20, 20, 20, 20, 20, 0, 2, 0.44, 0.17, 0, 0.44, 0.17, 0, 0.44, 0.17, 0, 0.44, 0.17, 0);
INSERT INTO `weenie_properties_generator` (`object_Id`, `probability`, `weenie_Class_Id`, `delay`, `init_Create`, `max_Create`, `when_Create`, `where_Create`, `stack_Size`, `palette_Id`, `shade`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`)
VALUES (90021, 0.25, 90020, 180, 1, 1, 1, 2, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL)
     , (90021, 0.5, 90011, 180, 1, 1, 1, 2, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL)
     , (90021, 1, 90012, 180, 1, 1, 1, 2, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL);
";

/// The server's names for the lists above, as `empyrean-import schema` writes them.
const SCHEMA_LISTS: &str = r#"{"properties":{"int":{"25":"Level"}},"enums":{"Skill":{"6":"MeleeDefense","15":"MagicDefense"},"RegenerationType":{"1":"Destruction","2":"PickUp"},"RegenLocationType":{"2":"Scatter","4":"Specific"},"Attribute":{"1":"Strength","2":"Endurance"},"Vital":{"1":"MaxHealth"}}}"#;

#[test]
fn a_weenies_lists_take_chances_shares_and_spells_by_name_and_are_put_back_as_the_servers() {
    use lyceum_editor::app::{Page, Section, Tab, TableRow};
    use lyceum_foundry::content::SqlDoc;
    use lyceum_foundry::edit::Weenie;
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-lists");
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, SERVER_LISTS).unwrap();
    let schema = dir.path().join("schema.json");
    std::fs::write(&schema, SCHEMA_LISTS).unwrap();
    let mut ed = editor_with(
        &world,
        &[
            "--weenies",
            sql.to_str().unwrap(),
            "--schema",
            schema.to_str().unwrap(),
            "--size",
            "1600x1000",
        ],
    );
    let shots = shots("weenie-lists");
    let shoot = |ed: &mut EditorApp, name: &str| {
        capture(ed, dir.path());
        std::fs::copy(dir.path().join("frame.png"), shots.join(name)).unwrap();
    };
    let page = |ed: &mut EditorApp, s: Section| {
        let r = ed
            .editor
            .page_rects
            .iter()
            .find(|(p, _)| *p == Page::Section(s))
            .map(|(_, r)| *r)
            .unwrap_or_else(|| panic!("{s:?} listed"));
        click(ed, (r.center().x, r.center().y));
        for _ in 0..2 {
            ed.frame();
        }
        assert_eq!(ed.editor.page, Page::Section(s));
    };
    let rows_of = |ed: &mut EditorApp, w: &Weenie, table: &str| -> Vec<TableRow> {
        let (file, _) = ed.session.weenie_file(w).expect("the pack's file");
        let doc = SqlDoc::parse(&ed.session.sql_text(&file).unwrap());
        lyceum_editor::app::table_rows(&doc, table)
    };
    let drudge = Weenie::Class(90020);
    ed.server_class_now(90020).unwrap().expect("its rows");
    change_in_world(&mut ed, 90020);
    for _ in 0..2 {
        ed.frame();
    }
    // What it wields, in Creature under what it has equipped: one always, then a set: the
    // hauberk at 50% and nothing at 50%.
    page(&mut ed, Section::Creature);
    shoot(&mut ed, "wields.png");
    // Whether each drops when it dies: the set's treasure does, the one wielded always does not;
    // what makes nothing is not marked.
    assert_eq!(
        ed.editor.drop_marks,
        [
            ("list:wields:0".to_owned(), false),
            ("list:wields:1".to_owned(), true)
        ]
    );
    type_into(&mut ed, "list:wields:1:chance", "80");
    let list = rows_of(&mut ed, &drudge, "weenie_properties_create_list");
    let shades: Vec<(f64, f64)> = list
        .iter()
        .map(|r| {
            (
                r.number("weenie_Class_Id").unwrap(),
                r.number("shade").unwrap(),
            )
        })
        .collect();
    assert_eq!(
        shades,
        [(90011.0, 0.0), (90012.0, 0.8), (0.0, 0.2)],
        "{list:?}"
    );
    // More than the set has room for: refused, saying how much it can have.
    let steps = ed.session.history();
    type_into(&mut ed, "list:wields:1:chance", "120");
    assert_eq!(ed.session.history(), steps, "nothing changed");
    assert!(ed.panels.message.contains("100"), "{}", ed.panels.message);
    // Marked changed from the server's; put back by its mark's menu.
    ed.frame();
    let mark = ed
        .editor
        .changed_marks
        .iter()
        .find(|(k, _)| k == "flow:wields")
        .map(|(_, r)| *r)
        .expect("the wields marked changed");
    shoot(&mut ed, "wields-changed.png");
    ed.queue(HostEvent::CursorMoved {
        x: f64::from(mark.center().x),
        y: f64::from(mark.center().y),
    });
    for pressed in [true, false] {
        ed.queue(HostEvent::MouseInput {
            button: MouseButton::Right,
            pressed,
        });
        ed.frame();
    }
    for _ in 0..4 {
        ed.frame();
    }
    let button = ed.editor.put_back_button.expect("its menu's Put back");
    click(&mut ed, (button.center().x, button.center().y));
    ed.frame();
    let back: Vec<f64> = rows_of(&mut ed, &drudge, "weenie_properties_create_list")
        .iter()
        .map(|r| r.number("shade").unwrap())
        .collect();
    assert_eq!(back, [0.0, 0.5, 0.5]);
    assert!(!ed
        .editor
        .changed_marks
        .iter()
        .any(|(k, _)| k == "flow:wields"));
    // Its Add… opens a picker that stays open while its search is clicked into and typed in; a
    // weenie clicked in it is added. The page is scrolled to its end first, where the Add… is in
    // view.
    let add = drawn(&ed.editor.add_buttons, "list:wields:add");
    hover(&mut ed, (add.center().x, add.min.y - 100.0));
    for _ in 0..10 {
        ed.queue(HostEvent::MouseWheel { notches: -1.0 });
        ed.frame();
    }
    let add = drawn(&ed.editor.add_buttons, "list:wields:add");
    click(&mut ed, (add.center().x, add.center().y));
    for _ in 0..2 {
        ed.frame();
    }
    let search = ed.editor.picker_search_field.expect("its search");
    type_in(&mut ed, search, "drudge", false);
    assert!(
        ed.editor.picker_search_field.is_some(),
        "still open after a click into its search"
    );
    assert_eq!(ed.editor.picker_search, "drudge");
    shoot(&mut ed, "wields-add.png");
    let (token, row) = ed
        .editor
        .picker_rows
        .first()
        .cloned()
        .expect("drudges listed");
    click(&mut ed, (row.center().x, row.center().y));
    for _ in 0..2 {
        ed.frame();
    }
    assert!(
        ed.editor.picker_search_field.is_none(),
        "closed on a choice"
    );
    let list = rows_of(&mut ed, &drudge, "weenie_properties_create_list");
    assert_eq!(list.len(), 4);
    assert!(list
        .iter()
        .any(|r| r.get("weenie_Class_Id") == Some(token.as_str())));
    ed.edit(&Edit::Undo).unwrap();
    // A spell added by name from the picker: its book grows by one.
    page(&mut ed, Section::Magic);
    let add = ed
        .editor
        .add_buttons
        .iter()
        .find(|(k, _)| k == "spell:add")
        .map(|(_, r)| *r)
        .expect("Add a spell");
    click(&mut ed, (add.center().x, add.center().y));
    for _ in 0..2 {
        ed.frame();
    }
    shoot(&mut ed, "spell-picker.png");
    let (spell, row) = ed
        .editor
        .picker_rows
        .first()
        .cloned()
        .expect("spells listed");
    click(&mut ed, (row.center().x, row.center().y));
    ed.frame();
    let book = rows_of(&mut ed, &drudge, "weenie_properties_spell_book");
    assert_eq!(book.len(), 2);
    assert_eq!(book[1].get("spell"), Some(spell.as_str()));
    // The first spell's chance to cast, in percent: written as the server reads it.
    type_into(&mut ed, "spell:0:chance", "30");
    let book = rows_of(&mut ed, &drudge, "weenie_properties_spell_book");
    assert!((book[0].number("probability").unwrap() - 2.3).abs() < 1e-9);
    shoot(&mut ed, "spells.png");
    // An attribute and a body part's damage typed in; a vital it lacks made.
    page(&mut ed, Section::Attributes);
    type_into(&mut ed, "attribute:1", "150");
    type_into(&mut ed, "vital:3:base", "80");
    let attributes = rows_of(&mut ed, &drudge, "weenie_properties_attribute");
    assert_eq!(attributes[0].get("init_Level"), Some("150"));
    let vitals = rows_of(&mut ed, &drudge, "weenie_properties_attribute_2nd");
    assert_eq!(vitals.len(), 2);
    assert_eq!(vitals[1].get("type"), Some("3"));
    assert_eq!(vitals[1].get("current_Level"), Some("80"));
    shoot(&mut ed, "attributes.png");
    page(&mut ed, Section::Skills);
    shoot(&mut ed, "skills.png");
    page(&mut ed, Section::BodyParts);
    type_into(&mut ed, "part:0:d_Val", "20");
    let parts = rows_of(&mut ed, &drudge, "weenie_properties_body_part");
    assert_eq!(parts[0].get("d_Val"), Some("20"));
    shoot(&mut ed, "body-parts.png");
    // The camp's spawns: shares in percent; the first made 50%, the others keeping theirs among
    // them, written back as running totals.
    let camp = Weenie::Class(90021);
    ed.server_class_now(90021).unwrap().expect("its rows");
    change_in_world(&mut ed, 90021);
    for _ in 0..2 {
        ed.frame();
    }
    page(&mut ed, Section::Spawns);
    type_into(&mut ed, "spawn:0:share", "50");
    let totals: Vec<f64> = rows_of(&mut ed, &camp, "weenie_properties_generator")
        .iter()
        .map(|r| r.number("probability").unwrap())
        .collect();
    let shares: Vec<f64> =
        lyceum_editor::app::generator_percents(&lyceum_editor::app::generator_draws(&totals))
            .into_iter()
            .flatten()
            .collect();
    let near = |a: f64, b: f64| (a - b).abs() < 1e-3;
    assert!(
        near(shares[0], 50.0) && near(shares[1], 50.0 / 3.0) && near(shares[2], 100.0 / 3.0),
        "{totals:?} {shares:?}"
    );
    assert!(near(totals[2], 1.0), "{totals:?}");
    shoot(&mut ed, "spawns.png");
    // When the first is made again: when it dies, then once it is gone again, each a step; the
    // others' rows written as they were.
    let when = |ed: &mut EditorApp| -> Vec<Option<String>> {
        rows_of(ed, &camp, "weenie_properties_generator")
            .iter()
            .map(|r| r.get("when_Create").map(str::to_owned))
            .collect()
    };
    let before = when(&mut ed);
    for (choice, want) in [("death", "4"), ("gone", "1")] {
        let steps = ed.session.history().0;
        let combo = drawn(&ed.editor.add_buttons, "spawn:0:when");
        click(&mut ed, (combo.center().x, combo.center().y));
        for _ in 0..2 {
            ed.frame();
        }
        if choice == "death" {
            shoot(&mut ed, "spawn-when.png");
        }
        let row = drawn(&ed.editor.picker_rows, &format!("spawn:0:when:{choice}"));
        click(&mut ed, (row.center().x, row.center().y));
        for _ in 0..2 {
            ed.frame();
        }
        let now = when(&mut ed);
        assert_eq!(now[0].as_deref(), Some(want), "{choice}");
        assert_eq!(now[1..], before[1..]);
        assert_eq!(ed.session.history().0, steps + 1, "{choice}: one step");
    }
    // Choosing what it says already writes nothing.
    let steps = ed.session.history();
    let combo = drawn(&ed.editor.add_buttons, "spawn:1:when");
    click(&mut ed, (combo.center().x, combo.center().y));
    for _ in 0..2 {
        ed.frame();
    }
    let row = drawn(&ed.editor.picker_rows, "spawn:1:when:gone");
    click(&mut ed, (row.center().x, row.center().y));
    ed.frame();
    assert_eq!(ed.session.history(), steps);
    // The Definition pane lists its wields with their chances.
    ed.selection = None;
    ed.panels.weenie = Some(drudge.clone());
    ed.editor.open = false;
    ed.show_tab(Tab::Definition);
    for _ in 0..3 {
        ed.frame();
    }
    let wields = ed
        .definition
        .drill_rects
        .iter()
        .find(|(d, _)| *d == lyceum_editor::app::Drill::Wields)
        .map(|(_, r)| *r)
        .expect("its wields counted");
    click(&mut ed, (wields.center().x, wields.center().y));
    for _ in 0..2 {
        ed.frame();
    }
    shoot(&mut ed, "definition-wields.png");
}

/// A server's database for the emotes, the findings and Find usages, as a dump writes its emotes
/// (each action naming its set by the set's row id): a greeter who says one of two things when
/// used and gives a longsword for a hauberk, a camp that spawns in two sets, a drudge that wields
/// the hauberk, and three more drudges' heads.
const SERVER_EMOTES: &str = "\
INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES (90040, 'greeter', 10, '2005-02-09 10:00:00')
     , (90041, 'camp', 1, '2005-02-09 10:00:00')
     , (90042, 'drudgewielder', 10, '2005-02-09 10:00:00')
     , (90043, 'drudgea', 10, '2005-02-09 10:00:00')
     , (90044, 'drudgeb', 10, '2005-02-09 10:00:00')
     , (90045, 'drudgec', 10, '2005-02-09 10:00:00')
     , (90011, 'longsword', 6, '2005-02-09 10:00:00')
     , (90012, 'hauberk', 2, '2005-02-09 10:00:00');
INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES (90040, 1, 'Greeter'), (90041, 1, 'Camp'), (90042, 1, 'Drudge Wielder')
     , (90011, 1, 'Longsword'), (90012, 1, 'Chainmail Hauberk');
INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES (90040, 19, False);
INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES (90040, 1, 0x020007DD), (90042, 1, 0x020007DD), (90011, 1, 0x02000181)
     , (90012, 1, 0x02000181);
INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES (90041, 81, 2), (90041, 82, 1);
INSERT INTO `weenie_properties_create_list` (`object_Id`, `destination_Type`, `weenie_Class_Id`, `stack_Size`, `palette`, `shade`, `try_To_Bond`)
VALUES (90042, 2, 90012, 1, 0, 0, False);
INSERT INTO `weenie_properties_generator` (`object_Id`, `probability`, `weenie_Class_Id`, `delay`, `init_Create`, `max_Create`, `when_Create`, `where_Create`, `stack_Size`, `palette_Id`, `shade`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`)
VALUES (90041, 0.5, 90040, 180, 1, 1, 1, 2, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL)
     , (90041, 1, 90011, 180, 1, 1, 1, 2, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL)
     , (90041, 0.2, 90012, 180, 1, 1, 1, 2, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL)
     , (90041, 1, 90042, 180, 1, 1, 1, 2, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL);
INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES (90042, 1, 4, 3, 0.75, 20, 20, 20, 20, 20, 20, 20, 20, 0, 2, 0.44, 0.17, 0, 0.44, 0.17, 0, 0.44, 0.17, 0, 0.44, 0.17, 0)
     , (90043, 0, 4, 7, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.33, 0, 0, 0.33, 0, 0, 0.33, 0, 0, 0.33, 0, 0)
     , (90044, 0, 4, 7, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.33, 0, 0, 0.33, 0, 0, 0.33, 0, 0, 0.33, 0, 0)
     , (90045, 0, 4, 9, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.33, 0, 0, 0.33, 0, 0, 0.33, 0, 0, 0.33, 0, 0);
INSERT INTO `weenie_properties_emote` (`id`, `object_Id`, `category`, `probability`, `weenie_Class_Id`, `style`, `substyle`, `quest`, `vendor_Type`, `min_Health`, `max_Health`)
VALUES (1, 90040, 7, 0.5, NULL, NULL, NULL, NULL, NULL, NULL, NULL)
     , (2, 90040, 7, 1, NULL, NULL, NULL, NULL, NULL, NULL, NULL)
     , (3, 90040, 6, 1, 90012, NULL, NULL, NULL, NULL, NULL, NULL);
INSERT INTO `weenie_properties_emote_action` (`id`, `emote_Id`, `order`, `type`, `delay`, `extent`, `motion`, `message`, `weenie_Class_Id`, `stack_Size`)
VALUES (1, 1, 0, 10, 0, 1, NULL, 'Hello.', NULL, NULL)
     , (2, 2, 0, 10, 0, 1, NULL, 'Well met.', NULL, NULL)
     , (3, 3, 0, 3, 0, 1, NULL, NULL, 90011, 1);
";

/// The server's names for the emotes above, as `empyrean-import schema` writes them.
const SCHEMA_EMOTES: &str = r#"{"properties":{"int":{"81":"MaxGeneratedObjects","82":"InitGeneratedObjects"},"string":{"1":"Name","16":"LongDesc"}},"enums":{"EmoteCategory":{"6":"Give","7":"Use"},"EmoteType":{"3":"Give","5":"Motion","10":"Tell"},"MotionCommand":{"318767229":"BowDeep"},"RegenerationType":{"1":"Destruction"},"RegenLocationType":{"2":"Scatter"}}}"#;

/// Type `lines` into the field of several lines `key` of weenie `w`'s form, Enter between them,
/// and keep them with Ctrl+Enter: the field asked for, which takes the keyboard, and everything in
/// it chosen first.
fn type_lines(ed: &mut EditorApp, w: &lyceum_foundry::edit::Weenie, key: &str, lines: &[&str]) {
    ed.editor.focus = Some((w.clone(), key.to_owned()));
    for _ in 0..3 {
        ed.frame();
    }
    assert!(
        ed.form_fields.iter().any(|(k, _)| k == key),
        "no field {key}"
    );
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
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            ed.queue(k(0x0D, true, None));
            ed.queue(k(0x0D, false, None));
        }
        for c in line.chars() {
            ed.queue(k(0x30, true, Some(&c.to_string())));
            ed.queue(k(0x30, false, None));
        }
        ed.frame();
    }
    ed.queue(k(0x11, true, None));
    ed.queue(k(0x0D, true, None));
    ed.queue(k(0x0D, false, None));
    ed.queue(k(0x11, false, None));
    ed.frame();
    ed.frame();
}

#[test]
#[allow(clippy::too_many_lines)]
fn emotes_read_as_lines_with_their_chances_a_finding_goes_to_its_field_and_find_usages_lists_who_names_a_weenie(
) {
    use lyceum_editor::app::{emote_chances, typing_key, Drill, Page, Section, Tab, TableRow};
    use lyceum_foundry::content::{unquote, PropertyKind, SqlDoc};
    use lyceum_foundry::edit::Weenie;
    use lyceum_foundry::sql_ids::Use;
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-emotes-usages");
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, SERVER_EMOTES).unwrap();
    let schema = dir.path().join("schema.json");
    std::fs::write(&schema, SCHEMA_EMOTES).unwrap();
    let mut ed = editor_with(
        &world,
        &[
            "--weenies",
            sql.to_str().unwrap(),
            "--schema",
            schema.to_str().unwrap(),
            "--size",
            "1600x1000",
        ],
    );
    let shots = shots("weenie-emotes");
    let shoot = |ed: &mut EditorApp, name: &str| {
        capture(ed, dir.path());
        std::fs::copy(dir.path().join("frame.png"), shots.join(name)).unwrap();
    };
    let page = |ed: &mut EditorApp, s: Section| {
        let r = ed
            .editor
            .page_rects
            .iter()
            .find(|(p, _)| *p == Page::Section(s))
            .map(|(_, r)| *r)
            .unwrap_or_else(|| panic!("{s:?} listed"));
        click(ed, (r.center().x, r.center().y));
        for _ in 0..2 {
            ed.frame();
        }
        assert_eq!(ed.editor.page, Page::Section(s));
    };
    let at = |list: &[(String, egui::Rect)], key: &str| {
        list.iter()
            .find(|(k, _)| k == key)
            .map(|(_, r)| *r)
            .unwrap_or_else(|| panic!("{key} drawn"))
    };
    let press = |ed: &mut EditorApp, r: egui::Rect| {
        click(ed, (r.center().x, r.center().y));
        for _ in 0..2 {
            ed.frame();
        }
    };
    let doc_of = |ed: &mut EditorApp, w: &Weenie| {
        let (file, _) = ed.session.weenie_file(w).expect("the pack's file");
        SqlDoc::parse(&ed.session.sql_text(&file).unwrap())
    };
    let open = |ed: &mut EditorApp, class: u32| {
        ed.server_class_now(class).unwrap().expect("its rows");
        change_in_world(ed, class);
        for _ in 0..2 {
            ed.frame();
        }
    };
    let near = |a: f64, b: f64| (a - b).abs() < 1e-6;

    // The greeter's emotes: two greetings, each half the time, and its answer to a hauberk.
    let greeter = Weenie::Class(90040);
    open(&mut ed, 90040);
    page(&mut ed, Section::Emotes);
    shoot(&mut ed, "emotes.png");
    let chances = |ed: &mut EditorApp| -> Vec<f64> {
        emote_chances(&doc_of(ed, &greeter))
            .iter()
            .map(|c| c.chance)
            .collect()
    };
    let c = chances(&mut ed);
    assert!(
        near(c[0], 0.5) && near(c[1], 0.5) && near(c[2], 1.0),
        "{c:?}"
    );
    for k in ["action:0:0", "action:1:0", "action:2:0"] {
        at(&ed.editor.row_buttons, k);
    }
    // The first made 30%: the other keeps its 50%, and 20% of the time it says nothing.
    type_into(&mut ed, "emote:0:chance", "30");
    let c = chances(&mut ed);
    assert!(near(c[0], 0.3) && near(c[1], 0.5), "{c:?}");
    // More than there is room for: refused, saying how much it can have.
    let steps = ed.session.history();
    type_into(&mut ed, "emote:1:chance", "80");
    assert_eq!(ed.session.history(), steps, "nothing changed");
    assert!(ed.panels.message.contains("70"), "{}", ed.panels.message);
    // Another greeting, from Add emotes for: it takes the 20% that said nothing, written beside
    // the other two before the answer to a hauberk.
    ed.frame();
    let add = at(&ed.editor.add_buttons, "emote:add");
    press(&mut ed, add);
    let used = at(&ed.editor.picker_rows, "emote-category:7");
    press(&mut ed, used);
    let c = chances(&mut ed);
    assert_eq!(c.len(), 4);
    assert!(near(c[2], 0.2), "{c:?}");
    let doc = doc_of(&mut ed, &greeter);
    let categories: Vec<String> = doc
        .emote_sets()
        .iter()
        .map(|s| doc.cell(s.emote, "category").unwrap())
        .collect();
    assert_eq!(categories, ["7", "7", "7", "6"]);
    // A bow added to the first greeting by its kind, and its words typed over two lines.
    let add = at(&ed.editor.add_buttons, "emote:0:add-action");
    press(&mut ed, add);
    let bow = at(&ed.editor.picker_rows, "action-kind:5");
    press(&mut ed, bow);
    let doc = doc_of(&mut ed, &greeter);
    let first = doc.emote_sets()[0];
    let types: Vec<String> = doc
        .actions(&first)
        .iter()
        .map(|a| doc.cell(*a, "type").unwrap())
        .collect();
    assert_eq!(types, ["10", "5"]);
    let line = at(&ed.editor.row_buttons, "action:0:0");
    press(&mut ed, line);
    type_lines(
        &mut ed,
        &greeter,
        "action:0:0:message",
        &["Hello there.", "Welcome to the camp."],
    );
    let doc = doc_of(&mut ed, &greeter);
    let said = doc
        .cell(doc.actions(&doc.emote_sets()[0])[0], "message")
        .unwrap();
    assert_eq!(unquote(&said), "Hello there.\nWelcome to the camp.");
    shoot(&mut ed, "emote-open.png");
    // Changed from the server's, the emotes are put back whole by their mark's menu.
    let mark = at(&ed.editor.changed_marks, "flow:emotes");
    ed.queue(HostEvent::CursorMoved {
        x: f64::from(mark.center().x),
        y: f64::from(mark.center().y),
    });
    for pressed in [true, false] {
        ed.queue(HostEvent::MouseInput {
            button: MouseButton::Right,
            pressed,
        });
        ed.frame();
    }
    for _ in 0..4 {
        ed.frame();
    }
    let button = ed.editor.put_back_button.expect("its menu's Put back");
    press(&mut ed, button);
    let c = chances(&mut ed);
    assert_eq!(c.len(), 3, "{c:?}");
    assert!(near(c[0], 0.5) && near(c[1], 0.5));
    assert!(!ed
        .editor
        .changed_marks
        .iter()
        .any(|(k, _)| k == "flow:emotes"));
    // Its description, written over two lines and kept with Ctrl+Enter.
    page(&mut ed, Section::Basic);
    type_lines(
        &mut ed,
        &greeter,
        "string:16",
        &["A greeter of the camp.", "Mostly harmless."],
    );
    let doc = doc_of(&mut ed, &greeter);
    assert_eq!(
        doc.property(PropertyKind::String, 16)
            .map(|v| unquote(&v))
            .as_deref(),
        Some("A greeter of the camp.\nMostly harmless.")
    );
    shoot(&mut ed, "description.png");
    // The Definition pane says its emotes the same way, read only.
    ed.selection = None;
    ed.panels.weenie = Some(greeter.clone());
    ed.show_tab(Tab::Definition);
    for _ in 0..3 {
        ed.frame();
    }
    let emotes = ed
        .definition
        .drill_rects
        .iter()
        .find(|(d, _)| *d == Drill::Emotes)
        .map(|(_, r)| *r)
        .expect("its emotes counted");
    press(&mut ed, emotes);
    assert_eq!(ed.definition.drill_of(&greeter), Some(Drill::Emotes));
    shoot(&mut ed, "definition-emotes.png");

    // The camp: made to make more at the start than it keeps at most, warned of in the footer,
    // whose finding goes to the field.
    let camp = Weenie::Class(90041);
    open(&mut ed, 90041);
    page(&mut ed, Section::Spawns);
    type_into(&mut ed, "int:82", "5");
    for _ in 0..2 {
        ed.frame();
    }
    let finding = ed
        .weenie_findings(&camp)
        .into_iter()
        .find(|f| f.warning && f.text.contains("makes 5 at the start"))
        .expect("the warning");
    shoot(&mut ed, "findings.png");
    page(&mut ed, Section::Basic);
    let row = ed
        .editor
        .finding_rows
        .iter()
        .find(|(l, _)| *l == finding.line)
        .map(|(_, r)| *r)
        .expect("the finding in the footer");
    press(&mut ed, row);
    assert_eq!(ed.editor.page, Page::Section(Section::Spawns));
    assert!(
        ed.panels
            .form_typing
            .contains_key(&typing_key(&camp, "int:82")),
        "the field it is about has the keyboard"
    );
    // Its spawns in two sets: a share that would join them is refused, saying so.
    page(&mut ed, Section::Spawns);
    shoot(&mut ed, "spawn-sets.png");
    let steps = ed.session.history();
    type_into(&mut ed, "spawn:2:share", "80");
    assert_eq!(ed.session.history(), steps, "nothing changed");
    assert!(ed.panels.message.contains("set 2"), "{}", ed.panels.message);

    // The drudge's new head: as the server's drudges typically have it.
    let wielder = Weenie::Class(90042);
    open(&mut ed, 90042);
    page(&mut ed, Section::BodyParts);
    let add = at(&ed.editor.add_buttons, "part:add");
    press(&mut ed, add);
    let head = at(&ed.editor.picker_rows, "part:0");
    press(&mut ed, head);
    let parts: Vec<TableRow> =
        lyceum_editor::app::table_rows(&doc_of(&mut ed, &wielder), "weenie_properties_body_part");
    let new = parts
        .iter()
        .find(|r| r.number("key") == Some(0.0))
        .expect("a head");
    assert_eq!(new.number("d_Val"), Some(7.0), "{new:?}");
    assert_eq!(new.number("h_l_f"), Some(0.33));

    // Find usages: the hauberk is answered to by the greeter, spawned by the camp (the pack's
    // copies of both) and wielded by the server's drudge.
    let hauberk = Weenie::Class(90012);
    let mut found: Vec<(Weenie, Use)> = ed.usages_of(&hauberk).to_vec();
    found.sort_by_key(|(w, _)| format!("{w:?}"));
    assert_eq!(
        found,
        [
            (
                Weenie::Class(90040),
                Use::Emote {
                    category: 6,
                    action: None
                }
            ),
            (Weenie::Class(90041), Use::Generator),
            (Weenie::Class(90042), Use::CreateList { destination: 2 }),
        ]
    );
    ed.selection = None;
    ed.panels.weenie = Some(hauberk.clone());
    ed.editor.open = false;
    ed.show_tab(Tab::Definition);
    for _ in 0..3 {
        ed.frame();
    }
    let used = ed
        .definition
        .drill_rects
        .iter()
        .find(|(d, _)| *d == Drill::UsedBy)
        .map(|(_, r)| *r)
        .expect("what names it, counted");
    press(&mut ed, used);
    assert_eq!(ed.definition.drill_of(&hauberk), Some(Drill::UsedBy));
    shoot(&mut ed, "used-by.png");
    // From the editor's menu, the pane is brought to the weenie asked about.
    let sword = Weenie::Class(90011);
    ed.find_usages(&sword);
    for _ in 0..2 {
        ed.frame();
    }
    assert_eq!(ed.form_weenie(), Some(sword.clone()));
    assert_eq!(ed.definition.drill_of(&sword), Some(Drill::UsedBy));
    assert_eq!(
        ed.usages_of(&sword).len(),
        2,
        "the greeter's gift and the camp"
    );
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

/// Hold key `vk` for `frames` frames: where the camera was before and after each.
fn fly_for(ed: &mut EditorApp, vk: usize, frames: u32) -> Vec<Vec3> {
    let k = |pressed| HostEvent::KeyboardInput {
        key: dereth_input::keys::Key::new(vk, 0),
        pressed,
        text: None,
    };
    ed.queue(k(true));
    let mut trail = vec![ed.camera.position];
    for _ in 0..frames {
        ed.frame();
        trail.push(ed.camera.position);
    }
    ed.queue(k(false));
    ed.frame();
    trail
}

#[test]
fn with_collide_on_the_flying_camera_slides_along_a_house_into_its_doorway_and_with_it_off_flies_through_the_house(
) {
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
        ed.set_speed(4); // 25 m/s
        ed.frame();
    };
    aim(&mut ed);
    ed.collide = true;
    let trail = fly_for(&mut ed, 0x57, 120);
    let end = ed.camera.position;
    // It met the house's front, slid along it, and went in by the doorway, whose door it passes:
    // it is in one of the house's rooms, against its far wall.
    let aside = trail
        .iter()
        .map(|p| (p.x - start.x).abs())
        .fold(0.0, f32::max);
    assert!(
        aside > 1.0,
        "slid aside along the front, {aside} m: {end:?}"
    );
    let room = ed.camera_cell().map(|(c, _)| c.0);
    assert!(
        room.is_some_and(|c| c >> 16 == u32::from(HOLTBURG) && c & 0xFFFF >= 0x100),
        "in a room of the house: {room:x?} at {end:?}"
    );
    let y = end.y - o.y;
    assert!(y > 37.5 && y < 37.5 + 8.0, "at its far wall: y {y}");
    let n = trail.len();
    assert!(
        (trail[n - 11].y - end.y).abs() < 1e-3,
        "still there: {:?} then {end:?}",
        trail[n - 11]
    );
    let c = ed.collider.as_ref().expect("the camera's world");
    assert!(c.stopped > 0 && c.blocks.contains(&HOLTBURG), "{c:?}");
    // Off, the same flight goes into the house and out through its far wall.
    aim(&mut ed);
    ed.collide = false;
    let off = fly_for(&mut ed, 0x57, 60);
    assert!(
        off.last().unwrap().y - o.y > 37.5 + 12.0,
        "{:?}",
        off.last()
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
    ed.enter_dungeon(0x019E).unwrap();
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
    ] {
        ed.mode = mode;
        scene(&mut ed, name);
    }
    // In the Drudge Hideout, its grid shown; a room selected; a piece chosen.
    ed.mode = Mode::Select;
    ed.enter_dungeon(0x019E).unwrap();
    scene(&mut ed, "in a dungeon, its grid");
    ed.choose_cell(0x019E, 0x0105).unwrap();
    scene(&mut ed, "in a dungeon, a room selected");
    ed.mode = Mode::Place;
    scene(&mut ed, "in a dungeon, place scenery");
    // Nothing idle reads a landblock's cells (they are read once until the world changes), or
    // picks. The times are printed for the record and asserted on nowhere.
    for (name, _, most) in &rows {
        assert_eq!(most.cells_read, 0, "{name}: {most:?}");
        assert_eq!(most.picks, 0, "{name}: {most:?}");
    }
}

#[test]
fn a_hidden_list_and_the_rooms_round_the_camera_are_not_worked_out_again_every_frame() {
    use lyceum_editor::app::Tab;
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-frame-work");
    let mut ed = editor(&world);
    ed.wait_for_names().unwrap();
    let z = ed.session.ground(HOLTBURG, 96.0, 96.0).unwrap();
    ed.look_at(HOLTBURG, Vec3::new(96.0, 96.0, z), 60.0, 30.0, -30.0);
    // The Nearby list, behind another tab: never gathered, however far the camera flies.
    ed.show_tab(Tab::Tool);
    ed.frame();
    let hidden = ed.stats.nearby_gathers;
    for _ in 0..40 {
        ed.camera.position.x += 2.0;
        ed.frame();
    }
    assert_eq!(ed.stats.nearby_gathers, hidden);
    // Brought forward: gathered as it is drawn, and not again while the camera stands still.
    ed.show_tab(Tab::Nearby);
    ed.frame();
    assert_eq!(ed.stats.nearby_gathers, hidden + 1);
    assert!(!ed.nearby.entries.is_empty());
    for _ in 0..5 {
        ed.frame();
    }
    assert_eq!(ed.stats.nearby_gathers, hidden + 1);
    // In a dungeon, flown out of its rooms, which lie outside its landblock's square: the rooms
    // of each landblock the camera is looked for in are read once, not every frame.
    ed.enter_dungeon(0x019E).unwrap();
    for _ in 0..3 {
        ed.frame();
    }
    ed.camera.position.z += 200.0;
    for _ in 0..3 {
        ed.frame();
    }
    let read = ed.stats.room_reads;
    for _ in 0..20 {
        ed.camera.position.x += 0.5;
        ed.frame();
    }
    assert_eq!(ed.stats.room_reads, read);
    // Seen from the south, rooms in a line north to south make one outline.
    let rooms = ed
        .cells_of(0x019E)
        .values()
        .filter(|c| c.frame.origin.z > lyceum_foundry::cells::CLOSED_DEPTH + 1.0)
        .count();
    let outlines = ed.layout.side_outlines;
    assert!(outlines > 0 && outlines < rooms, "{outlines} of {rooms}");
}

#[test]
fn a_brush_stroke_is_checked_once_when_it_ends_and_keeps_the_ground_s_textures() {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-stroke-work");
    let mut ed = editor(&world);
    let z = ed.session.ground(HOLTBURG, 150.0, 40.0).unwrap();
    ed.look_at(HOLTBURG, Vec3::new(150.0, 40.0, z), 50.0, 0.0, -50.0);
    ed.mode = lyceum_editor::app::Mode::Terrain;
    ed.panels.brush = Brush::Raise(1);
    ed.panels.radius = 10.0;
    for _ in 0..3 {
        ed.frame();
    }
    let o = pick::block_origin(HOLTBURG);
    let at = |ed: &EditorApp, x: f32| {
        let g = ed.session.ground(HOLTBURG, x, 40.0).unwrap();
        ed.view()
            .unwrap()
            .project(Vec3::new(o.x + x, o.y + 40.0, g))
            .unwrap()
    };
    let (checks, reloads, freed) = (ed.stats.checks, ed.stats.reloads, ed.block_textures_freed());
    let p = at(&ed, 140.0);
    ed.queue(HostEvent::CursorMoved {
        x: f64::from(p.0),
        y: f64::from(p.1),
    });
    ed.queue(HostEvent::MouseInput {
        button: MouseButton::Left,
        pressed: true,
    });
    ed.frame();
    for i in 1..6 {
        let p = at(&ed, 140.0 + 4.0 * i as f32);
        ed.queue(HostEvent::CursorMoved {
            x: f64::from(p.0),
            y: f64::from(p.1),
        });
        ed.frame();
    }
    // Every dab redrew the ground, and none checked the world.
    assert!(
        ed.stats.reloads >= reloads + 3,
        "{} dabs",
        ed.stats.reloads - reloads
    );
    assert_eq!(ed.stats.checks, checks);
    ed.queue(HostEvent::MouseInput {
        button: MouseButton::Left,
        pressed: false,
    });
    ed.frame();
    ed.frame();
    // Let go: checked once; one step to undo.
    assert_eq!(ed.stats.checks, checks + 1);
    assert_eq!(ed.session.history(), (1, 0));
    // The landblock built again over every dab gave back none of its textures: each was taken
    // again by the new build before the old one let it go.
    assert_eq!(ed.block_textures_freed(), freed);
}

#[test]
fn the_grid_and_the_floors_from_the_south_are_drawn_once_until_what_they_show_changes() {
    use lyceum_editor::app::Tab;
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-grid-work");
    let mut ed = editor_with(&world, &["--size", "1600x1000"]);
    ed.wait_for_pieces();
    ed.enter_dungeon(0x019E).unwrap();
    ed.show_tab(Tab::Grid);
    for _ in 0..3 {
        ed.frame();
    }
    let (grid, side, bytes) = (
        ed.layout.grid_drawings,
        ed.layout.side_drawings,
        ed.panel_texture_bytes(),
    );
    assert!(grid > 0 && side > 0);
    // Idle: neither is drawn again, nothing of theirs goes to the device, and each is one
    // picture on the overlay, not its rooms' triangles.
    for _ in 0..10 {
        ed.frame();
    }
    assert_eq!(
        (
            ed.layout.grid_drawings,
            ed.layout.side_drawings,
            ed.panel_texture_bytes()
        ),
        (grid, side, bytes)
    );
    let held = ed.work.overlay_vertices;
    // The dungeon's pieces offered in the Tool tab: only those scrolled into sight are drawn.
    let drawn = ed.kit.pieces_drawn;
    ed.frame();
    let offered = ed.kit.piece_rects.len() as u64;
    let one_frame = ed.kit.pieces_drawn - drawn;
    assert!(
        one_frame > 0 && one_frame < offered,
        "{one_frame} of {offered} drawn"
    );
    // As drawn shape by shape, as the pictures are made: the same pixels, but where two
    // triangles' edges cross a pixel's middle and the device and the picture round it apart.
    let pictured = capture(&mut ed, dir.path());
    ed.layout.live_panels = true;
    ed.frame();
    let live = capture(&mut ed, dir.path());
    let shapes = ed.work.overlay_vertices;
    assert!(
        held + 10_000 < shapes,
        "{held} corners held, {shapes} drawn live"
    );
    let differ = pictured
        .2
        .as_chunks::<4>()
        .0
        .iter()
        .zip(live.2.as_chunks::<4>().0)
        .filter(|(a, b)| a.iter().zip(*b).any(|(x, y)| x.abs_diff(*y) > 2))
        .count();
    let all = pictured.2.len() / 4;
    println!("{differ} of {all} pixels differ");
    assert!(differ * 1000 < all, "{differ} of {all} pixels differ");
    ed.layout.live_panels = false;
    ed.frame();
    // Panned by less than its margin: the picture slides, drawn again neither; past it, drawn
    // again once. Slid, it is still what the panel would draw.
    let grid = ed.layout.grid_drawings;
    let zoom = ed.layout.zoom;
    let pan = |ed: &mut EditorApp, points: f32| {
        let (x, y) = ed.layout.center.unwrap();
        ed.layout.center = Some((x + points / zoom, y));
    };
    pan(&mut ed, 10.0);
    ed.frame();
    assert_eq!(ed.layout.grid_drawings, grid);
    let slid = capture(&mut ed, dir.path());
    ed.layout.live_panels = true;
    ed.frame();
    let live = capture(&mut ed, dir.path());
    ed.layout.live_panels = false;
    let differ = slid
        .2
        .as_chunks::<4>()
        .0
        .iter()
        .zip(live.2.as_chunks::<4>().0)
        .filter(|(a, b)| a.iter().zip(*b).any(|(x, y)| x.abs_diff(*y) > 2))
        .count();
    assert!(differ * 1000 < all, "slid: {differ} of {all} pixels differ");
    let width = ed.layout.rect.unwrap().width();
    pan(&mut ed, width);
    ed.frame();
    assert_eq!(ed.layout.grid_drawings, grid + 1);
    // Zoomed: the grid is drawn again, once; the floors from the south are as they were.
    let grid = ed.layout.grid_drawings;
    ed.layout.zoom *= 1.25;
    for _ in 0..3 {
        ed.frame();
    }
    assert_eq!(ed.layout.grid_drawings, grid + 1);
    assert_eq!(ed.layout.side_drawings, side);
}

#[test]
fn in_a_window_a_dungeon_s_rooms_are_baked_off_the_frame_their_pictures_decoded_there_too() {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-rooms-off-frame");
    let cache = Path::new(env!("CARGO_TARGET_TMPDIR")).join("editor-cache");
    let argv: Vec<String> = [
        world.to_str().unwrap(),
        "--dats",
        dereth_dat::testing::dat_dir().to_str().unwrap(),
        "--headless",
        "--size",
        "1200x800",
        "--landblock",
        "A9B4",
        "--no-weenies",
        "--thumbnails",
        cache.to_str().unwrap(),
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    let args = lyceum_editor::args::parse(&argv).unwrap();
    let mut cfg = lyceum_editor::args::runtime_config(&args).unwrap();
    // As a window builds the world: a few milliseconds a frame.
    cfg.stream_budget_ms = Some(lyceum_editor::args::STREAM_BUDGET_MS);
    let mut ed = EditorApp::open(&args, cfg).unwrap();
    for _ in 0..3 {
        ed.frame();
    }
    let ((on_frame, off_frame), decoded) = ed.bakes();
    ed.enter_dungeon(0x019E).unwrap();
    // The dungeon's block, the camera's own now, is baked off the frame, its rooms' pictures
    // with it; the frames go on meanwhile. (A guard against a hang, not a time.)
    let mut frames = 0;
    while ed.bakes().0 .1 == off_frame {
        ed.frame();
        frames += 1;
        assert!(frames < 100_000, "the dungeon's block was never baked");
    }
    for _ in 0..3 {
        ed.frame();
    }
    let ((on, off), now) = ed.bakes();
    assert_eq!(on, on_frame, "no block baked on the frame");
    assert!(off > off_frame);
    assert!(
        now > decoded + 10,
        "{} pictures decoded off the frame",
        now - decoded
    );
}

#[test]
fn the_dungeons_are_found_off_the_frame_and_an_edit_is_written_off_it_in_a_window() {
    use lyceum_editor::app::Tab;
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-off-frame-work");
    let mut ed = editor(&world);
    ed.wait_for_names().unwrap();
    // The Dungeons tab shown: its list was found with the model names, off the frame.
    ed.show_tab(Tab::Dungeons);
    for _ in 0..3 {
        ed.frame();
    }
    assert!(ed.dungeons().iter().any(|d| d.landblock == 0x019E));
    assert_eq!(ed.stats.dungeon_scans, 0);
    // In a window, an edit's records are written off the frame: the world is built again from
    // them once they are, and not before.
    ed.write_off_frame = true;
    let reloads = ed.stats.reloads;
    ed.edit(&Edit::Op(lyceum_foundry::edit::Op::PlaceStatic {
        landblock: HOLTBURG,
        model: dereth_primitives::DataId(0x0200_040B),
        at: lyceum_foundry::source::Place {
            x: 60.0,
            y: 60.0,
            z: lyceum_foundry::source::Height::Ground,
        },
        rotation: lyceum_foundry::source::Rotation::Heading(0.0),
        name: None,
    }))
    .unwrap();
    assert_eq!(
        ed.stats.reloads, reloads,
        "not built again before the write is done"
    );
    ed.finish_write(true).unwrap();
    assert_eq!(ed.stats.reloads, reloads + 1);
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
    let mut ed = editor_with(&world, &["--size", "1400x1000"]);
    ed.show_findings();
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

#[test]
fn the_workspace_is_the_world_view_on_the_left_three_quarters_and_the_tool_and_lists_over_the_inspector_on_the_right(
) {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-workspace-layout");
    let mut ed = editor_with(&world, &["--size", "1920x1080"]);
    ed.wait_for_names().unwrap();
    ed.mode = lyceum_editor::app::Mode::Select;
    ed.frame();
    let select = ed.world_rect.unwrap();
    // The world view takes the left three quarters, from the bar to the status.
    assert!(select.left() < 20.0 && select.top() < 80.0, "{select:?}");
    let split = 1920.0 * lyceum_editor::docks::WORLD_WIDTH;
    assert!(
        select.right() < split && select.right() > split - 12.0,
        "{select:?}"
    );
    assert!(select.bottom() > 1000.0, "{select:?}");
    // The map, beside the tool on the right, holds the whole world square under its zoom, its
    // part fitted to it: no room to spare above or below.
    ed.show_tab(lyceum_editor::app::Tab::Map);
    ed.frame();
    ed.frame();
    let (map, panel) = (ed.map_rect.expect("the map drawn"), ed.map_panel.unwrap());
    assert!(map.left() > select.right(), "{map:?}");
    assert!(
        (map.width() - panel.width()).abs() < 12.0 && (map.height() - map.width()).abs() < 2.0,
        "{map:?} in {panel:?}"
    );
    assert!(
        (panel.height() - panel.width()).abs() < 6.0,
        "the map's part fits it: {panel:?}"
    );
    // A tool's controls take their tab: the world view stays where it is. Each tool is a button
    // of the bar, in the order Select, Place, Spawn, Terrain.
    let order: Vec<lyceum_editor::app::Mode> = ed.tool_rects.iter().map(|(m, _)| *m).collect();
    assert_eq!(
        order,
        [
            lyceum_editor::app::Mode::Select,
            lyceum_editor::app::Mode::Place,
            lyceum_editor::app::Mode::Spawn,
            lyceum_editor::app::Mode::Terrain,
        ]
    );
    for (mode, rect) in ed.tool_rects.clone() {
        // Chosen, a tool brings its tab forward.
        ed.show_tab(lyceum_editor::app::Tab::Map);
        ed.frame();
        click(&mut ed, (rect.center().x, rect.center().y));
        assert_eq!(ed.mode, mode);
        assert!(ed.tab_in_front(lyceum_editor::app::Tab::Tool), "{mode:?}");
        ed.frame();
        assert_eq!(ed.world_rect.unwrap(), select, "{mode:?}");
        for _ in 0..2 {
            ed.frame();
        }
        capture(&mut ed, dir.path());
        std::fs::copy(
            dir.path().join("frame.png"),
            shots("layout").join(format!("tool-{mode:?}.png").to_lowercase()),
        )
        .unwrap();
    }
    ed.mode = lyceum_editor::app::Mode::Select;
    ed.show_tab(lyceum_editor::app::Tab::Hierarchy);
    ed.frame();
    let (selection, rect) = ed
        .hierarchy_rects
        .iter()
        .find(|(s, _)| matches!(s, Selection::Building { .. }))
        .unwrap()
        .clone();
    click(&mut ed, (rect.center().x, rect.center().y));
    assert_eq!(ed.selection, Some(selection));
    ed.frame();
    ed.show_findings();
    ed.frame();
    assert_eq!(
        ed.faults.len(),
        (0..ed.faults.len())
            .filter(|i| ed.fault_rect(*i).is_some())
            .count()
    );
    ed.show_tab(lyceum_editor::app::Tab::Tool);
    for _ in 0..2 {
        ed.frame();
    }
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("layout").join("workspace.png"),
    )
    .unwrap();
    // In dungeon mode the grid takes a tab beside the inspector and the cross-section a strip
    // under the world view; the map and the list of dungeons are not shown.
    ed.new_dungeon(None).unwrap();
    ed.show_tab(lyceum_editor::app::Tab::Grid);
    for _ in 0..2 {
        ed.frame();
    }
    let world = ed.world_rect.unwrap();
    let grid = ed.layout.rect.expect("the grid drawn");
    assert!(
        grid.left() > world.right() && grid.top() > 500.0,
        "{grid:?}"
    );
    assert!(world.bottom() < 900.0, "the side view under it: {world:?}");
    for tab in [
        lyceum_editor::app::Tab::Map,
        lyceum_editor::app::Tab::Dungeons,
    ] {
        ed.show_tab(tab);
        assert!(!ed.tab_in_front(tab), "{tab:?}");
    }
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("layout").join("dungeon.png"),
    )
    .unwrap();
    ed.leave_dungeon();
    ed.frame();
    assert_eq!(ed.world_rect.unwrap(), select);
}

#[test]
fn recent_and_favorite_models_filter_the_browser_and_survive_reopening_the_world() {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-model-shelves");
    let mut ed = editor(&world);
    let models = ed.browser_list();
    let (first, second) = (models[0], models[1]);
    ed.choose_model(first).unwrap();
    ed.choose_model(second).unwrap();
    ed.choose_model(first).unwrap();
    ed.toggle_favorite(second).unwrap();
    ed.panels.shelf = lyceum_editor::browser_state::Shelf::Recent;
    assert_eq!(ed.browser_list(), [first, second]);
    ed.panels.shelf = lyceum_editor::browser_state::Shelf::Favorites;
    assert_eq!(ed.browser_list(), [second]);
    drop(ed);
    let mut ed = editor(&world);
    ed.panels.shelf = lyceum_editor::browser_state::Shelf::Recent;
    assert_eq!(ed.browser_list(), [first, second]);
    ed.panels.shelf = lyceum_editor::browser_state::Shelf::Favorites;
    assert_eq!(ed.browser_list(), [second]);
}

fn key(ed: &mut EditorApp, vk: usize, pressed: bool) {
    ed.queue(HostEvent::KeyboardInput {
        key: dereth_input::keys::Key::new(vk, 0),
        pressed,
        text: None,
    });
}

/// The world view's rectangle in the window's pixels.
fn view_pixels(ed: &EditorApp) -> dereth_primitives::Viewport {
    let r = ed.world_rect.expect("the world view");
    let s = ed.ui_scale();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let px = |v: f32| (v * s).round() as u32;
    dereth_primitives::Viewport {
        x: px(r.min.x),
        y: px(r.min.y),
        width: px(r.max.x) - px(r.min.x),
        height: px(r.max.y) - px(r.min.y),
    }
}

/// The world view's area in pixels: the whole of what the world is drawn in.
fn view_area(ed: &EditorApp) -> f32 {
    let vp = view_pixels(ed);
    #[allow(clippy::cast_precision_loss)]
    let area = (vp.width * vp.height) as f32;
    area
}

/// Wait for the scenes' worker threads to come down to `n`: a stopped playtest's are told to stop
/// and end as they finish what they were doing. A hang guard ends the wait.
fn workers_down_to(n: usize) -> usize {
    let start = std::time::Instant::now();
    while dereth_scene::workers::running() > n
        && start.elapsed() < std::time::Duration::from_secs(60)
    {
        std::thread::yield_now();
    }
    dereth_scene::workers::running()
}

#[test]
fn a_playtest_draws_in_the_world_view_with_the_panels_around_it_and_stopping_it_leaves_nothing_running(
) {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("embedded");
    let mut ed = editor(&world);
    assert!(ed.frame());
    let position = ed.camera.position;
    let workers = dereth_scene::workers::running();
    let settings = world.parent().unwrap().join("play-settings");
    let args = vec![
        "--no-connect".to_owned(),
        "--headless".to_owned(),
        "--dat-dir".to_owned(),
        dereth_dat::testing::dat_dir().display().to_string(),
    ];
    // Twice: a playtest stopped leaves nothing behind, so the second starts as the first did.
    for round in ["first", "second"] {
        ed.start_embedded(&args, &settings).unwrap();
        assert!(ed.is_playing() && ed.game_has_focus());
        for _ in 0..3 {
            assert!(ed.frame());
        }
        // The game is drawn in the world view, on a screen of the view's shape.
        let view = view_pixels(&ed);
        let region = ed.play_region().expect("the game drawn");
        assert_eq!(region.rect, view, "{round}");
        assert_eq!(
            region.extent,
            lyceum_editor::app::screen_for((view.width, view.height))
        );
        assert_eq!(ed.play_census().sessions, 1);
        ed.capture(&shots("embedded").join(format!("{round}-playing.png")))
            .unwrap();
        // The keyboard is the game's: the editor's Ctrl+Z does not undo the edit made before.
        ed.edit(&Edit::Brush {
            brush: Brush::Raise(2),
            landblock: HOLTBURG,
            x: 96.0,
            y: 96.0,
            radius: 12.0,
        })
        .unwrap();
        let undo = |ed: &mut EditorApp| {
            key(ed, 0x11, true);
            key(ed, 0x5A, true);
            key(ed, 0x5A, false);
            key(ed, 0x11, false);
            assert!(ed.frame());
        };
        undo(&mut ed);
        assert_eq!(ed.session.history().0, 1, "{round}");
        // A click in the view: the pointer is the game's, its cursor on the window.
        #[allow(clippy::cast_precision_loss)]
        let middle = (
            (view.x + view.width / 2) as f32,
            (view.y + view.height / 2) as f32,
        );
        click(&mut ed, middle);
        assert!(ed.game_has_focus());
        assert_eq!(ed.play_census().cursors, 1, "{round}");
        // Shift+F1 gives the editor the keyboard and the mouse, and its own arrow; the game plays
        // on, and Ctrl+Z is the editor's again.
        key(&mut ed, 0x10, true);
        key(&mut ed, 0x70, true);
        key(&mut ed, 0x70, false);
        key(&mut ed, 0x10, false);
        assert!(ed.frame());
        assert!(ed.is_playing() && !ed.game_has_focus());
        assert_eq!(ed.play_census().cursors, 0, "{round}");
        undo(&mut ed);
        assert_eq!(ed.session.history().0, 0, "{round}");
        // A click in the view gives them back to the game.
        click(&mut ed, middle);
        assert!(ed.game_has_focus());
        assert_eq!(ed.play_census().cursors, 1, "{round}");
        // Stopping (Stop in the view's corner, clicked) takes the game away: its session, connection,
        // sound, cursor, pointer hold and its part of the device, and its world's workers end.
        let stop = ed.play_button.expect("the Stop button");
        assert!(view_pixels(&ed).x as f32 <= stop.min.x && stop.max.x <= 800.0);
        click(&mut ed, (stop.center().x, stop.center().y));
        assert!(!ed.is_playing(), "{round}");
        assert_eq!(
            ed.play_census(),
            lyceum_editor::app::PlayCensus::default(),
            "{round}"
        );
        assert_eq!(workers_down_to(workers), workers, "{round}");
        assert!(ed.frame());
        // The editor has its own view again, where it was.
        assert_eq!(ed.camera.position, position);
        assert!(ed.core.world.is_some());
        assert_eq!(ed.core.present.size(), (800, 600));
    }
    ed.capture(&shots("embedded").join("editing.png")).unwrap();
}

#[test]
fn the_window_resizes_freely_during_a_playtest_and_the_game_follows_the_world_view() {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("embedded-resize");
    let mut ed = editor(&world);
    assert!(ed.frame());
    let settings = world.parent().unwrap().join("play-settings");
    let args = vec![
        "--no-connect".to_owned(),
        "--headless".to_owned(),
        "--dat-dir".to_owned(),
        dereth_dat::testing::dat_dir().display().to_string(),
    ];
    ed.start_embedded(&args, &settings).unwrap();
    assert!(ed.frame());
    let small = ed.play_region().unwrap();
    // Bigger, then smaller than the game's smallest screen: the window takes each size, and the
    // game's screen follows the view inside it.
    for size in [(1400, 900), (700, 500), (1100, 700)] {
        ed.resize(size.0, size.1);
        for _ in 0..3 {
            assert!(ed.frame());
        }
        assert_eq!(ed.core.present.size(), size);
        let view = view_pixels(&ed);
        let region = ed.play_region().unwrap();
        assert_eq!(region.rect, view);
        assert!(view.x + view.width <= size.0 && view.y + view.height <= size.1);
        assert_eq!(
            region.extent,
            lyceum_editor::app::screen_for((view.width, view.height))
        );
        let floor = lyceum_editor::app::SMALLEST_SCREEN;
        assert!(region.extent.0 >= floor.0 && region.extent.1 >= floor.1);
    }
    assert_ne!(ed.play_region().unwrap().rect, small.rect);
    ed.capture(&shots("embedded").join("resized.png")).unwrap();
    ed.stop_play();
    // Stopped, the editor keeps the window's size.
    assert!(ed.frame());
    assert_eq!(ed.core.present.size(), (1100, 700));
}

#[test]
fn a_playtest_hides_the_floating_windows_over_the_world_view_and_stopping_it_puts_them_back() {
    use lyceum_editor::app::Tab;
    let _gpu = gpu_lock();
    let (_dir, world) = sample("embedded-windows");
    let mut ed = editor(&world);
    assert!(ed.frame());
    // The Characters tab opens floating over the world view.
    ed.open_characters();
    for _ in 0..3 {
        assert!(ed.frame());
    }
    assert!(ed.characters_open());
    let docked = ed.tab_in_front(Tab::Inspector);
    let settings = world.parent().unwrap().join("play-settings");
    let args = vec![
        "--no-connect".to_owned(),
        "--headless".to_owned(),
        "--dat-dir".to_owned(),
        dereth_dat::testing::dat_dir().display().to_string(),
    ];
    ed.start_embedded(&args, &settings).unwrap();
    for _ in 0..3 {
        assert!(ed.frame());
    }
    assert!(!ed.characters_open() && ed.hides(Tab::Characters));
    assert_eq!(
        ed.tab_in_front(Tab::Inspector),
        docked,
        "a docked tab stays"
    );
    ed.stop_play();
    assert!(ed.frame());
    assert!(ed.characters_open() && !ed.hides(Tab::Characters));
}

#[test]
fn the_dungeons_list_beside_the_world_view_fills_it_to_the_bottom() {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("dungeon-list-fills");
    // A tall window, where rows taken to be taller than they are stopped the list short.
    let mut ed = editor_with(&world, &["--size", "1000x1400"]);
    for _ in 0..3 {
        assert!(ed.frame());
    }
    let body = ed
        .world_rect
        .expect("the world view, whose part the list shares");
    ed.show_tab(lyceum_editor::app::Tab::Dungeons);
    for _ in 0..3 {
        assert!(ed.frame());
    }
    let rows = &ed.dungeon_list.rects;
    assert!(rows.len() > 40, "{}", rows.len());
    let bottom = rows
        .iter()
        .map(|(_, r)| r.bottom())
        .fold(f32::MIN, f32::max);
    assert!(bottom >= body.bottom(), "{bottom} in {body:?}");
}

#[test]
fn a_right_drag_over_the_world_view_holds_the_pointer_and_one_over_the_panels_does_not() {
    use std::cell::RefCell;
    use std::rc::Rc;
    /// A hold that says what it was asked.
    struct Asked(Rc<RefCell<Vec<String>>>);
    impl dereth_client_shell::pointer::HostPointer for Asked {
        fn capture(&mut self, at: (f64, f64)) -> bool {
            self.0.borrow_mut().push(format!("hold {at:?}"));
            true
        }
        fn release(&mut self, at: (f64, f64)) {
            self.0.borrow_mut().push(format!("show {at:?}"));
        }
    }
    let _gpu = gpu_lock();
    let (_dir, world) = sample("look-hold");
    let mut ed = editor(&world);
    let asked = Rc::new(RefCell::new(Vec::new()));
    ed.hold_pointer_by(Box::new(Asked(Rc::clone(&asked))));
    for _ in 0..3 {
        assert!(ed.frame());
    }
    let w = ed.world_rect.expect("the world view");
    let right = |ed: &mut EditorApp, at: (f32, f32), pressed| {
        ed.queue(HostEvent::CursorMoved {
            x: f64::from(at.0),
            y: f64::from(at.1),
        });
        ed.queue(HostEvent::MouseInput {
            button: MouseButton::Right,
            pressed,
        });
        assert!(ed.frame());
    };
    let middle = (w.center().x.round(), w.center().y.round());
    right(&mut ed, middle, true);
    assert!(ed.holds_pointer());
    right(&mut ed, (middle.0 + 40.0, middle.1), false);
    assert!(!ed.holds_pointer());
    let (x, y) = (f64::from(middle.0), f64::from(middle.1));
    assert_eq!(
        *asked.borrow(),
        [format!("hold {:?}", (x, y)), format!("show {:?}", (x, y))]
    );
    // Over the panels beside the world view: no turn, and nothing held.
    asked.borrow_mut().clear();
    let panel = (w.max.x + 40.0, w.center().y);
    right(&mut ed, panel, true);
    assert!(!ed.holds_pointer());
    right(&mut ed, panel, false);
    assert!(asked.borrow().is_empty(), "{:?}", asked.borrow());
}

#[test]
fn the_panels_scale_with_the_ui_scale_and_ctrl_keys_step_it() {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("ui-scale");
    let mut ed = editor(&world);
    assert!(ed.frame());
    let at_100 = ed.world_rect.unwrap();
    assert!((ed.ui_scale() - 1.0).abs() < 1e-6);
    ed.set_ui_scale(1.5);
    assert!(ed.frame());
    assert!(ed.frame());
    // In points the window is two thirds the size: the panels take more of it.
    let at_150 = ed.world_rect.unwrap();
    assert!(at_150.max.x * 1.5 <= 800.5 && at_150.max.y * 1.5 <= 600.5);
    assert!(at_150.width() * 1.5 < at_100.width() || at_150.height() * 1.5 < at_100.height());
    ed.capture(&shots("ui-scale").join("150.png")).unwrap();
    // Ctrl+= a step bigger, Ctrl+- a step smaller, Ctrl+0 back to the screen's own.
    let ctrl = |ed: &mut EditorApp, vk: usize| {
        key(ed, 0x11, true);
        key(ed, vk, true);
        key(ed, vk, false);
        key(ed, 0x11, false);
        assert!(ed.frame());
        assert!(ed.frame());
        ed.ui_scale()
    };
    assert!((ctrl(&mut ed, 0xBB) - 1.75).abs() < 1e-6);
    assert!((ctrl(&mut ed, 0xBD) - 1.5).abs() < 1e-6);
    assert!((ctrl(&mut ed, 0xBD) - 1.25).abs() < 1e-6);
    assert!((ctrl(&mut ed, 0x30) - ed.monitor_scale()).abs() < 1e-6);
    assert!(
        ed.ui_scale_note().is_some(),
        "the change is shown in the status bar"
    );
}

#[cfg(feature = "live-server-tests")]
#[test]
fn live_edits_and_undo_reach_the_connected_character_and_only_the_changed_landblock() {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("live-connected");
    let server =
        PathBuf::from(std::env::var_os("EMPYREAN_SERVER_BIN").expect("EMPYREAN_SERVER_BIN"));
    let sql = PathBuf::from(std::env::var_os("EMPYREAN_WORLD_SQL").expect("EMPYREAN_WORLD_SQL"));
    let mut ed = editor_with(
        &world,
        &[
            "--empyrean",
            server.parent().unwrap().to_str().unwrap(),
            "--weenies",
            sql.to_str().unwrap(),
        ],
    );
    ed.set_live(true).unwrap();
    ed.wait_live().unwrap();
    ed.play(&[], false).unwrap();
    let start = std::time::Instant::now();
    while !ed.play_ready() {
        assert!(ed.frame());
        assert!(
            start.elapsed() < std::time::Duration::from_secs(120),
            "playtest entry timed out"
        );
    }
    let identity = ed.play_identity().unwrap();
    let id = dereth_primitives::DataId(0xA9B4FFFF);
    let other = dereth_primitives::DataId(0xA9B5FFFF);
    let before = ed.play_store().unwrap().read_cell(id).unwrap();
    let unaffected = ed.play_store().unwrap().read_cell(other).unwrap();
    ed.return_to_editor();
    let camera = ed.camera.position;
    let mark = ed.live_log(0).len();
    ed.edit(&Edit::Brush {
        brush: Brush::Raise(30),
        landblock: HOLTBURG,
        x: 96.0,
        y: 96.0,
        radius: 12.0,
    })
    .unwrap();
    ed.push_live().unwrap();
    assert!(ed.frame());
    let after = ed.play_identity().unwrap();
    assert_eq!(after.0, identity.0);
    assert!(after.1 > identity.1);
    assert_ne!(ed.play_store().unwrap().read_cell(id).unwrap(), before);
    assert_eq!(
        ed.play_store().unwrap().read_cell(other).unwrap(),
        unaffected
    );
    assert!(ed
        .live_log(mark)
        .iter()
        .any(|s| s.contains("of 1 landblocks rebuilt.")));
    assert_eq!(ed.camera.position, camera);
    ed.edit(&Edit::Undo).unwrap();
    ed.push_live().unwrap();
    assert!(ed.frame());
    let undone = ed.play_identity().unwrap();
    assert_eq!(undone.0, identity.0);
    assert!(undone.1 > after.1);
    assert_eq!(ed.play_store().unwrap().read_cell(id).unwrap(), before);
    let stale = lyceum_editor::live::Revision {
        number: after.1,
        overlay: ed
            .play_store()
            .unwrap()
            .overlay_dir()
            .unwrap()
            .path()
            .to_path_buf(),
        blocks: [HOLTBURG].into_iter().collect(),
    };
    assert!(ed.adopt_play_revision(&stale).is_err());
    assert_eq!(ed.play_identity(), Some(undone));
    let missing = lyceum_editor::live::Revision {
        number: undone.1 + 1,
        overlay: world.parent().unwrap().join("missing"),
        blocks: [HOLTBURG].into_iter().collect(),
    };
    assert!(ed.adopt_play_revision(&missing).is_err());
    assert_eq!(ed.play_identity(), Some(undone));
    assert_eq!(ed.play_store().unwrap().read_cell(id).unwrap(), before);
    ed.play(&[], false).unwrap();
    assert!(ed.frame());
    ed.capture(&shots("live-connected").join("after-undo.png"))
        .unwrap();
    ed.set_live(false).unwrap();
}

#[cfg(feature = "live-server-tests")]
#[test]
fn stopping_a_playtest_logs_its_character_off_leaves_nothing_running_and_playing_again_shows_the_edits(
) {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("live-stop");
    let server =
        PathBuf::from(std::env::var_os("EMPYREAN_SERVER_BIN").expect("EMPYREAN_SERVER_BIN"));
    let sql = PathBuf::from(std::env::var_os("EMPYREAN_WORLD_SQL").expect("EMPYREAN_WORLD_SQL"));
    let mut ed = editor_with(
        &world,
        &[
            "--empyrean",
            server.parent().unwrap().to_str().unwrap(),
            "--weenies",
            sql.to_str().unwrap(),
        ],
    );
    ed.set_live(true).unwrap();
    ed.wait_live().unwrap();
    assert!(ed.frame());
    let workers = dereth_scene::workers::running();
    ed.play_until_entered(&[]).unwrap();
    let first = ed.play_identity().unwrap();
    let census = ed.play_census();
    assert_eq!((census.sessions, census.connections), (1, 1));
    for _ in 0..30 {
        assert!(ed.frame());
    }
    ed.capture(&shots("live-stop").join("playing.png")).unwrap();
    let id = dereth_primitives::DataId(0xA9B4FFFF);
    let before = ed.play_store().unwrap().read_cell(id).unwrap();
    // Stopped: the game is gone, and the world lets its character go.
    ed.stop_play();
    assert_eq!(ed.play_census(), lyceum_editor::app::PlayCensus::default());
    assert!(ed.wait_world_empty(), "the character logged off");
    assert_eq!(workers_down_to(workers), workers);
    assert!(ed.frame());
    // An edit made while stopped is in the world the next playtest sees.
    ed.edit(&Edit::Brush {
        brush: Brush::Raise(30),
        landblock: HOLTBURG,
        x: 96.0,
        y: 96.0,
        radius: 12.0,
    })
    .unwrap();
    ed.play_until_entered(&[]).unwrap();
    let second = ed.play_identity().unwrap();
    assert_eq!(second.0, first.0, "the same character");
    assert!(second.1 > first.1, "the newer world");
    assert_ne!(ed.play_store().unwrap().read_cell(id).unwrap(), before);
    ed.capture(&shots("live-stop").join("played-again.png"))
        .unwrap();
    ed.stop_play();
    assert_eq!(ed.play_census(), lyceum_editor::app::PlayCensus::default());
    assert!(ed.wait_world_empty());
    assert_eq!(workers_down_to(workers), workers);
    ed.set_live(false).unwrap();
}

/// Where world point `p` (landblock meters of `lb`) is in the room `cell` of `lb`: along the
/// room's own axes, from its origin.
fn in_room(ed: &EditorApp, lb: u16, cell: u16, p: Vec3) -> Vec3 {
    let c = ed.session.cells(lb).remove(&cell).unwrap();
    dereth_primitives::frame::globaltolocal(&c.frame, p)
}

#[test]
fn what_is_placed_in_a_room_belongs_to_it_is_listed_in_place_and_moves_and_goes_with_it() {
    use lyceum_foundry::kit::Kind;
    let _g = gpu_lock();
    let (dir, world) = sample("editor-decorate");
    let mut ed = editor_with(&world, &["--size", "1400x1000"]);
    stone_west(&mut ed);
    ed.wait_for_pieces();
    let pieces = ed.kit.pieces.clone().unwrap();
    // The commonest corridor of one part, closed above, drawn in more than one look.
    let corridor = pieces
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            p.form.kind == Kind::Corridor
                && p.form.parts() == 1
                && !p.form.above
                && p.looks.len() > 1
        })
        .max_by_key(|(_, p)| p.placed)
        .map(|(i, _)| i)
        .unwrap();
    // Two corridors joined end to end in a new dungeon, drawn in the corridor's commonest look.
    let lb = ed.new_dungeon(None).unwrap();
    let first_look = ed.kit.families[pieces[corridor].looks[0].family].landblocks[0].0;
    ed.choose_dungeon_look(lb, Some(first_look)).unwrap();
    ed.frame();
    ed.kit.chosen = Some(corridor);
    ed.place_on_square((9, 9)).unwrap();
    ed.kit.chosen = Some(corridor);
    ed.frame();
    let at = fitting_square(&mut ed, lb, corridor).expect("a square at its end");
    ed.place_on_square(lyceum_foundry::grid::square_of(at.0, at.1))
        .unwrap();
    let rooms: Vec<(u16, String)> = ed.session.cell_names(lb).into_iter().collect();
    assert_eq!(rooms.len(), 2, "{}", ed.panels.message);
    let (b, b_name) = rooms[1].clone();
    // Another look of the world's chosen for the dungeon: its rooms drawn so, as one step.
    let other = &pieces[corridor].looks[1];
    let of = ed.kit.families[other.family].landblocks[0].0;
    let steps = ed.session.history().0;
    ed.choose_dungeon_look(lb, Some(of)).unwrap();
    assert_eq!(ed.session.history().0, steps + 1);
    assert_eq!(ed.session.dungeon_info(lb).look, Some(of));
    for (n, _) in &rooms {
        assert_eq!(ed.session.cells(lb)[n].surfaces, other.surfaces);
    }
    // In the second room, looking down at its floor: a static placed there stands in it, and a
    // spawn added there stands in it too.
    ed.kit.chosen = None;
    ed.fly_into_cell(lb, b).unwrap();
    ed.camera.pitch = -1.2;
    ed.mode = lyceum_editor::app::Mode::Place;
    ed.model = Some(dereth_primitives::DataId(0x0200_040B));
    for _ in 0..2 {
        ed.frame();
    }
    let w = ed.world_rect.unwrap();
    let s = ed.ui_scale();
    click(&mut ed, (w.center().x * s, w.center().y * s));
    let cell = ed.session.cells(lb).remove(&b).unwrap();
    assert_eq!(cell.static_objects.len(), 1, "{}", ed.panels.message);
    ed.mode = lyceum_editor::app::Mode::Spawn;
    ed.panels.weenie = Some(lyceum_foundry::edit::Weenie::Symbol(
        "holtburg:guide".into(),
    ));
    for _ in 0..2 {
        ed.frame();
    }
    click(&mut ed, ((w.center().x + 40.0) * s, w.center().y * s));
    let id = (u32::from(lb) << 16) | u32::from(b);
    let spawn = ed
        .session
        .spawns()
        .into_iter()
        .find(|x| x.cell == id)
        .expect("a spawn in the room");
    // The room selected on the grid lists both, each by its place in the room.
    ed.mode = lyceum_editor::app::Mode::Select;
    ed.selection = None;
    ed.show_tab(lyceum_editor::app::Tab::Grid);
    ed.frame();
    // Its middle: the mean of its corners.
    let mid = {
        let c = ed.session.cells(lb).remove(&b).unwrap();
        let e = ed.session.environment(c.environment).unwrap();
        let vs = &e.cells[usize::from(c.cell_struct)].vertex_array.vertices;
        #[allow(clippy::cast_precision_loss)]
        let n = vs.len() as f32;
        let (x, y) = vs.iter().fold((0.0, 0.0), |(x, y), v| {
            let p = dereth_primitives::frame::localtoglobal(&c.frame, v.position);
            (x + p.x / n, y + p.y / n)
        });
        (x, y)
    };
    let (_, shown) = ed.plan_floors(lb);
    let _ = shown;
    let g = grid_point(&ed, mid);
    click(&mut ed, g);
    assert_eq!(
        ed.selection,
        Some(Selection::Cell {
            landblock: lb,
            cell: b
        })
    );
    ed.show_tab(lyceum_editor::app::Tab::Inspector);
    ed.frame();
    assert_eq!(ed.panels.in_place.len(), 2, "the static and the spawn");
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("grid").join("in-place.png"),
    )
    .unwrap();
    // A click on the static's row selects it; its plus on X moves it a step along the room.
    let (sel, row) = ed.panels.in_place[0].clone();
    assert!(matches!(sel, Selection::CellStatic { .. }));
    click(&mut ed, (row.left() + 8.0, row.center().y));
    assert_eq!(ed.selection, Some(sel.clone()));
    ed.frame();
    let static_at =
        |ed: &EditorApp| ed.session.cells(lb).remove(&b).unwrap().static_objects[0].frame;
    let was = in_room(&ed, lb, b, static_at(&ed).origin);
    let (_, _, _, plus) = ed.panels.place_rects[0];
    click(&mut ed, (plus.center().x, plus.center().y));
    let now = in_room(&ed, lb, b, static_at(&ed).origin);
    assert!(
        (now.x - was.x - ed.panels.step).abs() < 0.01 && (now.y - was.y).abs() < 0.01,
        "{was:?} -> {now:?}"
    );
    // The room moved to another square: what stands in it moves with it, its place in the
    // room kept.
    let static_in = in_room(&ed, lb, b, static_at(&ed).origin);
    let spawn_in = in_room(&ed, lb, b, spawn.frame.origin);
    let before = ed.session.cells(lb).remove(&b).unwrap().frame;
    ed.lift_room(lb, b).unwrap();
    ed.frame();
    let to = ed
        .piece_marks(lb, corridor)
        .iter()
        .map(|p| p.frame.origin)
        .find(|o| (o.x - before.origin.x).abs() + (o.y - before.origin.y).abs() > 5.0)
        .map(|o| lyceum_foundry::grid::square_of(o.x, o.y))
        .expect("another square it fits on");
    let g = grid_point(&ed, lyceum_foundry::grid::square_middle(to));
    click(&mut ed, g);
    let after = ed.session.cells(lb).remove(&b).unwrap().frame;
    assert!(
        (after.origin.x - before.origin.x).abs() + (after.origin.y - before.origin.y).abs() > 5.0,
        "{before:?} -> {after:?}: {}",
        ed.panels.message
    );
    let moved = ed
        .session
        .spawns()
        .into_iter()
        .find(|x| x.guid == spawn.guid)
        .expect("the spawn still there");
    assert_eq!(moved.cell, id, "still in its room");
    let near = |a: Vec3, b: Vec3| (a.x - b.x).abs() + (a.y - b.y).abs() + (a.z - b.z).abs() < 0.02;
    assert!(near(in_room(&ed, lb, b, static_at(&ed).origin), static_in));
    assert!(near(in_room(&ed, lb, b, moved.frame.origin), spawn_in));
    // Taken away, what stood in it goes too.
    ed.edit(&Edit::Op(lyceum_foundry::edit::Op::RemoveCell {
        landblock: lb,
        name: b_name,
    }))
    .unwrap();
    assert!(!ed.session.spawns().iter().any(|x| x.guid == spawn.guid));
    assert_eq!(ed.session.cell_names(lb).len(), 1);
}

#[test]
fn a_portal_is_placed_like_any_spawn_its_card_sends_it_into_a_dungeon_or_here_and_the_map_marks_outdoor_ways_in(
) {
    let _g = gpu_lock();
    let (dir, world) = sample("editor-portal");
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, format!("{SERVER_PORTALS}{HIDEOUT_ENTRANCES}")).unwrap();
    let mut ed = editor_with(
        &world,
        &["--weenies", sql.to_str().unwrap(), "--size", "1400x1000"],
    );
    stone_west(&mut ed);
    ed.wait_for_names().unwrap();
    // A portal is made with the spawn tool's New..., beside the count of what it lists; placed
    // on the ground, it goes nowhere yet.
    ed.mode = lyceum_editor::app::Mode::Spawn;
    for _ in 0..2 {
        ed.frame();
    }
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("spawn-tool").join("new.png"),
    )
    .unwrap();
    new_portal(&mut ed, "Hideout Portal");
    assert!(matches!(
        ed.panels.weenie,
        Some(lyceum_foundry::edit::Weenie::Symbol(_))
    ));
    let w = ed.world_rect.unwrap();
    let s = ed.ui_scale();
    click(
        &mut ed,
        (w.center().x * s, (w.center().y + w.height() * 0.2) * s),
    );
    let Some(Selection::Spawn { file, guid }) = ed.selection.clone() else {
        panic!(
            "the portal selected: {:?} {}",
            ed.selection, ed.panels.message
        );
    };
    let spawn = |ed: &EditorApp| {
        ed.session
            .spawns()
            .into_iter()
            .find(|x| x.file == file && x.guid == guid)
            .unwrap()
    };
    assert_eq!(ed.portal_destination(&spawn(&ed)), Some(None));
    let portal = ed.form_weenie().expect("the portal's weenie");
    ed.frame();
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("portal").join("card.png"),
    )
    .unwrap();
    // Its card sends it into a dungeon, at its drop point: the outdoor portal's destination.
    ed.destination_dungeon(&portal, 0x019E).unwrap();
    let (cell, _) = ed.drop_point(0x019E).unwrap();
    assert_eq!(ed.portal_destination(&spawn(&ed)), Some(Some(cell)));
    assert_eq!(cell, 0x019E_0114);
    // Or where the camera is.
    ed.destination_here(&portal).unwrap();
    let to = ed.portal_destination(&spawn(&ed)).flatten().unwrap();
    assert_eq!(to >> 16, u32::from(HOLTBURG));
    assert!(to & 0xFFFF < 0x0100, "outdoors: {to:#010X}");
    // The map, after going into the hideout and out: its way in from outdoors marked, and not
    // the portals that stand in another dungeon's rooms.
    ed.enter_dungeon(0x019E).unwrap();
    ed.leave_dungeon();
    ed.mode = lyceum_editor::app::Mode::Select;
    ed.show_tab(lyceum_editor::app::Tab::Map);
    for _ in 0..2 {
        ed.frame();
    }
    let marks: Vec<lyceum_editor::app::MapMark> = ed
        .dungeon_list
        .map_marks
        .iter()
        .map(|(_, m)| m.clone())
        .collect();
    assert_eq!(
        marks,
        vec![lyceum_editor::app::MapMark::Entrance(
            HOLTBURG,
            Vec3::new(60.0, 60.0, 94.0)
        )],
        "{marks:?}"
    );
}

#[test]
fn a_dungeons_grid_shows_one_floor_at_a_time_and_its_look_leads_the_palette_in_groups_by_what_pieces_are(
) {
    let _g = gpu_lock();
    let (dir, world) = sample("editor-grid-floors");
    let mut ed = editor_with(&world, &["--size", "1400x1000"]);
    ed.wait_for_pieces();
    // A retail dungeon of several floors: PageUp and PageDown step through them, the world
    // view drawing the rooms round the camera all the while.
    let lb = 0x019E;
    ed.enter_dungeon(lb).unwrap();
    for _ in 0..2 {
        assert!(ed.frame());
    }
    assert!(ed.grid_shown());
    let (floors, shown) = ed.plan_floors(lb);
    println!("floors {floors:?}, shown {shown}");
    assert!(floors.len() >= 2, "{floors:?}");
    let at = floors
        .iter()
        .position(|f| (f - shown).abs() < 0.01)
        .unwrap();
    let (step, want) = if at + 1 < floors.len() {
        (0x21, floors[at + 1])
    } else {
        (0x22, floors[at - 1])
    };
    key(&mut ed, step, true);
    key(&mut ed, step, false);
    assert!(ed.frame());
    assert_eq!(ed.plan_floors(lb).1, want);
    assert!(
        ed.panels.message.starts_with("Floor "),
        "{}",
        ed.panels.message
    );
    let (here, _) = ed.camera_cell().expect("in a room");
    assert!(ed.cells_seen().get(&here.0).copied().unwrap_or(0.0) > 0.9 * view_area(&ed));
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("grid").join("floors.png"),
    )
    .unwrap();
    // The palette follows the dungeon's look, its pieces in groups by what each is on the grid,
    // each group once.
    let look = ed.dungeon_look(lb);
    assert!(look.is_some());
    for _ in 0..2 {
        assert!(ed.frame());
    }
    assert_eq!(ed.kit.look, look);
    let groups: Vec<lyceum_foundry::kit::Kind> =
        ed.kit.group_rects.iter().map(|(k, _)| *k).collect();
    assert!(groups.len() >= 3, "{groups:?}");
    let distinct: std::collections::BTreeSet<_> = groups.iter().collect();
    assert_eq!(distinct.len(), groups.len(), "{groups:?}");
    let pieces = ed.kit.pieces.clone().unwrap();
    assert!(ed
        .kit
        .piece_rects
        .iter()
        .all(|(i, _)| pieces[*i].in_look(look.unwrap())));
}

/// A server's generator: two entries of the drudge, one scattered and one at its own spot,
/// working by night.
const SERVER_GENERATOR: &str = "\
INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES (90005, 'drudgeskulker', 10, '2005-02-09 10:00:00')
     , (90100, 'drudgecamp', 1, '2005-02-09 10:00:00');
INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES (90005, 1, 'Drudge Skulker'), (90100, 1, 'Drudge Camp');
INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES (90005, 1, 0x020007DD), (90100, 1, 0x0200026B);
INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES (90100, 41, 300), (90100, 43, 12), (90100, 121, 30);
INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES (90100, 81, 6), (90100, 82, 3), (90100, 142, 4);
INSERT INTO `weenie_properties_generator` (`object_Id`, `probability`, `weenie_Class_Id`, `delay`, `init_Create`, `max_Create`, `when_Create`, `where_Create`, `stack_Size`, `palette_Id`, `shade`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`)
VALUES (90100, 0.25, 90005, 120, 1, 2, 4, 2, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL)
     , (90100, 1, 90005, 0, 1, 1, 1, 4, NULL, NULL, NULL, 0xA9B40011, 70, 70, ZZ, 1, 0, 0, 0);
INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`)
VALUES (0x7A9B4060, 90100, 0xA9B40011, 60, 60, ZZ, 1, 0, 0, 0, False, '2019-05-16 00:00:00');
";

#[test]
fn a_servers_generator_is_explained_from_its_own_rows_with_nothing_copied() {
    let _g = gpu_lock();
    let (dir, world) = sample("editor-server-generator");
    let z = {
        let s = lyceum_foundry::edit::Session::open(&world, &dereth_dat::testing::dat_dir(), None)
            .unwrap();
        s.ground(HOLTBURG, 60.0, 60.0).unwrap()
    };
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, SERVER_GENERATOR.replace("ZZ", &z.to_string())).unwrap();
    // A window tall enough for the generator's entries to be in the Definition pane's view: an
    // entry out of its view is not drawn.
    let mut ed = editor_with(
        &world,
        &["--weenies", sql.to_str().unwrap(), "--size", "1280x1000"],
    );
    ed.look_at(HOLTBURG, Vec3::new(60.0, 60.0, z + 1.0), 30.0, 0.0, -40.0);
    for _ in 0..3 {
        ed.frame();
    }
    let s = ed
        .session
        .spawns()
        .into_iter()
        .find(|s| s.server && s.weenie == "90100")
        .expect("the server's generator, shown around the camera");
    ed.selection = Some(Selection::Spawn {
        file: s.file.clone(),
        guid: s.guid.clone(),
    });
    ed.server_class_now(90100).unwrap().expect("its rows");
    let g = ed.spawn_generator(&s).expect("a generator");
    assert_eq!(
        (g.radius, g.interval, g.initial_delay),
        (Some(12.0), Some(300.0), Some(30.0))
    );
    assert_eq!((g.most, g.first, g.time), (Some(6), Some(3), Some(4)));
    assert_eq!(g.profiles.len(), 2);
    let (a, b) = (&g.profiles[0], &g.profiles[1]);
    assert_eq!(
        (a.init, a.max, a.when, a.place, a.delay),
        (1, 2, 4, 2, Some(120.0))
    );
    assert_eq!((b.when, b.place), (1, 4));
    let shares = lyceum_editor::app::generator_shares(&g.profiles);
    assert!((shares[0].unwrap() - 25.0).abs() < 1e-3 && (shares[1].unwrap() - 75.0).abs() < 1e-3);
    // The Definition pane counts what it spawns, and opens into the generator's account.
    ed.show_tab(lyceum_editor::app::Tab::Definition);
    for _ in 0..3 {
        ed.frame();
    }
    let camp = lyceum_foundry::edit::Weenie::Class(90100);
    let spawns = ed
        .definition
        .drill_rects
        .iter()
        .find(|(d, _)| *d == lyceum_editor::app::Drill::Spawns)
        .map(|(_, r)| *r)
        .expect("its spawns counted");
    click(&mut ed, (spawns.center().x, spawns.center().y));
    assert_eq!(
        ed.definition.drill_of(&camp),
        Some(lyceum_editor::app::Drill::Spawns)
    );
    for _ in 0..2 {
        ed.frame();
    }
    assert_eq!(ed.panels.spawn_rows.len(), 2, "each entry listed");
    // Shown, with nothing written into the pack.
    assert!(ed
        .session
        .weenie_file(&lyceum_foundry::edit::Weenie::Class(90100))
        .is_none());
    assert_eq!(ed.session.history(), (0, 0));
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("generator").join("explained.png"),
    )
    .unwrap();
}

/// Turn the camera about where it stands until world point `p` is drawn at window pixel column
/// `want`: a frame drawn at each try. Answers where it is drawn.
fn turn_until_at(ed: &mut EditorApp, p: Vec3, want: f32) -> (f32, f32) {
    let (mut lo, mut hi) = (ed.camera.yaw - 1.2, ed.camera.yaw + 1.2);
    let mut at = (0.0, 0.0);
    for _ in 0..24 {
        ed.camera.yaw = (hi + lo) / 2.0;
        ed.frame();
        at = ed.view().unwrap().project(p).unwrap_or((-1.0, 0.0));
        // Turning left moves what is ahead to the right.
        if at.0 > want {
            hi = ed.camera.yaw;
        } else {
            lo = ed.camera.yaw;
        }
    }
    at
}

#[test]
fn the_world_is_drawn_only_in_its_view_and_a_pick_near_the_views_edge_meets_what_is_drawn_there() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-world-view");
    let mut ed = editor_with(&world, &["--size", "1200x800"]);
    // The tool's column beside the world view, which takes the window's height.
    ed.mode = lyceum_editor::app::Mode::Place;
    let stone = stone_west(&mut ed);
    let vp = view_pixels(&ed);
    assert!(vp.x + vp.width < 1000 && vp.y + vp.height > 700, "{vp:?}");
    assert_eq!(
        ed.view().unwrap().viewport(),
        vp,
        "the projection is the view's"
    );
    // As tall a field of view as a 4:3 window's, whatever the view's shape: the angle between
    // the rays through the middles of its top and bottom edges.
    let view = ed.view().unwrap();
    #[allow(clippy::cast_precision_loss)]
    let (mid, top, bottom) = (
        vp.x as f32 + vp.width as f32 / 2.0,
        vp.y as f32,
        (vp.y + vp.height) as f32,
    );
    let (a, b) = (view.ray(mid, top).dir, view.ray(mid, bottom).dir);
    let tall = dereth_primitives::num::math::acosf(a.dot(b).clamp(-1.0, 1.0)).to_degrees();
    assert!((tall - 73.0).abs() < 1.0, "{tall} degrees in {vp:?}");
    // The stone drawn 30 pixels inside the view's right edge, next to the tool's column.
    let middle = Vec3::new(stone.x, stone.y, stone.z + 1.5);
    #[allow(clippy::cast_precision_loss)]
    let edge = (vp.x + vp.width) as f32 - 30.0;
    let at = turn_until_at(&mut ed, middle, edge);
    assert!((at.0 - edge).abs() < 1.0, "{at:?}");
    // A pick there meets it.
    let picked = ed.pick_at(at.0, at.1, pick::Mask::ALL).expect("a hit");
    assert!(
        matches!(
            picked.hit,
            pick::Hit::Static {
                landblock: HOLTBURG,
                index: 114,
                ..
            }
        ),
        "{:?}",
        picked.hit
    );
    // And it is drawn there: taken away, the pixels there change, and those far from it do not.
    for _ in 0..3 {
        ed.frame();
    }
    let with = capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("world-view").join("edge.png"),
    )
    .unwrap();
    let which = ed.session.origins(HOLTBURG)[114].clone();
    ed.edit(&Edit::Op(lyceum_foundry::edit::Op::RemoveStatic {
        landblock: HOLTBURG,
        which,
    }))
    .unwrap();
    for _ in 0..3 {
        ed.frame();
    }
    let without = capture(&mut ed, dir.path());
    let here = difference(&with, &without, at, 3.0);
    assert!(here > 15.0, "the stone is drawn where it is picked: {here}");
    #[allow(clippy::cast_precision_loss)]
    let far = (
        vp.x as f32 + vp.width as f32 * 0.25,
        vp.y as f32 + vp.height as f32 * 0.5,
    );
    assert!(difference(&with, &without, far, 3.0) < 4.0);
    // Nothing of the world is drawn outside its view: looking up into the sky instead of down
    // at the ground changes no pixel there. (The select tool shows the same panels either way.)
    ed.mode = lyceum_editor::app::Mode::Select;
    for _ in 0..3 {
        ed.frame();
    }
    let down = capture(&mut ed, dir.path());
    ed.camera.pitch = 0.6;
    for _ in 0..3 {
        ed.frame();
    }
    let up = capture(&mut ed, dir.path());
    let vp = view_pixels(&ed);
    let (w, h, _) = &down;
    let mut inside = 0;
    let mut outside = Vec::new();
    for y in 0..*h {
        for x in 0..*w {
            #[allow(clippy::cast_precision_loss)]
            let (a, b) = (
                pixel(&down, x as f32, y as f32),
                pixel(&up, x as f32, y as f32),
            );
            if a == b {
                continue;
            }
            let x32 = u32::try_from(x).unwrap();
            let y32 = u32::try_from(y).unwrap();
            if (vp.x..vp.x + vp.width).contains(&x32) && (vp.y..vp.y + vp.height).contains(&y32) {
                inside += 1;
            } else if y + 24 >= *h {
                // The status bar, which says where the eye is.
            } else {
                outside.push((x, y));
            }
        }
    }
    assert!(inside > 10_000, "the view changed: {inside}");
    assert!(
        outside.is_empty(),
        "{} pixels outside the view changed, the first {:?}",
        outside.len(),
        &outside[..outside.len().min(8)]
    );
}

#[test]
fn going_to_what_stands_in_a_room_from_nearby_or_hierarchy_puts_the_camera_in_that_room_and_draws_it(
) {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-go-to-room");
    // Tall enough for the Hierarchy tab to show Holtburg's rooms.
    let mut ed = editor_with(&world, &["--size", "1000x1200"]);
    // In a dungeon: a static of another room, some way off, chosen from the Nearby list.
    ed.enter_dungeon(0x019E).unwrap();
    ed.show_tab(lyceum_editor::app::Tab::Nearby);
    for _ in 0..3 {
        ed.frame();
    }
    let start = ed.camera_cell().expect("in a room").0 .0;
    let shown: Vec<lyceum_editor::app::NearbyEntry> =
        ed.nearby.shown().into_iter().cloned().collect();
    let i = shown
        .iter()
        .position(|e| {
            matches!(e.select, Some(Selection::CellStatic { cell, .. })
                if (u32::from(cell) | 0x019E_0000) != start)
                && e.distance > 20.0
        })
        .expect("a static of another room");
    let e = shown[i].clone();
    let Some(Selection::CellStatic { cell, .. }) = e.select else {
        unreachable!()
    };
    ed.nearby_go(i).unwrap();
    for _ in 0..3 {
        ed.frame();
    }
    let room = 0x019E_0000 | u32::from(cell);
    assert_eq!(ed.camera_cell().map(|c| c.0 .0), Some(room), "{e:?}");
    assert!(ed.cells_seen().contains_key(&room), "its room is drawn");
    // It is selected and on screen, in the world view.
    assert_eq!(ed.selection, e.select);
    let w = ed.view().unwrap();
    let o = pick::block_origin(0x019E);
    let (x, y) = w
        .project(Vec3::new(o.x + e.at.x, o.y + e.at.y, o.z + e.at.z))
        .expect("in front of the camera");
    let vp = view_pixels(&ed);
    #[allow(clippy::cast_precision_loss)]
    let inside = x > vp.x as f32
        && x < (vp.x + vp.width) as f32
        && y > vp.y as f32
        && y < (vp.y + vp.height) as f32;
    assert!(inside, "({x}, {y}) in {vp:?}");
    // Outdoors, a room of a Holtburg house entered from Hierarchy: the camera in it, drawn round
    // it; then a static outdoors from Nearby: the camera outdoors, looked for in no room.
    ed.fly_to(
        f32::from(0xA9u8) * 192.0 + 96.0,
        f32::from(0xB4u8) * 192.0 + 96.0,
    )
    .unwrap();
    ed.show_tab(lyceum_editor::app::Tab::Hierarchy);
    for _ in 0..2 {
        ed.frame();
    }
    // A house's rooms are listed under it once it is opened.
    let (_, house) = ed
        .hierarchy_rects
        .iter()
        .find(|(s, _)| matches!(s, Selection::Building { .. }))
        .cloned()
        .expect("a house of Holtburg");
    click(&mut ed, (house.left() + 30.0, house.center().y));
    // Selecting the house brought the inspector forward; back to the list.
    ed.show_tab(lyceum_editor::app::Tab::Hierarchy);
    ed.frame();
    let (room, rect) = ed
        .hierarchy_rects
        .iter()
        .find_map(|(s, r)| match s {
            Selection::Cell { landblock, cell } if *landblock == HOLTBURG => Some((*cell, *r)),
            _ => None,
        })
        .expect("a room of the house");
    assert!(
        rect.top() < house.bottom() + 60.0,
        "{rect:?} under {house:?}"
    );
    click(&mut ed, (rect.left() + 30.0, rect.center().y));
    for _ in 0..2 {
        ed.frame();
    }
    let id = (u32::from(HOLTBURG) << 16) | u32::from(room);
    assert_eq!(ed.camera_cell().map(|c| c.0 .0), Some(id));
    assert!(ed.cells_seen().contains_key(&id));
    ed.show_tab(lyceum_editor::app::Tab::Nearby);
    for _ in 0..2 {
        ed.frame();
    }
    let i = ed
        .nearby
        .shown()
        .iter()
        .position(|e| matches!(e.select, Some(Selection::Static { .. })))
        .expect("a static outdoors");
    ed.nearby_go(i).unwrap();
    for _ in 0..2 {
        ed.frame();
    }
    assert_eq!(ed.camera_cell(), None);
    assert!(!ed.in_dungeon && ed.dungeon_block.is_none());
}

/// The most placed cottage of the world.
const COTTAGE: u32 = 0x0100_2228;

/// Keep the frame drawn last among the shots of the building drop.
fn keep(ed: &mut EditorApp, name: &str) {
    ed.capture(&shots("drop-building").join(name)).unwrap();
}

/// Whether pixel `p` of `img` is the red of a refused drop's land cell, or the green of an
/// allowed one.
fn reddish(c: [u8; 3]) -> bool {
    c[0] > 170 && c[1] < 110 && c[2] < 110
}

fn greenish(c: [u8; 3]) -> bool {
    c[1] > 170 && c[0] < 140 && c[2] < 150
}

/// Whether the outline of land cell `(cx, cy)` of landblock `lb`, drawn on the ground, shows
/// pixels `of` the color: looked for about the middle of each of its edges.
fn outlined(
    ed: &EditorApp,
    img: &(usize, usize, Vec<u8>),
    lb: u16,
    (cx, cy): (i32, i32),
    of: fn([u8; 3]) -> bool,
) -> bool {
    let o = pick::block_origin(lb);
    #[allow(clippy::cast_precision_loss)]
    let (x0, y0) = (cx as f32 * 24.0, cy as f32 * 24.0);
    let view = ed.view().unwrap();
    [
        (x0 + 12.0, y0),
        (x0 + 24.0, y0 + 12.0),
        (x0 + 12.0, y0 + 24.0),
        (x0, y0 + 12.0),
    ]
    .into_iter()
    .filter_map(|(x, y)| {
        let z = ed.session.ground(lb, x.min(191.99), y.min(191.99))?;
        view.project(Vec3::new(o.x + x, o.y + y, o.z + z + 0.15))
    })
    .any(|(x, y)| {
        (-3..=3).any(|dx| (-3..=3).any(|dy| of(pixel(img, x + dx as f32, y + dy as f32))))
    })
}

#[test]
fn a_cottage_dropped_from_the_place_tool_is_refused_on_a_taken_land_cell_moved_with_its_rooms_and_undone(
) {
    use lyceum_editor::app::Mode;
    use lyceum_foundry::buildings::land_cell;
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-drop-building");
    let mut ed = editor_with(&world, &["--size", "1280x900"]);
    ed.wait_for_names().unwrap();
    assert!(
        ed.session.buildings().is_some(),
        "the world's buildings are read"
    );
    ed.mode = Mode::Place;
    for _ in 0..3 {
        ed.frame();
    }
    // Buildings are a kind of the dropdown: the gallery, the most placed first.
    let combo = ed.panels.category_combo.expect("the kind's dropdown");
    click(&mut ed, (combo.center().x, combo.center().y));
    ed.frame();
    let (_, row) = ed
        .panels
        .category_rows
        .iter()
        .find(|(c, _)| *c == Some(Category::Building))
        .copied()
        .expect("Building listed");
    click(&mut ed, (row.center().x, row.center().y));
    assert_eq!(ed.panels.kinds, [Category::Building].into_iter().collect());
    let list = ed.browser_list();
    let gallery = ed.session.buildings().unwrap().gallery();
    assert_eq!(list.len(), gallery.len());
    assert_eq!(
        list[0],
        dereth_primitives::DataId(COTTAGE),
        "the most placed first"
    );
    assert_eq!(
        lyceum_editor::app::building_name(gallery[0], None),
        "Cottage"
    );
    // A click on its picture chooses it, and its card offers its furniture.
    ed.frame();
    let (_, tile) = *ed
        .picture_rects
        .iter()
        .find(|(m, _)| m.raw() == COTTAGE)
        .expect("the cottage's picture");
    click(&mut ed, (tile.center().x, tile.center().y));
    assert_eq!(ed.model.map(|m| m.raw()), Some(COTTAGE));
    assert!(ed.dropping_building());
    ed.frame();
    let furniture = ed
        .panels
        .furnishing_combo
        .expect("the furniture's dropdown");
    click(&mut ed, (furniture.center().x, furniture.center().y));
    ed.frame();
    let rows = ed.panels.furnishing_rows.clone();
    assert!(rows.len() >= 2, "each variant and Bare: {rows:?}");
    assert_eq!(rows.last().map(|(k, _)| *k), Some(None), "Bare last");
    let (k, first) = rows[0];
    click(&mut ed, (first.center().x, first.center().y));
    assert_eq!(ed.panels.building_furnishing, k);
    // Over the land cell Holtburg's building 8 stands in: the ghost says why it is refused, its
    // land cell red, and a click drops nothing.
    let o = pick::block_origin(HOLTBURG);
    let taken = (74.0, 30.0);
    let gz = ed.session.ground(HOLTBURG, taken.0, taken.1).unwrap();
    ed.look_at(HOLTBURG, Vec3::new(taken.0, taken.1, gz), 70.0, 20.0, -45.0);
    for _ in 0..3 {
        ed.frame();
    }
    let p = ed
        .view()
        .unwrap()
        .project(Vec3::new(o.x + taken.0, o.y + taken.1, o.z + gz))
        .unwrap();
    hover(&mut ed, p);
    ed.frame();
    let g = ed.drop_ghost(p).unwrap();
    assert!(!g.allowed());
    assert_eq!(g.plan.as_ref().unwrap().taken, Some(8));
    let (_, words) = ed.ghost_note.clone().expect("words beside the ghost");
    assert_eq!(words, ["Another building is already in this cell."]);
    let img = capture(&mut ed, dir.path());
    let cell = land_cell(g.at.0, g.at.1);
    assert!(
        outlined(&ed, &img, HOLTBURG, cell, reddish)
            && !outlined(&ed, &img, HOLTBURG, cell, greenish),
        "its land cell is outlined red"
    );
    keep(&mut ed, "01-refused.png");
    click(&mut ed, p);
    assert!(!ed.session.compiled().dropped.contains_key(&HOLTBURG));
    assert_eq!(
        ed.panels.message,
        "Another building is already in this cell."
    );
    // Over a free land cell of Holtburg: green; a click drops it there, and selects it. It has room
    // to move east by its arrow: no other building's land or ground there that it may not stand on.
    let (fx, fy) = (1..7)
        .flat_map(|cx| (1..7).map(move |cy| (cx, cy)))
        .map(|(cx, cy)| (cx as f32 * 24.0 + 12.0, cy as f32 * 24.0 + 12.0))
        .find(|(x, y)| {
            let (sx, sy, h) = ed
                .session
                .drop_snap(dereth_primitives::DataId(COTTAGE), 0, *x, *y, 0)
                .unwrap();
            let plan = |east: f32| {
                ed.session.drop_plan(
                    dereth_primitives::DataId(COTTAGE),
                    0,
                    HOLTBURG,
                    sx + east,
                    sy,
                    h,
                )
            };
            plan(0.0).is_ok_and(|p| {
                p.refusal.is_none() && p.warnings.iter().all(|w| w.contains("doors"))
            }) && [4.0, 8.0, 12.0, 16.0]
                .into_iter()
                .all(|east| plan(east).is_ok_and(|p| p.refusal.is_none()))
        })
        .expect("a free land cell in Holtburg with room east of it");
    let fz = ed.session.ground(HOLTBURG, fx, fy).unwrap();
    ed.look_at(HOLTBURG, Vec3::new(fx, fy, fz), 70.0, 20.0, -45.0);
    for _ in 0..3 {
        ed.frame();
    }
    let p = ed
        .view()
        .unwrap()
        .project(Vec3::new(o.x + fx, o.y + fy, o.z + fz))
        .unwrap();
    hover(&mut ed, p);
    ed.frame();
    let g = ed.drop_ghost(p).unwrap();
    assert!(g.allowed(), "{:?}", g.plan);
    assert_eq!(
        land_cell(g.at.0, g.at.1),
        land_cell(fx, fy),
        "snapped in its land cell"
    );
    let img = capture(&mut ed, dir.path());
    let cell = land_cell(g.at.0, g.at.1);
    assert!(
        outlined(&ed, &img, HOLTBURG, cell, greenish)
            && !outlined(&ed, &img, HOLTBURG, cell, reddish),
        "its land cell is outlined green"
    );
    keep(&mut ed, "02-ghost.png");
    // A pointer that stays looks for nothing again.
    let picks = ed.stats.picks;
    for _ in 0..3 {
        ed.frame();
    }
    assert_eq!(ed.stats.picks, picks, "an idle ghost picks nothing");
    // R turns it a quarter.
    hold_key(&mut ed, 0x52, 1);
    assert_eq!(ed.panels.building_turns, 1);
    let turned = ed.drop_ghost(p).unwrap();
    let off = (turned.heading - g.heading - 90.0).rem_euclid(360.0);
    assert!(
        off.min(360.0 - off) < 0.01,
        "{} from {}",
        turned.heading,
        g.heading
    );
    for _ in 0..3 {
        hold_key(&mut ed, 0x52, 1);
    }
    assert_eq!(ed.panels.building_turns, 0);
    let steps = ed.session.history().0;
    click(&mut ed, p);
    assert_eq!(ed.session.history().0, steps + 1, "one step");
    let (name, index) = ed.session.compiled().dropped[&HOLTBURG][0].clone();
    assert_eq!(name, "holtburg:cottage-1");
    assert_eq!(
        ed.selection,
        Some(Selection::Building {
            landblock: HOLTBURG,
            index
        })
    );
    let dropped = ed.session.statics(HOLTBURG).unwrap().buildings[index].clone();
    assert!((dropped.frame.origin.x - g.at.0).abs() < 1e-3);
    assert!((dropped.frame.origin.y - g.at.1).abs() < 1e-3);
    let rooms: Vec<u16> = ed.session.compiled().rooms[&HOLTBURG]
        .values()
        .copied()
        .collect();
    assert_eq!(rooms.len(), 7, "a cottage's seven rooms");
    for _ in 0..2 {
        ed.frame();
    }
    keep(&mut ed, "03-dropped.png");
    // Moved by its east arrow: its row says where it stands, its rooms go with it, one step.
    ed.mode = Mode::Select;
    ed.frame();
    let before = ed.session.cells(HOLTBURG);
    let at = Vec3::new(
        o.x + dropped.frame.origin.x,
        o.y + dropped.frame.origin.y,
        o.z + dropped.frame.origin.z,
    );
    let steps = ed.session.history().0;
    let moved = drag_arrow(&mut ed, Handle::X, at);
    assert!(moved.abs() > 0.5, "moved {moved} m: {}", ed.panels.message);
    assert_eq!(ed.session.history().0, steps + 1, "one step");
    let now = ed.session.statics(HOLTBURG).unwrap().buildings[index]
        .frame
        .origin;
    assert!(
        (now.x - dropped.frame.origin.x - moved).abs() < 0.05,
        "the building: {now:?}, dropped at {:?}, moved {moved} m: {}",
        dropped.frame.origin,
        ed.panels.message
    );
    let row = ed.session.packs()[0]
        .edits
        .iter()
        .flat_map(|e| e.drops.iter())
        .next()
        .unwrap()
        .item
        .clone();
    assert!((row.at.x - now.x).abs() < 0.01, "its row: {:?}", row.at);
    assert!(
        ed.session.packs()[0]
            .edits
            .iter()
            .all(|e| e.buildings.is_empty()),
        "no move-building row"
    );
    let after = ed.session.cells(HOLTBURG);
    for c in &rooms {
        let dx = after[c].frame.origin.x - before[c].frame.origin.x;
        assert!((dx - moved).abs() < 0.05, "room {c:04X} moved {dx}");
    }
    ed.frame();
    keep(&mut ed, "04-moved.png");
    // Undone: where it was dropped; again: not there.
    ed.edit(&Edit::Undo).unwrap();
    assert_eq!(
        ed.session.statics(HOLTBURG).unwrap().buildings[index].frame,
        dropped.frame
    );
    ed.edit(&Edit::Undo).unwrap();
    assert!(!ed.session.compiled().dropped.contains_key(&HOLTBURG));
    ed.frame();
    keep(&mut ed, "05-undone.png");
}

/// The server's world database: `EMPYREAN_WORLD_SQL`, else the newest in the importer's cache.
fn world_sql() -> PathBuf {
    std::env::var_os("EMPYREAN_WORLD_SQL")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| lyceum_editor::catalog::server_database(dereth_primitives::EraId::Eor))
        .expect("the server's world database: set EMPYREAN_WORLD_SQL, or fetch it")
}

/// A land cell beside a retail building whose land cell's edge toward it is not level: its
/// landblock and the land cell.
const TILTED: (u16, (i32, i32)) = (0x0408, (5, 5));

#[test]
fn a_house_drops_as_one_players_can_buy_unless_its_card_says_otherwise_and_a_tilt_is_refused() {
    use lyceum_editor::app::Mode;
    use lyceum_foundry::buildings::land_cell;
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-drop-house");
    let sql = world_sql();
    let mut ed = editor_with(
        &world,
        &["--size", "1280x900", "--weenies", sql.to_str().unwrap()],
    );
    ed.wait_for_names().unwrap();
    let catalog = ed
        .session
        .buildings()
        .expect("the world's buildings are read");
    assert!(catalog.doors_known);
    let plain_model = catalog
        .gallery()
        .into_iter()
        .find(|e| e.kind == lyceum_foundry::buildings::Kind::Building)
        .unwrap()
        .model;
    ed.mode = Mode::Place;
    // A cottage's card: "Purchasable", checked.
    ed.choose_model(dereth_primitives::DataId(COTTAGE)).unwrap();
    for _ in 0..3 {
        ed.frame();
    }
    let house = ed.panels.house_box.expect("the house's check box");
    assert!(ed.panels.building_house && ed.chosen_house());
    keep(&mut ed, "house-01-card.png");
    // Unchecked, it drops as a plain building; checked, as a house players can buy.
    click(&mut ed, (house.center().x, house.center().y));
    assert!(!ed.panels.building_house && !ed.chosen_house());
    ed.frame();
    keep(&mut ed, "house-02-unchecked.png");
    let o = pick::block_origin(HOLTBURG);
    let (fx, fy) = (1..7)
        .flat_map(|cx| (1..7).map(move |cy| (cx, cy)))
        .map(|(cx, cy)| (cx as f32 * 24.0 + 12.0, cy as f32 * 24.0 + 12.0))
        .find(|(x, y)| {
            let (sx, sy, h) = ed
                .session
                .drop_snap(dereth_primitives::DataId(COTTAGE), 0, *x, *y, 0)
                .unwrap();
            ed.session
                .drop_plan_as(
                    dereth_primitives::DataId(COTTAGE),
                    0,
                    true,
                    HOLTBURG,
                    sx,
                    sy,
                    h,
                )
                .is_ok_and(|p| p.refusal.is_none())
        })
        .expect("a free land cell in Holtburg");
    let fz = ed.session.ground(HOLTBURG, fx, fy).unwrap();
    ed.look_at(HOLTBURG, Vec3::new(fx, fy, fz), 70.0, 20.0, -45.0);
    for _ in 0..3 {
        ed.frame();
    }
    let p = ed
        .view()
        .unwrap()
        .project(Vec3::new(o.x + fx, o.y + fy, o.z + fz))
        .unwrap();
    hover(&mut ed, p);
    ed.frame();
    click(&mut ed, p);
    let row = |ed: &EditorApp| {
        ed.session.packs()[0]
            .edits
            .iter()
            .flat_map(|e| e.drops.iter())
            .next()
            .map(|d| d.item.clone())
    };
    assert!(
        row(&ed).expect("dropped").house.is_empty(),
        "a plain building"
    );
    ed.edit(&Edit::Undo).unwrap();
    assert!(row(&ed).is_none());
    click(&mut ed, (house.center().x, house.center().y));
    assert!(ed.chosen_house());
    hover(&mut ed, p);
    ed.frame();
    click(&mut ed, p);
    let dropped = row(&ed).expect("dropped");
    assert_eq!(dropped.house, vec![format!("{}-house", dropped.name)]);
    let bare = dropped.name.strip_prefix("holtburg:").unwrap().to_owned();
    let sql_files: Vec<String> = ed
        .session
        .compiled()
        .sql
        .keys()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    assert!(
        sql_files
            .iter()
            .any(|f| f.contains(&format!("{bare}-house"))),
        "its house's spawns: {sql_files:?}"
    );
    for _ in 0..2 {
        ed.frame();
    }
    keep(&mut ed, "house-03-dropped.png");
    // A building that is no house has no such box.
    ed.choose_model(plain_model).unwrap();
    for _ in 0..2 {
        ed.frame();
    }
    assert_eq!(ed.panels.house_box, None);
    assert!(!ed.chosen_house());
    // Beside a building whose land is not level, the ghost's land cell is red, and the words
    // say the drop would tilt the ground under it.
    ed.choose_model(dereth_primitives::DataId(COTTAGE)).unwrap();
    let (lb, (cx, cy)) = TILTED;
    #[allow(clippy::cast_precision_loss)]
    let (tx, ty) = (cx as f32 * 24.0 + 12.0, cy as f32 * 24.0 + 12.0);
    let tz = ed.session.ground(lb, tx, ty).unwrap();
    ed.look_at(lb, Vec3::new(tx, ty, tz), 70.0, 20.0, -45.0);
    for _ in 0..3 {
        ed.frame();
    }
    let to = pick::block_origin(lb);
    let p = ed
        .view()
        .unwrap()
        .project(Vec3::new(to.x + tx, to.y + ty, to.z + tz))
        .unwrap();
    hover(&mut ed, p);
    ed.frame();
    let g = ed.drop_ghost(p).unwrap();
    assert!(!g.allowed());
    let (_, words) = ed.ghost_note.clone().expect("words beside the ghost");
    assert_eq!(words.len(), 1);
    assert!(
        words[0].starts_with("It would tilt the ground under the ")
            && words[0].ends_with(" next door."),
        "{words:?}"
    );
    let img = capture(&mut ed, dir.path());
    let cell = land_cell(g.at.0, g.at.1);
    assert!(
        outlined(&ed, &img, lb, cell, reddish) && !outlined(&ed, &img, lb, cell, greenish),
        "its land cell is outlined red"
    );
    keep(&mut ed, "house-04-tilt.png");
    let steps = ed.session.history().0;
    click(&mut ed, p);
    assert_eq!(ed.session.history().0, steps, "nothing dropped");
    assert_eq!(ed.panels.message, words[0]);
}

#[cfg(feature = "live-server-tests")]
#[test]
fn a_cottage_dropped_in_the_editor_is_walked_into_through_its_door_by_the_playtest_character() {
    use lyceum_editor::app::Mode;
    let _gpu = gpu_lock();
    let (_dir, world) = sample("live-drop-building");
    let server =
        PathBuf::from(std::env::var_os("EMPYREAN_SERVER_BIN").expect("EMPYREAN_SERVER_BIN"));
    let sql = PathBuf::from(std::env::var_os("EMPYREAN_WORLD_SQL").expect("EMPYREAN_WORLD_SQL"));
    let mut ed = editor_with(
        &world,
        &[
            "--empyrean",
            server.parent().unwrap().to_str().unwrap(),
            "--weenies",
            sql.to_str().unwrap(),
            "--size",
            "1280x900",
        ],
    );
    ed.wait_for_names().unwrap();
    let cottage = dereth_primitives::DataId(COTTAGE);
    let catalog = ed.session.buildings().expect("the world's buildings");
    assert!(
        catalog.doors_known,
        "the doors are read from the world database"
    );
    let entry = catalog.get(cottage).unwrap().clone();
    let door = entry.layouts[0].doors[0];
    // The doorway its door stands in, in the building's frame: its foot, and the way in, square
    // to the door and toward the building's origin.
    let base = ed.session.base();
    let read = |id: u32| base.cell.read(dereth_primitives::DataId(id)).ok();
    let t = lyceum_foundry::drops::Template::read(
        &read,
        entry.layouts[0].example,
        None,
        dereth_primitives::EraId::Eor,
    )
    .unwrap();
    let mut env = |id: dereth_primitives::DataId| ed.session.environment(id);
    let feet = lyceum_foundry::buildings::doorway_feet(&t.building, &t.rooms, &mut env);
    let local = |p: Vec3| dereth_primitives::frame::globaltolocal(&t.building.frame, p);
    let foot = feet
        .iter()
        .map(|f| local(*f))
        .min_by(|a, b| {
            let d = |p: &Vec3| {
                (p.x - door.frame.origin.x).powi(2) + (p.y - door.frame.origin.y).powi(2)
            };
            d(a).total_cmp(&d(b))
        })
        .unwrap();
    let inward = {
        let n = dereth_primitives::frame::localtoglobal(
            &dereth_primitives::Frame::new(Vec3::ZERO, door.frame.rotation),
            Vec3::new(0.0, 1.0, 0.0),
        );
        let toward = -(n.x * foot.x + n.y * foot.y);
        if toward < 0.0 {
            Vec3::new(-n.x, -n.y, 0.0)
        } else {
            Vec3::new(n.x, n.y, 0.0)
        }
    };
    // A free land cell of Holtburg, and the turn that faces that doorway's way in north, as the
    // character comes in facing.
    let o = pick::block_origin(HOLTBURG);
    let (cell, turns) = (1..7)
        .flat_map(|cx| (1..7).map(move |cy| (cx, cy)))
        .flat_map(|c| (0..4u8).map(move |k| (c, k)))
        .find(|((cx, cy), k)| {
            let (x, y) = (*cx as f32 * 24.0 + 12.0, *cy as f32 * 24.0 + 12.0);
            let Some((sx, sy, h)) = ed.session.drop_snap(cottage, 0, x, y, *k) else {
                return false;
            };
            let turn =
                dereth_primitives::Frame::new(Vec3::ZERO, lyceum_foundry::compile::heading_quat(h));
            dereth_primitives::frame::localtoglobal(&turn, inward).y > 0.9
                && ed
                    .session
                    .drop_plan(cottage, 0, HOLTBURG, sx, sy, h)
                    .is_ok_and(|p| p.refusal.is_none() && p.warnings.is_empty())
        })
        .expect("a free land cell of Holtburg");
    // Dropped from the place tool: the gallery's cottage, turned, clicked on the ground there.
    ed.mode = Mode::Place;
    ed.choose_model(cottage).unwrap();
    ed.panels.building_turns = turns;
    let (fx, fy) = (cell.0 as f32 * 24.0 + 12.0, cell.1 as f32 * 24.0 + 12.0);
    let fz = ed.session.ground(HOLTBURG, fx, fy).unwrap();
    ed.look_at(HOLTBURG, Vec3::new(fx, fy, fz), 60.0, 20.0, -45.0);
    for _ in 0..3 {
        ed.frame();
    }
    let p = ed
        .view()
        .unwrap()
        .project(Vec3::new(o.x + fx, o.y + fy, o.z + fz))
        .unwrap();
    hover(&mut ed, p);
    click(&mut ed, p);
    let (_, index) = ed.session.compiled().dropped[&HOLTBURG][0].clone();
    let rooms: std::collections::BTreeSet<u16> = ed.session.compiled().rooms[&HOLTBURG]
        .values()
        .copied()
        .collect();
    let dropped = ed.session.statics(HOLTBURG).unwrap().buildings[index].clone();
    // Its door's guid in the world.
    let symbol = ed
        .session
        .compiled()
        .lock
        .of(lyceum_foundry::symbol::Kind::Guid)
        .find(|(name, _)| name.contains("cottage-1-door-0"))
        .map(|(_, id)| id)
        .expect("the door's guid");
    // The camera four meters out from the doorway, looking in: where the character comes in.
    let at = dereth_primitives::frame::localtoglobal(&dropped.frame, foot);
    let gz = ed.session.ground(HOLTBURG, at.x, at.y - 4.0).unwrap();
    ed.look_at(HOLTBURG, Vec3::new(at.x, at.y, gz + 1.6), 4.0, 0.0, 0.0);
    ed.frame();
    ed.set_live(true).unwrap();
    ed.wait_live().unwrap();
    // In the world, under the interface whose script uses things, the door used as a player
    // would: open, it lets the character through.
    ed.play_options.interface = lyceum_editor::app::PlayInterface::Modern;
    ed.play_until_entered(&["--use".to_owned(), format!("0x{symbol:08X}")])
        .unwrap();
    let shots = shots("live-drop-building");
    for _ in 0..30 {
        assert!(ed.frame());
    }
    ed.capture(&shots.join("outside.png")).unwrap();
    // A door open is one the world lets things pass through.
    const ETHEREAL: u32 = 0x4;
    let start = std::time::Instant::now();
    while ed
        .play_object_state(symbol)
        .is_none_or(|s| s & ETHEREAL == 0)
    {
        assert!(ed.frame());
        // Only against a hang: the door opening is what ends the wait.
        assert!(
            start.elapsed() < std::time::Duration::from_secs(120),
            "the door did not open: {:?}",
            ed.play_object_state(symbol)
        );
    }
    for _ in 0..30 {
        assert!(ed.frame());
    }
    ed.capture(&shots.join("at-the-door.png")).unwrap();
    let w = dereth_input::keys::Key::new(0x57, 0x11);
    ed.queue(HostEvent::KeyboardInput {
        key: w,
        pressed: true,
        text: None,
    });
    let start = std::time::Instant::now();
    let inside = loop {
        assert!(ed.frame());
        let cell = ed.play_body().map(|b| b.cell.0);
        if let Some(c) = cell.filter(|c| c >> 16 == u32::from(HOLTBURG)) {
            if rooms.contains(&u16::try_from(c & 0xFFFF).unwrap()) {
                break c;
            }
        }
        // Only against a hang: the door opens and the walk in are what end the loop.
        assert!(
            start.elapsed() < std::time::Duration::from_secs(120),
            "the character did not walk in: {:?}",
            ed.play_body()
        );
    };
    ed.queue(HostEvent::KeyboardInput {
        key: w,
        pressed: false,
        text: None,
    });
    for _ in 0..30 {
        assert!(ed.frame());
    }
    ed.capture(&shots.join("inside.png")).unwrap();
    assert!(rooms.contains(&u16::try_from(inside & 0xFFFF).unwrap()));
    assert!(ed.session.compiled().dropped[&HOLTBURG]
        .iter()
        .any(|(_, i)| *i == index));
    ed.stop_play();
    assert!(ed.wait_world_empty(), "the character logged off");
    ed.set_live(false).unwrap();
}

#[test]
fn with_collide_on_a_camera_put_inside_a_wall_goes_on_through_it_and_once_clear_meets_it_again() {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-unstick");
    let mut ed = editor(&world);
    let o = pick::block_origin(HOLTBURG);
    // Flown north at the house at (79.5, 37.5): it slides in by its doorway and stops at the far
    // wall of the room inside.
    let z = ed.session.ground(HOLTBURG, 79.5, 20.0).unwrap();
    ed.camera.position = Vec3::new(o.x + 79.5, o.y + 20.0, z + 1.5);
    ed.camera.yaw = 0.0;
    ed.camera.pitch = 0.0;
    ed.set_speed(2); // 5 m/s
    ed.frame();
    ed.collide = true;
    let wall = fly_until_still(&mut ed, 0x57, 600);
    // Pressed against it, it stays: a step away still goes, so it is not held.
    let again = fly_until_still(&mut ed, 0x57, 30);
    assert!((again.y - wall.y).abs() < 0.01, "{again:?} at {wall:?}");
    assert!(!ed.collider.as_ref().unwrap().passing());
    // Put inside the wall, as a click on the map can: it goes on, through the wall and out.
    ed.camera.position = Vec3::new(wall.x, wall.y + 0.3, wall.z);
    if let Some(c) = ed.collider.as_mut() {
        c.lost();
    }
    ed.frame();
    let through = fly_for(&mut ed, 0x57, 30);
    let out = *through.last().unwrap();
    assert!(
        out.y > wall.y + 3.0,
        "through the wall and out: {out:?} from {wall:?}"
    );
    assert!(!ed.collider.as_ref().unwrap().passing());
    // Clear of it, it meets things again: lowered, it comes to rest on the ground, never under.
    let down = fly_for(&mut ed, 0x10, 60);
    let r = lyceum_editor::app::FLY_RADIUS;
    for p in &down {
        let under = ed.session.ground(HOLTBURG, p.x - o.x, p.y - o.y).unwrap();
        assert!(
            p.z > under + r - 0.05,
            "never under the ground: {p:?} over {under}"
        );
    }
    let end = *down.last().unwrap();
    let under = ed
        .session
        .ground(HOLTBURG, end.x - o.x, end.y - o.y)
        .unwrap();
    assert!(
        end.z < under + r + 0.35,
        "on the ground at {under}: {end:?}"
    );
    assert!(!ed.collider.as_ref().unwrap().passing());
}

/// A door of the server's, drawn with the model of Holtburg's house doors.
const SERVER_DOOR: &str = "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES (90412, 'door-aluvian-house', 19, '2005-02-09 10:00:00');
INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES (90412, 1, 'Door');
INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES (90412, 1, 0x020019FF);
";

#[test]
fn with_collide_on_the_flying_camera_passes_through_a_door_and_still_stops_at_a_stone() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-collide-door");
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, SERVER_DOOR).unwrap();
    let mut ed = editor_with(&world, &["--weenies", sql.to_str().unwrap()]);
    ed.wait_for_names().unwrap();
    let o = pick::block_origin(HOLTBURG);
    let place = |ed: &mut EditorApp, model: u32, x: f32, y: f32| {
        ed.edit(&Edit::Op(lyceum_foundry::edit::Op::PlaceStatic {
            landblock: HOLTBURG,
            model: dereth_primitives::DataId(model),
            at: lyceum_foundry::source::Place {
                x,
                y,
                z: lyceum_foundry::source::Height::Ground,
            },
            rotation: lyceum_foundry::source::Rotation::Heading(0.0),
            name: None,
        }))
        .unwrap();
    };
    // In the open meadow north of the houses: a door, and a stone further north.
    place(&mut ed, 0x0200_19FF, 120.0, 117.0);
    place(&mut ed, 0x0200_040B, 120.0, 140.0);
    // North from 5 m south of `y`, a body's height over the ground there: how far north it got.
    let fly_north = |ed: &mut EditorApp, y: f32| {
        let z = ed.session.ground(HOLTBURG, 120.0, y).unwrap();
        ed.collide = false;
        ed.camera.position = Vec3::new(o.x + 120.0, o.y + y - 5.0, z + 1.2);
        ed.camera.yaw = 0.0;
        ed.camera.pitch = 0.0;
        ed.set_speed(2);
        ed.frame();
        ed.collide = true;
        let mut end = ed.camera.position;
        let k = |pressed| HostEvent::KeyboardInput {
            key: dereth_input::keys::Key::new(0x57, 0),
            pressed,
            text: None,
        };
        ed.queue(k(true));
        for _ in 0..600 {
            ed.frame();
            end = ed.camera.position;
            if end.y - o.y > y + 3.0 {
                break;
            }
        }
        ed.queue(k(false));
        ed.frame();
        end.y - o.y
    };
    let past_door = fly_north(&mut ed, 117.0);
    assert!(past_door > 117.0 + 3.0, "through the door: y {past_door}");
    let at_stone = fly_north(&mut ed, 140.0);
    assert!(
        at_stone > 135.0 && at_stone < 140.0,
        "stopped at the stone: y {at_stone}"
    );
}
