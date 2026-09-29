// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/SocietyArmorWcids.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/Wcids/SocietyArmorWcids.cs`; do not edit by hand

//! The literal data of ACE's `SocietyArmorWcids` (`Factories/Tables/Wcids/SocietyArmorWcids.cs`).
//!
//! Runtime state, hand-ported in `crate::logic` (not literal here):
//! - `_combined` (`HashSet<WeenieClassName>`)

use crate::enums::WeenieClassName;

/// ACE `SocietyArmorWcids.CelestialHandWcids` (`List<WeenieClassName>`).
pub static CELESTIAL_HAND_WCIDS: [WeenieClassName; 9] = [
    WeenieClassName::ace38463_celestialhandbreastplate,
    WeenieClassName::ace38464_celestialhandgauntlets,
    WeenieClassName::ace38465_celestialhandgirth,
    WeenieClassName::ace38466_celestialhandgreaves,
    WeenieClassName::ace38467_celestialhandhelm,
    WeenieClassName::ace38468_celestialhandpauldrons,
    WeenieClassName::ace38469_celestialhandtassets,
    WeenieClassName::ace38470_celestialhandvambraces,
    WeenieClassName::ace38471_celestialhandsollerets,
];

/// ACE `SocietyArmorWcids.EldrytchWebWcids` (`List<WeenieClassName>`).
pub static ELDRYTCH_WEB_WCIDS: [WeenieClassName; 9] = [
    WeenieClassName::ace38472_eldrytchwebbreastplate,
    WeenieClassName::ace38473_eldrytchwebgauntlets,
    WeenieClassName::ace38474_eldrytchwebgirth,
    WeenieClassName::ace38475_eldrytchwebgreaves,
    WeenieClassName::ace38476_eldrytchwebhelm,
    WeenieClassName::ace38477_eldrytchwebpauldrons,
    WeenieClassName::ace38478_eldrytchwebtassets,
    WeenieClassName::ace38479_eldrytchwebvambraces,
    WeenieClassName::ace38480_eldrytchwebsollerets,
];

/// ACE `SocietyArmorWcids.RadiantBloodWcids` (`List<WeenieClassName>`).
pub static RADIANT_BLOOD_WCIDS: [WeenieClassName; 9] = [
    WeenieClassName::ace38481_radiantbloodbreastplate,
    WeenieClassName::ace38482_radiantbloodgauntlets,
    WeenieClassName::ace38483_radiantbloodgirth,
    WeenieClassName::ace38484_radiantbloodgreaves,
    WeenieClassName::ace38485_radiantbloodhelm,
    WeenieClassName::ace38486_radiantbloodpauldrons,
    WeenieClassName::ace38487_radiantbloodtassets,
    WeenieClassName::ace38488_radiantbloodvambraces,
    WeenieClassName::ace38489_radiantbloodsollerets,
];

/// ACE `SocietyArmorWcids.societyArmorTables` (`List<List<WeenieClassName>>`).
pub static SOCIETY_ARMOR_TABLES: [&[WeenieClassName]; 3] = [
    &CELESTIAL_HAND_WCIDS,
    &ELDRYTCH_WEB_WCIDS,
    &RADIANT_BLOOD_WCIDS,
];
