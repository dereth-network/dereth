//! A behavior being played: how it was asked for, where it is in its cycle, how much it weighs,
//! and what it has set off.
//!
//! A behavior's time is in cycles of its animation: it advances by its speed (the asked time
//! scale times the cycles a second its animation plays at) each frame. An idle loops forever; an
//! action plays once, ramping its weight in over its ramp time at the start and out over the same
//! time at the end; a hold plays its hold loop over and over until let go, then finishes as an
//! action. A behavior whose weight reaches zero is over.

use auberean_primitives::Vec3;

use crate::triggers::Gathered;

/// How a behavior is asked for: which behavior, how fast, how many times a hold repeats, the
/// effects and target that go with it, and flags for the camera and the game logic. Fields left
/// alone keep the client's defaults: full speed, one hold cycle, events sent, no target.
#[derive(Clone, Debug, PartialEq)]
pub struct Params {
    // D2-04530, D2-05685, D2-00251, D2-00252
    pub behavior: u32,
    pub time_scale: f32,
    pub mode: u32,
    pub hold_cycles: u32,
    pub fx_script: u32,
    pub fx: u32,
    pub target: u64,
    pub camera_target: u64,
    pub camera_behavior: u32,
    pub clone_visual: u32,
    pub clone_appearance: u32,
    pub clone_strengths: Vec<(u32, f32)>,
    pub impulse: Vec3,
    pub early_callback: bool,
    pub move_to_cancels: bool,
    pub camera_parent: bool,
    pub camera_targets: bool,
    pub camera_hold: bool,
    pub camera_restore: bool,
    pub fade_children: bool,
    pub propagate: bool,
    pub lock_actions: bool,
    pub send_event: bool,
    pub emote: u32,
    pub leave_idle_alone: bool,
    pub destroy_on_completion: bool,
    /// Who hears when the behavior ends (0: nobody).
    pub context: u32,
}

impl Default for Params {
    // D2-14912, D2-14913, D2-14914
    fn default() -> Self {
        Self {
            behavior: 0,
            time_scale: 1.0,
            mode: 0,
            hold_cycles: 1,
            fx_script: 0,
            fx: 0,
            target: 0,
            camera_target: 0,
            camera_behavior: 0,
            clone_visual: 0,
            clone_appearance: 0,
            clone_strengths: Vec::new(),
            impulse: Vec3::ZERO,
            early_callback: false,
            move_to_cancels: false,
            camera_parent: false,
            camera_targets: false,
            camera_hold: false,
            camera_restore: false,
            fade_children: false,
            propagate: false,
            lock_actions: false,
            send_event: true,
            emote: 0,
            leave_idle_alone: false,
            destroy_on_completion: false,
            context: 0,
        }
    }
}

/// The flag words of the parameters as sent: a bit per field that differs from its default.
pub mod bits {
    pub const BEHAVIOR: u32 = 0x1;
    pub const MODE: u32 = 0x2;
    pub const TIME_SCALE: u32 = 0x4;
    pub const EARLY_CALLBACK: u32 = 0x8;
    pub const HOLD_CYCLES: u32 = 0x10;
    pub const MOVE_TO_CANCELS: u32 = 0x20;
    pub const CAMERA_TARGETS: u32 = 0x40;
    pub const CAMERA_PARENT: u32 = 0x80;
    pub const FADE_CHILDREN: u32 = 0x100;
    pub const SEND_EVENT: u32 = 0x200;
    pub const PROPAGATE: u32 = 0x400;
    pub const LOCK_ACTIONS: u32 = 0x800;
    pub const FX_SCRIPT: u32 = 0x1000;
    pub const TARGET: u32 = 0x2000;
    pub const IMPULSE: u32 = 0x4000;
    pub const DESTROY_ON_COMPLETION: u32 = 0x8000;
    pub const CLONE: u32 = 0x1_0000;
    pub const LEAVE_IDLE_ALONE: u32 = 0x2_0000;
    pub const CAMERA_HOLD: u32 = 0x4_0000;
    pub const CAMERA_RESTORE: u32 = 0x8_0000;
    pub const FX: u32 = 0x10_0000;
    pub const CAMERA: u32 = 0x20_0000;
    pub const CONTEXT: u32 = 0x40_0000;
    pub const EMOTE: u32 = 0x80_0000;
    /// The single-word fields in the order they are sent after the others.
    pub const WORDS: [u32; 13] = [
        EARLY_CALLBACK,
        MOVE_TO_CANCELS,
        CAMERA_PARENT,
        CAMERA_TARGETS,
        CAMERA_HOLD,
        CAMERA_RESTORE,
        FADE_CHILDREN,
        PROPAGATE,
        LOCK_ACTIONS,
        SEND_EVENT,
        LEAVE_IDLE_ALONE,
        DESTROY_ON_COMPLETION,
        EMOTE,
    ];
}

impl Params {
    /// Play one behavior, everything else at its default.
    // D2-14914
    #[must_use]
    pub fn behavior(behavior: u32) -> Self {
        Self {
            behavior,
            ..Self::default()
        }
    }

    /// Whether a behavior is asked for at all.
    // D2-14909
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.behavior != 0
    }

    /// The flag word to send these parameters with: a bit per field that differs from its
    /// default (the impulse when an axis differs by 0.0002 or more).
    // D2-14910
    #[must_use]
    pub fn pack_bits(&self) -> u32 {
        let d = Self::default();
        let mut b = 0;
        let mut set = |on: bool, bit: u32| {
            if on {
                b |= bit;
            }
        };
        set(self.behavior != d.behavior, bits::BEHAVIOR);
        set(self.mode != d.mode, bits::MODE);
        set(self.time_scale != d.time_scale, bits::TIME_SCALE);
        set(
            self.early_callback != d.early_callback,
            bits::EARLY_CALLBACK,
        );
        set(self.hold_cycles != d.hold_cycles, bits::HOLD_CYCLES);
        set(
            self.move_to_cancels != d.move_to_cancels,
            bits::MOVE_TO_CANCELS,
        );
        set(
            self.camera_targets != d.camera_targets,
            bits::CAMERA_TARGETS,
        );
        set(self.camera_parent != d.camera_parent, bits::CAMERA_PARENT);
        set(self.camera_hold != d.camera_hold, bits::CAMERA_HOLD);
        set(
            self.camera_restore != d.camera_restore,
            bits::CAMERA_RESTORE,
        );
        set(self.camera_target != d.camera_target, bits::CAMERA);
        set(self.fade_children != d.fade_children, bits::FADE_CHILDREN);
        set(self.propagate != d.propagate, bits::PROPAGATE);
        set(self.lock_actions != d.lock_actions, bits::LOCK_ACTIONS);
        set(self.fx_script != d.fx_script, bits::FX_SCRIPT);
        set(self.fx != d.fx, bits::FX);
        set(self.send_event != d.send_event, bits::SEND_EVENT);
        set(self.target != d.target, bits::TARGET);
        set(self.clone_visual != d.clone_visual, bits::CLONE);
        set(
            (self.impulse.x - d.impulse.x).abs() >= 0.0002
                || (self.impulse.y - d.impulse.y).abs() >= 0.0002
                || (self.impulse.z - d.impulse.z).abs() >= 0.0002,
            bits::IMPULSE,
        );
        set(
            self.leave_idle_alone != d.leave_idle_alone,
            bits::LEAVE_IDLE_ALONE,
        );
        set(
            self.destroy_on_completion != d.destroy_on_completion,
            bits::DESTROY_ON_COMPLETION,
        );
        set(self.emote != d.emote, bits::EMOTE);
        set(self.context != d.context, bits::CONTEXT);
        b
    }

    /// Set the single-word fields from the words sent with flag word `flags`, in
    /// [`bits::WORDS`] order (a word whose bit is clear keeps its default).
    pub fn set_words(&mut self, flags: u32, words: &[u32; 13]) {
        for (bit, w) in bits::WORDS.iter().zip(words) {
            if flags & bit == 0 {
                continue;
            }
            let on = *w != 0;
            match *bit {
                bits::EARLY_CALLBACK => self.early_callback = on,
                bits::MOVE_TO_CANCELS => self.move_to_cancels = on,
                bits::CAMERA_PARENT => self.camera_parent = on,
                bits::CAMERA_TARGETS => self.camera_targets = on,
                bits::CAMERA_HOLD => self.camera_hold = on,
                bits::CAMERA_RESTORE => self.camera_restore = on,
                bits::FADE_CHILDREN => self.fade_children = on,
                bits::PROPAGATE => self.propagate = on,
                bits::LOCK_ACTIONS => self.lock_actions = on,
                bits::SEND_EVENT => self.send_event = on,
                bits::LEAVE_IDLE_ALONE => self.leave_idle_alone = on,
                bits::DESTROY_ON_COMPLETION => self.destroy_on_completion = on,
                _ => self.emote = *w,
            }
        }
    }
}

/// A behavior being played.
#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    // D2-14956, D2-14959, D2-14949, D2-14947
    /// Where it is, in cycles of its animation.
    pub time: f32,
    pub weight: f32,
    /// It passed the end of a cycle.
    pub looped: bool,
    /// What its animation has set off.
    pub gathered: Gathered,
    pub params: Params,
    /// How fast its weight changes, a second.
    pub weight_velocity: f32,
    /// How many cycles of its animation play a second.
    pub cycle_velocity: f32,
    pub is_action: bool,
    pub is_hold: bool,
    pub dynamic_fp_camera: bool,
    /// It was a hold and was let go.
    pub was_hold: bool,
    /// Who hears of its end hears through the let-go path.
    pub callback_with_release: bool,
    pub ramp_time: f32,
    pub layer: u32,
    pub anim_key: u32,
    pub ramping_down: bool,
    pub stopped_action: bool,
    /// It began ramping down this frame.
    pub started_ramping_down: bool,
    /// The effects it started were stopped.
    pub terminated_fx: bool,
    /// A hold's loop, in cycles.
    pub loop_start: f32,
    pub loop_end: f32,
    /// How many times a hold has looped.
    pub repeats: u32,
}

impl Default for Track {
    // D2-14950, D2-14953, D2-14967, D2-05150
    fn default() -> Self {
        Self {
            time: 0.0,
            weight: 0.0,
            looped: false,
            gathered: Gathered::default(),
            params: Params::default(),
            weight_velocity: 0.0,
            cycle_velocity: 0.0,
            is_action: false,
            is_hold: false,
            dynamic_fp_camera: false,
            was_hold: false,
            callback_with_release: false,
            ramp_time: 0.0,
            layer: 0,
            anim_key: 0,
            ramping_down: false,
            stopped_action: false,
            started_ramping_down: false,
            terminated_fx: false,
            loop_start: 0.0,
            loop_end: 0.0,
            repeats: 0,
        }
    }
}

impl Track {
    /// Whether it is ramping down.
    // D2-14954
    #[must_use]
    pub fn is_ramping_down(&self) -> bool {
        self.ramping_down
    }

    /// The behavior.
    #[must_use]
    pub fn behavior(&self) -> u32 {
        self.params.behavior
    }

    /// Advance `dt` seconds.
    ///
    /// The time moves by the time scale times the cycle speed; crossing a whole cycle marks it
    /// looped and, for an idle, forgets the effects gathered so they fire again; the time wraps
    /// back by 64 cycles past 64. The weight (held at 1 at most, its rise stopped there) moves by
    /// its velocity. An action that has not been stopped ramps in over its ramp time and out over
    /// the last ramp time of its period, marking itself ramping down. A hold with a loop jumps
    /// back from the loop's end to its start, counting repeats; one without a loop counts a
    /// repeat every frame.
    // D2-14965
    pub fn update(&mut self, dt: f32) {
        let before = self.time.floor();
        self.time += self.params.time_scale * self.cycle_velocity * dt;
        if self.time.floor() != before {
            self.looped = true;
            if !self.is_action && !self.is_hold {
                self.gathered.effects.clear();
            }
        }
        if self.time >= 64.0 {
            self.time -= 64.0;
        }
        if self.weight > 1.0 {
            self.weight = 1.0;
            if self.weight_velocity > 0.0 {
                self.weight_velocity = 0.0;
            }
        }
        self.weight += dt * self.weight_velocity;
        let period = if self.cycle_velocity == 0.0 {
            0.0
        } else {
            1.0 / self.cycle_velocity
        };
        let t = period * self.time;
        if self.is_action && !self.stopped_action {
            let ramp = self.ramp_time;
            if t < ramp {
                self.weight = if ramp == 0.0 { 0.0 } else { t / ramp };
            } else if !self.ramping_down && t > ramp && period - ramp > t {
                self.weight = 1.0;
            } else if period - ramp < t {
                self.weight = if ramp == 0.0 {
                    0.0
                } else {
                    (period - t) / ramp
                };
                if !self.ramping_down {
                    self.started_ramping_down = true;
                }
                self.ramping_down = true;
            }
        }
        if self.is_hold {
            if self.loop_end <= 0.0 || self.loop_end <= self.loop_start {
                self.repeats += 1;
            } else if self.loop_end < self.time {
                while self.loop_end < self.time {
                    self.repeats += 1;
                    self.looped = true;
                    self.time = self.time - self.loop_end + self.loop_start;
                }
            }
        }
    }

    /// Let go of a hold: it becomes an action that finishes from where it is, its time brought
    /// back inside the loop first. `false` when it is not a hold.
    // D2-14931
    pub fn release(&mut self) -> bool {
        if !self.is_hold {
            return false;
        }
        let mut t = self.time;
        self.is_action = true;
        self.is_hold = false;
        self.dynamic_fp_camera = false;
        self.was_hold = true;
        if self.loop_end != 0.0 && self.loop_end < t {
            while self.loop_end < t {
                t = t - self.loop_end + self.loop_start;
            }
        }
        self.time = t;
        true
    }
}
