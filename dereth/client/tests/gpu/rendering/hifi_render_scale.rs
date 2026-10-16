//! Behaviour: none (experimental Horizon rendering effects: the better lighting's world drawn at a
//! share of the frame's pixels and enlarged to the frame)
//!
//! At a render scale under a hundred percent the presentation draws its world at that share of
//! the frame's pixels each way, so the work of every pass that works per pixel falls with the
//! square of it, and enlarges the picture to fill the frame: the frame shows the same place as at
//! full scale, nothing shifted and nothing cut off, only softer.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware `wgpu` device.

#![cfg(gpu)]

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_render::device::{Backend, DeviceConfig, Gpu};
use dereth_render::wgpu::sidecar::SidecarReport;
use dereth_scene::world_scene::SceneWrites;

use crate::instruments::hifi_stations::{self, capture, Shot};

const W: u32 = 960;
const H: u32 = 540;
/// Frames the station is stepped before its frame is taken.
const STEPS: usize = 24;

fn device() -> Gpu {
    let cfg = DeviceConfig {
        width: W,
        height: H,
        hifi: true,
        ..DeviceConfig::default()
    };
    let gpu = Gpu::new_on(Backend::Wgpu, None, &cfg)
        .unwrap_or_else(|e| panic!("this station needs a wgpu device and none opened: {e}"));
    assert_eq!(
        gpu.adapter_kind(),
        dereth_render::device::AdapterKind::Hardware,
        "{} is a software rasteriser",
        gpu.adapter_name()
    );
    gpu
}

/// The town at noon with the better lighting, drawn at `percent` of the frame's pixels each way
/// and stepped [`STEPS`] frames: its last frame and the presentation's report of it.
fn town(percent: u32) -> (Vec<u8>, SidecarReport) {
    let store = crate::common::dats();
    let station = hifi_stations::stations(&store)
        .into_iter()
        .find(|s| s.name == "holtburg")
        .expect("the town's station");
    let mut gpu = device();
    gpu.hifi_set_render_scale(percent);
    let mut shot = Shot::open(&store, &mut gpu, &station);
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch("Lighting=1,Shadows=1,AmbientOcclusion=1,Sky=1")
            .expect("a fidelity spec");
    shot.scene
        .update_from_preferences(&store, &mut gpu)
        .expect("the preferences poll");
    for _ in 0..STEPS {
        shot.step(&mut gpu);
        assert!(
            gpu.hifi_settle(std::time::Duration::from_secs(60)),
            "the pipelines were not built in a minute"
        );
    }
    let report = gpu.hifi_report().expect("the presentation is installed");
    assert!(
        report.composited,
        "the presentation drew the frame: {report:?}"
    );
    (capture(&mut gpu), report)
}

fn note(report: &SidecarReport, name: &str) -> u64 {
    report
        .notes
        .iter()
        .find(|(n, _)| *n == name)
        .map_or_else(|| panic!("no note {name:?} in {report:?}"), |(_, v)| *v)
}

/// The mean brightness of each `cell`-pixel square of an RGBA frame, row by row.
fn cells(rgba: &[u8], cell: usize) -> Vec<f32> {
    let (w, h) = (W as usize, H as usize);
    let mut out = Vec::new();
    for cy in 0..h / cell {
        for cx in 0..w / cell {
            let mut sum = 0.0f32;
            for y in cy * cell..(cy + 1) * cell {
                for x in cx * cell..(cx + 1) * cell {
                    let p = &rgba[(y * w + x) * 4..(y * w + x) * 4 + 3];
                    sum += f32::from(p[0]) + f32::from(p[1]) + f32::from(p[2]);
                }
            }
            #[allow(clippy::cast_precision_loss)] // a small cell
            out.push(sum / (3 * cell * cell) as f32);
        }
    }
    out
}

/// At two thirds the world is drawn at two thirds of the frame's pixels each way; at a hundred
/// percent, at all of them.
#[test]
fn at_two_thirds_the_world_is_drawn_at_two_thirds_of_the_frames_pixels_each_way() {
    let _gpu = crate::common::gpu_lock();
    let (_, two_thirds) = town(67);
    assert_eq!(
        (
            note(&two_thirds, "world drawn width"),
            note(&two_thirds, "world drawn height")
        ),
        (643, 362)
    );
    let (_, full) = town(100);
    assert_eq!(
        (
            note(&full, "world drawn width"),
            note(&full, "world drawn height")
        ),
        (u64::from(W), u64::from(H))
    );
}

/// Enlarged to the frame, the picture drawn at two thirds shows the town where the full picture
/// shows it: over twenty-pixel squares, its brightness follows the full picture's far closer than
/// the full picture moved a square over does, and it fills the frame to its edges.
#[test]
fn the_world_drawn_at_two_thirds_fills_the_frame_where_the_full_picture_stands() {
    let _gpu = crate::common::gpu_lock();
    let (full, _) = town(100);
    let (two_thirds, _) = town(67);
    let cell = 20;
    let (a, b) = (cells(&full, cell), cells(&two_thirds, cell));
    let across = W as usize / cell;
    #[allow(clippy::cast_precision_loss)] // a few hundred cells
    let mean = |pairs: &mut dyn Iterator<Item = (f32, f32)>| {
        let (sum, n) = pairs.fold((0.0f32, 0usize), |(s, n), (x, y)| {
            (s + (x - y).abs(), n + 1)
        });
        sum / n.max(1) as f32
    };
    let aligned = mean(&mut a.iter().copied().zip(b.iter().copied()));
    // The full picture against itself a square to the right: what a shift of one square costs.
    let shifted = mean(
        &mut a
            .chunks(across)
            .flat_map(|row| row.iter().copied().zip(row.iter().copied().skip(1))),
    );
    eprintln!("aligned {aligned:.2} levels a cell, shifted by a cell {shifted:.2}");
    assert!(
        aligned * 2.0 < shifted,
        "the picture drawn at two thirds is {aligned:.2} levels a cell from the full one, a shift \
         of one cell {shifted:.2}"
    );
    // The last column and row of cells are drawn, not left black.
    let edge: Vec<f32> = b
        .chunks(across)
        .filter_map(|row| row.last().copied())
        .chain(b[b.len() - across..].iter().copied())
        .collect();
    let full_edge: Vec<f32> = a
        .chunks(across)
        .filter_map(|row| row.last().copied())
        .chain(a[a.len() - across..].iter().copied())
        .collect();
    let mean_of = |v: &[f32]| v.iter().sum::<f32>() / v.len().max(1) as f32;
    assert!(
        mean_of(&edge) > mean_of(&full_edge) * 0.8,
        "the edges of the enlarged picture are dark: {} against {}",
        mean_of(&edge),
        mean_of(&full_edge)
    );
}
