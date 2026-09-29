// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/SpellLevelProgression.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/SpellLevelProgression.cs`; do not edit by hand

//! The literal data of ACE's `SpellLevelProgression` (`Factories/Tables/SpellLevelProgression.cs`).
//!
//! Runtime state, hand-ported in `crate::logic` (not literal here):
//! - `spellProgression` (`Dictionary<SpellId, List<SpellId>>`)

use empyrean_entity::enums::SpellId;

/// ACE `SpellLevelProgression.StrengthOther` (`List<SpellId>`).
pub static STRENGTH_OTHER: [SpellId; 8] = [
    SpellId::StrengthOther1,
    SpellId::StrengthOther2,
    SpellId::StrengthOther3,
    SpellId::StrengthOther4,
    SpellId::StrengthOther5,
    SpellId::StrengthOther6,
    SpellId::StrengthOther7,
    SpellId::StrengthOther8,
];

/// ACE `SpellLevelProgression.StrengthSelf` (`List<SpellId>`).
pub static STRENGTH_SELF: [SpellId; 8] = [
    SpellId::StrengthSelf1,
    SpellId::StrengthSelf2,
    SpellId::StrengthSelf3,
    SpellId::StrengthSelf4,
    SpellId::StrengthSelf5,
    SpellId::StrengthSelf6,
    SpellId::StrengthSelf7,
    SpellId::StrengthSelf8,
];

/// ACE `SpellLevelProgression.WeaknessOther` (`List<SpellId>`).
pub static WEAKNESS_OTHER: [SpellId; 8] = [
    SpellId::WeaknessOther1,
    SpellId::WeaknessOther2,
    SpellId::WeaknessOther3,
    SpellId::WeaknessOther4,
    SpellId::WeaknessOther5,
    SpellId::WeaknessOther6,
    SpellId::WeaknessOther7,
    SpellId::WeaknessOther8,
];

/// ACE `SpellLevelProgression.WeaknessSelf` (`List<SpellId>`).
pub static WEAKNESS_SELF: [SpellId; 8] = [
    SpellId::WeaknessSelf1,
    SpellId::WeaknessSelf2,
    SpellId::WeaknessSelf3,
    SpellId::WeaknessSelf4,
    SpellId::WeaknessSelf5,
    SpellId::WeaknessSelf6,
    SpellId::WeaknessSelf7,
    SpellId::WeaknessSelf8,
];

/// ACE `SpellLevelProgression.HealOther` (`List<SpellId>`).
pub static HEAL_OTHER: [SpellId; 8] = [
    SpellId::HealOther1,
    SpellId::HealOther2,
    SpellId::HealOther3,
    SpellId::HealOther4,
    SpellId::HealOther5,
    SpellId::HealOther6,
    SpellId::HealOther7,
    SpellId::HealOther8,
];

/// ACE `SpellLevelProgression.HealSelf` (`List<SpellId>`).
pub static HEAL_SELF: [SpellId; 8] = [
    SpellId::HealSelf1,
    SpellId::HealSelf2,
    SpellId::HealSelf3,
    SpellId::HealSelf4,
    SpellId::HealSelf5,
    SpellId::HealSelf6,
    SpellId::HealSelf7,
    SpellId::HealSelf8,
];

/// ACE `SpellLevelProgression.HarmOther` (`List<SpellId>`).
pub static HARM_OTHER: [SpellId; 8] = [
    SpellId::HarmOther1,
    SpellId::HarmOther2,
    SpellId::HarmOther3,
    SpellId::HarmOther4,
    SpellId::HarmOther5,
    SpellId::HarmOther6,
    SpellId::HarmOther7,
    SpellId::HarmOther8,
];

/// ACE `SpellLevelProgression.HarmSelf` (`List<SpellId>`).
pub static HARM_SELF: [SpellId; 8] = [
    SpellId::HarmSelf1,
    SpellId::HarmSelf2,
    SpellId::HarmSelf3,
    SpellId::HarmSelf4,
    SpellId::HarmSelf5,
    SpellId::HarmSelf6,
    SpellId::HarmSelf7,
    SpellId::HarmSelf8,
];

/// ACE `SpellLevelProgression.InfuseMana` (`List<SpellId>`).
pub static INFUSE_MANA: [SpellId; 8] = [
    SpellId::InfuseMana1,
    SpellId::InfuseMana2,
    SpellId::InfuseMana3,
    SpellId::InfuseMana4,
    SpellId::InfuseMana5,
    SpellId::InfuseMana6,
    SpellId::InfuseMana7,
    SpellId::InfuseMana8,
];

/// ACE `SpellLevelProgression.VulnerabilityOther` (`List<SpellId>`).
pub static VULNERABILITY_OTHER: [SpellId; 8] = [
    SpellId::VulnerabilityOther1,
    SpellId::VulnerabilityOther2,
    SpellId::VulnerabilityOther3,
    SpellId::VulnerabilityOther4,
    SpellId::VulnerabilityOther5,
    SpellId::VulnerabilityOther6,
    SpellId::VulnerabilityOther7,
    SpellId::VulnerabilityOther8,
];

/// ACE `SpellLevelProgression.VulnerabilitySelf` (`List<SpellId>`).
pub static VULNERABILITY_SELF: [SpellId; 8] = [
    SpellId::VulnerabilitySelf1,
    SpellId::VulnerabilitySelf2,
    SpellId::VulnerabilitySelf3,
    SpellId::VulnerabilitySelf4,
    SpellId::VulnerabilitySelf5,
    SpellId::VulnerabilitySelf6,
    SpellId::VulnerabilitySelf7,
    SpellId::VulnerabilitySelf8,
];

/// ACE `SpellLevelProgression.InvulnerabilityOther` (`List<SpellId>`).
pub static INVULNERABILITY_OTHER: [SpellId; 8] = [
    SpellId::InvulnerabilityOther1,
    SpellId::InvulnerabilityOther2,
    SpellId::InvulnerabilityOther3,
    SpellId::InvulnerabilityOther4,
    SpellId::InvulnerabilityOther5,
    SpellId::InvulnerabilityOther6,
    SpellId::InvulnerabilityOther7,
    SpellId::InvulnerabilityOther8,
];

/// ACE `SpellLevelProgression.InvulnerabilitySelf` (`List<SpellId>`).
pub static INVULNERABILITY_SELF: [SpellId; 8] = [
    SpellId::InvulnerabilitySelf1,
    SpellId::InvulnerabilitySelf2,
    SpellId::InvulnerabilitySelf3,
    SpellId::InvulnerabilitySelf4,
    SpellId::InvulnerabilitySelf5,
    SpellId::InvulnerabilitySelf6,
    SpellId::InvulnerabilitySelf7,
    SpellId::InvulnerabilitySelf8,
];

/// ACE `SpellLevelProgression.FireProtectionOther` (`List<SpellId>`).
pub static FIRE_PROTECTION_OTHER: [SpellId; 8] = [
    SpellId::FireProtectionOther1,
    SpellId::FireProtectionOther2,
    SpellId::FireProtectionOther3,
    SpellId::FireProtectionOther4,
    SpellId::FireProtectionOther5,
    SpellId::FireProtectionOther6,
    SpellId::FireProtectionOther7,
    SpellId::FireProtectionOther8,
];

/// ACE `SpellLevelProgression.FireProtectionSelf` (`List<SpellId>`).
pub static FIRE_PROTECTION_SELF: [SpellId; 8] = [
    SpellId::FireProtectionSelf1,
    SpellId::FireProtectionSelf2,
    SpellId::FireProtectionSelf3,
    SpellId::FireProtectionSelf4,
    SpellId::FireProtectionSelf5,
    SpellId::FireProtectionSelf6,
    SpellId::FireProtectionSelf7,
    SpellId::FireProtectionSelf8,
];

/// ACE `SpellLevelProgression.FireVulnerabilityOther` (`List<SpellId>`).
pub static FIRE_VULNERABILITY_OTHER: [SpellId; 8] = [
    SpellId::FireVulnerabilityOther1,
    SpellId::FireVulnerabilityOther2,
    SpellId::FireVulnerabilityOther3,
    SpellId::FireVulnerabilityOther4,
    SpellId::FireVulnerabilityOther5,
    SpellId::FireVulnerabilityOther6,
    SpellId::FireVulnerabilityOther7,
    SpellId::FireVulnerabilityOther8,
];

/// ACE `SpellLevelProgression.FireVulnerabilitySelf` (`List<SpellId>`).
pub static FIRE_VULNERABILITY_SELF: [SpellId; 8] = [
    SpellId::FireVulnerabilitySelf1,
    SpellId::FireVulnerabilitySelf2,
    SpellId::FireVulnerabilitySelf3,
    SpellId::FireVulnerabilitySelf4,
    SpellId::FireVulnerabilitySelf5,
    SpellId::FireVulnerabilitySelf6,
    SpellId::FireVulnerabilitySelf7,
    SpellId::FireVulnerabilitySelf8,
];

/// ACE `SpellLevelProgression.ArmorOther` (`List<SpellId>`).
pub static ARMOR_OTHER: [SpellId; 8] = [
    SpellId::ArmorOther1,
    SpellId::ArmorOther2,
    SpellId::ArmorOther3,
    SpellId::ArmorOther4,
    SpellId::ArmorOther5,
    SpellId::ArmorOther6,
    SpellId::ArmorOther7,
    SpellId::ArmorOther8,
];

/// ACE `SpellLevelProgression.ArmorSelf` (`List<SpellId>`).
pub static ARMOR_SELF: [SpellId; 8] = [
    SpellId::ArmorSelf1,
    SpellId::ArmorSelf2,
    SpellId::ArmorSelf3,
    SpellId::ArmorSelf4,
    SpellId::ArmorSelf5,
    SpellId::ArmorSelf6,
    SpellId::ArmorSelf7,
    SpellId::ArmorSelf8,
];

/// ACE `SpellLevelProgression.ImperilOther` (`List<SpellId>`).
pub static IMPERIL_OTHER: [SpellId; 8] = [
    SpellId::ImperilOther1,
    SpellId::ImperilOther2,
    SpellId::ImperilOther3,
    SpellId::ImperilOther4,
    SpellId::ImperilOther5,
    SpellId::ImperilOther6,
    SpellId::ImperilOther7,
    SpellId::ImperilOther8,
];

/// ACE `SpellLevelProgression.ImperilSelf` (`List<SpellId>`).
pub static IMPERIL_SELF: [SpellId; 8] = [
    SpellId::ImperilSelf1,
    SpellId::ImperilSelf2,
    SpellId::ImperilSelf3,
    SpellId::ImperilSelf4,
    SpellId::ImperilSelf5,
    SpellId::ImperilSelf6,
    SpellId::ImperilSelf7,
    SpellId::ImperilSelf8,
];

/// ACE `SpellLevelProgression.FlameBolt` (`List<SpellId>`).
pub static FLAME_BOLT: [SpellId; 8] = [
    SpellId::FlameBolt1,
    SpellId::FlameBolt2,
    SpellId::FlameBolt3,
    SpellId::FlameBolt4,
    SpellId::FlameBolt5,
    SpellId::FlameBolt6,
    SpellId::FlameBolt7,
    SpellId::FlameBolt8,
];

/// ACE `SpellLevelProgression.FrostBolt` (`List<SpellId>`).
pub static FROST_BOLT: [SpellId; 8] = [
    SpellId::FrostBolt1,
    SpellId::FrostBolt2,
    SpellId::FrostBolt3,
    SpellId::FrostBolt4,
    SpellId::FrostBolt5,
    SpellId::FrostBolt6,
    SpellId::FrostBolt7,
    SpellId::FrostBolt8,
];

/// ACE `SpellLevelProgression.BloodDrinkerSelf` (`List<SpellId>`).
pub static BLOOD_DRINKER_SELF: [SpellId; 8] = [
    SpellId::BloodDrinkerSelf1,
    SpellId::BloodDrinkerSelf2,
    SpellId::BloodDrinkerSelf3,
    SpellId::BloodDrinkerSelf4,
    SpellId::BloodDrinkerSelf5,
    SpellId::BloodDrinkerSelf6,
    SpellId::BloodDrinkerSelf7,
    SpellId::BloodDrinkerSelf8,
];

/// ACE `SpellLevelProgression.BloodLoather` (`List<SpellId>`).
pub static BLOOD_LOATHER: [SpellId; 8] = [
    SpellId::BloodLoather,
    SpellId::BloodLoather2,
    SpellId::BloodLoather3,
    SpellId::BloodLoather4,
    SpellId::BloodLoather5,
    SpellId::BloodLoather6,
    SpellId::BloodLoather7,
    SpellId::BloodLoather8,
];

/// ACE `SpellLevelProgression.BladeBane` (`List<SpellId>`).
pub static BLADE_BANE: [SpellId; 8] = [
    SpellId::BladeBane1,
    SpellId::BladeBane2,
    SpellId::BladeBane3,
    SpellId::BladeBane4,
    SpellId::BladeBane5,
    SpellId::BladeBane6,
    SpellId::BladeBane7,
    SpellId::BladeBane8,
];

/// ACE `SpellLevelProgression.BladeLure` (`List<SpellId>`).
pub static BLADE_LURE: [SpellId; 8] = [
    SpellId::BladeLure1,
    SpellId::BladeLure2,
    SpellId::BladeLure3,
    SpellId::BladeLure4,
    SpellId::BladeLure5,
    SpellId::BladeLure6,
    SpellId::BladeLure7,
    SpellId::BladeLure8,
];

/// ACE `SpellLevelProgression.PortalTie` (`List<SpellId>`).
pub static PORTAL_TIE: [SpellId; 2] = [
    SpellId::PortalTie1,
    SpellId::PortalTie2,
];

/// ACE `SpellLevelProgression.PortalTieRecall` (`List<SpellId>`).
pub static PORTAL_TIE_RECALL: [SpellId; 2] = [
    SpellId::PortalTieRecall1,
    SpellId::PortalTieRecall2,
];

/// ACE `SpellLevelProgression.SwiftKillerSelf` (`List<SpellId>`).
pub static SWIFT_KILLER_SELF: [SpellId; 8] = [
    SpellId::SwiftKillerSelf1,
    SpellId::SwiftKillerSelf2,
    SpellId::SwiftKillerSelf3,
    SpellId::SwiftKillerSelf4,
    SpellId::SwiftKillerSelf5,
    SpellId::SwiftKillerSelf6,
    SpellId::SwiftKillerSelf7,
    SpellId::SwiftKillerSelf8,
];

/// ACE `SpellLevelProgression.LeadenWeapon` (`List<SpellId>`).
pub static LEADEN_WEAPON: [SpellId; 8] = [
    SpellId::LeadenWeapon1,
    SpellId::LeadenWeapon2,
    SpellId::LeadenWeapon3,
    SpellId::LeadenWeapon4,
    SpellId::LeadenWeapon5,
    SpellId::LeadenWeapon6,
    SpellId::LeadenWeapon7,
    SpellId::LeadenWeapon8,
];

/// ACE `SpellLevelProgression.Impenetrability` (`List<SpellId>`).
pub static IMPENETRABILITY: [SpellId; 8] = [
    SpellId::Impenetrability1,
    SpellId::Impenetrability2,
    SpellId::Impenetrability3,
    SpellId::Impenetrability4,
    SpellId::Impenetrability5,
    SpellId::Impenetrability6,
    SpellId::Impenetrability7,
    SpellId::Impenetrability8,
];

/// ACE `SpellLevelProgression.RejuvenationOther` (`List<SpellId>`).
pub static REJUVENATION_OTHER: [SpellId; 8] = [
    SpellId::RejuvenationOther1,
    SpellId::RejuvenationOther2,
    SpellId::RejuvenationOther3,
    SpellId::RejuvenationOther4,
    SpellId::RejuvenationOther5,
    SpellId::RejuvenationOther6,
    SpellId::RejuvenationOther7,
    SpellId::RejuvenationOther8,
];

/// ACE `SpellLevelProgression.RejuvenationSelf` (`List<SpellId>`).
pub static REJUVENATION_SELF: [SpellId; 8] = [
    SpellId::RejuvenationSelf1,
    SpellId::RejuvenationSelf2,
    SpellId::RejuvenationSelf3,
    SpellId::RejuvenationSelf4,
    SpellId::RejuvenationSelf5,
    SpellId::RejuvenationSelf6,
    SpellId::RejuvenationSelf7,
    SpellId::RejuvenationSelf8,
];

/// ACE `SpellLevelProgression.AcidStream` (`List<SpellId>`).
pub static ACID_STREAM: [SpellId; 8] = [
    SpellId::AcidStream1,
    SpellId::AcidStream2,
    SpellId::AcidStream3,
    SpellId::AcidStream4,
    SpellId::AcidStream5,
    SpellId::AcidStream6,
    SpellId::AcidStream7,
    SpellId::AcidStream8,
];

/// ACE `SpellLevelProgression.ShockWave` (`List<SpellId>`).
pub static SHOCK_WAVE: [SpellId; 8] = [
    SpellId::ShockWave1,
    SpellId::ShockWave2,
    SpellId::ShockWave3,
    SpellId::ShockWave4,
    SpellId::ShockWave5,
    SpellId::ShockWave6,
    SpellId::ShockWave7,
    SpellId::ShockWave8,
];

/// ACE `SpellLevelProgression.LightningBolt` (`List<SpellId>`).
pub static LIGHTNING_BOLT: [SpellId; 8] = [
    SpellId::LightningBolt1,
    SpellId::LightningBolt2,
    SpellId::LightningBolt3,
    SpellId::LightningBolt4,
    SpellId::LightningBolt5,
    SpellId::LightningBolt6,
    SpellId::LightningBolt7,
    SpellId::LightningBolt8,
];

/// ACE `SpellLevelProgression.ForceBolt` (`List<SpellId>`).
pub static FORCE_BOLT: [SpellId; 8] = [
    SpellId::ForceBolt1,
    SpellId::ForceBolt2,
    SpellId::ForceBolt3,
    SpellId::ForceBolt4,
    SpellId::ForceBolt5,
    SpellId::ForceBolt6,
    SpellId::ForceBolt7,
    SpellId::ForceBolt8,
];

/// ACE `SpellLevelProgression.WhirlingBlade` (`List<SpellId>`).
pub static WHIRLING_BLADE: [SpellId; 8] = [
    SpellId::WhirlingBlade1,
    SpellId::WhirlingBlade2,
    SpellId::WhirlingBlade3,
    SpellId::WhirlingBlade4,
    SpellId::WhirlingBlade5,
    SpellId::WhirlingBlade6,
    SpellId::WhirlingBlade7,
    SpellId::WhirlingBlade8,
];

/// ACE `SpellLevelProgression.AcidBlast` (`List<SpellId>`).
pub static ACID_BLAST: [SpellId; 8] = [
    SpellId::Undef,
    SpellId::Undef,
    SpellId::AcidBlast3,
    SpellId::AcidBlast4,
    SpellId::AcidBlast5,
    SpellId::AcidBlast6,
    SpellId::AcidBlast7,
    SpellId::AcidBlast8,
];

/// ACE `SpellLevelProgression.ShockBlast` (`List<SpellId>`).
pub static SHOCK_BLAST: [SpellId; 8] = [
    SpellId::Undef,
    SpellId::Undef,
    SpellId::ShockBlast3,
    SpellId::ShockBlast4,
    SpellId::ShockBlast5,
    SpellId::ShockBlast6,
    SpellId::ShockBlast7,
    SpellId::ShockBlast8,
];

/// ACE `SpellLevelProgression.FrostBlast` (`List<SpellId>`).
pub static FROST_BLAST: [SpellId; 8] = [
    SpellId::Undef,
    SpellId::Undef,
    SpellId::FrostBlast3,
    SpellId::FrostBlast4,
    SpellId::FrostBlast5,
    SpellId::FrostBlast6,
    SpellId::FrostBlast7,
    SpellId::FrostBlast8,
];

/// ACE `SpellLevelProgression.LightningBlast` (`List<SpellId>`).
pub static LIGHTNING_BLAST: [SpellId; 8] = [
    SpellId::Undef,
    SpellId::Undef,
    SpellId::LightningBlast3,
    SpellId::LightningBlast4,
    SpellId::LightningBlast5,
    SpellId::LightningBlast6,
    SpellId::LightningBlast7,
    SpellId::LightningBlast8,
];

/// ACE `SpellLevelProgression.FlameBlast` (`List<SpellId>`).
pub static FLAME_BLAST: [SpellId; 8] = [
    SpellId::Undef,
    SpellId::Undef,
    SpellId::FlameBlast3,
    SpellId::FlameBlast4,
    SpellId::FlameBlast5,
    SpellId::FlameBlast6,
    SpellId::FlameBlast7,
    SpellId::FlameBlast8,
];

/// ACE `SpellLevelProgression.ForceBlast` (`List<SpellId>`).
pub static FORCE_BLAST: [SpellId; 8] = [
    SpellId::Undef,
    SpellId::Undef,
    SpellId::ForceBlast3,
    SpellId::ForceBlast4,
    SpellId::ForceBlast5,
    SpellId::ForceBlast6,
    SpellId::ForceBlast7,
    SpellId::ForceBlast8,
];

/// ACE `SpellLevelProgression.BladeBlast` (`List<SpellId>`).
pub static BLADE_BLAST: [SpellId; 8] = [
    SpellId::Undef,
    SpellId::Undef,
    SpellId::BladeBlast3,
    SpellId::BladeBlast4,
    SpellId::BladeBlast5,
    SpellId::BladeBlast6,
    SpellId::BladeBlast7,
    SpellId::BladeBlast8,
];

/// ACE `SpellLevelProgression.AcidVolley` (`List<SpellId>`).
pub static ACID_VOLLEY: [SpellId; 8] = [
    SpellId::Undef,
    SpellId::Undef,
    SpellId::AcidVolley3,
    SpellId::AcidVolley4,
    SpellId::AcidVolley5,
    SpellId::AcidVolley6,
    SpellId::AcidVolley7,
    SpellId::AcidVolley8,
];

/// ACE `SpellLevelProgression.BludgeoningVolley` (`List<SpellId>`).
pub static BLUDGEONING_VOLLEY: [SpellId; 8] = [
    SpellId::Undef,
    SpellId::Undef,
    SpellId::BludgeoningVolley3,
    SpellId::BludgeoningVolley4,
    SpellId::BludgeoningVolley5,
    SpellId::BludgeoningVolley6,
    SpellId::BludgeoningVolley7,
    SpellId::BludgeoningVolley8,
];

/// ACE `SpellLevelProgression.FrostVolley` (`List<SpellId>`).
pub static FROST_VOLLEY: [SpellId; 8] = [
    SpellId::Undef,
    SpellId::Undef,
    SpellId::FrostVolley3,
    SpellId::FrostVolley4,
    SpellId::FrostVolley5,
    SpellId::FrostVolley6,
    SpellId::FrostVolley7,
    SpellId::FrostVolley8,
];

/// ACE `SpellLevelProgression.LightningVolley` (`List<SpellId>`).
pub static LIGHTNING_VOLLEY: [SpellId; 8] = [
    SpellId::Undef,
    SpellId::Undef,
    SpellId::LightningVolley3,
    SpellId::LightningVolley4,
    SpellId::LightningVolley5,
    SpellId::LightningVolley6,
    SpellId::LightningVolley7,
    SpellId::LightningVolley8,
];

/// ACE `SpellLevelProgression.FlameVolley` (`List<SpellId>`).
pub static FLAME_VOLLEY: [SpellId; 8] = [
    SpellId::Undef,
    SpellId::Undef,
    SpellId::FlameVolley3,
    SpellId::FlameVolley4,
    SpellId::FlameVolley5,
    SpellId::FlameVolley6,
    SpellId::FlameVolley7,
    SpellId::FlameVolley8,
];

/// ACE `SpellLevelProgression.ForceVolley` (`List<SpellId>`).
pub static FORCE_VOLLEY: [SpellId; 8] = [
    SpellId::Undef,
    SpellId::Undef,
    SpellId::ForceVolley3,
    SpellId::ForceVolley4,
    SpellId::ForceVolley5,
    SpellId::ForceVolley6,
    SpellId::ForceVolley7,
    SpellId::ForceVolley8,
];

/// ACE `SpellLevelProgression.BladeVolley` (`List<SpellId>`).
pub static BLADE_VOLLEY: [SpellId; 8] = [
    SpellId::Undef,
    SpellId::Undef,
    SpellId::BladeVolley3,
    SpellId::BladeVolley4,
    SpellId::BladeVolley5,
    SpellId::BladeVolley6,
    SpellId::BladeVolley7,
    SpellId::BladeVolley8,
];

/// ACE `SpellLevelProgression.SummonPortal` (`List<SpellId>`).
pub static SUMMON_PORTAL: [SpellId; 3] = [
    SpellId::SummonPortal1,
    SpellId::SummonPortal2,
    SpellId::SummonPortal3,
];

/// ACE `SpellLevelProgression.RegenerationOther` (`List<SpellId>`).
pub static REGENERATION_OTHER: [SpellId; 8] = [
    SpellId::RegenerationOther1,
    SpellId::RegenerationOther2,
    SpellId::RegenerationOther3,
    SpellId::RegenerationOther4,
    SpellId::RegenerationOther5,
    SpellId::RegenerationOther6,
    SpellId::RegenerationOther7,
    SpellId::RegenerationOther8,
];

/// ACE `SpellLevelProgression.RegenerationSelf` (`List<SpellId>`).
pub static REGENERATION_SELF: [SpellId; 8] = [
    SpellId::RegenerationSelf1,
    SpellId::RegenerationSelf2,
    SpellId::RegenerationSelf3,
    SpellId::RegenerationSelf4,
    SpellId::RegenerationSelf5,
    SpellId::RegenerationSelf6,
    SpellId::RegenerationSelf7,
    SpellId::RegenerationSelf8,
];

/// ACE `SpellLevelProgression.FesterOther` (`List<SpellId>`).
pub static FESTER_OTHER: [SpellId; 8] = [
    SpellId::FesterOther1,
    SpellId::FesterOther2,
    SpellId::FesterOther3,
    SpellId::FesterOther4,
    SpellId::FesterOther5,
    SpellId::FesterOther6,
    SpellId::FesterOther7,
    SpellId::FesterOther8,
];

/// ACE `SpellLevelProgression.FesterSelf` (`List<SpellId>`).
pub static FESTER_SELF: [SpellId; 8] = [
    SpellId::FesterSelf1,
    SpellId::FesterSelf2,
    SpellId::FesterSelf3,
    SpellId::FesterSelf4,
    SpellId::FesterSelf5,
    SpellId::FesterSelf6,
    SpellId::FesterSelf7,
    SpellId::FesterSelf8,
];

/// ACE `SpellLevelProgression.ExhaustionOther` (`List<SpellId>`).
pub static EXHAUSTION_OTHER: [SpellId; 8] = [
    SpellId::ExhaustionOther1,
    SpellId::ExhaustionOther2,
    SpellId::ExhaustionOther3,
    SpellId::ExhaustionOther4,
    SpellId::ExhaustionOther5,
    SpellId::ExhaustionOther6,
    SpellId::ExhaustionOther7,
    SpellId::ExhaustionOther8,
];

/// ACE `SpellLevelProgression.ExhaustionSelf` (`List<SpellId>`).
pub static EXHAUSTION_SELF: [SpellId; 8] = [
    SpellId::ExhaustionSelf1,
    SpellId::ExhaustionSelf2,
    SpellId::ExhaustionSelf3,
    SpellId::ExhaustionSelf4,
    SpellId::ExhaustionSelf5,
    SpellId::ExhaustionSelf6,
    SpellId::ExhaustionSelf7,
    SpellId::ExhaustionSelf8,
];

/// ACE `SpellLevelProgression.ManaRenewalOther` (`List<SpellId>`).
pub static MANA_RENEWAL_OTHER: [SpellId; 8] = [
    SpellId::ManaRenewalOther1,
    SpellId::ManaRenewalOther2,
    SpellId::ManaRenewalOther3,
    SpellId::ManaRenewalOther4,
    SpellId::ManaRenewalOther5,
    SpellId::ManaRenewalOther6,
    SpellId::ManaRenewalOther7,
    SpellId::ManaRenewalOther8,
];

/// ACE `SpellLevelProgression.ManaRenewalSelf` (`List<SpellId>`).
pub static MANA_RENEWAL_SELF: [SpellId; 8] = [
    SpellId::ManaRenewalSelf1,
    SpellId::ManaRenewalSelf2,
    SpellId::ManaRenewalSelf3,
    SpellId::ManaRenewalSelf4,
    SpellId::ManaRenewalSelf5,
    SpellId::ManaRenewalSelf6,
    SpellId::ManaRenewalSelf7,
    SpellId::ManaRenewalSelf8,
];

/// ACE `SpellLevelProgression.ManaDepletionOther` (`List<SpellId>`).
pub static MANA_DEPLETION_OTHER: [SpellId; 8] = [
    SpellId::ManaDepletionOther1,
    SpellId::ManaDepletionOther2,
    SpellId::ManaDepletionOther3,
    SpellId::ManaDepletionOther4,
    SpellId::ManaDepletionOther5,
    SpellId::ManaDepletionOther6,
    SpellId::ManaDepletionOther7,
    SpellId::ManaDepletionOther8,
];

/// ACE `SpellLevelProgression.ManaDepletionSelf` (`List<SpellId>`).
pub static MANA_DEPLETION_SELF: [SpellId; 8] = [
    SpellId::ManaDepletionSelf1,
    SpellId::ManaDepletionSelf2,
    SpellId::ManaDepletionSelf3,
    SpellId::ManaDepletionSelf4,
    SpellId::ManaDepletionSelf5,
    SpellId::ManaDepletionSelf6,
    SpellId::ManaDepletionSelf7,
    SpellId::ManaDepletionSelf8,
];

/// ACE `SpellLevelProgression.ImpregnabilityOther` (`List<SpellId>`).
pub static IMPREGNABILITY_OTHER: [SpellId; 8] = [
    SpellId::ImpregnabilityOther1,
    SpellId::ImpregnabilityOther2,
    SpellId::ImpregnabilityOther3,
    SpellId::ImpregnabilityOther4,
    SpellId::ImpregnabilityOther5,
    SpellId::ImpregnabilityOther6,
    SpellId::ImpregnabilityOther7,
    SpellId::ImpregnabilityOther8,
];

/// ACE `SpellLevelProgression.ImpregnabilitySelf` (`List<SpellId>`).
pub static IMPREGNABILITY_SELF: [SpellId; 8] = [
    SpellId::ImpregnabilitySelf1,
    SpellId::ImpregnabilitySelf2,
    SpellId::ImpregnabilitySelf3,
    SpellId::ImpregnabilitySelf4,
    SpellId::ImpregnabilitySelf5,
    SpellId::ImpregnabilitySelf6,
    SpellId::ImpregnabilitySelf7,
    SpellId::ImpregnabilitySelf8,
];

/// ACE `SpellLevelProgression.DefenselessnessOther` (`List<SpellId>`).
pub static DEFENSELESSNESS_OTHER: [SpellId; 8] = [
    SpellId::DefenselessnessOther1,
    SpellId::DefenselessnessOther2,
    SpellId::DefenselessnessOther3,
    SpellId::DefenselessnessOther4,
    SpellId::DefenselessnessOther5,
    SpellId::DefenselessnessOther6,
    SpellId::DefenselessnessOther7,
    SpellId::DefenselessnessOther8,
];

/// ACE `SpellLevelProgression.MagicResistanceOther` (`List<SpellId>`).
pub static MAGIC_RESISTANCE_OTHER: [SpellId; 8] = [
    SpellId::MagicResistanceOther1,
    SpellId::MagicResistanceOther2,
    SpellId::MagicResistanceOther3,
    SpellId::MagicResistanceOther4,
    SpellId::MagicResistanceOther5,
    SpellId::MagicResistanceOther6,
    SpellId::MagicResistanceOther7,
    SpellId::MagicResistanceOther8,
];

/// ACE `SpellLevelProgression.MagicResistanceSelf` (`List<SpellId>`).
pub static MAGIC_RESISTANCE_SELF: [SpellId; 8] = [
    SpellId::MagicResistanceSelf1,
    SpellId::MagicResistanceSelf2,
    SpellId::MagicResistanceSelf3,
    SpellId::MagicResistanceSelf4,
    SpellId::MagicResistanceSelf5,
    SpellId::MagicResistanceSelf6,
    SpellId::MagicResistanceSelf7,
    SpellId::MagicResistanceSelf8,
];

/// ACE `SpellLevelProgression.MagicYieldOther` (`List<SpellId>`).
pub static MAGIC_YIELD_OTHER: [SpellId; 8] = [
    SpellId::MagicYieldOther1,
    SpellId::MagicYieldOther2,
    SpellId::MagicYieldOther3,
    SpellId::MagicYieldOther4,
    SpellId::MagicYieldOther5,
    SpellId::MagicYieldOther6,
    SpellId::MagicYieldOther7,
    SpellId::MagicYieldOther8,
];

/// ACE `SpellLevelProgression.MagicYieldSelf` (`List<SpellId>`).
pub static MAGIC_YIELD_SELF: [SpellId; 8] = [
    SpellId::MagicYieldSelf1,
    SpellId::MagicYieldSelf2,
    SpellId::MagicYieldSelf3,
    SpellId::MagicYieldSelf4,
    SpellId::MagicYieldSelf5,
    SpellId::MagicYieldSelf6,
    SpellId::MagicYieldSelf7,
    SpellId::MagicYieldSelf8,
];

/// ACE `SpellLevelProgression.LightWeaponsMasteryOther` (`List<SpellId>`).
pub static LIGHT_WEAPONS_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::LightWeaponsMasteryOther1,
    SpellId::LightWeaponsMasteryOther2,
    SpellId::LightWeaponsMasteryOther3,
    SpellId::LightWeaponsMasteryOther4,
    SpellId::LightWeaponsMasteryOther5,
    SpellId::LightWeaponsMasteryOther6,
    SpellId::LightWeaponsMasteryOther7,
    SpellId::LightWeaponsMasteryOther8,
];

/// ACE `SpellLevelProgression.LightWeaponsMasterySelf` (`List<SpellId>`).
pub static LIGHT_WEAPONS_MASTERY_SELF: [SpellId; 8] = [
    SpellId::LightWeaponsMasterySelf1,
    SpellId::LightWeaponsMasterySelf2,
    SpellId::LightWeaponsMasterySelf3,
    SpellId::LightWeaponsMasterySelf4,
    SpellId::LightWeaponsMasterySelf5,
    SpellId::LightWeaponsMasterySelf6,
    SpellId::LightWeaponsMasterySelf7,
    SpellId::LightWeaponsMasterySelf8,
];

/// ACE `SpellLevelProgression.LightWeaponsIneptitudeOther` (`List<SpellId>`).
pub static LIGHT_WEAPONS_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::LightWeaponsIneptitudeOther1,
    SpellId::LightWeaponsIneptitudeOther2,
    SpellId::LightWeaponsIneptitudeOther3,
    SpellId::LightWeaponsIneptitudeOther4,
    SpellId::LightWeaponsIneptitudeOther5,
    SpellId::LightWeaponsIneptitudeOther6,
    SpellId::LightWeaponsIneptitudeOther7,
    SpellId::LightWeaponsIneptitudeOther8,
];

/// ACE `SpellLevelProgression.LightWeaponsIneptitudeSelf` (`List<SpellId>`).
pub static LIGHT_WEAPONS_INEPTITUDE_SELF: [SpellId; 8] = [
    SpellId::LightWeaponsIneptitudeSelf1,
    SpellId::LightWeaponsIneptitudeSelf2,
    SpellId::LightWeaponsIneptitudeSelf3,
    SpellId::LightWeaponsIneptitudeSelf4,
    SpellId::LightWeaponsIneptitudeSelf5,
    SpellId::LightWeaponsIneptitudeSelf6,
    SpellId::LightWeaponsIneptitudeSelf7,
    SpellId::LightWeaponsIneptitudeSelf8,
];

/// ACE `SpellLevelProgression.FinesseWeaponsMasteryOther` (`List<SpellId>`).
pub static FINESSE_WEAPONS_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::FinesseWeaponsMasteryOther1,
    SpellId::FinesseWeaponsMasteryOther2,
    SpellId::FinesseWeaponsMasteryOther3,
    SpellId::FinesseWeaponsMasteryOther4,
    SpellId::FinesseWeaponsMasteryOther5,
    SpellId::FinesseWeaponsMasteryOther6,
    SpellId::FinesseWeaponsMasteryOther7,
    SpellId::FinesseWeaponsMasteryOther8,
];

/// ACE `SpellLevelProgression.FinesseWeaponsMasterySelf` (`List<SpellId>`).
pub static FINESSE_WEAPONS_MASTERY_SELF: [SpellId; 8] = [
    SpellId::FinesseWeaponsMasterySelf1,
    SpellId::FinesseWeaponsMasterySelf2,
    SpellId::FinesseWeaponsMasterySelf3,
    SpellId::FinesseWeaponsMasterySelf4,
    SpellId::FinesseWeaponsMasterySelf5,
    SpellId::FinesseWeaponsMasterySelf6,
    SpellId::FinesseWeaponsMasterySelf7,
    SpellId::FinesseWeaponsMasterySelf8,
];

/// ACE `SpellLevelProgression.FinesseWeaponsIneptitudeOther` (`List<SpellId>`).
pub static FINESSE_WEAPONS_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::FinesseWeaponsIneptitudeOther1,
    SpellId::FinesseWeaponsIneptitudeOther2,
    SpellId::FinesseWeaponsIneptitudeOther3,
    SpellId::FinesseWeaponsIneptitudeOther4,
    SpellId::FinesseWeaponsIneptitudeOther5,
    SpellId::FinesseWeaponsIneptitudeOther6,
    SpellId::FinesseWeaponsIneptitudeOther7,
    SpellId::FinesseWeaponsIneptitudeOther8,
];

/// ACE `SpellLevelProgression.FinesseWeaponsIneptitudeSelf` (`List<SpellId>`).
pub static FINESSE_WEAPONS_INEPTITUDE_SELF: [SpellId; 8] = [
    SpellId::FinesseWeaponsIneptitudeSelf1,
    SpellId::FinesseWeaponsIneptitudeSelf2,
    SpellId::FinesseWeaponsIneptitudeSelf3,
    SpellId::FinesseWeaponsIneptitudeSelf4,
    SpellId::FinesseWeaponsIneptitudeSelf5,
    SpellId::FinesseWeaponsIneptitudeSelf6,
    SpellId::FinesseWeaponsIneptitudeSelf7,
    SpellId::FinesseWeaponsIneptitudeSelf8,
];

/// ACE `SpellLevelProgression.MaceMasteryOther` (`List<SpellId>`).
pub static MACE_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::MaceMasteryOther1,
    SpellId::MaceMasteryOther2,
    SpellId::MaceMasteryOther3,
    SpellId::MaceMasteryOther4,
    SpellId::MaceMasteryOther5,
    SpellId::MaceMasteryOther6,
    SpellId::MaceMasteryOther7,
    SpellId::MaceMasteryOther8,
];

/// ACE `SpellLevelProgression.MaceMasterySelf` (`List<SpellId>`).
pub static MACE_MASTERY_SELF: [SpellId; 8] = [
    SpellId::MaceMasterySelf1,
    SpellId::MaceMasterySelf2,
    SpellId::MaceMasterySelf3,
    SpellId::MaceMasterySelf4,
    SpellId::MaceMasterySelf5,
    SpellId::MaceMasterySelf6,
    SpellId::MaceMasterySelf7,
    SpellId::MaceMasterySelf8,
];

/// ACE `SpellLevelProgression.MaceIneptitudeOther` (`List<SpellId>`).
pub static MACE_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::MaceIneptitudeOther1,
    SpellId::MaceIneptitudeOther2,
    SpellId::MaceIneptitudeOther3,
    SpellId::MaceIneptitudeOther4,
    SpellId::MaceIneptitudeOther5,
    SpellId::MaceIneptitudeOther6,
    SpellId::MaceIneptitudeOther7,
    SpellId::MaceIneptitudeOther8,
];

/// ACE `SpellLevelProgression.MaceIneptitudeSelf` (`List<SpellId>`).
pub static MACE_INEPTITUDE_SELF: [SpellId; 8] = [
    SpellId::MaceIneptitudeSelf1,
    SpellId::MaceIneptitudeSelf2,
    SpellId::MaceIneptitudeSelf3,
    SpellId::MaceIneptitudeSelf4,
    SpellId::MaceIneptitudeSelf5,
    SpellId::MaceIneptitudeSelf6,
    SpellId::MaceIneptitudeSelf7,
    SpellId::MaceIneptitudeSelf8,
];

/// ACE `SpellLevelProgression.SpearMasteryOther` (`List<SpellId>`).
pub static SPEAR_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::SpearMasteryOther1,
    SpellId::SpearMasteryOther2,
    SpellId::SpearMasteryOther3,
    SpellId::SpearMasteryOther4,
    SpellId::SpearMasteryOther5,
    SpellId::SpearMasteryOther6,
    SpellId::SpearMasteryOther7,
    SpellId::SpearMasteryOther8,
];

/// ACE `SpellLevelProgression.SpearMasterySelf` (`List<SpellId>`).
pub static SPEAR_MASTERY_SELF: [SpellId; 8] = [
    SpellId::SpearMasterySelf1,
    SpellId::SpearMasterySelf2,
    SpellId::SpearMasterySelf3,
    SpellId::SpearMasterySelf4,
    SpellId::SpearMasterySelf5,
    SpellId::SpearMasterySelf6,
    SpellId::SpearMasterySelf7,
    SpellId::SpearMasterySelf8,
];

/// ACE `SpellLevelProgression.SpearIneptitudeOther` (`List<SpellId>`).
pub static SPEAR_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::SpearIneptitudeOther1,
    SpellId::SpearIneptitudeOther2,
    SpellId::SpearIneptitudeOther3,
    SpellId::SpearIneptitudeOther4,
    SpellId::SpearIneptitudeOther5,
    SpellId::SpearIneptitudeOther6,
    SpellId::SpearIneptitudeOther7,
    SpellId::SpearIneptitudeOther8,
];

/// ACE `SpellLevelProgression.SpearIneptitudeSelf` (`List<SpellId>`).
pub static SPEAR_INEPTITUDE_SELF: [SpellId; 8] = [
    SpellId::SpearIneptitudeSelf1,
    SpellId::SpearIneptitudeSelf2,
    SpellId::SpearIneptitudeSelf3,
    SpellId::SpearIneptitudeSelf4,
    SpellId::SpearIneptitudeSelf5,
    SpellId::SpearIneptitudeSelf6,
    SpellId::SpearIneptitudeSelf7,
    SpellId::SpearIneptitudeSelf8,
];

/// ACE `SpellLevelProgression.StaffMasteryOther` (`List<SpellId>`).
pub static STAFF_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::StaffMasteryOther1,
    SpellId::StaffMasteryOther2,
    SpellId::StaffMasteryOther3,
    SpellId::StaffMasteryOther4,
    SpellId::StaffMasteryOther5,
    SpellId::StaffMasteryOther6,
    SpellId::StaffMasteryOther7,
    SpellId::StaffMasteryOther8,
];

/// ACE `SpellLevelProgression.StaffMasterySelf` (`List<SpellId>`).
pub static STAFF_MASTERY_SELF: [SpellId; 8] = [
    SpellId::StaffMasterySelf1,
    SpellId::StaffMasterySelf2,
    SpellId::StaffMasterySelf3,
    SpellId::StaffMasterySelf4,
    SpellId::StaffMasterySelf5,
    SpellId::StaffMasterySelf6,
    SpellId::StaffMasterySelf7,
    SpellId::StaffMasterySelf8,
];

/// ACE `SpellLevelProgression.StaffIneptitudeOther` (`List<SpellId>`).
pub static STAFF_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::StaffIneptitudeOther1,
    SpellId::StaffIneptitudeOther2,
    SpellId::StaffIneptitudeOther3,
    SpellId::StaffIneptitudeOther4,
    SpellId::StaffIneptitudeOther5,
    SpellId::StaffIneptitudeOther6,
    SpellId::StaffIneptitudeOther7,
    SpellId::StaffIneptitudeOther8,
];

/// ACE `SpellLevelProgression.StaffIneptitudeSelf` (`List<SpellId>`).
pub static STAFF_INEPTITUDE_SELF: [SpellId; 8] = [
    SpellId::StaffIneptitudeSelf1,
    SpellId::StaffIneptitudeSelf2,
    SpellId::StaffIneptitudeSelf3,
    SpellId::StaffIneptitudeSelf4,
    SpellId::StaffIneptitudeSelf5,
    SpellId::StaffIneptitudeSelf6,
    SpellId::StaffIneptitudeSelf7,
    SpellId::StaffIneptitudeSelf8,
];

/// ACE `SpellLevelProgression.HeavyWeaponsMasteryOther` (`List<SpellId>`).
pub static HEAVY_WEAPONS_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::HeavyWeaponsMasteryOther1,
    SpellId::HeavyWeaponsMasteryOther2,
    SpellId::HeavyWeaponsMasteryOther3,
    SpellId::HeavyWeaponsMasteryOther4,
    SpellId::HeavyWeaponsMasteryOther5,
    SpellId::HeavyWeaponsMasteryOther6,
    SpellId::HeavyWeaponsMasteryOther7,
    SpellId::HeavyWeaponsMasteryOther8,
];

/// ACE `SpellLevelProgression.HeavyWeaponsMasterySelf` (`List<SpellId>`).
pub static HEAVY_WEAPONS_MASTERY_SELF: [SpellId; 8] = [
    SpellId::HeavyWeaponsMasterySelf1,
    SpellId::HeavyWeaponsMasterySelf2,
    SpellId::HeavyWeaponsMasterySelf3,
    SpellId::HeavyWeaponsMasterySelf4,
    SpellId::HeavyWeaponsMasterySelf5,
    SpellId::HeavyWeaponsMasterySelf6,
    SpellId::HeavyWeaponsMasterySelf7,
    SpellId::HeavyWeaponsMasterySelf8,
];

/// ACE `SpellLevelProgression.HeavyWeaponsIneptitudeOther` (`List<SpellId>`).
pub static HEAVY_WEAPONS_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::HeavyWeaponsIneptitudeOther1,
    SpellId::HeavyWeaponsIneptitudeOther2,
    SpellId::HeavyWeaponsIneptitudeOther3,
    SpellId::HeavyWeaponsIneptitudeOther4,
    SpellId::HeavyWeaponsIneptitudeOther5,
    SpellId::HeavyWeaponsIneptitudeOther6,
    SpellId::HeavyWeaponsIneptitudeOther7,
    SpellId::HeavyWeaponsIneptitudeOther8,
];

/// ACE `SpellLevelProgression.HeavyWeaponsIneptitudeSelf` (`List<SpellId>`).
pub static HEAVY_WEAPONS_INEPTITUDE_SELF: [SpellId; 8] = [
    SpellId::HeavyWeaponsIneptitudeSelf1,
    SpellId::HeavyWeaponsIneptitudeSelf2,
    SpellId::HeavyWeaponsIneptitudeSelf3,
    SpellId::HeavyWeaponsIneptitudeSelf4,
    SpellId::HeavyWeaponsIneptitudeSelf5,
    SpellId::HeavyWeaponsIneptitudeSelf6,
    SpellId::HeavyWeaponsIneptitudeSelf7,
    SpellId::HeavyWeaponsIneptitudeSelf8,
];

/// ACE `SpellLevelProgression.UnarmedCombatMasteryOther` (`List<SpellId>`).
pub static UNARMED_COMBAT_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::UnarmedCombatMasteryOther1,
    SpellId::UnarmedCombatMasteryOther2,
    SpellId::UnarmedCombatMasteryOther3,
    SpellId::UnarmedCombatMasteryOther4,
    SpellId::UnarmedCombatMasteryOther5,
    SpellId::UnarmedCombatMasteryOther6,
    SpellId::UnarmedCombatMasteryOther7,
    SpellId::UnarmedCombatMasteryOther8,
];

/// ACE `SpellLevelProgression.UnarmedCombatMasterySelf` (`List<SpellId>`).
pub static UNARMED_COMBAT_MASTERY_SELF: [SpellId; 8] = [
    SpellId::UnarmedCombatMasterySelf1,
    SpellId::UnarmedCombatMasterySelf2,
    SpellId::UnarmedCombatMasterySelf3,
    SpellId::UnarmedCombatMasterySelf4,
    SpellId::UnarmedCombatMasterySelf5,
    SpellId::UnarmedCombatMasterySelf6,
    SpellId::UnarmedCombatMasterySelf7,
    SpellId::UnarmedCombatMasterySelf8,
];

/// ACE `SpellLevelProgression.UnarmedCombatIneptitudeOther` (`List<SpellId>`).
pub static UNARMED_COMBAT_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::UnarmedCombatIneptitudeOther1,
    SpellId::UnarmedCombatIneptitudeOther2,
    SpellId::UnarmedCombatIneptitudeOther3,
    SpellId::UnarmedCombatIneptitudeOther4,
    SpellId::UnarmedCombatIneptitudeOther5,
    SpellId::UnarmedCombatIneptitudeOther6,
    SpellId::UnarmedCombatIneptitudeOther7,
    SpellId::UnarmedCombatIneptitudeOther8,
];

/// ACE `SpellLevelProgression.UnarmedCombatIneptitudeSelf` (`List<SpellId>`).
pub static UNARMED_COMBAT_INEPTITUDE_SELF: [SpellId; 8] = [
    SpellId::UnarmedCombatIneptitudeSelf1,
    SpellId::UnarmedCombatIneptitudeSelf2,
    SpellId::UnarmedCombatIneptitudeSelf3,
    SpellId::UnarmedCombatIneptitudeSelf4,
    SpellId::UnarmedCombatIneptitudeSelf5,
    SpellId::UnarmedCombatIneptitudeSelf6,
    SpellId::UnarmedCombatIneptitudeSelf7,
    SpellId::UnarmedCombatIneptitudeSelf8,
];

/// ACE `SpellLevelProgression.MissileWeaponsMasteryOther` (`List<SpellId>`).
pub static MISSILE_WEAPONS_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::MissileWeaponsMasteryOther1,
    SpellId::MissileWeaponsMasteryOther2,
    SpellId::MissileWeaponsMasteryOther3,
    SpellId::MissileWeaponsMasteryOther4,
    SpellId::MissileWeaponsMasteryOther5,
    SpellId::MissileWeaponsMasteryOther6,
    SpellId::MissileWeaponsMasteryOther7,
    SpellId::MissileWeaponsMasteryOther8,
];

/// ACE `SpellLevelProgression.MissileWeaponsMasterySelf` (`List<SpellId>`).
pub static MISSILE_WEAPONS_MASTERY_SELF: [SpellId; 8] = [
    SpellId::MissileWeaponsMasterySelf1,
    SpellId::MissileWeaponsMasterySelf2,
    SpellId::MissileWeaponsMasterySelf3,
    SpellId::MissileWeaponsMasterySelf4,
    SpellId::MissileWeaponsMasterySelf5,
    SpellId::MissileWeaponsMasterySelf6,
    SpellId::MissileWeaponsMasterySelf7,
    SpellId::MissileWeaponsMasterySelf8,
];

/// ACE `SpellLevelProgression.MissileWeaponsIneptitudeOther` (`List<SpellId>`).
pub static MISSILE_WEAPONS_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::MissileWeaponsIneptitudeOther1,
    SpellId::MissileWeaponsIneptitudeOther2,
    SpellId::MissileWeaponsIneptitudeOther3,
    SpellId::MissileWeaponsIneptitudeOther4,
    SpellId::MissileWeaponsIneptitudeOther5,
    SpellId::MissileWeaponsIneptitudeOther6,
    SpellId::MissileWeaponsIneptitudeOther7,
    SpellId::MissileWeaponsIneptitudeOther8,
];

/// ACE `SpellLevelProgression.MissileWeaponsIneptitudeSelf` (`List<SpellId>`).
pub static MISSILE_WEAPONS_INEPTITUDE_SELF: [SpellId; 8] = [
    SpellId::MissileWeaponsIneptitudeSelf1,
    SpellId::MissileWeaponsIneptitudeSelf2,
    SpellId::MissileWeaponsIneptitudeSelf3,
    SpellId::MissileWeaponsIneptitudeSelf4,
    SpellId::MissileWeaponsIneptitudeSelf5,
    SpellId::MissileWeaponsIneptitudeSelf6,
    SpellId::MissileWeaponsIneptitudeSelf7,
    SpellId::MissileWeaponsIneptitudeSelf8,
];

/// ACE `SpellLevelProgression.CrossbowMasteryOther` (`List<SpellId>`).
pub static CROSSBOW_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::CrossbowMasteryOther1,
    SpellId::CrossbowMasteryOther2,
    SpellId::CrossbowMasteryOther3,
    SpellId::CrossbowMasteryOther4,
    SpellId::CrossbowMasteryOther5,
    SpellId::CrossbowMasteryOther6,
    SpellId::CrossbowMasteryOther7,
    SpellId::CrossbowMasteryOther8,
];

/// ACE `SpellLevelProgression.CrossbowMasterySelf` (`List<SpellId>`).
pub static CROSSBOW_MASTERY_SELF: [SpellId; 8] = [
    SpellId::CrossbowMasterySelf1,
    SpellId::CrossbowMasterySelf2,
    SpellId::CrossbowMasterySelf3,
    SpellId::CrossbowMasterySelf4,
    SpellId::CrossbowMasterySelf5,
    SpellId::CrossbowMasterySelf6,
    SpellId::CrossbowMasterySelf7,
    SpellId::CrossbowMasterySelf8,
];

/// ACE `SpellLevelProgression.CrossbowIneptitudeOther` (`List<SpellId>`).
pub static CROSSBOW_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::CrossbowIneptitudeOther1,
    SpellId::CrossbowIneptitudeOther2,
    SpellId::CrossbowIneptitudeOther3,
    SpellId::CrossbowIneptitudeOther4,
    SpellId::CrossbowIneptitudeOther5,
    SpellId::CrossbowIneptitudeOther6,
    SpellId::CrossbowIneptitudeOther7,
    SpellId::CrossbowIneptitudeOther8,
];

/// ACE `SpellLevelProgression.CrossbowIneptitudeSelf` (`List<SpellId>`).
pub static CROSSBOW_INEPTITUDE_SELF: [SpellId; 8] = [
    SpellId::CrossbowIneptitudeSelf1,
    SpellId::CrossbowIneptitudeSelf2,
    SpellId::CrossbowIneptitudeSelf3,
    SpellId::CrossbowIneptitudeSelf4,
    SpellId::CrossbowIneptitudeSelf5,
    SpellId::CrossbowIneptitudeSelf6,
    SpellId::CrossbowIneptitudeSelf7,
    SpellId::CrossbowIneptitudeSelf8,
];

/// ACE `SpellLevelProgression.AcidProtectionOther` (`List<SpellId>`).
pub static ACID_PROTECTION_OTHER: [SpellId; 8] = [
    SpellId::AcidProtectionOther1,
    SpellId::AcidProtectionOther2,
    SpellId::AcidProtectionOther3,
    SpellId::AcidProtectionOther4,
    SpellId::AcidProtectionOther5,
    SpellId::AcidProtectionOther6,
    SpellId::AcidProtectionOther7,
    SpellId::AcidProtectionOther8,
];

/// ACE `SpellLevelProgression.AcidProtectionSelf` (`List<SpellId>`).
pub static ACID_PROTECTION_SELF: [SpellId; 8] = [
    SpellId::AcidProtectionSelf1,
    SpellId::AcidProtectionSelf2,
    SpellId::AcidProtectionSelf3,
    SpellId::AcidProtectionSelf4,
    SpellId::AcidProtectionSelf5,
    SpellId::AcidProtectionSelf6,
    SpellId::AcidProtectionSelf7,
    SpellId::AcidProtectionSelf8,
];

/// ACE `SpellLevelProgression.AcidVulnerabilityOther` (`List<SpellId>`).
pub static ACID_VULNERABILITY_OTHER: [SpellId; 8] = [
    SpellId::AcidVulnerabilityOther1,
    SpellId::AcidVulnerabilityOther2,
    SpellId::AcidVulnerabilityOther3,
    SpellId::AcidVulnerabilityOther4,
    SpellId::AcidVulnerabilityOther5,
    SpellId::AcidVulnerabilityOther6,
    SpellId::AcidVulnerabilityOther7,
    SpellId::AcidVulnerabilityOther8,
];

/// ACE `SpellLevelProgression.AcidVulnerabilitySelf` (`List<SpellId>`).
pub static ACID_VULNERABILITY_SELF: [SpellId; 8] = [
    SpellId::AcidVulnerabilitySelf1,
    SpellId::AcidVulnerabilitySelf2,
    SpellId::AcidVulnerabilitySelf3,
    SpellId::AcidVulnerabilitySelf4,
    SpellId::AcidVulnerabilitySelf5,
    SpellId::AcidVulnerabilitySelf6,
    SpellId::AcidVulnerabilitySelf7,
    SpellId::AcidVulnerabilitySelf8,
];

/// ACE `SpellLevelProgression.ThrownWeaponMasteryOther` (`List<SpellId>`).
pub static THROWN_WEAPON_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::ThrownWeaponMasteryOther1,
    SpellId::ThrownWeaponMasteryOther2,
    SpellId::ThrownWeaponMasteryOther3,
    SpellId::ThrownWeaponMasteryOther4,
    SpellId::ThrownWeaponMasteryOther5,
    SpellId::ThrownWeaponMasteryOther6,
    SpellId::ThrownWeaponMasteryOther7,
    SpellId::ThrownWeaponMasteryOther8,
];

/// ACE `SpellLevelProgression.ThrownWeaponMasterySelf` (`List<SpellId>`).
pub static THROWN_WEAPON_MASTERY_SELF: [SpellId; 8] = [
    SpellId::ThrownWeaponMasterySelf1,
    SpellId::ThrownWeaponMasterySelf2,
    SpellId::ThrownWeaponMasterySelf3,
    SpellId::ThrownWeaponMasterySelf4,
    SpellId::ThrownWeaponMasterySelf5,
    SpellId::ThrownWeaponMasterySelf6,
    SpellId::ThrownWeaponMasterySelf7,
    SpellId::ThrownWeaponMasterySelf8,
];

/// ACE `SpellLevelProgression.ThrownWeaponIneptitudeOther` (`List<SpellId>`).
pub static THROWN_WEAPON_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::ThrownWeaponIneptitudeOther1,
    SpellId::ThrownWeaponIneptitudeOther2,
    SpellId::ThrownWeaponIneptitudeOther3,
    SpellId::ThrownWeaponIneptitudeOther4,
    SpellId::ThrownWeaponIneptitudeOther5,
    SpellId::ThrownWeaponIneptitudeOther6,
    SpellId::ThrownWeaponIneptitudeOther7,
    SpellId::ThrownWeaponIneptitudeOther8,
];

/// ACE `SpellLevelProgression.ThrownWeaponIneptitudeSelf` (`List<SpellId>`).
pub static THROWN_WEAPON_INEPTITUDE_SELF: [SpellId; 8] = [
    SpellId::ThrownWeaponIneptitudeSelf1,
    SpellId::ThrownWeaponIneptitudeSelf2,
    SpellId::ThrownWeaponIneptitudeSelf3,
    SpellId::ThrownWeaponIneptitudeSelf4,
    SpellId::ThrownWeaponIneptitudeSelf5,
    SpellId::ThrownWeaponIneptitudeSelf6,
    SpellId::ThrownWeaponIneptitudeSelf7,
    SpellId::ThrownWeaponIneptitudeSelf8,
];

/// ACE `SpellLevelProgression.CreatureEnchantmentMasterySelf` (`List<SpellId>`).
pub static CREATURE_ENCHANTMENT_MASTERY_SELF: [SpellId; 8] = [
    SpellId::CreatureEnchantmentMasterySelf1,
    SpellId::CreatureEnchantmentMasterySelf2,
    SpellId::CreatureEnchantmentMasterySelf3,
    SpellId::CreatureEnchantmentMasterySelf4,
    SpellId::CreatureEnchantmentMasterySelf5,
    SpellId::CreatureEnchantmentMasterySelf6,
    SpellId::CreatureEnchantmentMasterySelf7,
    SpellId::CreatureEnchantmentMasterySelf8,
];

/// ACE `SpellLevelProgression.CreatureEnchantmentMasteryOther` (`List<SpellId>`).
pub static CREATURE_ENCHANTMENT_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::CreatureEnchantmentMasteryOther1,
    SpellId::CreatureEnchantmentMasteryOther2,
    SpellId::CreatureEnchantmentMasteryOther3,
    SpellId::CreatureEnchantmentMasteryOther4,
    SpellId::CreatureEnchantmentMasteryOther5,
    SpellId::CreatureEnchantmentMasteryOther6,
    SpellId::CreatureEnchantmentMasteryOther7,
    SpellId::CreatureEnchantmentMasteryOther8,
];

/// ACE `SpellLevelProgression.CreatureEnchantmentIneptitudeOther` (`List<SpellId>`).
pub static CREATURE_ENCHANTMENT_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::CreatureEnchantmentIneptitudeOther1,
    SpellId::CreatureEnchantmentIneptitudeOther2,
    SpellId::CreatureEnchantmentIneptitudeOther3,
    SpellId::CreatureEnchantmentIneptitudeOther4,
    SpellId::CreatureEnchantmentIneptitudeOther5,
    SpellId::CreatureEnchantmentIneptitudeOther6,
    SpellId::CreatureEnchantmentIneptitudeOther7,
    SpellId::CreatureEnchantmentIneptitudeOther8,
];

/// ACE `SpellLevelProgression.CreatureEnchantmentIneptitudeSelf` (`List<SpellId>`).
pub static CREATURE_ENCHANTMENT_INEPTITUDE_SELF: [SpellId; 8] = [
    SpellId::CreatureEnchantmentIneptitudeSelf1,
    SpellId::CreatureEnchantmentIneptitudeSelf2,
    SpellId::CreatureEnchantmentIneptitudeSelf3,
    SpellId::CreatureEnchantmentIneptitudeSelf4,
    SpellId::CreatureEnchantmentIneptitudeSelf5,
    SpellId::CreatureEnchantmentIneptitudeSelf6,
    SpellId::CreatureEnchantmentIneptitudeSelf7,
    SpellId::CreatureEnchantmentIneptitudeSelf8,
];

/// ACE `SpellLevelProgression.ItemEnchantmentMasterySelf` (`List<SpellId>`).
pub static ITEM_ENCHANTMENT_MASTERY_SELF: [SpellId; 8] = [
    SpellId::ItemEnchantmentMasterySelf1,
    SpellId::ItemEnchantmentMasterySelf2,
    SpellId::ItemEnchantmentMasterySelf3,
    SpellId::ItemEnchantmentMasterySelf4,
    SpellId::ItemEnchantmentMasterySelf5,
    SpellId::ItemEnchantmentMasterySelf6,
    SpellId::ItemEnchantmentMasterySelf7,
    SpellId::ItemEnchantmentMasterySelf8,
];

/// ACE `SpellLevelProgression.ItemEnchantmentMasteryOther` (`List<SpellId>`).
pub static ITEM_ENCHANTMENT_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::ItemEnchantmentMasteryOther1,
    SpellId::ItemEnchantmentMasteryOther2,
    SpellId::ItemEnchantmentMasteryOther3,
    SpellId::ItemEnchantmentMasteryOther4,
    SpellId::ItemEnchantmentMasteryOther5,
    SpellId::ItemEnchantmentMasteryOther6,
    SpellId::ItemEnchantmentMasteryOther7,
    SpellId::ItemEnchantmentMasteryOther8,
];

/// ACE `SpellLevelProgression.ItemEnchantmentIneptitudeOther` (`List<SpellId>`).
pub static ITEM_ENCHANTMENT_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::ItemEnchantmentIneptitudeOther1,
    SpellId::ItemEnchantmentIneptitudeOther2,
    SpellId::ItemEnchantmentIneptitudeOther3,
    SpellId::ItemEnchantmentIneptitudeOther4,
    SpellId::ItemEnchantmentIneptitudeOther5,
    SpellId::ItemEnchantmentIneptitudeOther6,
    SpellId::ItemEnchantmentIneptitudeOther7,
    SpellId::ItemEnchantmentIneptitudeOther8,
];

/// ACE `SpellLevelProgression.ItemEnchantmentIneptitudeSelf` (`List<SpellId>`).
pub static ITEM_ENCHANTMENT_INEPTITUDE_SELF: [SpellId; 8] = [
    SpellId::ItemEnchantmentIneptitudeSelf1,
    SpellId::ItemEnchantmentIneptitudeSelf2,
    SpellId::ItemEnchantmentIneptitudeSelf3,
    SpellId::ItemEnchantmentIneptitudeSelf4,
    SpellId::ItemEnchantmentIneptitudeSelf5,
    SpellId::ItemEnchantmentIneptitudeSelf6,
    SpellId::ItemEnchantmentIneptitudeSelf7,
    SpellId::ItemEnchantmentIneptitudeSelf8,
];

/// ACE `SpellLevelProgression.LifeMagicMasterySelf` (`List<SpellId>`).
pub static LIFE_MAGIC_MASTERY_SELF: [SpellId; 8] = [
    SpellId::LifeMagicMasterySelf1,
    SpellId::LifeMagicMasterySelf2,
    SpellId::LifeMagicMasterySelf3,
    SpellId::LifeMagicMasterySelf4,
    SpellId::LifeMagicMasterySelf5,
    SpellId::LifeMagicMasterySelf6,
    SpellId::LifeMagicMasterySelf7,
    SpellId::LifeMagicMasterySelf8,
];

/// ACE `SpellLevelProgression.LifeMagicMasteryOther` (`List<SpellId>`).
pub static LIFE_MAGIC_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::LifeMagicMasteryOther1,
    SpellId::LifeMagicMasteryOther2,
    SpellId::LifeMagicMasteryOther3,
    SpellId::LifeMagicMasteryOther4,
    SpellId::LifeMagicMasteryOther5,
    SpellId::LifeMagicMasteryOther6,
    SpellId::LifeMagicMasteryOther7,
    SpellId::LifeMagicMasteryOther8,
];

/// ACE `SpellLevelProgression.LifeMagicIneptitudeSelf` (`List<SpellId>`).
pub static LIFE_MAGIC_INEPTITUDE_SELF: [SpellId; 8] = [
    SpellId::LifeMagicIneptitudeSelf1,
    SpellId::LifeMagicIneptitudeSelf2,
    SpellId::LifeMagicIneptitudeSelf3,
    SpellId::LifeMagicIneptitudeSelf4,
    SpellId::LifeMagicIneptitudeSelf5,
    SpellId::LifeMagicIneptitudeSelf6,
    SpellId::LifeMagicIneptitudeSelf7,
    SpellId::LifeMagicIneptitudeSelf8,
];

/// ACE `SpellLevelProgression.LifeMagicIneptitudeOther` (`List<SpellId>`).
pub static LIFE_MAGIC_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::LifeMagicIneptitudeOther1,
    SpellId::LifeMagicIneptitudeOther2,
    SpellId::LifeMagicIneptitudeOther3,
    SpellId::LifeMagicIneptitudeOther4,
    SpellId::LifeMagicIneptitudeOther5,
    SpellId::LifeMagicIneptitudeOther6,
    SpellId::LifeMagicIneptitudeOther7,
    SpellId::LifeMagicIneptitudeOther8,
];

/// ACE `SpellLevelProgression.WarMagicMasterySelf` (`List<SpellId>`).
pub static WAR_MAGIC_MASTERY_SELF: [SpellId; 8] = [
    SpellId::WarMagicMasterySelf1,
    SpellId::WarMagicMasterySelf2,
    SpellId::WarMagicMasterySelf3,
    SpellId::WarMagicMasterySelf4,
    SpellId::WarMagicMasterySelf5,
    SpellId::WarMagicMasterySelf6,
    SpellId::WarMagicMasterySelf7,
    SpellId::WarMagicMasterySelf8,
];

/// ACE `SpellLevelProgression.WarMagicMasteryOther` (`List<SpellId>`).
pub static WAR_MAGIC_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::WarMagicMasteryOther1,
    SpellId::WarMagicMasteryOther2,
    SpellId::WarMagicMasteryOther3,
    SpellId::WarMagicMasteryOther4,
    SpellId::WarMagicMasteryOther5,
    SpellId::WarMagicMasteryOther6,
    SpellId::WarMagicMasteryOther7,
    SpellId::WarMagicMasteryOther8,
];

/// ACE `SpellLevelProgression.WarMagicIneptitudeSelf` (`List<SpellId>`).
pub static WAR_MAGIC_INEPTITUDE_SELF: [SpellId; 8] = [
    SpellId::WarMagicIneptitudeSelf1,
    SpellId::WarMagicIneptitudeSelf2,
    SpellId::WarMagicIneptitudeSelf3,
    SpellId::WarMagicIneptitudeSelf4,
    SpellId::WarMagicIneptitudeSelf5,
    SpellId::WarMagicIneptitudeSelf6,
    SpellId::WarMagicIneptitudeSelf7,
    SpellId::WarMagicIneptitudeSelf8,
];

/// ACE `SpellLevelProgression.WarMagicIneptitudeOther` (`List<SpellId>`).
pub static WAR_MAGIC_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::WarMagicIneptitudeOther1,
    SpellId::WarMagicIneptitudeOther2,
    SpellId::WarMagicIneptitudeOther3,
    SpellId::WarMagicIneptitudeOther4,
    SpellId::WarMagicIneptitudeOther5,
    SpellId::WarMagicIneptitudeOther6,
    SpellId::WarMagicIneptitudeOther7,
    SpellId::WarMagicIneptitudeOther8,
];

/// ACE `SpellLevelProgression.ManaMasterySelf` (`List<SpellId>`).
pub static MANA_MASTERY_SELF: [SpellId; 8] = [
    SpellId::ManaMasterySelf1,
    SpellId::ManaMasterySelf2,
    SpellId::ManaMasterySelf3,
    SpellId::ManaMasterySelf4,
    SpellId::ManaMasterySelf5,
    SpellId::ManaMasterySelf6,
    SpellId::ManaMasterySelf7,
    SpellId::ManaMasterySelf8,
];

/// ACE `SpellLevelProgression.ManaMasteryOther` (`List<SpellId>`).
pub static MANA_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::ManaMasteryOther1,
    SpellId::ManaMasteryOther2,
    SpellId::ManaMasteryOther3,
    SpellId::ManaMasteryOther4,
    SpellId::ManaMasteryOther5,
    SpellId::ManaMasteryOther6,
    SpellId::ManaMasteryOther7,
    SpellId::ManaMasteryOther8,
];

/// ACE `SpellLevelProgression.ManaIneptitudeSelf` (`List<SpellId>`).
pub static MANA_INEPTITUDE_SELF: [SpellId; 8] = [
    SpellId::ManaIneptitudeSelf1,
    SpellId::ManaIneptitudeSelf2,
    SpellId::ManaIneptitudeSelf3,
    SpellId::ManaIneptitudeSelf4,
    SpellId::ManaIneptitudeSelf5,
    SpellId::ManaIneptitudeSelf6,
    SpellId::ManaIneptitudeSelf7,
    SpellId::ManaIneptitudeSelf8,
];

/// ACE `SpellLevelProgression.ManaIneptitudeOther` (`List<SpellId>`).
pub static MANA_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::ManaIneptitudeOther1,
    SpellId::ManaIneptitudeOther2,
    SpellId::ManaIneptitudeOther3,
    SpellId::ManaIneptitudeOther4,
    SpellId::ManaIneptitudeOther5,
    SpellId::ManaIneptitudeOther6,
    SpellId::ManaIneptitudeOther7,
    SpellId::ManaIneptitudeOther8,
];

/// ACE `SpellLevelProgression.ArcaneEnlightenmentSelf` (`List<SpellId>`).
pub static ARCANE_ENLIGHTENMENT_SELF: [SpellId; 8] = [
    SpellId::ArcaneEnlightenmentSelf1,
    SpellId::ArcaneEnlightenmentSelf2,
    SpellId::ArcaneEnlightenmentSelf3,
    SpellId::ArcaneEnlightenmentSelf4,
    SpellId::ArcaneEnlightenmentSelf5,
    SpellId::ArcaneEnlightenmentSelf6,
    SpellId::ArcaneEnlightenmentSelf7,
    SpellId::ArcaneEnlightenmentSelf8,
];

/// ACE `SpellLevelProgression.ArcaneEnlightenmentOther` (`List<SpellId>`).
pub static ARCANE_ENLIGHTENMENT_OTHER: [SpellId; 8] = [
    SpellId::ArcaneEnlightenmentOther1,
    SpellId::ArcaneEnlightenmentOther2,
    SpellId::ArcaneEnlightenmentOther3,
    SpellId::ArcaneEnlightenmentOther4,
    SpellId::ArcaneEnlightenmentOther5,
    SpellId::ArcaneEnlightenmentOther6,
    SpellId::ArcaneEnlightenmentOther7,
    SpellId::ArcaneEnlightenmentOther8,
];

/// ACE `SpellLevelProgression.ArcaneBenightednessSelf` (`List<SpellId>`).
pub static ARCANE_BENIGHTEDNESS_SELF: [SpellId; 8] = [
    SpellId::ArcaneBenightednessSelf1,
    SpellId::ArcaneBenightednessSelf2,
    SpellId::ArcaneBenightednessSelf3,
    SpellId::ArcaneBenightednessSelf4,
    SpellId::ArcaneBenightednessSelf5,
    SpellId::ArcaneBenightednessSelf6,
    SpellId::ArcaneBenightednessSelf7,
    SpellId::ArcaneBenightednessSelf8,
];

/// ACE `SpellLevelProgression.ArcaneBenightednessOther` (`List<SpellId>`).
pub static ARCANE_BENIGHTEDNESS_OTHER: [SpellId; 8] = [
    SpellId::ArcaneBenightednessOther1,
    SpellId::ArcaneBenightednessOther2,
    SpellId::ArcaneBenightednessOther3,
    SpellId::ArcaneBenightednessOther4,
    SpellId::ArcaneBenightednessOther5,
    SpellId::ArcaneBenightednessOther6,
    SpellId::ArcaneBenightednessOther7,
    SpellId::ArcaneBenightednessOther8,
];

/// ACE `SpellLevelProgression.ArmorExpertiseSelf` (`List<SpellId>`).
pub static ARMOR_EXPERTISE_SELF: [SpellId; 8] = [
    SpellId::ArmorExpertiseSelf1,
    SpellId::ArmorExpertiseSelf2,
    SpellId::ArmorExpertiseSelf3,
    SpellId::ArmorExpertiseSelf4,
    SpellId::ArmorExpertiseSelf5,
    SpellId::ArmorExpertiseSelf6,
    SpellId::ArmorExpertiseSelf7,
    SpellId::ArmorExpertiseSelf8,
];

/// ACE `SpellLevelProgression.ArmorExpertiseOther` (`List<SpellId>`).
pub static ARMOR_EXPERTISE_OTHER: [SpellId; 8] = [
    SpellId::ArmorExpertiseOther1,
    SpellId::ArmorExpertiseOther2,
    SpellId::ArmorExpertiseOther3,
    SpellId::ArmorExpertiseOther4,
    SpellId::ArmorExpertiseOther5,
    SpellId::ArmorExpertiseOther6,
    SpellId::ArmorExpertiseOther7,
    SpellId::ArmorExpertiseOther8,
];

/// ACE `SpellLevelProgression.ArmorIgnoranceSelf` (`List<SpellId>`).
pub static ARMOR_IGNORANCE_SELF: [SpellId; 8] = [
    SpellId::ArmorIgnoranceSelf1,
    SpellId::ArmorIgnoranceSelf2,
    SpellId::ArmorIgnoranceSelf3,
    SpellId::ArmorIgnoranceSelf4,
    SpellId::ArmorIgnoranceSelf5,
    SpellId::ArmorIgnoranceSelf6,
    SpellId::ArmorIgnoranceSelf7,
    SpellId::ArmorIgnoranceSelf8,
];

/// ACE `SpellLevelProgression.ArmorIgnoranceOther` (`List<SpellId>`).
pub static ARMOR_IGNORANCE_OTHER: [SpellId; 8] = [
    SpellId::ArmorIgnoranceOther1,
    SpellId::ArmorIgnoranceOther2,
    SpellId::ArmorIgnoranceOther3,
    SpellId::ArmorIgnoranceOther4,
    SpellId::ArmorIgnoranceOther5,
    SpellId::ArmorIgnoranceOther6,
    SpellId::ArmorIgnoranceOther7,
    SpellId::ArmorIgnoranceOther8,
];

/// ACE `SpellLevelProgression.ItemExpertiseSelf` (`List<SpellId>`).
pub static ITEM_EXPERTISE_SELF: [SpellId; 8] = [
    SpellId::ItemExpertiseSelf1,
    SpellId::ItemExpertiseSelf2,
    SpellId::ItemExpertiseSelf3,
    SpellId::ItemExpertiseSelf4,
    SpellId::ItemExpertiseSelf5,
    SpellId::ItemExpertiseSelf6,
    SpellId::ItemExpertiseSelf7,
    SpellId::ItemExpertiseSelf8,
];

/// ACE `SpellLevelProgression.ItemExpertiseOther` (`List<SpellId>`).
pub static ITEM_EXPERTISE_OTHER: [SpellId; 8] = [
    SpellId::ItemExpertiseOther1,
    SpellId::ItemExpertiseOther2,
    SpellId::ItemExpertiseOther3,
    SpellId::ItemExpertiseOther4,
    SpellId::ItemExpertiseOther5,
    SpellId::ItemExpertiseOther6,
    SpellId::ItemExpertiseOther7,
    SpellId::ItemExpertiseOther8,
];

/// ACE `SpellLevelProgression.ItemIgnoranceSelf` (`List<SpellId>`).
pub static ITEM_IGNORANCE_SELF: [SpellId; 8] = [
    SpellId::ItemIgnoranceSelf1,
    SpellId::ItemIgnoranceSelf2,
    SpellId::ItemIgnoranceSelf3,
    SpellId::ItemIgnoranceSelf4,
    SpellId::ItemIgnoranceSelf5,
    SpellId::ItemIgnoranceSelf6,
    SpellId::ItemIgnoranceSelf7,
    SpellId::ItemIgnoranceSelf8,
];

/// ACE `SpellLevelProgression.ItemIgnoranceOther` (`List<SpellId>`).
pub static ITEM_IGNORANCE_OTHER: [SpellId; 8] = [
    SpellId::ItemIgnoranceOther1,
    SpellId::ItemIgnoranceOther2,
    SpellId::ItemIgnoranceOther3,
    SpellId::ItemIgnoranceOther4,
    SpellId::ItemIgnoranceOther5,
    SpellId::ItemIgnoranceOther6,
    SpellId::ItemIgnoranceOther7,
    SpellId::ItemIgnoranceOther8,
];

/// ACE `SpellLevelProgression.MagicItemExpertiseSelf` (`List<SpellId>`).
pub static MAGIC_ITEM_EXPERTISE_SELF: [SpellId; 8] = [
    SpellId::MagicItemExpertiseSelf1,
    SpellId::MagicItemExpertiseSelf2,
    SpellId::MagicItemExpertiseSelf3,
    SpellId::MagicItemExpertiseSelf4,
    SpellId::MagicItemExpertiseSelf5,
    SpellId::MagicItemExpertiseSelf6,
    SpellId::MagicItemExpertiseSelf7,
    SpellId::MagicItemExpertiseSelf8,
];

/// ACE `SpellLevelProgression.MagicItemExpertiseOther` (`List<SpellId>`).
pub static MAGIC_ITEM_EXPERTISE_OTHER: [SpellId; 8] = [
    SpellId::MagicItemExpertiseOther1,
    SpellId::MagicItemExpertiseOther2,
    SpellId::MagicItemExpertiseOther3,
    SpellId::MagicItemExpertiseOther4,
    SpellId::MagicItemExpertiseOther5,
    SpellId::MagicItemExpertiseOther6,
    SpellId::MagicItemExpertiseOther7,
    SpellId::MagicItemExpertiseOther8,
];

/// ACE `SpellLevelProgression.MagicItemIgnoranceSelf` (`List<SpellId>`).
pub static MAGIC_ITEM_IGNORANCE_SELF: [SpellId; 8] = [
    SpellId::MagicItemIgnoranceSelf1,
    SpellId::MagicItemIgnoranceSelf2,
    SpellId::MagicItemIgnoranceSelf3,
    SpellId::MagicItemIgnoranceSelf4,
    SpellId::MagicItemIgnoranceSelf5,
    SpellId::MagicItemIgnoranceSelf6,
    SpellId::MagicItemIgnoranceSelf7,
    SpellId::MagicItemIgnoranceSelf8,
];

/// ACE `SpellLevelProgression.MagicItemIgnoranceOther` (`List<SpellId>`).
pub static MAGIC_ITEM_IGNORANCE_OTHER: [SpellId; 8] = [
    SpellId::MagicItemIgnoranceOther1,
    SpellId::MagicItemIgnoranceOther2,
    SpellId::MagicItemIgnoranceOther3,
    SpellId::MagicItemIgnoranceOther4,
    SpellId::MagicItemIgnoranceOther5,
    SpellId::MagicItemIgnoranceOther6,
    SpellId::MagicItemIgnoranceOther7,
    SpellId::MagicItemIgnoranceOther8,
];

/// ACE `SpellLevelProgression.WeaponExpertiseSelf` (`List<SpellId>`).
pub static WEAPON_EXPERTISE_SELF: [SpellId; 8] = [
    SpellId::WeaponExpertiseSelf1,
    SpellId::WeaponExpertiseSelf2,
    SpellId::WeaponExpertiseSelf3,
    SpellId::WeaponExpertiseSelf4,
    SpellId::WeaponExpertiseSelf5,
    SpellId::WeaponExpertiseSelf6,
    SpellId::WeaponExpertiseSelf7,
    SpellId::WeaponExpertiseSelf8,
];

/// ACE `SpellLevelProgression.WeaponExpertiseOther` (`List<SpellId>`).
pub static WEAPON_EXPERTISE_OTHER: [SpellId; 8] = [
    SpellId::WeaponExpertiseOther1,
    SpellId::WeaponExpertiseOther2,
    SpellId::WeaponExpertiseOther3,
    SpellId::WeaponExpertiseOther4,
    SpellId::WeaponExpertiseOther5,
    SpellId::WeaponExpertiseOther6,
    SpellId::WeaponExpertiseOther7,
    SpellId::WeaponExpertiseOther8,
];

/// ACE `SpellLevelProgression.WeaponIgnoranceSelf` (`List<SpellId>`).
pub static WEAPON_IGNORANCE_SELF: [SpellId; 8] = [
    SpellId::WeaponIgnoranceSelf1,
    SpellId::WeaponIgnoranceSelf2,
    SpellId::WeaponIgnoranceSelf3,
    SpellId::WeaponIgnoranceSelf4,
    SpellId::WeaponIgnoranceSelf5,
    SpellId::WeaponIgnoranceSelf6,
    SpellId::WeaponIgnoranceSelf7,
    SpellId::WeaponIgnoranceSelf8,
];

/// ACE `SpellLevelProgression.WeaponIgnoranceOther` (`List<SpellId>`).
pub static WEAPON_IGNORANCE_OTHER: [SpellId; 8] = [
    SpellId::WeaponIgnoranceOther1,
    SpellId::WeaponIgnoranceOther2,
    SpellId::WeaponIgnoranceOther3,
    SpellId::WeaponIgnoranceOther4,
    SpellId::WeaponIgnoranceOther5,
    SpellId::WeaponIgnoranceOther6,
    SpellId::WeaponIgnoranceOther7,
    SpellId::WeaponIgnoranceOther8,
];

/// ACE `SpellLevelProgression.MonsterAttunementSelf` (`List<SpellId>`).
pub static MONSTER_ATTUNEMENT_SELF: [SpellId; 8] = [
    SpellId::MonsterAttunementSelf1,
    SpellId::MonsterAttunementSelf2,
    SpellId::MonsterAttunementSelf3,
    SpellId::MonsterAttunementSelf4,
    SpellId::MonsterAttunementSelf5,
    SpellId::MonsterAttunementSelf6,
    SpellId::MonsterAttunementSelf7,
    SpellId::MonsterAttunementSelf8,
];

/// ACE `SpellLevelProgression.MonsterAttunementOther` (`List<SpellId>`).
pub static MONSTER_ATTUNEMENT_OTHER: [SpellId; 8] = [
    SpellId::MonsterAttunementOther1,
    SpellId::MonsterAttunementOther2,
    SpellId::MonsterAttunementOther3,
    SpellId::MonsterAttunementOther4,
    SpellId::MonsterAttunementOther5,
    SpellId::MonsterAttunementOther6,
    SpellId::MonsterAttunementOther7,
    SpellId::MonsterAttunementOther8,
];

/// ACE `SpellLevelProgression.MonsterUnfamiliaritySelf` (`List<SpellId>`).
pub static MONSTER_UNFAMILIARITY_SELF: [SpellId; 8] = [
    SpellId::MonsterUnfamiliaritySelf1,
    SpellId::MonsterUnfamiliaritySelf2,
    SpellId::MonsterUnfamiliaritySelf3,
    SpellId::MonsterUnfamiliaritySelf4,
    SpellId::MonsterUnfamiliaritySelf5,
    SpellId::MonsterUnfamiliaritySelf6,
    SpellId::MonsterUnfamiliaritySelf7,
    SpellId::MonsterUnfamiliaritySelf8,
];

/// ACE `SpellLevelProgression.MonsterUnfamiliarityOther` (`List<SpellId>`).
pub static MONSTER_UNFAMILIARITY_OTHER: [SpellId; 8] = [
    SpellId::MonsterUnfamiliarityOther1,
    SpellId::MonsterUnfamiliarityOther2,
    SpellId::MonsterUnfamiliarityOther3,
    SpellId::MonsterUnfamiliarityOther4,
    SpellId::MonsterUnfamiliarityOther5,
    SpellId::MonsterUnfamiliarityOther6,
    SpellId::MonsterUnfamiliarityOther7,
    SpellId::MonsterUnfamiliarityOther8,
];

/// ACE `SpellLevelProgression.PersonAttunementSelf` (`List<SpellId>`).
pub static PERSON_ATTUNEMENT_SELF: [SpellId; 8] = [
    SpellId::PersonAttunementSelf1,
    SpellId::PersonAttunementSelf2,
    SpellId::PersonAttunementSelf3,
    SpellId::PersonAttunementSelf4,
    SpellId::PersonAttunementSelf5,
    SpellId::PersonAttunementSelf6,
    SpellId::PersonAttunementSelf7,
    SpellId::PersonAttunementSelf8,
];

/// ACE `SpellLevelProgression.PersonAttunementOther` (`List<SpellId>`).
pub static PERSON_ATTUNEMENT_OTHER: [SpellId; 8] = [
    SpellId::PersonAttunementOther1,
    SpellId::PersonAttunementOther2,
    SpellId::PersonAttunementOther3,
    SpellId::PersonAttunementOther4,
    SpellId::PersonAttunementOther5,
    SpellId::PersonAttunementOther6,
    SpellId::PersonAttunementOther7,
    SpellId::PersonAttunementOther8,
];

/// ACE `SpellLevelProgression.PersonUnfamiliaritySelf` (`List<SpellId>`).
pub static PERSON_UNFAMILIARITY_SELF: [SpellId; 8] = [
    SpellId::PersonUnfamiliaritySelf1,
    SpellId::PersonUnfamiliaritySelf2,
    SpellId::PersonUnfamiliaritySelf3,
    SpellId::PersonUnfamiliaritySelf4,
    SpellId::PersonUnfamiliaritySelf5,
    SpellId::PersonUnfamiliaritySelf6,
    SpellId::PersonUnfamiliaritySelf7,
    SpellId::PersonUnfamiliaritySelf8,
];

/// ACE `SpellLevelProgression.PersonUnfamiliarityOther` (`List<SpellId>`).
pub static PERSON_UNFAMILIARITY_OTHER: [SpellId; 8] = [
    SpellId::PersonUnfamiliarityOther1,
    SpellId::PersonUnfamiliarityOther2,
    SpellId::PersonUnfamiliarityOther3,
    SpellId::PersonUnfamiliarityOther4,
    SpellId::PersonUnfamiliarityOther5,
    SpellId::PersonUnfamiliarityOther6,
    SpellId::PersonUnfamiliarityOther7,
    SpellId::PersonUnfamiliarityOther8,
];

/// ACE `SpellLevelProgression.DeceptionMasterySelf` (`List<SpellId>`).
pub static DECEPTION_MASTERY_SELF: [SpellId; 8] = [
    SpellId::DeceptionMasterySelf1,
    SpellId::DeceptionMasterySelf2,
    SpellId::DeceptionMasterySelf3,
    SpellId::DeceptionMasterySelf4,
    SpellId::DeceptionMasterySelf5,
    SpellId::DeceptionMasterySelf6,
    SpellId::DeceptionMasterySelf7,
    SpellId::DeceptionMasterySelf8,
];

/// ACE `SpellLevelProgression.DeceptionMasteryOther` (`List<SpellId>`).
pub static DECEPTION_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::DeceptionMasteryOther1,
    SpellId::DeceptionMasteryOther2,
    SpellId::DeceptionMasteryOther3,
    SpellId::DeceptionMasteryOther4,
    SpellId::DeceptionMasteryOther5,
    SpellId::DeceptionMasteryOther6,
    SpellId::DeceptionMasteryOther7,
    SpellId::DeceptionMasteryOther8,
];

/// ACE `SpellLevelProgression.DeceptionIneptitudeSelf` (`List<SpellId>`).
pub static DECEPTION_INEPTITUDE_SELF: [SpellId; 8] = [
    SpellId::DeceptionIneptitudeSelf1,
    SpellId::DeceptionIneptitudeSelf2,
    SpellId::DeceptionIneptitudeSelf3,
    SpellId::DeceptionIneptitudeSelf4,
    SpellId::DeceptionIneptitudeSelf5,
    SpellId::DeceptionIneptitudeSelf6,
    SpellId::DeceptionIneptitudeSelf7,
    SpellId::DeceptionIneptitudeSelf8,
];

/// ACE `SpellLevelProgression.DeceptionIneptitudeOther` (`List<SpellId>`).
pub static DECEPTION_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::DeceptionIneptitudeOther1,
    SpellId::DeceptionIneptitudeOther2,
    SpellId::DeceptionIneptitudeOther3,
    SpellId::DeceptionIneptitudeOther4,
    SpellId::DeceptionIneptitudeOther5,
    SpellId::DeceptionIneptitudeOther6,
    SpellId::DeceptionIneptitudeOther7,
    SpellId::DeceptionIneptitudeOther8,
];

/// ACE `SpellLevelProgression.HealingMasterySelf` (`List<SpellId>`).
pub static HEALING_MASTERY_SELF: [SpellId; 8] = [
    SpellId::HealingMasterySelf1,
    SpellId::HealingMasterySelf2,
    SpellId::HealingMasterySelf3,
    SpellId::HealingMasterySelf4,
    SpellId::HealingMasterySelf5,
    SpellId::HealingMasterySelf6,
    SpellId::HealingMasterySelf7,
    SpellId::HealingMasterySelf8,
];

/// ACE `SpellLevelProgression.HealingMasteryOther` (`List<SpellId>`).
pub static HEALING_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::HealingMasteryOther1,
    SpellId::HealingMasteryOther2,
    SpellId::HealingMasteryOther3,
    SpellId::HealingMasteryOther4,
    SpellId::HealingMasteryOther5,
    SpellId::HealingMasteryOther6,
    SpellId::HealingMasteryOther7,
    SpellId::HealingMasteryOther8,
];

/// ACE `SpellLevelProgression.HealingIneptitudeSelf` (`List<SpellId>`).
pub static HEALING_INEPTITUDE_SELF: [SpellId; 8] = [
    SpellId::HealingIneptitudeSelf1,
    SpellId::HealingIneptitudeSelf2,
    SpellId::HealingIneptitudeSelf3,
    SpellId::HealingIneptitudeSelf4,
    SpellId::HealingIneptitudeSelf5,
    SpellId::HealingIneptitudeSelf6,
    SpellId::HealingIneptitudeSelf7,
    SpellId::HealingIneptitudeSelf8,
];

/// ACE `SpellLevelProgression.HealingIneptitudeOther` (`List<SpellId>`).
pub static HEALING_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::HealingIneptitudeOther1,
    SpellId::HealingIneptitudeOther2,
    SpellId::HealingIneptitudeOther3,
    SpellId::HealingIneptitudeOther4,
    SpellId::HealingIneptitudeOther5,
    SpellId::HealingIneptitudeOther6,
    SpellId::HealingIneptitudeOther7,
    SpellId::HealingIneptitudeOther8,
];

/// ACE `SpellLevelProgression.LeadershipMasterySelf` (`List<SpellId>`).
pub static LEADERSHIP_MASTERY_SELF: [SpellId; 8] = [
    SpellId::LeadershipMasterySelf1,
    SpellId::LeadershipMasterySelf2,
    SpellId::LeadershipMasterySelf3,
    SpellId::LeadershipMasterySelf4,
    SpellId::LeadershipMasterySelf5,
    SpellId::LeadershipMasterySelf6,
    SpellId::LeadershipMasterySelf7,
    SpellId::LeadershipMasterySelf8,
];

/// ACE `SpellLevelProgression.LeadershipMasteryOther` (`List<SpellId>`).
pub static LEADERSHIP_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::LeadershipMasteryOther1,
    SpellId::LeadershipMasteryOther2,
    SpellId::LeadershipMasteryOther3,
    SpellId::LeadershipMasteryOther4,
    SpellId::LeadershipMasteryOther5,
    SpellId::LeadershipMasteryOther6,
    SpellId::LeadershipMasteryOther7,
    SpellId::LeadershipMasteryOther8,
];

/// ACE `SpellLevelProgression.LeadershipIneptitudeSelf` (`List<SpellId>`).
pub static LEADERSHIP_INEPTITUDE_SELF: [SpellId; 8] = [
    SpellId::LeadershipIneptitudeSelf1,
    SpellId::LeadershipIneptitudeSelf2,
    SpellId::LeadershipIneptitudeSelf3,
    SpellId::LeadershipIneptitudeSelf4,
    SpellId::LeadershipIneptitudeSelf5,
    SpellId::LeadershipIneptitudeSelf6,
    SpellId::LeadershipIneptitudeSelf7,
    SpellId::LeadershipIneptitudeSelf8,
];

/// ACE `SpellLevelProgression.LeadershipIneptitudeOther` (`List<SpellId>`).
pub static LEADERSHIP_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::LeadershipIneptitudeOther1,
    SpellId::LeadershipIneptitudeOther2,
    SpellId::LeadershipIneptitudeOther3,
    SpellId::LeadershipIneptitudeOther4,
    SpellId::LeadershipIneptitudeOther5,
    SpellId::LeadershipIneptitudeOther6,
    SpellId::LeadershipIneptitudeOther7,
    SpellId::LeadershipIneptitudeOther8,
];

/// ACE `SpellLevelProgression.LockpickMasterySelf` (`List<SpellId>`).
pub static LOCKPICK_MASTERY_SELF: [SpellId; 8] = [
    SpellId::LockpickMasterySelf1,
    SpellId::LockpickMasterySelf2,
    SpellId::LockpickMasterySelf3,
    SpellId::LockpickMasterySelf4,
    SpellId::LockpickMasterySelf5,
    SpellId::LockpickMasterySelf6,
    SpellId::LockpickMasterySelf7,
    SpellId::LockpickMasterySelf8,
];

/// ACE `SpellLevelProgression.LockpickMasteryOther` (`List<SpellId>`).
pub static LOCKPICK_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::LockpickMasteryOther1,
    SpellId::LockpickMasteryOther2,
    SpellId::LockpickMasteryOther3,
    SpellId::LockpickMasteryOther4,
    SpellId::LockpickMasteryOther5,
    SpellId::LockpickMasteryOther6,
    SpellId::LockpickMasteryOther7,
    SpellId::LockpickMasteryOther8,
];

/// ACE `SpellLevelProgression.LockpickIneptitudeSelf` (`List<SpellId>`).
pub static LOCKPICK_INEPTITUDE_SELF: [SpellId; 8] = [
    SpellId::LockpickIneptitudeSelf1,
    SpellId::LockpickIneptitudeSelf2,
    SpellId::LockpickIneptitudeSelf3,
    SpellId::LockpickIneptitudeSelf4,
    SpellId::LockpickIneptitudeSelf5,
    SpellId::LockpickIneptitudeSelf6,
    SpellId::LockpickIneptitudeSelf7,
    SpellId::LockpickIneptitudeSelf8,
];

/// ACE `SpellLevelProgression.LockpickIneptitudeOther` (`List<SpellId>`).
pub static LOCKPICK_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::LockpickIneptitudeOther1,
    SpellId::LockpickIneptitudeOther2,
    SpellId::LockpickIneptitudeOther3,
    SpellId::LockpickIneptitudeOther4,
    SpellId::LockpickIneptitudeOther5,
    SpellId::LockpickIneptitudeOther6,
    SpellId::LockpickIneptitudeOther7,
    SpellId::LockpickIneptitudeOther8,
];

/// ACE `SpellLevelProgression.FealtySelf` (`List<SpellId>`).
pub static FEALTY_SELF: [SpellId; 8] = [
    SpellId::FealtySelf1,
    SpellId::FealtySelf2,
    SpellId::FealtySelf3,
    SpellId::FealtySelf4,
    SpellId::FealtySelf5,
    SpellId::FealtySelf6,
    SpellId::FealtySelf7,
    SpellId::FealtySelf8,
];

/// ACE `SpellLevelProgression.FealtyOther` (`List<SpellId>`).
pub static FEALTY_OTHER: [SpellId; 8] = [
    SpellId::FealtyOther1,
    SpellId::FealtyOther2,
    SpellId::FealtyOther3,
    SpellId::FealtyOther4,
    SpellId::FealtyOther5,
    SpellId::FealtyOther6,
    SpellId::FealtyOther7,
    SpellId::FealtyOther8,
];

/// ACE `SpellLevelProgression.FaithlessnessSelf` (`List<SpellId>`).
pub static FAITHLESSNESS_SELF: [SpellId; 8] = [
    SpellId::FaithlessnessSelf1,
    SpellId::FaithlessnessSelf2,
    SpellId::FaithlessnessSelf3,
    SpellId::FaithlessnessSelf4,
    SpellId::FaithlessnessSelf5,
    SpellId::FaithlessnessSelf6,
    SpellId::FaithlessnessSelf7,
    SpellId::FaithlessnessSelf8,
];

/// ACE `SpellLevelProgression.FaithlessnessOther` (`List<SpellId>`).
pub static FAITHLESSNESS_OTHER: [SpellId; 8] = [
    SpellId::FaithlessnessOther1,
    SpellId::FaithlessnessOther2,
    SpellId::FaithlessnessOther3,
    SpellId::FaithlessnessOther4,
    SpellId::FaithlessnessOther5,
    SpellId::FaithlessnessOther6,
    SpellId::FaithlessnessOther7,
    SpellId::FaithlessnessOther8,
];

/// ACE `SpellLevelProgression.JumpingMasterySelf` (`List<SpellId>`).
pub static JUMPING_MASTERY_SELF: [SpellId; 8] = [
    SpellId::JumpingMasterySelf1,
    SpellId::JumpingMasterySelf2,
    SpellId::JumpingMasterySelf3,
    SpellId::JumpingMasterySelf4,
    SpellId::JumpingMasterySelf5,
    SpellId::JumpingMasterySelf6,
    SpellId::JumpingMasterySelf7,
    SpellId::JumpingMasterySelf8,
];

/// ACE `SpellLevelProgression.JumpingMasteryOther` (`List<SpellId>`).
pub static JUMPING_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::JumpingMasteryOther1,
    SpellId::JumpingMasteryOther2,
    SpellId::JumpingMasteryOther3,
    SpellId::JumpingMasteryOther4,
    SpellId::JumpingMasteryOther5,
    SpellId::JumpingMasteryOther6,
    SpellId::JumpingMasteryOther7,
    SpellId::JumpingMasteryOther8,
];

/// ACE `SpellLevelProgression.SprintSelf` (`List<SpellId>`).
pub static SPRINT_SELF: [SpellId; 8] = [
    SpellId::SprintSelf1,
    SpellId::SprintSelf2,
    SpellId::SprintSelf3,
    SpellId::SprintSelf4,
    SpellId::SprintSelf5,
    SpellId::SprintSelf6,
    SpellId::SprintSelf7,
    SpellId::SprintSelf8,
];

/// ACE `SpellLevelProgression.SprintOther` (`List<SpellId>`).
pub static SPRINT_OTHER: [SpellId; 8] = [
    SpellId::SprintOther1,
    SpellId::SprintOther2,
    SpellId::SprintOther3,
    SpellId::SprintOther4,
    SpellId::SprintOther5,
    SpellId::SprintOther6,
    SpellId::SprintOther7,
    SpellId::SprintOther8,
];

/// ACE `SpellLevelProgression.LeadenFeetSelf` (`List<SpellId>`).
pub static LEADEN_FEET_SELF: [SpellId; 8] = [
    SpellId::LeadenFeetSelf1,
    SpellId::LeadenFeetSelf2,
    SpellId::LeadenFeetSelf3,
    SpellId::LeadenFeetSelf4,
    SpellId::LeadenFeetSelf5,
    SpellId::LeadenFeetSelf6,
    SpellId::LeadenFeetSelf7,
    SpellId::LeadenFeetSelf8,
];

/// ACE `SpellLevelProgression.LeadenFeetOther` (`List<SpellId>`).
pub static LEADEN_FEET_OTHER: [SpellId; 8] = [
    SpellId::LeadenFeetOther1,
    SpellId::LeadenFeetOther2,
    SpellId::LeadenFeetOther3,
    SpellId::LeadenFeetOther4,
    SpellId::LeadenFeetOther5,
    SpellId::LeadenFeetOther6,
    SpellId::LeadenFeetOther7,
    SpellId::LeadenFeetOther8,
];

/// ACE `SpellLevelProgression.JumpingIneptitudeSelf` (`List<SpellId>`).
pub static JUMPING_INEPTITUDE_SELF: [SpellId; 8] = [
    SpellId::JumpingIneptitudeSelf1,
    SpellId::JumpingIneptitudeSelf2,
    SpellId::JumpingIneptitudeSelf3,
    SpellId::JumpingIneptitudeSelf4,
    SpellId::JumpingIneptitudeSelf5,
    SpellId::JumpingIneptitudeSelf6,
    SpellId::JumpingIneptitudeSelf7,
    SpellId::JumpingIneptitudeSelf8,
];

/// ACE `SpellLevelProgression.JumpingIneptitudeOther` (`List<SpellId>`).
pub static JUMPING_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::JumpingIneptitudeOther1,
    SpellId::JumpingIneptitudeOther2,
    SpellId::JumpingIneptitudeOther3,
    SpellId::JumpingIneptitudeOther4,
    SpellId::JumpingIneptitudeOther5,
    SpellId::JumpingIneptitudeOther6,
    SpellId::JumpingIneptitudeOther7,
    SpellId::JumpingIneptitudeOther8,
];

/// ACE `SpellLevelProgression.BludgeonProtectionSelf` (`List<SpellId>`).
pub static BLUDGEON_PROTECTION_SELF: [SpellId; 8] = [
    SpellId::BludgeonProtectionSelf1,
    SpellId::BludgeonProtectionSelf2,
    SpellId::BludgeonProtectionSelf3,
    SpellId::BludgeonProtectionSelf4,
    SpellId::BludgeonProtectionSelf5,
    SpellId::BludgeonProtectionSelf6,
    SpellId::BludgeonProtectionSelf7,
    SpellId::BludgeonProtectionSelf8,
];

/// ACE `SpellLevelProgression.BludgeonProtectionOther` (`List<SpellId>`).
pub static BLUDGEON_PROTECTION_OTHER: [SpellId; 8] = [
    SpellId::BludgeonProtectionOther1,
    SpellId::BludgeonProtectionOther2,
    SpellId::BludgeonProtectionOther3,
    SpellId::BludgeonProtectionOther4,
    SpellId::BludgeonProtectionOther5,
    SpellId::BludgeonProtectionOther6,
    SpellId::BludgeonProtectionOther7,
    SpellId::BludgeonProtectionOther8,
];

/// ACE `SpellLevelProgression.ColdProtectionSelf` (`List<SpellId>`).
pub static COLD_PROTECTION_SELF: [SpellId; 8] = [
    SpellId::ColdProtectionSelf1,
    SpellId::ColdProtectionSelf2,
    SpellId::ColdProtectionSelf3,
    SpellId::ColdProtectionSelf4,
    SpellId::ColdProtectionSelf5,
    SpellId::ColdProtectionSelf6,
    SpellId::ColdProtectionSelf7,
    SpellId::ColdProtectionSelf8,
];

/// ACE `SpellLevelProgression.ColdProtectionOther` (`List<SpellId>`).
pub static COLD_PROTECTION_OTHER: [SpellId; 8] = [
    SpellId::ColdProtectionOther1,
    SpellId::ColdProtectionOther2,
    SpellId::ColdProtectionOther3,
    SpellId::ColdProtectionOther4,
    SpellId::ColdProtectionOther5,
    SpellId::ColdProtectionOther6,
    SpellId::ColdProtectionOther7,
    SpellId::ColdProtectionOther8,
];

/// ACE `SpellLevelProgression.BludgeonVulnerabilitySelf` (`List<SpellId>`).
pub static BLUDGEON_VULNERABILITY_SELF: [SpellId; 8] = [
    SpellId::BludgeonVulnerabilitySelf1,
    SpellId::BludgeonVulnerabilitySelf2,
    SpellId::BludgeonVulnerabilitySelf3,
    SpellId::BludgeonVulnerabilitySelf4,
    SpellId::BludgeonVulnerabilitySelf5,
    SpellId::BludgeonVulnerabilitySelf6,
    SpellId::BludgeonVulnerabilitySelf7,
    SpellId::BludgeonVulnerabilitySelf8,
];

/// ACE `SpellLevelProgression.BludgeonVulnerabilityOther` (`List<SpellId>`).
pub static BLUDGEON_VULNERABILITY_OTHER: [SpellId; 8] = [
    SpellId::BludgeonVulnerabilityOther1,
    SpellId::BludgeonVulnerabilityOther2,
    SpellId::BludgeonVulnerabilityOther3,
    SpellId::BludgeonVulnerabilityOther4,
    SpellId::BludgeonVulnerabilityOther5,
    SpellId::BludgeonVulnerabilityOther6,
    SpellId::BludgeonVulnerabilityOther7,
    SpellId::BludgeonVulnerabilityOther8,
];

/// ACE `SpellLevelProgression.ColdVulnerabilitySelf` (`List<SpellId>`).
pub static COLD_VULNERABILITY_SELF: [SpellId; 8] = [
    SpellId::ColdVulnerabilitySelf1,
    SpellId::ColdVulnerabilitySelf2,
    SpellId::ColdVulnerabilitySelf3,
    SpellId::ColdVulnerabilitySelf4,
    SpellId::ColdVulnerabilitySelf5,
    SpellId::ColdVulnerabilitySelf6,
    SpellId::ColdVulnerabilitySelf7,
    SpellId::ColdVulnerabilitySelf8,
];

/// ACE `SpellLevelProgression.ColdVulnerabilityOther` (`List<SpellId>`).
pub static COLD_VULNERABILITY_OTHER: [SpellId; 8] = [
    SpellId::ColdVulnerabilityOther1,
    SpellId::ColdVulnerabilityOther2,
    SpellId::ColdVulnerabilityOther3,
    SpellId::ColdVulnerabilityOther4,
    SpellId::ColdVulnerabilityOther5,
    SpellId::ColdVulnerabilityOther6,
    SpellId::ColdVulnerabilityOther7,
    SpellId::ColdVulnerabilityOther8,
];

/// ACE `SpellLevelProgression.LightningProtectionSelf` (`List<SpellId>`).
pub static LIGHTNING_PROTECTION_SELF: [SpellId; 8] = [
    SpellId::LightningProtectionSelf1,
    SpellId::LightningProtectionSelf2,
    SpellId::LightningProtectionSelf3,
    SpellId::LightningProtectionSelf4,
    SpellId::LightningProtectionSelf5,
    SpellId::LightningProtectionSelf6,
    SpellId::LightningProtectionSelf7,
    SpellId::LightningProtectionSelf8,
];

/// ACE `SpellLevelProgression.LightningProtectionOther` (`List<SpellId>`).
pub static LIGHTNING_PROTECTION_OTHER: [SpellId; 8] = [
    SpellId::LightningProtectionOther1,
    SpellId::LightningProtectionOther2,
    SpellId::LightningProtectionOther3,
    SpellId::LightningProtectionOther4,
    SpellId::LightningProtectionOther5,
    SpellId::LightningProtectionOther6,
    SpellId::LightningProtectionOther7,
    SpellId::LightningProtectionOther8,
];

/// ACE `SpellLevelProgression.LightningVulnerabilitySelf` (`List<SpellId>`).
pub static LIGHTNING_VULNERABILITY_SELF: [SpellId; 8] = [
    SpellId::LightningVulnerabilitySelf1,
    SpellId::LightningVulnerabilitySelf2,
    SpellId::LightningVulnerabilitySelf3,
    SpellId::LightningVulnerabilitySelf4,
    SpellId::LightningVulnerabilitySelf5,
    SpellId::LightningVulnerabilitySelf6,
    SpellId::LightningVulnerabilitySelf7,
    SpellId::LightningVulnerabilitySelf8,
];

/// ACE `SpellLevelProgression.LightningVulnerabilityOther` (`List<SpellId>`).
pub static LIGHTNING_VULNERABILITY_OTHER: [SpellId; 8] = [
    SpellId::LightningVulnerabilityOther1,
    SpellId::LightningVulnerabilityOther2,
    SpellId::LightningVulnerabilityOther3,
    SpellId::LightningVulnerabilityOther4,
    SpellId::LightningVulnerabilityOther5,
    SpellId::LightningVulnerabilityOther6,
    SpellId::LightningVulnerabilityOther7,
    SpellId::LightningVulnerabilityOther8,
];

/// ACE `SpellLevelProgression.BladeProtectionSelf` (`List<SpellId>`).
pub static BLADE_PROTECTION_SELF: [SpellId; 8] = [
    SpellId::BladeProtectionSelf1,
    SpellId::BladeProtectionSelf2,
    SpellId::BladeProtectionSelf3,
    SpellId::BladeProtectionSelf4,
    SpellId::BladeProtectionSelf5,
    SpellId::BladeProtectionSelf6,
    SpellId::BladeProtectionSelf7,
    SpellId::BladeProtectionSelf8,
];

/// ACE `SpellLevelProgression.BladeProtectionOther` (`List<SpellId>`).
pub static BLADE_PROTECTION_OTHER: [SpellId; 8] = [
    SpellId::BladeProtectionOther1,
    SpellId::BladeProtectionOther2,
    SpellId::BladeProtectionOther3,
    SpellId::BladeProtectionOther4,
    SpellId::BladeProtectionOther5,
    SpellId::BladeProtectionOther6,
    SpellId::BladeProtectionOther7,
    SpellId::BladeProtectionOther8,
];

/// ACE `SpellLevelProgression.BladeVulnerabilitySelf` (`List<SpellId>`).
pub static BLADE_VULNERABILITY_SELF: [SpellId; 8] = [
    SpellId::BladeVulnerabilitySelf1,
    SpellId::BladeVulnerabilitySelf2,
    SpellId::BladeVulnerabilitySelf3,
    SpellId::BladeVulnerabilitySelf4,
    SpellId::BladeVulnerabilitySelf5,
    SpellId::BladeVulnerabilitySelf6,
    SpellId::BladeVulnerabilitySelf7,
    SpellId::BladeVulnerabilitySelf8,
];

/// ACE `SpellLevelProgression.BladeVulnerabilityOther` (`List<SpellId>`).
pub static BLADE_VULNERABILITY_OTHER: [SpellId; 8] = [
    SpellId::BladeVulnerabilityOther1,
    SpellId::BladeVulnerabilityOther2,
    SpellId::BladeVulnerabilityOther3,
    SpellId::BladeVulnerabilityOther4,
    SpellId::BladeVulnerabilityOther5,
    SpellId::BladeVulnerabilityOther6,
    SpellId::BladeVulnerabilityOther7,
    SpellId::BladeVulnerabilityOther8,
];

/// ACE `SpellLevelProgression.PiercingProtectionSelf` (`List<SpellId>`).
pub static PIERCING_PROTECTION_SELF: [SpellId; 8] = [
    SpellId::PiercingProtectionSelf1,
    SpellId::PiercingProtectionSelf2,
    SpellId::PiercingProtectionSelf3,
    SpellId::PiercingProtectionSelf4,
    SpellId::PiercingProtectionSelf5,
    SpellId::PiercingProtectionSelf6,
    SpellId::PiercingProtectionSelf7,
    SpellId::PiercingProtectionSelf8,
];

/// ACE `SpellLevelProgression.PiercingProtectionOther` (`List<SpellId>`).
pub static PIERCING_PROTECTION_OTHER: [SpellId; 8] = [
    SpellId::PiercingProtectionOther1,
    SpellId::PiercingProtectionOther2,
    SpellId::PiercingProtectionOther3,
    SpellId::PiercingProtectionOther4,
    SpellId::PiercingProtectionOther5,
    SpellId::PiercingProtectionOther6,
    SpellId::PiercingProtectionOther7,
    SpellId::PiercingProtectionOther8,
];

/// ACE `SpellLevelProgression.PiercingVulnerabilitySelf` (`List<SpellId>`).
pub static PIERCING_VULNERABILITY_SELF: [SpellId; 8] = [
    SpellId::PiercingVulnerabilitySelf1,
    SpellId::PiercingVulnerabilitySelf2,
    SpellId::PiercingVulnerabilitySelf3,
    SpellId::PiercingVulnerabilitySelf4,
    SpellId::PiercingVulnerabilitySelf5,
    SpellId::PiercingVulnerabilitySelf6,
    SpellId::PiercingVulnerabilitySelf7,
    SpellId::PiercingVulnerabilitySelf8,
];

/// ACE `SpellLevelProgression.PiercingVulnerabilityOther` (`List<SpellId>`).
pub static PIERCING_VULNERABILITY_OTHER: [SpellId; 8] = [
    SpellId::PiercingVulnerabilityOther1,
    SpellId::PiercingVulnerabilityOther2,
    SpellId::PiercingVulnerabilityOther3,
    SpellId::PiercingVulnerabilityOther4,
    SpellId::PiercingVulnerabilityOther5,
    SpellId::PiercingVulnerabilityOther6,
    SpellId::PiercingVulnerabilityOther7,
    SpellId::PiercingVulnerabilityOther8,
];

/// ACE `SpellLevelProgression.RevitalizeSelf` (`List<SpellId>`).
pub static REVITALIZE_SELF: [SpellId; 8] = [
    SpellId::RevitalizeSelf1,
    SpellId::RevitalizeSelf2,
    SpellId::RevitalizeSelf3,
    SpellId::RevitalizeSelf4,
    SpellId::RevitalizeSelf5,
    SpellId::RevitalizeSelf6,
    SpellId::RevitalizeSelf7,
    SpellId::RevitalizeSelf8,
];

/// ACE `SpellLevelProgression.RevitalizeOther` (`List<SpellId>`).
pub static REVITALIZE_OTHER: [SpellId; 8] = [
    SpellId::RevitalizeOther1,
    SpellId::RevitalizeOther2,
    SpellId::RevitalizeOther3,
    SpellId::RevitalizeOther4,
    SpellId::RevitalizeOther5,
    SpellId::RevitalizeOther6,
    SpellId::RevitalizeOther7,
    SpellId::RevitalizeOther8,
];

/// ACE `SpellLevelProgression.EnfeebleSelf` (`List<SpellId>`).
pub static ENFEEBLE_SELF: [SpellId; 8] = [
    SpellId::EnfeebleSelf1,
    SpellId::EnfeebleSelf2,
    SpellId::EnfeebleSelf3,
    SpellId::EnfeebleSelf4,
    SpellId::EnfeebleSelf5,
    SpellId::EnfeebleSelf6,
    SpellId::EnfeebleSelf7,
    SpellId::EnfeebleSelf8,
];

/// ACE `SpellLevelProgression.EnfeebleOther` (`List<SpellId>`).
pub static ENFEEBLE_OTHER: [SpellId; 8] = [
    SpellId::EnfeebleOther1,
    SpellId::EnfeebleOther2,
    SpellId::EnfeebleOther3,
    SpellId::EnfeebleOther4,
    SpellId::EnfeebleOther5,
    SpellId::EnfeebleOther6,
    SpellId::EnfeebleOther7,
    SpellId::EnfeebleOther8,
];

/// ACE `SpellLevelProgression.ManaBoostSelf` (`List<SpellId>`).
pub static MANA_BOOST_SELF: [SpellId; 8] = [
    SpellId::ManaBoostSelf1,
    SpellId::ManaBoostSelf2,
    SpellId::ManaBoostSelf3,
    SpellId::ManaBoostSelf4,
    SpellId::ManaBoostSelf5,
    SpellId::ManaBoostSelf6,
    SpellId::ManaBoostSelf7,
    SpellId::ManaBoostSelf8,
];

/// ACE `SpellLevelProgression.ManaBoostOther` (`List<SpellId>`).
pub static MANA_BOOST_OTHER: [SpellId; 8] = [
    SpellId::ManaBoostOther1,
    SpellId::ManaBoostOther2,
    SpellId::ManaBoostOther3,
    SpellId::ManaBoostOther4,
    SpellId::ManaBoostOther5,
    SpellId::ManaBoostOther6,
    SpellId::ManaBoostOther7,
    SpellId::ManaBoostOther8,
];

/// ACE `SpellLevelProgression.ManaDrainSelf` (`List<SpellId>`).
pub static MANA_DRAIN_SELF: [SpellId; 8] = [
    SpellId::ManaDrainSelf1,
    SpellId::ManaDrainSelf2,
    SpellId::ManaDrainSelf3,
    SpellId::ManaDrainSelf4,
    SpellId::ManaDrainSelf5,
    SpellId::ManaDrainSelf6,
    SpellId::ManaDrainSelf7,
    SpellId::ManaDrainSelf8,
];

/// ACE `SpellLevelProgression.ManaDrainOther` (`List<SpellId>`).
pub static MANA_DRAIN_OTHER: [SpellId; 8] = [
    SpellId::ManaDrainOther1,
    SpellId::ManaDrainOther2,
    SpellId::ManaDrainOther3,
    SpellId::ManaDrainOther4,
    SpellId::ManaDrainOther5,
    SpellId::ManaDrainOther6,
    SpellId::ManaDrainOther7,
    SpellId::ManaDrainOther8,
];

/// ACE `SpellLevelProgression.InfuseHealth` (`List<SpellId>`).
pub static INFUSE_HEALTH: [SpellId; 8] = [
    SpellId::InfuseHealth1,
    SpellId::InfuseHealth2,
    SpellId::InfuseHealth3,
    SpellId::InfuseHealth4,
    SpellId::InfuseHealth5,
    SpellId::InfuseHealth6,
    SpellId::InfuseHealth7,
    SpellId::InfuseHealth8,
];

/// ACE `SpellLevelProgression.DrainHealth` (`List<SpellId>`).
pub static DRAIN_HEALTH: [SpellId; 8] = [
    SpellId::DrainHealth1,
    SpellId::DrainHealth2,
    SpellId::DrainHealth3,
    SpellId::DrainHealth4,
    SpellId::DrainHealth5,
    SpellId::DrainHealth6,
    SpellId::DrainHealth7,
    SpellId::DrainHealth8,
];

/// ACE `SpellLevelProgression.InfuseStamina` (`List<SpellId>`).
pub static INFUSE_STAMINA: [SpellId; 8] = [
    SpellId::InfuseStamina1,
    SpellId::InfuseStamina2,
    SpellId::InfuseStamina3,
    SpellId::InfuseStamina4,
    SpellId::InfuseStamina5,
    SpellId::InfuseStamina6,
    SpellId::InfuseStamina7,
    SpellId::InfuseStamina8,
];

/// ACE `SpellLevelProgression.DrainStamina` (`List<SpellId>`).
pub static DRAIN_STAMINA: [SpellId; 8] = [
    SpellId::DrainStamina1,
    SpellId::DrainStamina2,
    SpellId::DrainStamina3,
    SpellId::DrainStamina4,
    SpellId::DrainStamina5,
    SpellId::DrainStamina6,
    SpellId::DrainStamina7,
    SpellId::DrainStamina8,
];

/// ACE `SpellLevelProgression.DrainMana` (`List<SpellId>`).
pub static DRAIN_MANA: [SpellId; 8] = [
    SpellId::DrainMana1,
    SpellId::DrainMana2,
    SpellId::DrainMana3,
    SpellId::DrainMana4,
    SpellId::DrainMana5,
    SpellId::DrainMana6,
    SpellId::DrainMana7,
    SpellId::DrainMana8,
];

/// ACE `SpellLevelProgression.HealthToStaminaOther` (`List<SpellId>`).
pub static HEALTH_TO_STAMINA_OTHER: [SpellId; 8] = [
    SpellId::HealthToStaminaOther1,
    SpellId::HealthToStaminaOther2,
    SpellId::HealthToStaminaOther3,
    SpellId::HealthToStaminaOther4,
    SpellId::HealthToStaminaOther5,
    SpellId::HealthToStaminaOther6,
    SpellId::HealthToStaminaOther7,
    SpellId::HealthToStaminaOther8,
];

/// ACE `SpellLevelProgression.HealthToStaminaSelf` (`List<SpellId>`).
pub static HEALTH_TO_STAMINA_SELF: [SpellId; 8] = [
    SpellId::HealthToStaminaSelf1,
    SpellId::HealthToStaminaSelf2,
    SpellId::HealthToStaminaSelf3,
    SpellId::HealthToStaminaSelf4,
    SpellId::HealthToStaminaSelf5,
    SpellId::HealthToStaminaSelf6,
    SpellId::HealthToStaminaSelf7,
    SpellId::HealthToStaminaSelf8,
];

/// ACE `SpellLevelProgression.HealthToManaSelf` (`List<SpellId>`).
pub static HEALTH_TO_MANA_SELF: [SpellId; 8] = [
    SpellId::HealthToManaSelf1,
    SpellId::HealthToManaSelf2,
    SpellId::HealthToManaSelf3,
    SpellId::HealthToManaSelf4,
    SpellId::HealthToManaSelf5,
    SpellId::HealthToManaSelf6,
    SpellId::HealthToManaSelf7,
    SpellId::HealthToManaSelf8,
];

/// ACE `SpellLevelProgression.HealthToManaOther` (`List<SpellId>`).
pub static HEALTH_TO_MANA_OTHER: [SpellId; 8] = [
    SpellId::HealthToManaOther1,
    SpellId::HealthToManaOther2,
    SpellId::HealthToManaOther3,
    SpellId::HealthToManaOther4,
    SpellId::HealthToManaOther5,
    SpellId::HealthToManaOther6,
    SpellId::HealthToManaOther7,
    SpellId::HealthToManaOther8,
];

/// ACE `SpellLevelProgression.ManaToHealthOther` (`List<SpellId>`).
pub static MANA_TO_HEALTH_OTHER: [SpellId; 8] = [
    SpellId::ManaToHealthOther1,
    SpellId::ManaToHealthOther2,
    SpellId::ManaToHealthOther3,
    SpellId::ManaToHealthOther4,
    SpellId::ManaToHealthOther5,
    SpellId::ManaToHealthOther6,
    SpellId::ManaToHealthOther7,
    SpellId::ManaToHealthOther8,
];

/// ACE `SpellLevelProgression.ManaToHealthSelf` (`List<SpellId>`).
pub static MANA_TO_HEALTH_SELF: [SpellId; 8] = [
    SpellId::ManaToHealthSelf1,
    SpellId::ManaToHealthSelf2,
    SpellId::ManaToHealthSelf3,
    SpellId::ManaToHealthSelf4,
    SpellId::ManaToHealthSelf5,
    SpellId::ManaToHealthSelf6,
    SpellId::ManaToHealthSelf7,
    SpellId::ManaToHealthSelf8,
];

/// ACE `SpellLevelProgression.ManaToStaminaSelf` (`List<SpellId>`).
pub static MANA_TO_STAMINA_SELF: [SpellId; 8] = [
    SpellId::ManaToStaminaSelf1,
    SpellId::ManaToStaminaSelf2,
    SpellId::ManaToStaminaSelf3,
    SpellId::ManaToStaminaSelf4,
    SpellId::ManaToStaminaSelf5,
    SpellId::ManaToStaminaSelf6,
    SpellId::ManaToStaminaSelf7,
    SpellId::ManaToStaminaSelf8,
];

/// ACE `SpellLevelProgression.ManaToStaminaOther` (`List<SpellId>`).
pub static MANA_TO_STAMINA_OTHER: [SpellId; 8] = [
    SpellId::ManaToStaminaOther1,
    SpellId::ManaToStaminaOther2,
    SpellId::ManaToStaminaOther3,
    SpellId::ManaToStaminaOther4,
    SpellId::ManaToStaminaOther5,
    SpellId::ManaToStaminaOther6,
    SpellId::ManaToStaminaOther7,
    SpellId::ManaToStaminaOther8,
];

/// ACE `SpellLevelProgression.EnduranceSelf` (`List<SpellId>`).
pub static ENDURANCE_SELF: [SpellId; 8] = [
    SpellId::EnduranceSelf1,
    SpellId::EnduranceSelf2,
    SpellId::EnduranceSelf3,
    SpellId::EnduranceSelf4,
    SpellId::EnduranceSelf5,
    SpellId::EnduranceSelf6,
    SpellId::EnduranceSelf7,
    SpellId::EnduranceSelf8,
];

/// ACE `SpellLevelProgression.EnduranceOther` (`List<SpellId>`).
pub static ENDURANCE_OTHER: [SpellId; 8] = [
    SpellId::EnduranceOther1,
    SpellId::EnduranceOther2,
    SpellId::EnduranceOther3,
    SpellId::EnduranceOther4,
    SpellId::EnduranceOther5,
    SpellId::EnduranceOther6,
    SpellId::EnduranceOther7,
    SpellId::EnduranceOther8,
];

/// ACE `SpellLevelProgression.FrailtySelf` (`List<SpellId>`).
pub static FRAILTY_SELF: [SpellId; 8] = [
    SpellId::FrailtySelf1,
    SpellId::FrailtySelf2,
    SpellId::FrailtySelf3,
    SpellId::FrailtySelf4,
    SpellId::FrailtySelf5,
    SpellId::FrailtySelf6,
    SpellId::FrailtySelf7,
    SpellId::FrailtySelf8,
];

/// ACE `SpellLevelProgression.FrailtyOther` (`List<SpellId>`).
pub static FRAILTY_OTHER: [SpellId; 8] = [
    SpellId::FrailtyOther1,
    SpellId::FrailtyOther2,
    SpellId::FrailtyOther3,
    SpellId::FrailtyOther4,
    SpellId::FrailtyOther5,
    SpellId::FrailtyOther6,
    SpellId::FrailtyOther7,
    SpellId::FrailtyOther8,
];

/// ACE `SpellLevelProgression.CoordinationSelf` (`List<SpellId>`).
pub static COORDINATION_SELF: [SpellId; 8] = [
    SpellId::CoordinationSelf1,
    SpellId::CoordinationSelf2,
    SpellId::CoordinationSelf3,
    SpellId::CoordinationSelf4,
    SpellId::CoordinationSelf5,
    SpellId::CoordinationSelf6,
    SpellId::CoordinationSelf7,
    SpellId::CoordinationSelf8,
];

/// ACE `SpellLevelProgression.CoordinationOther` (`List<SpellId>`).
pub static COORDINATION_OTHER: [SpellId; 8] = [
    SpellId::CoordinationOther1,
    SpellId::CoordinationOther2,
    SpellId::CoordinationOther3,
    SpellId::CoordinationOther4,
    SpellId::CoordinationOther5,
    SpellId::CoordinationOther6,
    SpellId::CoordinationOther7,
    SpellId::CoordinationOther8,
];

/// ACE `SpellLevelProgression.ClumsinessSelf` (`List<SpellId>`).
pub static CLUMSINESS_SELF: [SpellId; 8] = [
    SpellId::ClumsinessSelf1,
    SpellId::ClumsinessSelf2,
    SpellId::ClumsinessSelf3,
    SpellId::ClumsinessSelf4,
    SpellId::ClumsinessSelf5,
    SpellId::ClumsinessSelf6,
    SpellId::ClumsinessSelf7,
    SpellId::ClumsinessSelf8,
];

/// ACE `SpellLevelProgression.ClumsinessOther` (`List<SpellId>`).
pub static CLUMSINESS_OTHER: [SpellId; 8] = [
    SpellId::ClumsinessOther1,
    SpellId::ClumsinessOther2,
    SpellId::ClumsinessOther3,
    SpellId::ClumsinessOther4,
    SpellId::ClumsinessOther5,
    SpellId::ClumsinessOther6,
    SpellId::ClumsinessOther7,
    SpellId::ClumsinessOther8,
];

/// ACE `SpellLevelProgression.QuicknessSelf` (`List<SpellId>`).
pub static QUICKNESS_SELF: [SpellId; 8] = [
    SpellId::QuicknessSelf1,
    SpellId::QuicknessSelf2,
    SpellId::QuicknessSelf3,
    SpellId::QuicknessSelf4,
    SpellId::QuicknessSelf5,
    SpellId::QuicknessSelf6,
    SpellId::QuicknessSelf7,
    SpellId::QuicknessSelf8,
];

/// ACE `SpellLevelProgression.QuicknessOther` (`List<SpellId>`).
pub static QUICKNESS_OTHER: [SpellId; 8] = [
    SpellId::QuicknessOther1,
    SpellId::QuicknessOther2,
    SpellId::QuicknessOther3,
    SpellId::QuicknessOther4,
    SpellId::QuicknessOther5,
    SpellId::QuicknessOther6,
    SpellId::QuicknessOther7,
    SpellId::QuicknessOther8,
];

/// ACE `SpellLevelProgression.SlownessSelf` (`List<SpellId>`).
pub static SLOWNESS_SELF: [SpellId; 8] = [
    SpellId::SlownessSelf1,
    SpellId::SlownessSelf2,
    SpellId::SlownessSelf3,
    SpellId::SlownessSelf4,
    SpellId::SlownessSelf5,
    SpellId::SlownessSelf6,
    SpellId::SlownessSelf7,
    SpellId::SlownessSelf8,
];

/// ACE `SpellLevelProgression.SlownessOther` (`List<SpellId>`).
pub static SLOWNESS_OTHER: [SpellId; 8] = [
    SpellId::SlownessOther1,
    SpellId::SlownessOther2,
    SpellId::SlownessOther3,
    SpellId::SlownessOther4,
    SpellId::SlownessOther5,
    SpellId::SlownessOther6,
    SpellId::SlownessOther7,
    SpellId::SlownessOther8,
];

/// ACE `SpellLevelProgression.FocusSelf` (`List<SpellId>`).
pub static FOCUS_SELF: [SpellId; 8] = [
    SpellId::FocusSelf1,
    SpellId::FocusSelf2,
    SpellId::FocusSelf3,
    SpellId::FocusSelf4,
    SpellId::FocusSelf5,
    SpellId::FocusSelf6,
    SpellId::FocusSelf7,
    SpellId::FocusSelf8,
];

/// ACE `SpellLevelProgression.FocusOther` (`List<SpellId>`).
pub static FOCUS_OTHER: [SpellId; 8] = [
    SpellId::FocusOther1,
    SpellId::FocusOther2,
    SpellId::FocusOther3,
    SpellId::FocusOther4,
    SpellId::FocusOther5,
    SpellId::FocusOther6,
    SpellId::FocusOther7,
    SpellId::FocusOther8,
];

/// ACE `SpellLevelProgression.BafflementSelf` (`List<SpellId>`).
pub static BAFFLEMENT_SELF: [SpellId; 8] = [
    SpellId::BafflementSelf1,
    SpellId::BafflementSelf2,
    SpellId::BafflementSelf3,
    SpellId::BafflementSelf4,
    SpellId::BafflementSelf5,
    SpellId::BafflementSelf6,
    SpellId::BafflementSelf7,
    SpellId::BafflementSelf8,
];

/// ACE `SpellLevelProgression.BafflementOther` (`List<SpellId>`).
pub static BAFFLEMENT_OTHER: [SpellId; 8] = [
    SpellId::BafflementOther1,
    SpellId::BafflementOther2,
    SpellId::BafflementOther3,
    SpellId::BafflementOther4,
    SpellId::BafflementOther5,
    SpellId::BafflementOther6,
    SpellId::BafflementOther7,
    SpellId::BafflementOther8,
];

/// ACE `SpellLevelProgression.WillpowerSelf` (`List<SpellId>`).
pub static WILLPOWER_SELF: [SpellId; 8] = [
    SpellId::WillpowerSelf1,
    SpellId::WillpowerSelf2,
    SpellId::WillpowerSelf3,
    SpellId::WillpowerSelf4,
    SpellId::WillpowerSelf5,
    SpellId::WillpowerSelf6,
    SpellId::WillpowerSelf7,
    SpellId::WillpowerSelf8,
];

/// ACE `SpellLevelProgression.WillpowerOther` (`List<SpellId>`).
pub static WILLPOWER_OTHER: [SpellId; 8] = [
    SpellId::WillpowerOther1,
    SpellId::WillpowerOther2,
    SpellId::WillpowerOther3,
    SpellId::WillpowerOther4,
    SpellId::WillpowerOther5,
    SpellId::WillpowerOther6,
    SpellId::WillpowerOther7,
    SpellId::WillpowerOther8,
];

/// ACE `SpellLevelProgression.FeeblemindSelf` (`List<SpellId>`).
pub static FEEBLEMIND_SELF: [SpellId; 8] = [
    SpellId::FeeblemindSelf1,
    SpellId::FeeblemindSelf2,
    SpellId::FeeblemindSelf3,
    SpellId::FeeblemindSelf4,
    SpellId::FeeblemindSelf5,
    SpellId::FeeblemindSelf6,
    SpellId::FeeblemindSelf7,
    SpellId::FeeblemindSelf8,
];

/// ACE `SpellLevelProgression.FeeblemindOther` (`List<SpellId>`).
pub static FEEBLEMIND_OTHER: [SpellId; 8] = [
    SpellId::FeeblemindOther1,
    SpellId::FeeblemindOther2,
    SpellId::FeeblemindOther3,
    SpellId::FeeblemindOther4,
    SpellId::FeeblemindOther5,
    SpellId::FeeblemindOther6,
    SpellId::FeeblemindOther7,
    SpellId::FeeblemindOther8,
];

/// ACE `SpellLevelProgression.HermeticVoid` (`List<SpellId>`).
pub static HERMETIC_VOID: [SpellId; 8] = [
    SpellId::HermeticVoid1,
    SpellId::HermeticVoid2,
    SpellId::HermeticVoid3,
    SpellId::HermeticVoid4,
    SpellId::HermeticVoid5,
    SpellId::HermeticVoid6,
    SpellId::HermeticVoid7,
    SpellId::HermeticVoid8,
];

/// ACE `SpellLevelProgression.HermeticLinkSelf` (`List<SpellId>`).
pub static HERMETIC_LINK_SELF: [SpellId; 8] = [
    SpellId::HermeticLinkSelf1,
    SpellId::HermeticLinkSelf2,
    SpellId::HermeticLinkSelf3,
    SpellId::HermeticLinkSelf4,
    SpellId::HermeticLinkSelf5,
    SpellId::HermeticLinkSelf6,
    SpellId::HermeticLinkSelf7,
    SpellId::HermeticLinkSelf8,
];

/// ACE `SpellLevelProgression.Brittlemail` (`List<SpellId>`).
pub static BRITTLEMAIL: [SpellId; 8] = [
    SpellId::Brittlemail1,
    SpellId::Brittlemail2,
    SpellId::Brittlemail3,
    SpellId::Brittlemail4,
    SpellId::Brittlemail5,
    SpellId::Brittlemail6,
    SpellId::Brittlemail7,
    SpellId::Brittlemail8,
];

/// ACE `SpellLevelProgression.AcidBane` (`List<SpellId>`).
pub static ACID_BANE: [SpellId; 8] = [
    SpellId::AcidBane1,
    SpellId::AcidBane2,
    SpellId::AcidBane3,
    SpellId::AcidBane4,
    SpellId::AcidBane5,
    SpellId::AcidBane6,
    SpellId::AcidBane7,
    SpellId::AcidBane8,
];

/// ACE `SpellLevelProgression.AcidLure` (`List<SpellId>`).
pub static ACID_LURE: [SpellId; 8] = [
    SpellId::AcidLure1,
    SpellId::AcidLure2,
    SpellId::AcidLure3,
    SpellId::AcidLure4,
    SpellId::AcidLure5,
    SpellId::AcidLure6,
    SpellId::AcidLure7,
    SpellId::AcidLure8,
];

/// ACE `SpellLevelProgression.BludgeonLure` (`List<SpellId>`).
pub static BLUDGEON_LURE: [SpellId; 8] = [
    SpellId::BludgeonLure1,
    SpellId::BludgeonLure2,
    SpellId::BludgeonLure3,
    SpellId::BludgeonLure4,
    SpellId::BludgeonLure5,
    SpellId::BludgeonLure6,
    SpellId::BludgeonLure7,
    SpellId::BludgeonLure8,
];

/// ACE `SpellLevelProgression.BludgeonBane` (`List<SpellId>`).
pub static BLUDGEON_BANE: [SpellId; 8] = [
    SpellId::BludgeonBane1,
    SpellId::BludgeonBane2,
    SpellId::BludgeonBane3,
    SpellId::BludgeonBane4,
    SpellId::BludgeonBane5,
    SpellId::BludgeonBane6,
    SpellId::BludgeonBane7,
    SpellId::BludgeonBane8,
];

/// ACE `SpellLevelProgression.FrostLure` (`List<SpellId>`).
pub static FROST_LURE: [SpellId; 8] = [
    SpellId::FrostLure1,
    SpellId::FrostLure2,
    SpellId::FrostLure3,
    SpellId::FrostLure4,
    SpellId::FrostLure5,
    SpellId::FrostLure6,
    SpellId::FrostLure7,
    SpellId::FrostLure8,
];

/// ACE `SpellLevelProgression.FrostBane` (`List<SpellId>`).
pub static FROST_BANE: [SpellId; 8] = [
    SpellId::FrostBane1,
    SpellId::FrostBane2,
    SpellId::FrostBane3,
    SpellId::FrostBane4,
    SpellId::FrostBane5,
    SpellId::FrostBane6,
    SpellId::FrostBane7,
    SpellId::FrostBane8,
];

/// ACE `SpellLevelProgression.LightningLure` (`List<SpellId>`).
pub static LIGHTNING_LURE: [SpellId; 8] = [
    SpellId::LightningLure1,
    SpellId::LightningLure2,
    SpellId::LightningLure3,
    SpellId::LightningLure4,
    SpellId::LightningLure5,
    SpellId::LightningLure6,
    SpellId::LightningLure7,
    SpellId::LightningLure8,
];

/// ACE `SpellLevelProgression.LightningBane` (`List<SpellId>`).
pub static LIGHTNING_BANE: [SpellId; 8] = [
    SpellId::LightningBane1,
    SpellId::LightningBane2,
    SpellId::LightningBane3,
    SpellId::LightningBane4,
    SpellId::LightningBane5,
    SpellId::LightningBane6,
    SpellId::LightningBane7,
    SpellId::LightningBane8,
];

/// ACE `SpellLevelProgression.FlameLure` (`List<SpellId>`).
pub static FLAME_LURE: [SpellId; 8] = [
    SpellId::FlameLure1,
    SpellId::FlameLure2,
    SpellId::FlameLure3,
    SpellId::FlameLure4,
    SpellId::FlameLure5,
    SpellId::FlameLure6,
    SpellId::FlameLure7,
    SpellId::FlameLure8,
];

/// ACE `SpellLevelProgression.FlameBane` (`List<SpellId>`).
pub static FLAME_BANE: [SpellId; 8] = [
    SpellId::FlameBane1,
    SpellId::FlameBane2,
    SpellId::FlameBane3,
    SpellId::FlameBane4,
    SpellId::FlameBane5,
    SpellId::FlameBane6,
    SpellId::FlameBane7,
    SpellId::FlameBane8,
];

/// ACE `SpellLevelProgression.PiercingLure` (`List<SpellId>`).
pub static PIERCING_LURE: [SpellId; 8] = [
    SpellId::PiercingLure1,
    SpellId::PiercingLure2,
    SpellId::PiercingLure3,
    SpellId::PiercingLure4,
    SpellId::PiercingLure5,
    SpellId::PiercingLure6,
    SpellId::PiercingLure7,
    SpellId::PiercingLure8,
];

/// ACE `SpellLevelProgression.PiercingBane` (`List<SpellId>`).
pub static PIERCING_BANE: [SpellId; 8] = [
    SpellId::PiercingBane1,
    SpellId::PiercingBane2,
    SpellId::PiercingBane3,
    SpellId::PiercingBane4,
    SpellId::PiercingBane5,
    SpellId::PiercingBane6,
    SpellId::PiercingBane7,
    SpellId::PiercingBane8,
];

/// ACE `SpellLevelProgression.StrengthenLock` (`List<SpellId>`).
pub static STRENGTHEN_LOCK: [SpellId; 8] = [
    SpellId::StrengthenLock1,
    SpellId::StrengthenLock2,
    SpellId::StrengthenLock3,
    SpellId::StrengthenLock4,
    SpellId::StrengthenLock5,
    SpellId::StrengthenLock6,
    SpellId::StrengthenLock7,
    SpellId::StrengthenLock8,
];

/// ACE `SpellLevelProgression.WeakenLock` (`List<SpellId>`).
pub static WEAKEN_LOCK: [SpellId; 8] = [
    SpellId::WeakenLock1,
    SpellId::WeakenLock2,
    SpellId::WeakenLock3,
    SpellId::WeakenLock4,
    SpellId::WeakenLock5,
    SpellId::WeakenLock6,
    SpellId::WeakenLock7,
    SpellId::WeakenLock8,
];

/// ACE `SpellLevelProgression.HeartSeekerSelf` (`List<SpellId>`).
pub static HEART_SEEKER_SELF: [SpellId; 8] = [
    SpellId::HeartSeekerSelf1,
    SpellId::HeartSeekerSelf2,
    SpellId::HeartSeekerSelf3,
    SpellId::HeartSeekerSelf4,
    SpellId::HeartSeekerSelf5,
    SpellId::HeartSeekerSelf6,
    SpellId::HeartSeekerSelf7,
    SpellId::HeartSeekerSelf8,
];

/// ACE `SpellLevelProgression.TurnBlade` (`List<SpellId>`).
pub static TURN_BLADE: [SpellId; 8] = [
    SpellId::TurnBlade1,
    SpellId::TurnBlade2,
    SpellId::TurnBlade3,
    SpellId::TurnBlade4,
    SpellId::TurnBlade5,
    SpellId::TurnBlade6,
    SpellId::TurnBlade7,
    SpellId::TurnBlade8,
];

/// ACE `SpellLevelProgression.DefenderSelf` (`List<SpellId>`).
pub static DEFENDER_SELF: [SpellId; 8] = [
    SpellId::DefenderSelf1,
    SpellId::DefenderSelf2,
    SpellId::DefenderSelf3,
    SpellId::DefenderSelf4,
    SpellId::DefenderSelf5,
    SpellId::DefenderSelf6,
    SpellId::DefenderSelf7,
    SpellId::DefenderSelf8,
];

/// ACE `SpellLevelProgression.LureBlade` (`List<SpellId>`).
pub static LURE_BLADE: [SpellId; 8] = [
    SpellId::LureBlade1,
    SpellId::LureBlade2,
    SpellId::LureBlade3,
    SpellId::LureBlade4,
    SpellId::LureBlade5,
    SpellId::LureBlade6,
    SpellId::LureBlade7,
    SpellId::LureBlade8,
];

/// ACE `SpellLevelProgression.DefenselessnessSelf` (`List<SpellId>`).
pub static DEFENSELESSNESS_SELF: [SpellId; 8] = [
    SpellId::DefenselessnessSelf1,
    SpellId::DefenselessnessSelf2,
    SpellId::DefenselessnessSelf3,
    SpellId::DefenselessnessSelf4,
    SpellId::DefenselessnessSelf5,
    SpellId::DefenselessnessSelf6,
    SpellId::DefenselessnessSelf7,
    SpellId::DefenselessnessSelf8,
];

/// ACE `SpellLevelProgression.StaminaToHealthOther` (`List<SpellId>`).
pub static STAMINA_TO_HEALTH_OTHER: [SpellId; 8] = [
    SpellId::StaminaToHealthOther1,
    SpellId::StaminaToHealthOther2,
    SpellId::StaminaToHealthOther3,
    SpellId::StaminaToHealthOther4,
    SpellId::StaminaToHealthOther5,
    SpellId::StaminaToHealthOther6,
    SpellId::StaminaToHealthOther7,
    SpellId::StaminaToHealthOther8,
];

/// ACE `SpellLevelProgression.StaminaToHealthSelf` (`List<SpellId>`).
pub static STAMINA_TO_HEALTH_SELF: [SpellId; 8] = [
    SpellId::StaminaToHealthSelf1,
    SpellId::StaminaToHealthSelf2,
    SpellId::StaminaToHealthSelf3,
    SpellId::StaminaToHealthSelf4,
    SpellId::StaminaToHealthSelf5,
    SpellId::StaminaToHealthSelf6,
    SpellId::StaminaToHealthSelf7,
    SpellId::StaminaToHealthSelf8,
];

/// ACE `SpellLevelProgression.StaminaToManaOther` (`List<SpellId>`).
pub static STAMINA_TO_MANA_OTHER: [SpellId; 8] = [
    SpellId::StaminaToManaOther1,
    SpellId::StaminaToManaOther2,
    SpellId::StaminaToManaOther3,
    SpellId::StaminaToManaOther4,
    SpellId::StaminaToManaOther5,
    SpellId::StaminaToManaOther6,
    SpellId::StaminaToManaOther7,
    SpellId::StaminaToManaOther8,
];

/// ACE `SpellLevelProgression.StaminaToManaSelf` (`List<SpellId>`).
pub static STAMINA_TO_MANA_SELF: [SpellId; 8] = [
    SpellId::StaminaToManaSelf1,
    SpellId::StaminaToManaSelf2,
    SpellId::StaminaToManaSelf3,
    SpellId::StaminaToManaSelf4,
    SpellId::StaminaToManaSelf5,
    SpellId::StaminaToManaSelf6,
    SpellId::StaminaToManaSelf7,
    SpellId::StaminaToManaSelf8,
];

/// ACE `SpellLevelProgression.CookingMasteryOther` (`List<SpellId>`).
pub static COOKING_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::CookingMasteryOther1,
    SpellId::CookingMasteryOther2,
    SpellId::CookingMasteryOther3,
    SpellId::CookingMasteryOther4,
    SpellId::CookingMasteryOther5,
    SpellId::CookingMasteryOther6,
    SpellId::CookingMasteryOther7,
    SpellId::CookingMasteryOther8,
];

/// ACE `SpellLevelProgression.CookingMasterySelf` (`List<SpellId>`).
pub static COOKING_MASTERY_SELF: [SpellId; 8] = [
    SpellId::CookingMasterySelf1,
    SpellId::CookingMasterySelf2,
    SpellId::CookingMasterySelf3,
    SpellId::CookingMasterySelf4,
    SpellId::CookingMasterySelf5,
    SpellId::CookingMasterySelf6,
    SpellId::CookingMasterySelf7,
    SpellId::CookingMasterySelf8,
];

/// ACE `SpellLevelProgression.CookingIneptitudeOther` (`List<SpellId>`).
pub static COOKING_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::CookingIneptitudeOther1,
    SpellId::CookingIneptitudeOther2,
    SpellId::CookingIneptitudeOther3,
    SpellId::CookingIneptitudeOther4,
    SpellId::CookingIneptitudeOther5,
    SpellId::CookingIneptitudeOther6,
    SpellId::CookingIneptitudeOther7,
    SpellId::CookingIneptitudeOther8,
];

/// ACE `SpellLevelProgression.CookingIneptitudeSelf` (`List<SpellId>`).
pub static COOKING_INEPTITUDE_SELF: [SpellId; 8] = [
    SpellId::CookingIneptitudeSelf1,
    SpellId::CookingIneptitudeSelf2,
    SpellId::CookingIneptitudeSelf3,
    SpellId::CookingIneptitudeSelf4,
    SpellId::CookingIneptitudeSelf5,
    SpellId::CookingIneptitudeSelf6,
    SpellId::CookingIneptitudeSelf7,
    SpellId::CookingIneptitudeSelf8,
];

/// ACE `SpellLevelProgression.FletchingMasteryOther` (`List<SpellId>`).
pub static FLETCHING_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::FletchingMasteryOther1,
    SpellId::FletchingMasteryOther2,
    SpellId::FletchingMasteryOther3,
    SpellId::FletchingMasteryOther4,
    SpellId::FletchingMasteryOther5,
    SpellId::FletchingMasteryOther6,
    SpellId::FletchingMasteryOther7,
    SpellId::FletchingMasteryOther8,
];

/// ACE `SpellLevelProgression.FletchingMasterySelf` (`List<SpellId>`).
pub static FLETCHING_MASTERY_SELF: [SpellId; 8] = [
    SpellId::FletchingMasterySelf1,
    SpellId::FletchingMasterySelf2,
    SpellId::FletchingMasterySelf3,
    SpellId::FletchingMasterySelf4,
    SpellId::FletchingMasterySelf5,
    SpellId::FletchingMasterySelf6,
    SpellId::FletchingMasterySelf7,
    SpellId::FletchingMasterySelf8,
];

/// ACE `SpellLevelProgression.FletchingIneptitudeOther` (`List<SpellId>`).
pub static FLETCHING_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::FletchingIneptitudeOther1,
    SpellId::FletchingIneptitudeOther2,
    SpellId::FletchingIneptitudeOther3,
    SpellId::FletchingIneptitudeOther4,
    SpellId::FletchingIneptitudeOther5,
    SpellId::FletchingIneptitudeOther6,
    SpellId::FletchingIneptitudeOther7,
    SpellId::FletchingIneptitudeOther8,
];

/// ACE `SpellLevelProgression.FletchingIneptitudeSelf` (`List<SpellId>`).
pub static FLETCHING_INEPTITUDE_SELF: [SpellId; 8] = [
    SpellId::FletchingIneptitudeSelf1,
    SpellId::FletchingIneptitudeSelf2,
    SpellId::FletchingIneptitudeSelf3,
    SpellId::FletchingIneptitudeSelf4,
    SpellId::FletchingIneptitudeSelf5,
    SpellId::FletchingIneptitudeSelf6,
    SpellId::FletchingIneptitudeSelf7,
    SpellId::FletchingIneptitudeSelf8,
];

/// ACE `SpellLevelProgression.AlchemyMasteryOther` (`List<SpellId>`).
pub static ALCHEMY_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::AlchemyMasteryOther1,
    SpellId::AlchemyMasteryOther2,
    SpellId::AlchemyMasteryOther3,
    SpellId::AlchemyMasteryOther4,
    SpellId::AlchemyMasteryOther5,
    SpellId::AlchemyMasteryOther6,
    SpellId::AlchemyMasteryOther7,
    SpellId::AlchemyMasteryOther8,
];

/// ACE `SpellLevelProgression.AlchemyMasterySelf` (`List<SpellId>`).
pub static ALCHEMY_MASTERY_SELF: [SpellId; 8] = [
    SpellId::AlchemyMasterySelf1,
    SpellId::AlchemyMasterySelf2,
    SpellId::AlchemyMasterySelf3,
    SpellId::AlchemyMasterySelf4,
    SpellId::AlchemyMasterySelf5,
    SpellId::AlchemyMasterySelf6,
    SpellId::AlchemyMasterySelf7,
    SpellId::AlchemyMasterySelf8,
];

/// ACE `SpellLevelProgression.AlchemyIneptitudeOther` (`List<SpellId>`).
pub static ALCHEMY_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::AlchemyIneptitudeOther1,
    SpellId::AlchemyIneptitudeOther2,
    SpellId::AlchemyIneptitudeOther3,
    SpellId::AlchemyIneptitudeOther4,
    SpellId::AlchemyIneptitudeOther5,
    SpellId::AlchemyIneptitudeOther6,
    SpellId::AlchemyIneptitudeOther7,
    SpellId::AlchemyIneptitudeOther8,
];

/// ACE `SpellLevelProgression.AlchemyIneptitudeSelf` (`List<SpellId>`).
pub static ALCHEMY_INEPTITUDE_SELF: [SpellId; 8] = [
    SpellId::AlchemyIneptitudeSelf1,
    SpellId::AlchemyIneptitudeSelf2,
    SpellId::AlchemyIneptitudeSelf3,
    SpellId::AlchemyIneptitudeSelf4,
    SpellId::AlchemyIneptitudeSelf5,
    SpellId::AlchemyIneptitudeSelf6,
    SpellId::AlchemyIneptitudeSelf7,
    SpellId::AlchemyIneptitudeSelf8,
];

/// ACE `SpellLevelProgression.AcidStreak` (`List<SpellId>`).
pub static ACID_STREAK: [SpellId; 8] = [
    SpellId::AcidStreak1,
    SpellId::AcidStreak2,
    SpellId::AcidStreak3,
    SpellId::AcidStreak4,
    SpellId::AcidStreak5,
    SpellId::AcidStreak6,
    SpellId::AcidStreak7,
    SpellId::AcidStreak8,
];

/// ACE `SpellLevelProgression.FlameStreak` (`List<SpellId>`).
pub static FLAME_STREAK: [SpellId; 8] = [
    SpellId::FlameStreak1,
    SpellId::FlameStreak2,
    SpellId::FlameStreak3,
    SpellId::FlameStreak4,
    SpellId::FlameStreak5,
    SpellId::FlameStreak6,
    SpellId::FlameStreak7,
    SpellId::FlameStreak8,
];

/// ACE `SpellLevelProgression.ForceStreak` (`List<SpellId>`).
pub static FORCE_STREAK: [SpellId; 8] = [
    SpellId::ForceStreak1,
    SpellId::ForceStreak2,
    SpellId::ForceStreak3,
    SpellId::ForceStreak4,
    SpellId::ForceStreak5,
    SpellId::ForceStreak6,
    SpellId::ForceStreak7,
    SpellId::ForceStreak8,
];

/// ACE `SpellLevelProgression.FrostStreak` (`List<SpellId>`).
pub static FROST_STREAK: [SpellId; 8] = [
    SpellId::FrostStreak1,
    SpellId::FrostStreak2,
    SpellId::FrostStreak3,
    SpellId::FrostStreak4,
    SpellId::FrostStreak5,
    SpellId::FrostStreak6,
    SpellId::FrostStreak7,
    SpellId::FrostStreak8,
];

/// ACE `SpellLevelProgression.LightningStreak` (`List<SpellId>`).
pub static LIGHTNING_STREAK: [SpellId; 8] = [
    SpellId::LightningStreak1,
    SpellId::LightningStreak2,
    SpellId::LightningStreak3,
    SpellId::LightningStreak4,
    SpellId::LightningStreak5,
    SpellId::LightningStreak6,
    SpellId::LightningStreak7,
    SpellId::LightningStreak8,
];

/// ACE `SpellLevelProgression.ShockwaveStreak` (`List<SpellId>`).
pub static SHOCKWAVE_STREAK: [SpellId; 8] = [
    SpellId::ShockwaveStreak1,
    SpellId::ShockwaveStreak2,
    SpellId::ShockwaveStreak3,
    SpellId::ShockwaveStreak4,
    SpellId::ShockwaveStreak5,
    SpellId::ShockwaveStreak6,
    SpellId::ShockwaveStreak7,
    SpellId::ShockwaveStreak8,
];

/// ACE `SpellLevelProgression.WhirlingBladeStreak` (`List<SpellId>`).
pub static WHIRLING_BLADE_STREAK: [SpellId; 8] = [
    SpellId::WhirlingBladeStreak1,
    SpellId::WhirlingBladeStreak2,
    SpellId::WhirlingBladeStreak3,
    SpellId::WhirlingBladeStreak4,
    SpellId::WhirlingBladeStreak5,
    SpellId::WhirlingBladeStreak6,
    SpellId::WhirlingBladeStreak7,
    SpellId::WhirlingBladeStreak8,
];

/// ACE `SpellLevelProgression.DispelAllNeutralOther` (`List<SpellId>`).
pub static DISPEL_ALL_NEUTRAL_OTHER: [SpellId; 8] = [
    SpellId::DispelAllNeutralOther1,
    SpellId::DispelAllNeutralOther2,
    SpellId::DispelAllNeutralOther3,
    SpellId::DispelAllNeutralOther4,
    SpellId::DispelAllNeutralOther5,
    SpellId::DispelAllNeutralOther6,
    SpellId::DispelAllNeutralOther7,
    SpellId::DispelAllNeutralOther8,
];

/// ACE `SpellLevelProgression.DispelAllGoodOther` (`List<SpellId>`).
pub static DISPEL_ALL_GOOD_OTHER: [SpellId; 8] = [
    SpellId::DispelAllGoodOther1,
    SpellId::DispelAllGoodOther2,
    SpellId::DispelAllGoodOther3,
    SpellId::DispelAllGoodOther4,
    SpellId::DispelAllGoodOther5,
    SpellId::DispelAllGoodOther6,
    SpellId::DispelAllGoodOther7,
    SpellId::DispelAllGoodOther8,
];

/// ACE `SpellLevelProgression.DispelAllBadOther` (`List<SpellId>`).
pub static DISPEL_ALL_BAD_OTHER: [SpellId; 8] = [
    SpellId::DispelAllBadOther1,
    SpellId::DispelAllBadOther2,
    SpellId::DispelAllBadOther3,
    SpellId::DispelAllBadOther4,
    SpellId::DispelAllBadOther5,
    SpellId::DispelAllBadOther6,
    SpellId::DispelAllBadOther7,
    SpellId::DispelAllBadOther8,
];

/// ACE `SpellLevelProgression.DispelAllNeutralSelf` (`List<SpellId>`).
pub static DISPEL_ALL_NEUTRAL_SELF: [SpellId; 8] = [
    SpellId::DispelAllNeutralSelf1,
    SpellId::DispelAllNeutralSelf2,
    SpellId::DispelAllNeutralSelf3,
    SpellId::DispelAllNeutralSelf4,
    SpellId::DispelAllNeutralSelf5,
    SpellId::DispelAllNeutralSelf6,
    SpellId::DispelAllNeutralSelf7,
    SpellId::DispelAllNeutralSelf8,
];

/// ACE `SpellLevelProgression.DispelAllGoodSelf` (`List<SpellId>`).
pub static DISPEL_ALL_GOOD_SELF: [SpellId; 8] = [
    SpellId::DispelAllGoodSelf1,
    SpellId::DispelAllGoodSelf2,
    SpellId::DispelAllGoodSelf3,
    SpellId::DispelAllGoodSelf4,
    SpellId::DispelAllGoodSelf5,
    SpellId::DispelAllGoodSelf6,
    SpellId::DispelAllGoodSelf7,
    SpellId::DispelAllGoodSelf8,
];

/// ACE `SpellLevelProgression.DispelAllBadSelf` (`List<SpellId>`).
pub static DISPEL_ALL_BAD_SELF: [SpellId; 8] = [
    SpellId::DispelAllBadSelf1,
    SpellId::DispelAllBadSelf2,
    SpellId::DispelAllBadSelf3,
    SpellId::DispelAllBadSelf4,
    SpellId::DispelAllBadSelf5,
    SpellId::DispelAllBadSelf6,
    SpellId::DispelAllBadSelf7,
    SpellId::DispelAllBadSelf8,
];

/// ACE `SpellLevelProgression.DispelCreatureNeutralOther` (`List<SpellId>`).
pub static DISPEL_CREATURE_NEUTRAL_OTHER: [SpellId; 8] = [
    SpellId::DispelCreatureNeutralOther1,
    SpellId::DispelCreatureNeutralOther2,
    SpellId::DispelCreatureNeutralOther3,
    SpellId::DispelCreatureNeutralOther4,
    SpellId::DispelCreatureNeutralOther5,
    SpellId::DispelCreatureNeutralOther6,
    SpellId::DispelCreatureNeutralOther7,
    SpellId::DispelCreatureNeutralOther8,
];

/// ACE `SpellLevelProgression.DispelCreatureGoodOther` (`List<SpellId>`).
pub static DISPEL_CREATURE_GOOD_OTHER: [SpellId; 8] = [
    SpellId::DispelCreatureGoodOther1,
    SpellId::DispelCreatureGoodOther2,
    SpellId::DispelCreatureGoodOther3,
    SpellId::DispelCreatureGoodOther4,
    SpellId::DispelCreatureGoodOther5,
    SpellId::DispelCreatureGoodOther6,
    SpellId::DispelCreatureGoodOther7,
    SpellId::DispelCreatureGoodOther8,
];

/// ACE `SpellLevelProgression.DispelCreatureBadOther` (`List<SpellId>`).
pub static DISPEL_CREATURE_BAD_OTHER: [SpellId; 8] = [
    SpellId::DispelCreatureBadOther1,
    SpellId::DispelCreatureBadOther2,
    SpellId::DispelCreatureBadOther3,
    SpellId::DispelCreatureBadOther4,
    SpellId::DispelCreatureBadOther5,
    SpellId::DispelCreatureBadOther6,
    SpellId::DispelCreatureBadOther7,
    SpellId::DispelCreatureBadOther8,
];

/// ACE `SpellLevelProgression.DispelCreatureNeutralSelf` (`List<SpellId>`).
pub static DISPEL_CREATURE_NEUTRAL_SELF: [SpellId; 8] = [
    SpellId::DispelCreatureNeutralSelf1,
    SpellId::DispelCreatureNeutralSelf2,
    SpellId::DispelCreatureNeutralSelf3,
    SpellId::DispelCreatureNeutralSelf4,
    SpellId::DispelCreatureNeutralSelf5,
    SpellId::DispelCreatureNeutralSelf6,
    SpellId::DispelCreatureNeutralSelf7,
    SpellId::DispelCreatureNeutralSelf8,
];

/// ACE `SpellLevelProgression.DispelCreatureGoodSelf` (`List<SpellId>`).
pub static DISPEL_CREATURE_GOOD_SELF: [SpellId; 8] = [
    SpellId::DispelCreatureGoodSelf1,
    SpellId::DispelCreatureGoodSelf2,
    SpellId::DispelCreatureGoodSelf3,
    SpellId::DispelCreatureGoodSelf4,
    SpellId::DispelCreatureGoodSelf5,
    SpellId::DispelCreatureGoodSelf6,
    SpellId::DispelCreatureGoodSelf7,
    SpellId::DispelCreatureGoodSelf8,
];

/// ACE `SpellLevelProgression.DispelCreatureBadSelf` (`List<SpellId>`).
pub static DISPEL_CREATURE_BAD_SELF: [SpellId; 8] = [
    SpellId::DispelCreatureBadSelf1,
    SpellId::DispelCreatureBadSelf2,
    SpellId::DispelCreatureBadSelf3,
    SpellId::DispelCreatureBadSelf4,
    SpellId::DispelCreatureBadSelf5,
    SpellId::DispelCreatureBadSelf6,
    SpellId::DispelCreatureBadSelf7,
    SpellId::DispelCreatureBadSelf8,
];

/// ACE `SpellLevelProgression.DispelItemNeutralOther` (`List<SpellId>`).
pub static DISPEL_ITEM_NEUTRAL_OTHER: [SpellId; 8] = [
    SpellId::DispelItemNeutralOther1,
    SpellId::DispelItemNeutralOther2,
    SpellId::DispelItemNeutralOther3,
    SpellId::DispelItemNeutralOther4,
    SpellId::DispelItemNeutralOther5,
    SpellId::DispelItemNeutralOther6,
    SpellId::DispelItemNeutralOther7,
    SpellId::DispelItemNeutralOther8,
];

/// ACE `SpellLevelProgression.DispelItemGoodOther` (`List<SpellId>`).
pub static DISPEL_ITEM_GOOD_OTHER: [SpellId; 8] = [
    SpellId::DispelItemGoodOther1,
    SpellId::DispelItemGoodOther2,
    SpellId::DispelItemGoodOther3,
    SpellId::DispelItemGoodOther4,
    SpellId::DispelItemGoodOther5,
    SpellId::DispelItemGoodOther6,
    SpellId::DispelItemGoodOther7,
    SpellId::DispelItemGoodOther8,
];

/// ACE `SpellLevelProgression.DispelItemBadOther` (`List<SpellId>`).
pub static DISPEL_ITEM_BAD_OTHER: [SpellId; 8] = [
    SpellId::DispelItemBadOther1,
    SpellId::DispelItemBadOther2,
    SpellId::DispelItemBadOther3,
    SpellId::DispelItemBadOther4,
    SpellId::DispelItemBadOther5,
    SpellId::DispelItemBadOther6,
    SpellId::DispelItemBadOther7,
    SpellId::DispelItemBadOther8,
];

/// ACE `SpellLevelProgression.DispelItemNeutralSelf` (`List<SpellId>`).
pub static DISPEL_ITEM_NEUTRAL_SELF: [SpellId; 6] = [
    SpellId::DispelItemNeutralSelf1,
    SpellId::DispelItemNeutralSelf2,
    SpellId::DispelItemNeutralSelf3,
    SpellId::DispelItemNeutralSelf4,
    SpellId::DispelItemNeutralSelf5,
    SpellId::DispelItemNeutralSelf6,
];

/// ACE `SpellLevelProgression.DispelItemGoodSelf` (`List<SpellId>`).
pub static DISPEL_ITEM_GOOD_SELF: [SpellId; 6] = [
    SpellId::DispelItemGoodSelf1,
    SpellId::DispelItemGoodSelf2,
    SpellId::DispelItemGoodSelf3,
    SpellId::DispelItemGoodSelf4,
    SpellId::DispelItemGoodSelf5,
    SpellId::DispelItemGoodSelf6,
];

/// ACE `SpellLevelProgression.DispelItemBadSelf` (`List<SpellId>`).
pub static DISPEL_ITEM_BAD_SELF: [SpellId; 6] = [
    SpellId::DispelItemBadSelf1,
    SpellId::DispelItemBadSelf2,
    SpellId::DispelItemBadSelf3,
    SpellId::DispelItemBadSelf4,
    SpellId::DispelItemBadSelf5,
    SpellId::DispelItemBadSelf6,
];

/// ACE `SpellLevelProgression.DispelLifeNeutralOther` (`List<SpellId>`).
pub static DISPEL_LIFE_NEUTRAL_OTHER: [SpellId; 8] = [
    SpellId::DispelLifeNeutralOther1,
    SpellId::DispelLifeNeutralOther2,
    SpellId::DispelLifeNeutralOther3,
    SpellId::DispelLifeNeutralOther4,
    SpellId::DispelLifeNeutralOther5,
    SpellId::DispelLifeNeutralOther6,
    SpellId::DispelLifeNeutralOther7,
    SpellId::DispelLifeNeutralOther8,
];

/// ACE `SpellLevelProgression.DispelLifeGoodOther` (`List<SpellId>`).
pub static DISPEL_LIFE_GOOD_OTHER: [SpellId; 8] = [
    SpellId::DispelLifeGoodOther1,
    SpellId::DispelLifeGoodOther2,
    SpellId::DispelLifeGoodOther3,
    SpellId::DispelLifeGoodOther4,
    SpellId::DispelLifeGoodOther5,
    SpellId::DispelLifeGoodOther6,
    SpellId::DispelLifeGoodOther7,
    SpellId::DispelLifeGoodOther8,
];

/// ACE `SpellLevelProgression.DispelLifeBadOther` (`List<SpellId>`).
pub static DISPEL_LIFE_BAD_OTHER: [SpellId; 8] = [
    SpellId::DispelLifeBadOther1,
    SpellId::DispelLifeBadOther2,
    SpellId::DispelLifeBadOther3,
    SpellId::DispelLifeBadOther4,
    SpellId::DispelLifeBadOther5,
    SpellId::DispelLifeBadOther6,
    SpellId::DispelLifeBadOther7,
    SpellId::DispelLifeBadOther8,
];

/// ACE `SpellLevelProgression.DispelLifeNeutralSelf` (`List<SpellId>`).
pub static DISPEL_LIFE_NEUTRAL_SELF: [SpellId; 8] = [
    SpellId::DispelLifeNeutralSelf1,
    SpellId::DispelLifeNeutralSelf2,
    SpellId::DispelLifeNeutralSelf3,
    SpellId::DispelLifeNeutralSelf4,
    SpellId::DispelLifeNeutralSelf5,
    SpellId::DispelLifeNeutralSelf6,
    SpellId::DispelLifeNeutralSelf7,
    SpellId::DispelLifeNeutralSelf8,
];

/// ACE `SpellLevelProgression.DispelLifeGoodSelf` (`List<SpellId>`).
pub static DISPEL_LIFE_GOOD_SELF: [SpellId; 8] = [
    SpellId::DispelLifeGoodSelf1,
    SpellId::DispelLifeGoodSelf2,
    SpellId::DispelLifeGoodSelf3,
    SpellId::DispelLifeGoodSelf4,
    SpellId::DispelLifeGoodSelf5,
    SpellId::DispelLifeGoodSelf6,
    SpellId::DispelLifeGoodSelf7,
    SpellId::DispelLifeGoodSelf8,
];

/// ACE `SpellLevelProgression.DispelLifeBadSelf` (`List<SpellId>`).
pub static DISPEL_LIFE_BAD_SELF: [SpellId; 8] = [
    SpellId::DispelLifeBadSelf1,
    SpellId::DispelLifeBadSelf2,
    SpellId::DispelLifeBadSelf3,
    SpellId::DispelLifeBadSelf4,
    SpellId::DispelLifeBadSelf5,
    SpellId::DispelLifeBadSelf6,
    SpellId::DispelLifeBadSelf7,
    SpellId::DispelLifeBadSelf8,
];

/// ACE `SpellLevelProgression.RecallAsmolum` (`List<SpellId>`).
pub static RECALL_ASMOLUM: [SpellId; 3] = [
    SpellId::RecallAsmolum1,
    SpellId::RecallAsmolum2,
    SpellId::RecallAsmolum3,
];

/// ACE `SpellLevelProgression.PortalSendTrial` (`List<SpellId>`).
pub static PORTAL_SEND_TRIAL: [SpellId; 5] = [
    SpellId::PortalSendTrial1,
    SpellId::PortalSendTrial2,
    SpellId::PortalSendTrial3,
    SpellId::PortalSendTrial4,
    SpellId::PortalSendTrial5,
];

/// ACE `SpellLevelProgression.CANTRIPALCHEMICALPROWESS` (`List<SpellId>`).
pub static CANTRIPALCHEMICALPROWESS: [SpellId; 4] = [
    SpellId::CANTRIPALCHEMICALPROWESS1,
    SpellId::CANTRIPALCHEMICALPROWESS2,
    SpellId::CANTRIPALCHEMICALPROWESS3,
    SpellId::CantripAlchemicalProwess4,
];

/// ACE `SpellLevelProgression.CANTRIPARCANEPROWESS` (`List<SpellId>`).
pub static CANTRIPARCANEPROWESS: [SpellId; 4] = [
    SpellId::CANTRIPARCANEPROWESS1,
    SpellId::CANTRIPARCANEPROWESS2,
    SpellId::CANTRIPARCANEPROWESS3,
    SpellId::CantripArcaneProwess4,
];

/// ACE `SpellLevelProgression.CANTRIPARMOREXPERTISE` (`List<SpellId>`).
pub static CANTRIPARMOREXPERTISE: [SpellId; 4] = [
    SpellId::CANTRIPARMOREXPERTISE1,
    SpellId::CANTRIPARMOREXPERTISE2,
    SpellId::CANTRIPARMOREXPERTISE3,
    SpellId::CantripArmorExpertise4,
];

/// ACE `SpellLevelProgression.CANTRIPLIGHTWEAPONSAPTITUDE` (`List<SpellId>`).
pub static CANTRIPLIGHTWEAPONSAPTITUDE: [SpellId; 4] = [
    SpellId::CANTRIPLIGHTWEAPONSAPTITUDE1,
    SpellId::CANTRIPLIGHTWEAPONSAPTITUDE2,
    SpellId::CANTRIPLIGHTWEAPONSAPTITUDE3,
    SpellId::CantripLightWeaponsAptitude4,
];

/// ACE `SpellLevelProgression.CANTRIPMISSILEWEAPONSAPTITUDE` (`List<SpellId>`).
pub static CANTRIPMISSILEWEAPONSAPTITUDE: [SpellId; 4] = [
    SpellId::CANTRIPMISSILEWEAPONSAPTITUDE1,
    SpellId::CANTRIPMISSILEWEAPONSAPTITUDE2,
    SpellId::CANTRIPMISSILEWEAPONSAPTITUDE3,
    SpellId::CantripMissileWeaponsAptitude4,
];

/// ACE `SpellLevelProgression.CANTRIPCOOKINGPROWESS` (`List<SpellId>`).
pub static CANTRIPCOOKINGPROWESS: [SpellId; 4] = [
    SpellId::CANTRIPCOOKINGPROWESS1,
    SpellId::CANTRIPCOOKINGPROWESS2,
    SpellId::CANTRIPCOOKINGPROWESS3,
    SpellId::CantripCookingProwess4,
];

/// ACE `SpellLevelProgression.CANTRIPCREATUREENCHANTMENTAPTITUDE` (`List<SpellId>`).
pub static CANTRIPCREATUREENCHANTMENTAPTITUDE: [SpellId; 4] = [
    SpellId::CANTRIPCREATUREENCHANTMENTAPTITUDE1,
    SpellId::CANTRIPCREATUREENCHANTMENTAPTITUDE2,
    SpellId::CANTRIPCREATUREENCHANTMENTAPTITUDE3,
    SpellId::CantripCreatureEnchantmentAptitude4,
];

/// ACE `SpellLevelProgression.CANTRIPCROSSBOWAPTITUDE` (`List<SpellId>`).
pub static CANTRIPCROSSBOWAPTITUDE: [SpellId; 3] = [
    SpellId::CANTRIPCROSSBOWAPTITUDE1,
    SpellId::CANTRIPCROSSBOWAPTITUDE2,
    SpellId::CANTRIPCROSSBOWAPTITUDE3,
];

/// ACE `SpellLevelProgression.CANTRIPFINESSEWEAPONSAPTITUDE` (`List<SpellId>`).
pub static CANTRIPFINESSEWEAPONSAPTITUDE: [SpellId; 4] = [
    SpellId::CANTRIPFINESSEWEAPONSAPTITUDE1,
    SpellId::CANTRIPFINESSEWEAPONSAPTITUDE2,
    SpellId::CANTRIPFINESSEWEAPONSAPTITUDE3,
    SpellId::CantripFinesseWeaponsAptitude4,
];

/// ACE `SpellLevelProgression.CANTRIPDECEPTIONPROWESS` (`List<SpellId>`).
pub static CANTRIPDECEPTIONPROWESS: [SpellId; 4] = [
    SpellId::CANTRIPDECEPTIONPROWESS1,
    SpellId::CANTRIPDECEPTIONPROWESS2,
    SpellId::CANTRIPDECEPTIONPROWESS3,
    SpellId::CantripDeceptionProwess4,
];

/// ACE `SpellLevelProgression.CANTRIPFEALTY` (`List<SpellId>`).
pub static CANTRIPFEALTY: [SpellId; 4] = [
    SpellId::CANTRIPFEALTY1,
    SpellId::CANTRIPFEALTY2,
    SpellId::CANTRIPFEALTY3,
    SpellId::CantripFealty4,
];

/// ACE `SpellLevelProgression.CANTRIPFLETCHINGPROWESS` (`List<SpellId>`).
pub static CANTRIPFLETCHINGPROWESS: [SpellId; 4] = [
    SpellId::CANTRIPFLETCHINGPROWESS1,
    SpellId::CANTRIPFLETCHINGPROWESS2,
    SpellId::CANTRIPFLETCHINGPROWESS3,
    SpellId::CantripFletchingProwess4,
];

/// ACE `SpellLevelProgression.CANTRIPHEALINGPROWESS` (`List<SpellId>`).
pub static CANTRIPHEALINGPROWESS: [SpellId; 4] = [
    SpellId::CANTRIPHEALINGPROWESS1,
    SpellId::CANTRIPHEALINGPROWESS2,
    SpellId::CANTRIPHEALINGPROWESS3,
    SpellId::CantripHealingProwess4,
];

/// ACE `SpellLevelProgression.CANTRIPIMPREGNABILITY` (`List<SpellId>`).
pub static CANTRIPIMPREGNABILITY: [SpellId; 4] = [
    SpellId::CANTRIPIMPREGNABILITY1,
    SpellId::CANTRIPIMPREGNABILITY2,
    SpellId::CANTRIPIMPREGNABILITY3,
    SpellId::CantripImpregnability4,
];

/// ACE `SpellLevelProgression.CANTRIPINVULNERABILITY` (`List<SpellId>`).
pub static CANTRIPINVULNERABILITY: [SpellId; 4] = [
    SpellId::CANTRIPINVULNERABILITY1,
    SpellId::CANTRIPINVULNERABILITY2,
    SpellId::CANTRIPINVULNERABILITY3,
    SpellId::CantripInvulnerability4,
];

/// ACE `SpellLevelProgression.CANTRIPITEMENCHANTMENTAPTITUDE` (`List<SpellId>`).
pub static CANTRIPITEMENCHANTMENTAPTITUDE: [SpellId; 4] = [
    SpellId::CANTRIPITEMENCHANTMENTAPTITUDE1,
    SpellId::CANTRIPITEMENCHANTMENTAPTITUDE2,
    SpellId::CANTRIPITEMENCHANTMENTAPTITUDE3,
    SpellId::CantripItemEnchantmentAptitude4,
];

/// ACE `SpellLevelProgression.CANTRIPITEMEXPERTISE` (`List<SpellId>`).
pub static CANTRIPITEMEXPERTISE: [SpellId; 4] = [
    SpellId::CANTRIPITEMEXPERTISE1,
    SpellId::CANTRIPITEMEXPERTISE2,
    SpellId::CANTRIPITEMEXPERTISE3,
    SpellId::CantripItemExpertise4,
];

/// ACE `SpellLevelProgression.CANTRIPJUMPINGPROWESS` (`List<SpellId>`).
pub static CANTRIPJUMPINGPROWESS: [SpellId; 4] = [
    SpellId::CANTRIPJUMPINGPROWESS1,
    SpellId::CANTRIPJUMPINGPROWESS2,
    SpellId::CANTRIPJUMPINGPROWESS3,
    SpellId::CantripJumpingProwess4,
];

/// ACE `SpellLevelProgression.CANTRIPLEADERSHIP` (`List<SpellId>`).
pub static CANTRIPLEADERSHIP: [SpellId; 4] = [
    SpellId::CANTRIPLEADERSHIP1,
    SpellId::CANTRIPLEADERSHIP2,
    SpellId::CANTRIPLEADERSHIP3,
    SpellId::CantripLeadership4,
];

/// ACE `SpellLevelProgression.CANTRIPLIFEMAGICAPTITUDE` (`List<SpellId>`).
pub static CANTRIPLIFEMAGICAPTITUDE: [SpellId; 4] = [
    SpellId::CANTRIPLIFEMAGICAPTITUDE1,
    SpellId::CANTRIPLIFEMAGICAPTITUDE2,
    SpellId::CANTRIPLIFEMAGICAPTITUDE3,
    SpellId::CantripLifeMagicAptitude4,
];

/// ACE `SpellLevelProgression.CANTRIPLOCKPICKPROWESS` (`List<SpellId>`).
pub static CANTRIPLOCKPICKPROWESS: [SpellId; 4] = [
    SpellId::CANTRIPLOCKPICKPROWESS1,
    SpellId::CANTRIPLOCKPICKPROWESS2,
    SpellId::CANTRIPLOCKPICKPROWESS3,
    SpellId::CantripLockpickProwess4,
];

/// ACE `SpellLevelProgression.CANTRIPMACEAPTITUDE` (`List<SpellId>`).
pub static CANTRIPMACEAPTITUDE: [SpellId; 3] = [
    SpellId::CANTRIPMACEAPTITUDE1,
    SpellId::CANTRIPMACEAPTITUDE2,
    SpellId::CANTRIPMACEAPTITUDE3,
];

/// ACE `SpellLevelProgression.CANTRIPMAGICITEMEXPERTISE` (`List<SpellId>`).
pub static CANTRIPMAGICITEMEXPERTISE: [SpellId; 4] = [
    SpellId::CANTRIPMAGICITEMEXPERTISE1,
    SpellId::CANTRIPMAGICITEMEXPERTISE2,
    SpellId::CANTRIPMAGICITEMEXPERTISE3,
    SpellId::CantripMagicItemExpertise4,
];

/// ACE `SpellLevelProgression.CANTRIPMAGICRESISTANCE` (`List<SpellId>`).
pub static CANTRIPMAGICRESISTANCE: [SpellId; 4] = [
    SpellId::CANTRIPMAGICRESISTANCE1,
    SpellId::CANTRIPMAGICRESISTANCE2,
    SpellId::CANTRIPMAGICRESISTANCE3,
    SpellId::CantripMagicResistance4,
];

/// ACE `SpellLevelProgression.CANTRIPMANACONVERSIONPROWESS` (`List<SpellId>`).
pub static CANTRIPMANACONVERSIONPROWESS: [SpellId; 4] = [
    SpellId::CANTRIPMANACONVERSIONPROWESS1,
    SpellId::CANTRIPMANACONVERSIONPROWESS2,
    SpellId::CANTRIPMANACONVERSIONPROWESS3,
    SpellId::CantripManaConversionProwess4,
];

/// ACE `SpellLevelProgression.CANTRIPMONSTERATTUNEMENT` (`List<SpellId>`).
pub static CANTRIPMONSTERATTUNEMENT: [SpellId; 4] = [
    SpellId::CANTRIPMONSTERATTUNEMENT1,
    SpellId::CANTRIPMONSTERATTUNEMENT2,
    SpellId::CANTRIPMONSTERATTUNEMENT3,
    SpellId::CantripMonsterAttunement4,
];

/// ACE `SpellLevelProgression.CANTRIPPERSONATTUNEMENT` (`List<SpellId>`).
pub static CANTRIPPERSONATTUNEMENT: [SpellId; 4] = [
    SpellId::CANTRIPPERSONATTUNEMENT1,
    SpellId::CANTRIPPERSONATTUNEMENT2,
    SpellId::CANTRIPPERSONATTUNEMENT3,
    SpellId::CantripPersonAttunement4,
];

/// ACE `SpellLevelProgression.CANTRIPSPEARAPTITUDE` (`List<SpellId>`).
pub static CANTRIPSPEARAPTITUDE: [SpellId; 3] = [
    SpellId::CANTRIPSPEARAPTITUDE1,
    SpellId::CANTRIPSPEARAPTITUDE2,
    SpellId::CANTRIPSPEARAPTITUDE3,
];

/// ACE `SpellLevelProgression.CANTRIPSPRINT` (`List<SpellId>`).
pub static CANTRIPSPRINT: [SpellId; 4] = [
    SpellId::CANTRIPSPRINT1,
    SpellId::CANTRIPSPRINT2,
    SpellId::CANTRIPSPRINT3,
    SpellId::CantripSprint4,
];

/// ACE `SpellLevelProgression.CANTRIPSTAFFAPTITUDE` (`List<SpellId>`).
pub static CANTRIPSTAFFAPTITUDE: [SpellId; 3] = [
    SpellId::CANTRIPSTAFFAPTITUDE1,
    SpellId::CANTRIPSTAFFAPTITUDE2,
    SpellId::CANTRIPSTAFFAPTITUDE3,
];

/// ACE `SpellLevelProgression.CANTRIPHEAVYWEAPONSAPTITUDE` (`List<SpellId>`).
pub static CANTRIPHEAVYWEAPONSAPTITUDE: [SpellId; 4] = [
    SpellId::CANTRIPHEAVYWEAPONSAPTITUDE1,
    SpellId::CANTRIPHEAVYWEAPONSAPTITUDE2,
    SpellId::CANTRIPHEAVYWEAPONSAPTITUDE3,
    SpellId::CantripHeavyWeaponsAptitude4,
];

/// ACE `SpellLevelProgression.CANTRIPTHROWNAPTITUDE` (`List<SpellId>`).
pub static CANTRIPTHROWNAPTITUDE: [SpellId; 3] = [
    SpellId::CANTRIPTHROWNAPTITUDE1,
    SpellId::CANTRIPTHROWNAPTITUDE2,
    SpellId::CANTRIPTHROWNAPTITUDE3,
];

/// ACE `SpellLevelProgression.CANTRIPUNARMEDAPTITUDE` (`List<SpellId>`).
pub static CANTRIPUNARMEDAPTITUDE: [SpellId; 3] = [
    SpellId::CANTRIPUNARMEDAPTITUDE1,
    SpellId::CANTRIPUNARMEDAPTITUDE2,
    SpellId::CANTRIPUNARMEDAPTITUDE3,
];

/// ACE `SpellLevelProgression.CANTRIPWARMAGICAPTITUDE` (`List<SpellId>`).
pub static CANTRIPWARMAGICAPTITUDE: [SpellId; 4] = [
    SpellId::CANTRIPWARMAGICAPTITUDE1,
    SpellId::CANTRIPWARMAGICAPTITUDE2,
    SpellId::CANTRIPWARMAGICAPTITUDE3,
    SpellId::CantripWarMagicAptitude4,
];

/// ACE `SpellLevelProgression.CANTRIPWEAPONEXPERTISE` (`List<SpellId>`).
pub static CANTRIPWEAPONEXPERTISE: [SpellId; 4] = [
    SpellId::CANTRIPWEAPONEXPERTISE1,
    SpellId::CANTRIPWEAPONEXPERTISE2,
    SpellId::CANTRIPWEAPONEXPERTISE3,
    SpellId::CantripWeaponExpertise4,
];

/// ACE `SpellLevelProgression.CANTRIPARMOR` (`List<SpellId>`).
pub static CANTRIPARMOR: [SpellId; 4] = [
    SpellId::CANTRIPARMOR1,
    SpellId::CANTRIPARMOR2,
    SpellId::CANTRIPARMOR3,
    SpellId::CantripArmor4,
];

/// ACE `SpellLevelProgression.CANTRIPCOORDINATION` (`List<SpellId>`).
pub static CANTRIPCOORDINATION: [SpellId; 4] = [
    SpellId::CANTRIPCOORDINATION1,
    SpellId::CANTRIPCOORDINATION2,
    SpellId::CANTRIPCOORDINATION3,
    SpellId::CantripCoordination4,
];

/// ACE `SpellLevelProgression.CANTRIPENDURANCE` (`List<SpellId>`).
pub static CANTRIPENDURANCE: [SpellId; 4] = [
    SpellId::CANTRIPENDURANCE1,
    SpellId::CANTRIPENDURANCE2,
    SpellId::CANTRIPENDURANCE3,
    SpellId::CantripEndurance4,
];

/// ACE `SpellLevelProgression.CANTRIPFOCUS` (`List<SpellId>`).
pub static CANTRIPFOCUS: [SpellId; 4] = [
    SpellId::CANTRIPFOCUS1,
    SpellId::CANTRIPFOCUS2,
    SpellId::CANTRIPFOCUS3,
    SpellId::CantripFocus4,
];

/// ACE `SpellLevelProgression.CANTRIPQUICKNESS` (`List<SpellId>`).
pub static CANTRIPQUICKNESS: [SpellId; 4] = [
    SpellId::CANTRIPQUICKNESS1,
    SpellId::CANTRIPQUICKNESS2,
    SpellId::CANTRIPQUICKNESS3,
    SpellId::CantripQuickness4,
];

/// ACE `SpellLevelProgression.CANTRIPSTRENGTH` (`List<SpellId>`).
pub static CANTRIPSTRENGTH: [SpellId; 4] = [
    SpellId::CANTRIPSTRENGTH1,
    SpellId::CANTRIPSTRENGTH2,
    SpellId::CANTRIPSTRENGTH3,
    SpellId::CantripStrength4,
];

/// ACE `SpellLevelProgression.CANTRIPWILLPOWER` (`List<SpellId>`).
pub static CANTRIPWILLPOWER: [SpellId; 4] = [
    SpellId::CANTRIPWILLPOWER1,
    SpellId::CANTRIPWILLPOWER2,
    SpellId::CANTRIPWILLPOWER3,
    SpellId::CantripWillpower4,
];

/// ACE `SpellLevelProgression.CANTRIPACIDBANE` (`List<SpellId>`).
pub static CANTRIPACIDBANE: [SpellId; 4] = [
    SpellId::CANTRIPACIDBANE1,
    SpellId::CANTRIPACIDBANE2,
    SpellId::CANTRIPACIDBANE3,
    SpellId::CantripAcidBane4,
];

/// ACE `SpellLevelProgression.CANTRIPBLOODTHIRST` (`List<SpellId>`).
pub static CANTRIPBLOODTHIRST: [SpellId; 4] = [
    SpellId::CANTRIPBLOODTHIRST1,
    SpellId::CANTRIPBLOODTHIRST2,
    SpellId::CANTRIPBLOODTHIRST3,
    SpellId::CantripBloodThirst4,
];

/// ACE `SpellLevelProgression.CANTRIPBLUDGEONINGBANE` (`List<SpellId>`).
pub static CANTRIPBLUDGEONINGBANE: [SpellId; 4] = [
    SpellId::CANTRIPBLUDGEONINGBANE1,
    SpellId::CANTRIPBLUDGEONINGBANE2,
    SpellId::CANTRIPBLUDGEONINGBANE3,
    SpellId::CantripBludgeoningBane4,
];

/// ACE `SpellLevelProgression.CANTRIPDEFENDER` (`List<SpellId>`).
pub static CANTRIPDEFENDER: [SpellId; 4] = [
    SpellId::CANTRIPDEFENDER1,
    SpellId::CANTRIPDEFENDER2,
    SpellId::CANTRIPDEFENDER3,
    SpellId::CantripDefender4,
];

/// ACE `SpellLevelProgression.CANTRIPFLAMEBANE` (`List<SpellId>`).
pub static CANTRIPFLAMEBANE: [SpellId; 4] = [
    SpellId::CANTRIPFLAMEBANE1,
    SpellId::CANTRIPFLAMEBANE2,
    SpellId::CANTRIPFLAMEBANE3,
    SpellId::CantripFlameBane4,
];

/// ACE `SpellLevelProgression.CANTRIPFROSTBANE` (`List<SpellId>`).
pub static CANTRIPFROSTBANE: [SpellId; 4] = [
    SpellId::CANTRIPFROSTBANE1,
    SpellId::CANTRIPFROSTBANE2,
    SpellId::CANTRIPFROSTBANE3,
    SpellId::CantripFrostBane4,
];

/// ACE `SpellLevelProgression.CANTRIPHEARTTHIRST` (`List<SpellId>`).
pub static CANTRIPHEARTTHIRST: [SpellId; 4] = [
    SpellId::CANTRIPHEARTTHIRST1,
    SpellId::CANTRIPHEARTTHIRST2,
    SpellId::CANTRIPHEARTTHIRST3,
    SpellId::CantripHeartThirst4,
];

/// ACE `SpellLevelProgression.CANTRIPIMPENETRABILITY` (`List<SpellId>`).
pub static CANTRIPIMPENETRABILITY: [SpellId; 4] = [
    SpellId::CANTRIPIMPENETRABILITY1,
    SpellId::CANTRIPIMPENETRABILITY2,
    SpellId::CANTRIPIMPENETRABILITY3,
    SpellId::CantripImpenetrability4,
];

/// ACE `SpellLevelProgression.CANTRIPPIERCINGBANE` (`List<SpellId>`).
pub static CANTRIPPIERCINGBANE: [SpellId; 4] = [
    SpellId::CANTRIPPIERCINGBANE1,
    SpellId::CANTRIPPIERCINGBANE2,
    SpellId::CANTRIPPIERCINGBANE3,
    SpellId::CantripPiercingBane4,
];

/// ACE `SpellLevelProgression.CANTRIPSLASHINGBANE` (`List<SpellId>`).
pub static CANTRIPSLASHINGBANE: [SpellId; 4] = [
    SpellId::CANTRIPSLASHINGBANE1,
    SpellId::CANTRIPSLASHINGBANE2,
    SpellId::CANTRIPSLASHINGBANE3,
    SpellId::CantripSlashingBane4,
];

/// ACE `SpellLevelProgression.CANTRIPSTORMBANE` (`List<SpellId>`).
pub static CANTRIPSTORMBANE: [SpellId; 4] = [
    SpellId::CANTRIPSTORMBANE1,
    SpellId::CANTRIPSTORMBANE2,
    SpellId::CANTRIPSTORMBANE3,
    SpellId::CantripStormBane4,
];

/// ACE `SpellLevelProgression.CANTRIPSWIFTHUNTER` (`List<SpellId>`).
pub static CANTRIPSWIFTHUNTER: [SpellId; 4] = [
    SpellId::CANTRIPSWIFTHUNTER1,
    SpellId::CANTRIPSWIFTHUNTER2,
    SpellId::CANTRIPSWIFTHUNTER3,
    SpellId::CantripSwiftHunter4,
];

/// ACE `SpellLevelProgression.CANTRIPACIDWARD` (`List<SpellId>`).
pub static CANTRIPACIDWARD: [SpellId; 4] = [
    SpellId::CANTRIPACIDWARD1,
    SpellId::CANTRIPACIDWARD2,
    SpellId::CANTRIPACIDWARD3,
    SpellId::CantripAcidWard4,
];

/// ACE `SpellLevelProgression.CANTRIPBLUDGEONINGWARD` (`List<SpellId>`).
pub static CANTRIPBLUDGEONINGWARD: [SpellId; 4] = [
    SpellId::CANTRIPBLUDGEONINGWARD1,
    SpellId::CANTRIPBLUDGEONINGWARD2,
    SpellId::CANTRIPBLUDGEONINGWARD3,
    SpellId::CantripBludgeoningWard4,
];

/// ACE `SpellLevelProgression.CANTRIPFLAMEWARD` (`List<SpellId>`).
pub static CANTRIPFLAMEWARD: [SpellId; 4] = [
    SpellId::CANTRIPFLAMEWARD1,
    SpellId::CANTRIPFLAMEWARD2,
    SpellId::CANTRIPFLAMEWARD3,
    SpellId::CantripFlameWard4,
];

/// ACE `SpellLevelProgression.CANTRIPFROSTWARD` (`List<SpellId>`).
pub static CANTRIPFROSTWARD: [SpellId; 4] = [
    SpellId::CANTRIPFROSTWARD1,
    SpellId::CANTRIPFROSTWARD2,
    SpellId::CANTRIPFROSTWARD3,
    SpellId::CantripFrostWard4,
];

/// ACE `SpellLevelProgression.CANTRIPPIERCINGWARD` (`List<SpellId>`).
pub static CANTRIPPIERCINGWARD: [SpellId; 4] = [
    SpellId::CANTRIPPIERCINGWARD1,
    SpellId::CANTRIPPIERCINGWARD2,
    SpellId::CANTRIPPIERCINGWARD3,
    SpellId::CantripPiercingWard4,
];

/// ACE `SpellLevelProgression.CANTRIPSLASHINGWARD` (`List<SpellId>`).
pub static CANTRIPSLASHINGWARD: [SpellId; 4] = [
    SpellId::CANTRIPSLASHINGWARD1,
    SpellId::CANTRIPSLASHINGWARD2,
    SpellId::CANTRIPSLASHINGWARD3,
    SpellId::CantripSlashingWard4,
];

/// ACE `SpellLevelProgression.CANTRIPSTORMWARD` (`List<SpellId>`).
pub static CANTRIPSTORMWARD: [SpellId; 4] = [
    SpellId::CANTRIPSTORMWARD1,
    SpellId::CANTRIPSTORMWARD2,
    SpellId::CANTRIPSTORMWARD3,
    SpellId::CantripStormWard4,
];

/// ACE `SpellLevelProgression.CANTRIPHEALTHGAIN` (`List<SpellId>`).
pub static CANTRIPHEALTHGAIN: [SpellId; 4] = [
    SpellId::CANTRIPHEALTHGAIN1,
    SpellId::CANTRIPHEALTHGAIN2,
    SpellId::CANTRIPHEALTHGAIN3,
    SpellId::CantripHealthGain4,
];

/// ACE `SpellLevelProgression.CANTRIPMANAGAIN` (`List<SpellId>`).
pub static CANTRIPMANAGAIN: [SpellId; 4] = [
    SpellId::CANTRIPMANAGAIN1,
    SpellId::CANTRIPMANAGAIN2,
    SpellId::CANTRIPMANAGAIN3,
    SpellId::CantripManaGain4,
];

/// ACE `SpellLevelProgression.CANTRIPSTAMINAGAIN` (`List<SpellId>`).
pub static CANTRIPSTAMINAGAIN: [SpellId; 4] = [
    SpellId::CANTRIPSTAMINAGAIN1,
    SpellId::CANTRIPSTAMINAGAIN2,
    SpellId::CANTRIPSTAMINAGAIN3,
    SpellId::CantripStaminaGain4,
];

/// ACE `SpellLevelProgression.SummonSecondPortal` (`List<SpellId>`).
pub static SUMMON_SECOND_PORTAL: [SpellId; 3] = [
    SpellId::SummonSecondPortal1,
    SpellId::SummonSecondPortal2,
    SpellId::SummonSecondPortal3,
];

/// ACE `SpellLevelProgression.MartineRing` (`List<SpellId>`).
pub static MARTINE_RING: [SpellId; 2] = [
    SpellId::MartineRing1,
    SpellId::MartineRing2,
];

/// ACE `SpellLevelProgression.ElementalFury` (`List<SpellId>`).
pub static ELEMENTAL_FURY: [SpellId; 4] = [
    SpellId::ElementalFury1,
    SpellId::ElementalFury2,
    SpellId::ElementalFury3,
    SpellId::ElementalFury4,
];

/// ACE `SpellLevelProgression.AcidArc` (`List<SpellId>`).
pub static ACID_ARC: [SpellId; 8] = [
    SpellId::AcidArc1,
    SpellId::AcidArc2,
    SpellId::AcidArc3,
    SpellId::AcidArc4,
    SpellId::AcidArc5,
    SpellId::AcidArc6,
    SpellId::AcidArc7,
    SpellId::AcidArc8,
];

/// ACE `SpellLevelProgression.ForceArc` (`List<SpellId>`).
pub static FORCE_ARC: [SpellId; 8] = [
    SpellId::ForceArc1,
    SpellId::ForceArc2,
    SpellId::ForceArc3,
    SpellId::ForceArc4,
    SpellId::ForceArc5,
    SpellId::ForceArc6,
    SpellId::ForceArc7,
    SpellId::ForceArc8,
];

/// ACE `SpellLevelProgression.FrostArc` (`List<SpellId>`).
pub static FROST_ARC: [SpellId; 8] = [
    SpellId::FrostArc1,
    SpellId::FrostArc2,
    SpellId::FrostArc3,
    SpellId::FrostArc4,
    SpellId::FrostArc5,
    SpellId::FrostArc6,
    SpellId::FrostArc7,
    SpellId::FrostArc8,
];

/// ACE `SpellLevelProgression.LightningArc` (`List<SpellId>`).
pub static LIGHTNING_ARC: [SpellId; 8] = [
    SpellId::LightningArc1,
    SpellId::LightningArc2,
    SpellId::LightningArc3,
    SpellId::LightningArc4,
    SpellId::LightningArc5,
    SpellId::LightningArc6,
    SpellId::LightningArc7,
    SpellId::LightningArc8,
];

/// ACE `SpellLevelProgression.FlameArc` (`List<SpellId>`).
pub static FLAME_ARC: [SpellId; 8] = [
    SpellId::FlameArc1,
    SpellId::FlameArc2,
    SpellId::FlameArc3,
    SpellId::FlameArc4,
    SpellId::FlameArc5,
    SpellId::FlameArc6,
    SpellId::FlameArc7,
    SpellId::FlameArc8,
];

/// ACE `SpellLevelProgression.ShockArc` (`List<SpellId>`).
pub static SHOCK_ARC: [SpellId; 8] = [
    SpellId::ShockArc1,
    SpellId::ShockArc2,
    SpellId::ShockArc3,
    SpellId::ShockArc4,
    SpellId::ShockArc5,
    SpellId::ShockArc6,
    SpellId::ShockArc7,
    SpellId::ShockArc8,
];

/// ACE `SpellLevelProgression.BladeArc` (`List<SpellId>`).
pub static BLADE_ARC: [SpellId; 8] = [
    SpellId::BladeArc1,
    SpellId::BladeArc2,
    SpellId::BladeArc3,
    SpellId::BladeArc4,
    SpellId::BladeArc5,
    SpellId::BladeArc6,
    SpellId::BladeArc7,
    SpellId::BladeArc8,
];

/// ACE `SpellLevelProgression.HealthBolt` (`List<SpellId>`).
pub static HEALTH_BOLT: [SpellId; 8] = [
    SpellId::HealthBolt1,
    SpellId::HealthBolt2,
    SpellId::HealthBolt3,
    SpellId::HealthBolt4,
    SpellId::HealthBolt5,
    SpellId::HealthBolt6,
    SpellId::HealthBolt7,
    SpellId::HealthBolt8,
];

/// ACE `SpellLevelProgression.StaminaBolt` (`List<SpellId>`).
pub static STAMINA_BOLT: [SpellId; 8] = [
    SpellId::StaminaBolt1,
    SpellId::StaminaBolt2,
    SpellId::StaminaBolt3,
    SpellId::StaminaBolt4,
    SpellId::StaminaBolt5,
    SpellId::StaminaBolt6,
    SpellId::StaminaBolt7,
    SpellId::StaminaBolt8,
];

/// ACE `SpellLevelProgression.ManaBolt` (`List<SpellId>`).
pub static MANA_BOLT: [SpellId; 8] = [
    SpellId::ManaBolt1,
    SpellId::ManaBolt2,
    SpellId::ManaBolt3,
    SpellId::ManaBolt4,
    SpellId::ManaBolt5,
    SpellId::ManaBolt6,
    SpellId::ManaBolt7,
    SpellId::ManaBolt8,
];

/// ACE `SpellLevelProgression.FireworkOutBlack` (`List<SpellId>`).
pub static FIREWORK_OUT_BLACK: [SpellId; 8] = [
    SpellId::FireworkOutBlack1,
    SpellId::FireworkOutBlack2,
    SpellId::FireworkOutBlack3,
    SpellId::FireworkOutBlack4,
    SpellId::FireworkOutBlack5,
    SpellId::FireworkOutBlack6,
    SpellId::FireworkOutBlack7,
    SpellId::FireworkOutBlack8,
];

/// ACE `SpellLevelProgression.FireworkOutBlue` (`List<SpellId>`).
pub static FIREWORK_OUT_BLUE: [SpellId; 8] = [
    SpellId::FireworkOutBlue1,
    SpellId::FireworkOutBlue2,
    SpellId::FireworkOutBlue3,
    SpellId::FireworkOutBlue4,
    SpellId::FireworkOutBlue5,
    SpellId::FireworkOutBlue6,
    SpellId::FireworkOutBlue7,
    SpellId::FireworkOutBlue8,
];

/// ACE `SpellLevelProgression.FireworkOutGreen` (`List<SpellId>`).
pub static FIREWORK_OUT_GREEN: [SpellId; 8] = [
    SpellId::FireworkOutGreen1,
    SpellId::FireworkOutGreen2,
    SpellId::FireworkOutGreen3,
    SpellId::FireworkOutGreen4,
    SpellId::FireworkOutGreen5,
    SpellId::FireworkOutGreen6,
    SpellId::FireworkOutGreen7,
    SpellId::FireworkOutGreen8,
];

/// ACE `SpellLevelProgression.FireworkOutOrange` (`List<SpellId>`).
pub static FIREWORK_OUT_ORANGE: [SpellId; 8] = [
    SpellId::FireworkOutOrange1,
    SpellId::FireworkOutOrange2,
    SpellId::FireworkOutOrange3,
    SpellId::FireworkOutOrange4,
    SpellId::FireworkOutOrange5,
    SpellId::FireworkOutOrange6,
    SpellId::FireworkOutOrange7,
    SpellId::FireworkOutOrange8,
];

/// ACE `SpellLevelProgression.FireworkOutPurple` (`List<SpellId>`).
pub static FIREWORK_OUT_PURPLE: [SpellId; 8] = [
    SpellId::FireworkOutPurple1,
    SpellId::FireworkOutPurple2,
    SpellId::FireworkOutPurple3,
    SpellId::FireworkOutPurple4,
    SpellId::FireworkOutPurple5,
    SpellId::FireworkOutPurple6,
    SpellId::FireworkOutPurple7,
    SpellId::FireworkOutPurple8,
];

/// ACE `SpellLevelProgression.FireworkOutRed` (`List<SpellId>`).
pub static FIREWORK_OUT_RED: [SpellId; 8] = [
    SpellId::FireworkOutRed1,
    SpellId::FireworkOutRed2,
    SpellId::FireworkOutRed3,
    SpellId::FireworkOutRed4,
    SpellId::FireworkOutRed5,
    SpellId::FireworkOutRed6,
    SpellId::FireworkOutRed7,
    SpellId::FireworkOutRed8,
];

/// ACE `SpellLevelProgression.FireworkOutWhite` (`List<SpellId>`).
pub static FIREWORK_OUT_WHITE: [SpellId; 8] = [
    SpellId::FireworkOutWhite1,
    SpellId::FireworkOutWhite2,
    SpellId::FireworkOutWhite3,
    SpellId::FireworkOutWhite4,
    SpellId::FireworkOutWhite5,
    SpellId::FireworkOutWhite6,
    SpellId::FireworkOutWhite7,
    SpellId::FireworkOutWhite8,
];

/// ACE `SpellLevelProgression.FireworkOutYellow` (`List<SpellId>`).
pub static FIREWORK_OUT_YELLOW: [SpellId; 8] = [
    SpellId::FireworkOutYellow1,
    SpellId::FireworkOutYellow2,
    SpellId::FireworkOutYellow3,
    SpellId::FireworkOutYellow4,
    SpellId::FireworkOutYellow5,
    SpellId::FireworkOutYellow6,
    SpellId::FireworkOutYellow7,
    SpellId::FireworkOutYellow8,
];

/// ACE `SpellLevelProgression.FireworkUpBlack` (`List<SpellId>`).
pub static FIREWORK_UP_BLACK: [SpellId; 8] = [
    SpellId::FireworkUpBlack1,
    SpellId::FireworkUpBlack2,
    SpellId::FireworkUpBlack3,
    SpellId::FireworkUpBlack4,
    SpellId::FireworkUpBlack5,
    SpellId::FireworkUpBlack6,
    SpellId::FireworkUpBlack7,
    SpellId::FireworkUpBlack8,
];

/// ACE `SpellLevelProgression.FireworkUpBlue` (`List<SpellId>`).
pub static FIREWORK_UP_BLUE: [SpellId; 8] = [
    SpellId::FireworkUpBlue1,
    SpellId::FireworkUpBlue2,
    SpellId::FireworkUpBlue3,
    SpellId::FireworkUpBlue4,
    SpellId::FireworkUpBlue5,
    SpellId::FireworkUpBlue6,
    SpellId::FireworkUpBlue7,
    SpellId::FireworkUpBlue8,
];

/// ACE `SpellLevelProgression.FireworkUpGreen` (`List<SpellId>`).
pub static FIREWORK_UP_GREEN: [SpellId; 8] = [
    SpellId::FireworkUpGreen1,
    SpellId::FireworkUpGreen2,
    SpellId::FireworkUpGreen3,
    SpellId::FireworkUpGreen4,
    SpellId::FireworkUpGreen5,
    SpellId::FireworkUpGreen6,
    SpellId::FireworkUpGreen7,
    SpellId::FireworkUpGreen8,
];

/// ACE `SpellLevelProgression.FireworkUpOrange` (`List<SpellId>`).
pub static FIREWORK_UP_ORANGE: [SpellId; 8] = [
    SpellId::FireworkUpOrange1,
    SpellId::FireworkUpOrange2,
    SpellId::FireworkUpOrange3,
    SpellId::FireworkUpOrange4,
    SpellId::FireworkUpOrange5,
    SpellId::FireworkUpOrange6,
    SpellId::FireworkUpOrange7,
    SpellId::FireworkUpOrange8,
];

/// ACE `SpellLevelProgression.FireworkUpPurple` (`List<SpellId>`).
pub static FIREWORK_UP_PURPLE: [SpellId; 8] = [
    SpellId::FireworkUpPurple1,
    SpellId::FireworkUpPurple2,
    SpellId::FireworkUpPurple3,
    SpellId::FireworkUpPurple4,
    SpellId::FireworkUpPurple5,
    SpellId::FireworkUpPurple6,
    SpellId::FireworkUpPurple7,
    SpellId::FireworkUpPurple8,
];

/// ACE `SpellLevelProgression.FireworkUpRed` (`List<SpellId>`).
pub static FIREWORK_UP_RED: [SpellId; 8] = [
    SpellId::FireworkUpRed1,
    SpellId::FireworkUpRed2,
    SpellId::FireworkUpRed3,
    SpellId::FireworkUpRed4,
    SpellId::FireworkUpRed5,
    SpellId::FireworkUpRed6,
    SpellId::FireworkUpRed7,
    SpellId::FireworkUpRed8,
];

/// ACE `SpellLevelProgression.FireworkUpWhite` (`List<SpellId>`).
pub static FIREWORK_UP_WHITE: [SpellId; 8] = [
    SpellId::FireworkUpWhite1,
    SpellId::FireworkUpWhite2,
    SpellId::FireworkUpWhite3,
    SpellId::FireworkUpWhite4,
    SpellId::FireworkUpWhite5,
    SpellId::FireworkUpWhite6,
    SpellId::FireworkUpWhite7,
    SpellId::FireworkUpWhite8,
];

/// ACE `SpellLevelProgression.FireworkUpYellow` (`List<SpellId>`).
pub static FIREWORK_UP_YELLOW: [SpellId; 8] = [
    SpellId::FireworkUpYellow1,
    SpellId::FireworkUpYellow2,
    SpellId::FireworkUpYellow3,
    SpellId::FireworkUpYellow4,
    SpellId::FireworkUpYellow5,
    SpellId::FireworkUpYellow6,
    SpellId::FireworkUpYellow7,
    SpellId::FireworkUpYellow8,
];

/// ACE `SpellLevelProgression.PortalSendingKnorr` (`List<SpellId>`).
pub static PORTAL_SENDING_KNORR: [SpellId; 3] = [
    SpellId::PortalSendingKnorr,
    SpellId::PortalSendingKnorr2,
    SpellId::PortalSendingKnorr3,
];

/// ACE `SpellLevelProgression.PortalSendingFellowshipLiazkBurun` (`List<SpellId>`).
pub static PORTAL_SENDING_FELLOWSHIP_LIAZK_BURUN: [SpellId; 4] = [
    SpellId::PortalSendingFellowshipLiazkBurun40,
    SpellId::PortalSendingFellowshipLiazkBurun60,
    SpellId::PortalSendingFellowshipLiazkBurun80,
    SpellId::PortalSendingFellowshipLiazkBurun100,
];

/// ACE `SpellLevelProgression.PortalSendingLiazkBurun` (`List<SpellId>`).
pub static PORTAL_SENDING_LIAZK_BURUN: [SpellId; 4] = [
    SpellId::PortalSendingLiazkBurun40,
    SpellId::PortalSendingLiazkBurun60,
    SpellId::PortalSendingLiazkBurun80,
    SpellId::PortalSendingLiazkBurun100,
];

/// ACE `SpellLevelProgression.PortalSendingLiazkJump` (`List<SpellId>`).
pub static PORTAL_SENDING_LIAZK_JUMP: [SpellId; 4] = [
    SpellId::PortalSendingLiazkJump40,
    SpellId::PortalSendingLiazkJump60,
    SpellId::PortalSendingLiazkJump80,
    SpellId::PortalSendingLiazkJump100,
];

/// ACE `SpellLevelProgression.PortalSendingLiazkSpirits` (`List<SpellId>`).
pub static PORTAL_SENDING_LIAZK_SPIRITS: [SpellId; 4] = [
    SpellId::PortalSendingLiazkSpirits40,
    SpellId::PortalSendingLiazkSpirits60,
    SpellId::PortalSendingLiazkSpirits80,
    SpellId::PortalSendingLiazkSpirits100,
];

/// ACE `SpellLevelProgression.PortalSendingLiazkTest` (`List<SpellId>`).
pub static PORTAL_SENDING_LIAZK_TEST: [SpellId; 4] = [
    SpellId::PortalSendingLiazkTest40,
    SpellId::PortalSendingLiazkTest60,
    SpellId::PortalSendingLiazkTest80,
    SpellId::PortalSendingLiazkTest100,
];

/// ACE `SpellLevelProgression.CoordinationFellowship` (`List<SpellId>`).
pub static COORDINATION_FELLOWSHIP: [SpellId; 5] = [
    SpellId::CoordinationFellowship4,
    SpellId::CoordinationFellowship5,
    SpellId::CoordinationFellowship6,
    SpellId::CoordinationFellowship7,
    SpellId::CoordinationFellowship8,
];

/// ACE `SpellLevelProgression.EnduranceFellowship` (`List<SpellId>`).
pub static ENDURANCE_FELLOWSHIP: [SpellId; 5] = [
    SpellId::EnduranceFellowship4,
    SpellId::EnduranceFellowship5,
    SpellId::EnduranceFellowship6,
    SpellId::EnduranceFellowship7,
    SpellId::EnduranceFellowship8,
];

/// ACE `SpellLevelProgression.FocusFellowship` (`List<SpellId>`).
pub static FOCUS_FELLOWSHIP: [SpellId; 5] = [
    SpellId::FocusFellowship4,
    SpellId::FocusFellowship5,
    SpellId::FocusFellowship6,
    SpellId::FocusFellowship7,
    SpellId::FocusFellowship8,
];

/// ACE `SpellLevelProgression.QuicknessFellowship` (`List<SpellId>`).
pub static QUICKNESS_FELLOWSHIP: [SpellId; 5] = [
    SpellId::QuicknessFellowship4,
    SpellId::QuicknessFellowship5,
    SpellId::QuicknessFellowship6,
    SpellId::QuicknessFellowship7,
    SpellId::QuicknessFellowship8,
];

/// ACE `SpellLevelProgression.SelfFellowship` (`List<SpellId>`).
pub static SELF_FELLOWSHIP: [SpellId; 5] = [
    SpellId::SelfFellowship4,
    SpellId::SelfFellowship5,
    SpellId::SelfFellowship6,
    SpellId::SelfFellowship7,
    SpellId::SelfFellowship8,
];

/// ACE `SpellLevelProgression.StrengthFellowship` (`List<SpellId>`).
pub static STRENGTH_FELLOWSHIP: [SpellId; 5] = [
    SpellId::StrengthFellowship4,
    SpellId::StrengthFellowship5,
    SpellId::StrengthFellowship6,
    SpellId::StrengthFellowship7,
    SpellId::StrengthFellowship8,
];

/// ACE `SpellLevelProgression.CantripHermeticLink` (`List<SpellId>`).
pub static CANTRIP_HERMETIC_LINK: [SpellId; 4] = [
    SpellId::CantripHermeticLink1,
    SpellId::CantripHermeticLink2,
    SpellId::CantripHermeticLink3,
    SpellId::CantripHermeticLink4,
];

/// ACE `SpellLevelProgression.CantripSpiritThirst` (`List<SpellId>`).
pub static CANTRIP_SPIRIT_THIRST: [SpellId; 4] = [
    SpellId::CantripSpiritThirst1,
    SpellId::CantripSpiritThirst2,
    SpellId::CANTRIPSPIRITTHIRST3,
    SpellId::CantripSpiritThirst4,
];

/// ACE `SpellLevelProgression.SpiritDrinkerSelf` (`List<SpellId>`).
pub static SPIRIT_DRINKER_SELF: [SpellId; 8] = [
    SpellId::SpiritDrinkerSelf1,
    SpellId::SpiritDrinkerSelf2,
    SpellId::SpiritDrinkerSelf3,
    SpellId::SpiritDrinkerSelf4,
    SpellId::SpiritDrinkerSelf5,
    SpellId::SpiritDrinkerSelf6,
    SpellId::SpiritDrinkerSelf7,
    SpellId::SpiritDrinkerSelf8,
];

/// ACE `SpellLevelProgression.SpiritLoather` (`List<SpellId>`).
pub static SPIRIT_LOATHER: [SpellId; 8] = [
    SpellId::SpiritLoather1,
    SpellId::SpiritLoather2,
    SpellId::SpiritLoather3,
    SpellId::SpiritLoather4,
    SpellId::SpiritLoather5,
    SpellId::SpiritLoather6,
    SpellId::SpiritLoather7,
    SpellId::SpiritLoather8,
];

/// ACE `SpellLevelProgression.PortalSendingHezhitFight` (`List<SpellId>`).
pub static PORTAL_SENDING_HEZHIT_FIGHT: [SpellId; 3] = [
    SpellId::PortalSendingHezhitFight1,
    SpellId::PortalSendingHezhitFight2,
    SpellId::PortalSendingHezhitFight3,
];

/// ACE `SpellLevelProgression.PortalSendingHezhitPrison` (`List<SpellId>`).
pub static PORTAL_SENDING_HEZHIT_PRISON: [SpellId; 6] = [
    SpellId::PortalSendingHezhitPrison1,
    SpellId::PortalSendingHezhitPrison2,
    SpellId::PortalSendingHezhitPrison3,
    SpellId::PortalSendingHezhitPrison4,
    SpellId::PortalSendingHezhitPrison5,
    SpellId::PortalSendingHezhitPrison6,
];

/// ACE `SpellLevelProgression.PortalSendingHizkRiGauntlet` (`List<SpellId>`).
pub static PORTAL_SENDING_HIZK_RI_GAUNTLET: [SpellId; 3] = [
    SpellId::PortalSendingHizkRiGauntlet60,
    SpellId::PortalSendingHizkRiGauntlet80,
    SpellId::PortalSendingHizkRiGauntlet100,
];

/// ACE `SpellLevelProgression.PortalSendingHizkRiWell` (`List<SpellId>`).
pub static PORTAL_SENDING_HIZK_RI_WELL: [SpellId; 3] = [
    SpellId::PortalSendingHizkRiWell60,
    SpellId::PortalSendingHizkRiWell80,
    SpellId::PortalSendingHizkRiWell100,
];

/// ACE `SpellLevelProgression.PortalSendingJrvikFight` (`List<SpellId>`).
pub static PORTAL_SENDING_JRVIK_FIGHT: [SpellId; 3] = [
    SpellId::PortalSendingJrvikFight1,
    SpellId::PortalSendingJrvikFight2,
    SpellId::PortalSendingJrvikFight3,
];

/// ACE `SpellLevelProgression.PortalSendingJrvikPrison` (`List<SpellId>`).
pub static PORTAL_SENDING_JRVIK_PRISON: [SpellId; 6] = [
    SpellId::PortalSendingJrvikPrison1,
    SpellId::PortalSendingJrvikPrison2,
    SpellId::PortalSendingJrvikPrison3,
    SpellId::PortalSendingJrvikPrison4,
    SpellId::PortalSendingJrvikPrison5,
    SpellId::PortalSendingJrvikPrison6,
];

/// ACE `SpellLevelProgression.PortalSendingZixkFight` (`List<SpellId>`).
pub static PORTAL_SENDING_ZIXK_FIGHT: [SpellId; 3] = [
    SpellId::PortalSendingZixkFight1,
    SpellId::PortalSendingZixkFight2,
    SpellId::PortalSendingZixkFight3,
];

/// ACE `SpellLevelProgression.PortalSendingZixkPrison` (`List<SpellId>`).
pub static PORTAL_SENDING_ZIXK_PRISON: [SpellId; 6] = [
    SpellId::PortalSendingZixkPrison1,
    SpellId::PortalSendingZixkPrison2,
    SpellId::PortalSendingZixkPrison3,
    SpellId::PortalSendingZixkPrison4,
    SpellId::PortalSendingZixkPrison5,
    SpellId::PortalSendingZixkPrison6,
];

/// ACE `SpellLevelProgression.PortalSendingHizkRiGuruk` (`List<SpellId>`).
pub static PORTAL_SENDING_HIZK_RI_GURUK: [SpellId; 3] = [
    SpellId::PortalSendingHizkRiGuruk60,
    SpellId::PortalSendingHizkRiGuruk80,
    SpellId::PortalSendingHizkRiGuruk100,
];

/// ACE `SpellLevelProgression.AcidProtectionFellowship` (`List<SpellId>`).
pub static ACID_PROTECTION_FELLOWSHIP: [SpellId; 5] = [
    SpellId::AcidProtectionFellowship4,
    SpellId::AcidProtectionFellowship5,
    SpellId::AcidProtectionFellowship6,
    SpellId::AcidProtectionFellowship7,
    SpellId::AcidProtectionFellowship8,
];

/// ACE `SpellLevelProgression.BladeProtectionFellowship` (`List<SpellId>`).
pub static BLADE_PROTECTION_FELLOWSHIP: [SpellId; 5] = [
    SpellId::BladeProtectionFellowship4,
    SpellId::BladeProtectionFellowship5,
    SpellId::BladeProtectionFellowship6,
    SpellId::BladeProtectionFellowship7,
    SpellId::BladeProtectionFellowship8,
];

/// ACE `SpellLevelProgression.BludgeonProtectionFellowship` (`List<SpellId>`).
pub static BLUDGEON_PROTECTION_FELLOWSHIP: [SpellId; 5] = [
    SpellId::BludgeonProtectionFellowship4,
    SpellId::BludgeonProtectionFellowship5,
    SpellId::BludgeonProtectionFellowship6,
    SpellId::BludgeonProtectionFellowship7,
    SpellId::BludgeonProtectionFellowship8,
];

/// ACE `SpellLevelProgression.ColdProtectionFellowship` (`List<SpellId>`).
pub static COLD_PROTECTION_FELLOWSHIP: [SpellId; 5] = [
    SpellId::ColdProtectionFellowship4,
    SpellId::ColdProtectionFellowship5,
    SpellId::ColdProtectionFellowship6,
    SpellId::ColdProtectionFellowship7,
    SpellId::ColdProtectionFellowship8,
];

/// ACE `SpellLevelProgression.FireProtectionFellowship` (`List<SpellId>`).
pub static FIRE_PROTECTION_FELLOWSHIP: [SpellId; 5] = [
    SpellId::FireProtectionFellowship4,
    SpellId::FireProtectionFellowship5,
    SpellId::FireProtectionFellowship6,
    SpellId::FireProtectionFellowship7,
    SpellId::FireProtectionFellowship8,
];

/// ACE `SpellLevelProgression.LightningProtectionFellowship` (`List<SpellId>`).
pub static LIGHTNING_PROTECTION_FELLOWSHIP: [SpellId; 5] = [
    SpellId::LightningProtectionFellowship4,
    SpellId::LightningProtectionFellowship5,
    SpellId::LightningProtectionFellowship6,
    SpellId::LightningProtectionFellowship7,
    SpellId::LightningProtectionFellowship8,
];

/// ACE `SpellLevelProgression.PierceProtectionFellowship` (`List<SpellId>`).
pub static PIERCE_PROTECTION_FELLOWSHIP: [SpellId; 5] = [
    SpellId::PierceProtectionFellowship4,
    SpellId::PierceProtectionFellowship5,
    SpellId::PierceProtectionFellowship6,
    SpellId::PierceProtectionFellowship7,
    SpellId::PierceProtectionFellowship8,
];

/// ACE `SpellLevelProgression.ImpregnabilityFellowship` (`List<SpellId>`).
pub static IMPREGNABILITY_FELLOWSHIP: [SpellId; 5] = [
    SpellId::ImpregnabilityFellowship4,
    SpellId::ImpregnabilityFellowship5,
    SpellId::ImpregnabilityFellowship6,
    SpellId::ImpregnabilityFellowship7,
    SpellId::ImpregnabilityFellowship8,
];

/// ACE `SpellLevelProgression.InvulnerabilityFellowship` (`List<SpellId>`).
pub static INVULNERABILITY_FELLOWSHIP: [SpellId; 5] = [
    SpellId::InvulnerabilityFellowship4,
    SpellId::InvulnerabilityFellowship5,
    SpellId::InvulnerabilityFellowship6,
    SpellId::InvulnerabilityFellowship7,
    SpellId::InvulnerabilityFellowship8,
];

/// ACE `SpellLevelProgression.MagicResistanceFellowship` (`List<SpellId>`).
pub static MAGIC_RESISTANCE_FELLOWSHIP: [SpellId; 5] = [
    SpellId::MagicResistanceFellowship4,
    SpellId::MagicResistanceFellowship5,
    SpellId::MagicResistanceFellowship6,
    SpellId::MagicResistanceFellowship7,
    SpellId::MagicResistanceFellowship8,
];

/// ACE `SpellLevelProgression.CreatureEnchantmentMasteryFellow` (`List<SpellId>`).
pub static CREATURE_ENCHANTMENT_MASTERY_FELLOW: [SpellId; 5] = [
    SpellId::CreatureEnchantmentMasteryFellow4,
    SpellId::CreatureEnchantmentMasteryFellow5,
    SpellId::CreatureEnchantmentMasteryFellow6,
    SpellId::CreatureEnchantmentMasteryFellow7,
    SpellId::CreatureEnchantmentMasteryFellow8,
];

/// ACE `SpellLevelProgression.ItemEnchantmentMasteryFellow` (`List<SpellId>`).
pub static ITEM_ENCHANTMENT_MASTERY_FELLOW: [SpellId; 5] = [
    SpellId::ItemEnchantmentMasteryFellow4,
    SpellId::ItemEnchantmentMasteryFellow5,
    SpellId::ItemEnchantmentMasteryFellow6,
    SpellId::ItemEnchantmentMasteryFellow7,
    SpellId::ItemEnchantmentMasteryFellow8,
];

/// ACE `SpellLevelProgression.LifeMagicMasteryFellow` (`List<SpellId>`).
pub static LIFE_MAGIC_MASTERY_FELLOW: [SpellId; 5] = [
    SpellId::LifeMagicMasteryFellow4,
    SpellId::LifeMagicMasteryFellow5,
    SpellId::LifeMagicMasteryFellow6,
    SpellId::LifeMagicMasteryFellow7,
    SpellId::LifeMagicMasteryFellow8,
];

/// ACE `SpellLevelProgression.ManaConversionMasteryFellow` (`List<SpellId>`).
pub static MANA_CONVERSION_MASTERY_FELLOW: [SpellId; 5] = [
    SpellId::ManaConversionMasteryFellow4,
    SpellId::ManaConversionMasteryFellow5,
    SpellId::ManaConversionMasteryFellow6,
    SpellId::ManaConversionMasteryFellow7,
    SpellId::ManaConversionMasteryFellow8,
];

/// ACE `SpellLevelProgression.WarMagicMasteryFellow` (`List<SpellId>`).
pub static WAR_MAGIC_MASTERY_FELLOW: [SpellId; 5] = [
    SpellId::WarMagicMasteryFellow4,
    SpellId::WarMagicMasteryFellow5,
    SpellId::WarMagicMasteryFellow6,
    SpellId::WarMagicMasteryFellow7,
    SpellId::WarMagicMasteryFellow8,
];

/// ACE `SpellLevelProgression.PortalSendingKivikLirArena` (`List<SpellId>`).
pub static PORTAL_SENDING_KIVIK_LIR_ARENA: [SpellId; 3] = [
    SpellId::PortalSendingKivikLirArena60,
    SpellId::PortalSendingKivikLirArena80,
    SpellId::PortalSendingKivikLirArena100,
];

/// ACE `SpellLevelProgression.PortalSendingKivikLirBoss` (`List<SpellId>`).
pub static PORTAL_SENDING_KIVIK_LIR_BOSS: [SpellId; 3] = [
    SpellId::PortalSendingKivikLirBoss60,
    SpellId::PortalSendingKivikLirBoss80,
    SpellId::PortalSendingKivikLirBoss100,
];

/// ACE `SpellLevelProgression.PortalSendingKivikLirHaven` (`List<SpellId>`).
pub static PORTAL_SENDING_KIVIK_LIR_HAVEN: [SpellId; 3] = [
    SpellId::PortalSendingKivikLirHaven60,
    SpellId::PortalSendingKivikLirHaven80,
    SpellId::PortalSendingKivikLirHaven100,
];

/// ACE `SpellLevelProgression.ManaRenewalFellowship` (`List<SpellId>`).
pub static MANA_RENEWAL_FELLOWSHIP: [SpellId; 5] = [
    SpellId::ManaRenewalFellowship4,
    SpellId::ManaRenewalFellowship5,
    SpellId::ManaRenewalFellowship6,
    SpellId::ManaRenewalFellowship7,
    SpellId::ManaRenewalFellowship8,
];

/// ACE `SpellLevelProgression.RegenerationFellowship` (`List<SpellId>`).
pub static REGENERATION_FELLOWSHIP: [SpellId; 5] = [
    SpellId::RegenerationFellowship4,
    SpellId::RegenerationFellowship5,
    SpellId::RegenerationFellowship6,
    SpellId::RegenerationFellowship7,
    SpellId::RegenerationFellowship8,
];

/// ACE `SpellLevelProgression.RejuvenationFellowship` (`List<SpellId>`).
pub static REJUVENATION_FELLOWSHIP: [SpellId; 5] = [
    SpellId::RejuvenationFellowship4,
    SpellId::RejuvenationFellowship5,
    SpellId::RejuvenationFellowship6,
    SpellId::RejuvenationFellowship7,
    SpellId::RejuvenationFellowship8,
];

/// ACE `SpellLevelProgression.PortalSendingIzjiQoGauntlet` (`List<SpellId>`).
pub static PORTAL_SENDING_IZJI_QO_GAUNTLET: [SpellId; 3] = [
    SpellId::PortalSendingIzjiQoGauntlet60,
    SpellId::PortalSendingIzjiQoGauntlet80,
    SpellId::PortalSendingIzjiQoGauntlet100,
];

/// ACE `SpellLevelProgression.PortalSendingIzjiQoReceivingChamber` (`List<SpellId>`).
pub static PORTAL_SENDING_IZJI_QO_RECEIVING_CHAMBER: [SpellId; 8] = [
    SpellId::PortalSendingIzjiQoReceivingChamber,
    SpellId::PortalSendingIzjiQoReceivingChamber1,
    SpellId::PortalSendingIzjiQoReceivingChamber2,
    SpellId::PortalSendingIzjiQoReceivingChamber3,
    SpellId::PortalSendingIzjiQoReceivingChamber4,
    SpellId::PortalSendingIzjiQoReceivingChamber5,
    SpellId::PortalSendingIzjiQoReceivingChamber6,
    SpellId::PortalSendingIzjiQoReceivingChamber7,
];

/// ACE `SpellLevelProgression.PortalSendingIzjiQoTest` (`List<SpellId>`).
pub static PORTAL_SENDING_IZJI_QO_TEST: [SpellId; 3] = [
    SpellId::PortalSendingIzjiQoTest60,
    SpellId::PortalSendingIzjiQoTest80,
    SpellId::PortalSendingIzjiQoTest100,
];

/// ACE `SpellLevelProgression.ArcanumSalvagingSelf` (`List<SpellId>`).
pub static ARCANUM_SALVAGING_SELF: [SpellId; 8] = [
    SpellId::ArcanumSalvagingSelf1,
    SpellId::ArcanumSalvagingSelf2,
    SpellId::ArcanumSalvagingSelf3,
    SpellId::ArcanumSalvagingSelf4,
    SpellId::ArcanumSalvagingSelf5,
    SpellId::ArcanumSalvagingSelf6,
    SpellId::ArcanumSalvagingSelf7,
    SpellId::ArcanumSalvagingSelf8,
];

/// ACE `SpellLevelProgression.ArcanumSalvagingOther` (`List<SpellId>`).
pub static ARCANUM_SALVAGING_OTHER: [SpellId; 8] = [
    SpellId::ArcanumSalvagingOther1,
    SpellId::ArcanumSalvagingOther2,
    SpellId::ArcanumSalvagingOther3,
    SpellId::ArcanumSalvagingOther4,
    SpellId::ArcanumSalvagingOther5,
    SpellId::ArcanumSalvagingOther6,
    SpellId::ArcanumSalvagingOther7,
    SpellId::ArcanumSalvagingOther8,
];

/// ACE `SpellLevelProgression.NuhmudirasWisdom` (`List<SpellId>`).
pub static NUHMUDIRAS_WISDOM: [SpellId; 8] = [
    SpellId::NuhmudirasWisdom1,
    SpellId::NuhmudirasWisdom2,
    SpellId::NuhmudirasWisdom3,
    SpellId::NuhmudirasWisdom4,
    SpellId::NuhmudirasWisdom5,
    SpellId::NuhmudirasWisdom6,
    SpellId::NuhmudirasWisdom7,
    SpellId::NuhmudirasWisdom8,
];

/// ACE `SpellLevelProgression.NuhmudirasWisdomOther` (`List<SpellId>`).
pub static NUHMUDIRAS_WISDOM_OTHER: [SpellId; 8] = [
    SpellId::NuhmudirasWisdomOther1,
    SpellId::NuhmudirasWisdomOther2,
    SpellId::NuhmudirasWisdomOther3,
    SpellId::NuhmudirasWisdomOther4,
    SpellId::NuhmudirasWisdomOther5,
    SpellId::NuhmudirasWisdomOther6,
    SpellId::NuhmudirasWisdomOther7,
    SpellId::NuhmudirasWisdomOther8,
];

/// ACE `SpellLevelProgression.Intoxication` (`List<SpellId>`).
pub static INTOXICATION: [SpellId; 3] = [
    SpellId::Intoxication1,
    SpellId::Intoxication2,
    SpellId::Intoxication3,
];

/// ACE `SpellLevelProgression.AxemansBoon` (`List<SpellId>`).
pub static AXEMANS_BOON: [SpellId; 2] = [
    SpellId::AxemansBoon,
    SpellId::AxemansBoon3,
];

/// ACE `SpellLevelProgression.BowmansBoon` (`List<SpellId>`).
pub static BOWMANS_BOON: [SpellId; 2] = [
    SpellId::BowmansBoon,
    SpellId::BowmansBoon3,
];

/// ACE `SpellLevelProgression.ChuckersBoon` (`List<SpellId>`).
pub static CHUCKERS_BOON: [SpellId; 2] = [
    SpellId::ChuckersBoon,
    SpellId::ChuckersBoon3,
];

/// ACE `SpellLevelProgression.CrossbowmansBoon` (`List<SpellId>`).
pub static CROSSBOWMANS_BOON: [SpellId; 2] = [
    SpellId::CrossbowmansBoon,
    SpellId::CrossbowmansBoon3,
];

/// ACE `SpellLevelProgression.EnchantersBoon` (`List<SpellId>`).
pub static ENCHANTERS_BOON: [SpellId; 2] = [
    SpellId::EnchantersBoon,
    SpellId::EnchantersBoon3,
];

/// ACE `SpellLevelProgression.HieromancersBoon` (`List<SpellId>`).
pub static HIEROMANCERS_BOON: [SpellId; 2] = [
    SpellId::HieromancersBoon,
    SpellId::HieromancersBoon3,
];

/// ACE `SpellLevelProgression.KnifersBoon` (`List<SpellId>`).
pub static KNIFERS_BOON: [SpellId; 2] = [
    SpellId::KnifersBoon,
    SpellId::KnifersBoon3,
];

/// ACE `SpellLevelProgression.LifeGiversBoon` (`List<SpellId>`).
pub static LIFE_GIVERS_BOON: [SpellId; 2] = [
    SpellId::LifeGiversBoon,
    SpellId::LifeGiversBoon3,
];

/// ACE `SpellLevelProgression.MacersBoon` (`List<SpellId>`).
pub static MACERS_BOON: [SpellId; 2] = [
    SpellId::MacersBoon,
    SpellId::MacersBoon3,
];

/// ACE `SpellLevelProgression.PugilistsBoon` (`List<SpellId>`).
pub static PUGILISTS_BOON: [SpellId; 2] = [
    SpellId::PugilistsBoon,
    SpellId::PugilistsBoon3,
];

/// ACE `SpellLevelProgression.SpearmansBoon` (`List<SpellId>`).
pub static SPEARMANS_BOON: [SpellId; 2] = [
    SpellId::SpearmansBoon,
    SpellId::SpearmansBoon3,
];

/// ACE `SpellLevelProgression.StafferBoon` (`List<SpellId>`).
pub static STAFFER_BOON: [SpellId; 2] = [
    SpellId::StafferBoon,
    SpellId::StafferBoon3,
];

/// ACE `SpellLevelProgression.SwordsmansBoon` (`List<SpellId>`).
pub static SWORDSMANS_BOON: [SpellId; 2] = [
    SpellId::SwordsmansBoon,
    SpellId::SwordsmansBoon3,
];

/// ACE `SpellLevelProgression.SalvagingMasteryForge` (`List<SpellId>`).
pub static SALVAGING_MASTERY_FORGE: [SpellId; 2] = [
    SpellId::SalvagingMasteryForge1,
    SpellId::SalvagingMasteryForge2,
];

/// ACE `SpellLevelProgression.AlchemyMasteryForge` (`List<SpellId>`).
pub static ALCHEMY_MASTERY_FORGE: [SpellId; 2] = [
    SpellId::AlchemyMasteryForge1,
    SpellId::AlchemyMasteryForge2,
];

/// ACE `SpellLevelProgression.CookingMasteryForge` (`List<SpellId>`).
pub static COOKING_MASTERY_FORGE: [SpellId; 2] = [
    SpellId::CookingMasteryForge1,
    SpellId::CookingMasteryForge2,
];

/// ACE `SpellLevelProgression.FletchingMasteryForge` (`List<SpellId>`).
pub static FLETCHING_MASTERY_FORGE: [SpellId; 2] = [
    SpellId::FletchingMasteryForge1,
    SpellId::FletchingMasteryForge2,
];

/// ACE `SpellLevelProgression.LockpickMasteryForge` (`List<SpellId>`).
pub static LOCKPICK_MASTERY_FORGE: [SpellId; 2] = [
    SpellId::LockpickMasteryForge1,
    SpellId::LockpickMasteryForge2,
];

/// ACE `SpellLevelProgression.PortalSendingPvPHate20Entry` (`List<SpellId>`).
pub static PORTAL_SENDING_PV_P_HATE20_ENTRY: [SpellId; 6] = [
    SpellId::PortalSendingPvPHate20Entry1,
    SpellId::PortalSendingPvPHate20Entry2,
    SpellId::PortalSendingPvPHate20Entry3,
    SpellId::PortalSendingPvPHate20Entry4,
    SpellId::PortalSendingPvPHate20Entry5,
    SpellId::PortalSendingPvPHate20Entry6,
];

/// ACE `SpellLevelProgression.PortalSendingPvPHate40Entry` (`List<SpellId>`).
pub static PORTAL_SENDING_PV_P_HATE40_ENTRY: [SpellId; 6] = [
    SpellId::PortalSendingPvPHate40Entry1,
    SpellId::PortalSendingPvPHate40Entry2,
    SpellId::PortalSendingPvPHate40Entry3,
    SpellId::PortalSendingPvPHate40Entry4,
    SpellId::PortalSendingPvPHate40Entry5,
    SpellId::PortalSendingPvPHate40Entry6,
];

/// ACE `SpellLevelProgression.PortalSendingPvPHate60Entry` (`List<SpellId>`).
pub static PORTAL_SENDING_PV_P_HATE60_ENTRY: [SpellId; 6] = [
    SpellId::PortalSendingPvPHate60Entry1,
    SpellId::PortalSendingPvPHate60Entry2,
    SpellId::PortalSendingPvPHate60Entry3,
    SpellId::PortalSendingPvPHate60Entry4,
    SpellId::PortalSendingPvPHate60Entry5,
    SpellId::PortalSendingPvPHate60Entry6,
];

/// ACE `SpellLevelProgression.PortalSendingPvPHate80AccursedEntry` (`List<SpellId>`).
pub static PORTAL_SENDING_PV_P_HATE80_ACCURSED_ENTRY: [SpellId; 6] = [
    SpellId::PortalSendingPvPHate80AccursedEntry1,
    SpellId::PortalSendingPvPHate80AccursedEntry2,
    SpellId::PortalSendingPvPHate80AccursedEntry3,
    SpellId::PortalSendingPvPHate80AccursedEntry4,
    SpellId::PortalSendingPvPHate80AccursedEntry5,
    SpellId::PortalSendingPvPHate80AccursedEntry6,
];

/// ACE `SpellLevelProgression.PortalSendingPvPHate80UnholyEntry` (`List<SpellId>`).
pub static PORTAL_SENDING_PV_P_HATE80_UNHOLY_ENTRY: [SpellId; 6] = [
    SpellId::PortalSendingPvPHate80UnholyEntry1,
    SpellId::PortalSendingPvPHate80UnholyEntry2,
    SpellId::PortalSendingPvPHate80UnholyEntry3,
    SpellId::PortalSendingPvPHate80UnholyEntry4,
    SpellId::PortalSendingPvPHate80UnholyEntry5,
    SpellId::PortalSendingPvPHate80UnholyEntry6,
];

/// ACE `SpellLevelProgression.CantripSalvaging` (`List<SpellId>`).
pub static CANTRIP_SALVAGING: [SpellId; 4] = [
    SpellId::CantripSalvaging1,
    SpellId::CantripSalvaging2,
    SpellId::CANTRIPSALVAGING3,
    SpellId::CantripSalvaging4,
];

/// ACE `SpellLevelProgression.RecallSonPooky` (`List<SpellId>`).
pub static RECALL_SON_POOKY: [SpellId; 3] = [
    SpellId::RecallSonPooky1,
    SpellId::RecallSonPooky2,
    SpellId::RecallSonPooky3,
];

/// ACE `SpellLevelProgression.PortalSendingColosseumA` (`List<SpellId>`).
pub static PORTAL_SENDING_COLOSSEUM_A: [SpellId; 2] = [
    SpellId::PortalSendingColosseumA1,
    SpellId::PortalSendingColosseumA6,
];

/// ACE `SpellLevelProgression.PortalSendingColosseumB` (`List<SpellId>`).
pub static PORTAL_SENDING_COLOSSEUM_B: [SpellId; 2] = [
    SpellId::PortalSendingColosseumB1,
    SpellId::PortalSendingColosseumB6,
];

/// ACE `SpellLevelProgression.PortalSendingColosseumC` (`List<SpellId>`).
pub static PORTAL_SENDING_COLOSSEUM_C: [SpellId; 2] = [
    SpellId::PortalSendingColosseumC1,
    SpellId::PortalSendingColosseumC6,
];

/// ACE `SpellLevelProgression.PortalSendingColosseumD` (`List<SpellId>`).
pub static PORTAL_SENDING_COLOSSEUM_D: [SpellId; 2] = [
    SpellId::PortalSendingColosseumD1,
    SpellId::PortalSendingColosseumD6,
];

/// ACE `SpellLevelProgression.PortalSendingColosseumE` (`List<SpellId>`).
pub static PORTAL_SENDING_COLOSSEUM_E: [SpellId; 2] = [
    SpellId::PortalSendingColosseumE1,
    SpellId::PortalSendingColosseumE6,
];

/// ACE `SpellLevelProgression.PortalSendingVisionQuestBranch4Stage` (`List<SpellId>`).
pub static PORTAL_SENDING_VISION_QUEST_BRANCH4_STAGE: [SpellId; 5] = [
    SpellId::PortalSendingVisionQuestBranch4Stage1,
    SpellId::PortalSendingVisionQuestBranch4Stage2,
    SpellId::PortalSendingVisionQuestBranch4Stage3,
    SpellId::PortalSendingVisionQuestBranch4Stage4,
    SpellId::PortalSendingVisionQuestBranch4Stage6,
];

/// ACE `SpellLevelProgression.PortalSendingVisionQuestBranch5Stage` (`List<SpellId>`).
pub static PORTAL_SENDING_VISION_QUEST_BRANCH5_STAGE: [SpellId; 5] = [
    SpellId::PortalSendingVisionQuestBranch5Stage1,
    SpellId::PortalSendingVisionQuestBranch5Stage2,
    SpellId::PortalSendingVisionQuestBranch5Stage3,
    SpellId::PortalSendingVisionQuestBranch5Stage4,
    SpellId::PortalSendingVisionQuestBranch5Stage6,
];

/// ACE `SpellLevelProgression.PortalSendingVisionQuestBranch1Stage` (`List<SpellId>`).
pub static PORTAL_SENDING_VISION_QUEST_BRANCH1_STAGE: [SpellId; 5] = [
    SpellId::PortalSendingVisionQuestBranch1Stage1,
    SpellId::PortalSendingVisionQuestBranch1Stage2,
    SpellId::PortalSendingVisionQuestBranch1Stage3,
    SpellId::PortalSendingVisionQuestBranch1Stage4,
    SpellId::PortalSendingVisionQuestBranch1Stage6,
];

/// ACE `SpellLevelProgression.PortalSendingVisionQuestBranch2Stage` (`List<SpellId>`).
pub static PORTAL_SENDING_VISION_QUEST_BRANCH2_STAGE: [SpellId; 5] = [
    SpellId::PortalSendingVisionQuestBranch2Stage1,
    SpellId::PortalSendingVisionQuestBranch2Stage2,
    SpellId::PortalSendingVisionQuestBranch2Stage3,
    SpellId::PortalSendingVisionQuestBranch2Stage4,
    SpellId::PortalSendingVisionQuestBranch2Stage6,
];

/// ACE `SpellLevelProgression.PortalSendingVisionQuestBranch3Stage` (`List<SpellId>`).
pub static PORTAL_SENDING_VISION_QUEST_BRANCH3_STAGE: [SpellId; 5] = [
    SpellId::PortalSendingVisionQuestBranch3Stage1,
    SpellId::PortalSendingVisionQuestBranch3Stage2,
    SpellId::PortalSendingVisionQuestBranch3Stage3,
    SpellId::PortalSendingVisionQuestBranch3Stage4,
    SpellId::PortalSendingVisionQuestBranch3Stage6,
];

/// ACE `SpellLevelProgression.PortalSendDarkCrypt` (`List<SpellId>`).
pub static PORTAL_SEND_DARK_CRYPT: [SpellId; 3] = [
    SpellId::PortalSendDarkCrypt1,
    SpellId::PortalSendDarkCrypt2,
    SpellId::PortalSendDarkCrypt3,
];

/// ACE `SpellLevelProgression.PortalSendJesterPrison` (`List<SpellId>`).
pub static PORTAL_SEND_JESTER_PRISON: [SpellId; 2] = [
    SpellId::PortalSendJesterPrison,
    SpellId::PortalSendJesterPrison2,
];

/// ACE `SpellLevelProgression.RecallJester` (`List<SpellId>`).
pub static RECALL_JESTER: [SpellId; 8] = [
    SpellId::RecallJester1,
    SpellId::RecallJester2,
    SpellId::RecallJester3,
    SpellId::RecallJester4,
    SpellId::RecallJester5,
    SpellId::RecallJester6,
    SpellId::RecallJester7,
    SpellId::RecallJester8,
];

/// ACE `SpellLevelProgression.PortalSendingCHDSStage` (`List<SpellId>`).
pub static PORTAL_SENDING_CHDS_STAGE: [SpellId; 3] = [
    SpellId::PortalSendingCHDSStage1,
    SpellId::PortalSendingCHDSStage2,
    SpellId::PortalSendingCHDSStage3,
];

/// ACE `SpellLevelProgression.MoarsmanPoison` (`List<SpellId>`).
pub static MOARSMAN_POISON: [SpellId; 3] = [
    SpellId::MoarsmanPoison1,
    SpellId::MoarsmanPoison2,
    SpellId::MoarsmanPoison3,
];

/// ACE `SpellLevelProgression.SetCoordination` (`List<SpellId>`).
pub static SET_COORDINATION: [SpellId; 4] = [
    SpellId::SetCoordination1,
    SpellId::SetCoordination2,
    SpellId::SetCoordination3,
    SpellId::SetCoordination4,
];

/// ACE `SpellLevelProgression.SetEndurance` (`List<SpellId>`).
pub static SET_ENDURANCE: [SpellId; 4] = [
    SpellId::SetEndurance1,
    SpellId::SetEndurance2,
    SpellId::SetEndurance3,
    SpellId::SetEndurance4,
];

/// ACE `SpellLevelProgression.SetFocus` (`List<SpellId>`).
pub static SET_FOCUS: [SpellId; 4] = [
    SpellId::SetFocus1,
    SpellId::SetFocus2,
    SpellId::SetFocus3,
    SpellId::SetFocus4,
];

/// ACE `SpellLevelProgression.SetQuickness` (`List<SpellId>`).
pub static SET_QUICKNESS: [SpellId; 4] = [
    SpellId::SetQuickness1,
    SpellId::SetQuickness2,
    SpellId::SetQuickness3,
    SpellId::SetQuickness4,
];

/// ACE `SpellLevelProgression.SetStrength` (`List<SpellId>`).
pub static SET_STRENGTH: [SpellId; 4] = [
    SpellId::SetStrength1,
    SpellId::SetStrength2,
    SpellId::SetStrength3,
    SpellId::SetStrength4,
];

/// ACE `SpellLevelProgression.SetWillpower` (`List<SpellId>`).
pub static SET_WILLPOWER: [SpellId; 4] = [
    SpellId::SetWillpower1,
    SpellId::SetWillpower2,
    SpellId::SetWillpower3,
    SpellId::SetWillpower4,
];

/// ACE `SpellLevelProgression.SetHealth` (`List<SpellId>`).
pub static SET_HEALTH: [SpellId; 2] = [
    SpellId::SetHealth2,
    SpellId::SetHealth3,
];

/// ACE `SpellLevelProgression.SetMana` (`List<SpellId>`).
pub static SET_MANA: [SpellId; 2] = [
    SpellId::SetMana2,
    SpellId::SetMana3,
];

/// ACE `SpellLevelProgression.SetStamina` (`List<SpellId>`).
pub static SET_STAMINA: [SpellId; 2] = [
    SpellId::SetStamina2,
    SpellId::SetStamina3,
];

/// ACE `SpellLevelProgression.SetAcidResistance` (`List<SpellId>`).
pub static SET_ACID_RESISTANCE: [SpellId; 4] = [
    SpellId::SetAcidResistance1,
    SpellId::SetAcidResistance2,
    SpellId::SetAcidResistance3,
    SpellId::SetAcidResistance4,
];

/// ACE `SpellLevelProgression.SetBludgeonResistance` (`List<SpellId>`).
pub static SET_BLUDGEON_RESISTANCE: [SpellId; 4] = [
    SpellId::SetBludgeonResistance1,
    SpellId::SetBludgeonResistance2,
    SpellId::SetBludgeonResistance3,
    SpellId::SetBludgeonResistance4,
];

/// ACE `SpellLevelProgression.SetFlameResistance` (`List<SpellId>`).
pub static SET_FLAME_RESISTANCE: [SpellId; 4] = [
    SpellId::SetFlameResistance1,
    SpellId::SetFlameResistance2,
    SpellId::SetFlameResistance3,
    SpellId::SetFlameResistance4,
];

/// ACE `SpellLevelProgression.SetFrostResistance` (`List<SpellId>`).
pub static SET_FROST_RESISTANCE: [SpellId; 4] = [
    SpellId::SetFrostResistance1,
    SpellId::SetFrostResistance2,
    SpellId::SetFrostResistance3,
    SpellId::SetFrostResistance4,
];

/// ACE `SpellLevelProgression.SetLightningResistance` (`List<SpellId>`).
pub static SET_LIGHTNING_RESISTANCE: [SpellId; 4] = [
    SpellId::SetLightningResistance1,
    SpellId::SetLightningResistance2,
    SpellId::SetLightningResistance3,
    SpellId::SetLightningResistance4,
];

/// ACE `SpellLevelProgression.SetPierceResistance` (`List<SpellId>`).
pub static SET_PIERCE_RESISTANCE: [SpellId; 4] = [
    SpellId::SetPierceResistance1,
    SpellId::SetPierceResistance2,
    SpellId::SetPierceResistance3,
    SpellId::SetPierceResistance4,
];

/// ACE `SpellLevelProgression.SetSlashingResistance` (`List<SpellId>`).
pub static SET_SLASHING_RESISTANCE: [SpellId; 4] = [
    SpellId::SetSlashingResistance1,
    SpellId::SetSlashingResistance2,
    SpellId::SetSlashingResistance3,
    SpellId::SetSlashingResistance4,
];

/// ACE `SpellLevelProgression.SetAlchemyAptitude` (`List<SpellId>`).
pub static SET_ALCHEMY_APTITUDE: [SpellId; 4] = [
    SpellId::SetAlchemyAptitude1,
    SpellId::SetAlchemyAptitude2,
    SpellId::SetAlchemyAptitude3,
    SpellId::SetAlchemyAptitude4,
];

/// ACE `SpellLevelProgression.SetArmorExpertiseAptitude` (`List<SpellId>`).
pub static SET_ARMOR_EXPERTISE_APTITUDE: [SpellId; 4] = [
    SpellId::SetArmorExpertiseAptitude1,
    SpellId::SetArmorExpertiseAptitude2,
    SpellId::SetArmorExpertiseAptitude3,
    SpellId::SetArmorExpertiseAptitude4,
];

/// ACE `SpellLevelProgression.SetAxeAptitude` (`List<SpellId>`).
pub static SET_AXE_APTITUDE: [SpellId; 4] = [
    SpellId::SetAxeAptitude1,
    SpellId::SetAxeAptitude2,
    SpellId::SetAxeAptitude3,
    SpellId::SetAxeAptitude4,
];

/// ACE `SpellLevelProgression.SetBowAptitude` (`List<SpellId>`).
pub static SET_BOW_APTITUDE: [SpellId; 4] = [
    SpellId::SetBowAptitude1,
    SpellId::SetBowAptitude2,
    SpellId::SetBowAptitude3,
    SpellId::SetBowAptitude4,
];

/// ACE `SpellLevelProgression.SetCookingAptitude` (`List<SpellId>`).
pub static SET_COOKING_APTITUDE: [SpellId; 4] = [
    SpellId::SetCookingAptitude1,
    SpellId::SetCookingAptitude2,
    SpellId::SetCookingAptitude3,
    SpellId::SetCookingAptitude4,
];

/// ACE `SpellLevelProgression.SetCreatureEnchantmentAptitude` (`List<SpellId>`).
pub static SET_CREATURE_ENCHANTMENT_APTITUDE: [SpellId; 4] = [
    SpellId::SetCreatureEnchantmentAptitude1,
    SpellId::SetCreatureEnchantmentAptitude2,
    SpellId::SetCreatureEnchantmentAptitude3,
    SpellId::SetCreatureEnchantmentAptitude4,
];

/// ACE `SpellLevelProgression.SetCrossbowAptitude` (`List<SpellId>`).
pub static SET_CROSSBOW_APTITUDE: [SpellId; 4] = [
    SpellId::SetCrossbowAptitude1,
    SpellId::SetCrossbowAptitude2,
    SpellId::SetCrossbowAptitude3,
    SpellId::SetCrossbowAptitude4,
];

/// ACE `SpellLevelProgression.SetDaggerAptitude` (`List<SpellId>`).
pub static SET_DAGGER_APTITUDE: [SpellId; 4] = [
    SpellId::SetDaggerAptitude1,
    SpellId::SetDaggerAptitude2,
    SpellId::SetDaggerAptitude3,
    SpellId::SetDaggerAptitude4,
];

/// ACE `SpellLevelProgression.SetFletchingAptitude` (`List<SpellId>`).
pub static SET_FLETCHING_APTITUDE: [SpellId; 4] = [
    SpellId::SetFletchingAptitude1,
    SpellId::SetFletchingAptitude2,
    SpellId::SetFletchingAptitude3,
    SpellId::SetFletchingAptitude4,
];

/// ACE `SpellLevelProgression.SetItemEnchantmentAptitude` (`List<SpellId>`).
pub static SET_ITEM_ENCHANTMENT_APTITUDE: [SpellId; 4] = [
    SpellId::SetItemEnchantmentAptitude1,
    SpellId::SetItemEnchantmentAptitude2,
    SpellId::SetItemEnchantmentAptitude3,
    SpellId::SetItemEnchantmentAptitude4,
];

/// ACE `SpellLevelProgression.SetItemExpertiseAptitude` (`List<SpellId>`).
pub static SET_ITEM_EXPERTISE_APTITUDE: [SpellId; 4] = [
    SpellId::SetItemExpertiseAptitude1,
    SpellId::SetItemExpertiseAptitude2,
    SpellId::SetItemExpertiseAptitude3,
    SpellId::SetItemExpertiseAptitude4,
];

/// ACE `SpellLevelProgression.SetJumpingAptitude` (`List<SpellId>`).
pub static SET_JUMPING_APTITUDE: [SpellId; 4] = [
    SpellId::SetJumpingAptitude1,
    SpellId::SetJumpingAptitude2,
    SpellId::SetJumpingAptitude3,
    SpellId::SetJumpingAptitude4,
];

/// ACE `SpellLevelProgression.SetLifeMagicAptitude` (`List<SpellId>`).
pub static SET_LIFE_MAGIC_APTITUDE: [SpellId; 4] = [
    SpellId::SetLifeMagicAptitude1,
    SpellId::SetLifeMagicAptitude2,
    SpellId::SetLifeMagicAptitude3,
    SpellId::SetLifeMagicAptitude4,
];

/// ACE `SpellLevelProgression.SetLockpickAptitude` (`List<SpellId>`).
pub static SET_LOCKPICK_APTITUDE: [SpellId; 4] = [
    SpellId::SetLockpickAptitude1,
    SpellId::SetLockpickAptitude2,
    SpellId::SetLockpickAptitude3,
    SpellId::SetLockpickAptitude4,
];

/// ACE `SpellLevelProgression.SetLoyaltyAptitude` (`List<SpellId>`).
pub static SET_LOYALTY_APTITUDE: [SpellId; 2] = [
    SpellId::SetLoyaltyAptitude1,
    SpellId::SetLoyaltyAptitude2,
];

/// ACE `SpellLevelProgression.SetMaceAptitude` (`List<SpellId>`).
pub static SET_MACE_APTITUDE: [SpellId; 4] = [
    SpellId::SetMaceAptitude1,
    SpellId::SetMaceAptitude2,
    SpellId::SetMaceAptitude3,
    SpellId::SetMaceAptitude4,
];

/// ACE `SpellLevelProgression.SetMagicDefenseAptitude` (`List<SpellId>`).
pub static SET_MAGIC_DEFENSE_APTITUDE: [SpellId; 4] = [
    SpellId::SetMagicDefenseAptitude1,
    SpellId::SetMagicDefenseAptitude2,
    SpellId::SetMagicDefenseAptitude3,
    SpellId::SetMagicDefenseAptitude4,
];

/// ACE `SpellLevelProgression.SetMagicItemExpertiseAptitude` (`List<SpellId>`).
pub static SET_MAGIC_ITEM_EXPERTISE_APTITUDE: [SpellId; 4] = [
    SpellId::SetMagicItemExpertiseAptitude1,
    SpellId::SetMagicItemExpertiseAptitude2,
    SpellId::SetMagicItemExpertiseAptitude3,
    SpellId::SetMagicItemExpertiseAptitude4,
];

/// ACE `SpellLevelProgression.SetMeleeDefenseAptitude` (`List<SpellId>`).
pub static SET_MELEE_DEFENSE_APTITUDE: [SpellId; 4] = [
    SpellId::SetMeleeDefenseAptitude1,
    SpellId::SetMeleeDefenseAptitude2,
    SpellId::SetMeleeDefenseAptitude3,
    SpellId::SetMeleeDefenseAptitude4,
];

/// ACE `SpellLevelProgression.SetMissileDefenseAptitude` (`List<SpellId>`).
pub static SET_MISSILE_DEFENSE_APTITUDE: [SpellId; 4] = [
    SpellId::SetMissileDefenseAptitude1,
    SpellId::SetMissileDefenseAptitude2,
    SpellId::SetMissileDefenseAptitude3,
    SpellId::SetMissileDefenseAptitude4,
];

/// ACE `SpellLevelProgression.SetSalvagingAptitude` (`List<SpellId>`).
pub static SET_SALVAGING_APTITUDE: [SpellId; 2] = [
    SpellId::SetSalvagingAptitude1,
    SpellId::SetSalvagingAptitude2,
];

/// ACE `SpellLevelProgression.SetSpearAptitude` (`List<SpellId>`).
pub static SET_SPEAR_APTITUDE: [SpellId; 4] = [
    SpellId::SetSpearAptitude1,
    SpellId::SetSpearAptitude2,
    SpellId::SetSpearAptitude3,
    SpellId::SetSpearAptitude4,
];

/// ACE `SpellLevelProgression.SetSprintAptitude` (`List<SpellId>`).
pub static SET_SPRINT_APTITUDE: [SpellId; 4] = [
    SpellId::SetSprintAptitude1,
    SpellId::SetSprintAptitude2,
    SpellId::SetSprintAptitude3,
    SpellId::SetSprintAptitude4,
];

/// ACE `SpellLevelProgression.SetStaffAptitude` (`List<SpellId>`).
pub static SET_STAFF_APTITUDE: [SpellId; 4] = [
    SpellId::SetStaffAptitude1,
    SpellId::SetStaffAptitude2,
    SpellId::SetStaffAptitude3,
    SpellId::SetStaffAptitude4,
];

/// ACE `SpellLevelProgression.SetSwordAptitude` (`List<SpellId>`).
pub static SET_SWORD_APTITUDE: [SpellId; 4] = [
    SpellId::SetSwordAptitude1,
    SpellId::SetSwordAptitude2,
    SpellId::SetSwordAptitude3,
    SpellId::SetSwordAptitude4,
];

/// ACE `SpellLevelProgression.SetThrownAptitude` (`List<SpellId>`).
pub static SET_THROWN_APTITUDE: [SpellId; 4] = [
    SpellId::SetThrownAptitude1,
    SpellId::SetThrownAptitude2,
    SpellId::SetThrownAptitude3,
    SpellId::SetThrownAptitude4,
];

/// ACE `SpellLevelProgression.SetUnarmedAptitude` (`List<SpellId>`).
pub static SET_UNARMED_APTITUDE: [SpellId; 4] = [
    SpellId::SetUnarmedAptitude1,
    SpellId::SetUnarmedAptitude2,
    SpellId::SetUnarmedAptitude3,
    SpellId::SetUnarmedAptitude4,
];

/// ACE `SpellLevelProgression.SetWarMagicAptitude` (`List<SpellId>`).
pub static SET_WAR_MAGIC_APTITUDE: [SpellId; 4] = [
    SpellId::SetWarMagicAptitude1,
    SpellId::SetWarMagicAptitude2,
    SpellId::SetWarMagicAptitude3,
    SpellId::SetWarMagicAptitude4,
];

/// ACE `SpellLevelProgression.SetWeaponExpertiseAptitude` (`List<SpellId>`).
pub static SET_WEAPON_EXPERTISE_APTITUDE: [SpellId; 4] = [
    SpellId::SetWeaponExpertiseAptitude1,
    SpellId::SetWeaponExpertiseAptitude2,
    SpellId::SetWeaponExpertiseAptitude3,
    SpellId::SetWeaponExpertiseAptitude4,
];

/// ACE `SpellLevelProgression.SetSocietyAttributeAll` (`List<SpellId>`).
pub static SET_SOCIETY_ATTRIBUTE_ALL: [SpellId; 5] = [
    SpellId::SetSocietyAttributeAll1,
    SpellId::SetSocietyAttributeAll2,
    SpellId::SetSocietyAttributeAll3,
    SpellId::SetSocietyAttributeAll4,
    SpellId::SetSocietyAttributeAll5,
];

/// ACE `SpellLevelProgression.SetRejuvenation` (`List<SpellId>`).
pub static SET_REJUVENATION: [SpellId; 2] = [
    SpellId::SetRejuvenation1,
    SpellId::SetRejuvenation2,
];

/// ACE `SpellLevelProgression.AcidStream8Spellpower` (`List<SpellId>`).
pub static ACID_STREAM8_SPELLPOWER: [SpellId; 2] = [
    SpellId::AcidStream8Spellpower300,
    SpellId::AcidStream8Spellpower350,
];

/// ACE `SpellLevelProgression.MiniArcaneDeath` (`List<SpellId>`).
pub static MINI_ARCANE_DEATH: [SpellId; 4] = [
    SpellId::MiniArcaneDeath,
    SpellId::MiniArcaneDeath2,
    SpellId::MiniArcaneDeath3,
    SpellId::MiniArcaneDeath4,
];

/// ACE `SpellLevelProgression.MiniFireball` (`List<SpellId>`).
pub static MINI_FIREBALL: [SpellId; 4] = [
    SpellId::MiniFireball1,
    SpellId::MiniFireball2,
    SpellId::MiniFireball3,
    SpellId::MiniFireball4,
];

/// ACE `SpellLevelProgression.MiniIceball` (`List<SpellId>`).
pub static MINI_ICEBALL: [SpellId; 4] = [
    SpellId::MiniIceball1,
    SpellId::MiniIceball2,
    SpellId::MiniIceball3,
    SpellId::MiniIceball4,
];

/// ACE `SpellLevelProgression.MiniArrow` (`List<SpellId>`).
pub static MINI_ARROW: [SpellId; 4] = [
    SpellId::MiniArrow1,
    SpellId::MiniArrow2,
    SpellId::MiniArrow3,
    SpellId::MiniArrow4,
];

/// ACE `SpellLevelProgression.MiniRing` (`List<SpellId>`).
pub static MINI_RING: [SpellId; 4] = [
    SpellId::MiniRing1,
    SpellId::MiniRing2,
    SpellId::MiniRing3,
    SpellId::MiniRing4,
];

/// ACE `SpellLevelProgression.PortalSendingAssassinsRoost` (`List<SpellId>`).
pub static PORTAL_SENDING_ASSASSINS_ROOST: [SpellId; 5] = [
    SpellId::PortalSendingAssassinsRoost1,
    SpellId::PortalSendingAssassinsRoost2,
    SpellId::PortalSendingAssassinsRoost3,
    SpellId::PortalSendingAssassinsRoost4,
    SpellId::PortalSendingAssassinsRoost5,
];

/// ACE `SpellLevelProgression.TwoHandedBoon` (`List<SpellId>`).
pub static TWO_HANDED_BOON: [SpellId; 2] = [
    SpellId::TwoHandedBoon,
    SpellId::TwoHandedBoon3,
];

/// ACE `SpellLevelProgression.TwoHandedMasterySelf` (`List<SpellId>`).
pub static TWO_HANDED_MASTERY_SELF: [SpellId; 8] = [
    SpellId::TwoHandedMasterySelf1,
    SpellId::TwoHandedMasterySelf2,
    SpellId::TwoHandedMasterySelf3,
    SpellId::TwoHandedMasterySelf4,
    SpellId::TwoHandedMasterySelf5,
    SpellId::TwoHandedMasterySelf6,
    SpellId::TwoHandedMasterySelf7,
    SpellId::TwoHandedMasterySelf8,
];

/// ACE `SpellLevelProgression.CANTRIPGEARCRAFTAPTITUDE` (`List<SpellId>`).
pub static CANTRIPGEARCRAFTAPTITUDE: [SpellId; 3] = [
    SpellId::CANTRIPGEARCRAFTAPTITUDE1,
    SpellId::CANTRIPGEARCRAFTAPTITUDE2,
    SpellId::CANTRIPGEARCRAFTAPTITUDE3,
];

/// ACE `SpellLevelProgression.CANTRIPTWOHANDEDAPTITUDE` (`List<SpellId>`).
pub static CANTRIPTWOHANDEDAPTITUDE: [SpellId; 4] = [
    SpellId::CANTRIPTWOHANDEDAPTITUDE1,
    SpellId::CANTRIPTWOHANDEDAPTITUDE2,
    SpellId::CANTRIPTWOHANDEDAPTITUDE3,
    SpellId::CantripTwoHandedAptitude4,
];

/// ACE `SpellLevelProgression.GearcraftIneptitude` (`List<SpellId>`).
pub static GEARCRAFT_INEPTITUDE: [SpellId; 8] = [
    SpellId::GearcraftIneptitude1,
    SpellId::GearcraftIneptitude2,
    SpellId::GearcraftIneptitude3,
    SpellId::GearcraftIneptitude4,
    SpellId::GearcraftIneptitude5,
    SpellId::GearcraftIneptitude6,
    SpellId::GearcraftIneptitude7,
    SpellId::GearcraftIneptitude8,
];

/// ACE `SpellLevelProgression.GearcraftIneptitudeSelf` (`List<SpellId>`).
pub static GEARCRAFT_INEPTITUDE_SELF: [SpellId; 8] = [
    SpellId::GearcraftIneptitudeSelf1,
    SpellId::GearcraftIneptitudeSelf2,
    SpellId::GearcraftIneptitudeSelf3,
    SpellId::GearcraftIneptitudeSelf4,
    SpellId::GearcraftIneptitudeSelf5,
    SpellId::GearcraftIneptitudeSelf6,
    SpellId::GearcraftIneptitudeSelf7,
    SpellId::GearcraftIneptitudeSelf8,
];

/// ACE `SpellLevelProgression.GearcraftMastery` (`List<SpellId>`).
pub static GEARCRAFT_MASTERY: [SpellId; 8] = [
    SpellId::GearcraftMastery1,
    SpellId::GearcraftMastery2,
    SpellId::GearcraftMastery3,
    SpellId::GearcraftMastery4,
    SpellId::GearcraftMastery5,
    SpellId::GearcraftMastery6,
    SpellId::GearcraftMastery7,
    SpellId::GearcraftMastery8,
];

/// ACE `SpellLevelProgression.GearcraftMasterySelf` (`List<SpellId>`).
pub static GEARCRAFT_MASTERY_SELF: [SpellId; 8] = [
    SpellId::GearcraftMasterySelf1,
    SpellId::GearcraftMasterySelf2,
    SpellId::GearcraftMasterySelf3,
    SpellId::GearcraftMasterySelf4,
    SpellId::GearcraftMasterySelf5,
    SpellId::GearcraftMasterySelf6,
    SpellId::GearcraftMasterySelf7,
    SpellId::GearcraftMasterySelf8,
];

/// ACE `SpellLevelProgression.TwoHandedIneptitude` (`List<SpellId>`).
pub static TWO_HANDED_INEPTITUDE: [SpellId; 8] = [
    SpellId::TwoHandedIneptitude1,
    SpellId::TwoHandedIneptitude2,
    SpellId::TwoHandedIneptitude3,
    SpellId::TwoHandedIneptitude4,
    SpellId::TwoHandedIneptitude5,
    SpellId::TwoHandedIneptitude6,
    SpellId::TwoHandedIneptitude7,
    SpellId::TwoHandedIneptitude8,
];

/// ACE `SpellLevelProgression.TwoHandedIneptitudeSelf` (`List<SpellId>`).
pub static TWO_HANDED_INEPTITUDE_SELF: [SpellId; 8] = [
    SpellId::TwoHandedIneptitudeSelf1,
    SpellId::TwoHandedIneptitudeSelf2,
    SpellId::TwoHandedIneptitudeSelf3,
    SpellId::TwoHandedIneptitudeSelf4,
    SpellId::TwoHandedIneptitudeSelf5,
    SpellId::TwoHandedIneptitudeSelf6,
    SpellId::TwoHandedIneptitudeSelf7,
    SpellId::TwoHandedIneptitudeSelf8,
];

/// ACE `SpellLevelProgression.TwoHandedMasteryOther` (`List<SpellId>`).
pub static TWO_HANDED_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::TwoHandedMasteryOther1,
    SpellId::TwoHandedMasteryOther2,
    SpellId::TwoHandedMasteryOther3,
    SpellId::TwoHandedMasteryOther4,
    SpellId::TwoHandedMasteryOther5,
    SpellId::TwoHandedMasteryOther6,
    SpellId::TwoHandedMasteryOther7,
    SpellId::TwoHandedMasteryOther8,
];

/// ACE `SpellLevelProgression.SetGearCraftAptitude` (`List<SpellId>`).
pub static SET_GEAR_CRAFT_APTITUDE: [SpellId; 4] = [
    SpellId::SetGearCraftAptitude1,
    SpellId::SetGearCraftAptitude2,
    SpellId::SetGearCraftAptitude3,
    SpellId::SetGearCraftAptitude4,
];

/// ACE `SpellLevelProgression.SetTwoHandedAptitude` (`List<SpellId>`).
pub static SET_TWO_HANDED_APTITUDE: [SpellId; 4] = [
    SpellId::SetTwoHandedAptitude1,
    SpellId::SetTwoHandedAptitude2,
    SpellId::SetTwoHandedAptitude3,
    SpellId::SetTwoHandedAptitude4,
];

/// ACE `SpellLevelProgression.ExposeWeakness` (`List<SpellId>`).
pub static EXPOSE_WEAKNESS: [SpellId; 8] = [
    SpellId::ExposeWeakness1,
    SpellId::ExposeWeakness2,
    SpellId::ExposeWeakness3,
    SpellId::ExposeWeakness4,
    SpellId::ExposeWeakness5,
    SpellId::ExposeWeakness6,
    SpellId::ExposeWeakness7,
    SpellId::ExposeWeakness8,
];

/// ACE `SpellLevelProgression.CallOfLeadership` (`List<SpellId>`).
pub static CALL_OF_LEADERSHIP: [SpellId; 5] = [
    SpellId::CallOfLeadership1,
    SpellId::CallOfLeadership2,
    SpellId::CallOfLeadership3,
    SpellId::CallOfLeadership4,
    SpellId::CallOfLeadership5,
];

/// ACE `SpellLevelProgression.AnswerOfLoyaltyMana` (`List<SpellId>`).
pub static ANSWER_OF_LOYALTY_MANA: [SpellId; 5] = [
    SpellId::AnswerOfLoyaltyMana1,
    SpellId::AnswerOfLoyaltyMana2,
    SpellId::AnswerOfLoyaltyMana3,
    SpellId::AnswerOfLoyaltyMana4,
    SpellId::AnswerOfLoyaltyMana5,
];

/// ACE `SpellLevelProgression.AnswerOfLoyaltyStam` (`List<SpellId>`).
pub static ANSWER_OF_LOYALTY_STAM: [SpellId; 5] = [
    SpellId::AnswerOfLoyaltyStam1,
    SpellId::AnswerOfLoyaltyStam2,
    SpellId::AnswerOfLoyaltyStam3,
    SpellId::AnswerOfLoyaltyStam4,
    SpellId::AnswerOfLoyaltyStam5,
];

/// ACE `SpellLevelProgression.TrinketXPBoost` (`List<SpellId>`).
pub static TRINKET_XP_BOOST: [SpellId; 3] = [
    SpellId::TrinketXPBoost1,
    SpellId::TrinketXPBoost2,
    SpellId::TrinketXPBoost3,
];

/// ACE `SpellLevelProgression.TrinketDamageBoost` (`List<SpellId>`).
pub static TRINKET_DAMAGE_BOOST: [SpellId; 3] = [
    SpellId::TrinketDamageBoost1,
    SpellId::TrinketDamageBoost2,
    SpellId::TrinketDamageBoost3,
];

/// ACE `SpellLevelProgression.TrinketDamageReduction` (`List<SpellId>`).
pub static TRINKET_DAMAGE_REDUCTION: [SpellId; 3] = [
    SpellId::TrinketDamageReduction1,
    SpellId::TrinketDamageReduction2,
    SpellId::TrinketDamageReduction3,
];

/// ACE `SpellLevelProgression.TrinketHealth` (`List<SpellId>`).
pub static TRINKET_HEALTH: [SpellId; 3] = [
    SpellId::TrinketHealth1,
    SpellId::TrinketHealth2,
    SpellId::TrinketHealth3,
];

/// ACE `SpellLevelProgression.TrinketMana` (`List<SpellId>`).
pub static TRINKET_MANA: [SpellId; 3] = [
    SpellId::TrinketMana1,
    SpellId::TrinketMana2,
    SpellId::TrinketMana3,
];

/// ACE `SpellLevelProgression.TrinketStamina` (`List<SpellId>`).
pub static TRINKET_STAMINA: [SpellId; 3] = [
    SpellId::TrinketStamina1,
    SpellId::TrinketStamina2,
    SpellId::TrinketStamina3,
];

/// ACE `SpellLevelProgression.DeceptionArcane` (`List<SpellId>`).
pub static DECEPTION_ARCANE: [SpellId; 5] = [
    SpellId::DeceptionArcane1,
    SpellId::DeceptionArcane2,
    SpellId::DeceptionArcane3,
    SpellId::DeceptionArcane4,
    SpellId::DeceptionArcane5,
];

/// ACE `SpellLevelProgression.SpectralFountain_PortalMaze` (`List<SpellId>`).
pub static SPECTRAL_FOUNTAIN_PORTAL_MAZE: [SpellId; 2] = [
    SpellId::SpectralFountain_PortalMaze1,
    SpellId::SpectralFountain_PortalMaze2,
];

/// ACE `SpellLevelProgression.RareDamageBoost` (`List<SpellId>`).
pub static RARE_DAMAGE_BOOST: [SpellId; 10] = [
    SpellId::RareDamageBoost1,
    SpellId::RareDamageBoost2,
    SpellId::RareDamageBoost3,
    SpellId::RareDamageBoost4,
    SpellId::RareDamageBoost5,
    SpellId::RareDamageBoost6,
    SpellId::RareDamageBoost7,
    SpellId::RareDamageBoost8,
    SpellId::RareDamageBoost9,
    SpellId::RareDamageBoost10,
];

/// ACE `SpellLevelProgression.RareDamageReduction` (`List<SpellId>`).
pub static RARE_DAMAGE_REDUCTION: [SpellId; 10] = [
    SpellId::RareDamageReduction1,
    SpellId::RareDamageReduction2,
    SpellId::RareDamageReduction3,
    SpellId::RareDamageReduction4,
    SpellId::RareDamageReduction5,
    SpellId::RareDamageReduction6,
    SpellId::RareDamageReduction7,
    SpellId::RareDamageReduction8,
    SpellId::RareDamageReduction9,
    SpellId::RareDamageReduction10,
];

/// ACE `SpellLevelProgression.AetheriaCriticalDamageBoost` (`List<SpellId>`).
pub static AETHERIA_CRITICAL_DAMAGE_BOOST: [SpellId; 15] = [
    SpellId::AetheriaCriticalDamageBoost1,
    SpellId::AetheriaCriticalDamageBoost2,
    SpellId::AetheriaCriticalDamageBoost3,
    SpellId::AetheriaCriticalDamageBoost4,
    SpellId::AetheriaCriticalDamageBoost5,
    SpellId::AetheriaCriticalDamageBoost6,
    SpellId::AetheriaCriticalDamageBoost7,
    SpellId::AetheriaCriticalDamageBoost8,
    SpellId::AetheriaCriticalDamageBoost9,
    SpellId::AetheriaCriticalDamageBoost10,
    SpellId::AetheriaCriticalDamageBoost11,
    SpellId::AetheriaCriticalDamageBoost12,
    SpellId::AetheriaCriticalDamageBoost13,
    SpellId::AetheriaCriticalDamageBoost14,
    SpellId::AetheriaCriticalDamageBoost15,
];

/// ACE `SpellLevelProgression.AetheriaDamageBoost` (`List<SpellId>`).
pub static AETHERIA_DAMAGE_BOOST: [SpellId; 15] = [
    SpellId::AetheriaDamageBoost1,
    SpellId::AetheriaDamageBoost2,
    SpellId::AetheriaDamageBoost3,
    SpellId::AetheriaDamageBoost4,
    SpellId::AetheriaDamageBoost5,
    SpellId::AetheriaDamageBoost6,
    SpellId::AetheriaDamageBoost7,
    SpellId::AetheriaDamageBoost8,
    SpellId::AetheriaDamageBoost9,
    SpellId::AetheriaDamageBoost10,
    SpellId::AetheriaDamageBoost11,
    SpellId::AetheriaDamageBoost12,
    SpellId::AetheriaDamageBoost13,
    SpellId::AetheriaDamageBoost14,
    SpellId::AetheriaDamageBoost15,
];

/// ACE `SpellLevelProgression.AetheriaDamageReduction` (`List<SpellId>`).
pub static AETHERIA_DAMAGE_REDUCTION: [SpellId; 15] = [
    SpellId::AetheriaDamageReduction1,
    SpellId::AetheriaDamageReduction2,
    SpellId::AetheriaDamageReduction3,
    SpellId::AetheriaDamageReduction4,
    SpellId::AetheriaDamageReduction5,
    SpellId::AetheriaDamageReduction6,
    SpellId::AetheriaDamageReduction7,
    SpellId::AetheriaDamageReduction8,
    SpellId::AetheriaDamageReduction9,
    SpellId::AetheriaDamageReduction10,
    SpellId::AetheriaDamageReduction11,
    SpellId::AetheriaDamageReduction12,
    SpellId::AetheriaDamageReduction13,
    SpellId::AetheriaDamageReduction14,
    SpellId::AetheriaDamageReduction15,
];

/// ACE `SpellLevelProgression.AetheriaHealBuff` (`List<SpellId>`).
pub static AETHERIA_HEAL_BUFF: [SpellId; 15] = [
    SpellId::AetheriaHealBuff1,
    SpellId::AetheriaHealBuff2,
    SpellId::AetheriaHealBuff3,
    SpellId::AetheriaHealBuff4,
    SpellId::AetheriaHealBuff5,
    SpellId::AetheriaHealBuff6,
    SpellId::AetheriaHealBuff7,
    SpellId::AetheriaHealBuff8,
    SpellId::AetheriaHealBuff9,
    SpellId::AetheriaHealBuff10,
    SpellId::AetheriaHealBuff11,
    SpellId::AetheriaHealBuff12,
    SpellId::AetheriaHealBuff13,
    SpellId::AetheriaHealBuff14,
    SpellId::AetheriaHealBuff15,
];

/// ACE `SpellLevelProgression.AetheriaHealth` (`List<SpellId>`).
pub static AETHERIA_HEALTH: [SpellId; 15] = [
    SpellId::AetheriaHealth1,
    SpellId::AetheriaHealth2,
    SpellId::AetheriaHealth3,
    SpellId::AetheriaHealth4,
    SpellId::AetheriaHealth5,
    SpellId::AetheriaHealth6,
    SpellId::AetheriaHealth7,
    SpellId::AetheriaHealth8,
    SpellId::AetheriaHealth9,
    SpellId::AetheriaHealth10,
    SpellId::AetheriaHealth11,
    SpellId::AetheriaHealth12,
    SpellId::AetheriaHealth13,
    SpellId::AetheriaHealth14,
    SpellId::AetheriaHealth15,
];

/// ACE `SpellLevelProgression.AetheriaMana` (`List<SpellId>`).
pub static AETHERIA_MANA: [SpellId; 15] = [
    SpellId::AetheriaMana1,
    SpellId::AetheriaMana2,
    SpellId::AetheriaMana3,
    SpellId::AetheriaMana4,
    SpellId::AetheriaMana5,
    SpellId::AetheriaMana6,
    SpellId::AetheriaMana7,
    SpellId::AetheriaMana8,
    SpellId::AetheriaMana9,
    SpellId::AetheriaMana10,
    SpellId::AetheriaMana11,
    SpellId::AetheriaMana12,
    SpellId::AetheriaMana13,
    SpellId::AetheriaMana14,
    SpellId::AetheriaMana15,
];

/// ACE `SpellLevelProgression.AetheriaStamina` (`List<SpellId>`).
pub static AETHERIA_STAMINA: [SpellId; 15] = [
    SpellId::AetheriaStamina1,
    SpellId::AetheriaStamina2,
    SpellId::AetheriaStamina3,
    SpellId::AetheriaStamina4,
    SpellId::AetheriaStamina5,
    SpellId::AetheriaStamina6,
    SpellId::AetheriaStamina7,
    SpellId::AetheriaStamina8,
    SpellId::AetheriaStamina9,
    SpellId::AetheriaStamina10,
    SpellId::AetheriaStamina11,
    SpellId::AetheriaStamina12,
    SpellId::AetheriaStamina13,
    SpellId::AetheriaStamina14,
    SpellId::AetheriaStamina15,
];

/// ACE `SpellLevelProgression.AetheriaEndurance` (`List<SpellId>`).
pub static AETHERIA_ENDURANCE: [SpellId; 15] = [
    SpellId::AetheriaEndurance1,
    SpellId::AetheriaEndurance2,
    SpellId::AetheriaEndurance3,
    SpellId::AetheriaEndurance4,
    SpellId::AetheriaEndurance5,
    SpellId::AetheriaEndurance6,
    SpellId::AetheriaEndurance7,
    SpellId::AetheriaEndurance8,
    SpellId::AetheriaEndurance9,
    SpellId::AetheriaEndurance10,
    SpellId::AetheriaEndurance11,
    SpellId::AetheriaEndurance12,
    SpellId::AetheriaEndurance13,
    SpellId::AetheriaEndurance14,
    SpellId::AetheriaEndurance15,
];

/// ACE `SpellLevelProgression.BaelzharonsCurseDestruction` (`List<SpellId>`).
pub static BAELZHARONS_CURSE_DESTRUCTION: [SpellId; 2] = [
    SpellId::BaelzharonsCurseDestruction,
    SpellId::BaelzharonsCurseDestruction2,
];

/// ACE `SpellLevelProgression.CurseDestructionOther` (`List<SpellId>`).
pub static CURSE_DESTRUCTION_OTHER: [SpellId; 8] = [
    SpellId::CurseDestructionOther1,
    SpellId::CurseDestructionOther2,
    SpellId::CurseDestructionOther3,
    SpellId::CurseDestructionOther4,
    SpellId::CurseDestructionOther5,
    SpellId::CurseDestructionOther6,
    SpellId::CurseDestructionOther7,
    SpellId::CurseDestructionOther8,
];

/// ACE `SpellLevelProgression.NetherStreak` (`List<SpellId>`).
pub static NETHER_STREAK: [SpellId; 8] = [
    SpellId::NetherStreak1,
    SpellId::NetherStreak2,
    SpellId::NetherStreak3,
    SpellId::NetherStreak4,
    SpellId::NetherStreak5,
    SpellId::NetherStreak6,
    SpellId::NetherStreak7,
    SpellId::NetherStreak8,
];

/// ACE `SpellLevelProgression.NetherBolt` (`List<SpellId>`).
pub static NETHER_BOLT: [SpellId; 8] = [
    SpellId::NetherBolt1,
    SpellId::NetherBolt2,
    SpellId::NetherBolt3,
    SpellId::NetherBolt4,
    SpellId::NetherBolt5,
    SpellId::NetherBolt6,
    SpellId::NetherBolt7,
    SpellId::NetherBolt8,
];

/// ACE `SpellLevelProgression.NetherArc` (`List<SpellId>`).
pub static NETHER_ARC: [SpellId; 8] = [
    SpellId::NetherArc1,
    SpellId::NetherArc2,
    SpellId::NetherArc3,
    SpellId::NetherArc4,
    SpellId::NetherArc5,
    SpellId::NetherArc6,
    SpellId::NetherArc7,
    SpellId::NetherArc8,
];

/// ACE `SpellLevelProgression.CurseFestering` (`List<SpellId>`).
pub static CURSE_FESTERING: [SpellId; 8] = [
    SpellId::CurseFestering1,
    SpellId::CurseFestering2,
    SpellId::CurseFestering3,
    SpellId::CurseFestering4,
    SpellId::CurseFestering5,
    SpellId::CurseFestering6,
    SpellId::CurseFestering7,
    SpellId::CurseFestering8,
];

/// ACE `SpellLevelProgression.CurseWeakness` (`List<SpellId>`).
pub static CURSE_WEAKNESS: [SpellId; 8] = [
    SpellId::CurseWeakness1,
    SpellId::CurseWeakness2,
    SpellId::CurseWeakness3,
    SpellId::CurseWeakness4,
    SpellId::CurseWeakness5,
    SpellId::CurseWeakness6,
    SpellId::CurseWeakness7,
    SpellId::CurseWeakness8,
];

/// ACE `SpellLevelProgression.Corrosion` (`List<SpellId>`).
pub static CORROSION: [SpellId; 8] = [
    SpellId::Corrosion1,
    SpellId::Corrosion2,
    SpellId::Corrosion3,
    SpellId::Corrosion4,
    SpellId::Corrosion5,
    SpellId::Corrosion6,
    SpellId::Corrosion7,
    SpellId::Corrosion8,
];

/// ACE `SpellLevelProgression.Corruption` (`List<SpellId>`).
pub static CORRUPTION: [SpellId; 8] = [
    SpellId::Corruption1,
    SpellId::Corruption2,
    SpellId::Corruption3,
    SpellId::Corruption4,
    SpellId::Corruption5,
    SpellId::Corruption6,
    SpellId::Corruption7,
    SpellId::Corruption8,
];

/// ACE `SpellLevelProgression.VoidMagicMasteryOther` (`List<SpellId>`).
pub static VOID_MAGIC_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::VoidMagicMasteryOther1,
    SpellId::VoidMagicMasteryOther2,
    SpellId::VoidMagicMasteryOther3,
    SpellId::VoidMagicMasteryOther4,
    SpellId::VoidMagicMasteryOther5,
    SpellId::VoidMagicMasteryOther6,
    SpellId::VoidMagicMasteryOther7,
    SpellId::VoidMagicMasteryOther8,
];

/// ACE `SpellLevelProgression.VoidMagicMasterySelf` (`List<SpellId>`).
pub static VOID_MAGIC_MASTERY_SELF: [SpellId; 8] = [
    SpellId::VoidMagicMasterySelf1,
    SpellId::VoidMagicMasterySelf2,
    SpellId::VoidMagicMasterySelf3,
    SpellId::VoidMagicMasterySelf4,
    SpellId::VoidMagicMasterySelf5,
    SpellId::VoidMagicMasterySelf6,
    SpellId::VoidMagicMasterySelf7,
    SpellId::VoidMagicMasterySelf8,
];

/// ACE `SpellLevelProgression.VoidMagicIneptitudeOther` (`List<SpellId>`).
pub static VOID_MAGIC_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::VoidMagicIneptitudeOther1,
    SpellId::VoidMagicIneptitudeOther2,
    SpellId::VoidMagicIneptitudeOther3,
    SpellId::VoidMagicIneptitudeOther4,
    SpellId::VoidMagicIneptitudeOther5,
    SpellId::VoidMagicIneptitudeOther6,
    SpellId::VoidMagicIneptitudeOther7,
    SpellId::VoidMagicIneptitudeOther8,
];

/// ACE `SpellLevelProgression.CantripVoidMagicAptitude` (`List<SpellId>`).
pub static CANTRIP_VOID_MAGIC_APTITUDE: [SpellId; 4] = [
    SpellId::CantripVoidMagicAptitude1,
    SpellId::CantripVoidMagicAptitude2,
    SpellId::CantripVoidMagicAptitude3,
    SpellId::CantripVoidMagicAptitude4,
];

/// ACE `SpellLevelProgression.SetVoidMagicAptitude` (`List<SpellId>`).
pub static SET_VOID_MAGIC_APTITUDE: [SpellId; 4] = [
    SpellId::SetVoidMagicAptitude1,
    SpellId::SetVoidMagicAptitude2,
    SpellId::SetVoidMagicAptitude3,
    SpellId::SetVoidMagicAptitude4,
];

/// ACE `SpellLevelProgression.CorruptorsBoon` (`List<SpellId>`).
pub static CORRUPTORS_BOON: [SpellId; 2] = [
    SpellId::CorruptorsBoon,
    SpellId::CorruptorsBoon3,
];

/// ACE `SpellLevelProgression.AcidSpitStreak` (`List<SpellId>`).
pub static ACID_SPIT_STREAK: [SpellId; 2] = [
    SpellId::AcidSpitStreak1,
    SpellId::AcidSpitStreak2,
];

/// ACE `SpellLevelProgression.AcidSpit` (`List<SpellId>`).
pub static ACID_SPIT: [SpellId; 2] = [
    SpellId::AcidSpit1,
    SpellId::AcidSpit2,
];

/// ACE `SpellLevelProgression.AcidSpitArc` (`List<SpellId>`).
pub static ACID_SPIT_ARC: [SpellId; 2] = [
    SpellId::AcidSpitArc1,
    SpellId::AcidSpitArc2,
];

/// ACE `SpellLevelProgression.AcidSpitBlast` (`List<SpellId>`).
pub static ACID_SPIT_BLAST: [SpellId; 2] = [
    SpellId::AcidSpitBlast1,
    SpellId::AcidspitBlast2,
];

/// ACE `SpellLevelProgression.AcidSpitVolley` (`List<SpellId>`).
pub static ACID_SPIT_VOLLEY: [SpellId; 2] = [
    SpellId::AcidSpitVolley1,
    SpellId::AcidSpitVolley2,
];

/// ACE `SpellLevelProgression.OlthoiCriticalDamageBoost` (`List<SpellId>`).
pub static OLTHOI_CRITICAL_DAMAGE_BOOST: [SpellId; 11] = [
    SpellId::OlthoiCriticalDamageBoost1,
    SpellId::OlthoiCriticalDamageBoost2,
    SpellId::OlthoiCriticalDamageBoost3,
    SpellId::OlthoiCriticalDamageBoost4,
    SpellId::OlthoiCriticalDamageBoost5,
    SpellId::OlthoiCriticalDamageBoost6,
    SpellId::OlthoiCriticalDamageBoost7,
    SpellId::OlthoiCriticalDamageBoost8,
    SpellId::OlthoiCriticalDamageBoost9,
    SpellId::OlthoiCriticalDamageBoost10,
    SpellId::OlthoiCriticalDamageBoost11,
];

/// ACE `SpellLevelProgression.OlthoiCriticalDamageReduction` (`List<SpellId>`).
pub static OLTHOI_CRITICAL_DAMAGE_REDUCTION: [SpellId; 11] = [
    SpellId::OlthoiCriticalDamageReduction1,
    SpellId::OlthoiCriticalDamageReduction2,
    SpellId::OlthoiCriticalDamageReduction3,
    SpellId::OlthoiCriticalDamageReduction4,
    SpellId::OlthoiCriticalDamageReduction5,
    SpellId::OlthoiCriticalDamageReduction6,
    SpellId::OlthoiCriticalDamageReduction7,
    SpellId::OlthoiCriticalDamageReduction8,
    SpellId::OlthoiCriticalDamageReduction9,
    SpellId::OlthoiCriticalDamageReduction10,
    SpellId::OlthoiCriticalDamageReduction11,
];

/// ACE `SpellLevelProgression.OlthoiDamageBoost` (`List<SpellId>`).
pub static OLTHOI_DAMAGE_BOOST: [SpellId; 11] = [
    SpellId::OlthoiDamageBoost1,
    SpellId::OlthoiDamageBoost2,
    SpellId::OlthoiDamageBoost3,
    SpellId::OlthoiDamageBoost4,
    SpellId::OlthoiDamageBoost5,
    SpellId::OlthoiDamageBoost6,
    SpellId::OlthoiDamageBoost7,
    SpellId::OlthoiDamageBoost8,
    SpellId::OlthoiDamageBoost9,
    SpellId::OlthoiDamageBoost10,
    SpellId::OlthoiDamageBoost11,
];

/// ACE `SpellLevelProgression.OlthoiDamageReduction` (`List<SpellId>`).
pub static OLTHOI_DAMAGE_REDUCTION: [SpellId; 11] = [
    SpellId::OlthoiDamageReduction1,
    SpellId::OlthoiDamageReduction2,
    SpellId::OlthoiDamageReduction3,
    SpellId::OlthoiDamageReduction4,
    SpellId::OlthoiDamageReduction5,
    SpellId::OlthoiDamageReduction6,
    SpellId::OlthoiDamageReduction7,
    SpellId::OlthoiDamageReduction8,
    SpellId::OlthoiDamageReduction9,
    SpellId::OlthoiDamageReduction10,
    SpellId::OlthoiDamageReduction11,
];

/// ACE `SpellLevelProgression.AcidSpitVulnerability` (`List<SpellId>`).
pub static ACID_SPIT_VULNERABILITY: [SpellId; 2] = [
    SpellId::AcidSpitVulnerability1,
    SpellId::AcidSpitVulnerability2,
];

/// ACE `SpellLevelProgression.BloodstoneBolt` (`List<SpellId>`).
pub static BLOODSTONE_BOLT: [SpellId; 8] = [
    SpellId::BloodstoneBolt1,
    SpellId::BloodstoneBolt2,
    SpellId::BloodstoneBolt3,
    SpellId::BloodstoneBolt4,
    SpellId::BloodstoneBolt5,
    SpellId::BloodstoneBolt6,
    SpellId::BloodstoneBolt7,
    SpellId::BloodstoneBolt8,
];

/// ACE `SpellLevelProgression.PortalSendingBloodstoneFactory` (`List<SpellId>`).
pub static PORTAL_SENDING_BLOODSTONE_FACTORY: [SpellId; 2] = [
    SpellId::PortalSendingBloodstoneFactory1,
    SpellId::PortalSendingBloodstoneFactory2,
];

/// ACE `SpellLevelProgression.PortalSendingRitualTime` (`List<SpellId>`).
pub static PORTAL_SENDING_RITUAL_TIME: [SpellId; 2] = [
    SpellId::PortalSendingRitualTime1,
    SpellId::PortalSendingRitualTime2,
];

/// ACE `SpellLevelProgression.NetherBlast` (`List<SpellId>`).
pub static NETHER_BLAST: [SpellId; 8] = [
    SpellId::NetherBlast1,
    SpellId::NetherBlast2,
    SpellId::NetherBlast3,
    SpellId::NetherBlast4,
    SpellId::NetherBlast5,
    SpellId::NetherBlast6,
    SpellId::NetherBlast7,
    SpellId::NetherBlast8,
];

/// ACE `SpellLevelProgression.AetheriaDoTResistance` (`List<SpellId>`).
pub static AETHERIA_DO_T_RESISTANCE: [SpellId; 15] = [
    SpellId::AetheriaDoTResistance1,
    SpellId::AetheriaDoTResistance2,
    SpellId::AetheriaDoTResistance3,
    SpellId::AetheriaDoTResistance4,
    SpellId::AetheriaDoTResistance5,
    SpellId::AetheriaDoTResistance6,
    SpellId::AetheriaDoTResistance7,
    SpellId::AetheriaDoTResistance8,
    SpellId::AetheriaDoTResistance9,
    SpellId::AetheriaDoTResistance10,
    SpellId::AetheriaDoTResistance11,
    SpellId::AetheriaDoTResistance12,
    SpellId::AetheriaDoTResistance13,
    SpellId::AetheriaDoTResistance14,
    SpellId::AetheriaDoTResistance15,
];

/// ACE `SpellLevelProgression.AetheriaHealthResistance` (`List<SpellId>`).
pub static AETHERIA_HEALTH_RESISTANCE: [SpellId; 15] = [
    SpellId::AetheriaHealthResistance1,
    SpellId::AetheriaHealthResistance2,
    SpellId::AetheriaHealthResistance3,
    SpellId::AetheriaHealthResistance4,
    SpellId::AetheriaHealthResistance5,
    SpellId::AetheriaHealthResistance6,
    SpellId::AetheriaHealthResistance7,
    SpellId::AetheriaHealthResistance8,
    SpellId::AetheriaHealthResistance9,
    SpellId::AetheriaHealthResistance10,
    SpellId::AetheriaHealthResistance11,
    SpellId::AetheriaHealthResistance12,
    SpellId::AetheriaHealthResistance13,
    SpellId::AetheriaHealthResistance14,
    SpellId::AetheriaHealthResistance15,
];

/// ACE `SpellLevelProgression.CloakAlchemyMastery` (`List<SpellId>`).
pub static CLOAK_ALCHEMY_MASTERY: [SpellId; 5] = [
    SpellId::CloakAlchemyMastery1,
    SpellId::CloakAlchemyMastery2,
    SpellId::CloakAlchemyMastery3,
    SpellId::CloakAlchemyMastery4,
    SpellId::CloakAlchemyMastery5,
];

/// ACE `SpellLevelProgression.CloakArcaneloreMastery` (`List<SpellId>`).
pub static CLOAK_ARCANELORE_MASTERY: [SpellId; 5] = [
    SpellId::CloakArcaneloreMastery1,
    SpellId::CloakArcaneloreMastery2,
    SpellId::CloakArcaneloreMastery3,
    SpellId::CloakArcaneloreMastery4,
    SpellId::CloakArcaneloreMastery5,
];

/// ACE `SpellLevelProgression.CloakArmortinkeringMastery` (`List<SpellId>`).
pub static CLOAK_ARMORTINKERING_MASTERY: [SpellId; 5] = [
    SpellId::CloakArmortinkeringMastery1,
    SpellId::CloakArmortinkeringMastery2,
    SpellId::CloakArmortinkeringMastery3,
    SpellId::CloakArmortinkeringMastery4,
    SpellId::CloakArmortinkeringMastery5,
];

/// ACE `SpellLevelProgression.CloakAssesspersonMastery` (`List<SpellId>`).
pub static CLOAK_ASSESSPERSON_MASTERY: [SpellId; 5] = [
    SpellId::CloakAssesspersonMastery1,
    SpellId::CloakAssesspersonMastery2,
    SpellId::CloakAssesspersonMastery3,
    SpellId::CloakAssesspersonMastery4,
    SpellId::CloakAssesspersonMastery5,
];

/// ACE `SpellLevelProgression.CloakAxeMastery` (`List<SpellId>`).
pub static CLOAK_AXE_MASTERY: [SpellId; 5] = [
    SpellId::CloakAxeMastery1,
    SpellId::CloakAxeMastery2,
    SpellId::CloakAxeMastery3,
    SpellId::CloakAxeMastery4,
    SpellId::CloakAxeMastery5,
];

/// ACE `SpellLevelProgression.CloakBowMastery` (`List<SpellId>`).
pub static CLOAK_BOW_MASTERY: [SpellId; 5] = [
    SpellId::CloakBowMastery1,
    SpellId::CloakBowMastery2,
    SpellId::CloakBowMastery3,
    SpellId::CloakBowMastery4,
    SpellId::CloakBowMastery5,
];

/// ACE `SpellLevelProgression.CloakCookingMastery` (`List<SpellId>`).
pub static CLOAK_COOKING_MASTERY: [SpellId; 5] = [
    SpellId::CloakCookingMastery1,
    SpellId::CloakCookingMastery2,
    SpellId::CloakCookingMastery3,
    SpellId::CloakCookingMastery4,
    SpellId::CloakCookingMastery5,
];

/// ACE `SpellLevelProgression.CloakCreatureenchantmentMastery` (`List<SpellId>`).
pub static CLOAK_CREATUREENCHANTMENT_MASTERY: [SpellId; 5] = [
    SpellId::CloakCreatureenchantmentMastery1,
    SpellId::CloakCreatureenchantmentMastery2,
    SpellId::CloakCreatureenchantmentMastery3,
    SpellId::CloakCreatureenchantmentMastery4,
    SpellId::CloakCreatureenchantmentMastery5,
];

/// ACE `SpellLevelProgression.CloakCrossbowMastery` (`List<SpellId>`).
pub static CLOAK_CROSSBOW_MASTERY: [SpellId; 5] = [
    SpellId::CloakCrossbowMastery1,
    SpellId::CloakCrossbowMastery2,
    SpellId::CloakCrossbowMastery3,
    SpellId::CloakCrossbowMastery4,
    SpellId::CloakCrossbowMastery5,
];

/// ACE `SpellLevelProgression.CloakDaggerMastery` (`List<SpellId>`).
pub static CLOAK_DAGGER_MASTERY: [SpellId; 5] = [
    SpellId::CloakDaggerMastery1,
    SpellId::CloakDaggerMastery2,
    SpellId::CloakDaggerMastery3,
    SpellId::CloakDaggerMastery4,
    SpellId::CloakDaggerMastery5,
];

/// ACE `SpellLevelProgression.CloakDeceptionMastery` (`List<SpellId>`).
pub static CLOAK_DECEPTION_MASTERY: [SpellId; 5] = [
    SpellId::CloakDeceptionMastery1,
    SpellId::CloakDeceptionMastery2,
    SpellId::CloakDeceptionMastery3,
    SpellId::CloakDeceptionMastery4,
    SpellId::CloakDeceptionMastery5,
];

/// ACE `SpellLevelProgression.CloakFletchingMastery` (`List<SpellId>`).
pub static CLOAK_FLETCHING_MASTERY: [SpellId; 5] = [
    SpellId::CloakFletchingMastery1,
    SpellId::CloakFletchingMastery2,
    SpellId::CloakFletchingMastery3,
    SpellId::CloakFletchingMastery4,
    SpellId::CloakFletchingMastery5,
];

/// ACE `SpellLevelProgression.CloakHealingMastery` (`List<SpellId>`).
pub static CLOAK_HEALING_MASTERY: [SpellId; 5] = [
    SpellId::CloakHealingMastery1,
    SpellId::CloakHealingMastery2,
    SpellId::CloakHealingMastery3,
    SpellId::CloakHealingMastery4,
    SpellId::CloakHealingMastery5,
];

/// ACE `SpellLevelProgression.CloakItemenchantmentMastery` (`List<SpellId>`).
pub static CLOAK_ITEMENCHANTMENT_MASTERY: [SpellId; 5] = [
    SpellId::CloakItemenchantmentMastery1,
    SpellId::CloakItemenchantmentMastery2,
    SpellId::CloakItemenchantmentMastery3,
    SpellId::CloakItemenchantmentMastery4,
    SpellId::CloakItemenchantmentMastery5,
];

/// ACE `SpellLevelProgression.CloakItemtinkeringMastery` (`List<SpellId>`).
pub static CLOAK_ITEMTINKERING_MASTERY: [SpellId; 5] = [
    SpellId::CloakItemtinkeringMastery1,
    SpellId::CloakItemtinkeringMastery2,
    SpellId::CloakItemtinkeringMastery3,
    SpellId::CloakItemtinkeringMastery4,
    SpellId::CloakItemtinkeringMastery5,
];

/// ACE `SpellLevelProgression.CloakLeadershipMastery` (`List<SpellId>`).
pub static CLOAK_LEADERSHIP_MASTERY: [SpellId; 5] = [
    SpellId::CloakLeadershipMastery1,
    SpellId::CloakLeadershipMastery2,
    SpellId::CloakLeadershipMastery3,
    SpellId::CloakLeadershipMastery4,
    SpellId::CloakLeadershipMastery5,
];

/// ACE `SpellLevelProgression.CloakLifemagicMastery` (`List<SpellId>`).
pub static CLOAK_LIFEMAGIC_MASTERY: [SpellId; 5] = [
    SpellId::CloakLifemagicMastery1,
    SpellId::CloakLifemagicMastery2,
    SpellId::CloakLifemagicMastery3,
    SpellId::CloakLifemagicMastery4,
    SpellId::CloakLifemagicMastery5,
];

/// ACE `SpellLevelProgression.CloakLoyaltyMastery` (`List<SpellId>`).
pub static CLOAK_LOYALTY_MASTERY: [SpellId; 5] = [
    SpellId::CloakLoyaltyMastery1,
    SpellId::CloakLoyaltyMastery2,
    SpellId::CloakLoyaltyMastery3,
    SpellId::CloakLoyaltyMastery4,
    SpellId::CloakLoyaltyMastery5,
];

/// ACE `SpellLevelProgression.CloakMaceMastery` (`List<SpellId>`).
pub static CLOAK_MACE_MASTERY: [SpellId; 5] = [
    SpellId::CloakMaceMastery1,
    SpellId::CloakMaceMastery2,
    SpellId::CloakMaceMastery3,
    SpellId::CloakMaceMastery4,
    SpellId::CloakMaceMastery5,
];

/// ACE `SpellLevelProgression.CloakMagicdefenseMastery` (`List<SpellId>`).
pub static CLOAK_MAGICDEFENSE_MASTERY: [SpellId; 5] = [
    SpellId::CloakMagicdefenseMastery1,
    SpellId::CloakMagicdefenseMastery2,
    SpellId::CloakMagicdefenseMastery3,
    SpellId::CloakMagicdefenseMastery4,
    SpellId::CloakMagicdefenseMastery5,
];

/// ACE `SpellLevelProgression.CloakMagictinkeringMastery` (`List<SpellId>`).
pub static CLOAK_MAGICTINKERING_MASTERY: [SpellId; 5] = [
    SpellId::CloakMagictinkeringMastery1,
    SpellId::CloakMagictinkeringMastery2,
    SpellId::CloakMagictinkeringMastery3,
    SpellId::CloakMagictinkeringMastery4,
    SpellId::CloakMagictinkeringMastery5,
];

/// ACE `SpellLevelProgression.CloakManaconversionMastery` (`List<SpellId>`).
pub static CLOAK_MANACONVERSION_MASTERY: [SpellId; 5] = [
    SpellId::CloakManaconversionMastery1,
    SpellId::CloakManaconversionMastery2,
    SpellId::CloakManaconversionMastery3,
    SpellId::CloakManaconversionMastery4,
    SpellId::CloakManaconversionMastery5,
];

/// ACE `SpellLevelProgression.CloakMeleedefenseMastery` (`List<SpellId>`).
pub static CLOAK_MELEEDEFENSE_MASTERY: [SpellId; 5] = [
    SpellId::CloakMeleedefenseMastery1,
    SpellId::CloakMeleedefenseMastery2,
    SpellId::CloakMeleedefenseMastery3,
    SpellId::CloakMeleedefenseMastery4,
    SpellId::CloakMeleedefenseMastery5,
];

/// ACE `SpellLevelProgression.CloakMissiledefenseMastery` (`List<SpellId>`).
pub static CLOAK_MISSILEDEFENSE_MASTERY: [SpellId; 5] = [
    SpellId::CloakMissiledefenseMastery1,
    SpellId::CloakMissiledefenseMastery2,
    SpellId::CloakMissiledefenseMastery3,
    SpellId::CloakMissiledefenseMastery4,
    SpellId::CloakMissiledefenseMastery5,
];

/// ACE `SpellLevelProgression.CloakSalvagingMastery` (`List<SpellId>`).
pub static CLOAK_SALVAGING_MASTERY: [SpellId; 5] = [
    SpellId::CloakSalvagingMastery1,
    SpellId::CloakSalvagingMastery2,
    SpellId::CloakSalvagingMastery3,
    SpellId::CloakSalvagingMastery4,
    SpellId::CloakSalvagingMastery5,
];

/// ACE `SpellLevelProgression.CloakSpearMastery` (`List<SpellId>`).
pub static CLOAK_SPEAR_MASTERY: [SpellId; 5] = [
    SpellId::CloakSpearMastery1,
    SpellId::CloakSpearMastery2,
    SpellId::CloakSpearMastery3,
    SpellId::CloakSpearMastery4,
    SpellId::CloakSpearMastery5,
];

/// ACE `SpellLevelProgression.CloakStaffMastery` (`List<SpellId>`).
pub static CLOAK_STAFF_MASTERY: [SpellId; 5] = [
    SpellId::CloakStaffMastery1,
    SpellId::CloakStaffMastery2,
    SpellId::CloakStaffMastery3,
    SpellId::CloakStaffMastery4,
    SpellId::CloakStaffMastery5,
];

/// ACE `SpellLevelProgression.CloakSwordMastery` (`List<SpellId>`).
pub static CLOAK_SWORD_MASTERY: [SpellId; 5] = [
    SpellId::CloakSwordMastery1,
    SpellId::CloakSwordMastery2,
    SpellId::CloakSwordMastery3,
    SpellId::CloakSwordMastery4,
    SpellId::CloakSwordMastery5,
];

/// ACE `SpellLevelProgression.CloakThrownWeaponMastery` (`List<SpellId>`).
pub static CLOAK_THROWN_WEAPON_MASTERY: [SpellId; 5] = [
    SpellId::CloakThrownWeaponMastery1,
    SpellId::CloakThrownWeaponMastery2,
    SpellId::CloakThrownWeaponMastery3,
    SpellId::CloakThrownWeaponMastery4,
    SpellId::CloakThrownWeaponMastery5,
];

/// ACE `SpellLevelProgression.CloakTwoHandedCombatMastery` (`List<SpellId>`).
pub static CLOAK_TWO_HANDED_COMBAT_MASTERY: [SpellId; 5] = [
    SpellId::CloakTwoHandedCombatMastery1,
    SpellId::CloakTwoHandedCombatMastery2,
    SpellId::CloakTwoHandedCombatMastery3,
    SpellId::CloakTwoHandedCombatMastery4,
    SpellId::CloakTwoHandedCombatMastery5,
];

/// ACE `SpellLevelProgression.CloakUnarmedCombatMastery` (`List<SpellId>`).
pub static CLOAK_UNARMED_COMBAT_MASTERY: [SpellId; 5] = [
    SpellId::CloakUnarmedCombatMastery1,
    SpellId::CloakUnarmedCombatMastery2,
    SpellId::CloakUnarmedCombatMastery3,
    SpellId::CloakUnarmedCombatMastery4,
    SpellId::CloakUnarmedCombatMastery5,
];

/// ACE `SpellLevelProgression.CloakVoidMagicMastery` (`List<SpellId>`).
pub static CLOAK_VOID_MAGIC_MASTERY: [SpellId; 5] = [
    SpellId::CloakVoidMagicMastery1,
    SpellId::CloakVoidMagicMastery2,
    SpellId::CloakVoidMagicMastery3,
    SpellId::CloakVoidMagicMastery4,
    SpellId::CloakVoidMagicMastery5,
];

/// ACE `SpellLevelProgression.CloakWarMagicMastery` (`List<SpellId>`).
pub static CLOAK_WAR_MAGIC_MASTERY: [SpellId; 5] = [
    SpellId::CloakWarMagicMastery1,
    SpellId::CloakWarMagicMastery2,
    SpellId::CloakWarMagicMastery3,
    SpellId::CloakWarMagicMastery4,
    SpellId::CloakWarMagicMastery5,
];

/// ACE `SpellLevelProgression.CloakWeapontinkeringMastery` (`List<SpellId>`).
pub static CLOAK_WEAPONTINKERING_MASTERY: [SpellId; 5] = [
    SpellId::CloakWeapontinkeringMastery1,
    SpellId::CloakWeapontinkeringMastery2,
    SpellId::CloakWeapontinkeringMastery3,
    SpellId::CloakWeapontinkeringMastery4,
    SpellId::CloakWeapontinkeringMastery5,
];

/// ACE `SpellLevelProgression.CloakAssessCreatureMastery` (`List<SpellId>`).
pub static CLOAK_ASSESS_CREATURE_MASTERY: [SpellId; 5] = [
    SpellId::CloakAssessCreatureMastery1,
    SpellId::CloakAssessCreatureMastery2,
    SpellId::CloakAssessCreatureMastery3,
    SpellId::CloakAssessCreatureMastery4,
    SpellId::CloakAssessCreatureMastery5,
];

/// ACE `SpellLevelProgression.DirtyFightingIneptitudeOther` (`List<SpellId>`).
pub static DIRTY_FIGHTING_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::DirtyFightingIneptitudeOther1,
    SpellId::DirtyFightingIneptitudeOther2,
    SpellId::DirtyFightingIneptitudeOther3,
    SpellId::DirtyFightingIneptitudeOther4,
    SpellId::DirtyFightingIneptitudeOther5,
    SpellId::DirtyFightingIneptitudeOther6,
    SpellId::DirtyFightingIneptitudeOther7,
    SpellId::DirtyFightingIneptitudeOther8,
];

/// ACE `SpellLevelProgression.DirtyFightingMasteryOther` (`List<SpellId>`).
pub static DIRTY_FIGHTING_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::DirtyFightingMasteryOther1,
    SpellId::DirtyFightingMasteryOther2,
    SpellId::DirtyFightingMasteryOther3,
    SpellId::DirtyFightingMasteryOther4,
    SpellId::DirtyFightingMasteryOther5,
    SpellId::DirtyFightingMasteryOther6,
    SpellId::DirtyFightingMasteryOther7,
    SpellId::DirtyFightingMasteryOther8,
];

/// ACE `SpellLevelProgression.DirtyFightingMasterySelf` (`List<SpellId>`).
pub static DIRTY_FIGHTING_MASTERY_SELF: [SpellId; 8] = [
    SpellId::DirtyFightingMasterySelf1,
    SpellId::DirtyFightingMasterySelf2,
    SpellId::DirtyFightingMasterySelf3,
    SpellId::DirtyFightingMasterySelf4,
    SpellId::DirtyFightingMasterySelf5,
    SpellId::DirtyFightingMasterySelf6,
    SpellId::DirtyFightingMasterySelf7,
    SpellId::DirtyFightingMasterySelf8,
];

/// ACE `SpellLevelProgression.DualWieldIneptitudeOther` (`List<SpellId>`).
pub static DUAL_WIELD_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::DualWieldIneptitudeOther1,
    SpellId::DualWieldIneptitudeOther2,
    SpellId::DualWieldIneptitudeOther3,
    SpellId::DualWieldIneptitudeOther4,
    SpellId::DualWieldIneptitudeOther5,
    SpellId::DualWieldIneptitudeOther6,
    SpellId::DualWieldIneptitudeOther7,
    SpellId::DualWieldIneptitudeOther8,
];

/// ACE `SpellLevelProgression.DualWieldMasteryOther` (`List<SpellId>`).
pub static DUAL_WIELD_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::DualWieldMasteryOther1,
    SpellId::DualWieldMasteryOther2,
    SpellId::DualWieldMasteryOther3,
    SpellId::DualWieldMasteryOther4,
    SpellId::DualWieldMasteryOther5,
    SpellId::DualWieldMasteryOther6,
    SpellId::DualWieldMasteryOther7,
    SpellId::DualWieldMasteryOther8,
];

/// ACE `SpellLevelProgression.DualWieldMasterySelf` (`List<SpellId>`).
pub static DUAL_WIELD_MASTERY_SELF: [SpellId; 8] = [
    SpellId::DualWieldMasterySelf1,
    SpellId::DualWieldMasterySelf2,
    SpellId::DualWieldMasterySelf3,
    SpellId::DualWieldMasterySelf4,
    SpellId::DualWieldMasterySelf5,
    SpellId::DualWieldMasterySelf6,
    SpellId::DualWieldMasterySelf7,
    SpellId::DualWieldMasterySelf8,
];

/// ACE `SpellLevelProgression.RecklessnessIneptitudeOther` (`List<SpellId>`).
pub static RECKLESSNESS_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::RecklessnessIneptitudeOther1,
    SpellId::RecklessnessIneptitudeOther2,
    SpellId::RecklessnessIneptitudeOther3,
    SpellId::RecklessnessIneptitudeOther4,
    SpellId::RecklessnessIneptitudeOther5,
    SpellId::RecklessnessIneptitudeOther6,
    SpellId::RecklessnessIneptitudeOther7,
    SpellId::RecklessnessIneptitudeOther8,
];

/// ACE `SpellLevelProgression.RecklessnessMasteryOther` (`List<SpellId>`).
pub static RECKLESSNESS_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::RecklessnessMasteryOther1,
    SpellId::RecklessnessMasteryOther2,
    SpellId::RecklessnessMasteryOther3,
    SpellId::RecklessnessMasteryOther4,
    SpellId::RecklessnessMasteryOther5,
    SpellId::RecklessnessMasteryOther6,
    SpellId::RecklessnessMasteryOther7,
    SpellId::RecklessnessMasteryOther8,
];

/// ACE `SpellLevelProgression.RecklessnessMasterySelf` (`List<SpellId>`).
pub static RECKLESSNESS_MASTERY_SELF: [SpellId; 8] = [
    SpellId::RecklessnessMasterySelf1,
    SpellId::RecklessnessMasterySelf2,
    SpellId::RecklessnessMasterySelf3,
    SpellId::RecklessnessMasterySelf4,
    SpellId::RecklessnessMasterySelf5,
    SpellId::RecklessnessMasterySelf6,
    SpellId::RecklessnessMasterySelf7,
    SpellId::RecklessnessMasterySelf8,
];

/// ACE `SpellLevelProgression.ShieldIneptitudeOther` (`List<SpellId>`).
pub static SHIELD_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::ShieldIneptitudeOther1,
    SpellId::ShieldIneptitudeOther2,
    SpellId::ShieldIneptitudeOther3,
    SpellId::ShieldIneptitudeOther4,
    SpellId::ShieldIneptitudeOther5,
    SpellId::ShieldIneptitudeOther6,
    SpellId::ShieldIneptitudeOther7,
    SpellId::ShieldIneptitudeOther8,
];

/// ACE `SpellLevelProgression.ShieldMasteryOther` (`List<SpellId>`).
pub static SHIELD_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::ShieldMasteryOther1,
    SpellId::ShieldMasteryOther2,
    SpellId::ShieldMasteryOther3,
    SpellId::ShieldMasteryOther4,
    SpellId::ShieldMasteryOther5,
    SpellId::ShieldMasteryOther6,
    SpellId::ShieldMasteryOther7,
    SpellId::ShieldMasteryOther8,
];

/// ACE `SpellLevelProgression.ShieldMasterySelf` (`List<SpellId>`).
pub static SHIELD_MASTERY_SELF: [SpellId; 8] = [
    SpellId::ShieldMasterySelf1,
    SpellId::ShieldMasterySelf2,
    SpellId::ShieldMasterySelf3,
    SpellId::ShieldMasterySelf4,
    SpellId::ShieldMasterySelf5,
    SpellId::ShieldMasterySelf6,
    SpellId::ShieldMasterySelf7,
    SpellId::ShieldMasterySelf8,
];

/// ACE `SpellLevelProgression.SneakAttackIneptitudeOther` (`List<SpellId>`).
pub static SNEAK_ATTACK_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::SneakAttackIneptitudeOther1,
    SpellId::SneakAttackIneptitudeOther2,
    SpellId::SneakAttackIneptitudeOther3,
    SpellId::SneakAttackIneptitudeOther4,
    SpellId::SneakAttackIneptitudeOther5,
    SpellId::SneakAttackIneptitudeOther6,
    SpellId::SneakAttackIneptitudeOther7,
    SpellId::SneakAttackIneptitudeOther8,
];

/// ACE `SpellLevelProgression.SneakAttackMasteryOther` (`List<SpellId>`).
pub static SNEAK_ATTACK_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::SneakAttackMasteryOther1,
    SpellId::SneakAttackMasteryOther2,
    SpellId::SneakAttackMasteryOther3,
    SpellId::SneakAttackMasteryOther4,
    SpellId::SneakAttackMasteryOther5,
    SpellId::SneakAttackMasteryOther6,
    SpellId::SneakAttackMasteryOther7,
    SpellId::SneakAttackMasteryOther8,
];

/// ACE `SpellLevelProgression.SneakAttackMasterySelf` (`List<SpellId>`).
pub static SNEAK_ATTACK_MASTERY_SELF: [SpellId; 8] = [
    SpellId::SneakAttackMasterySelf1,
    SpellId::SneakAttackMasterySelf2,
    SpellId::SneakAttackMasterySelf3,
    SpellId::SneakAttackMasterySelf4,
    SpellId::SneakAttackMasterySelf5,
    SpellId::SneakAttackMasterySelf6,
    SpellId::SneakAttackMasterySelf7,
    SpellId::SneakAttackMasterySelf8,
];

/// ACE `SpellLevelProgression.CantripDirtyFightingProwess` (`List<SpellId>`).
pub static CANTRIP_DIRTY_FIGHTING_PROWESS: [SpellId; 4] = [
    SpellId::CantripDirtyFightingProwess1,
    SpellId::CantripDirtyFightingProwess2,
    SpellId::CantripDirtyFightingProwess3,
    SpellId::CantripDirtyFightingProwess4,
];

/// ACE `SpellLevelProgression.CantripDualWieldAptitude` (`List<SpellId>`).
pub static CANTRIP_DUAL_WIELD_APTITUDE: [SpellId; 4] = [
    SpellId::CantripDualWieldAptitude1,
    SpellId::CantripDualWieldAptitude2,
    SpellId::CantripDualWieldAptitude3,
    SpellId::CantripDualWieldAptitude4,
];

/// ACE `SpellLevelProgression.CantripRecklessnessProwess` (`List<SpellId>`).
pub static CANTRIP_RECKLESSNESS_PROWESS: [SpellId; 4] = [
    SpellId::CantripRecklessnessProwess1,
    SpellId::CantripRecklessnessProwess2,
    SpellId::CantripRecklessnessProwess3,
    SpellId::CantripRecklessnessProwess4,
];

/// ACE `SpellLevelProgression.CantripShieldAptitude` (`List<SpellId>`).
pub static CANTRIP_SHIELD_APTITUDE: [SpellId; 4] = [
    SpellId::CantripShieldAptitude1,
    SpellId::CantripShieldAptitude2,
    SpellId::CantripShieldAptitude3,
    SpellId::CantripShieldAptitude4,
];

/// ACE `SpellLevelProgression.CantripSneakAttackProwess` (`List<SpellId>`).
pub static CANTRIP_SNEAK_ATTACK_PROWESS: [SpellId; 4] = [
    SpellId::CantripSneakAttackProwess1,
    SpellId::CantripSneakAttackProwess2,
    SpellId::CantripSneakAttackProwess3,
    SpellId::CantripSneakAttackProwess4,
];

/// ACE `SpellLevelProgression.CloakDirtyFightingMastery` (`List<SpellId>`).
pub static CLOAK_DIRTY_FIGHTING_MASTERY: [SpellId; 5] = [
    SpellId::CloakDirtyFightingMastery1,
    SpellId::CloakDirtyFightingMastery2,
    SpellId::CloakDirtyFightingMastery3,
    SpellId::CloakDirtyFightingMastery4,
    SpellId::CloakDirtyFightingMastery5,
];

/// ACE `SpellLevelProgression.CloakDualWieldMastery` (`List<SpellId>`).
pub static CLOAK_DUAL_WIELD_MASTERY: [SpellId; 5] = [
    SpellId::CloakDualWieldMastery1,
    SpellId::CloakDualWieldMastery2,
    SpellId::CloakDualWieldMastery3,
    SpellId::CloakDualWieldMastery4,
    SpellId::CloakDualWieldMastery5,
];

/// ACE `SpellLevelProgression.CloakRecklessnessMastery` (`List<SpellId>`).
pub static CLOAK_RECKLESSNESS_MASTERY: [SpellId; 5] = [
    SpellId::CloakRecklessnessMastery1,
    SpellId::CloakRecklessnessMastery2,
    SpellId::CloakRecklessnessMastery3,
    SpellId::CloakRecklessnessMastery4,
    SpellId::CloakRecklessnessMastery5,
];

/// ACE `SpellLevelProgression.CloakShieldMastery` (`List<SpellId>`).
pub static CLOAK_SHIELD_MASTERY: [SpellId; 5] = [
    SpellId::CloakShieldMastery1,
    SpellId::CloakShieldMastery2,
    SpellId::CloakShieldMastery3,
    SpellId::CloakShieldMastery4,
    SpellId::CloakShieldMastery5,
];

/// ACE `SpellLevelProgression.CloakSneakAttackMastery` (`List<SpellId>`).
pub static CLOAK_SNEAK_ATTACK_MASTERY: [SpellId; 5] = [
    SpellId::CloakSneakAttackMastery1,
    SpellId::CloakSneakAttackMastery2,
    SpellId::CloakSneakAttackMastery3,
    SpellId::CloakSneakAttackMastery4,
    SpellId::CloakSneakAttackMastery5,
];

/// ACE `SpellLevelProgression.SetDirtyFightingAptitude` (`List<SpellId>`).
pub static SET_DIRTY_FIGHTING_APTITUDE: [SpellId; 4] = [
    SpellId::SetDirtyFightingAptitude1,
    SpellId::SetDirtyFightingAptitude2,
    SpellId::SetDirtyFightingAptitude3,
    SpellId::SetDirtyFightingAptitude4,
];

/// ACE `SpellLevelProgression.SetDualWieldAptitude` (`List<SpellId>`).
pub static SET_DUAL_WIELD_APTITUDE: [SpellId; 4] = [
    SpellId::SetDualWieldAptitude1,
    SpellId::SetDualWieldAptitude2,
    SpellId::SetDualWieldAptitude3,
    SpellId::SetDualWieldAptitude4,
];

/// ACE `SpellLevelProgression.SetRecklessnessAptitude` (`List<SpellId>`).
pub static SET_RECKLESSNESS_APTITUDE: [SpellId; 4] = [
    SpellId::SetRecklessnessAptitude1,
    SpellId::SetRecklessnessAptitude2,
    SpellId::SetRecklessnessAptitude3,
    SpellId::SetRecklessnessAptitude4,
];

/// ACE `SpellLevelProgression.SetShieldAptitude` (`List<SpellId>`).
pub static SET_SHIELD_APTITUDE: [SpellId; 4] = [
    SpellId::SetShieldAptitude1,
    SpellId::SetShieldAptitude2,
    SpellId::SetShieldAptitude3,
    SpellId::SetShieldAptitude4,
];

/// ACE `SpellLevelProgression.SetSneakAttackAptitude` (`List<SpellId>`).
pub static SET_SNEAK_ATTACK_APTITUDE: [SpellId; 4] = [
    SpellId::SetSneakAttackAptitude1,
    SpellId::SetSneakAttackAptitude2,
    SpellId::SetSneakAttackAptitude3,
    SpellId::SetSneakAttackAptitude4,
];

/// ACE `SpellLevelProgression.RareArmorDamageBoost` (`List<SpellId>`).
pub static RARE_ARMOR_DAMAGE_BOOST: [SpellId; 5] = [
    SpellId::RareArmorDamageBoost1,
    SpellId::RareArmorDamageBoost2,
    SpellId::RareArmorDamageBoost3,
    SpellId::RareArmorDamageBoost4,
    SpellId::RareArmorDamageBoost5,
];

/// ACE `SpellLevelProgression.HermeticLinkOther` (`List<SpellId>`).
pub static HERMETIC_LINK_OTHER: [SpellId; 8] = [
    SpellId::HermeticLinkOther1,
    SpellId::HermeticLinkOther2,
    SpellId::HermeticLinkOther3,
    SpellId::HermeticLinkOther4,
    SpellId::HermeticLinkOther5,
    SpellId::HermeticLinkOther6,
    SpellId::HermeticLinkOther7,
    SpellId::HermeticLinkOther8,
];

/// ACE `SpellLevelProgression.BloodDrinkerOther` (`List<SpellId>`).
pub static BLOOD_DRINKER_OTHER: [SpellId; 8] = [
    SpellId::BloodDrinkerOther1,
    SpellId::BloodDrinkerOther2,
    SpellId::BloodDrinkerOther3,
    SpellId::BloodDrinkerOther4,
    SpellId::BloodDrinkerOther5,
    SpellId::BloodDrinkerOther6,
    SpellId::BloodDrinkerOther7,
    SpellId::BloodDrinkerOther8,
];

/// ACE `SpellLevelProgression.DefenderOther` (`List<SpellId>`).
pub static DEFENDER_OTHER: [SpellId; 8] = [
    SpellId::DefenderOther1,
    SpellId::DefenderOther2,
    SpellId::DefenderOther3,
    SpellId::DefenderOther4,
    SpellId::DefenderOther5,
    SpellId::DefenderOther6,
    SpellId::DefenderOther7,
    SpellId::DefenderOther8,
];

/// ACE `SpellLevelProgression.HeartSeekerOther` (`List<SpellId>`).
pub static HEART_SEEKER_OTHER: [SpellId; 8] = [
    SpellId::HeartSeekerOther1,
    SpellId::HeartSeekerOther2,
    SpellId::HeartSeekerOther3,
    SpellId::HeartSeekerOther4,
    SpellId::HeartSeekerOther5,
    SpellId::HeartSeekerOther6,
    SpellId::HeartSeekerOther7,
    SpellId::HeartSeekerOther8,
];

/// ACE `SpellLevelProgression.SpiritDrinkerOther` (`List<SpellId>`).
pub static SPIRIT_DRINKER_OTHER: [SpellId; 8] = [
    SpellId::SpiritDrinkerOther1,
    SpellId::SpiritDrinkerOther2,
    SpellId::SpiritDrinkerOther3,
    SpellId::SpiritDrinkerOther4,
    SpellId::SpiritDrinkerOther5,
    SpellId::SpiritDrinkerOther6,
    SpellId::SpiritDrinkerOther7,
    SpellId::SpiritDrinkerOther8,
];

/// ACE `SpellLevelProgression.SwiftKillerOther` (`List<SpellId>`).
pub static SWIFT_KILLER_OTHER: [SpellId; 8] = [
    SpellId::SwiftKillerOther1,
    SpellId::SwiftKillerOther2,
    SpellId::SwiftKillerOther3,
    SpellId::SwiftKillerOther4,
    SpellId::SwiftKillerOther5,
    SpellId::SwiftKillerOther6,
    SpellId::SwiftKillerOther7,
    SpellId::SwiftKillerOther8,
];

/// ACE `SpellLevelProgression.SummoningMasteryOther` (`List<SpellId>`).
pub static SUMMONING_MASTERY_OTHER: [SpellId; 8] = [
    SpellId::SummoningMasteryOther1,
    SpellId::SummoningMasteryOther2,
    SpellId::SummoningMasteryOther3,
    SpellId::SummoningMasteryOther4,
    SpellId::SummoningMasteryOther5,
    SpellId::SummoningMasteryOther6,
    SpellId::SummoningMasteryOther7,
    SpellId::SummoningMasteryOther8,
];

/// ACE `SpellLevelProgression.SummoningMasterySelf` (`List<SpellId>`).
pub static SUMMONING_MASTERY_SELF: [SpellId; 8] = [
    SpellId::SummoningMasterySelf1,
    SpellId::SummoningMasterySelf2,
    SpellId::SummoningMasterySelf3,
    SpellId::SummoningMasterySelf4,
    SpellId::SummoningMasterySelf5,
    SpellId::SummoningMasterySelf6,
    SpellId::SummoningMasterySelf7,
    SpellId::SummoningMasterySelf8,
];

/// ACE `SpellLevelProgression.CantripSummoningProwess` (`List<SpellId>`).
pub static CANTRIP_SUMMONING_PROWESS: [SpellId; 4] = [
    SpellId::CantripSummoningProwess1,
    SpellId::CantripSummoningProwess2,
    SpellId::CantripSummoningProwess3,
    SpellId::CantripSummoningProwess4,
];

/// ACE `SpellLevelProgression.SummoningIneptitudeOther` (`List<SpellId>`).
pub static SUMMONING_INEPTITUDE_OTHER: [SpellId; 8] = [
    SpellId::SummoningIneptitudeOther1,
    SpellId::SummoningIneptitudeOther2,
    SpellId::SummoningIneptitudeOther3,
    SpellId::SummoningIneptitudeOther4,
    SpellId::SummoningIneptitudeOther5,
    SpellId::SummoningIneptitudeOther6,
    SpellId::SummoningIneptitudeOther7,
    SpellId::SummoningIneptitudeOther8,
];

/// ACE `SpellLevelProgression.CloakSummoningMastery` (`List<SpellId>`).
pub static CLOAK_SUMMONING_MASTERY: [SpellId; 5] = [
    SpellId::CloakSummoningMastery1,
    SpellId::CloakSummoningMastery2,
    SpellId::CloakSummoningMastery3,
    SpellId::CloakSummoningMastery4,
    SpellId::CloakSummoningMastery5,
];

/// ACE `SpellLevelProgression.SetSummoningAptitude` (`List<SpellId>`).
pub static SET_SUMMONING_APTITUDE: [SpellId; 4] = [
    SpellId::SetSummoningAptitude1,
    SpellId::SetSummoningAptitude2,
    SpellId::SetSummoningAptitude3,
    SpellId::SetSummoningAptitude4,
];

/// ACE `SpellLevelProgression.ReturnToTheStronghold` (`List<SpellId>`).
pub static RETURN_TO_THE_STRONGHOLD: [SpellId; 3] = [
    SpellId::ReturnToTheStronghold1,
    SpellId::ReturnToTheStronghold2,
    SpellId::ReturnToTheStronghold3,
];

/// ACE `SpellLevelProgression.ParagonsDualWieldMastery` (`List<SpellId>`).
pub static PARAGONS_DUAL_WIELD_MASTERY: [SpellId; 5] = [
    SpellId::ParagonsDualWieldMasteryI,
    SpellId::ParagonsDualWieldMasteryII,
    SpellId::ParagonsDualWieldMasteryIII,
    SpellId::ParagonsDualWieldMasteryIV,
    SpellId::ParagonsDualWieldMasteryV,
];

/// ACE `SpellLevelProgression.ParagonsFinesseWeaponMastery` (`List<SpellId>`).
pub static PARAGONS_FINESSE_WEAPON_MASTERY: [SpellId; 5] = [
    SpellId::ParagonsFinesseWeaponMasteryI,
    SpellId::ParagonsFinesseWeaponMasteryII,
    SpellId::ParagonsFinesseWeaponMasteryIII,
    SpellId::ParagonsFinesseWeaponMasteryIV,
    SpellId::ParagonsFinesseWeaponMasteryV,
];

/// ACE `SpellLevelProgression.ParagonsHeavyWeaponMastery` (`List<SpellId>`).
pub static PARAGONS_HEAVY_WEAPON_MASTERY: [SpellId; 5] = [
    SpellId::ParagonsHeavyWeaponMasteryI,
    SpellId::ParagonsHeavyWeaponMasteryII,
    SpellId::ParagonsHeavyWeaponMasteryIII,
    SpellId::ParagonsHeavyWeaponMasteryIV,
    SpellId::ParagonsHeavyWeaponMasteryV,
];

/// ACE `SpellLevelProgression.ParagonsLifeMagicMastery` (`List<SpellId>`).
pub static PARAGONS_LIFE_MAGIC_MASTERY: [SpellId; 5] = [
    SpellId::ParagonsLifeMagicMasteryI,
    SpellId::ParagonsLifeMagicMasteryII,
    SpellId::ParagonsLifeMagicMasteryIII,
    SpellId::ParagonsLifeMagicMasteryIV,
    SpellId::ParagonsLifeMagicMasteryV,
];

/// ACE `SpellLevelProgression.ParagonsLightWeaponMastery` (`List<SpellId>`).
pub static PARAGONS_LIGHT_WEAPON_MASTERY: [SpellId; 5] = [
    SpellId::ParagonsLightWeaponMasteryI,
    SpellId::ParagonsLightWeaponMasteryII,
    SpellId::ParagonsLightWeaponMasteryIII,
    SpellId::ParagonsLightWeaponMasteryIV,
    SpellId::ParagonsLightWeaponMasteryV,
];

/// ACE `SpellLevelProgression.ParagonsMissileWeaponMastery` (`List<SpellId>`).
pub static PARAGONS_MISSILE_WEAPON_MASTERY: [SpellId; 5] = [
    SpellId::ParagonsMissileWeaponMasteryI,
    SpellId::ParagonsMissileWeaponMasteryII,
    SpellId::ParagonsMissileWeaponMasteryIII,
    SpellId::ParagonsMissileWeaponMasteryIV,
    SpellId::ParagonsMissileWeaponMasteryV,
];

/// ACE `SpellLevelProgression.ParagonsRecklessnessMastery` (`List<SpellId>`).
pub static PARAGONS_RECKLESSNESS_MASTERY: [SpellId; 5] = [
    SpellId::ParagonsRecklessnessMasteryI,
    SpellId::ParagonsRecklessnessMasteryII,
    SpellId::ParagonsRecklessnessMasteryIII,
    SpellId::ParagonsRecklessnessMasteryIV,
    SpellId::ParagonsRecklessnessMasteryV,
];

/// ACE `SpellLevelProgression.ParagonsSneakAttackMastery` (`List<SpellId>`).
pub static PARAGONS_SNEAK_ATTACK_MASTERY: [SpellId; 5] = [
    SpellId::ParagonsSneakAttackMasteryI,
    SpellId::ParagonsSneakAttackMasteryII,
    SpellId::ParagonsSneakAttackMasteryIII,
    SpellId::ParagonsSneakAttackMasteryIV,
    SpellId::ParagonsSneakAttackMasteryV,
];

/// ACE `SpellLevelProgression.ParagonsTwoHandedCombatMastery` (`List<SpellId>`).
pub static PARAGONS_TWO_HANDED_COMBAT_MASTERY: [SpellId; 5] = [
    SpellId::ParagonsTwoHandedCombatMasteryI,
    SpellId::ParagonsTwoHandedCombatMasteryII,
    SpellId::ParagonsTwoHandedCombatMasteryIII,
    SpellId::ParagonsTwoHandedCombatMasteryIV,
    SpellId::ParagonsTwoHandedCombatMasteryV,
];

/// ACE `SpellLevelProgression.ParagonsVoidMagicMastery` (`List<SpellId>`).
pub static PARAGONS_VOID_MAGIC_MASTERY: [SpellId; 5] = [
    SpellId::ParagonsVoidMagicMasteryI,
    SpellId::ParagonsVoidMagicMasteryII,
    SpellId::ParagonsVoidMagicMasteryIII,
    SpellId::ParagonsVoidMagicMasteryIV,
    SpellId::ParagonsVoidMagicMasteryV,
];

/// ACE `SpellLevelProgression.ParagonsWarMagicMastery` (`List<SpellId>`).
pub static PARAGONS_WAR_MAGIC_MASTERY: [SpellId; 5] = [
    SpellId::ParagonsWarMagicMasteryI,
    SpellId::ParagonsWarMagicMasteryII,
    SpellId::ParagonsWarMagicMasteryIII,
    SpellId::ParagonsWarMagicMasteryIV,
    SpellId::ParagonsWarMagicMasteryV,
];

/// ACE `SpellLevelProgression.ParagonsDirtyFightingMastery` (`List<SpellId>`).
pub static PARAGONS_DIRTY_FIGHTING_MASTERY: [SpellId; 5] = [
    SpellId::ParagonsDirtyFightingMasteryI,
    SpellId::ParagonsDirtyFightingMasteryII,
    SpellId::ParagonsDirtyFightingMasteryIII,
    SpellId::ParagonsDirtyFightingMasteryIV,
    SpellId::ParagonsDirtyFightingMasteryV,
];

/// ACE `SpellLevelProgression.ParagonsWillpower` (`List<SpellId>`).
pub static PARAGONS_WILLPOWER: [SpellId; 5] = [
    SpellId::ParagonsWillpowerI,
    SpellId::ParagonsWillpowerII,
    SpellId::ParagonsWillpowerIII,
    SpellId::ParagonsWillpowerIV,
    SpellId::ParagonsWillpowerV,
];

/// ACE `SpellLevelProgression.ParagonsCoordination` (`List<SpellId>`).
pub static PARAGONS_COORDINATION: [SpellId; 5] = [
    SpellId::ParagonsCoordinationI,
    SpellId::ParagonsCoordinationII,
    SpellId::ParagonsCoordinationIII,
    SpellId::ParagonsCoordinationIV,
    SpellId::ParagonsCoordinationV,
];

/// ACE `SpellLevelProgression.ParagonsEndurance` (`List<SpellId>`).
pub static PARAGONS_ENDURANCE: [SpellId; 5] = [
    SpellId::ParagonsEnduranceI,
    SpellId::ParagonsEnduranceII,
    SpellId::ParagonsEnduranceIII,
    SpellId::ParagonsEnduranceIV,
    SpellId::ParagonsEnduranceV,
];

/// ACE `SpellLevelProgression.ParagonsFocus` (`List<SpellId>`).
pub static PARAGONS_FOCUS: [SpellId; 5] = [
    SpellId::ParagonsFocusI,
    SpellId::ParagonsFocusII,
    SpellId::ParagonsFocusIII,
    SpellId::ParagonsFocusIV,
    SpellId::ParagonsFocusV,
];

/// ACE `SpellLevelProgression.ParagonQuickness` (`List<SpellId>`).
pub static PARAGON_QUICKNESS: [SpellId; 5] = [
    SpellId::ParagonQuicknessI,
    SpellId::ParagonQuicknessII,
    SpellId::ParagonQuicknessIII,
    SpellId::ParagonQuicknessIV,
    SpellId::ParagonQuicknessV,
];

/// ACE `SpellLevelProgression.ParagonsStrength` (`List<SpellId>`).
pub static PARAGONS_STRENGTH: [SpellId; 5] = [
    SpellId::ParagonsStrengthI,
    SpellId::ParagonsStrengthII,
    SpellId::ParagonsStrengthIII,
    SpellId::ParagonsStrengthIV,
    SpellId::ParagonsStrengthV,
];

/// ACE `SpellLevelProgression.ParagonsStamina` (`List<SpellId>`).
pub static PARAGONS_STAMINA: [SpellId; 5] = [
    SpellId::ParagonsStaminaI,
    SpellId::ParagonsStaminaII,
    SpellId::ParagonsStaminaIII,
    SpellId::ParagonsStaminaIV,
    SpellId::ParagonsStaminaV,
];

/// ACE `SpellLevelProgression.ParagonsCriticalDamageBoost` (`List<SpellId>`).
pub static PARAGONS_CRITICAL_DAMAGE_BOOST: [SpellId; 4] = [
    SpellId::ParagonsCriticalDamageBoostII,
    SpellId::ParagonsCriticalDamageBoostIII,
    SpellId::ParagonsCriticalDamageBoostIV,
    SpellId::ParagonsCriticalDamageBoostV,
];

/// ACE `SpellLevelProgression.ParagonsCriticalDamageReduction` (`List<SpellId>`).
pub static PARAGONS_CRITICAL_DAMAGE_REDUCTION: [SpellId; 5] = [
    SpellId::ParagonsCriticalDamageReductionI,
    SpellId::ParagonsCriticalDamageReductionII,
    SpellId::ParagonsCriticalDamageReductionIII,
    SpellId::ParagonsCriticalDamageReductionIV,
    SpellId::ParagonsCriticalDamageReductionV,
];

/// ACE `SpellLevelProgression.ParagonsDamageBoost` (`List<SpellId>`).
pub static PARAGONS_DAMAGE_BOOST: [SpellId; 5] = [
    SpellId::ParagonsDamageBoostI,
    SpellId::ParagonsDamageBoostII,
    SpellId::ParagonsDamageBoostIII,
    SpellId::ParagonsDamageBoostIV,
    SpellId::ParagonsDamageBoostV,
];

/// ACE `SpellLevelProgression.ParagonsDamageReduction` (`List<SpellId>`).
pub static PARAGONS_DAMAGE_REDUCTION: [SpellId; 5] = [
    SpellId::ParagonsDamageReductionI,
    SpellId::ParagonsDamageReductionII,
    SpellId::ParagonsDamageReductionIII,
    SpellId::ParagonsDamageReductionIV,
    SpellId::ParagonsDamageReductionV,
];

/// ACE `SpellLevelProgression.ParagonsMana` (`List<SpellId>`).
pub static PARAGONS_MANA: [SpellId; 5] = [
    SpellId::ParagonsManaI,
    SpellId::ParagonsManaII,
    SpellId::ParagonsManaIII,
    SpellId::ParagonsManaIV,
    SpellId::ParagonsManaV,
];

/// ACE `SpellLevelProgression.GauntletCriticalDamageBoost` (`List<SpellId>`).
pub static GAUNTLET_CRITICAL_DAMAGE_BOOST: [SpellId; 2] = [
    SpellId::GauntletCriticalDamageBoostI,
    SpellId::GauntletCriticalDamageBoostII,
];

/// ACE `SpellLevelProgression.GauntletDamageBoost` (`List<SpellId>`).
pub static GAUNTLET_DAMAGE_BOOST: [SpellId; 2] = [
    SpellId::GauntletDamageBoostI,
    SpellId::GauntletDamageBoostII,
];

/// ACE `SpellLevelProgression.GauntletDamageReduction` (`List<SpellId>`).
pub static GAUNTLET_DAMAGE_REDUCTION: [SpellId; 2] = [
    SpellId::GauntletDamageReductionI,
    SpellId::GauntletDamageReductionII,
];

/// ACE `SpellLevelProgression.GauntletCriticalDamageReduction` (`List<SpellId>`).
pub static GAUNTLET_CRITICAL_DAMAGE_REDUCTION: [SpellId; 2] = [
    SpellId::GauntletCriticalDamageReductionI,
    SpellId::GauntletCriticalDamageReductionII,
];

/// ACE `SpellLevelProgression.GauntletHealingBoost` (`List<SpellId>`).
pub static GAUNTLET_HEALING_BOOST: [SpellId; 2] = [
    SpellId::GauntletHealingBoostI,
    SpellId::GauntletHealingBoostII,
];

/// ACE `SpellLevelProgression.GauntletVitality` (`List<SpellId>`).
pub static GAUNTLET_VITALITY: [SpellId; 3] = [
    SpellId::GauntletVitalityI,
    SpellId::GauntletVitalityII,
    SpellId::GauntletVitalityIII,
];

/// The arguments of the `AddSpells(..)` calls that make up ACE's `static SpellLevelProgression()`, in
/// call order.
pub static STATIC_CTOR_ADD_SPELLS_ARGS: [&[SpellId]; 739] = [
    &STRENGTH_OTHER,
    &STRENGTH_SELF,
    &WEAKNESS_OTHER,
    &WEAKNESS_SELF,
    &HEAL_OTHER,
    &HEAL_SELF,
    &HARM_OTHER,
    &HARM_SELF,
    &INFUSE_MANA,
    &VULNERABILITY_OTHER,
    &VULNERABILITY_SELF,
    &INVULNERABILITY_OTHER,
    &INVULNERABILITY_SELF,
    &FIRE_PROTECTION_OTHER,
    &FIRE_PROTECTION_SELF,
    &FIRE_VULNERABILITY_OTHER,
    &FIRE_VULNERABILITY_SELF,
    &ARMOR_OTHER,
    &ARMOR_SELF,
    &IMPERIL_OTHER,
    &IMPERIL_SELF,
    &FLAME_BOLT,
    &FROST_BOLT,
    &BLOOD_DRINKER_SELF,
    &BLOOD_LOATHER,
    &BLADE_BANE,
    &BLADE_LURE,
    &PORTAL_TIE,
    &PORTAL_TIE_RECALL,
    &SWIFT_KILLER_SELF,
    &LEADEN_WEAPON,
    &IMPENETRABILITY,
    &REJUVENATION_OTHER,
    &REJUVENATION_SELF,
    &ACID_STREAM,
    &SHOCK_WAVE,
    &LIGHTNING_BOLT,
    &FORCE_BOLT,
    &WHIRLING_BLADE,
    &ACID_BLAST,
    &SHOCK_BLAST,
    &FROST_BLAST,
    &LIGHTNING_BLAST,
    &FLAME_BLAST,
    &FORCE_BLAST,
    &BLADE_BLAST,
    &ACID_VOLLEY,
    &BLUDGEONING_VOLLEY,
    &FROST_VOLLEY,
    &LIGHTNING_VOLLEY,
    &FLAME_VOLLEY,
    &FORCE_VOLLEY,
    &BLADE_VOLLEY,
    &SUMMON_PORTAL,
    &REGENERATION_OTHER,
    &REGENERATION_SELF,
    &FESTER_OTHER,
    &FESTER_SELF,
    &EXHAUSTION_OTHER,
    &EXHAUSTION_SELF,
    &MANA_RENEWAL_OTHER,
    &MANA_RENEWAL_SELF,
    &MANA_DEPLETION_OTHER,
    &MANA_DEPLETION_SELF,
    &IMPREGNABILITY_OTHER,
    &IMPREGNABILITY_SELF,
    &DEFENSELESSNESS_OTHER,
    &MAGIC_RESISTANCE_OTHER,
    &MAGIC_RESISTANCE_SELF,
    &MAGIC_YIELD_OTHER,
    &MAGIC_YIELD_SELF,
    &LIGHT_WEAPONS_MASTERY_OTHER,
    &LIGHT_WEAPONS_MASTERY_SELF,
    &LIGHT_WEAPONS_INEPTITUDE_OTHER,
    &LIGHT_WEAPONS_INEPTITUDE_SELF,
    &FINESSE_WEAPONS_MASTERY_OTHER,
    &FINESSE_WEAPONS_MASTERY_SELF,
    &FINESSE_WEAPONS_INEPTITUDE_OTHER,
    &FINESSE_WEAPONS_INEPTITUDE_SELF,
    &MACE_MASTERY_OTHER,
    &MACE_MASTERY_SELF,
    &MACE_INEPTITUDE_OTHER,
    &MACE_INEPTITUDE_SELF,
    &SPEAR_MASTERY_OTHER,
    &SPEAR_MASTERY_SELF,
    &SPEAR_INEPTITUDE_OTHER,
    &SPEAR_INEPTITUDE_SELF,
    &STAFF_MASTERY_OTHER,
    &STAFF_MASTERY_SELF,
    &STAFF_INEPTITUDE_OTHER,
    &STAFF_INEPTITUDE_SELF,
    &HEAVY_WEAPONS_MASTERY_OTHER,
    &HEAVY_WEAPONS_MASTERY_SELF,
    &HEAVY_WEAPONS_INEPTITUDE_OTHER,
    &HEAVY_WEAPONS_INEPTITUDE_SELF,
    &UNARMED_COMBAT_MASTERY_OTHER,
    &UNARMED_COMBAT_MASTERY_SELF,
    &UNARMED_COMBAT_INEPTITUDE_OTHER,
    &UNARMED_COMBAT_INEPTITUDE_SELF,
    &MISSILE_WEAPONS_MASTERY_OTHER,
    &MISSILE_WEAPONS_MASTERY_SELF,
    &MISSILE_WEAPONS_INEPTITUDE_OTHER,
    &MISSILE_WEAPONS_INEPTITUDE_SELF,
    &CROSSBOW_MASTERY_OTHER,
    &CROSSBOW_MASTERY_SELF,
    &CROSSBOW_INEPTITUDE_OTHER,
    &CROSSBOW_INEPTITUDE_SELF,
    &ACID_PROTECTION_OTHER,
    &ACID_PROTECTION_SELF,
    &ACID_VULNERABILITY_OTHER,
    &ACID_VULNERABILITY_SELF,
    &THROWN_WEAPON_MASTERY_OTHER,
    &THROWN_WEAPON_MASTERY_SELF,
    &THROWN_WEAPON_INEPTITUDE_OTHER,
    &THROWN_WEAPON_INEPTITUDE_SELF,
    &CREATURE_ENCHANTMENT_MASTERY_SELF,
    &CREATURE_ENCHANTMENT_MASTERY_OTHER,
    &CREATURE_ENCHANTMENT_INEPTITUDE_OTHER,
    &CREATURE_ENCHANTMENT_INEPTITUDE_SELF,
    &ITEM_ENCHANTMENT_MASTERY_SELF,
    &ITEM_ENCHANTMENT_MASTERY_OTHER,
    &ITEM_ENCHANTMENT_INEPTITUDE_OTHER,
    &ITEM_ENCHANTMENT_INEPTITUDE_SELF,
    &LIFE_MAGIC_MASTERY_SELF,
    &LIFE_MAGIC_MASTERY_OTHER,
    &LIFE_MAGIC_INEPTITUDE_SELF,
    &LIFE_MAGIC_INEPTITUDE_OTHER,
    &WAR_MAGIC_MASTERY_SELF,
    &WAR_MAGIC_MASTERY_OTHER,
    &WAR_MAGIC_INEPTITUDE_SELF,
    &WAR_MAGIC_INEPTITUDE_OTHER,
    &MANA_MASTERY_SELF,
    &MANA_MASTERY_OTHER,
    &MANA_INEPTITUDE_SELF,
    &MANA_INEPTITUDE_OTHER,
    &ARCANE_ENLIGHTENMENT_SELF,
    &ARCANE_ENLIGHTENMENT_OTHER,
    &ARCANE_BENIGHTEDNESS_SELF,
    &ARCANE_BENIGHTEDNESS_OTHER,
    &ARMOR_EXPERTISE_SELF,
    &ARMOR_EXPERTISE_OTHER,
    &ARMOR_IGNORANCE_SELF,
    &ARMOR_IGNORANCE_OTHER,
    &ITEM_EXPERTISE_SELF,
    &ITEM_EXPERTISE_OTHER,
    &ITEM_IGNORANCE_SELF,
    &ITEM_IGNORANCE_OTHER,
    &MAGIC_ITEM_EXPERTISE_SELF,
    &MAGIC_ITEM_EXPERTISE_OTHER,
    &MAGIC_ITEM_IGNORANCE_SELF,
    &MAGIC_ITEM_IGNORANCE_OTHER,
    &WEAPON_EXPERTISE_SELF,
    &WEAPON_EXPERTISE_OTHER,
    &WEAPON_IGNORANCE_SELF,
    &WEAPON_IGNORANCE_OTHER,
    &MONSTER_ATTUNEMENT_SELF,
    &MONSTER_ATTUNEMENT_OTHER,
    &MONSTER_UNFAMILIARITY_SELF,
    &MONSTER_UNFAMILIARITY_OTHER,
    &PERSON_ATTUNEMENT_SELF,
    &PERSON_ATTUNEMENT_OTHER,
    &PERSON_UNFAMILIARITY_SELF,
    &PERSON_UNFAMILIARITY_OTHER,
    &DECEPTION_MASTERY_SELF,
    &DECEPTION_MASTERY_OTHER,
    &DECEPTION_INEPTITUDE_SELF,
    &DECEPTION_INEPTITUDE_OTHER,
    &HEALING_MASTERY_SELF,
    &HEALING_MASTERY_OTHER,
    &HEALING_INEPTITUDE_SELF,
    &HEALING_INEPTITUDE_OTHER,
    &LEADERSHIP_MASTERY_SELF,
    &LEADERSHIP_MASTERY_OTHER,
    &LEADERSHIP_INEPTITUDE_SELF,
    &LEADERSHIP_INEPTITUDE_OTHER,
    &LOCKPICK_MASTERY_SELF,
    &LOCKPICK_MASTERY_OTHER,
    &LOCKPICK_INEPTITUDE_SELF,
    &LOCKPICK_INEPTITUDE_OTHER,
    &FEALTY_SELF,
    &FEALTY_OTHER,
    &FAITHLESSNESS_SELF,
    &FAITHLESSNESS_OTHER,
    &JUMPING_MASTERY_SELF,
    &JUMPING_MASTERY_OTHER,
    &SPRINT_SELF,
    &SPRINT_OTHER,
    &LEADEN_FEET_SELF,
    &LEADEN_FEET_OTHER,
    &JUMPING_INEPTITUDE_SELF,
    &JUMPING_INEPTITUDE_OTHER,
    &BLUDGEON_PROTECTION_SELF,
    &BLUDGEON_PROTECTION_OTHER,
    &COLD_PROTECTION_SELF,
    &COLD_PROTECTION_OTHER,
    &BLUDGEON_VULNERABILITY_SELF,
    &BLUDGEON_VULNERABILITY_OTHER,
    &COLD_VULNERABILITY_SELF,
    &COLD_VULNERABILITY_OTHER,
    &LIGHTNING_PROTECTION_SELF,
    &LIGHTNING_PROTECTION_OTHER,
    &LIGHTNING_VULNERABILITY_SELF,
    &LIGHTNING_VULNERABILITY_OTHER,
    &BLADE_PROTECTION_SELF,
    &BLADE_PROTECTION_OTHER,
    &BLADE_VULNERABILITY_SELF,
    &BLADE_VULNERABILITY_OTHER,
    &PIERCING_PROTECTION_SELF,
    &PIERCING_PROTECTION_OTHER,
    &PIERCING_VULNERABILITY_SELF,
    &PIERCING_VULNERABILITY_OTHER,
    &REVITALIZE_SELF,
    &REVITALIZE_OTHER,
    &ENFEEBLE_SELF,
    &ENFEEBLE_OTHER,
    &MANA_BOOST_SELF,
    &MANA_BOOST_OTHER,
    &MANA_DRAIN_SELF,
    &MANA_DRAIN_OTHER,
    &INFUSE_HEALTH,
    &DRAIN_HEALTH,
    &INFUSE_STAMINA,
    &DRAIN_STAMINA,
    &DRAIN_MANA,
    &HEALTH_TO_STAMINA_OTHER,
    &HEALTH_TO_STAMINA_SELF,
    &HEALTH_TO_MANA_SELF,
    &HEALTH_TO_MANA_OTHER,
    &MANA_TO_HEALTH_OTHER,
    &MANA_TO_HEALTH_SELF,
    &MANA_TO_STAMINA_SELF,
    &MANA_TO_STAMINA_OTHER,
    &ENDURANCE_SELF,
    &ENDURANCE_OTHER,
    &FRAILTY_SELF,
    &FRAILTY_OTHER,
    &COORDINATION_SELF,
    &COORDINATION_OTHER,
    &CLUMSINESS_SELF,
    &CLUMSINESS_OTHER,
    &QUICKNESS_SELF,
    &QUICKNESS_OTHER,
    &SLOWNESS_SELF,
    &SLOWNESS_OTHER,
    &FOCUS_SELF,
    &FOCUS_OTHER,
    &BAFFLEMENT_SELF,
    &BAFFLEMENT_OTHER,
    &WILLPOWER_SELF,
    &WILLPOWER_OTHER,
    &FEEBLEMIND_SELF,
    &FEEBLEMIND_OTHER,
    &HERMETIC_VOID,
    &HERMETIC_LINK_SELF,
    &BRITTLEMAIL,
    &ACID_BANE,
    &ACID_LURE,
    &BLUDGEON_LURE,
    &BLUDGEON_BANE,
    &FROST_LURE,
    &FROST_BANE,
    &LIGHTNING_LURE,
    &LIGHTNING_BANE,
    &FLAME_LURE,
    &FLAME_BANE,
    &PIERCING_LURE,
    &PIERCING_BANE,
    &STRENGTHEN_LOCK,
    &WEAKEN_LOCK,
    &HEART_SEEKER_SELF,
    &TURN_BLADE,
    &DEFENDER_SELF,
    &LURE_BLADE,
    &DEFENSELESSNESS_SELF,
    &STAMINA_TO_HEALTH_OTHER,
    &STAMINA_TO_HEALTH_SELF,
    &STAMINA_TO_MANA_OTHER,
    &STAMINA_TO_MANA_SELF,
    &COOKING_MASTERY_OTHER,
    &COOKING_MASTERY_SELF,
    &COOKING_INEPTITUDE_OTHER,
    &COOKING_INEPTITUDE_SELF,
    &FLETCHING_MASTERY_OTHER,
    &FLETCHING_MASTERY_SELF,
    &FLETCHING_INEPTITUDE_OTHER,
    &FLETCHING_INEPTITUDE_SELF,
    &ALCHEMY_MASTERY_OTHER,
    &ALCHEMY_MASTERY_SELF,
    &ALCHEMY_INEPTITUDE_OTHER,
    &ALCHEMY_INEPTITUDE_SELF,
    &ACID_STREAK,
    &FLAME_STREAK,
    &FORCE_STREAK,
    &FROST_STREAK,
    &LIGHTNING_STREAK,
    &SHOCKWAVE_STREAK,
    &WHIRLING_BLADE_STREAK,
    &DISPEL_ALL_NEUTRAL_OTHER,
    &DISPEL_ALL_GOOD_OTHER,
    &DISPEL_ALL_BAD_OTHER,
    &DISPEL_ALL_NEUTRAL_SELF,
    &DISPEL_ALL_GOOD_SELF,
    &DISPEL_ALL_BAD_SELF,
    &DISPEL_CREATURE_NEUTRAL_OTHER,
    &DISPEL_CREATURE_GOOD_OTHER,
    &DISPEL_CREATURE_BAD_OTHER,
    &DISPEL_CREATURE_NEUTRAL_SELF,
    &DISPEL_CREATURE_GOOD_SELF,
    &DISPEL_CREATURE_BAD_SELF,
    &DISPEL_ITEM_NEUTRAL_OTHER,
    &DISPEL_ITEM_GOOD_OTHER,
    &DISPEL_ITEM_BAD_OTHER,
    &DISPEL_ITEM_NEUTRAL_SELF,
    &DISPEL_ITEM_GOOD_SELF,
    &DISPEL_ITEM_BAD_SELF,
    &DISPEL_LIFE_NEUTRAL_OTHER,
    &DISPEL_LIFE_GOOD_OTHER,
    &DISPEL_LIFE_BAD_OTHER,
    &DISPEL_LIFE_NEUTRAL_SELF,
    &DISPEL_LIFE_GOOD_SELF,
    &DISPEL_LIFE_BAD_SELF,
    &RECALL_ASMOLUM,
    &PORTAL_SEND_TRIAL,
    &CANTRIPALCHEMICALPROWESS,
    &CANTRIPARCANEPROWESS,
    &CANTRIPARMOREXPERTISE,
    &CANTRIPLIGHTWEAPONSAPTITUDE,
    &CANTRIPMISSILEWEAPONSAPTITUDE,
    &CANTRIPCOOKINGPROWESS,
    &CANTRIPCREATUREENCHANTMENTAPTITUDE,
    &CANTRIPCROSSBOWAPTITUDE,
    &CANTRIPFINESSEWEAPONSAPTITUDE,
    &CANTRIPDECEPTIONPROWESS,
    &CANTRIPFEALTY,
    &CANTRIPFLETCHINGPROWESS,
    &CANTRIPHEALINGPROWESS,
    &CANTRIPIMPREGNABILITY,
    &CANTRIPINVULNERABILITY,
    &CANTRIPITEMENCHANTMENTAPTITUDE,
    &CANTRIPITEMEXPERTISE,
    &CANTRIPJUMPINGPROWESS,
    &CANTRIPLEADERSHIP,
    &CANTRIPLIFEMAGICAPTITUDE,
    &CANTRIPLOCKPICKPROWESS,
    &CANTRIPMACEAPTITUDE,
    &CANTRIPMAGICITEMEXPERTISE,
    &CANTRIPMAGICRESISTANCE,
    &CANTRIPMANACONVERSIONPROWESS,
    &CANTRIPMONSTERATTUNEMENT,
    &CANTRIPPERSONATTUNEMENT,
    &CANTRIPSPEARAPTITUDE,
    &CANTRIPSPRINT,
    &CANTRIPSTAFFAPTITUDE,
    &CANTRIPHEAVYWEAPONSAPTITUDE,
    &CANTRIPTHROWNAPTITUDE,
    &CANTRIPUNARMEDAPTITUDE,
    &CANTRIPWARMAGICAPTITUDE,
    &CANTRIPWEAPONEXPERTISE,
    &CANTRIPARMOR,
    &CANTRIPCOORDINATION,
    &CANTRIPENDURANCE,
    &CANTRIPFOCUS,
    &CANTRIPQUICKNESS,
    &CANTRIPSTRENGTH,
    &CANTRIPWILLPOWER,
    &CANTRIPACIDBANE,
    &CANTRIPBLOODTHIRST,
    &CANTRIPBLUDGEONINGBANE,
    &CANTRIPDEFENDER,
    &CANTRIPFLAMEBANE,
    &CANTRIPFROSTBANE,
    &CANTRIPHEARTTHIRST,
    &CANTRIPIMPENETRABILITY,
    &CANTRIPPIERCINGBANE,
    &CANTRIPSLASHINGBANE,
    &CANTRIPSTORMBANE,
    &CANTRIPSWIFTHUNTER,
    &CANTRIPACIDWARD,
    &CANTRIPBLUDGEONINGWARD,
    &CANTRIPFLAMEWARD,
    &CANTRIPFROSTWARD,
    &CANTRIPPIERCINGWARD,
    &CANTRIPSLASHINGWARD,
    &CANTRIPSTORMWARD,
    &CANTRIPHEALTHGAIN,
    &CANTRIPMANAGAIN,
    &CANTRIPSTAMINAGAIN,
    &SUMMON_SECOND_PORTAL,
    &MARTINE_RING,
    &ELEMENTAL_FURY,
    &ACID_ARC,
    &FORCE_ARC,
    &FROST_ARC,
    &LIGHTNING_ARC,
    &FLAME_ARC,
    &SHOCK_ARC,
    &BLADE_ARC,
    &HEALTH_BOLT,
    &STAMINA_BOLT,
    &MANA_BOLT,
    &FIREWORK_OUT_BLACK,
    &FIREWORK_OUT_BLUE,
    &FIREWORK_OUT_GREEN,
    &FIREWORK_OUT_ORANGE,
    &FIREWORK_OUT_PURPLE,
    &FIREWORK_OUT_RED,
    &FIREWORK_OUT_WHITE,
    &FIREWORK_OUT_YELLOW,
    &FIREWORK_UP_BLACK,
    &FIREWORK_UP_BLUE,
    &FIREWORK_UP_GREEN,
    &FIREWORK_UP_ORANGE,
    &FIREWORK_UP_PURPLE,
    &FIREWORK_UP_RED,
    &FIREWORK_UP_WHITE,
    &FIREWORK_UP_YELLOW,
    &PORTAL_SENDING_KNORR,
    &PORTAL_SENDING_FELLOWSHIP_LIAZK_BURUN,
    &PORTAL_SENDING_LIAZK_BURUN,
    &PORTAL_SENDING_LIAZK_JUMP,
    &PORTAL_SENDING_LIAZK_SPIRITS,
    &PORTAL_SENDING_LIAZK_TEST,
    &COORDINATION_FELLOWSHIP,
    &ENDURANCE_FELLOWSHIP,
    &FOCUS_FELLOWSHIP,
    &QUICKNESS_FELLOWSHIP,
    &SELF_FELLOWSHIP,
    &STRENGTH_FELLOWSHIP,
    &CANTRIP_HERMETIC_LINK,
    &CANTRIP_SPIRIT_THIRST,
    &SPIRIT_DRINKER_SELF,
    &SPIRIT_LOATHER,
    &PORTAL_SENDING_HEZHIT_FIGHT,
    &PORTAL_SENDING_HEZHIT_PRISON,
    &PORTAL_SENDING_HIZK_RI_GAUNTLET,
    &PORTAL_SENDING_HIZK_RI_WELL,
    &PORTAL_SENDING_JRVIK_FIGHT,
    &PORTAL_SENDING_JRVIK_PRISON,
    &PORTAL_SENDING_ZIXK_FIGHT,
    &PORTAL_SENDING_ZIXK_PRISON,
    &PORTAL_SENDING_HIZK_RI_GURUK,
    &ACID_PROTECTION_FELLOWSHIP,
    &BLADE_PROTECTION_FELLOWSHIP,
    &BLUDGEON_PROTECTION_FELLOWSHIP,
    &COLD_PROTECTION_FELLOWSHIP,
    &FIRE_PROTECTION_FELLOWSHIP,
    &LIGHTNING_PROTECTION_FELLOWSHIP,
    &PIERCE_PROTECTION_FELLOWSHIP,
    &IMPREGNABILITY_FELLOWSHIP,
    &INVULNERABILITY_FELLOWSHIP,
    &MAGIC_RESISTANCE_FELLOWSHIP,
    &CREATURE_ENCHANTMENT_MASTERY_FELLOW,
    &ITEM_ENCHANTMENT_MASTERY_FELLOW,
    &LIFE_MAGIC_MASTERY_FELLOW,
    &MANA_CONVERSION_MASTERY_FELLOW,
    &WAR_MAGIC_MASTERY_FELLOW,
    &PORTAL_SENDING_KIVIK_LIR_ARENA,
    &PORTAL_SENDING_KIVIK_LIR_BOSS,
    &PORTAL_SENDING_KIVIK_LIR_HAVEN,
    &MANA_RENEWAL_FELLOWSHIP,
    &REGENERATION_FELLOWSHIP,
    &REJUVENATION_FELLOWSHIP,
    &PORTAL_SENDING_IZJI_QO_GAUNTLET,
    &PORTAL_SENDING_IZJI_QO_RECEIVING_CHAMBER,
    &PORTAL_SENDING_IZJI_QO_TEST,
    &ARCANUM_SALVAGING_SELF,
    &ARCANUM_SALVAGING_OTHER,
    &NUHMUDIRAS_WISDOM,
    &NUHMUDIRAS_WISDOM_OTHER,
    &INTOXICATION,
    &AXEMANS_BOON,
    &BOWMANS_BOON,
    &CHUCKERS_BOON,
    &CROSSBOWMANS_BOON,
    &ENCHANTERS_BOON,
    &HIEROMANCERS_BOON,
    &KNIFERS_BOON,
    &LIFE_GIVERS_BOON,
    &MACERS_BOON,
    &PUGILISTS_BOON,
    &SPEARMANS_BOON,
    &STAFFER_BOON,
    &SWORDSMANS_BOON,
    &SALVAGING_MASTERY_FORGE,
    &ALCHEMY_MASTERY_FORGE,
    &COOKING_MASTERY_FORGE,
    &FLETCHING_MASTERY_FORGE,
    &LOCKPICK_MASTERY_FORGE,
    &PORTAL_SENDING_PV_P_HATE20_ENTRY,
    &PORTAL_SENDING_PV_P_HATE40_ENTRY,
    &PORTAL_SENDING_PV_P_HATE60_ENTRY,
    &PORTAL_SENDING_PV_P_HATE80_ACCURSED_ENTRY,
    &PORTAL_SENDING_PV_P_HATE80_UNHOLY_ENTRY,
    &CANTRIP_SALVAGING,
    &RECALL_SON_POOKY,
    &PORTAL_SENDING_COLOSSEUM_A,
    &PORTAL_SENDING_COLOSSEUM_B,
    &PORTAL_SENDING_COLOSSEUM_C,
    &PORTAL_SENDING_COLOSSEUM_D,
    &PORTAL_SENDING_COLOSSEUM_E,
    &PORTAL_SENDING_VISION_QUEST_BRANCH4_STAGE,
    &PORTAL_SENDING_VISION_QUEST_BRANCH5_STAGE,
    &PORTAL_SENDING_VISION_QUEST_BRANCH1_STAGE,
    &PORTAL_SENDING_VISION_QUEST_BRANCH2_STAGE,
    &PORTAL_SENDING_VISION_QUEST_BRANCH3_STAGE,
    &PORTAL_SEND_DARK_CRYPT,
    &PORTAL_SEND_JESTER_PRISON,
    &RECALL_JESTER,
    &PORTAL_SENDING_CHDS_STAGE,
    &MOARSMAN_POISON,
    &SET_COORDINATION,
    &SET_ENDURANCE,
    &SET_FOCUS,
    &SET_QUICKNESS,
    &SET_STRENGTH,
    &SET_WILLPOWER,
    &SET_HEALTH,
    &SET_MANA,
    &SET_STAMINA,
    &SET_ACID_RESISTANCE,
    &SET_BLUDGEON_RESISTANCE,
    &SET_FLAME_RESISTANCE,
    &SET_FROST_RESISTANCE,
    &SET_LIGHTNING_RESISTANCE,
    &SET_PIERCE_RESISTANCE,
    &SET_SLASHING_RESISTANCE,
    &SET_ALCHEMY_APTITUDE,
    &SET_ARMOR_EXPERTISE_APTITUDE,
    &SET_AXE_APTITUDE,
    &SET_BOW_APTITUDE,
    &SET_COOKING_APTITUDE,
    &SET_CREATURE_ENCHANTMENT_APTITUDE,
    &SET_CROSSBOW_APTITUDE,
    &SET_DAGGER_APTITUDE,
    &SET_FLETCHING_APTITUDE,
    &SET_ITEM_ENCHANTMENT_APTITUDE,
    &SET_ITEM_EXPERTISE_APTITUDE,
    &SET_JUMPING_APTITUDE,
    &SET_LIFE_MAGIC_APTITUDE,
    &SET_LOCKPICK_APTITUDE,
    &SET_LOYALTY_APTITUDE,
    &SET_MACE_APTITUDE,
    &SET_MAGIC_DEFENSE_APTITUDE,
    &SET_MAGIC_ITEM_EXPERTISE_APTITUDE,
    &SET_MELEE_DEFENSE_APTITUDE,
    &SET_MISSILE_DEFENSE_APTITUDE,
    &SET_SALVAGING_APTITUDE,
    &SET_SPEAR_APTITUDE,
    &SET_SPRINT_APTITUDE,
    &SET_STAFF_APTITUDE,
    &SET_SWORD_APTITUDE,
    &SET_THROWN_APTITUDE,
    &SET_UNARMED_APTITUDE,
    &SET_WAR_MAGIC_APTITUDE,
    &SET_WEAPON_EXPERTISE_APTITUDE,
    &SET_SOCIETY_ATTRIBUTE_ALL,
    &SET_REJUVENATION,
    &ACID_STREAM8_SPELLPOWER,
    &MINI_ARCANE_DEATH,
    &MINI_FIREBALL,
    &MINI_ICEBALL,
    &MINI_ARROW,
    &MINI_RING,
    &PORTAL_SENDING_ASSASSINS_ROOST,
    &TWO_HANDED_BOON,
    &TWO_HANDED_MASTERY_SELF,
    &CANTRIPGEARCRAFTAPTITUDE,
    &CANTRIPTWOHANDEDAPTITUDE,
    &GEARCRAFT_INEPTITUDE,
    &GEARCRAFT_INEPTITUDE_SELF,
    &GEARCRAFT_MASTERY,
    &GEARCRAFT_MASTERY_SELF,
    &TWO_HANDED_INEPTITUDE,
    &TWO_HANDED_INEPTITUDE_SELF,
    &TWO_HANDED_MASTERY_OTHER,
    &SET_GEAR_CRAFT_APTITUDE,
    &SET_TWO_HANDED_APTITUDE,
    &EXPOSE_WEAKNESS,
    &CALL_OF_LEADERSHIP,
    &ANSWER_OF_LOYALTY_MANA,
    &ANSWER_OF_LOYALTY_STAM,
    &TRINKET_XP_BOOST,
    &TRINKET_DAMAGE_BOOST,
    &TRINKET_DAMAGE_REDUCTION,
    &TRINKET_HEALTH,
    &TRINKET_MANA,
    &TRINKET_STAMINA,
    &DECEPTION_ARCANE,
    &SPECTRAL_FOUNTAIN_PORTAL_MAZE,
    &RARE_DAMAGE_BOOST,
    &RARE_DAMAGE_REDUCTION,
    &AETHERIA_CRITICAL_DAMAGE_BOOST,
    &AETHERIA_DAMAGE_BOOST,
    &AETHERIA_DAMAGE_REDUCTION,
    &AETHERIA_HEAL_BUFF,
    &AETHERIA_HEALTH,
    &AETHERIA_MANA,
    &AETHERIA_STAMINA,
    &AETHERIA_ENDURANCE,
    &BAELZHARONS_CURSE_DESTRUCTION,
    &CURSE_DESTRUCTION_OTHER,
    &NETHER_STREAK,
    &NETHER_BOLT,
    &NETHER_ARC,
    &CURSE_FESTERING,
    &CURSE_WEAKNESS,
    &CORROSION,
    &CORRUPTION,
    &VOID_MAGIC_MASTERY_OTHER,
    &VOID_MAGIC_MASTERY_SELF,
    &VOID_MAGIC_INEPTITUDE_OTHER,
    &CANTRIP_VOID_MAGIC_APTITUDE,
    &SET_VOID_MAGIC_APTITUDE,
    &CORRUPTORS_BOON,
    &ACID_SPIT_STREAK,
    &ACID_SPIT,
    &ACID_SPIT_ARC,
    &ACID_SPIT_BLAST,
    &ACID_SPIT_VOLLEY,
    &OLTHOI_CRITICAL_DAMAGE_BOOST,
    &OLTHOI_CRITICAL_DAMAGE_REDUCTION,
    &OLTHOI_DAMAGE_BOOST,
    &OLTHOI_DAMAGE_REDUCTION,
    &ACID_SPIT_VULNERABILITY,
    &BLOODSTONE_BOLT,
    &PORTAL_SENDING_BLOODSTONE_FACTORY,
    &PORTAL_SENDING_RITUAL_TIME,
    &NETHER_BLAST,
    &AETHERIA_DO_T_RESISTANCE,
    &AETHERIA_HEALTH_RESISTANCE,
    &CLOAK_ALCHEMY_MASTERY,
    &CLOAK_ARCANELORE_MASTERY,
    &CLOAK_ARMORTINKERING_MASTERY,
    &CLOAK_ASSESSPERSON_MASTERY,
    &CLOAK_AXE_MASTERY,
    &CLOAK_BOW_MASTERY,
    &CLOAK_COOKING_MASTERY,
    &CLOAK_CREATUREENCHANTMENT_MASTERY,
    &CLOAK_CROSSBOW_MASTERY,
    &CLOAK_DAGGER_MASTERY,
    &CLOAK_DECEPTION_MASTERY,
    &CLOAK_FLETCHING_MASTERY,
    &CLOAK_HEALING_MASTERY,
    &CLOAK_ITEMENCHANTMENT_MASTERY,
    &CLOAK_ITEMTINKERING_MASTERY,
    &CLOAK_LEADERSHIP_MASTERY,
    &CLOAK_LIFEMAGIC_MASTERY,
    &CLOAK_LOYALTY_MASTERY,
    &CLOAK_MACE_MASTERY,
    &CLOAK_MAGICDEFENSE_MASTERY,
    &CLOAK_MAGICTINKERING_MASTERY,
    &CLOAK_MANACONVERSION_MASTERY,
    &CLOAK_MELEEDEFENSE_MASTERY,
    &CLOAK_MISSILEDEFENSE_MASTERY,
    &CLOAK_SALVAGING_MASTERY,
    &CLOAK_SPEAR_MASTERY,
    &CLOAK_STAFF_MASTERY,
    &CLOAK_SWORD_MASTERY,
    &CLOAK_THROWN_WEAPON_MASTERY,
    &CLOAK_TWO_HANDED_COMBAT_MASTERY,
    &CLOAK_UNARMED_COMBAT_MASTERY,
    &CLOAK_VOID_MAGIC_MASTERY,
    &CLOAK_WAR_MAGIC_MASTERY,
    &CLOAK_WEAPONTINKERING_MASTERY,
    &CLOAK_ASSESS_CREATURE_MASTERY,
    &DIRTY_FIGHTING_INEPTITUDE_OTHER,
    &DIRTY_FIGHTING_MASTERY_OTHER,
    &DIRTY_FIGHTING_MASTERY_SELF,
    &DUAL_WIELD_INEPTITUDE_OTHER,
    &DUAL_WIELD_MASTERY_OTHER,
    &DUAL_WIELD_MASTERY_SELF,
    &RECKLESSNESS_INEPTITUDE_OTHER,
    &RECKLESSNESS_MASTERY_OTHER,
    &RECKLESSNESS_MASTERY_SELF,
    &SHIELD_INEPTITUDE_OTHER,
    &SHIELD_MASTERY_OTHER,
    &SHIELD_MASTERY_SELF,
    &SNEAK_ATTACK_INEPTITUDE_OTHER,
    &SNEAK_ATTACK_MASTERY_OTHER,
    &SNEAK_ATTACK_MASTERY_SELF,
    &CANTRIP_DIRTY_FIGHTING_PROWESS,
    &CANTRIP_DUAL_WIELD_APTITUDE,
    &CANTRIP_RECKLESSNESS_PROWESS,
    &CANTRIP_SHIELD_APTITUDE,
    &CANTRIP_SNEAK_ATTACK_PROWESS,
    &CLOAK_DIRTY_FIGHTING_MASTERY,
    &CLOAK_DUAL_WIELD_MASTERY,
    &CLOAK_RECKLESSNESS_MASTERY,
    &CLOAK_SHIELD_MASTERY,
    &CLOAK_SNEAK_ATTACK_MASTERY,
    &SET_DIRTY_FIGHTING_APTITUDE,
    &SET_DUAL_WIELD_APTITUDE,
    &SET_RECKLESSNESS_APTITUDE,
    &SET_SHIELD_APTITUDE,
    &SET_SNEAK_ATTACK_APTITUDE,
    &RARE_ARMOR_DAMAGE_BOOST,
    &HERMETIC_LINK_OTHER,
    &BLOOD_DRINKER_OTHER,
    &DEFENDER_OTHER,
    &HEART_SEEKER_OTHER,
    &SPIRIT_DRINKER_OTHER,
    &SWIFT_KILLER_OTHER,
    &SUMMONING_MASTERY_OTHER,
    &SUMMONING_MASTERY_SELF,
    &CANTRIP_SUMMONING_PROWESS,
    &SUMMONING_INEPTITUDE_OTHER,
    &CLOAK_SUMMONING_MASTERY,
    &SET_SUMMONING_APTITUDE,
    &RETURN_TO_THE_STRONGHOLD,
    &PARAGONS_DUAL_WIELD_MASTERY,
    &PARAGONS_FINESSE_WEAPON_MASTERY,
    &PARAGONS_HEAVY_WEAPON_MASTERY,
    &PARAGONS_LIFE_MAGIC_MASTERY,
    &PARAGONS_LIGHT_WEAPON_MASTERY,
    &PARAGONS_MISSILE_WEAPON_MASTERY,
    &PARAGONS_RECKLESSNESS_MASTERY,
    &PARAGONS_SNEAK_ATTACK_MASTERY,
    &PARAGONS_TWO_HANDED_COMBAT_MASTERY,
    &PARAGONS_VOID_MAGIC_MASTERY,
    &PARAGONS_WAR_MAGIC_MASTERY,
    &PARAGONS_DIRTY_FIGHTING_MASTERY,
    &PARAGONS_WILLPOWER,
    &PARAGONS_COORDINATION,
    &PARAGONS_ENDURANCE,
    &PARAGONS_FOCUS,
    &PARAGON_QUICKNESS,
    &PARAGONS_STRENGTH,
    &PARAGONS_STAMINA,
    &PARAGONS_CRITICAL_DAMAGE_BOOST,
    &PARAGONS_CRITICAL_DAMAGE_REDUCTION,
    &PARAGONS_DAMAGE_BOOST,
    &PARAGONS_DAMAGE_REDUCTION,
    &PARAGONS_MANA,
    &GAUNTLET_CRITICAL_DAMAGE_BOOST,
    &GAUNTLET_DAMAGE_BOOST,
    &GAUNTLET_DAMAGE_REDUCTION,
    &GAUNTLET_CRITICAL_DAMAGE_REDUCTION,
    &GAUNTLET_HEALING_BOOST,
    &GAUNTLET_VITALITY,
];
