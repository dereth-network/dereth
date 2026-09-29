// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/AttackQueue.cs
//! Port of `Source/ACE.Server/Entity/AttackQueue.cs`.
//!
//! The power/accuracy bar levels a player clicked while an attack was already running: each click
//! adds one, and each attack fetches the next (the last one repeats).

use std::collections::VecDeque;

use empyrean_entity::ObjectGuid;

/// A player's queued power/accuracy levels (`Queue<float> PowerAccuracy`).
// ACE: AttackQueue
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AttackQueue {
    /// `Player` (a guid; only ACE's commented-out log reads it).
    pub player: ObjectGuid,
    /// `PowerAccuracy`.
    pub power_accuracy: VecDeque<f32>,
}

impl AttackQueue {
    // ACE: AttackQueue.AttackQueue
    #[must_use]
    pub fn new(player: ObjectGuid) -> Self {
        Self {
            player,
            power_accuracy: VecDeque::new(),
        }
    }

    // ACE: AttackQueue.Add
    pub fn add(&mut self, power_accuracy: f32) {
        self.power_accuracy.push_back(power_accuracy);
    }

    /// Drops the oldest level while another is queued behind it, then answers the front one (0.5
    /// for an empty queue).
    // ACE: AttackQueue.Fetch
    pub fn fetch(&mut self) -> f32 {
        if self.power_accuracy.len() > 1 {
            self.power_accuracy.pop_front();
        }

        let Some(&power_accuracy) = self.power_accuracy.front() else {
            //log.Error($"{Player.Name}.AttackQueue.Fetch() - empty queue");
            return 0.5;
        };
        power_accuracy
    }

    // ACE: AttackQueue.Clear
    pub fn clear(&mut self) {
        self.power_accuracy.clear();
    }
}
