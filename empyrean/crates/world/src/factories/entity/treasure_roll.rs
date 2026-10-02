// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Entity/TreasureRoll.cs
//! Port of `Source/ACE.Server/Factories/Entity/TreasureRoll.cs`.

use empyrean_tables::enums::{
    TreasureArmorType, TreasureItemType, TreasureWeaponType, WeenieClassName,
};

use crate::world_objects::world_object::WorldObject;

/// ACE `TreasureRoll`: what lootgen rolled for one item (type, sub-type, wcid) and what it
/// accumulates while mutating it.
// ACE: TreasureRoll
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TreasureRoll {
    pub item_type: TreasureItemType,
    pub armor_type: TreasureArmorType,
    pub weapon_type: TreasureWeaponType,

    pub wcid: WeenieClassName,

    pub base_armor_level: i32,

    /// A cumulative addon to the ItemDifficulty / Arcane Lore requirement
    pub item_difficulty: f32,

    /// Not ACE: a weapon rolled from an earlier era's tables names the kind its mutation scripts
    /// are written for (`sword_ms`, `bow_short`, ...); `None` for ACE's.
    pub era_script: Option<&'static str>,
}

impl TreasureRoll {
    /// `new TreasureRoll()`; `new TreasureRoll(itemType)` is [`TreasureRoll::with_item_type`].
    // ACE: TreasureRoll.TreasureRoll
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `new TreasureRoll(TreasureItemType itemType)`.
    #[must_use]
    pub fn with_item_type(item_type: TreasureItemType) -> Self {
        Self {
            item_type,
            ..Self::default()
        }
    }

    /// The armor or weapon type for those rolls, else the item type (`Enum.ToString()`).
    // ACE: TreasureRoll.GetItemType
    #[must_use]
    pub fn get_item_type(&self) -> String {
        match self.item_type {
            TreasureItemType::Armor => self.armor_type.to_dotnet_string(),
            TreasureItemType::Weapon => self.weapon_type.to_dotnet_string(),
            _ => self.item_type.to_dotnet_string(),
        }
    }

    /// Returns TRUE if this roll is for a MeleeWeapon / MissileWeapon / Caster
    // ACE: TreasureRoll.IsWeapon
    #[must_use]
    pub fn is_weapon(&self) -> bool {
        self.weapon_type != TreasureWeaponType::Undef
    }

    // ACE: TreasureRoll.IsMeleeWeapon
    #[must_use]
    pub fn is_melee_weapon(&self) -> bool {
        self.weapon_type.is_melee_weapon()
    }

    // ACE: TreasureRoll.IsMissileWeapon
    #[must_use]
    pub fn is_missile_weapon(&self) -> bool {
        self.weapon_type.is_missile_weapon()
    }

    // ACE: TreasureRoll.IsCaster
    #[must_use]
    pub fn is_caster(&self) -> bool {
        self.weapon_type.is_caster()
    }

    /// Returns TRUE if this roll is for a piece of armor
    /// (clothing w/ armor level)
    // ACE: TreasureRoll.IsArmor
    #[must_use]
    pub fn is_armor(&self) -> bool {
        self.armor_type != TreasureArmorType::Undef
    }

    // ACE: TreasureRoll.IsClothing
    #[must_use]
    pub fn is_clothing(&self) -> bool {
        self.item_type == TreasureItemType::Clothing
    }

    // ACE: TreasureRoll.IsCloak
    #[must_use]
    pub fn is_cloak(&self) -> bool {
        self.item_type == TreasureItemType::Cloak
    }

    /// Returns TRUE if wo has an ArmorLevel > 0
    // ACE: TreasureRoll.HasArmorLevel
    #[must_use]
    pub fn has_armor_level(&self, wo: &WorldObject) -> bool {
        wo.armor_level().unwrap_or(0) > 0
    }

    // ACE: TreasureRoll.IsGem
    #[must_use]
    pub fn is_gem(&self) -> bool {
        self.item_type == TreasureItemType::Gem
    }

    // ACE: TreasureRoll.IsJewelry
    #[must_use]
    pub fn is_jewelry(&self) -> bool {
        self.item_type == TreasureItemType::Jewelry
    }

    // ACE: TreasureRoll.IsDinnerware
    #[must_use]
    pub fn is_dinnerware(&self) -> bool {
        self.item_type == TreasureItemType::ArtObject
    }
}
