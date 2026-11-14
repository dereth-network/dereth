//! The remaster's air at a report's place, drawn each of several ways on one device held still: the
//! frames written, how many pixels change over frames held still, what the air costs the device,
//! and, for the first way, each pixel's class, kind and depth, for finding what the air leaves out
//! or what stands in it that should not.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.
//!
//! - `DERETH_AIR_PLACE`: the report line saying where the camera stands (required). A report with a
//!   time of day and no clock is drawn on a sunny day at that time.
//! - `DERETH_AIR_WAYS`: the ways, `tag=boxes` entries joined by `#`, the boxes as `--fidelity`
//!   takes them (required).
//! - `DERETH_AIR_RADIUS`: the land held, blocks each way (default the report's draw distance).
//! - `DERETH_AIR_STILL`: frames drawn held still after each way's picture, each against the last
//!   (default 0).
//! - `DERETH_ART_PACK`: an art pack drawn, all of it.
//! - `DERETH_REMASTER_SIZE`: `WIDTHxHEIGHT` (default 960x540).
//! - `DERETH_REMASTER_CAPTURE_DIR`: where the pictures go (none are written without it):
//!   `<tag>.png` for each way, and `parts.bin`: the render size (two little-endian words), then
//!   each pixel's class, then each one's kind, then each one's depth (four bytes, little-endian).

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_client_runtime::report::VisualReport;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::art_pack::ArtPack;
use dereth_scene::world_scene::RemasterPacks;

use super::stations::{self, report_station, Shot};

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// The pixels whose largest channel differs by more than `levels` between two frames.
fn apart(a: &[u8], b: &[u8], levels: u8) -> usize {
    a.as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0)
        .filter(|(p, q)| (0..3).any(|k| p[k].abs_diff(q[k]) > levels))
        .count()
}

/// The pixels' classes, kinds and depths, as `parts.bin` holds them.
fn parts_bytes(c: &dereth_scene::world_scene::RemasterCapture) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend(c.render_size.0.to_le_bytes());
    bytes.extend(c.render_size.1.to_le_bytes());
    bytes.extend(&c.class);
    bytes.extend(&c.kind);
    for v in &c.depth {
        bytes.extend(v.to_le_bytes());
    }
    bytes
}

#[test]
#[ignore = "instrument: draws a report's place each of several ways and writes pictures; run with --ignored"]
fn draw_the_air_at_a_report_place() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let size = super::remaster_frames::size();
    let line = env("DERETH_AIR_PLACE").expect("DERETH_AIR_PLACE names the place");
    let ways: Vec<(String, String)> = env("DERETH_AIR_WAYS")
        .expect("DERETH_AIR_WAYS names the ways")
        .split('#')
        .filter_map(|e| e.split_once('='))
        .map(|(n, b)| (n.trim().to_owned(), b.trim().to_owned()))
        .collect();
    let still: usize = env("DERETH_AIR_STILL").map_or(0, |v| v.trim().parse().expect("a count"));
    let radius: Option<u32> = env("DERETH_AIR_RADIUS").map(|v| v.trim().parse().expect("blocks"));
    let dir = env("DERETH_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    if let Some(d) = dir.as_ref() {
        std::fs::create_dir_all(d).expect("the pictures' folder");
    }
    let report = VisualReport::parse(&line).expect("the place's report reads");
    let mut station = report_station(&report, radius);
    if let Some(t) = report.time.as_ref().filter(|t| t.clock == 0.0) {
        station.clock = stations::Clock::Sunny(t.fraction);
    }
    #[allow(unused_mut)]
    let mut cfg = DeviceConfig {
        width: size.0,
        height: size.1,
        remaster: true,
        ..DeviceConfig::default()
    };
    #[cfg(all(feature = "dlss", windows))]
    {
        cfg.remaster_extensions = dereth_scene::world_scene::remaster_device_extensions();
    }
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let mut shot = Shot::open(&store, &mut gpu, &station);
    shot.apply_report(&report);
    shot.poll(&mut gpu);
    let pack = env("DERETH_ART_PACK").map(std::path::PathBuf::from);
    if let Some(path) = pack.as_ref() {
        let p = Arc::new(ArtPack::open_path(path).expect("the pack opens"));
        let folder = path.is_dir().then(|| path.clone());
        shot.scene.draw.set_remaster_pack(Some(p), folder);
    }
    for (k, (tag, boxes)) in ways.iter().enumerate() {
        shot.scene.draw.cfg.render.fidelity =
            FidelityPreferences::parse_switch(boxes).expect("the boxes read");
        shot.poll(&mut gpu);
        if !shot.scene.draw.remaster_on() {
            shot.scene
                .draw
                .set_remaster(true, &store, &mut gpu)
                .expect("the remaster starts");
            shot.poll(&mut gpu);
        }
        if pack.is_some() {
            shot.scene
                .draw
                .set_remaster_packs(RemasterPacks::all(), &mut gpu);
        }
        shot.step_held(&mut gpu);
        shot.scene.wait_for_remaster_ground();
        // The far trees worked out, and every far tree's pictures on the device, a few a frame.
        shot.step_held(&mut gpu);
        shot.scene.wait_for_remaster_far_trees();
        for _ in 0..400 {
            shot.step_held(&mut gpu);
            shot.scene.wait_for_remaster_far_trees();
            let s = shot.scene.draw.remaster_forest_stats(&mut gpu);
            if s.is_none_or(|s| s.wanted > 0 && s.pictures >= s.wanted) {
                break;
            }
        }
        // The smoothing's or the upscaler's frames gathered.
        for _ in 0..48 {
            shot.step_held(&mut gpu);
        }
        let rgba = stations::capture(&mut gpu);
        println!(
            "{tag}: upscaler {}",
            dereth_scene::world_scene::remaster_upscaler_drawn()
        );
        let mut last = rgba.clone();
        let mut air_ms = Vec::new();
        for f in 0..still {
            shot.step_held(&mut gpu);
            let now = stations::capture(&mut gpu);
            println!(
                "  held {f:3}: {:8} px more than 1 level apart, {:8} more than 4",
                apart(&last, &now, 1),
                apart(&last, &now, 4)
            );
            last = now;
            air_ms.extend(
                shot.scene
                    .draw
                    .remaster_timings(&mut gpu)
                    .into_iter()
                    .filter(|t| t.0 == "air")
                    .map(|t| t.1),
            );
        }
        if !air_ms.is_empty() {
            #[allow(clippy::cast_precision_loss)] // a few frames
            let mean = air_ms.iter().sum::<f64>() / air_ms.len() as f64;
            println!("  the air: {mean:.3} ms on the device");
        }
        let passes: Vec<String> = shot
            .scene
            .draw
            .remaster_timings(&mut gpu)
            .iter()
            .map(|(l, ms)| format!("{l} {ms:.2}"))
            .collect();
        println!("  passes: {}", passes.join(", "));
        if let Some(d) = dir.as_ref() {
            stations::write_png(&d.join(format!("{tag}.png")), size.0, size.1, &rgba);
            if k == 0 {
                let c = shot
                    .scene
                    .draw
                    .remaster_capture(&mut gpu)
                    .expect("the remaster drew");
                std::fs::write(d.join("parts.bin"), parts_bytes(&c)).expect("the parts written");
            }
        }
    }
    shot.scene
        .draw
        .set_remaster(false, &store, &mut gpu)
        .expect("the remaster stops");
}
