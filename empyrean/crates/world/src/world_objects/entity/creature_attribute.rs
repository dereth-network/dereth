// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Entity/CreatureAttribute.cs
//! Port of `Source/ACE.Server/WorldObjects/Entity/CreatureAttribute.cs`.
//!
//! ACE's `CreatureAttribute` wraps one `PropertiesAttribute` record of its creature's biota and
//! holds a reference to the creature. Here it is a `Copy` handle naming the attribute; every
//! member takes the creature (`&WorldObject`) explicitly and reads the record from its biota. The
//! constructor adds the record when the biota lacks it, as ACE's does, so a handle built by it
//! always finds its record.
//!
//! The enchanted getters take a [`StatCtx`]: the creature in the world (its caching
//! `EnchantmentManager`) or, under construction, detached. [`em`] holds the
//! `EnchantmentManager` members the stat classes read.

use empyrean_common::dotnet::CsCast;
use empyrean_common::extensions::float_extensions;
use empyrean_entity::enums::{ModifierType, PropertyAttribute};
use empyrean_entity::models::properties_attribute::PropertiesAttribute;

use empyrean_entity::ObjectGuid;

use crate::world_objects::world_object::WorldObject;
use crate::World;

// ACE: CreatureAttribute
/// One primary attribute of a creature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CreatureAttribute {
    // ACE: CreatureAttribute.Attribute
    pub attribute: PropertyAttribute,
}

impl CreatureAttribute {
    // ACE: CreatureAttribute.CreatureAttribute
    /// If the creature's biota does not contain this attribute, a new record is created.
    ///
    /// # Panics
    /// When the biota has no attribute dictionary (ACE: `NullReferenceException`; a creature's
    /// constructor always creates it).
    pub fn new(creature: &mut WorldObject, attribute: PropertyAttribute) -> Self {
        let dict = creature
            .biota
            .properties_attribute
            .as_mut()
            .expect("NullReferenceException: Biota.PropertiesAttribute");
        if !dict.contains_key(&attribute) {
            dict.insert(attribute, PropertiesAttribute::default());
        }
        Self { attribute }
    }

    /// `propertiesAttribute`, the record the constructor found or created.
    fn record(self, creature: &WorldObject) -> &PropertiesAttribute {
        creature
            .biota
            .properties_attribute
            .as_ref()
            .and_then(|d| d.get(&self.attribute))
            .unwrap_or_else(|| {
                panic!(
                    "CreatureAttribute({}): the biota record is gone",
                    self.attribute.to_dotnet_string()
                )
            })
    }

    fn record_mut(self, creature: &mut WorldObject) -> &mut PropertiesAttribute {
        creature
            .biota
            .properties_attribute
            .as_mut()
            .and_then(|d| d.get_mut(&self.attribute))
            .unwrap_or_else(|| {
                panic!(
                    "CreatureAttribute({}): the biota record is gone",
                    self.attribute.to_dotnet_string()
                )
            })
    }

    // ACE: CreatureAttribute.StartingValue
    /// The base value; for players, set during character creation.
    #[must_use]
    pub fn starting_value(self, creature: &WorldObject) -> u32 {
        self.record(creature).init_level
    }

    /// The `StartingValue` setter.
    pub fn set_starting_value(self, creature: &mut WorldObject, value: u32) {
        self.record_mut(creature).init_level = value;
    }

    // ACE: CreatureAttribute.ExperienceSpent
    /// Total experience spent on this attribute.
    #[must_use]
    pub fn experience_spent(self, creature: &WorldObject) -> u32 {
        self.record(creature).cp_spent
    }

    /// The `ExperienceSpent` setter.
    pub fn set_experience_spent(self, creature: &mut WorldObject, value: u32) {
        self.record_mut(creature).cp_spent = value;
    }

    // ACE: CreatureAttribute.ExperienceLeft
    /// The attribute experience remaining until max rank (`uint` arithmetic: wraps when more
    /// than the table's last entry has been spent).
    #[must_use]
    pub fn experience_left(self, w: &World, creature: &WorldObject) -> u32 {
        let attribute_xp_table = &w.dats.portal_dat().xp_table().attribute_xp;

        attribute_xp_table[attribute_xp_table.len() - 1]
            .wrapping_sub(self.experience_spent(creature))
    }

    // ACE: CreatureAttribute.Ranks
    /// The number of times this attribute has been raised, derived from `ExperienceSpent`.
    #[must_use]
    pub fn ranks(self, creature: &WorldObject) -> u32 {
        self.record(creature).level_from_cp
    }

    /// The `Ranks` setter.
    pub fn set_ranks(self, creature: &mut WorldObject, value: u32) {
        self.record_mut(creature).level_from_cp = value;
    }

    // ACE: CreatureAttribute.IsMaxRank
    /// True if this attribute has been raised the maximum number of times.
    #[must_use]
    pub fn is_max_rank(self, w: &World, creature: &WorldObject) -> bool {
        let attribute_xp_table = &w.dats.portal_dat().xp_table().attribute_xp;

        // `Ranks >= (attributeXPTable.Count - 1)`: uint against int, compared as long
        i64::from(self.ranks(creature))
            >= count_i32(attribute_xp_table.len()).wrapping_sub(1).into()
    }

    // ACE: CreatureAttribute.Base
    /// The starting value plus the ranks. (ACE's commented-out innate augmentation bonus has
    /// moved into `InitLevel`.)
    #[must_use]
    pub fn base(self, creature: &WorldObject) -> u32 {
        self.ranks(creature)
            .wrapping_add(self.starting_value(creature))
    }

    // ACE: CreatureAttribute.Current
    #[must_use]
    pub fn current(self, c: &mut StatCtx<'_>) -> u32 {
        self.get_current(c, true)
    }

    // ACE: CreatureAttribute.GetCurrent
    /// `Base` with the enchantment multipliers, then the additives, rounded half away from zero;
    /// never below 10 (1 for a base under 10).
    #[must_use]
    pub fn get_current(self, c: &mut StatCtx<'_>, enchanted: bool) -> u32 {
        let multipliers = if enchanted {
            em::get_attribute_mod_multiplier(c, self.attribute)
        } else {
            1.0f32
        };
        let additives = if enchanted {
            em::get_attribute_mod_additive(c, self.attribute)
        } else {
            0
        };

        let creature = c.creature();
        let base: i32 = self.base(creature).cs_cast();
        let mut total = int_to_f32(base) * multipliers + int_to_f32(additives);

        total = int_to_f32(float_extensions::round(total, 0));

        // attributes cannot be debuffed below 10 normally,
        // or 1 for creatures with very low starting attributes
        let minimum_attribute: i32 = if self.base(creature) >= 10 { 10 } else { 1 };

        math_max_f32(int_to_f32(minimum_attribute), total).cs_cast()
    }

    // ACE: CreatureAttribute.ModifierType
    #[must_use]
    pub fn modifier_type(self, c: &mut StatCtx<'_>) -> ModifierType {
        let enchanted: i32 = self.get_current(c, true).cs_cast();
        let unenchanted: i32 = self.get_current(c, false).cs_cast();
        let diff = enchanted.wrapping_sub(unenchanted);

        if diff > 0 {
            ModifierType::Buffed
        } else if diff < 0 {
            ModifierType::Debuffed
        } else {
            ModifierType::None
        }
    }
}

/// C#'s implicit `int` to `float` conversion (round to nearest).
#[allow(clippy::cast_precision_loss)]
pub(crate) fn int_to_f32(v: i32) -> f32 {
    v as f32
}

/// C#'s implicit `uint` to `float` conversion (round to nearest).
#[allow(clippy::cast_precision_loss)]
pub(crate) fn uint_to_f32(v: u32) -> f32 {
    v as f32
}

/// A `List<T>.Count` (an `int`).
pub(crate) fn count_i32(len: usize) -> i32 {
    i32::try_from(len).expect("a List's Count fits an int")
}

/// `Math.Max(float, float)`: NaN if either is NaN, else the larger.
pub(crate) fn math_max_f32(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() {
        f32::NAN
    } else if a > b {
        a
    } else {
        b
    }
}

/// The creature a stat member reads, and the way to its `EnchantmentManager`.
///
/// ACE's stat objects hold their creature, and every enchanted getter calls
/// `creature.EnchantmentManager`, an `EnchantmentManagerWithCaching`. Its caches live
/// in the object and its members take `(w, guid)`, so an enchanted getter needs the creature
/// **in the world**: [`StatCtx::InWorld`]. A creature under construction is not in the store yet
/// (`Creature.SetEphemeralValues` reads `MaxValue`): [`StatCtx::Detached`] reads the same
/// registry without the caches (see [`em`]). Unenchanted reads may use either.
pub enum StatCtx<'a> {
    /// A creature in `w.objects`.
    InWorld(&'a mut World, ObjectGuid),
    /// A creature outside the store (construction), or an unenchanted read.
    Detached(&'a World, &'a WorldObject),
}

impl std::fmt::Debug for StatCtx<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InWorld(_, this) => f.debug_tuple("InWorld").field(this).finish(),
            Self::Detached(_, o) => f.debug_tuple("Detached").field(&o.guid).finish(),
        }
    }
}

impl<'a> StatCtx<'a> {
    /// The creature `this`, in the world.
    pub fn in_world(w: &'a mut World, this: ObjectGuid) -> Self {
        Self::InWorld(w, this)
    }

    /// A creature outside the world (or an unenchanted read of one in it).
    #[must_use]
    pub fn detached(w: &'a World, creature: &'a WorldObject) -> Self {
        Self::Detached(w, creature)
    }

    /// The world (the dats, the clock, the managers).
    #[must_use]
    pub fn world(&self) -> &World {
        match self {
            Self::InWorld(w, _) => w,
            Self::Detached(w, _) => w,
        }
    }

    /// The creature.
    ///
    /// # Panics
    /// When an in-world creature is not in the store (ACE: `NullReferenceException`).
    #[must_use]
    pub fn creature(&self) -> &WorldObject {
        match self {
            Self::InWorld(w, this) => w
                .objects
                .get(*this)
                .unwrap_or_else(|| panic!("Creature {this:?}: missing object")),
            Self::Detached(_, o) => o,
        }
    }
}

/// The `EnchantmentManager` members the stat classes read, by ACE name.
///
/// - **In the world** each calls the `enchantment_manager_with_caching` override (every
///   ACE `WorldObject.EnchantmentManager` is one), which fills and reads the object's caches.
///   `GetVitae` is not overridden and calls the base manager.
/// - **Detached** (a creature under construction, not in the store yet): the base
///   `EnchantmentManager` aggregation over the object's own registry, through empyrean-entity's
///   registry queries, without the caches. The caches only memoise and start empty, so the values
///   are the ones ACE computes. Only the members a constructor reaches are offered detached
///   (attribute and vital modifiers, vitae); the skill and regeneration modifiers take the world.
pub mod em {
    use empyrean_common::dotnet::CsCast;
    use empyrean_entity::enums::{EnchantmentTypeFlags, PropertyAttribute, Skill, SpellId};
    use empyrean_entity::models::properties_enchantment_registry_extensions as reg;
    use empyrean_entity::models::PropertiesEnchantmentRegistry;
    use empyrean_entity::ObjectGuid;

    use super::StatCtx;
    use crate::world_objects::entity::creature_vital::CreatureVital;
    use crate::world_objects::managers::enchantment_manager as base;
    use crate::world_objects::managers::enchantment_manager_with_caching as caching;
    use crate::world_objects::world_object::WorldObject;
    use crate::World;

    /// `EnchantmentManager.GetAttributeMod_Multiplier(attribute)`.
    #[must_use]
    pub fn get_attribute_mod_multiplier(c: &mut StatCtx<'_>, attribute: PropertyAttribute) -> f32 {
        match c {
            StatCtx::InWorld(w, this) => caching::get_attribute_mod_multiplier(w, *this, attribute),
            StatCtx::Detached(_, o) => {
                let flags = EnchantmentTypeFlags::Attribute | EnchantmentTypeFlags::Multiplicative;
                let mut multiplier = 1.0f32;
                for enchantment in top_layer(o, flags, u32::from(attribute.0)) {
                    multiplier *= enchantment.stat_mod_value;
                }
                multiplier
            }
        }
    }

    /// `EnchantmentManager.GetAttributeMod_Additive(attribute)`.
    #[must_use]
    pub fn get_attribute_mod_additive(c: &mut StatCtx<'_>, attribute: PropertyAttribute) -> i32 {
        match c {
            StatCtx::InWorld(w, this) => caching::get_attribute_mod_additive(w, *this, attribute),
            StatCtx::Detached(_, o) => {
                let flags = EnchantmentTypeFlags::Attribute | EnchantmentTypeFlags::Additive;
                let mut attribute_mod = 0i32;
                for enchantment in top_layer(o, flags, u32::from(attribute.0)) {
                    let v: i32 = enchantment.stat_mod_value.cs_cast();
                    attribute_mod = attribute_mod.wrapping_add(v);
                }
                attribute_mod
            }
        }
    }

    /// `EnchantmentManager.GetVitalMod_Multiplier(vital)`.
    #[must_use]
    pub fn get_vital_mod_multiplier(c: &mut StatCtx<'_>, vital: CreatureVital) -> f32 {
        match c {
            StatCtx::InWorld(w, this) => caching::get_vital_mod_multiplier(w, *this, vital.vital),
            StatCtx::Detached(_, o) => {
                let flags = EnchantmentTypeFlags::SecondAtt | EnchantmentTypeFlags::Multiplicative;
                let mut multiplier = 1.0f32;
                for enchantment in top_layer(o, flags, u32::from(vital.vital.0)) {
                    multiplier *= enchantment.stat_mod_value;
                }
                multiplier
            }
        }
    }

    /// `EnchantmentManager.GetVitalMod_Additives(vital)`.
    #[must_use]
    pub fn get_vital_mod_additives(c: &mut StatCtx<'_>, vital: CreatureVital) -> f32 {
        match c {
            StatCtx::InWorld(w, this) => caching::get_vital_mod_additives(w, *this, vital.vital),
            StatCtx::Detached(_, o) => {
                let flags = EnchantmentTypeFlags::SecondAtt | EnchantmentTypeFlags::Additive;
                let mut modifier = 0.0f32;
                for enchantment in top_layer(o, flags, u32::from(vital.vital.0)) {
                    modifier += enchantment.stat_mod_value;
                }
                modifier
            }
        }
    }

    /// `EnchantmentManager.GetSkillMod_Multiplier(skill)` (in the world only).
    #[must_use]
    pub fn get_skill_mod_multiplier(w: &mut World, this: ObjectGuid, skill: Skill) -> f32 {
        caching::get_skill_mod_multiplier(w, this, skill)
    }

    /// `EnchantmentManager.GetSkillMod_Additives(skill)`, including the defense and attack
    /// debuffs (in the world only).
    #[must_use]
    pub fn get_skill_mod_additives(w: &mut World, this: ObjectGuid, skill: Skill) -> i32 {
        caching::get_skill_mod_additives(w, this, skill)
    }

    /// `EnchantmentManager.GetRegenerationMod(vital)` (in the world only).
    #[must_use]
    pub fn get_regeneration_mod(w: &mut World, this: ObjectGuid, vital: CreatureVital) -> f32 {
        caching::get_regeneration_mod(w, this, vital.vital)
    }

    /// `EnchantmentManager.GetVitae()?.StatModValue`: `None` without a vitae entry.
    #[must_use]
    pub fn get_vitae(c: &StatCtx<'_>) -> Option<f32> {
        match c {
            StatCtx::InWorld(w, this) => base::get_vitae(w, *this).map(|e| e.stat_mod_value),
            StatCtx::Detached(_, o) => reg::get_enchantment_by_spell(
                Some(registry(o)),
                SpellId::Vitae.0.cast_signed(),
                None,
            )
            .map(|e| e.stat_mod_value),
        }
    }

    /// `WorldObject.Biota.PropertiesEnchantmentRegistry` (`InitializePropertyDictionaries` always
    /// creates it; a missing one is ACE's `NullReferenceException`).
    fn registry(o: &WorldObject) -> &Vec<PropertiesEnchantmentRegistry> {
        o.biota
            .properties_enchantment_registry
            .as_ref()
            .expect("ACE: Biota.PropertiesEnchantmentRegistry is null (NullReferenceException)")
    }

    /// `GetEnchantments_TopLayer(statModType, statModKey, handleMultiple: true)` over a detached
    /// creature's registry (the only form the stat getters use).
    fn top_layer(
        o: &WorldObject,
        stat_mod_type: EnchantmentTypeFlags,
        stat_mod_key: u32,
    ) -> Vec<&PropertiesEnchantmentRegistry> {
        reg::get_enchantments_top_layer_by_stat_mod_type_and_key(
            Some(registry(o)),
            stat_mod_type,
            stat_mod_key,
            true,
        )
        .expect("the registry is not null")
    }
}
