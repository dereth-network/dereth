//! Vectors: local player-description quality sets and synthetic biotas in this module
//! A PlayerDescription's qualities go to a biota and back as the same sets; the biota holds wire
//! values under ACE keys; empty biota has only health.
//! Fixture: enum values and synthetic entity records.

use dereth_primitives::ObjectId;
use dereth_protocol::archive::PackedHash;
use dereth_protocol::types::qualities::{
    base_flags, quality_flags, AcBaseQualities, AcQualities, Attribute, AttributeCache,
    Enchantment, EnchantmentRegistry, PropertyTables, SecondaryAttribute, Skill as WireSkill,
    SpellBookPage, StatMod,
};
use dereth_protocol::types::space::{Frame, PositionWire, Quat, Vec3};
use empyrean_entity::adapter::quality_bridge::{
    ac_qualities_to_biota, biota_to_ac_qualities, in_key_order, ENCHANTMENT_CATEGORY_FROM_SPELL,
};
use empyrean_entity::enums::{PropertyInt, SpellId};

fn h<V>(size: u32, entries: Vec<(u32, V)>) -> Option<PackedHash<u32, V>> {
    Some(PackedHash {
        table_size: size,
        entries,
    })
}

fn enchantment(spell: u32, layer: u32, kind: u32, spell_set: Option<u32>) -> Enchantment {
    Enchantment {
        id: spell | (layer << 16),
        category_word: 13 | if spell_set.is_some() { 1 << 16 } else { 0 },
        power_level: 250,
        start_time: -12.5,
        duration: 1800.0,
        caster: ObjectId(0x5000_0001),
        degrade_modifier: 1.0,
        degrade_limit: -666.0,
        last_time_degraded: 0.0,
        smod: StatMod {
            kind,
            key: 6,
            value: 1.25,
        },
        spell_set_id: spell_set,
    }
}

/// A PlayerDescription-shaped record with every block the bridge maps, the way ACE writes them:
/// full attribute cache, skills with format version 1, 4-byte spell pages, the spell-set id sent.
fn sample() -> AcQualities {
    let a = |n: u32| {
        Some(Attribute {
            level_from_cp: n,
            init_level: 10 + n,
            cp_spent: 100 * n,
        })
    };
    let v = |n: u32| {
        Some(SecondaryAttribute {
            attribute: Attribute {
                level_from_cp: n,
                init_level: 0,
                cp_spent: n * 7,
            },
            current_level: 50 + n,
        })
    };
    let tables = PropertyTables {
        ints: h(64, vec![(25, 12), (5, 1500), (218, -3)]),
        int64s: h(64, vec![(1, 5_000_000_000), (2, 17)]),
        bools: h(32, vec![(4, 1), (1, 0)]),
        floats: h(32, vec![(1, 0.5), (167, 1.0e9)]),
        strings: h(
            32,
            vec![(1, "Synthetic".to_owned()), (5, "title".to_owned())],
        ),
        dids: h(32, vec![(1, 0x0200_0001), (3, 0x0400_1234)]),
        iids: h(32, vec![(2, ObjectId(0x8000_0010))]),
        positions: h(
            16,
            vec![(
                14,
                PositionWire {
                    objcell_id: 0xA9B4_0017,
                    frame: Frame {
                        origin: Vec3 {
                            x: 1.5,
                            y: 2.25,
                            z: 94.0,
                        },
                        orientation: Quat {
                            w: 1.0,
                            x: 0.0,
                            y: 0.0,
                            z: 0.0,
                        },
                    },
                },
            )],
        ),
    };
    let flags = base_flags::INT
        | base_flags::INT64
        | base_flags::BOOL
        | base_flags::FLOAT
        | base_flags::STRING
        | base_flags::DID
        | base_flags::IID
        | base_flags::POSITION;
    let mult = 0x4000 | 0x0200_0000 | 0x10;
    let add = 0x8000 | 0x10;
    AcQualities {
        base: AcBaseQualities {
            flags,
            weenie_type: 10,
            tables,
        },
        flags: quality_flags::ATTRIBUTE_CACHE
            | quality_flags::SKILLS
            | quality_flags::SPELL_BOOK
            | quality_flags::ENCHANTMENT_REGISTRY,
        has_health: 1,
        attribute_cache: Some(AttributeCache {
            flags: 0x1FF,
            strength: a(1),
            endurance: a(2),
            quickness: a(3),
            coordination: a(4),
            focus: a(5),
            self_: a(6),
            health: v(1),
            stamina: v(2),
            mana: v(3),
        }),
        skills: h(
            32,
            vec![
                (
                    6,
                    WireSkill {
                        level_from_pp: 40,
                        format_version: 1,
                        sac: 3,
                        pp: 1000,
                        init_level: 10,
                        ..WireSkill::default()
                    },
                ),
                (
                    24,
                    WireSkill {
                        level_from_pp: 0,
                        format_version: 1,
                        sac: 1,
                        ..WireSkill::default()
                    },
                ),
            ],
        ),
        spell_book: h(
            64,
            vec![
                (1635, SpellBookPage::default()),
                (2, SpellBookPage::default()),
            ],
        ),
        enchantments: Some(EnchantmentRegistry {
            flags: EnchantmentRegistry::MULTIPLICATIVE
                | EnchantmentRegistry::ADDITIVE
                | EnchantmentRegistry::COOLDOWN
                | EnchantmentRegistry::VITAE,
            multiplicative: Some(vec![
                enchantment(1635, 2, mult, Some(0)),
                enchantment(2, 1, mult, Some(0)),
            ]),
            additive: Some(vec![enchantment(1234, 1, add, Some(7))]),
            cooldowns: Some(vec![enchantment(0x8005, 1, 0x0100_0000, None)]),
            vitae: Some(enchantment(SpellId::Vitae.0, 0, 0x0080_0000 | 0x4000, None)),
        }),
        event_filter: None,
        creation_profiles: None,
    }
}

#[test]
fn a_player_description_goes_through_a_biota_and_back_as_the_same_sets() {
    let q = sample();
    let back = biota_to_ac_qualities(&ac_qualities_to_biota(&q));
    assert_eq!(in_key_order(&back), in_key_order(&q));
}

#[test]
fn the_biota_holds_the_wire_values_under_aces_keys() {
    let b = ac_qualities_to_biota(&sample());
    assert_eq!(b.weenie_type.0, 10);
    assert_eq!(b.get_property(PropertyInt::EncumbranceVal), Some(1500));
    let rows = b.properties_enchantment_registry.as_ref().unwrap();
    let got: Vec<_> = rows
        .iter()
        .map(|r| {
            (
                r.spell_id,
                r.layer_id,
                r.enchantment_category,
                r.has_spell_set_id,
            )
        })
        .collect();
    // multiplicative, additive, cooldowns, then vitae; vitae is stored as layer 1
    assert_eq!(
        got,
        [
            (1635, 2, ENCHANTMENT_CATEGORY_FROM_SPELL, true),
            (2, 1, ENCHANTMENT_CATEGORY_FROM_SPELL, true),
            (1234, 1, ENCHANTMENT_CATEGORY_FROM_SPELL, true),
            (0x8005, 1, 0x8, false),
            (666, 1, 0x4, false),
        ]
    );
    // and sent as layer 0 again
    let out = biota_to_ac_qualities(&b);
    assert_eq!(
        out.enchantments.unwrap().vitae.unwrap().id,
        SpellId::Vitae.0
    );
}

#[test]
fn an_empty_biota_has_no_tables_and_only_has_health() {
    let q = biota_to_ac_qualities(&empyrean_entity::Biota::default());
    assert_eq!((q.base.flags, q.flags, q.has_health), (0, 0, 1));
    assert!(q.attribute_cache.is_none() && q.skills.is_none() && q.enchantments.is_none());
}
