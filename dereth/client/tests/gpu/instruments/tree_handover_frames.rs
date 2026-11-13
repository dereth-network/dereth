//! The remaster's trees as the viewer walks across a block line, where the blocks whose own
//! scenery stands in the world move with the viewer: a step at a time along the body's heading,
//! a frame drawn and held at each step, the frames written as one strip down the page. The camera
//! either follows the body, as the player's does, or stays where the report says it stood, so
//! a step changes only what the blocks moving changes. For each step it prints where the body
//! stands, how many pixels changed plainly since the step before, and what the levels of detail, the
//! impostors and the far forest drew.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware device.
//!
//! - `DERETH_ART_PACK`: the art packs drawn, joined by `;`, bottom first (none without it).
//! - `DERETH_REMASTER_CAPTURE_DIR`: where the strips go, `handover-<camera>-<tag>.png`, with
//!   what each step changed, `handover-<camera>-<tag>-changed.png` (the frame dimmed, the pixels
//!   it changed red), and the still, `handover-still-<tag>.png` (none are written without it).
//! - `DERETH_HANDOVER_PLACE`: a report line saying where the body and the camera stand (default
//!   a rainy hill below a band of pines, the body a step from a block line).
//! - `DERETH_HANDOVER_STEPS`: `from,to,step`, meters along the body's heading from where it
//!   stands (default `-3,5,1`).
//! - `DERETH_HANDOVER_CAMERAS`: a comma list of `follow` and `held` (default both).
//! - `DERETH_HANDOVER_STILL`: `WIDTHxHEIGHT` of a still drawn where the report stands, after the
//!   strips (none without it).
//! - `DERETH_HANDOVER_SCENERY`: how many rings of blocks round the viewer's grow their scenery
//!   (default the client's).
//! - `DERETH_REMASTER_LOOK_BOXES`, `DERETH_REMASTER_LOOK_TAG`, `DERETH_REMASTER_SIZE` as the
//!   look's instruments take them; the art pack's box is set over them where a pack is drawn.

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_client_runtime::report::VisualReport;
use dereth_dat::RetailDatStore;
use dereth_primitives::num::math;
use dereth_primitives::LandblockId;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::art_pack::ArtPack;

use super::remaster_look_frames::EVERY_BOX;
use super::stations::{self, capture, report_station, Place, Shot};

/// A rainy hill below a band of pines, the body a quarter of a meter east of a block line and
/// facing west of north across it, the camera behind it.
const HILL: &str = "dereth report 1; loc 0xB3B50006 [0.240147 121.806526 98.005005] -0.936959 0 \
                    0 -0.349440; camera 0xB3B5 [4.164169 117.277565 99.20517] yaw -5.569238 \
                    pitch 0.05000011 cell 0xB3B50005; time 0.523; weather rain";

/// The frames drawn with the clock held at each step before its frame is taken: the blocks the
/// step brings are built, and the upscaler's history settles.
const SETTLE: usize = 12;

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// Where the body stands `metres` along its heading from where the report stands it: the block
/// the report names, block-local meters (past its edge in the block there), and the ground under
/// it.
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
        .ground_height(LandblockId(next), x - bx * 192.0, y - by * 192.0)
        .unwrap_or(0.0);
    (x, y, ground)
}

/// Step `shot` one frame with its clock held, adding to `made` the trees whose impostor pictures
/// it made and the device's time making them, milliseconds, then the far pictures it made and the
/// device's time making those.
fn held(shot: &mut Shot, gpu: &mut Gpu, made: &mut [(u32, f64); 2]) {
    shot.step_held(gpu);
    let draw = &shot.scene.draw;
    made[0].0 += draw.remaster_impostor_stats(gpu).map_or(0, |s| s.baked);
    made[1].0 += draw.remaster_forest_stats(gpu).map_or(0, |s| s.made_now);
    let timings = draw.remaster_timings(gpu);
    for (k, label) in ["impostor pictures", "far pictures"]
        .into_iter()
        .enumerate()
    {
        made[k].1 += timings
            .iter()
            .filter(|t| t.0 == label)
            .map(|t| t.1)
            .sum::<f64>();
    }
}

/// `b` dimmed, with the pixels that differ from `a`'s as [`changed`] counts them in red.
fn changes(a: &[u8], b: &[u8]) -> Vec<u8> {
    a.as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0)
        .flat_map(|(x, y)| {
            if x.iter().zip(y.iter()).any(|(p, q)| p.abs_diff(*q) > 32) {
                [255, 0, 0, 255]
            } else {
                [y[0] / 4, y[1] / 4, y[2] / 4, 255]
            }
        })
        .collect()
}

/// How many pixels of `b` differ from `a`'s by more than an eighth in any channel: what a step
/// changed that a held view's last few bits of noise do not.
fn changed(a: &[u8], b: &[u8]) -> usize {
    a.as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0)
        .filter(|(x, y)| x.iter().zip(y.iter()).any(|(p, q)| p.abs_diff(*q) > 32))
        .count()
}

/// One step's frame and what was drawn for it.
struct Step {
    rgba: Vec<u8>,
    line: String,
}

/// The report's place opened on a device of `size` with `packs` and `boxes`, the body walked
/// along `steps`, the camera following it (`follow`) or held where the report says.
fn walk(
    store: &Arc<RetailDatStore>,
    report: &VisualReport,
    pack: Option<&Arc<ArtPack>>,
    boxes: &str,
    (size, steps, follow, scenery): ((u32, u32), &[f32], bool, Option<u32>),
) -> Vec<Step> {
    let station = report_station(report, None);
    let Place::Body { cell, origin, yaw } = station.place else {
        panic!("the report names where the body stood");
    };
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: the top 16 bits of a cell id
    let block = (cell >> 16) as u16;
    let yaw = yaw.to_radians();
    let camera = report
        .camera
        .expect("the report names where the camera stood");
    let cfg = DeviceConfig {
        width: size.0,
        height: size.1,
        remaster: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let mut shot = Shot::open_with(store, &mut gpu, &station, false, scenery);
    shot.apply_report(report);
    shot.poll(&mut gpu);
    shot.scene.draw.set_remaster_pack(pack.cloned(), None);
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(boxes).expect("the boxes read");
    shot.poll(&mut gpu);
    if follow {
        // Behind the body as the report's camera stood behind it: as far back, as far up, the
        // same pitch and heading.
        let (dx, dy) = (camera.origin.x - origin.x, camera.origin.y - origin.y);
        shot.held = None;
        shot.follow = Some([
            math::hypotf(dx, dy),
            camera.origin.z - origin.z,
            camera.pitch,
            camera.yaw,
        ]);
    }
    let mut out: Vec<Step> = Vec::with_capacity(steps.len());
    // The impostors' pictures and the far pictures made over every frame drawn: how many, and
    // the device's time.
    let mut made = [(0u32, 0.0f64); 2];
    for (k, metres) in steps.iter().enumerate() {
        let (x, y, ground) = along(store, (block, origin.x, origin.y, yaw), *metres);
        shot.place_body(block, (x, y), ground, yaw);
        if k == 0 {
            held(&mut shot, &mut gpu, &mut made);
            shot.scene.wait_for_remaster_ground();
            for _ in 0..SETTLE * 2 {
                held(&mut shot, &mut gpu, &mut made);
            }
        }
        for _ in 0..SETTLE {
            held(&mut shot, &mut gpu, &mut made);
        }
        let rgba = capture(&mut gpu);
        let changed = out.last().map_or(0, |s| changed(&s.rgba, &rgba));
        let draw = &shot.scene.draw;
        let lod = draw.remaster_lod_stats(&mut gpu).unwrap_or_default();
        let imp = draw.remaster_impostor_stats(&mut gpu).unwrap_or_default();
        let far = draw.remaster_forest_stats(&mut gpu).unwrap_or_default();
        let ms: f64 = draw.remaster_timings(&mut gpu).iter().map(|t| t.1).sum();
        let line = format!(
            "{metres:+5.1} m  block {:04X} x {x:7.2} y {y:7.2}  changed {changed:8}  parts {} \
             fading {} | impostor cards {} trees {} pages {} | far standing {} pictures {} \
             ({} MB) made {} ({} MiB) | {ms:.2} ms | impostor pictures made {} in {:.2} ms, \
             far pictures {} in {:.2} ms",
            shot.scene.viewer_block().map_or(0, |(bx, by)| {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                // LINT-OK: block indices, 0..=0xFE
                let id = ((bx as u16) << 8) | by as u16;
                id
            }),
            lod.parts,
            lod.fading,
            imp.cards,
            imp.trees,
            imp.pages,
            far.standing,
            far.pictures,
            far.picture_bytes >> 20,
            far.made,
            far.made_bytes >> 20,
            made[0].0,
            made[0].1,
            made[1].0,
            made[1].1,
        );
        println!("{line}");
        out.push(Step { rgba, line });
    }
    drop(shot);
    drop(gpu);
    out
}

/// The strips across the block line, each camera's, and the still where the report stands.
#[test]
#[ignore = "instrument: writes the trees across a block line as strips; run with --ignored"]
fn capture_the_trees_across_a_block_line() {
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
        VisualReport::parse(&env("DERETH_HANDOVER_PLACE").unwrap_or_else(|| HILL.to_owned()))
            .expect("the place's report reads");
    let mut boxes = env("DERETH_REMASTER_LOOK_BOXES").unwrap_or_else(|| EVERY_BOX.to_owned());
    if pack.is_some() {
        boxes.push_str(",RemasterArtPack=1");
    }
    let [from, to, step]: [f32; 3] = env("DERETH_HANDOVER_STEPS")
        .and_then(|v| {
            let n: Vec<f32> = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
            n.try_into().ok()
        })
        .unwrap_or([-3.0, 5.0, 1.0]);
    let mut steps = Vec::new();
    let mut at = from;
    while at <= to + 1e-3 {
        steps.push(at);
        at += step.max(0.05);
    }
    let scenery = env("DERETH_HANDOVER_SCENERY").and_then(|v| v.parse().ok());
    let cameras = env("DERETH_HANDOVER_CAMERAS").unwrap_or_else(|| "follow,held".to_owned());
    for camera in cameras.split(',').map(str::trim) {
        let follow = camera == "follow";
        println!("{camera}:");
        let frames = walk(
            &store,
            &report,
            pack.as_ref(),
            &boxes,
            (size, &steps, follow, scenery),
        );
        if let Some(dir) = dir.as_ref() {
            let strip: Vec<u8> = frames.iter().flat_map(|f| f.rgba.iter().copied()).collect();
            let height = size.1 * u32::try_from(frames.len()).unwrap_or(0);
            stations::write_png(
                &dir.join(format!("handover-{camera}-{tag}.png")),
                size.0,
                height,
                &strip,
            );
            let mut marked: Vec<u8> = frames[0].rgba.iter().map(|c| c / 4).collect();
            for pair in frames.windows(2) {
                marked.extend(changes(&pair[0].rgba, &pair[1].rgba));
            }
            stations::write_png(
                &dir.join(format!("handover-{camera}-{tag}-changed.png")),
                size.0,
                height,
                &marked,
            );
            let lines: Vec<&str> = frames.iter().map(|f| f.line.as_str()).collect();
            std::fs::write(
                dir.join(format!("handover-{camera}-{tag}.txt")),
                lines.join("\n") + "\n",
            )
            .expect("the log writes");
        }
    }
    if let Some((w, h)) = env("DERETH_HANDOVER_STILL").and_then(|s| {
        let (w, h) = s.trim().split_once('x')?;
        Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?))
    }) {
        println!("still:");
        let frames = walk(
            &store,
            &report,
            pack.as_ref(),
            &boxes,
            ((w, h), &[0.0], false, scenery),
        );
        if let (Some(dir), Some(f)) = (dir.as_ref(), frames.first()) {
            stations::write_png(
                &dir.join(format!("handover-still-{tag}.png")),
                w,
                h,
                &f.rgba,
            );
        }
    }
}
