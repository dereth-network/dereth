// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/SpellComponentWcids.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/Wcids/SpellComponentWcids.cs`; do not edit by hand

//! The literal data of ACE's `SpellComponentWcids` (`Factories/Tables/Wcids/SpellComponentWcids.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ACE `SpellComponentWcids.T1_Chances` (`ChanceTable<WeenieClassName>`).
pub static T1_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::peascarablead, 1.00),
]);

/// ACE `SpellComponentWcids.T2_Chances` (`ChanceTable<WeenieClassName>`).
pub static T2_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::peascarablead, 0.50),
    (WeenieClassName::peascarabiron, 0.50),
]);

/// ACE `SpellComponentWcids.T3_Chances` (`ChanceTable<WeenieClassName>`).
pub static T3_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::peascarablead, 0.25),
    (WeenieClassName::peascarabiron, 0.50),
    (WeenieClassName::peascarabcopper, 0.25),
]);

/// ACE `SpellComponentWcids.T4_Chances` (`ChanceTable<WeenieClassName>`).
pub static T4_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::peascarabiron, 0.25),
    (WeenieClassName::peascarabcopper, 0.50),
    (WeenieClassName::peascarabsilver, 0.25),
]);

/// ACE `SpellComponentWcids.T5_Chances` (`ChanceTable<WeenieClassName>`).
pub static T5_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::peascarabcopper, 0.25),
    (WeenieClassName::peascarabsilver, 0.50),
    (WeenieClassName::peascarabgold, 0.25),
]);

/// ACE `SpellComponentWcids.T6_T8_Chances` (`ChanceTable<WeenieClassName>`).
pub static T6_T8_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::peascarabsilver, 0.25),
    (WeenieClassName::peascarabgold, 0.50),
    (WeenieClassName::peascarabpyreal, 0.25),
]);

/// ACE `SpellComponentWcids.peaTiers` (`List<ChanceTable<WeenieClassName>>`).
pub static PEA_TIERS: [&ChanceTable<WeenieClassName>; 8] = [
    &T1_CHANCES,
    &T2_CHANCES,
    &T3_CHANCES,
    &T4_CHANCES,
    &T5_CHANCES,
    &T6_T8_CHANCES,
    &T6_T8_CHANCES,
    &T6_T8_CHANCES,
];

/// ACE `SpellComponentWcids.level8SpellComponentChance` (`ChanceTable<bool>`).
pub static LEVEL8_SPELL_COMPONENT_CHANCE: ChanceTable<bool> = ChanceTable::new(&[
    (false, 0.6),
    (true, 0.4),
]);

/// ACE `SpellComponentWcids.Quills` (`ChanceTable<WeenieClassName>`).
pub static QUILLS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace37363_quillofinfliction, 0.50),
    (WeenieClassName::ace37364_quillofintrospection, 0.35),
    (WeenieClassName::ace37365_quillofbenevolence, 0.10),
    (WeenieClassName::ace37362_quillofextraction, 0.05),
]);

/// ACE `SpellComponentWcids.Inks` (`ChanceTable<WeenieClassName>`).
pub static INKS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace37353_inkofformation, 0.30),
    (WeenieClassName::ace37360_inkofconveyance, 0.24),
    (WeenieClassName::ace37355_inkofobjectification, 0.10),
    (WeenieClassName::ace37354_inkofnullification, 0.06),
    (WeenieClassName::ace37356_parabolicink, 0.06),
    (WeenieClassName::ace37357_inkofpartition, 0.06),
    (WeenieClassName::ace37358_inkofseparation, 0.06),
    (WeenieClassName::ace37359_alacritousink, 0.06),
    (WeenieClassName::ace37361_inkofdirection, 0.06),
]);

/// ACE `SpellComponentWcids.Glyphs` (`List<WeenieClassName>`).
pub static GLYPHS: [WeenieClassName; 60] = [
    WeenieClassName::ace37343_glyphofalchemy,
    WeenieClassName::ace37344_glyphofarcanelore,
    WeenieClassName::ace37345_glyphofarmor,
    WeenieClassName::ace37346_glyphofarmortinkering,
    WeenieClassName::ace37347_glyphofbludgeoning,
    WeenieClassName::ace37349_glyphofcooking,
    WeenieClassName::ace37350_glyphofcoordination,
    WeenieClassName::ace37342_glyphofcorrosion,
    WeenieClassName::ace37351_glyphofcreatureenchantment,
    WeenieClassName::ace43379_glyphofdamage,
    WeenieClassName::ace37352_glyphofdeception,
    WeenieClassName::ace45370_glyphofdirtyfighting,
    WeenieClassName::ace45371_glyphofdualwield,
    WeenieClassName::ace37300_glyphofendurance,
    WeenieClassName::ace37373_glyphoffinesseweapons,
    WeenieClassName::ace37301_glyphofflame,
    WeenieClassName::ace37302_glyphoffletching,
    WeenieClassName::ace37303_glyphoffocus,
    WeenieClassName::ace37348_glyphoffrost,
    WeenieClassName::ace37304_glyphofhealing,
    WeenieClassName::ace37305_glyphofhealth,
    WeenieClassName::ace37369_glyphofheavyweapons,
    WeenieClassName::ace37309_glyphofitemenchantment,
    WeenieClassName::ace37310_glyphofitemtinkering,
    WeenieClassName::ace37311_glyphofjump,
    WeenieClassName::ace37312_glyphofleadership,
    WeenieClassName::ace37313_glyphoflifemagic,
    WeenieClassName::ace37339_glyphoflightweapons,
    WeenieClassName::ace37314_glyphoflightning,
    WeenieClassName::ace37315_glyphoflockpick,
    WeenieClassName::ace37316_glyphofloyalty,
    WeenieClassName::ace37317_glyphofmagicdefense,
    WeenieClassName::ace38760_glyphofmagicitemtinkering,
    WeenieClassName::ace37318_glyphofmana,
    WeenieClassName::ace37319_glyphofmanaconversion,
    WeenieClassName::ace37321_glyphofmanaregeneration,
    WeenieClassName::ace37323_glyphofmeleedefense,
    WeenieClassName::ace37324_glyphofmissiledefense,
    WeenieClassName::ace37338_glyphofmissileweapons,
    WeenieClassName::ace37325_glyphofmonsterappraisal,
    WeenieClassName::ace43387_glyphofnether,
    WeenieClassName::ace37326_glyphofpersonappraisal,
    WeenieClassName::ace37327_glyphofpiercing,
    WeenieClassName::ace37328_glyphofquickness,
    WeenieClassName::ace45372_glyphofrecklessness,
    WeenieClassName::ace37307_glyphofregeneration,
    WeenieClassName::ace37329_glyphofrun,
    WeenieClassName::ace37330_glyphofsalvaging,
    WeenieClassName::ace37331_glyphofself,
    WeenieClassName::ace45373_glyphofshield,
    WeenieClassName::ace37332_glyphofslashing,
    WeenieClassName::ace45374_glyphofsneakattack,
    WeenieClassName::ace37333_glyphofstamina,
    WeenieClassName::ace37336_glyphofstaminaregeneration,
    WeenieClassName::ace37337_glyphofstrength,
    WeenieClassName::ace49455_glyphofsummoning,
    WeenieClassName::ace41747_glyphoftwohandedcombat,
    WeenieClassName::ace43380_glyphofvoidmagic,
    WeenieClassName::ace37340_glyphofwarmagic,
    WeenieClassName::ace37341_glyphofweapontinkering,
];
