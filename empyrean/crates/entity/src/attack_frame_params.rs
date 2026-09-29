// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/AttackFrameParams.cs
//! `AttackFrameParams`: the key of the attack-frame cache.

use std::hash::{Hash, Hasher};

use crate::enums::{MotionCommand, MotionStance};

/// ACE: AttackFrameParams. Equality is on all three fields; `Hash` feeds ACE's `GetHashCode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttackFrameParams {
    // ACE: AttackFrameParams.MotionTableId
    pub motion_table_id: u32,
    // ACE: AttackFrameParams.Stance
    pub stance: MotionStance,
    // ACE: AttackFrameParams.Motion
    pub motion: MotionCommand,
}

impl AttackFrameParams {
    // ACE: AttackFrameParams.AttackFrameParams
    #[must_use]
    pub fn new(motion_table_id: u32, stance: MotionStance, motion: MotionCommand) -> Self {
        AttackFrameParams {
            motion_table_id,
            stance,
            motion,
        }
    }

    // ACE: AttackFrameParams.Equals
    #[must_use]
    pub fn equals(&self, attack_frame_params: &AttackFrameParams) -> bool {
        self.motion_table_id == attack_frame_params.motion_table_id
            && self.stance == attack_frame_params.stance
            && self.motion == attack_frame_params.motion
    }

    /// `uint.GetHashCode()` and a `uint`-backed enum's `GetHashCode()` are the value as `int`;
    /// the arithmetic is unchecked.
    // ACE: AttackFrameParams.GetHashCode
    #[must_use]
    #[allow(clippy::cast_possible_wrap)]
    pub fn get_hash_code(&self) -> i32 {
        let mut hash: i32 = 0;
        hash = hash.wrapping_mul(397) ^ self.motion_table_id as i32;
        hash = hash.wrapping_mul(397) ^ self.stance.0 as i32;
        hash = hash.wrapping_mul(397) ^ self.motion.0 as i32;
        hash
    }
}

impl Hash for AttackFrameParams {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.get_hash_code().hash(state);
    }
}
