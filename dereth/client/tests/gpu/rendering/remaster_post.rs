//! The remaster's post passes: the scene in linear light, the temporal anti-aliasing, the bloom
//! and the tone map. Off, they leave the frame drawn without them; on, a still view settles to a still
//! picture, the motion finds last frame's surfaces on a moving camera and on moving parts, the
//! frames laid over each other come nearer a finer picture, and linear light goes past where
//! retail clamps while the tone map brings it back.
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
    RemasterAo, RemasterCapture, RemasterLight, RemasterPost, RemasterPostPicture,
};

use crate::instruments::remaster_frames;
use crate::instruments::stations::{self, Shot, Station};

const W: u32 = 640;
const H: u32 = 480;

fn device(width: u32, height: u32) -> Gpu {
    let cfg = DeviceConfig {
        width,
        height,
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

/// One frame of the remaster and of its post passes.
struct Frame {
    capture: RemasterCapture,
    post: Option<RemasterPostPicture>,
    pipelines: u64,
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

/// How a run is drawn: its post passes, its occlusion, its light, its size, what happens before
/// each step, and the seconds each step moves the clock on.
struct Run<'a> {
    post: RemasterPost,
    ao: RemasterAo,
    light: RemasterLight,
    size: (u32, u32),
    before: Before<'a>,
    seconds: f64,
}

impl Default for Run<'_> {
    fn default() -> Self {
        Self {
            post: RemasterPost::default(),
            ao: RemasterAo::default(),
            light: RemasterLight::Pixel,
            size: (W, H),
            before: &|_, _, _, _| {},
            seconds: 0.0,
        }
    }
}

/// The remaster at `station` drawn as `run` says for `frames` steps; the last `keep` frames.
fn run(
    store: &Arc<RetailDatStore>,
    station: &Station,
    run: &Run<'_>,
    frames: usize,
    keep: usize,
) -> Vec<Frame> {
    let mut gpu = device(run.size.0, run.size.1);
    let mut shot = Shot::open(store, &mut gpu, station);
    shot.poll(&mut gpu);
    shot.scene.draw.set_remaster_ao(run.ao, &mut gpu);
    shot.scene.draw.set_remaster_post(run.post, &mut gpu);
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    shot.scene.draw.set_remaster_light(run.light, &mut gpu);
    shot.step_held(&mut gpu);
    let start = Start {
        position: shot.scene.camera.position,
        yaw: shot.scene.camera.yaw,
        pitch: shot.scene.camera.pitch,
    };
    let mut out = Vec::new();
    for k in 0..frames {
        (run.before)(k, &mut shot, &mut gpu, start);
        shot.step_by(&mut gpu, run.seconds);
        if k + keep >= frames {
            let capture = shot
                .scene
                .draw
                .remaster_capture(&mut gpu)
                .expect("the remaster drew");
            let post = shot.scene.draw.remaster_post_picture(&mut gpu);
            let pipelines = shot
                .scene
                .draw
                .remaster_counts(&mut gpu)
                .map_or(0, |c| c.pipelines_built);
            out.push(Frame {
                capture,
                post,
                pipelines,
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

/// Linear light, anti-aliased, without the bloom.
fn hdr() -> RemasterPost {
    RemasterPost {
        hdr: true,
        taa: true,
        bloom: 0.0,
        ..RemasterPost::default()
    }
}

/// The pixels the world's light is resolved for: those the G-buffer holds. The sky's own meshes
/// move from frame to frame, so the sky is left out of the counts.
fn resolved(c: &RemasterCapture) -> impl Iterator<Item = usize> + '_ {
    (0..c.class.len()).filter(|i| c.class[*i] != 0)
}

/// The resolved pixels whose color differs between `a` and `b`.
fn changed(a: &RemasterCapture, b: &RemasterCapture) -> usize {
    resolved(a)
        .filter(|i| a.color[i * 4..i * 4 + 3] != b.color[i * 4..i * 4 + 3])
        .count()
}

/// The largest channel difference of two colors, in display levels.
fn levels(a: [f32; 4], b: [f32; 4]) -> f32 {
    (0..3)
        .map(|k| (a[k] - b[k]).abs() * 255.0)
        .fold(0.0, f32::max)
}

/// The mean of the largest channel difference over the resolved pixels of two captures, in
/// display levels.
fn mean_levels(a: &RemasterCapture, b: &RemasterCapture) -> f64 {
    let mut sum = 0.0f64;
    let mut n = 0usize;
    for i in resolved(a) {
        let d = (0..3)
            .map(|k| a.color[i * 4 + k].abs_diff(b.color[i * 4 + k]))
            .max()
            .unwrap_or(0);
        sum += f64::from(d);
        n += 1;
    }
    #[allow(clippy::cast_precision_loss)] // pixel counts
    let n = n.max(1) as f64;
    sum / n
}

/// The camera turned `degrees` a step and moved `metres` a step to its right.
fn moving(k: usize, shot: &mut Shot, start: Start, degrees: f32, metres: f32) {
    #[allow(clippy::cast_precision_loss)] // a few steps
    let k = k as f32 + 1.0;
    let yaw = start.yaw + (degrees * k).to_radians();
    let (s, p) = (metres * k, start.position);
    let at = Vec3::new(p.x + math::cosf(yaw) * s, p.y + math::sinf(yaw) * s, p.z);
    shot.hold_camera(at, yaw, start.pitch);
}

/// **The passes off leave the frame drawn without them, and on build no pipeline**: at Holtburg, the frame
/// with the passes on and then off again is the frame of a run of as many frames that never
/// turned them on, and the pipelines counted with them on are those counted without.
#[test]
fn the_passes_off_leave_the_frame_drawn_without_them_and_on_build_no_pipeline() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "holtburg");
    let never = run(&store, &st, &Run::default(), 8, 1)
        .pop()
        .expect("a frame");
    let toggled = Run {
        before: &|k, shot, gpu, _| {
            if k == 0 {
                shot.scene.draw.set_remaster_post(hdr(), gpu);
            }
            if k == 4 {
                shot.scene
                    .draw
                    .set_remaster_post(RemasterPost::default(), gpu);
            }
        },
        ..Run::default()
    };
    let frames = run(&store, &st, &toggled, 8, 8);
    let on = &frames[3];
    let off = frames.last().expect("a frame");
    assert!(on.post.is_some(), "the passes ran");
    assert!(off.post.is_none(), "the passes stopped");
    assert_eq!(on.pipelines, never.pipelines, "no pipeline built for them");
    assert_eq!(
        changed(&off.capture, &never.capture),
        0,
        "the frame drawn without them"
    );
    assert_eq!(off.capture.color, never.capture.color, "the sky too");
}

/// **A still view settles to a still picture**: at Holtburg with the occlusion on, linear light
/// and the anti-aliasing, the view and the clock held, every resolved pixel whose own frames
/// draw it the same as a sequence (eight frames) before is the same from one frame to the next.
/// The pixels whose frames do not repeat are those the moving sky shows through (the cut-out
/// edges blended over it): a few, against the resolved pixels. With the bloom, which spreads the
/// moving sky's light over its neighbors, under one in two hundred change.
#[test]
fn a_still_view_settles_to_a_still_picture() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "holtburg");
    let ao = RemasterAo {
        on: true,
        ..RemasterAo::default()
    };
    for bloom in [0.0, 0.04] {
        let still = Run {
            post: RemasterPost { bloom, ..hdr() },
            ao,
            ..Run::default()
        };
        let frames = run(&store, &st, &still, 40, 10);
        let scene = |k: usize, i: usize| frames[k].post.as_ref().expect("the passes ran").scene[i];
        let (a, b) = (&frames[8].capture, &frames[9].capture);
        let (mut repeated, mut moved, mut changing) = (0usize, 0usize, 0usize);
        for i in resolved(a) {
            let same = a.color[i * 4..i * 4 + 3] == b.color[i * 4..i * 4 + 3];
            if scene(8, i) == scene(0, i) && scene(9, i) == scene(1, i) {
                repeated += 1;
                moved += usize::from(!same);
            } else {
                changing += 1;
            }
        }
        eprintln!(
            "holtburg, still, bloom {bloom}: {moved} of {repeated} repeating resolved pixels \
             changed; {changing} do not repeat"
        );
        if bloom == 0.0 {
            assert_eq!(moved, 0, "the repeating pixels are still");
        } else {
            assert!(moved * 200 < repeated, "{moved} repeating pixels changed");
        }
        assert!(changing * 200 < repeated, "{changing} pixels do not repeat");
    }
}

/// **The motion finds last frame's surfaces on a moving camera**: at Holtburg and in the forest,
/// the camera turning half a degree and moving a quarter metre a step, last frame's picture found
/// where each pixel's surface was (the projection unmoved while checking) is nearer this frame's
/// than the same pixel of last frame is: by its mean, and by the pixels more than 8 levels off.
#[test]
fn the_motion_finds_last_frames_surfaces_on_a_moving_camera() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    for name in ["holtburg", "forest"] {
        let st = station(&store, name);
        let walk = Run {
            post: RemasterPost {
                taa: true,
                check: true,
                ..RemasterPost::default()
            },
            before: &|k, shot, _, start| moving(k, shot, start, 0.5, 0.25),
            ..Run::default()
        };
        let frames = run(&store, &st, &walk, 6, 2);
        let (last, now) = (&frames[0], &frames[1]);
        let (lp, np) = (
            last.post.as_ref().expect("the passes ran"),
            now.post.as_ref().expect("the passes ran"),
        );
        let (mut found, mut wrong, mut unmoved) = (0usize, 0usize, 0usize);
        let (mut off, mut same) = (0.0f64, 0.0f64);
        for i in resolved(&now.capture) {
            if np.shown[i][3] < 0.5 {
                continue;
            }
            found += 1;
            let (r, u) = (
                levels(np.shown[i], np.scene[i]),
                levels(lp.scene[i], np.scene[i]),
            );
            wrong += usize::from(r > 8.0);
            unmoved += usize::from(u > 8.0);
            off += f64::from(r);
            same += f64::from(u);
        }
        let all = resolved(&now.capture).count();
        #[allow(clippy::cast_precision_loss)] // pixel counts
        let (off, same) = (off / found.max(1) as f64, same / found.max(1) as f64);
        eprintln!(
            "{name}, moving: {found} of {all} resolved pixels found last frame; {off:.2} levels \
             off on average, {wrong} more than 8; the same pixel {same:.2} and {unmoved}"
        );
        assert!(found * 10 > all * 9, "most pixels found: {found} of {all}");
        assert!(off * 3.0 < same, "{off:.2} levels off against {same:.2}");
        assert!(wrong * 3 < unmoved, "{wrong} wrong against {unmoved}");
    }
}

/// **Moving parts carry their own motion**: a person standing four metres ahead of the held
/// camera at Holtburg, the clock running, has pixels in the motion target, and their last places
/// are not where the view alone puts them (by a hundredth of a pixel or more).
#[test]
fn moving_parts_carry_their_own_motion() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "holtburg");
    let person = Run {
        post: RemasterPost {
            taa: true,
            ..RemasterPost::default()
        },
        before: &|k, shot, _, start| {
            if k == 0 {
                let (x, y) = (
                    (start.position.x - math::sinf(start.yaw) * 4.0).clamp(0.5, 191.5),
                    (start.position.y + math::cosf(start.yaw) * 4.0).clamp(0.5, 191.5),
                );
                shot.create_object(
                    0x8300_2100,
                    (0x0200_0001, Some(0x0900_0001)),
                    stations::HOLTBURG,
                    (x, y),
                    start.yaw.to_degrees() + 180.0,
                );
            }
        },
        seconds: 1.0 / 30.0,
        ..Run::default()
    };
    let frames = run(&store, &st, &person, 30, 1);
    let f = frames.last().expect("a frame");
    let p = f.post.as_ref().expect("the passes ran");
    let (w, h) = (p.width, p.height);
    let mut drawn = 0usize;
    let mut own = 0usize;
    for (i, m) in p.motion.iter().enumerate() {
        if m[3] < 0.5 {
            continue;
        }
        drawn += 1;
        // Where the view alone puts the pixel's surface last frame: its center, the frame's
        // projection's move taken off.
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)] // pixel positions
        let (x, y) = ((i % w as usize) as f32 + 0.5, (i / w as usize) as f32 + 0.5);
        #[allow(clippy::cast_precision_loss)]
        let ndc = [
            x / w as f32 * 2.0 - 1.0 - p.jitter[0],
            1.0 - y / h as f32 * 2.0 - p.jitter[1],
        ];
        let px = [
            (m[0] - ndc[0]).abs() * w as f32 * 0.5,
            (m[1] - ndc[1]).abs() * h as f32 * 0.5,
        ];
        own += usize::from(px[0].max(px[1]) > 0.01);
    }
    eprintln!("holtburg, a person: {drawn} pixels in the motion target, {own} moved by their own");
    assert!(drawn > 500, "{drawn} pixels in the motion target");
    assert!(own > 0, "moved by their own motion");
}

/// **The frames laid over each other come nearer a finer picture at the edges**: in the forest,
/// where the frame drawn at twice the size has an edge inside a pixel (its four pixels of
/// different classes, or depths apart by more than a hundredth), the anti-aliased frame of a
/// still view is nearer that frame brought down (each pixel the mean of four) than the frame
/// drawn without it is, by a fifth or more.
#[test]
fn the_frames_laid_over_each_other_come_nearer_a_finer_picture_at_the_edges() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "forest");
    let plain = run(&store, &st, &Run::default(), 3, 1)
        .pop()
        .expect("a frame");
    let taa = Run {
        post: RemasterPost {
            taa: true,
            ..RemasterPost::default()
        },
        ..Run::default()
    };
    let smooth = run(&store, &st, &taa, 24, 1).pop().expect("a frame");
    let big = Run {
        size: (W * 2, H * 2),
        ..Run::default()
    };
    let fine = run(&store, &st, &big, 3, 1).pop().expect("a frame");
    let (bw, w) = (fine.capture.width as usize, plain.capture.width as usize);
    let mut down = plain.capture.clone();
    let mut edges = vec![false; plain.capture.class.len()];
    for (i, edge) in edges.iter_mut().enumerate() {
        let (x, y) = (i % w, i / w);
        let four: Vec<usize> = [(0, 0), (1, 0), (0, 1), (1, 1)]
            .iter()
            .map(|(dx, dy)| (y * 2 + dy) * bw + x * 2 + dx)
            .collect();
        let mut classes = four.iter().map(|f| fine.capture.class[*f]);
        let depths: Vec<f32> = four.iter().map(|f| fine.capture.depth[*f]).collect();
        let (near, far) = depths
            .iter()
            .fold((f32::MAX, f32::MIN), |(n, f), d| (n.min(*d), f.max(*d)));
        let first = fine.capture.class[four[0]];
        *edge = classes.any(|c| c != first) || (1.0 - near) > (1.0 - far) * 1.01;
        for k in 0..3 {
            let sum: u32 = [(0, 0), (1, 0), (0, 1), (1, 1)]
                .iter()
                .map(|(dx, dy)| {
                    u32::from(fine.capture.color[((y * 2 + dy) * bw + x * 2 + dx) * 4 + k])
                })
                .sum();
            down.color[i * 4 + k] = u8::try_from((sum + 2) / 4).unwrap_or(255);
        }
    }
    let at_edges = |c: &RemasterCapture| -> (f64, usize) {
        let mut sum = 0.0f64;
        let mut n = 0usize;
        for (i, e) in edges.iter().enumerate() {
            if !e {
                continue;
            }
            let d = (0..3)
                .map(|k| c.color[i * 4 + k].abs_diff(down.color[i * 4 + k]))
                .max()
                .unwrap_or(0);
            sum += f64::from(d);
            n += 1;
        }
        #[allow(clippy::cast_precision_loss)] // pixel counts
        let mean = sum / n.max(1) as f64;
        (mean, n)
    };
    let ((p, n), (s, _)) = (at_edges(&plain.capture), at_edges(&smooth.capture));
    eprintln!(
        "forest, against twice the size at {n} edge pixels: {p:.2} levels without, {s:.2} with; \
         everywhere {:.3} and {:.3}",
        mean_levels(&plain.capture, &down),
        mean_levels(&smooth.capture, &down)
    );
    assert!(n > 5_000, "{n} edge pixels");
    assert!(s * 5.0 < p * 4.0, "{s:.2} levels with, {p:.2} without");
}

/// **Linear light goes past where retail clamps, and the tone map brings it back**: at Holtburg
/// with linear light and the default exposure, the frame shown stays near the frame drawn in display values (a mean
/// under 8 levels) and a stop more brightens it; with a look that doubles the sun, some of the scene's colors are
/// over 1, which retail's arithmetic clamps, and the tone map brings every one of them under 255.
#[test]
fn linear_light_goes_past_where_retail_clamps_and_the_tone_map_brings_it_back() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "holtburg");
    let linear = |stops: f32| RemasterPost {
        hdr: true,
        exposure: RemasterPost::default().exposure + stops,
        ..RemasterPost::default()
    };
    let plain = |post: RemasterPost| Run {
        post,
        ..Run::default()
    };
    let frame = |post: RemasterPost| run(&store, &st, &plain(post), 3, 1).pop().expect("a frame");
    let display = frame(RemasterPost::default());
    let lit = frame(linear(0.0));
    let brighter = frame(linear(1.0));
    let luma = |c: &RemasterCapture| -> f64 {
        let (sum, n) = resolved(c).fold((0.0f64, 0usize), |(s, n), i| {
            (
                s + f64::from(c.color[i * 4]) * 0.2126
                    + f64::from(c.color[i * 4 + 1]) * 0.7152
                    + f64::from(c.color[i * 4 + 2]) * 0.0722,
                n + 1,
            )
        });
        #[allow(clippy::cast_precision_loss)] // pixel counts
        let n = n.max(1) as f64;
        sum / n
    };
    let sunny = Run {
        post: linear(0.0),
        before: &|k, shot, gpu, _| {
            if k == 0 {
                shot.scene
                    .draw
                    .set_remaster_look(
                        dereth_scene::world_scene::RemasterLookLayer::Player,
                        Some("sun = 2.0"),
                        gpu,
                    )
                    .expect("a look");
            }
        },
        ..Run::default()
    };
    let mapped = run(&store, &st, &sunny, 3, 1).pop().expect("a frame");
    let p = mapped.post.as_ref().expect("the passes ran");
    let over: Vec<usize> = resolved(&mapped.capture)
        .filter(|i| p.scene[*i][..3].iter().any(|v| *v > 1.0))
        .collect();
    let at_255 = over
        .iter()
        .filter(|i| mapped.capture.color[*i * 4..*i * 4 + 3].contains(&255))
        .count();
    let mean = mean_levels(&lit.capture, &display.capture);
    eprintln!(
        "holtburg, linear: mean brightness {:.1} ({mean:.2} levels from display values), {:.1} a stop \
         up, display values {:.1}; the sun doubled: {} pixels over 1, {at_255} of them at 255",
        luma(&lit.capture),
        luma(&brighter.capture),
        luma(&display.capture),
        over.len()
    );
    assert!(mean < 8.0, "{mean:.2} levels from display values");
    assert!(luma(&brighter.capture) > luma(&lit.capture));
    assert!(over.len() > 100, "{} pixels over 1", over.len());
    assert_eq!(at_255, 0, "none at 255");
}
