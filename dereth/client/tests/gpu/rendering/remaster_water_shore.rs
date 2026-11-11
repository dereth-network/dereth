//! The remaster's water at its shores:
//!
//! - **Reflecting the scene drawn mirrored in it**: on still water across a lake, what it
//!   reflects is what the reflection found on screen shows, the hills and the shore across the
//!   water, not a band of the mirrored picture taken from the wrong place in it.
//! - **Shallow water shows its bed**: near the shore the bed shows through the water, clearing
//!   toward the edge, while deep water out in the lake takes on the water's own color.
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
/// The ground's light class.
const GROUND: u8 = 1;

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

/// The lake's shallows by a stone pillar at noon, the body standing in them, the camera behind it
/// looking along the bank: the bank to the right, the open lake to the left.
const SHALLOWS: &str = "dereth report 1; loc 0xA9B50015 [59.258690 104.685966 27.904999] \
                        0.723986 0.000000 0.000000 -0.689815; time 0.5; clock 306486600.0; \
                        weather clear";

/// The remaster's last frame of the shallows, the water still and taking the light of the bed
/// seen through it `absorption` times as strongly as it is made to.
fn shallows(absorption: f32) -> RemasterCapture {
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
    let report = VisualReport::parse(SHALLOWS).expect("the report reads");
    let mut shot = Shot::open(&store, &mut gpu, &report_station(&report, Some(8)));
    shot.apply_report(&report);
    shot.poll(&mut gpu);
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(BOXES).expect("the boxes read");
    shot.follow = Some([6.0, 2.2, -0.3, -2.5]);
    shot.poll(&mut gpu);
    let w = shot.scene.draw.remaster_water();
    shot.scene.draw.set_remaster_water(
        RemasterWater {
            waves: 0.0,
            rings: false,
            wake: false,
            absorption,
            ..w
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

/// Where pixel `i` of `c` stands, from its depth.
#[allow(clippy::cast_precision_loss)]
fn place_of(c: &RemasterCapture, inv: &glam::Mat4, i: usize) -> glam::Vec3 {
    let (w, h) = (c.width as usize, c.height as usize);
    let (x, y) = ((i % w) as f32, (i / w) as f32);
    let ndc = glam::Vec4::new(
        (x + 0.5) / w as f32 * 2.0 - 1.0,
        1.0 - (y + 0.5) / h as f32 * 2.0,
        c.depth[i],
        1.0,
    );
    let p = *inv * ndc;
    (p / p.w).truncate()
}

/// How far from the water's edge each water pixel of `c` lies, meters: from the nearest place
/// the frame shows dry ground beside the water. `None` for a pixel that is not water.
fn from_edge(c: &RemasterCapture) -> Vec<Option<f32>> {
    let inv = glam::Mat4::from_cols_array(&c.view_projection).inverse();
    let (w, n) = (c.width as usize, c.kind.len());
    let ground = |i: usize| c.class[i] == GROUND;
    let water = |i: usize| c.kind[i] == WATER && ground(i);
    // The dry ground beside the water: a dry ground pixel with water next to it.
    let mut edge = Vec::new();
    for i in 0..n {
        if !ground(i) || water(i) {
            continue;
        }
        let beside = [i.wrapping_sub(1), i + 1, i.wrapping_sub(w), i + w];
        if beside.iter().any(|&j| j < n && water(j)) {
            edge.push(place_of(c, &inv, i).truncate());
        }
    }
    (0..n)
        .map(|i| {
            water(i).then(|| {
                let p = place_of(c, &inv, i).truncate();
                edge.iter()
                    .map(|e| e.distance(p))
                    .fold(f32::INFINITY, f32::min)
            })
        })
        .collect()
}

/// **Shallow water near the shore shows its bed, and deep water does not**: in a lake's shallows
/// by its bank, the water drawn taking the light of its bed as it is made to, as not at all, and
/// as wholly as it can: of how far the water's own color can cover the bed, it covers under 60%
/// within four meters of the water's edge and under 90% six to ten meters out, and over 85%
/// fifteen meters out and farther.
#[test]
fn shallow_water_near_the_shore_shows_its_bed_and_deep_water_does_not() {
    let _gpu = crate::common::gpu_lock();
    let made = shallows(1.0);
    let clear = shallows(0.0);
    let thick = shallows(50.0);
    let edge = from_edge(&made);
    if let Some(dir) = std::env::var_os("DERETH_REMASTER_CAPTURE_DIR") {
        crate::instruments::stations::write_png(
            &std::path::Path::new(&dir).join("water-shallows.png"),
            made.width,
            made.height,
            &made.color,
        );
    }
    // Of how far the water's own color can move each pixel from the bed it covers, how far it
    // does, over the pixels `from`..`to` meters from the edge; with how many pixels, and how far
    // it can move them all.
    let covered = |from: f32, to: f32| {
        let (mut moved, mut most, mut n) = (0u64, 0u64, 0usize);
        for (i, d) in edge.iter().enumerate() {
            let Some(d) = d else { continue };
            if *d < from || *d >= to || clear.kind[i] != WATER || thick.kind[i] != WATER {
                continue;
            }
            for k in 0..3 {
                let c = clear.color[i * 4 + k];
                moved += u64::from(made.color[i * 4 + k].abs_diff(c));
                most += u64::from(thick.color[i * 4 + k].abs_diff(c));
            }
            n += 1;
        }
        #[allow(clippy::cast_precision_loss)]
        let share = moved as f64 / most.max(1) as f64;
        (share, n, most)
    };
    let near = covered(0.0, 4.0);
    let mid = covered(6.0, 10.0);
    let deep = covered(15.0, f32::INFINITY);
    println!(
        "the water's own color covers {:.2} of the bed within 4 m of the edge ({} px), {:.2} \
         6-10 m out ({} px), {:.2} past 15 m ({} px)",
        near.0, near.1, mid.0, mid.1, deep.0, deep.1
    );
    for (what, band) in [
        ("near", near),
        ("six to ten meters out", mid),
        ("deep", deep),
    ] {
        assert!(
            band.1 > 2_000 && band.2 > 3 * band.1 as u64,
            "{what}: the band shows the water's color apart from its bed: {band:?}"
        );
    }
    assert!(
        near.0 < 0.6,
        "the shallows show their bed: {:.2} covered",
        near.0
    );
    assert!(
        mid.0 < 0.9,
        "six to ten meters out, the bed shows: {:.2} covered",
        mid.0
    );
    assert!(
        deep.0 > 0.85,
        "deep water takes the water's own color: {:.2}",
        deep.0
    );
}
