//! Frames and costs where generated scenery meets a landblock line: in forests, on the line,
//! across it, from above the corner where four blocks meet, and in the middle of a block, plus a
//! block of slope-aligned pieces. Each station is written as a PNG to
//! `DERETH_TEST_SCENERY_EDGE_DUMP`, with one row of measurements (scene load, warm frame time,
//! draw calls a frame, the process's working set) appended to `stations.tsv` there.
//! `DERETH_TEST_SCENERY_EDGE_STATION` picks one station by name, so each can run in a process of
//! its own. They assert nothing.

#![cfg(gpu)]

use std::io::Write;
use std::sync::Arc;
use std::time::Instant;

use dereth_client_runtime::character::CharacterInput;
use dereth_client_runtime::objects::ObjectStream;
use dereth_client_runtime::world_build::read_landblock;
use dereth_dat::RetailDatStore;
use dereth_primitives::{LocalTime, Vec3};
use dereth_render::device::Gpu;
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

const W: u32 = 1280;
const H: u32 = 720;
/// Frames run before anything is measured, so streaming has finished and caches are warm.
const SETTLE: usize = 40;
/// Frames timed.
const TIMED: usize = 120;

/// One viewpoint: a block, and an eye in that block's coordinates, `above` metres over the
/// ground under it, turned `yaw` (0 north, counter-clockwise) and `pitch` (radians, up positive).
struct Station {
    name: &'static str,
    block: u16,
    x: f32,
    y: f32,
    above: f32,
    yaw: f32,
    pitch: f32,
}

const WEST: f32 = std::f32::consts::FRAC_PI_2;

/// Each forest gets four: standing on its west line looking north along it, standing 40 m inside
/// looking west across it, over its south-west corner looking down, and in the block's middle.
const fn forest(prefix: &'static [&'static str; 4], block: u16) -> [Station; 4] {
    [
        Station {
            name: prefix[0],
            block,
            x: 0.5,
            y: 30.0,
            above: 1.8,
            yaw: 0.0,
            pitch: -0.05,
        },
        Station {
            name: prefix[1],
            block,
            x: 40.0,
            y: 96.0,
            above: 1.8,
            yaw: WEST,
            pitch: -0.05,
        },
        Station {
            name: prefix[2],
            block,
            x: 0.0,
            y: 0.0,
            above: 220.0,
            yaw: 0.0,
            pitch: -1.45,
        },
        Station {
            name: prefix[3],
            block,
            x: 96.0,
            y: 96.0,
            above: 1.8,
            yaw: 0.0,
            pitch: -0.05,
        },
    ]
}

fn stations() -> Vec<Station> {
    let mut v = Vec::new();
    v.extend(forest(
        &["42cb-line", "42cb-across", "42cb-corner", "42cb-middle"],
        0x42CB,
    ));
    v.extend(forest(
        &["5ce9-line", "5ce9-across", "5ce9-corner", "5ce9-middle"],
        0x5CE9,
    ));
    v.extend(forest(
        &["ce3f-line", "ce3f-across", "ce3f-corner", "ce3f-middle"],
        0xCE3F,
    ));
    v.extend(forest(
        &["eb44-line", "eb44-across", "eb44-corner", "eb44-middle"],
        0xEB44,
    ));
    v.extend(forest(
        &["a9b3-line", "a9b3-across", "a9b3-corner", "a9b3-middle"],
        0xA9B3,
    ));
    // Slope-aligned pieces: one on a slope, seen from 12 m south-west of it, and the block from
    // above.
    v.push(Station {
        name: "805e-aligned",
        block: 0x805E,
        x: 129.0,
        y: 131.5,
        above: 3.0,
        yaw: -std::f32::consts::FRAC_PI_4,
        pitch: -0.12,
    });
    v.push(Station {
        name: "805e-above",
        block: 0x805E,
        x: 96.0,
        y: 96.0,
        above: 260.0,
        yaw: 0.0,
        pitch: -1.45,
    });
    v
}

/// The ground under a point of a block, from the block's own terrain.
fn ground(store: &RetailDatStore, block: u16, x: f32, y: f32) -> f32 {
    let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
    let table = dereth_terrain::land::mesh::height_table(&region);
    let (bx, by) = (i32::from(block >> 8), i32::from(block & 0xFF));
    let lb = read_landblock(store, bx, by).expect("the landblock");
    let mesh = dereth_terrain::land::mesh::generate_landblock_with_table(
        &lb,
        &region,
        &table,
        bx,
        by,
        1,
        dereth_terrain::land::mesh::Direction::InViewerBlock,
    );
    let (cx, cy) = (x.clamp(0.0, 191.99), y.clamp(0.0, 191.99));
    let cell = dereth_terrain::scenery::outside_cell_index(cx, cy);
    let plane = dereth_terrain::scenery::find_terrain_poly(&mesh, cell, cx, cy)
        .expect("a terrain polygon under the station");
    dereth_terrain::land::mesh::plane_set_height(plane, cx, cy).expect("a height")
}

/// One frame. Returns the draw calls it issued.
fn frame(store: &Arc<RetailDatStore>, gpu: &mut Gpu, scene: &mut WorldScene, t: f64) -> u64 {
    let mut stream = ObjectStream::new();
    scene
        .sync_objects(store, gpu, &mut stream)
        .expect("sync_objects");
    scene.update(
        dereth_client_runtime::camera::CameraInput::default(),
        CharacterInput::default(),
        LocalTime(t),
        1.0 / 30.0,
    );
    scene.stream(store, gpu).expect("stream");
    scene.reserve_upload_arena(gpu).expect("reserve the arena");
    let before = gpu.draw_calls();
    gpu.begin_frame().expect("begin");
    scene.draw(gpu).expect("draw");
    gpu.end_frame().expect("end");
    gpu.draw_calls() - before
}

/// The process's working set and peak working set, in MiB, as the system reports them.
fn working_set() -> (f64, f64) {
    let pid = std::process::id();
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!("$p = Get-Process -Id {pid}; \"$($p.WorkingSet64) $($p.PeakWorkingSet64)\""),
        ])
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

fn write_png(path: &str, rgba: &[u8]) {
    let f = std::fs::File::create(path).expect("create the png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), W, H);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().expect("png header");
    w.write_image_data(rgba).expect("png data");
}

#[test]
#[ignore = "instrument: writes PNGs and measurements; run with DERETH_TEST_SCENERY_EDGE_DUMP set and --ignored"]
fn render_the_scenery_edge_stations() {
    let Ok(dir) = std::env::var("DERETH_TEST_SCENERY_EDGE_DUMP") else {
        return;
    };
    std::fs::create_dir_all(&dir).expect("the dump folder");
    let only = std::env::var("DERETH_TEST_SCENERY_EDGE_STATION").ok();
    let store = crate::common::dats();
    for st in stations() {
        if only.as_deref().is_some_and(|o| o != st.name) {
            continue;
        }
        let z = ground(&store, st.block, st.x, st.y) + st.above;
        let mut gpu = crate::common::test_gpu(W, H);
        let cfg = SceneConfig {
            landblock: st.block,
            character: false,
            time_of_day: Some(0.5),
            particles: false,
            land_radius: dereth_client_runtime::config::Config::default().land_radius,
            scenery_radius: dereth_client_runtime::config::Config::default().scenery_radius,
            ..SceneConfig::default()
        };
        let t0 = Instant::now();
        let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the scene loads");
        let load_ms = t0.elapsed().as_secs_f64() * 1e3;
        scene.camera.position = Vec3::new(st.x, st.y, z);
        scene.camera.yaw = st.yaw;
        scene.camera.pitch = st.pitch;
        let mut t = 1.0;
        let t1 = Instant::now();
        let _ = frame(&store, &mut gpu, &mut scene, t);
        let first_frame_ms = t1.elapsed().as_secs_f64() * 1e3;
        for _ in 1..SETTLE {
            t += 1.0 / 30.0;
            let _ = frame(&store, &mut gpu, &mut scene, t);
        }
        let _ = gpu.capture().expect("capture");
        let mut times = Vec::with_capacity(TIMED);
        let mut draws = 0u64;
        for _ in 0..TIMED {
            t += 1.0 / 30.0;
            let s = Instant::now();
            draws += frame(&store, &mut gpu, &mut scene, t);
            times.push(s.elapsed().as_secs_f64() * 1e3);
        }
        // Wait for the device so the last frames' work is inside the timing.
        let s = Instant::now();
        let rgba = gpu.capture().expect("capture").to_rgba();
        let drain_ms = s.elapsed().as_secs_f64() * 1e3;
        let total: f64 = times.iter().sum::<f64>() + drain_ms;
        times.sort_by(f64::total_cmp);
        let median = times[times.len() / 2];
        let (ws, peak) = working_set();
        #[allow(clippy::cast_precision_loss)]
        let (mean, dpf) = (total / TIMED as f64, draws as f64 / TIMED as f64);
        write_png(&format!("{dir}/{}.png", st.name), &rgba);
        let mut tsv = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!("{dir}/stations.tsv"))
            .expect("the measurements file");
        writeln!(
            tsv,
            "{}\t{load_ms:.1}\t{first_frame_ms:.1}\t{mean:.3}\t{median:.3}\t{dpf:.1}\t{ws:.1}\t{peak:.1}",
            st.name
        )
        .expect("a row");
        eprintln!(
            "{}: load {load_ms:.1} ms, first frame {first_frame_ms:.1} ms, warm frame mean \
             {mean:.3} ms median {median:.3} ms, {dpf:.1} draw calls a frame, working set \
             {ws:.1} MiB (peak {peak:.1})",
            st.name
        );
    }
}
