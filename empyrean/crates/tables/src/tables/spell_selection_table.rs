// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/SpellSelectionTable.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/SpellSelectionTable.cs`; do not edit by hand

//! The literal data of ACE's `SpellSelectionTable` (`Factories/Tables/SpellSelectionTable.cs`).

use crate::entity::ChanceTable;
use empyrean_entity::enums::SpellId;

/// ACE `SpellSelectionTable.spellSelectionGroup1` (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP1: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::StrengthSelf1, 0.06),
    (SpellId::EnduranceSelf1, 0.06),
    (SpellId::CoordinationSelf1, 0.06),
    (SpellId::QuicknessSelf1, 0.06),
    (SpellId::FocusSelf1, 0.06),
    (SpellId::WillpowerSelf1, 0.06),
    (SpellId::RegenerationSelf1, 0.11),
    (SpellId::RejuvenationSelf1, 0.11),
    (SpellId::ManaRenewalSelf1, 0.11),
    (SpellId::AcidProtectionSelf1, 0.03),
    (SpellId::BludgeonProtectionSelf1, 0.03),
    (SpellId::ColdProtectionSelf1, 0.03),
    (SpellId::LightningProtectionSelf1, 0.03),
    (SpellId::FireProtectionSelf1, 0.03),
    (SpellId::BladeProtectionSelf1, 0.03),
    (SpellId::PiercingProtectionSelf1, 0.03),
    (SpellId::ArmorSelf1, 0.10),
]);

/// ACE `SpellSelectionTable.spellSelectionGroup2` (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP2: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::MagicResistanceSelf1, 0.08),
    (SpellId::ArmorSelf1, 0.05),
    (SpellId::AcidProtectionSelf1, 0.05),
    (SpellId::BludgeonProtectionSelf1, 0.05),
    (SpellId::ColdProtectionSelf1, 0.05),
    (SpellId::LightningProtectionSelf1, 0.05),
    (SpellId::FireProtectionSelf1, 0.05),
    (SpellId::BladeProtectionSelf1, 0.05),
    (SpellId::PiercingProtectionSelf1, 0.05),
    (SpellId::StrengthSelf1, 0.04),
    (SpellId::EnduranceSelf1, 0.04),
    (SpellId::CoordinationSelf1, 0.04),
    (SpellId::QuicknessSelf1, 0.04),
    (SpellId::FocusSelf1, 0.04),
    (SpellId::WillpowerSelf1, 0.04),
    (SpellId::ManaRenewalSelf1, 0.04),
    (SpellId::ManaMasterySelf1, 0.04),
    (SpellId::RegenerationSelf1, 0.03),
    (SpellId::RejuvenationSelf1, 0.03),
    (SpellId::ItemExpertiseSelf1, 0.03),
    (SpellId::ArmorExpertiseSelf1, 0.02),
    (SpellId::ArcaneEnlightenmentSelf1, 0.02),
    (SpellId::DeceptionMasterySelf1, 0.01),
    (SpellId::FealtySelf1, 0.01),
    (SpellId::MonsterAttunementSelf1, 0.01),
    (SpellId::PersonAttunementSelf1, 0.01),
    (SpellId::ArcanumSalvagingSelf1, 0.01),
    (SpellId::MagicItemExpertiseSelf1, 0.01),
    (SpellId::WeaponExpertiseSelf1, 0.01),
]);

/// ACE `SpellSelectionTable.spellSelectionGroup3` (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP3: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::LeadershipMasterySelf1, 0.10),
    (SpellId::ImpregnabilitySelf1, 0.10),
    (SpellId::InvulnerabilitySelf1, 0.10),
    (SpellId::MagicResistanceSelf1, 0.10),
    (SpellId::FocusSelf1, 0.05),
    (SpellId::WillpowerSelf1, 0.05),
    (SpellId::ArmorSelf1, 0.05),
    (SpellId::RegenerationSelf1, 0.05),
    (SpellId::RejuvenationSelf1, 0.05),
    (SpellId::ManaRenewalSelf1, 0.05),
    (SpellId::ManaMasterySelf1, 0.05),
    (SpellId::ArcaneEnlightenmentSelf1, 0.05),
    (SpellId::HealingMasterySelf1, 0.05),
    (SpellId::DeceptionMasterySelf1, 0.05),
    (SpellId::MonsterAttunementSelf1, 0.05),
    (SpellId::PersonAttunementSelf1, 0.05),
]);

/// ACE `SpellSelectionTable.spellSelectionGroup4` (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP4: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::LifeMagicMasterySelf1, 0.20),
    (SpellId::CreatureEnchantmentMasterySelf1, 0.15),
    (SpellId::ItemEnchantmentMasterySelf1, 0.10),
    (SpellId::ArcaneEnlightenmentSelf1, 0.10),
    (SpellId::FocusSelf1, 0.09),
    (SpellId::WillpowerSelf1, 0.09),
    (SpellId::WarMagicMasterySelf1, 0.09),
    (SpellId::SneakAttackMasterySelf1, 0.09),
    (SpellId::ManaMasterySelf1, 0.09),
]);

/// ACE `SpellSelectionTable.spellSelectionGroup5` (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP5: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::WarMagicMasterySelf1, 0.25),
    (SpellId::WillpowerSelf1, 0.15),
    (SpellId::CreatureEnchantmentMasterySelf1, 0.10),
    (SpellId::ItemEnchantmentMasterySelf1, 0.10),
    (SpellId::LifeMagicMasterySelf1, 0.08),
    (SpellId::FocusSelf1, 0.08),
    (SpellId::ArcaneEnlightenmentSelf1, 0.08),
    (SpellId::ManaMasterySelf1, 0.08),
    (SpellId::SneakAttackMasterySelf1, 0.08),
]);

/// ACE `SpellSelectionTable.spellSelectionGroup6` (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP6: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::QuicknessSelf1, 0.25),
    (SpellId::StrengthSelf1, 0.15),
    (SpellId::EnduranceSelf1, 0.15),
    (SpellId::CoordinationSelf1, 0.15),
    (SpellId::DualWieldMasterySelf1, 0.10),
    (SpellId::DirtyFightingMasterySelf1, 0.10),
    (SpellId::SneakAttackMasterySelf1, 0.10),
]);

/// ACE `SpellSelectionTable.spellSelectionGroup7` (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP7: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::StrengthSelf1, 0.25),
    (SpellId::EnduranceSelf1, 0.25),
    (SpellId::MagicResistanceSelf1, 0.15),
    (SpellId::RejuvenationSelf1, 0.10),
    (SpellId::RegenerationSelf1, 0.10),
    (SpellId::SummoningMasterySelf1, 0.10),
    (SpellId::FealtySelf1, 0.05),
]);

/// ACE `SpellSelectionTable.spellSelectionGroup8` (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP8: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::ImpregnabilitySelf1, 0.15),
    (SpellId::InvulnerabilitySelf1, 0.15),
    (SpellId::FealtySelf1, 0.15),
    (SpellId::RejuvenationSelf1, 0.15),
    (SpellId::StrengthSelf1, 0.10),
    (SpellId::EnduranceSelf1, 0.10),
    (SpellId::MagicResistanceSelf1, 0.10),
    (SpellId::ShieldMasterySelf1, 0.10),
]);

/// ACE `SpellSelectionTable.spellSelectionGroup9` (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP9: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::CoordinationSelf1, 0.22),
    (SpellId::ShieldMasterySelf1, 0.12),
    (SpellId::HeavyWeaponsMasterySelf1, 0.11),
    (SpellId::LightWeaponsMasterySelf1, 0.11),
    (SpellId::FinesseWeaponsMasterySelf1, 0.11),
    (SpellId::MissileWeaponsMasterySelf1, 0.11),
    (SpellId::TwoHandedMasterySelf1, 0.11),
    (SpellId::HealingMasterySelf1, 0.11),
]);

/// ACE `SpellSelectionTable.spellSelectionGroup10` (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP10: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::MagicResistanceSelf1, 0.15),
    (SpellId::ImpregnabilitySelf1, 0.10),
    (SpellId::InvulnerabilitySelf1, 0.10),
    (SpellId::ArmorExpertiseSelf1, 0.05),
    (SpellId::ItemExpertiseSelf1, 0.05),
    (SpellId::WeaponExpertiseSelf1, 0.05),
    (SpellId::MonsterAttunementSelf1, 0.05),
    (SpellId::HealingMasterySelf1, 0.05),
    (SpellId::RegenerationSelf1, 0.05),
    (SpellId::RejuvenationSelf1, 0.05),
    (SpellId::ManaRenewalSelf1, 0.05),
    (SpellId::DualWieldMasterySelf1, 0.05),
    (SpellId::DirtyFightingMasterySelf1, 0.05),
    (SpellId::RecklessnessMasterySelf1, 0.05),
    (SpellId::SneakAttackMasterySelf1, 0.05),
    (SpellId::FealtySelf1, 0.05),
]);

/// ACE `SpellSelectionTable.spellSelectionGroup11` (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP11: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::QuicknessSelf1, 0.23),
    (SpellId::HeavyWeaponsMasterySelf1, 0.10),
    (SpellId::FinesseWeaponsMasterySelf1, 0.10),
    (SpellId::MissileWeaponsMasterySelf1, 0.10),
    (SpellId::HealingMasterySelf1, 0.10),
    (SpellId::LightWeaponsMasterySelf1, 0.09),
    (SpellId::TwoHandedMasterySelf1, 0.09),
    (SpellId::CoordinationSelf1, 0.09),
    (SpellId::JumpingMasterySelf1, 0.05),
    (SpellId::SprintSelf1, 0.05),
]);

/// ACE `SpellSelectionTable.spellSelectionGroup12` (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP12: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::ArmorSelf1, 0.30),
    (SpellId::AcidProtectionSelf1, 0.10),
    (SpellId::BludgeonProtectionSelf1, 0.10),
    (SpellId::ColdProtectionSelf1, 0.10),
    (SpellId::LightningProtectionSelf1, 0.10),
    (SpellId::FireProtectionSelf1, 0.10),
    (SpellId::BladeProtectionSelf1, 0.10),
    (SpellId::PiercingProtectionSelf1, 0.10),
]);

/// ACE `SpellSelectionTable.spellSelectionGroup13` (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP13: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::FocusSelf1, 0.04),
    (SpellId::WillpowerSelf1, 0.04),
    (SpellId::RejuvenationSelf1, 0.04),
    (SpellId::RegenerationSelf1, 0.04),
    (SpellId::ArmorSelf1, 0.03),
    (SpellId::CreatureEnchantmentMasterySelf1, 0.03),
    (SpellId::ItemEnchantmentMasterySelf1, 0.03),
    (SpellId::LifeMagicMasterySelf1, 0.03),
    (SpellId::WarMagicMasterySelf1, 0.03),
    (SpellId::VoidMagicMasterySelf1, 0.03),
    (SpellId::DualWieldMasterySelf1, 0.03),
    (SpellId::DirtyFightingMasterySelf1, 0.03),
    (SpellId::RecklessnessMasterySelf1, 0.03),
    (SpellId::SneakAttackMasterySelf1, 0.03),
    (SpellId::MagicResistanceSelf1, 0.03),
    (SpellId::ManaRenewalSelf1, 0.03),
    (SpellId::AlchemyMasterySelf1, 0.03),
    (SpellId::CookingMasterySelf1, 0.03),
    (SpellId::FletchingMasterySelf1, 0.03),
    (SpellId::HealingMasterySelf1, 0.03),
    (SpellId::LockpickMasterySelf1, 0.03),
    (SpellId::ArcaneEnlightenmentSelf1, 0.03),
    (SpellId::DeceptionMasterySelf1, 0.03),
    (SpellId::FealtySelf1, 0.03),
    (SpellId::ManaMasterySelf1, 0.03),
    (SpellId::ArcanumSalvagingSelf1, 0.03),
    (SpellId::ArmorExpertiseSelf1, 0.03),
    (SpellId::MagicItemExpertiseSelf1, 0.03),
    (SpellId::ItemExpertiseSelf1, 0.03),
    (SpellId::WeaponExpertiseSelf1, 0.03),
    (SpellId::MonsterAttunementSelf1, 0.03),
    (SpellId::PersonAttunementSelf1, 0.03),
]);

/// ACE `SpellSelectionTable.spellSelectionGroup14` (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP14: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::CoordinationSelf1, 0.04),
    (SpellId::QuicknessSelf1, 0.04),
    (SpellId::CreatureEnchantmentMasterySelf1, 0.04),
    (SpellId::ItemEnchantmentMasterySelf1, 0.04),
    (SpellId::LifeMagicMasterySelf1, 0.04),
    (SpellId::WarMagicMasterySelf1, 0.04),
    (SpellId::VoidMagicMasterySelf1, 0.04),
    (SpellId::ManaMasterySelf1, 0.04),
    (SpellId::ArcaneEnlightenmentSelf1, 0.04),
    (SpellId::ArcanumSalvagingSelf1, 0.04),
    (SpellId::ArmorExpertiseSelf1, 0.04),
    (SpellId::ItemExpertiseSelf1, 0.04),
    (SpellId::MagicItemExpertiseSelf1, 0.04),
    (SpellId::WeaponExpertiseSelf1, 0.04),
    (SpellId::HeavyWeaponsMasterySelf1, 0.04),
    (SpellId::LightWeaponsMasterySelf1, 0.04),
    (SpellId::FinesseWeaponsMasterySelf1, 0.04),
    (SpellId::MissileWeaponsMasterySelf1, 0.04),
    (SpellId::TwoHandedMasterySelf1, 0.04),
    (SpellId::ShieldMasterySelf1, 0.04),
    (SpellId::AlchemyMasterySelf1, 0.04),
    (SpellId::CookingMasterySelf1, 0.04),
    (SpellId::FletchingMasterySelf1, 0.04),
    (SpellId::HealingMasterySelf1, 0.04),
    (SpellId::LockpickMasterySelf1, 0.04),
]);

/// ACE `SpellSelectionTable.spellSelectionGroup15` (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP15: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::StrengthSelf1, 0.25),
    (SpellId::QuicknessSelf1, 0.25),
    (SpellId::SummoningMasterySelf1, 0.20),
    (SpellId::JumpingMasterySelf1, 0.10),
    (SpellId::SprintSelf1, 0.10),
    (SpellId::EnduranceSelf1, 0.10),
]);

/// ACE `SpellSelectionTable.spellSelectionGroup16` (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP16: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::AlchemyMasterySelf1, 0.09),
    (SpellId::CookingMasterySelf1, 0.09),
    (SpellId::FletchingMasterySelf1, 0.09),
    (SpellId::LockpickMasterySelf1, 0.08),
    (SpellId::ArcanumSalvagingSelf1, 0.08),
    (SpellId::ArmorExpertiseSelf1, 0.08),
    (SpellId::ItemExpertiseSelf1, 0.08),
    (SpellId::MagicItemExpertiseSelf1, 0.08),
    (SpellId::WeaponExpertiseSelf1, 0.08),
    (SpellId::WillpowerSelf1, 0.05),
    (SpellId::StrengthSelf1, 0.04),
    (SpellId::EnduranceSelf1, 0.04),
    (SpellId::CoordinationSelf1, 0.04),
    (SpellId::QuicknessSelf1, 0.04),
    (SpellId::FocusSelf1, 0.04),
]);

/// ACE `SpellSelectionTable.spellSelectionGroup17` (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP17: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::StrengthSelf1, 0.16),
    (SpellId::EnduranceSelf1, 0.15),
    (SpellId::CoordinationSelf1, 0.15),
    (SpellId::QuicknessSelf1, 0.15),
    (SpellId::DirtyFightingMasterySelf1, 0.13),
    (SpellId::RecklessnessMasterySelf1, 0.13),
    (SpellId::SneakAttackMasterySelf1, 0.13),
]);

/// ACE `SpellSelectionTable.spellSelectionGroup18` (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP18: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::StrengthSelf1, 0.06),
    (SpellId::QuicknessSelf1, 0.06),
    (SpellId::ImpregnabilitySelf1, 0.06),
    (SpellId::InvulnerabilitySelf1, 0.06),
    (SpellId::MagicResistanceSelf1, 0.06),
    (SpellId::ArcaneEnlightenmentSelf1, 0.06),
    (SpellId::ManaMasterySelf1, 0.06),
    (SpellId::HealingMasterySelf1, 0.06),
    (SpellId::JumpingMasterySelf1, 0.06),
    (SpellId::SprintSelf1, 0.06),
    (SpellId::HeavyWeaponsMasterySelf1, 0.06),
    (SpellId::LightWeaponsMasterySelf1, 0.06),
    (SpellId::FinesseWeaponsMasterySelf1, 0.06),
    (SpellId::MissileWeaponsMasterySelf1, 0.06),
    (SpellId::TwoHandedMasteryOther1, 0.06),
    (SpellId::EnduranceSelf1, 0.05),
    (SpellId::CoordinationSelf1, 0.05),
]);

/// ACE `SpellSelectionTable.spellSelectionGroup19` (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP19: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::VoidMagicMasterySelf1, 0.25),
    (SpellId::WillpowerSelf1, 0.15),
    (SpellId::ManaMasterySelf1, 0.10),
    (SpellId::LifeMagicMasterySelf1, 0.10),
    (SpellId::ArcaneEnlightenmentSelf1, 0.10),
    (SpellId::FocusSelf1, 0.09),
    (SpellId::CreatureEnchantmentMasterySelf1, 0.09),
    (SpellId::ItemEnchantmentMasterySelf1, 0.06),
    (SpellId::SneakAttackMasterySelf1, 0.06),
]);

/// ACE `SpellSelectionTable.spellSelectionGroup20` (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP20: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::RecklessnessMasterySelf1, 0.075),
    (SpellId::LockpickMasterySelf1, 0.075),
    (SpellId::CookingMasterySelf1, 0.05),
    (SpellId::FletchingMasterySelf1, 0.05),
    (SpellId::ItemEnchantmentMasterySelf1, 0.05),
    (SpellId::CreatureEnchantmentMasterySelf1, 0.04),
    (SpellId::FealtySelf1, 0.04),
    (SpellId::ManaMasterySelf1, 0.04),
    (SpellId::SneakAttackMasterySelf1, 0.04),
    (SpellId::WillpowerSelf1, 0.04),
    (SpellId::ItemExpertiseSelf1, 0.03),
    (SpellId::PersonAttunementSelf1, 0.03),
    (SpellId::RegenerationSelf1, 0.03),
    (SpellId::VoidMagicMasterySelf1, 0.03),
    (SpellId::WarMagicMasterySelf1, 0.03),
    (SpellId::WeaponExpertiseSelf1, 0.03),
    (SpellId::AlchemyMasterySelf1, 0.025),
    (SpellId::ArcaneEnlightenmentSelf1, 0.025),
    (SpellId::ArcanumSalvagingSelf1, 0.025),
    (SpellId::DeceptionMasterySelf1, 0.025),
    (SpellId::DualWieldMasterySelf1, 0.025),
    (SpellId::MonsterAttunementSelf1, 0.025),
    (SpellId::ArmorExpertiseSelf1, 0.02),
    (SpellId::DirtyFightingMasterySelf1, 0.02),
    (SpellId::FocusSelf1, 0.02),
    (SpellId::HealingMasterySelf1, 0.02),
    (SpellId::MagicItemExpertiseSelf1, 0.02),
    (SpellId::MagicResistanceSelf1, 0.02),
    (SpellId::ManaRenewalSelf1, 0.02),
    (SpellId::RejuvenationSelf1, 0.02),
    (SpellId::LifeMagicMasterySelf1, 0.01),
]);

/// ACE `SpellSelectionTable.spellSelectionGroup` (`List<ChanceTable<SpellId>>`).
pub static SPELL_SELECTION_GROUP: [&ChanceTable<SpellId>; 20] = [
    &SPELL_SELECTION_GROUP1,
    &SPELL_SELECTION_GROUP2,
    &SPELL_SELECTION_GROUP3,
    &SPELL_SELECTION_GROUP4,
    &SPELL_SELECTION_GROUP5,
    &SPELL_SELECTION_GROUP6,
    &SPELL_SELECTION_GROUP7,
    &SPELL_SELECTION_GROUP8,
    &SPELL_SELECTION_GROUP9,
    &SPELL_SELECTION_GROUP10,
    &SPELL_SELECTION_GROUP11,
    &SPELL_SELECTION_GROUP12,
    &SPELL_SELECTION_GROUP13,
    &SPELL_SELECTION_GROUP14,
    &SPELL_SELECTION_GROUP15,
    &SPELL_SELECTION_GROUP16,
    &SPELL_SELECTION_GROUP17,
    &SPELL_SELECTION_GROUP18,
    &SPELL_SELECTION_GROUP19,
    &SPELL_SELECTION_GROUP20,
];
