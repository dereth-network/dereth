//! The remaster's drawn ground: retail's surface with the scene's visual offset over it, drawn as
//! a grid of patches.
//!
//! - **The ground the device draws is the ground the scene stands bodies on.** Over a river
//!   valley made for the test, the drawn heights the device works out beside and under the water
//!   are the scene's own to a couple of millimeters, and a body standing on retail's ground there
//!   is drawn on them. At a road in a town, round the player's body, the device's heights are the
//!   scene's.
//! - **A still view is still.** Stepped with its clock held, the ground draws the same frame
//!   again.
//! - **Far rings.** With the ground drawn to the world's edge, ground stands where the frame
//!   showed sky.
//! - **What stands on the ground stands on it as drawn.** The targeting ring lies on it; the
//!   particles outdoors are drawn lifted onto it; the far trees' impostor cards stand on its mesh,
//!   near the switch and far out.
//! - **The water's bed lies where bodies stand.** Over the valley, a body standing in the river
//!   where physics stands it is drawn there, on the bed the device draws. In the lake's shallows
//!   and out where it is deep, a body is drawn where physics puts it, water drawn or not, with the
//!   bed under its feet; a pillar standing in the water and a lily pad on it are drawn where
//!   retail places them, and a body on a boardwalk over the water, the boardwalk and its posts
//!   where they stand. What a body standing in the water holds is drawn in its hand.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.
//!
//! Behaviour: none (experimental)

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_client_runtime::report::VisualReport;
use dereth_dat::RetailDatStore;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::world_scene::remaster as rm;
use dereth_scene::world_scene::RemasterTerrain;
use dereth_terrain::land::drawn::{self, shore, GroundSource, Knob, Knobs, Point, Vertex};
use glam::Vec2;

use crate::instruments::remaster_frames::{self, Draw, Setup};
use crate::instruments::stations::{report_station, Shot, Station};

const SIZE: (u32, u32) = (960, 540);
/// The world's landscape vertices a side.
const SIDE: usize = 2041;
/// The block the made-up valley is drawn about.
const BLOCK: (i32, i32) = (100, 100);

/// A road running past a building, as the frame a player saw it in.
const ROAD: &str = "dereth report 1; loc 0xA7B40034 [153.813522 78.367264 28.822794] -0.810931 \
                    0.000000 0.000000 -0.585141; camera 0xA7B4 [159.19102 76.58122 32.295414] \
                    yaw -5.033056 pitch -0.33500063 cell 0xA7B40034; eye 0xA7B4 [159.19102 \
                    76.58122 32.06834]; fov 89.8 vertical 39.553017 aspect 1; mode orbit; time \
                    0.757 Evensong, set here; clock 306486183.56536025; weather clear; draw \
                    distance 4";

/// Every piece of the ground.
fn every_piece() -> RemasterTerrain {
    RemasterTerrain {
        splat: true,
        grid: true,
        drawn: true,
        horizon: true,
    }
}

/// `station` on a device of its own with the remaster on and its ground as `ground` says, the
/// world's vertices read and the camera held where `report` says.
fn opened(
    store: &Arc<RetailDatStore>,
    station: &Station,
    report: Option<&VisualReport>,
    ground: RemasterTerrain,
) -> (Gpu, Shot) {
    opened_with(store, station, report, ground, None)
}

/// [`opened`], with the scenery drawn `scenery_radius` blocks round the viewer's block where it
/// is given.
fn opened_with(
    store: &Arc<RetailDatStore>,
    station: &Station,
    report: Option<&VisualReport>,
    ground: RemasterTerrain,
    scenery_radius: Option<u32>,
) -> (Gpu, Shot) {
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
    let mut shot = Shot::open_with(store, &mut gpu, station, false, scenery_radius);
    if let Some(r) = report {
        shot.apply_report(r);
    }
    shot.poll(&mut gpu);
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_settings(dereth_scene::world_scene::RemasterSettings::off(), &mut gpu);
    shot.scene.draw.set_remaster_terrain(ground, &mut gpu);
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    for _ in 0..4 {
        shot.step_held(&mut gpu);
    }
    (gpu, shot)
}

/// A river running north through block (100, 100): a hillside eight meters a cell rising to the
/// west, whose foot meets a flat bank two cells wide; the river at 28 meters, eight cells wide;
/// and a gentle bank rising to the east.
struct Valley {
    heights: Vec<f32>,
    words: Vec<u16>,
}

impl Valley {
    fn new() -> Self {
        let mut heights = vec![0.0f32; SIDE * SIDE];
        let mut words = vec![1u16 << 2; SIDE * SIDE];
        for gy in 760..860usize {
            for gx in 760..860usize {
                #[allow(clippy::cast_precision_loss)] // vertex coordinates, exact in f32
                let x = gx as f32;
                let (h, water) = match gx {
                    0..=803 => (28.0 + 8.0 * (804.0 - x), false),
                    804..=805 => (28.0, false),
                    806..=813 => (28.0, true),
                    _ => (28.0 + 2.0 * (x - 813.0), false),
                };
                heights[gy * SIDE + gx] = h;
                if water {
                    words[gy * SIDE + gx] = 17 << 2;
                }
            }
        }
        Self { heights, words }
    }
}

impl GroundSource for Valley {
    fn vertex(&self, gx: i32, gy: i32) -> Option<Vertex> {
        let side = i32::try_from(SIDE).ok()?;
        if !(0..side).contains(&gx) || !(0..side).contains(&gy) {
            return None;
        }
        let k = usize::try_from(gy).ok()? * SIDE + usize::try_from(gx).ok()?;
        Some(Vertex {
            height: self.heights[k],
            word: self.words[k],
        })
    }
}

/// The valley's ground as the remaster draws it with `knobs`: the valley, its water field, its
/// block's offset and the ground for the device.
fn valley_ground(knobs: &Knobs) -> (Valley, Vec<[f32; 2]>, drawn::BlockField, rm::DrawnGround) {
    let valley = Valley::new();
    let field = shore::held(&shore::water_field(SIDE, &valley.heights, &valley.words));
    let block = drawn::block_field(&valley, BLOCK, &[], knobs);
    let ground = rm::DrawnGround {
        world: Arc::new(rm::DrawnWorld {
            serial: 1_000_000,
            #[allow(clippy::cast_possible_truncation)] // 2041
            side: SIDE as u32,
            heights: Arc::new(valley.heights.clone()),
            words: Arc::new(valley.words.clone()),
            water: Arc::new(field.clone()),
        }),
        tiles: vec![rm::DrawnTile {
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)] // block coordinates
            block: ((BLOCK.0 as u16) << 8) | BLOCK.1 as u16,
            generation: 1,
            offset: Arc::new(block.offset.clone()),
            weight: Arc::new(block.weight.clone()),
        }],
        reach: 1,
        global: Vec2::ZERO,
        knobs: rm::DrawnKnobs {
            micro: (0.0, 0.7),
            water: Some(knobs.get(Knob::BedSlope)),
            ..rm::DrawnKnobs::default()
        },
        bodies: Vec::new(),
    };
    (valley, field, block, ground)
}

/// Points across the hillside's foot, the bank, the river and the far bank, along three lines.
fn across_the_valley() -> Vec<[f32; 3]> {
    #[allow(clippy::cast_precision_loss)] // block coordinates, exact in f32
    let (ox, oy) = (BLOCK.0 as f32 * 192.0, BLOCK.1 as f32 * 192.0);
    let mut points = Vec::new();
    for row in [41.25, 96.5, 150.75] {
        for k in 0..100u8 {
            points.push([ox + 40.0 + f32::from(k) * 1.5, oy + row, 30.0]);
        }
    }
    points
}

/// The knobs the valley is compared under: a body stands on the ground under its middle, so the
/// two are compared exactly, and the near ground is drawn throughout.
fn valley_knobs() -> Knobs {
    let mut knobs = Knobs::default();
    knobs.set(Knob::WalkLowpass, 0.0);
    knobs.set(Knob::Far, 0.0);
    knobs
}

/// How far physics stands a body under retail's ground at global `(x, y)` of the valley, by its
/// cell's water corners: 0.9 m in a cell of water corners alone, in one with some 0.45 m where
/// the corner nearest is water and 0.1 m where it is land, and none in a cell with none.
fn valley_sink(valley: &Valley, x: f32, y: f32) -> f32 {
    let wet = |gx: f32, gy: f32| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // inside the valley
        let k = gy as usize * SIDE + gx as usize;
        (valley.words[k] >> 2) & 0x1F == 17
    };
    let (cx, cy) = ((x / 24.0).floor(), (y / 24.0).floor());
    let corners = [
        wet(cx, cy),
        wet(cx + 1.0, cy),
        wet(cx, cy + 1.0),
        wet(cx + 1.0, cy + 1.0),
    ];
    let nearest = wet(
        cx + f32::from(u8::from(x / 24.0 - cx > 0.5)),
        cy + f32::from(u8::from(y / 24.0 - cy > 0.5)),
    );
    if corners.iter().all(|w| *w) {
        0.9
    } else if !corners.iter().any(|w| *w) {
        0.0
    } else if nearest {
        0.45
    } else {
        0.1
    }
}

/// **The ground the device draws is the ground the scene stands bodies on, and in the water a
/// body's feet meet the bed**: across a valley made for the test, its hillside, its banks and
/// its river, the device draws the ground where the scene works it out; on land a body standing
/// on retail's ground is drawn on it, and in the river a body standing where physics stands it,
/// sunk by its cell's depth, is drawn there, on the bed the device draws under it, clear of the
/// water's edge and of where one depth eases into the next.
#[test]
fn the_ground_beside_and_under_the_water_is_drawn_where_the_scene_stands_bodies() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let station = remaster_frames::stations(&store, &["holtburg"])
        .pop()
        .expect("the town");
    let (mut gpu, shot) = opened(&store, &station, None, every_piece());
    let knobs = valley_knobs();
    let (valley, field, block, ground) = valley_ground(&knobs);
    let water = drawn::Water {
        shore: shore::Shore {
            side: SIDE,
            heights: &valley.heights,
            field: &field,
        },
        slope: knobs.get(Knob::BedSlope),
    };
    #[allow(clippy::cast_precision_loss)] // block coordinates, exact in f32
    let (ox, oy) = (BLOCK.0 as f32 * 192.0, BLOCK.1 as f32 * 192.0);
    let points = across_the_valley();
    let shader = shot
        .scene
        .draw
        .remaster_drawn_heights(&mut gpu, Some(&ground), &points);
    assert_eq!(shader.len(), points.len(), "the probe read back");
    let (mut dipped, mut depths, mut worst) = (0, [0usize; 3], 0.0f32);
    for (q, drawn_h) in points.iter().zip(&shader) {
        let (x, y) = (q[0], q[1]);
        let p = Point::at(&valley, x, y);
        let near = drawn::sample(&block.offset, x - ox, y - oy);
        let scene = drawn::drawn_at(&p, x, y, near, 0.0, Some(&water), &knobs);
        let off = (scene.height - drawn_h).abs();
        worst = worst.max(off);
        assert!(
            off < 2e-3,
            "at ({x}, {y}) the device draws the ground at {drawn_h}, the scene at {}",
            scene.height
        );
        // A body standing there, where physics puts it: on retail's ground on land, sunk as
        // physics sinks it in its water. Drawn where physics puts it, and in the water on the bed.
        let sink = valley_sink(&valley, x, y);
        let z = p.retail - sink;
        let feet = z + drawn::shift(drawn::held_from(z, p.retail), scene, 1.0);
        let inside = water.shore.distance(x, y, 0.0);
        let seam = |v: f32| (v - (v / 12.0).round() * 12.0).abs() < shore::SINK_EASE;
        if sink == 0.0 {
            if inside <= 0.0 {
                assert!(
                    (feet - drawn_h).abs() < 2e-3,
                    "at ({x}, {y}) a body stands at {feet} on ground drawn at {drawn_h}"
                );
            }
            if near < -0.2 {
                dipped += 1;
            }
        } else {
            assert!(
                (feet - z).abs() < 2e-3,
                "at ({x}, {y}) a body sunk {sink} m is drawn at {feet} for {z}"
            );
            if inside > water.slope && !seam(x) && !seam(y) {
                assert!(
                    (feet - drawn_h).abs() < 5e-3,
                    "at ({x}, {y}) a body sunk {sink} m stands at {feet}, the bed at {drawn_h}"
                );
                depths[usize::from(sink > 0.1) + usize::from(sink > 0.5)] += 1;
            }
        }
    }
    println!(
        "{} points, worst difference {worst:e}; {dipped} where the rounding dips; feet on the bed \
         sunk 0.1, 0.45, 0.9 m: {depths:?}",
        points.len()
    );
    assert!(dipped > 5, "the hill's foot dips under retail's: {dipped}");
    assert!(
        depths[1] > 5 && depths[2] > 20,
        "bodies stand on the bed in the shallows and the deep: {depths:?}"
    );
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}

#[test]
fn round_the_players_body_the_device_draws_the_ground_the_scene_stands_it_on() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let report = VisualReport::parse(ROAD).expect("the report reads");
    let mut station = report_station(&report, None);
    station.weather = false;
    let (mut gpu, shot) = opened(&store, &station, Some(&report), every_piece());
    assert!(
        shot.scene.remaster_ground_active(),
        "the drawn ground is drawn"
    );
    let body = shot
        .scene
        .character
        .as_ref()
        .map(|c| c.render_frame().origin)
        .expect("a body");
    let eye = shot.scene.camera.position;
    let mut points = Vec::new();
    let mut cpu = Vec::new();
    for dy in -10..=10 {
        for dx in -10..=10 {
            let (x, y) = (body.x + dx as f32, body.y + dy as f32);
            let Some(h) = shot.scene.remaster_ground_at(x, y) else {
                continue;
            };
            let dist = ((x - eye.x).powi(2) + (y - eye.y).powi(2) + (h - eye.z).powi(2)).sqrt();
            points.push([x, y, dist]);
            cpu.push(h);
        }
    }
    let gpu_heights = shot
        .scene
        .draw
        .remaster_drawn_heights(&mut gpu, None, &points);
    assert_eq!(gpu_heights.len(), points.len(), "the probe read back");
    let worst = cpu
        .iter()
        .zip(&gpu_heights)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);
    let feet = body.z + shot.scene.remaster_shift_at(body);
    let under = shot
        .scene
        .remaster_ground_at(body.x, body.y)
        .expect("ground");
    println!(
        "{} points round the body, the worst {worst:e} apart; the body's feet at {feet}, the \
         ground under them at {under}",
        points.len()
    );
    assert!(worst < 2e-3, "the device's ground is the scene's: {worst}");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}

#[test]
fn a_still_view_of_the_drawn_ground_draws_the_same_frame_again() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let station = remaster_frames::stations(&store, &["holtburg"])
        .pop()
        .expect("the town");
    let (mut gpu, mut shot) = opened(&store, &station, None, every_piece());
    // The far trees come to the device a few models a frame, and a tree stands once its model's
    // pictures have come: the view is still once every far tree's have.
    for _ in 0..400 {
        shot.step_held(&mut gpu);
        let s = shot.scene.draw.remaster_forest_stats(&mut gpu);
        if s.is_none_or(|s| s.wanted > 0 && s.pictures >= s.wanted) {
            break;
        }
    }
    shot.step_held(&mut gpu);
    let a = shot.scene.draw.remaster_capture(&mut gpu).expect("drew");
    shot.step_held(&mut gpu);
    let b = shot.scene.draw.remaster_capture(&mut gpu).expect("drew");
    let world = (0..a.class.len()).filter(|&i| a.class[i] != 0 && a.class[i] != 5);
    let changed = world
        .filter(|&i| a.color[i * 4..i * 4 + 3] != b.color[i * 4..i * 4 + 3])
        .count();
    println!("{changed} world pixels changed under a still view");
    assert_eq!(changed, 0, "the ground holds still");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}

#[test]
fn with_the_ground_drawn_to_the_worlds_edge_ground_stands_where_the_sky_was() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let station = remaster_frames::stations(&store, &["holtburg"])
        .pop()
        .expect("the town");
    let run = |horizon| {
        remaster_frames::run_set(
            &store,
            &station,
            Draw::Vertex,
            SIZE,
            &Setup {
                ground: Some(RemasterTerrain {
                    splat: true,
                    grid: true,
                    drawn: false,
                    horizon,
                }),
                ..Setup::default()
            },
        )
    };
    let ground = |f: &remaster_frames::Frame| f.class.iter().filter(|&&c| c == 1).count();
    let (near, far) = (run(false), run(true));
    println!(
        "ground pixels: {} to the blocks held, {} to the world's edge",
        ground(&near),
        ground(&far)
    );
    // Measured: 242,129 and 291,269 at the town.
    assert!(
        ground(&far) > ground(&near) + 20_000,
        "the far rings stand where the sky was"
    );
}

/// **The targeting ring lies on the ground as the remaster draws it**: at the road, a ring laid
/// under an object standing eight meters ahead has its corners on the drawn ground, the scene's
/// and the device's, where with the ground drawn on retail's surface alone it lies on retail's.
#[test]
fn the_targeting_ring_lies_on_the_ground_as_the_remaster_draws_it() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let report = VisualReport::parse(ROAD).expect("the report reads");
    let mut station = report_station(&report, None);
    station.weather = false;
    let (mut gpu, mut shot) = opened(&store, &station, Some(&report), every_piece());
    shot.create_ahead(&station, 0x0200_0001, 8.0);
    for _ in 0..3 {
        shot.step_held(&mut gpu);
    }
    let marker = dereth_client_contract::overlay::GroundMarker {
        object: dereth_primitives::ObjectId(0x8300_0070),
        texture: dereth_client_contract::overlay::OverlayTexture {
            space: dereth_client_contract::overlay::OverlaySpace::Local,
            key: 1,
        },
        uv: [0.0, 0.0, 1.0, 1.0],
        tint: 0xFFFF_FFFF,
        scale: 1.0,
        turn: 0.0,
    };
    // Laid just over the ground, and over the most its fine relief rises.
    let lift = dereth_scene::world_scene::MARKER_LIFT + shot.scene.remaster_fine_relief();
    let drawn = shot.scene.ground_marker_corners(&marker);
    assert!(!drawn.is_empty(), "the object stands in the world");
    let eye = shot.scene.camera.position;
    let points: Vec<[f32; 3]> = drawn
        .iter()
        .map(|c| {
            let p = c.position;
            let h = p.z - lift;
            [
                p.x,
                p.y,
                ((p.x - eye.x).powi(2) + (p.y - eye.y).powi(2) + (h - eye.z).powi(2)).sqrt(),
            ]
        })
        .collect();
    let device = shot
        .scene
        .draw
        .remaster_drawn_heights(&mut gpu, None, &points);
    assert_eq!(device.len(), drawn.len(), "the probe read back");
    let (mut scene_worst, mut device_worst) = (0.0f32, 0.0f32);
    for (c, d) in drawn.iter().zip(&device) {
        let h = c.position.z - lift;
        let at = shot
            .scene
            .remaster_ground_at(c.position.x, c.position.y)
            .expect("the ground");
        scene_worst = scene_worst.max((h - at).abs());
        device_worst = device_worst.max((h - d).abs());
    }
    // The same ring with the ground drawn on retail's surface alone.
    shot.scene.draw.set_remaster_terrain(
        RemasterTerrain {
            splat: true,
            grid: true,
            ..RemasterTerrain::default()
        },
        &mut gpu,
    );
    shot.step_held(&mut gpu);
    let retail = shot.scene.ground_marker_corners(&marker);
    assert_eq!(retail.len(), drawn.len());
    let moved = drawn
        .iter()
        .zip(&retail)
        .map(|(a, b)| (a.position.z - b.position.z).abs())
        .fold(0.0f32, f32::max);
    println!(
        "{} corners: on the scene's drawn ground to {scene_worst:e}, the device's to \
         {device_worst:e}; moved off retail's ground by up to {moved}",
        drawn.len()
    );
    assert!(scene_worst < 1e-4, "on the scene's ground: {scene_worst}");
    assert!(
        device_worst < 2e-3,
        "on the device's ground: {device_worst}"
    );
    assert!(
        moved > 0.01,
        "the drawn ground lies off retail's here: {moved}"
    );
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}

/// **Particles outdoors stand on the ground as the remaster draws it**: at the road, every
/// particle outdoors the remaster draws (an object's ahead, hung on its parts) is drawn where the
/// walk puts it, lifted by the scene's own shift where it stands (the drawn ground's offset
/// there), each to a millimeter, some of them by more than a centimeter.
#[test]
fn outdoor_particles_stand_on_the_ground_as_the_remaster_draws_it() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let report = VisualReport::parse(ROAD).expect("the report reads");
    let mut station = report_station(&report, None);
    // The particles drawn.
    station.weather = true;
    let (mut gpu, mut shot) = opened(&store, &station, Some(&report), every_piece());
    // An object eight meters ahead playing a script of ten particles hung on its parts.
    shot.create_ahead(&station, 0x0200_0177, 8.0);
    for _ in 0..20 {
        shot.step_held(&mut gpu);
    }
    assert!(
        shot.scene.play_script_id(
            dereth_primitives::ObjectId(0x8300_0070),
            dereth_primitives::DataId(0x3300_088E)
        ),
        "the script plays"
    );
    for _ in 0..10 {
        shot.step(&mut gpu);
    }
    let walked = shot.scene.draw.remaster_walked_particles();
    let drawn = shot.scene.draw.remaster_particle_origins(&mut gpu);
    assert_eq!(walked.len(), drawn.len(), "every particle handed over");
    let mut worst = 0.0f32;
    let (mut outdoors, mut lifted) = (0usize, 0usize);
    for ((at, lift, out), d) in walked.iter().zip(&drawn) {
        if !out {
            continue;
        }
        outdoors += 1;
        lifted += usize::from(lift.abs() > 0.01);
        let want = [at[0], at[1], at[2] + lift];
        for k in 0..3 {
            worst = worst.max((d[k] - want[k]).abs());
        }
    }
    println!(
        "{} particles, {outdoors} outdoors, {lifted} lifted by more than a centimeter; the \
         worst {worst:e} off",
        drawn.len()
    );
    assert!(
        outdoors >= 10,
        "the script's particles stand outdoors: {outdoors}"
    );
    assert!(
        lifted > 0,
        "some stand where the drawn ground lies off retail's"
    );
    assert!(worst < 1e-3, "drawn where they stand: {worst}");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}

/// One far tree's card: its distance from the eye across the ground, meters; and how far its
/// tree stands off the ground's mesh, lifted and as retail stands it, and off the drawn surface
/// the mesh's vertices are laid on, each against the height retail stands it at over its own
/// ground (some trees are sunk into it).
#[derive(Debug, Clone, Copy)]
struct Row {
    distance: f32,
    off_mesh: f32,
    off_mesh_unlifted: f32,
    off_surface: f32,
}

/// One band of far trees' cards: the nearest and farthest card's distance, meters; how far off
/// the drawn mesh their trees stand, at most and on average, lifted and unlifted; and how far off
/// the drawn surface, at most.
struct Band {
    from: f32,
    to: f32,
    worst: f32,
    mean: f32,
    worst_unlifted: f32,
    mean_unlifted: f32,
    worst_surface: f32,
}

impl Band {
    fn of(rows: &[Row]) -> Self {
        let worst = |f: &dyn Fn(&Row) -> f32| rows.iter().map(f).fold(0.0f32, f32::max);
        #[allow(clippy::cast_precision_loss)] // a few hundred cards
        let mean =
            |f: &dyn Fn(&Row) -> f32| rows.iter().map(f).sum::<f32>() / rows.len().max(1) as f32;
        Self {
            from: rows.first().map_or(0.0, |x| x.distance),
            to: rows.last().map_or(0.0, |x| x.distance),
            worst: worst(&|x| x.off_mesh.abs()),
            mean: mean(&|x| x.off_mesh.abs()),
            worst_unlifted: worst(&|x| x.off_mesh_unlifted.abs()),
            mean_unlifted: mean(&|x| x.off_mesh_unlifted.abs()),
            worst_surface: worst(&|x| x.off_surface.abs()),
        }
    }
}

/// Which trees a station's rows are of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Trees {
    /// The impostor cards', their bases as the cards stand them.
    Cards,
    /// The trees drawn as meshes, their bases as the G-buffer's vertex shader stands them.
    Meshes,
}

/// At station `name`, its scenery drawn `scenery_radius` blocks round, with every piece of the
/// ground and the levels of detail on, each of the `trees` in the view whose base the ground's
/// mesh lies under. For the meshes, the vertex shader's lift is checked against the cards'
/// arithmetic for the same tree.
fn tree_rows(
    store: &Arc<RetailDatStore>,
    name: &str,
    scenery_radius: u32,
    trees: Trees,
) -> Vec<Row> {
    let station = remaster_frames::stations(store, &[name])
        .pop()
        .expect("the station");
    let (mut gpu, mut shot) =
        opened_with(store, &station, None, every_piece(), Some(scenery_radius));
    shot.scene
        .draw
        .set_remaster_lod(dereth_scene::world_scene::RemasterLod::ON, &mut gpu);
    for _ in 0..4 {
        shot.step_held(&mut gpu);
    }
    // The trees in the view: those behind the eye are drawn nowhere.
    let view = glam::Mat4::from_cols_array(
        &shot
            .scene
            .draw
            .remaster_capture(&mut gpu)
            .expect("the remaster drew")
            .view_projection,
    );
    let seen = |b: &[f32; 3]| {
        let c = view * glam::Vec4::new(b[0], b[1], b[2], 1.0);
        c.w > 0.0 && c.x.abs() <= c.w && c.y.abs() <= c.w
    };
    // Each base as it is drawn, and how far it is lifted.
    let bases: Vec<([f32; 3], f32)> = match trees {
        Trees::Cards => shot.scene.draw.remaster_impostor_bases(&mut gpu),
        Trees::Meshes => {
            let placed = shot.scene.draw.remaster_mesh_tree_bases(&mut gpu);
            let worst = placed
                .iter()
                .map(|(_, shader, cards)| (shader - cards).abs())
                .fold(0.0f32, f32::max);
            println!(
                "{name}: {} trees drawn as meshes; the vertex shader's lift and the cards' \
                 {worst:e} apart",
                placed.len()
            );
            assert!(
                worst < 1e-4,
                "{name}: the vertex shader lifts as the cards do"
            );
            placed
                .into_iter()
                .map(|(o, lift, _)| ([o[0], o[1], o[2] + lift], lift))
                .collect()
        }
    }
    .into_iter()
    .filter(|(b, _)| seen(b))
    .collect();
    let points: Vec<[f32; 2]> = bases.iter().map(|(b, _)| [b[0], b[1]]).collect();
    let mesh = shot
        .scene
        .draw
        .remaster_drawn_mesh_heights(&mut gpu, &points);
    let eye = shot.scene.camera.position;
    let probe: Vec<[f32; 3]> = bases
        .iter()
        .map(|(b, _)| {
            [
                b[0],
                b[1],
                ((b[0] - eye.x).powi(2) + (b[1] - eye.y).powi(2) + (b[2] - eye.z).powi(2)).sqrt(),
            ]
        })
        .collect();
    let surface = shot
        .scene
        .draw
        .remaster_drawn_heights(&mut gpu, None, &probe);
    let retail = shot.scene.draw.remaster_retail_heights(&mut gpu, &points);
    // A tree keeps the height retail stands it at over its ground (some are sunk into it): how
    // far it is lifted against how far the drawn ground rises over retail's under it.
    let rows: Vec<Row> = bases
        .iter()
        .zip(&mesh)
        .zip(&surface)
        .zip(&retail)
        .filter_map(|((((b, lift), m), f), r)| {
            let m = (*m)?;
            let d = ((b[0] - eye.x).powi(2) + (b[1] - eye.y).powi(2)).sqrt();
            Some(Row {
                distance: d,
                off_mesh: lift - (m - r),
                off_mesh_unlifted: r - m,
                off_surface: lift - (f - r),
            })
        })
        .collect();
    println!(
        "{name}: {} {trees:?} in the view, the mesh under {}",
        bases.len(),
        rows.len()
    );
    assert!(
        rows.len() * 10 >= bases.len() * 9,
        "{name}: the mesh lies under the trees"
    );
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(
        errors.is_empty(),
        "{name}: the validation layer: {errors:#?}"
    );
    rows
}

/// `rows` split into their nearest quarter and their farthest, each printed under its name.
fn bands(mut rows: Vec<Row>, names: [&str; 2]) -> (Band, Band) {
    rows.sort_by(|a, b| a.distance.total_cmp(&b.distance));
    let quarter = rows.len() / 4;
    let near = Band::of(&rows[..quarter]);
    let far = Band::of(&rows[rows.len() - quarter..]);
    for (name, b) in [(names[0], &near), (names[1], &far)] {
        println!(
            "{name}, {:.0} to {:.0} m: off the drawn mesh by {:.3} at most, {:.3} on average; \
             unlifted {:.3} at most, {:.3} on average; off the drawn surface by {:.4} at most",
            b.from, b.to, b.worst, b.mean, b.worst_unlifted, b.mean_unlifted, b.worst_surface
        );
    }
    (near, far)
}

/// **Far trees stand on the drawn ground, near the switch to impostors and far out**: looking out
/// over the land from the heights by Holtburg, and in the forest, with every piece of the ground
/// and the levels of detail on, each impostor card's tree stands as far above (or into) the
/// ground's own mesh drawn under it as retail stands it over retail's ground,
/// within a bound measured near the switch (the nearest quarter of the cards) and far out (the
/// farthest quarter); unlifted, standing on retail's ground, they stand further off it.
#[test]
fn far_trees_stand_on_the_drawn_ground_near_the_switch_and_far_out() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let mut rows: Vec<Row> = Vec::new();
    for (name, scenery_radius) in [("vista", 6), ("forest", 1)] {
        rows.extend(tree_rows(&store, name, scenery_radius, Trees::Cards));
    }
    assert!(rows.len() > 80, "cards measured: {}", rows.len());
    let (near, far) = bands(rows, ["near the switch", "far out"]);
    // Measured: near the switch 0.090 m at most off the mesh, far out 0.117 m; off the surface
    // the mesh's vertices are laid on, 0.077 m and 0.004 m.
    assert!(near.worst < 0.15, "near the switch: {}", near.worst);
    assert!(far.worst < 0.2, "far out: {}", far.worst);
    assert!(
        near.worst_surface < 0.1 && far.worst_surface < 0.1,
        "on the drawn surface: {} {}",
        near.worst_surface,
        far.worst_surface
    );
    assert!(
        near.mean < near.mean_unlifted && far.mean < far.mean_unlifted,
        "the lift brings them onto the ground"
    );
}

/// **Trees drawn as meshes stand on the drawn ground, near and just before the switch to
/// impostors**: at the vista and in the forest, with every piece of the ground and the levels of
/// detail on, the G-buffer's vertex shader lifts each tree drawn as a mesh exactly as the
/// impostor cards' arithmetic lifts it, and stands it as far above (or into) the ground's own mesh
/// drawn under it as retail stands it over retail's ground, within a bound measured near the eye
/// (the nearest quarter of them) and just before the switch (the farthest quarter); unlifted, they
/// stand further off it.
#[test]
fn mesh_trees_stand_on_the_drawn_ground_near_and_just_before_the_switch() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let mut rows: Vec<Row> = Vec::new();
    for (name, scenery_radius) in [("vista", 6), ("forest", 1)] {
        rows.extend(tree_rows(&store, name, scenery_radius, Trees::Meshes));
    }
    assert!(rows.len() > 80, "trees measured: {}", rows.len());
    let (near, before) = bands(rows, ["near the eye", "just before the switch"]);
    // Measured: near the eye 0.014 m at most off the mesh (its 1 cm laid under the surface among
    // it), just before the switch 0.077 m; off the surface 0.005 m and 0.006 m.
    assert!(near.worst < 0.05, "near the eye: {}", near.worst);
    assert!(
        before.worst < 0.15,
        "just before the switch: {}",
        before.worst
    );
    assert!(
        near.worst_surface < 0.1 && before.worst_surface < 0.1,
        "on the drawn surface: {} {}",
        near.worst_surface,
        before.worst_surface
    );
    assert!(
        near.mean < near.mean_unlifted && before.mean < before.mean_unlifted,
        "the lift brings them onto the ground"
    );
}

/// The lake's shallows where a player stood: the body a little under the water's surface, a
/// stone pillar standing in the water a meter off.
const SHALLOWS: &str = "dereth report 1; loc 0xA9B50015 [59.258690 104.685966 27.904999] \
                        0.723986 0.000000 0.000000 -0.689815; time 0.5; clock 306486600.0; \
                        weather clear";
/// The pillar's model and where it stands in the shallows' block.
const PILLAR: (u32, [f32; 3]) = (0x0200_03CA, [60.11, 103.55, 27.9]);
/// A lily pad's model, as the lake lays them on its surface farther along, set on the water
/// beside the pillar.
const LILY_PAD: (u32, [f32; 3]) = (0x0200_03D2, [61.0, 105.5, 28.0]);
/// The same lake out where it is deep, the body standing in it.
const DEEP: &str = "dereth report 1; loc 0xA9B5001E [85.0 125.0 27.1] 1.0 0.0 0.0 0.0; time 0.5; \
                    clock 306486600.0; weather clear";
/// A boardwalk over the same lake, a little over the water, a body standing on its deck.
const BOARDWALK: &str = "dereth report 1; loc 0xABB50007 [8.40 147.43 28.35] 0.723986 0.000000 \
                         0.000000 -0.689815; time 0.5; clock 306486600.0; weather clear";
/// One of the boardwalk's planks, and one of the posts it stands on in the water: their models
/// and where they stand in the boardwalk's block.
const PLANK: (u32, [f32; 3]) = (0x0100_104C, [8.40, 147.43, 28.077]);
const DOCK_POST: (u32, [f32; 3]) = (0x0200_03B4, [0.2, 136.9, 27.55]);

/// What [`wading_at`] found.
#[derive(Debug)]
struct Wading {
    /// Where physics puts the body (its cell's frame).
    placed: dereth_primitives::Position,
    /// Where the scene draws its feet, and the drawn ground under them (render space).
    feet: f32,
    ground: f32,
    /// How far each placement asked for is drawn above where it stands, near and far, and the
    /// drawn ground under it.
    lifts: Vec<([f32; 2], f32)>,
}

/// The place `line` reports with every piece of the ground drawn, the water drawn where `water`,
/// the body standing a second; and the placements `things` (model, and where it stands
/// block-local in the body's block).
fn wading_at(
    store: &Arc<RetailDatStore>,
    line: &str,
    water: bool,
    things: &[(u32, [f32; 3])],
) -> Wading {
    let report = VisualReport::parse(line).expect("the report reads");
    let station = report_station(&report, Some(3));
    let (mut gpu, mut shot) = opened(store, &station, Some(&report), every_piece());
    let w = shot.scene.draw.remaster_water();
    shot.scene.draw.set_remaster_water(
        dereth_scene::world_scene::RemasterWater { on: water, ..w },
        &mut gpu,
    );
    for _ in 0..30 {
        shot.step_by(&mut gpu, 1.0 / 30.0);
    }
    shot.step_held(&mut gpu);
    let c = shot.scene.character.as_ref().expect("a body");
    let (placed, body) = (c.position(), c.render_frame().origin);
    let feet = body.z + shot.scene.remaster_shift_at(body);
    let ground = shot
        .scene
        .remaster_ground_at(body.x, body.y)
        .expect("the drawn ground");
    let local = placed.frame.origin;
    let lifts = things
        .iter()
        .map(|(_, at)| {
            let thing = dereth_primitives::Vec3::new(
                body.x + at[0] - local.x,
                body.y + at[1] - local.y,
                at[2],
            );
            let lifts = shot.scene.remaster_lifts_at(thing);
            let under = shot
                .scene
                .remaster_ground_at(thing.x, thing.y)
                .expect("the drawn ground");
            (lifts, under)
        })
        .collect();
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    Wading {
        placed,
        feet,
        ground,
        lifts,
    }
}

/// **What stands in the water is drawn where physics and retail put it, and a body's feet meet
/// the bed drawn under it**: in the lake's shallows, where physics stands a body a hand's breadth
/// under the water's surface, and out where it is deep, where physics sinks it 0.9 m, the body is
/// drawn where physics puts it, whether the water is drawn or not, and the bed is drawn under its
/// feet; the stone pillar beside it and a lily pad on the water are drawn where retail places
/// them. On a boardwalk over the lake a body, the boardwalk's planks and the posts it stands on
/// are drawn where they stand.
#[test]
fn what_stands_in_water_is_drawn_where_it_stands_and_a_bodys_feet_meet_the_bed() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let level = 28.0;
    let shallows = wading_at(&store, SHALLOWS, false, &[PILLAR, LILY_PAD]);
    let deep = wading_at(&store, DEEP, false, &[]);
    let deep_water = wading_at(&store, DEEP, true, &[]);
    let deck = wading_at(&store, BOARDWALK, true, &[PLANK, DOCK_POST]);
    println!(
        "shallows: {shallows:?}\ndeep: {deep:?}\ndeep, the water drawn: {deep_water:?}\n\
         boardwalk: {deck:?}"
    );
    assert_eq!(
        deep.placed, deep_water.placed,
        "physics places the body the same"
    );
    let z = shallows.placed.frame.origin.z;
    assert!(
        z > level - 0.2 && z < level - 0.05,
        "physics stands the body a little under the surface: {z}"
    );
    for (body, what, sunk) in [
        (&shallows, "in the shallows", 0.1),
        (&deep, "out in the lake", 0.9),
        (&deep_water, "out in the lake, the water drawn", 0.9),
    ] {
        let z = body.placed.frame.origin.z;
        assert!(
            (z - (level - sunk)).abs() < 0.02,
            "{what}, physics sinks the body {sunk} m: {z}"
        );
        assert!(
            (body.feet - z).abs() < 0.01,
            "{what}, the feet drawn where physics puts them: {} for {z}",
            body.feet
        );
        assert!(
            (body.ground - body.feet).abs() < 0.03,
            "{what}, the bed drawn under the feet: {} for {}",
            body.ground,
            body.feet
        );
    }
    for ((lift, under), what) in shallows.lifts.iter().zip(["pillar", "lily pad"]) {
        assert!(
            lift.iter().all(|l| l.abs() < 0.01),
            "the {what} drawn where retail places it: {lift:?} over {under}"
        );
    }
    let on_deck = deck.placed.frame.origin.z;
    assert!(
        on_deck > level + 0.2,
        "the body stands on the deck: {on_deck}"
    );
    assert!(
        deck.feet > on_deck - 0.02,
        "a body on the deck drawn on it: {} for {on_deck}",
        deck.feet
    );
    for ((lift, _), what) in deck.lifts.iter().zip(["plank", "post"]) {
        assert!(
            lift.iter().all(|l| *l > -0.02),
            "the boardwalk's {what} drawn where it stands: {lift:?}"
        );
    }
}

/// A human (its setup and motion table) and the sword it holds in its right hand, created as a
/// server would create them.
const HOLDER: (u32, u32, u32) = (0x8300_07A0, 0x0200_0001, 0x0900_0001);
const HELD: (u32, u32) = (0x8300_07A1, 0x0200_1713);
const RIGHT_HAND: u32 = 1;

/// **What a body holds is drawn in its hand while the body stands in the water**: a human
/// standing in the lake's shallows, holding a sword in its right hand high over the water, is
/// drawn where physics stands it, the bed drawn under the water there, and the sword is drawn
/// lifted as far as the body, so it stays in the hand.
#[test]
fn what_a_body_holds_stays_in_its_hand_while_it_stands_in_the_water() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let report = VisualReport::parse(SHALLOWS).expect("the report reads");
    let station = report_station(&report, Some(3));
    let (mut gpu, mut shot) = opened(&store, &station, Some(&report), every_piece());
    shot.read_data_files();
    shot.create_object(
        HOLDER.0,
        (HOLDER.1, Some(HOLDER.2)),
        0xA9B5,
        (60.5, 106.0),
        0.0,
    );
    shot.step_held(&mut gpu);
    shot.create_held(HELD.0, HELD.1, HOLDER.0, RIGHT_HAND);
    for _ in 0..30 {
        shot.step_by(&mut gpu, 1.0 / 30.0);
    }
    let holder = dereth_primitives::ObjectId(HOLDER.0);
    let held = dereth_primitives::ObjectId(HELD.0);
    let at = |id, what: &str| {
        shot.scene
            .objects
            .get(&id)
            .map(|o| (o.drawn, o.frame.origin))
            .unwrap_or_else(|| panic!("the {what} is in the world"))
    };
    let ((holder_drawn, feet), (held_drawn, hand)) = (at(holder, "holder"), at(held, "sword"));
    let lifts = (
        shot.scene.remaster_object_lift(holder).expect("the holder"),
        shot.scene.remaster_object_lift(held).expect("the sword"),
    );
    let ground = shot
        .scene
        .remaster_ground_at(feet.x, feet.y)
        .expect("the drawn ground");
    drop(shot);
    drop(gpu);
    println!(
        "the holder at {feet:?} lifted {}, the drawn ground under it at {ground}; the sword at \
         {hand:?} lifted {}",
        lifts.0, lifts.1
    );
    assert!(holder_drawn && held_drawn, "both are drawn");
    assert!(
        hand.z > 28.25,
        "the sword is held over the water: {}",
        hand.z
    );
    assert!(
        feet.z < 27.99 && ground < 27.99 && lifts.0.abs() < 0.01,
        "the holder is drawn where physics stands it in the water, over the bed: lifted {} from \
         {} over {ground}",
        lifts.0,
        feet.z
    );
    assert!(
        (lifts.1 - lifts.0).abs() < 1e-4,
        "the sword is drawn lifted with its holder: {} against {}",
        lifts.1,
        lifts.0
    );
}
