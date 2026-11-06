//! What the trackers ask of the object they animate.
//!
//! The behavior and mode trackers do not draw anything or own the body: they ask the object how
//! fast an animation cycles and where a hold loops, tell it when a behavior or a change of mode
//! that someone waits on is over, and pass on what behaviors do to the camera, the effects and the
//! object itself. [`crate::body::Body`] answers for a body put together from its records and
//! queues everything else as [`Event`]s for its owner.
//!
//! **Waiting on a behavior.** Whoever asks for a behavior (a script's latent call, a server
//! message) puts a non-zero context in its parameters; the body answers that context exactly once,
//! as a [`Completion`] ([`crate::body::Body::take_completions`]): when the behavior ends (an action
//! after ramping out, a hold after its hold cycles, or at the start of ramping out with the early
//! call back), when it is stopped or replaced on its layer ([`error::STOPPED`]), or at once when it
//! cannot play (already playing, unknown to the template, no combination, no animation). A behavior
//! that is neither an action nor a hold answers at once. Changes of mode and slider moves answer
//! the same way.

use auberean_assets::animation::BehaviorKey;
use auberean_world::chargen::Random;

use crate::triggers::{FrameTriggers, Gathered};

/// Why something someone waited on ended: 0 when it ran its course.
pub mod error {
    /// It ran its course.
    pub const NONE: u32 = 0;
    /// A newer request cancelled it.
    pub const CANCELLED: u32 = 0x36;
    /// The behavior's combination is not in the table.
    pub const NO_COMBO: u32 = 0x54;
    /// Another behavior holds the layer.
    pub const LAYER_TAKEN: u32 = 0x55;
    /// The behavior is already playing.
    pub const ALREADY_PLAYING: u32 = 0x56;
    /// The layer has no behavior.
    pub const LAYER_EMPTY: u32 = 0x57;
    /// The behavior is not playing.
    pub const NOT_PLAYING: u32 = 0x58;
    /// The character template does not know the behavior.
    pub const UNKNOWN_BEHAVIOR: u32 = 0x59;
    /// No animation plays it (or no way leads to the mode).
    pub const NO_ANIMATION: u32 = 0x5A;
    /// It was stopped, or the mode changed under it.
    pub const STOPPED: u32 = 0x5B;
}

/// What a behavior asks of the object beyond its own animation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// A behavior someone waited on is over.
    BehaviorDone {
        context: u32,
        error: u32,
    },
    /// A behavior someone waited on is over, heard through the path that let a hold go.
    ReleaseDone {
        context: u32,
        error: u32,
    },
    /// A change of mode someone waited on is over.
    ModeDone {
        context: u32,
        error: u32,
    },
    /// A slider move someone waited on was taken.
    SliderDone {
        context: u32,
    },
    /// Start an effect.
    DoFx {
        fx: u32,
        intensity: f32,
    },
    /// Stop an effect.
    StopFx {
        fx: u32,
    },
    /// Fade the object's children to an opacity over a time.
    ChildrenOpacity {
        opacity: f32,
        time: f32,
    },
    /// The object is to be destroyed.
    Remove,
    /// Let go of the object an effect aimed at.
    ClearFxTarget,
    LockInput,
    UnlockInput,
    CameraBack,
    /// The camera rides the animation (`time` -1: until let go).
    CameraAnimParent {
        time: f32,
        ramp: f32,
    },
    /// The camera looks along the animation.
    CameraAnimTarget {
        time: f32,
        ramp: f32,
    },
    /// The camera turns to an object as the object plays `behavior`, if it can; `words` are the
    /// two the client passes along (`0x40000002`, `0x40000001`), `ramp` half a second.
    TargetedCamera {
        target: u64,
        behavior: u32,
        words: (u32, u32),
        time: f32,
        ramp: f32,
    },
}

/// What a waiter was waiting on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Waited {
    Behavior,
    /// A behavior heard through the path that let a hold go.
    Release,
    Mode,
    Slider,
}

/// A wait answered: the waiter's context, why it ended (0: it ran its course; see [`error`]),
/// and what it waited on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Completion {
    pub context: u32,
    pub error: u32,
    pub waited: Waited,
}

impl Event {
    /// The wait this event answers, if it answers one.
    #[must_use]
    pub fn completion(&self) -> Option<Completion> {
        let (context, error, waited) = match *self {
            Event::BehaviorDone { context, error } => (context, error, Waited::Behavior),
            Event::ReleaseDone { context, error } => (context, error, Waited::Release),
            Event::ModeDone { context, error } => (context, error, Waited::Mode),
            Event::SliderDone { context } => (context, 0, Waited::Slider),
            _ => return None,
        };
        Some(Completion {
            context,
            error,
            waited,
        })
    }
}

/// The object the trackers animate.
pub trait Host {
    /// How many cycles a second the animation of a combination plays: 0 when none does.
    fn combo_cycle_velocity(&mut self, key: &BehaviorKey) -> f32;
    /// The hold loop of a combination's animation, in cycles.
    fn hold_loop(&mut self, key: &BehaviorKey) -> Option<(f32, f32)>;
    /// A behavior's flags from the character template, when it knows the behavior.
    fn behavior_flags(&self, behavior: u32) -> Option<u64>;
    /// Whether the object is the player's own.
    fn is_the_player(&self) -> bool;
    /// The effects animations set off, when the object keeps them.
    fn triggers(&mut self) -> Option<&mut FrameTriggers>;
    /// Gather what an effect script sets off by `time` (in cycles) into `g`.
    fn script_triggers(&mut self, script: u32, time: f32, g: &mut Gathered);
    /// The session's dice.
    fn dice(&mut self) -> &mut Random;
    /// The session's dice and the trigger update, borrowed together.
    fn dice_and_triggers(&mut self) -> (&mut Random, Option<&mut FrameTriggers>);
    /// Something for the object's owner.
    fn event(&mut self, e: Event);
}
