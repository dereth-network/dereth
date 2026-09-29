//! Ten hook-rich real animations give a bit-reproducible trace; part placement is a step function
//! of time; one big dt fires the same hooks as many small steps.
//! Fixture: the shipped retail DAT records and recorded inputs.

use super::common;

use dereth_animation::data::{AnimAssets, AnimData};
use dereth_animation::hooks::AnimHook;
use dereth_animation::seq::Sequence;
use dereth_dat::DbType;
use dereth_primitives::{DataId, Frame};

/// The ten busiest animations in the portal dat: the most hooks, which is where a trace has the
/// most to say.
fn busiest_ten(assets: &common::DatAssets) -> Vec<DataId> {
    let mut scored: Vec<(usize, DataId)> = Vec::new();
    for id in assets.ids_of(DbType::Anim) {
        let Some(a) = assets.animation(id) else {
            continue;
        };
        let hooks: usize = a.part_frames.iter().map(|f| f.hooks.len()).sum();
        if hooks > 0 && a.num_frames >= 4 {
            scored.push((hooks, id));
        }
    }
    scored.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.truncate(10);
    scored.into_iter().map(|(_, id)| id).collect()
}

fn trace(seq: &mut Sequence, dts: &[f64]) -> (Vec<(i32, u32)>, Frame) {
    let mut acc = Frame::default();
    let mut fired = Vec::new();
    for dt in dts {
        let mut out: Vec<AnimHook> = Vec::new();
        let before = seq.curr_frame_number();
        seq.update(*dt, Some(&mut acc), &mut out);
        for h in out {
            fired.push((before, h.hook_type()));
        }
    }
    (fired, acc)
}

#[test]
fn ten_real_animations_produce_a_reproducible_frame_crossing_trace() {
    let assets = common::open();
    let ids = busiest_ten(&assets);
    assert_eq!(
        ids.len(),
        10,
        "ten animations with hooks and at least four frames"
    );

    // A deliberately uneven step sequence: some steps cross no frame, some cross several.
    let dts = [
        1.0 / 60.0,
        1.0 / 30.0,
        1.0 / 30.0,
        0.2,
        1.0 / 120.0,
        0.5,
        1.0 / 30.0,
        0.05,
        0.3,
        1.0 / 30.0,
    ];

    let mut total_hooks = 0usize;
    for id in ids {
        let anim = assets.animation(id).expect("animation");
        let mk = || {
            let mut s = Sequence::new();
            s.append_animation(
                AnimData {
                    anim_id: id,
                    low_frame: 0,
                    high_frame: -1,
                    framerate: 30.0,
                },
                &assets,
            );
            s
        };

        let (a, fa) = trace(&mut mk(), &dts);
        let (b, fb) = trace(&mut mk(), &dts);
        assert_eq!(a, b, "{id}: the trace is not reproducible");
        assert_eq!(
            fa.origin, fb.origin,
            "{id}: the accumulated frame is not reproducible"
        );
        assert!(!a.is_empty(), "{id}: a busy animation fired nothing");
        total_hooks += a.len();

        // Every hook that fired belongs to some frame of this animation and is one of the 27
        // types. Type 4 can never appear from the data — it is dropped on unpack — but the
        // player's own global `AnimDoneHook` is type 4, and a single looping node never raises it.
        for (frame, ty) in &a {
            assert!(
                *frame >= 0 && *frame < i32::try_from(anim.num_frames).expect("frames"),
                "{id}: hook reported at frame {frame} of {}",
                anim.num_frames
            );
            assert!(*ty <= 26, "{id}: hook type {ty}");
            assert_ne!(
                *ty, 4,
                "{id}: a data-driven AnimationDone survived the unpacker"
            );
        }
    }
    eprintln!("frame trace: {total_hooks} hook firings across ten animations");
    assert!(
        total_hooks > 100,
        "only {total_hooks} firings: the trace is too thin to protect 6.2"
    );
}

/// Behaviour: animation.sequence.parts-step-with-no-interpolation-and-hooks-fire-on-crossed-frames
#[test]
fn part_placement_is_a_step_function_of_time_with_no_interpolation() {
    let assets = common::open();
    let ids = busiest_ten(&assets);
    for id in ids {
        let anim = assets.animation(id).expect("animation");
        if anim.num_parts == 0 {
            continue;
        }
        let mut s = Sequence::new();
        s.append_animation(
            AnimData {
                anim_id: id,
                low_frame: 0,
                high_frame: -1,
                framerate: 30.0,
            },
            &assets,
        );
        // Sample forty times per animation frame for one whole cycle.
        let mut seen: Vec<Frame> = Vec::new();
        let mut out = Vec::new();
        for _ in 0..(anim.num_frames * 40) {
            let af = s.get_curr_animframe().expect("an animation frame");
            let f = af.frames[0];
            if seen.last() != Some(&f) {
                seen.push(f);
            }
            s.update(1.0 / (30.0 * 40.0), None, &mut out);
            out.clear();
        }
        // One distinct placement per frame, however finely time is sampled — and never more.
        assert!(
            seen.len() <= usize::try_from(anim.num_frames).expect("frames") + 1,
            "{id}: {} distinct placements over {} frames — that is interpolation",
            seen.len(),
            anim.num_frames
        );
        // And the placement always equals `part_frames[floor(frame_number)]` exactly.
        let i = s.curr_frame_number();
        let expect = anim.part_frame(i).expect("frame").frames[0];
        assert_eq!(
            s.get_curr_animframe().expect("frame").frames[0],
            expect,
            "{id}"
        );
    }
}

/// Behaviour: animation.sequence.parts-step-with-no-interpolation-and-hooks-fire-on-crossed-frames
/// One big step and many small ones cross the same frames, so they fire the same hooks in the same
/// order. A "sample the animation at time t" player would fire only the last frame's hooks in the
/// big step.
#[test]
fn one_big_step_fires_the_same_hooks_as_many_small_ones() {
    let assets = common::open();
    for id in busiest_ten(&assets) {
        let mk = || {
            let mut s = Sequence::new();
            s.append_animation(
                AnimData {
                    anim_id: id,
                    low_frame: 0,
                    high_frame: -1,
                    framerate: 30.0,
                },
                &assets,
            );
            s
        };
        let mut big = mk();
        let mut out_big = Vec::new();
        big.update(6.0 / 30.0, None, &mut out_big);

        let mut small = mk();
        let mut out_small = Vec::new();
        for _ in 0..6 {
            small.update(1.0 / 30.0, None, &mut out_small);
        }
        assert_eq!(
            out_big.iter().map(|h| h.hook_type()).collect::<Vec<_>>(),
            out_small.iter().map(|h| h.hook_type()).collect::<Vec<_>>(),
            "{id}: the crossing loop is driven by frames, not by calls"
        );
        assert_eq!(big.curr_frame_number(), small.curr_frame_number(), "{id}");
    }
}
