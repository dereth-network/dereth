//! The remaster's ambient occlusion: off it is 1 everywhere; on, a still view settles to a still
//! picture of it; open ground keeps its sky while what stands close round a pixel takes some of
//! it; and rooms and what is in them take none unless asked.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.
//!
//! Behaviour: none (experimental)

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_dat::RetailDatStore;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::world_scene::{RemasterAo, RemasterCapture};

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
    let gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
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

/// One frame's occlusion and the remaster's pictures with it.
struct Frame {
    ao: Vec<f32>,
    capture: RemasterCapture,
}

/// The remaster at `station` with the occlusion `ao`, its light per pixel, stepped `frames`
/// frames with the view held still; the last `keep` frames.
fn run(
    store: &Arc<RetailDatStore>,
    station: &Station,
    ao: RemasterAo,
    frames: usize,
    keep: usize,
) -> Vec<Frame> {
    run_lit(store, station, ao, frames, keep, true)
}

/// [`run`], the light per pixel when `pixel`, per vertex otherwise.
fn run_lit(
    store: &Arc<RetailDatStore>,
    station: &Station,
    ao: RemasterAo,
    frames: usize,
    keep: usize,
    pixel: bool,
) -> Vec<Frame> {
    let mut gpu = device();
    let mut shot = Shot::open(store, &mut gpu, station);
    shot.poll(&mut gpu);
    shot.scene.draw.set_remaster_ao(ao, &mut gpu);
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    if pixel {
        shot.scene
            .draw
            .set_remaster_light(dereth_scene::world_scene::RemasterLight::Pixel, &mut gpu);
    }
    let mut out = Vec::new();
    for k in 0..frames {
        shot.step_held(&mut gpu);
        if k + keep >= frames {
            let capture = shot
                .scene
                .draw
                .remaster_capture(&mut gpu)
                .expect("the remaster drew");
            let ao = match shot.scene.draw.remaster_ao_picture(&mut gpu) {
                Some((w, h, v)) => {
                    assert_eq!((w, h), (capture.width, capture.height));
                    v
                }
                None => vec![1.0; (capture.width * capture.height) as usize],
            };
            out.push(Frame { ao, capture });
        }
    }
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    out
}

fn on() -> RemasterAo {
    RemasterAo {
        on: true,
        ..RemasterAo::default()
    }
}

/// The occlusion of the pixels of each class: their count, their mean, and how many are below
/// `under`.
fn by_class(f: &Frame, under: f32) -> [(usize, f64, usize); 6] {
    let mut out = [(0usize, 0.0f64, 0usize); 6];
    for (i, c) in f.capture.class.iter().enumerate() {
        let Some(slot) = out.get_mut(usize::from(*c)) else {
            continue;
        };
        let a = f.ao[i];
        slot.0 += 1;
        slot.1 += f64::from(a);
        slot.2 += usize::from(a < under);
    }
    for slot in &mut out {
        #[allow(clippy::cast_precision_loss)] // pixel counts
        if slot.0 > 0 {
            slot.1 /= slot.0 as f64;
        }
    }
    out
}

/// **A still view settles to a still picture**: with the view and the clock held, the occlusion
/// of the last frames is the same in every pixel, and so is every pixel the world's light is
/// resolved for (the sky, which the resolve leaves, is not the occlusion's).
#[test]
fn a_still_view_settles_to_a_still_occlusion() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "holtburg");
    let frames = run(&store, &st, on(), 24, 3);
    let changed = |a: &Frame, b: &Frame| a.ao.iter().zip(&b.ao).filter(|(x, y)| x != y).count();
    for pair in frames.windows(2) {
        let n = changed(&pair[0], &pair[1]);
        let (a, b) = (&pair[0].capture, &pair[1].capture);
        let lit = (0..a.class.len())
            .filter(|i| a.class[*i] != 0 && a.color[i * 4..i * 4 + 4] != b.color[i * 4..i * 4 + 4])
            .count();
        eprintln!("holtburg, still: {n} occlusion values and {lit} resolved pixels changed");
        assert_eq!(n, 0, "the occlusion settled");
        assert_eq!(lit, 0, "the resolved world settled");
    }
}

/// **Open ground keeps its sky, and what stands close round a pixel takes some of it**: in the
/// forest, the ground's mean occlusion stays near 1 while what the sun lights (the trees) is
/// occluded measurably, every value is between 0 and 1, and where the G-buffer holds nothing (the sky)
/// there is none.
#[test]
fn open_ground_keeps_its_sky_and_close_surroundings_take_some() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "forest");
    let f = run(&store, &st, on(), 12, 1).pop().expect("a frame");
    let classes = by_class(&f, 0.8);
    eprintln!("forest, by class (pixels, mean, under 0.8): {classes:?}");
    assert!(f.ao.iter().all(|a| (0.0..=1.0).contains(a)));
    let (ground, sun, sky) = (classes[1], classes[2], classes[0]);
    assert!(ground.0 > 10_000 && sun.0 > 1_000, "{classes:?}");
    assert!(ground.1 > 0.97, "open ground: mean {:.3}", ground.1);
    assert!(
        sun.2 * 100 > sun.0,
        "objects occluded under 0.8: {} of {}",
        sun.2,
        sun.0
    );
    let sky_occluded = f
        .capture
        .class
        .iter()
        .zip(&f.ao)
        .filter(|(c, a)| **c == 0 && **a != 1.0)
        .count();
    assert!(sky.0 > 0, "sky pixels: {classes:?}");
    assert_eq!(sky_occluded, 0, "the sky takes none");
    let off = run(&store, &st, RemasterAo::default(), 2, 1)
        .pop()
        .expect("a frame");
    assert!(off.ao.iter().all(|a| *a == 1.0), "off, 1 everywhere");
}

/// **Rooms and what is in them take no occlusion unless asked**: in a Holtburg room, their
/// pixels' occlusion is 1 with the default settings, and some is below 1 with rooms asked for.
#[test]
fn rooms_take_no_occlusion_unless_asked() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "indoor");
    let f = run(&store, &st, on(), 8, 1).pop().expect("a frame");
    let classes = by_class(&f, 1.0);
    eprintln!("indoor, by class (pixels, mean, under 1): {classes:?}");
    assert!(classes[4].0 > 10_000, "room pixels: {classes:?}");
    assert_eq!(classes[3].2 + classes[4].2, 0, "no room pixel occluded");
    let asked = RemasterAo {
        indoors: true,
        ..on()
    };
    let f = run(&store, &st, asked, 8, 1).pop().expect("a frame");
    let classes = by_class(&f, 1.0);
    eprintln!("indoor, rooms asked, by class: {classes:?}");
    assert!(
        classes[4].2 * 10 > classes[4].0,
        "room pixels occluded: {classes:?}"
    );
}

/// **The occlusion only takes ambient light away, and only from the light per pixel**: in the
/// forest, the resolved world with it is nowhere brighter than without it, darker wherever it is
/// measurably below 1, and the same wherever it is 1; with the light per vertex, retail's own
/// arithmetic, it changes no pixel.
#[test]
fn the_occlusion_only_darkens_and_only_the_light_per_pixel() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "forest");
    let with = run(&store, &st, on(), 12, 1).pop().expect("a frame");
    let without = run(&store, &st, RemasterAo::default(), 12, 1)
        .pop()
        .expect("a frame");
    let (mut brighter, mut darker, mut open_changed, mut occluded) =
        (0usize, 0usize, 0usize, 0usize);
    for i in 0..with.capture.class.len() {
        if with.capture.class[i] == 0 {
            continue;
        }
        let a = &with.capture.color[i * 4..i * 4 + 3];
        let b = &without.capture.color[i * 4..i * 4 + 3];
        if a.iter().zip(b).any(|(x, y)| x > y) {
            brighter += 1;
        }
        if a.iter().zip(b).any(|(x, y)| x < y) {
            darker += 1;
        }
        if with.ao[i] == 1.0 && a != b {
            open_changed += 1;
        }
        occluded += usize::from(with.ao[i] < 0.9);
    }
    eprintln!(
        "forest: {darker} pixels darker, {brighter} brighter, {open_changed} unoccluded changed, \
         {occluded} occluded under 0.9"
    );
    assert_eq!(brighter, 0, "nowhere brighter");
    assert_eq!(open_changed, 0, "unchanged where unoccluded");
    assert!(
        darker * 2 > occluded,
        "darker where occluded: {darker} of {occluded}"
    );
    assert!(occluded > 5_000, "occluded pixels: {occluded}");
    let vertex_with = run_lit(&store, &st, on(), 12, 1, false)
        .pop()
        .expect("a frame");
    let vertex_without = run_lit(&store, &st, RemasterAo::default(), 12, 1, false)
        .pop()
        .expect("a frame");
    let lit_changed = (0..vertex_with.capture.class.len())
        .filter(|i| {
            vertex_with.capture.class[*i] != 0
                && vertex_with.capture.color[i * 4..i * 4 + 4]
                    != vertex_without.capture.color[i * 4..i * 4 + 4]
        })
        .count();
    assert_eq!(lit_changed, 0, "the light per vertex is retail's own");
}
