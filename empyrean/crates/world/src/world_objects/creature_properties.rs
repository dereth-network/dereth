// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Properties.cs
//! Port of `Source/ACE.Server/WorldObjects/Creature_Properties.cs`: the members with logic (the
//! typed property wrappers are generated in `props/creature_properties.rs`). The resistance
//! getters are re-exported by `creature_combat/shim.rs`.

use empyrean_common::dotnet::math;
use empyrean_entity::enums::{DamageType, FactionBits, PropertyFloat, ResistanceType};
use empyrean_entity::ObjectGuid;

use crate::dispatch::get_natural_resistance::get_natural_resistance;
use crate::world_objects::creature_rating;
use crate::world_objects::managers::enchantment_manager_with_caching as emc;
use crate::world_objects::player_properties::get_augmentation_resistance;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// Non-property fields declared in `Creature_Properties.cs`.
#[derive(Debug, Default)]
pub struct CreaturePropertiesFields {}

fn object(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

// ACE: Creature.GetResistanceMod
/// `GetResistanceMod(DamageType damageType, WorldObject attacker, WorldObject weapon, float
/// weaponResistanceMod = 1.0f)`.
pub fn get_resistance_mod_damage(
    w: &mut World,
    this: ObjectGuid,
    damage_type: DamageType,
    attacker: Option<ObjectGuid>,
    weapon: Option<ObjectGuid>,
    weapon_resistance_mod: f32,
) -> f32 {
    let ignore_magic_resist = weapon.is_some_and(|g| object(w, g).ignore_magic_resist())
        || attacker.is_some_and(|g| object(w, g).ignore_magic_resist());

    // hollow weapons also ignore player natural resistances
    #[allow(clippy::float_cmp)]
    if ignore_magic_resist
        && (!attacker.is_some_and(|g| object(w, g).is_player())
            || !object(w, this).is_player()
            || crate::managers::property_manager::get_double(
                w,
                "ignore_magic_resist_pvp_scalar",
                0.0,
                true,
            )
            .item
                == 1.0)
    {
        return weapon_resistance_mod;
    }

    let mut prot_mod = emc::get_protection_resistance_mod(w, this, damage_type);
    let mut vuln_mod = emc::get_vulnerability_resistance_mod(w, this, damage_type);

    let natural_resist_mod = get_natural_resistance(w, this, damage_type);

    // protection mod becomes either life protection or natural resistance,
    // whichever is more powerful (more powerful = lower value here)
    if prot_mod > natural_resist_mod {
        prot_mod = natural_resist_mod;
    }

    // does this stack with natural resistance?
    if object(w, this).is_player() {
        let resist_aug = get_augmentation_resistance(object(w, this), damage_type);
        if resist_aug > 0 {
            #[allow(clippy::cast_precision_loss)]
            let aug_factor = math::min_f32(1.0, resist_aug as f32 * 0.1);
            prot_mod *= 1.0 - aug_factor;
        }
    }

    // vulnerability mod becomes either life vuln or weapon resistance mod,
    // whichever is more powerful
    if vuln_mod < weapon_resistance_mod {
        vuln_mod = weapon_resistance_mod;
    }

    if ignore_magic_resist {
        // convert to additive space
        let mut add_prot = creature_rating::mod_to_rating(prot_mod).wrapping_neg();
        let mut add_vuln = creature_rating::mod_to_rating(vuln_mod);

        // scale
        add_prot =
            crate::world_objects::monster_melee::ignore_magic_resist_scaled(w, this, add_prot);
        add_vuln =
            crate::world_objects::monster_melee::ignore_magic_resist_scaled(w, this, add_vuln);

        prot_mod = creature_rating::get_negative_rating_mod(add_prot, false);
        vuln_mod = creature_rating::get_positive_rating_mod(add_vuln);
    }

    prot_mod * vuln_mod
}

// ACE: Creature.GetArmorVsType
/// `GetProperty(ArmorModVs<Type>) ?? 1.0f`.
#[must_use]
pub fn get_armor_vs_type(o: &WorldObject, damage_type: DamageType) -> f64 {
    let key = match damage_type {
        DamageType::Slash => PropertyFloat::ArmorModVsSlash,
        DamageType::Pierce => PropertyFloat::ArmorModVsPierce,
        DamageType::Bludgeon => PropertyFloat::ArmorModVsBludgeon,
        DamageType::Fire => PropertyFloat::ArmorModVsFire,
        DamageType::Cold => PropertyFloat::ArmorModVsCold,
        DamageType::Acid => PropertyFloat::ArmorModVsAcid,
        DamageType::Electric => PropertyFloat::ArmorModVsElectric,
        DamageType::Nether => PropertyFloat::ArmorModVsNether,
        _ => return 1.0,
    };
    o.get_property(key).unwrap_or(1.0)
}

/// `Resist<Type>` for the eight damage types.
fn resist_property(o: &WorldObject, damage_type: DamageType) -> Option<f64> {
    match damage_type {
        DamageType::Slash => o.resist_slash(),
        DamageType::Pierce => o.resist_pierce(),
        DamageType::Bludgeon => o.resist_bludgeon(),
        DamageType::Fire => o.resist_fire(),
        DamageType::Cold => o.resist_cold(),
        DamageType::Acid => o.resist_acid(),
        DamageType::Electric => o.resist_electric(),
        DamageType::Nether => o.resist_nether(),
        _ => None,
    }
}

// ACE: Creature.GetResistanceMod
/// `GetResistanceMod(ResistanceType resistance, WorldObject attacker = null, WorldObject weapon =
/// null, float weaponResistanceMod = 1.0f)`.
pub fn get_resistance_mod(
    w: &mut World,
    this: ObjectGuid,
    resistance: ResistanceType,
    attacker: Option<ObjectGuid>,
    weapon: Option<ObjectGuid>,
    weapon_resistance_mod: f32,
) -> f64 {
    let damage_type = match resistance {
        ResistanceType::Slash => DamageType::Slash,
        ResistanceType::Pierce => DamageType::Pierce,
        ResistanceType::Bludgeon => DamageType::Bludgeon,
        ResistanceType::Fire => DamageType::Fire,
        ResistanceType::Cold => DamageType::Cold,
        ResistanceType::Acid => DamageType::Acid,
        ResistanceType::Electric => DamageType::Electric,
        ResistanceType::Nether => DamageType::Nether,
        ResistanceType::HealthBoost => {
            let r = object(w, this).resist_health_boost().unwrap_or(1.0);
            return r * f64::from(creature_rating::get_healing_rating_mod(w, this));
        }
        ResistanceType::HealthDrain => {
            let r = object(w, this).resist_health_drain().unwrap_or(1.0);
            let natural = get_natural_resistance(w, this, DamageType::Health);
            return r
                * f64::from(natural)
                * f64::from(creature_rating::get_life_resist_rating_mod(w, this));
        }
        ResistanceType::StaminaBoost => {
            let r = object(w, this).resist_stamina_boost().unwrap_or(1.0);
            return r * f64::from(creature_rating::get_healing_rating_mod(w, this));
            // does healing rating affect these?
        }
        ResistanceType::StaminaDrain => {
            let r = object(w, this).resist_stamina_drain().unwrap_or(1.0);
            return r * f64::from(get_natural_resistance(w, this, DamageType::Stamina));
        }
        ResistanceType::ManaBoost => {
            let r = object(w, this).resist_mana_boost().unwrap_or(1.0);
            return r * f64::from(creature_rating::get_healing_rating_mod(w, this));
        }
        ResistanceType::ManaDrain => {
            let r = object(w, this).resist_mana_drain().unwrap_or(1.0);
            return r * f64::from(get_natural_resistance(w, this, DamageType::Mana));
        }
        _ => return 1.0,
    };
    let base = resist_property(object(w, this), damage_type).unwrap_or(1.0);
    base * f64::from(get_resistance_mod_damage(
        w,
        this,
        damage_type,
        attacker,
        weapon,
        weapon_resistance_mod,
    ))
}

/// `(Resist<Type> ?? 1.0) * EnchantmentManager.GetResistanceMod(DamageType.<Type>)`: the body of
/// the eight `Resist<Type>Mod` getters.
pub fn resist_mod(w: &mut World, this: ObjectGuid, damage_type: DamageType) -> f64 {
    let base = resist_property(object(w, this), damage_type).unwrap_or(1.0);
    base * f64::from(emc::get_resistance_mod(w, this, damage_type))
}

// ACE: Creature.ResistSlashMod
pub fn resist_slash_mod(w: &mut World, this: ObjectGuid) -> f64 {
    resist_mod(w, this, DamageType::Slash)
}

// ACE: Creature.ResistPierceMod
pub fn resist_pierce_mod(w: &mut World, this: ObjectGuid) -> f64 {
    resist_mod(w, this, DamageType::Pierce)
}

// ACE: Creature.ResistBludgeonMod
pub fn resist_bludgeon_mod(w: &mut World, this: ObjectGuid) -> f64 {
    resist_mod(w, this, DamageType::Bludgeon)
}

// ACE: Creature.ResistFireMod
pub fn resist_fire_mod(w: &mut World, this: ObjectGuid) -> f64 {
    resist_mod(w, this, DamageType::Fire)
}

// ACE: Creature.ResistColdMod
pub fn resist_cold_mod(w: &mut World, this: ObjectGuid) -> f64 {
    resist_mod(w, this, DamageType::Cold)
}

// ACE: Creature.ResistAcidMod
pub fn resist_acid_mod(w: &mut World, this: ObjectGuid) -> f64 {
    resist_mod(w, this, DamageType::Acid)
}

// ACE: Creature.ResistElectricMod
pub fn resist_electric_mod(w: &mut World, this: ObjectGuid) -> f64 {
    resist_mod(w, this, DamageType::Electric)
}

// ACE: Creature.ResistNetherMod
pub fn resist_nether_mod(w: &mut World, this: ObjectGuid) -> f64 {
    resist_mod(w, this, DamageType::Nether)
}

impl WorldObject {
    // ACE: Creature.Society
    /// `Faction1Bits ?? FactionBits.None`.
    #[must_use]
    pub fn society(&self) -> FactionBits {
        self.faction1_bits().unwrap_or(FactionBits::None)
    }
}
