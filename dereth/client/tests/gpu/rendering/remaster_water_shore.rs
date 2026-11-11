//! The remaster's water reflecting the scene drawn mirrored in it: on still water across a lake,
//! what it reflects is what the reflection found on screen shows, the hills and the shore across
//! the water, not a band of the mirrored picture taken from the wrong place in it.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.
//!
//! Behaviour: none (experimental)

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_client_runtime::report::VisualReport;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::world_scene::{RemasterCapture, RemasterWater};

use crate::instruments::stations::{report_station, Shot};

const W: u32 = 960;
const H: u32 = 540;

/// The water's kind among the material's kinds.
const WATER: u8 = 7;

/// A lake between hills at noon, the body standing on its shore.
const LAKE: &str = "dereth report 1; loc 0xA8B60037 [148.139191 164.770142 27.554998] \
                    -0.993797 0.000000 0.000000 -0.111209; time 0.5; clock 306486600.0; \
                    weather clear";

/// The boxes the water is drawn with: the sky, the fog, the drawn ground and the water.
const BOXES: &str = "Remaster=1,RemasterSky=1,RemasterFog=1,RemasterDetail=1,RemasterWater=1";

/// The remaster's last frame of the lake, the camera held `back` metres behind the body and `up`
/// over it, pitched `pitch` and turned `heading` radians from north, the clock held, the water
/// still and drawn as `water` changes it.
fn lake(heading: f32, water: impl Fn(RemasterWater) -> RemasterWater) -> RemasterCapture {
    let store = crate::common::dats();
    let cfg = DeviceConfig {
        width: W,
        height: H,
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
    let report = VisualReport::parse(LAKE).expect("the report reads");
    let mut shot = Shot::open(&store, &mut gpu, &report_station(&report, Some(8)));
    shot.apply_report(&report);
    shot.poll(&mut gpu);
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(BOXES).expect("the boxes read");
    shot.follow = Some([12.0, 3.0, -0.12, heading]);
    shot.poll(&mut gpu);
    let w = shot.scene.draw.remaster_water();
    shot.scene.draw.set_remaster_water(
        water(RemasterWater {
            waves: 0.0,
            rings: false,
            wake: false,
            ..w
        }),
        &mut gpu,
    );
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    for _ in 0..8 {
        shot.step_held(&mut gpu);
    }
    let capture = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    capture
}

/// **Still water reflects in the scene mirrored in it what it reflects found on screen**: across
/// a lake from its shore, turned two ways, the water drawn with the mirrored scene and the water
/// drawn with the reflection found on screen differ by more than sixteen levels on at most a
/// twentieth of the water: the same hills, shore and sky, mirrored where they stand.
#[test]
fn still_water_reflects_in_the_mirrored_scene_what_it_reflects_on_screen() {
    let _gpu = crate::common::gpu_lock();
    let mut out = Vec::new();
    for heading in [4.19, 4.8] {
        let mirrored = lake(heading, |w| RemasterWater { planar: true, ..w });
        let on_screen = lake(heading, |w| RemasterWater { planar: false, ..w });
        if let Some(dir) = std::env::var_os("DERETH_REMASTER_CAPTURE_DIR") {
            for (what, c) in [("mirrored", &mirrored), ("on-screen", &on_screen)] {
                crate::instruments::stations::write_png(
                    &std::path::Path::new(&dir).join(format!("water-shore-{what}-{heading}.png")),
                    c.width,
                    c.height,
                    &c.color,
                );
            }
        }
        let (mut apart, mut n) = (0usize, 0usize);
        for i in 0..mirrored.kind.len() {
            if mirrored.kind[i] != WATER || on_screen.kind[i] != WATER {
                continue;
            }
            let d = (0..3)
                .map(|k| u32::from(mirrored.color[i * 4 + k].abs_diff(on_screen.color[i * 4 + k])))
                .sum::<u32>();
            n += 1;
            if d > 3 * 16 {
                apart += 1;
            }
        }
        #[allow(clippy::cast_precision_loss)]
        let share = apart as f64 / n.max(1) as f64;
        eprintln!(
            "heading {heading}: {:.1}% of {n} water pixels differ by more than 16 levels",
            share * 100.0
        );
        out.push((heading, share, n));
    }
    for (heading, share, n) in out {
        assert!(
            n > 50_000,
            "heading {heading}: the water fills the view: {n} pixels"
        );
        assert!(
            share < 0.05,
            "heading {heading}: {:.1}% of the water reflects otherwise",
            share * 100.0
        );
    }
}
