//! What the scenery radius costs where a report stands: for each radius, the scene loaded with
//! the scenery grown that many rings of blocks round the viewer's, and printed for it the load's
//! time, the frame's time on the processor and on the device, the process's memory and the
//! device memory it holds, what the scene holds, and the time the step across a block line takes
//! to build the blocks it brings; with a still where the report stands.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware device.
//!
//! - `DERETH_ART_PACK`: the art packs drawn, joined by `;`, bottom first (none without it).
//! - `DERETH_REMASTER_CAPTURE_DIR`: where the stills go, `radius-<r>-<tag>.png`, with a row a
//!   radius appended to `radius-<tag>.tsv` (none are written without it).
//! - `DERETH_RADIUS_PLACE`: a report line saying where the body and the camera stand (default
//!   the hill the block-line crossing instrument stands on).
//! - `DERETH_RADIUS_LIST`: the radii, a comma list (default `1,2,3,4`). The memory read is the
//!   process's own, so a radius a process gives the truest numbers.
//! - `DERETH_RADIUS_CROSS`: `from,to`, meters along the body's heading the crossing is timed
//!   between, a meter a step (default `-1,1`).
//! - `DERETH_REMASTER_LOOK_BOXES`, `DERETH_REMASTER_LOOK_TAG`, `DERETH_REMASTER_SIZE` as the
//!   look's instruments take them; the art pack's box is set over them where a pack is drawn.

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::io::Write as _;
use std::sync::Arc;
use std::time::Instant;

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_client_runtime::report::VisualReport;
use dereth_dat::RetailDatStore;
use dereth_primitives::num::math;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::art_pack::ArtPack;

use super::remaster_look_frames::EVERY_BOX;
use super::stations::{self, capture, report_station, Place, Shot};

/// A rainy hill below a band of pines, the body a quarter of a meter east of a block line and
/// facing west of north across it, the camera behind it.
const HILL: &str = "dereth report 1; loc 0xB3B50006 [0.240147 121.806526 98.005005] -0.936959 0 \
                    0 -0.349440; camera 0xB3B5 [4.164169 117.277565 99.20517] yaw -5.569238 \
                    pitch 0.05000011 cell 0xB3B50005; time 0.523; weather rain";

/// The frames drawn before anything is timed: the pack's models in, the ground's offsets worked
/// out, the upscaler's history settled.
const SETTLE: usize = 48;
/// The frames timed.
const TIMED: usize = 64;

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// The process's working set, MiB, and the device memory it holds, MiB, as the system's counters
/// report them (0 where a counter reads nothing).
fn memory() -> (f64, f64) {
    let pid = std::process::id();
    let script = format!(
        "$p = Get-Process -Id {pid}; $d = 0; \
         try {{ $d = ((Get-Counter -Counter '\\GPU Process Memory(pid_{pid}_*)\\Dedicated Usage' \
         -ErrorAction Stop).CounterSamples | Measure-Object -Property CookedValue -Sum).Sum }} \
         catch {{ }}; \"$($p.WorkingSet64) $d\""
    );
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    let v: Vec<f64> = out
        .split_whitespace()
        .filter_map(|s| s.parse::<f64>().ok())
        .collect();
    let mib = |b: f64| b / (1024.0 * 1024.0);
    (
        v.first().copied().map_or(0.0, mib),
        v.get(1).copied().map_or(0.0, mib),
    )
}

/// The middle of `v`, and its least: what a frame takes with nothing else in its way.
fn median_least(v: &mut [f64]) -> (f64, f64) {
    if v.is_empty() {
        return (0.0, 0.0);
    }
    v.sort_by(f64::total_cmp);
    (v[v.len() / 2], v[0])
}

/// Where the body stands `metres` along its heading from where the report stands it: block-local
/// meters of the report's block (past its edge in the block there), and the ground under it.
fn along(
    store: &Arc<RetailDatStore>,
    (block, x, y, yaw): (u16, f32, f32, f32),
    metres: f32,
) -> (f32, f32, f32) {
    let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
    let land = dereth_world_data::land_source::DatLandSource::new(Arc::clone(store), &region)
        .expect("the land source opens");
    let (x, y) = (x - math::sinf(yaw) * metres, y + math::cosf(yaw) * metres);
    let (bx, by) = ((x / 192.0).floor(), (y / 192.0).floor());
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: a walk of a few blocks at most, inside the map
    let next =
        ((i32::from(block >> 8) + bx as i32) << 8 | (i32::from(block & 0xFF) + by as i32)) as u16;
    let ground = land
        .ground_height(
            dereth_primitives::LandblockId(next),
            x - bx * 192.0,
            y - by * 192.0,
        )
        .unwrap_or(0.0);
    (x, y, ground)
}

/// What one radius cost.
struct Measured {
    radius: u32,
    line: String,
    rgba: Vec<u8>,
}

/// The report's place opened at `radius` on a device of `size`, with `pack` and `boxes`: timed,
/// measured, walked across the block line and drawn once more where it stood.
fn measure(
    store: &Arc<RetailDatStore>,
    report: &VisualReport,
    pack: Option<&Arc<ArtPack>>,
    boxes: &str,
    (size, radius, cross): ((u32, u32), u32, [f32; 2]),
) -> Measured {
    let station = report_station(report, None);
    let Place::Body { cell, origin, yaw } = station.place else {
        panic!("the report names where the body stood");
    };
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: the top 16 bits of a cell id
    let block = (cell >> 16) as u16;
    let yaw = yaw.to_radians();
    let cfg = DeviceConfig {
        width: size.0,
        height: size.1,
        remaster: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let (ws0, dev0) = memory();
    let t = Instant::now();
    let mut shot = Shot::open_with(store, &mut gpu, &station, false, Some(radius));
    let load_ms = t.elapsed().as_secs_f64() * 1e3;
    let t = Instant::now();
    shot.apply_report(report);
    shot.poll(&mut gpu);
    shot.scene.draw.set_remaster_pack(pack.cloned(), None);
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(boxes).expect("the boxes read");
    shot.poll(&mut gpu);
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    let first_ms = t.elapsed().as_secs_f64() * 1e3;
    for _ in 0..SETTLE {
        shot.step_held(&mut gpu);
    }
    let (mut wall, mut cpu, mut end, mut device) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    // The world's work by its parts, summed: the objects' sync, the simulation, the streaming,
    // the arena and the draw, and the draw's walk and its handing over to the remaster.
    let mut phases = [0.0f64; 7];
    for _ in 0..TIMED {
        let t = Instant::now();
        shot.step_held(&mut gpu);
        wall.push(t.elapsed().as_secs_f64() * 1e3);
        let l = shot.last;
        // The world's own work on the processor, and the frame's end apart: the presentation's
        // preparation, recording and submission, and any wait for the device there. The arena
        // the station reserves before each frame is left out: it waits for the device, and the
        // client reserves it once, at the load.
        cpu.push(l.sync + l.update + l.stream + l.draw);
        end.push(l.end);
        let c = shot.scene.draw.frame_costs();
        for (k, v) in [
            l.sync,
            l.update,
            l.stream,
            l.reserve,
            l.draw,
            f64::from(c.walk),
            f64::from(c.remaster),
        ]
        .into_iter()
        .enumerate()
        {
            phases[k] += v;
        }
        device.push(
            shot.scene
                .draw
                .remaster_timings(&mut gpu)
                .iter()
                .map(|t| t.1)
                .sum::<f64>(),
        );
    }
    let rgba = capture(&mut gpu);
    let (ws, dev) = memory();
    let stats = shot.scene.draw.stats;
    let lod = shot
        .scene
        .draw
        .remaster_lod_stats(&mut gpu)
        .unwrap_or_default();
    let imp = shot
        .scene
        .draw
        .remaster_impostor_stats(&mut gpu)
        .unwrap_or_default();
    let far = shot
        .scene
        .draw
        .remaster_forest_stats(&mut gpu)
        .unwrap_or_default();
    // The step across the block line: each meter's frame, the crossing's the one whose block
    // building shows.
    let mut steps = Vec::new();
    let mut at = cross[0];
    while at <= cross[1] + 1e-3 {
        let (x, y, ground) = along(store, (block, origin.x, origin.y, yaw), at);
        shot.place_body(block, (x, y), ground, yaw);
        let t = Instant::now();
        shot.step_held(&mut gpu);
        let wall = t.elapsed().as_secs_f64() * 1e3;
        steps.push((at, shot.last.stream, shot.last.end, wall));
        let (l, c) = (shot.last, shot.scene.draw.frame_costs());
        println!(
            "  step {at:+.1} m by part: sync {:.1} update {:.1} stream {:.1} reserve {:.1} draw \
             {:.1} (walk {:.1}, remaster {:.1}) end {:.1} ms",
            l.sync, l.update, l.stream, l.reserve, l.draw, c.walk, c.remaster, l.end
        );
        for _ in 0..4 {
            shot.step_held(&mut gpu);
        }
        at += 1.0;
    }
    let crossing = steps
        .iter()
        .copied()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap_or_default();
    let (wall_med, wall_least) = median_least(&mut wall);
    let (cpu_med, cpu_least) = median_least(&mut cpu);
    let (end_med, end_least) = median_least(&mut end);
    let (dev_med, dev_least) = median_least(&mut device);
    let line = format!(
        "{radius}\t{load_ms:.0}\t{first_ms:.0}\t{wall_med:.2}\t{wall_least:.2}\t{cpu_med:.2}\t\
         {cpu_least:.2}\t{end_med:.2}\t{end_least:.2}\t{dev_med:.2}\t{dev_least:.2}\t{ws:.0}\t{:.0}\t\
         {dev:.0}\t{:.0}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.1}\t{:.1}\t{:.1}\t{:.1}",
        ws - ws0,
        dev - dev0,
        stats.blocks_meshed,
        stats.scenery_objects,
        stats.static_objects,
        stats.object_triangles,
        lod.parts,
        imp.cards,
        far.standing,
        far.picture_bytes >> 20,
        crossing.0,
        crossing.1,
        crossing.2,
        crossing.3,
    );
    println!(
        "radius {radius}: load {load_ms:.0} ms, first frame {first_ms:.0} ms | frame wall \
         {wall_med:.2} ms (least {wall_least:.2}), world {cpu_med:.2} ms (least {cpu_least:.2}), end \
         {end_med:.2} ms (least {end_least:.2}), device {dev_med:.2} ms (least {dev_least:.2}) | \
         working set {ws:.0} MiB (+{:.0}), device {dev:.0} MiB (+{:.0}) | blocks {} scenery {} \
         statics {} object triangles {} | parts {} impostor cards {} far standing {} ({} MiB) | \
         crossing at {:+.1} m: stream {:.1} ms, end {:.1} ms, frame {:.1} ms",
        ws - ws0,
        dev - dev0,
        stats.blocks_meshed,
        stats.scenery_objects,
        stats.static_objects,
        stats.object_triangles,
        lod.parts,
        imp.cards,
        far.standing,
        far.picture_bytes >> 20,
        crossing.0,
        crossing.1,
        crossing.2,
        crossing.3,
    );
    #[allow(clippy::cast_precision_loss)] // a few dozen frames
    let per = |v: f64| v / TIMED as f64;
    println!(
        "  world by part, mean: sync {:.2} ms, update {:.2} ms, stream {:.2} ms, reserve {:.2} \
         ms, draw {:.2} ms (its walk {:.2} ms, the remaster's {:.2} ms)",
        per(phases[0]),
        per(phases[1]),
        per(phases[2]),
        per(phases[3]),
        per(phases[4]),
        per(phases[5]),
        per(phases[6]),
    );
    for (at, stream, end, wall) in &steps {
        println!("  step {at:+.1} m: stream {stream:.1} ms, end {end:.1} ms, frame {wall:.1} ms");
    }
    drop(shot);
    drop(gpu);
    Measured { radius, line, rgba }
}

/// Each radius's costs where the report stands, and its still.
#[test]
#[ignore = "instrument: prints what each scenery radius costs and writes stills; run with --ignored"]
fn measure_the_scenery_radius() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let pack = env("DERETH_ART_PACK").map(|list| {
        let paths: Vec<std::path::PathBuf> =
            list.split(';').map(std::path::PathBuf::from).collect();
        Arc::new(ArtPack::open_stack(&paths).expect("the packs open"))
    });
    let dir = env("DERETH_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    let tag = env("DERETH_REMASTER_LOOK_TAG").unwrap_or_else(|| "remaster".to_owned());
    let size = env("DERETH_REMASTER_SIZE").map_or((1280, 540), |_| super::remaster_frames::size());
    let report =
        VisualReport::parse(&env("DERETH_RADIUS_PLACE").unwrap_or_else(|| HILL.to_owned()))
            .expect("the place's report reads");
    let mut boxes = env("DERETH_REMASTER_LOOK_BOXES").unwrap_or_else(|| EVERY_BOX.to_owned());
    if pack.is_some() {
        boxes.push_str(",RemasterArtPack=1");
    }
    let radii: Vec<u32> = env("DERETH_RADIUS_LIST")
        .unwrap_or_else(|| "1,2,3,4".to_owned())
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    let cross: [f32; 2] = env("DERETH_RADIUS_CROSS")
        .and_then(|v| {
            let n: Vec<f32> = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
            n.try_into().ok()
        })
        .unwrap_or([-1.0, 1.0]);
    for radius in radii {
        let m = measure(
            &store,
            &report,
            pack.as_ref(),
            &boxes,
            (size, radius, cross),
        );
        if let Some(dir) = dir.as_ref() {
            stations::write_png(
                &dir.join(format!("radius-{}-{tag}.png", m.radius)),
                size.0,
                size.1,
                &m.rgba,
            );
            let path = dir.join(format!("radius-{tag}.tsv"));
            let fresh = !path.exists();
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .expect("the measurements file");
            if fresh {
                writeln!(
                    f,
                    "radius\tload_ms\tfirst_ms\twall_med\twall_least\tworld_med\tworld_least\t\
                     end_med\tend_least\tdevice_med\tdevice_least\tworking_set_mib\t\
                     working_set_added\tdevice_mib\tdevice_added\tblocks\tscenery\tstatics\t\
                     object_triangles\tparts\timpostor_cards\tfar_standing\tfar_mib\t\
                     crossing_at\tcrossing_stream_ms\tcrossing_end_ms\tcrossing_frame_ms"
                )
                .expect("the header");
            }
            writeln!(f, "{}", m.line).expect("a row");
        }
    }
}
