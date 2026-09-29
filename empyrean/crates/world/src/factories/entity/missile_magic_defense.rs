// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Entity/MissileMagicDefense.cs
//! Port of `Source/ACE.Server/Factories/Entity/MissileMagicDefense.cs`.

use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_tables::entity::{ChanceTable, DotNetString};

/// A `float` chance-table result. empyrean-tables has no `ChanceTable<float>`, so the element is a
/// local newtype that prints as .NET prints a `float` (for `VerifyTable`'s log line).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Float(pub f32);

impl DotNetString for Float {
    fn dotnet_string(&self) -> String {
        empyrean_common::dotnet::to_string(self.0)
    }
}

// WeaponMissileDefense / WeaponMagicDefense

static T1_T6_DEFENSE: ChanceTable<Float> = ChanceTable::new(&[
    (Float(1.005), 0.10),
    (Float(1.010), 0.20),
    (Float(1.015), 0.40),
    (Float(1.020), 0.20),
    (Float(1.025), 0.10),
]);

static T7_T8_DEFENSE: ChanceTable<Float> = ChanceTable::new(&[
    (Float(1.005), 0.05),
    (Float(1.010), 0.10),
    (Float(1.015), 0.15),
    (Float(1.020), 0.20),
    (Float(1.025), 0.20),
    (Float(1.030), 0.15),
    (Float(1.035), 0.10),
    (Float(1.040), 0.05),
]);

/// A 10% chance of a defense bonus, then the tier's table.
// ACE: MissileMagicDefense.Roll
#[must_use]
pub fn roll(tier: i32) -> Option<f32> {
    // preliminary roll: 10% chance
    let rng = ThreadSafeRandom::next_float(0.0, 1.0);
    if rng >= f64::from(0.1f32) {
        return None;
    }

    if tier < 7 {
        Some(T1_T6_DEFENSE.roll(0.0).0)
    } else {
        Some(T7_T8_DEFENSE.roll(0.0).0)
    }
}
