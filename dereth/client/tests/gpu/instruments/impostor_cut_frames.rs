//! A tree drawn as its impostor beside the same tree drawn as its mesh: the largest tree the
//! impostors draw near the report's place, seen from a few distances along the clearest line to
//! it, each way first held still and then walking slowly sideways. For each distance and way it
//! prints how much of the crown the tree covers, how much the impostor leaves open that the mesh
//! covers and the reverse, how many lone pixels the crown's coverage holds (a dot pattern's mark),
//! and how much the shown picture changes from one frame to the next; and it writes the crown as
//! the display shows it.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware device.
//!
//! - `DERETH_ART_PACK`: the art packs drawn, joined by `;`, bottom first (none without it).
//! - `DERETH_REMASTER_CAPTURE_DIR`: where the pictures go: `crown-<tag>-<distance>-<way>.png`, the
//!   crown as the display shows it once settled, and `crown-<tag>-<distance>-<way>-still.png` and
//!   `-moving.png`, each run's frames side by side (none are written without it).
//! - `DERETH_REMASTER_LOOK_TAG`: a word in the pictures' names, to keep runs apart.
//! - `DERETH_REMASTER_SIZE`: `WIDTHxHEIGHT` (default 1280x540).
//! - `DERETH_REMASTER_LOOK_BOXES`: the boxes, as `--fidelity` takes them (default every box);
//!   the art pack's box is set over them where a pack is drawn.
//! - `DERETH_IMPOSTOR_PLACE`: a report line saying where the body stands (default a rainy hill
//!   below a band of pines).
//! - `DERETH_IMPOSTOR_DISTANCES`: meters from the tree, a comma list (default `60,120,180`).
//! - `DERETH_IMPOSTOR_BETWEEN`: where the eye stands between two of the impostor's views, 0 on
//!   one and 0.5 halfway (default wherever the clearest line puts it).
//! - `DERETH_IMPOSTOR_WAYS`: a comma list of `mesh` (no tree drawn as its impostor),
//!   `impostor` (every tree its impostor wherever its error allows: past an art pack's own levels
//!   where it has them) and `levels` (the levels as the player has them) (default `mesh,impostor`).
//! - `DERETH_IMPOSTOR_FRAMES`: frames in each run (default 16).
//! - `DERETH_IMPOSTOR_EYE`: `level` to stand the eye level with the middle of the tree's pictures,
//!   as they were drawn, rather than on the ground looking up at it.

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_client_runtime::report::VisualReport;
use dereth_primitives::num::math;
use dereth_primitives::Vec3;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::art_pack::ArtPack;
use dereth_scene::world_scene::{RemasterCapture, RemasterLod};

use super::far_shadow_frames::distances;
use super::remaster_look_frames::EVERY_BOX;
use super::stations::{self, report_station, Shot};

/// A rainy hill below a band of pines.
const HILL: &str = "dereth report 1; loc 0xB3B50006 [0.240147 121.806526 98.005005] -0.936959 0 \
                    0 -0.349440; camera 0xB3B5 [4.164169 117.277565 99.20517] yaw -5.569238 \
                    pitch 0.05000011 cell 0xB3B50005; time 0.523; weather rain";

/// The frames drawn with the clock held after the camera moves or the way changes, before a run:
/// the upscaler's or the smoothing's history settles.
const SETTLE: usize = 48;

/// The eye's height over the ground where it stands, meters.
const EYE: f32 = 1.8;

/// How fast the eye walks sideways in a moving run, meters a second, at sixty frames a second.
const WALK: f32 = 1.5;

/// The views round a tree its impostor holds.
const VIEWS: f32 = 16.0;

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// A tree the impostors drew: its first level's GfxObj, the centre of its pictures' sphere
/// (render space) and its radius, and the turn of its x axis.
#[derive(Debug, Clone, Copy)]
struct Tree {
    gfxobj: u32,
    centre: [f32; 3],
    radius: f32,
    turn: f32,
}

/// The clearest bearing from `target` out to `reach` meters (radians, from east toward north):
/// the one whose line to the tree the fewest other `trees` stand across, nearest the eye's
/// height; with `between`, turned onto that share of the way between two of its views.
fn clearest(target: &Tree, trees: &[Tree], reach: f32, between: Option<f32>) -> f32 {
    let tau = std::f32::consts::TAU;
    let mut best = (usize::MAX, 0.0f32);
    for k in 0..64 {
        #[allow(clippy::cast_precision_loss)] // a few dozen bearings
        let bearing = tau * k as f32 / 64.0;
        let (dx, dy) = (math::cosf(bearing), math::sinf(bearing));
        let crossing = trees
            .iter()
            .filter(|t| t.centre != target.centre)
            .filter(|t| {
                let (rx, ry) = (
                    t.centre[0] - target.centre[0],
                    t.centre[1] - target.centre[1],
                );
                let along = rx * dx + ry * dy;
                let off = (rx * dy - ry * dx).abs();
                along > -t.radius && along < reach + t.radius && off < t.radius + target.radius
            })
            .count();
        if crossing < best.0 {
            best = (crossing, bearing);
        }
    }
    let Some(between) = between else {
        return best.1;
    };
    // The eye's angle round the tree in views is the bearing less the tree's turn.
    let step = tau / VIEWS;
    let views = ((best.1 - target.turn) / step).floor();
    target.turn + (views + between) * step
}

/// What a run measured over the crown.
#[derive(Debug, Default)]
struct Measured {
    /// The crown's pixels on the target the world is drawn in, and each one's share of the
    /// frames the tree covered it in.
    covered: Vec<f32>,
    /// Lone pixels in the coverage, a frame's mean: covered with at most one covered neighbor
    /// of four, or open with at least three.
    lone: f64,
    /// The share of the crown's shown pixels that changed by more than 16 levels from one frame to
    /// the next, the frames' mean.
    changed: f64,
    /// The share of the crown's shown pixels that stand out from their four neighbors by more than
    /// 24 levels of brightness in the settled frame: speckle.
    speckle: f64,
    /// The crown as the last frame showed it, and its size.
    picture: Vec<u8>,
    size: (u32, u32),
    /// Each frame's crown, side by side.
    strip: Vec<Vec<u8>>,
}

/// A box on a target: left, top, right, bottom, pixels.
type Area = [u32; 4];

/// The box round the tree's crown (the top half of its sphere's card, a little wider) on a target
/// `size`, seen through `c`.
fn crown_box(c: &RemasterCapture, tree: &Tree, size: (u32, u32)) -> Area {
    let m = glam::Mat4::from_cols_array(&c.view_projection);
    let centre = glam::Vec3::from_array(tree.centre);
    let eye = glam::Vec3::from_array(c.eye);
    let to = (centre - eye) * glam::Vec3::new(1.0, 1.0, 0.0);
    let right = glam::Vec3::new(to.y, -to.x, 0.0).normalize_or_zero();
    let r = tree.radius;
    let mut lo = glam::Vec2::splat(f32::MAX);
    let mut hi = glam::Vec2::splat(f32::MIN);
    for (sx, sz) in [(-1.1, 1.05), (1.1, 1.05), (-1.1, 0.0), (1.1, 0.0)] {
        let p = centre + right * (sx * r) + glam::Vec3::Z * (sz * r);
        let q = m * p.extend(1.0);
        let ndc = q.truncate() / q.w;
        #[allow(clippy::cast_precision_loss)] // a target's size
        let px = glam::Vec2::new(
            (ndc.x + 1.0) * 0.5 * size.0 as f32,
            (1.0 - ndc.y) * 0.5 * size.1 as f32,
        );
        lo = lo.min(px);
        hi = hi.max(px);
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped to the target
    let clamp = |v: f32, n: u32| v.clamp(0.0, n as f32) as u32;
    [
        clamp(lo.x, size.0),
        clamp(lo.y, size.1),
        clamp(hi.x, size.0),
        clamp(hi.y, size.1),
    ]
}

/// The crown's pixels of `c` the tree covers: not the sky, and as far from the eye as some part
/// of the tree's sphere.
fn coverage(c: &RemasterCapture, tree: &Tree, area: Area) -> Vec<bool> {
    let d = distances(c);
    let eye = glam::Vec3::from_array(c.eye);
    let far = eye.distance(glam::Vec3::from_array(tree.centre));
    let (near, beyond) = (
        f64::from(far - tree.radius * 1.1),
        f64::from(far + tree.radius * 1.1),
    );
    let w = c.render_size.0;
    let mut out = Vec::new();
    for y in area[1]..area[3] {
        for x in area[0]..area[2] {
            let i = (y * w + x) as usize;
            out.push(c.class[i] != 5 && d[i] >= near && d[i] <= beyond);
        }
    }
    out
}

/// Lone pixels of `mask` (`width` wide).
fn lone(mask: &[bool], width: usize) -> usize {
    let height = mask.len() / width.max(1);
    let mut n = 0;
    for y in 1..height.saturating_sub(1) {
        for x in 1..width.saturating_sub(1) {
            let i = y * width + x;
            let around = [i - 1, i + 1, i - width, i + width]
                .iter()
                .filter(|&&j| mask[j])
                .count();
            if (mask[i] && around <= 1) || (!mask[i] && around >= 3) {
                n += 1;
            }
        }
    }
    n
}

/// The pixels of `rgba` (`size`) in `area`, RGBA.
fn cut(rgba: &[u8], size: (u32, u32), area: Area) -> Vec<u8> {
    let mut out = Vec::new();
    for y in area[1]..area[3] {
        let row = (y * size.0) as usize * 4;
        out.extend_from_slice(&rgba[row + area[0] as usize * 4..row + area[2] as usize * 4]);
    }
    out
}

/// A pixel's brightness, 0 to 255.
fn luma(p: &[u8]) -> f32 {
    0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2])
}

/// The share of `rgba`'s pixels (`width` wide) standing out from their four neighbors' mean
/// brightness by more than `levels`.
fn speckle(rgba: &[u8], width: usize, levels: f32) -> f64 {
    let height = rgba.len() / 4 / width.max(1);
    let l = |x: usize, y: usize| luma(&rgba[(y * width + x) * 4..]);
    let (mut n, mut all) = (0usize, 0usize);
    for y in 1..height.saturating_sub(1) {
        for x in 1..width.saturating_sub(1) {
            let around = (l(x - 1, y) + l(x + 1, y) + l(x, y - 1) + l(x, y + 1)) * 0.25;
            all += 1;
            if (l(x, y) - around).abs() > levels {
                n += 1;
            }
        }
    }
    #[allow(clippy::cast_precision_loss)] // pixel counts
    let share = n as f64 / all.max(1) as f64;
    share
}

/// The share of pixels that differ by more than `levels` in any channel.
fn apart(a: &[u8], b: &[u8], levels: u8) -> f64 {
    let n = a
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0)
        .filter(|(p, q)| (0..3).any(|k| p[k].abs_diff(q[k]) > levels))
        .count();
    #[allow(clippy::cast_precision_loss)] // pixel counts
    let share = n as f64 / (a.len() / 4).max(1) as f64;
    share
}

/// One run: `frames` frames looking at `tree` from `eye`, still or walking sideways.
fn run(
    (shot, gpu): (&mut Shot, &mut Gpu),
    tree: &Tree,
    eye: Vec3,
    (frames, display): (usize, (u32, u32)),
    moving: bool,
) -> Measured {
    let look = |e: Vec3| {
        let (dx, dy, dz) = (
            tree.centre[0] - e.x,
            tree.centre[1] - e.y,
            tree.centre[2] - e.z,
        );
        let yaw = math::atan2f(-dx, dy);
        let pitch = math::atan2f(dz, math::hypotf(dx, dy));
        (yaw, pitch)
    };
    let side = {
        let (dx, dy) = (tree.centre[0] - eye.x, tree.centre[1] - eye.y);
        let l = math::hypotf(dx, dy).max(1e-3);
        (dy / l, -dx / l)
    };
    let (yaw, pitch) = look(eye);
    shot.hold_camera(eye, yaw, pitch);
    for _ in 0..SETTLE {
        shot.step_held(gpu);
    }
    let mut m = Measured::default();
    let mut area: Option<(Area, Area)> = None;
    let mut last: Option<Vec<u8>> = None;
    let mut changes = 0.0;
    for k in 0..frames {
        if moving {
            #[allow(clippy::cast_precision_loss)] // a few frames
            let s = WALK * k as f32 / 60.0;
            let at = Vec3::new(eye.x + side.0 * s, eye.y + side.1 * s, eye.z);
            let (yaw, pitch) = look(at);
            shot.hold_camera(at, yaw, pitch);
        }
        shot.step_held(gpu);
        let c = shot
            .scene
            .draw
            .remaster_capture(gpu)
            .expect("the remaster drew");
        let shown = stations::capture(gpu);
        if k == 0 && !moving {
            println!(
                "  eye held at {:?}, drawn at {:?}; the world {:?} on {display:?}",
                eye, c.eye, c.render_size
            );
        }
        let (inner, outer) = *area.get_or_insert_with(|| {
            (
                crown_box(&c, tree, c.render_size),
                crown_box(&c, tree, display),
            )
        });
        let mask = coverage(&c, tree, inner);
        if m.covered.is_empty() {
            m.covered = vec![0.0; mask.len()];
        }
        for (s, &v) in m.covered.iter_mut().zip(&mask) {
            *s += f32::from(u8::from(v));
        }
        #[allow(clippy::cast_precision_loss)] // pixel counts
        {
            m.lone += lone(&mask, (inner[2] - inner[0]) as usize) as f64;
        }
        let crown = cut(&shown, display, outer);
        if let Some(was) = last.as_ref() {
            changes += apart(was, &crown, 16);
        }
        m.size = (outer[2] - outer[0], outer[3] - outer[1]);
        m.strip.push(crown.clone());
        last = Some(crown);
    }
    #[allow(clippy::cast_precision_loss)] // a few frames
    let n = frames.max(1) as f32;
    for s in &mut m.covered {
        *s /= n;
    }
    m.lone /= f64::from(n);
    m.changed = changes / f64::from((n - 1.0).max(1.0));
    m.picture = last.unwrap_or_default();
    m.speckle = speckle(&m.picture, m.size.0 as usize, 24.0);
    m
}

/// `frames` (each `size`) side by side, as one picture.
fn side_by_side(frames: &[Vec<u8>], size: (u32, u32)) -> (u32, Vec<u8>) {
    let (w, h) = (size.0 as usize, size.1 as usize);
    let n = frames.len();
    let mut out = vec![0u8; w * n * h * 4];
    for (k, f) in frames.iter().enumerate() {
        for y in 0..h {
            let from = &f[y * w * 4..(y + 1) * w * 4];
            let at = (y * w * n + k * w) * 4;
            out[at..at + w * 4].copy_from_slice(from);
        }
    }
    (u32::try_from(w * n).unwrap_or(0), out)
}

/// The largest tree the impostors draw near the report's place, from each distance, each way.
#[test]
#[ignore = "instrument: draws a tree as its impostor and as its mesh and writes its crown; run with --ignored"]
fn capture_a_tree_as_its_impostor_and_its_mesh() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let pack = env("DERETH_ART_PACK").map(|list| {
        let paths: Vec<std::path::PathBuf> =
            list.split(';').map(std::path::PathBuf::from).collect();
        Arc::new(ArtPack::open_stack(&paths).expect("the packs open"))
    });
    let dir = env("DERETH_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    if let Some(d) = dir.as_ref() {
        std::fs::create_dir_all(d).expect("the pictures' folder");
    }
    let tag = env("DERETH_REMASTER_LOOK_TAG").unwrap_or_else(|| "remaster".to_owned());
    let size = env("DERETH_REMASTER_SIZE").map_or((1280, 540), |_| super::remaster_frames::size());
    let report =
        VisualReport::parse(&env("DERETH_IMPOSTOR_PLACE").unwrap_or_else(|| HILL.to_owned()))
            .expect("the place's report reads");
    let mut boxes = env("DERETH_REMASTER_LOOK_BOXES").unwrap_or_else(|| EVERY_BOX.to_owned());
    if pack.is_some() {
        boxes.push_str(",RemasterArtPack=1");
    }
    let distances: Vec<f32> = env("DERETH_IMPOSTOR_DISTANCES")
        .unwrap_or_else(|| "60,120,180".to_owned())
        .split(',')
        .map(|v| v.trim().parse().expect("a distance"))
        .collect();
    let between: Option<f32> =
        env("DERETH_IMPOSTOR_BETWEEN").map(|v| v.trim().parse().expect("a share"));
    let ways = env("DERETH_IMPOSTOR_WAYS").unwrap_or_else(|| "mesh,impostor".to_owned());
    let frames: usize =
        env("DERETH_IMPOSTOR_FRAMES").map_or(16, |v| v.trim().parse().expect("a count"));
    let level = env("DERETH_IMPOSTOR_EYE").is_some_and(|v| v.trim() == "level");

    #[allow(unused_mut)]
    let mut cfg = DeviceConfig {
        width: size.0,
        height: size.1,
        remaster: true,
        ..DeviceConfig::default()
    };
    #[cfg(all(feature = "dlss", windows))]
    {
        cfg.remaster_extensions = dereth_scene::world_scene::remaster_device_extensions();
    }
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let station = report_station(&report, None);
    let mut shot = Shot::open_with(&store, &mut gpu, &station, false, None);
    shot.apply_report(&report);
    shot.poll(&mut gpu);
    shot.scene.draw.set_remaster_pack(pack.clone(), None);
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(&boxes).expect("the boxes read");
    shot.poll(&mut gpu);
    // Every tree its impostor wherever its error allows, to find them.
    let every = RemasterLod {
        error_px: 1.0e9,
        ..RemasterLod::ON
    };
    shot.scene.draw.set_remaster_lod(every, &mut gpu);
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    for _ in 0..SETTLE {
        shot.step_held(&mut gpu);
    }
    let trees: Vec<Tree> = shot
        .scene
        .draw
        .remaster_impostor_cards(&mut gpu)
        .into_iter()
        .map(|(gfxobj, centre, radius, turn, _)| Tree {
            gfxobj,
            centre,
            radius,
            turn,
        })
        .collect();
    let eye = shot.scene.camera.position;
    // The largest, of those near the viewer: a step past it stays among the blocks whose own
    // trees stand in the world.
    let target = *trees
        .iter()
        .filter(|t| math::hypotf(t.centre[0] - eye.x, t.centre[1] - eye.y) < 120.0)
        .max_by(|a, b| a.radius.total_cmp(&b.radius))
        .expect("a tree drawn as its impostor near the place");
    let reach = distances.iter().copied().fold(0.0f32, f32::max);
    let bearing = clearest(&target, &trees, reach, between);
    println!(
        "{} trees drawn as impostors; the tree: GfxObj {:08X} at [{:.1} {:.1} {:.1}] radius {:.2} \
         turned {:.3}; seen from bearing {:.3} (eye at {eye:?})",
        trees.len(),
        target.gfxobj,
        target.centre[0],
        target.centre[1],
        target.centre[2],
        target.radius,
        target.turn,
        bearing,
    );
    for d in &distances {
        let (x, y) = (
            target.centre[0] + math::cosf(bearing) * d,
            target.centre[1] + math::sinf(bearing) * d,
        );
        let ground = shot
            .scene
            .remaster_ground_at(x, y)
            .unwrap_or(target.centre[2]);
        let height = if level {
            target.centre[2]
        } else {
            ground + EYE
        };
        let eye = Vec3::new(x, y, height);
        let mut mesh: Option<Vec<f32>> = None;
        for way in ways.split(',').map(str::trim) {
            let lod = match way {
                "mesh" => RemasterLod {
                    impostors: false,
                    ..RemasterLod::ON
                },
                "impostor" => every,
                "levels" => RemasterLod::ON,
                other => panic!("no way {other}: mesh, impostor or levels"),
            };
            shot.scene.draw.set_remaster_lod(lod, &mut gpu);
            for moving in [false, true] {
                let m = run((&mut shot, &mut gpu), &target, eye, (frames, size), moving);
                let imp = shot
                    .scene
                    .draw
                    .remaster_impostor_stats(&mut gpu)
                    .unwrap_or_default();
                // How much of the tree its own card draws: 1 all of it, between while it fades.
                let share = shot
                    .scene
                    .draw
                    .remaster_impostor_cards(&mut gpu)
                    .into_iter()
                    .find(|c| {
                        c.0 == target.gfxobj
                            && math::hypotf(c.1[0] - target.centre[0], c.1[1] - target.centre[1])
                                < 0.5
                    })
                    .map_or(0.0, |c| c.4);
                let crown = m.covered.len().max(1);
                let covered: f32 = m.covered.iter().sum();
                #[allow(clippy::cast_precision_loss)] // pixel counts
                let held = covered / crown as f32;
                let (open, extra) = mesh.as_ref().map_or((0.0, 0.0), |base| {
                    let held = base.iter().filter(|&&v| v > 0.5).count().max(1);
                    let open = base
                        .iter()
                        .zip(&m.covered)
                        .filter(|(b, v)| **b > 0.5 && **v < 0.25)
                        .count();
                    let extra = base
                        .iter()
                        .zip(&m.covered)
                        .filter(|(b, v)| **b < 0.25 && **v > 0.5)
                        .count();
                    #[allow(clippy::cast_precision_loss)] // pixel counts
                    (open as f32 / held as f32, extra as f32 / held as f32)
                });
                #[allow(clippy::cast_precision_loss)] // pixel counts
                let lone_share = m.lone / f64::from(covered.max(1.0));
                let run_name = if moving { "moving" } else { "still" };
                println!(
                    "{d:5.0} m {way:8} {run_name:6}: crown {crown:7} px covered {held:.3} \
                     (open where the mesh covers {open:.3}, covered where it is open {extra:.3}) \
                     lone {lone_share:.4} of covered | shown changed {:.4} a frame, speckle {:.4} \
                     | cards {}, the tree's {share:.2} | {}",
                    m.changed,
                    m.speckle,
                    imp.cards,
                    dereth_scene::world_scene::remaster_upscaler_drawn(),
                );
                if way == "mesh" && !moving {
                    mesh = Some(m.covered.clone());
                }
                if let Some(dir) = dir.as_ref() {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    // LINT-OK: meters, a few hundred
                    let name = format!("crown-{tag}-{:03}-{way}", *d as u32);
                    if !moving {
                        stations::write_png(
                            &dir.join(format!("{name}.png")),
                            m.size.0,
                            m.size.1,
                            &m.picture,
                        );
                    }
                    let (w, strip) = side_by_side(&m.strip, m.size);
                    stations::write_png(
                        &dir.join(format!("{name}-{run_name}.png")),
                        w,
                        m.size.1,
                        &strip,
                    );
                }
            }
        }
    }
    drop(shot);
    drop(gpu);
}
