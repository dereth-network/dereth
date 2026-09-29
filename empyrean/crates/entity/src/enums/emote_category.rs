// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/EmoteCategory.cs
// @generated from ACE's `Source/ACE.Entity/Enum/EmoteCategory.cs`; do not edit by hand

/// exported from the retail client. actual usage of these is 100% speculative.
///
/// ACE enum `EmoteCategory`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct EmoteCategory(pub i32);

#[allow(non_upper_case_globals)]
impl EmoteCategory {
    pub const Invalid: Self = Self(0);
    pub const Refuse: Self = Self(1);
    pub const Vendor: Self = Self(2);
    pub const Death: Self = Self(3);
    pub const Portal: Self = Self(4);
    pub const HeartBeat: Self = Self(5);
    pub const Give: Self = Self(6);
    pub const Use: Self = Self(7);
    pub const Activation: Self = Self(8);
    pub const Generation: Self = Self(9);
    pub const PickUp: Self = Self(10);
    pub const Drop: Self = Self(11);
    pub const QuestSuccess: Self = Self(12);
    pub const QuestFailure: Self = Self(13);
    pub const Taunt: Self = Self(14);
    pub const WoundedTaunt: Self = Self(15);
    pub const KillTaunt: Self = Self(16);
    pub const NewEnemy: Self = Self(17);
    pub const Scream: Self = Self(18);
    pub const Homesick: Self = Self(19);
    pub const ReceiveCritical: Self = Self(20);
    pub const ResistSpell: Self = Self(21);
    pub const TestSuccess: Self = Self(22);
    pub const TestFailure: Self = Self(23);
    pub const HearChat: Self = Self(24);
    pub const Wield: Self = Self(25);
    pub const UnWield: Self = Self(26);
    pub const EventSuccess: Self = Self(27);
    pub const EventFailure: Self = Self(28);
    pub const TestNoQuality: Self = Self(29);
    pub const QuestNoFellow: Self = Self(30);
    pub const TestNoFellow: Self = Self(31);
    pub const GotoSet: Self = Self(32);
    pub const NumFellowsSuccess: Self = Self(33);
    pub const NumFellowsFailure: Self = Self(34);
    pub const NumCharacterTitlesSuccess: Self = Self(35);
    pub const NumCharacterTitlesFailure: Self = Self(36);
    pub const ReceiveLocalSignal: Self = Self(37);
    pub const ReceiveTalkDirect: Self = Self(38);
}

impl EmoteCategory {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Invalid, Self::Refuse, Self::Vendor, Self::Death, Self::Portal, Self::HeartBeat, Self::Give, Self::Use, Self::Activation, Self::Generation, Self::PickUp, Self::Drop, Self::QuestSuccess, Self::QuestFailure, Self::Taunt, Self::WoundedTaunt, Self::KillTaunt, Self::NewEnemy, Self::Scream, Self::Homesick, Self::ReceiveCritical, Self::ResistSpell, Self::TestSuccess, Self::TestFailure, Self::HearChat, Self::Wield, Self::UnWield, Self::EventSuccess, Self::EventFailure, Self::TestNoQuality, Self::QuestNoFellow, Self::TestNoFellow, Self::GotoSet, Self::NumFellowsSuccess, Self::NumFellowsFailure, Self::NumCharacterTitlesSuccess, Self::NumCharacterTitlesFailure, Self::ReceiveLocalSignal, Self::ReceiveTalkDirect];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Invalid", "Refuse", "Vendor", "Death", "Portal", "HeartBeat", "Give", "Use", "Activation", "Generation", "PickUp", "Drop", "QuestSuccess", "QuestFailure", "Taunt", "WoundedTaunt", "KillTaunt", "NewEnemy", "Scream", "Homesick", "ReceiveCritical", "ResistSpell", "TestSuccess", "TestFailure", "HearChat", "Wield", "UnWield", "EventSuccess", "EventFailure", "TestNoQuality", "QuestNoFellow", "TestNoFellow", "GotoSet", "NumFellowsSuccess", "NumFellowsFailure", "NumCharacterTitlesSuccess", "NumCharacterTitlesFailure", "ReceiveLocalSignal", "ReceiveTalkDirect"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[8, 3, 11, 28, 27, 9, 6, 32, 24, 5, 19, 0, 16, 17, 36, 35, 34, 33, 10, 4, 13, 30, 12, 20, 37, 38, 1, 21, 18, 14, 23, 31, 29, 22, 26, 7, 2, 25, 15];
}

super::support::ace_enum!(EmoteCategory, i32, plain);
super::support::ace_enum_from!(EmoteCategory, i32 => i64);
