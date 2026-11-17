//! The remaster's lamps against Retail Look's light, at the lamps' stations: each drawn by Retail
//! Look and by the remaster with its boxes as the options page ticks them at first, then with one
//! box at a time off, and the brightness of the lit parts printed.
//!
//! - **Indoors** (rooms and dungeons): each room's walls' mean brightness where both drew, every
//!   indoor object's together, and the brightest quarter of Retail Look's indoor pixels against
//!   the same pixels of the remaster.
//! - **Outdoors at night** (the lamps' streets): the ground and what stands on it within the lamps'
//!   pools (where the lamps add light), against the same pixels with the outdoor lamps off and
//!   Retail Look's.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware device.
//!
//! - `DERETH_REMASTER_CAPTURE_DIR`: where the pictures go (none are written without it).
//! - `DERETH_LAMP_LEVEL_STATIONS`: a comma list of the stations (default [`STATIONS`]).
//! - `DERETH_LAMP_LEVEL_TAG`: a word in the remaster's pictures' names, to keep runs apart.
//! - `DERETH_REMASTER_SIZE`: `WIDTHxHEIGHT` (default 960x540).

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_client_runtime::config::Preferences;
use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_dat::RetailDatStore;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::world_scene::RemasterLampCapture;

use super::remaster_frames::{self, Draw};
use super::stations::{self, capture, Shot, Station};

/// The stations measured by default.
const STATIONS: [&str; 7] = [
    "indoor",
    "doorway",
    "dungeon",
    "dungeon-candles",
    "lamps-street",
    "lamps-west",
    "lanterns-square",
];

/// The light classes of the remaster's material target.
const GROUND: u8 = 1;
const SUN: u8 = 2;
const INDOOR: u8 = 3;
const ROOM: u8 = 4;

/// The depth retail stamps an opening with, and anything beyond: not the world.
const STAMP: f32 = 0.999_998;

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// The boxes as the options page ticks them at first, the remaster's own ticked, with `more` over
/// them.
pub(crate) fn at_first(more: &str) -> FidelityPreferences {
    let mut f = FidelityPreferences::from_preferences(&Preferences::default());
    f.apply_switch("Remaster=1").expect("the remaster's box");
    if !more.is_empty() {
        f.apply_switch(more).expect("the boxes read");
    }
    f
}

/// One draw of a station by the remaster: the frame, and on the frame's grid the light class, the
/// room (on its walls), the depth and where the surface stands in the frame's space; what the
/// lamps drew.
pub(crate) struct Drawn {
    pub rgba: Vec<u8>,
    pub class: Vec<u8>,
    pub room: Vec<u16>,
    pub depth: Vec<f32>,
    pub world: Vec<Option<glam::Vec3>>,
    /// The frame's view and projection together.
    pub view_projection: glam::Mat4,
    /// The lamps' light, on the frame's grid (nought where they drew none).
    pub light: Vec<[f32; 4]>,
    pub lamps: Option<RemasterLampCapture>,
}

/// `station` drawn by the remaster with `boxes`, stepped as the light study steps it and then
/// held until the remaster settles.
pub(crate) fn drawn(
    store: &Arc<RetailDatStore>,
    station: &Station,
    boxes: &FidelityPreferences,
    size: (u32, u32),
) -> Drawn {
    let cfg = DeviceConfig {
        width: size.0,
        height: size.1,
        remaster: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let mut shot = Shot::open(store, &mut gpu, station);
    shot.poll(&mut gpu);
    shot.scene.draw.cfg.render.fidelity = *boxes;
    shot.poll(&mut gpu);
    for _ in 0..6 {
        shot.step(&mut gpu);
    }
    shot.scene.wait_for_remaster_ground();
    for _ in 0..24 {
        shot.step_held(&mut gpu);
    }
    let rgba = capture(&mut gpu);
    let (w, h) = gpu.size();
    let c = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew");
    let lamps = shot.scene.draw.remaster_lamp_capture(&mut gpu);
    let n = (w * h) as usize;
    let (mut class, mut room, mut depth) = (vec![0u8; n], vec![0u16; n], vec![1.0f32; n]);
    let mut world = vec![None; n];
    let mut light = vec![[0.0f32; 4]; n];
    let inverse = glam::DMat4::from_cols_array(&c.view_projection.map(f64::from)).inverse();
    for y in 0..c.height {
        for x in 0..c.width {
            let (fx, fy) = (x + c.x, y + c.y);
            if fx < w && fy < h {
                let at = (fy * w + fx) as usize;
                let from = (y * c.width + x) as usize;
                class[at] = c.class[from];
                depth[at] = c.depth[from];
                let d = f64::from(c.depth[from]);
                if d > 0.0 && d < 1.0 {
                    let ndc = glam::DVec4::new(
                        (f64::from(x) + 0.5) / f64::from(c.width) * 2.0 - 1.0,
                        1.0 - (f64::from(y) + 0.5) / f64::from(c.height) * 2.0,
                        d,
                        1.0,
                    );
                    let p = inverse * ndc;
                    world[at] = Some((p.truncate() / p.w).as_vec3());
                }
                if let Some(m) = lamps.as_ref().and_then(|l| l.material.get(from)) {
                    room[at] = u16::from(m[2]) | (u16::from(m[3]) << 8);
                }
                if let Some(l) = lamps
                    .as_ref()
                    .filter(|l| l.width == c.width)
                    .and_then(|l| l.light.get(from))
                {
                    light[at] = *l;
                }
            }
        }
    }
    drop(shot);
    Drawn {
        rgba,
        class,
        room,
        depth,
        world,
        view_projection: glam::Mat4::from_cols_array(&c.view_projection),
        light,
        lamps,
    }
}

/// A pixel's brightness, the mean of its three channels, display levels.
pub(crate) fn level(rgba: &[u8], i: usize) -> f64 {
    (0..3).map(|c| f64::from(rgba[i * 4 + c])).sum::<f64>() / 3.0
}

/// The mean of `f` over `pixels`.
#[allow(clippy::cast_precision_loss)] // pixel counts
pub(crate) fn mean(pixels: &[usize], f: impl Fn(usize) -> f64) -> f64 {
    pixels.iter().map(|&i| f(i)).sum::<f64>() / pixels.len().max(1) as f64
}

/// The indoor pixels both drew, by room (0 for the objects indoors).
pub(crate) fn indoor_pixels(
    retail: &remaster_frames::Frame,
    r: &Drawn,
) -> std::collections::BTreeMap<u32, Vec<usize>> {
    // ORDER-OK: reported in room order.
    let mut out: std::collections::BTreeMap<u32, Vec<usize>> = std::collections::BTreeMap::new();
    for i in 0..r.class.len() {
        let drew = retail.depth.get(i).is_some_and(|d| *d < STAMP) && r.depth[i] < 1.0;
        if !drew {
            continue;
        }
        match r.class[i] {
            ROOM => out.entry(1 + u32::from(r.room[i])).or_default().push(i),
            INDOOR => out.entry(0).or_default().push(i),
            _ => {}
        }
    }
    out
}

/// The brightest quarter of Retail Look's pixels of `pixels`.
pub(crate) fn brightest_quarter(retail: &[u8], pixels: &[usize]) -> Vec<usize> {
    let mut sorted = pixels.to_vec();
    sorted.sort_by(|a, b| level(retail, *b).total_cmp(&level(retail, *a)));
    sorted.truncate(sorted.len() / 4);
    sorted
}

/// The outdoor pixels within the lamps' pools: the ground and what the sun lights where the lamps
/// add at least `least` of light.
pub(crate) fn pool_pixels(r: &Drawn, least: f32) -> Vec<usize> {
    (0..r.class.len())
        .filter(|&i| {
            let p = r.light[i];
            matches!(r.class[i], GROUND | SUN) && p[0].max(p[1]).max(p[2]) >= least
        })
        .collect()
}

/// The outdoor placements' lamps of a draw: where each stands, its color, its reach and its
/// intensity.
pub(crate) fn placed_lamps(d: &Drawn) -> Vec<(glam::Vec3, [f32; 3], f32, f32)> {
    let Some(capture) = d.lamps.as_ref() else {
        return Vec::new();
    };
    capture
        .places
        .iter()
        .zip(&capture.strengths)
        .filter(|((_, _, burned), (indoor, _, _))| *burned && !*indoor)
        .map(|((at, color, _), (_, reach, s))| (glam::Vec3::from_array(*at), *color, *reach, s[2]))
        .collect()
}

/// How far across the ground pixel `i` of `d` stands from the nearest of `lamps`, metres.
pub(crate) fn from_lamp(
    d: &Drawn,
    i: usize,
    lamps: &[(glam::Vec3, [f32; 3], f32, f32)],
) -> Option<f32> {
    let p = d.world[i]?;
    lamps
        .iter()
        .map(|l| (l.0 - p).truncate().length())
        .min_by(f32::total_cmp)
}

/// The distances from a lamp the pool's shape is read at, metres, each over a metre's ring.
pub(crate) const RINGS: [f32; 5] = [1.0, 3.0, 5.0, 8.0, 12.0];

/// The ground of `d` by its distance from the nearest placement's lamp: for each of [`RINGS`], the
/// pixels within half a metre of it; then every pixel within the farthest.
pub(crate) fn rings(d: &Drawn) -> Vec<Vec<usize>> {
    let lamps = placed_lamps(d);
    let mut out = vec![Vec::new(); RINGS.len() + 1];
    for i in 0..d.class.len() {
        if d.class[i] != GROUND {
            continue;
        }
        let Some(r) = from_lamp(d, i, &lamps) else {
            continue;
        };
        for (k, at) in RINGS.iter().enumerate() {
            if (r - at).abs() <= 0.5 {
                out[k].push(i);
            }
        }
        if r <= RINGS[RINGS.len() - 1] {
            out[RINGS.len()].push(i);
        }
    }
    out
}

/// The pool's shape on a frame `rgba`, as one line: how much brighter than `off` the ground is at
/// each of [`RINGS`], and over every pixel within the farthest.
pub(crate) fn pool_shape(rgba: &[u8], off: &[u8], rings: &[Vec<usize>]) -> String {
    let mut line = String::new();
    for (k, px) in rings.iter().enumerate() {
        let what = RINGS
            .get(k)
            .map_or_else(|| "within 12 m".to_owned(), |r| format!("{r} m"));
        line += &format!(
            "{what} {:+.1} ({:.0}), ",
            mean(px, |i| level(rgba, i)) - mean(px, |i| level(off, i)),
            mean(px, |i| level(rgba, i))
        );
    }
    line
}

/// Each station drawn by Retail Look and the remaster, its boxes at first and one at a time off:
/// the lit parts' brightness printed and the pictures written.
#[test]
#[ignore = "instrument: prints the lamps' brightness beside Retail Look's and writes pictures; run with --ignored"]
fn measure_the_lamps_beside_retail() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let names: Vec<String> = env("DERETH_LAMP_LEVEL_STATIONS").map_or_else(
        || STATIONS.iter().map(|s| (*s).to_owned()).collect(),
        |v| v.split(',').map(|s| s.trim().to_owned()).collect(),
    );
    let dir = env("DERETH_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    let tag = env("DERETH_LAMP_LEVEL_TAG").unwrap_or_else(|| "remaster".to_owned());
    let size = remaster_frames::size();
    let write = |name: &str, what: &str, rgba: &[u8]| {
        if let Some(dir) = dir.as_ref() {
            stations::write_png(
                &dir.join(format!("{name}-{what}.png")),
                size.0,
                size.1,
                rgba,
            );
        }
    };
    let variants: [(&str, &str); 6] = [
        ("defaults", ""),
        ("no-indoor-lamps", "RemasterIndoorLamps=0"),
        ("no-outdoor-lamps", "RemasterOutdoorLamps=0"),
        ("no-bloom", "RemasterBloom=0"),
        ("no-bounce", "RemasterBounce=0"),
        ("no-grade", "RemasterGrade=0"),
    ];
    for name in &names {
        let station = remaster_frames::stations(&store, &[name.as_str()])
            .pop()
            .unwrap_or_else(|| panic!("no station {name}"));
        let retail = remaster_frames::run(&store, &station, Draw::Retail, size);
        write(name, "retail", &retail.rgba);
        let draws: Vec<(&str, Drawn)> = variants
            .iter()
            .map(|(v, boxes)| (*v, drawn(&store, &station, &at_first(boxes), size)))
            .collect();
        for (v, d) in &draws {
            write(name, &format!("{tag}-{v}"), &d.rgba);
        }
        let base = &draws[0].1;
        println!("{name}");
        if let Some(l) = base.lamps.as_ref() {
            let mut kinds: Vec<String> = l
                .strengths
                .iter()
                .map(|(indoor, reach, s)| {
                    format!(
                        "{} reach {reach:.1} walls {:.2} objects {:.2} outdoors {:.2}",
                        if *indoor { "indoor" } else { "outdoor" },
                        s[0],
                        s[1],
                        s[2]
                    )
                })
                .collect();
            kinds.sort();
            kinds.dedup();
            println!("  {} lamps: {kinds:?}", l.lamps);
        }
        let rooms = indoor_pixels(&retail, base);
        let all_indoor: Vec<usize> = rooms.values().flatten().copied().collect();
        if all_indoor.len() > 2_000 {
            let top = brightest_quarter(&retail.rgba, &all_indoor);
            let mut line = format!(
                "  indoor {} px: retail {:.1}",
                all_indoor.len(),
                mean(&all_indoor, |i| level(&retail.rgba, i))
            );
            for (v, d) in &draws {
                line += &format!(", {v} {:.1}", mean(&all_indoor, |i| level(&d.rgba, i)));
            }
            println!("{line}");
            let mut line = format!(
                "  brightest quarter {} px: retail {:.1}",
                top.len(),
                mean(&top, |i| level(&retail.rgba, i))
            );
            for (v, d) in &draws {
                line += &format!(", {v} {:.1}", mean(&top, |i| level(&d.rgba, i)));
            }
            println!("{line}");
            for (room, px) in rooms.iter().filter(|r| r.1.len() >= 2_000) {
                let what = if *room == 0 {
                    "objects".to_owned()
                } else {
                    format!("room {:04x}", room - 1)
                };
                let mut line = format!(
                    "  {what} {} px: retail {:.1}",
                    px.len(),
                    mean(px, |i| level(&retail.rgba, i))
                );
                for (v, d) in &draws {
                    line += &format!(", {v} {:.1}", mean(px, |i| level(&d.rgba, i)));
                }
                println!("{line}");
            }
        }
        let lamps = placed_lamps(base);
        if !lamps.is_empty() {
            let off = &draws[2].1;
            let rings = rings(base);
            println!(
                "  the pool's shape, brighter than without the outdoor lamps (and its level): {}",
                pool_shape(&base.rgba, &off.rgba, &rings)
            );
            // Earlier pictures of the same frame, `label=path` with `{name}` for the station.
            for set in env("DERETH_LAMP_LEVEL_PICTURES")
                .unwrap_or_default()
                .split(';')
                .filter_map(|e| e.split_once('='))
            {
                let path = set.1.replace("{name}", name);
                if let Some((_, _, rgba)) = stations::read_png(std::path::Path::new(&path)) {
                    println!("  {:>8}: {}", set.0, pool_shape(&rgba, &off.rgba, &rings));
                }
            }
            #[allow(clippy::cast_precision_loss)] // a window's size
            let (fw, fh) = (size.0 as f32, size.1 as f32);
            for l in &lamps {
                let c = base.view_projection * l.0.extend(1.0);
                if c.w > 0.0 && (c.x / c.w).abs() < 1.0 && (c.y / c.w).abs() < 1.0 {
                    println!(
                        "  a lamp in view at ({:.0},{:.0}) on the frame, {:.1} m off, at ({:.1},{:.1},{:.1}), reach {:.1}, intensity {:.0}",
                        (c.x / c.w + 1.0) * 0.5 * fw,
                        (1.0 - c.y / c.w) * 0.5 * fh,
                        c.w,
                        l.0.x,
                        l.0.y,
                        l.0.z,
                        l.2,
                        l.3
                    );
                }
            }
        }
        for least in [0.02f32, 0.1, 0.3] {
            let pool = pool_pixels(base, least);
            if pool.len() < 50 {
                continue;
            }
            let mut line = format!(
                "  pools (lamps add {least}) {} px: retail {:.1}",
                pool.len(),
                mean(&pool, |i| level(&retail.rgba, i))
            );
            for (v, d) in &draws {
                line += &format!(", {v} {:.1}", mean(&pool, |i| level(&d.rgba, i)));
            }
            line += &format!(
                "; the lamps' light there {:.3}",
                mean(&pool, |i| {
                    let p = base.light[i];
                    f64::from(p[0] + p[1] + p[2]) / 3.0
                })
            );
            println!("{line}");
        }
    }
}

/// How far the outdoor lamps are lit at each hour of each kind of day, by the sun's height, as the
/// remaster works it out: printed, with the sun's brightness and height.
#[test]
#[ignore = "instrument: prints how lit the outdoor lamps are over the day; run with --ignored"]
fn print_the_outdoor_lamps_night_over_the_day() {
    let store = crate::common::dats();
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let sky = region.sky_info.as_ref().expect("the region's sky");
    for (k, group) in sky.day_groups.iter().enumerate() {
        let mut line = format!("group {k:2} {:10}", group.day_name);
        for h in 0..24 {
            #[allow(clippy::cast_precision_loss)] // an hour
            let t = h as f32 / 24.0;
            let l = dereth_world_render::sky::get_lighting(group, t);
            let s = l.sun_vec;
            let b = (s.x * s.x + s.y * s.y + s.z * s.z).sqrt();
            let elevation = if b > 1e-4 { l.sun_vec.z / b } else { -1.0 };
            let n = ((0.18 - elevation) / 0.16).clamp(0.0, 1.0);
            let n = n * n * (3.0 - 2.0 * n);
            line += &format!(
                " {h:02}:{n:.2}/{b:.2}/{:.0}",
                dereth_primitives::num::math::asinf(elevation).to_degrees()
            );
        }
        println!("{line}");
    }
}

/// Each lamps' station at midnight with the boxes at first: how many of its halos something
/// stands in front of, and how many the eye sees, printed.
#[test]
#[ignore = "instrument: prints each lamps' station's halos, cut and seen; run with --ignored"]
fn print_the_halos_cut_and_seen() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let names = env("DERETH_LAMP_LEVEL_STATIONS").unwrap_or_else(|| {
        "lamps-street,lamps-west,lamps-hill,lamps-above,lamps-torch,lamps-lantern,lanterns-facade,lanterns-square".to_owned()
    });
    for name in names.split(',') {
        let station = remaster_frames::stations(&store, &[name.trim()])
            .pop()
            .expect("the station");
        let d = drawn(&store, &station, &at_first(""), (1280, 720));
        let halos = d.lamps.map(|l| l.halos).unwrap_or_default();
        let cut = halos
            .iter()
            .filter(|h| h.surface < h.distance - 0.35 * h.size - 0.15)
            .count();
        println!("{name}: {} halos, {cut} cut", halos.len());
    }
}
