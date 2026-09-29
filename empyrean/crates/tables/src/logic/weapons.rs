// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/Weapons/*.cs

//! The methods of the `Factories/Tables/Wcids/Weapons` classes: per-tier rolls, the
//! weapon-type rolls of the heavy/light/finesse/two-handed tables, and the wcid -> weapon type
//! lookups their static constructors build.

/// The static constructor shared by the per-tier missile classes: every wcid of every tier maps
/// to one weapon type (`TryAdd`: first wins).
fn build_tier_lookup(
    tiers: &[&crate::entity::ChanceTable<crate::enums::WeenieClassName>],
    weapon_type: crate::enums::TreasureWeaponType,
) -> std::collections::HashMap<crate::enums::WeenieClassName, crate::enums::TreasureWeaponType> {
    let mut combined = std::collections::HashMap::new();
    for tier in tiers {
        for entry in tier.entries() {
            combined.entry(entry.0).or_insert(weapon_type);
        }
    }
    combined
}

/// The static constructor shared by the heavy/light/finesse/two-handed classes: every wcid maps
/// to the weapon type of the first table it appears in.
fn build_table_lookup(
    tables: &[(
        &crate::entity::ChanceTable<crate::enums::WeenieClassName>,
        crate::enums::TreasureWeaponType,
    )],
) -> std::collections::HashMap<crate::enums::WeenieClassName, crate::enums::TreasureWeaponType> {
    let mut combined = std::collections::HashMap::new();
    for (table, weapon_type) in tables {
        for wcid in table.entries() {
            combined.entry(wcid.0).or_insert(*weapon_type);
        }
    }
    combined
}

macro_rules! tier_class {
    ($module:ident, $class:literal, $data:ident, $tiers:ident, $weapon_type:ident,
     $ctor:ident, $roll:literal, $ctor_anchor:literal, $lookup:literal) => {
        #[doc = concat!("ACE `", $class, "`.")]
        pub mod $module {
            use std::collections::HashMap;
            use std::sync::LazyLock;

            use crate::enums::{TreasureWeaponType, WeenieClassName};
            use crate::logic::at;
            use crate::tables::wcids::weapons::$data::$tiers;

            #[doc = concat!("ACE `", $roll, "`.")]
            pub fn roll(tier: i32) -> WeenieClassName {
                at(&$tiers, tier - 1).roll(0.0)
            }

            static COMBINED: LazyLock<HashMap<WeenieClassName, TreasureWeaponType>> =
                LazyLock::new($ctor);

            #[doc = concat!("ACE `", $ctor_anchor, "` (the static constructor).")]
            fn $ctor() -> HashMap<WeenieClassName, TreasureWeaponType> {
                super::build_tier_lookup(&$tiers, TreasureWeaponType::$weapon_type)
            }

            #[doc = concat!("ACE `", $lookup, "`.")]
            pub fn try_get_value(wcid: WeenieClassName) -> Option<TreasureWeaponType> {
                COMBINED.get(&wcid).copied()
            }
        }
    };
}

// ACE: AtlatlWcids.Roll, AtlatlWcids.AtlatlWcids, AtlatlWcids.TryGetValue
tier_class!(
    atlatl_wcids,
    "AtlatlWcids",
    atlatl_wcids,
    ATLATL_TIERS,
    Atlatl,
    atlatl_wcids,
    "AtlatlWcids.Roll",
    "AtlatlWcids.AtlatlWcids",
    "AtlatlWcids.TryGetValue"
);
// ACE: BowWcids_Aluvian.Roll, BowWcids_Aluvian.BowWcids_Aluvian, BowWcids_Aluvian.TryGetValue
tier_class!(
    bow_wcids_aluvian,
    "BowWcids_Aluvian",
    bow_wcids_aluvian,
    BOW_TIERS,
    Bow,
    bow_wcids_aluvian,
    "BowWcids_Aluvian.Roll",
    "BowWcids_Aluvian.BowWcids_Aluvian",
    "BowWcids_Aluvian.TryGetValue"
);
// ACE: BowWcids_Gharundim.Roll, BowWcids_Gharundim.BowWcids_Gharundim, BowWcids_Gharundim.TryGetValue
tier_class!(
    bow_wcids_gharundim,
    "BowWcids_Gharundim",
    bow_wcids_gharundim,
    BOW_TIERS,
    Bow,
    bow_wcids_gharundim,
    "BowWcids_Gharundim.Roll",
    "BowWcids_Gharundim.BowWcids_Gharundim",
    "BowWcids_Gharundim.TryGetValue"
);
// ACE: BowWcids_Sho.Roll, BowWcids_Sho.BowWcids_Sho, BowWcids_Sho.TryGetValue
tier_class!(
    bow_wcids_sho,
    "BowWcids_Sho",
    bow_wcids_sho,
    BOW_TIERS,
    Bow,
    bow_wcids_sho,
    "BowWcids_Sho.Roll",
    "BowWcids_Sho.BowWcids_Sho",
    "BowWcids_Sho.TryGetValue"
);
// ACE: CrossbowWcids.Roll, CrossbowWcids.CrossbowWcids, CrossbowWcids.TryGetValue
tier_class!(
    crossbow_wcids,
    "CrossbowWcids",
    crossbow_wcids,
    CROSSBOW_TIERS,
    Crossbow,
    crossbow_wcids,
    "CrossbowWcids.Roll",
    "CrossbowWcids.CrossbowWcids",
    "CrossbowWcids.TryGetValue"
);

/// ACE `CasterWcids`.
pub mod caster_wcids {
    use std::collections::HashSet;
    use std::sync::LazyLock;

    use crate::enums::WeenieClassName;
    use crate::logic::at;
    use crate::tables::wcids::weapons::caster_wcids::CASTER_TIERS;

    // ACE: CasterWcids.Roll
    pub fn roll(tier: i32) -> WeenieClassName {
        at(&CASTER_TIERS, tier - 1).roll(0.0)
    }

    static COMBINED: LazyLock<HashSet<WeenieClassName>> = LazyLock::new(caster_wcids);

    // ACE: CasterWcids.CasterWcids
    fn caster_wcids() -> HashSet<WeenieClassName> {
        let mut combined = HashSet::new();
        for caster_tier in CASTER_TIERS {
            for entry in caster_tier.entries() {
                combined.insert(entry.0);
            }
        }
        combined
    }

    // ACE: CasterWcids.Contains
    pub fn contains(wcid: WeenieClassName) -> bool {
        COMBINED.contains(&wcid)
    }
}

macro_rules! typed_class {
    ($module:ident, $class:literal, $tables:ident, $ctor:ident,
     $roll:literal, $ctor_anchor:literal, $lookup:literal) => {
        #[doc = concat!("ACE `", $class, "`.")]
        pub mod $module {
            use std::collections::HashMap;
            use std::sync::LazyLock;

            use crate::enums::{TreasureWeaponType, WeenieClassName};
            use crate::logic::{at, count};
            use crate::rng;
            use crate::tables::wcids::weapons::$module::$tables;

            #[doc = concat!("ACE `", $roll, "`: an even pick of weapon table, then its roll; ")]
            /// `weapon_type` is ACE's `out` parameter.
            pub fn roll(weapon_type: &mut TreasureWeaponType) -> WeenieClassName {
                let weapon_table = at(&$tables, rng::next_int(0, count(&$tables) - 1));
                *weapon_type = weapon_table.1;
                weapon_table.0.roll(0.0)
            }

            static COMBINED: LazyLock<HashMap<WeenieClassName, TreasureWeaponType>> =
                LazyLock::new($ctor);

            #[doc = concat!("ACE `", $ctor_anchor, "` (the static constructor).")]
            fn $ctor() -> HashMap<WeenieClassName, TreasureWeaponType> {
                super::build_table_lookup(&$tables)
            }

            #[doc = concat!("ACE `", $lookup, "`.")]
            pub fn try_get_value(wcid: WeenieClassName) -> Option<TreasureWeaponType> {
                COMBINED.get(&wcid).copied()
            }
        }
    };
}

// ACE: FinesseWeaponWcids.Roll, FinesseWeaponWcids.FinesseWeaponWcids, FinesseWeaponWcids.TryGetValue
typed_class!(
    finesse_weapon_wcids,
    "FinesseWeaponWcids",
    FINESSE_WEAPONS_TABLES,
    finesse_weapon_wcids,
    "FinesseWeaponWcids.Roll",
    "FinesseWeaponWcids.FinesseWeaponWcids",
    "FinesseWeaponWcids.TryGetValue"
);
// ACE: HeavyWeaponWcids.Roll, HeavyWeaponWcids.HeavyWeaponWcids, HeavyWeaponWcids.TryGetValue
typed_class!(
    heavy_weapon_wcids,
    "HeavyWeaponWcids",
    HEAVY_WEAPONS_TABLES,
    heavy_weapon_wcids,
    "HeavyWeaponWcids.Roll",
    "HeavyWeaponWcids.HeavyWeaponWcids",
    "HeavyWeaponWcids.TryGetValue"
);
// ACE: LightWeaponWcids.Roll, LightWeaponWcids.LightWeaponWcids, LightWeaponWcids.TryGetValue
typed_class!(
    light_weapon_wcids,
    "LightWeaponWcids",
    LIGHT_WEAPONS_TABLES,
    light_weapon_wcids,
    "LightWeaponWcids.Roll",
    "LightWeaponWcids.LightWeaponWcids",
    "LightWeaponWcids.TryGetValue"
);
// ACE: TwoHandedWeaponWcids.Roll, TwoHandedWeaponWcids.TwoHandedWeaponWcids, TwoHandedWeaponWcids.TryGetValue
typed_class!(
    two_handed_weapon_wcids,
    "TwoHandedWeaponWcids",
    TWO_HANDED_WEAPON_TABLES,
    two_handed_weapon_wcids,
    "TwoHandedWeaponWcids.Roll",
    "TwoHandedWeaponWcids.TwoHandedWeaponWcids",
    "TwoHandedWeaponWcids.TryGetValue"
);
