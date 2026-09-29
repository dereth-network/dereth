// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/EmoteType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/EmoteType.cs`; do not edit by hand

/// exported from the retail client. actual usage of these is 100% speculative.
///
/// ACE enum `EmoteType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct EmoteType(pub i32);

#[allow(non_upper_case_globals)]
impl EmoteType {
    pub const Invalid: Self = Self(0);
    pub const InvalidVendor: Self = Self(0);
    pub const Act: Self = Self(1);
    pub const AwardXP: Self = Self(2);
    pub const Give: Self = Self(3);
    pub const MoveHome: Self = Self(4);
    pub const Motion: Self = Self(5);
    pub const Move: Self = Self(6);
    pub const PhysScript: Self = Self(7);
    pub const Say: Self = Self(8);
    pub const Sound: Self = Self(9);
    pub const Tell: Self = Self(10);
    pub const Turn: Self = Self(11);
    pub const TurnToTarget: Self = Self(12);
    pub const TextDirect: Self = Self(13);
    pub const CastSpell: Self = Self(14);
    pub const Activate: Self = Self(15);
    pub const WorldBroadcast: Self = Self(16);
    pub const LocalBroadcast: Self = Self(17);
    pub const DirectBroadcast: Self = Self(18);
    pub const CastSpellInstant: Self = Self(19);
    pub const UpdateQuest: Self = Self(20);
    pub const InqQuest: Self = Self(21);
    pub const StampQuest: Self = Self(22);
    pub const StartEvent: Self = Self(23);
    pub const StopEvent: Self = Self(24);
    pub const BLog: Self = Self(25);
    pub const AdminSpam: Self = Self(26);
    pub const TeachSpell: Self = Self(27);
    pub const AwardSkillXP: Self = Self(28);
    pub const AwardSkillPoints: Self = Self(29);
    pub const InqQuestSolves: Self = Self(30);
    pub const EraseQuest: Self = Self(31);
    pub const DecrementQuest: Self = Self(32);
    pub const IncrementQuest: Self = Self(33);
    pub const AddCharacterTitle: Self = Self(34);
    pub const InqBoolStat: Self = Self(35);
    pub const InqIntStat: Self = Self(36);
    pub const InqFloatStat: Self = Self(37);
    pub const InqStringStat: Self = Self(38);
    pub const InqAttributeStat: Self = Self(39);
    pub const InqRawAttributeStat: Self = Self(40);
    pub const InqSecondaryAttributeStat: Self = Self(41);
    pub const InqRawSecondaryAttributeStat: Self = Self(42);
    pub const InqSkillStat: Self = Self(43);
    pub const InqRawSkillStat: Self = Self(44);
    pub const InqSkillTrained: Self = Self(45);
    pub const InqSkillSpecialized: Self = Self(46);
    pub const AwardTrainingCredits: Self = Self(47);
    pub const InflictVitaePenalty: Self = Self(48);
    pub const AwardLevelProportionalXP: Self = Self(49);
    pub const AwardLevelProportionalSkillXP: Self = Self(50);
    pub const InqEvent: Self = Self(51);
    pub const ForceMotion: Self = Self(52);
    pub const SetIntStat: Self = Self(53);
    pub const IncrementIntStat: Self = Self(54);
    pub const DecrementIntStat: Self = Self(55);
    pub const CreateTreasure: Self = Self(56);
    pub const ResetHomePosition: Self = Self(57);
    pub const InqFellowQuest: Self = Self(58);
    pub const InqFellowNum: Self = Self(59);
    pub const UpdateFellowQuest: Self = Self(60);
    pub const StampFellowQuest: Self = Self(61);
    pub const AwardNoShareXP: Self = Self(62);
    pub const SetSanctuaryPosition: Self = Self(63);
    pub const TellFellow: Self = Self(64);
    pub const FellowBroadcast: Self = Self(65);
    pub const LockFellow: Self = Self(66);
    pub const Goto: Self = Self(67);
    pub const PopUp: Self = Self(68);
    pub const SetBoolStat: Self = Self(69);
    pub const SetQuestCompletions: Self = Self(70);
    pub const InqNumCharacterTitles: Self = Self(71);
    pub const Generate: Self = Self(72);
    pub const PetCastSpellOnOwner: Self = Self(73);
    pub const TakeItems: Self = Self(74);
    pub const InqYesNo: Self = Self(75);
    pub const InqOwnsItems: Self = Self(76);
    pub const DeleteSelf: Self = Self(77);
    pub const KillSelf: Self = Self(78);
    pub const UpdateMyQuest: Self = Self(79);
    pub const InqMyQuest: Self = Self(80);
    pub const StampMyQuest: Self = Self(81);
    pub const InqMyQuestSolves: Self = Self(82);
    pub const EraseMyQuest: Self = Self(83);
    pub const DecrementMyQuest: Self = Self(84);
    pub const IncrementMyQuest: Self = Self(85);
    pub const SetMyQuestCompletions: Self = Self(86);
    pub const MoveToPos: Self = Self(87);
    pub const LocalSignal: Self = Self(88);
    pub const InqPackSpace: Self = Self(89);
    pub const RemoveVitaePenalty: Self = Self(90);
    pub const SetEyeTexture: Self = Self(91);
    pub const SetEyePalette: Self = Self(92);
    pub const SetNoseTexture: Self = Self(93);
    pub const SetNosePalette: Self = Self(94);
    pub const SetMouthTexture: Self = Self(95);
    pub const SetMouthPalette: Self = Self(96);
    pub const SetHeadObject: Self = Self(97);
    pub const SetHeadPalette: Self = Self(98);
    pub const TeleportTarget: Self = Self(99);
    pub const TeleportSelf: Self = Self(100);
    pub const StartBarber: Self = Self(101);
    pub const InqQuestBitsOn: Self = Self(102);
    pub const InqQuestBitsOff: Self = Self(103);
    pub const InqMyQuestBitsOn: Self = Self(104);
    pub const InqMyQuestBitsOff: Self = Self(105);
    pub const SetQuestBitsOn: Self = Self(106);
    pub const SetQuestBitsOff: Self = Self(107);
    pub const SetMyQuestBitsOn: Self = Self(108);
    pub const SetMyQuestBitsOff: Self = Self(109);
    pub const UntrainSkill: Self = Self(110);
    pub const SetAltRacialSkills: Self = Self(111);
    pub const SpendLuminance: Self = Self(112);
    pub const AwardLuminance: Self = Self(113);
    pub const InqInt64Stat: Self = Self(114);
    pub const SetInt64Stat: Self = Self(115);
    pub const OpenMe: Self = Self(116);
    pub const CloseMe: Self = Self(117);
    pub const SetFloatStat: Self = Self(118);
    pub const AddContract: Self = Self(119);
    pub const RemoveContract: Self = Self(120);
    pub const InqContractsFull: Self = Self(121);
    pub const Enlightenment: Self = Self(9001);
}

impl EmoteType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Invalid, Self::InvalidVendor, Self::Act, Self::AwardXP, Self::Give, Self::MoveHome, Self::Motion, Self::Move, Self::PhysScript, Self::Say, Self::Sound, Self::Tell, Self::Turn, Self::TurnToTarget, Self::TextDirect, Self::CastSpell, Self::Activate, Self::WorldBroadcast, Self::LocalBroadcast, Self::DirectBroadcast, Self::CastSpellInstant, Self::UpdateQuest, Self::InqQuest, Self::StampQuest, Self::StartEvent, Self::StopEvent, Self::BLog, Self::AdminSpam, Self::TeachSpell, Self::AwardSkillXP, Self::AwardSkillPoints, Self::InqQuestSolves, Self::EraseQuest, Self::DecrementQuest, Self::IncrementQuest, Self::AddCharacterTitle, Self::InqBoolStat, Self::InqIntStat, Self::InqFloatStat, Self::InqStringStat, Self::InqAttributeStat, Self::InqRawAttributeStat, Self::InqSecondaryAttributeStat, Self::InqRawSecondaryAttributeStat, Self::InqSkillStat, Self::InqRawSkillStat, Self::InqSkillTrained, Self::InqSkillSpecialized, Self::AwardTrainingCredits, Self::InflictVitaePenalty, Self::AwardLevelProportionalXP, Self::AwardLevelProportionalSkillXP, Self::InqEvent, Self::ForceMotion, Self::SetIntStat, Self::IncrementIntStat, Self::DecrementIntStat, Self::CreateTreasure, Self::ResetHomePosition, Self::InqFellowQuest, Self::InqFellowNum, Self::UpdateFellowQuest, Self::StampFellowQuest, Self::AwardNoShareXP, Self::SetSanctuaryPosition, Self::TellFellow, Self::FellowBroadcast, Self::LockFellow, Self::Goto, Self::PopUp, Self::SetBoolStat, Self::SetQuestCompletions, Self::InqNumCharacterTitles, Self::Generate, Self::PetCastSpellOnOwner, Self::TakeItems, Self::InqYesNo, Self::InqOwnsItems, Self::DeleteSelf, Self::KillSelf, Self::UpdateMyQuest, Self::InqMyQuest, Self::StampMyQuest, Self::InqMyQuestSolves, Self::EraseMyQuest, Self::DecrementMyQuest, Self::IncrementMyQuest, Self::SetMyQuestCompletions, Self::MoveToPos, Self::LocalSignal, Self::InqPackSpace, Self::RemoveVitaePenalty, Self::SetEyeTexture, Self::SetEyePalette, Self::SetNoseTexture, Self::SetNosePalette, Self::SetMouthTexture, Self::SetMouthPalette, Self::SetHeadObject, Self::SetHeadPalette, Self::TeleportTarget, Self::TeleportSelf, Self::StartBarber, Self::InqQuestBitsOn, Self::InqQuestBitsOff, Self::InqMyQuestBitsOn, Self::InqMyQuestBitsOff, Self::SetQuestBitsOn, Self::SetQuestBitsOff, Self::SetMyQuestBitsOn, Self::SetMyQuestBitsOff, Self::UntrainSkill, Self::SetAltRacialSkills, Self::SpendLuminance, Self::AwardLuminance, Self::InqInt64Stat, Self::SetInt64Stat, Self::OpenMe, Self::CloseMe, Self::SetFloatStat, Self::AddContract, Self::RemoveContract, Self::InqContractsFull, Self::Enlightenment];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Invalid", "InvalidVendor", "Act", "AwardXP", "Give", "MoveHome", "Motion", "Move", "PhysScript", "Say", "Sound", "Tell", "Turn", "TurnToTarget", "TextDirect", "CastSpell", "Activate", "WorldBroadcast", "LocalBroadcast", "DirectBroadcast", "CastSpellInstant", "UpdateQuest", "InqQuest", "StampQuest", "StartEvent", "StopEvent", "BLog", "AdminSpam", "TeachSpell", "AwardSkillXP", "AwardSkillPoints", "InqQuestSolves", "EraseQuest", "DecrementQuest", "IncrementQuest", "AddCharacterTitle", "InqBoolStat", "InqIntStat", "InqFloatStat", "InqStringStat", "InqAttributeStat", "InqRawAttributeStat", "InqSecondaryAttributeStat", "InqRawSecondaryAttributeStat", "InqSkillStat", "InqRawSkillStat", "InqSkillTrained", "InqSkillSpecialized", "AwardTrainingCredits", "InflictVitaePenalty", "AwardLevelProportionalXP", "AwardLevelProportionalSkillXP", "InqEvent", "ForceMotion", "SetIntStat", "IncrementIntStat", "DecrementIntStat", "CreateTreasure", "ResetHomePosition", "InqFellowQuest", "InqFellowNum", "UpdateFellowQuest", "StampFellowQuest", "AwardNoShareXP", "SetSanctuaryPosition", "TellFellow", "FellowBroadcast", "LockFellow", "Goto", "PopUp", "SetBoolStat", "SetQuestCompletions", "InqNumCharacterTitles", "Generate", "PetCastSpellOnOwner", "TakeItems", "InqYesNo", "InqOwnsItems", "DeleteSelf", "KillSelf", "UpdateMyQuest", "InqMyQuest", "StampMyQuest", "InqMyQuestSolves", "EraseMyQuest", "DecrementMyQuest", "IncrementMyQuest", "SetMyQuestCompletions", "MoveToPos", "LocalSignal", "InqPackSpace", "RemoveVitaePenalty", "SetEyeTexture", "SetEyePalette", "SetNoseTexture", "SetNosePalette", "SetMouthTexture", "SetMouthPalette", "SetHeadObject", "SetHeadPalette", "TeleportTarget", "TeleportSelf", "StartBarber", "InqQuestBitsOn", "InqQuestBitsOff", "InqMyQuestBitsOn", "InqMyQuestBitsOff", "SetQuestBitsOn", "SetQuestBitsOff", "SetMyQuestBitsOn", "SetMyQuestBitsOff", "UntrainSkill", "SetAltRacialSkills", "SpendLuminance", "AwardLuminance", "InqInt64Stat", "SetInt64Stat", "OpenMe", "CloseMe", "SetFloatStat", "AddContract", "RemoveContract", "InqContractsFull", "Enlightenment"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 16, 35, 120, 27, 51, 50, 114, 63, 30, 29, 48, 3, 26, 15, 20, 118, 57, 56, 85, 33, 78, 19, 123, 84, 32, 66, 53, 73, 4, 68, 55, 86, 34, 49, 40, 36, 122, 52, 60, 59, 38, 115, 37, 81, 106, 105, 83, 72, 77, 90, 22, 104, 103, 31, 41, 43, 45, 42, 47, 44, 46, 39, 76, 0, 1, 79, 18, 89, 67, 6, 7, 5, 88, 117, 74, 8, 69, 121, 91, 58, 9, 112, 70, 93, 92, 119, 98, 99, 116, 54, 97, 96, 110, 109, 87, 95, 94, 108, 107, 71, 64, 10, 113, 62, 82, 23, 102, 24, 25, 75, 28, 101, 100, 11, 65, 14, 12, 13, 111, 61, 80, 21, 17];
}

super::support::ace_enum!(EmoteType, i32, plain);
super::support::ace_enum_from!(EmoteType, i32 => i64);
