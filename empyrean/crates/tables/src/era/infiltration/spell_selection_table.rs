// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/SpellSelectionTable.cs
// @generated from ClassicACE's `Source/ACE.Server/Factories/Tables/SpellSelectionTable.cs`; do not edit by hand

//! The tables of ClassicACE's `SpellSelectionTable` under its Infiltration ruleset
//! (`Factories/Tables/SpellSelectionTable.cs`).

use crate::entity::ChanceTable;
use empyrean_entity::enums::SpellId;

/// ClassicACE `SpellSelectionTable.spellSelectionGroup1` (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP1: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::StrengthOther1, 0.06),
    (SpellId::EnduranceOther1, 0.06),
    (SpellId::CoordinationOther1, 0.06),
    (SpellId::QuicknessOther1, 0.06),
    (SpellId::FocusOther1, 0.06),
    (SpellId::WillpowerOther1, 0.06),
    (SpellId::RegenerationOther1, 0.11),
    (SpellId::RejuvenationOther1, 0.11),
    (SpellId::ManaRenewalOther1, 0.11),
    (SpellId::AcidProtectionOther1, 0.03),
    (SpellId::BludgeonProtectionOther1, 0.03),
    (SpellId::ColdProtectionOther1, 0.03),
    (SpellId::LightningProtectionOther1, 0.03),
    (SpellId::FireProtectionOther1, 0.03),
    (SpellId::BladeProtectionOther1, 0.03),
    (SpellId::PiercingProtectionOther1, 0.03),
    (SpellId::ArmorOther1, 0.10),
]);

/// ClassicACE `SpellSelectionTable.spellSelectionGroup2` (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP2: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::MagicResistanceOther1, 0.08),
    (SpellId::ArmorOther1, 0.05),
    (SpellId::AcidProtectionOther1, 0.05),
    (SpellId::BludgeonProtectionOther1, 0.05),
    (SpellId::ColdProtectionOther1, 0.05),
    (SpellId::LightningProtectionOther1, 0.05),
    (SpellId::FireProtectionOther1, 0.05),
    (SpellId::BladeProtectionOther1, 0.05),
    (SpellId::PiercingProtectionOther1, 0.05),
    (SpellId::StrengthOther1, 0.04),
    (SpellId::EnduranceOther1, 0.04),
    (SpellId::CoordinationOther1, 0.04),
    (SpellId::QuicknessOther1, 0.04),
    (SpellId::FocusOther1, 0.04),
    (SpellId::WillpowerOther1, 0.04),
    (SpellId::ManaRenewalOther1, 0.04),
    (SpellId::ManaMasteryOther1, 0.04),
    (SpellId::RegenerationOther1, 0.03),
    (SpellId::RejuvenationOther1, 0.03),
    (SpellId::ItemExpertiseOther1, 0.03),
    (SpellId::ArmorExpertiseOther1, 0.02),
    (SpellId::ArcaneEnlightenmentOther1, 0.02),
    (SpellId::DeceptionMasteryOther1, 0.01),
    (SpellId::FealtyOther1, 0.01),
    (SpellId::MonsterAttunementOther1, 0.01),
    (SpellId::PersonAttunementOther1, 0.01),
    (SpellId::ArcanumSalvagingOther1, 0.01),
    (SpellId::MagicItemExpertiseOther1, 0.01),
    (SpellId::WeaponExpertiseOther1, 0.01),
]);

/// ClassicACE `SpellSelectionTable.spellSelectionGroup3` (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP3: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::LeadershipMasteryOther1, 0.10),
    (SpellId::ImpregnabilityOther1, 0.10),
    (SpellId::InvulnerabilityOther1, 0.10),
    (SpellId::MagicResistanceOther1, 0.10),
    (SpellId::FocusOther1, 0.05),
    (SpellId::WillpowerOther1, 0.05),
    (SpellId::ArmorOther1, 0.05),
    (SpellId::RegenerationOther1, 0.05),
    (SpellId::RejuvenationOther1, 0.05),
    (SpellId::ManaRenewalOther1, 0.05),
    (SpellId::ManaMasteryOther1, 0.05),
    (SpellId::ArcaneEnlightenmentOther1, 0.05),
    (SpellId::HealingMasteryOther1, 0.05),
    (SpellId::DeceptionMasteryOther1, 0.05),
    (SpellId::MonsterAttunementOther1, 0.05),
    (SpellId::PersonAttunementOther1, 0.05),
]);

/// ClassicACE `SpellSelectionTable.spellSelectionGroup4`, as its Infiltration ruleset sets it (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP4: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::LifeMagicMasteryOther1, 0.20),
    (SpellId::CreatureEnchantmentMasteryOther1, 0.16),
    (SpellId::ItemEnchantmentMasteryOther1, 0.12),
    (SpellId::ArcaneEnlightenmentOther1, 0.12),
    (SpellId::FocusOther1, 0.10),
    (SpellId::WillpowerOther1, 0.10),
    (SpellId::WarMagicMasteryOther1, 0.10),
    (SpellId::ManaMasteryOther1, 0.10),
]);

/// ClassicACE `SpellSelectionTable.spellSelectionGroup5`, as its Infiltration ruleset sets it (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP5: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::WarMagicMasteryOther1, 0.26),
    (SpellId::WillpowerOther1, 0.16),
    (SpellId::CreatureEnchantmentMasteryOther1, 0.11),
    (SpellId::ItemEnchantmentMasteryOther1, 0.11),
    (SpellId::LifeMagicMasteryOther1, 0.09),
    (SpellId::FocusOther1, 0.09),
    (SpellId::ArcaneEnlightenmentOther1, 0.09),
    (SpellId::ManaMasteryOther1, 0.09),
]);

/// ClassicACE `SpellSelectionTable.spellSelectionGroup6`, as its Infiltration ruleset sets it (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP6: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::QuicknessOther1, 0.325),
    (SpellId::StrengthOther1, 0.225),
    (SpellId::EnduranceOther1, 0.225),
    (SpellId::CoordinationOther1, 0.225),
]);

/// ClassicACE `SpellSelectionTable.spellSelectionGroup7`, as its Infiltration ruleset sets it (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP7: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::StrengthOther1, 0.30),
    (SpellId::EnduranceOther1, 0.30),
    (SpellId::MagicResistanceOther1, 0.15),
    (SpellId::RejuvenationOther1, 0.10),
    (SpellId::RegenerationOther1, 0.10),
    (SpellId::FealtyOther1, 0.05),
]);

/// ClassicACE `SpellSelectionTable.spellSelectionGroup8`, as its Infiltration ruleset sets it (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP8: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::ImpregnabilityOther1, 0.17),
    (SpellId::InvulnerabilityOther1, 0.17),
    (SpellId::FealtyOther1, 0.17),
    (SpellId::RejuvenationOther1, 0.16),
    (SpellId::StrengthOther1, 0.11),
    (SpellId::EnduranceOther1, 0.11),
    (SpellId::MagicResistanceOther1, 0.11),
]);

/// ClassicACE `SpellSelectionTable.spellSelectionGroup9`, as its Infiltration ruleset sets it (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP9: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::CoordinationOther1, 0.20),
    (SpellId::HealingMasteryOther1, 0.10),
    (SpellId::LightWeaponsMasteryOther1, 0.07),
    (SpellId::FinesseWeaponsMasteryOther1, 0.07),
    (SpellId::MaceMasteryOther1, 0.07),
    (SpellId::SpearMasteryOther1, 0.07),
    (SpellId::StaffMasteryOther1, 0.07),
    (SpellId::HeavyWeaponsMasteryOther1, 0.07),
    (SpellId::UnarmedCombatMasteryOther1, 0.07),
    (SpellId::MissileWeaponsMasteryOther1, 0.07),
    (SpellId::CrossbowMasteryOther1, 0.07),
    (SpellId::ThrownWeaponMasteryOther1, 0.07),
]);

/// ClassicACE `SpellSelectionTable.spellSelectionGroup10`, as its Infiltration ruleset sets it (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP10: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::MagicResistanceOther1, 0.15),
    (SpellId::ImpregnabilityOther1, 0.11),
    (SpellId::InvulnerabilityOther1, 0.11),
    (SpellId::ArmorExpertiseOther1, 0.07),
    (SpellId::ItemExpertiseOther1, 0.07),
    (SpellId::WeaponExpertiseOther1, 0.07),
    (SpellId::MonsterAttunementOther1, 0.07),
    (SpellId::HealingMasteryOther1, 0.07),
    (SpellId::RegenerationOther1, 0.07),
    (SpellId::RejuvenationOther1, 0.07),
    (SpellId::ManaRenewalOther1, 0.07),
    (SpellId::FealtyOther1, 0.07),
]);

/// ClassicACE `SpellSelectionTable.spellSelectionGroup11`, as its Infiltration ruleset sets it (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP11: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::QuicknessOther1, 0.20),
    (SpellId::HealingMasteryOther1, 0.10),
    (SpellId::CoordinationOther1, 0.10),
    (SpellId::JumpingMasteryOther1, 0.05),
    (SpellId::SprintOther1, 0.05),
    (SpellId::LightWeaponsMasteryOther1, 0.05),
    (SpellId::FinesseWeaponsMasteryOther1, 0.05),
    (SpellId::MaceMasteryOther1, 0.05),
    (SpellId::SpearMasteryOther1, 0.05),
    (SpellId::StaffMasteryOther1, 0.05),
    (SpellId::HeavyWeaponsMasteryOther1, 0.05),
    (SpellId::UnarmedCombatMasteryOther1, 0.05),
    (SpellId::MissileWeaponsMasteryOther1, 0.05),
    (SpellId::CrossbowMasteryOther1, 0.05),
    (SpellId::ThrownWeaponMasteryOther1, 0.05),
]);

/// ClassicACE `SpellSelectionTable.spellSelectionGroup12` (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP12: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::ArmorOther1, 0.30),
    (SpellId::AcidProtectionOther1, 0.10),
    (SpellId::BludgeonProtectionOther1, 0.10),
    (SpellId::ColdProtectionOther1, 0.10),
    (SpellId::LightningProtectionOther1, 0.10),
    (SpellId::FireProtectionOther1, 0.10),
    (SpellId::BladeProtectionOther1, 0.10),
    (SpellId::PiercingProtectionOther1, 0.10),
]);

/// ClassicACE `SpellSelectionTable.spellSelectionGroup13`, as its Infiltration ruleset sets it (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP13: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::FocusOther1, 0.05),
    (SpellId::WillpowerOther1, 0.05),
    (SpellId::RejuvenationOther1, 0.05),
    (SpellId::RegenerationOther1, 0.05),
    (SpellId::ArmorOther1, 0.04),
    (SpellId::CreatureEnchantmentMasteryOther1, 0.04),
    (SpellId::ItemEnchantmentMasteryOther1, 0.04),
    (SpellId::LifeMagicMasteryOther1, 0.04),
    (SpellId::WarMagicMasteryOther1, 0.04),
    (SpellId::MagicResistanceOther1, 0.04),
    (SpellId::ManaRenewalOther1, 0.04),
    (SpellId::HealingMasteryOther1, 0.04),
    (SpellId::ArcaneEnlightenmentOther1, 0.04),
    (SpellId::FealtyOther1, 0.04),
    (SpellId::ManaMasteryOther1, 0.04),
    (SpellId::AlchemyMasteryOther1, 0.03),
    (SpellId::CookingMasteryOther1, 0.03),
    (SpellId::FletchingMasteryOther1, 0.03),
    (SpellId::LockpickMasteryOther1, 0.03),
    (SpellId::DeceptionMasteryOther1, 0.03),
    (SpellId::ArcanumSalvagingOther1, 0.03),
    (SpellId::ArmorExpertiseOther1, 0.03),
    (SpellId::MagicItemExpertiseOther1, 0.03),
    (SpellId::ItemExpertiseOther1, 0.03),
    (SpellId::WeaponExpertiseOther1, 0.03),
    (SpellId::MonsterAttunementOther1, 0.03),
    (SpellId::PersonAttunementOther1, 0.03),
]);

/// ClassicACE `SpellSelectionTable.spellSelectionGroup14`, as its Infiltration ruleset sets it (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP14: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::CoordinationOther1, 0.05),
    (SpellId::QuicknessOther1, 0.05),
    (SpellId::AlchemyMasteryOther1, 0.04),
    (SpellId::CookingMasteryOther1, 0.04),
    (SpellId::FletchingMasteryOther1, 0.04),
    (SpellId::HealingMasteryOther1, 0.04),
    (SpellId::LockpickMasteryOther1, 0.04),
    (SpellId::CreatureEnchantmentMasteryOther1, 0.04),
    (SpellId::ItemEnchantmentMasteryOther1, 0.04),
    (SpellId::LifeMagicMasteryOther1, 0.04),
    (SpellId::WarMagicMasteryOther1, 0.04),
    (SpellId::ManaMasteryOther1, 0.04),
    (SpellId::ArcaneEnlightenmentOther1, 0.04),
    (SpellId::ArcanumSalvagingOther1, 0.04),
    (SpellId::ArmorExpertiseOther1, 0.03),
    (SpellId::ItemExpertiseOther1, 0.03),
    (SpellId::MagicItemExpertiseOther1, 0.03),
    (SpellId::WeaponExpertiseOther1, 0.03),
    (SpellId::LightWeaponsMasteryOther1, 0.03),
    (SpellId::FinesseWeaponsMasteryOther1, 0.03),
    (SpellId::MaceMasteryOther1, 0.03),
    (SpellId::SpearMasteryOther1, 0.03),
    (SpellId::StaffMasteryOther1, 0.03),
    (SpellId::HeavyWeaponsMasteryOther1, 0.03),
    (SpellId::UnarmedCombatMasteryOther1, 0.03),
    (SpellId::MissileWeaponsMasteryOther1, 0.03),
    (SpellId::CrossbowMasteryOther1, 0.03),
    (SpellId::ThrownWeaponMasteryOther1, 0.03),
]);

/// ClassicACE `SpellSelectionTable.spellSelectionGroup15`, as its Infiltration ruleset sets it (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP15: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::StrengthOther1, 0.29),
    (SpellId::QuicknessOther1, 0.29),
    (SpellId::JumpingMasteryOther1, 0.14),
    (SpellId::SprintOther1, 0.14),
    (SpellId::EnduranceOther1, 0.14),
]);

/// ClassicACE `SpellSelectionTable.spellSelectionGroup16` (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP16: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::AlchemyMasteryOther1, 0.09),
    (SpellId::CookingMasteryOther1, 0.09),
    (SpellId::FletchingMasteryOther1, 0.09),
    (SpellId::LockpickMasteryOther1, 0.08),
    (SpellId::ArcanumSalvagingOther1, 0.08),
    (SpellId::ArmorExpertiseOther1, 0.08),
    (SpellId::ItemExpertiseOther1, 0.08),
    (SpellId::MagicItemExpertiseOther1, 0.08),
    (SpellId::WeaponExpertiseOther1, 0.08),
    (SpellId::WillpowerOther1, 0.05),
    (SpellId::StrengthOther1, 0.04),
    (SpellId::EnduranceOther1, 0.04),
    (SpellId::CoordinationOther1, 0.04),
    (SpellId::QuicknessOther1, 0.04),
    (SpellId::FocusOther1, 0.04),
]);

/// ClassicACE `SpellSelectionTable.spellSelectionGroup17`, as its Infiltration ruleset sets it (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP17: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::StrengthOther1, 0.25),
    (SpellId::EnduranceOther1, 0.25),
    (SpellId::CoordinationOther1, 0.25),
    (SpellId::QuicknessOther1, 0.25),
]);

/// ClassicACE `SpellSelectionTable.spellSelectionGroup18`, as its Infiltration ruleset sets it (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP18: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::StrengthOther1, 0.06),
    (SpellId::QuicknessOther1, 0.06),
    (SpellId::EnduranceOther1, 0.06),
    (SpellId::CoordinationOther1, 0.06),
    (SpellId::ImpregnabilityOther1, 0.05),
    (SpellId::InvulnerabilityOther1, 0.05),
    (SpellId::MagicResistanceOther1, 0.05),
    (SpellId::ArcaneEnlightenmentOther1, 0.05),
    (SpellId::ManaMasteryOther1, 0.04),
    (SpellId::HealingMasteryOther1, 0.04),
    (SpellId::JumpingMasteryOther1, 0.04),
    (SpellId::SprintOther1, 0.04),
    (SpellId::LightWeaponsMasteryOther1, 0.04),
    (SpellId::FinesseWeaponsMasteryOther1, 0.04),
    (SpellId::MaceMasteryOther1, 0.04),
    (SpellId::SpearMasteryOther1, 0.04),
    (SpellId::StaffMasteryOther1, 0.04),
    (SpellId::HeavyWeaponsMasteryOther1, 0.04),
    (SpellId::UnarmedCombatMasteryOther1, 0.04),
    (SpellId::MissileWeaponsMasteryOther1, 0.04),
    (SpellId::CrossbowMasteryOther1, 0.04),
    (SpellId::ThrownWeaponMasteryOther1, 0.04),
]);

/// ClassicACE `SpellSelectionTable.spellSelectionGroup19`, as its Infiltration ruleset sets it (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP19: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::WarMagicMasteryOther1, 0.28),
    (SpellId::WillpowerOther1, 0.18),
    (SpellId::ManaMasteryOther1, 0.10),
    (SpellId::LifeMagicMasteryOther1, 0.10),
    (SpellId::ArcaneEnlightenmentOther1, 0.10),
    (SpellId::FocusOther1, 0.09),
    (SpellId::CreatureEnchantmentMasteryOther1, 0.09),
    (SpellId::ItemEnchantmentMasteryOther1, 0.06),
]);

/// ClassicACE `SpellSelectionTable.spellSelectionGroup20`, as its Infiltration ruleset sets it (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP20: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::LockpickMasteryOther1, 0.08),
    (SpellId::CookingMasteryOther1, 0.06),
    (SpellId::FletchingMasteryOther1, 0.06),
    (SpellId::AlchemyMasteryOther1, 0.06),
    (SpellId::ItemEnchantmentMasteryOther1, 0.06),
    (SpellId::CreatureEnchantmentMasteryOther1, 0.05),
    (SpellId::ItemExpertiseOther1, 0.05),
    (SpellId::PersonAttunementOther1, 0.05),
    (SpellId::WeaponExpertiseOther1, 0.05),
    (SpellId::ArmorExpertiseOther1, 0.05),
    (SpellId::WarMagicMasteryOther1, 0.04),
    (SpellId::FealtyOther1, 0.04),
    (SpellId::ManaMasteryOther1, 0.04),
    (SpellId::WillpowerOther1, 0.03),
    (SpellId::FocusOther1, 0.03),
    (SpellId::RegenerationOther1, 0.03),
    (SpellId::ArcaneEnlightenmentOther1, 0.025),
    (SpellId::ArcanumSalvagingOther1, 0.025),
    (SpellId::DeceptionMasteryOther1, 0.025),
    (SpellId::MonsterAttunementOther1, 0.025),
    (SpellId::LifeMagicMasteryOther1, 0.02),
    (SpellId::HealingMasteryOther1, 0.02),
    (SpellId::MagicItemExpertiseOther1, 0.02),
    (SpellId::MagicResistanceOther1, 0.02),
    (SpellId::ManaRenewalOther1, 0.02),
    (SpellId::RejuvenationOther1, 0.02),
]);

/// ClassicACE `SpellSelectionTable.spellSelectionGroup21` (`ChanceTable<SpellId>`).
pub static SPELL_SELECTION_GROUP21: ChanceTable<SpellId> = ChanceTable::new(&[
]);

/// ClassicACE `SpellSelectionTable.spellSelectionGroup`, as its Infiltration ruleset sets it (`List<ChanceTable<SpellId>>`).
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
