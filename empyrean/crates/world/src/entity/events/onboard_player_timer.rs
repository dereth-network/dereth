// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Events/OnboardPlayerTimer.cs
//! Port of `Source/ACE.Server/Entity/Events/OnboardPlayerTimer.cs`.

use empyrean_entity::ObjectGuid;

// ACE: OnboardPlayerTimer
/// A timer carrying a player and the objects to onboard (unused in ACE).
// DIVERGE: ACE derives from `System.Timers.Timer` (a thread-pool timer); the port has no timers of its own, so this keeps the data and the interval only.
#[derive(Debug, Clone, PartialEq)]
pub struct OnboardPlayerTimer {
    // ACE: OnboardPlayerTimer.Player
    pub player: ObjectGuid,
    // ACE: OnboardPlayerTimer.Objects
    pub objects: Vec<ObjectGuid>,
    /// `Timer.Interval` in milliseconds (the `Timer()` default is 100).
    pub interval: f64,
}

impl OnboardPlayerTimer {
    // ACE: OnboardPlayerTimer.OnboardPlayerTimer
    #[must_use]
    pub fn new(player: ObjectGuid, objects: Vec<ObjectGuid>) -> Self {
        Self {
            player,
            objects,
            interval: 100.0,
        }
    }

    // ACE: OnboardPlayerTimer.OnboardPlayerTimer
    #[must_use]
    pub fn with_interval(player: ObjectGuid, objects: Vec<ObjectGuid>, interval: f64) -> Self {
        Self {
            player,
            objects,
            interval,
        }
    }
}
