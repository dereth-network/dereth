//! Behaviour: none (experimental Horizon rendering effects: how much work the presentation does
//! for a frame)
//!
//! What the presentation's world replay costs over the ordinary frame's: in a frame with
//! buildings in view, it puts the recording's state back only after a pass has drawn at a mark,
//! and it keeps the world's depth at the first building stamp, so the depth seen through the
//! buildings' openings needs only what follows that stamp drawn again, not the world up to it.
//! And on a picture larger than 2560x1440 the ambient occlusion searches a quarter of the texels.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware `wgpu` device.

#![cfg(gpu)]

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_render::device::{Backend, DeviceConfig, Gpu};
use dereth_render::wgpu::sidecar::SidecarReport;
use dereth_scene::world_scene::SceneWrites;

use crate::instruments::hifi_stations::{self, Shot};

const W: u32 = 960;
const H: u32 = 540;
/// Frames the station is stepped before its report is read.
const STEPS: usize = 6;

fn device(width: u32, height: u32) -> Gpu {
    let cfg = DeviceConfig {
        width,
        height,
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

/// The town at noon, with `spec` ticked, stepped [`STEPS`] frames, the presentation's pipelines
/// waited for after each: the presentation's report of the last frame.
fn town(spec: &str) -> SidecarReport {
    town_sized(spec, W, H)
}

/// [`town`] on a device `width` by `height`.
fn town_sized(spec: &str, width: u32, height: u32) -> SidecarReport {
    let store = crate::common::dats();
    let station = hifi_stations::stations(&store)
        .into_iter()
        .find(|s| s.name == "holtburg")
        .expect("the town's station");
    let mut gpu = device(width, height);
    let mut shot = Shot::open(&store, &mut gpu, &station);
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(spec).expect("a fidelity spec");
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
    gpu.hifi_report().expect("the presentation is installed")
}

/// The count the presentation noted under `name` for the last frame.
fn note(report: &SidecarReport, name: &str) -> u64 {
    report
        .notes
        .iter()
        .find(|(n, _)| *n == name)
        .map_or_else(|| panic!("no note {name:?} in {report:?}"), |(_, v)| *v)
}

/// A frame holds thousands of marks; the replay puts the recording's state back only after the
/// few at which a pass drew: none with the better lighting alone, and with the physical sky at
/// most twice, once for the sky and once for the horizon's veil.
#[test]
fn the_world_replay_puts_its_state_back_only_after_a_pass_draws_at_a_mark() {
    let lit = town("Lighting=1");
    assert!(lit.census.reshaded > 0, "re-shaded: {:?}", lit.census);
    assert_eq!(lit.census.restored, 0, "{:?}", lit.census);
    let sky = town("Lighting=1,Sky=1");
    assert!(sky.census.reshaded > 0, "re-shaded: {:?}", sky.census);
    assert!(
        (1..=2).contains(&sky.census.restored),
        "the sky drew at its marks: {:?}",
        sky.census
    );
}

/// With buildings in view, the world's depth is kept at the first building stamp, and of the
/// world's draws not one in ten is drawn again for the depth each pixel sees: the re-shaded world
/// and the world drawn as recorded alike.
#[test]
fn the_depth_seen_through_openings_draws_again_only_what_follows_the_first_building_stamp() {
    for spec in ["Lighting=1", "Sky=1"] {
        let report = town(spec);
        let drawn = u64::from(report.census.total());
        let again = note(&report, "drawn again for the unstamped depth");
        assert_eq!(
            note(&report, "kept the depth before the first building stamp"),
            1,
            "{spec}: the town's buildings are stamped: {report:?}"
        );
        assert!(
            drawn > 0 && again * 10 < drawn,
            "{spec}: {again} of {drawn} draws drawn again"
        );
    }
}

/// The ambient occlusion searches every pixel of a 1920x1080 or a 2560x1440 picture, and one
/// texel for each two by two pixels of a 5120x2160 one.
#[test]
fn on_a_picture_larger_than_1440p_the_occlusion_searches_a_quarter_of_the_texels() {
    let _gpu = crate::common::gpu_lock();
    for (width, height, share) in [(1920, 1080, 1), (2560, 1440, 1), (5120, 2160, 4)] {
        let report = town_sized("AmbientOcclusion=1", width, height);
        let searched = note(&report, "occlusion texels searched");
        assert_eq!(
            searched * share,
            u64::from(width) * u64::from(height),
            "{width}x{height}: {searched} texels searched"
        );
    }
}
