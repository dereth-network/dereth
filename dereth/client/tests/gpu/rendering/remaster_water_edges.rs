//! The remaster's water at the picture's left and right edges: where the scene is drawn mirrored
//! in the water, the water there reflects what that picture shows at its edge, as the water beside
//! it does, rather than a sky found elsewhere and lit otherwise, which showed as a bright bar down
//! each side.
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

/// The boxes the water is drawn with: the sky, the fog, the drawn ground and the water, which
/// draws the scene mirrored in the water nearest the eye where the reflections are not traced.
const BOXES: &str = "Remaster=1,RemasterSky=1,RemasterFog=1,RemasterDetail=1,RemasterWater=1";

/// The remaster's last frame of the lake, the camera held `back` metres behind the body and `up`
/// over it, pitched `pitch` and turned `heading` radians from north, the clock held; each water
/// pixel showing what its reflection found: red a surface, green the land, blue the sky.
fn lake_checked(back: f32, up: f32, pitch: f32, heading: f32) -> RemasterCapture {
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
    shot.follow = Some([back, up, pitch, heading]);
    shot.poll(&mut gpu);
    let water = shot.scene.draw.remaster_water();
    assert!(
        water.planar && !water.traced,
        "the scene is drawn mirrored in the water"
    );
    shot.scene.draw.set_remaster_water(
        RemasterWater {
            check: true,
            ..water
        },
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

/// Of the water pixels in the columns from `from` to `to` (shares of the width, counted in from
/// both sides), the share whose reflection found no surface (the checked color is not red), and
/// how many water pixels there are.
fn not_found(c: &RemasterCapture, from: f64, to: f64) -> (f64, usize) {
    let (w, h) = (c.width as usize, c.height as usize);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let at = |share: f64| (w as f64 * share) as usize;
    let (mut missed, mut n) = (0usize, 0usize);
    for y in 0..h {
        for x in at(from)..at(to) {
            for i in [y * w + x, y * w + w - 1 - x] {
                if c.kind[i] != WATER {
                    continue;
                }
                n += 1;
                let p = &c.color[i * 4..i * 4 + 3];
                if !(p[0] > 200 && p[1] < 60 && p[2] < 60) {
                    missed += 1;
                }
            }
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let share = missed as f64 / n.max(1) as f64;
    (share, n)
}

/// **The water at the picture's left and right edges reflects what the mirrored picture shows,
/// as the water beside it does**: across a lake from its shore, turned several ways, the rippled
/// water's reflection in the outer 4% of the width on each side is found in the scene mirrored in
/// it (at most 1% of those pixels find anything else), not in a sky found elsewhere, which is lit
/// otherwise and showed as a bright bar down each side as the camera turned.
#[test]
fn the_water_at_the_pictures_sides_reflects_what_the_water_beside_it_does() {
    let _gpu = crate::common::gpu_lock();
    let mut out = Vec::new();
    for heading in [0.44, 3.2, 4.8] {
        let c = lake_checked(12.0, 3.0, -0.12, heading);
        if let Some(dir) = std::env::var_os("DERETH_REMASTER_CAPTURE_DIR") {
            crate::instruments::stations::write_png(
                &std::path::Path::new(&dir).join(format!("water-edges-{heading}.png")),
                c.width,
                c.height,
                &c.color,
            );
        }
        let (edge, n) = not_found(&c, 0.0, 0.04);
        let (inside, _) = not_found(&c, 0.1, 0.3);
        eprintln!(
            "heading {heading}: {:.2}% of {n} water pixels at the sides found no surface, {:.2}% inside",
            edge * 100.0,
            inside * 100.0
        );
        out.push((heading, edge, n));
    }
    for (heading, edge, n) in out {
        assert!(
            n > 10_000,
            "heading {heading}: the water reaches the sides: {n} pixels"
        );
        assert!(
            edge < 0.01,
            "heading {heading}: {:.2}% of the water at the sides found no surface",
            edge * 100.0
        );
    }
}
