// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/TreasureProfile_Mundane.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/TreasureProfile_Mundane.cs`; do not edit by hand

//! The literal data of ACE's `TreasureProfile_Mundane` (`Factories/Tables/TreasureProfile_Mundane.cs`).

use crate::entity::ChanceTable;
use crate::enums::TreasureItemType;

/// ACE `TreasureProfile_Mundane.mundaneProfile1` (`ChanceTable<TreasureItemType>`).
pub static MUNDANE_PROFILE1: ChanceTable<TreasureItemType> = ChanceTable::new(&[
    (TreasureItemType::Consumable, 1.0),
]);

/// ACE `TreasureProfile_Mundane.mundaneProfile2` (`ChanceTable<TreasureItemType>`).
pub static MUNDANE_PROFILE2: ChanceTable<TreasureItemType> = ChanceTable::new(&[
    (TreasureItemType::HealKit, 1.0),
]);

/// ACE `TreasureProfile_Mundane.mundaneProfile3` (`ChanceTable<TreasureItemType>`).
pub static MUNDANE_PROFILE3: ChanceTable<TreasureItemType> = ChanceTable::new(&[
    (TreasureItemType::Lockpick, 1.0),
]);

/// ACE `TreasureProfile_Mundane.mundaneProfile4` (`ChanceTable<TreasureItemType>`).
pub static MUNDANE_PROFILE4: ChanceTable<TreasureItemType> = ChanceTable::new(&[
    (TreasureItemType::SpellComponent, 1.0),
]);

/// ACE `TreasureProfile_Mundane.mundaneProfile5` (`ChanceTable<TreasureItemType>`).
pub static MUNDANE_PROFILE5: ChanceTable<TreasureItemType> = ChanceTable::new(&[
    (TreasureItemType::ManaStone, 1.0),
]);

/// ACE `TreasureProfile_Mundane.mundaneProfile6` (`ChanceTable<TreasureItemType>`).
pub static MUNDANE_PROFILE6: ChanceTable<TreasureItemType> = ChanceTable::new(&[
    (TreasureItemType::Pyreal, 1.0),
]);

/// ACE `TreasureProfile_Mundane.mundaneProfile7` (`ChanceTable<TreasureItemType>`).
pub static MUNDANE_PROFILE7: ChanceTable<TreasureItemType> = ChanceTable::new(&[
    (TreasureItemType::Pyreal, 0.17),
    (TreasureItemType::Consumable, 0.17),
    (TreasureItemType::HealKit, 0.16),
    (TreasureItemType::Lockpick, 0.16),
    (TreasureItemType::SpellComponent, 0.17),
    (TreasureItemType::ManaStone, 0.17),
]);

/// ACE `TreasureProfile_Mundane.mundaneProfile8` (`ChanceTable<TreasureItemType>`).
pub static MUNDANE_PROFILE8: ChanceTable<TreasureItemType> = ChanceTable::new(&[
    (TreasureItemType::Pyreal, 0.34),
    (TreasureItemType::SpellComponent, 0.33),
    (TreasureItemType::ManaStone, 0.33),
]);

/// ACE `TreasureProfile_Mundane.mundaneProfiles` (`List<ChanceTable<TreasureItemType>>`).
pub static MUNDANE_PROFILES: [&ChanceTable<TreasureItemType>; 8] = [
    &MUNDANE_PROFILE1,
    &MUNDANE_PROFILE2,
    &MUNDANE_PROFILE3,
    &MUNDANE_PROFILE4,
    &MUNDANE_PROFILE5,
    &MUNDANE_PROFILE6,
    &MUNDANE_PROFILE7,
    &MUNDANE_PROFILE8,
];
