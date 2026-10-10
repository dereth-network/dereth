//! The optional high-fidelity presentation changes nothing while it is off, and its seam
//! changes nothing when it draws the world with no pass of its own: the same recording, the same
//! pixels, the same counters, the same device, the same floating-point environment. A failure of
//! the presentation draws the ordinary frame and removes it.
//!
//! The frames are compared at stations whose view does not move from one frame to the next (the
//! camera pitched down at the ground, the weather layer off), or between two loads of the same
//! station stepped the same way, because the sky's clouds move with every frame drawn.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware `wgpu` device; the comparison
//! with the base build also reads a capture of it (frames, recordings and counts) named by
//! `DERETH_HIFI_BASELINE_DIR`.

#![cfg(gpu)]

use std::sync::Arc;

use dereth_dat::RetailDatStore;
use dereth_render::device::{Backend, DeviceConfig, Gpu};
#[cfg(feature = "hifi")]
use dereth_scene::world_scene::SceneWrites;

use crate::instruments::hifi_stations::{self, capture, counted_step, listed, moved, Shot};

const W: u32 = 640;
const H: u32 = 480;

/// A `wgpu` device off screen, asked for the presentation's features when `hifi` says so.
fn wgpu_device(width: u32, height: u32, hifi: bool) -> Gpu {
    let cfg = DeviceConfig {
        width,
        height,
        #[cfg(feature = "hifi")]
        hifi,
        ..DeviceConfig::default()
    };
    #[cfg(not(feature = "hifi"))]
    let _ = hifi;
    let gpu = Gpu::new_on(Backend::Wgpu, None, &cfg)
        .unwrap_or_else(|e| panic!("these stations need a wgpu device and none opened: {e}"));
    assert_eq!(
        gpu.adapter_kind(),
        dereth_render::device::AdapterKind::Hardware,
        "{} is a software rasteriser",
        gpu.adapter_name()
    );
    gpu
}

/// The recorded frame's digest and the device's counters after one frame: its own tables, and
/// every object it holds by its own count, whoever made it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Counters {
    digest: Option<u64>,
    draw_calls: u64,
    live_textures: usize,
    descriptors: dereth_render::descriptor::DescriptorStats,
    passes: Option<u64>,
    frames: u64,
    objects: Option<dereth_render::device::DeviceObjects>,
}

/// Draw the frame `shot` stands at again, and its pixels and counters; the counters that
/// accumulate are taken as this frame's share.
fn frame(shot: &mut Shot, gpu: &mut Gpu) -> (Vec<u8>, Counters) {
    let calls = gpu.draw_calls();
    let passes = gpu.passes_encoded();
    let stamp = gpu.frame_stamp();
    shot.draw(gpu);
    let counters = Counters {
        digest: gpu.last_frame_digest(),
        draw_calls: gpu.draw_calls() - calls,
        live_textures: gpu.live_textures(),
        descriptors: gpu.descriptor_stats(),
        passes: gpu.passes_encoded().zip(passes).map(|(a, b)| a - b),
        frames: gpu.frame_stamp() - stamp,
        objects: gpu.device_objects(),
    };
    (capture(gpu), counters)
}

/// Holtburg with the camera pitched steeply down at the ground and the weather off: a view that
/// draws the same frame however many times it is drawn.
fn still_view(store: &Arc<RetailDatStore>, gpu: &mut Gpu) -> Shot {
    let station = hifi_stations::Station {
        name: "still",
        why: "the ground under the town, looked straight down at, with no sky in view",
        place: hifi_stations::Place::Free {
            block: hifi_stations::HOLTBURG,
            x: 96.0,
            y: 96.0,
            eye: 30.0,
            yaw: 0.0,
            pitch: -1.2,
        },
        clock: hifi_stations::Clock::Sunny(0.5),
        weather: false,
        land_radius: 2,
    };
    let mut shot = Shot::open(store, gpu, &station);
    gpu.set_frame_digest(true);
    for _ in 0..6 {
        shot.step(gpu);
    }
    shot
}

/// The environment floating-point arithmetic is done in, as the bits of operations whose answers
/// tell its settings apart: a result too small to be normal is kept (not flushed to zero); a
/// subnormal operand is read as itself (not as zero), in single and double precision; rounding
/// is to nearest (`1 + 0.75 ulp` rounds up, which rounding down or toward zero would not; `1/3`
/// rounds down, which rounding up would not; `-1/3` rounds up, which rounding down would not);
/// and division by zero, an invalid operation and an overflow give their quiet answers (an
/// unmasked exception would stop the process here).
fn float_environment() -> [u64; 9] {
    use std::hint::black_box as b;
    let made_tiny = b(f32::MIN_POSITIVE) / b(4.0f32);
    let read_tiny = b(f32::from_bits(0x0020_0000)) * b(2.0f32);
    let read_tiny_wide = b(f64::from_bits(0x0000_0000_0000_0003)) * b(4.0f64);
    let half_ulp = b(1.0f32) + b(f32::EPSILON * 0.75);
    let third = b(1.0f64) / b(3.0f64);
    let minus_third = b(-1.0f64) / b(3.0f64);
    let by_zero = b(1.0f32) / b(0.0f32);
    let invalid = b(0.0f64) / b(0.0f64);
    let overflow = b(f32::MAX) * b(2.0f32);
    [
        u64::from(made_tiny.to_bits()),
        u64::from(read_tiny.to_bits()),
        read_tiny_wide.to_bits(),
        u64::from(half_ulp.to_bits()),
        third.to_bits(),
        minus_third.to_bits(),
        u64::from(by_zero.to_bits()),
        u64::from(invalid.is_nan()),
        u64::from(overflow.to_bits()),
    ]
}

/// The floating-point environment the game's arithmetic needs, as [`float_environment`] reads
/// it: what the client's physics is computed in.
const FLOAT_ENVIRONMENT: [u64; 9] = [
    0x0020_0000,
    0x0040_0000,
    0x0000_0000_0000_000C,
    0x3F80_0001,
    0x3FD5_5555_5555_5555,
    0xBFD5_5555_5555_5555,
    0x7F80_0000,
    1,
    0x7F80_0000,
];

/// Behaviour: hifi.off.the-recorded-frame-and-pixels-match-the-base-build
/// Every isolation station, drawn by this build under the classic or the modern interface, records
/// the same frame and draws the same pixels as the build before the effects existed, on the same
/// device and driver, with no box ticked and with every box ticked: on the `wgpu` device the
/// recording and the pixels, on the Vulkan and Direct3D 12 devices the pixels; and no effect is
/// installed on any of them. A `wgpu` device asked for what the effects draw with, as a client
/// started in Horizon has it, draws the same frame too once the client leaves Horizon.
#[test]
#[ignore = "compares with a capture of the base build: set DERETH_HIFI_BASELINE_DIR and run with --ignored"]
#[allow(clippy::too_many_lines)]
fn every_station_records_and_draws_what_the_base_build_did() {
    let _gpu = crate::common::gpu_lock();
    let root = std::env::var("DERETH_HIFI_BASELINE_DIR")
        .expect("DERETH_HIFI_BASELINE_DIR names the base build's capture");
    let store = crate::common::dats();
    let stations = hifi_stations::isolation_stations(&store);
    let every_box = dereth_client_runtime::render_prefs::FidelityPreferences::parse_switch(
        "Lighting=1,Shadows=1,GlobalIllumination=1,AmbientOcclusion=1,Lamps=1,Sky=1,Weather=1,Interface=0",
    )
    .expect("every box under another interface");
    let mut backends = vec![Backend::Wgpu, Backend::Vulkan];
    if cfg!(all(windows, feature = "d3d12")) {
        backends.push(Backend::D3d12);
    }
    let mut checked = 0;
    for backend in backends {
        let dir = std::path::Path::new(&root).join(backend.name());
        let digests = std::fs::read_to_string(dir.join("digest.txt")).unwrap_or_default();
        let counts = std::fs::read_to_string(dir.join("counters.txt")).unwrap_or_default();
        for station in &stations {
            let Some((width, height, base)) =
                hifi_stations::read_png(&dir.join(format!("{}-off.png", station.name)))
            else {
                panic!(
                    "the base capture has no {} frame of {}",
                    backend.name(),
                    station.name
                );
            };
            let widened_too = cfg!(feature = "hifi") && backend == Backend::Wgpu;
            let runs: &[(bool, bool)] = if widened_too {
                &[(false, false), (true, false), (true, true)]
            } else {
                &[(false, false), (true, false)]
            };
            for &(ticked, widened) in runs {
                let cfg = DeviceConfig {
                    width,
                    height,
                    #[cfg(feature = "hifi")]
                    hifi: widened,
                    ..DeviceConfig::default()
                };
                #[cfg(not(feature = "hifi"))]
                let _ = widened;
                let mut gpu = Gpu::new_on(backend, None, &cfg).expect("the device opens");
                gpu.set_frame_digest(true);
                let mut shot = Shot::open(&store, &mut gpu, station);
                if ticked {
                    shot.scene.draw.cfg.render.fidelity = every_box;
                }
                shot.poll(&mut gpu);
                for _ in 1..6 {
                    shot.step(&mut gpu);
                }
                let counted = counted_step(&mut shot, &mut gpu);
                let pixels = capture(&mut gpu);
                let what = format!(
                    "{} {}{}{}",
                    backend.name(),
                    station.name,
                    if ticked { " with every box ticked" } else { "" },
                    if widened {
                        " on the device asked for the effects"
                    } else {
                        ""
                    }
                );
                #[cfg(feature = "hifi")]
                assert!(
                    gpu.hifi_sidecar_mut().is_none(),
                    "{what}: an effect was installed"
                );
                // A device made without the effects' request keeps nothing for the lamps.
                #[cfg(feature = "hifi")]
                if !widened {
                    assert_eq!(
                        shot.scene.draw.hifi_lamp_sites(),
                        0,
                        "{what}: the blocks kept their placements for the lamps"
                    );
                }
                // Only the `wgpu` device records its frames, and there the recording is required.
                let digest = gpu.last_frame_digest();
                if backend == Backend::Wgpu {
                    let digest = digest.expect("the wgpu device digests its frames");
                    let want = listed(&digests, station.name).unwrap_or_else(|| {
                        panic!("the base capture has no digest of {}", station.name)
                    });
                    assert_eq!(
                        format!("{digest:016x}"),
                        want,
                        "{what}: the recorded frame differs from the base build's"
                    );
                }
                let want = listed(&counts, station.name).unwrap_or_else(|| {
                    panic!(
                        "the base capture has no {} counts of {}",
                        backend.name(),
                        station.name
                    )
                });
                assert_eq!(
                    counted, want,
                    "{what}: the device or the scene counted another frame than the base build's"
                );
                let changed = moved(&pixels, &base);
                eprintln!("{what}: {changed} bytes differ");
                assert_eq!(
                    changed, 0,
                    "{what}: the pixels differ from the base build's"
                );
                checked += 1;
            }
        }
    }
    assert!(checked >= 2 * 2 * stations.len(), "{checked}");
}

#[cfg(feature = "hifi")]
mod presentation {
    use super::*;
    use dereth_client_runtime::render_prefs::FidelityPreferences;
    use dereth_render::wgpu::sidecar::{
        wgpu, FrameSidecar, SidecarContext, SidecarError, SidecarFrame,
    };

    fn fidelity(spec: &str) -> FidelityPreferences {
        FidelityPreferences::parse_switch(spec).expect("a fidelity spec")
    }

    /// Set the shot's `[Fidelity]` preferences and poll them, as the application does, and what
    /// the poll did.
    fn poll_with(
        shot: &mut Shot,
        gpu: &mut Gpu,
        prefs: FidelityPreferences,
    ) -> dereth_client_runtime::frame_events::RenderPrefWork {
        shot.scene.draw.cfg.render.fidelity = prefs;
        shot.scene
            .update_from_preferences(&crate::common::dats(), gpu)
            .expect("the preferences poll")
    }

    /// [`poll_with`], and whether the presentation was installed, changed or removed.
    fn set(shot: &mut Shot, gpu: &mut Gpu, prefs: FidelityPreferences) -> bool {
        poll_with(shot, gpu, prefs).fidelity_changed
    }

    /// Behaviour: hifi.off.the-device-request-is-unchanged-with-the-presentation-off
    /// A device made without the presentation's request has exactly the features and limits of
    /// the ordinary device; one made with it has those and only the presentation's additions.
    /// Neither, nor a frame drawn through the presentation, changes how floating-point
    /// arithmetic rounds or keeps subnormals.
    #[test]
    fn the_device_request_and_the_float_environment_are_unchanged_with_the_presentation_off() {
        let _gpu = crate::common::gpu_lock();
        let before = float_environment();
        assert_eq!(
            before, FLOAT_ENVIRONMENT,
            "the test process does not start in the environment the game computes in"
        );
        let ordinary = {
            let cfg = DeviceConfig {
                width: W,
                height: H,
                ..DeviceConfig::default()
            };
            assert!(!cfg.hifi, "the presentation's request is not the default");
            let gpu = Gpu::new_on(Backend::Wgpu, None, &cfg).expect("a wgpu device");
            gpu.hifi_device_features().expect("a wgpu device")
        };
        let off = wgpu_device(W, H, false)
            .hifi_device_features()
            .expect("a wgpu device");
        assert_eq!(off.0, ordinary.0, "features");
        assert_eq!(off.1, ordinary.1, "limits");
        assert_eq!(float_environment(), before, "after making the device");
        let store = crate::common::dats();
        let mut gpu = wgpu_device(W, H, true);
        let (features, limits) = gpu.hifi_device_features().expect("a wgpu device");
        assert!(features.contains(ordinary.0));
        let extra = features - ordinary.0;
        assert!(
            (dereth_render::wgpu::sidecar::HIFI_FEATURES | wgpu::Features::EXPERIMENTAL_RAY_QUERY)
                .contains(extra),
            "{extra:?}"
        );
        assert!(limits.max_color_attachments >= ordinary.1.max_color_attachments);
        assert_eq!(float_environment(), before, "after making the request");
        let mut shot = still_view(&store, &mut gpu);
        for view in ["Debug=1", "Debug=2"] {
            assert!(set(&mut shot, &mut gpu, fidelity(view)));
            shot.draw(&mut gpu);
            assert!(gpu.hifi_report().is_some_and(|r| r.composited));
            assert_eq!(
                float_environment(),
                before,
                "after a frame presented with {view}"
            );
        }
    }

    /// Behaviour: hifi.options.no-sidecar-without-an-effective-feature
    /// With every box ticked under the classic or the modern interface the scene installs no
    /// presentation; under Horizon with one option in effect it does, and leaving Horizon or
    /// turning the option off removes it. A `wgpu` device made without the presentation's request,
    /// as a client started in another interface has, refuses it and the poll says so.
    #[test]
    fn the_scene_installs_the_presentation_only_with_an_effective_option() {
        let _gpu = crate::common::gpu_lock();
        let store = crate::common::dats();
        let mut gpu = wgpu_device(W, H, true);
        let mut shot = still_view(&store, &mut gpu);
        let every_box =
            "Lighting=1,Shadows=1,GlobalIllumination=1,AmbientOcclusion=1,Lamps=1,Sky=1,Weather=1";
        assert!(!set(&mut shot, &mut gpu, FidelityPreferences::default()));
        assert!(gpu.hifi_sidecar_mut().is_none());
        assert!(set(
            &mut shot,
            &mut gpu,
            fidelity(&format!("{every_box},Interface=0"))
        ));
        assert!(
            gpu.hifi_sidecar_mut().is_none(),
            "every box under another interface"
        );
        for _ in 0..4 {
            shot.step(&mut gpu);
        }
        assert_eq!(
            shot.scene.draw.hifi_lamp_blocks_looked(),
            0,
            "lamps looked for under another interface"
        );
        assert!(set(&mut shot, &mut gpu, fidelity("Debug=1")));
        assert!(gpu.hifi_sidecar_mut().is_some());
        shot.step(&mut gpu);
        assert_eq!(
            shot.scene.draw.hifi_lamp_blocks_looked(),
            0,
            "lamps looked for while the presentation does not draw them"
        );
        assert!(set(&mut shot, &mut gpu, fidelity("Debug=1,Interface=0")));
        assert!(gpu.hifi_sidecar_mut().is_none(), "leaving Horizon");
        assert!(set(&mut shot, &mut gpu, fidelity(every_box)));
        assert!(gpu.hifi_sidecar_mut().is_some());
        assert!(set(&mut shot, &mut gpu, FidelityPreferences::default()));
        assert!(gpu.hifi_sidecar_mut().is_none());
        drop(shot);
        drop(gpu);

        // A wgpu device made without the request refuses it, and the poll says so once.
        let mut plain = wgpu_device(W, H, false);
        assert!(!plain.hifi_requested());
        let mut shot = still_view(&store, &mut plain);
        let work = poll_with(&mut shot, &mut plain, fidelity("Debug=1"));
        assert!(work.fidelity_changed && work.fidelity_refused, "{work:?}");
        assert!(plain.hifi_sidecar_mut().is_none());
        drop(shot);
        drop(plain);

        // A device the presentation cannot draw on refuses it, and the poll says so.
        let cfg = DeviceConfig {
            width: W,
            height: H,
            ..DeviceConfig::default()
        };
        let mut vulkan = Gpu::new_on(Backend::Vulkan, None, &cfg).expect("a Vulkan device");
        let mut shot = still_view(&store, &mut vulkan);
        let work = poll_with(&mut shot, &mut vulkan, fidelity("Debug=1"));
        assert!(work.fidelity_changed && work.fidelity_refused, "{work:?}");
        assert!(vulkan.hifi_sidecar_mut().is_none());
        let work = poll_with(&mut shot, &mut vulkan, fidelity("Debug=1"));
        assert!(
            !work.fidelity_refused,
            "an unchanged poll does not say it again"
        );
    }

    /// Behaviour: hifi.off.toggling-off-restores-every-pixel-and-counter
    /// The same still frame, drawn with the presentation off, then drawn through it with no pass
    /// of its own, then with its depth view, then with it off again: off and pass-through record
    /// the same frame and draw the same pixels; the depth view draws a different picture; and
    /// after turning it off every pixel, the recording and every device counter are as they
    /// were, with one render pass a frame.
    #[test]
    fn toggling_the_presentation_restores_every_pixel_recording_and_counter() {
        let _gpu = crate::common::gpu_lock();
        let store = crate::common::dats();
        let mut gpu = wgpu_device(W, H, true);
        let mut shot = still_view(&store, &mut gpu);
        let (off_px, off) = frame(&mut shot, &mut gpu);
        let (again_px, again) = frame(&mut shot, &mut gpu);
        assert_eq!(
            moved(&off_px, &again_px),
            0,
            "the view is not still, so this test cannot see"
        );
        assert_eq!(off, again, "the counters of a still frame");
        assert_eq!(off.passes, Some(off.frames), "one render pass a frame");
        assert!(off.digest.is_some());

        assert!(set(&mut shot, &mut gpu, fidelity("Debug=1")));
        let (through_px, through) = frame(&mut shot, &mut gpu);
        let report = gpu.hifi_report().expect("installed");
        assert!(report.composited, "the presentation drew the frame");
        let (off_objects, through_objects) = (
            off.objects.expect("the wgpu device counts its objects"),
            through.objects.expect("the wgpu device counts its objects"),
        );
        assert!(
            through_objects.texture_memory > off_objects.texture_memory
                && through_objects.query_sets > off_objects.query_sets,
            "the device's count did not see the presentation's targets and timer: \
             {off_objects:?} {through_objects:?}"
        );
        assert!(report.census.legacy > 0, "{report:?}");
        assert_eq!(
            moved(&off_px, &through_px),
            0,
            "pass-through changed the picture"
        );
        assert_eq!(
            through.digest, off.digest,
            "the scene recorded another frame with the presentation on"
        );
        assert_eq!(through.draw_calls, off.draw_calls);

        assert!(set(&mut shot, &mut gpu, fidelity("Debug=2")));
        let (depth_px, _) = frame(&mut shot, &mut gpu);
        let changed = moved(&off_px, &depth_px);
        assert!(
            changed > 10_000,
            "the depth view drew the ordinary picture: {changed}"
        );

        assert!(set(&mut shot, &mut gpu, FidelityPreferences::default()));
        assert!(gpu.hifi_sidecar_mut().is_none());
        let (back_px, back) = frame(&mut shot, &mut gpu);
        assert_eq!(moved(&off_px, &back_px), 0, "turning it off");
        assert_eq!(back.digest, off.digest);
        assert_eq!(back.draw_calls, off.draw_calls);
        assert_eq!(back.live_textures, off.live_textures);
        assert_eq!(back.passes, off.passes, "no extra pass once it is off");
        assert_eq!(back.frames, off.frames);
        assert_eq!(
            back.descriptors, off.descriptors,
            "the presentation took texture slots"
        );
        assert_eq!(
            back.objects, off.objects,
            "the presentation left an object on the device"
        );
    }

    /// Every box, as the options page ticks them.
    const EVERY_BOX: &str =
        "Lighting=1,Shadows=1,GlobalIllumination=1,AmbientOcclusion=1,Lamps=1,Sky=1,Weather=1";

    /// [`frame`], with the scene's census.
    fn frame_and_census(
        shot: &mut Shot,
        gpu: &mut Gpu,
    ) -> (
        Vec<u8>,
        Counters,
        dereth_client_runtime::present::SceneCensus,
    ) {
        let (px, counters) = frame(shot, gpu);
        let census = dereth_client_runtime::present::Scene::census(&shot.scene);
        (px, counters, census)
    }

    /// Behaviour: hifi.off.leaving-horizon-with-every-box-ticked-restores-everything
    /// At the town by day, at night, under the street lamps, at a doorway, underground, in the
    /// town's street in the rain and in a snowbound village, each on a device of its own: a frame
    /// drawn with every box ticked under Horizon for thirty
    /// frames and then left by leaving Horizon is, once it is left, the frame drawn before, pixel
    /// for pixel, with the same recording, draws, textures, texture slots, render passes and
    /// scene census, and every object the device holds by its own count, its memory included,
    /// is as it was; no lamp is left looked for.
    #[test]
    #[allow(clippy::too_many_lines)]
    fn leaving_horizon_with_every_box_ticked_restores_every_pixel_counter_and_device_object() {
        let _gpu = crate::common::gpu_lock();
        let store = crate::common::dats();
        let names = [
            "holtburg",
            "night",
            "lamps-street",
            "doorway",
            "dungeon",
            "rain-street",
            "snow-village",
        ];
        let stations: Vec<_> = hifi_stations::isolation_stations(&store)
            .into_iter()
            .filter(|s| names.contains(&s.name))
            .collect();
        assert_eq!(stations.len(), names.len());
        for station in &stations {
            let mut gpu = wgpu_device(W, H, true);
            let mut shot = Shot::open(&store, &mut gpu, station);
            gpu.set_frame_digest(true);
            for _ in 0..6 {
                shot.step(&mut gpu);
            }
            let (off_px, off, off_census) = frame_and_census(&mut shot, &mut gpu);
            let (again_px, _, _) = frame_and_census(&mut shot, &mut gpu);
            assert_eq!(
                moved(&off_px, &again_px),
                0,
                "{}: the frame drawn again is not the same, so this test cannot see",
                station.name
            );

            assert!(set(&mut shot, &mut gpu, fidelity(EVERY_BOX)));
            for _ in 0..30 {
                shot.draw(&mut gpu);
            }
            assert!(
                gpu.hifi_settle(std::time::Duration::from_secs(30)),
                "{}: the presentation's background work did not finish",
                station.name
            );
            shot.draw(&mut gpu);
            let report = gpu.hifi_report().expect("installed");
            assert!(report.composited, "{}: {report:?}", station.name);
            assert!(gpu.hifi_failed().is_none(), "{}", station.name);
            let on_px = capture(&mut gpu);
            let changed = moved(&off_px, &on_px);
            eprintln!("{}: every box ticked changes {changed} bytes", station.name);
            assert!(changed > 0, "{}: the effects drew nothing", station.name);
            let held = gpu
                .device_objects()
                .expect("the wgpu device counts its objects");

            assert!(set(
                &mut shot,
                &mut gpu,
                fidelity(&format!("{EVERY_BOX},Interface=0"))
            ));
            assert!(
                gpu.hifi_sidecar_mut().is_none(),
                "{}: leaving Horizon",
                station.name
            );
            assert_eq!(
                shot.scene.draw.hifi_lamp_blocks_looked(),
                0,
                "{}: the lamps found were kept",
                station.name
            );
            let (back_px, back, back_census) = frame_and_census(&mut shot, &mut gpu);
            let what = station.name;
            assert_eq!(moved(&off_px, &back_px), 0, "{what}: the pixels");
            assert_eq!(back.digest, off.digest, "{what}: the recording");
            assert_eq!(back.draw_calls, off.draw_calls, "{what}: the draws");
            assert_eq!(
                back.live_textures, off.live_textures,
                "{what}: the textures"
            );
            assert_eq!(
                back.descriptors, off.descriptors,
                "{what}: the texture slots"
            );
            assert_eq!(back.passes, off.passes, "{what}: the render passes");
            assert_eq!(back.frames, off.frames, "{what}: the frames");
            assert_eq!(back_census, off_census, "{what}: the scene census");
            let (off_objects, back_objects) = (
                off.objects.expect("the wgpu device counts its objects"),
                back.objects.expect("the wgpu device counts its objects"),
            );
            assert!(
                held.texture_memory > off_objects.texture_memory,
                "{what}: the device's count did not see the effects' targets: \
                 {off_objects:?} {held:?}"
            );
            assert_eq!(
                back_objects, off_objects,
                "{what}: the effects left an object on the device"
            );
        }
    }

    /// Behaviour: hifi.off.unticking-an-effect-gives-back-what-it-held
    /// With the better lighting alone ticked, then every box, then the better lighting alone
    /// again, the device holds as much texture, buffer and traced-scene memory as it did the
    /// first time: what the other effects held is given back as soon as their boxes are
    /// unticked, not only when the last box is.
    #[test]
    fn unticking_an_effect_gives_back_the_memory_it_held() {
        let _gpu = crate::common::gpu_lock();
        let store = crate::common::dats();
        let mut gpu = wgpu_device(W, H, true);
        let mut shot = still_view(&store, &mut gpu);
        let held = |shot: &mut Shot, gpu: &mut Gpu, spec: &str| {
            set(shot, gpu, fidelity(spec));
            for _ in 0..10 {
                shot.draw(gpu);
            }
            assert!(gpu.hifi_settle(std::time::Duration::from_secs(30)));
            shot.draw(gpu);
            assert!(gpu.hifi_failed().is_none(), "{spec}");
            gpu.device_objects()
                .expect("the wgpu device counts its objects")
        };
        let lighting = held(&mut shot, &mut gpu, "Lighting=1");
        let every = held(&mut shot, &mut gpu, EVERY_BOX);
        assert!(
            every.texture_memory > lighting.texture_memory,
            "{lighting:?} {every:?}"
        );
        let again = held(&mut shot, &mut gpu, "Lighting=1");
        eprintln!("lighting {lighting:?}\nevery box {every:?}\nlighting again {again:?}");
        assert_eq!(
            again.texture_memory, lighting.texture_memory,
            "texture memory"
        );
        assert_eq!(again.buffer_memory, lighting.buffer_memory, "buffer memory");
        assert_eq!(
            again.acceleration_structure_memory, lighting.acceleration_structure_memory,
            "traced-scene memory"
        );
    }

    /// Behaviour: hifi.off.passthrough-is-pixel-identical
    /// At every station the frame drawn through the presentation with no pass of its own is the
    /// ordinary frame, byte for byte.
    #[test]
    fn passthrough_draws_every_station_byte_for_byte() {
        let _gpu = crate::common::gpu_lock();
        let store = crate::common::dats();
        let mut gpu = wgpu_device(W, H, true);
        let stations = hifi_stations::isolation_stations(&store);
        for station in &stations {
            let mut shots = Vec::new();
            for prefs in [FidelityPreferences::default(), fidelity("Debug=1")] {
                let mut shot = Shot::open(&store, &mut gpu, station);
                set(&mut shot, &mut gpu, prefs);
                for _ in 0..6 {
                    shot.step(&mut gpu);
                }
                shots.push(capture(&mut gpu));
                if prefs.any_effective() {
                    let report = gpu.hifi_report().expect("installed");
                    assert!(report.composited, "{}: {report:?}", station.name);
                }
                set(&mut shot, &mut gpu, FidelityPreferences::default());
            }
            let changed = moved(&shots[0], &shots[1]);
            eprintln!("{}: {changed} bytes differ", station.name);
            assert_eq!(
                changed, 0,
                "{}: pass-through changed the picture",
                station.name
            );
        }
    }

    /// A sidecar that fails the way it is told to.
    struct Failing {
        in_prepare: bool,
    }

    impl FrameSidecar for Failing {
        fn prepare(&mut self, _cx: &mut SidecarContext<'_>) -> Result<SidecarFrame, SidecarError> {
            if self.in_prepare {
                return Err(SidecarError("planted in preparation".into()));
            }
            Ok(SidecarFrame::Composite)
        }

        fn encode(
            &mut self,
            cx: &mut SidecarContext<'_>,
            encoder: &mut wgpu::CommandEncoder,
            target: &wgpu::TextureView,
        ) -> Result<(), SidecarError> {
            // A pass the device refuses: a depth target of another size than the colour.
            let depth = cx.device().create_texture(&wgpu::TextureDescriptor {
                label: Some("wrong size"),
                size: wgpu::Extent3d {
                    width: 7,
                    height: 5,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = depth.create_view(&wgpu::TextureViewDescriptor::default());
            drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("refused"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations::default(),
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &view,
                    depth_ops: Some(wgpu::Operations::default()),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            }));
            Ok(())
        }

        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    }

    /// Behaviour: hifi.off.a-failure-falls-back-to-identical-pixels-and-uninstalls
    /// A presentation that fails, by its own error or by a step the device refuses, leaves that
    /// frame exactly the ordinary frame and is removed; the scene does not put it back until the
    /// preferences change.
    #[test]
    fn a_failing_presentation_draws_the_ordinary_frame_and_is_removed() {
        let _gpu = crate::common::gpu_lock();
        let store = crate::common::dats();
        let mut gpu = wgpu_device(W, H, true);
        let mut shot = still_view(&store, &mut gpu);
        let (off_px, off) = frame(&mut shot, &mut gpu);
        for in_prepare in [true, false] {
            gpu.hifi_install(Box::new(Failing { in_prepare }))
                .expect("installs");
            let (px, counters) = frame(&mut shot, &mut gpu);
            assert_eq!(moved(&off_px, &px), 0, "in_prepare={in_prepare}");
            assert_eq!(counters.digest, off.digest);
            assert!(gpu.hifi_sidecar_mut().is_none(), "in_prepare={in_prepare}");
            assert!(gpu.hifi_failed().is_some());
            let (after_px, _) = frame(&mut shot, &mut gpu);
            assert_eq!(moved(&off_px, &after_px), 0);
        }
        // The scene's own presentation, lost, is not reinstalled by an unchanged poll.
        assert!(set(&mut shot, &mut gpu, fidelity("Debug=1")));
        assert!(gpu.hifi_sidecar_mut().is_some());
        drop(gpu.hifi_take());
        assert!(!set(&mut shot, &mut gpu, fidelity("Debug=1")));
        assert!(
            gpu.hifi_sidecar_mut().is_none(),
            "reinstalled without a change"
        );
        assert!(set(&mut shot, &mut gpu, fidelity("Debug=2")));
        assert!(
            gpu.hifi_sidecar_mut().is_some(),
            "a change installs it again"
        );
    }
}
