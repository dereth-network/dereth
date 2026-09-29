// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/DamageType.cs

use empyrean_common::extensions::enum_helper::get_flags;
use empyrean_common::thread_safe_random::ThreadSafeRandom;

use crate::enums::{AceEnum, DamageType};

impl DamageType {
    // ACE: DamageTypeExtensions.GetName
    pub fn get_name(self) -> Option<&'static str> {
        match self {
            DamageType::Undef => Some("Undefined"),
            DamageType::Slash => Some("Slashing"),
            DamageType::Pierce => Some("Piercing"),
            DamageType::Bludgeon => Some("Bludgeoning"),
            DamageType::Cold => Some("Cold"),
            DamageType::Fire => Some("Fire"),
            DamageType::Acid => Some("Acid"),
            DamageType::Electric => Some("Electric"),
            DamageType::Health => Some("Health"),
            DamageType::Stamina => Some("Stamina"),
            DamageType::Mana => Some("Mana"),
            DamageType::Nether => Some("Nether"),
            DamageType::Base => Some("Base"),
            _ => None,
        }
    }

    // ACE: DamageTypeExtensions.IsMultiDamage
    pub fn is_multi_damage(self) -> bool {
        #[allow(clippy::cast_sign_loss)]
        has_multiple(self.0 as u32)
    }

    /// Picks one damage type out of this mask: ACE's `EnumHelper.GetFlags` list indexed by
    /// `ThreadSafeRandom.Next(1, n - 1)`. With a `power_level`, the physical types (below 0.33) or
    /// the others are kept first, falling back to the whole mask.
    ///
    /// # Panics
    /// When the list is too short for the draw (`ArgumentOutOfRangeException` in ACE), as for
    /// `Undef`.
    // ACE: DamageTypeExtensions.SelectDamageType
    // ACE-BUG: `EnumHelper.GetFlags` lists every declared member whose bits are all set, so the composite `Physical`/`Elemental` members are candidates too and `Next(1, Count - 1)` can return a multi-damage type.
    #[must_use]
    pub fn select_damage_type(self, power_level: Option<f32>) -> DamageType {
        let Some(power_level) = power_level else {
            // select random damage type
            let damage_types = get_flags(DamageType::ALL, |d| d.key(), self.key());

            let count = i32::try_from(damage_types.len()).unwrap_or(i32::MAX);
            let rng = ThreadSafeRandom::next(1, count - 1);

            return damage_types[usize::try_from(rng).expect("ACE: ArgumentOutOfRangeException")];
        };

        let mut player_types = if power_level < 0.33 {
            self & DamageType::Physical
        } else {
            self & !DamageType::Physical
        };

        if player_types == DamageType::Undef {
            player_types = self;
        }

        player_types.select_damage_type(None)
    }
}

/// ACE.Common's `EnumHelper.HasMultiple`, kept private here until `empyrean-common` exports it.
// ACE: EnumHelper.HasMultiple
fn has_multiple(enum_val: u32) -> bool {
    (enum_val & enum_val.wrapping_sub(1)) != 0
}
