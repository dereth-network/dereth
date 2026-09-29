// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Entity/CreatureVital.cs
//! Port of `Source/ACE.Server/WorldObjects/Entity/CreatureVital.cs`.
//!
//! ACE's `CreatureVital` wraps one `PropertiesAttribute2nd` record of its creature's biota, and
//! carries two runtime fields, `RegenRate` and `PartialRegen`. The live object is the value in
//! the creature's `Vitals` dictionary (`CreatureVitalsFields::vitals`); `Health`, `Stamina` and
//! `Mana` hand out copies. Members that read the biota take the creature explicitly, so a copy
//! reads the same record; writes to the runtime fields go through
//! [`WorldObject::vital_mut`](crate::world_objects::world_object::WorldObject::vital_mut).

use empyrean_common::dotnet::CsCast;
use empyrean_common::extensions::float_extensions;
use empyrean_entity::enums::{ModifierType, PropertyAttribute2nd, PropertyFloat, Vital};
use empyrean_entity::models::properties_attribute_2nd::PropertiesAttribute2nd;

use crate::entity::attribute_formula;
use crate::world_objects::entity::creature_attribute::{count_i32, em, uint_to_f32, StatCtx};
use crate::world_objects::player_properties::player_vitae;
use crate::world_objects::world_object::WorldObject;
use crate::World;

// ACE: CreatureVital
/// One vital (health, stamina or mana) of a creature.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CreatureVital {
    // ACE: CreatureVital.Vital
    pub vital: PropertyAttribute2nd,
    // ACE: CreatureVital.RegenRate
    pub regen_rate: f64,
    // ACE: CreatureVital.PartialRegen
    /// For tracking partial regeneration between ticks.
    pub partial_regen: f64,
}

impl CreatureVital {
    // ACE: CreatureVital.CreatureVital
    /// If the creature's biota does not contain this vital, a new record is created. The regen
    /// rate comes from the creature's `HealthRate`, `StaminaRate` or `ManaRate`.
    ///
    /// # Panics
    /// When the biota has no vital dictionary (ACE: `NullReferenceException`).
    pub fn new(creature: &mut WorldObject, vital: PropertyAttribute2nd) -> Self {
        let dict = creature
            .biota
            .properties_attribute_2nd
            .as_mut()
            .expect("NullReferenceException: Biota.PropertiesAttribute2nd");
        if !dict.contains_key(&vital) {
            dict.insert(vital, PropertiesAttribute2nd::default());
        }

        let mut regen_rate = 0.0;
        match vital {
            PropertyAttribute2nd::MaxHealth => {
                regen_rate = creature
                    .get_property(PropertyFloat::HealthRate)
                    .unwrap_or(0.0)
            }
            PropertyAttribute2nd::MaxStamina => {
                regen_rate = creature
                    .get_property(PropertyFloat::StaminaRate)
                    .unwrap_or(0.0)
            }
            PropertyAttribute2nd::MaxMana => {
                regen_rate = creature
                    .get_property(PropertyFloat::ManaRate)
                    .unwrap_or(0.0)
            }
            _ => {}
        }

        Self {
            vital,
            regen_rate,
            partial_regen: 0.0,
        }
    }

    fn record(self, creature: &WorldObject) -> &PropertiesAttribute2nd {
        creature
            .biota
            .properties_attribute_2nd
            .as_ref()
            .and_then(|d| d.get(&self.vital))
            .unwrap_or_else(|| {
                panic!(
                    "CreatureVital({}): the biota record is gone",
                    self.vital.to_dotnet_string()
                )
            })
    }

    fn record_mut(self, creature: &mut WorldObject) -> &mut PropertiesAttribute2nd {
        creature
            .biota
            .properties_attribute_2nd
            .as_mut()
            .and_then(|d| d.get_mut(&self.vital))
            .unwrap_or_else(|| {
                panic!(
                    "CreatureVital({}): the biota record is gone",
                    self.vital.to_dotnet_string()
                )
            })
    }

    // ACE: CreatureVital.StartingValue
    #[must_use]
    pub fn starting_value(self, creature: &WorldObject) -> u32 {
        self.record(creature).init_level
    }

    /// The `StartingValue` setter.
    pub fn set_starting_value(self, creature: &mut WorldObject, value: u32) {
        self.record_mut(creature).init_level = value;
    }

    // ACE: CreatureVital.ExperienceSpent
    /// Total experience spent on this vital.
    #[must_use]
    pub fn experience_spent(self, creature: &WorldObject) -> u32 {
        self.record(creature).cp_spent
    }

    /// The `ExperienceSpent` setter.
    pub fn set_experience_spent(self, creature: &mut WorldObject, value: u32) {
        self.record_mut(creature).cp_spent = value;
    }

    // ACE: CreatureVital.ExperienceLeft
    /// The vital experience remaining until max rank (`uint`: wraps past the last entry).
    #[must_use]
    pub fn experience_left(self, w: &World, creature: &WorldObject) -> u32 {
        let vital_xp_table = &w.dats.portal_dat().xp_table().vital_xp;

        vital_xp_table[vital_xp_table.len() - 1].wrapping_sub(self.experience_spent(creature))
    }

    // ACE: CreatureVital.Ranks
    /// The number of times this vital has been raised, derived from `ExperienceSpent`.
    #[must_use]
    pub fn ranks(self, creature: &WorldObject) -> u32 {
        self.record(creature).level_from_cp
    }

    /// The `Ranks` setter.
    pub fn set_ranks(self, creature: &mut WorldObject, value: u32) {
        self.record_mut(creature).level_from_cp = value;
    }

    // ACE: CreatureVital.IsMaxRank
    /// True if this vital has been raised the maximum number of times.
    #[must_use]
    pub fn is_max_rank(self, w: &World, creature: &WorldObject) -> bool {
        let vital_xp_table = &w.dats.portal_dat().xp_table().vital_xp;

        i64::from(self.ranks(creature)) >= count_i32(vital_xp_table.len()).wrapping_sub(1).into()
    }

    // ACE: CreatureVital.Base
    /// The starting value, the ranks and the attribute formula (unenchanted); a player's max
    /// health adds `Enlightenment * 2 + GetGearMaxHealth()`, Enlightenment only when positive
    /// (retail's gate, V244; ACE added a negative count).
    #[must_use]
    pub fn base(self, w: &World, creature: &WorldObject) -> u32 {
        let attr = attribute_formula::get_formula_vital(
            &mut StatCtx::detached(w, creature),
            self.vital,
            false,
        );

        let mut total = self
            .starting_value(creature)
            .wrapping_add(self.ranks(creature))
            .wrapping_add(attr);

        if creature.is_player() && self.vital == PropertyAttribute2nd::MaxHealth {
            let bonus: u32 = enlightenment_bonus(creature)
                .wrapping_mul(2)
                .wrapping_add(get_gear_max_health(w, creature))
                .cs_cast();
            total = total.wrapping_add(bonus);
        }

        total
    }

    // ACE: CreatureVital.Current
    #[must_use]
    pub fn current(self, creature: &WorldObject) -> u32 {
        self.record(creature).current_level
    }

    /// The `Current` setter.
    pub fn set_current(self, creature: &mut WorldObject, value: u32) {
        self.record_mut(creature).current_level = value;
    }

    // ACE: CreatureVital.MaxValue
    #[must_use]
    pub fn max_value(self, c: &mut StatCtx<'_>) -> u32 {
        self.get_max_value(c, true)
    }

    // ACE: CreatureVital.GetMaxValue
    /// The formula total, times the multiplicative enchantments and vitae, plus the additive
    /// enchantments, rounded half away from zero; never below 5 (1 for a total under 5).
    #[must_use]
    pub fn get_max_value(self, c: &mut StatCtx<'_>, enchanted: bool) -> u32 {
        let attr = attribute_formula::get_formula_vital(c, self.vital, enchanted);

        let (w, creature) = (c.world(), c.creature());
        let mut total: u32 = self
            .starting_value(creature)
            .wrapping_add(self.ranks(creature))
            .wrapping_add(attr);

        let player = creature.is_player();

        if player {
            // Enlightenment and GearMaxHealth didn't work like other additives
            // most additives (ie. from enchantments) were added in *after* multipliers,
            // but Enlightenment and GearMaxHealth were an exception, and added in beforehand

            // this means Enlightenment and GearMaxHealth would get scaled by multipliers,
            // including Asheron's Benediction, and oddly enough, vitae as well

            // it's also possible these were considered "base"
            if self.vital == PropertyAttribute2nd::MaxHealth {
                let bonus: u32 = enlightenment_bonus(creature)
                    .wrapping_mul(2)
                    .wrapping_add(get_gear_max_health(w, creature))
                    .cs_cast();
                total = total.wrapping_add(bonus);
            }
        }

        // apply multiplicative enchantments first
        let multiplier = if enchanted {
            em::get_vital_mod_multiplier(c, self)
        } else {
            1.0f32
        };

        let mut f_total = uint_to_f32(total) * multiplier;

        if player {
            let vitae = player_vitae(c);

            #[allow(clippy::float_cmp)]
            if vitae != 1.0f32 {
                f_total *= vitae;
            }
        }

        // everything beyond this point does not get scaled by vitae
        let additives = if enchanted {
            em::get_vital_mod_additives(c, self)
        } else {
            0.0f32
        };

        let mut i_total = float_extensions::round(f_total + additives, 0);

        // a creature cannot fall below 5 MaxVital from enchantments / vitae normally,
        // or 1 MaxVital for creatures with very low starting vitals
        let min_vital: i32 = if total >= 5 { 5 } else { 1 };

        i_total = min_vital.max(i_total);

        i_total.cs_cast()
    }

    // ACE: CreatureVital.Missing
    /// `MaxValue - Current` (`uint`: wraps when the current value is above the maximum).
    #[must_use]
    pub fn missing(self, c: &mut StatCtx<'_>) -> u32 {
        self.max_value(c).wrapping_sub(self.current(c.creature()))
    }

    // ACE: CreatureVital.Percent
    #[must_use]
    pub fn percent(self, c: &mut StatCtx<'_>) -> f32 {
        uint_to_f32(self.current(c.creature())) / uint_to_f32(self.max_value(c))
    }

    // ACE: CreatureVital.ModifierType
    #[must_use]
    pub fn modifier_type(self, c: &mut StatCtx<'_>) -> ModifierType {
        let enchanted: i32 = self.get_max_value(c, true).cs_cast();
        let unenchanted: i32 = self.get_max_value(c, false).cs_cast();
        let diff = enchanted.wrapping_sub(unenchanted);

        if diff > 0 {
            ModifierType::Buffed
        } else if diff < 0 {
            ModifierType::Debuffed
        } else {
            ModifierType::None
        }
    }

    // ACE: CreatureVital.ToEnum
    #[must_use]
    pub fn to_enum(self) -> Vital {
        match self.vital {
            PropertyAttribute2nd::MaxHealth => Vital::Health,
            PropertyAttribute2nd::MaxStamina => Vital::Stamina,
            PropertyAttribute2nd::MaxMana => Vital::Mana,
            _ => Vital::Undefined,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Not ACE: pointers to members of other ACE files, named after them.
// ---------------------------------------------------------------------------------------------

/// `Creature.GetGearMaxHealth()` (`Creature_Rating.cs`): `GetEquippedItemsRatingSum(PropertyInt.GearMaxHealth)`,
/// read from the creature's own rating cache (the creature may not be in the store yet, so
/// `creature_rating::get_gear_max_health`'s guid lookup is not usable here).
#[must_use]
pub fn get_gear_max_health(_w: &World, creature: &WorldObject) -> i32 {
    creature
        .creature
        .as_ref()
        .and_then(|c| c.creature_equipment.equipped_items_rating_cache.as_ref())
        .and_then(|cache| {
            cache
                .get(&empyrean_entity::enums::PropertyInt::GearMaxHealth)
                .copied()
        })
        .unwrap_or(0)
}

/// Not ACE: the Enlightenment count max health adds, only when positive (retail's gate, V244:
/// the client adds it only when above zero). ACE read `Enlightenment` as is.
fn enlightenment_bonus(creature: &WorldObject) -> i32 {
    creature.enlightenment().max(0)
}
