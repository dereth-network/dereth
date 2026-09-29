//! Vectors: local ACE enum value, name, flag and attribute cases in this module
//! Generated enums carry ACE values/names, evaluate constant expressions, keep underlying types,
//! round-trip unknowns, .NET GetValues/Flags formatting and alias naming, attribute queries.
//! Fixture: enum values and synthetic entity records.

use std::mem::size_of;

use empyrean_entity::enums::*;

#[test]
fn members_carry_aces_values_and_names() {
    assert_eq!(PropertyInt::Level.0, 25);
    assert_eq!(PropertyInt::Level.name(), Some("Level"));
    assert_eq!(PropertyInt::from_name("Level"), Some(PropertyInt::Level));
    assert_eq!(
        PropertyInt::from_name("level"),
        None,
        "names are case-sensitive, like Enum.Parse"
    );
    assert_eq!(
        empyrean_entity::enums::properties::PropertyInt::Level,
        PropertyInt::Level
    );
    // implicit values count up from the previous member
    assert_eq!(Skill::None.0, 0);
    assert_eq!(Skill::Summoning.0, 54);
    assert_eq!(SpellId::StrengthOther1.0, 1);
}

#[test]
fn constant_expressions_evaluate_as_csharp_does() {
    assert_eq!(
        CommandMask::Command.0,
        0x00FF_FFFF,
        "~(Style | ... | Mappable) in a uint enum"
    );
    assert_eq!(ChatFilterMask::AllMessageTypes.0, -1, "~0 in an int enum");
    assert_eq!(EquipMask::Clothing.0, 0x8000_01FF);
    assert_eq!(
        CoverageMaskHelper::Underwear.0,
        0x7E,
        "members of another enum"
    );
    assert_eq!(
        DamageType::Physical,
        DamageType::Slash | DamageType::Pierce | DamageType::Bludgeon
    );
    assert_eq!(SquelchMask::AllChannels.0, 0xFFFF_FFFF);
}

#[test]
fn underlying_types_follow_the_csharp_declaration() {
    assert_eq!(size_of::<PropertyInt>(), 2); // : ushort
    assert_eq!(size_of::<MotionFlags>(), 1); // : byte
    assert_eq!(size_of::<SpellId>(), 4); // : uint
    assert_eq!(size_of::<Skill>(), 4); // int
    assert_eq!(size_of::<ChatDisplayMask>(), 8); // : long
    assert_eq!(WeenieClassName::W_HUMAN_CLASS.0, 1u16);
    assert_eq!(PropertyInt::default(), PropertyInt(0));
}

#[test]
fn unknown_values_round_trip() {
    let v = PropertyInt(0xFFFE);
    assert_eq!(v.0, 0xFFFE);
    assert_eq!(v.name(), None);
    assert!(!v.is_defined());
    assert_eq!(v.to_dotnet_string(), "65534");
    assert_eq!(format!("{v:?}"), "PropertyInt(65534)");
    assert_eq!(format!("{:?}", PropertyInt::Level), "PropertyInt::Level");
    assert_eq!(format!("{}", AnimationHookDir(-5)), "-5");
}

#[test]
fn aliases_name_the_first_member_in_get_values_order() {
    assert_eq!(StatType(6).name(), Some("DID"));
    assert_eq!(StatType::from_name("DID"), Some(StatType(6)));
    assert_eq!(StatType::DID, StatType::DataID);
    // Both aliases stay in ALL, as .NET GetValues keeps them.
    assert_eq!(StatType::ALL.iter().filter(|s| s.0 == 6).count(), 2);
}

#[test]
fn all_is_in_dotnet_get_values_order() {
    // .NET sorts by the value as an unsigned number: -2 and -1 sort after 1.
    assert_eq!(
        AnimationHookDir::ALL,
        &[
            AnimationHookDir::Both,
            AnimationHookDir::Forward,
            AnimationHookDir::Unknown,
            AnimationHookDir::Backward
        ]
    );
    assert_eq!(
        AnimationHookDir::NAMES,
        &["Both", "Forward", "Unknown", "Backward"]
    );
    for w in WeenieClassName::ALL.windows(2) {
        assert!(w[0] <= w[1]);
    }
    assert_eq!(<Skill as AceEnum>::MEMBERS.len(), 55);
}

#[test]
fn flags_format_like_dotnet() {
    assert_eq!(
        FactionBits(3).to_dotnet_string(),
        "CelestialHand, EldrytchWeb"
    );
    assert_eq!(
        FactionBits(7).to_dotnet_string(),
        "ValidFactions",
        "an exact composite wins"
    );
    assert_eq!(
        FactionBits(8).to_dotnet_string(),
        "8",
        "an unnamed bit prints the number"
    );
    // Largest first: Physical (7) is not wholly present in 3, so Pierce then Slash are taken.
    assert_eq!(DamageType(3).to_dotnet_string(), "Slash, Pierce");
    assert_eq!(DamageType(0x88).to_dotnet_string(), "Cold, Health");
    assert_eq!(
        AttackHeight(9).to_dotnet_string(),
        "9",
        "not [Flags]: no decomposition"
    );
    assert_eq!(DamageType::Undef.to_dotnet_string(), "Undef");
}

#[test]
fn flag_operators() {
    let mut d = DamageType::Slash | DamageType::Pierce;
    assert_eq!(d, DamageType(3));
    assert!(DamageType::Physical.contains(d));
    assert!(!d.contains(DamageType::Physical));
    assert!(d.intersects(DamageType::Physical));
    d |= DamageType::Fire;
    d &= !DamageType::Slash;
    assert_eq!(d, DamageType::Pierce | DamageType::Fire);
    d ^= DamageType::Pierce;
    assert_eq!(d, DamageType::Fire);
    assert!(DamageType::Undef.is_empty());
    assert_eq!((DamageType::Cold & DamageType::Elemental).bits(), 0x8);
}

#[test]
fn rust_keyword_members_are_renamed_but_keep_their_ace_name() {
    assert_eq!(PropertyAttribute::Self_.0, 6);
    assert_eq!(PropertyAttribute::Self_.name(), Some("Self"));
    assert_eq!(
        PropertyAttribute::from_name("Self"),
        Some(PropertyAttribute::Self_)
    );
    assert_eq!(TradeSide::Self_.0, 1);
}

#[test]
fn boolean_attributes_are_queries() {
    assert!(PropertyInt::CoinValue.is_send_on_login());
    assert!(PropertyInt::CoinValue.is_ephemeral());
    assert!(PropertyInt::EncumbranceVal.is_assessment_property());
    assert!(PropertyInt::EncumbranceVal.is_send_on_login());
    assert!(PropertyInt::CreatureType.is_assessment_property());
    assert!(!PropertyInt::CreatureType.is_send_on_login());
    assert!(!PropertyInt::Level.is_ephemeral());
    // Counts from ACE's source (an independent regex count agrees: 176/146/48 in all).
    assert_eq!(PropertyInt::ASSESSMENT_PROPERTY.len(), 122);
    assert_eq!(PropertyInt::SEND_ON_LOGIN.len(), 119);
    assert_eq!(PropertyInt::EPHEMERAL.len(), 9);
    assert_eq!(PositionType::EPHEMERAL.len(), 1);
    // Declaration order, as ACE's reflection returns it.
    assert_eq!(PropertyInt::SEND_ON_LOGIN[0], PropertyInt::EncumbranceVal);
}

#[test]
fn valued_attributes_are_typed_queries() {
    assert_eq!(
        CharacterOption::AutoRepeatAttacks.character_options1(),
        Some(CharacterOptions1::AutoRepeatAttack)
    );
    assert_eq!(
        CharacterOption::AutoRepeatAttacks.character_options2(),
        None
    );
    assert_eq!(
        CharacterOption::AlwaysDaylightOutdoors.character_options2(),
        Some(CharacterOptions2::PersistentAtDay)
    );
    assert_eq!(
        CharacterOption::AlwaysDaylightOutdoors.character_options1(),
        None
    );
    assert_eq!(
        CharacterOption::CharacterOptions2Default.character_options2(),
        Some(CharacterOptions2::Default)
    );
}

#[test]
fn get_name_converts_like_dotnet_to_uint64() {
    assert_eq!(get_name::<Skill>(6), Some("MeleeDefense"));
    assert_eq!(get_name::<MotionCommand>(0x4100_0003), Some("Ready"));
    assert_eq!(get_name::<AnimationHookDir>(-2), Some("Unknown"));
    assert_eq!(get_name::<Skill>(-1), None);
}

#[test]
fn enums_convert_losslessly_to_and_from_integers() {
    // to and from the storage type
    assert_eq!(u16::from(PropertyInt::Level), 25);
    assert_eq!(PropertyInt::from(25u16), PropertyInt::Level);
    assert_eq!(i32::from(Skill::Summoning), 54);
    assert_eq!(Skill::from(54), Skill::Summoning);
    // into the wider integers the storage type widens into
    assert_eq!(u32::from(PropertyInt::Level), 25);
    assert_eq!(i32::from(PropertyInt::Level), 25);
    let mc: u32 = MotionCommand::Ready.into();
    assert_eq!(mc, 0x4100_0003);
    assert_eq!(u64::from(MotionCommand::Ready), 0x4100_0003);
    assert_eq!(
        i64::from(AnimationHookDir::Unknown),
        -2,
        "a signed enum sign-extends"
    );
    assert_eq!(u32::from(CombatUse::Melee), u32::from(CombatUse::Melee.0));
}
