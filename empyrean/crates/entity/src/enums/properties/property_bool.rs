// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/Properties/PropertyBool.cs
// @generated from ACE's `Source/ACE.Entity/Enum/Properties/PropertyBool.cs`; do not edit by hand

/// ACE enum `PropertyBool`, underlying `ushort`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PropertyBool(pub u16);

#[allow(non_upper_case_globals)]
impl PropertyBool {
    pub const Undef: Self = Self(0);
    pub const Stuck: Self = Self(1);
    pub const Open: Self = Self(2);
    pub const Locked: Self = Self(3);
    pub const RotProof: Self = Self(4);
    pub const AllegianceUpdateRequest: Self = Self(5);
    pub const AiUsesMana: Self = Self(6);
    pub const AiUseHumanMagicAnimations: Self = Self(7);
    pub const AllowGive: Self = Self(8);
    pub const CurrentlyAttacking: Self = Self(9);
    pub const AttackerAi: Self = Self(10);
    pub const IgnoreCollisions: Self = Self(11);
    pub const ReportCollisions: Self = Self(12);
    pub const Ethereal: Self = Self(13);
    pub const GravityStatus: Self = Self(14);
    pub const LightsStatus: Self = Self(15);
    pub const ScriptedCollision: Self = Self(16);
    pub const Inelastic: Self = Self(17);
    pub const Visibility: Self = Self(18);
    pub const Attackable: Self = Self(19);
    pub const SafeSpellComponents: Self = Self(20);
    pub const AdvocateState: Self = Self(21);
    pub const Inscribable: Self = Self(22);
    pub const DestroyOnSell: Self = Self(23);
    pub const UiHidden: Self = Self(24);
    pub const IgnoreHouseBarriers: Self = Self(25);
    pub const HiddenAdmin: Self = Self(26);
    pub const PkWounder: Self = Self(27);
    pub const PkKiller: Self = Self(28);
    pub const NoCorpse: Self = Self(29);
    pub const UnderLifestoneProtection: Self = Self(30);
    pub const ItemManaUpdatePending: Self = Self(31);
    pub const GeneratorStatus: Self = Self(32);
    pub const ResetMessagePending: Self = Self(33);
    pub const DefaultOpen: Self = Self(34);
    pub const DefaultLocked: Self = Self(35);
    pub const DefaultOn: Self = Self(36);
    pub const OpenForBusiness: Self = Self(37);
    pub const IsFrozen: Self = Self(38);
    pub const DealMagicalItems: Self = Self(39);
    pub const LogoffImDead: Self = Self(40);
    pub const ReportCollisionsAsEnvironment: Self = Self(41);
    pub const AllowEdgeSlide: Self = Self(42);
    pub const AdvocateQuest: Self = Self(43);
    pub const IsAdmin: Self = Self(44);
    pub const IsArch: Self = Self(45);
    pub const IsSentinel: Self = Self(46);
    pub const IsAdvocate: Self = Self(47);
    pub const CurrentlyPoweringUp: Self = Self(48);
    pub const GeneratorEnteredWorld: Self = Self(49);
    pub const NeverFailCasting: Self = Self(50);
    pub const VendorService: Self = Self(51);
    pub const AiImmobile: Self = Self(52);
    pub const DamagedByCollisions: Self = Self(53);
    pub const IsDynamic: Self = Self(54);
    pub const IsHot: Self = Self(55);
    pub const IsAffecting: Self = Self(56);
    pub const AffectsAis: Self = Self(57);
    pub const SpellQueueActive: Self = Self(58);
    pub const GeneratorDisabled: Self = Self(59);
    pub const IsAcceptingTells: Self = Self(60);
    pub const LoggingChannel: Self = Self(61);
    pub const OpensAnyLock: Self = Self(62);
    pub const UnlimitedUse: Self = Self(63);
    pub const GeneratedTreasureItem: Self = Self(64);
    pub const IgnoreMagicResist: Self = Self(65);
    pub const IgnoreMagicArmor: Self = Self(66);
    pub const AiAllowTrade: Self = Self(67);
    pub const SpellComponentsRequired: Self = Self(68);
    pub const IsSellable: Self = Self(69);
    pub const IgnoreShieldsBySkill: Self = Self(70);
    pub const NoDraw: Self = Self(71);
    pub const ActivationUntargeted: Self = Self(72);
    pub const HouseHasGottenPriorityBootPos: Self = Self(73);
    pub const GeneratorAutomaticDestruction: Self = Self(74);
    pub const HouseHooksVisible: Self = Self(75);
    pub const HouseRequiresMonarch: Self = Self(76);
    pub const HouseHooksEnabled: Self = Self(77);
    pub const HouseNotifiedHudOfHookCount: Self = Self(78);
    pub const AiAcceptEverything: Self = Self(79);
    pub const IgnorePortalRestrictions: Self = Self(80);
    pub const RequiresBackpackSlot: Self = Self(81);
    pub const DontTurnOrMoveWhenGiving: Self = Self(82);
    pub const NpcLooksLikeObject: Self = Self(83);
    pub const IgnoreCloIcons: Self = Self(84);
    pub const AppraisalHasAllowedWielder: Self = Self(85);
    pub const ChestRegenOnClose: Self = Self(86);
    pub const LogoffInMinigame: Self = Self(87);
    pub const PortalShowDestination: Self = Self(88);
    pub const PortalIgnoresPkAttackTimer: Self = Self(89);
    pub const NpcInteractsSilently: Self = Self(90);
    pub const Retained: Self = Self(91);
    pub const IgnoreAuthor: Self = Self(92);
    pub const Limbo: Self = Self(93);
    pub const AppraisalHasAllowedActivator: Self = Self(94);
    pub const ExistedBeforeAllegianceXpChanges: Self = Self(95);
    pub const IsDeaf: Self = Self(96);
    pub const IsPsr: Self = Self(97);
    pub const Invincible: Self = Self(98);
    pub const Ivoryable: Self = Self(99);
    pub const Dyable: Self = Self(100);
    pub const CanGenerateRare: Self = Self(101);
    pub const CorpseGeneratedRare: Self = Self(102);
    pub const NonProjectileMagicImmune: Self = Self(103);
    pub const ActdReceivedItems: Self = Self(104);
    pub const Unknown105: Self = Self(105);
    pub const FirstEnterWorldDone: Self = Self(106);
    pub const RecallsDisabled: Self = Self(107);
    pub const RareUsesTimer: Self = Self(108);
    pub const ActdPreorderReceivedItems: Self = Self(109);
    pub const Afk: Self = Self(110);
    pub const IsGagged: Self = Self(111);
    pub const ProcSpellSelfTargeted: Self = Self(112);
    pub const IsAllegianceGagged: Self = Self(113);
    pub const EquipmentSetTriggerPiece: Self = Self(114);
    pub const Uninscribe: Self = Self(115);
    pub const WieldOnUse: Self = Self(116);
    pub const ChestClearedWhenClosed: Self = Self(117);
    pub const NeverAttack: Self = Self(118);
    pub const SuppressGenerateEffect: Self = Self(119);
    pub const TreasureCorpse: Self = Self(120);
    pub const EquipmentSetAddLevel: Self = Self(121);
    pub const BarberActive: Self = Self(122);
    pub const TopLayerPriority: Self = Self(123);
    pub const NoHeldItemShown: Self = Self(124);
    pub const LoginAtLifestone: Self = Self(125);
    pub const OlthoiPk: Self = Self(126);
    pub const Account15Days: Self = Self(127);
    pub const HadNoVitae: Self = Self(128);
    pub const NoOlthoiTalk: Self = Self(129);
    pub const AutowieldLeft: Self = Self(130);
    pub const LinkedPortalOneSummon: Self = Self(9001);
    pub const LinkedPortalTwoSummon: Self = Self(9002);
    pub const HouseEvicted: Self = Self(9003);
    pub const UntrainedSkills: Self = Self(9004);
    pub const IsEnvoy: Self = Self(9005);
    pub const UnspecializedSkills: Self = Self(9006);
    pub const FreeSkillResetRenewed: Self = Self(9007);
    pub const FreeAttributeResetRenewed: Self = Self(9008);
    pub const SkillTemplesTimerReset: Self = Self(9009);
    pub const FreeMasteryResetRenewed: Self = Self(9010);
}

impl PropertyBool {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Stuck, Self::Open, Self::Locked, Self::RotProof, Self::AllegianceUpdateRequest, Self::AiUsesMana, Self::AiUseHumanMagicAnimations, Self::AllowGive, Self::CurrentlyAttacking, Self::AttackerAi, Self::IgnoreCollisions, Self::ReportCollisions, Self::Ethereal, Self::GravityStatus, Self::LightsStatus, Self::ScriptedCollision, Self::Inelastic, Self::Visibility, Self::Attackable, Self::SafeSpellComponents, Self::AdvocateState, Self::Inscribable, Self::DestroyOnSell, Self::UiHidden, Self::IgnoreHouseBarriers, Self::HiddenAdmin, Self::PkWounder, Self::PkKiller, Self::NoCorpse, Self::UnderLifestoneProtection, Self::ItemManaUpdatePending, Self::GeneratorStatus, Self::ResetMessagePending, Self::DefaultOpen, Self::DefaultLocked, Self::DefaultOn, Self::OpenForBusiness, Self::IsFrozen, Self::DealMagicalItems, Self::LogoffImDead, Self::ReportCollisionsAsEnvironment, Self::AllowEdgeSlide, Self::AdvocateQuest, Self::IsAdmin, Self::IsArch, Self::IsSentinel, Self::IsAdvocate, Self::CurrentlyPoweringUp, Self::GeneratorEnteredWorld, Self::NeverFailCasting, Self::VendorService, Self::AiImmobile, Self::DamagedByCollisions, Self::IsDynamic, Self::IsHot, Self::IsAffecting, Self::AffectsAis, Self::SpellQueueActive, Self::GeneratorDisabled, Self::IsAcceptingTells, Self::LoggingChannel, Self::OpensAnyLock, Self::UnlimitedUse, Self::GeneratedTreasureItem, Self::IgnoreMagicResist, Self::IgnoreMagicArmor, Self::AiAllowTrade, Self::SpellComponentsRequired, Self::IsSellable, Self::IgnoreShieldsBySkill, Self::NoDraw, Self::ActivationUntargeted, Self::HouseHasGottenPriorityBootPos, Self::GeneratorAutomaticDestruction, Self::HouseHooksVisible, Self::HouseRequiresMonarch, Self::HouseHooksEnabled, Self::HouseNotifiedHudOfHookCount, Self::AiAcceptEverything, Self::IgnorePortalRestrictions, Self::RequiresBackpackSlot, Self::DontTurnOrMoveWhenGiving, Self::NpcLooksLikeObject, Self::IgnoreCloIcons, Self::AppraisalHasAllowedWielder, Self::ChestRegenOnClose, Self::LogoffInMinigame, Self::PortalShowDestination, Self::PortalIgnoresPkAttackTimer, Self::NpcInteractsSilently, Self::Retained, Self::IgnoreAuthor, Self::Limbo, Self::AppraisalHasAllowedActivator, Self::ExistedBeforeAllegianceXpChanges, Self::IsDeaf, Self::IsPsr, Self::Invincible, Self::Ivoryable, Self::Dyable, Self::CanGenerateRare, Self::CorpseGeneratedRare, Self::NonProjectileMagicImmune, Self::ActdReceivedItems, Self::Unknown105, Self::FirstEnterWorldDone, Self::RecallsDisabled, Self::RareUsesTimer, Self::ActdPreorderReceivedItems, Self::Afk, Self::IsGagged, Self::ProcSpellSelfTargeted, Self::IsAllegianceGagged, Self::EquipmentSetTriggerPiece, Self::Uninscribe, Self::WieldOnUse, Self::ChestClearedWhenClosed, Self::NeverAttack, Self::SuppressGenerateEffect, Self::TreasureCorpse, Self::EquipmentSetAddLevel, Self::BarberActive, Self::TopLayerPriority, Self::NoHeldItemShown, Self::LoginAtLifestone, Self::OlthoiPk, Self::Account15Days, Self::HadNoVitae, Self::NoOlthoiTalk, Self::AutowieldLeft, Self::LinkedPortalOneSummon, Self::LinkedPortalTwoSummon, Self::HouseEvicted, Self::UntrainedSkills, Self::IsEnvoy, Self::UnspecializedSkills, Self::FreeSkillResetRenewed, Self::FreeAttributeResetRenewed, Self::SkillTemplesTimerReset, Self::FreeMasteryResetRenewed];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Stuck", "Open", "Locked", "RotProof", "AllegianceUpdateRequest", "AiUsesMana", "AiUseHumanMagicAnimations", "AllowGive", "CurrentlyAttacking", "AttackerAi", "IgnoreCollisions", "ReportCollisions", "Ethereal", "GravityStatus", "LightsStatus", "ScriptedCollision", "Inelastic", "Visibility", "Attackable", "SafeSpellComponents", "AdvocateState", "Inscribable", "DestroyOnSell", "UiHidden", "IgnoreHouseBarriers", "HiddenAdmin", "PkWounder", "PkKiller", "NoCorpse", "UnderLifestoneProtection", "ItemManaUpdatePending", "GeneratorStatus", "ResetMessagePending", "DefaultOpen", "DefaultLocked", "DefaultOn", "OpenForBusiness", "IsFrozen", "DealMagicalItems", "LogoffImDead", "ReportCollisionsAsEnvironment", "AllowEdgeSlide", "AdvocateQuest", "IsAdmin", "IsArch", "IsSentinel", "IsAdvocate", "CurrentlyPoweringUp", "GeneratorEnteredWorld", "NeverFailCasting", "VendorService", "AiImmobile", "DamagedByCollisions", "IsDynamic", "IsHot", "IsAffecting", "AffectsAis", "SpellQueueActive", "GeneratorDisabled", "IsAcceptingTells", "LoggingChannel", "OpensAnyLock", "UnlimitedUse", "GeneratedTreasureItem", "IgnoreMagicResist", "IgnoreMagicArmor", "AiAllowTrade", "SpellComponentsRequired", "IsSellable", "IgnoreShieldsBySkill", "NoDraw", "ActivationUntargeted", "HouseHasGottenPriorityBootPos", "GeneratorAutomaticDestruction", "HouseHooksVisible", "HouseRequiresMonarch", "HouseHooksEnabled", "HouseNotifiedHudOfHookCount", "AiAcceptEverything", "IgnorePortalRestrictions", "RequiresBackpackSlot", "DontTurnOrMoveWhenGiving", "NpcLooksLikeObject", "IgnoreCloIcons", "AppraisalHasAllowedWielder", "ChestRegenOnClose", "LogoffInMinigame", "PortalShowDestination", "PortalIgnoresPkAttackTimer", "NpcInteractsSilently", "Retained", "IgnoreAuthor", "Limbo", "AppraisalHasAllowedActivator", "ExistedBeforeAllegianceXpChanges", "IsDeaf", "IsPsr", "Invincible", "Ivoryable", "Dyable", "CanGenerateRare", "CorpseGeneratedRare", "NonProjectileMagicImmune", "ActdReceivedItems", "Unknown105", "FirstEnterWorldDone", "RecallsDisabled", "RareUsesTimer", "ActdPreorderReceivedItems", "Afk", "IsGagged", "ProcSpellSelfTargeted", "IsAllegianceGagged", "EquipmentSetTriggerPiece", "Uninscribe", "WieldOnUse", "ChestClearedWhenClosed", "NeverAttack", "SuppressGenerateEffect", "TreasureCorpse", "EquipmentSetAddLevel", "BarberActive", "TopLayerPriority", "NoHeldItemShown", "LoginAtLifestone", "OlthoiPk", "Account15Days", "HadNoVitae", "NoOlthoiTalk", "AutowieldLeft", "LinkedPortalOneSummon", "LinkedPortalTwoSummon", "HouseEvicted", "UntrainedSkills", "IsEnvoy", "UnspecializedSkills", "FreeSkillResetRenewed", "FreeAttributeResetRenewed", "SkillTemplesTimerReset", "FreeMasteryResetRenewed"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[127, 109, 104, 72, 43, 21, 57, 110, 79, 67, 52, 7, 6, 5, 42, 8, 94, 85, 19, 10, 130, 122, 101, 117, 86, 102, 9, 48, 53, 39, 35, 36, 34, 23, 82, 100, 121, 114, 13, 95, 106, 138, 140, 137, 64, 74, 59, 49, 32, 14, 128, 26, 133, 73, 77, 75, 78, 76, 92, 84, 11, 25, 66, 65, 80, 70, 17, 22, 98, 60, 44, 47, 56, 113, 45, 96, 54, 135, 38, 111, 55, 97, 69, 46, 31, 99, 15, 93, 131, 132, 3, 61, 125, 40, 87, 118, 50, 29, 71, 124, 129, 103, 90, 83, 126, 2, 37, 62, 28, 27, 89, 88, 112, 108, 107, 12, 41, 81, 33, 91, 4, 20, 16, 139, 68, 58, 1, 119, 123, 120, 24, 0, 30, 115, 105, 63, 136, 134, 51, 18, 116];

    /// The members marked `[AssessmentProperty]`, in declaration order (ACE's reflection order).
    pub const ASSESSMENT_PROPERTY: &'static [Self] = &[Self::Open, Self::Locked, Self::UnlimitedUse, Self::IsSellable, Self::AppraisalHasAllowedWielder, Self::Retained, Self::AppraisalHasAllowedActivator, Self::Ivoryable, Self::Dyable, Self::RareUsesTimer, Self::AutowieldLeft];
    /// Whether this value is marked `[AssessmentProperty]` (by value, like ACE's `HashSet` lookups).
    pub fn is_assessment_property(self) -> bool {
        Self::ASSESSMENT_PROPERTY.contains(&self)
    }

    /// The members marked `[Ephemeral]`, in declaration order (ACE's reflection order).
    pub const EPHEMERAL: &'static [Self] = &[Self::Stuck, Self::Open, Self::Visibility, Self::GeneratorStatus, Self::ResetMessagePending, Self::IsAdmin, Self::IsArch, Self::IsSentinel, Self::GeneratorEnteredWorld, Self::GeneratorDisabled, Self::GeneratorAutomaticDestruction, Self::IsPsr, Self::FirstEnterWorldDone, Self::Afk, Self::IsEnvoy];
    /// Whether this value is marked `[Ephemeral]` (by value, like ACE's `HashSet` lookups).
    pub fn is_ephemeral(self) -> bool {
        Self::EPHEMERAL.contains(&self)
    }

    /// The members marked `[SendOnLogin]`, in declaration order (ACE's reflection order).
    pub const SEND_ON_LOGIN: &'static [Self] = &[Self::AdvocateState, Self::IsAdmin, Self::IsArch, Self::IsSentinel, Self::IsAdvocate, Self::SpellComponentsRequired, Self::IsPsr, Self::ActdReceivedItems, Self::NoHeldItemShown, Self::LoginAtLifestone, Self::Account15Days];
    /// Whether this value is marked `[SendOnLogin]` (by value, like ACE's `HashSet` lookups).
    pub fn is_send_on_login(self) -> bool {
        Self::SEND_ON_LOGIN.contains(&self)
    }
}

super::support::ace_enum!(PropertyBool, u16, plain);
super::support::ace_enum_from!(PropertyBool, u16 => u32, u64, i32, i64);
