//! The remaster's ground drawn from its splats: each cell's composite kept as its layers (the
//! base tile, then each overlay's alpha map and tile) and blended at every pixel as retail blends
//! them, the layers shared by every cell.
//!
//! - **Like Retail Look's ground, with no seams along the cells.** At a road beside a building
//!   and at a grass field running into a road's verge, the ground is within a measured mean of
//!   Retail Look's. The color step between neighboring pixels on either side of a cell's edge is
//!   no larger, against the step between neighbors within a cell, than Retail Look's own.
//! - **Memory.** The composites go: the ground's pictures held fall to the shared layers, and the
//!   scene's bytes on the device fall with them. What the scene no longer draws leaves the device.
//! - **Water by its share.** Where water is most of a pixel's blend, the ground is marked water,
//!   and the water's share is kept beside it, so the water ends along the shore's own blend.
//! - **The ground alone.** A block built again for its ground alone (its level of detail or its
//!   stitching) takes back its ground and nothing else of it.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.
//!
//! Behaviour: none (experimental)

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_client_runtime::report::VisualReport;
use dereth_dat::RetailDatStore;
use glam::{Mat4, Vec4};

use crate::instruments::remaster_frames::{self, Draw, Frame, Setup};
use crate::instruments::stations::{report_station, Station};

const SIZE: (u32, u32) = (960, 540);

/// A road running past a building, and a grass field running into a road's verge, each as the
/// frame a player saw it in.
const SPOTS: [(&str, &str); 2] = [
    (
        "road",
        "dereth report 1; loc 0xA7B40034 [153.813522 78.367264 28.822794] -0.810931 0.000000 \
         0.000000 -0.585141; camera 0xA7B4 [159.19102 76.58122 32.295414] yaw -5.033056 pitch \
         -0.33500063 cell 0xA7B40034; eye 0xA7B4 [159.19102 76.58122 32.06834]; fov 89.8 \
         vertical 39.553017 aspect 1; mode orbit; time 0.757 Evensong, set here; clock \
         306486183.56536025; weather clear; draw distance 4",
    ),
    (
        "grass",
        "dereth report 1; loc 0xA9B40009 [35.883541 3.020298 89.249931] 0.107557 0.000000 \
         0.000000 -0.994199; camera 0xA9B4 [34.621105 8.786784 91.82415] yaw -2.9260616 pitch \
         -0.18000092 cell 0xA9B40009; eye 0xA9B4 [34.621105 8.786784 91.64148]; fov 89.8 \
         vertical 39.553017 aspect 1; mode orbit; time 0.739 Warmtide-and-Half, set here; clock \
         306486044.9000565; weather clear; draw distance 4",
    ),
];

/// The ground's light class.
const GROUND: u8 = 1;

/// The most the splats' ground may be off Retail Look's on average, in display levels (the
/// largest channel). Measured: 0.54 at the road and 0.67 at the grass.
const GROUND_ERROR: f64 = 1.0;

/// The most the ground from its splats may hold, against its composites, in the town at land
/// radius 3. Measured: 39.0 MB against 225.7 MB.
const SPLAT_SHARE: u64 = 4;

/// A cell's side, meters.
const CELL: f32 = 24.0;

/// Retail's grid of cells, drawn from their splats or their composites.
fn splats(on: bool) -> dereth_scene::world_scene::RemasterTerrain {
    dereth_scene::world_scene::RemasterTerrain {
        splat: on,
        ..Default::default()
    }
}

fn spot(line: &str) -> (VisualReport, Station) {
    let report = VisualReport::parse(line).expect("the report reads");
    let mut station = report_station(&report, None);
    station.weather = false;
    (report, station)
}

/// One draw of a spot.
fn draw(store: &Arc<RetailDatStore>, line: &str, draw: Draw, splat: bool) -> Frame {
    let (report, station) = spot(line);
    remaster_frames::run_set(
        store,
        &station,
        draw,
        SIZE,
        &Setup {
            sky: None,
            ground: Some(splats(splat)),
            report: Some(&report),
            ..Setup::default()
        },
    )
}

/// Each ground pixel's cell, from where the remaster's depth puts it.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
fn cells(r: &Frame) -> Vec<Option<(i32, i32)>> {
    let inv = Mat4::from_cols_array(&r.view_projection).inverse();
    let (w, h) = (r.width as usize, r.height as usize);
    (0..w * h)
        .map(|i| {
            if r.class[i] != GROUND || r.remaster_depth[i] >= 1.0 {
                return None;
            }
            let (x, y) = ((i % w) as f32, (i / w) as f32);
            let ndc = Vec4::new(
                (x + 0.5) / w as f32 * 2.0 - 1.0,
                1.0 - (y + 0.5) / h as f32 * 2.0,
                r.remaster_depth[i],
                1.0,
            );
            let p = inv * ndc;
            let p = p / p.w;
            Some((
                dereth_primitives::num::floor_to_i32(p.x / CELL),
                dereth_primitives::num::floor_to_i32(p.y / CELL),
            ))
        })
        .collect()
}

/// The largest channel step between two pixels of `f`.
fn step(f: &Frame, a: usize, b: usize) -> f64 {
    (0..3)
        .map(|c| f64::from(f.rgba[a * 4 + c].abs_diff(f.rgba[b * 4 + c])))
        .fold(0.0, f64::max)
}

/// The mean step between neighboring ground pixels on either side of a cell's edge, over the
/// mean step between neighbors within one cell, in `f`; and how many pairs straddled an edge.
#[allow(clippy::cast_precision_loss)]
fn seam_ratio(f: &Frame, cells: &[Option<(i32, i32)>], width: usize) -> (f64, usize) {
    let (mut edge, mut edges, mut inner, mut inners) = (0.0, 0usize, 0.0, 0usize);
    for i in 0..cells.len() {
        let Some(a) = cells[i] else { continue };
        for j in [i + 1, i + width] {
            if (j == i + 1 && j % width == 0) || j >= cells.len() {
                continue;
            }
            let Some(b) = cells[j] else { continue };
            if a == b {
                inner += step(f, i, j);
                inners += 1;
            } else {
                edge += step(f, i, j);
                edges += 1;
            }
        }
    }
    (
        (edge / edges.max(1) as f64) / (inner / inners.max(1) as f64).max(1e-9),
        edges,
    )
}

/// The mean of the largest channel difference between `a` and `b` over the ground's pixels.
#[allow(clippy::cast_precision_loss)]
fn ground_error(a: &Frame, b: &Frame, cells: &[Option<(i32, i32)>]) -> (f64, usize) {
    let mut sum = 0.0;
    let mut n = 0usize;
    for (i, c) in cells.iter().enumerate() {
        if c.is_some() {
            sum += step_between(a, b, i);
            n += 1;
        }
    }
    (sum / n.max(1) as f64, n)
}

fn step_between(a: &Frame, b: &Frame, i: usize) -> f64 {
    (0..3)
        .map(|c| f64::from(a.rgba[i * 4 + c].abs_diff(b.rgba[i * 4 + c])))
        .fold(0.0, f64::max)
}

#[test]
fn the_ground_from_its_splats_is_retail_looks_without_seams_along_the_cells() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    for (name, line) in SPOTS {
        let retail = draw(&store, line, Draw::Retail, false);
        let composite = draw(&store, line, Draw::Vertex, false);
        let splat = draw(&store, line, Draw::Vertex, true);
        let at = cells(&splat);
        let width = splat.width as usize;
        let (error, ground) = ground_error(&retail, &splat, &at);
        let (composite_error, _) = ground_error(&retail, &composite, &at);
        let (retail_seams, edges) = seam_ratio(&retail, &at, width);
        let (composite_seams, _) = seam_ratio(&composite, &at, width);
        let (splat_seams, _) = seam_ratio(&splat, &at, width);
        if let Ok(dir) = std::env::var("DERETH_REMASTER_CAPTURE_DIR") {
            let dir = std::path::Path::new(&dir);
            for (what, f) in [
                ("retail", &retail),
                ("composite", &composite),
                ("splat", &splat),
            ] {
                crate::instruments::stations::write_png(
                    &dir.join(format!("ground-{name}-{what}.png")),
                    f.width,
                    f.height,
                    &f.rgba,
                );
            }
        }
        println!(
            "{name}: {ground} ground pixels, {edges} pairs across a cell's edge; against Retail \
             Look the splats {error:.2}, the composites {composite_error:.2}; seams Retail Look \
             {retail_seams:.3}, composites {composite_seams:.3}, splats {splat_seams:.3}"
        );
        assert!(edges > 1000, "{name}: the cells' edges are in view");
        assert!(
            error <= GROUND_ERROR,
            "{name}: the ground is {error:.2} levels off Retail Look's"
        );
        assert!(
            splat_seams <= retail_seams,
            "{name}: the cells' edges step {splat_seams:.3} times their insides, Retail Look's \
             {retail_seams:.3}"
        );
    }
}

/// Every piece of the ground.
fn every_piece() -> dereth_scene::world_scene::RemasterTerrain {
    dereth_scene::world_scene::RemasterTerrain {
        splat: true,
        grid: true,
        drawn: true,
        horizon: true,
    }
}

/// The town by day at land radius `radius`, its ground drawn as `ground` says, and what the
/// remaster counted.
fn town_counts_with(
    store: &Arc<RetailDatStore>,
    radius: u32,
    ground: dereth_scene::world_scene::RemasterTerrain,
) -> dereth_scene::world_scene::RemasterCounts {
    let mut station = remaster_frames::stations(store, &["holtburg"])
        .pop()
        .expect("the town");
    station.land_radius = radius;
    remaster_frames::run_set(
        store,
        &station,
        Draw::Vertex,
        SIZE,
        &Setup {
            ground: Some(ground),
            ..Setup::default()
        },
    )
    .counts
    .expect("the remaster counted")
}

/// The town by day at land radius `radius`, drawn with the ground's composites or its splats, and
/// what the remaster counted.
fn town_counts(
    store: &Arc<RetailDatStore>,
    radius: u32,
    splat: bool,
) -> dereth_scene::world_scene::RemasterCounts {
    let mut station = remaster_frames::stations(store, &["holtburg"])
        .pop()
        .expect("the town");
    station.land_radius = radius;
    remaster_frames::run_set(
        store,
        &station,
        Draw::Vertex,
        SIZE,
        &Setup {
            ground: Some(splats(splat)),
            ..Setup::default()
        },
    )
    .counts
    .expect("the remaster counted")
}

#[test]
fn the_ground_from_its_splats_holds_no_composite_and_a_fraction_of_their_bytes() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let composites = town_counts(&store, 3, false);
    let splats = town_counts(&store, 3, true);
    println!(
        "composites: {} pictures, {} bytes held; splats: {} splats of {} pictures, {} bytes held",
        composites.terrain_textures,
        composites.resident_bytes,
        splats.splats,
        splats.terrain_sources,
        splats.resident_bytes
    );
    assert!(composites.terrain_textures > 100 && composites.splats == 0);
    assert_eq!(splats.terrain_textures, 0, "no composite is held");
    assert!(splats.splats > 100 && splats.terrain_sources > 10);
    assert!(
        splats.resident_bytes * SPLAT_SHARE <= composites.resident_bytes,
        "{} bytes held against {}",
        splats.resident_bytes,
        composites.resident_bytes
    );
}

/// Instrument: the bytes the remaster's scene holds on the device at each draw distance, the
/// ground drawn from its composites, from its splats, and with every piece of it on (with the
/// bytes the ground's grid holds for the world's vertices and the near offset).
#[test]
#[ignore = "instrument: prints the scene's bytes on the device by draw distance; run with --ignored"]
fn print_the_scenes_bytes_by_draw_distance() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    for radius in [3, 5, 10, 15, 20, 25] {
        for splat in [false, true] {
            let c = town_counts(&store, radius, splat);
            if splat {
                let g = town_counts_with(&store, radius, every_piece());
                #[allow(clippy::cast_precision_loss)]
                let (mb, ground) = (g.resident_bytes as f64 / 1e6, g.ground_bytes as f64 / 1e6);
                println!(
                    "radius {radius:2} every piece: {mb:.1} MB held, {ground:.1} MB for the grid's world"
                );
            }
            #[allow(clippy::cast_precision_loss)]
            let mb = c.resident_bytes as f64 / 1e6;
            println!(
                "radius {radius:2} {}: {} blocks, {mb:.1} MB held ({:.1} MB of the pools in use), \
                 {} composites, {} splats of {} pictures",
                if splat { "splats    " } else { "composites" },
                c.blocks,
                c.pool_bytes_used as f64 / 1e6,
                c.terrain_textures,
                c.splats,
                c.terrain_sources
            );
        }
    }
}

/// The town on a device of its own with the remaster on, its ground from its splats.
fn walking_town(
    store: &Arc<RetailDatStore>,
) -> (
    dereth_render::device::Gpu,
    crate::instruments::stations::Shot,
) {
    use dereth_render::device::{DeviceConfig, Gpu};
    let station = remaster_frames::stations(store, &["holtburg"])
        .pop()
        .expect("the town");
    let cfg = DeviceConfig {
        width: SIZE.0,
        height: SIZE.1,
        remaster: true,
        debug: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    assert_eq!(
        gpu.adapter_kind(),
        dereth_render::device::AdapterKind::Hardware,
        "{} is a software rasterizer",
        gpu.adapter_name()
    );
    let mut shot = crate::instruments::stations::Shot::open(store, &mut gpu, &station);
    shot.poll(&mut gpu);
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    shot.scene.draw.set_remaster_terrain(splats(true), &mut gpu);
    (gpu, shot)
}

#[test]
fn a_block_built_again_for_its_ground_alone_takes_back_its_ground_alone() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let (mut gpu, mut shot) = walking_town(&store);
    shot.step(&mut gpu);
    let start = shot
        .scene
        .draw
        .remaster_counts(&mut gpu)
        .expect("the remaster counted");
    // East two meters a frame for 150 frames, across two of the town's block lines, the clock
    // held.
    for _ in 0..150 {
        shot.scene.camera.position.x += 2.0;
        shot.step_held(&mut gpu);
    }
    for _ in 0..4 {
        shot.step_held(&mut gpu);
    }
    let end = shot
        .scene
        .draw
        .remaster_counts(&mut gpu)
        .expect("the remaster counted");
    let walked = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew");
    drop(shot);
    drop(gpu);

    // The same place, come to at once.
    let (mut gpu, mut shot) = walking_town(&store);
    shot.step_held(&mut gpu);
    shot.scene.camera.position.x += 300.0;
    for _ in 0..12 {
        shot.step_held(&mut gpu);
    }
    let direct = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew");
    drop(shot);
    drop(gpu);
    let world: Vec<usize> = (0..walked.class.len())
        .filter(|&i| walked.class[i] != 0 || direct.class[i] != 0)
        .collect();
    let differ = world
        .iter()
        .filter(|&&i| walked.color[i * 4..i * 4 + 3] != direct.color[i * 4..i * 4 + 3])
        .count();
    println!(
        "walked: {} blocks in, {} built again whole, {} for their ground alone; {} scene bytes \
         sent; {} blocks and {} grounds held; {differ} of {} world pixels differ from the place \
         come to at once",
        end.blocks_in - start.blocks_in,
        end.blocks_changed - start.blocks_changed,
        end.grounds_changed - start.grounds_changed,
        end.scene_bytes - start.scene_bytes,
        end.blocks,
        end.grounds,
        world.len()
    );
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    // Measured: 14 blocks in, 12 built again whole (their objects came or went with their
    // distance), 40 for their ground alone; sent whole, the 52 took 2,263,044 bytes and now
    // 1,685,508.
    assert!(
        end.grounds_changed - start.grounds_changed >= 20,
        "the walk built blocks again for their ground"
    );
    assert!(end.blocks_changed - start.blocks_changed <= 20);
    assert_eq!(
        end.grounds, end.blocks,
        "a ground for every block held, and no more"
    );
    assert_eq!(
        differ, 0,
        "the walked scene draws as the scene come to at once"
    );
}

#[test]
fn the_ground_from_its_splats_marks_water_by_its_share_of_the_blend() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let station = remaster_frames::stations(&store, &["coast"])
        .pop()
        .expect("the coast");
    let run = |splat| {
        remaster_frames::run_set(
            &store,
            &station,
            Draw::Vertex,
            SIZE,
            &Setup {
                ground: Some(splats(splat)),
                ..Setup::default()
            },
        )
    };
    let count = |f: &Frame| {
        let ground = (0..f.class.len()).filter(|&i| f.class[i] == GROUND);
        let (mut water, mut shore) = (0usize, 0usize);
        for i in ground {
            if f.kind[i] == 7 {
                water += 1;
            }
            if f.water[i] > 0 && f.water[i] < 255 {
                shore += 1;
            }
        }
        (water, shore)
    };
    let (composite_water, composite_shore) = count(&run(false));
    let (splat_water, splat_shore) = count(&run(true));
    println!(
        "water pixels: composites {composite_water} ({composite_shore} partly), splats \
         {splat_water} ({splat_shore} partly)"
    );
    // Measured: 230,265 with none partly under the composites, whose cells are water only when
    // every corner is; 313,142 with 54,889 partly from the splats.
    assert_eq!(composite_shore, 0);
    assert!(
        splat_water > composite_water,
        "the shore's cells are water where water is most of them"
    );
    assert!(
        splat_shore > 1000,
        "the water's share fades along the shore"
    );
}

#[test]
fn what_the_scene_no_longer_draws_leaves_the_device() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let (mut gpu, mut shot) = walking_town(&store);
    shot.step_held(&mut gpu);
    let town = shot
        .scene
        .draw
        .remaster_counts(&mut gpu)
        .expect("the remaster counted");
    // Twelve blocks east, past the town's window, and as many frames as it takes to stream.
    shot.scene.camera.position.x += 12.0 * 192.0;
    for _ in 0..12 {
        shot.step_held(&mut gpu);
    }
    let away = shot
        .scene
        .draw
        .remaster_counts(&mut gpu)
        .expect("the remaster counted");
    println!(
        "the town: {} blocks, {} pictures, {} meshes, {} bytes held; away: {} blocks, {} \
         pictures, {} meshes, {} bytes held",
        town.blocks,
        town.textures,
        town.meshes,
        town.resident_bytes,
        away.blocks,
        away.textures,
        away.meshes,
        away.resident_bytes
    );
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}"); // Measured: 324 pictures, 613 meshes and 41,121,868 bytes held in the town; 65, 104 and
                                                                     // 31,179,196 away from it (the pools keep the room they grew to).
    assert_eq!(away.blocks, town.blocks, "the window holds as many blocks");
    assert!(
        away.textures * 2 < town.textures,
        "the town's pictures went"
    );
    assert!(away.meshes * 2 < town.meshes, "the town's meshes went");
    assert!(away.resident_bytes < town.resident_bytes);
}
