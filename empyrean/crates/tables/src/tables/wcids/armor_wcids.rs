// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/ArmorWcids.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/Wcids/ArmorWcids.cs`; do not edit by hand

//! The literal data of ACE's `ArmorWcids` (`Factories/Tables/Wcids/ArmorWcids.cs`).
//!
//! Runtime state, hand-ported in `crate::logic` (not literal here):
//! - `_combined` (`Dictionary<WeenieClassName, TreasureArmorType>`)

use crate::entity::ChanceTable;
use crate::enums::{TreasureArmorType, WeenieClassName};

/// ACE `ArmorWcids.LeatherWcids` (`ChanceTable<WeenieClassName>`).
pub static LEATHER_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::buckler, 0.07),
    (WeenieClassName::capleather, 0.02),
    (WeenieClassName::cowlleathernew, 0.02),
    (WeenieClassName::basinetleathernew, 0.03),
    (WeenieClassName::bootsleathernew, 0.06),
    (WeenieClassName::bracersleathernew, 0.06),
    (WeenieClassName::breastplateleathernew, 0.06),
    (WeenieClassName::coatleathernew, 0.03),
    (WeenieClassName::cuirassleathernew, 0.06),
    (WeenieClassName::gauntletsleathernew, 0.06),
    (WeenieClassName::girthleathernew, 0.06),
    (WeenieClassName::greavesleathernew, 0.05),
    (WeenieClassName::leggingsleathernew, 0.05),
    (WeenieClassName::longgauntletsleathernew, 0.06),
    (WeenieClassName::pantsleathernew, 0.05),
    (WeenieClassName::pauldronsleathernew, 0.06),
    (WeenieClassName::shirtleathernew, 0.04),
    (WeenieClassName::shortsleathernew, 0.05),
    (WeenieClassName::sleevesleathernew, 0.06),
    (WeenieClassName::tassetsleathernew, 0.05),
]);

/// ACE `ArmorWcids.StuddedLeatherWcids` (`ChanceTable<WeenieClassName>`).
pub static STUDDED_LEATHER_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::shieldkite, 0.04),
    (WeenieClassName::shieldround, 0.04),
    (WeenieClassName::cowlstuddedleather, 0.04),
    (WeenieClassName::basinetstuddedleather, 0.04),
    (WeenieClassName::bootsreinforcedleather, 0.08),
    (WeenieClassName::bracersstuddedleather, 0.07),
    (WeenieClassName::breastplatestuddedleather, 0.07),
    (WeenieClassName::coatstuddedleather, 0.03),
    (WeenieClassName::cuirassstuddedleather, 0.06),
    (WeenieClassName::gauntletsstuddedleather, 0.08),
    (WeenieClassName::girthstuddedleather, 0.07),
    (WeenieClassName::greavesstuddedleather, 0.07),
    (WeenieClassName::leggingsstuddedleather, 0.07),
    (WeenieClassName::pauldronsstuddedleather, 0.07),
    (WeenieClassName::shirtstuddedleather, 0.04),
    (WeenieClassName::sleevesstuddedleather, 0.06),
    (WeenieClassName::tassetsstuddedleather, 0.07),
]);

/// ACE `ArmorWcids.ChainmailWcids` (`ChanceTable<WeenieClassName>`).
pub static CHAINMAIL_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::shieldkitelarge, 0.04),
    (WeenieClassName::shieldroundlarge, 0.04),
    (WeenieClassName::capmetal, 0.02),
    (WeenieClassName::mailcoif, 0.02),
    (WeenieClassName::coifscale, 0.02),
    (WeenieClassName::basinetchainmail, 0.02),
    (WeenieClassName::bootssteeltoe, 0.08),
    (WeenieClassName::bracerschainmail, 0.07),
    (WeenieClassName::breastplatechainmail, 0.07),
    (WeenieClassName::hauberkchainmail, 0.05),
    (WeenieClassName::gauntletschainmail, 0.08),
    (WeenieClassName::girthchainmail, 0.07),
    (WeenieClassName::greaveschainmail, 0.07),
    (WeenieClassName::leggingschainmail, 0.08),
    (WeenieClassName::pauldronschainmail, 0.08),
    (WeenieClassName::shirtchainmail, 0.05),
    (WeenieClassName::sleeveschainmail, 0.06),
    (WeenieClassName::tassetschainmail, 0.08),
]);

/// ACE `ArmorWcids.PlatemailWcids` (`ChanceTable<WeenieClassName>`).
pub static PLATEMAIL_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::shieldtower, 0.08),
    (WeenieClassName::helmhorned, 0.02),
    (WeenieClassName::helmet, 0.02),
    (WeenieClassName::armet, 0.02),
    (WeenieClassName::heaumenew, 0.02),
    (WeenieClassName::sollerets, 0.08),
    (WeenieClassName::vambracesplatemail, 0.06),
    (WeenieClassName::breastplateplatemail, 0.08),
    (WeenieClassName::cuirassplatemail, 0.08),
    (WeenieClassName::gauntletsplatemail, 0.08),
    (WeenieClassName::girthplatemail, 0.05),
    (WeenieClassName::greavesplatemail, 0.07),
    (WeenieClassName::tassetsplatemail, 0.07),
    (WeenieClassName::hauberkplatemail, 0.06),
    (WeenieClassName::leggingsplatemail, 0.08),
    (WeenieClassName::pauldronsplatemail, 0.05),
    (WeenieClassName::sleevesplatemail, 0.08),
]);

/// ACE `ArmorWcids.ScalemailWcids` (`ChanceTable<WeenieClassName>`).
pub static SCALEMAIL_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::shieldtower, 0.07),
    (WeenieClassName::baigha, 0.02),
    (WeenieClassName::helmet, 0.02),
    (WeenieClassName::armet, 0.02),
    (WeenieClassName::heaumenew, 0.02),
    (WeenieClassName::basinetscalemail, 0.01),
    (WeenieClassName::sollerets, 0.07),
    (WeenieClassName::bracersscalemail, 0.07),
    (WeenieClassName::breastplatescalemail, 0.06),
    (WeenieClassName::cuirassscalemail, 0.06),
    (WeenieClassName::gauntletsscalemail, 0.07),
    (WeenieClassName::girthscalemail, 0.06),
    (WeenieClassName::greavesscalemail, 0.07),
    (WeenieClassName::tassetsscalemail, 0.07),
    (WeenieClassName::hauberkscalemail, 0.06),
    (WeenieClassName::leggingsscalemail, 0.07),
    (WeenieClassName::pauldronsscalemail, 0.06),
    (WeenieClassName::sleevesscalemail, 0.06),
    (WeenieClassName::shirtscalemail, 0.06),
]);

/// ACE `ArmorWcids.YoroiWcids` (`ChanceTable<WeenieClassName>`).
pub static YOROI_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::shieldtower, 0.08),
    (WeenieClassName::kabuton, 0.02),
    (WeenieClassName::helmet, 0.02),
    (WeenieClassName::armet, 0.02),
    (WeenieClassName::heaumenew, 0.02),
    (WeenieClassName::sollerets, 0.08),
    (WeenieClassName::kote, 0.08),
    (WeenieClassName::breastplateyoroi, 0.08),
    (WeenieClassName::cuirassyoroi, 0.08),
    (WeenieClassName::gauntletsplatemail, 0.06),
    (WeenieClassName::girthyoroi, 0.08),
    (WeenieClassName::greavesyoroi, 0.08),
    (WeenieClassName::tassetsyoroi, 0.08),
    (WeenieClassName::leggingsyoroi, 0.07),
    (WeenieClassName::pauldronsyoroi, 0.08),
    (WeenieClassName::sleevesyoroi, 0.07),
]);

/// ACE `ArmorWcids.CeldonWcids` (`ChanceTable<WeenieClassName>`).
pub static CELDON_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::girthceldon, 0.25),
    (WeenieClassName::breastplateceldon, 0.25),
    (WeenieClassName::leggingsceldon, 0.25),
    (WeenieClassName::sleevesceldon, 0.25),
]);

/// ACE `ArmorWcids.AmuliWcids` (`ChanceTable<WeenieClassName>`).
pub static AMULI_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::coatamullian, 0.50),
    (WeenieClassName::leggingsamullian, 0.50),
]);

/// ACE `ArmorWcids.KoujiaWcids` (`ChanceTable<WeenieClassName>`).
pub static KOUJIA_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::breastplatekoujia, 0.33),
    (WeenieClassName::leggingskoujia, 0.34),
    (WeenieClassName::sleeveskoujia, 0.33),
]);

/// ACE `ArmorWcids.CovenantWcids` (`ChanceTable<WeenieClassName>`).
pub static COVENANT_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::shieldcovenant, 0.10),
    (WeenieClassName::helmcovenant, 0.10),
    (WeenieClassName::gauntletscovenant, 0.10),
    (WeenieClassName::bracerscovenant, 0.10),
    (WeenieClassName::pauldronscovenant, 0.10),
    (WeenieClassName::breastplatecovenant, 0.10),
    (WeenieClassName::girthcovenant, 0.10),
    (WeenieClassName::tassetscovenant, 0.10),
    (WeenieClassName::greavescovenant, 0.10),
    (WeenieClassName::bootscovenant, 0.10),
]);

/// ACE `ArmorWcids.LoricaWcids` (`ChanceTable<WeenieClassName>`).
pub static LORICA_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::bootslorica, 0.16),
    (WeenieClassName::gauntletslorica, 0.16),
    (WeenieClassName::helmlorica, 0.17),
    (WeenieClassName::breastplatelorica, 0.17),
    (WeenieClassName::leggingslorica, 0.17),
    (WeenieClassName::sleeveslorica, 0.17),
]);

/// ACE `ArmorWcids.NariyidWcids` (`ChanceTable<WeenieClassName>`).
pub static NARIYID_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::bootsnariyid, 0.14),
    (WeenieClassName::gauntletsnariyid, 0.14),
    (WeenieClassName::helmnariyid, 0.14),
    (WeenieClassName::breastplatenariyid, 0.14),
    (WeenieClassName::girthnariyid, 0.14),
    (WeenieClassName::leggingsnariyid, 0.15),
    (WeenieClassName::sleevesnariyid, 0.15),
]);

/// ACE `ArmorWcids.ChiranWcids` (`ChanceTable<WeenieClassName>`).
pub static CHIRAN_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::sandalschiran, 0.20),
    (WeenieClassName::gauntletschiran, 0.20),
    (WeenieClassName::helmchiran, 0.20),
    (WeenieClassName::coatchiran, 0.20),
    (WeenieClassName::leggingschiran, 0.20),
]);

/// ACE `ArmorWcids.DiforsaWcids` (`ChanceTable<WeenieClassName>`).
pub static DIFORSA_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::shieldtower, 0.08),
    (WeenieClassName::helmdiforsa, 0.02),
    (WeenieClassName::helmet, 0.02),
    (WeenieClassName::armet, 0.02),
    (WeenieClassName::heaumenew, 0.02),
    (WeenieClassName::solleretsdiforsa, 0.04),
    (WeenieClassName::sollerets, 0.04),
    (WeenieClassName::bracersdiforsa, 0.06),
    (WeenieClassName::breastplatediforsa, 0.08),
    (WeenieClassName::cuirassdiforsa, 0.08),
    (WeenieClassName::gauntletsdiforsa, 0.08),
    (WeenieClassName::girthdiforsa, 0.05),
    (WeenieClassName::greavesdiforsa, 0.07),
    (WeenieClassName::tassetsdiforsa, 0.07),
    (WeenieClassName::hauberkdiforsa, 0.06),
    (WeenieClassName::leggingsdiforsa, 0.08),
    (WeenieClassName::pauldronsdiforsa, 0.05),
    (WeenieClassName::sleevesdiforsa, 0.08),
]);

/// ACE `ArmorWcids.TenassaWcids` (`ChanceTable<WeenieClassName>`).
pub static TENASSA_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::breastplatetenassa, 0.33),
    (WeenieClassName::leggingstenassa, 0.34),
    (WeenieClassName::sleevestenassa, 0.33),
]);

/// ACE `ArmorWcids.AlduressaWcids` (`ChanceTable<WeenieClassName>`).
pub static ALDURESSA_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::bootsalduressa, 0.20),
    (WeenieClassName::gauntletsalduressa, 0.20),
    (WeenieClassName::helmalduressa, 0.20),
    (WeenieClassName::coatalduressa, 0.20),
    (WeenieClassName::leggingsalduressa, 0.20),
]);

/// ACE `ArmorWcids.OlthoiWcids` (`ChanceTable<WeenieClassName>`).
pub static OLTHOI_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace37191_olthoigauntlets, 0.10),
    (WeenieClassName::ace37193_olthoigirth, 0.10),
    (WeenieClassName::ace37194_olthoigreaves, 0.10),
    (WeenieClassName::ace37199_olthoihelm, 0.10),
    (WeenieClassName::ace37204_olthoipauldrons, 0.10),
    (WeenieClassName::ace37211_olthoisollerets, 0.10),
    (WeenieClassName::ace37212_olthoitassets, 0.10),
    (WeenieClassName::ace37213_olthoibracers, 0.10),
    (WeenieClassName::ace37216_olthoibreastplate, 0.10),
    (WeenieClassName::ace37291_olthoishield, 0.10),
]);

/// ACE `ArmorWcids.OlthoiCeldonWcids` (`ChanceTable<WeenieClassName>`).
pub static OLTHOI_CELDON_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace37189_olthoiceldongauntlets, 0.14),
    (WeenieClassName::ace37192_olthoiceldongirth, 0.14),
    (WeenieClassName::ace37197_olthoiceldonhelm, 0.14),
    (WeenieClassName::ace37202_olthoiceldonleggings, 0.15),
    (WeenieClassName::ace37205_olthoiceldonsleeves, 0.15),
    (WeenieClassName::ace37209_olthoiceldonsollerets, 0.14),
    (WeenieClassName::ace37214_olthoiceldonbreastplate, 0.14),
]);

/// ACE `ArmorWcids.OlthoiAmuliWcids` (`ChanceTable<WeenieClassName>`).
pub static OLTHOI_AMULI_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace37188_olthoiamuligauntlets, 0.20),
    (WeenieClassName::ace37196_olthoiamulihelm, 0.20),
    (WeenieClassName::ace37201_olthoiamulileggings, 0.20),
    (WeenieClassName::ace37208_olthoiamulisollerets, 0.20),
    (WeenieClassName::ace37299_olthoiamulicoat, 0.20),
]);

/// ACE `ArmorWcids.OlthoiKoujiaWcids` (`ChanceTable<WeenieClassName>`).
pub static OLTHOI_KOUJIA_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace37190_olthoikoujiagauntlets, 0.16),
    (WeenieClassName::ace37198_olthoikoujiakabuton, 0.17),
    (WeenieClassName::ace37203_olthoikoujialeggings, 0.17),
    (WeenieClassName::ace37206_olthoikoujiasleeves, 0.17),
    (WeenieClassName::ace37210_olthoikoujiasollerets, 0.16),
    (WeenieClassName::ace37215_olthoikoujiabreastplate, 0.17),
]);

/// ACE `ArmorWcids.OlthoiAlduressaWcids` (`ChanceTable<WeenieClassName>`).
pub static OLTHOI_ALDURESSA_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace37187_olthoialduressagauntlets, 0.20),
    (WeenieClassName::ace37195_olthoialduressahelm, 0.20),
    (WeenieClassName::ace37200_olthoialduressaleggings, 0.20),
    (WeenieClassName::ace37207_olthoialduressaboots, 0.20),
    (WeenieClassName::ace37217_olthoialduressacoat, 0.20),
]);

/// ACE `ArmorWcids.CelestialHandWcids` (`ChanceTable<WeenieClassName>`).
pub static CELESTIAL_HAND_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace38463_celestialhandbreastplate, 0.34),
    (WeenieClassName::ace38464_celestialhandgauntlets, 0.33),
    (WeenieClassName::ace38465_celestialhandgirth, 0.33),
]);

/// ACE `ArmorWcids.EldrytchWebWcids` (`ChanceTable<WeenieClassName>`).
pub static ELDRYTCH_WEB_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace38472_eldrytchwebbreastplate, 0.34),
    (WeenieClassName::ace38473_eldrytchwebgauntlets, 0.33),
    (WeenieClassName::ace38474_eldrytchwebgirth, 0.33),
]);

/// ACE `ArmorWcids.RadiantBloodWcids` (`ChanceTable<WeenieClassName>`).
pub static RADIANT_BLOOD_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace38481_radiantbloodbreastplate, 0.34),
    (WeenieClassName::ace38482_radiantbloodgauntlets, 0.33),
    (WeenieClassName::ace38483_radiantbloodgirth, 0.33),
]);

/// ACE `ArmorWcids.HaebreanWcids` (`ChanceTable<WeenieClassName>`).
pub static HAEBREAN_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace42749_haebreanbreastplate, 0.11),
    (WeenieClassName::ace42750_haebreangauntlets, 0.11),
    (WeenieClassName::ace42751_haebreangirth, 0.11),
    (WeenieClassName::ace42752_haebreangreaves, 0.11),
    (WeenieClassName::ace42753_haebreanhelm, 0.12),
    (WeenieClassName::ace42754_haebreanpauldrons, 0.11),
    (WeenieClassName::ace42755_haebreanboots, 0.11),
    (WeenieClassName::ace42756_haebreantassets, 0.11),
    (WeenieClassName::ace42757_haebreanvambraces, 0.11),
]);

/// ACE `ArmorWcids.KnorrAcademyWcids` (`ChanceTable<WeenieClassName>`).
pub static KNORR_ACADEMY_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace43048_knorracademybreastplate, 0.12),
    (WeenieClassName::ace43049_knorracademygauntlets, 0.11),
    (WeenieClassName::ace43050_knorracademygirth, 0.11),
    (WeenieClassName::ace43051_knorracademygreaves, 0.11),
    (WeenieClassName::ace43052_knorracademypauldrons, 0.11),
    (WeenieClassName::ace43053_knorracademyboots, 0.11),
    (WeenieClassName::ace43054_knorracademytassets, 0.11),
    (WeenieClassName::ace43055_knorracademyvambraces, 0.11),
    (WeenieClassName::ace43068_knorracademyhelm, 0.11),
]);

/// ACE `ArmorWcids.SedgemailLeatherWcids` (`ChanceTable<WeenieClassName>`).
pub static SEDGEMAIL_LEATHER_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace43828_sedgemailleathervest, 0.17),
    (WeenieClassName::ace43829_sedgemailleathercowl, 0.17),
    (WeenieClassName::ace43830_sedgemailleathergauntlets, 0.16),
    (WeenieClassName::ace43831_sedgemailleatherpants, 0.17),
    (WeenieClassName::ace43832_sedgemailleathershoes, 0.16),
    (WeenieClassName::ace43833_sedgemailleathersleeves, 0.17),
]);

/// ACE `ArmorWcids.OverRobe_T3_T5_Wcids` (`ChanceTable<WeenieClassName>`).
pub static OVER_ROBE_T3_T5_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace44799_faranoverrobe, 0.25),
    (WeenieClassName::ace44800_dhovestandoverrobe, 0.25),
    (WeenieClassName::ace44801_suikanoverrobe, 0.25),
    (WeenieClassName::ace44802_vestirioverrobe, 0.25),
]);

/// ACE `ArmorWcids.OverRobe_T6_T8_Wcids` (`ChanceTable<WeenieClassName>`).
pub static OVER_ROBE_T6_T8_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace44799_faranoverrobe, 0.20),
    (WeenieClassName::ace44800_dhovestandoverrobe, 0.20),
    (WeenieClassName::ace44801_suikanoverrobe, 0.20),
    (WeenieClassName::ace44802_vestirioverrobe, 0.20),
    (WeenieClassName::ace44803_empyreanoverrobe, 0.20),
]);

/// The arguments of the `BuildCombined(..)` calls that make up ACE's `static ArmorWcids()`, in
/// call order.
pub static STATIC_CTOR_BUILD_COMBINED_ARGS: [(&ChanceTable<WeenieClassName>, TreasureArmorType); 26] = [
    (&LEATHER_WCIDS, TreasureArmorType::Leather),
    (&STUDDED_LEATHER_WCIDS, TreasureArmorType::StuddedLeather),
    (&CHAINMAIL_WCIDS, TreasureArmorType::Chainmail),
    (&PLATEMAIL_WCIDS, TreasureArmorType::Platemail),
    (&SCALEMAIL_WCIDS, TreasureArmorType::Scalemail),
    (&YOROI_WCIDS, TreasureArmorType::Yoroi),
    (&CELDON_WCIDS, TreasureArmorType::Celdon),
    (&AMULI_WCIDS, TreasureArmorType::Amuli),
    (&KOUJIA_WCIDS, TreasureArmorType::Koujia),
    (&COVENANT_WCIDS, TreasureArmorType::Covenant),
    (&LORICA_WCIDS, TreasureArmorType::Lorica),
    (&NARIYID_WCIDS, TreasureArmorType::Nariyid),
    (&CHIRAN_WCIDS, TreasureArmorType::Chiran),
    (&DIFORSA_WCIDS, TreasureArmorType::Diforsa),
    (&TENASSA_WCIDS, TreasureArmorType::Tenassa),
    (&ALDURESSA_WCIDS, TreasureArmorType::Alduressa),
    (&OLTHOI_WCIDS, TreasureArmorType::Olthoi),
    (&OLTHOI_CELDON_WCIDS, TreasureArmorType::OlthoiCeldon),
    (&OLTHOI_AMULI_WCIDS, TreasureArmorType::OlthoiAmuli),
    (&OLTHOI_KOUJIA_WCIDS, TreasureArmorType::OlthoiKoujia),
    (&OLTHOI_ALDURESSA_WCIDS, TreasureArmorType::OlthoiAlduressa),
    (&HAEBREAN_WCIDS, TreasureArmorType::Haebrean),
    (&KNORR_ACADEMY_WCIDS, TreasureArmorType::KnorrAcademy),
    (&SEDGEMAIL_LEATHER_WCIDS, TreasureArmorType::Sedgemail),
    (&OVER_ROBE_T3_T5_WCIDS, TreasureArmorType::Overrobe),
    (&OVER_ROBE_T6_T8_WCIDS, TreasureArmorType::Overrobe),
];
