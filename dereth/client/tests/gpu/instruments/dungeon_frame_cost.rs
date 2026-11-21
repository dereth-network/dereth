//! What a frame costs on the processor deep in a big dungeon: a body standing in a cell of a
//! block with thousands of cells, drawn by Retail Look and by the remaster with every box on,
//! with the camera held still and with it turning round the body a little each frame. It asserts
//! nothing; it prints the mean, median and 95th percentile of the whole step and of its draw.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware device.
//!
//! - `DERETH_DUNGEON_COST_CELL`: the cell the body stands in (default `0x00D1030A`, in the
//!   retail dungeon with the most cells drawn).
//! - `DERETH_DUNGEON_COST_FRAMES`: the frames timed (default 600), after 300 to settle.
//! - `DERETH_REMASTER_SIZE`: `WIDTHxHEIGHT` (default 960x540).

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::time::Instant;

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_primitives::Quat;
use dereth_render::device::{DeviceConfig, Gpu};

use super::remaster_frames;
use super::remaster_look_frames::EVERY_BOX;
use super::stations::{Clock, Place, Shot, Station};

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// The mean, median and 95th percentile of `v`.
fn spread(v: &mut [f64]) -> (f64, f64, f64) {
    v.sort_by(f64::total_cmp);
    // LINT-OK: a few hundred frames.
    #[allow(clippy::cast_precision_loss)]
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    (mean, v[v.len() / 2], v[v.len() * 95 / 100])
}

#[test]
#[ignore = "instrument: times frames deep in a big dungeon; run with --ignored"]
fn time_frames_deep_in_a_big_dungeon() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let cell = env("DERETH_DUNGEON_COST_CELL").map_or(0x00D1_030A, |v| {
        u32::from_str_radix(v.trim().trim_start_matches("0x"), 16).expect("a cell id")
    });
    let frames: usize =
        env("DERETH_DUNGEON_COST_FRAMES").map_or(600, |v| v.trim().parse().expect("a count"));
    // LINT-OK: the high half of a cell id.
    #[allow(clippy::cast_possible_truncation)]
    let block = (cell >> 16) as u16;
    let room = dereth_world_data::env_cells::EnvCellLoader::new()
        .load_block(&store, block)
        .into_iter()
        .find(|d| d.id.0 == cell)
        .expect("the cell is in its block");
    let station = Station {
        name: "dungeon",
        why: "a big dungeon's frame cost",
        place: Place::Body {
            cell,
            origin: room.cell.frame.origin,
            yaw: 0.0,
        },
        clock: Clock::Sunny(0.5),
        weather: false,
        land_radius: 2,
    };
    let size = remaster_frames::size();
    for (look, remaster) in [("classic", false), ("remaster", true)] {
        for turning in [false, true] {
            let cfg = DeviceConfig {
                width: size.0,
                height: size.1,
                remaster,
                ..DeviceConfig::default()
            };
            let mut gpu =
                Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
            let mut shot = Shot::open(&store, &mut gpu, &station);
            if remaster {
                shot.scene.draw.cfg.render.fidelity =
                    FidelityPreferences::parse_switch(EVERY_BOX).expect("the boxes read");
            }
            shot.poll(&mut gpu);
            if let Some(c) = shot.scene.character.as_mut() {
                let mut at = c.position();
                at.frame = dereth_primitives::Frame::new(at.frame.origin, Quat::IDENTITY);
                c.teleport(at);
            }
            shot.follow = Some([3.0, 1.6, -0.2, 0.0]);
            for _ in 0..300 {
                shot.step(&mut gpu);
            }
            let (mut whole, mut draw) = (Vec::with_capacity(frames), Vec::with_capacity(frames));
            for k in 0..frames {
                if turning {
                    // LINT-OK: a frame counter.
                    #[allow(clippy::cast_precision_loss)]
                    let yaw = dereth_primitives::num::math::sinf(k as f32 * 0.01) * 0.3;
                    shot.follow = Some([3.0, 1.6, -0.2, yaw]);
                }
                let t = Instant::now();
                shot.step(&mut gpu);
                whole.push(t.elapsed().as_secs_f64() * 1000.0);
                draw.push(shot.last.draw);
            }
            let viewer = shot.scene.viewer_cell_id().map_or(0, |c| c.0);
            let (wm, wmed, w95) = spread(&mut whole);
            let (dm, dmed, d95) = spread(&mut draw);
            eprintln!(
                "dungeon-cost {look:8} {} cell {cell:#010X} viewer {viewer:#010X} body {:?}: step \
                 mean {wm:.2} median {wmed:.2} p95 {w95:.2} ms; draw mean {dm:.2} median {dmed:.2} \
                 p95 {d95:.2} ms",
                if turning { "turning" } else { "still  " },
                shot.scene
                    .character
                    .as_ref()
                    .map(|c| c.position().frame.origin),
            );
        }
    }
}
