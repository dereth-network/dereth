//! Divergence: V557, V558
//! Assessing a creature or a player: what a reply carries and how a pet is rolled.
//! Fixture: synthetic dats and isolated world state, with a seeded roll.

use std::sync::Arc;

use empyrean_common::dotnet::DotNetDict;
use empyrean_common::era::{EraExt, EraId};
use empyrean_entity::enums::{
    CombatBodyPart, FactionBits, IdentifyResponseFlags, PropertyBool, PropertyInt, Skill,
    SkillAdvancementClass,
};
use empyrean_entity::models::properties_skill::PropertiesSkill;
use empyrean_entity::models::PropertiesBodyPart;
use empyrean_entity::ObjectGuid;
use empyrean_world::dispatch::Class;
use empyrean_world::network::game_event::game_event_type::GameEventType;
use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
use empyrean_world::network::game_messages::game_message_opcode::GameMessageOpcode;
use empyrean_world::network::structure::appraise_info::{appraise_info_new, AppraiseInfo};
use empyrean_world::network::structure::armor_level;
use empyrean_world::world_objects::player;

use crate::monsters::monster_ai::{H, MONSTER, PLAYER};

const PET: ObjectGuid = ObjectGuid::new(0x8000_0300);

fn set_skill(h: &mut H, g: ObjectGuid, skill: Skill, level: u32) {
    h.w.objects
        .get_mut(g)
        .expect("the object")
        .biota
        .properties_skill
        .get_or_insert_with(DotNetDict::new)
        .insert(
            skill,
            PropertiesSkill {
                init_level: level,
                sac: SkillAdvancementClass::Trained,
                ..PropertiesSkill::default()
            },
        );
}

/// The success flag of every assessment reply sent since the capture started.
fn assessments_sent() -> Vec<(u32, bool)> {
    take_sent()
        .into_iter()
        .filter_map(|(_, _, b)| {
            let word = |at: usize| u32::from_le_bytes(b[at..at + 4].try_into().unwrap());
            (word(0) == GameMessageOpcode::GameEvent.0
                && word(12) == GameEventType::IdentifyObjectResponse.0)
                .then(|| (word(16), word(24) != 0))
        })
        .collect()
}

/// A player with Assess Creature at 100 beside a pet of `class` with Deception at `deception`.
fn beside_a_pet(class: Class, deception: u32) -> H {
    let mut h = H::new();
    h.player(10.0, 10.0);
    set_skill(&mut h, PLAYER, Skill::AssessCreature, 100);
    h.creature(class, PET, "Pet", 11.0, 10.0);
    set_skill(&mut h, PET, Skill::Deception, deception);
    h
}

/// Divergence: V558
/// A pet or a combat pet is assessed by the same roll as any creature: a player far below its
/// Deception fails to assess it, and one with no Deception to beat succeeds.
#[test]
fn a_pet_is_assessed_by_the_same_roll_as_any_creature() {
    for class in [Class::Pet, Class::CombatPet] {
        let mut h = beside_a_pet(class, 500);
        start_capture();
        for _ in 0..5 {
            player::examine(&mut h.w, PLAYER, PET);
        }
        assert_eq!(
            assessments_sent(),
            vec![(PET.full(), false); 5],
            "{class:?} against Deception 500"
        );

        let mut h = beside_a_pet(class, 0);
        start_capture();
        player::examine(&mut h.w, PLAYER, PET);
        assert_eq!(
            assessments_sent(),
            [(PET.full(), true)],
            "{class:?} with no Deception"
        );
    }
}

/// A monster carrying a faction, a society rank, a stored rating and a level, with armor on its
/// head and chest.
fn ranked_monster() -> H {
    let mut h = H::new();
    h.player(10.0, 10.0);
    h.monster(12.0, 10.0);
    let o = h.w.objects.get_mut(MONSTER).unwrap();
    o.set_property(PropertyInt::Faction1Bits, FactionBits::RadiantBlood.0);
    o.set_property(PropertyInt::SocietyRankRadblo, 301);
    o.set_property(PropertyInt::DamageRating, 12);
    o.set_property(PropertyInt::CreatureType, 1);
    o.set_property(PropertyInt::Level, 80);
    let mut parts = DotNetDict::new();
    for (part, armor) in [(CombatBodyPart::Head, 200), (CombatBodyPart::Chest, 150)] {
        parts.insert(
            part,
            PropertiesBodyPart {
                base_armor: armor,
                ..PropertiesBodyPart::default()
            },
        );
    }
    o.biota.properties_body_part = Some(Arc::new(parts));
    h
}

fn ints(info: &AppraiseInfo) -> Vec<PropertyInt> {
    let mut keys: Vec<PropertyInt> = info.properties_int.keys().copied().collect();
    keys.sort_by_key(|p| p.0);
    keys
}

/// Divergence: V557
/// A failed assessment of a creature shows its type and level but no faction, society rank or
/// rating, even one it stores; a successful one shows them all.
#[test]
fn a_failed_assessment_shows_no_faction_society_rank_or_rating() {
    let mut h = ranked_monster();
    let failed = appraise_info_new(&mut h.w, MONSTER, PLAYER, false);
    assert_eq!(
        ints(&failed),
        [PropertyInt::CreatureType, PropertyInt::Level],
        "{:?}",
        failed.properties_int
    );
    assert!(failed.armor_levels.is_none());

    let shown = appraise_info_new(&mut h.w, MONSTER, PLAYER, true);
    for p in [
        PropertyInt::Faction1Bits,
        PropertyInt::SocietyRankRadblo,
        PropertyInt::DamageRating,
        PropertyInt::Level,
    ] {
        assert!(shown.properties_int.contains_key(&p), "{p:?}");
    }
}

/// Divergence: V557
/// A failed assessment of a player keeps what a success shows of it but the faction and society
/// ranks; its level is still shown.
#[test]
fn a_failed_assessment_of_a_player_hides_its_faction_and_society_rank() {
    let mut h = ranked_monster();
    let o = h.w.objects.get_mut(PLAYER).unwrap();
    o.set_property(PropertyInt::Faction1Bits, FactionBits::CelestialHand.0);
    o.set_property(PropertyInt::SocietyRankCelhan, 101);
    o.set_property(PropertyInt::Level, 120);
    o.set_property(PropertyInt::Gender, 1);
    let failed = appraise_info_new(&mut h.w, PLAYER, MONSTER, false);
    let shown = appraise_info_new(&mut h.w, PLAYER, MONSTER, true);
    let mut kept = ints(&shown);
    kept.retain(|p| *p != PropertyInt::Faction1Bits && *p != PropertyInt::SocietyRankCelhan);
    assert_eq!(ints(&failed), kept);
    assert!(kept.contains(&PropertyInt::Level) && kept.contains(&PropertyInt::Gender));
}

/// Divergence: V557, V394
/// A monster assessed successfully shows its armor levels, its body's armor in the places it has
/// a body part, a weak body's negative armor as it is; a player's are what it wears alone. An era
/// before assessment showed armor levels shows none.
#[test]
fn a_monster_assessed_successfully_shows_its_bodys_armor_levels() {
    let mut h = ranked_monster();
    assert!(h.o(MONSTER).attackable());
    let levels = appraise_info_new(&mut h.w, MONSTER, PLAYER, true)
        .armor_levels
        .expect("the monster's armor levels");
    assert_eq!(
        (levels.head, levels.chest, levels.abdomen, levels.foot),
        (200, 150, 0, 0)
    );

    let parts = h.o(MONSTER).biota.properties_body_part.clone();
    let player = h.w.objects.get_mut(PLAYER).unwrap();
    player.biota.properties_body_part = parts;
    assert_eq!(
        appraise_info_new(&mut h.w, PLAYER, MONSTER, true).armor_levels,
        None,
        "a player's come from its armor alone, and it wears none"
    );

    // A weak body's negative armor is sent as it is, and is no zero.
    let mut weak = h
        .o(MONSTER)
        .biota
        .properties_body_part
        .as_deref()
        .cloned()
        .unwrap();
    weak.get_mut(&CombatBodyPart::Head).unwrap().base_armor = -300;
    h.w.objects
        .get_mut(MONSTER)
        .unwrap()
        .biota
        .properties_body_part = Some(Arc::new(weak));
    let levels = appraise_info_new(&mut h.w, MONSTER, PLAYER, true)
        .armor_levels
        .expect("the monster's armor levels");
    assert_eq!(levels.head.cast_signed(), -300);
    assert_eq!(levels.head, 0xFFFF_FED4);

    h.w.era = EraId::Infiltration.rules();
    let old = appraise_info_new(&mut h.w, MONSTER, PLAYER, true);
    assert!(old.armor_levels.is_none());
    assert!(old.creature_profile.is_some());
    h.w.objects
        .get_mut(MONSTER)
        .unwrap()
        .set_property(PropertyBool::Attackable, false);
    assert!(appraise_info_new(&mut h.w, MONSTER, PLAYER, true)
        .armor_levels
        .is_none());
}

/// Whether a successful assessment of `target` by `examiner` carries armor levels, flagged as such.
fn shows_armor_levels(h: &mut H, target: ObjectGuid, examiner: ObjectGuid) -> bool {
    let info = appraise_info_new(&mut h.w, target, examiner, true);
    let flagged = info.flags.0 & IdentifyResponseFlags::ArmorLevels.0 != 0;
    assert_eq!(
        flagged,
        info.armor_levels.is_some(),
        "the flag follows the levels"
    );
    flagged
}

/// Divergence: V557
/// A successful assessment leaves out the armor levels when all nine places are zero: a creature
/// with no body armor and nothing worn, attackable or not, and a player wearing nothing show none.
/// A creature armored in a single place still shows all nine, and so does one whose body armor is
/// below zero, which is not zero.
#[test]
fn a_successful_assessment_leaves_out_armor_levels_that_are_all_zero() {
    let mut h = ranked_monster();
    let levels = appraise_info_new(&mut h.w, MONSTER, PLAYER, true)
        .armor_levels
        .expect("the armored monster's armor levels");
    assert_eq!(
        armor_level::record(&levels),
        [200, 150, 0, 0, 0, 0, 0, 0, 0]
    );

    let mut h = H::new();
    h.player(10.0, 10.0);
    h.monster(12.0, 10.0);
    assert!(h.o(MONSTER).attackable());
    assert!(!shows_armor_levels(&mut h, MONSTER, PLAYER), "attackable");
    h.w.objects
        .get_mut(MONSTER)
        .unwrap()
        .set_property(PropertyBool::Attackable, false);
    assert!(!shows_armor_levels(&mut h, MONSTER, PLAYER), "unattackable");

    let mut parts = DotNetDict::new();
    parts.insert(
        CombatBodyPart::Head,
        PropertiesBodyPart {
            base_armor: 30,
            ..PropertiesBodyPart::default()
        },
    );
    h.w.objects
        .get_mut(MONSTER)
        .unwrap()
        .biota
        .properties_body_part = Some(Arc::new(parts.clone()));
    assert!(shows_armor_levels(&mut h, MONSTER, PLAYER), "armored head");

    for part in parts.values_mut() {
        part.base_armor = -250;
    }
    h.w.objects
        .get_mut(MONSTER)
        .unwrap()
        .biota
        .properties_body_part = Some(Arc::new(parts));
    assert!(
        shows_armor_levels(&mut h, MONSTER, PLAYER),
        "below zero is not zero"
    );

    assert!(
        !shows_armor_levels(&mut h, PLAYER, MONSTER),
        "a player wearing nothing"
    );
}
