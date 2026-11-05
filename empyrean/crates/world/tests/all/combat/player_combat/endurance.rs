//! Divergence: V470
//! Launch omits attribute-derived benefits while preserving other vital and resistance factors.
//! Fixture: isolated combatants, explicit enchantments and seeded random draws.

use super::*;
use dereth_primitives::era::EraId;
use empyrean_common::era::{with_features, EraFeatures};
use empyrean_entity::enums::{EquipMask, ParentLocation};
use empyrean_world::world_objects::{creature_properties, creature_vitals};

fn actor(
    era: EraId,
    class: &str,
    endurance: u32,
    trained: u32,
    combat: bool,
) -> (World, ObjectGuid) {
    let mut w = world();
    w.era = era.rules();
    let mut input = spec(class, 0x5000_0001, None, endurance, trained, combat);
    input["skills"]
        .as_array_mut()
        .unwrap()
        .push(json!([Skill::MissileDefense.0, trained, 200, 0]));
    let p = combatant_in(&mut w, &input);
    if class == "Player" {
        with_experience(&mut w, p);
        with_session(&mut w, p);
    }
    (w, p)
}

fn enchant(w: &mut World, p: ObjectGuid, values: &[(PropertyFloat, f32)]) {
    w.objects
        .get_mut(p)
        .unwrap()
        .biota
        .properties_enchantment_registry = Some(
        values
            .iter()
            .enumerate()
            .map(|(i, (key, value))| PropertiesEnchantmentRegistry {
                spell_id: i32::try_from(i + 1).unwrap(),
                layer_id: 1,
                spell_category: SpellCategory(u32::try_from(i + 1).unwrap()),
                power_level: 100,
                stat_mod_type: EnchantmentTypeFlags(0x5008),
                stat_mod_key: u32::from(key.0),
                stat_mod_value: *value,
                ..PropertiesEnchantmentRegistry::default()
            })
            .collect(),
    );
}

#[test]
fn launch_attack_costs_ignore_endurance_for_melee_and_missile_slots() {
    for era in EraId::ALL {
        for endurance in [10, 50, 170, 290, 500] {
            for (class, slot) in [
                (Class::MeleeWeapon, EquipMask::MeleeWeapon),
                (Class::MissileLauncher, EquipMask::MissileWeapon),
                (Class::Missile, EquipMask::MissileWeapon),
            ] {
                let (mut w, p) = actor(era, "Player", endurance, 2, true);
                let mut item = WorldObject::allocate(class);
                item.guid = ObjectGuid::new(0x8000_0020);
                item.set_current_wielded_location(Some(slot));
                item.set_parent_location(Some(ParentLocation::RightHand));
                item.set_encumbrance_val(Some(3200));
                let item_id = item.guid;
                w.objects.insert(item).unwrap();
                w.objects
                    .get_mut(p)
                    .unwrap()
                    .creature
                    .as_mut()
                    .unwrap()
                    .creature_equipment
                    .equipped_objects
                    .insert(item_id, ());
                assert_eq!(player_combat::get_held_item_burden(&w, p), 3200);
                let expected = if era == EraId::Launch || endurance <= 50 {
                    [3, 6, 12]
                } else if endurance == 170 {
                    [2, 4, 9]
                } else {
                    [2, 3, 6]
                };
                for (bar, cost) in [
                    PowerAccuracy::Low,
                    PowerAccuracy::Medium,
                    PowerAccuracy::High,
                ]
                .into_iter()
                .zip(expected)
                {
                    assert_eq!(
                        player_combat::get_attack_stamina(&w, p, bar),
                        cost,
                        "{era:?}, endurance {endurance}, {slot:?}, {bar:?}"
                    );
                }
                w.objects
                    .get_mut(item_id)
                    .unwrap()
                    .set_encumbrance_val(Some(0));
                assert_eq!(
                    player_combat::get_attack_stamina(&w, p, PowerAccuracy::High),
                    1
                );
            }
        }
    }
}

#[test]
fn launch_evades_spend_stamina_in_combat_regardless_of_endurance_or_training() {
    for era in EraId::ALL {
        for attack in [CombatType::Melee, CombatType::Missile] {
            for trained in [1, 2, 3] {
                for combat in [false, true] {
                    for seed in 0..12 {
                        let (mut w, p) = actor(era, "Player", 500, trained, combat);
                        let m = combatant_in(
                            &mut w,
                            &spec("Creature", 0x8000_0001, None, 100, 2, false),
                        );
                        ThreadSafeRandom::seed(seed);
                        let roll = ThreadSafeRandom::next_float(0.0, 1.0);
                        let expected = u32::from(
                            combat && (era == EraId::Launch || trained < 2 || roll >= 0.75),
                        );
                        ThreadSafeRandom::seed(seed);
                        start_capture();
                        player_combat::on_evade(&mut w, p, m, attack);
                        take_sent();
                        let o = w.objects.get(p).unwrap();
                        assert_eq!(
                            100 - o.stamina().current(o),
                            expected,
                            "{era:?}, {attack:?}, trained {trained}, combat {combat}, seed {seed}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn launch_regeneration_keeps_stance_and_spells_without_the_attribute_bonus() {
    for era in EraId::ALL {
        for class in ["Player", "Creature"] {
            for combat in [false, true] {
                let (mut w, p) = actor(era, class, 500, 2, combat);
                enchant(
                    &mut w,
                    p,
                    &[
                        (PropertyFloat::HealthRate, 2.0),
                        (PropertyFloat::StaminaRate, 2.0),
                    ],
                );
                let o = w.objects.get_mut(p).unwrap();
                for vital in [
                    PropertyAttribute2nd::MaxHealth,
                    PropertyAttribute2nd::MaxStamina,
                    PropertyAttribute2nd::MaxMana,
                ] {
                    let v = *o.vitals().get(&vital).unwrap();
                    v.set_current(o, 10);
                    o.vital_mut(vital).regen_rate = 10.0;
                }
                if let Some(player) = o.player.as_mut() {
                    player.player_tick.heartbeat_elapsed = Some(5.0);
                }
                let stance = if class == "Player" && combat {
                    0.5
                } else {
                    1.0
                };
                let health = if class == "Player" && era != EraId::Launch {
                    2.1_f32
                } else {
                    1.0
                };
                start_capture();
                for (vital, expected) in [
                    (
                        PropertyAttribute2nd::MaxHealth,
                        20.0 * stance * f64::from(health),
                    ),
                    (PropertyAttribute2nd::MaxStamina, 20.0 * stance),
                    (PropertyAttribute2nd::MaxMana, 10.0),
                ] {
                    assert!(creature_vitals::vital_heart_beat(&mut w, p, vital));
                    let o = w.objects.get(p).unwrap();
                    let v = o.vitals().get(&vital).unwrap();
                    let actual = f64::from(v.current(o) - 10) + v.partial_regen;
                    assert!(
                        (actual - expected).abs() < 0.0001,
                        "{era:?}, {class}, combat {combat}, {vital:?}: {actual} vs {expected}"
                    );
                }
                take_sent();
            }
        }
    }
}

#[test]
fn launch_resistance_preserves_protections_vulnerabilities_and_explicit_drain_modifiers() {
    for era in EraId::ALL {
        for dtype in [
            DamageType::Slash,
            DamageType::Pierce,
            DamageType::Bludgeon,
            DamageType::Fire,
            DamageType::Cold,
            DamageType::Acid,
            DamageType::Electric,
        ] {
            let (mut w, p) = actor(era, "Player", 500, 2, true);
            assert_eq!(
                player_combat::get_natural_resistance(&w, p, dtype),
                if era == EraId::Launch { 1.0 } else { 0.5 }
            );
            assert_eq!(
                creature_properties::get_resistance_mod_damage(&mut w, p, dtype, None, None, 1.0),
                if era == EraId::Launch { 1.0 } else { 0.5 }
            );
        }
        for (protection, vulnerability) in [(0.75, 1.0), (0.25, 1.0), (0.75, 2.0)] {
            let (mut w, p) = actor(era, "Player", 500, 2, true);
            enchant(
                &mut w,
                p,
                &[
                    (PropertyFloat::ResistSlash, protection),
                    (PropertyFloat::ResistSlash, vulnerability),
                ],
            );
            let expected = if era == EraId::Launch {
                protection
            } else {
                protection.min(0.5)
            } * vulnerability;
            assert_eq!(
                creature_properties::get_resistance_mod_damage(
                    &mut w,
                    p,
                    DamageType::Slash,
                    None,
                    None,
                    1.0
                ),
                expected
            );
        }
        let (mut w, p) = actor(era, "Player", 500, 2, true);
        let o = w.objects.get_mut(p).unwrap();
        o.set_resist_health_drain(Some(0.8));
        o.set_resist_stamina_drain(Some(0.8));
        o.set_resist_mana_drain(Some(0.8));
        o.set_resist_health_boost(Some(1.25));
        for resistance in [
            ResistanceType::HealthDrain,
            ResistanceType::StaminaDrain,
            ResistanceType::ManaDrain,
        ] {
            let got =
                creature_properties::get_resistance_mod(&mut w, p, resistance, None, None, 1.0);
            assert_eq!(got, if era == EraId::Launch { 0.8 } else { 0.4 });
        }
        assert_eq!(
            creature_properties::get_resistance_mod(
                &mut w,
                p,
                ResistanceType::HealthBoost,
                None,
                None,
                1.0
            ),
            1.25
        );
        assert_eq!(
            player_combat::get_natural_resistance(&w, p, DamageType::Nether),
            0.5
        );
        assert_eq!(
            player_combat::get_natural_resistance(&w, p, DamageType::Undef),
            1.0
        );
        assert_eq!(
            player_combat::get_natural_resistance_string(&w, p, ResistanceType::Slash),
            if era == EraId::Launch {
                "None"
            } else {
                "Indomitable"
            }
        );
        assert_eq!(
            player_combat::get_regen_bonus_string(&w, p),
            if era == EraId::Launch {
                "None"
            } else {
                "Indomitable"
            }
        );
    }
}

/// A world's `[era]` setting turns the benefits on in Launch, and off at the end of retail, all
/// of them together.
#[test]
fn a_world_turns_the_endurance_benefits_on_and_off_whatever_its_era() {
    for (era, on) in [(EraId::Launch, true), (EraId::Eor, false)] {
        let (mut w, p) = actor(era, "Player", 500, 2, true);
        let features = EraFeatures {
            endurance_benefits: on,
            ..era.features()
        };
        w.era = with_features(era.rules(), features);
        let expected = if on { 0.5 } else { 1.0 };
        assert_eq!(
            player_combat::get_natural_resistance(&w, p, DamageType::Slash),
            expected,
            "{era}"
        );
        assert_eq!(
            player_combat::get_natural_resistance(&w, p, DamageType::Health),
            expected,
            "{era}"
        );
        assert_eq!(player_combat::get_stamina_mod(&w, p) < 1.0, on, "{era}");
        assert_eq!(
            player_combat::get_regen_bonus_string(&w, p) != "None",
            on,
            "{era}"
        );
    }
}
