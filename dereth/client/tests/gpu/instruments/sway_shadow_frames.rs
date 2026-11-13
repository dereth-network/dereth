//! The remaster's sun shadows while an art pack's trees and ground cover sway: a view held still
//! while the clock runs, a frame every tenth of a second written as one strip down the page, with
//! the sway on and off. For each run it prints how many of the sun's cascades were drawn again
//! each frame, and, at the last frame, how much of the sun's share was stale: the share of the
//! pixels whose sun visibility changes once every cascade is drawn again at the same moment.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware device.
//!
//! - `DERETH_REMASTER_CAPTURE_DIR`: where the strips go, `sway-on-<tag>.png` and
//!   `sway-off-<tag>.png` (none are written without it).
//! - `DERETH_ART_PACK`: the art pack drawn, or several joined by `;`, bottom first (required:
//!   only a pack's models sway).
//! - `DERETH_SWAY_PLACE`: a report line saying where the camera stands (default a grassy hill
//!   by Holtburg's lake, mid-morning).
//! - `DERETH_SWAY_HOUR`: the hour of a fair day, a share of the day (default 0.463).
//! - `DERETH_SWAY_FRAMES`: the frames stepped at thirty a second (default 24), every third
//!   written.
//! - `DERETH_REMASTER_LOOK_BOXES`, `DERETH_REMASTER_LOOK_TAG`, `DERETH_REMASTER_SIZE` as the
//!   look's instruments take them; the sway box is set on and off over them.

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_client_runtime::report::VisualReport;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::art_pack::ArtPack;

use super::stations::{self, capture, report_station, Shot, Station};

/// A grassy hill by Holtburg's lake, mid-morning, its trees and cover in view.
const HILL: &str = "dereth report 1; loc 0xA9B50013 [63.063522 58.064453 29.494415] -0.073621 0 \
                    0 -0.997286; camera 0xA9B5 [65.26223 72.87509 30.095005] yaw -3.2889686 \
                    pitch 0.05999962 cell 0xA9B50014; time 0.463; weather auto";

/// The boxes drawn by default: the shadows, the screen's occlusion and the pack's landscape.
const BOXES: &str = "Remaster=1,RemasterShadows=1,RemasterSky=1,RemasterFog=1,\
                     RemasterOcclusion=1,RemasterBloom=1,RemasterSmoothing=1,RemasterBounce=1,\
                     RemasterArtPack=1,RemasterGrade=1,RemasterAir=1,RemasterGrass=1,\
                     RemasterFlowers=1";

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// One run: its strip, the cascades drawn again each frame, and the stale share at the last.
pub(crate) struct Run {
    pub strip: Vec<u8>,
    pub frames: usize,
    pub redrawn: Vec<u32>,
    pub stale: f64,
    /// The shadows' time on the device a frame, and the whole frame's, milliseconds on the mean.
    pub shadow_ms: f64,
    pub frame_ms: f64,
}

/// `station` (moved to `report`'s camera, where there is one) drawn with `pack` and `boxes`, the
/// sway on or off as `sway` says, from a device of its own `size`: held still until the ground is
/// laid, then `frames` frames with the clock running at thirty a second, every third written.
pub(crate) fn run(
    store: &Arc<dereth_dat::RetailDatStore>,
    (station, report): (&Station, Option<&VisualReport>),
    pack: &Arc<ArtPack>,
    boxes: &str,
    sway: bool,
    (size, frames): ((u32, u32), usize),
) -> Run {
    let cfg = DeviceConfig {
        width: size.0,
        height: size.1,
        remaster: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let mut shot = Shot::open(store, &mut gpu, station);
    if let Some(r) = report {
        shot.apply_report(r);
    }
    shot.poll(&mut gpu);
    shot.scene
        .draw
        .set_remaster_pack(Some(Arc::clone(pack)), None);
    let boxes = format!("{boxes},RemasterSway={}", u8::from(sway));
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(&boxes).expect("the boxes read");
    shot.poll(&mut gpu);
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    for _ in 0..24 {
        shot.step_held(&mut gpu);
    }
    let (mut strip, mut written, mut redrawn) = (Vec::new(), 0, Vec::new());
    let (mut shadow_ms, mut frame_ms) = (0.0, 0.0);
    for k in 0..frames {
        shot.step_by(&mut gpu, 1.0 / 30.0);
        let rgba = capture(&mut gpu);
        let t = shot.scene.draw.remaster_timings(&mut gpu);
        frame_ms += t.iter().map(|p| p.1).sum::<f64>();
        shadow_ms += t
            .iter()
            .filter(|p| p.0.contains("shadow"))
            .map(|p| p.1)
            .sum::<f64>();
        if let Some(c) = shot.scene.draw.remaster_counts(&mut gpu) {
            redrawn.push(c.shadows.last_redrawn);
        }
        if k % 3 == 2 {
            strip.extend(rgba);
            written += 1;
        }
    }
    let stale = stale_share(&mut shot, &mut gpu);
    drop(shot);
    drop(gpu);
    #[allow(clippy::cast_precision_loss)] // a frame count
    let n = frames.max(1) as f64;
    Run {
        strip,
        frames: written,
        redrawn,
        stale,
        shadow_ms: shadow_ms / n,
        frame_ms: frame_ms / n,
    }
}

/// The share of the pixels the sun lights whose sun visibility changes by more than a quarter
/// once every cascade is drawn again at the same moment: the cascades' maps made again by
/// changing their size and back, the clock held.
fn stale_share(shot: &mut Shot, gpu: &mut Gpu) -> f64 {
    let was = shot
        .scene
        .draw
        .remaster_capture(gpu)
        .and_then(|c| c.sun_visibility)
        .unwrap_or_default();
    let s = dereth_scene::world_scene::RemasterShadows {
        on: true,
        ..Default::default()
    };
    shot.scene.draw.set_remaster_shadows(
        dereth_scene::world_scene::RemasterShadows {
            size: s.size / 2,
            ..s
        },
        gpu,
    );
    shot.step_held(gpu);
    shot.scene.draw.set_remaster_shadows(s, gpu);
    shot.step_held(gpu);
    let now = shot
        .scene
        .draw
        .remaster_capture(gpu)
        .and_then(|c| c.sun_visibility)
        .unwrap_or_default();
    let n = was.len().min(now.len()).max(1);
    let changed = was
        .iter()
        .zip(&now)
        .filter(|(a, b)| a.abs_diff(**b) > 64)
        .count();
    #[allow(clippy::cast_precision_loss)] // pixel counts
    let share = changed as f64 / n as f64;
    share
}

/// The view with the sway on and off, each a strip, and what moved in each.
#[test]
#[ignore = "instrument: writes the sun's shadows while the trees sway; run with --ignored"]
fn capture_the_shadows_while_the_trees_sway() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let list = env("DERETH_ART_PACK").expect("DERETH_ART_PACK names the packs");
    let paths: Vec<std::path::PathBuf> = list.split(';').map(std::path::PathBuf::from).collect();
    let pack = Arc::new(ArtPack::open_stack(&paths).expect("the packs open"));
    let dir = env("DERETH_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    let tag = env("DERETH_REMASTER_LOOK_TAG").unwrap_or_else(|| "remaster".to_owned());
    let size = super::remaster_frames::size();
    let report = VisualReport::parse(&env("DERETH_SWAY_PLACE").unwrap_or_else(|| HILL.to_owned()))
        .expect("the place's report reads");
    let mut station = report_station(&report, Some(4));
    station.clock = stations::Clock::Sunny(
        env("DERETH_SWAY_HOUR")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.463),
    );
    let boxes = env("DERETH_REMASTER_LOOK_BOXES").unwrap_or_else(|| BOXES.to_owned());
    let frames: usize = env("DERETH_SWAY_FRAMES")
        .and_then(|v| v.parse().ok())
        .unwrap_or(24);
    for sway in [true, false] {
        let r = run(
            &store,
            (&station, Some(&report)),
            &pack,
            &boxes,
            sway,
            (size, frames),
        );
        let name = if sway { "sway-on" } else { "sway-off" };
        println!(
            "{name}: cascades drawn again each frame {:?}; stale at the last frame: {:.4} of the \
             pixels; the shadows {:.3} ms of {:.3} ms on the device a frame",
            r.redrawn, r.stale, r.shadow_ms, r.frame_ms
        );
        if let Some(dir) = dir.as_ref() {
            let height = size.1 * u32::try_from(r.frames).unwrap_or(0);
            stations::write_png(
                &dir.join(format!("{name}-{tag}.png")),
                size.0,
                height,
                &r.strip,
            );
        }
    }
}
