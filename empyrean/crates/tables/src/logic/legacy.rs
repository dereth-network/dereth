// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/*.cs

//! The methods of the `Factories/Tables/Wcids/Weapons/Legacy` classes (pre-MoA weapon tables):
//! per-heritage rolls and per-tier rolls.

macro_rules! heritage_class {
    ($module:ident, $class:literal, $aluvian:ident, $gharundim:ident, $sho:ident) => {
        #[doc = concat!("ACE `", $class, "`.")]
        pub mod $module {
            use crate::enums::{TreasureHeritageGroup, WeenieClassName};
            use crate::tables::wcids::weapons::legacy::$module::{$aluvian, $gharundim, $sho};

            /// The heritage's table rolled; `undef` for any other heritage.
            pub fn roll(heritage: TreasureHeritageGroup) -> WeenieClassName {
                match heritage {
                    TreasureHeritageGroup::Aluvian => $aluvian.roll(0.0),
                    TreasureHeritageGroup::Gharundim => $gharundim.roll(0.0),
                    TreasureHeritageGroup::Sho => $sho.roll(0.0),
                    _ => WeenieClassName::undef,
                }
            }
        }
    };
}

// ACE: AxeWcids.Roll
heritage_class!(
    axe_wcids,
    "AxeWcids",
    AXE_WCIDS_ALUVIAN,
    AXE_WCIDS_GHARUNDIM,
    AXE_WCIDS_SHO
);
// ACE: MaceWcids.Roll
heritage_class!(
    mace_wcids,
    "MaceWcids",
    MACE_WCIDS_ALUVIAN,
    MACE_WCIDS_GHARUNDIM,
    MACE_WCIDS_SHO
);
// ACE: SpearWcids.Roll
heritage_class!(
    spear_wcids,
    "SpearWcids",
    SPEAR_WCIDS_ALUVIAN,
    SPEAR_WCIDS_GHARUNDIM,
    SPEAR_WCIDS_SHO
);
// ACE: StaffWcids.Roll
heritage_class!(
    staff_wcids,
    "StaffWcids",
    STAFF_WCIDS_ALUVIAN,
    STAFF_WCIDS_GHARUNDIM,
    STAFF_WCIDS_SHO
);
// ACE: UnarmedWcids.Roll
heritage_class!(
    unarmed_wcids,
    "UnarmedWcids",
    UNARMED_WCIDS_ALUVIAN,
    UNARMED_WCIDS_GHARUNDIM,
    UNARMED_WCIDS_SHO
);

macro_rules! tier_class {
    ($module:ident, $class:literal) => {
        #[doc = concat!("ACE `", $class, "`.")]
        pub mod $module {
            use crate::enums::WeenieClassName;
            use crate::logic::at;
            use crate::tables::wcids::weapons::legacy::$module::WEAPON_TIERS;

            /// The tier (clamped to 1-6) table rolled.
            pub fn roll(tier: i32) -> WeenieClassName {
                let tier = tier.clamp(1, 6);
                at(&WEAPON_TIERS, tier - 1).roll(0.0)
            }
        }
    };
}

// ACE: DaggerWcids_Aluvian_Sho.Roll
tier_class!(dagger_wcids_aluvian_sho, "DaggerWcids_Aluvian_Sho");
// ACE: DaggerWcids_Gharundim.Roll
tier_class!(dagger_wcids_gharundim, "DaggerWcids_Gharundim");
// ACE: SwordWcids_Aluvian.Roll
tier_class!(sword_wcids_aluvian, "SwordWcids_Aluvian");
// ACE: SwordWcids_Gharundim.Roll
tier_class!(sword_wcids_gharundim, "SwordWcids_Gharundim");
// ACE: SwordWcids_Sho.Roll
tier_class!(sword_wcids_sho, "SwordWcids_Sho");
