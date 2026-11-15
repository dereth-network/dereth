//! Behaviour: none (experimental)
//!
//! What moves under the anti-aliasing: the moving bodies, and the leaves the wind sways. Fixtures:
//! the town by day, the object standing nearest the look point taken as a body of the frame, moved
//! [`STEP`] meters east between two frames; the camp's ferns by day, swaying in the wind as the
//! clock runs.
//!
//! `AUBEREAN_REMASTER_CAPTURE_DIR`, when set, names a folder the frames after the move (and of the
//! swaying ferns), with the motion and without it, and the settled frames are written to.

use auberean_draw::scene::{Frame, Pass};
use auberean_remaster::Remaster;
use dereth_gfx_passes::post::PostSettings;

use crate::common::{apart, frame, records, turn, within_devices, write_png, STATIONS};

const W: u32 = 640;
const H: u32 = 480;
/// How far the body moves, meters.
const STEP: f32 = 1.0;
/// The frames drawn before the move, which the anti-aliasing settles over.
const SETTLE: usize = 24;

/// The town's frame with its object nearest the look point a body, and the same frame with that
/// body moved.
fn frames() -> (Frame, Frame) {
    let s = *STATIONS
        .iter()
        .find(|s| s.name == "town-6fa9")
        .expect("the station");
    let mut before = frame(&s);
    let place = |k: usize| {
        let t = before.objects[k].transform.0[3];
        (t[0] - s.at[0]).powi(2) + (t[1] - s.at[1]).powi(2)
    };
    let nearest = (0..before.objects.len())
        .filter(|&k| {
            let b = &before.objects[k];
            !b.indices.is_empty() && b.pass() == Pass::Opaque && b.posed.is_none()
        })
        .min_by(|&a, &b| place(a).total_cmp(&place(b)))
        .expect("an object to move");
    // The object's draws: those beside it standing where it stands.
    let at = before.objects[nearest].transform;
    let mut start = nearest;
    while start > 0 && before.objects[start - 1].transform == at {
        start -= 1;
    }
    let mut end = nearest + 1;
    while end < before.objects.len() && before.objects[end].transform == at {
        end += 1;
    }
    before.bodies = vec![(7, start..end)];
    let mut after = before.clone();
    for b in &mut after.objects[start..end] {
        b.transform.0[3][0] += STEP;
    }
    (before, after)
}

fn remaster() -> Remaster {
    let mut r = Remaster::with(&dereth_gfx::DeviceDesc {
        debug: true,
        ..dereth_gfx::DeviceDesc::default()
    })
    .expect("the remaster's device");
    r.set_post(PostSettings {
        taa: true,
        ..PostSettings::default()
    });
    r
}

/// The frame settled before the move and the frame drawn right after it, with the motion or not.
fn across_the_move(motion: bool, before: &Frame, after: &Frame) -> (Vec<u8>, Vec<u8>) {
    let records = records();
    let mut load = |id: u32| auberean_draw::picture::load(records, id);
    let mut r = remaster();
    r.set_motion(motion);
    let mut settled = Vec::new();
    for _ in 0..SETTLE {
        settled = r.render(before, W, H, &mut load).expect("the frame").rgba;
    }
    let moved = r.render(after, W, H, &mut load).expect("the frame").rgba;
    (settled, moved)
}

/// The mean of the largest channel difference between `a` and `b` over the pixels `of` picks.
fn mean_apart(a: &[u8], b: &[u8], of: &[bool]) -> f64 {
    let (mut sum, mut n) = (0u64, 0u64);
    for ((x, y), on) in a.chunks(4).zip(b.chunks(4)).zip(of) {
        if *on {
            sum += u64::from((0..3).map(|c| x[c].abs_diff(y[c])).max().unwrap_or(0));
            n += 1;
        }
    }
    #[allow(clippy::cast_precision_loss)] // pixel counts and level sums
    let mean = sum as f64 / n.max(1) as f64;
    mean
}

/// **A moving body is gathered from where it moved from**: at the town under the anti-aliasing,
/// with an object taken as a body and moved a meter, the frame right after the move is nearer
/// the frame settled with the body where it moved to, over the pixels the move changed, with the
/// motion drawn than without it, by at least a fifth; and while nothing moves the motion changes no
/// pixel (each drawn on a device of its own: byte for byte, or within [`CROSS_DEVICE`]).
///
/// [`CROSS_DEVICE`]: crate::common::CROSS_DEVICE
#[test]
fn a_moving_body_is_gathered_from_where_it_moved_from() {
    let _turn = turn();
    let (before, after) = frames();
    let (still_with, moved_with) = across_the_move(true, &before, &after);
    let (still_without, moved_without) = across_the_move(false, &before, &after);
    let records = records();
    let mut load = |id: u32| auberean_draw::picture::load(records, id);
    let mut r = remaster();
    let mut settled = Vec::new();
    for _ in 0..SETTLE {
        settled = r.render(&after, W, H, &mut load).expect("the frame").rgba;
    }
    let errors = dereth_gfx::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    if let Some(dir) = std::env::var_os("AUBEREAN_REMASTER_CAPTURE_DIR") {
        let dir = std::path::PathBuf::from(dir);
        write_png(&dir.join("motion-with.png"), W, H, &moved_with);
        write_png(&dir.join("motion-without.png"), W, H, &moved_without);
        write_png(&dir.join("motion-settled.png"), W, H, &settled);
    }
    // Where the move changed the settled frame by more than sixteen levels.
    let changed: Vec<bool> = still_with
        .chunks(4)
        .zip(settled.chunks(4))
        .map(|(x, y)| (0..3).any(|c| x[c].abs_diff(y[c]) > 16))
        .collect();
    let n = changed.iter().filter(|c| **c).count();
    let with = mean_apart(&moved_with, &settled, &changed);
    let without = mean_apart(&moved_without, &settled, &changed);
    eprintln!(
        "town-6fa9: the move changed {n} pixels; right after it, {with:.2} levels from the settled \
         frame with the motion, {without:.2} without"
    );
    assert!(n >= 200, "the move changed too little: {n} pixels");
    assert!(
        with <= without * 0.8,
        "the motion gathered no nearer: {with:.2} with, {without:.2} without"
    );
    assert!(
        within_devices(&still_with, &still_without),
        "while nothing moves, the motion changes the frame: {:?} (pixels, levels) apart",
        apart(&still_with, &still_without)
    );
}

/// The wind the camp's ferns sway in, meters a second east (a gale, so the leaves flutter by
/// pixels a frame), and the frames a second the clock runs at.
const WIND: f32 = 12.0;
const RATE: f64 = 10.0;

/// The camp's frame at `frames` frames of the clock, in the wind `wind`.
fn camp_at(base: &Frame, wind: f32, frames: usize) -> Frame {
    let mut f = base.clone();
    f.wind = [wind, 0.0, 0.0];
    #[allow(clippy::cast_precision_loss)] // a few dozen frames
    let at = frames as f64 / RATE;
    f.uptime = base.uptime + at;
    f
}

/// The camp drawn for [`SETTLE`] frames under the anti-aliasing with the sway on, the clock
/// running a frame at a time from `from` (or held there, when `held`), with the motion or not: the
/// last frame.
fn swaying(base: &Frame, wind: f32, from: usize, held: bool, motion: bool) -> Vec<u8> {
    let records = records();
    let mut load = |id: u32| auberean_draw::picture::load(records, id);
    let mut r = remaster();
    r.set_sway(true);
    r.set_motion(motion);
    let mut last = Vec::new();
    for k in 0..SETTLE {
        let f = camp_at(base, wind, if held { from } else { from + k });
        last = r.render(&f, W, H, &mut load).expect("the frame").rgba;
    }
    last
}

/// **Swaying leaves are gathered from where the wind bent them**: at the camp under the
/// anti-aliasing, with the ferns swaying in the wind as the clock runs, the last frame is nearer
/// the frame settled with the clock held at that moment, over the pixels the sway moves, with the
/// motion drawn than without it, by at least a twentieth (the anti-aliasing still softens what
/// moves: each frame takes its history from between last frame's pixels); and with no wind the
/// motion changes no pixel (each drawn on a device of its own: byte for byte, or within
/// [`CROSS_DEVICE`]).
///
/// [`CROSS_DEVICE`]: crate::common::CROSS_DEVICE
#[test]
fn swaying_leaves_are_gathered_from_where_the_wind_bent_them() {
    let _turn = turn();
    let s = *STATIONS
        .iter()
        .find(|s| s.name == "camp-c156")
        .expect("the station");
    let base = frame(&s);
    let end = SETTLE - 1;
    let with = swaying(&base, WIND, 0, false, true);
    let without = swaying(&base, WIND, 0, false, false);
    let settled = swaying(&base, WIND, end, true, false);
    // Where the sway moves the settled frame: held a frame earlier, by over sixteen levels.
    let earlier = swaying(&base, WIND, end - 1, true, false);
    let errors = dereth_gfx::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    if let Some(dir) = std::env::var_os("AUBEREAN_REMASTER_CAPTURE_DIR") {
        let dir = std::path::PathBuf::from(dir);
        write_png(&dir.join("sway-motion-with.png"), W, H, &with);
        write_png(&dir.join("sway-motion-without.png"), W, H, &without);
        write_png(&dir.join("sway-motion-settled.png"), W, H, &settled);
    }
    let moving: Vec<bool> = earlier
        .chunks(4)
        .zip(settled.chunks(4))
        .map(|(x, y)| (0..3).any(|c| x[c].abs_diff(y[c]) > 16))
        .collect();
    let n = moving.iter().filter(|c| **c).count();
    let near_with = mean_apart(&with, &settled, &moving);
    let near_without = mean_apart(&without, &settled, &moving);
    eprintln!(
        "camp-c156: the sway moves {n} pixels; swaying, {near_with:.2} levels from the settled \
         frame with the motion, {near_without:.2} without"
    );
    assert!(n >= 1000, "the sway moved too little: {n} pixels");
    assert!(
        near_with <= near_without * 0.95,
        "the motion gathered no nearer: {near_with:.2} with, {near_without:.2} without"
    );
    let calm_with = swaying(&base, 0.0, 0, false, true);
    let calm_without = swaying(&base, 0.0, 0, false, false);
    assert!(
        within_devices(&calm_with, &calm_without),
        "with no wind, the motion changes the frame: {:?} (pixels, levels) apart",
        apart(&calm_with, &calm_without)
    );
}
