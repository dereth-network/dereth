//! The remaster's rock bands on a steep face ("cliff shelves"), drawn nowhere, far off only (the
//! box) and everywhere, near and far: a body standing at the foot of an unwalkable face with the camera
//! behind it looking up the face, and the same face from farther off. The log says how far the
//! bands take the face off its rounding up from its foot, and how many of each spot's pixels each
//! setting changes against the others.
//!
//! The face is found from where the walk starts: the first unwalkable ground straight ahead of it,
//! the body standing a few meters short of it.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware device.
//!
//! - `DERETH_REMASTER_CAPTURE_DIR`: where the pictures go (none are written without it).
//! - `DERETH_CLIFF_FROM`: where the walk to the face starts and its heading, `block,x,y,degrees`
//!   (block in hex, block-local meters, degrees from north; default up toward a tall face in the
//!   mountains east of Holtburg).
//! - `DERETH_CLIFF_SPOTS`: a comma list of `near`, `mid` and `far` (default all three).
//! - `DERETH_CLIFF_SETTINGS`: a comma list of `off`, `far` and `on` (default all three).
//! - `DERETH_CLIFF_BACK`: how far back from the face's foot the `mid` and `far` spots stand,
//!   meters, `mid,far` (default `120,320`).
//! - `DERETH_REMASTER_LOOK_BOXES`: the boxes, as `--fidelity` takes them (default every box that
//!   draws something of its own).
//! - `DERETH_REMASTER_KNOBS`: the ground's knobs, `name = value` lines joined by `;`.
//! - `DERETH_REMASTER_LOOK_TAG`: a word in the pictures' names, to keep runs apart.
//! - `DERETH_REMASTER_SIZE`: `WIDTHxHEIGHT` (default 960x540).
//! - `DERETH_ART_PACK`: the art pack the remaster's pack boxes draw (default none).

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_dat::RetailDatStore;
use dereth_primitives::num::math;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::art_pack::ArtPack;
use dereth_terrain::land::drawn::{block_field, sample, CliffShelves, Knobs, Point, BLOCK_METERS};

use super::remaster_frames;
use super::remaster_look_frames::EVERY_BOX;
use super::stations::{self, capture, Clock, DatGround, Place, Shot, Station};

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn list(name: &str) -> Option<Vec<String>> {
    env(name).map(|v| v.split(',').map(|s| s.trim().to_owned()).collect())
}

/// The settings, by their names in the pictures.
const SETTINGS: [(&str, CliffShelves); 3] = [
    ("off", CliffShelves::Off),
    ("far", CliffShelves::FarOnly),
    ("on", CliffShelves::On),
];

/// Where a walk starts, global meters, and its heading, radians from north.
pub(crate) struct Walk {
    pub at: (f32, f32),
    pub heading: f32,
}

impl Walk {
    /// Up toward a tall face in the mountains east of Holtburg, three hundred meters high.
    pub(crate) const TALL_FACE: (u16, f32, f32, f32) = (0xBDB8, 152.0, 8.0, -124.0);

    pub(crate) fn from(block: u16, x: f32, y: f32, degrees: f32) -> Self {
        let (bx, by) = (f32::from(block >> 8), f32::from(block & 0xFF));
        Self {
            at: (bx * BLOCK_METERS + x, by * BLOCK_METERS + y),
            heading: degrees.to_radians(),
        }
    }

    /// The point `d` meters along the walk.
    pub(crate) fn along(&self, d: f32) -> (f32, f32) {
        (
            self.at.0 - math::sinf(self.heading) * d,
            self.at.1 + math::cosf(self.heading) * d,
        )
    }
}

/// The face a walk meets: how far along it the first unwalkable ground lies, and how high the
/// face rises over its foot within `reach` meters on. A walk that starts on the face is taken back
/// first, to where twenty meters of walkable ground lie before it.
pub(crate) fn face_ahead(ground: &DatGround, walk: &Walk, reach: f32) -> Option<(f32, f32)> {
    let walkable = |d: f32| {
        let (x, y) = walk.along(d);
        Point::at(ground, x, y).walkable
    };
    let mut d = 0.0f32;
    while !(0..40u8).all(|k| walkable(d - f32::from(k) * 0.5)) {
        d -= 0.5;
        if d < -800.0 {
            return None;
        }
    }
    while d < 800.0 {
        let (x, y) = walk.along(d);
        if !Point::at(ground, x, y).walkable {
            let foot = Point::at(ground, x, y).retail;
            let mut top = foot;
            let mut k = 0.0;
            while k < reach {
                let (x, y) = walk.along(d + k);
                top = top.max(Point::at(ground, x, y).retail);
                k += 1.0;
            }
            return Some((d, top - foot));
        }
        d += 0.5;
    }
    None
}

/// Global meters as a block and block-local meters.
pub(crate) fn local(at: (f32, f32)) -> (u16, f32, f32) {
    let (bx, by) = (
        (at.0 / BLOCK_METERS).floor().clamp(0.0, 254.0),
        (at.1 / BLOCK_METERS).floor().clamp(0.0, 254.0),
    );
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: block coordinates, 0..255.
    let block = ((bx as u16) << 8) | by as u16;
    (block, at.0 - bx * BLOCK_METERS, at.1 - by * BLOCK_METERS)
}

/// The near spot: a body standing `short` meters short of the face's foot, facing it.
pub(crate) fn near_station(walk: &Walk, foot: f32, short: f32) -> Station {
    let (block, x, y) = local(walk.along(foot - short));
    Station {
        name: "near",
        why: "a body at the foot of a steep face",
        place: Place::Ground {
            block,
            x,
            y,
            yaw: -walk.heading.to_degrees(),
        },
        clock: Clock::Sunny(0.5),
        weather: false,
        land_radius: 4,
    }
}

/// A spot `back` meters back from the face's foot, the eye three meters up, looking at the face
/// `up` meters past its foot.
pub(crate) fn far_station(
    name: &'static str,
    walk: &Walk,
    foot: f32,
    back: f32,
    up: f32,
) -> Station {
    let (block, x, y) = local(walk.along(foot - back));
    let at = walk.along(foot + up);
    let (bx, by) = (f32::from(block >> 8), f32::from(block & 0xFF));
    Station {
        name,
        why: "a steep face from afar",
        place: Place::Look {
            block,
            x,
            y,
            eye: 3.0,
            at: [at.0 - bx * BLOCK_METERS, at.1 - by * BLOCK_METERS, f32::NAN],
        },
        clock: Clock::Sunny(0.5),
        weather: false,
        land_radius: 6,
    }
}

/// `station` drawn with `boxes` and the rock bands drawn as `shelves`, the camera behind a body
/// facing `heading`, held still until it settles; `configure` sets anything more on the shot
/// before the boxes take effect.
pub(crate) fn drawn_face(
    store: &Arc<RetailDatStore>,
    station: &Station,
    (boxes, shelves): (&str, CliffShelves),
    heading: f32,
    configure: impl FnOnce(&mut Shot),
) -> Vec<u8> {
    let size = remaster_frames::size();
    let cfg = DeviceConfig {
        width: size.0,
        height: size.1,
        remaster: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let mut shot = Shot::open(store, &mut gpu, station);
    shot.poll(&mut gpu);
    configure(&mut shot);
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(boxes).expect("the boxes read");
    shot.scene.draw.set_remaster_cliff_shelves(shelves);
    if matches!(station.place, Place::Ground { .. }) {
        shot.follow = Some([6.0, 2.2, 0.3, heading]);
    }
    shot.poll(&mut gpu);
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_far_trees();
    for _ in 0..400 {
        shot.step_held(&mut gpu);
        shot.scene.wait_for_remaster_far_trees();
        let s = shot.scene.draw.remaster_forest_stats(&mut gpu);
        if s.is_none_or(|s| s.wanted > 0 && s.pictures >= s.wanted) {
            break;
        }
    }
    for _ in 0..24 {
        shot.step_held(&mut gpu);
    }
    let rgba = capture(&mut gpu);
    drop(shot);
    rgba
}

/// How many pixels of `a` and `b` differ by more than 8 levels in any channel.
pub(crate) fn changed(a: &[u8], b: &[u8]) -> usize {
    a.as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0)
        .filter(|(p, q)| {
            p.iter()
                .zip(q.iter())
                .take(3)
                .any(|(x, y)| x.abs_diff(*y) > 8)
        })
        .count()
}

/// Each spot drawn under each setting, the pictures written under `DERETH_REMASTER_CAPTURE_DIR`.
#[test]
#[ignore = "instrument: writes the remaster's cliff shelves under each setting; run with --ignored"]
fn capture_the_remasters_cliff_shelves() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let ground = DatGround::new(&store);
    let walk = match env("DERETH_CLIFF_FROM") {
        Some(v) => {
            let p: Vec<&str> = v.split(',').map(str::trim).collect();
            let block = u16::from_str_radix(p[0].trim_start_matches("0x"), 16).expect("a block");
            let n = |k: usize| p[k].parse::<f32>().expect("a number");
            Walk::from(block, n(1), n(2), n(3))
        }
        None => {
            let (b, x, y, d) = Walk::TALL_FACE;
            Walk::from(b, x, y, d)
        }
    };
    let (foot, rise) = face_ahead(&ground, &walk, 60.0).expect("a face ahead");
    println!("the face's foot {foot:.1} m ahead, rising {rise:.1} m over the next 60 m");
    // How far the bands take the face off its rounding along the walk, up it.
    let knobs = env("DERETH_REMASTER_KNOBS").map_or_else(Knobs::default, |lines| {
        Knobs::parse(&lines.replace(';', "\n")).0
    });
    let mut most = 0.0f32;
    for k in 0..60u8 {
        let (x, y) = walk.along(foot + f32::from(k));
        let (block, lx, ly) = local((x, y));
        let field = block_field(
            &ground,
            (i32::from(block >> 8), i32::from(block & 0xFF)),
            &[],
            &knobs,
        );
        let bands = sample(&field.bands, lx, ly);
        most = most.max(bands.abs());
        if k % 6 == 0 {
            println!("  {k:2} m up the face: the bands take it {bands:+.2} m off its rounding");
        }
    }
    println!("  the most: {most:.2} m");
    let back: Vec<f32> = env("DERETH_CLIFF_BACK")
        .unwrap_or_else(|| "120,320".to_owned())
        .split(',')
        .map(|v| v.trim().parse().expect("a number"))
        .collect();
    let spots = list("DERETH_CLIFF_SPOTS");
    let settings = list("DERETH_CLIFF_SETTINGS");
    let boxes = env("DERETH_REMASTER_LOOK_BOXES").unwrap_or_else(|| EVERY_BOX.to_owned());
    let dir = env("DERETH_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    let tag = env("DERETH_REMASTER_LOOK_TAG").unwrap_or_else(|| "remaster".to_owned());
    let size = remaster_frames::size();
    let at = [
        near_station(&walk, foot, 5.0),
        far_station("mid", &walk, foot, back[0], 20.0),
        far_station("far", &walk, foot, back[1], 20.0),
    ];
    for station in &at {
        if spots
            .as_ref()
            .is_some_and(|s| !s.iter().any(|n| n == station.name))
        {
            continue;
        }
        let mut frames = Vec::new();
        for (name, value) in SETTINGS {
            if settings
                .as_ref()
                .is_some_and(|s| !s.iter().any(|n| n == name))
            {
                continue;
            }
            let rgba = drawn_face(&store, station, (&boxes, value), walk.heading, |shot| {
                if let Some(path) = env("DERETH_ART_PACK").map(std::path::PathBuf::from) {
                    let pack = Arc::new(ArtPack::open_path(&path).expect("the pack opens"));
                    let folder = path.is_dir().then(|| path.clone());
                    shot.scene.draw.set_remaster_pack(Some(pack), folder);
                }
                if let Some(lines) = env("DERETH_REMASTER_KNOBS") {
                    let (knobs, notes) = Knobs::parse(&lines.replace(';', "\n"));
                    assert!(notes.is_empty(), "the knobs read: {notes:?}");
                    shot.scene.set_remaster_ground_knobs(knobs);
                }
            });
            let what = format!("{}-{name}-{tag}", station.name);
            if let Some(dir) = dir.as_ref() {
                stations::write_png(&dir.join(format!("{what}.png")), size.0, size.1, &rgba);
            }
            frames.push((name, rgba));
        }
        for i in 0..frames.len() {
            for j in i + 1..frames.len() {
                println!(
                    "{}: {} against {}: {} pixels changed",
                    station.name,
                    frames[i].0,
                    frames[j].0,
                    changed(&frames[i].1, &frames[j].1)
                );
            }
        }
    }
}

/// The steepest tall faces within `DERETH_CLIFF_SCOUT` blocks of a block (`block,radius`, block in
/// hex; default Holtburg's, 8): each block's count of unwalkable cells and the tallest relief
/// over them, the twenty with the most, and a walk up to each from downhill of its middle.
#[test]
#[ignore = "instrument: lists steep faces near a block; run with --ignored"]
fn scout_steep_faces() {
    let store = crate::common::dats();
    let ground = DatGround::new(&store);
    let spec = env("DERETH_CLIFF_SCOUT").unwrap_or_else(|| "A9B4,8".to_owned());
    let p: Vec<&str> = spec.split(',').map(str::trim).collect();
    let block = u16::from_str_radix(p[0].trim_start_matches("0x"), 16).expect("a block");
    let r: i32 = p.get(1).map_or(8, |v| v.parse().expect("a radius"));
    let (cx, cy) = (i32::from(block >> 8), i32::from(block & 0xFF));
    let mut found = Vec::new();
    for bx in (cx - r).max(0)..=(cx + r).min(254) {
        for by in (cy - r).max(0)..=(cy + r).min(254) {
            let (mut steep, mut relief, mut sum) = (0u32, 0.0f32, (0.0f32, 0.0f32));
            for i in 0..8 {
                for j in 0..8 {
                    #[allow(clippy::cast_precision_loss)] // cell coordinates
                    let (x, y) = (
                        (bx * 8 + i) as f32 * 24.0 + 12.0,
                        (by * 8 + j) as f32 * 24.0 + 12.0,
                    );
                    let pt = Point::at(&ground, x, y);
                    if !pt.walkable {
                        steep += 1;
                        relief = relief.max(pt.relief);
                        sum = (sum.0 + x, sum.1 + y);
                    }
                }
            }
            if steep > 0 {
                #[allow(clippy::cast_precision_loss)] // a cell count
                let mid = (sum.0 / steep as f32, sum.1 / steep as f32);
                found.push((steep, relief, bx, by, mid));
            }
        }
    }
    found.sort_by_key(|f| std::cmp::Reverse(f.0));
    for (steep, relief, bx, by, mid) in found.iter().take(20) {
        // From 150 meters downhill of the steep cells' middle, walking uphill.
        let g = Point::at(&ground, mid.0, mid.1).gradient;
        let n = (g.0 * g.0 + g.1 * g.1).sqrt().max(1e-6);
        let (ux, uy) = (g.0 / n, g.1 / n);
        let (block, x, y) = local((mid.0 - ux * 150.0, mid.1 - uy * 150.0));
        let heading = math::atan2f(-ux, uy).to_degrees();
        println!(
            "{bx:02X}{by:02X}: {steep} steep cells, relief to {relief:.0} m; a walk from \
             {block:04X},{x:.0},{y:.0},{heading:.0}"
        );
    }
}
