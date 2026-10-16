//! Frame writers for an art pack in the high-fidelity presentation: each place and time drawn
//! three ways at one camera -- the ordinary renderer, the presentation with the retail look, and
//! the presentation with the pack's models -- written as PNGs with the frame times. They assert
//! nothing.
//!
//! - `DERETH_ART_PACK`: the pack's folder (required).
//! - `DERETH_HIFI_CAPTURE_DIR`: where the files go (default: `art-pack/` in cargo's scratch folder
//!   for integration tests).
//! - `DERETH_ART_PACK_STATIONS`: a comma list of the places to draw, or `all` (the default).
//! - `DERETH_ART_PACK_TIMES`: a comma list of `day`, `overcast`, `dusk` and `night` (default all
//!   four).
//! - `DERETH_ART_PACK_WAYS`: a comma list of `retail`, `hifi` and `pack` (default all three).
//! - `DERETH_ART_PACK_VIEWS`: `0` leaves out the pack's normals and sun shadow views.
//! - `DERETH_ART_PACK_AT`: `<name>,<block hex>,<x>,<y>,<eye>,<yaw>,<pitch>`, more places, a `;`
//!   between them: each a free camera that far over the ground at the block-local point, turned
//!   and pitched (radians).
//! - `DERETH_HIFI_SIZE`: `WxH`, default `1920x1080`; the frames are timed at the same size.
//! - `DERETH_HIFI_RENDER_SCALE`: the share of the frame's pixels the presentation draws its world
//!   at, each way, percent (default 100).
//! - `DERETH_ART_PACK_WARM`: how many frames a set is stepped before its frame is taken
//!   (default 24).
//! - `DERETH_ART_PACK_LAND_RADIUS`: the blocks of landscape loaded round the viewer's each way
//!   (default 3; the client's own default is 8).
//! - `DERETH_ART_PACK_BLENDING`: `composite` to draw the landscape from its cells' composites;
//!   otherwise its layers are blended as it is drawn, the client's own default.
//! - `DERETH_ART_PACK_FIDELITY`: more of the presentation's boxes, `Name=value` a comma between,
//!   for every frame drawn with it (`NoAntiAliasing=1`, say).
//! - `DERETH_ART_PACK_MOTION`: for the frames in motion, a comma list of `pan`, `walk` and
//!   `still` (default `pan,walk`), or `dolly` (the free camera moving forward);
//!   `DERETH_ART_PACK_MOTION_FRAMES`, how many frames each draws (default 40);
//!   `DERETH_ART_PACK_STILL_AT`, the place `still` and `dolly` stand at (default `forest`);
//!   `DERETH_ART_PACK_PAN_AT`, the place `pan` slides from (default `forest`);
//!   `DERETH_ART_PACK_MOTION_TIME`, the time of day they are drawn at, one of the times below
//!   (default `day`).
//! - `DERETH_ART_PACK_IN`: `<name>,<cell hex>,<facing>[,<x>,<y>,<z>]`, more places, a `;`
//!   between them: each a body standing in an indoor cell (a room or a dungeon's), facing that
//!   many degrees, at the block-local point when one is named (a standable point of the cell
//!   otherwise), seen from its chase camera.
//! - `DERETH_ART_PACK_WALK_AT`: `<name>,<block hex>,<x>,<y>,<facing>`, a body standing on the
//!   ground at the block-local point, facing that many degrees, for `walk` (default the forest's
//!   `player`).
//! - `DERETH_TERRAIN_KNOBS`: the drawn ground's knobs for the frames drawn with the pack, in the
//!   tuning file's form with `;` between lines (`walk_cap = 0.3; cliffs = false`); the frames wait
//!   for the world's landscape vertices the drawn ground is drawn from.
//! - `DERETH_REPORT`: for [`capture_the_frame_a_report_describes`], the line the client's
//!   `@report` printed, or the name of a file holding it. The frame is drawn as the report says it
//!   was: the body where the player stood, the camera held where it stood, the time, the weather,
//!   the window's size, the field of view, the landscape's draw distance, Better Lighting's
//!   effects and render scale, and the drawn ground's changed knobs; drawn the way the report says
//!   unless `DERETH_ART_PACK_WAYS` names others. `DERETH_HIFI_SIZE`, `DERETH_HIFI_RENDER_SCALE`,
//!   `DERETH_ART_PACK_LAND_RADIUS`, `DERETH_ART_PACK_FIDELITY` and `DERETH_TERRAIN_KNOBS` still
//!   say otherwise where they are set.
//!
//! The places: the south edge of Holtburg at a standing eye's height; the block where the most
//! common tree stands thickest; a block where the boulder is most common; a player's view from
//! behind a body standing in that forest; and five places a player stood at, each a body standing
//! there, seen from its chase camera. The device is the one `DERETH_TEST_RENDERER`
//! names, a fresh one for every frame set; the presentation draws on `wgpu` alone.

#![cfg(all(gpu, feature = "hifi"))]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use dereth_render::device::{DeviceConfig, Gpu};

use super::hifi_stations::{capture, write_png, Clock, Place, Shot, Station, HOLTBURG};

/// The block where the most common tree (`0x02000258`) stands thickest.
const FOREST: u16 = 0x48C6;
/// A block where the boulder (`0x020005C9`) is most common.
const ROCKS: u16 = 0x2A16;

/// Frames a set is stepped before it is captured: enough for the anti-aliasing to have blended
/// three rounds of its jitter. `DERETH_ART_PACK_WARM` asks for another number.
fn warm() -> usize {
    env("DERETH_ART_PACK_WARM")
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(24)
}
/// Frames drawn before timing, and frames timed.
const TIMING_WARM: usize = 30;
const TIMED: usize = 120;

/// The presentation's boxes for the frames drawn with it: every effect but the weather, which
/// changes the overcast frames by itself.
const EFFECTS: &str = "Lighting=1,Shadows=1,GlobalIllumination=1,AmbientOcclusion=1,Lamps=1,Sky=1";

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn root() -> PathBuf {
    env("DERETH_HIFI_CAPTURE_DIR").map_or_else(
        || Path::new(env!("CARGO_TARGET_TMPDIR")).join("art-pack"),
        PathBuf::from,
    )
}

fn size() -> (u32, u32) {
    env("DERETH_HIFI_SIZE")
        .and_then(|s| {
            let (w, h) = s.split_once(['x', 'X'])?;
            Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
        })
        .unwrap_or((1920, 1080))
}

/// Whether the landscape's layers are blended as it is drawn, the client's default: unless
/// `DERETH_ART_PACK_BLENDING=composite` asks for the cells' composites.
fn splat() -> bool {
    !env("DERETH_ART_PACK_BLENDING").is_some_and(|v| v.trim().eq_ignore_ascii_case("composite"))
}

fn listed(var: &str, name: &str) -> bool {
    env(var).is_none_or(|list| {
        list.eq_ignore_ascii_case("all")
            || list.split(',').any(|s| s.trim().eq_ignore_ascii_case(name))
    })
}

/// The places, each at noon; the times are set per frame set.
fn places() -> Vec<Station> {
    let at = |name, why, place| Station {
        name,
        why,
        place,
        clock: Clock::Sunny(0.5),
        weather: false,
        land_radius: env("DERETH_ART_PACK_LAND_RADIUS")
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(3),
    };
    let walk = env("DERETH_ART_PACK_WALK_AT").and_then(|v| {
        let f: Vec<&str> = v.split(',').map(str::trim).collect();
        let num = |i: usize| f.get(i)?.parse::<f32>().ok();
        let name: &'static str = Box::leak(f.first()?.to_string().into_boxed_str());
        Some(at(
            name,
            "a body standing on the ground named for this run",
            Place::Ground {
                block: u16::from_str_radix(f.get(1)?.trim_start_matches("0x"), 16).ok()?,
                x: num(2)?,
                y: num(3)?,
                yaw: num(4)?,
            },
        ))
    });
    let more = env("DERETH_ART_PACK_AT").unwrap_or_default();
    let extra = more
        .split(';')
        .filter(|v| !v.trim().is_empty())
        .filter_map(|v| {
            let f: Vec<&str> = v.split(',').map(str::trim).collect();
            let num = |i: usize| f.get(i)?.parse::<f32>().ok();
            let name: &'static str = Box::leak(f.first()?.to_string().into_boxed_str());
            Some(at(
                name,
                "a place named for this run",
                Place::Free {
                    block: u16::from_str_radix(f.get(1)?.trim_start_matches("0x"), 16).ok()?,
                    x: num(2)?,
                    y: num(3)?,
                    eye: num(4)?,
                    yaw: num(5)?,
                    pitch: num(6)?,
                },
            ))
        });
    let inside = env("DERETH_ART_PACK_IN").unwrap_or_default();
    let rooms = inside
        .split(';')
        .filter(|v| !v.trim().is_empty())
        .filter_map(|v| {
            let f: Vec<&str> = v.split(',').map(str::trim).collect();
            let num = |i: usize| f.get(i)?.parse::<f32>().ok();
            let name: &'static str = Box::leak(f.first()?.to_string().into_boxed_str());
            let origin = match (num(3), num(4), num(5)) {
                (Some(x), Some(y), Some(z)) => dereth_primitives::Vec3::new(x, y, z),
                _ => dereth_primitives::Vec3::ZERO,
            };
            Some(at(
                name,
                "a body standing in an indoor cell named for this run",
                Place::Body {
                    cell: u32::from_str_radix(f.get(1)?.trim_start_matches("0x"), 16).ok()?,
                    origin,
                    yaw: num(2)?,
                },
            ))
        })
        .collect::<Vec<_>>();
    let mut all = vec![
        at(
            "holtburg",
            "the south edge of Holtburg at a standing eye's height, looking along the road out of              town at the trees on the rise beyond it",
            Place::Free {
                block: HOLTBURG,
                x: 110.0,
                y: 20.0,
                eye: 1.8,
                yaw: 2.8,
                pitch: 0.04,
            },
        ),
        at(
            "forest",
            "the block where the most common tree stands thickest, from three metres up",
            Place::Free {
                block: FOREST,
                x: 96.0,
                y: 96.0,
                eye: 3.0,
                yaw: 0.6,
                pitch: 0.02,
            },
        ),
        at(
            "rocks",
            "a block where the boulder is most common, from two metres up, looking down a little",
            Place::Free {
                block: ROCKS,
                x: 96.0,
                y: 96.0,
                eye: 2.0,
                yaw: 0.3,
                pitch: -0.12,
            },
        ),
        at(
            "player",
            "a body standing in the forest block, seen from its chase camera",
            Place::Ground {
                block: FOREST,
                x: 96.0,
                y: 96.0,
                yaw: 35.0,
            },
        ),
    ];
    // Places a player stood at, from where they stood and the turn they faced (twice the angle of
    // the position's quaternion): a body standing there, seen from its chase camera.
    for (name, why, block, x, y, yaw) in [
        (
            "cliff_bands",
            "the plateau's edge, walking toward the cliffs' rock bands",
            0x2780,
            115.372,
            173.655,
            43.8,
        ),
        (
            "holtburg_hill",
            "Holtburg's hill beside a stone house, over the town and the far water",
            0xA9B4,
            73.087,
            45.066,
            -15.2,
        ),
        (
            "woodsman_camp",
            "the woodsman's camp and its row of snowy firs",
            0xA9B5,
            8.857,
            142.057,
            152.2,
        ),
        (
            "wading",
            "wading in the river below Holtburg, its far shore and the trees on the far bank",
            0xA9B5,
            59.952,
            140.634,
            -117.2,
        ),
        (
            "river_bank",
            "the foot of a bank by the river below Holtburg, where the rounded ground dips",
            0xA9B5,
            65.008,
            59.555,
            53.4,
        ),
    ] {
        all.push(at(name, why, Place::Ground { block, x, y, yaw }));
    }
    all.extend(extra);
    all.extend(rooms);
    all.extend(walk);
    all
}

/// The drawn ground's knobs for the frames drawn with the pack, from `DERETH_TERRAIN_KNOBS`.
fn terrain_knobs() -> Option<dereth_terrain::land::drawn::Knobs> {
    let text = env("DERETH_TERRAIN_KNOBS")?.replace(';', "\n");
    let (knobs, notes) = dereth_terrain::land::drawn::Knobs::parse(&text);
    assert!(notes.is_empty(), "DERETH_TERRAIN_KNOBS: {notes:?}");
    Some(knobs)
}

/// Step `shot` once and wait for the world's landscape vertices the drawn ground is drawn from,
/// so it is drawn from the frames that follow; then step it while the blocks bake again for the
/// drawn ground, and poll the preferences as a running client does every frame, which looks for
/// the lamps of the blocks baked again.
fn settle_drawn_ground(shot: &mut Shot, gpu: &mut Gpu) {
    shot.step(gpu);
    shot.scene.wait_for_drawn_ground();
    for _ in 0..8 {
        shot.step(gpu);
    }
    // The art pack's far trees within reach, worked out before the frames are taken.
    let started = Instant::now();
    shot.scene.wait_for_far_trees();
    let (blocks, trees, thread_ms, _) = shot.scene.far_trees_counts();
    eprintln!(
        "far trees: {blocks} blocks, {trees} trees within reach, worked out in {thread_ms:.0} ms on their thread ({:.0} ms waited)",
        started.elapsed().as_secs_f64() * 1000.0
    );
    shot.step(gpu);
    shot.poll(gpu);
}

/// The times of day: noon on a sunny day, noon on the rainy day without its rain (overcast),
/// the evening with the sun five degrees up, and midnight on the sunny day; asked for by name,
/// the same noon with the rain falling (`rain`), the morning with the sun six degrees up
/// (`dawn`), the night before dawn with both moons up (`moons`), and noon on the first clear and
/// the first cloudy day (`clear`, `cloudy`).
const TIMES: [(&str, Clock); 9] = [
    ("day", Clock::Sunny(0.5)),
    ("overcast", Clock::Rainy(0.5)),
    ("dusk", Clock::SunUp(5.0)),
    ("night", Clock::Sunny(0.0)),
    ("rain", Clock::Rainy(0.5)),
    ("dawn", Clock::SunRise(6.0)),
    ("moons", Clock::Sunny(0.1)),
    ("clear", Clock::Group("Clear", 0.5)),
    ("cloudy", Clock::Group("Cloudy", 0.5)),
];

/// The first four of [`TIMES`], drawn when no times are asked for.
const USUAL_TIMES: usize = 4;

/// Whether the time named `time`, `index`th of [`TIMES`], is drawn: one of those asked for, or
/// one of the usual four when none are asked for.
fn time_wanted(index: usize, time: &str) -> bool {
    match env("DERETH_ART_PACK_TIMES") {
        Some(_) => listed("DERETH_ART_PACK_TIMES", time),
        None => index < USUAL_TIMES,
    }
}

/// `station` at `time`: the rain falls at the time named for it.
fn at_time(station: &Station, time: &str, clock: Clock) -> Station {
    Station {
        clock,
        weather: station.weather || time == "rain",
        ..station.clone()
    }
}

/// The three ways a set is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Way {
    /// The ordinary renderer.
    Retail,
    /// The presentation, with the retail look asked for.
    Hifi,
    /// The presentation, with the pack's models.
    Pack,
}

impl Way {
    const ALL: [Self; 3] = [Self::Retail, Self::Hifi, Self::Pack];

    fn name(self) -> &'static str {
        match self {
            Self::Retail => "retail",
            Self::Hifi => "hifi",
            Self::Pack => "pack",
        }
    }
}

fn device(width: u32, height: u32) -> Gpu {
    let cfg = DeviceConfig {
        width,
        height,
        hifi: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new(None, &cfg)
        .unwrap_or_else(|e| panic!("the capture needs a device ({width}x{height}): {e}"));
    if let Some(percent) = env("DERETH_HIFI_RENDER_SCALE").and_then(|v| v.trim().parse().ok()) {
        gpu.hifi_set_render_scale(percent);
    }
    assert_eq!(
        gpu.adapter_kind(),
        dereth_render::device::AdapterKind::Hardware,
        "captures are taken on the hardware adapter, not {}",
        gpu.adapter_name()
    );
    gpu
}

/// What one set's frames cost.
struct Timed {
    /// Milliseconds a frame, wall clock, frames back to back.
    frame_ms: f64,
    /// The processor's milliseconds a frame, a tenth of the way up from the quickest.
    cpu_ms: f64,
    /// Each pass's GPU milliseconds, the median of the timed frames.
    passes: Vec<(&'static str, f32)>,
    /// What the presentation counted on the last frame.
    notes: Vec<(&'static str, u64)>,
}

fn time_frames(shot: &mut Shot, gpu: &mut Gpu) -> Timed {
    use dereth_scene::world_scene::SceneReads as _;
    for _ in 0..TIMING_WARM {
        shot.draw(gpu);
    }
    gpu.wait_idle().expect("the device finishes");
    let mut samples: Vec<(&'static str, Vec<f32>)> = Vec::new();
    let mut cpu: Vec<f64> = Vec::with_capacity(TIMED);
    let started = Instant::now();
    for _ in 0..TIMED {
        let t = Instant::now();
        gpu.begin_frame().expect("begin");
        shot.scene.draw(gpu).expect("draw");
        gpu.end_frame().expect("end");
        cpu.push(t.elapsed().as_secs_f64() * 1000.0);
        for (name, ms) in gpu.hifi_report().map(|r| r.gpu_ms).unwrap_or_default() {
            match samples.iter_mut().find(|(n, _)| *n == name) {
                Some((_, v)) => v.push(ms),
                None => samples.push((name, vec![ms])),
            }
        }
    }
    gpu.wait_idle().expect("the device finishes");
    #[allow(clippy::cast_precision_loss)] // a small frame count
    let frame_ms = started.elapsed().as_secs_f64() * 1000.0 / TIMED as f64;
    cpu.sort_by(f64::total_cmp);
    Timed {
        frame_ms,
        cpu_ms: cpu[cpu.len() / 10],
        passes: samples
            .into_iter()
            .map(|(name, mut v)| {
                v.sort_by(f32::total_cmp);
                (name, v[v.len() / 2])
            })
            .collect(),
        notes: gpu.hifi_report().map(|r| r.notes).unwrap_or_default(),
    }
}

/// The presentation's boxes for a frame drawn `way` (not the ordinary renderer), with the
/// diagnostic view `debug`.
fn fidelity(way: Way, debug: Option<u32>) -> String {
    let mut spec = EFFECTS.to_owned();
    if way == Way::Hifi {
        spec.push_str(",RetailLook=1");
    }
    if let Some(view) = debug {
        let _ = write!(spec, ",Debug={view}");
    }
    if let Some(more) = env("DERETH_ART_PACK_FIDELITY") {
        let _ = write!(spec, ",{more}");
    }
    spec
}

/// `spec` with the weather's box ticked when `station` brings its weather.
fn with_weather(mut spec: String, station: &Station) -> String {
    if station.weather {
        spec.push_str(",Weather=1");
    }
    spec
}

/// One set: `station` drawn `way` on a device of its own, its last frame and what it cost.
fn run(
    store: &Arc<dereth_dat::RetailDatStore>,
    pack: &Arc<dereth_scene::art_pack::ArtPack>,
    (width, height): (u32, u32),
    station: &Station,
    way: Way,
    debug: Option<u32>,
) -> (Vec<u8>, Option<Timed>, usize) {
    use dereth_client_runtime::render_prefs::FidelityPreferences;
    let mut device = device(width, height);
    let gpu = &mut device;
    let mut shot = Shot::open_blending(store, gpu, station, splat());
    if way != Way::Retail {
        shot.scene
            .set_art_pack(store, gpu, Some(Arc::clone(pack)))
            .expect("the blocks bake for the pack");
        shot.scene.draw.cfg.render.fidelity =
            FidelityPreferences::parse_switch(&with_weather(fidelity(way, debug), station))
                .expect("the boxes parse");
        if let Some(k) = terrain_knobs().filter(|_| way == Way::Pack) {
            shot.scene.set_terrain_knobs(k);
        }
    }
    shot.poll(gpu);
    if way == Way::Pack {
        settle_drawn_ground(&mut shot, gpu);
    }
    for _ in 0..warm() {
        shot.step(gpu);
        assert!(
            gpu.hifi_settle(std::time::Duration::from_secs(60)),
            "the presentation's pipelines were not built in a minute"
        );
    }
    shot.step(gpu);
    if let Some(e) = gpu.hifi_failed() {
        panic!(
            "{} {}: the presentation failed: {e}",
            station.name,
            way.name()
        );
    }
    let rgba = capture(gpu);
    if way == Way::Pack {
        let eye = shot.scene.camera.position;
        eprintln!(
            "{} drawn ground under the eye: {}",
            station.name,
            shot.scene.drawn_ground_probe(eye)
        );
    }
    let placements = shot.scene.draw.art_pack_placements();
    let timed = debug.is_none().then(|| time_frames(&mut shot, gpu));
    (rgba, timed, placements)
}

/// Every place at every time, drawn the three ways: `<root>/<place>-<time>-<way>.png`, the pack's
/// normals and sun shadow beside the day's (`-pack-normals`, `-pack-shadow`), and the frame
/// times in `<root>/timing.csv`.
#[test]
#[ignore = "instrument: writes PNGs; run with DERETH_ART_PACK set and --ignored"]
#[allow(clippy::too_many_lines)]
fn capture_an_art_pack_beside_the_retail_look() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let dir = env("DERETH_ART_PACK").expect("DERETH_ART_PACK names the art pack's folder");
    let opened = Instant::now();
    let pack = Arc::new(
        dereth_scene::art_pack::ArtPack::open_dir(Path::new(&dir)).expect("the art pack opens"),
    );
    eprintln!(
        "art pack {:?}: {} models, {} triangles, {} pictures, {} materials, opened in {:.2} s;          problems {:?}",
        pack.name,
        pack.models.len(),
        pack.triangles(),
        pack.images.len(),
        pack.materials.len(),
        opened.elapsed().as_secs_f64(),
        pack.problems
    );
    let (width, height) = size();
    let out = root();
    std::fs::create_dir_all(&out).expect("the capture folder");
    let adapter = device(width, height).adapter_name().to_owned();
    let mut csv = String::from(
        "place,time,way,size,frame_ms,cpu_ms,gpu_ms_total,pack_placements,passes,notes\n",
    );
    for base in places()
        .into_iter()
        .filter(|s| listed("DERETH_ART_PACK_STATIONS", s.name))
    {
        for (index, (time, clock)) in TIMES.into_iter().enumerate() {
            if !time_wanted(index, time) {
                continue;
            }
            let station = at_time(&base, time, clock);
            for way in Way::ALL
                .into_iter()
                .filter(|w| listed("DERETH_ART_PACK_WAYS", w.name()))
            {
                let (rgba, timed, placements) =
                    run(&store, &pack, (width, height), &station, way, None);
                write_png(
                    &out.join(format!("{}-{time}-{}.png", station.name, way.name())),
                    width,
                    height,
                    &rgba,
                );
                if let Some(t) = timed {
                    let total: f32 = t.passes.iter().map(|(_, ms)| ms).sum();
                    let list = |v: &[(&str, f32)]| {
                        v.iter()
                            .map(|(n, ms)| format!("{n}={ms:.3}"))
                            .collect::<Vec<_>>()
                            .join(" ")
                    };
                    let notes = t
                        .notes
                        .iter()
                        .filter(|(n, _)| {
                            n.starts_with("art pack")
                                || n.starts_with("drawn ground")
                                || n.starts_with("anti-aliasing")
                                || n.starts_with("sky")
                                || *n == "re-shaded"
                        })
                        .map(|(n, v)| format!("{n}={v}"))
                        .collect::<Vec<_>>()
                        .join(" ");
                    eprintln!(
                        "{} {time} {}: {:.2} ms a frame, cpu {:.2} ms, gpu {total:.2} ms; {notes}",
                        station.name,
                        way.name(),
                        t.frame_ms,
                        t.cpu_ms
                    );
                    let _ = writeln!(
                        csv,
                        "{},{time},{},{width}x{height},{:.3},{:.3},{total:.3},{placements},\"{}\",\"{notes}\"",
                        station.name,
                        way.name(),
                        t.frame_ms,
                        t.cpu_ms,
                        list(&t.passes)
                    );
                }
            }
            if time == "day" && env("DERETH_ART_PACK_VIEWS").as_deref() != Some("0") {
                for (view, name) in [(3, "normals"), (5, "shadow")] {
                    let (rgba, _, _) = run(
                        &store,
                        &pack,
                        (width, height),
                        &station,
                        Way::Pack,
                        Some(view),
                    );
                    write_png(
                        &out.join(format!("{}-{time}-pack-{name}.png", station.name)),
                        width,
                        height,
                        &rgba,
                    );
                }
            }
            std::fs::write(out.join("timing.csv"), &csv).expect("timing.csv");
        }
    }
    std::fs::write(
        out.join("adapter.txt"),
        format!("{adapter}\n{width}x{height}\n{dir}\n"),
    )
    .expect("adapter.txt");
    eprintln!("wrote {}", out.display());
}

/// The way a report's frame was drawn: the ordinary renderer without Better Lighting, the
/// presentation with the retail look, or the presentation with the art pack.
fn report_way(report: &dereth_client_runtime::report::VisualReport) -> Way {
    match (report.render.better_lighting, report.render.retail_look) {
        (false, _) => Way::Retail,
        (true, true) => Way::Hifi,
        (true, false) => Way::Pack,
    }
}

/// The frame a report describes (`DERETH_REPORT`, the line the client's `@report` prints), drawn
/// as it was drawn there: `<root>/report-<way>.png`. The camera it was drawn from, and the eye
/// where Better Lighting draws the camera moved with the ground, are printed beside the
/// report's, so a frame drawn from another place is seen at once.
#[test]
#[ignore = "instrument: writes PNGs; run with DERETH_REPORT set and --ignored"]
#[allow(clippy::too_many_lines)]
fn capture_the_frame_a_report_describes() {
    use dereth_client_runtime::render_prefs::FidelityPreferences;
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let report = super::hifi_stations::report_from_env()
        .expect("DERETH_REPORT holds the line @report printed, or names a file holding it");
    let (width, height) = if env("DERETH_HIFI_SIZE").is_some() || report.window.0 == 0 {
        size()
    } else {
        report.window
    };
    let station = super::hifi_stations::report_station(
        &report,
        env("DERETH_ART_PACK_LAND_RADIUS").and_then(|v| v.trim().parse().ok()),
    );
    let pack = env("DERETH_ART_PACK").map(|dir| {
        Arc::new(
            dereth_scene::art_pack::ArtPack::open_dir(Path::new(&dir)).expect("the art pack opens"),
        )
    });
    if let (Some(pack), Some((name, hash))) = (pack.as_ref(), report.render.art_pack.as_ref()) {
        if &pack.hash != hash {
            eprintln!(
                "the report's art pack is {name:?} {hash}; this one is {:?} {}: the frame may differ",
                pack.name, pack.hash
            );
        }
    }
    let out = root();
    std::fs::create_dir_all(&out).expect("the capture folder");
    let ways: Vec<Way> = if env("DERETH_ART_PACK_WAYS").is_some() {
        Way::ALL
            .into_iter()
            .filter(|w| listed("DERETH_ART_PACK_WAYS", w.name()))
            .collect()
    } else {
        vec![report_way(&report)]
    };
    for way in ways {
        let mut device = device(width, height);
        let gpu = &mut device;
        if env("DERETH_HIFI_RENDER_SCALE").is_none() && report.render.render_scale > 0 {
            gpu.hifi_set_render_scale(report.render.render_scale);
        }
        let mut shot = Shot::open_blending(&store, gpu, &station, splat());
        shot.apply_report(&report);
        if way != Way::Retail {
            if way == Way::Pack {
                match pack.as_ref() {
                    Some(pack) => shot
                        .scene
                        .set_art_pack(&store, gpu, Some(Arc::clone(pack)))
                        .expect("the blocks bake for the pack"),
                    None => assert!(
                        report.render.art_pack.is_none(),
                        "DERETH_ART_PACK names the art pack the report drew"
                    ),
                }
            }
            let mut spec = if report.render.effects.is_empty() {
                EFFECTS.to_owned()
            } else {
                report.render.effects.clone()
            };
            if way == Way::Hifi && !spec.contains("RetailLook") {
                spec.push_str(",RetailLook=1");
            }
            if way == Way::Pack {
                spec = spec.replace("RetailLook=1", "RetailLook=0");
            }
            if let Some(more) = env("DERETH_ART_PACK_FIDELITY") {
                let _ = write!(spec, ",{more}");
            }
            shot.scene.draw.cfg.render.fidelity =
                FidelityPreferences::parse_switch(&spec).expect("the boxes parse");
            if way == Way::Pack {
                let knobs = terrain_knobs().unwrap_or_else(|| {
                    let (knobs, notes) =
                        dereth_terrain::land::drawn::Knobs::parse(&report.knobs_text());
                    assert!(notes.is_empty(), "the report's knobs: {notes:?}");
                    knobs
                });
                shot.scene.set_terrain_knobs(knobs);
            }
        }
        shot.poll(gpu);
        if way == Way::Pack {
            settle_drawn_ground(&mut shot, gpu);
        }
        for _ in 0..warm() {
            shot.step(gpu);
            assert!(
                gpu.hifi_settle(std::time::Duration::from_secs(60)),
                "the presentation's pipelines were not built in a minute"
            );
        }
        shot.step(gpu);
        if let Some(e) = gpu.hifi_failed() {
            panic!("report {}: the presentation failed: {e}", way.name());
        }
        let rgba = capture(gpu);
        let file = out.join(format!("report-{}.png", way.name()));
        write_png(&file, width, height, &rgba);
        // Both as the report writes them, from the block the world is drawn around.
        let block = shot.scene.viewer_block().unwrap_or_default();
        let drawn = dereth_client_runtime::report::ReportCamera::from_render(
            &shot.scene.camera,
            block,
            None,
        );
        let point = |p: dereth_primitives::Vec3| format!("[{} {} {}]", p.x, p.y, p.z);
        let eye = shot.scene.draw.drawn_eye(&shot.scene.world).origin;
        eprintln!(
            "report {}: camera {:#06X} {} yaw {} pitch {}; the report's {}",
            way.name(),
            drawn.block,
            point(drawn.origin),
            drawn.yaw,
            drawn.pitch,
            report.camera.map_or_else(
                || "none".to_owned(),
                |c| format!(
                    "{:#06X} {} yaw {} pitch {}",
                    c.block,
                    point(c.origin),
                    c.yaw,
                    c.pitch
                )
            ),
        );
        eprintln!(
            "report {}: eye {}; the report's {}; wrote {}",
            way.name(),
            point(eye),
            report.eye.map_or_else(|| "the camera".to_owned(), point),
            file.display()
        );
    }
}

/// How many frames each sequence in motion draws.
fn motion_frames() -> usize {
    env("DERETH_ART_PACK_MOTION_FRAMES")
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(40)
}

/// Short sequences in the forest block, each drawn the three ways and written frame by frame
/// (`<root>/motion/<sequence>-<way>/f<frame>.png`): a free camera sliding sideways and turning a
/// little (`pan`), where the edges of the needles crawl if anything does; the body running
/// forward with its chase camera (`walk`), where a moving figure over a moving background leaves
/// a trail if anything does; and, asked for alone, the free camera standing still (`still`),
/// where nothing should change from frame to frame, or standing still while the clock runs
/// `DERETH_ART_PACK_LAPSE_SECONDS` a frame (default 1; `lapse`), the clouds drifting over it.
#[test]
#[ignore = "instrument: writes PNGs; run with DERETH_ART_PACK set and --ignored"]
fn capture_an_art_pack_in_motion() {
    use dereth_client_runtime::render_prefs::FidelityPreferences;
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let dir = env("DERETH_ART_PACK").expect("DERETH_ART_PACK names the art pack's folder");
    let pack = Arc::new(
        dereth_scene::art_pack::ArtPack::open_dir(Path::new(&dir)).expect("the art pack opens"),
    );
    let (width, height) = size();
    let out = root().join("motion");
    let frames = motion_frames();
    let all = places();
    let find = |name: &str| {
        all.iter()
            .find(|s| s.name == name)
            .cloned()
            .expect("the place is one of the places")
    };
    let asked = env("DERETH_ART_PACK_MOTION").unwrap_or_else(|| "pan,walk".to_owned());
    let lapse = env("DERETH_ART_PACK_LAPSE_SECONDS")
        .and_then(|v| v.trim().parse::<f64>().ok())
        .unwrap_or(1.0);
    for sequence in ["pan", "walk", "still", "dolly", "lapse"] {
        if !asked
            .split(',')
            .any(|s| s.trim().eq_ignore_ascii_case(sequence))
        {
            continue;
        }
        let still_at = env("DERETH_ART_PACK_STILL_AT").unwrap_or_else(|| "forest".to_owned());
        let pan_at = env("DERETH_ART_PACK_PAN_AT").unwrap_or_else(|| "forest".to_owned());
        let walk_at = env("DERETH_ART_PACK_WALK_AT")
            .and_then(|v| v.split(',').next().map(|n| n.trim().to_owned()))
            .unwrap_or_else(|| "player".to_owned());
        let mut station = find(match sequence {
            "walk" => &walk_at,
            "still" | "dolly" | "lapse" => &still_at,
            _ => &pan_at,
        });
        if let Some((_, clock)) = env("DERETH_ART_PACK_MOTION_TIME").and_then(|t| {
            TIMES
                .into_iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(t.trim()))
        }) {
            station.clock = clock;
        }
        for way in Way::ALL
            .into_iter()
            .filter(|w| listed("DERETH_ART_PACK_WAYS", w.name()))
        {
            let mut device = device(width, height);
            let gpu = &mut device;
            let mut shot = Shot::open_blending(&store, gpu, &station, splat());
            if way != Way::Retail {
                shot.scene
                    .set_art_pack(&store, gpu, Some(Arc::clone(&pack)))
                    .expect("the blocks bake for the pack");
                shot.scene.draw.cfg.render.fidelity =
                    FidelityPreferences::parse_switch(&fidelity(way, None))
                        .expect("the boxes parse");
                if let Some(k) = terrain_knobs().filter(|_| way == Way::Pack) {
                    shot.scene.set_terrain_knobs(k);
                }
            }
            shot.poll(gpu);
            if way == Way::Pack {
                settle_drawn_ground(&mut shot, gpu);
            }
            for _ in 0..warm() {
                shot.step(gpu);
                assert!(
                    gpu.hifi_settle(std::time::Duration::from_secs(60)),
                    "the presentation's pipelines were not built in a minute"
                );
            }
            let start = (shot.scene.camera.position, shot.scene.camera.yaw);
            if sequence == "walk" {
                shot.input = dereth_client_runtime::character::CharacterInput {
                    forward: true,
                    run: true,
                    ..Default::default()
                };
            }
            let folder = out.join(format!("{sequence}-{}-{}", station.name, way.name()));
            for k in 0..frames {
                if sequence == "dolly" {
                    // Forward along the heading, three and a half metres a second at thirty
                    // frames a second, at the same height over the ground.
                    #[allow(clippy::cast_precision_loss)] // a short sequence
                    let t = k as f32;
                    shot.walk_to(&station, 0.12 * t);
                }
                if sequence == "pan" {
                    // A metre and a half a second sideways at thirty frames a second, turning a
                    // third of a degree a frame.
                    #[allow(clippy::cast_precision_loss)] // a short sequence
                    let t = k as f32;
                    let (position, yaw) = start;
                    let side = dereth_primitives::Vec3::new(
                        dereth_primitives::num::math::cosf(yaw),
                        dereth_primitives::num::math::sinf(yaw),
                        0.0,
                    );
                    shot.scene.camera.position = dereth_primitives::Vec3::new(
                        position.x + side.x * 0.05 * t,
                        position.y + side.y * 0.05 * t,
                        position.z,
                    );
                    shot.scene.camera.yaw = yaw + 0.006 * t;
                }
                if sequence == "lapse" {
                    shot.step_by(gpu, lapse);
                } else {
                    shot.step(gpu);
                }
                if let Some(e) = gpu.hifi_failed() {
                    panic!("{sequence} {}: the presentation failed: {e}", way.name());
                }
                let rgba = capture(gpu);
                write_png(&folder.join(format!("f{k:03}.png")), width, height, &rgba);
            }
            let notes = gpu
                .hifi_report()
                .map(|r| r.notes)
                .unwrap_or_default()
                .iter()
                .filter(|(n, _)| n.starts_with("anti-aliasing"))
                .map(|(n, v)| format!("{n}={v}"))
                .collect::<Vec<_>>()
                .join(" ");
            eprintln!("{sequence} {}: {frames} frames; {notes}", way.name());
        }
    }
    eprintln!("wrote {}", out.display());
}

/// How many frames a still camera is drawn for its measure.
fn still_frames() -> usize {
    env("DERETH_ART_PACK_STILL_FRAMES")
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(24)
}

/// The pixels of two frames whose color changes by more than `threshold` levels in any channel.
fn changed(a: &[u8], b: &[u8], threshold: u8) -> usize {
    a.as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0)
        .filter(|(p, q)| (0..3).any(|c| p[c].abs_diff(q[c]) > threshold))
        .count()
}

/// Each place drawn from a camera that does not move, the world stepped with its clock held (the
/// sun, the weather and everything the clock animates held; `DERETH_ART_PACK_STILL_CLOCK=running`
/// lets it run), at noon or at the time `DERETH_ART_PACK_STILL_TIME` names (one of the frame
/// writer's times), once settled: `DERETH_ART_PACK_STILL_FRAMES` frames
/// (default 24), the anti-aliasing stepping through its jitter as it does. For each place and way
/// (`DERETH_ART_PACK_WAYS`, default `pack,hifi`), `<root>/still/still.csv` counts the pixels
/// that change from one frame to the next by more than 8 and by more than 32 levels (the most and
/// the mean over the frames), `<place>-<way>-first.png` is the first frame and
/// `<place>-<way>-change.png` shows where it changed (red, four times the largest change of each
/// pixel, over the frame darkened). `DERETH_ART_PACK_STILL_KEEP=1` writes every frame too.
#[test]
#[ignore = "instrument: writes PNGs; run with DERETH_ART_PACK set and --ignored"]
#[allow(clippy::too_many_lines)]
fn measure_a_still_camera() {
    use dereth_client_runtime::render_prefs::FidelityPreferences;
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let dir = env("DERETH_ART_PACK").expect("DERETH_ART_PACK names the art pack's folder");
    let pack = Arc::new(
        dereth_scene::art_pack::ArtPack::open_dir(Path::new(&dir)).expect("the art pack opens"),
    );
    let (width, height) = size();
    let out = root().join("still");
    std::fs::create_dir_all(&out).expect("the capture folder");
    let frames = still_frames().max(2);
    let ways = env("DERETH_ART_PACK_WAYS").unwrap_or_else(|| "pack,hifi".to_owned());
    let mut csv = String::from("place,way,frames,over8_max,over8_mean,over32_max,over32_mean\n");
    let time = env("DERETH_ART_PACK_STILL_TIME").and_then(|t| {
        TIMES
            .into_iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(t.trim()))
    });
    for base in places()
        .into_iter()
        .filter(|s| listed("DERETH_ART_PACK_STATIONS", s.name))
    {
        let station = match time {
            Some((name, clock)) => at_time(&base, name, clock),
            None => base,
        };
        for way in [Way::Pack, Way::Hifi, Way::Retail] {
            if !ways
                .split(',')
                .any(|w| w.trim().eq_ignore_ascii_case(way.name()))
            {
                continue;
            }
            let mut device = device(width, height);
            let gpu = &mut device;
            let mut shot = Shot::open_blending(&store, gpu, &station, splat());
            if way != Way::Retail {
                shot.scene
                    .set_art_pack(&store, gpu, Some(Arc::clone(&pack)))
                    .expect("the blocks bake for the pack");
                shot.scene.draw.cfg.render.fidelity =
                    FidelityPreferences::parse_switch(&fidelity(way, None))
                        .expect("the boxes parse");
                if let Some(k) = terrain_knobs().filter(|_| way == Way::Pack) {
                    shot.scene.set_terrain_knobs(k);
                }
            }
            shot.poll(gpu);
            if way == Way::Pack {
                settle_drawn_ground(&mut shot, gpu);
            }
            for _ in 0..warm() {
                shot.step(gpu);
                assert!(
                    gpu.hifi_settle(std::time::Duration::from_secs(60)),
                    "the presentation's pipelines were not built in a minute"
                );
            }
            // Stepped with the camera where it is, the clock held unless asked to run.
            let running = env("DERETH_ART_PACK_STILL_CLOCK").is_some_and(|v| v == "running");
            let step = |shot: &mut Shot, gpu: &mut Gpu| {
                if running {
                    shot.step(gpu);
                } else {
                    shot.step_held(gpu);
                }
            };
            for _ in 0..16 {
                step(&mut shot, gpu);
            }
            let mut taken: Vec<Vec<u8>> = Vec::with_capacity(frames);
            for _ in 0..frames {
                step(&mut shot, gpu);
                taken.push(capture(gpu));
            }
            let pairs: Vec<(usize, usize)> = taken
                .windows(2)
                .map(|w| (changed(&w[0], &w[1], 8), changed(&w[0], &w[1], 32)))
                .collect();
            let max8 = pairs.iter().map(|p| p.0).max().unwrap_or(0);
            let max32 = pairs.iter().map(|p| p.1).max().unwrap_or(0);
            #[allow(clippy::cast_precision_loss)] // pixel counts
            let mean = |f: &dyn Fn(&(usize, usize)) -> usize| {
                pairs.iter().map(f).sum::<usize>() as f64 / pairs.len().max(1) as f64
            };
            let (mean8, mean32) = (mean(&|p| p.0), mean(&|p| p.1));
            let mut heat = taken[0].clone();
            for px in heat.as_chunks_mut::<4>().0 {
                for c in px.iter_mut().take(3) {
                    *c /= 3;
                }
            }
            for w in taken.windows(2) {
                for (i, (p, q)) in w[0]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .zip(w[1].as_chunks::<4>().0)
                    .enumerate()
                {
                    let d = (0..3).map(|c| p[c].abs_diff(q[c])).max().unwrap_or(0);
                    let v = d.saturating_mul(4);
                    let h = &mut heat[i * 4..i * 4 + 4];
                    if v > h[0] {
                        h[0] = v;
                    }
                }
            }
            let name = format!("{}-{}", station.name, way.name());
            write_png(
                &out.join(format!("{name}-first.png")),
                width,
                height,
                &taken[0],
            );
            write_png(
                &out.join(format!("{name}-change.png")),
                width,
                height,
                &heat,
            );
            if env("DERETH_ART_PACK_STILL_KEEP").is_some() {
                for (k, rgba) in taken.iter().enumerate() {
                    write_png(
                        &out.join(format!("{name}-f{k:02}.png")),
                        width,
                        height,
                        rgba,
                    );
                }
            }
            eprintln!(
                "{name}: {frames} frames still; over 8 levels {max8} at most ({mean8:.0} a frame), over 32 {max32} ({mean32:.0})"
            );
            let _ = writeln!(
                csv,
                "{},{},{frames},{max8},{mean8:.1},{max32},{mean32:.1}",
                station.name,
                way.name()
            );
            std::fs::write(out.join("still.csv"), &csv).expect("still.csv");
        }
    }
    eprintln!("wrote {}", out.display());
}

/// The far trees' upkeep on the processor as the eye travels: from `DERETH_ART_PACK_STILL_AT`'s
/// place (default `forest`), the free camera moved east twelve meters a frame for
/// `DERETH_ART_PACK_MOTION_FRAMES` frames (default 96, six blocks crossed), drawn with the pack.
/// For each frame, the milliseconds the scene spent keeping the far trees, and the microseconds
/// the presentation spent on them and on taking in a changed set, with the trees standing; the
/// most and the mean of each are printed, and every frame's in `<root>/upkeep.csv`.
#[test]
#[ignore = "instrument: measures; run with DERETH_ART_PACK set and --ignored"]
fn measure_far_trees_upkeep() {
    use dereth_client_runtime::render_prefs::FidelityPreferences;
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let dir = env("DERETH_ART_PACK").expect("DERETH_ART_PACK names the art pack's folder");
    let pack = Arc::new(
        dereth_scene::art_pack::ArtPack::open_dir(Path::new(&dir)).expect("the art pack opens"),
    );
    let (width, height) = size();
    let frames = env("DERETH_ART_PACK_MOTION_FRAMES")
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(96usize);
    let at = env("DERETH_ART_PACK_STILL_AT").unwrap_or_else(|| "forest".to_owned());
    let station = places()
        .into_iter()
        .find(|s| s.name == at)
        .expect("the place is one of the places");
    let mut device = device(width, height);
    let gpu = &mut device;
    let mut shot = Shot::open_blending(&store, gpu, &station, splat());
    shot.scene
        .set_art_pack(&store, gpu, Some(Arc::clone(&pack)))
        .expect("the blocks bake for the pack");
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(&fidelity(Way::Pack, None)).expect("the boxes parse");
    if let Some(k) = terrain_knobs() {
        shot.scene.set_terrain_knobs(k);
    }
    shot.poll(gpu);
    settle_drawn_ground(&mut shot, gpu);
    for _ in 0..warm() {
        shot.step(gpu);
    }
    let note = |gpu: &Gpu, name: &str| {
        gpu.hifi_report()
            .map(|r| r.notes)
            .unwrap_or_default()
            .iter()
            .find(|(n, _)| *n == name)
            .map_or(0, |(_, v)| *v)
    };
    let mut csv = String::from("frame,scene_ms,prepare_us,taken_us,standing\n");
    let (mut scene_ms, mut prepare_us, mut taken) = (Vec::new(), Vec::new(), Vec::new());
    let mut last_taken = note(gpu, "art pack far trees taken us");
    for k in 0..frames {
        // The window re-centres on the camera, carrying its position with it: step it from
        // where it stands.
        shot.scene.camera.position.x += 12.0;
        let (_, _, _, before) = shot.scene.far_trees_counts();
        shot.step(gpu);
        let (_, _, _, after) = shot.scene.far_trees_counts();
        let (p, t) = (
            note(gpu, "art pack far trees prepare us"),
            note(gpu, "art pack far trees taken us"),
        );
        let standing = note(gpu, "art pack far trees");
        scene_ms.push(after - before);
        prepare_us.push(p);
        if t != last_taken {
            taken.push(t);
            last_taken = t;
        }
        let _ = writeln!(csv, "{k},{:.3},{p},{t},{standing}", after - before);
    }
    #[allow(clippy::cast_precision_loss)] // frame counts
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
    let max = |v: &[f64]| v.iter().copied().fold(0.0, f64::max);
    #[allow(clippy::cast_precision_loss)] // microseconds
    let us: Vec<f64> = prepare_us.iter().map(|v| *v as f64).collect();
    #[allow(clippy::cast_precision_loss)] // microseconds
    let took: Vec<f64> = taken.iter().map(|v| *v as f64).collect();
    let (blocks, trees, thread_ms, _) = shot.scene.far_trees_counts();
    eprintln!(
        "{at}: {frames} frames, {:.0} m east: the scene's upkeep {:.3} ms a frame ({:.3} at most); the presentation's {:.0} us a frame ({:.0} at most); the set taken in {} times, {:.0} us at most; {blocks} blocks worked out on their thread in {thread_ms:.0} ms; {trees} trees within reach at the end",
        12.0 * frames as f64,
        mean(&scene_ms),
        max(&scene_ms),
        mean(&us),
        max(&us),
        took.len(),
        max(&took),
    );
    let out = root();
    std::fs::create_dir_all(&out).expect("the capture folder");
    std::fs::write(out.join("upkeep.csv"), csv).expect("upkeep.csv");
}
