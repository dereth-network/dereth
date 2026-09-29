//! Backends, a full-collection biota and a canonical order-sensitive dump.
//! Fixture: synthetic account and shard records on the memory and SQLite backends.

use std::sync::Arc;

use empyrean_common::dotnet::{DotNetDict, DotNetHashSet};
use empyrean_entity::enums::*;
use empyrean_entity::models::*;
use empyrean_entity::Biota;
use empyrean_store::{MemShard, ShardDatabase, SqliteShard};

/// Both shard backends, labelled.
pub fn backends() -> Vec<(&'static str, Box<dyn ShardDatabase>)> {
    vec![
        ("mem", Box::new(MemShard::new())),
        (
            "sqlite",
            Box::new(SqliteShard::open_in_memory().expect("open :memory:")),
        ),
    ]
}

fn dict<K: std::hash::Hash + Eq + Clone, V>(items: Vec<(K, V)>) -> DotNetDict<K, V> {
    let mut d = DotNetDict::new();
    for (k, v) in items {
        d.insert(k, v);
    }
    d
}

/// A biota with every collection populated. With `sorted`, dictionary keys are inserted in the
/// order the database reads them back (ascending key); otherwise in descending order.
#[allow(clippy::too_many_lines)]
pub fn rich_biota(id: u32, sorted: bool) -> Biota {
    let o = |mut v: Vec<(u16, i32)>| {
        if !sorted {
            v.reverse();
        }
        v
    };
    let ints = o(vec![(1, 10), (5, -7), (25, 1000), (93, 1_044_563)]);
    let mut b = Biota {
        id,
        weenie_class_id: 1,
        weenie_type: WeenieType::Creature,
        ..Default::default()
    };
    b.properties_int = Some(dict(
        ints.iter().map(|&(k, v)| (PropertyInt(k), v)).collect(),
    ));
    b.properties_int64 = Some(dict(
        o(vec![(1, 0), (2, 0)])
            .iter()
            .map(|&(k, v)| (PropertyInt64(k), i64::from(v) + 5_000_000_000))
            .collect(),
    ));
    b.properties_bool = Some(dict(
        o(vec![(1, 1), (7, 0), (99, 1)])
            .iter()
            .map(|&(k, v)| (PropertyBool(k), v != 0))
            .collect(),
    ));
    b.properties_float = Some(dict(
        o(vec![(3, 0), (11, 0)])
            .iter()
            .map(|&(k, _)| (PropertyFloat(k), f64::from(k) * 1.25 + 0.1))
            .collect(),
    ));
    b.properties_string = Some(dict(
        o(vec![(1, 0), (16, 0)])
            .iter()
            .map(|&(k, _)| (PropertyString(k), format!("s{k} é")))
            .collect(),
    ));
    b.properties_did = Some(dict(
        o(vec![(1, 0), (8, 0)])
            .iter()
            .map(|&(k, _)| (PropertyDataId(k), 0x0200_0000 + u32::from(k)))
            .collect(),
    ));
    b.properties_iid = Some(dict(
        o(vec![(2, 0), (3, 0)])
            .iter()
            .map(|&(k, _)| (PropertyInstanceId(k), 0x8000_0000 + u32::from(k)))
            .collect(),
    ));
    let pos = |c: u32| PropertiesPosition {
        obj_cell_id: c,
        position_x: 10.5,
        position_y: -3.25,
        position_z: 0.005,
        rotation_w: 0.707_106_77,
        rotation_x: 0.0,
        rotation_y: 0.0,
        rotation_z: -0.707_106_77,
    };
    b.properties_position = Some(dict(
        o(vec![(1, 0), (14, 0)])
            .iter()
            .map(|&(k, _)| (PositionType(k), pos(0x7D64_0027 + u32::from(k))))
            .collect(),
    ));
    let mut spells = vec![(1636, 0.5f32), (2000, 1.0), (3, 0.25)];
    spells.sort_by_key(|s| s.0);
    if !sorted {
        spells.reverse();
    }
    b.properties_spell_book = Some(dict(spells));
    b.properties_anim_part = Some(vec![
        PropertiesAnimPart {
            index: 3,
            animation_id: 0x0100_0001,
        },
        PropertiesAnimPart {
            index: 1,
            animation_id: 0x0100_0002,
        },
    ]);
    b.properties_palette = Some(vec![
        PropertiesPalette {
            sub_palette_id: 0x0400_0001,
            offset: 0,
            length: 12,
        },
        PropertiesPalette {
            sub_palette_id: 0x0400_0002,
            offset: 12,
            length: 4,
        },
    ]);
    b.properties_texture_map = Some(vec![PropertiesTextureMap {
        part_index: 2,
        old_texture: 0x0500_0001,
        new_texture: 0x0500_0002,
    }]);
    b.properties_create_list = Some(Arc::new(vec![
        PropertiesCreateList {
            destination_type: DestinationType(2),
            weenie_class_id: 273,
            stack_size: 5,
            palette: -1,
            shade: 0.5,
            try_to_bond: true,
            ..Default::default()
        },
        PropertiesCreateList {
            destination_type: DestinationType(8),
            weenie_class_id: 12,
            stack_size: -1,
            palette: 3,
            shade: 0.0,
            try_to_bond: false,
            ..Default::default()
        },
    ]));
    let action = |t: u32, msg: &str| PropertiesEmoteAction {
        r#type: t,
        delay: 1.5,
        extent: 0.0,
        motion: Some(MotionCommand(0x1000_0057)),
        message: Some(msg.to_owned()),
        min: Some(-3),
        max_64: Some(9_000_000_000),
        percent: Some(0.25),
        p_script: Some(PlayScript(0x93)),
        sound: Some(Sound(0x23)),
        destination_type: Some(-2),
        try_to_bond: Some(false),
        angles_w: Some(1.0),
        ..Default::default()
    };
    b.properties_emote = Some(Arc::new(vec![
        PropertiesEmote {
            category: EmoteCategory(1),
            probability: 1.0,
            weenie_class_id: Some(273),
            style: Some(MotionStance(0x8000_003D)),
            quest: Some("TestQuest".into()),
            vendor_type: Some(VendorType(1)),
            min_health: Some(0.5),
            properties_emote_action: vec![
                action(10, "hello"),
                action(1, "bye"),
                action(5, "third"),
            ],
            ..Default::default()
        },
        PropertiesEmote {
            category: EmoteCategory(6),
            probability: 0.5,
            properties_emote_action: vec![action(2, "x")],
            ..Default::default()
        },
    ]));
    let mut events = DotNetHashSet::new();
    let mut ev = vec![0x0100, 0x0002, 0x0400];
    ev.sort_unstable();
    if !sorted {
        ev.reverse();
    }
    for e in ev {
        events.insert(e);
    }
    b.properties_event_filter = Some(Arc::new(events));
    b.properties_generator = Some(Arc::new(vec![PropertiesGenerator {
        probability: -1.0,
        weenie_class_id: 7,
        delay: Some(600.0),
        init_create: 1,
        max_create: 3,
        when_create: RegenerationType(2),
        where_create: RegenLocationType(4),
        stack_size: Some(1),
        obj_cell_id: Some(0x7D64_0027),
        origin_x: Some(1.0),
        ..Default::default()
    }]));
    b.properties_attribute = Some(dict(
        o(vec![(1, 0), (2, 0), (6, 0)])
            .iter()
            .map(|&(k, _)| {
                (
                    PropertyAttribute(k),
                    PropertiesAttribute {
                        init_level: 100,
                        level_from_cp: u32::from(k),
                        cp_spent: 1234,
                    },
                )
            })
            .collect(),
    ));
    b.properties_attribute_2nd = Some(dict(
        o(vec![(1, 0), (3, 0), (5, 0)])
            .iter()
            .map(|&(k, _)| {
                (
                    PropertyAttribute2nd(k),
                    PropertiesAttribute2nd {
                        init_level: 0,
                        level_from_cp: 1,
                        cp_spent: 2,
                        current_level: 50 + u32::from(k),
                    },
                )
            })
            .collect(),
    ));
    let mut parts: Vec<i32> = vec![0, 1, 2, 8];
    if !sorted {
        parts.reverse();
    }
    b.properties_body_part = Some(Arc::new(dict(
        parts
            .into_iter()
            .map(|k| {
                (
                    CombatBodyPart(k),
                    PropertiesBodyPart {
                        d_type: DamageType(1),
                        d_val: k,
                        d_var: 0.75,
                        base_armor: 10,
                        hlf: 0.1,
                        lrb: 0.9,
                        ..Default::default()
                    },
                )
            })
            .collect(),
    )));
    b.properties_skill = Some(dict(
        o(vec![(6, 0), (15, 0), (44, 0)])
            .iter()
            .map(|&(k, _)| {
                (
                    Skill(i32::from(k)),
                    PropertiesSkill {
                        level_from_pp: k,
                        sac: SkillAdvancementClass(2),
                        pp: 500,
                        init_level: 10,
                        resistance_at_last_check: 3,
                        last_used_time: 1234.5,
                    },
                )
            })
            .collect(),
    ));
    b.properties_book = Some(PropertiesBook {
        max_num_pages: 10,
        max_num_chars_per_page: 1000,
    });
    b.properties_book_page_data = Some(vec![
        PropertiesBookPageData {
            author_id: 1,
            author_name: Some("A".into()),
            author_account: Some("prewritten".into()),
            ignore_author: false,
            page_text: Some("page one".into()),
        },
        PropertiesBookPageData {
            author_id: 2,
            author_name: Some("B".into()),
            author_account: Some("acct".into()),
            ignore_author: true,
            page_text: Some(String::new()),
        },
    ]);
    let mut enchantments = vec![
        PropertiesEnchantmentRegistry {
            spell_id: 2,
            layer_id: 1,
            caster_object_id: 0x5000_0002,
            stat_mod_value: 1.5,
            ..Default::default()
        },
        PropertiesEnchantmentRegistry {
            spell_id: 5,
            layer_id: 1,
            caster_object_id: 0x5000_0001,
            duration: 3600.0,
            stat_mod_type: EnchantmentTypeFlags(0x0000_8009),
            spell_category: SpellCategory(2),
            spell_set_id: EquipmentSet(0),
            ..Default::default()
        },
    ];
    if !sorted {
        enchantments.reverse();
    }
    b.properties_enchantment_registry = Some(enchantments);
    b.house_permissions = Some(dict(if sorted {
        vec![(0x5000_0001, true), (0x5000_0009, false)]
    } else {
        vec![(0x5000_0009, false), (0x5000_0001, true)]
    }));
    b
}

/// One line per entry, collections in field order and entries in enumeration order. Record ids
/// (`database_record_id`) are included only with `with_ids`.
pub fn dump(b: &Biota, with_ids: bool) -> Vec<String> {
    let mut out = vec![format!(
        "id={} wcid={} type={:?}",
        b.id, b.weenie_class_id, b.weenie_type
    )];
    macro_rules! d {
        ($name:literal, $field:expr) => {
            match &$field {
                None => out.push(format!("{} None", $name)),
                Some(d) => {
                    out.push(format!("{} {}", $name, d.len()));
                    for (k, v) in d.iter() {
                        out.push(format!("  {:?} => {:?}", k, v));
                    }
                }
            }
        };
    }
    macro_rules! l {
        ($name:literal, $field:expr) => {
            match &$field {
                None => out.push(format!("{} None", $name)),
                Some(l) => {
                    out.push(format!("{} {}", $name, l.len()));
                    for v in l.iter() {
                        out.push(format!("  {:?}", v));
                    }
                }
            }
        };
    }
    d!("bool", b.properties_bool);
    d!("did", b.properties_did);
    d!("float", b.properties_float);
    d!("iid", b.properties_iid);
    d!("int", b.properties_int);
    d!("int64", b.properties_int64);
    d!("string", b.properties_string);
    d!("position", b.properties_position);
    d!("spellbook", b.properties_spell_book);
    l!("animpart", b.properties_anim_part);
    l!("palette", b.properties_palette);
    l!("texturemap", b.properties_texture_map);
    match &b.properties_create_list {
        None => out.push("createlist None".into()),
        Some(l) => {
            out.push(format!("createlist {}", l.len()));
            for v in l.iter() {
                let mut v = v.clone();
                if !with_ids {
                    v.database_record_id = 0;
                }
                out.push(format!("  {v:?}"));
            }
        }
    }
    match &b.properties_emote {
        None => out.push("emote None".into()),
        Some(l) => {
            out.push(format!("emote {}", l.len()));
            for e in l.iter() {
                let mut e = e.clone();
                if !with_ids {
                    e.database_record_id = 0;
                    for a in &mut e.properties_emote_action {
                        a.database_record_id = 0;
                    }
                }
                out.push(format!("  {e:?}"));
            }
        }
    }
    match &b.properties_event_filter {
        None => out.push("eventfilter None".into()),
        Some(s) => out.push(format!("eventfilter {:?}", s.iter().collect::<Vec<_>>())),
    }
    match &b.properties_generator {
        None => out.push("generator None".into()),
        Some(l) => {
            out.push(format!("generator {}", l.len()));
            for v in l.iter() {
                let mut v = v.clone();
                if !with_ids {
                    v.database_record_id = 0;
                }
                out.push(format!("  {v:?}"));
            }
        }
    }
    d!("attribute", b.properties_attribute);
    d!("attribute2nd", b.properties_attribute_2nd);
    match &b.properties_body_part {
        None => out.push("bodypart None".into()),
        Some(d) => {
            out.push(format!("bodypart {}", d.len()));
            for (k, v) in d.iter() {
                out.push(format!("  {k:?} => {v:?}"));
            }
        }
    }
    d!("skill", b.properties_skill);
    out.push(format!("book {:?}", b.properties_book));
    l!("bookpages", b.properties_book_page_data);
    d!("allegiance", b.properties_allegiance);
    l!("enchantments", b.properties_enchantment_registry);
    d!("housepermissions", b.house_permissions);
    out
}

/// Asserts two dumps are equal, showing the first difference.
pub fn assert_same(label: &str, got: &[String], want: &[String]) {
    for (i, (g, w)) in got.iter().zip(want.iter()).enumerate() {
        assert_eq!(g, w, "{label}: line {i} differs");
    }
    assert_eq!(got.len(), want.len(), "{label}: line counts differ");
}
