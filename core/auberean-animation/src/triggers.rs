//! What animations set off besides moving bones: effects started and stopped at moments of a
//! cycle, hooks the game logic hears, and impulses that push the body.
//!
//! A playing behavior gathers, as its time passes each moment, the effects, hooks and impulses of
//! its animation that are due. Once a frame the body's trigger update collects them from every
//! playing behavior: an effect not yet running is rolled against its probability and its intensity
//! varied, and starts; a running one has its time and intensity refreshed; a behavior that ends, or
//! ramps down, stops every effect it started. A hook or impulse fires once. The body's owner plays
//! what the update holds and then clears it ([`FrameTriggers::clean_up`]): what started is running
//! from then on, and what stopped is gone.

use std::collections::BTreeSet;

use auberean_assets::animation::{EffectTrigger, GameHook, Impulse};
use auberean_world::chargen::Random;

use crate::dice;
use crate::table::Table;

/// An effect trigger: the effect, when, whether it stops the effect, its intensity, how much that
/// varies, how likely it is to play, and whether it plays.
pub type Trigger = EffectTrigger;

/// A trigger for `effect` at `time` with every other field at its default: full intensity, no
/// variation, always played.
// D2-15218, D2-15219, D2-15220, D2-15221
#[must_use]
pub fn trigger(effect: u32, time: f32, intensity: f32, play: bool) -> Trigger {
    Trigger {
        effect,
        time,
        stop: false,
        probability: 1.0,
        intensity,
        intensity_variance: 0.0,
        play,
    }
}

/// Two effect triggers are the same when their effect, time and probability are.
// D2-15216
#[must_use]
pub fn same_trigger(a: &Trigger, b: &Trigger) -> bool {
    a.effect == b.effect && a.time == b.time && a.probability == b.probability
}

/// Triggers order by their time.
// D2-15217, D2-15230, D2-15149
#[must_use]
pub fn earlier(a: f32, b: f32) -> bool {
    a < b
}

/// Two hooks are the same when their kind, data and time are.
// D2-15229
#[must_use]
pub fn same_hook(a: &GameHook, b: &GameHook) -> bool {
    a.kind == b.kind && a.data == b.data && a.time == b.time
}

/// A hook's default: kind, number and data none, not fired, at the end of the cycle.
// D2-14837, D2-15231
#[must_use]
pub fn hook(kind: u32, number: u32, data: u32, time: f32) -> GameHook {
    GameHook {
        kind,
        number,
        data,
        triggered: false,
        time,
    }
}

/// An impulse trigger as a playing behavior holds it: the stored trigger and whether it fired.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImpulseTrigger {
    // D2-05153
    pub impulse: Impulse,
    pub triggered: bool,
}

impl Default for ImpulseTrigger {
    // D2-05152
    fn default() -> Self {
        Self {
            impulse: Impulse {
                jump: false,
                impulse: None,
                number: 0,
                time: 1.0,
            },
            triggered: false,
        }
    }
}

/// Two impulses are the same when both are jumps or both are not, their vectors are within
/// 0.0002 on every axis, and their times are equal.
// D2-15148
#[must_use]
pub fn same_impulse(a: &Impulse, b: &Impulse) -> bool {
    let v = |i: &Impulse| i.impulse.unwrap_or_default();
    let (va, vb) = (v(a), v(b));
    a.jump == b.jump
        && (va.x - vb.x).abs() < 0.0002
        && (va.y - vb.y).abs() < 0.0002
        && (va.z - vb.z).abs() < 0.0002
        && a.time == b.time
}

/// What one playing animation has gathered: its due effects, hooks and impulses, and the effects
/// it started.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Gathered {
    pub effects: Vec<Trigger>,
    pub hooks: Vec<GameHook>,
    pub impulses: Vec<ImpulseTrigger>,
    /// The effects this animation started, by id.
    pub started: BTreeSet<u32>,
}

impl Gathered {
    /// Add an effect trigger unless the same one is there.
    pub fn add_effect(&mut self, t: &Trigger) {
        if !self.effects.iter().any(|e| same_trigger(e, t)) {
            self.effects.push(*t);
        }
    }

    /// Add a hook unless the same one is there.
    pub fn add_hook(&mut self, h: &GameHook) {
        if !self.hooks.iter().any(|e| same_hook(e, h)) {
            self.hooks.push(*h);
        }
    }

    /// Add an impulse unless the same one is there.
    pub fn add_impulse(&mut self, i: &Impulse) {
        if !self.impulses.iter().any(|e| same_impulse(&e.impulse, i)) {
            self.impulses.push(ImpulseTrigger {
                impulse: *i,
                triggered: false,
            });
        }
    }
}

/// The effects, hooks and impulses a body's animations set off this frame.
// D2-06349, D2-06351
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameTriggers {
    /// Effects starting this frame.
    pub starting: Table<Trigger>,
    /// Effects running.
    pub active: Table<Trigger>,
    /// Effects stopping this frame.
    pub stopping: Table<Trigger>,
    /// Effects played once, started by behaviors and changes of mode.
    pub once: Table<Trigger>,
    /// Hooks due.
    pub hooks: Vec<GameHook>,
    /// Impulses due.
    pub impulses: Vec<Impulse>,
}

impl FrameTriggers {
    /// Nothing to play.
    // D2-15209
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.starting.is_empty()
            && self.active.is_empty()
            && self.stopping.is_empty()
            && self.once.is_empty()
            && self.hooks.is_empty()
            && self.impulses.is_empty()
    }

    /// Queue an animation's hooks that have not fired. A hook is marked fired when queued: the
    /// owner plays the queue every frame it is filled.
    // D2-15210
    pub fn add_hooks(&mut self, g: &mut Gathered) {
        for h in &mut g.hooks {
            if !h.triggered {
                self.hooks.push(*h);
                h.triggered = true;
            }
        }
    }

    /// Queue an animation's impulses that have not fired, marking them fired as hooks are.
    // D2-15211
    pub fn add_impulses(&mut self, g: &mut Gathered) {
        for i in &mut g.impulses {
            if !i.triggered {
                self.impulses.push(i.impulse);
                i.triggered = true;
            }
        }
    }

    /// An animation ended: every effect it started is stopped (one not running is only taken out
    /// of the starting ones), and it no longer counts as having looped.
    // D2-15212
    pub fn on_track_terminated(&mut self, g: &Gathered, looped: &mut bool) {
        for &name in &g.started {
            self.starting.remove(name);
            if self.active.remove(name).is_some() {
                self.stopping.add(name, trigger(name, 0.0, 0.0, true));
            }
        }
        *looped = false;
    }

    /// The frame's effects were played: the starting ones are running, and nothing is starting,
    /// stopping or due.
    // D2-15213
    pub fn clean_up(&mut self) {
        let starting: Vec<(u32, Trigger)> = self.starting.iter().map(|(k, t)| (k, *t)).collect();
        for (k, t) in starting {
            self.active.add(k, t);
        }
        self.starting.clear();
        self.stopping.clear();
        self.impulses.clear();
        self.hooks.clear();
    }

    /// Start, or refresh, an animation's due effects; an animation that is ramping down, or that
    /// has looped this frame, stops what it started instead.
    ///
    /// For each due effect: a running one takes the trigger's time and intensity; otherwise the
    /// effect starts with probability its trigger's (rolled unless it is 1) and an intensity varied
    /// by up to half its variance either way, and is remembered as started by this animation.
    // D2-15214
    pub fn add_effects(
        &mut self,
        g: &mut Gathered,
        ramping_down: bool,
        looped: &mut bool,
        dice: &mut Random,
    ) {
        if ramping_down || *looped {
            self.on_track_terminated(g, looped);
            return;
        }
        for e in &g.effects {
            if let Some(a) = self.active.get_mut(e.effect) {
                a.time = e.time;
                a.intensity = e.intensity;
                continue;
            }
            let play = e.probability == 1.0 || dice::roll_float(dice, 0.0, 1.0) <= e.probability;
            let r = dice::roll_float(dice, 0.0, 1.0);
            let v = (r - 0.5) * e.intensity_variance;
            self.starting.add(
                e.effect,
                trigger(e.effect, e.time, v + v + e.intensity, play),
            );
            g.started.insert(e.effect);
        }
    }

    /// Gather an animation's effects, hooks and impulses into the update.
    // D2-15215
    pub fn add_track_triggers(
        &mut self,
        g: &mut Gathered,
        ramping_down: bool,
        looped: &mut bool,
        dice: &mut Random,
    ) {
        if !g.effects.is_empty() {
            self.add_effects(g, ramping_down, looped, dice);
        }
        if !g.hooks.is_empty() {
            self.add_hooks(g);
        }
        if !g.impulses.is_empty() {
            self.add_impulses(g);
        }
    }
}

/// Gather what an effect script sets off by `time`: its effect triggers and impulses at or
/// before then that the animation does not have yet.
// D2-15126
pub fn script_triggers(script: &auberean_assets::fx::FxScript, time: f32, g: &mut Gathered) {
    for t in script.triggers.iter().filter(|t| t.time <= time) {
        g.add_effect(&Trigger {
            effect: t.name,
            time: t.time,
            stop: t.stop,
            probability: t.probability,
            intensity: t.intensity,
            intensity_variance: t.intensity_variance,
            play: t.play,
        });
    }
    for i in script.impulses.iter().filter(|i| i.time <= time) {
        g.add_impulse(&Impulse {
            jump: i.jump,
            impulse: i.impulse,
            number: i.number,
            time: i.time,
        });
    }
}
