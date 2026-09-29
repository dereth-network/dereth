// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/Properties/PropertyFloat.cs
// @generated from ACE's `Source/ACE.Entity/Enum/Properties/PropertyFloat.cs`; do not edit by hand

/// ACE enum `PropertyFloat`, underlying `ushort`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PropertyFloat(pub u16);

#[allow(non_upper_case_globals)]
impl PropertyFloat {
    pub const Undef: Self = Self(0);
    pub const HeartbeatInterval: Self = Self(1);
    pub const HeartbeatTimestamp: Self = Self(2);
    pub const HealthRate: Self = Self(3);
    pub const StaminaRate: Self = Self(4);
    pub const ManaRate: Self = Self(5);
    pub const HealthUponResurrection: Self = Self(6);
    pub const StaminaUponResurrection: Self = Self(7);
    pub const ManaUponResurrection: Self = Self(8);
    pub const StartTime: Self = Self(9);
    pub const StopTime: Self = Self(10);
    pub const ResetInterval: Self = Self(11);
    pub const Shade: Self = Self(12);
    pub const ArmorModVsSlash: Self = Self(13);
    pub const ArmorModVsPierce: Self = Self(14);
    pub const ArmorModVsBludgeon: Self = Self(15);
    pub const ArmorModVsCold: Self = Self(16);
    pub const ArmorModVsFire: Self = Self(17);
    pub const ArmorModVsAcid: Self = Self(18);
    pub const ArmorModVsElectric: Self = Self(19);
    pub const CombatSpeed: Self = Self(20);
    pub const WeaponLength: Self = Self(21);
    pub const DamageVariance: Self = Self(22);
    pub const CurrentPowerMod: Self = Self(23);
    pub const AccuracyMod: Self = Self(24);
    pub const StrengthMod: Self = Self(25);
    pub const MaximumVelocity: Self = Self(26);
    pub const RotationSpeed: Self = Self(27);
    pub const MotionTimestamp: Self = Self(28);
    pub const WeaponDefense: Self = Self(29);
    pub const WimpyLevel: Self = Self(30);
    pub const VisualAwarenessRange: Self = Self(31);
    pub const AuralAwarenessRange: Self = Self(32);
    pub const PerceptionLevel: Self = Self(33);
    pub const PowerupTime: Self = Self(34);
    pub const MaxChargeDistance: Self = Self(35);
    pub const ChargeSpeed: Self = Self(36);
    pub const BuyPrice: Self = Self(37);
    pub const SellPrice: Self = Self(38);
    pub const DefaultScale: Self = Self(39);
    pub const LockpickMod: Self = Self(40);
    pub const RegenerationInterval: Self = Self(41);
    pub const RegenerationTimestamp: Self = Self(42);
    pub const GeneratorRadius: Self = Self(43);
    pub const TimeToRot: Self = Self(44);
    pub const DeathTimestamp: Self = Self(45);
    pub const PkTimestamp: Self = Self(46);
    pub const VictimTimestamp: Self = Self(47);
    pub const LoginTimestamp: Self = Self(48);
    pub const CreationTimestamp: Self = Self(49);
    pub const MinimumTimeSincePk: Self = Self(50);
    pub const DeprecatedHousekeepingPriority: Self = Self(51);
    pub const AbuseLoggingTimestamp: Self = Self(52);
    pub const LastPortalTeleportTimestamp: Self = Self(53);
    pub const UseRadius: Self = Self(54);
    pub const HomeRadius: Self = Self(55);
    pub const ReleasedTimestamp: Self = Self(56);
    pub const MinHomeRadius: Self = Self(57);
    pub const Facing: Self = Self(58);
    pub const ResetTimestamp: Self = Self(59);
    pub const LogoffTimestamp: Self = Self(60);
    pub const EconRecoveryInterval: Self = Self(61);
    pub const WeaponOffense: Self = Self(62);
    pub const DamageMod: Self = Self(63);
    pub const ResistSlash: Self = Self(64);
    pub const ResistPierce: Self = Self(65);
    pub const ResistBludgeon: Self = Self(66);
    pub const ResistFire: Self = Self(67);
    pub const ResistCold: Self = Self(68);
    pub const ResistAcid: Self = Self(69);
    pub const ResistElectric: Self = Self(70);
    pub const ResistHealthBoost: Self = Self(71);
    pub const ResistStaminaDrain: Self = Self(72);
    pub const ResistStaminaBoost: Self = Self(73);
    pub const ResistManaDrain: Self = Self(74);
    pub const ResistManaBoost: Self = Self(75);
    pub const Translucency: Self = Self(76);
    pub const PhysicsScriptIntensity: Self = Self(77);
    pub const Friction: Self = Self(78);
    pub const Elasticity: Self = Self(79);
    pub const AiUseMagicDelay: Self = Self(80);
    pub const ItemMinSpellcraftMod: Self = Self(81);
    pub const ItemMaxSpellcraftMod: Self = Self(82);
    pub const ItemRankProbability: Self = Self(83);
    pub const Shade2: Self = Self(84);
    pub const Shade3: Self = Self(85);
    pub const Shade4: Self = Self(86);
    pub const ItemEfficiency: Self = Self(87);
    pub const ItemManaUpdateTimestamp: Self = Self(88);
    pub const SpellGestureSpeedMod: Self = Self(89);
    pub const SpellStanceSpeedMod: Self = Self(90);
    pub const AllegianceAppraisalTimestamp: Self = Self(91);
    pub const PowerLevel: Self = Self(92);
    pub const AccuracyLevel: Self = Self(93);
    pub const AttackAngle: Self = Self(94);
    pub const AttackTimestamp: Self = Self(95);
    pub const CheckpointTimestamp: Self = Self(96);
    pub const SoldTimestamp: Self = Self(97);
    pub const UseTimestamp: Self = Self(98);
    pub const UseLockTimestamp: Self = Self(99);
    pub const HealkitMod: Self = Self(100);
    pub const FrozenTimestamp: Self = Self(101);
    pub const HealthRateMod: Self = Self(102);
    pub const AllegianceSwearTimestamp: Self = Self(103);
    pub const ObviousRadarRange: Self = Self(104);
    pub const HotspotCycleTime: Self = Self(105);
    pub const HotspotCycleTimeVariance: Self = Self(106);
    pub const SpamTimestamp: Self = Self(107);
    pub const SpamRate: Self = Self(108);
    pub const BondWieldedTreasure: Self = Self(109);
    pub const BulkMod: Self = Self(110);
    pub const SizeMod: Self = Self(111);
    pub const GagTimestamp: Self = Self(112);
    pub const GeneratorUpdateTimestamp: Self = Self(113);
    pub const DeathSpamTimestamp: Self = Self(114);
    pub const DeathSpamRate: Self = Self(115);
    pub const WildAttackProbability: Self = Self(116);
    pub const FocusedProbability: Self = Self(117);
    pub const CrashAndTurnProbability: Self = Self(118);
    pub const CrashAndTurnRadius: Self = Self(119);
    pub const CrashAndTurnBias: Self = Self(120);
    pub const GeneratorInitialDelay: Self = Self(121);
    pub const AiAcquireHealth: Self = Self(122);
    pub const AiAcquireStamina: Self = Self(123);
    pub const AiAcquireMana: Self = Self(124);
    pub const ResistHealthDrain: Self = Self(125);
    pub const LifestoneProtectionTimestamp: Self = Self(126);
    pub const AiCounteractEnchantment: Self = Self(127);
    pub const AiDispelEnchantment: Self = Self(128);
    pub const TradeTimestamp: Self = Self(129);
    pub const AiTargetedDetectionRadius: Self = Self(130);
    pub const EmotePriority: Self = Self(131);
    pub const LastTeleportStartTimestamp: Self = Self(132);
    pub const EventSpamTimestamp: Self = Self(133);
    pub const EventSpamRate: Self = Self(134);
    pub const InventoryOffset: Self = Self(135);
    pub const CriticalMultiplier: Self = Self(136);
    pub const ManaStoneDestroyChance: Self = Self(137);
    pub const SlayerDamageBonus: Self = Self(138);
    pub const AllegianceInfoSpamTimestamp: Self = Self(139);
    pub const AllegianceInfoSpamRate: Self = Self(140);
    pub const NextSpellcastTimestamp: Self = Self(141);
    pub const AppraisalRequestedTimestamp: Self = Self(142);
    pub const AppraisalHeartbeatDueTimestamp: Self = Self(143);
    pub const ManaConversionMod: Self = Self(144);
    pub const LastPkAttackTimestamp: Self = Self(145);
    pub const FellowshipUpdateTimestamp: Self = Self(146);
    pub const CriticalFrequency: Self = Self(147);
    pub const LimboStartTimestamp: Self = Self(148);
    pub const WeaponMissileDefense: Self = Self(149);
    pub const WeaponMagicDefense: Self = Self(150);
    pub const IgnoreShield: Self = Self(151);
    pub const ElementalDamageMod: Self = Self(152);
    pub const StartMissileAttackTimestamp: Self = Self(153);
    pub const LastRareUsedTimestamp: Self = Self(154);
    pub const IgnoreArmor: Self = Self(155);
    pub const ProcSpellRate: Self = Self(156);
    pub const ResistanceModifier: Self = Self(157);
    pub const AllegianceGagTimestamp: Self = Self(158);
    pub const AbsorbMagicDamage: Self = Self(159);
    pub const CachedMaxAbsorbMagicDamage: Self = Self(160);
    pub const GagDuration: Self = Self(161);
    pub const AllegianceGagDuration: Self = Self(162);
    pub const GlobalXpMod: Self = Self(163);
    pub const HealingModifier: Self = Self(164);
    pub const ArmorModVsNether: Self = Self(165);
    pub const ResistNether: Self = Self(166);
    pub const CooldownDuration: Self = Self(167);
    pub const WeaponAuraOffense: Self = Self(168);
    pub const WeaponAuraDefense: Self = Self(169);
    pub const WeaponAuraElemental: Self = Self(170);
    pub const WeaponAuraManaConv: Self = Self(171);
    pub const PCAPRecordedWorkmanship: Self = Self(8004);
    pub const PCAPRecordedVelocityX: Self = Self(8010);
    pub const PCAPRecordedVelocityY: Self = Self(8011);
    pub const PCAPRecordedVelocityZ: Self = Self(8012);
    pub const PCAPRecordedAccelerationX: Self = Self(8013);
    pub const PCAPRecordedAccelerationY: Self = Self(8014);
    pub const PCAPRecordedAccelerationZ: Self = Self(8015);
    pub const PCAPRecordeOmegaX: Self = Self(8016);
    pub const PCAPRecordeOmegaY: Self = Self(8017);
    pub const PCAPRecordeOmegaZ: Self = Self(8018);
}

impl PropertyFloat {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::HeartbeatInterval, Self::HeartbeatTimestamp, Self::HealthRate, Self::StaminaRate, Self::ManaRate, Self::HealthUponResurrection, Self::StaminaUponResurrection, Self::ManaUponResurrection, Self::StartTime, Self::StopTime, Self::ResetInterval, Self::Shade, Self::ArmorModVsSlash, Self::ArmorModVsPierce, Self::ArmorModVsBludgeon, Self::ArmorModVsCold, Self::ArmorModVsFire, Self::ArmorModVsAcid, Self::ArmorModVsElectric, Self::CombatSpeed, Self::WeaponLength, Self::DamageVariance, Self::CurrentPowerMod, Self::AccuracyMod, Self::StrengthMod, Self::MaximumVelocity, Self::RotationSpeed, Self::MotionTimestamp, Self::WeaponDefense, Self::WimpyLevel, Self::VisualAwarenessRange, Self::AuralAwarenessRange, Self::PerceptionLevel, Self::PowerupTime, Self::MaxChargeDistance, Self::ChargeSpeed, Self::BuyPrice, Self::SellPrice, Self::DefaultScale, Self::LockpickMod, Self::RegenerationInterval, Self::RegenerationTimestamp, Self::GeneratorRadius, Self::TimeToRot, Self::DeathTimestamp, Self::PkTimestamp, Self::VictimTimestamp, Self::LoginTimestamp, Self::CreationTimestamp, Self::MinimumTimeSincePk, Self::DeprecatedHousekeepingPriority, Self::AbuseLoggingTimestamp, Self::LastPortalTeleportTimestamp, Self::UseRadius, Self::HomeRadius, Self::ReleasedTimestamp, Self::MinHomeRadius, Self::Facing, Self::ResetTimestamp, Self::LogoffTimestamp, Self::EconRecoveryInterval, Self::WeaponOffense, Self::DamageMod, Self::ResistSlash, Self::ResistPierce, Self::ResistBludgeon, Self::ResistFire, Self::ResistCold, Self::ResistAcid, Self::ResistElectric, Self::ResistHealthBoost, Self::ResistStaminaDrain, Self::ResistStaminaBoost, Self::ResistManaDrain, Self::ResistManaBoost, Self::Translucency, Self::PhysicsScriptIntensity, Self::Friction, Self::Elasticity, Self::AiUseMagicDelay, Self::ItemMinSpellcraftMod, Self::ItemMaxSpellcraftMod, Self::ItemRankProbability, Self::Shade2, Self::Shade3, Self::Shade4, Self::ItemEfficiency, Self::ItemManaUpdateTimestamp, Self::SpellGestureSpeedMod, Self::SpellStanceSpeedMod, Self::AllegianceAppraisalTimestamp, Self::PowerLevel, Self::AccuracyLevel, Self::AttackAngle, Self::AttackTimestamp, Self::CheckpointTimestamp, Self::SoldTimestamp, Self::UseTimestamp, Self::UseLockTimestamp, Self::HealkitMod, Self::FrozenTimestamp, Self::HealthRateMod, Self::AllegianceSwearTimestamp, Self::ObviousRadarRange, Self::HotspotCycleTime, Self::HotspotCycleTimeVariance, Self::SpamTimestamp, Self::SpamRate, Self::BondWieldedTreasure, Self::BulkMod, Self::SizeMod, Self::GagTimestamp, Self::GeneratorUpdateTimestamp, Self::DeathSpamTimestamp, Self::DeathSpamRate, Self::WildAttackProbability, Self::FocusedProbability, Self::CrashAndTurnProbability, Self::CrashAndTurnRadius, Self::CrashAndTurnBias, Self::GeneratorInitialDelay, Self::AiAcquireHealth, Self::AiAcquireStamina, Self::AiAcquireMana, Self::ResistHealthDrain, Self::LifestoneProtectionTimestamp, Self::AiCounteractEnchantment, Self::AiDispelEnchantment, Self::TradeTimestamp, Self::AiTargetedDetectionRadius, Self::EmotePriority, Self::LastTeleportStartTimestamp, Self::EventSpamTimestamp, Self::EventSpamRate, Self::InventoryOffset, Self::CriticalMultiplier, Self::ManaStoneDestroyChance, Self::SlayerDamageBonus, Self::AllegianceInfoSpamTimestamp, Self::AllegianceInfoSpamRate, Self::NextSpellcastTimestamp, Self::AppraisalRequestedTimestamp, Self::AppraisalHeartbeatDueTimestamp, Self::ManaConversionMod, Self::LastPkAttackTimestamp, Self::FellowshipUpdateTimestamp, Self::CriticalFrequency, Self::LimboStartTimestamp, Self::WeaponMissileDefense, Self::WeaponMagicDefense, Self::IgnoreShield, Self::ElementalDamageMod, Self::StartMissileAttackTimestamp, Self::LastRareUsedTimestamp, Self::IgnoreArmor, Self::ProcSpellRate, Self::ResistanceModifier, Self::AllegianceGagTimestamp, Self::AbsorbMagicDamage, Self::CachedMaxAbsorbMagicDamage, Self::GagDuration, Self::AllegianceGagDuration, Self::GlobalXpMod, Self::HealingModifier, Self::ArmorModVsNether, Self::ResistNether, Self::CooldownDuration, Self::WeaponAuraOffense, Self::WeaponAuraDefense, Self::WeaponAuraElemental, Self::WeaponAuraManaConv, Self::PCAPRecordedWorkmanship, Self::PCAPRecordedVelocityX, Self::PCAPRecordedVelocityY, Self::PCAPRecordedVelocityZ, Self::PCAPRecordedAccelerationX, Self::PCAPRecordedAccelerationY, Self::PCAPRecordedAccelerationZ, Self::PCAPRecordeOmegaX, Self::PCAPRecordeOmegaY, Self::PCAPRecordeOmegaZ];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "HeartbeatInterval", "HeartbeatTimestamp", "HealthRate", "StaminaRate", "ManaRate", "HealthUponResurrection", "StaminaUponResurrection", "ManaUponResurrection", "StartTime", "StopTime", "ResetInterval", "Shade", "ArmorModVsSlash", "ArmorModVsPierce", "ArmorModVsBludgeon", "ArmorModVsCold", "ArmorModVsFire", "ArmorModVsAcid", "ArmorModVsElectric", "CombatSpeed", "WeaponLength", "DamageVariance", "CurrentPowerMod", "AccuracyMod", "StrengthMod", "MaximumVelocity", "RotationSpeed", "MotionTimestamp", "WeaponDefense", "WimpyLevel", "VisualAwarenessRange", "AuralAwarenessRange", "PerceptionLevel", "PowerupTime", "MaxChargeDistance", "ChargeSpeed", "BuyPrice", "SellPrice", "DefaultScale", "LockpickMod", "RegenerationInterval", "RegenerationTimestamp", "GeneratorRadius", "TimeToRot", "DeathTimestamp", "PkTimestamp", "VictimTimestamp", "LoginTimestamp", "CreationTimestamp", "MinimumTimeSincePk", "DeprecatedHousekeepingPriority", "AbuseLoggingTimestamp", "LastPortalTeleportTimestamp", "UseRadius", "HomeRadius", "ReleasedTimestamp", "MinHomeRadius", "Facing", "ResetTimestamp", "LogoffTimestamp", "EconRecoveryInterval", "WeaponOffense", "DamageMod", "ResistSlash", "ResistPierce", "ResistBludgeon", "ResistFire", "ResistCold", "ResistAcid", "ResistElectric", "ResistHealthBoost", "ResistStaminaDrain", "ResistStaminaBoost", "ResistManaDrain", "ResistManaBoost", "Translucency", "PhysicsScriptIntensity", "Friction", "Elasticity", "AiUseMagicDelay", "ItemMinSpellcraftMod", "ItemMaxSpellcraftMod", "ItemRankProbability", "Shade2", "Shade3", "Shade4", "ItemEfficiency", "ItemManaUpdateTimestamp", "SpellGestureSpeedMod", "SpellStanceSpeedMod", "AllegianceAppraisalTimestamp", "PowerLevel", "AccuracyLevel", "AttackAngle", "AttackTimestamp", "CheckpointTimestamp", "SoldTimestamp", "UseTimestamp", "UseLockTimestamp", "HealkitMod", "FrozenTimestamp", "HealthRateMod", "AllegianceSwearTimestamp", "ObviousRadarRange", "HotspotCycleTime", "HotspotCycleTimeVariance", "SpamTimestamp", "SpamRate", "BondWieldedTreasure", "BulkMod", "SizeMod", "GagTimestamp", "GeneratorUpdateTimestamp", "DeathSpamTimestamp", "DeathSpamRate", "WildAttackProbability", "FocusedProbability", "CrashAndTurnProbability", "CrashAndTurnRadius", "CrashAndTurnBias", "GeneratorInitialDelay", "AiAcquireHealth", "AiAcquireStamina", "AiAcquireMana", "ResistHealthDrain", "LifestoneProtectionTimestamp", "AiCounteractEnchantment", "AiDispelEnchantment", "TradeTimestamp", "AiTargetedDetectionRadius", "EmotePriority", "LastTeleportStartTimestamp", "EventSpamTimestamp", "EventSpamRate", "InventoryOffset", "CriticalMultiplier", "ManaStoneDestroyChance", "SlayerDamageBonus", "AllegianceInfoSpamTimestamp", "AllegianceInfoSpamRate", "NextSpellcastTimestamp", "AppraisalRequestedTimestamp", "AppraisalHeartbeatDueTimestamp", "ManaConversionMod", "LastPkAttackTimestamp", "FellowshipUpdateTimestamp", "CriticalFrequency", "LimboStartTimestamp", "WeaponMissileDefense", "WeaponMagicDefense", "IgnoreShield", "ElementalDamageMod", "StartMissileAttackTimestamp", "LastRareUsedTimestamp", "IgnoreArmor", "ProcSpellRate", "ResistanceModifier", "AllegianceGagTimestamp", "AbsorbMagicDamage", "CachedMaxAbsorbMagicDamage", "GagDuration", "AllegianceGagDuration", "GlobalXpMod", "HealingModifier", "ArmorModVsNether", "ResistNether", "CooldownDuration", "WeaponAuraOffense", "WeaponAuraDefense", "WeaponAuraElemental", "WeaponAuraManaConv", "PCAPRecordedWorkmanship", "PCAPRecordedVelocityX", "PCAPRecordedVelocityY", "PCAPRecordedVelocityZ", "PCAPRecordedAccelerationX", "PCAPRecordedAccelerationY", "PCAPRecordedAccelerationZ", "PCAPRecordeOmegaX", "PCAPRecordeOmegaY", "PCAPRecordeOmegaZ"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[159, 52, 93, 24, 122, 124, 123, 127, 128, 130, 80, 91, 162, 158, 140, 139, 103, 143, 142, 18, 15, 16, 19, 17, 165, 14, 13, 94, 95, 32, 109, 110, 37, 160, 36, 96, 20, 167, 120, 118, 119, 49, 147, 136, 23, 63, 22, 115, 114, 45, 39, 51, 61, 79, 152, 131, 134, 133, 58, 146, 117, 78, 101, 161, 112, 121, 43, 113, 163, 164, 100, 3, 102, 6, 1, 2, 55, 105, 106, 155, 151, 135, 87, 88, 82, 81, 83, 145, 53, 154, 132, 126, 148, 40, 48, 60, 144, 5, 137, 8, 35, 26, 57, 50, 28, 141, 104, 179, 180, 181, 176, 177, 178, 173, 174, 175, 172, 33, 77, 46, 92, 34, 156, 41, 42, 56, 11, 59, 69, 66, 68, 70, 67, 71, 125, 75, 74, 166, 65, 64, 73, 72, 157, 27, 38, 12, 84, 85, 86, 111, 138, 97, 108, 107, 89, 90, 4, 7, 153, 9, 10, 25, 44, 129, 76, 0, 99, 54, 98, 47, 31, 169, 170, 171, 168, 29, 21, 150, 149, 62, 116, 30];

    /// The members marked `[AssessmentProperty]`, in declaration order (ACE's reflection order).
    pub const ASSESSMENT_PROPERTY: &'static [Self] = &[Self::ManaRate, Self::WeaponDefense, Self::ItemEfficiency, Self::HealkitMod, Self::CriticalMultiplier, Self::ManaStoneDestroyChance, Self::ManaConversionMod, Self::CriticalFrequency, Self::WeaponMissileDefense, Self::WeaponMagicDefense, Self::ElementalDamageMod, Self::IgnoreArmor, Self::ResistanceModifier, Self::AbsorbMagicDamage, Self::CooldownDuration];
    /// Whether this value is marked `[AssessmentProperty]` (by value, like ACE's `HashSet` lookups).
    pub fn is_assessment_property(self) -> bool {
        Self::ASSESSMENT_PROPERTY.contains(&self)
    }

    /// The members marked `[Ephemeral]`, in declaration order (ACE's reflection order).
    pub const EPHEMERAL: &'static [Self] = &[Self::HeartbeatTimestamp, Self::LastPortalTeleportTimestamp, Self::ResetTimestamp, Self::UseLockTimestamp, Self::LastTeleportStartTimestamp, Self::AppraisalRequestedTimestamp];
    /// Whether this value is marked `[Ephemeral]` (by value, like ACE's `HashSet` lookups).
    pub fn is_ephemeral(self) -> bool {
        Self::EPHEMERAL.contains(&self)
    }

    /// The members marked `[SendOnLogin]`, in declaration order (ACE's reflection order).
    pub const SEND_ON_LOGIN: &'static [Self] = &[Self::ResistHealthDrain, Self::GlobalXpMod, Self::WeaponAuraOffense, Self::WeaponAuraDefense, Self::WeaponAuraElemental, Self::WeaponAuraManaConv];
    /// Whether this value is marked `[SendOnLogin]` (by value, like ACE's `HashSet` lookups).
    pub fn is_send_on_login(self) -> bool {
        Self::SEND_ON_LOGIN.contains(&self)
    }
}

super::support::ace_enum!(PropertyFloat, u16, plain);
super::support::ace_enum_from!(PropertyFloat, u16 => u32, u64, i32, i64);
