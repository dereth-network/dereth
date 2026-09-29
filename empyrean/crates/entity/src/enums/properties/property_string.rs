// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/Properties/PropertyString.cs
// @generated from ACE's `Source/ACE.Entity/Enum/Properties/PropertyString.cs`; do not edit by hand

/// ACE enum `PropertyString`, underlying `ushort`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PropertyString(pub u16);

#[allow(non_upper_case_globals)]
impl PropertyString {
    pub const Undef: Self = Self(0);
    pub const Name: Self = Self(1);
    pub const Title: Self = Self(2);
    pub const Sex: Self = Self(3);
    pub const HeritageGroup: Self = Self(4);
    pub const Template: Self = Self(5);
    pub const AttackersName: Self = Self(6);
    pub const Inscription: Self = Self(7);
    pub const ScribeName: Self = Self(8);
    pub const VendorsName: Self = Self(9);
    pub const Fellowship: Self = Self(10);
    pub const MonarchsName: Self = Self(11);
    pub const LockCode: Self = Self(12);
    pub const KeyCode: Self = Self(13);
    pub const Use: Self = Self(14);
    pub const ShortDesc: Self = Self(15);
    pub const LongDesc: Self = Self(16);
    pub const ActivationTalk: Self = Self(17);
    pub const UseMessage: Self = Self(18);
    pub const ItemHeritageGroupRestriction: Self = Self(19);
    pub const PluralName: Self = Self(20);
    pub const MonarchsTitle: Self = Self(21);
    pub const ActivationFailure: Self = Self(22);
    pub const ScribeAccount: Self = Self(23);
    pub const TownName: Self = Self(24);
    pub const CraftsmanName: Self = Self(25);
    pub const UsePkServerError: Self = Self(26);
    pub const ScoreCachedText: Self = Self(27);
    pub const ScoreDefaultEntryFormat: Self = Self(28);
    pub const ScoreFirstEntryFormat: Self = Self(29);
    pub const ScoreLastEntryFormat: Self = Self(30);
    pub const ScoreOnlyEntryFormat: Self = Self(31);
    pub const ScoreNoEntry: Self = Self(32);
    pub const Quest: Self = Self(33);
    pub const GeneratorEvent: Self = Self(34);
    pub const PatronsTitle: Self = Self(35);
    pub const HouseOwnerName: Self = Self(36);
    pub const QuestRestriction: Self = Self(37);
    pub const AppraisalPortalDestination: Self = Self(38);
    pub const TinkerName: Self = Self(39);
    pub const ImbuerName: Self = Self(40);
    pub const HouseOwnerAccount: Self = Self(41);
    pub const DisplayName: Self = Self(42);
    pub const DateOfBirth: Self = Self(43);
    pub const ThirdPartyApi: Self = Self(44);
    pub const KillQuest: Self = Self(45);
    pub const Afk: Self = Self(46);
    pub const AllegianceName: Self = Self(47);
    pub const AugmentationAddQuest: Self = Self(48);
    pub const KillQuest2: Self = Self(49);
    pub const KillQuest3: Self = Self(50);
    pub const UseSendsSignal: Self = Self(51);
    pub const GearPlatingName: Self = Self(52);
    pub const PCAPRecordedCurrentMotionState: Self = Self(8006);
    pub const PCAPRecordedServerName: Self = Self(8031);
    pub const PCAPRecordedCharacterName: Self = Self(8032);
    pub const AllegianceMotd: Self = Self(9001);
    pub const AllegianceMotdSetBy: Self = Self(9002);
    pub const AllegianceSpeakerTitle: Self = Self(9003);
    pub const AllegianceSeneschalTitle: Self = Self(9004);
    pub const AllegianceCastellanTitle: Self = Self(9005);
    pub const GodState: Self = Self(9006);
    pub const TinkerLog: Self = Self(9007);
}

impl PropertyString {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Name, Self::Title, Self::Sex, Self::HeritageGroup, Self::Template, Self::AttackersName, Self::Inscription, Self::ScribeName, Self::VendorsName, Self::Fellowship, Self::MonarchsName, Self::LockCode, Self::KeyCode, Self::Use, Self::ShortDesc, Self::LongDesc, Self::ActivationTalk, Self::UseMessage, Self::ItemHeritageGroupRestriction, Self::PluralName, Self::MonarchsTitle, Self::ActivationFailure, Self::ScribeAccount, Self::TownName, Self::CraftsmanName, Self::UsePkServerError, Self::ScoreCachedText, Self::ScoreDefaultEntryFormat, Self::ScoreFirstEntryFormat, Self::ScoreLastEntryFormat, Self::ScoreOnlyEntryFormat, Self::ScoreNoEntry, Self::Quest, Self::GeneratorEvent, Self::PatronsTitle, Self::HouseOwnerName, Self::QuestRestriction, Self::AppraisalPortalDestination, Self::TinkerName, Self::ImbuerName, Self::HouseOwnerAccount, Self::DisplayName, Self::DateOfBirth, Self::ThirdPartyApi, Self::KillQuest, Self::Afk, Self::AllegianceName, Self::AugmentationAddQuest, Self::KillQuest2, Self::KillQuest3, Self::UseSendsSignal, Self::GearPlatingName, Self::PCAPRecordedCurrentMotionState, Self::PCAPRecordedServerName, Self::PCAPRecordedCharacterName, Self::AllegianceMotd, Self::AllegianceMotdSetBy, Self::AllegianceSpeakerTitle, Self::AllegianceSeneschalTitle, Self::AllegianceCastellanTitle, Self::GodState, Self::TinkerLog];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Name", "Title", "Sex", "HeritageGroup", "Template", "AttackersName", "Inscription", "ScribeName", "VendorsName", "Fellowship", "MonarchsName", "LockCode", "KeyCode", "Use", "ShortDesc", "LongDesc", "ActivationTalk", "UseMessage", "ItemHeritageGroupRestriction", "PluralName", "MonarchsTitle", "ActivationFailure", "ScribeAccount", "TownName", "CraftsmanName", "UsePkServerError", "ScoreCachedText", "ScoreDefaultEntryFormat", "ScoreFirstEntryFormat", "ScoreLastEntryFormat", "ScoreOnlyEntryFormat", "ScoreNoEntry", "Quest", "GeneratorEvent", "PatronsTitle", "HouseOwnerName", "QuestRestriction", "AppraisalPortalDestination", "TinkerName", "ImbuerName", "HouseOwnerAccount", "DisplayName", "DateOfBirth", "ThirdPartyApi", "KillQuest", "Afk", "AllegianceName", "AugmentationAddQuest", "KillQuest2", "KillQuest3", "UseSendsSignal", "GearPlatingName", "PCAPRecordedCurrentMotionState", "PCAPRecordedServerName", "PCAPRecordedCharacterName", "AllegianceMotd", "AllegianceMotdSetBy", "AllegianceSpeakerTitle", "AllegianceSeneschalTitle", "AllegianceCastellanTitle", "GodState", "TinkerLog"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[22, 17, 46, 60, 56, 57, 47, 59, 58, 38, 6, 48, 25, 43, 42, 10, 52, 34, 61, 4, 41, 36, 40, 7, 19, 13, 45, 49, 50, 12, 16, 11, 21, 1, 55, 53, 54, 35, 20, 33, 37, 27, 28, 29, 30, 32, 31, 23, 8, 3, 15, 5, 44, 62, 39, 2, 24, 0, 14, 18, 26, 51, 9];

    /// The members marked `[AssessmentProperty]`, in declaration order (ACE's reflection order).
    pub const ASSESSMENT_PROPERTY: &'static [Self] = &[Self::Template, Self::Inscription, Self::ScribeName, Self::Fellowship, Self::Use, Self::ShortDesc, Self::LongDesc, Self::MonarchsTitle, Self::ScribeAccount, Self::CraftsmanName, Self::PatronsTitle, Self::AppraisalPortalDestination, Self::TinkerName, Self::ImbuerName, Self::DateOfBirth, Self::AllegianceName, Self::GearPlatingName];
    /// Whether this value is marked `[AssessmentProperty]` (by value, like ACE's `HashSet` lookups).
    pub fn is_assessment_property(self) -> bool {
        Self::ASSESSMENT_PROPERTY.contains(&self)
    }

    /// The members marked `[Ephemeral]`, in declaration order (ACE's reflection order).
    pub const EPHEMERAL: &'static [Self] = &[Self::Afk];
    /// Whether this value is marked `[Ephemeral]` (by value, like ACE's `HashSet` lookups).
    pub fn is_ephemeral(self) -> bool {
        Self::EPHEMERAL.contains(&self)
    }

    /// The members marked `[SendOnLogin]`, in declaration order (ACE's reflection order).
    pub const SEND_ON_LOGIN: &'static [Self] = &[Self::Name, Self::Template];
    /// Whether this value is marked `[SendOnLogin]` (by value, like ACE's `HashSet` lookups).
    pub fn is_send_on_login(self) -> bool {
        Self::SEND_ON_LOGIN.contains(&self)
    }
}

super::support::ace_enum!(PropertyString, u16, plain);
super::support::ace_enum_from!(PropertyString, u16 => u32, u64, i32, i64);
