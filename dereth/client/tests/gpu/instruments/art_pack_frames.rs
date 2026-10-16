//! Frame writers for an art pack in the high-fidelity presentation: each place and time drawn
//! three ways at one camera -- the ordinary renderer, the presentation with the retail look, and
//! the presentation with the pack's models -- written as PNGs with the frame times. They assert
//! nothing.
//!
//! - `DERETH_ART_PACK`: the pack's folder (required).
//! - `DERETH_HIFI_CAPTURE_DIR`: where the files go (default: `art-pack/` in cargo's scratch folder
//!   for integration tests).
//! - `DERETH_ART_PACK_STATIONS`: a comma list of the places to draw, or `all` (the default).
//! - `DERETH_ART_PACK_TIMES`: a comma list of `day`, `overcast` and `dusk` (default all three).
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
//! - `DERETH_ART_PACK_BLENDING`: `composite` to draw the landscape from its cells' composites;
//!   otherwise its layers are blended as it is drawn, the client's own default.
//! - `DERETH_ART_PACK_FIDELITY`: more of the presentation's boxes, `Name=value` a comma between,
//!   for every frame drawn with it (`NoAntiAliasing=1`, say).
//! - `DERETH_ART_PACK_MOTION`: for the frames in motion, a comma list of `pan`, `walk` and
//!   `still` (default `pan,walk`); `DERETH_ART_PACK_MOTION_FRAMES`, how many frames each draws
//!   (default 40); `DERETH_ART_PACK_STILL_AT`, the place `still` stands at (default `forest`).
//! - `DERETH_ART_PACK_IN`: `<name>,<cell hex>,<facing>[,<x>,<y>,<z>]`, more places, a `;`
//!   between them: each a body standing in an indoor cell (a room or a dungeon's), facing that
//!   many degrees, at the block-local point when one is named (a standable point of the cell
//!   otherwise), seen from its chase camera.
//!
//! The places: the south edge of Holtburg at a standing eye's height; the block where the most
//! common tree stands thickest; a block where the boulder is most common; and a player's view
//! from behind a body standing in that forest. The device is the one `DERETH_TEST_RENDERER`
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
        land_radius: 3,
    };
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
    all.extend(extra);
    all.extend(rooms);
    all
}

/// The times of day: noon on a sunny day, noon on the rainy day without its rain (overcast),
/// and the evening with the sun five degrees up.
const TIMES: [(&str, Clock); 3] = [
    ("day", Clock::Sunny(0.5)),
    ("overcast", Clock::Rainy(0.5)),
    ("dusk", Clock::SunUp(5.0)),
];

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
            FidelityPreferences::parse_switch(&fidelity(way, debug)).expect("the boxes parse");
    }
    shot.poll(gpu);
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
        for (time, clock) in TIMES {
            if !listed("DERETH_ART_PACK_TIMES", time) {
                continue;
            }
            let station = Station {
                clock,
                ..base.clone()
            };
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
                                || n.starts_with("anti-aliasing")
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
/// where nothing should change from frame to frame.
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
    for sequence in ["pan", "walk", "still"] {
        if !asked
            .split(',')
            .any(|s| s.trim().eq_ignore_ascii_case(sequence))
        {
            continue;
        }
        let still_at = env("DERETH_ART_PACK_STILL_AT").unwrap_or_else(|| "forest".to_owned());
        let station = find(match sequence {
            "walk" => "player",
            "still" => &still_at,
            _ => "forest",
        });
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
            }
            shot.poll(gpu);
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
            let folder = out.join(format!("{sequence}-{}", way.name()));
            for k in 0..frames {
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
                shot.step(gpu);
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
