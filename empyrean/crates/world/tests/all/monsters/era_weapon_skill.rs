//! Divergence: V395
//! Before the 2012 weapon-skill consolidation a creature attacks with its highest old weapon skill
//! of the weapon's kind; at the end of retail it is the weapon's own, converted when the
//! creature lacks it.
//! Fixture: a drudge wielding a sword, synthetic dats, isolated world state.

use empyrean_common::era::EraExt as _;
use empyrean_content::models::world::WeeniePropertiesSkill;
use empyrean_entity::enums::Skill;
use empyrean_world::world_objects::creature_combat;

use crate::support::creature_world::*;

fn skill(object_id: u32, s: Skill, init_level: u32) -> WeeniePropertiesSkill {
    WeeniePropertiesSkill {
        id: 0,
        object_id,
        r#type: u16::try_from(s.0).expect("a skill id"),
        level_from_pp: 0,
        sac: 2,
        pp: 0,
        init_level,
        resistance_at_last_check: 0,
        last_used_time: 0.0,
    }
}

/// A drudge with Sword 10 and Mace 50 wielding a sword whose skill is Sword.
fn world() -> (H, ObjectGuid) {
    let mut h = H::new();
    let mut d = drudge();
    d.weenie_properties_skill = vec![
        skill(DRUDGE_WCID, Skill::Sword, 10),
        skill(DRUDGE_WCID, Skill::Mace, 50),
    ];
    let s = sword().with_int(PropertyInt::WeaponSkill, Skill::Sword.0);
    h.w.content = Arc::new(
        MemContent::new()
            .weenie(d)
            .weenie(s)
            .weenie(generator())
            .weenie(door())
            .weenie(ghost()),
    );
    let g = h.place_new(DRUDGE_WCID, 0x8000_0100);
    (h, g)
}

#[test]
fn a_creature_before_the_consolidation_attacks_with_its_highest_old_melee_skill() {
    let (mut h, g) = world();
    assert!(
        creature_equipment::get_equipped_weapon(&h.w, g, false).is_some(),
        "the drudge wields its sword"
    );
    // End of retail: the sword's own skill, which the drudge has.
    assert_eq!(
        creature_combat::get_current_weapon_skill(&mut h.w, g),
        Skill::Sword
    );
    h.w.era = empyrean_common::era::EraId::Infiltration.rules();
    assert_eq!(
        creature_combat::get_current_weapon_skill(&mut h.w, g),
        Skill::Mace
    );
}
