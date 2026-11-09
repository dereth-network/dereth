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

/// The portal by the meeting hall in Holtburg, as the server places it: its look, its room and
/// where it stands in the block, turned as it stands.
const HALL_PORTAL_SETUP: u32 = 0x0200_01B3;
const HALL_PORTAL_CELL: u32 = 0xA9B4_017A;
const HALL_PORTAL_ORIGIN: dereth_primitives::Vec3 =
    dereth_primitives::Vec3::new(159.225, 39.0226, 96.005);
const HALL_PORTAL_HEADING: f32 = -39.5;

/// The meeting hall and its portal from outside, at `clock`, from `(x, y)` block-local, `eye`
/// metres over the ground.
fn hall_station(
    clock: crate::instruments::stations::Clock,
    (x, y, eye): (f32, f32, f32),
) -> Station {
    use crate::instruments::stations::{Place, HOLTBURG};
    Station {
        name: "hall-portal",
        why: "the portal by Holtburg's meeting hall and the hall's walls round it",
        place: Place::Look {
            block: HOLTBURG,
            x,
            y,
            eye,
            at: [159.2, 39.0, 97.0],
        },
        clock,
        weather: false,
        land_radius: 3,
    }
}

/// Where the hall is seen from: before its door, the portal in the doorway.
const HALL_VIEWS: [(&str, (f32, f32, f32)); 1] = [("door", (151.0, 29.0, 1.8))];

/// [`run`] at `station` with the hall's portal standing in it.
fn run_with_portal(
    store: &Arc<RetailDatStore>,
    station: &Station,
    gi: RemasterGi,
    frames: usize,
) -> RemasterCapture {
    let mut gpu = device();
    let mut shot = Shot::open(store, &mut gpu, station);
    shot.create_object_in(
        0x8300_0071,
        HALL_PORTAL_SETUP,
        HALL_PORTAL_CELL,
        HALL_PORTAL_ORIGIN,
        HALL_PORTAL_HEADING,
    );
    shot.poll(&mut gpu);
    shot.scene.draw.set_remaster_shadows(
        RemasterShadows {
            on: true,
            ..RemasterShadows::default()
        },
        &mut gpu,
    );
    shot.scene.draw.set_remaster_gi(gi, &mut gpu);
    // In linear light and bloomed, as the client draws it, where bright light is not cut off at
    // white.
    shot.scene.draw.set_remaster_post(
        dereth_scene::world_scene::RemasterPost {
            hdr: true,
            ..dereth_scene::world_scene::RemasterPost::default()
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
    capture
}

/// Not a claim: draws the meeting hall's portal in Holtburg by day and by night, without the
/// bounced light and with it, into `DERETH_REMASTER_CAPTURE_DIR`, for a person to look at.
#[test]
#[ignore = "an instrument: set DERETH_REMASTER_CAPTURE_DIR and run with --ignored"]
fn draw_the_hall_portal_with_and_without_the_bounced_light() {
    use crate::instruments::stations::{write_png, Clock};
    let Some(dir) = std::env::var_os("DERETH_REMASTER_CAPTURE_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("the capture folder");
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    for (view, at) in HALL_VIEWS {
        for (when, clock) in [("day", Clock::Sunny(0.4)), ("night", Clock::Sunny(0.0))] {
            let station = hall_station(clock, at);
            for (what, gi) in [("off", RemasterGi::default()), ("on", RemasterGi::on())] {
                let c = run_with_portal(&store, &station, gi, 300);
                let path = dir.join(format!("hall-portal-{view}-{when}-gi-{what}.png"));
                write_png(&path, c.width, c.height, &c.color);
                eprintln!("{}", path.display());
            }
        }
    }
}

/// **A portal's light bounces softly**: at night before the door of Holtburg's meeting hall, its
/// portal in the doorway, the bounced light adds at most a quarter of the display's range to any
/// pixel of the hall, and more than twenty levels to under two thousand of them. Retail's lights
/// fall off over the distance, so a wall close to the portal is lit far past white; what it sends
/// on is what it is drawn with, at most white.
#[test]
fn a_portals_light_bounces_softly_onto_the_walls_round_it() {
    use crate::instruments::stations::Clock;
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let station = hall_station(Clock::Sunny(0.0), HALL_VIEWS[0].1);
    let off = run_with_portal(&store, &station, RemasterGi::default(), 300);
    let on = run_with_portal(&store, &station, RemasterGi::on(), 300);
    let gained: Vec<u8> = on
        .color
        .as_chunks::<4>()
        .0
        .iter()
        .zip(off.color.as_chunks::<4>().0)
        .map(|(a, b)| (0..3).map(|k| a[k].saturating_sub(b[k])).max().unwrap_or(0))
        .collect();
    let most = gained.iter().copied().max().unwrap_or(0);
    let over = gained.iter().filter(|g| **g > 20).count();
    eprintln!("the bounced light adds at most {most} levels, over twenty to {over} pixels");
    assert!(
        most <= 64,
        "a pixel is {most} levels lighter with the bounced light"
    );
    assert!(over < 2000, "{over} pixels are over twenty levels lighter");
}
