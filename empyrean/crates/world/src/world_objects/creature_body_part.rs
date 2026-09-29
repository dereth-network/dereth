// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_BodyPart.cs
//! Port of `Source/ACE.Server/WorldObjects/Creature_BodyPart.cs`.
//!
//! A creature's body part as the damage pipeline sees it: the creature (a guid)
//! and one entry of its biota's body part table. ACE's `KeyValuePair<CombatBodyPart,
//! PropertiesBodyPart>` has a null value when the biota lacks the part; here that is `None`, and
//! reading it panics as ACE's `NullReferenceException` would.

#![allow(clippy::cast_possible_truncation)] // C#'s `(float)` of a double, as ACE writes it

use empyrean_entity::enums::{CombatBodyPart, CoverageMask, DamageType, PropertyInt};
use empyrean_entity::models::PropertiesBodyPart;
use empyrean_entity::ObjectGuid;

use crate::entity::body_part;
use crate::world_objects::creature_equipment;
use crate::world_objects::managers::enchantment_manager_with_caching as emc;
use crate::world_objects::skill_formula;
use crate::world_objects::world_object::WorldObject;
use crate::World;

fn object(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

/// `Math.Clamp(float, float, float)` (min <= max): a NaN passes through.
fn math_clamp_f32(value: f32, min: f32, max: f32) -> f32 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

/// `Creature_BodyPart`: a creature and one of its biota's body parts.
// ACE: Creature_BodyPart
#[derive(Debug, Clone, PartialEq)]
pub struct CreatureBodyPart {
    /// `Creature`.
    pub creature: ObjectGuid,
    /// `Biota`.
    pub biota: (CombatBodyPart, Option<PropertiesBodyPart>),
}

impl CreatureBodyPart {
    // ACE: Creature_BodyPart.Creature_BodyPart
    #[must_use]
    pub fn new(creature: ObjectGuid, biota: (CombatBodyPart, Option<PropertiesBodyPart>)) -> Self {
        Self { creature, biota }
    }

    /// `Creature.EnchantmentManager`: the creature whose manager answers.
    // ACE: Creature_BodyPart.EnchantmentManager
    #[must_use]
    pub fn enchantment_manager(&self) -> ObjectGuid {
        self.creature
    }

    /// `Biota.Value`.
    ///
    /// # Panics
    /// When the biota had no such part (ACE: `NullReferenceException`).
    fn value(&self) -> &PropertiesBodyPart {
        self.biota
            .1
            .as_ref()
            .expect("System.NullReferenceException: Creature_BodyPart.Biota.Value")
    }

    /// Main entry point for getting the armor mod (`GetArmorMod(DamageType, List<WorldObject>
    /// armorLayers, Creature attacker, WorldObject weapon, float armorRendingMod = 1.0f)`).
    // ACE: Creature_BodyPart.GetArmorMod
    pub fn get_armor_mod(
        &self,
        w: &mut World,
        damage_type: DamageType,
        armor_layers: &[ObjectGuid],
        attacker: Option<ObjectGuid>,
        weapon: Option<ObjectGuid>,
        armor_rending_mod: f32,
    ) -> f32 {
        let effective_armor_vs_type = self.get_effective_armor_vs_type(
            w,
            damage_type,
            armor_layers,
            attacker,
            weapon,
            armor_rending_mod,
        );

        skill_formula::calc_armor_mod(effective_armor_vs_type)
    }

    /// The body part's effective armor level against a damage type: the base AL times the
    /// creature's `ArmorModVs<type>`, plus the armor enchantments, plus every armor layer.
    // ACE: Creature_BodyPart.GetEffectiveArmorVsType
    pub fn get_effective_armor_vs_type(
        &self,
        w: &mut World,
        damage_type: DamageType,
        armor_layers: &[ObjectGuid],
        attacker: Option<ObjectGuid>,
        weapon: Option<ObjectGuid>,
        armor_rending_mod: f32,
    ) -> f32 {
        let ignore_magic_armor = weapon.is_some_and(|g| object(w, g).ignore_magic_armor())
            || attacker.is_some_and(|g| object(w, g).ignore_magic_armor());
        let ignore_magic_resist = weapon.is_some_and(|g| object(w, g).ignore_magic_resist())
            || attacker.is_some_and(|g| object(w, g).ignore_magic_resist());

        // get base AL / RL
        let armor_vs_type = self.value().base_armor as f32
            * crate::world_objects::creature_properties::get_armor_vs_type(
                object(w, self.creature),
                damage_type,
            ) as f32;

        // additive enchantments:
        // imperil / armor
        let enchantment_mod = if ignore_magic_resist {
            0
        } else {
            emc::get_body_armor_mod(w, self.enchantment_manager())
        };

        let mut effective_al = armor_vs_type + enchantment_mod as f32;

        // handle monsters w/ multiple layers of armor
        for &armor_layer in armor_layers {
            effective_al +=
                self.get_armor_mod_piece(w, armor_layer, damage_type, ignore_magic_armor);
        }

        // armor rending reduces base armor + all physical armor too?
        if effective_al > 0.0 {
            effective_al *= armor_rending_mod;
        }

        effective_al
    }

    /// Returns the effective AL for 1 piece of armor/clothing (`GetArmorMod(WorldObject armor,
    /// DamageType, bool ignoreMagicArmor)`).
    // ACE: Creature_BodyPart.GetArmorMod
    pub fn get_armor_mod_piece(
        &self,
        w: &mut World,
        armor: ObjectGuid,
        damage_type: DamageType,
        ignore_magic_armor: bool,
    ) -> f32 {
        // get base armor/resistance level
        let base_armor = object(w, armor)
            .get_property(PropertyInt::ArmorLevel)
            .unwrap_or(0);
        //var armorType = armor.GetProperty(PropertyInt.ArmorType) ?? 0;
        let resistance =
            crate::world_objects::monster_melee::get_resistance(object(w, armor), damage_type);

        /*Console.WriteLine(armor.Name);
        Console.WriteLine("--");
        Console.WriteLine("Base AL: " + baseArmor);
        Console.WriteLine("Base RL: " + resistance);*/

        // armor level additives
        let armor_mod = if ignore_magic_armor {
            0
        } else {
            emc::get_armor_mod(w, armor)
        };
        // Console.WriteLine("Impen: " + armorMod);
        let effective_al = base_armor.wrapping_add(armor_mod);

        // resistance additives
        let armor_bane = if ignore_magic_armor {
            0.0
        } else {
            emc::get_armor_mod_vs_type(w, armor, damage_type)
        };
        // Console.WriteLine("Bane: " + armorBane);
        let mut effective_rl = (resistance + f64::from(armor_bane)) as f32;

        // resistance clamp
        effective_rl = math_clamp_f32(effective_rl, -2.0, 2.0);

        // TODO: could brittlemail / lures send a piece of armor or clothing's AL into the negatives?
        //if (effectiveAL < 0 && effectiveRL != 0)
        //effectiveRL = 1.0f / effectiveRL;

        /*Console.WriteLine("Effective AL: " + effectiveAL);
        Console.WriteLine("Effective RL: " + effectiveRL);
        Console.WriteLine();*/

        effective_al as f32 * effective_rl
    }

    /// The creature's equipped clothing covering the body part, in `EquippedObjects` order.
    // ACE: Creature_BodyPart.GetArmorLayers
    #[must_use]
    pub fn get_armor_layers(&self, w: &World, body_part: CombatBodyPart) -> Vec<ObjectGuid> {
        let coverage_mask = body_part::get_coverage_mask_combat(body_part);

        creature_equipment::equipped_objects_values(w, self.creature)
            .into_iter()
            .filter(|&e| {
                let o = object(w, e);
                o.is_clothing()
                    && (o.clothing_priority().unwrap_or(CoverageMask(0)) & coverage_mask)
                        != CoverageMask(0)
            })
            .collect()
    }
}
