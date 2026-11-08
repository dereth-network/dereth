//! The remaster's seam on the Vulkan device changes nothing while nothing draws through it: the
//! device asked for the remaster's features (as every Vulkan device of a build with the remaster
//! is), on a card that has them or one that lacks them, and the frame split at the world's end around a
//! hook that draws nothing, draw the same pixels, submit the same frame and count the same as
//! the ordinary device, at every station.
//!
//! Each run opens its own device and loads the station afresh, stepped the same way, because
//! the sky's clouds move with every frame drawn.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.
//!
//! Behaviour: none (experimental)

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use dereth_render::device::{DeviceConfig, Gpu};
use dereth_render::vulkan::world_hook::Untouched;

use crate::instruments::stations::{self, capture, counted_step, moved, Shot};

const W: u32 = 640;
const H: u32 = 480;

/// How a run draws the station.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Run {
    /// The ordinary device.
    Ordinary,
    /// The device asked for the remaster's features.
    Requested,
    /// The device asked for them on a card taken as lacking them, as an older card is.
    Lacking,
    /// That device, the frame split around a hook that draws nothing.
    Hooked,
    /// The same, the hook taken before the last frame.
    Unhooked,
}

/// The station's last frame under `run`: its pixels, the digest of what it submitted, its
/// counters, and how many frames the hook drew.
fn draw(
    store: &std::sync::Arc<dereth_dat::RetailDatStore>,
    station: &stations::Station,
    run: Run,
) -> (Vec<u8>, Option<u64>, String, u64) {
    let cfg = DeviceConfig {
        width: W,
        height: H,
        remaster: run != Run::Ordinary,
        // Checked by the validation layer where it is installed: an error fails the test.
        debug: true,
        ..DeviceConfig::default()
    };
    dereth_render::vulkan::world_hook::lack_remaster_features_for_tests(run == Run::Lacking);
    let made = Gpu::new_on(crate::common::remaster_backend(), None, &cfg);
    dereth_render::vulkan::world_hook::lack_remaster_features_for_tests(false);
    let mut gpu = made.expect("a device");
    assert_eq!(
        gpu.adapter_kind(),
        dereth_render::device::AdapterKind::Hardware,
        "{} is a software rasterizer",
        gpu.adapter_name()
    );
    if run == Run::Lacking {
        assert!(
            gpu.remaster_requested(),
            "asked for the remaster's features"
        );
        if crate::common::remaster_backend() == dereth_render::device::Backend::Vulkan {
            assert!(!gpu.remaster_supported(), "and made without them");
        }
    } else if run != Run::Ordinary {
        assert!(
            gpu.remaster_supported(),
            "{} has Vulkan 1.3",
            gpu.adapter_name()
        );
    }
    gpu.set_frame_digest(true);
    let mut shot = Shot::open(store, &mut gpu, station);
    if matches!(run, Run::Hooked | Run::Unhooked) {
        gpu.install_world_hook(Box::new(Untouched::default()))
            .expect("installs");
    }
    shot.poll(&mut gpu);
    for _ in 1..6 {
        shot.step(&mut gpu);
    }
    let mut hooked = 0;
    if run == Run::Unhooked {
        let mut hook = gpu.take_world_hook().expect("installed");
        hooked = hook
            .as_any_mut()
            .downcast_mut::<Untouched>()
            .map_or(0, |u| u.handed);
    }
    let counted = counted_step(&mut shot, &mut gpu);
    let pixels = capture(&mut gpu);
    if let Some(hook) = gpu.world_hook_mut() {
        hooked = hook
            .as_any_mut()
            .downcast_mut::<Untouched>()
            .map_or(0, |u| u.handed);
    }
    assert!(gpu.world_hook_failed().is_none());
    let digest = gpu.last_frame_digest();
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(
        errors.is_empty(),
        "{} {run:?}: the validation layer: {errors:#?}",
        station.name
    );
    (pixels, digest, counted, hooked)
}

/// At the town by day, in the forest, at night, under the street lamps, at a doorway, in a room,
/// underground, in the rain and in a snowbound village: the device asked for the remaster's
/// features, the device asked for them on a card taken as lacking them (as an older card is,
/// which then starts as the ordinary device does), the same device with the frame split at the world's end around a hook that draws
/// nothing, and that device once the hook is taken each draw the ordinary device's last frame
/// byte for byte, with the same submitted draws and the same counters and scene census; the
/// hook was handed every frame whose world it was installed for.
#[test]
fn every_station_draws_the_ordinary_frame_through_the_world_hook() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let names = [
        "holtburg",
        "forest",
        "night",
        "lamps-street",
        "doorway",
        "indoor",
        "dungeon",
        "rain-street",
        "snow-village",
    ];
    let stations: Vec<_> = stations::isolation_stations(&store)
        .into_iter()
        .filter(|s| names.contains(&s.name))
        .collect();
    assert_eq!(stations.len(), names.len());
    for station in &stations {
        let (base, base_digest, base_counted, _) = draw(&store, station, Run::Ordinary);
        // The Vulkan device digests what it submits; the Direct3D 12 one digests nothing.
        assert_eq!(
            base_digest.is_some(),
            crate::common::remaster_backend() == dereth_render::device::Backend::Vulkan,
            "the digest"
        );
        for run in [Run::Requested, Run::Lacking, Run::Hooked, Run::Unhooked] {
            let (pixels, digest, counted, hooked) = draw(&store, station, run);
            let what = format!("{} {run:?}", station.name);
            let changed = moved(&pixels, &base);
            eprintln!("{what}: {changed} bytes differ; the hook drew {hooked} frames");
            assert_eq!(changed, 0, "{what}: the pixels");
            assert_eq!(digest, base_digest, "{what}: the submitted frame");
            assert_eq!(counted, base_counted, "{what}: the counters");
            match run {
                Run::Hooked => assert_eq!(hooked, 6, "{what}: every frame drawn"),
                Run::Unhooked => assert_eq!(hooked, 5, "{what}: every frame before it was taken"),
                _ => assert_eq!(hooked, 0),
            }
        }
    }
}
