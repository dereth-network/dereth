//! The remaster's upscaler: on, the world is drawn at the render size and the frame comes out at
//! the display's; off, the frame is the one drawn without it. Presence and choice, never quality
//! or time.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.
//!
//! Behaviour: none (experimental)

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_dat::RetailDatStore;
use dereth_primitives::num::math;
use dereth_primitives::Vec3;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::world_scene::{
    RemasterCapture, RemasterLight, RemasterPost, RemasterUpscale, RemasterUpscaleMode,
    RemasterWeather,
};

use crate::instruments::remaster_frames;
use crate::instruments::stations::{self, Shot, Station};

const W: u32 = 640;
const H: u32 = 480;

fn device(width: u32, height: u32) -> Gpu {
    device_checked(width, height, true)
}

/// [`device`], under the validation layer or not: NVIDIA's library leaves its own objects to the
/// end of the process, which the layer reports.
fn device_checked(width: u32, height: u32, debug: bool) -> Gpu {
    #[allow(unused_mut)]
    let mut cfg = DeviceConfig {
        width,
        height,
        remaster: true,
        debug,
        ..DeviceConfig::default()
    };
    // What DLSS asks of the device, as the client asks for it: nothing unless its library stands
    // beside the test binary.
    #[cfg(all(feature = "dlss", windows))]
    {
        cfg.remaster_extensions = dereth_scene::world_scene::remaster_device_extensions();
    }
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

/// FSR at `scale`, with a little sharpening.
fn fsr(scale: f32) -> RemasterUpscale {
    RemasterUpscale {
        mode: RemasterUpscaleMode::Fsr,
        scale,
        sharpness: 0.5,
        ..RemasterUpscale::default()
    }
}

/// One frame of a run: what it drew, the pipelines built by then, and the frames upscaled by then.
struct Frame {
    capture: RemasterCapture,
    pipelines: u64,
    upscaled: u64,
    #[cfg(target_os = "macos")]
    scaled: u64,
}

/// The camera as the station first drew it.
#[derive(Clone, Copy)]
struct Start {
    position: Vec3,
    yaw: f32,
    pitch: f32,
}

/// What happens before each step: `k` the step, from 0.
type Before<'a> = &'a dyn Fn(usize, &mut Shot, &mut Gpu, Start);

/// How a run is drawn: its post passes, its upscaler from the start, and what happens before each
/// step.
struct Run<'a> {
    post: RemasterPost,
    upscale: Option<RemasterUpscale>,
    before: Before<'a>,
    /// Under the validation layer.
    checked: bool,
}

impl Default for Run<'_> {
    fn default() -> Self {
        Self {
            post: RemasterPost::default(),
            upscale: None,
            before: &|_, _, _, _| {},
            checked: true,
        }
    }
}

/// The remaster at `station` drawn as `run` says for `frames` steps, the clock held; the last
/// `keep` frames, and no error from the validation layer. The upscaler is never mentioned unless
/// the run asks for it.
fn run(
    store: &Arc<RetailDatStore>,
    station: &Station,
    run: &Run<'_>,
    frames: usize,
    keep: usize,
) -> Vec<Frame> {
    let mut gpu = device_checked(W, H, run.checked);
    let mut shot = Shot::open(store, &mut gpu, station);
    shot.poll(&mut gpu);
    shot.scene.draw.set_remaster_post(run.post, &mut gpu);
    if let Some(u) = run.upscale {
        shot.scene.draw.set_remaster_upscale(u, &mut gpu);
    }
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_light(RemasterLight::Pixel, &mut gpu);
    shot.step_held(&mut gpu);
    let start = Start {
        position: shot.scene.camera.position,
        yaw: shot.scene.camera.yaw,
        pitch: shot.scene.camera.pitch,
    };
    let mut out = Vec::new();
    for k in 0..frames {
        (run.before)(k, &mut shot, &mut gpu, start);
        shot.step_by(&mut gpu, 0.0);
        if k + keep >= frames {
            let capture = shot
                .scene
                .draw
                .remaster_capture(&mut gpu)
                .expect("the remaster drew");
            let pipelines = shot
                .scene
                .draw
                .remaster_counts(&mut gpu)
                .map_or(0, |c| c.pipelines_built);
            let upscaled = shot.scene.draw.remaster_upscaled_frames(&mut gpu);
            #[cfg(target_os = "macos")]
            let scaled = shot.scene.draw.remaster_metalfx_frames(&mut gpu);
            out.push(Frame {
                capture,
                pipelines,
                upscaled,
                #[cfg(target_os = "macos")]
                scaled,
            });
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

/// The pixels of a capture's colors that are not black.
fn lit(c: &RemasterCapture) -> usize {
    c.color
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] > 8 || p[1] > 8 || p[2] > 8)
        .count()
}

/// The pixels of the world, not the sky's, whose color differs by more than `levels` between two
/// captures: each display pixel judged by the class of the render pixel under it.
fn world_apart(a: &RemasterCapture, b: &RemasterCapture, levels: u8) -> usize {
    let (w, h) = (a.width as usize, a.height as usize);
    let (rw, rh) = (a.render_size.0 as usize, a.render_size.1 as usize);
    let mut n = 0;
    for y in 0..h {
        for x in 0..w {
            let class = a.class[(y * rh / h) * rw + x * rw / w];
            let at = (y * w + x) * 4;
            let moved = (0..3).any(|k| a.color[at + k].abs_diff(b.color[at + k]) > levels);
            n += usize::from(moved && class != 0 && class != 5);
        }
    }
    n
}

/// **FSR runs, and its frame is the display's size**: at half the scale the world is drawn at a
/// quarter of the pixels and the colors come out at the display's size, with no error from the
/// validation layer and no pipeline built after the start.
#[test]
fn fsr_runs_and_its_frame_is_the_display_size() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "holtburg");
    let never = run(&store, &st, &Run::default(), 2, 1)
        .pop()
        .expect("a frame");
    assert_eq!(never.upscaled, 0, "no frame upscaled unasked");
    let on = Run {
        upscale: Some(fsr(0.5)),
        ..Run::default()
    };
    let frames = run(&store, &st, &on, 4, 2);
    let f = frames.last().expect("a frame");
    assert_eq!((f.capture.width, f.capture.height), (W, H));
    assert_eq!(f.capture.render_size, (W / 2, H / 2));
    assert_eq!(f.capture.color.len(), (W * H * 4) as usize);
    assert_eq!(f.capture.class.len(), (W * H / 4) as usize);
    assert!(f.upscaled >= 4, "{} frames upscaled", f.upscaled);
    assert_eq!(frames[0].pipelines, f.pipelines, "a pipeline while drawing");
    assert_eq!(
        f.pipelines, never.pipelines,
        "a pipeline for the upscaler after the start"
    );
}

/// **The upscaler draws at the whole scale and at half**: each leaves the frame lit, as the frame
/// without it is, by the same pixels within a few parts in a hundred.
#[test]
fn fsr_draws_at_the_whole_scale_and_at_half() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "holtburg");
    let never = run(&store, &st, &Run::default(), 2, 1)
        .pop()
        .expect("a frame");
    let total = (W * H) as usize;
    assert!(lit(&never.capture) * 2 > total, "the frame without it");
    for scale in [1.0, 0.5] {
        let on = Run {
            upscale: Some(fsr(scale)),
            ..Run::default()
        };
        let f = run(&store, &st, &on, 6, 1).pop().expect("a frame");
        let (a, b) = (lit(&never.capture), lit(&f.capture));
        eprintln!("scale {scale}: {b} pixels lit, against {a} without it");
        assert!(a.abs_diff(b) * 20 < total, "scale {scale}: {b} against {a}");
    }
}

/// **The DLSS options draw too**: until DLSS draws them, each is drawn by FSR, so choosing one
/// still brings the frame up to the display's size.
#[test]
fn the_dlss_options_fall_through_to_fsr() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "holtburg");
    for mode in [RemasterUpscaleMode::DlssSr, RemasterUpscaleMode::DlssRr] {
        let on = Run {
            upscale: Some(RemasterUpscale { mode, ..fsr(0.5) }),
            ..Run::default()
        };
        let f = run(&store, &st, &on, 3, 1).pop().expect("a frame");
        assert_eq!((f.capture.width, f.capture.height), (W, H), "{mode:?}");
        assert_eq!(f.capture.render_size, (W / 2, H / 2), "{mode:?}");
        assert!(f.upscaled >= 3, "{mode:?}");
    }
}

/// **The upscaler stands in for the anti-aliasing, with the linear colors and the bloom**: asked
/// for together, the frame comes out at the display's size, and lit.
#[test]
fn fsr_stands_in_for_the_anti_aliasing_with_linear_light_and_the_bloom() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "holtburg");
    let on = Run {
        post: RemasterPost {
            hdr: true,
            taa: true,
            ..RemasterPost::default()
        },
        upscale: Some(fsr(0.5)),
        ..Run::default()
    };
    let f = run(&store, &st, &on, 6, 1).pop().expect("a frame");
    assert_eq!((f.capture.width, f.capture.height), (W, H));
    assert!(lit(&f.capture) * 2 > (W * H) as usize);
}

/// How sharp the world of a capture is: the mean difference between neighboring pixels of the
/// lower half of the picture, on the ground and what stands on it.
fn sharpness(c: &RemasterCapture) -> f64 {
    let (w, h) = (c.width as usize, c.height as usize);
    let mut sum = 0u64;
    let mut n = 0u64;
    for y in h / 2..h {
        for x in 0..w - 1 {
            let (a, b) = ((y * w + x) * 4, (y * w + x + 1) * 4);
            sum += u64::from(c.color[a + 1].abs_diff(c.color[b + 1]));
            n += 1;
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let mean = sum as f64 / n as f64;
    mean
}

/// **Drawn on fewer pixels, the ground keeps its detail**: its textures take finer levels than the
/// pixels drawn would choose, so with half the pixels each way, gathered over the frames, the
/// picture holds most of the detail of the one drawn whole: 0.80 of it, 0.61 at the levels the
/// pixels drawn choose, which is what Metal's samplers take, having no bias of their levels.
#[test]
fn at_half_the_pixels_the_ground_keeps_its_detail() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "holtburg");
    let whole = run(&store, &st, &Run::default(), 4, 1)
        .pop()
        .expect("a frame");
    let half = run(
        &store,
        &st,
        &Run {
            upscale: Some(fsr(0.5)),
            ..Run::default()
        },
        64,
        1,
    )
    .pop()
    .expect("a frame");
    let (w, h) = (sharpness(&whole.capture), sharpness(&half.capture));
    eprintln!("sharpness: {w:.3} whole, {h:.3} at half the pixels");
    let bar = if cfg!(target_os = "macos") { 6.0 } else { 7.0 };
    assert!(h * 10.0 > w * bar, "{h:.3} at half against {w:.3} whole");
}

/// How far apart two captures are, in levels a channel.
fn apart(a: &RemasterCapture, b: &RemasterCapture) -> f64 {
    let total: u64 = a
        .color
        .iter()
        .zip(&b.color)
        .map(|(p, q)| u64::from(p.abs_diff(*q)))
        .sum();
    #[allow(clippy::cast_precision_loss)]
    let mean = total as f64 / a.color.len() as f64;
    mean
}

/// The rainy street held still, with its rain falling or not, at the whole of its pixels or at
/// half of them with the upscaler told what the blended surfaces change or not: how far its last
/// frame is from the one before, in levels a channel.
fn rainy(store: &Arc<RetailDatStore>, upscale: Option<bool>) -> f64 {
    let mut st = stations::stations(store)
        .into_iter()
        .find(|s| s.name == "rain-street")
        .expect("the rainy street");
    st.weather = true;
    let mut gpu = device(W, H);
    let mut shot = Shot::open(store, &mut gpu, &st);
    shot.scene.weather = dereth_client_runtime::weather::Weather::Rain;
    shot.poll(&mut gpu);
    if let Some(reactive) = upscale {
        shot.scene.draw.set_remaster_upscale(
            RemasterUpscale {
                reactive,
                ..fsr(0.5)
            },
            &mut gpu,
        );
    }
    shot.scene
        .draw
        .set_remaster_weather(RemasterWeather { on: true }, &mut gpu);
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    for _ in 0..48 {
        shot.step_by(&mut gpu, 1.0 / 60.0);
    }
    let before = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew");
    shot.step_by(&mut gpu, 1.0 / 60.0);
    let last = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew");
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    apart(&last, &before)
}

/// **Rain stays rain**: the streaks fall through a view held still, with no motion for the upscaler
/// to follow, and left to gather they smear into a veil that hardly changes from frame to frame.
/// Told what they changed, it keeps them moving as the frame drawn whole does.
#[test]
fn the_upscaler_keeps_the_rain_falling_when_told_what_it_changed() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let whole = rainy(&store, None);
    let told = rainy(&store, Some(true));
    let untold = rainy(&store, Some(false));
    eprintln!(
        "rain moved, levels a channel: {whole:.3} whole, {told:.3} told, {untold:.3} not told"
    );
    assert!(whole > 0.01, "the rain falls: {whole:.3}");
    assert!(
        (told - whole).abs() < (untold - whole).abs(),
        "{told:.3} told, {untold:.3} not, against {whole:.3}"
    );
}

/// **A held view settles**: at half the scale, the view and the clock held, what the world drew
/// comes to rest: late on, few of its pixels are more than 16 levels from the frame before or from
/// the frame a whole sequence of the jitter before (thirty-two frames at this scale). A frame that
/// gathered nothing moves with its jitter from each frame to the next. The sky is left out: its
/// own meshes move from frame to frame.
#[test]
fn a_held_view_settles() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    for name in ["holtburg", "forest"] {
        let st = station(&store, name);
        let on = Run {
            upscale: Some(fsr(0.5)),
            ..Run::default()
        };
        let frames = run(&store, &st, &on, 100, 100);
        let last = &frames[99].capture;
        let (next, sequence) = (
            world_apart(&frames[98].capture, last, 16),
            world_apart(&frames[67].capture, last, 16),
        );
        eprintln!(
            "{name}: pixels more than 16 levels apart: {next} a frame on, {sequence} a sequence on"
        );
        assert!(next <= 128, "{name}: {next} pixels apart a frame on");
        assert!(
            sequence <= 128,
            "{name}: {sequence} pixels apart a sequence on"
        );
    }
}

/// **A moving view keeps drawing, and a new scale starts it again**: the camera turning and moving
/// at half the scale, then the scale changed, the frame stays lit at the display's size with no
/// error from the validation layer, and its render size follows.
#[test]
fn a_moving_view_and_a_new_scale_keep_the_frame_drawn() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "holtburg");
    let on = Run {
        upscale: Some(fsr(0.5)),
        before: &|k, shot, gpu, start| {
            #[allow(clippy::cast_precision_loss)] // a few steps
            let step = k as f32 + 1.0;
            let yaw = start.yaw + (0.5 * step).to_radians();
            let (s, p) = (0.25 * step, start.position);
            let at = Vec3::new(p.x + math::cosf(yaw) * s, p.y + math::sinf(yaw) * s, p.z);
            shot.hold_camera(at, yaw, start.pitch);
            if k == 12 {
                shot.scene.draw.set_remaster_upscale(fsr(0.75), gpu);
            }
        },
        ..Run::default()
    };
    let frames = run(&store, &st, &on, 16, 8);
    let (before, after) = (&frames[0], frames.last().expect("a frame"));
    assert_eq!(before.capture.render_size, (W / 2, H / 2));
    assert_eq!(after.capture.render_size, (W * 3 / 4, H * 3 / 4));
    for f in &frames {
        assert_eq!((f.capture.width, f.capture.height), (W, H));
        assert!(lit(&f.capture) * 2 > (W * H) as usize);
    }
}

/// MetalFX's scaler at `scale`.
#[cfg(target_os = "macos")]
fn metalfx(scale: f32) -> RemasterUpscale {
    RemasterUpscale {
        mode: RemasterUpscaleMode::MetalFx,
        ..fsr(scale)
    }
}

/// **Over Metal, MetalFX's scaler brings the frame up**: every frame goes through it, with the
/// linear colors and the bloom, and comes out lit at the display's size.
#[cfg(target_os = "macos")]
#[test]
fn metalfx_scales_each_frame_up_to_the_display_size() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "holtburg");
    for post in [
        RemasterPost::default(),
        RemasterPost {
            hdr: true,
            taa: true,
            ..RemasterPost::default()
        },
    ] {
        let on = Run {
            post,
            upscale: Some(metalfx(0.5)),
            ..Run::default()
        };
        let f = run(&store, &st, &on, 6, 1).pop().expect("a frame");
        assert_eq!((f.capture.width, f.capture.height), (W, H));
        assert_eq!(f.capture.render_size, (W / 2, H / 2));
        assert!(f.scaled >= 6, "{} frames scaled", f.scaled);
        assert!(lit(&f.capture) * 2 > (W * H) as usize);
    }
}

/// **MetalFX keeps the ground's detail and rests on a held view**: with half the pixels each way,
/// gathered over the frames, the picture holds more of the detail of the one drawn whole than it
/// does told the wrong way the projection moved (0.68 of it, 0.63 at most; 0.56 told it held
/// still), Metal's samplers taking no bias of their levels; and late on it hardly moves from frame
/// to frame.
#[cfg(target_os = "macos")]
#[test]
fn metalfx_keeps_the_grounds_detail_and_rests_on_a_held_view() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "holtburg");
    let whole = run(&store, &st, &Run::default(), 4, 1)
        .pop()
        .expect("a frame");
    let on = Run {
        upscale: Some(metalfx(0.5)),
        ..Run::default()
    };
    let frames = run(&store, &st, &on, 64, 2);
    let (w, h) = (sharpness(&whole.capture), sharpness(&frames[1].capture));
    let next = world_apart(&frames[0].capture, &frames[1].capture, 16);
    eprintln!("sharpness: {w:.3} whole, {h:.3} at half the pixels; {next} pixels apart a frame on");
    assert!(h * 100.0 > w * 65.0, "{h:.3} at half against {w:.3} whole");
    assert!(next <= 128, "{next} pixels apart a frame on");
}

/// **A new scale makes MetalFX's scaler again**: its descriptor names its sizes, so the camera
/// moving and the scale changed leave the frame lit at the display's size, each frame scaled.
#[cfg(target_os = "macos")]
#[test]
fn a_new_scale_makes_metalfxs_scaler_again() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "holtburg");
    let on = Run {
        upscale: Some(metalfx(0.5)),
        before: &|k, shot, gpu, start| {
            #[allow(clippy::cast_precision_loss)] // a few steps
            let yaw = start.yaw + (0.5 * (k as f32 + 1.0)).to_radians();
            shot.hold_camera(start.position, yaw, start.pitch);
            if k == 12 {
                shot.scene.draw.set_remaster_upscale(metalfx(0.75), gpu);
            }
        },
        ..Run::default()
    };
    let frames = run(&store, &st, &on, 16, 8);
    let (before, after) = (&frames[0], frames.last().expect("a frame"));
    assert_eq!(before.capture.render_size, (W / 2, H / 2));
    assert_eq!(after.capture.render_size, (W * 3 / 4, H * 3 / 4));
    assert!(after.scaled >= 16, "{} frames scaled", after.scaled);
    for f in &frames {
        assert_eq!((f.capture.width, f.capture.height), (W, H));
        assert!(lit(&f.capture) * 2 > (W * H) as usize);
    }
}

/// **Off, the frame is the one drawn without it**: the upscaler asked for off at another scale, and
/// the upscaler on and then off again, each leave the frame byte for byte the frame of a run that
/// never mentioned it.
#[test]
fn off_the_frame_is_the_one_drawn_without_the_upscaler() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "holtburg");
    let never = run(&store, &st, &Run::default(), 8, 1)
        .pop()
        .expect("a frame");
    let asked_off = Run {
        upscale: Some(RemasterUpscale {
            mode: RemasterUpscaleMode::Off,
            scale: 0.5,
            sharpness: 1.0,
            reactive: true,
        }),
        ..Run::default()
    };
    let off = run(&store, &st, &asked_off, 8, 1).pop().expect("a frame");
    assert_eq!(off.upscaled, 0);
    assert_eq!(off.capture.color, never.capture.color, "asked for off");
    let toggled = Run {
        before: &|k, shot, gpu, _| {
            if k == 0 {
                shot.scene.draw.set_remaster_upscale(fsr(0.5), gpu);
            }
            if k == 4 {
                shot.scene
                    .draw
                    .set_remaster_upscale(RemasterUpscale::default(), gpu);
            }
        },
        ..Run::default()
    };
    let frames = run(&store, &st, &toggled, 8, 8);
    assert_eq!(frames[3].capture.render_size, (W / 2, H / 2), "on");
    let back = frames.last().expect("a frame");
    assert_eq!(back.capture.render_size, (W, H), "off again");
    assert_eq!(back.capture.color, never.capture.color, "on and off again");
}

/// Not a claim: draws a moving view over water and steep ground with DLSS, with FSR and without an
/// upscaler, a frame a step, into `DERETH_REMASTER_CAPTURE_DIR` (`<run>-<step>.png`), and prints how
/// much each upscaled run's difference from the frame drawn without one changes from one frame to
/// the next, over the water and over the rest of the world: what a person sees as flicker. DLSS
/// draws only where its library stands beside the test binary; elsewhere FSR stands in for it.
#[test]
#[ignore = "an instrument: set DERETH_REMASTER_CAPTURE_DIR and run with --ignored"]
fn draw_a_moving_view_over_water_with_each_upscaler() {
    let Some(dir) = std::env::var_os("DERETH_REMASTER_CAPTURE_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("the capture folder");
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let named = std::env::var("DERETH_REMASTER_STATION").ok();
    let lake = Station {
        name: "lake",
        why: "the lake north of Holtburg and the steep hills across it",
        place: stations::Place::Free {
            block: stations::HOLTBURG,
            x: 96.0,
            y: 150.0,
            eye: 6.0,
            yaw: 0.0,
            pitch: -0.12,
        },
        clock: stations::Clock::Sunny(0.4),
        weather: false,
        land_radius: 3,
    };
    // Another station of the light study's, by its name, in place of the lake.
    let st = named.as_deref().map_or(lake, |n| station(&store, n));
    let steps = std::env::var("DERETH_REMASTER_STEPS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(24usize);
    let moving: Before<'_> = &|k, shot, gpu, start| {
        if k == 0 {
            // The water, the sun's shadows and the sky as the client draws them.
            shot.scene.draw.set_remaster_water(
                dereth_scene::world_scene::RemasterWater {
                    on: true,
                    ..dereth_scene::world_scene::RemasterWater::default()
                },
                gpu,
            );
            if std::env::var_os("DERETH_REMASTER_ALL").is_some() {
                shot.scene
                    .draw
                    .set_remaster_gi(dereth_scene::world_scene::RemasterGi::on(), gpu);
                shot.scene.draw.set_remaster_ao(
                    dereth_scene::world_scene::RemasterAo {
                        on: true,
                        ..dereth_scene::world_scene::RemasterAo::default()
                    },
                    gpu,
                );
            }
            shot.scene.draw.set_remaster_shadows(
                dereth_scene::world_scene::RemasterShadows {
                    on: true,
                    ..dereth_scene::world_scene::RemasterShadows::default()
                },
                gpu,
            );
        }
        #[allow(clippy::cast_precision_loss)] // a few steps
        let step = k as f32;
        let yaw = start.yaw + (0.4 * step).to_radians();
        let p = start.position;
        let at = Vec3::new(p.x + 0.15 * step, p.y + 0.1 * step, p.z);
        shot.hold_camera(at, yaw, start.pitch);
    };
    let post = RemasterPost {
        hdr: true,
        taa: std::env::var_os("DERETH_REMASTER_TAA").is_some(),
        ..RemasterPost::default()
    };
    let upscale = |mode| RemasterUpscale {
        mode,
        scale: 0.67,
        sharpness: 0.0,
        ..RemasterUpscale::default()
    };
    let runs = [
        ("native", None),
        ("dlss", Some(upscale(RemasterUpscaleMode::DlssSr))),
        ("fsr", Some(upscale(RemasterUpscaleMode::Fsr))),
    ];
    let mut drawn = Vec::new();
    for (name, up) in runs {
        let r = Run {
            post,
            upscale: up,
            before: moving,
            checked: false,
        };
        let frames = run(&store, &st, &r, steps, steps);
        for (k, f) in frames.iter().enumerate() {
            let c = &f.capture;
            stations::write_png(
                &dir.join(format!("{name}-{k:02}.png")),
                c.width,
                c.height,
                &c.color,
            );
        }
        eprintln!(
            "{name}: the upscaler drawn {:?}",
            dereth_scene::world_scene::remaster_upscaler_drawn()
        );
        drawn.push((name, frames));
    }
    let (_, native) = &drawn[0];
    for (name, frames) in &drawn[1..] {
        let (mut water, mut land) = ((0.0f64, 0usize), (0.0f64, 0usize));
        for k in 1..frames.len() {
            let (a, b) = (&frames[k - 1].capture, &frames[k].capture);
            let (na, nb) = (&native[k - 1].capture, &native[k].capture);
            let (w, h) = (b.width as usize, b.height as usize);
            let (rw, rh) = (nb.render_size.0 as usize, nb.render_size.1 as usize);
            for y in 0..h {
                for x in 0..w {
                    let r = (y * rh / h) * rw + x * rw / w;
                    let class = nb.class[r];
                    if class == 0 || class == 5 {
                        continue;
                    }
                    let at = (y * w + x) * 4;
                    let e = |c: &RemasterCapture, n: &RemasterCapture, i: usize| {
                        f64::from(c.color[at + i]) - f64::from(n.color[at + i])
                    };
                    let change: f64 = (0..3)
                        .map(|i| (e(b, nb, i) - e(a, na, i)).abs())
                        .sum::<f64>()
                        / 3.0;
                    let slot = if nb.kind[r] == 7 {
                        &mut water
                    } else {
                        &mut land
                    };
                    slot.0 += change;
                    slot.1 += 1;
                }
            }
        }
        #[allow(clippy::cast_precision_loss)] // pixel counts
        let mean = |(s, n): (f64, usize)| s / n.max(1) as f64;
        eprintln!(
            "{name}: the frame's difference from the native one changes by {:.2} levels a frame over the water, {:.2} over the rest",
            mean(water),
            mean(land)
        );
    }
}
