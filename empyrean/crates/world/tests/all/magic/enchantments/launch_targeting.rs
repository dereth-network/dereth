//! Divergence: V469
//! Fixture: synthetic item spells and equipment, with exact registry and mana expectations.

use super::*;
use empyrean_common::era::{with_features, EraId};
use empyrean_entity::enums::{CombatUse, EquipMask, ItemType, SpellType, WeenieType};
use empyrean_world::world_objects::{creature_magic, player_magic, world_object_magic};

const IMPEN: u32 = 51;
const BANE: u32 = 100;
const ARMOR: ObjectGuid = ObjectGuid::new(0x8000_0020);
const SHIRT: ObjectGuid = ObjectGuid::new(0x8000_0021);
const SHIELD: ObjectGuid = ObjectGuid::new(0x8000_0022);
const PACKED: ObjectGuid = ObjectGuid::new(0x8000_0023);
const IMMUNE: ObjectGuid = ObjectGuid::new(0x8000_0024);
const ALL: [ObjectGuid; 8] = [
    PLAYER,
    OTHER_PLAYER,
    ARMOR,
    SHIRT,
    SHIELD,
    PACKED,
    IMMUNE,
    SWORD,
];

fn fixture(era: EraId) -> World {
    let mut defs = Vec::new();
    for (id, category, target) in [
        (
            IMPEN,
            SpellCategory::ArmorValueRaising,
            ItemType::Vestements,
        ),
        (
            BANE,
            SpellCategory::FireResistanceRaising,
            ItemType::Vestements,
        ),
        (
            SWIFT_KILLER_I,
            SpellCategory::WeaponTimeRaising,
            ItemType::Weapon,
        ),
    ] {
        let mut def = d(
            id,
            category.0,
            10,
            300.0,
            true,
            ADD_INT,
            u32::from(PropertyInt::ArmorLevel.0),
            20.0,
        );
        def.school = 3;
        def.target = target.0;
        defs.push(def);
    }
    let mut w = world_with(&defs, Vec::new(), BTreeMap::new());
    w.era = era.rules();
    w.sessions.get_mut(S).unwrap().account = Some("caster".into());
    w.sessions.insert(
        SessionId {
            client_id: 2,
            generation: 1,
        },
        SessionData {
            player: Some(OTHER_PLAYER),
            account: Some("other".into()),
            ..SessionData::default()
        },
    );
    for (guid, class, slot) in [
        (ARMOR, Class::Clothing, Some(EquipMask::ChestArmor)),
        (SHIRT, Class::Clothing, Some(EquipMask::ChestWear)),
        (SHIELD, Class::Clothing, Some(EquipMask::Shield)),
        (PACKED, Class::Clothing, None),
        (IMMUNE, Class::Clothing, Some(EquipMask::HeadWear)),
        (SWORD, Class::MeleeWeapon, Some(EquipMask::MeleeWeapon)),
    ] {
        let mut o = WorldObject::allocate(class);
        o.guid = guid;
        o.biota.id = guid.full();
        o.biota.weenie_type = if class == Class::Clothing {
            WeenieType::Clothing
        } else {
            WeenieType::MeleeWeapon
        };
        o.biota.properties_enchantment_registry = Some(Vec::new());
        o.set_current_wielded_location(slot);
        if guid == SHIELD {
            o.set_combat_use(Some(CombatUse::Shield));
        }
        if guid == IMMUNE {
            o.set_resist_magic(Some(9999));
        }
        if guid == PACKED {
            o.set_container_id(Some(PLAYER.full()));
        }
        w.objects.insert(o).unwrap();
        if slot.is_some() {
            let equipment = &mut w
                .objects
                .get_mut(PLAYER)
                .unwrap()
                .creature
                .as_mut()
                .unwrap()
                .creature_equipment;
            equipment.equipped_objects.insert(guid, ());
            equipment.equipped_objects_loaded = true;
        }
    }
    w
}

fn affected(w: &World, id: u32) -> Vec<ObjectGuid> {
    ALL.into_iter().filter(|&guid| holds(w, guid, id)).collect()
}

#[test]
fn launch_refuses_creature_targets_before_cast_costs_and_effects() {
    for id in [IMPEN, BANE, SWIFT_KILLER_I] {
        for target in [PLAYER, OTHER_PLAYER, MONSTER] {
            let mut w = fixture(EraId::Launch);
            let s = spell(&w, id);
            start_capture();
            assert!(!player_magic::verify_spell_target(
                &mut w, PLAYER, &s, target
            ));
            assert!(!take_sent().is_empty(), "the caller receives a refusal");
            // Exercise the cast entry point too: a built-in spell bypasses components,
            // then target validation refuses before skill, mana or windup processing.
            assert!(!player_magic::create_player_spell(
                &mut w,
                PLAYER,
                target,
                player_magic::TargetCategory::Self_,
                id,
                Some(SWORD),
            ));
            take_sent();
            world_object_magic::try_cast_item_enchantment_with_redirects(
                &mut w,
                PLAYER,
                &s,
                Some(target),
                None,
            );
            assert!(affected(&w, id).is_empty());
            assert!(take_sent().is_empty(), "dispatch performs no effects");
            assert!(!player_magic::fields(&w, PLAYER).magic_state.is_casting);
        }
    }
}

#[test]
fn launch_individual_armor_shield_weapon_and_packed_items_take_only_their_spell() {
    for (id, target) in [
        (IMPEN, ARMOR),
        (BANE, SHIRT),
        (IMPEN, SHIELD),
        (IMPEN, PACKED),
        (SWIFT_KILLER_I, SWORD),
    ] {
        let mut w = fixture(EraId::Launch);
        let s = spell(&w, id);
        assert!(!player_magic::is_invalid_target(&mut w, PLAYER, &s, target));
        world_object_magic::try_cast_item_enchantment_with_redirects(
            &mut w,
            PLAYER,
            &s,
            Some(target),
            None,
        );
        assert_eq!(affected(&w, id), [target]);
    }
}

#[test]
fn launch_still_rejects_wrong_item_types_and_unenchantable_items() {
    let mut w = fixture(EraId::Launch);
    for (id, target) in [(IMPEN, SWORD), (SWIFT_KILLER_I, ARMOR), (IMPEN, IMMUNE)] {
        let s = spell(&w, id);
        assert!(player_magic::is_invalid_target(&mut w, PLAYER, &s, target));
    }
}

#[test]
fn later_eras_keep_self_armor_expansion_without_touching_packed_or_immune_items() {
    for era in [EraId::Infiltration, EraId::Eor] {
        for id in [IMPEN, BANE] {
            let mut w = fixture(era);
            let s = spell(&w, id);
            assert!(!player_magic::is_invalid_target(&mut w, PLAYER, &s, PLAYER));
            world_object_magic::try_cast_item_enchantment_with_redirects(
                &mut w,
                PLAYER,
                &s,
                Some(PLAYER),
                None,
            );
            assert_eq!(affected(&w, id), [ARMOR, SHIRT, SHIELD]);
        }
    }
}

#[test]
fn another_player_is_not_a_launch_shortcut_to_their_shield() {
    for era in EraId::ALL {
        let mut w = fixture(era);
        let shield = w.objects.get_mut(SHIELD).unwrap();
        shield.wielder = Some(OTHER_PLAYER);
        shield.set_wielder_id(Some(OTHER_PLAYER.full()));
        w.objects
            .get_mut(PLAYER)
            .unwrap()
            .creature
            .as_mut()
            .unwrap()
            .creature_equipment
            .equipped_objects
            .clear();
        w.objects
            .get_mut(OTHER_PLAYER)
            .unwrap()
            .creature
            .as_mut()
            .unwrap()
            .creature_equipment
            .equipped_objects
            .insert(SHIELD, ());
        let s = spell(&w, IMPEN);
        world_object_magic::try_cast_item_enchantment_with_redirects(
            &mut w,
            PLAYER,
            &s,
            Some(OTHER_PLAYER),
            None,
        );
        assert_eq!(
            affected(&w, IMPEN),
            if era == EraId::Launch {
                vec![]
            } else {
                vec![SHIELD]
            }
        );
        // Explicitly selecting that item continues to enchant just it.
        world_object_magic::try_cast_item_enchantment_with_redirects(
            &mut w,
            PLAYER,
            &s,
            Some(SHIELD),
            None,
        );
        assert_eq!(affected(&w, IMPEN), [SHIELD]);
    }
}

#[test]
fn equipped_item_mana_surcharge_follows_redirection_not_auras() {
    for era in EraId::ALL {
        let mut w = fixture(era);
        let mut s = spell(&w, IMPEN);
        let base = s.spell_base.as_mut().unwrap();
        base.base_mana = 10;
        base.mana_mod = 2;
        assert_eq!(
            creature_magic::calculate_mana_usage(&mut w, PLAYER, PLAYER, &s, Some(PLAYER)),
            if era == EraId::Launch { 10 } else { 16 }
        );
        assert_eq!(
            creature_magic::calculate_mana_usage(&mut w, PLAYER, PLAYER, &s, Some(ARMOR)),
            10
        );
    }
}

#[test]
fn aura_override_does_not_enable_launch_item_redirection() {
    let mut w = fixture(EraId::Launch);
    let mut features = w.era.features;
    features.item_spell_auras = true;
    w.era = with_features(w.era, features);
    let s = spell(&w, IMPEN);
    assert!(player_magic::is_invalid_target(&mut w, PLAYER, &s, PLAYER));
    world_object_magic::try_cast_item_enchantment_with_redirects(
        &mut w,
        PLAYER,
        &s,
        Some(PLAYER),
        None,
    );
    assert!(affected(&w, IMPEN).is_empty());
}

#[test]
fn creature_typed_equipment_spells_cannot_bypass_launch_validation_but_portals_can() {
    let mut w = fixture(EraId::Launch);
    for id in [IMPEN, SWIFT_KILLER_I] {
        let mut s = spell(&w, id);
        s.spell_base.as_mut().unwrap().non_component_target_type = ItemType::Creature.0;
        assert!(player_magic::is_invalid_target(&mut w, PLAYER, &s, PLAYER));
        world_object_magic::try_cast_item_enchantment_with_redirects(
            &mut w,
            PLAYER,
            &s,
            Some(PLAYER),
            None,
        );
        assert!(affected(&w, id).is_empty());
        s.spell_base.as_mut().unwrap().meta_spell_type = SpellType::PortalRecall.0.cast_unsigned();
        assert!(!player_magic::is_invalid_target(&mut w, PLAYER, &s, PLAYER));
    }
}

#[test]
fn launch_item_activation_still_enchants_the_item() {
    let mut w = fixture(EraId::Launch);
    assert!(creature_magic::create_item_spell(
        &mut w, PLAYER, ARMOR, IMPEN
    ));
    assert_eq!(affected(&w, IMPEN), [ARMOR]);
    creature_magic::remove_item_spell(&mut w, PLAYER, Some(ARMOR), IMPEN, true);
    assert!(affected(&w, IMPEN).is_empty());
}
