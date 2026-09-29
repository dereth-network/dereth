// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/AttackType.cs

use crate::enums::AttackType;

impl AttackType {
    // ACE: AttackTypeExtensions.IsMultiStrike
    pub fn is_multi_strike(self) -> bool {
        (self & AttackType::MultiStrike).0 != 0
    }

    // ACE: AttackTypeExtensions.ReduceMultiStrike
    pub fn reduce_multi_strike(self) -> AttackType {
        if !self.is_multi_strike() {
            return AttackType::Undef;
        }

        match self {
            AttackType::DoubleThrust | AttackType::TripleThrust => AttackType::Thrust,

            AttackType::DoubleSlash | AttackType::TripleSlash => AttackType::Slash,

            AttackType::OffhandDoubleThrust | AttackType::OffhandTripleThrust => {
                AttackType::OffhandThrust
            }

            AttackType::OffhandDoubleSlash | AttackType::OffhandTripleSlash => {
                AttackType::OffhandSlash
            }

            _ => AttackType::Undef,
        }
    }
}
