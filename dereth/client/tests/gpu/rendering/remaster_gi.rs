//! The remaster's bounced light: never taking light away, so no pixel of any station comes out
//! darker with it than with the direct light alone, the tree in the rain among them; found by
//! probes traced against the scene where the device traces rays, and from the screen otherwise;
//! and adding light where surfaces face each other.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.
//!
//! Behaviour: none (experimental)

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_dat::RetailDatStore;
use dereth_render::device::{Backend, DeviceConfig, Gpu};
use dereth_scene::world_scene::{
    RemasterAo, RemasterCapture, RemasterGi, RemasterLight, RemasterShadows, RemasterTracedShadows,
};

use crate::instruments::remaster_frames;
use crate::instruments::stations::{Shot, Station};

const W: u32 = 640;
const H: u32 = 480;

fn device() -> Gpu {
    let cfg = DeviceConfig {
        width: W,
        height: H,
        remaster: true,
        debug: true,
        ..DeviceConfig::default()
    };
    let gpu = Gpu::new_on(Backend::Vulkan, None, &cfg).expect("a Vulkan device");
    assert_eq!(
        gpu.adapter_kind(),
        dereth_render::device::AdapterKind::Hardware,
        "{} is a software rasterizer",
        gpu.adapter_name()
    );
    gpu
}

/// The remaster at `station`, its light per pixel under the sun's shadows and the ambient
/// occlusion, with the bounced light `gi`, stepped `frames` frames held still; the last frame,
/// and whether the device traces rays.
fn run(
    store: &Arc<RetailDatStore>,
    station: &Station,
    gi: RemasterGi,
    frames: usize,
) -> (RemasterCapture, bool) {
    let mut gpu = device();
    let mut shot = Shot::open(store, &mut gpu, station);
    shot.poll(&mut gpu);
    shot.scene.draw.set_remaster_shadows(
        RemasterShadows {
            on: true,
            ..RemasterShadows::default()
        },
        &mut gpu,
    );
    shot.scene.draw.set_remaster_ao(
        RemasterAo {
            on: true,
            ..RemasterAo::default()
        },
        &mut gpu,
    );
    shot.scene.draw.set_remaster_gi(gi, &mut gpu);
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_light(RemasterLight::Pixel, &mut gpu);
    let traces = shot.scene.draw.remaster_traces_rays(&mut gpu);
    for _ in 0..frames {
        shot.step_held(&mut gpu);
    }
    let capture = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew");
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    (capture, traces)
}

/// Of the pixels of `on` and `off`, those darker in any channel with the bounced light, and
/// those lighter in any.
fn compared(on: &RemasterCapture, off: &RemasterCapture) -> (usize, usize) {
    let mut darker = 0;
    let mut lighter = 0;
    for (a, b) in on
        .color
        .as_chunks::<4>()
        .0
        .iter()
        .zip(off.color.as_chunks::<4>().0)
    {
        if (0..3).any(|k| a[k] < b[k]) {
            darker += 1;
        }
        if (0..3).any(|k| a[k] > b[k]) {
            lighter += 1;
        }
    }
    (darker, lighter)
}

/// **No pixel of any station is darker with the bounced light than with the direct light
/// alone**: at every station of the light study, the forest in the rain among them (the tree in
/// the rain), under the sun's shadows and the ambient occlusion, the bounced light found by
/// probes (where the device traces rays) and from the screen leaves no pixel darker in any
/// channel than the same frame without it; and outdoors by day it lightens some.
#[test]
fn no_pixel_of_any_station_is_darker_with_the_bounced_light() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let stations = remaster_frames::stations(&store, &remaster_frames::STATIONS);
    assert!(
        stations.iter().any(|s| s.name == "rain-forest"),
        "the tree in the rain"
    );
    for st in &stations {
        let (off, _) = run(&store, st, RemasterGi::default(), 8);
        let (rays, traces) = run(&store, st, RemasterGi::on(), 8);
        let (screen, _) = run(
            &store,
            st,
            RemasterGi {
                rays: false,
                ..RemasterGi::on()
            },
            8,
        );
        let by_rays = compared(&rays, &off);
        let by_screen = compared(&screen, &off);
        eprintln!(
            "{}: darker, lighter by the probes{} {by_rays:?}, by the screen {by_screen:?}",
            st.name,
            if traces {
                ""
            } else {
                " (no ray queries: the screen)"
            }
        );
        assert_eq!(by_rays.0, 0, "{}: the probes darken", st.name);
        assert_eq!(by_screen.0, 0, "{}: the screen darkens", st.name);
        // Only where there are probes: the screen finds little on open ground.
        if traces && (st.name == "holtburg" || st.name == "forest") {
            assert!(by_rays.1 > 1000, "{}: the probes add light", st.name);
        }
    }
}

/// The structures a frame made in `gi` and `traced` from when the scene has settled: how many
/// bottom-level structures it made over ten more frames, and how many instances it holds. `None`
/// where the device doesn't trace rays.
fn structures_made_when_settled(
    store: &Arc<RetailDatStore>,
    station: &Station,
    gi: RemasterGi,
    traced: bool,
) -> Option<(u64, u32)> {
    let cfg = DeviceConfig {
        width: W,
        height: H,
        remaster: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let mut shot = Shot::open(store, &mut gpu, station);
    shot.poll(&mut gpu);
    shot.scene.draw.set_remaster_shadows(
        RemasterShadows {
            on: true,
            ..RemasterShadows::default()
        },
        &mut gpu,
    );
    shot.scene.draw.set_remaster_gi(gi, &mut gpu);
    shot.scene.draw.set_remaster_traced_shadows(
        RemasterTracedShadows {
            on: traced,
            ..RemasterTracedShadows::default()
        },
        &mut gpu,
    );
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_light(RemasterLight::Pixel, &mut gpu);
    if !shot.scene.draw.remaster_traces_rays(&mut gpu) {
        return None;
    }
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    for _ in 0..30 {
        shot.step_held(&mut gpu);
    }
    let before = shot
        .scene
        .draw
        .remaster_acceleration(&mut gpu)
        .expect("the remaster drew");
    for _ in 0..10 {
        shot.step_held(&mut gpu);
    }
    let after = shot
        .scene
        .draw
        .remaster_acceleration(&mut gpu)
        .expect("the remaster drew");
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    Some((after.made - before.made, after.statics))
}

/// **A settled scene's acceleration structures are kept**: once the scene has stopped changing, no
/// frame makes another bottom-level structure, whichever pass it is that traces against them:
/// the bounced light alone, a traced shadow alone, or both. They are held all the while.
#[test]
fn a_settled_scene_keeps_its_acceleration_structures_whichever_pass_traces_them() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let station = remaster_frames::stations(&store, &["holtburg"])
        .pop()
        .expect("the station");
    let on = RemasterGi {
        on: true,
        ..RemasterGi::default()
    };
    let off = RemasterGi {
        on: false,
        ..RemasterGi::default()
    };
    for (what, gi, traced) in [
        ("the bounced light", on, false),
        ("a traced shadow", off, true),
        ("both", on, true),
    ] {
        let Some((made, statics)) = structures_made_when_settled(&store, &station, gi, traced)
        else {
            eprintln!("the device doesn't trace rays");
            return;
        };
        eprintln!("{what}: {made} made over ten frames, {statics} statics held");
        assert_eq!(made, 0, "{what}: structures made in a settled scene");
        assert!(statics > 0, "{what}: the structures are held");
    }
}
