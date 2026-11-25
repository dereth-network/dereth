//! What creatures cost a frame: rows of one kind of creature standing in view of a camera held
//! still, made as a server makes them, each count of them timed over many frames. Each frame's
//! time on the processor (the whole step, and the skinned bodies' posing in it) and on the device
//! is printed per kind and count, and each frame's row written to a file. It asserts nothing.
//!
//! The device's time is the wait for it to finish the frame once the frame is handed over (every
//! frame is waited for, so frames do not overlap), and in the remaster also the sum of its own
//! passes' timings.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware device.
//!
//! - `DERETH_COST_OVERLAY`: a world's overlay folder laid over the dats, and `DERETH_COST_WORLD`
//!   the world it was written for (default `ac2-creatures`); none by default.
//! - `DERETH_COST_KINDS`: the creatures, a comma list of `name=setup:mtable` (hex), a `:parts`
//!   after it drawing a skinned body's parts instead, and an `@scale` last their size (default a
//!   drudge's, at `DERETH_COST_SCALE`).
//! - `DERETH_COST_COUNTS`: how many stand at once, a comma list (default `0,1,10,50`).
//! - `DERETH_COST_LOOKS`: `classic`, `remaster` or both (default both).
//! - `DERETH_COST_RUNS`: how many times the whole is run, each from a fresh scene (default 3).
//! - `DERETH_COST_FRAMES`, `DERETH_COST_SETTLE`: frames timed, and frames stepped first (default
//!   240 and 60).
//! - `DERETH_COST_SCALE`: their size (default 1); `DERETH_COST_SPACING`: meters between them
//!   (default 6); ten to a row.
//! - `DERETH_COST_AT`: where the first row's middle stands, `block:x,y` (default open ground south
//!   of Holtburg); the camera stands south of the rows, high enough to see them all, or
//!   `DERETH_COST_BACK` meters south of their middle.
//! - `DERETH_COST_CSV`: where each frame's row is written (default none).
//! - `DERETH_COST_SIZE`: `WIDTHxHEIGHT` (default 1280x720).

#![cfg(gpu)]

use std::fmt::Write as _;
use std::sync::Arc;
use std::time::Instant;

use dereth_dat::overlay::OverlayDir;
use dereth_primitives::num::math;
use dereth_primitives::{LandblockId, Vec3};
use dereth_render::device::{DeviceConfig, Gpu};

use super::stations::{Clock, Place, Shot, Station};

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn hex(v: &str) -> u32 {
    u32::from_str_radix(v.trim().trim_start_matches("0x"), 16).expect("a hex id")
}

fn number(name: &str, default: f32) -> f32 {
    env(name).map_or(default, |v| v.trim().parse().expect("a number"))
}

/// One kind of creature: its name, setup and motion table, whether a skinned body is drawn, and
/// its size when it has its own.
struct Kind {
    name: String,
    setup: u32,
    mtable: u32,
    skinned: bool,
    scale: Option<f32>,
}

fn kinds() -> Vec<Kind> {
    env("DERETH_COST_KINDS")
        .unwrap_or_else(|| "drudge=020007DD:09000008".into())
        .split(',')
        .map(|k| {
            let (name, rest) = k.split_once('=').expect("name=setup:mtable");
            let (rest, scale) = match rest.split_once('@') {
                Some((r, s)) => (r, Some(s.trim().parse().expect("a scale"))),
                None => (rest, None),
            };
            let mut fields = rest.split(':');
            let setup = hex(fields.next().expect("a setup"));
            let mtable = hex(fields.next().expect("a motion table"));
            let skinned = fields.next().map(str::trim) != Some("parts");
            Kind {
                name: name.trim().to_owned(),
                setup,
                mtable,
                skinned,
                scale,
            }
        })
        .collect()
}

fn size() -> (u32, u32) {
    env("DERETH_COST_SIZE").map_or((1280, 720), |v| {
        let (w, h) = v.split_once('x').expect("WIDTHxHEIGHT");
        (w.parse().expect("a width"), h.parse().expect("a height"))
    })
}

/// What `values` hold at `share` of the way up (0 to 1).
fn rank(values: &[f64], share: f64) -> f64 {
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    if v.is_empty() {
        return 0.0;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let i = ((v.len() - 1) as f64 * share).round() as usize;
    v[i]
}

fn mean(values: &[f64]) -> f64 {
    #[allow(clippy::cast_precision_loss)] // a few hundred frames
    let n = values.len().max(1) as f64;
    values.iter().sum::<f64>() / n
}

/// One frame's times, milliseconds.
#[derive(Clone, Copy, Default)]
struct Times {
    cpu: f64,
    posing: f64,
    wait: f64,
    device: f64,
}

/// A device for `look`.
fn device(remaster: bool, (w, h): (u32, u32)) -> Gpu {
    if !remaster {
        return crate::common::test_gpu(w, h);
    }
    let cfg = DeviceConfig {
        width: w,
        height: h,
        remaster: true,
        ..DeviceConfig::default()
    };
    Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device")
}

#[cfg(feature = "remaster")]
fn set_remaster(shot: &mut Shot, on: bool, store: &dereth_dat::RetailDatStore, gpu: &mut Gpu) {
    shot.scene
        .draw
        .set_remaster(on, store, gpu)
        .expect("the remaster turns");
    if on {
        shot.scene
            .draw
            .set_remaster_light(dereth_scene::world_scene::RemasterLight::Pixel, gpu);
        shot.step_held(gpu);
        shot.scene.wait_for_remaster_ground();
    }
}

#[cfg(not(feature = "remaster"))]
fn set_remaster(_: &mut Shot, on: bool, _: &dereth_dat::RetailDatStore, _: &mut Gpu) {
    assert!(!on, "the remaster is not built");
}

#[cfg(feature = "remaster")]
fn device_ms(shot: &Shot, gpu: &mut Gpu) -> f64 {
    shot.scene
        .draw
        .remaster_timings(gpu)
        .iter()
        .map(|t| t.1)
        .sum()
}

#[cfg(not(feature = "remaster"))]
fn device_ms(_: &Shot, _: &mut Gpu) -> f64 {
    0.0
}

/// Every kind at every count, in one look, from a fresh scene: each kind's and count's frames.
#[allow(clippy::too_many_lines)] // one run, start to end
fn run(
    store: &Arc<dereth_dat::RetailDatStore>,
    gpu: &mut Gpu,
    remaster: bool,
    kinds: &[Kind],
    counts: &[u32],
) -> Vec<(usize, u32, Vec<Times>)> {
    let at = env("DERETH_COST_AT").unwrap_or_else(|| "A9B3:96,70".into());
    let (block, rest) = at.split_once(':').expect("block:x,y");
    let (x, y) = rest.split_once(',').expect("block:x,y");
    #[allow(clippy::cast_possible_truncation)]
    let block = hex(block) as u16;
    let (cx, cy): (f32, f32) = (
        x.trim().parse().expect("a number"),
        y.trim().parse().expect("a number"),
    );
    let spacing = number("DERETH_COST_SPACING", 6.0);
    let scale = number("DERETH_COST_SCALE", 1.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = number("DERETH_COST_FRAMES", 240.0) as u32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let settle = number("DERETH_COST_SETTLE", 60.0) as u32;
    // The camera south of the rows, looking down on their middle; the rows run north from
    // `(cx, cy)`, five deep at most.
    let rows = 5.0f32;
    let middle = (cx, cy + spacing * (rows - 1.0) * 0.5);
    let back = number("DERETH_COST_BACK", 18.0 + spacing * 6.0);
    let up = 4.0 + spacing * 1.5;
    // The body stands under the camera, so the camera the frame steps with, which follows the
    // body until the held one is put back, is where the held one is.
    let station = Station {
        name: "creature cost",
        why: "creatures standing in view",
        place: Place::Ground {
            block,
            x: middle.0,
            y: middle.1 - back,
            yaw: 0.0,
        },
        clock: Clock::Sunny(0.5),
        weather: false,
        land_radius: 3,
    };
    let mut shot = Shot::open(store, gpu, &station);
    shot.frame_seconds = 1.0 / 60.0;
    shot.poll(gpu);
    if remaster {
        set_remaster(&mut shot, true, store, gpu);
    }
    let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
    let land = dereth_world_data::land_source::DatLandSource::new(Arc::clone(store), &region)
        .expect("the land source opens");
    let ground = |x: f32, y: f32| land.ground_height(LandblockId(block), x, y).unwrap_or(0.0);
    let eye = Vec3::new(
        middle.0,
        middle.1 - back,
        ground(middle.0, middle.1 - back) + up,
    );
    let pitch = -math::atan2f(eye.z - ground(middle.0, middle.1), back);
    shot.hold_camera(eye, 0.0, pitch);
    for _ in 0..30 {
        shot.step(gpu);
    }
    let mut out = Vec::new();
    let mut next_id = 0x8300_6000u32;
    for (k, kind) in kinds.iter().enumerate() {
        shot.scene.draw.set_skinned_bodies(kind.skinned);
        for &count in counts {
            let ids: Vec<u32> = (0..count).map(|i| next_id + i).collect();
            next_id += count.max(1);
            for (i, id) in ids.iter().enumerate() {
                let i = u32::try_from(i).expect("a few");
                #[allow(clippy::cast_precision_loss)]
                let (col, row) = ((i % 10) as f32, (i / 10) as f32);
                let across = count.min(10);
                #[allow(clippy::cast_precision_loss)]
                let left = -(across.saturating_sub(1) as f32) * 0.5;
                let (x, y) = (cx + (left + col) * spacing, cy + row * spacing);
                shot.create_object_scaled(
                    *id,
                    (kind.setup, Some(kind.mtable)),
                    block,
                    (x, y),
                    180.0,
                    kind.scale.unwrap_or(scale),
                );
            }
            for _ in 0..settle {
                shot.step(gpu);
            }
            let mut times = Vec::with_capacity(frames as usize);
            for _ in 0..frames {
                shot.step(gpu);
                let waited = Instant::now();
                gpu.wait_idle().expect("the device finishes");
                let wait = waited.elapsed().as_secs_f64() * 1000.0;
                let l = shot.last;
                times.push(Times {
                    cpu: l.sync + l.update + l.stream + l.draw + l.end,
                    posing: f64::from(shot.scene.draw.skinned_costs().ms),
                    wait,
                    device: if remaster { device_ms(&shot, gpu) } else { 0.0 },
                });
            }
            for id in &ids {
                shot.delete_object(*id);
            }
            for _ in 0..10 {
                shot.step(gpu);
            }
            out.push((k, count, times));
        }
    }
    if remaster {
        set_remaster(&mut shot, false, store, gpu);
    }
    out
}

/// Every kind at every count, in each look, timed; the summary printed and the rows written.
#[test]
#[ignore = "instrument: times frames with creatures standing in view; run with --ignored"]
fn time_frames_with_creatures_in_view() {
    let _gpu = crate::common::gpu_lock();
    let mut store = crate::common::dat_store();
    if let Some(overlay) = env("DERETH_COST_OVERLAY") {
        let world = env("DERETH_COST_WORLD").unwrap_or_else(|| "ac2-creatures".into());
        let dir = OverlayDir::new(std::path::Path::new(&overlay)).expect("the overlay folder");
        store = store
            .with_overlay(&dir, Some(&world))
            .expect("the overlay lays over the dats");
    }
    let store = Arc::new(store);
    let kinds = kinds();
    let counts: Vec<u32> = env("DERETH_COST_COUNTS")
        .unwrap_or_else(|| "0,1,10,50".into())
        .split(',')
        .map(|c| c.trim().parse().expect("a count"))
        .collect();
    let looks: Vec<bool> = env("DERETH_COST_LOOKS")
        .unwrap_or_else(|| "classic,remaster".into())
        .split(',')
        .map(|l| match l.trim() {
            "classic" => false,
            "remaster" => true,
            other => panic!("{other} is not a look"),
        })
        .collect();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let runs = number("DERETH_COST_RUNS", 3.0) as u32;
    let mut csv = String::from("look,run,kind,count,frame,cpu,posing,wait,device\n");
    let mut summary = String::from(
        "look kind count | cpu median per run | cpu p95 | posing mean | wait median | device median\n",
    );
    for remaster in looks {
        let look = if remaster { "remaster" } else { "classic" };
        let mut gpu = device(remaster, size());
        // Each kind's and count's frames from every run.
        let mut all: Vec<Vec<Vec<Times>>> = Vec::new();
        for r in 0..runs {
            let results = run(&store, &mut gpu, remaster, &kinds, &counts);
            for (i, (k, count, times)) in results.into_iter().enumerate() {
                for (f, t) in times.iter().enumerate() {
                    writeln!(
                        csv,
                        "{look},{r},{},{count},{f},{:.3},{:.3},{:.3},{:.3}",
                        kinds[k].name, t.cpu, t.posing, t.wait, t.device
                    )
                    .expect("a row");
                }
                if all.len() <= i {
                    all.push(Vec::new());
                }
                all[i].push(times);
            }
            println!("{look}: run {r} done");
        }
        for (i, per_run) in all.iter().enumerate() {
            let k = i / counts.len();
            let count = counts[i % counts.len()];
            let medians: Vec<String> = per_run
                .iter()
                .map(|t| {
                    let cpu: Vec<f64> = t.iter().map(|t| t.cpu).collect();
                    format!("{:.2}", rank(&cpu, 0.5))
                })
                .collect();
            let every: Vec<Times> = per_run.iter().flatten().copied().collect();
            let of = |f: fn(&Times) -> f64| every.iter().map(f).collect::<Vec<f64>>();
            writeln!(
                summary,
                "{look} {} {count} | {} | {:.2} | {:.3} | {:.2} | {:.2}",
                kinds[k].name,
                medians.join(" "),
                rank(&of(|t| t.cpu), 0.95),
                mean(&of(|t| t.posing)),
                rank(&of(|t| t.wait), 0.5),
                rank(&of(|t| t.device), 0.5),
            )
            .expect("a line");
        }
    }
    println!("{summary}");
    if let Some(path) = env("DERETH_COST_CSV") {
        std::fs::write(&path, csv).expect("the rows are written");
        println!("rows: {path}");
    }
}
