//! Frame costs: where each frame's time goes, on the processor step by step and on the device
//! pass by pass, at fixed places, standing still and travelling a fixed path. It asserts nothing.
//!
//! Each set is one place drawn one way at one size, on a device of its own: the ordinary renderer
//! (`retail`), the presentation with the retail look (`hifi`), or the presentation with the art
//! pack (`pack`). Its frames are stepped as the client steps them (the objects, the simulation,
//! the streaming, the preferences poll, the draw and the end of the frame), each part timed on
//! the processor; the device is then waited for, so the frame's own device time is the wait, and
//! the presentation's passes are timed by its timestamps. A frame's time as the client sees it
//! is the larger of the processor's and the device's: the two overlap with frames in flight.
//!
//! - `DERETH_ART_PACK`: the pack's folder (for `hifi` and `pack`).
//! - `DERETH_HIFI_CAPTURE_DIR`: where the files go (default `perf/` in cargo's scratch folder).
//! - `DERETH_PERF_PLACES`: a comma list of the places below (default all).
//! - `DERETH_PERF_WAYS`: a comma list of `retail`, `hifi` and `pack` (default all three).
//! - `DERETH_PERF_MOTION`: a comma list of `still` and `walk` (default both).
//! - `DERETH_PERF_STILL_FRAMES`, `DERETH_PERF_WALK_FRAMES`: frames measured (default 180, 600).
//! - `DERETH_PERF_SPEED`: metres a frame along a walk (default 0.4, a run at thirty frames a
//!   second).
//! - `DERETH_PERF_TIME`: `day`, `overcast`, `dusk` or `night` (default `day`).
//! - `DERETH_PERF_TAG`: a word naming the run in the files (default `base`).
//! - `DERETH_HIFI_SIZE`: `WxH` (default `1920x1080`).
//! - `DERETH_HIFI_RENDER_SCALE`: the share of the frame's pixels the presentation draws its world
//!   at, each way, percent (default 100).
//! - `DERETH_ART_PACK_FIDELITY`: more of the presentation's boxes, `Name=value` a comma between.
//! - `DERETH_TERRAIN_KNOBS`: the drawn ground's knobs, the tuning file's lines with `;` between.
//! - `DERETH_ART_PACK_LAND_RADIUS`: the landscape's blocks each way (default 8, the client's).
//! - `DERETH_HIFI_STREAM_BUDGET_MS`, `DERETH_HIFI_PARTICLES`: as for every station.
//! - `DERETH_REPORT`: the line the client's `@report` printed, or the name of a file holding it:
//!   one more place, `report`, stood as the report says (the body, the camera held where it
//!   stood, the time, the weather, the field of view, the draw distance), drawn with the report's
//!   Better Lighting effects; walking, the body runs and its chase camera follows.
//!
//! Files: `<root>/frames/<tag>-<place>-<way>-<size>-<motion>.csv`, a row a frame, and
//! `<root>/summary-<tag>.csv`, a row a set.

#![cfg(all(gpu, feature = "hifi"))]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use dereth_primitives::num::math;
use dereth_primitives::{LandblockId, Vec3};
use dereth_render::device::{DeviceConfig, Gpu};

use super::hifi_stations::{Clock, Place, Shot, Station, HOLTBURG};

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn env_num<T: std::str::FromStr>(name: &str, default: T) -> T {
    env(name)
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

fn listed(var: &str, name: &str) -> bool {
    env(var).is_none_or(|list| {
        list.eq_ignore_ascii_case("all")
            || list.split(',').any(|s| s.trim().eq_ignore_ascii_case(name))
    })
}

fn size() -> (u32, u32) {
    env("DERETH_HIFI_SIZE")
        .and_then(|s| {
            let (w, h) = s.split_once(['x', 'X'])?;
            Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
        })
        .unwrap_or((1920, 1080))
}

/// A place measured: where the viewer stands, which way a walk goes, and what stands there.
#[derive(Clone)]
struct PerfPlace {
    name: &'static str,
    place: Place,
    /// For a free camera, the heading a walk takes (radians, as the camera's yaw).
    walk_yaw: f32,
    /// A crowd made to stand in front of the camera: how many.
    crowd: u32,
    /// The report the place was stood from, for `report`.
    report: Option<dereth_client_runtime::report::VisualReport>,
}

/// The places: Holtburg's south edge looking into town; the block where the most common tree
/// stands thickest; the Sho jungle from a little above its canopy; a dungeon's corridors (a body
/// in its cell); the lake beside Holtburg; Neydisa's crater rim looking north; and a crowd of
/// sixty people in Holtburg.
fn places() -> Vec<PerfPlace> {
    let free = |block, x, y, eye, yaw, pitch| Place::Free {
        block,
        x,
        y,
        eye,
        yaw,
        pitch,
    };
    vec![
        PerfPlace {
            name: "holtburg",
            place: free(HOLTBURG, 110.0, 20.0, 1.8, -0.34, 0.0),
            walk_yaw: -0.34,
            crowd: 0,
            report: None,
        },
        PerfPlace {
            name: "forest",
            place: free(0x48C6, 96.0, 96.0, 3.0, 0.6, 0.02),
            walk_yaw: 0.6,
            crowd: 0,
            report: None,
        },
        PerfPlace {
            name: "sho",
            place: free(0xFB7E, 48.0, 48.0, 12.0, 0.785, -0.04),
            walk_yaw: 0.785,
            crowd: 0,
            report: None,
        },
        PerfPlace {
            name: "dungeon",
            place: Place::Body {
                cell: 0x00B0_0647,
                origin: Vec3::new(0.0, -20.0, 18.0),
                yaw: 0.0,
            },
            walk_yaw: 0.0,
            crowd: 0,
            report: None,
        },
        PerfPlace {
            name: "lake",
            place: free(0xA8B4, 84.0, 132.0, 3.5, -0.35, -0.06),
            walk_yaw: -0.35,
            crowd: 0,
            report: None,
        },
        PerfPlace {
            name: "neydisa",
            place: free(0x92D1, 83.0, 12.0, 6.0, -2.79, -0.122),
            walk_yaw: -2.79,
            crowd: 0,
            report: None,
        },
        PerfPlace {
            name: "crowd",
            place: free(HOLTBURG, 96.0, 60.0, 1.8, 0.0, -0.05),
            walk_yaw: 0.0,
            crowd: 60,
            report: None,
        },
    ]
    .into_iter()
    .chain(super::hifi_stations::report_from_env().map(|report| {
        let station = super::hifi_stations::report_station(&report, None);
        PerfPlace {
            name: "report",
            place: station.place,
            walk_yaw: report.camera.map_or(0.0, |c| c.yaw),
            crowd: 0,
            report: Some(report),
        }
    }))
    .collect()
}

/// The three ways a set is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Way {
    Retail,
    Hifi,
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

/// The presentation's boxes for a frame drawn `way`: every effect but the weather, or the
/// effects `report` names.
fn fidelity(way: Way, report: Option<&dereth_client_runtime::report::VisualReport>) -> String {
    let mut spec = report
        .map(|r| r.render.effects.replace("RetailLook=1", "RetailLook=0"))
        .filter(|e| !e.is_empty())
        .unwrap_or_else(|| {
            "Lighting=1,Shadows=1,GlobalIllumination=1,AmbientOcclusion=1,Lamps=1,Sky=1".to_owned()
        });
    if way == Way::Hifi {
        spec.push_str(",RetailLook=1");
    }
    if let Some(more) = env("DERETH_ART_PACK_FIDELITY") {
        let _ = write!(spec, ",{more}");
    }
    spec
}

fn clock() -> Clock {
    match env("DERETH_PERF_TIME").as_deref().map(str::trim) {
        Some("overcast") => Clock::Rainy(0.5),
        Some("dusk") => Clock::SunUp(5.0),
        Some("night") => Clock::Sunny(0.0),
        _ => Clock::Sunny(0.5),
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
        .unwrap_or_else(|e| panic!("the measure needs a device ({width}x{height}): {e}"));
    gpu.hifi_set_render_scale(env_num("DERETH_HIFI_RENDER_SCALE", 100));
    assert_eq!(
        gpu.adapter_kind(),
        dereth_render::device::AdapterKind::Hardware,
        "frame costs are measured on the hardware adapter, not {}",
        gpu.adapter_name()
    );
    gpu
}

/// `list` as `name=value` pairs, a space between.
fn pairs(list: &[(&str, f32)]) -> String {
    list.iter()
        .map(|(n, v)| format!("{}={v:.3}", n.replace(' ', "_")))
        .collect::<Vec<_>>()
        .join(" ")
}

/// One frame's costs, milliseconds.
#[derive(Debug, Clone, Default)]
struct Row {
    sync: f64,
    update: f64,
    stream: f64,
    poll: f64,
    reserve: f64,
    draw: f64,
    walk: f64,
    snapshot: f64,
    drawn_ground: f64,
    end: f64,
    sidecar: f64,
    /// The processor's whole frame.
    cpu: f64,
    /// The device's: the wait for it once the frame is handed over.
    gpu: f64,
    /// The presentation's passes on the device, by their timestamps: filled in a frame later.
    gpu_total: f64,
    gpu_passes: String,
    steps: String,
    prepare: String,
    encode: String,
    blocks: usize,
    /// The draws the frame recorded.
    draws: u64,
}

const HEADER: &str = "frame,sync,update,stream,poll,reserve,draw,walk,snapshot,drawn_ground,end,sidecar,cpu,gpu,frame_ms,gpu_total,blocks,draws,steps,prepare,encode,gpu_passes\n";

impl Row {
    fn frame_ms(&self) -> f64 {
        self.cpu.max(self.gpu)
    }

    fn line(&self, k: usize) -> String {
        format!(
            "{k},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{},{},\"{}\",\"{}\",\"{}\",\"{}\"\n",
            self.sync,
            self.update,
            self.stream,
            self.poll,
            self.reserve,
            self.draw,
            self.walk,
            self.snapshot,
            self.drawn_ground,
            self.end,
            self.sidecar,
            self.cpu,
            self.gpu,
            self.frame_ms(),
            self.gpu_total,
            self.blocks,
            self.draws,
            self.steps,
            self.prepare,
            self.encode,
            self.gpu_passes,
        )
    }
}

/// The free camera's path: where it stands in the world (meters east and north of the world's
/// corner), and the landscape to keep it over.
struct Route {
    land: dereth_world_data::land_source::DatLandSource,
    global: (f32, f32),
    eye: f32,
    yaw: f32,
}

impl Route {
    fn new(store: &Arc<dereth_dat::RetailDatStore>, place: &PerfPlace) -> Option<Self> {
        let Place::Free {
            block, x, y, eye, ..
        } = place.place
        else {
            return None;
        };
        let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
        let land = dereth_world_data::land_source::DatLandSource::new(Arc::clone(store), &region)
            .expect("the land source opens");
        let (bx, by) = (f32::from(block >> 8), f32::from(block & 0xFF));
        Some(Self {
            land,
            global: (bx * 192.0 + x, by * 192.0 + y),
            eye,
            yaw: place.walk_yaw,
        })
    }

    /// Move `shot`'s camera `metres` along the path, at its height over the ground.
    fn advance(&mut self, shot: &mut Shot, metres: f32) {
        let (dx, dy) = (
            -math::sinf(self.yaw) * metres,
            math::cosf(self.yaw) * metres,
        );
        self.global.0 += dx;
        self.global.1 += dy;
        let (bx, by) = (
            (self.global.0 / 192.0).floor(),
            (self.global.1 / 192.0).floor(),
        );
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: a block inside the map.
        let block = ((bx as u16) << 8) | (by as u16);
        let ground = self
            .land
            .ground_height(
                LandblockId(block),
                self.global.0 - bx * 192.0,
                self.global.1 - by * 192.0,
            )
            .unwrap_or(0.0);
        let camera = &mut shot.scene.camera;
        camera.position = Vec3::new(
            camera.position.x + dx,
            camera.position.y + dy,
            ground + self.eye,
        );
        camera.yaw = self.yaw;
    }
}

/// One set: `place` drawn `way` on a device of its own, `motion` measured, its rows.
#[allow(clippy::too_many_lines)]
fn run(
    store: &Arc<dereth_dat::RetailDatStore>,
    pack: Option<&Arc<dereth_scene::art_pack::ArtPack>>,
    (width, height): (u32, u32),
    place: &PerfPlace,
    way: Way,
    motion: &str,
) -> Vec<Row> {
    use dereth_client_runtime::render_prefs::FidelityPreferences;
    let station = match &place.report {
        Some(report) => super::hifi_stations::report_station(
            report,
            env("DERETH_ART_PACK_LAND_RADIUS").and_then(|v| v.trim().parse().ok()),
        ),
        None => Station {
            name: place.name,
            why: "a place measured",
            place: place.place,
            clock: clock(),
            weather: false,
            land_radius: env_num("DERETH_ART_PACK_LAND_RADIUS", 8),
        },
    };
    let mut device = device(width, height);
    let gpu = &mut device;
    let mut shot = Shot::open_blending(store, gpu, &station, true);
    if let Some(report) = &place.report {
        shot.apply_report(report);
    }
    if way != Way::Retail {
        let pack = pack.expect("DERETH_ART_PACK names the art pack's folder");
        shot.scene
            .set_art_pack(store, gpu, Some(Arc::clone(pack)))
            .expect("the blocks bake for the pack");
        shot.scene.draw.cfg.render.fidelity =
            FidelityPreferences::parse_switch(&fidelity(way, place.report.as_ref()))
                .expect("the boxes parse");
        if way == Way::Pack {
            if let Some(text) = env("DERETH_TERRAIN_KNOBS") {
                let (knobs, notes) =
                    dereth_terrain::land::drawn::Knobs::parse(&text.replace(';', "\n"));
                assert!(notes.is_empty(), "DERETH_TERRAIN_KNOBS: {notes:?}");
                shot.scene.set_terrain_knobs(knobs);
            }
        }
    }
    shot.poll(gpu);
    if place.crowd > 0 {
        if let Place::Free { block, x, y, .. } = place.place {
            // Rows of ten, three metres apart, from eight metres ahead, facing the camera.
            for i in 0..place.crowd {
                #[allow(clippy::cast_precision_loss)] // a small crowd
                let (row, col) = ((i / 10) as f32, (i % 10) as f32);
                shot.create_object(
                    0x8300_1000 + i,
                    (0x0200_0001, Some(0x0900_0001)),
                    block,
                    (x - 13.5 + col * 3.0, y + 8.0 + row * 3.0),
                    180.0,
                );
            }
        }
    }
    if way == Way::Pack {
        shot.step(gpu);
        shot.scene.wait_for_drawn_ground();
        for _ in 0..8 {
            shot.step(gpu);
        }
        shot.scene.wait_for_far_trees();
        shot.step(gpu);
        shot.poll(gpu);
    }
    for _ in 0..30 {
        shot.step(gpu);
        shot.poll(gpu);
        assert!(
            gpu.hifi_settle(std::time::Duration::from_secs(60)),
            "the presentation's pipelines were not built in a minute"
        );
    }
    gpu.wait_idle().expect("the device finishes");
    if let Some(e) = gpu.hifi_failed() {
        panic!(
            "{} {}: the presentation failed: {e}",
            place.name,
            way.name()
        );
    }
    let walking = motion == "walk";
    let frames = if walking {
        env_num("DERETH_PERF_WALK_FRAMES", 600usize)
    } else {
        env_num("DERETH_PERF_STILL_FRAMES", 180usize)
    };
    let speed: f32 = env_num("DERETH_PERF_SPEED", 0.4);
    let mut path = if walking {
        Route::new(store, place)
    } else {
        None
    };
    if walking && path.is_none() {
        // A body walks: running forward, turning now and then, its chase camera following.
        shot.held = None;
        shot.input = dereth_client_runtime::character::CharacterInput {
            forward: true,
            run: true,
            ..Default::default()
        };
    }
    let mut rows: Vec<Row> = Vec::with_capacity(frames);
    for k in 0..frames {
        if let Some(p) = path.as_mut() {
            p.advance(&mut shot, speed);
        } else if walking {
            // Turn right for a second in every four.
            shot.input.turn_right = k % 120 >= 90;
        }
        let calls = gpu.draw_calls();
        let started = Instant::now();
        shot.step(gpu);
        let t = Instant::now();
        shot.poll(gpu);
        let poll = t.elapsed().as_secs_f64() * 1000.0;
        let cpu = started.elapsed().as_secs_f64() * 1000.0;
        let t = Instant::now();
        gpu.wait_idle().expect("the device finishes");
        let waited = t.elapsed().as_secs_f64() * 1000.0;
        let costs = shot.scene.draw.frame_costs();
        let report = gpu.hifi_report().unwrap_or_default();
        let last = shot.last;
        let row = Row {
            sync: last.sync,
            update: last.update,
            stream: last.stream,
            poll,
            reserve: last.reserve,
            draw: last.draw,
            walk: f64::from(costs.walk),
            snapshot: f64::from(costs.snapshot),
            drawn_ground: f64::from(costs.drawn_ground),
            end: last.end,
            sidecar: f64::from(report.cpu_ms),
            cpu,
            gpu: waited,
            steps: pairs(&report.cpu_steps),
            prepare: pairs(&report.cpu_prepare),
            encode: pairs(&report.cpu_encode),
            blocks: shot.scene.draw.stats.blocks_meshed,
            draws: gpu.draw_calls() - calls,
            ..Row::default()
        };
        // The timestamps read in this frame are the last frame's.
        if let Some(prev) = rows.last_mut() {
            prev.gpu_total = report.gpu_ms.iter().map(|(_, ms)| f64::from(*ms)).sum();
            prev.gpu_passes = pairs(&report.gpu_ms);
        }
        rows.push(row);
        if let Some(e) = gpu.hifi_failed() {
            panic!(
                "{} {}: the presentation failed: {e}",
                place.name,
                way.name()
            );
        }
    }
    rows
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    // LINT-OK: an index into a short list.
    let i = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[i.min(sorted.len() - 1)]
}

fn stats(rows: &[Row], f: impl Fn(&Row) -> f64) -> (f64, f64, f64) {
    let mut v: Vec<f64> = rows.iter().map(f).collect();
    v.sort_by(f64::total_cmp);
    (
        percentile(&v, 0.5),
        percentile(&v, 0.95),
        v.last().copied().unwrap_or(0.0),
    )
}

fn root() -> PathBuf {
    env("DERETH_HIFI_CAPTURE_DIR").map_or_else(
        || Path::new(env!("CARGO_TARGET_TMPDIR")).join("perf"),
        PathBuf::from,
    )
}

/// Every place drawn every way at one size, still and walking: the frames' costs.
#[test]
#[ignore = "instrument: measures; run with DERETH_ART_PACK set and --ignored"]
#[allow(clippy::too_many_lines)]
fn measure_frame_costs() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let pack = env("DERETH_ART_PACK").map(|dir| {
        Arc::new(
            dereth_scene::art_pack::ArtPack::open_dir(Path::new(&dir)).expect("the art pack opens"),
        )
    });
    let (width, height) = size();
    let out = root();
    std::fs::create_dir_all(out.join("frames")).expect("the measure's folder");
    let tag = env("DERETH_PERF_TAG").unwrap_or_else(|| "base".to_owned());
    let adapter = device(width, height).adapter_name().to_owned();
    let summary = out.join(format!("summary-{tag}.csv"));
    let mut csv = std::fs::read_to_string(&summary).unwrap_or_else(|_| {
        "tag,place,way,size,motion,frames,cpu_med,cpu_p95,cpu_max,gpu_med,gpu_p95,gpu_max,frame_med,frame_p95,frame_max,hitches,sync_med,update_med,stream_med,stream_max,poll_med,draw_med,walk_med,snapshot_med,snapshot_max,drawn_ground_med,drawn_ground_max,end_med,end_max,sidecar_med,gpu_total_med,adapter\n".to_owned()
    });
    for place in places()
        .into_iter()
        .filter(|p| listed("DERETH_PERF_PLACES", p.name))
    {
        for way in Way::ALL
            .into_iter()
            .filter(|w| listed("DERETH_PERF_WAYS", w.name()))
        {
            for motion in ["still", "walk"] {
                if !listed("DERETH_PERF_MOTION", motion) {
                    continue;
                }
                let rows = run(&store, pack.as_ref(), (width, height), &place, way, motion);
                let name = format!(
                    "{tag}-{}-{}-{width}x{height}-{motion}",
                    place.name,
                    way.name()
                );
                let mut frames = String::from(HEADER);
                for (k, r) in rows.iter().enumerate() {
                    frames.push_str(&r.line(k));
                }
                std::fs::write(out.join("frames").join(format!("{name}.csv")), frames)
                    .expect("the frames' file");
                let cpu = stats(&rows, |r| r.cpu);
                let gpu = stats(&rows, |r| r.gpu);
                let frame = stats(&rows, Row::frame_ms);
                let hitches = rows.iter().filter(|r| r.frame_ms() > 2.0 * frame.0).count();
                let med = |f: fn(&Row) -> f64| stats(&rows, f).0;
                let max = |f: fn(&Row) -> f64| stats(&rows, f).2;
                let _ = writeln!(
                    csv,
                    "{tag},{},{},{width}x{height},{motion},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{hitches},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{adapter}",
                    place.name,
                    way.name(),
                    rows.len(),
                    cpu.0,
                    cpu.1,
                    cpu.2,
                    gpu.0,
                    gpu.1,
                    gpu.2,
                    frame.0,
                    frame.1,
                    frame.2,
                    med(|r| r.sync),
                    med(|r| r.update),
                    med(|r| r.stream),
                    max(|r| r.stream),
                    med(|r| r.poll),
                    med(|r| r.draw),
                    med(|r| r.walk),
                    med(|r| r.snapshot),
                    max(|r| r.snapshot),
                    med(|r| r.drawn_ground),
                    max(|r| r.drawn_ground),
                    med(|r| r.end),
                    max(|r| r.end),
                    med(|r| r.sidecar),
                    med(|r| r.gpu_total),
                );
                std::fs::write(&summary, &csv).expect("the summary");
                eprintln!(
                    "{name}: cpu {:.2} ms (p95 {:.2}, max {:.2}), gpu {:.2} ms (p95 {:.2}), frame {:.2} ms (p95 {:.2}, max {:.2}), {hitches} hitches",
                    cpu.0, cpu.1, cpu.2, gpu.0, gpu.1, frame.0, frame.1, frame.2
                );
            }
        }
    }
    eprintln!("wrote {}", out.display());
}
