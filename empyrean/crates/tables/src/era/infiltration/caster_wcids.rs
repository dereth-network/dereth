// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/Weapons/CasterWcids.cs
// @generated from ClassicACE's `Source/ACE.Server/Factories/Tables/Wcids/Weapons/CasterWcids.cs`; do not edit by hand

//! The tables of ClassicACE's `CasterWcids` under its Infiltration ruleset
//! (`Factories/Tables/Wcids/Weapons/CasterWcids.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ClassicACE `CasterWcids.T1_T2_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T1_T2_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::orb, 1.0),
    (WeenieClassName::sceptre, 1.0),
    (WeenieClassName::staff, 1.0),
    (WeenieClassName::wand, 1.0),
]);

/// ClassicACE `CasterWcids.T3_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T3_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::orb, 4.0),
    (WeenieClassName::sceptre, 4.0),
    (WeenieClassName::staff, 4.0),
    (WeenieClassName::wand, 4.0),
    (WeenieClassName::wandslashing, 1.0),
    (WeenieClassName::wandpiercing, 1.0),
    (WeenieClassName::wandblunt, 1.0),
    (WeenieClassName::wandacid, 1.0),
    (WeenieClassName::wandfire, 1.0),
    (WeenieClassName::wandfrost, 1.0),
    (WeenieClassName::wandelectric, 1.0),
]);

/// ClassicACE `CasterWcids.T4_Chances` (`ChanceTable<WeenieClassName>`).
pub static T4_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::orb, 0.13),
    (WeenieClassName::sceptre, 0.13),
    (WeenieClassName::staff, 0.13),
    (WeenieClassName::wand, 0.13),
    (WeenieClassName::wandslashing, 0.03),
    (WeenieClassName::wandpiercing, 0.03),
    (WeenieClassName::wandblunt, 0.03),
    (WeenieClassName::wandacid, 0.03),
    (WeenieClassName::wandfire, 0.03),
    (WeenieClassName::wandfrost, 0.03),
    (WeenieClassName::wandelectric, 0.03),
    (WeenieClassName::ace43381_nethersceptre, 0.03),
    (WeenieClassName::ace31819_slashingbaton, 0.03),
    (WeenieClassName::ace31825_piercingbaton, 0.03),
    (WeenieClassName::ace31821_bluntbaton, 0.03),
    (WeenieClassName::ace31820_acidbaton, 0.03),
    (WeenieClassName::ace31823_firebaton, 0.03),
    (WeenieClassName::ace31824_frostbaton, 0.03),
    (WeenieClassName::ace31822_electricbaton, 0.03),
    (WeenieClassName::ace43382_netherbaton, 0.03),
]);

/// ClassicACE `CasterWcids.T5_T6_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T5_T6_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::orb, 1.0),
    (WeenieClassName::sceptre, 1.0),
    (WeenieClassName::staff, 1.0),
    (WeenieClassName::wand, 1.0),
    (WeenieClassName::wandslashing, 1.0),
    (WeenieClassName::wandpiercing, 1.0),
    (WeenieClassName::wandblunt, 1.0),
    (WeenieClassName::wandacid, 1.0),
    (WeenieClassName::wandfire, 1.0),
    (WeenieClassName::wandfrost, 1.0),
    (WeenieClassName::wandelectric, 1.0),
]);

/// ClassicACE `CasterWcids.T7_Chances` (`ChanceTable<WeenieClassName>`).
pub static T7_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::orb, 0.04),
    (WeenieClassName::sceptre, 0.04),
    (WeenieClassName::staff, 0.04),
    (WeenieClassName::wand, 0.04),
    (WeenieClassName::wandslashing, 0.045),
    (WeenieClassName::wandpiercing, 0.045),
    (WeenieClassName::wandblunt, 0.045),
    (WeenieClassName::wandacid, 0.045),
    (WeenieClassName::wandfire, 0.045),
    (WeenieClassName::wandfrost, 0.045),
    (WeenieClassName::wandelectric, 0.045),
    (WeenieClassName::ace43381_nethersceptre, 0.045),
    (WeenieClassName::ace31819_slashingbaton, 0.045),
    (WeenieClassName::ace31825_piercingbaton, 0.045),
    (WeenieClassName::ace31821_bluntbaton, 0.045),
    (WeenieClassName::ace31820_acidbaton, 0.045),
    (WeenieClassName::ace31823_firebaton, 0.045),
    (WeenieClassName::ace31824_frostbaton, 0.045),
    (WeenieClassName::ace31822_electricbaton, 0.045),
    (WeenieClassName::ace43382_netherbaton, 0.045),
    (WeenieClassName::ace37223_slashingstaff, 0.015),
    (WeenieClassName::ace37222_piercingstaff, 0.015),
    (WeenieClassName::ace37225_bluntstaff, 0.015),
    (WeenieClassName::ace37224_acidstaff, 0.015),
    (WeenieClassName::ace37220_firestaff, 0.015),
    (WeenieClassName::ace37221_froststaff, 0.015),
    (WeenieClassName::ace37219_electricstaff, 0.015),
    (WeenieClassName::ace43383_netherstaff, 0.015),
]);

/// ClassicACE `CasterWcids.T8_Chances` (`ChanceTable<WeenieClassName>`).
pub static T8_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::orb, 0.036),
    (WeenieClassName::sceptre, 0.036),
    (WeenieClassName::staff, 0.036),
    (WeenieClassName::wand, 0.036),
    (WeenieClassName::wandslashing, 0.036),
    (WeenieClassName::wandpiercing, 0.036),
    (WeenieClassName::wandblunt, 0.036),
    (WeenieClassName::wandacid, 0.036),
    (WeenieClassName::wandfire, 0.036),
    (WeenieClassName::wandfrost, 0.036),
    (WeenieClassName::wandelectric, 0.036),
    (WeenieClassName::ace43381_nethersceptre, 0.036),
    (WeenieClassName::ace31819_slashingbaton, 0.036),
    (WeenieClassName::ace31825_piercingbaton, 0.036),
    (WeenieClassName::ace31821_bluntbaton, 0.036),
    (WeenieClassName::ace31820_acidbaton, 0.036),
    (WeenieClassName::ace31823_firebaton, 0.036),
    (WeenieClassName::ace31824_frostbaton, 0.036),
    (WeenieClassName::ace31822_electricbaton, 0.036),
    (WeenieClassName::ace43382_netherbaton, 0.036),
    (WeenieClassName::ace37223_slashingstaff, 0.035),
    (WeenieClassName::ace37222_piercingstaff, 0.035),
    (WeenieClassName::ace37225_bluntstaff, 0.035),
    (WeenieClassName::ace37224_acidstaff, 0.035),
    (WeenieClassName::ace37220_firestaff, 0.035),
    (WeenieClassName::ace37221_froststaff, 0.035),
    (WeenieClassName::ace37219_electricstaff, 0.035),
    (WeenieClassName::ace43383_netherstaff, 0.035),
]);

/// ClassicACE `CasterWcids.casterTiers`, as its Infiltration ruleset sets it (`List<ChanceTable<WeenieClassName>>`).
pub static CASTER_TIERS: [&ChanceTable<WeenieClassName>; 8] = [
    &T1_T2_CHANCES,
    &T1_T2_CHANCES,
    &T3_CHANCES,
    &T3_CHANCES,
    &T5_T6_CHANCES,
    &T5_T6_CHANCES,
    &T5_T6_CHANCES,
    &T5_T6_CHANCES,
];
