//! Vectors: local ACE enum-extension input/output cases in this module
//!  enum extension helpers (attack height/type, aug type, chat mask, damage types, environ
//! change, quadrant, skill lists, spell sets, squelch mask, usable, option flags, property
//! extensions) as ACE.
//! Fixture: enum values and synthetic entity records.

use empyrean_entity::enums::ext::*;
use empyrean_entity::enums::*;

#[test]
fn attack_height_extensions() {
    assert_eq!(AttackHeight::Medium.get_string(), Some("Med"));
    assert_eq!(AttackHeight(0).get_string(), None);
    assert_eq!(AttackHeight::Low.to_quadrant(), Quadrant::Low);
    assert_eq!(AttackHeight(7).to_quadrant(), Quadrant::None);
}

#[test]
fn attack_type_extensions() {
    assert!(AttackType::TripleSlash.is_multi_strike());
    assert!(!AttackType::Punch.is_multi_strike());
    assert_eq!(
        AttackType::TripleSlash.reduce_multi_strike(),
        AttackType::Slash
    );
    assert_eq!(
        AttackType::OffhandDoubleThrust.reduce_multi_strike(),
        AttackType::OffhandThrust
    );
    assert_eq!(AttackType::Punch.reduce_multi_strike(), AttackType::Undef);
    // A multi-strike mask that is no single member falls to the default arm.
    assert_eq!(
        AttackType::MultiStrike.reduce_multi_strike(),
        AttackType::Undef
    );
}

#[test]
fn aug_type_helper() {
    use aug_type_helper as h;
    assert!(h::is_attribute(AugmentationType::Self_));
    assert!(!h::is_attribute(AugmentationType::Salvage));
    assert!(h::is_resist(AugmentationType::ResistCold));
    assert!(h::is_skill(AugmentationType::WeaponTinkering));
    assert_eq!(
        h::get_attribute(AugmentationType::Self_),
        PropertyAttribute::Self_
    );
    assert_eq!(
        h::get_attribute(AugmentationType::Salvage),
        PropertyAttribute::Undef
    );
    assert_eq!(h::get_skill(AugmentationType::Salvage), Skill::Salvaging);
    assert_eq!(
        h::get_effect(AugmentationType::Focus),
        PlayScript::AugmentationUseAttribute
    );
    assert_eq!(
        h::get_effect(AugmentationType::ResistFire),
        PlayScript::AugmentationUseResistances
    );
    assert_eq!(
        h::get_effect(AugmentationType::ItemTinkering),
        PlayScript::AugmentationUseSkill
    );
    assert_eq!(
        h::get_effect(AugmentationType::BonusXP),
        PlayScript::AugmentationUseOther
    );
}

#[test]
fn chat_message_type_to_mask() {
    assert_eq!(ChatMessageType::Speech.to_mask(), SquelchMask::Speech);
    assert_eq!(
        ChatMessageType::AllChannels.to_mask(),
        SquelchMask::AllChannels
    );
    // C# masks an int shift count to five bits: 1 << 33 == 1 << 1.
    assert_eq!(ChatMessageType(33).to_mask(), SquelchMask(2));
    assert_eq!(ChatMessageType(31).to_mask(), SquelchMask(0x8000_0000));
}

#[test]
fn damage_type_extensions() {
    assert_eq!(DamageType::Bludgeon.get_name(), Some("Bludgeoning"));
    assert_eq!(DamageType::Undef.get_name(), Some("Undefined"));
    assert_eq!(DamageType::Physical.get_name(), None);
    assert!(DamageType::Physical.is_multi_damage());
    assert!(!DamageType::Slash.is_multi_damage());
    assert!(!DamageType::Undef.is_multi_damage());
}

#[test]
fn select_damage_type_draws_from_get_flags_as_ace_does() {
    use std::collections::BTreeSet;
    // Physical's GetFlags list is [Undef, Slash, Pierce, Bludgeon, Physical]; Next(1, 3) skips
    // Undef and (ACE-BUG) can return Physical itself.
    empyrean_common::thread_safe_random::ThreadSafeRandom::seed(7);
    let seen: BTreeSet<i32> = (0..200)
        .map(|_| DamageType::Physical.select_damage_type(None).0)
        .collect();
    let want: BTreeSet<i32> = [
        DamageType::Slash,
        DamageType::Pierce,
        DamageType::Bludgeon,
        DamageType::Physical,
    ]
    .iter()
    .map(|d| d.0)
    .collect();
    assert_eq!(seen, want);
    // A power level below 0.33 keeps the physical part: [Undef, Slash] and Next(1, 1) is Slash.
    assert_eq!(
        (DamageType::Slash | DamageType::Fire).select_damage_type(Some(0.2)),
        DamageType::Slash
    );
    assert_eq!(
        (DamageType::Slash | DamageType::Fire).select_damage_type(Some(0.5)),
        DamageType::Fire
    );
    // No match on the chosen side falls back to the whole mask.
    assert_eq!(
        DamageType::Cold.select_damage_type(Some(0.1)),
        DamageType::Cold
    );
}

#[test]
fn environ_change_type_extensions() {
    assert!(EnvironChangeType::Clear.is_fog());
    assert!(EnvironChangeType::BlackFog2.is_fog());
    assert!(!EnvironChangeType::RoarSound.is_fog());
    assert!(EnvironChangeType::RoarSound.is_sound());
    assert!(!EnvironChangeType::BlackFog2.is_sound());
}

#[test]
fn to_sentence_helpers() {
    assert_eq!(FactionBits::RadiantBlood.to_sentence(), "Radiant Blood");
    assert_eq!(FactionBits::ValidFactions.to_sentence(), "Valid Factions");
    // ToString gives "CelestialHand, EldrytchWeb"; the fallback spaces the capital after ", ".
    assert_eq!(
        FactionBits(3).to_sentence(),
        "Celestial Hand,  Eldrytch Web"
    );
    assert_eq!(HeritageGroup::Gharundim.to_sentence(), "Gharu'ndim");
    assert_eq!(HeritageGroup::Shadowbound.to_sentence(), "Umbraen");
    assert_eq!(HeritageGroup::Aluvian.to_sentence(), "Aluvian");
    assert_eq!(HeritageGroup(99).to_sentence(), "99");
    assert_eq!(
        HookGroupType::SpellCastingItems.to_sentence(),
        "Spell Casting Items"
    );
    assert_eq!(
        HookGroupType(3).to_sentence(),
        "3",
        "not [Flags]: ToString is the number"
    );
    assert_eq!(
        Skill::ArmsAndArmorRepair.to_sentence(),
        "Arms And Armor Repair"
    );
    assert_eq!(Skill(99).to_sentence(), "99");
    assert_eq!(
        PropertyAttribute2nd::MaxHealth.to_sentence(),
        "Maximum Health"
    );
    assert_eq!(PropertyAttribute2nd(9).to_sentence(), "9");
    assert_eq!(PropertyAttribute::Self_.get_description(), "Self");
    assert_eq!(PropertyAttribute2nd::MaxMana.get_description(), "MaxMana");
}

#[test]
fn motion_command_helper() {
    assert_eq!(
        motion_command_helper::get_motion(MotionCommand(0x1000_0162)),
        MotionCommand::Fishing
    );
    assert_eq!(
        motion_command_helper::get_motion(MotionCommand::Ready),
        MotionCommand::Ready
    );
    assert!(MotionCommand::TripleThrustHigh.is_multi_strike());
    assert!(!MotionCommand::ThrustHigh.is_multi_strike());
    assert_eq!(
        MotionCommand::OffhandTripleThrustMed.reduce_multi_strike(),
        MotionCommand::ThrustMed
    );
    assert_eq!(
        MotionCommand::DoubleSlashLow.reduce_multi_strike(),
        MotionCommand::SlashLow
    );
    assert_eq!(
        MotionCommand::Ready.reduce_multi_strike(),
        MotionCommand::Invalid
    );
    assert!(MotionCommand::AttackMed4.is_subsequent());
    assert!(!MotionCommand::AttackMed1.is_subsequent());
    assert_eq!(
        MotionCommand::AttackMed4.reduce_subsequent(),
        MotionCommand::AttackMed1
    );
    assert_eq!(
        MotionCommand::AttackHigh2.reduce_subsequent(),
        MotionCommand::AttackHigh1
    );
    assert_eq!(MotionCommand::AimLow45.get_aim_angle(), -45.0);
    assert_eq!(MotionCommand::AimHigh90.get_aim_angle(), 90.0);
    assert_eq!(MotionCommand::Ready.get_aim_angle(), 0.0);
}

#[test]
fn quadrant_index_extensions() {
    assert_eq!(
        QuadrantIndex::LRB.to_quadrant(),
        Quadrant(0x4 | 0x10 | 0x40)
    );
    assert_eq!(
        quadrant_index_extensions::HLF,
        Quadrant::High | Quadrant::Left | Quadrant::Front
    );
    for i in QuadrantIndex::ALL {
        assert_eq!(i.to_quadrant().get_index(), *i);
    }
    assert_eq!(Quadrant::High.get_index(), QuadrantIndex(0));
    assert_eq!(QuadrantIndex(12).to_quadrant(), Quadrant::None);
}

#[test]
fn skill_lists() {
    assert_eq!(skill_extensions::RETIRED_MELEE.len(), 7);
    assert_eq!(skill_extensions::RETIRED_MISSILE.len(), 4);
    assert_eq!(skill_extensions::RETIRED_WEAPONS.len(), 11);
    assert_eq!(skill_extensions::RETIRED_WEAPONS[7], Skill::Bow);
    assert_eq!(skill_helper::VALID_SKILLS.len(), 38);
    assert!(!skill_helper::VALID_SKILLS.contains(&Skill::Axe));
    assert_eq!(skill_helper::ATTACK_SKILLS.len(), 20);
    assert!(!skill_helper::ATTACK_SKILLS.contains(&Skill::Recklessness));
    assert_eq!(
        skill_helper::DEFENSE_SKILLS,
        &[
            Skill::MeleeDefense,
            Skill::MissileDefense,
            Skill::MagicDefense,
            Skill::Shield
        ]
    );
}

#[test]
fn spell_sets() {
    assert_eq!(spell_extensions::DIRTY_FIGHTING_SPELLS.len(), 8);
    assert_eq!(spell_extensions::VOID_MAGIC_SPELLS.len(), 75);
    assert!(spell_extensions::VOID_MAGIC_SPELLS.contains(&SpellId::NetherRing));
}

#[test]
fn squelch_mask_add_and_remove() {
    assert_eq!(
        SquelchMask::Speech.add(SquelchMask::AllChannels),
        SquelchMask::AllChannels
    );
    assert_eq!(SquelchMask::Speech.add(SquelchMask::Tell), SquelchMask(0xC));
    let all_but_speech = SquelchMask(SquelchMask::Combined.0 & !SquelchMask::Speech.0);
    assert_eq!(
        all_but_speech.add(SquelchMask::Speech),
        SquelchMask::AllChannels
    );
    assert_eq!(
        SquelchMask::AllChannels.remove(SquelchMask::Speech),
        all_but_speech
    );
    assert_eq!(
        SquelchMask::Tell.remove(SquelchMask::AllChannels),
        SquelchMask::None
    );
    assert_eq!(
        SquelchMask(0xC).remove(SquelchMask::Tell),
        SquelchMask::Speech
    );
    // Removing nothing from AllChannels goes through Combined and back to AllChannels.
    assert_eq!(
        SquelchMask::AllChannels.remove(SquelchMask::None),
        SquelchMask::AllChannels
    );
}

#[test]
fn usable_extensions() {
    let u = Usable::SourceRemoteTargetRemoteNeverWalk; // 0x600020
    assert_eq!(u.get_source_flags(), Usable(0x20));
    assert_eq!(u.get_target_flags(), Usable(0x60));
}

#[test]
fn character_option_flags() {
    let options = [
        (CharacterOption::AutoRepeatAttacks, true),
        (CharacterOption::IgnoreAllegianceRequests, false),
        (CharacterOption::AlwaysDaylightOutdoors, true),
        (CharacterOption::ListenToPKDeathMessages, true),
    ];
    assert_eq!(
        character_option_extensions::get_character_options1_flag(options),
        CharacterOptions1::AutoRepeatAttack.0
    );
    assert_eq!(
        character_option_extensions::get_character_options2_flag(options),
        CharacterOptions2::PersistentAtDay.0 | CharacterOptions2::HearPKDeath.0
    );
}

#[test]
fn property_data_id_extensions() {
    assert!(!PropertyDataId::Spell.is_hex_data());
    assert!(PropertyDataId::Setup.is_hex_data());
    assert!(!PropertyDataId::PCAPRecordedWeenieHeader.is_hex_data());
    assert!(!PropertyDataId::PCAPPhysicsDIDDataTemplatedFrom.is_hex_data());
    assert_eq!(
        PropertyDataId::Spell.get_value_enum_name(1).as_deref(),
        Some("StrengthOther1")
    );
    assert_eq!(
        PropertyDataId::InitMotion
            .get_value_enum_name(0x4100_0003)
            .as_deref(),
        Some("Ready")
    );
    assert_eq!(
        PropertyDataId::WieldedTreasureType.get_value_enum_name(1),
        None
    );
    assert_eq!(PropertyDataId::Setup.get_value_enum_name(1), None);
}

#[test]
fn property_int_extensions() {
    assert_eq!(
        PropertyInt::Gender.get_value_enum_name(1).as_deref(),
        Some("Male")
    );
    assert_eq!(
        PropertyInt::ItemType.get_value_enum_name(1).as_deref(),
        Some("MeleeWeapon")
    );
    assert_eq!(
        PropertyInt::WieldSkillType2
            .get_value_enum_name(6)
            .as_deref(),
        Some("MeleeDefense")
    );
    // GetName does not decompose flags: 3 is no single DamageType member.
    assert_eq!(PropertyInt::DamageType.get_value_enum_name(3), None);
    assert_eq!(PropertyInt::Level.get_value_enum_name(1), None);
    // Invariant-culture DateTime: MM/dd/yyyy HH:mm:ss, UTC.
    let t = |v| PropertyInt::GeneratorStartTime.get_value_enum_name(v);
    assert_eq!(t(0).as_deref(), Some("01/01/1970 00:00:00"));
    assert_eq!(t(1_000_000_000).as_deref(), Some("09/09/2001 01:46:40"));
    assert_eq!(t(-1).as_deref(), Some("12/31/1969 23:59:59"));
    assert_eq!(
        PropertyInt::GeneratorEndTime
            .get_value_enum_name(951_782_400)
            .as_deref(),
        Some("02/29/2000 00:00:00")
    );
}

#[test]
fn property_attribute_sets() {
    assert_eq!(send_on_login_properties::PROPERTIES_INT.len(), 119);
    assert_eq!(assessment_properties::PROPERTIES_INT.len(), 122);
    assert!(assessment_properties::PROPERTIES_INSTANCE_ID.is_empty());
    assert!(ephemeral_properties::PROPERTIES_INT64.is_empty());
    assert!(ephemeral_properties::PROPERTIES_DATA_ID.is_empty());
    assert_eq!(ephemeral_properties::POSITION_TYPES.len(), 1);
    assert_eq!(ephemeral_properties::PROPERTIES_INSTANCE_ID.len(), 16);
}
