// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/ArmorWcids.cs
// @generated from ClassicACE's `Source/ACE.Server/Factories/Tables/Wcids/ArmorWcids.cs`; do not edit by hand

//! The tables of ClassicACE's `ArmorWcids` under its Infiltration ruleset
//! (`Factories/Tables/Wcids/ArmorWcids.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ClassicACE `ArmorWcids.LeatherWcids`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static LEATHER_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::buckler, 1.0),
    (WeenieClassName::capleather, 1.0),
    (WeenieClassName::cowlleathernew, 1.0),
    (WeenieClassName::basinetleathernew, 1.0),
    (WeenieClassName::gauntletsleathernew, 2.0),
    (WeenieClassName::longgauntletsleathernew, 2.0),
    (WeenieClassName::bootsleathernew, 2.0),
    (WeenieClassName::coatleathernew, 1.0),
    (WeenieClassName::cuirassleathernew, 1.0),
    (WeenieClassName::shirtleathernew, 1.0),
    (WeenieClassName::breastplateleathernew, 1.0),
    (WeenieClassName::girthleathernew, 1.0),
    (WeenieClassName::shortsleathernew, 1.0),
    (WeenieClassName::pantsleathernew, 1.0),
    (WeenieClassName::leggingsleathernew, 1.0),
    (WeenieClassName::tassetsleathernew, 1.0),
    (WeenieClassName::greavesleathernew, 1.0),
    (WeenieClassName::sleevesleathernew, 1.0),
    (WeenieClassName::pauldronsleathernew, 1.0),
    (WeenieClassName::bracersleathernew, 1.0),
]);

/// ClassicACE `ArmorWcids.StuddedLeatherWcids`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static STUDDED_LEATHER_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::shieldkite, 1.0),
    (WeenieClassName::shieldround, 1.0),
    (WeenieClassName::cowlstuddedleather, 1.0),
    (WeenieClassName::basinetstuddedleather, 1.0),
    (WeenieClassName::gauntletsstuddedleather, 2.0),
    (WeenieClassName::bootsreinforcedleather, 2.0),
    (WeenieClassName::coatstuddedleather, 1.0),
    (WeenieClassName::shirtstuddedleather, 1.0),
    (WeenieClassName::cuirassstuddedleather, 1.0),
    (WeenieClassName::breastplatestuddedleather, 1.0),
    (WeenieClassName::girthstuddedleather, 1.0),
    (WeenieClassName::leggingsstuddedleather, 1.0),
    (WeenieClassName::tassetsstuddedleather, 1.0),
    (WeenieClassName::greavesstuddedleather, 1.0),
    (WeenieClassName::sleevesstuddedleather, 1.0),
    (WeenieClassName::pauldronsstuddedleather, 1.0),
    (WeenieClassName::bracersstuddedleather, 1.0),
]);

/// ClassicACE `ArmorWcids.ChainmailWcids`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static CHAINMAIL_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::shieldkitelarge, 1.0),
    (WeenieClassName::shieldroundlarge, 1.0),
    (WeenieClassName::capmetal, 1.0),
    (WeenieClassName::mailcoif, 1.0),
    (WeenieClassName::basinetchainmail, 1.0),
    (WeenieClassName::gauntletschainmail, 2.0),
    (WeenieClassName::bootssteeltoe, 2.0),
    (WeenieClassName::shirtchainmail, 1.0),
    (WeenieClassName::hauberkchainmail, 1.0),
    (WeenieClassName::breastplatechainmail, 1.0),
    (WeenieClassName::girthchainmail, 1.0),
    (WeenieClassName::leggingschainmail, 1.0),
    (WeenieClassName::tassetschainmail, 1.0),
    (WeenieClassName::greaveschainmail, 1.0),
    (WeenieClassName::sleeveschainmail, 1.0),
    (WeenieClassName::pauldronschainmail, 1.0),
    (WeenieClassName::bracerschainmail, 1.0),
]);

/// ClassicACE `ArmorWcids.PlatemailWcids`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static PLATEMAIL_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::shieldtower, 1.0),
    (WeenieClassName::helmhorned, 1.0),
    (WeenieClassName::helmet, 1.0),
    (WeenieClassName::armet, 1.0),
    (WeenieClassName::heaumenew, 1.0),
    (WeenieClassName::gauntletsplatemail, 2.0),
    (WeenieClassName::sollerets, 2.0),
    (WeenieClassName::hauberkplatemail, 1.0),
    (WeenieClassName::cuirassplatemail, 1.0),
    (WeenieClassName::breastplateplatemail, 1.0),
    (WeenieClassName::girthplatemail, 1.0),
    (WeenieClassName::leggingsplatemail, 1.0),
    (WeenieClassName::tassetsplatemail, 1.0),
    (WeenieClassName::greavesplatemail, 1.0),
    (WeenieClassName::sleevesplatemail, 1.0),
    (WeenieClassName::pauldronsplatemail, 1.0),
    (WeenieClassName::vambracesplatemail, 1.0),
]);

/// ClassicACE `ArmorWcids.ScalemailWcids`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static SCALEMAIL_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::shieldtower, 1.0),
    (WeenieClassName::baigha, 1.0),
    (WeenieClassName::helmet, 1.0),
    (WeenieClassName::armet, 1.0),
    (WeenieClassName::heaumenew, 1.0),
    (WeenieClassName::basinetscalemail, 1.0),
    (WeenieClassName::coifscale, 1.0),
    (WeenieClassName::gauntletsscalemail, 2.0),
    (WeenieClassName::sollerets, 2.0),
    (WeenieClassName::breastplatescalemail, 1.0),
    (WeenieClassName::cuirassscalemail, 1.0),
    (WeenieClassName::hauberkscalemail, 1.0),
    (WeenieClassName::shirtscalemail, 1.0),
    (WeenieClassName::girthscalemail, 1.0),
    (WeenieClassName::leggingsscalemail, 1.0),
    (WeenieClassName::tassetsscalemail, 1.0),
    (WeenieClassName::greavesscalemail, 1.0),
    (WeenieClassName::sleevesscalemail, 1.0),
    (WeenieClassName::pauldronsscalemail, 1.0),
    (WeenieClassName::bracersscalemail, 1.0),
]);

/// ClassicACE `ArmorWcids.YoroiWcids`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static YOROI_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::shieldtower, 1.0),
    (WeenieClassName::kabuton, 1.0),
    (WeenieClassName::helmet, 1.0),
    (WeenieClassName::armet, 1.0),
    (WeenieClassName::heaumenew, 1.0),
    (WeenieClassName::gauntletsplatemail, 2.0),
    (WeenieClassName::sollerets, 2.0),
    (WeenieClassName::breastplateyoroi, 1.0),
    (WeenieClassName::cuirassyoroi, 1.0),
    (WeenieClassName::girthyoroi, 1.0),
    (WeenieClassName::leggingsyoroi, 1.0),
    (WeenieClassName::tassetsyoroi, 1.0),
    (WeenieClassName::greavesyoroi, 1.0),
    (WeenieClassName::sleevesyoroi, 1.0),
    (WeenieClassName::pauldronsyoroi, 1.0),
    (WeenieClassName::kote, 1.0),
]);

/// ClassicACE `ArmorWcids.CeldonWcids`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static CELDON_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::girthceldon, 1.0),
    (WeenieClassName::breastplateceldon, 1.0),
    (WeenieClassName::leggingsceldon, 1.0),
    (WeenieClassName::sleevesceldon, 1.0),
]);

/// ClassicACE `ArmorWcids.AmuliWcids`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static AMULI_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::coatamullian, 1.0),
    (WeenieClassName::leggingsamullian, 1.0),
]);

/// ClassicACE `ArmorWcids.KoujiaWcids`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static KOUJIA_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::breastplatekoujia, 1.0),
    (WeenieClassName::leggingskoujia, 1.0),
    (WeenieClassName::sleeveskoujia, 1.0),
]);

/// ClassicACE `ArmorWcids.CovenantWcids`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static COVENANT_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::shieldcovenant, 1.0),
    (WeenieClassName::helmcovenant, 1.0),
    (WeenieClassName::gauntletscovenant, 2.0),
    (WeenieClassName::bootscovenant, 2.0),
    (WeenieClassName::breastplatecovenant, 1.0),
    (WeenieClassName::girthcovenant, 1.0),
    (WeenieClassName::tassetscovenant, 1.0),
    (WeenieClassName::greavescovenant, 1.0),
    (WeenieClassName::pauldronscovenant, 1.0),
    (WeenieClassName::bracerscovenant, 1.0),
]);

/// ClassicACE `ArmorWcids.LoricaWcids`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static LORICA_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::helmlorica, 1.0),
    (WeenieClassName::bootslorica, 2.0),
    (WeenieClassName::gauntletslorica, 2.0),
    (WeenieClassName::breastplatelorica, 1.0),
    (WeenieClassName::leggingslorica, 1.0),
    (WeenieClassName::sleeveslorica, 1.0),
]);

/// ClassicACE `ArmorWcids.NariyidWcids`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static NARIYID_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::helmnariyid, 1.0),
    (WeenieClassName::bootsnariyid, 2.0),
    (WeenieClassName::gauntletsnariyid, 2.0),
    (WeenieClassName::breastplatenariyid, 1.0),
    (WeenieClassName::girthnariyid, 1.0),
    (WeenieClassName::leggingsnariyid, 1.0),
    (WeenieClassName::sleevesnariyid, 1.0),
]);

/// ClassicACE `ArmorWcids.ChiranWcids`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static CHIRAN_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::helmchiran, 1.0),
    (WeenieClassName::sandalschiran, 2.0),
    (WeenieClassName::gauntletschiran, 2.0),
    (WeenieClassName::coatchiran, 1.0),
    (WeenieClassName::leggingschiran, 1.0),
]);

/// ClassicACE `ArmorWcids.DiforsaWcids` (`ChanceTable<WeenieClassName>`).
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

/// ClassicACE `ArmorWcids.TenassaWcids` (`ChanceTable<WeenieClassName>`).
pub static TENASSA_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::breastplatetenassa, 0.33),
    (WeenieClassName::leggingstenassa, 0.34),
    (WeenieClassName::sleevestenassa, 0.33),
]);

/// ClassicACE `ArmorWcids.AlduressaWcids` (`ChanceTable<WeenieClassName>`).
pub static ALDURESSA_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::bootsalduressa, 0.20),
    (WeenieClassName::gauntletsalduressa, 0.20),
    (WeenieClassName::helmalduressa, 0.20),
    (WeenieClassName::coatalduressa, 0.20),
    (WeenieClassName::leggingsalduressa, 0.20),
]);

/// ClassicACE `ArmorWcids.OlthoiWcids` (`ChanceTable<WeenieClassName>`).
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

/// ClassicACE `ArmorWcids.OlthoiCeldonWcids` (`ChanceTable<WeenieClassName>`).
pub static OLTHOI_CELDON_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace37189_olthoiceldongauntlets, 0.14),
    (WeenieClassName::ace37192_olthoiceldongirth, 0.14),
    (WeenieClassName::ace37197_olthoiceldonhelm, 0.14),
    (WeenieClassName::ace37202_olthoiceldonleggings, 0.15),
    (WeenieClassName::ace37205_olthoiceldonsleeves, 0.15),
    (WeenieClassName::ace37209_olthoiceldonsollerets, 0.14),
    (WeenieClassName::ace37214_olthoiceldonbreastplate, 0.14),
]);

/// ClassicACE `ArmorWcids.OlthoiAmuliWcids` (`ChanceTable<WeenieClassName>`).
pub static OLTHOI_AMULI_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace37188_olthoiamuligauntlets, 0.20),
    (WeenieClassName::ace37196_olthoiamulihelm, 0.20),
    (WeenieClassName::ace37201_olthoiamulileggings, 0.20),
    (WeenieClassName::ace37208_olthoiamulisollerets, 0.20),
    (WeenieClassName::ace37299_olthoiamulicoat, 0.20),
]);

/// ClassicACE `ArmorWcids.OlthoiKoujiaWcids` (`ChanceTable<WeenieClassName>`).
pub static OLTHOI_KOUJIA_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace37190_olthoikoujiagauntlets, 0.20),
    (WeenieClassName::ace37198_olthoikoujiakabuton, 0.20),
    (WeenieClassName::ace37203_olthoikoujialeggings, 0.20),
    (WeenieClassName::ace37206_olthoikoujiasleeves, 0.20),
    (WeenieClassName::ace37215_olthoikoujiabreastplate, 0.20),
]);

/// ClassicACE `ArmorWcids.OlthoiAlduressaWcids` (`ChanceTable<WeenieClassName>`).
pub static OLTHOI_ALDURESSA_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace37187_olthoialduressagauntlets, 0.20),
    (WeenieClassName::ace37195_olthoialduressahelm, 0.20),
    (WeenieClassName::ace37200_olthoialduressaleggings, 0.20),
    (WeenieClassName::ace37207_olthoialduressaboots, 0.20),
    (WeenieClassName::ace37217_olthoialduressacoat, 0.20),
]);

/// ClassicACE `ArmorWcids.CelestialHandWcids` (`ChanceTable<WeenieClassName>`).
pub static CELESTIAL_HAND_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace38463_celestialhandbreastplate, 0.34),
    (WeenieClassName::ace38464_celestialhandgauntlets, 0.33),
    (WeenieClassName::ace38465_celestialhandgirth, 0.33),
]);

/// ClassicACE `ArmorWcids.EldrytchWebWcids` (`ChanceTable<WeenieClassName>`).
pub static ELDRYTCH_WEB_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace38472_eldrytchwebbreastplate, 0.34),
    (WeenieClassName::ace38473_eldrytchwebgauntlets, 0.33),
    (WeenieClassName::ace38474_eldrytchwebgirth, 0.33),
]);

/// ClassicACE `ArmorWcids.RadiantBloodWcids` (`ChanceTable<WeenieClassName>`).
pub static RADIANT_BLOOD_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace38481_radiantbloodbreastplate, 0.34),
    (WeenieClassName::ace38482_radiantbloodgauntlets, 0.33),
    (WeenieClassName::ace38483_radiantbloodgirth, 0.33),
]);

/// ClassicACE `ArmorWcids.HaebreanWcids` (`ChanceTable<WeenieClassName>`).
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

/// ClassicACE `ArmorWcids.KnorrAcademyWcids` (`ChanceTable<WeenieClassName>`).
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

/// ClassicACE `ArmorWcids.SedgemailLeatherWcids` (`ChanceTable<WeenieClassName>`).
pub static SEDGEMAIL_LEATHER_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace43828_sedgemailleathervest, 0.17),
    (WeenieClassName::ace43829_sedgemailleathercowl, 0.17),
    (WeenieClassName::ace43830_sedgemailleathergauntlets, 0.16),
    (WeenieClassName::ace43831_sedgemailleatherpants, 0.17),
    (WeenieClassName::ace43832_sedgemailleathershoes, 0.16),
    (WeenieClassName::ace43833_sedgemailleathersleeves, 0.17),
]);

/// ClassicACE `ArmorWcids.OverRobe_T3_T5_Wcids` (`ChanceTable<WeenieClassName>`).
pub static OVER_ROBE_T3_T5_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace44799_faranoverrobe, 0.25),
    (WeenieClassName::ace44800_dhovestandoverrobe, 0.25),
    (WeenieClassName::ace44801_suikanoverrobe, 0.25),
    (WeenieClassName::ace44802_vestirioverrobe, 0.25),
]);

/// ClassicACE `ArmorWcids.OverRobe_T6_T8_Wcids` (`ChanceTable<WeenieClassName>`).
pub static OVER_ROBE_T6_T8_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace44799_faranoverrobe, 0.20),
    (WeenieClassName::ace44800_dhovestandoverrobe, 0.20),
    (WeenieClassName::ace44801_suikanoverrobe, 0.20),
    (WeenieClassName::ace44802_vestirioverrobe, 0.20),
    (WeenieClassName::ace44803_empyreanoverrobe, 0.20),
]);

/// ClassicACE `ArmorWcids.ClothAluvianWcids` (`ChanceTable<WeenieClassName>`).
pub static CLOTH_ALUVIAN_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::robealuvianhood, 1.0),
    (WeenieClassName::robealuviannohood, 1.0),
    (WeenieClassName::dressaluvian, 0.5),
    (WeenieClassName::dressaluvianlowcut, 0.5),
]);

/// ClassicACE `ArmorWcids.ClothGharuWcids` (`ChanceTable<WeenieClassName>`).
pub static CLOTH_GHARU_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::robegharundimhood, 1.0),
    (WeenieClassName::robegharundimnohood, 1.0),
    (WeenieClassName::dressgharundim, 1.0),
]);

/// ClassicACE `ArmorWcids.ClothShoWcids` (`ChanceTable<WeenieClassName>`).
pub static CLOTH_SHO_WCIDS: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::robeshohood, 1.0),
    (WeenieClassName::robeshonohood, 1.0),
    (WeenieClassName::dresssho, 1.0),
]);
