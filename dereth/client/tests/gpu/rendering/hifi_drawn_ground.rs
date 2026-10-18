//! The ground Better Lighting draws itself with an art pack's ground: it holds still under a
//! still camera, it is laid where it lies however a pixel samples it, and a ground's material for
//! its steep slopes covers that ground's cliffs until the slope materials' box is ticked.
//!
//! The pack is made in the test: a flat ground of its own color for each dry terrain type, so a
//! ground laid in the wrong place shows as a color laid in the wrong place, and for the obsidian
//! plains' ground a magenta material for its steep slopes, a color no ground has.
//!
//! The frames are drawn without the anti-aliasing, so every frame of a scene that does not
//! change draws its land the same but for the air's dither of a level or two: what changes more,
//! changes because something drew it differently.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware `wgpu` device off screen.
//!
//! Behaviour: none (experimental hi-fi art packs)

#![cfg(all(gpu, feature = "hifi"))]

use std::collections::HashMap;
use std::sync::Arc;

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_render::device::{AdapterKind, Backend, DeviceConfig, Gpu};
use dereth_scene::art_pack::ArtPack;

use crate::instruments::hifi_stations::{capture, write_png, Clock, Place, Shot, Station};

/// The frame's size.
const SIZE: (u32, u32) = (960, 540);

/// The obsidian plains' terrain type.
const OBSIDIAN: u8 = 6;

/// A four-texel square PNG of one color.
fn flat_png(rgba: [u8; 4]) -> Vec<u8> {
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, 4, 4);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().expect("header");
        w.write_image_data(&rgba.repeat(16)).expect("data");
    }
    png
}

/// A color for terrain type `t`, each its own: no two types alike, and none of them magenta.
fn type_color(t: u8) -> [u8; 4] {
    let h = u32::from(t).wrapping_mul(0x9E37_79B9).rotate_left(7);
    let [a, b, c, _] = h.to_le_bytes();
    [40 + a / 2, 60 + b / 2, 30 + c / 3, 255]
}

/// The pack: a flat ground of its own color for each dry terrain type, and a magenta material
/// for the obsidian plains' steep slopes.
pub(super) fn ground_pack() -> Arc<ArtPack> {
    let mut files: HashMap<String, Vec<u8>> = HashMap::new();
    let mut manifest = String::from("[pack]\nname = \"Grounds\"\nformat = 1\n");
    let credit = "author = \"a\"\ntool = \"t\"\nlicense = \"CC0-1.0\"\n";
    for t in (0..16).chain(21..32) {
        let name = format!("g{t}.png");
        files.insert(name.clone(), flat_png(type_color(t)));
        manifest.push_str(&format!(
            "[[terrain]]\nground = {t}\nbase_color = \"{name}\"\nsize = 4\n{credit}"
        ));
    }
    files.insert("steep.png".to_owned(), flat_png([255, 0, 255, 255]));
    manifest.push_str(&format!(
        "[[steep]]\nground = {OBSIDIAN}\nbase_color = \"steep.png\"\nsize = 4\n{credit}"
    ));
    Arc::new(
        ArtPack::open(&manifest, &mut |path| {
            files
                .get(path)
                .cloned()
                .ok_or_else(|| format!("{path}: missing"))
        })
        .expect("the pack opens"),
    )
}

/// Holtburg from sixty metres up, looking north over the lake to the mountains on the horizon:
/// most of the frame is ground hundreds of metres to tens of kilometres off.
fn over_holtburg() -> Station {
    Station {
        name: "over-holtburg",
        why: "far ground to the horizon",
        place: Place::Free {
            block: 0xA9B3,
            x: 84.0,
            y: 139.0,
            eye: 60.0,
            yaw: 0.0,
            pitch: -0.36,
        },
        clock: Clock::Sunny(0.5),
        weather: false,
        land_radius: 2,
    }
}

/// The crater rim west of Neydisa, the highest ground in Dereth, looking out over the north: most
/// of the frame is ground kilometres off, to the horizon.
fn neydisa_rim() -> Station {
    Station {
        name: "neydisa-rim",
        why: "far ground to the horizon",
        place: Place::Free {
            block: 0x92D1,
            x: 83.0,
            y: 12.0,
            eye: 6.0,
            yaw: -2.79,
            pitch: -0.122,
        },
        clock: Clock::Sunny(0.5),
        weather: false,
        land_radius: 2,
    }
}

/// Half a kilometre west of an obsidian peak on the obsidian plains, looking up at its cliffs.
fn obsidian_cliffs() -> Station {
    Station {
        name: "obsidian-cliffs",
        why: "the obsidian plains' cliffs",
        place: Place::Free {
            block: 0x2084,
            x: 96.0,
            y: 96.0,
            eye: 40.0,
            yaw: -std::f32::consts::FRAC_PI_2,
            pitch: 0.42,
        },
        clock: Clock::Sunny(0.5),
        weather: false,
        land_radius: 2,
    }
}

/// The boxes every frame is drawn with: the light, the sun's shadows and the sky, without the
/// anti-aliasing.
const BOXES: &str = "Lighting=1,Shadows=1,Sky=1,NoAntiAliasing=1";

/// `station` opened on a device of its own with `pack` and `boxes`, the ground the pack lays
/// drawn and settled.
fn opened(station: &Station, pack: &Arc<ArtPack>, boxes: &str) -> (Gpu, Shot) {
    let store = crate::common::dats();
    let cfg = DeviceConfig {
        width: SIZE.0,
        height: SIZE.1,
        hifi: true,
        ..DeviceConfig::default()
    };
    let mut device = Gpu::new_on(Backend::Wgpu, None, &cfg)
        .unwrap_or_else(|e| panic!("this test needs a wgpu device and none opened: {e}"));
    assert_eq!(
        device.adapter_kind(),
        AdapterKind::Hardware,
        "{} is a software rasteriser",
        device.adapter_name()
    );
    let gpu = &mut device;
    let mut shot = Shot::open(&store, gpu, station);
    shot.scene
        .set_art_pack(&store, gpu, Some(Arc::clone(pack)))
        .expect("the blocks bake for the pack");
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(boxes).expect("the boxes parse");
    shot.poll(gpu);
    shot.step(gpu);
    shot.scene.wait_for_drawn_ground();
    shot.poll(gpu);
    for _ in 0..12 {
        shot.step(gpu);
        assert!(
            gpu.hifi_settle(std::time::Duration::from_secs(60)),
            "the presentation's pipelines were not built in a minute"
        );
    }
    shot.step(gpu);
    assert!(gpu.hifi_failed().is_none(), "{:?}", gpu.hifi_failed());
    assert!(
        shot.scene.drawn_ground_active(),
        "the presentation draws the ground"
    );
    (device, shot)
}

/// Where the frames go, in cargo's scratch folder, to look at when a test fails.
fn keep(name: &str, rgba: &[u8]) {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("hifi-drawn-ground");
    std::fs::create_dir_all(&dir).expect("the scratch folder");
    write_png(&dir.join(format!("{name}.png")), SIZE.0, SIZE.1, rgba);
}

fn luminance(p: &[u8]) -> i32 {
    (i32::from(p[0]) * 3 + i32::from(p[1]) * 6 + i32::from(p[2])) / 10
}

/// The pixels where `a` is smooth (each of its 3 by 3 neighbors within four levels of it) and
/// `b` differs from it by more than 32 levels: a picture moved a fraction of a pixel changes a
/// smooth stretch by a fraction of its little contrast, so these are pixels drawn as something
/// else.
fn smooth_changes(a: &[u8], b: &[u8]) -> usize {
    let (w, h) = (SIZE.0 as usize, SIZE.1 as usize);
    let at = |img: &[u8], x: usize, y: usize| luminance(&img[(y * w + x) * 4..(y * w + x) * 4 + 4]);
    let mut n = 0;
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let c = at(a, x, y);
            let smooth = (0..9).all(|k| (at(a, x + k % 3 - 1, y + k / 3 - 1) - c).abs() <= 4);
            if smooth && (at(b, x, y) - c).abs() > 32 {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn a_still_camera_stepped_with_its_clock_held_draws_the_same_frame_again() {
    let _gpu = crate::common::gpu_lock();
    let pack = ground_pack();
    let (mut device, mut shot) = opened(&over_holtburg(), &pack, BOXES);
    let gpu = &mut device;
    shot.step_held(gpu);
    let first = capture(gpu);
    keep("still-first", &first);
    for k in 0..6 {
        shot.step_held(gpu);
        let again = capture(gpu);
        // The land below the horizon (which lies in the top fifth of the frame): the air's
        // dither moves a level or two from frame to frame, nothing more; the sky's clouds drift.
        let row = SIZE.0 as usize * 4;
        let below = SIZE.1 as usize / 5 * row;
        let differ = first[below..]
            .as_chunks::<4>()
            .0
            .iter()
            .zip(again[below..].as_chunks::<4>().0)
            .filter(|(p, q)| (0..3).any(|c| p[c].abs_diff(q[c]) > 8))
            .count();
        if differ != 0 {
            keep("still-again", &again);
        }
        assert_eq!(
            differ, 0,
            "frame {k} after the first differs in {differ} pixels"
        );
    }
}

#[test]
fn the_far_ground_is_laid_where_it_lies_whatever_point_of_a_pixel_is_drawn() {
    let _gpu = crate::common::gpu_lock();
    let pack = ground_pack();
    let (mut device, mut shot) = opened(&neydisa_rim(), &pack, BOXES);
    let gpu = &mut device;
    shot.draw(gpu);
    let first = capture(gpu);
    keep("far-first", &first);
    // A third of a pixel's turn each way, across and up, as the anti-aliasing's jitter moves the
    // picture from one frame to the next.
    let pixel = 1.2 / f32::from(u16::try_from(SIZE.0).expect("a width"));
    let (yaw, pitch) = (shot.scene.camera.yaw, shot.scene.camera.pitch);
    let mut changed = 0;
    for (k, (dy, dp)) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)]
        .into_iter()
        .enumerate()
    {
        shot.scene.camera.yaw = yaw + dy * pixel / 3.0;
        shot.scene.camera.pitch = pitch + dp * pixel / 3.0;
        shot.draw(gpu);
        let turned = capture(gpu);
        keep(&format!("far-turned-{k}"), &turned);
        changed += smooth_changes(&first, &turned);
    }
    eprintln!("{changed} pixels of smooth ground drawn as other ground over four turns");
    assert!(
        changed < 200,
        "{changed} pixels of smooth ground were drawn as other ground over four turns"
    );
}

/// The share of the frame's pixels that are magenta: red and blue well over green.
fn magenta(rgba: &[u8]) -> f64 {
    let px = rgba.as_chunks::<4>().0;
    let n = px
        .iter()
        .filter(|p| {
            let (r, g, b) = (i32::from(p[0]), i32::from(p[1]), i32::from(p[2]));
            r - g > 60 && b - g > 60
        })
        .count();
    #[allow(clippy::cast_precision_loss)] // pixel counts
    let share = n as f64 / px.len() as f64;
    share
}

#[test]
fn a_grounds_steep_material_covers_its_cliffs_until_the_slope_materials_box_is_ticked() {
    let _gpu = crate::common::gpu_lock();
    let pack = ground_pack();
    assert_eq!(pack.grounds.len(), 28, "{:?}", pack.problems);
    assert!(pack
        .steep_ground(dereth_scene::art_pack::Ground::Type(OBSIDIAN))
        .is_some());
    let (mut device, mut shot) = opened(&obsidian_cliffs(), &pack, BOXES);
    let gpu = &mut device;
    let steep = capture(gpu);
    // Ticked, it takes effect on the next frame.
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(&format!("{BOXES},NoSlopeMaterials=1"))
            .expect("the boxes parse");
    shot.poll(gpu);
    shot.step(gpu);
    let off = capture(gpu);
    keep("cliffs-steep", &steep);
    keep("cliffs-no-slope-materials", &off);
    assert!(
        magenta(&steep) > 0.02,
        "{} of the frame is the obsidian's steep material",
        magenta(&steep)
    );
    assert!(
        magenta(&off) < 0.0005,
        "{} of the frame is still the steep material with the box ticked",
        magenta(&off)
    );
}

/// On the hill above Holtburg beside a stone house, whose own cell takes in the hilltop, at a
/// standing eye's height, looking north down the hill toward the town: past the hill's crest
/// stand the town's houses and their doorways.
fn hill_beside_a_house() -> Station {
    Station {
        name: "hill-beside-a-house",
        why: "a hill in a house's own cell, with other houses' doorways beyond it",
        place: Place::Free {
            block: 0xA9B4,
            x: 71.6,
            y: 39.6,
            eye: 1.6,
            yaw: -0.2657,
            pitch: -0.1,
        },
        clock: Clock::Sunny(0.5),
        weather: false,
        land_radius: 2,
    }
}

/// The pixels of the frame's lower half (the ground near the viewer) that differ by more than
/// 32 levels in any channel between `a` and `b`.
fn lower_changes(a: &[u8], b: &[u8]) -> usize {
    let (w, h) = (SIZE.0 as usize, SIZE.1 as usize);
    (h / 2..h)
        .flat_map(|y| (0..w).map(move |x| (y * w + x) * 4))
        .filter(|&i| (0..3).any(|c| a[i + c].abs_diff(b[i + c]) > 32))
        .count()
}

#[test]
fn no_doorway_shows_through_a_hill_in_a_houses_own_cell() {
    let _gpu = crate::common::gpu_lock();
    let pack = ground_pack();
    let (mut device, mut shot) = opened(&hill_beside_a_house(), &pack, BOXES);
    let gpu = &mut device;
    shot.draw(gpu);
    let stamped = capture(gpu);
    // The same frame with no building stamping its openings: nothing shows through any ground.
    shot.scene.draw.cfg.portal_depth_stamp = false;
    shot.draw(gpu);
    shot.draw(gpu);
    let unstamped = capture(gpu);
    let through = lower_changes(&stamped, &unstamped);
    if through > 0 {
        keep("hill-stamped", &stamped);
        keep("hill-unstamped", &unstamped);
    }
    assert_eq!(
        through, 0,
        "{through} pixels of the hill show a doorway through it"
    );
}
