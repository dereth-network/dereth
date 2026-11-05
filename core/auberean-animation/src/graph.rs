//! The animation graph: how an animation record becomes bones.
//!
//! An animation is a keyframe clip or a node over others:
//!
//! - a **blend** (a "cube") places its children on a grid of slider ticks and plays them weighted
//!   by where the sliders stand between the ticks, every child at the same phase;
//! - an **axis cube** weighs each child by its own slider's value, an origin animation taking the
//!   weight they leave;
//! - a **switch** plays one child chosen by the body's mode, element or ground, or its default;
//! - a **body** node plays its one child (the body's locomotion sliders are set for it, see
//!   [`crate::body`]);
//! - a **random** node crossfades among its children along a slider, now and then rolling which
//!   child comes next;
//! - **look** and **angle** nodes turn bones of the pose built so far toward where the body looks
//!   or to the ground's slope (see [`crate::turns`]).
//!
//! A node's period is its children's, weighted the same way, so a blend's cycle lengthens and
//! shortens with its sliders. A node's triggers (effects, hooks, impulses at moments of the cycle)
//! are its children's: a blend merges its children's effects, weighting their times and strengths.

use std::sync::Arc;

use auberean_assets::animation::{Animation, AnimationKind, GameHook, SliderTicks};
use auberean_primitives::DataId;
use auberean_world::chargen::Random;

use crate::clip::{Clip, Pose};
use crate::dice;
use crate::sliders::{SliderInterp, Sliders};
use crate::table::Table;
use crate::triggers::{trigger, Gathered, Trigger};
use crate::turns::{self, Ground, Lean};

/// Weights below this are dropped.
pub const MIN_WEIGHT: f32 = 0.0002;

/// An axis cube's slider weights below this are dropped.
const AXIS_MIN_WEIGHT: f32 = 0.01;

/// How deep a graph may nest before it is taken for a broken one.
const MAX_DEPTH: usize = 32;

/// The animation kinds, as the records number them.
pub mod kind {
    pub const KEYFRAMES: u32 = 0x4000_0001;
    pub const BLEND: u32 = 0x4000_0002;
    pub const BODY: u32 = 0x4000_0003;
    pub const LOOK: u32 = 0x4000_0005;
    pub const AXIS_CUBE: u32 = 0x4000_0006;
    pub const RANDOM: u32 = 0x4000_0007;
    pub const SWITCH: u32 = 0x4000_0008;
    pub const ANGLE: u32 = 0x4000_000A;
}

/// Where the animation records come from.
pub trait Animations {
    /// The animation record `id`, decoded.
    fn animation(&self, id: DataId) -> Option<Arc<Animation>>;
}

/// What a graph is read against: the body's sliders (and their crossfades), and the mode,
/// element and ground switches choose by.
#[derive(Clone, Copy, Debug)]
pub struct GraphState<'a> {
    pub sliders: &'a Sliders,
    pub mode: u32,
    pub element: u32,
    pub terrain: u32,
}

/// What a graph is played against: the state, mutable where playing changes it (a random node
/// starts crossfades and rolls dice, the slope lean steers its sliders), the type of each bone of
/// the part played (the turns pick their bones by type) and the ground under the body.
#[derive(Debug)]
pub struct Eval<'a> {
    pub sliders: &'a mut Sliders,
    pub dice: &'a mut Random,
    pub mode: u32,
    pub element: u32,
    pub terrain: u32,
    pub bone_types: &'a [u32],
    pub ground: Ground,
}

impl Eval<'_> {
    /// The state to read the graph against.
    #[must_use]
    pub fn state(&self) -> GraphState<'_> {
        GraphState {
            sliders: self.sliders,
            mode: self.mode,
            element: self.element,
            terrain: self.terrain,
        }
    }
}

/// The switch kinds: by mode, by element, by the ground under the body.
pub mod switch_kind {
    pub const MODE: u32 = 0;
    pub const ELEMENT: u32 = 1;
    pub const TERRAIN: u32 = 2;
}

/// The kind number of an animation.
// D2-15415, D2-15339, D2-15406, D2-15380
#[must_use]
pub fn kind_of(a: &Animation) -> u32 {
    match &a.animator {
        AnimationKind::Keyframes(_) => kind::KEYFRAMES,
        AnimationKind::Blend { .. } => kind::BLEND,
        AnimationKind::Body { .. } => kind::BODY,
        AnimationKind::Look => kind::LOOK,
        AnimationKind::AxisCube { .. } => kind::AXIS_CUBE,
        AnimationKind::Random { .. } => kind::RANDOM,
        AnimationKind::Switch { .. } => kind::SWITCH,
        AnimationKind::Angle { .. } => kind::ANGLE,
    }
}

/// A blend's weight for each of its children: multilinear over the tick grid, the first slider
/// varying fastest; weights below [`MIN_WEIGHT`] dropped, the rest not renormalized.
// D2-15302
#[must_use]
pub fn cube_weights(sliders: &[SliderTicks], children: usize, state: &GraphState<'_>) -> Vec<f32> {
    let mut out = vec![0.0; children];
    struct Axis {
        lo: usize,
        hi: usize,
        a: f32,
        count: usize,
    }
    let mut axes = Vec::with_capacity(sliders.len());
    for s in sliders {
        let ticks: Vec<f32> = s.ticks.iter().map(|&t| f32::from_bits(t)).collect();
        if ticks.is_empty() {
            return out;
        }
        let v = state.sliders.current(s.slider);
        let (first, last) = (ticks[0], ticks[ticks.len() - 1]);
        let c = v.clamp(first.min(last), first.max(last));
        let (lo, hi) = match ticks.iter().position(|&t| t == c) {
            Some(k) => (k, k),
            None => {
                let lo = ticks.iter().rposition(|&t| t <= c).unwrap_or(0);
                (lo, (lo + 1).min(ticks.len() - 1))
            }
        };
        let a = if lo == hi {
            0.5
        } else {
            (c - ticks[hi]) / (ticks[lo] - ticks[hi])
        };
        axes.push(Axis {
            lo,
            hi,
            a,
            count: ticks.len(),
        });
    }
    let corners = 1usize << axes.len().min(16);
    for corner in 0..corners {
        let (mut w, mut idx, mut stride) = (1.0f32, 0usize, 1usize);
        for (s, axis) in axes.iter().enumerate() {
            let (a, k) = if corner & (1 << s) != 0 {
                (1.0 - axis.a, axis.hi)
            } else {
                (axis.a, axis.lo)
            };
            w *= a;
            idx += k * stride;
            stride *= axis.count;
        }
        if let Some(slot) = out.get_mut(idx) {
            *slot += w;
        }
    }
    for w in &mut out {
        if *w < MIN_WEIGHT {
            *w = 0.0;
        }
    }
    out
}

/// An axis cube's weights: each child takes its slider's value (a child past the sliders takes
/// none), values under 0.01 dropped; when they sum over 1 they are scaled back to 1. The origin
/// takes what they leave (none when they sum to 1 or more, all when they sum to 0).
// D2-15364
#[must_use]
pub fn axis_weights(sliders: &[u32], children: usize, state: &GraphState<'_>) -> (Vec<f32>, f32) {
    let mut w = vec![0.0f32; children];
    for (i, s) in sliders.iter().enumerate() {
        if let Some(slot) = w.get_mut(i) {
            *slot += state.sliders.current(*s);
        }
    }
    let mut sum = 0.0f32;
    for v in &mut w {
        if *v < AXIS_MIN_WEIGHT {
            *v = 0.0;
        }
        sum += *v;
    }
    if w.is_empty() || sum == 0.0 {
        return (w, 1.0);
    }
    let origin = 1.0 - sum;
    if sum > 1.0 {
        let k = 1.0 / sum;
        for v in &mut w {
            *v *= k;
        }
    }
    (w, origin.max(0.0))
}

/// What a random node's crossfade plays: the child its crossfade started from and, while one
/// runs, the child it goes to and how far it is (the same child twice counts once). Choices are
/// cut toward zero into child indices.
// D2-15419, D2-06519
#[must_use]
pub fn interp_pick(children: &[DataId], c: &SliderInterp) -> (DataId, DataId, f32) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let pick = |v: f32| {
        children
            .get(v as i64 as usize)
            .copied()
            .unwrap_or(DataId::NONE)
    };
    let a = pick(c.start);
    if !c.active {
        return (a, DataId::NONE, 0.0);
    }
    let b = pick(c.end);
    if a == b {
        return (a, DataId::NONE, 0.0);
    }
    (a, b, c.current)
}

/// A slider's crossfade as the graph reads it: a slider without one reads as a new crossfade
/// (idle, at child 0), as the client makes one when first asked.
fn read_interp(state: &GraphState<'_>, slider: u32) -> SliderInterp {
    state
        .sliders
        .interps
        .get(slider)
        .copied()
        .unwrap_or_default()
}

/// The child a switch plays.
// D2-15391
#[must_use]
pub fn switch_child(
    switch_kind: u32,
    default: DataId,
    overrides: &[(u32, DataId)],
    state: &GraphState<'_>,
) -> DataId {
    let value = match switch_kind {
        switch_kind::MODE => state.mode,
        switch_kind::ELEMENT => state.element,
        switch_kind::TERRAIN => state.terrain,
        _ => return default,
    };
    overrides
        .iter()
        .find(|(k, _)| *k == value)
        .map_or(default, |(_, id)| *id)
}

/// The period of animation `id`, in seconds, at the current sliders: a clip's own, a blend's the
/// weighted sum of its children's, a random node's its crossfade's pair weighted by how far it is,
/// a pass-through node's its child's; an axis cube's, a look's and an angle's are fixed (0, 1, 1).
// D2-15115, D2-15237, D2-15308, D2-15322, D2-15392, D2-15405, D2-15421, D2-15371, D2-15332
#[must_use]
pub fn period(anims: &dyn Animations, id: DataId, state: &GraphState<'_>) -> f32 {
    period_at(anims, id, state, 0)
}

fn period_at(anims: &dyn Animations, id: DataId, state: &GraphState<'_>, depth: usize) -> f32 {
    if depth > MAX_DEPTH {
        return 0.0;
    }
    let Some(a) = anims.animation(id) else {
        return 0.0;
    };
    match &a.animator {
        AnimationKind::Keyframes(k) => k.period,
        AnimationKind::Blend {
            sliders,
            animations,
        } => {
            let w = cube_weights(sliders, animations.len(), state);
            animations
                .iter()
                .zip(&w)
                .filter(|(_, &w)| w > 0.0)
                .map(|(c, &w)| w * period_at(anims, *c, state, depth + 1))
                .sum()
        }
        AnimationKind::Body { animation } => period_at(anims, *animation, state, depth + 1),
        AnimationKind::Switch {
            switch_kind,
            default,
            overrides,
        } => {
            let c = switch_child(*switch_kind, *default, overrides, state);
            period_at(anims, c, state, depth + 1)
        }
        AnimationKind::Random {
            slider, animations, ..
        } => {
            let (a, b, f) = interp_pick(animations, &read_interp(state, *slider));
            if a.is_none() {
                return 0.0;
            }
            let mut p = 0.0;
            if anims.animation(a).is_some() {
                p = (1.0 - f) * period_at(anims, a, state, depth + 1);
            }
            if !b.is_none() && anims.animation(b).is_some() {
                p += f * period_at(anims, b, state, depth + 1);
            }
            p
        }
        AnimationKind::AxisCube { .. } => 0.0,
        AnimationKind::Look | AnimationKind::Angle { .. } => 1.0,
    }
}

/// Add animation `id` at `phase` cycles, scaled by `weight`, into `out`.
///
/// A look or an angle node turns bones of `out` in place ([`crate::turns`]). A random node,
/// when no crossfade runs on its rolling slider and more time has passed since the last than its
/// frequency give or take a roll of its variation, rolls a child and starts a crossfade to it over
/// its ramp time; it then plays its crossfade's pair.
// D2-15112, D2-15307, D2-15398, D2-15327, D2-15411, D2-15420, D2-15370
#[allow(clippy::too_many_arguments)]
pub fn animate(
    anims: &dyn Animations,
    id: DataId,
    eval: &mut Eval<'_>,
    weight: f32,
    phase: f32,
    parents: &[Option<usize>],
    out: &mut Pose,
) {
    animate_at(anims, id, eval, weight, phase, parents, out, 0);
}

#[allow(clippy::too_many_arguments)]
fn animate_at(
    anims: &dyn Animations,
    id: DataId,
    eval: &mut Eval<'_>,
    weight: f32,
    phase: f32,
    parents: &[Option<usize>],
    out: &mut Pose,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let Some(a) = anims.animation(id) else {
        return;
    };
    let child = |id: DataId, w: f32, eval: &mut Eval<'_>, out: &mut Pose| {
        animate_at(anims, id, eval, w, phase, parents, out, depth + 1);
    };
    match &a.animator {
        AnimationKind::Keyframes(k) => Clip { keys: k }.animate(weight, phase, parents, out),
        AnimationKind::Blend {
            sliders,
            animations,
        } => {
            let w = cube_weights(sliders, animations.len(), &eval.state());
            for (c, &wc) in animations.iter().zip(&w) {
                if wc >= MIN_WEIGHT {
                    child(*c, wc * weight, eval, out);
                }
            }
        }
        AnimationKind::Body { animation } => child(*animation, weight, eval, out),
        AnimationKind::Switch {
            switch_kind,
            default,
            overrides,
        } => {
            let c = switch_child(*switch_kind, *default, overrides, &eval.state());
            child(c, weight, eval, out);
        }
        AnimationKind::Random {
            slider,
            animations,
            frequency,
            ramp_time,
            random_slider,
            variation,
        } => {
            if !eval.sliders.interp(*random_slider).active {
                let roll = dice::roll_float(eval.dice, -variation, *variation);
                if roll + frequency < eval.sliders.interp(*random_slider).time {
                    if animations.is_empty() {
                        return;
                    }
                    let n = i32::try_from(animations.len()).unwrap_or(i32::MAX);
                    let k = dice::roll_int(eval.dice, 0, n - 1);
                    #[allow(clippy::cast_precision_loss)]
                    let to = k as f32;
                    let c = eval.sliders.interp(*random_slider);
                    let from = c.start;
                    c.activate(from, to, *ramp_time);
                }
            }
            let c = *eval.sliders.interp(*slider);
            let (a, b, f) = interp_pick(animations, &c);
            if a.is_none() {
                return;
            }
            child(a, (1.0 - f) * weight, eval, out);
            if !b.is_none() {
                child(b, f * weight, eval, out);
            }
        }
        AnimationKind::AxisCube {
            origin,
            animations,
            sliders,
        } => {
            let (w, o) = axis_weights(sliders, animations.len(), &eval.state());
            if o != 0.0 {
                child(*origin, o * weight, eval, out);
            }
            for (c, &wc) in animations.iter().zip(&w) {
                if wc != 0.0 {
                    child(*c, weight * wc, eval, out);
                }
            }
        }
        AnimationKind::Look => turns::look(weight, eval.sliders, eval.bone_types, out),
        AnimationKind::Angle {
            bones,
            max_x,
            min_x,
            max_y,
            min_y,
        } => {
            let node = Lean {
                bones,
                max_x: *max_x,
                min_x: *min_x,
                max_y: *max_y,
                min_y: *min_y,
            };
            turns::angle(
                &node,
                weight,
                &eval.ground,
                eval.sliders,
                eval.bone_types,
                out,
            );
        }
    }
}

/// Gather what animation `id` sets off by `time` cycles into `g`; `wrap` takes a clip's time
/// modulo its cycle. A blend, an axis cube and a random node gather each child's whole cycle,
/// merge the effects by weight (time, intensity, variation and probability summed, each times its
/// child's weight; whatever the animation had gathered before merges in with the first child) and
/// keep those due before `time` (wrapped into the cycle), counting them as started. A blend merges
/// its children's hooks the same way, one per kind of data (the time summed by weight, the rest
/// the first's), and keeps those due before `time`; an axis cube's and a random node's hooks, and
/// every node's impulses, are all gathered at once.
// D2-15113, D2-15238, D2-15315, D2-15325, D2-15394, D2-15408, D2-15429, D2-15379, D2-15340
pub fn triggers(
    anims: &dyn Animations,
    id: DataId,
    state: &GraphState<'_>,
    time: f32,
    wrap: bool,
    g: &mut Gathered,
) -> bool {
    triggers_at(anims, id, state, time, wrap, g, 0)
}

fn frac(t: f32) -> f32 {
    if t > 1.0 {
        t - t.trunc()
    } else {
        t
    }
}

fn triggers_at(
    anims: &dyn Animations,
    id: DataId,
    state: &GraphState<'_>,
    time: f32,
    wrap: bool,
    g: &mut Gathered,
    depth: usize,
) -> bool {
    if depth > MAX_DEPTH {
        return false;
    }
    let Some(a) = anims.animation(id) else {
        return false;
    };
    match &a.animator {
        // D2-14881
        AnimationKind::Keyframes(k) => {
            if let Some(ev) = &k.events {
                let t = if wrap { frac(time) } else { time };
                for e in ev.effects.iter().filter(|e| e.time <= t) {
                    g.add_effect(e);
                }
                for h in ev.hooks.iter().filter(|h| h.time <= t) {
                    g.add_hook(h);
                }
                for i in ev.impulses.iter().filter(|i| i.time <= t) {
                    g.add_impulse(i);
                }
            }
            true
        }
        AnimationKind::Body { animation } => {
            triggers_at(anims, *animation, state, time, wrap, g, depth + 1)
        }
        AnimationKind::Switch {
            switch_kind,
            default,
            overrides,
        } => {
            let c = switch_child(*switch_kind, *default, overrides, state);
            anims.animation(c).is_some() && triggers_at(anims, c, state, time, wrap, g, depth + 1)
        }
        AnimationKind::Blend {
            sliders,
            animations,
        } => {
            let w = cube_weights(sliders, animations.len(), state);
            let kids: Vec<(DataId, f32)> = animations
                .iter()
                .zip(&w)
                .filter(|(_, &w)| w >= MIN_WEIGHT)
                .map(|(c, &w)| (*c, w))
                .collect();
            blend_triggers(anims, &kids, state, frac(time), g, depth, true);
            true
        }
        AnimationKind::AxisCube {
            origin,
            animations,
            sliders,
        } => {
            let (w, o) = axis_weights(sliders, animations.len(), state);
            let mut kids = Vec::new();
            if o != 0.0 {
                kids.push((*origin, o));
            }
            kids.extend(
                animations
                    .iter()
                    .zip(&w)
                    .filter(|(_, &w)| w != 0.0)
                    .map(|(c, &w)| (*c, w)),
            );
            blend_triggers(anims, &kids, state, frac(time), g, depth, false);
            true
        }
        AnimationKind::Random {
            slider, animations, ..
        } => {
            let (a, b, f) = interp_pick(animations, &read_interp(state, *slider));
            if a.is_none() {
                return true;
            }
            let mut kids = vec![(a, 1.0 - f)];
            if !b.is_none() {
                kids.push((b, f));
            }
            blend_triggers(anims, &kids, state, frac(time), g, depth, false);
            true
        }
        AnimationKind::Look | AnimationKind::Angle { .. } => true,
    }
}

/// Merge children's whole-cycle effects (and, with `hooks`, their hooks) by weight into `g`,
/// keeping those due before `time`.
fn blend_triggers(
    anims: &dyn Animations,
    kids: &[(DataId, f32)],
    state: &GraphState<'_>,
    time: f32,
    g: &mut Gathered,
    depth: usize,
    hooks: bool,
) {
    let mut merged: Table<Trigger> = Table::default();
    let mut merged_hooks: Table<GameHook> = Table::default();
    for &(c, w) in kids {
        if anims.animation(c).is_none() {
            continue;
        }
        triggers_at(anims, c, state, 1.0, false, g, depth + 1);
        for e in g.effects.drain(..) {
            if let Some(m) = merged.get_mut(e.effect) {
                m.time += w * e.time;
                m.intensity += w * e.intensity;
                m.intensity_variance += w * e.intensity_variance;
                m.probability += w * e.probability;
            } else {
                let mut t = trigger(e.effect, w * e.time, w * e.intensity, true);
                t.intensity_variance = w * e.intensity_variance;
                t.probability = w * e.probability;
                merged.add(e.effect, t);
            }
        }
        if hooks {
            for h in g.hooks.drain(..) {
                if let Some(m) = merged_hooks.get_mut(h.data) {
                    m.time += w * h.time;
                } else {
                    merged_hooks.add(
                        h.data,
                        GameHook {
                            time: w * h.time,
                            ..h
                        },
                    );
                }
            }
        }
    }
    for (_, m) in merged.iter() {
        if m.time < time {
            g.effects.push(*m);
            g.started.insert(m.effect);
        }
    }
    for (_, h) in merged_hooks.iter() {
        if h.time < time {
            g.hooks.push(*h);
        }
    }
}

/// The hold loop of animation `id`: a clip's own when it has one, a pass-through node's its
/// child's; other nodes have none.
// D2-15114, D2-15239, D2-14835, D2-15395
#[must_use]
pub fn hold_loop(anims: &dyn Animations, id: DataId, state: &GraphState<'_>) -> Option<(f32, f32)> {
    let a = anims.animation(id)?;
    match &a.animator {
        AnimationKind::Keyframes(k) => k
            .events
            .as_ref()
            .filter(|e| e.has_hold_loop)
            .map(|e| (e.loop_start, e.loop_end)),
        AnimationKind::Switch {
            switch_kind,
            default,
            overrides,
        } => {
            let c = switch_child(*switch_kind, *default, overrides, state);
            hold_loop(anims, c, state)
        }
        _ => None,
    }
}

/// The animations a node names directly.
// D2-15110, D2-15360, D2-15383, D2-15401, D2-15416, D2-14010
#[must_use]
pub fn sub_ids(a: &Animation) -> Vec<DataId> {
    match &a.animator {
        AnimationKind::Keyframes(_) | AnimationKind::Look | AnimationKind::Angle { .. } => {
            Vec::new()
        }
        AnimationKind::Blend { animations, .. } | AnimationKind::Random { animations, .. } => {
            animations.clone()
        }
        AnimationKind::AxisCube {
            origin, animations, ..
        } => std::iter::once(*origin)
            .chain(animations.iter().copied())
            .collect(),
        AnimationKind::Body { animation } => vec![*animation],
        AnimationKind::Switch {
            default, overrides, ..
        } => std::iter::once(*default)
            .chain(overrides.iter().map(|(_, id)| *id))
            .collect(),
    }
}

/// Every animation under a node, depth first, each named before its own.
// D2-15111, D2-15304, D2-15330, D2-15386, D2-15402, D2-15426, D2-15369
pub fn recursive_sub_ids(anims: &dyn Animations, id: DataId, out: &mut Vec<DataId>) {
    recursive_at(anims, id, out, 0);
}

fn recursive_at(anims: &dyn Animations, id: DataId, out: &mut Vec<DataId>, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    let Some(a) = anims.animation(id) else {
        return;
    };
    for c in sub_ids(&a) {
        out.push(c);
        recursive_at(anims, c, out, depth + 1);
    }
}

/// Whether every clip under animation `id` has `bones` bones (a missing child is no fault).
// D2-15120, D2-15291, D2-15297, D2-15321, D2-15384, D2-15404, D2-15417, D2-15362, D2-15338
#[must_use]
pub fn bone_count_matches(anims: &dyn Animations, id: DataId, bones: usize) -> bool {
    let Some(a) = anims.animation(id) else {
        return true;
    };
    match &a.animator {
        AnimationKind::Keyframes(k) => k.bones.len() == bones,
        _ => sub_ids(&a)
            .into_iter()
            .all(|c| bone_count_matches(anims, c, bones)),
    }
}

/// The record kind of an animation.
pub const ANIMATION_RECORD: u32 = 0x4000_0009;
