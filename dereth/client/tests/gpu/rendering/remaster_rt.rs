//! The remaster's acceleration structures, where the device traces rays: what they hold at a
//! station, and rays cast from the eye through the picture finding what the picture shows, as
//! far away as it shows it.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.
//!
//! Behaviour: none (experimental)

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_dat::RetailDatStore;
use dereth_render::device::{Backend, DeviceConfig, Gpu};
use dereth_scene::world_scene::{RemasterAcceleration, RemasterCapture};

use crate::instruments::remaster_frames;
use crate::instruments::stations::{Shot, Station};

const W: u32 = 640;
const H: u32 = 480;

/// Every 8th pixel a side is checked.
const STEP: u32 = 8;

/// Every instance but the viewer's own body.
const ALL_BUT_BODY: u8 = !16;

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

fn station(store: &RetailDatStore, name: &str) -> Station {
    remaster_frames::stations(store, &[name])
        .pop()
        .unwrap_or_else(|| panic!("the {name} station"))
}

/// One station's frame with the structures kept: the picture, what the structures held, and
/// the check's rays; `None` where the device does not trace rays.
struct Traced {
    capture: RemasterCapture,
    counts: RemasterAcceleration,
    rays: Vec<(f32, u32)>,
}

fn traced(store: &Arc<RetailDatStore>, station: &Station) -> Option<Traced> {
    let mut gpu = device();
    let mut shot = Shot::open(store, &mut gpu, station);
    shot.poll(&mut gpu);
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_light(dereth_scene::world_scene::RemasterLight::Pixel, &mut gpu);
    if !shot.scene.draw.remaster_traces_rays(&mut gpu) {
        eprintln!("{}: no ray queries", gpu.adapter_name());
        return None;
    }
    shot.scene.draw.set_remaster_acceleration(true, &mut gpu);
    for _ in 0..6 {
        shot.step_held(&mut gpu);
    }
    let capture = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew");
    let counts = shot
        .scene
        .draw
        .remaster_acceleration(&mut gpu)
        .expect("the remaster is on");
    let rays = shot
        .scene
        .draw
        .remaster_acceleration_check(&mut gpu, STEP, ALL_BUT_BODY)
        .expect("the structures were built");
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    Some(Traced {
        capture,
        counts,
        rays,
    })
}

/// How the checked rays compare with the picture: those whose pixel shows the world and whose
/// ray met something within 2% (and 10 cm) of the distance the picture shows it at, those that
/// met something else, those that met nothing; and of the pixels that show the sky, those whose
/// ray met something.
#[derive(Debug, Default)]
struct Agreement {
    agree: usize,
    differ: usize,
    missed: usize,
    sky_hit: usize,
}

#[allow(clippy::cast_precision_loss)]
fn agreement(t: &Traced) -> Agreement {
    let c = &t.capture;
    let inv = glam::Mat4::from_cols_array(&c.view_projection).inverse();
    let eye = glam::Vec3::from(c.eye);
    let (w, h) = (c.width as usize, c.height as usize);
    let cols = (c.width.div_ceil(STEP)) as usize;
    let mut out = Agreement::default();
    for (k, (hit, _)) in t.rays.iter().enumerate() {
        let (x, y) = ((k % cols) * STEP as usize, (k / cols) * STEP as usize);
        if x >= w || y >= h {
            continue;
        }
        let i = y * w + x;
        let world = c.class[i] != 0 && c.depth[i] < 1.0;
        if !world {
            if *hit >= 0.0 {
                out.sky_hit += 1;
            }
            continue;
        }
        let ndc = glam::Vec4::new(
            (x as f32 + 0.5) / w as f32 * 2.0 - 1.0,
            1.0 - (y as f32 + 0.5) / h as f32 * 2.0,
            c.depth[i],
            1.0,
        );
        let p = inv * ndc;
        let d = (p.truncate() / p.w - eye).length();
        if *hit < 0.0 {
            out.missed += 1;
        } else if (hit - d).abs() <= 0.02 * d + 0.1 {
            out.agree += 1;
        } else {
            out.differ += 1;
        }
    }
    out
}

/// **Rays cast from the eye find what the picture shows, as far away as it shows it**: at the
/// town, the forest and a room, the acceleration structures hold statics, their ground and the
/// meshes they need, and of the checked pixels that show the world, at least 95% have their ray
/// meet something within 2% of the distance the picture shows it at.
#[test]
fn rays_cast_from_the_eye_find_what_the_picture_shows() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    for name in ["holtburg", "forest", "indoor"] {
        let st = station(&store, name);
        let Some(t) = traced(&store, &st) else {
            return;
        };
        let a = agreement(&t);
        eprintln!("{name}: {:?}; {a:?}", t.counts);
        assert!(
            t.counts.statics > 0 && t.counts.meshes > 0,
            "{name}: statics held"
        );
        let shown = a.agree + a.differ + a.missed;
        assert!(shown > 1000, "{name}: the world is in the picture: {a:?}");
        assert!(a.agree * 100 >= shown * 95, "{name}: {a:?}");
    }
}

/// **A device made without ray queries traces none, and the frame draws without them**: with
/// the device asked for no ray queries, the remaster says it traces none, its structures hold
/// nothing even when kept, and its bounced light, asked to trace, is found from the screen.
#[test]
fn a_device_made_without_ray_queries_traces_none() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let cfg = DeviceConfig {
        width: W,
        height: H,
        remaster: true,
        remaster_ray_queries: false,
        debug: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(Backend::Vulkan, None, &cfg).expect("a Vulkan device");
    let st = station(&store, "holtburg");
    let mut shot = Shot::open(&store, &mut gpu, &st);
    shot.poll(&mut gpu);
    shot.scene
        .draw
        .set_remaster_gi(dereth_scene::world_scene::RemasterGi::on(), &mut gpu);
    shot.scene
        .draw
        .set_remaster(true, &store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_light(dereth_scene::world_scene::RemasterLight::Pixel, &mut gpu);
    shot.scene.draw.set_remaster_acceleration(true, &mut gpu);
    for _ in 0..3 {
        shot.step_held(&mut gpu);
    }
    let traces = shot.scene.draw.remaster_traces_rays(&mut gpu);
    let counts = shot
        .scene
        .draw
        .remaster_acceleration(&mut gpu)
        .expect("the remaster is on");
    let check = shot
        .scene
        .draw
        .remaster_acceleration_check(&mut gpu, STEP, ALL_BUT_BODY);
    let capture = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew");
    shot.scene
        .draw
        .set_remaster(false, &store, &mut gpu)
        .expect("the remaster stops");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    eprintln!("without ray queries: traces {traces}, {counts:?}");
    assert!(!traces);
    assert_eq!(counts, RemasterAcceleration::default());
    assert!(check.is_none(), "nothing to check");
    assert!(capture.class.iter().any(|c| *c != 0), "the frame is drawn");
}
