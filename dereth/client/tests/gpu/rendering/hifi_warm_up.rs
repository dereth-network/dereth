//! The presentation's pipelines are built ahead, off the frame: with every box ticked, the
//! anti-aliasing on and then an art pack's ground laid, no frame builds a pipeline of the
//! presentation's on its own thread. The frames are drawn the ordinary way until the warm-up has
//! built the presentation's pipelines, and then without the pack's ground until it has built its
//! passes'; the sun's shadows come to hold every block the frames handed over meanwhile.
//!
//! The warm-up is held while the test looks at the frames drawn before it has built what they
//! wait for, so that they are drawn however fast the device builds. What a frame built is the
//! presentation's own count of the pipelines made on the frame's thread, never a time.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware `wgpu` device off screen.

#![cfg(all(gpu, feature = "hifi"))]

use std::sync::Arc;

use dereth_client_runtime::present::Scene as _;
use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_render::device::{AdapterKind, Backend, DeviceConfig, Gpu};
use dereth_render::wgpu::sidecar::SidecarReport;

use crate::instruments::hifi_stations::{self, Shot};

const W: u32 = 960;
const H: u32 = 540;

/// Every box, at its highest.
const EVERY_BOX: &str = "Lighting=ultra,Shadows=ultra,GlobalIllumination=ultra,\
                         AmbientOcclusion=ultra,Lamps=ultra,Sky=ultra,Weather=ultra";

/// A guard against a hang, not a measure: the slowest device builds the pipelines in under a
/// minute.
const HANG: std::time::Duration = std::time::Duration::from_secs(600);

fn note(report: &SidecarReport, name: &str) -> u64 {
    report
        .notes
        .iter()
        .find(|(n, _)| *n == name)
        .map_or(0, |(_, v)| *v)
}

/// Behaviour: hifi.warm-up.no-frame-builds-a-pipeline-of-the-presentations
#[test]
fn with_every_box_ticked_no_frame_builds_a_pipeline_and_the_presentation_is_drawn_once_built() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let cfg = DeviceConfig {
        width: W,
        height: H,
        hifi: true,
        ..DeviceConfig::default()
    };
    let mut device = Gpu::new_on(Backend::Wgpu, None, &cfg)
        .unwrap_or_else(|e| panic!("this test needs a wgpu device and none opened: {e}"));
    assert_eq!(
        device.adapter_kind(),
        AdapterKind::Hardware,
        "{} is a software rasteriser",
        device.adapter_name()
    );
    let gpu = &mut device;
    let station = hifi_stations::stations(&store)
        .into_iter()
        .find(|s| s.name == "holtburg")
        .expect("the town station");
    let mut shot = Shot::open(&store, gpu, &station);
    // The ordinary frames first, which build the device's own pipelines.
    for _ in 0..4 {
        shot.step(gpu);
    }
    let mut frames = 0;
    // The boxes ticked with the warm-up held: the frames are drawn the ordinary way.
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(EVERY_BOX).expect("the boxes parse");
    shot.poll(gpu);
    gpu.hifi_hold(true);
    for _ in 0..4 {
        let report = next_frame(&mut shot, gpu, &mut frames);
        assert!(
            !report.composited,
            "frame {frames} drew the presentation before its pipelines were built"
        );
        assert!(note(&report, "pipelines being built ahead") > 0);
    }
    // Let go, the presentation is drawn once they are built.
    gpu.hifi_hold(false);
    let report = settled_until(&mut shot, gpu, &mut frames, "the presentation", |r| {
        r.composited
    });
    assert!(
        note(&report, "pipelines built ahead") > 0,
        "the warm-up built nothing"
    );
    // An art pack laid with the warm-up held: the presentation goes on, and its ground's passes
    // sit out until their pipelines are built.
    gpu.hifi_hold(true);
    shot.scene
        .set_art_pack(
            &store,
            gpu,
            Some(Arc::clone(&super::hifi_drawn_ground::ground_pack())),
        )
        .expect("the blocks bake for the pack");
    next_frame(&mut shot, gpu, &mut frames);
    // The pack's ground, which the scene works out off the frame.
    shot.scene.wait_for_drawn_ground();
    for _ in 0..4 {
        let report = next_frame(&mut shot, gpu, &mut frames);
        assert!(
            report.composited,
            "frame {frames} fell back to the ordinary frame"
        );
        for pass in ["drawn ground", "art pack ground"] {
            assert!(
                !report.passes.contains(&pass),
                "frame {frames} drew {pass} before its pipelines were built"
            );
        }
        assert!(note(&report, "pipelines being built ahead") > 0);
    }
    gpu.hifi_hold(false);
    let report = settled_until(&mut shot, gpu, &mut frames, "the pack's ground", |r| {
        r.passes.contains(&"drawn ground") && r.passes.contains(&"art pack ground")
    });
    assert!(
        shot.scene.drawn_ground_active(),
        "the presentation draws the pack's ground"
    );
    for pass in ["lighting", "gi", "gtao", "lighting post"] {
        assert!(
            report.passes.contains(&pass),
            "{pass} is drawn: {:?}",
            report.passes
        );
    }
    // The blocks were handed over on the frames it sat out: it comes to hold every one's.
    let meshed = u64::try_from(shot.scene.census().blocks_meshed).unwrap_or(u64::MAX);
    settled_until(
        &mut shot,
        gpu,
        &mut frames,
        "the sun's shadows of every block the landscape meshed",
        |r| note(r, "sun shadow blocks held") == meshed,
    );
}

/// The presentation's report on the next frame, which builds none of its pipelines.
fn next_frame(shot: &mut Shot, gpu: &mut Gpu, frames: &mut u32) -> SidecarReport {
    shot.step(gpu);
    *frames += 1;
    assert!(gpu.hifi_failed().is_none(), "{:?}", gpu.hifi_failed());
    let report = gpu.hifi_report().expect("the presentation is installed");
    assert_eq!(
        note(&report, "pipelines built on the frame"),
        0,
        "frame {frames} built a pipeline: {report:?}"
    );
    report
}

/// Frame after frame, each once what was being built is built, until `done` holds of one, within
/// two hundred: its report. `what` is what is waited for.
fn settled_until(
    shot: &mut Shot,
    gpu: &mut Gpu,
    frames: &mut u32,
    what: &str,
    done: impl Fn(&SidecarReport) -> bool,
) -> SidecarReport {
    let mut report = next_frame(shot, gpu, frames);
    for _ in 0..200 {
        if done(&report) {
            return report;
        }
        assert!(gpu.hifi_settle(HANG), "the pipelines were never built");
        report = next_frame(shot, gpu, frames);
    }
    panic!("{what} never came: {report:?}");
}
