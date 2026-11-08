//! The remaster beside Retail Look at the twelve stations of the light study: each station drawn
//! by Retail Look, by the remaster with retail's light worked out per vertex, and by the remaster
//! with it worked out per pixel, each on a device of its own, loaded and stepped the same way.
//!
//! The numbers are taken on the lit pixels, those both drew the world on with a lit surface:
//! the mean error in display levels over the three channels, and the share of pixels whose
//! largest channel error is over 8. The silhouettes are the pixels only one of the two drew the
//! world on. Retail Look's own depth is read through a world hook that draws nothing, which keeps
//! the frame's depth without changing a pixel of it.
//!
//! The weather's sheets drawn over the world are left out of every run, as the study left them
//! out.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.
//!
//! - `DERETH_REMASTER_CAPTURE_DIR`: where the pictures go (none are written without it).
//! - `DERETH_REMASTER_STATIONS`: a comma list of the stations to draw (default: all twelve).
//! - `DERETH_REMASTER_SIZE`: `WIDTHxHEIGHT` (default 960x540).
//! - `DERETH_REMASTER_SHADOWS`: set, each station is drawn per pixel under the sun's shadows too,
//!   with a picture of the sun's share at each pixel.
//! - `DERETH_REMASTER_SKY`: set, each station is drawn per pixel under the sky's atmosphere and
//!   the fog from its volume too, the fog glowing as `DERETH_REMASTER_GLOW` says (default 0).
//! - `DERETH_REMASTER_GROUND`: a comma list of the ground's pieces the remaster draws: `splat`,
//!   `grid`, `drawn`, `horizon` (default none: retail's grid of composites).

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_dat::RetailDatStore;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_render::vulkan::world_hook::Untouched;
use dereth_scene::world_scene::RemasterCounts;

use super::stations::{self, capture, Shot, Station};

/// The stations of the light study, in its order.
pub(crate) const STATIONS: [&str; 12] = [
    "holtburg",
    "forest",
    "dusk",
    "rain",
    "rain-forest",
    "night",
    "lamps-street",
    "snow-village",
    "doorway",
    "indoor",
    "dungeon",
    "dungeon-candles",
];

/// How a run draws its station.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Draw {
    /// Retail Look, through a hook that draws nothing, so its depth is kept.
    Retail,
    /// The remaster, retail's light per vertex.
    Vertex,
    /// The remaster, retail's light per pixel.
    Pixel,
    /// The remaster, retail's light per pixel, under the sun's shadows.
    Shadowed,
    /// The remaster, retail's light per pixel, under the sky's atmosphere.
    Atmosphere,
}

/// One run's last frame.
#[derive(Debug, Clone)]
pub(crate) struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Retail Look's depth, a value a pixel; empty for the remaster.
    pub depth: Vec<f32>,
    /// The remaster's light class and depth, a value a pixel of the frame; empty for Retail Look.
    pub class: Vec<u8>,
    pub remaster_depth: Vec<f32>,
    /// The remaster's material kind at each pixel, and on the ground the water's share of it.
    pub kind: Vec<u8>,
    pub water: Vec<u8>,
    pub counts: Option<RemasterCounts>,
    /// The remaster's passes' times on the device in its last frame, milliseconds.
    pub timings: Vec<(&'static str, f64)>,
    /// The remaster's pipelines built before and after its frames.
    pub pipelines_at_start: u64,
    /// The sun's share at each pixel of the frame, when the shadows were drawn.
    pub sun_visibility: Vec<u8>,
    /// The remaster's view and projection together, as sixteen numbers by column; the identity
    /// for Retail Look.
    pub view_projection: [f32; 16],
}

/// What a run sets beyond its draw: the sky, the ground, a report whose camera, time and
/// weather it holds, the passes past retail's own light (every one off when none are given), and
/// the device's backend (the one the tests draw the remaster on when none is given).
#[derive(Debug, Clone, Default)]
pub(crate) struct Setup<'a> {
    pub sky: Option<dereth_scene::world_scene::RemasterSky>,
    pub ground: Option<dereth_scene::world_scene::RemasterTerrain>,
    pub report: Option<&'a dereth_client_runtime::report::VisualReport>,
    pub settings: Option<dereth_scene::world_scene::RemasterSettings>,
    pub backend: Option<dereth_render::device::Backend>,
    pub lod: Option<dereth_scene::world_scene::RemasterLod>,
    /// How many frames are stepped before the one kept, past the usual.
    pub more_steps: usize,
}

/// The ground `DERETH_REMASTER_GROUND` names, when it names one.
pub(crate) fn ground_from_env() -> Option<dereth_scene::world_scene::RemasterTerrain> {
    let list = env("DERETH_REMASTER_GROUND")?;
    let has = |w: &str| list.split(',').any(|x| x.trim() == w);
    Some(dereth_scene::world_scene::RemasterTerrain {
        splat: has("splat"),
        grid: has("grid"),
        drawn: has("drawn"),
        horizon: has("horizon"),
    })
}

/// The study's stations, as the instruments define them.
pub(crate) fn stations(store: &RetailDatStore, names: &[&str]) -> Vec<Station> {
    let all: Vec<Station> = stations::stations(store)
        .into_iter()
        .chain(stations::close_stations(store))
        .collect();
    names
        .iter()
        .filter_map(|n| all.iter().find(|s| s.name == *n).cloned())
        .map(|mut s| {
            s.weather = false;
            s
        })
        .collect()
}

/// The frames a run is stepped before its last.
const STEPS: usize = 6;

/// Draw `station` as `draw` says, on a device of its own, and its last frame.
pub(crate) fn run(
    store: &Arc<RetailDatStore>,
    station: &Station,
    draw: Draw,
    size: (u32, u32),
) -> Frame {
    let sky = (draw == Draw::Atmosphere).then(|| dereth_scene::world_scene::RemasterSky {
        atmosphere: true,
        fog: true,
        far_keep: 0.0,
        air: 0.0,
        glow: env("DERETH_REMASTER_GLOW")
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0.0),
    });
    run_with(store, station, draw, size, sky)
}

/// [`run`], the remaster's sky set to `sky` when it is given.
pub(crate) fn run_with(
    store: &Arc<RetailDatStore>,
    station: &Station,
    draw: Draw,
    size: (u32, u32),
    sky: Option<dereth_scene::world_scene::RemasterSky>,
) -> Frame {
    run_set(
        store,
        station,
        draw,
        size,
        &Setup {
            sky,
            ground: ground_from_env(),
            ..Setup::default()
        },
    )
}

/// [`run`], set up as `setup` says.
pub(crate) fn run_set(
    store: &Arc<RetailDatStore>,
    station: &Station,
    draw: Draw,
    size: (u32, u32),
    setup: &Setup<'_>,
) -> Frame {
    let sky = setup.sky;
    let cfg = DeviceConfig {
        width: size.0,
        height: size.1,
        remaster: true,
        debug: true,
        ..DeviceConfig::default()
    };
    let backend = setup
        .backend
        .unwrap_or_else(crate::common::remaster_backend);
    let mut gpu = Gpu::new_on(backend, None, &cfg).expect("a device");
    assert_eq!(
        gpu.adapter_kind(),
        dereth_render::device::AdapterKind::Hardware,
        "{} is a software rasterizer",
        gpu.adapter_name()
    );
    assert!(
        gpu.remaster_supported(),
        "{} draws the remaster",
        gpu.adapter_name()
    );
    let mut shot = Shot::open(store, &mut gpu, station);
    if let Some(r) = setup.report {
        shot.apply_report(r);
    }
    shot.poll(&mut gpu);
    let mut pipelines_at_start = 0;
    match draw {
        Draw::Retail => {
            gpu.install_world_hook(Box::new(Untouched::default()))
                .expect("installs");
        }
        Draw::Vertex | Draw::Pixel | Draw::Shadowed | Draw::Atmosphere => {
            shot.scene
                .draw
                .set_remaster(true, store, &mut gpu)
                .expect("the remaster starts");
            // Every pass past retail's own light off, but for what the run asks for.
            shot.scene.draw.set_remaster_settings(
                setup
                    .settings
                    .unwrap_or_else(dereth_scene::world_scene::RemasterSettings::off),
                &mut gpu,
            );
            if let Some(sky) = sky {
                shot.scene.draw.set_remaster_sky(sky, &mut gpu);
            }
            if let Some(lod) = setup.lod {
                shot.scene.draw.set_remaster_lod(lod, &mut gpu);
            }
            if let Some(ground) = setup.ground {
                shot.scene.draw.set_remaster_terrain(ground, &mut gpu);
            }
            if draw != Draw::Vertex {
                shot.scene
                    .draw
                    .set_remaster_light(dereth_remaster_light_pixel(), &mut gpu);
            }
            if draw == Draw::Shadowed {
                shot.scene.draw.set_remaster_shadows(
                    dereth_scene::world_scene::RemasterShadows {
                        on: true,
                        ..Default::default()
                    },
                    &mut gpu,
                );
            }
            pipelines_at_start = shot
                .scene
                .draw
                .remaster_counts(&mut gpu)
                .map_or(0, |c| c.pipelines_built);
        }
    }
    for k in 0..STEPS + setup.more_steps {
        shot.step(&mut gpu);
        // The world's vertices are read on a thread of their own, asked for by the first frame:
        // wait for them, so the frames after draw the ground from them. Every draw of a run
        // steps alike, Retail Look's too.
        if k == 0 {
            shot.scene.wait_for_remaster_ground();
        }
    }
    let rgba = capture(&mut gpu);
    let (w, h) = gpu.size();
    let mut frame = Frame {
        width: w,
        height: h,
        rgba,
        depth: Vec::new(),
        class: Vec::new(),
        remaster_depth: Vec::new(),
        kind: Vec::new(),
        water: Vec::new(),
        counts: None,
        timings: Vec::new(),
        pipelines_at_start,
        sun_visibility: Vec::new(),
        view_projection: glam::Mat4::IDENTITY.to_cols_array(),
    };
    match draw {
        Draw::Retail => {
            frame.depth = gpu.capture_depth().expect("the depth");
            drop(gpu.take_world_hook());
        }
        Draw::Vertex | Draw::Pixel | Draw::Shadowed | Draw::Atmosphere => {
            frame.counts = shot.scene.draw.remaster_counts(&mut gpu);
            frame.timings = shot.scene.draw.remaster_timings(&mut gpu);
            let c = shot
                .scene
                .draw
                .remaster_capture(&mut gpu)
                .expect("the remaster drew");
            frame.view_projection = c.view_projection;
            // The remaster's pictures placed on the frame.
            frame.class = vec![0; (w * h) as usize];
            frame.kind = vec![0; (w * h) as usize];
            frame.water = vec![0; (w * h) as usize];
            frame.remaster_depth = vec![1.0; (w * h) as usize];
            if c.sun_visibility.is_some() {
                frame.sun_visibility = vec![255; (w * h) as usize];
            }
            for y in 0..c.height {
                for x in 0..c.width {
                    let (fx, fy) = (x + c.x, y + c.y);
                    if fx < w && fy < h {
                        let at = (fy * w + fx) as usize;
                        let from = (y * c.width + x) as usize;
                        frame.class[at] = c.class[from];
                        frame.kind[at] = c.kind[from];
                        frame.water[at] = c.water[from];
                        frame.remaster_depth[at] = c.depth[from];
                        if let Some(v) = c.sun_visibility.as_ref() {
                            frame.sun_visibility[at] = v[from];
                        }
                    }
                }
            }
            shot.scene
                .draw
                .set_remaster(false, store, &mut gpu)
                .expect("the remaster stops");
        }
    }
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(
        errors.is_empty(),
        "{} {draw:?}: the validation layer: {errors:#?}",
        station.name
    );
    frame
}

/// The per-pixel light mode, by its scene-side name.
fn dereth_remaster_light_pixel() -> dereth_scene::world_scene::RemasterLight {
    dereth_scene::world_scene::RemasterLight::Pixel
}

/// The numbers of one comparison.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Numbers {
    /// The lit pixels compared.
    pub lit: usize,
    /// Their mean error, display levels, over the three channels.
    pub mean: f64,
    /// The share of them whose largest channel error is over 8, percent.
    pub over8: f64,
    /// The pixels only one of the two drew the world on, percent of the frame.
    pub silhouette: f64,
}

/// The depth retail stamps an opening with, and anything beyond: not the world.
const STAMP: f32 = 0.999_998;

/// The remaster's frame `r` against Retail Look's `retail`.
pub(crate) fn compare(retail: &Frame, r: &Frame) -> Numbers {
    let n = retail.rgba.len() / 4;
    let (mut lit, mut sum, mut over, mut sil) = (0usize, 0u64, 0usize, 0usize);
    for i in 0..n {
        // An opening's depth stamp (the far depth retail writes through a doorway) is not the
        // world.
        let retail_drew = retail.depth.get(i).is_some_and(|d| *d < STAMP);
        let class = r.class.get(i).copied().unwrap_or(0);
        let remaster_drew = r.remaster_depth.get(i).is_some_and(|d| *d < 1.0);
        if retail_drew != remaster_drew {
            sil += 1;
        }
        if !(retail_drew && (1..=4).contains(&class)) {
            continue;
        }
        lit += 1;
        let mut worst = 0;
        for c in 0..3 {
            let e = retail.rgba[i * 4 + c].abs_diff(r.rgba[i * 4 + c]);
            sum += u64::from(e);
            worst = worst.max(e);
        }
        if worst > 8 {
            over += 1;
        }
    }
    #[allow(clippy::cast_precision_loss)] // pixel counts
    Numbers {
        lit,
        mean: if lit == 0 {
            0.0
        } else {
            sum as f64 / (lit * 3) as f64
        },
        over8: if lit == 0 {
            0.0
        } else {
            over as f64 * 100.0 / lit as f64
        },
        silhouette: sil as f64 * 100.0 / n.max(1) as f64,
    }
}

/// An error picture: black none, then blue, yellow, red at 4, 16 and 32 levels or more.
fn heat(retail: &Frame, r: &Frame) -> Vec<u8> {
    let n = retail.rgba.len() / 4;
    let mut out = vec![0u8; n * 4];
    for i in 0..n {
        let e = (0..3)
            .map(|c| retail.rgba[i * 4 + c].abs_diff(r.rgba[i * 4 + c]))
            .max()
            .unwrap_or(0);
        let c: [u8; 3] = match e {
            0 => [0, 0, 0],
            1..=3 => [20, 40, 160],
            4..=15 => [60, 90, 230],
            16..=31 => [255, 216, 26],
            _ => [255, 26, 13],
        };
        out[i * 4..i * 4 + 3].copy_from_slice(&c);
        out[i * 4 + 3] = 255;
    }
    out
}

/// A silhouette picture: white where only Retail Look drew the world, red where only the
/// remaster did, gray where both did.
fn silhouettes(retail: &Frame, r: &Frame) -> Vec<u8> {
    let n = retail.rgba.len() / 4;
    let mut out = vec![0u8; n * 4];
    for i in 0..n {
        let a = retail.depth.get(i).is_some_and(|d| *d < STAMP);
        let b = r.remaster_depth.get(i).is_some_and(|d| *d < 1.0);
        let c: [u8; 3] = match (a, b) {
            (true, false) => [255, 255, 255],
            (false, true) => [255, 0, 0],
            (true, true) => [60, 60, 60],
            (false, false) => [0, 0, 0],
        };
        out[i * 4..i * 4 + 3].copy_from_slice(&c);
        out[i * 4 + 3] = 255;
    }
    out
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// The size the stations are drawn at.
pub(crate) fn size() -> (u32, u32) {
    env("DERETH_REMASTER_SIZE")
        .and_then(|s| {
            let (w, h) = s.trim().split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or((960, 540))
}

/// Each station drawn by Retail Look and by the remaster both ways: the numbers printed, and the
/// pictures written under `DERETH_REMASTER_CAPTURE_DIR` when it is set.
#[test]
#[ignore = "instrument: prints the remaster's numbers at the stations and writes pictures; run with --ignored"]
fn capture_the_remaster_beside_retail_look() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let names: Vec<String> = env("DERETH_REMASTER_STATIONS").map_or_else(
        || STATIONS.iter().map(|s| (*s).to_owned()).collect(),
        |v| v.split(',').map(|s| s.trim().to_owned()).collect(),
    );
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let dir = env("DERETH_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    let size = size();
    println!("station          parity mean  >8%   per pixel mean  >8%   silhouettes  lit");
    for station in stations(&store, &names) {
        let retail = run(&store, &station, Draw::Retail, size);
        let vertex = run(&store, &station, Draw::Vertex, size);
        let pixel = run(&store, &station, Draw::Pixel, size);
        let v = compare(&retail, &vertex);
        let p = compare(&retail, &pixel);
        println!(
            "{:16} {:>10.2} {:>5.1} {:>14.2} {:>5.1} {:>12.2} {:>6}",
            station.name, v.mean, v.over8, p.mean, p.over8, v.silhouette, v.lit
        );
        if let Some(c) = vertex.counts {
            println!(
                "  counts: {c:?}; pipelines at start {}",
                vertex.pipelines_at_start
            );
        }
        if !pixel.timings.is_empty() {
            let line: Vec<String> = pixel
                .timings
                .iter()
                .map(|(l, ms)| format!("{l} {ms:.3}"))
                .collect();
            println!("  per pixel, ms: {}", line.join(", "));
        }
        if let Some(dir) = dir.as_ref() {
            let (w, h) = (retail.width, retail.height);
            let name = |what: &str| dir.join(format!("{}-{what}.png", station.name));
            stations::write_png(&name("retail"), w, h, &retail.rgba);
            stations::write_png(&name("remaster-vertex"), w, h, &vertex.rgba);
            stations::write_png(&name("remaster-pixel"), w, h, &pixel.rgba);
            stations::write_png(&name("error-vertex"), w, h, &heat(&retail, &vertex));
            stations::write_png(&name("error-pixel"), w, h, &heat(&retail, &pixel));
            stations::write_png(&name("silhouettes"), w, h, &silhouettes(&retail, &vertex));
            if env("DERETH_REMASTER_SKY").is_some() {
                let sky = run(&store, &station, Draw::Atmosphere, size);
                stations::write_png(&name("remaster-sky"), w, h, &sky.rgba);
            }
            if env("DERETH_REMASTER_SHADOWS").is_some() {
                let shadowed = run(&store, &station, Draw::Shadowed, size);
                stations::write_png(&name("remaster-shadowed"), w, h, &shadowed.rgba);
                let grey: Vec<u8> = shadowed
                    .sun_visibility
                    .iter()
                    .flat_map(|v| [*v, *v, *v, 255])
                    .collect();
                if !grey.is_empty() {
                    stations::write_png(&name("sun-visibility"), w, h, &grey);
                }
                if let Some(c) = shadowed.counts {
                    println!("  shadows: {:?}", c.shadows);
                }
            }
        }
    }
}
