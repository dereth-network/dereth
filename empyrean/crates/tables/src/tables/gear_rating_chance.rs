// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/GearRatingChance.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/GearRatingChance.cs`; do not edit by hand

//! The literal data of ACE's `GearRatingChance` (`Factories/Tables/GearRatingChance.cs`).

use crate::entity::ChanceTable;

/// ACE `GearRatingChance.RatingChance` (`ChanceTable<bool>`).
pub static RATING_CHANCE: ChanceTable<bool> = ChanceTable::new(&[
    (false, 0.75),
    (true, 0.25),
]);

/// ACE `GearRatingChance.ArmorRating` (`ChanceTable<int>`).
pub static ARMOR_RATING: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.95),
    (2, 0.05),
]);

/// ACE `GearRatingChance.ClothingJewelryRating` (`ChanceTable<int>`).
pub static CLOTHING_JEWELRY_RATING: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.70),
    (2, 0.25),
    (3, 0.05),
]);
