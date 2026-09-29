// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/BaseDamageMod.cs
//! Port of `Source/ACE.Server/Entity/BaseDamageMod.cs`.

#![allow(clippy::cast_possible_truncation)] // C#'s `(float)` of a double, as ACE writes it

use empyrean_entity::enums::PropertyFloat;
use empyrean_entity::ObjectGuid;

use crate::entity::base_damage::BaseDamage;
use crate::entity::range::Range;
use crate::world_objects::managers::enchantment_manager_with_caching as emc;
use crate::world_objects::world_object_weapon::is_enchantable;
use crate::World;

/// A weapon's base damage with its enchantment modifiers.
// ACE: BaseDamageMod
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BaseDamageMod {
    // ACE: BaseDamageMod.BaseDamage
    pub base_damage: BaseDamage,

    /// blood drinker
    // ACE: BaseDamageMod.DamageBonus
    pub damage_bonus: f32,
    /// for missile launchers (+113% yumis = 2.13)
    // ACE: BaseDamageMod.DamageMod
    pub damage_mod: f32,
    // ACE: BaseDamageMod.VarianceMod
    pub variance_mod: f32,

    // ACE: BaseDamageMod.ElementalBonus
    pub elemental_bonus: i32,
}

impl BaseDamageMod {
    // ACE: BaseDamageMod.BaseDamageMod
    /// `new BaseDamageMod(BaseDamage)`.
    #[must_use]
    pub fn new(base_damage: BaseDamage) -> Self {
        Self {
            base_damage,
            damage_bonus: 0.0,
            damage_mod: 1.0,
            variance_mod: 1.0,
            elemental_bonus: 0,
        }
    }

    // ACE: BaseDamageMod.BaseDamageMod
    /// `new BaseDamageMod(BaseDamage, Creature wielder, WorldObject weapon)`: the weapon's
    /// enchantments, and the wielder's auras for an enchantable weapon.
    pub fn with_wielder(
        w: &mut World,
        base_damage: BaseDamage,
        wielder: ObjectGuid,
        weapon: Option<ObjectGuid>,
    ) -> Self {
        let mut this = Self::new(base_damage);

        let Some(weapon) = weapon else { return this };

        this.damage_bonus += emc::get_damage_bonus(w, weapon) as f32;
        this.variance_mod *= emc::get_variance_mod(w, weapon);

        let weapon_damage_mod = w
            .objects
            .get(weapon)
            .and_then(|o| o.get_property(PropertyFloat::DamageMod))
            .unwrap_or(1.0) as f32;
        this.damage_mod = weapon_damage_mod + emc::get_damage_mod(w, weapon);

        if w.objects.get(weapon).is_some_and(is_enchantable) {
            // factor in wielder auras for enchantable weapons
            this.damage_bonus += emc::get_damage_bonus(w, wielder) as f32;
            this.variance_mod *= emc::get_variance_mod(w, wielder);

            this.damage_mod += emc::get_damage_mod(w, wielder);
        }

        this
    }

    // ACE: BaseDamageMod.MaxDamage
    /// `(MaxDamage + DamageBonus + ElementalBonus) * DamageMod`, not crossing zero.
    #[must_use]
    pub fn max_damage(&self) -> f32 {
        let mut max_damage =
            (self.base_damage.max_damage as f32 + self.damage_bonus + self.elemental_bonus as f32)
                * self.damage_mod;

        if self.base_damage.max_damage >= 0 {
            max_damage = empyrean_common::dotnet::math::max_f32(0.0, max_damage);
        } else {
            max_damage = empyrean_common::dotnet::math::min_f32(0.0, max_damage);
        }

        max_damage
    }

    // ACE: BaseDamageMod.MinDamage
    /// `MaxDamage * (1.0f - Variance * VarianceMod)`.
    #[must_use]
    pub fn min_damage(&self) -> f32 {
        self.max_damage() * (1.0 - self.base_damage.variance * self.variance_mod)
    }

    // ACE: BaseDamageMod.Range
    #[must_use]
    pub fn range(&self) -> Range {
        Range::new(self.min_damage(), self.max_damage())
    }
}
