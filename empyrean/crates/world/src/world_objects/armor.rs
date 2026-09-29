// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Armor.cs
//! Port of `Source/ACE.Server/WorldObjects/Armor.cs`: a player's armor level for one body part
//! (unused by ACE itself). `Player` and `ArmorClothing` are guids.

use empyrean_common::dotnet::{math, CsCast};
use empyrean_entity::enums::DamageType;
use empyrean_entity::ObjectGuid;
use empyrean_store::models::shard::BiotaPropertiesBodyPart;

use crate::world_objects::managers::enchantment_manager_with_caching as emc;
use crate::World;

// ACE: Armor
#[derive(Debug, Clone)]
pub struct Armor {
    pub player: ObjectGuid,
    pub armor_clothing: Option<ObjectGuid>,
    pub biota: BiotaPropertiesBodyPart,
}

impl Armor {
    // ACE: Armor.Armor
    #[must_use]
    pub fn new(
        player: ObjectGuid,
        armor_clothing: Option<ObjectGuid>,
        biota: BiotaPropertiesBodyPart,
    ) -> Self {
        Self {
            player,
            armor_clothing,
            biota,
        }
    }

    // ACE: Armor.EnchantmentManager
    /// `Player.EnchantmentManager` (TODO: differntiate between auras): the object whose
    /// enchantment registry the getters read.
    #[must_use]
    pub fn enchantment_manager(&self) -> ObjectGuid {
        self.player
    }

    // ACE: Armor.BaseArmorMod
    pub fn base_armor_mod(&self, w: &mut World) -> i32 {
        self.biota.base_armor + emc::get_armor_mod(w, self.enchantment_manager())
    }

    // ACE: Armor.ArmorVsSlash
    pub fn armor_vs_slash(&self, w: &mut World) -> i32 {
        self.get_armor_vs_type(w, DamageType::Slash, self.biota.armor_vs_slash)
    }

    // ACE: Armor.ArmorVsPierce
    pub fn armor_vs_pierce(&self, w: &mut World) -> i32 {
        self.get_armor_vs_type(w, DamageType::Pierce, self.biota.armor_vs_pierce)
    }

    // ACE: Armor.ArmorVsBludgeon
    pub fn armor_vs_bludgeon(&self, w: &mut World) -> i32 {
        self.get_armor_vs_type(w, DamageType::Bludgeon, self.biota.armor_vs_bludgeon)
    }

    // ACE: Armor.ArmorVsFire
    pub fn armor_vs_fire(&self, w: &mut World) -> i32 {
        self.get_armor_vs_type(w, DamageType::Fire, self.biota.armor_vs_fire)
    }

    // ACE: Armor.ArmorVsCold
    pub fn armor_vs_cold(&self, w: &mut World) -> i32 {
        self.get_armor_vs_type(w, DamageType::Cold, self.biota.armor_vs_cold)
    }

    // ACE: Armor.ArmorVsAcid
    pub fn armor_vs_acid(&self, w: &mut World) -> i32 {
        self.get_armor_vs_type(w, DamageType::Acid, self.biota.armor_vs_acid)
    }

    // ACE: Armor.ArmorVsElectric
    pub fn armor_vs_electric(&self, w: &mut World) -> i32 {
        self.get_armor_vs_type(w, DamageType::Electric, self.biota.armor_vs_electric)
    }

    // ACE: Armor.ArmorVsNether
    pub fn armor_vs_nether(&self, w: &mut World) -> i32 {
        self.get_armor_vs_type(w, DamageType::Nether, self.biota.armor_vs_nether)
    }

    // ACE: Armor.GetArmorVsType
    pub fn get_armor_vs_type(
        &self,
        w: &mut World,
        damage_type: DamageType,
        armor_vs_type: i32,
    ) -> i32 {
        let m = emc::get_resistance_mod(w, self.enchantment_manager(), damage_type);
        get_armor_vs_type_formula(
            self.biota.base_armor,
            self.base_armor_mod(w),
            m,
            armor_vs_type,
        )
    }
}

/// The arithmetic of `Armor.GetArmorVsType` once the enchantment reads are done: `base_armor` is
/// `Biota.BaseArmor`, `base_armor_mod` is `BaseArmorMod`, `resistance_mod` is
/// `EnchantmentManager.GetResistanceMod(damageType)`.
#[must_use]
pub fn get_armor_vs_type_formula(
    base_armor: i32,
    base_armor_mod: i32,
    resistance_mod: f32,
    armor_vs_type: i32,
) -> i32 {
    #[allow(clippy::cast_precision_loss)]
    let resistance = armor_vs_type as f32 / base_armor as f32;
    let m = resistance_mod;

    let mut resistance_mod = resistance / m;
    if base_armor_mod < 0 {
        resistance_mod = 1.0 + (1.0 - resistance_mod);
    }

    /*Console.WriteLine("BaseArmor: " + Biota.BaseArmor);
    Console.WriteLine("BaseArmorMod: " + baseArmorMod);
    Console.WriteLine("Resistance: " + resistance);
    Console.WriteLine("ResistanceMod: " + resistanceMod);*/

    #[allow(clippy::cast_precision_loss)]
    let product = base_armor_mod as f32 * resistance_mod;
    math::round(f64::from(product)).cs_cast()
}
