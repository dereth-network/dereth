//! Better Lighting replays the frame's recording more than once, and each replay leaves out the
//! draws another draws: the translucent ones from the opaque world's, the opaque ones from the
//! translucent's. A recorded texture is bound only for a draw the replay draws, so the draws it
//! leaves out cost it nothing.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware `wgpu` device off screen.
//!
//! Behaviour: none (experimental hi-fi presentation)

#![cfg(all(gpu, feature = "hifi"))]

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_render::device::{AdapterKind, Backend, DeviceConfig, Gpu};

use crate::instruments::hifi_stations::{Clock, Place, Shot, Station, HOLTBURG};

/// The frame's size.
const SIZE: (u32, u32) = (960, 540);

/// A body standing in the middle of Holtburg, among its houses and trees, by day.
fn holtburg_centre() -> Station {
    Station {
        name: "holtburg-centre",
        why: "a town's houses and trees all round",
        place: Place::Ground {
            block: HOLTBURG,
            x: 84.0,
            y: 7.1,
            yaw: 0.0,
        },
        clock: Clock::Sunny(0.5),
        weather: false,
        land_radius: 3,
    }
}

#[test]
fn the_worlds_replays_bind_a_recorded_texture_only_for_a_draw_they_draw() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let cfg = DeviceConfig {
        width: SIZE.0,
        height: SIZE.1,
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
    let mut shot = Shot::open(&store, gpu, &holtburg_centre());
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch("Lighting=1").expect("the boxes parse");
    shot.poll(gpu);
    for _ in 0..12 {
        shot.step(gpu);
        assert!(
            gpu.hifi_settle(std::time::Duration::from_secs(60)),
            "the presentation's pipelines were not built in a minute"
        );
    }
    shot.draw(gpu);
    let report = gpu.hifi_report().expect("the presentation reports");
    assert!(report.composited, "the presentation drew the frame");
    let c = report.census;
    let drawn = c.legacy + c.reshaded + c.reframed;
    assert!(drawn > 100, "the town is drawn: {c:?}");
    // A draw drawn rebinds the groups it changes, about three of its four here, as does the
    // recording's state put back after a pass drew at a mark. Binding as the recording does, every
    // replay's every texture, binds over four a draw.
    assert!(
        2 * c.bound <= 7 * (drawn + c.restored) + 128,
        "{} groups bound for {drawn} draws drawn: {c:?}",
        c.bound
    );
}
