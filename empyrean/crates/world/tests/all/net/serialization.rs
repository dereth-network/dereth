//! Vectors: fixtures/vectors/messages/
//! Network/motion structures, CreateObject/UpdateObject/ObjDescEvent, game data and player
//! description writers match ACE.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state, retail dats or world.pack in the real-content tier.

// Vector fields are C# values of the width the structure declares: narrowing them is intended.
#![allow(clippy::cast_possible_truncation)]

use std::time::Duration;

use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::binary_reader::BinaryReader;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::dotnet::DotNetDict;
use empyrean_common::vectors::{self, f32_of, f64_of, i64_of, u64_of};
use empyrean_dat::{DatDatabaseType, FakeDats};
use empyrean_entity::enums::*;
use empyrean_entity::models::{
    PropertiesAnimPart, PropertiesAttribute, PropertiesAttribute2nd, PropertiesEnchantmentRegistry,
    PropertiesPalette, PropertiesPosition, PropertiesSkill, PropertiesTextureMap,
};
use empyrean_entity::{ObjectGuid, Position, Quaternion, Vector3};
use empyrean_store::models::shard::{Character, CharacterPropertiesFillCompBook};
use empyrean_world::dispatch::Class;
use empyrean_world::network::game_messages::messages::{
    game_message_create_object, game_message_obj_desc_event, game_message_update_object,
};
use empyrean_world::network::motion::interpreted_motion_state::InterpretedMotionState;
use empyrean_world::network::motion::motion_item::MotionItem;
use empyrean_world::network::motion::move_to_state::MoveToState;
use empyrean_world::network::motion::movement_data::{self, Motion, MovementData};
use empyrean_world::network::motion::raw_motion_state::RawMotionState;
use empyrean_world::network::sequence::sequence_manager::SequenceManager;
use empyrean_world::network::sequence::sequence_type::SequenceType;
use empyrean_world::network::structure::*;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::world_objects::world_object_equipment::HeldItem;
use empyrean_world::World;
use serde_json::Value;

// ---- helpers ------------------------------------------------------------------------------

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}

fn u(v: &Value) -> u32 {
    u32::try_from(u64_of(v).unwrap_or_else(|| panic!("uint: {v}"))).expect("u32")
}
fn i(v: &Value) -> i32 {
    i32::try_from(i64_of(v).unwrap_or_else(|| panic!("int: {v}"))).expect("i32")
}
fn f(v: &Value) -> f32 {
    f32_of(v).unwrap_or_else(|| panic!("float: {v}"))
}
fn d(v: &Value) -> f64 {
    f64_of(v).unwrap_or_else(|| panic!("double: {v}"))
}
fn b(v: &Value) -> bool {
    v.as_bool().unwrap_or_else(|| panic!("bool: {v}"))
}
fn arr(v: &Value) -> &Vec<Value> {
    v.as_array().unwrap_or_else(|| panic!("array: {v}"))
}
fn os(v: &Value) -> Option<String> {
    v.as_str().map(str::to_owned)
}
fn g(v: u32) -> ObjectGuid {
    ObjectGuid::new(v)
}

fn world_with(dats: FakeDats) -> World {
    let now = ClockSnapshot {
        portal_year_ticks: 0.0,
        unix_time: 1_000_000.0,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    };
    World::new(now, dats.build().expect("fake dats"))
}

fn world() -> World {
    world_with(FakeDats::new())
}

/// `[cell, x, y, z, qw, qx, qy, qz]`.
fn position(v: &Value) -> Position {
    let a = arr(v);
    Position::from_components(
        u(&a[0]),
        f(&a[1]),
        f(&a[2]),
        f(&a[3]),
        f(&a[5]),
        f(&a[6]),
        f(&a[7]),
        f(&a[4]),
        false,
    )
}

fn hex_out(case: &vectors::Case) -> String {
    case.output["hex"]
        .as_str()
        .unwrap_or_else(|| panic!("no hex: {}", case.output))
        .to_owned()
}

// ---- packers ------------------------------------------------------------------------------

#[test]
fn packers_match_aces_writers() {
    let file = vectors::load_named("messages", "serialization_packers");
    assert!(file.cases.len() > 100);
    for case in &file.cases {
        let input = &case.input;
        let mut w = Vec::new();
        match input["kind"].as_str().expect("kind") {
            "packable_list" => {
                let list: Vec<u32> = arr(&input["list"]).iter().map(u).collect();
                packable_list::write(&mut w, &list);
            }
            "packable_hash_table_header" => {
                packable_hash_table::write_header(&mut w, i(&input["count"]), i(&input["buckets"]));
            }
            "write_old" => {
                let fills: Vec<CharacterPropertiesFillCompBook> = arr(&input["fill"])
                    .iter()
                    .map(|x| {
                        let x = arr(x);
                        CharacterPropertiesFillCompBook {
                            character_id: 0,
                            spell_component_id: i(&x[0]),
                            quantity_to_rebuy: i(&x[1]),
                        }
                    })
                    .collect();
                packable_hash_table::write_old(&mut w, &fills);
            }
            "p_hash_table_header" => {
                let count = u(&input["count"]);
                p_hash_table::write_header(&mut w, count);
                assert_eq!(
                    p_hash_table::get_num_bits(count),
                    u(&case.output["num_bits"]),
                    "GetNumBits({count})"
                );
            }
            k => panic!("unknown kind {k}"),
        }
        assert_eq!(hex(&w), hex_out(case), "{input}");
    }
}

// ---- structures ---------------------------------------------------------------------------

fn enchantment(v: &Value) -> Enchantment {
    let a = arr(v);
    Enchantment {
        spell_id: u(&a[0]) as u16,
        layer: u(&a[1]) as u16,
        spell_category: u(&a[2]) as u16,
        has_spell_set_id: u(&a[3]) as u16,
        power_level: u(&a[4]),
        start_time: d(&a[5]),
        duration: d(&a[6]),
        caster_guid: u(&a[7]),
        degrade_modifier: f(&a[8]),
        degrade_limit: f(&a[9]),
        last_time_degraded: d(&a[10]),
        stat_mod_type: EnchantmentTypeFlags(u(&a[11]).cast_signed()),
        stat_mod_key: u(&a[12]),
        stat_mod_value: f(&a[13]),
        spell_set_id: u(&a[14]),
        ..Enchantment::default()
    }
}

fn house_payment(v: &Value) -> house_payment::HousePayment {
    let a = arr(v);
    house_payment::HousePayment {
        num: i(&a[0]),
        paid: i(&a[1]),
        weenie_id: u(&a[2]),
        name: os(&a[3]),
        plural_name: os(&a[4]),
    }
}

fn payments(v: &Value) -> Vec<house_payment::HousePayment> {
    v.as_array()
        .map(|a| a.iter().map(house_payment).collect())
        .unwrap_or_default()
}

#[allow(clippy::too_many_lines)]
fn write_structure(kind: &str, x: &[Value]) -> Vec<u8> {
    let mut w = Vec::new();
    match kind {
        "allegiance_data_null" => {
            allegiance_data::write(&mut w, &allegiance_data::AllegianceData { node: None })
        }
        "allegiance_profile_null" => {
            allegiance_profile::write(&mut w, &allegiance_profile::AllegianceProfile::default())
        }
        "position" => {
            // The vectors' first position is `new Position()`, not the cell constructor.
            let p = if u(&x[0]) == 0 {
                Position::new()
            } else {
                position(&Value::Array(x.to_vec()))
            };
            allegiance_hierarchy::write_position(&mut w, &p);
        }
        "officers" => {
            let mut officers = DotNetDict::new();
            for o in x {
                let o = arr(o);
                officers.add(g(u(&o[0])), AllegianceOfficerLevel(u(&o[1])));
            }
            allegiance_hierarchy::write_officers(&mut w, &officers);
        }
        "strings" => {
            let s: Vec<String> = x
                .iter()
                .map(|v| v.as_str().expect("str").to_owned())
                .collect();
            allegiance_hierarchy::write_strings(&mut w, &s);
        }
        "armor_level" => {
            let v: Vec<u32> = x.iter().map(u).collect();
            let a = armor_level::ArmorLevel {
                head: v[0],
                chest: v[1],
                abdomen: v[2],
                upper_arm: v[3],
                lower_arm: v[4],
                hand: v[5],
                upper_leg: v[6],
                lower_leg: v[7],
                foot: v[8],
            };
            armor_level::write(&mut w, &a);
        }
        "armor_profile" => {
            let v: Vec<f32> = x.iter().map(f).collect();
            let a = armor_profile::ArmorProfile {
                slashing_protection: v[0],
                piercing_protection: v[1],
                bludgeoning_protection: v[2],
                cold_protection: v[3],
                fire_protection: v[4],
                acid_protection: v[5],
                nether_protection: v[6],
                lightning_protection: v[7],
            };
            armor_profile::write(&mut w, &a);
        }
        "creature_profile" => {
            let v: Vec<u32> = x.iter().map(u).collect();
            let c = creature_profile::CreatureProfile {
                flags: v[0].cast_signed(),
                health: v[1],
                health_max: v[2],
                strength: v[3],
                endurance: v[4],
                quickness: v[5],
                coordination: v[6],
                focus: v[7],
                self_: v[8],
                stamina: v[9],
                mana: v[10],
                stamina_max: v[11],
                mana_max: v[12],
                attribute_highlights: v[13],
                attribute_colors: v[14],
            };
            creature_profile::write(&mut w, &c);
        }
        "hook_profile" => {
            let h = hook_profile::HookProfile {
                flags: i(&x[0]),
                valid_locations: EquipMask(u(&x[1])),
                ammo_type: AmmoType(u(&x[2]) as u16),
            };
            hook_profile::write(&mut w, &h);
        }
        "weapon_profile" => {
            let p = weapon_profile::WeaponProfile {
                damage_type: DamageType(i(&x[0])),
                weapon_time: u(&x[1]),
                weapon_skill: Skill(i(&x[2])),
                damage: u(&x[3]),
                damage_variance: d(&x[4]),
                damage_mod: d(&x[5]),
                weapon_length: d(&x[6]),
                max_velocity: d(&x[7]),
                weapon_offense: d(&x[8]),
                max_velocity_estimated: u(&x[9]),
                ..Default::default()
            };
            weapon_profile::write(&mut w, &p);
        }
        "enchantment" => enchantment::write(&mut w, &enchantment(&Value::Array(x.to_vec()))),
        "enchantment_list" => {
            let list: Vec<Enchantment> = x.iter().map(enchantment).collect();
            enchantment::write_list(&mut w, &list);
        }
        "enchantment_registry" => {
            let list = |v: &Value| arr(v).iter().map(enchantment).collect::<Vec<_>>();
            let r = enchantment_registry::EnchantmentRegistry {
                enchantment_mask: EnchantmentMask(i(&x[0])),
                enchantments: enchantment_registry::EnchantmentCategories {
                    multiplicative: list(&x[1]),
                    additive: list(&x[2]),
                    vitae: list(&x[3]),
                    cooldown: list(&x[4]),
                },
            };
            enchantment_registry::write(&mut w, &r);
        }
        "layered_spell_list" => {
            let l: Vec<layered_spell::LayeredSpell> = x
                .iter()
                .map(|s| {
                    layered_spell::LayeredSpell::new(u(&arr(s)[0]) as u16, u(&arr(s)[1]) as u16)
                })
                .collect();
            layered_spell::write_list(&mut w, &l);
        }
        "registry_list" => {
            let l: Vec<PropertiesEnchantmentRegistry> = x
                .iter()
                .map(|s| PropertiesEnchantmentRegistry {
                    spell_id: i(&arr(s)[0]),
                    layer_id: u(&arr(s)[1]) as u16,
                    ..Default::default()
                })
                .collect();
            layered_spell::write_registry_list(&mut w, &l);
        }
        "shortcut_list" => {
            let l: Vec<shortcut::Shortcut> = x
                .iter()
                .map(|s| shortcut::Shortcut::new(u(&arr(s)[0]), u(&arr(s)[1])))
                .collect();
            shortcut::write_list(&mut w, &l);
        }
        "squelch_info" => {
            let info = squelch_info::SquelchInfo::from_filters(
                arr(&x[0]).iter().map(|m| SquelchMask(u(m))).collect(),
                os(&x[1]),
                b(&x[2]),
            );
            squelch_info::write(&mut w, &info);
        }
        "squelch_db" => {
            let mut db = squelch_db::SquelchDB::default();
            for c in arr(&x[0]) {
                let c = arr(c);
                db.characters.add(
                    u(&c[0]),
                    squelch_info::SquelchInfo::from_filter(
                        SquelchMask(u(&c[1])),
                        c[2].as_str().expect("name"),
                        b(&c[3]),
                    ),
                );
            }
            for m in arr(&x[1]) {
                db.globals.filters.push(SquelchMask(u(m)));
            }
            squelch_db::write(&mut w, &db);
        }
        "guest_info" => guest_info::write(
            &mut w,
            &guest_info::GuestInfo::new(b(&x[0]), x[1].as_str().expect("name")),
        ),
        "restriction_db" => {
            let mut r = restriction_db::RestrictionDB {
                house_owner: u(&x[0]),
                open_status: b(&x[1]),
                monarch_id: g(u(&x[2])),
                ..Default::default()
            };
            for e in arr(&x[3]) {
                r.table.add(g(u(&arr(e)[0])), u(&arr(e)[1]));
            }
            restriction_db::write(&mut w, &r);
        }
        "house_access" => {
            let mut h = house_access::HouseAccess {
                bitmask: HARBitfield(i(&x[0])),
                ..Default::default()
            };
            if u(&x[1]) != 0 {
                h.monarch_id = g(u(&x[1]));
            }
            for e in arr(&x[2]) {
                let e = arr(e);
                h.guest_list.add(
                    g(u(&e[0])),
                    guest_info::GuestInfo::new(b(&e[1]), e[2].as_str().expect("name")),
                );
            }
            h.roommates = arr(&x[3]).iter().map(|r| g(u(r))).collect();
            house_access::write(&mut w, &h);
        }
        "house_payment_list" => {
            house_payment::write_list(&mut w, &payments(&Value::Array(x.to_vec())))
        }
        "house_data" => {
            let hd = house_data::HouseData {
                buy_time: u(&x[0]),
                rent_time: u(&x[1]),
                r#type: HouseType(i(&x[2])),
                maintenance_free: b(&x[3]),
                buy: payments(&x[4]),
                rent: payments(&x[5]),
                position: Some(position(&x[6])),
            };
            house_data::write(&mut w, &hd);
        }
        "house_profile" => {
            let hp = house_profile::HouseProfile {
                dwelling_id: u(&x[0]),
                owner_id: g(u(&x[1])),
                bitmask: HouseBitfield(i(&x[2])),
                min_level: i(&x[3]),
                max_level: i(&x[4]),
                min_alleg_rank: i(&x[5]),
                max_alleg_rank: i(&x[6]),
                maintenance_free: b(&x[7]),
                r#type: HouseType(i(&x[8])),
                owner_name: os(&x[9]),
                buy: payments(&x[10]),
                rent: payments(&x[11]),
            };
            house_profile::write(&mut w, &hp);
        }
        "chess_move_data" => {
            let coord = |v: &Value| chess_move_data::ChessPieceCoord {
                x: i(&arr(v)[0]),
                y: i(&arr(v)[1]),
            };
            let data = chess_move_data::ChessMoveData {
                r#type: ChessMoveType(i(&x[0])),
                player_guid: g(u(&x[1])),
                color: ChessColor(i(&x[2])),
                piece_guid: g(u(&x[3])),
                from: Some(coord(&x[4])),
                to: Some(coord(&x[5])),
            };
            chess_move_data::write(&mut w, &data);
        }
        "contract_tracker" => panic!("contract_tracker is checked with a contract table"),
        "fellowship_lock_data" => {
            let mut l = fellowship_lock_data::FellowshipLockData::new(d(&x[0]));
            l.update_timestamp(d(&x[1]));
            fellowship_lock_data::write(&mut w, &l);
        }
        "fellowship_locks" => {
            let mut locks = DotNetDict::new();
            for e in x {
                let e = arr(e);
                locks.add(
                    e[0].as_str().expect("key").to_owned(),
                    fellowship_lock_data::FellowshipLockData::new(d(&e[1])),
                );
            }
            fellowship_lock_data::write_locks(&mut w, &locks);
        }
        "origin" => origin::write(
            &mut w,
            &origin::Origin::new(u(&x[0]), Vector3::new(f(&x[1]), f(&x[2]), f(&x[3]))),
        ),
        "page_data" => {
            let p = page_data::PageData {
                author_guid: u(&x[0]),
                author_name: os(&x[1]),
                author_account: os(&x[2]),
                has_text: b(&x[3]),
                ignore_author: b(&x[4]),
                page_text: os(&x[5]),
                ..Default::default()
            };
            page_data::write(&mut w, &p);
        }
        "salvage_result" => {
            let m = salvage_result::SalvageMessage {
                amount: u(&x[3]),
                material_type: MaterialType(u(&x[0])),
                workmanship: f(&x[1]),
                num_items_in_material: i(&x[2]),
                skill: Skill::Salvaging,
            };
            salvage_result::write(&mut w, &salvage_result::salvage_result_new(&m));
        }
        "position_pack" => {
            let mut p = position_pack::PositionPack {
                origin: origin::Origin::new(u(&x[0]), Vector3::new(f(&x[1]), f(&x[2]), f(&x[3]))),
                rotation: Quaternion::new(f(&x[5]), f(&x[6]), f(&x[7]), f(&x[4])),
                velocity: Vector3::new(f(&x[8]), f(&x[9]), f(&x[10])),
                placement_id: x[11]
                    .as_i64()
                    .map(|p| Placement(u32::try_from(p).expect("placement"))),
                instance_sequence: vec![1, 0],
                position_sequence: vec![2, 0],
                teleport_sequence: vec![3, 0],
                force_position_sequence: vec![4, 0],
                ..Default::default()
            };
            p.flags = p.build_flags(false);
            position_pack::write(&mut w, &p);
        }
        "appraise_info_empty" => {
            appraise_info::write(&mut w, &appraise_info::appraise_info_empty())
        }
        "appraise_info_full" => appraise_info::write(&mut w, &full_appraisal()),
        k => panic!("unknown kind {k}"),
    }
    w
}

/// The `appraise_info_full` structure of the vectors (`SerializationVectors.cs`).
fn full_appraisal() -> appraise_info::AppraiseInfo {
    let mut ai = appraise_info::AppraiseInfo {
        success: true,
        flags: IdentifyResponseFlags(0x7FFF),
        ..Default::default()
    };
    for (k, v) in [(19, 100), (5, 7), (35, -1), (21, 3), (3, 44)] {
        ai.properties_int.add(PropertyInt(k), v);
    }
    for (k, v) in [(2, 5_000_000_000), (10, -1)] {
        ai.properties_int64.add(PropertyInt64(k), v);
    }
    for (k, v) in [(9, true), (1, false), (17, true)] {
        ai.properties_bool.add(PropertyBool(k), v);
    }
    for (k, v) in [(13, 1.5), (5, -0.25), (21, 1e10)] {
        ai.properties_float.add(PropertyFloat(k), v);
    }
    for (k, v) in [(16, "Long"), (8, "Scribe"), (1, "Name")] {
        ai.properties_string.add(PropertyString(k), v.to_owned());
    }
    for (k, v) in [(9, 0x0600_1234), (1, 0x0200_0001)] {
        ai.properties_did.add(PropertyDataId(k), v);
    }
    ai.spell_book = vec![0x8000_0123, 5];
    ai.armor_profile = Some(armor_profile::ArmorProfile {
        slashing_protection: 1.25,
        ..Default::default()
    });
    ai.creature_profile = Some(creature_profile::CreatureProfile {
        flags: 8,
        health: 5,
        ..Default::default()
    });
    ai.weapon_profile = Some(weapon_profile::WeaponProfile {
        damage: 9,
        ..Default::default()
    });
    ai.hook_profile = Some(hook_profile::HookProfile {
        flags: 1,
        ..Default::default()
    });
    ai.armor_highlight = 1;
    ai.armor_color = 2;
    ai.weapon_highlight = 3;
    ai.weapon_color = 4;
    ai.resist_highlight = 5;
    ai.resist_color = 6;
    ai.armor_levels = Some(armor_level::ArmorLevel {
        foot: 3,
        ..Default::default()
    });
    ai
}

use empyrean_world::network::structure::enchantment::Enchantment;

#[test]
fn structures_match_aces_writers() {
    let file = vectors::load_named("messages", "serialization_structures");
    let mut checked = 0;
    for case in &file.cases {
        let kind = case.input["kind"].as_str().expect("kind");
        if kind == "contract_tracker" {
            continue; // `contract_tracker_reads_the_contract_table`
        }
        let fields = arr(&case.input["fields"]);
        // ACE's bytes, or retail's where retail layout is used (V279: lock-table order).
        let ace = hex_out(case);
        let unhexed: Vec<u8> = (0..ace.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&ace[i..i + 2], 16).unwrap())
            .collect();
        let want =
            crate::proto_identity::retail_ruled_structure(kind, &unhexed).map_or(ace, |v| hex(&v));
        assert_eq!(
            hex(&write_structure(kind, fields)),
            want,
            "{kind}: {fields:?}"
        );
        checked += 1;
    }
    assert!(checked > 50, "{checked}");
}

#[test]
fn contract_tracker_reads_the_contract_table() {
    use dereth_assets::tables::{Contract, ContractTable};
    use dereth_primitives::DataId;

    let file = vectors::load_named("messages", "serialization_structures");
    let case = file
        .cases
        .iter()
        .find(|c| c.input["kind"] == "contract_tracker")
        .expect("contract case");
    let x = arr(&case.input["fields"]);

    use dereth_primitives::{CellId, Frame, Position as DatPosition};
    let nowhere = DatPosition::new(CellId(0), Frame::default());
    let mut contracts = std::collections::BTreeMap::new();
    contracts.insert(
        u(&x[1]),
        Contract {
            version: u(&x[0]),
            contract_id: u(&x[1]),
            strings: Default::default(),
            location_npc_start: nowhere,
            location_npc_end: nowhere,
            location_quest_area: nowhere,
        },
    );
    let table = ContractTable {
        id: DataId(0x0E00_001D),
        buckets: 0,
        contracts,
    };
    let w = world_with(FakeDats::new().with_portal(0x0E00_001D, table));
    let mut t = contract_tracker::contract_tracker_from_id(&w, None, u(&x[1]));
    t.stage = i(&x[2]);
    t.time_when_done = d(&x[3]);
    t.time_when_repeats = d(&x[4]);
    let mut out = Vec::new();
    contract_tracker::write(&mut out, &w, &mut t);
    assert_eq!(hex(&out), hex_out(case));
}

// ---- readers ------------------------------------------------------------------------------

fn raw_json(s: &RawMotionState) -> Vec<Value> {
    use serde_json::json;
    vec![
        json!(s.packed_flags),
        json!(s.flags),
        json!(s.command_list_length),
        json!(s.current_hold_key.0),
        json!(s.current_style.0),
        json!(s.forward_command.0),
        json!(s.forward_hold_key.0),
        json!(f64::from(s.forward_speed)),
        json!(s.sidestep_command.0),
        json!(s.sidestep_hold_key.0),
        json!(f64::from(s.sidestep_speed)),
        json!(s.turn_command.0),
        json!(s.turn_hold_key.0),
        json!(f64::from(s.turn_speed)),
    ]
}

/// Compares a value read here with the vector's (numbers by value, floats exactly).
fn same(a: &Value, e: &Value) -> bool {
    match (a, e) {
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q))
        }
        (Value::Number(_), Value::Number(_)) => f64_of(a) == f64_of(e),
        _ => a == e,
    }
}

#[test]
fn readers_match_aces_readers() {
    use serde_json::json;
    let file = vectors::load_named("messages", "serialization_readers");
    for case in &file.cases {
        let bytes = unhex(case.input["hex"].as_str().expect("hex"));
        let mut r = BinaryReader::new(&bytes);
        let kind = case.input["kind"].as_str().expect("kind");
        let wo = g(0x5000_0001);
        let value: Result<Value, String> = match kind {
            "list_u_int32" => packable_list::read_list_u_int32(&mut r)
                .map(|l| json!(l))
                .map_err(|e| format!("{e:?}")),
            "c_all_iteration_list" => c_all_iteration_list::read_c_all_iteration_list(&mut r)
                .map(|l| {
                    json!(l
                        .lists
                        .iter()
                        .map(|x| json!([
                            x.dat_file_type,
                            x.dat_file_id,
                            x.list.iterations,
                            x.list.ints
                        ]))
                        .collect::<Vec<_>>())
                })
                .map_err(|e| format!("{e:?}")),
            "layered_spell" => layered_spell::read_layered_spell(&mut r)
                .map(|s| json!([s.spell_id, s.layer]))
                .map_err(|e| format!("{e:?}")),
            "shortcut" => shortcut::read_shortcut(&mut r)
                .map(|s| json!([s.index, s.object_id, s.spell.spell_id, s.spell.layer]))
                .map_err(|e| format!("{e:?}")),
            "vector3" => extensions::read_vector3(&mut r)
                .map(|v| json!([f64::from(v.x), f64::from(v.y), f64::from(v.z)]))
                .map_err(|e| format!("{e:?}")),
            "quaternion" => extensions::read_quaternion(&mut r)
                .map(|q| {
                    json!([
                        f64::from(q.w),
                        f64::from(q.x),
                        f64::from(q.y),
                        f64::from(q.z)
                    ])
                })
                .map_err(|e| format!("{e:?}")),
            "jump_pack" => jump_pack::JumpPack::read(&mut r)
                .map(|j| {
                    json!([
                        f64::from(j.extent),
                        f64::from(j.velocity.x),
                        f64::from(j.velocity.y),
                        f64::from(j.velocity.z),
                        j.instance_sequence,
                        j.server_control_sequence,
                        j.teleport_sequence,
                        j.force_position_sequence
                    ])
                })
                .map_err(|e| format!("{e:?}")),
            "motion_item" => MotionItem::read(
                wo,
                &mut r,
                dereth_world_data::command_numbering::CommandNumbering::Final,
            )
            .map(|m| {
                json!([
                    m.motion_command.0,
                    m.packed_sequence,
                    m.server_action_sequence,
                    m.is_autonomous,
                    f64::from(m.speed)
                ])
            })
            .map_err(|e| format!("{e:?}")),
            "raw_motion_state" => RawMotionState::read(
                wo,
                &mut r,
                dereth_world_data::command_numbering::CommandNumbering::Final,
            )
            .map(|s| json!(raw_json(&s)))
            .map_err(|e| format!("{e:?}")),
            "move_to_state" => MoveToState::read(
                wo,
                &mut r,
                dereth_world_data::command_numbering::CommandNumbering::Final,
            )
            .map(|s| {
                let p = s.position.expect("read");
                json!([
                    raw_json(&s.raw_motion_state),
                    [
                        p.cell(),
                        f64::from(p.position_x),
                        f64::from(p.position_y),
                        f64::from(p.position_z),
                        f64::from(p.rotation_w),
                        f64::from(p.rotation_x),
                        f64::from(p.rotation_y),
                        f64::from(p.rotation_z)
                    ],
                    s.instance_sequence,
                    s.server_control_sequence,
                    s.teleport_sequence,
                    s.force_position_sequence,
                    s.contact_long_jump,
                    s.contact,
                    s.standing_long_jump
                ])
            })
            .map_err(|e| format!("{e:?}")),
            k => panic!("unknown kind {k}"),
        };
        match vectors::throws(&case.output) {
            Some(t) => assert!(value.is_err(), "{kind}: ACE throws {t}, read {value:?}"),
            None => {
                let value = value.unwrap_or_else(|e| panic!("{kind}: {e}"));
                assert!(
                    same(&value, &case.output["value"]),
                    "{kind}: {value} vs {}",
                    case.output["value"]
                );
                assert_eq!(
                    r.position() as u64,
                    u64_of(&case.output["pos"]).expect("pos"),
                    "{kind}: position"
                );
            }
        }
    }
}

/// A client's motion item names its command by the raw index; the target-selection block reads as
/// the retail client's commands (V331, V331), where ACE's 2013 numbering would give
/// `PreviousMonster`..`ClosestPlayer` at 0x110..0x114 and nothing at 0x115..0x117.
#[test]
fn a_motion_items_raw_index_reads_as_the_clients_command() {
    for (raw, want) in [
        (0x10F_u16, MotionCommand::SkillHealOther),
        (0x110, MotionCommand::CombatEat),
        (0x111, MotionCommand::CombatDrink),
        (0x112, MotionCommand::NextMonster),
        (0x113, MotionCommand::PreviousMonster),
        (0x114, MotionCommand::ClosestMonster),
        (0x115, MotionCommand::NextPlayer),
        (0x116, MotionCommand::PreviousPlayer),
        (0x117, MotionCommand::ClosestPlayer),
    ] {
        let mut bytes = raw.to_le_bytes().to_vec();
        bytes.extend_from_slice(&[1, 0, 0, 0, 0xC0, 0x3F]);
        let item = MotionItem::read(
            g(0x5000_0001),
            &mut BinaryReader::new(&bytes),
            dereth_world_data::command_numbering::CommandNumbering::Final,
        )
        .expect("read");
        assert_eq!(item.motion_command, want, "raw {raw:#x}");
    }
    assert_eq!(MotionCommand::CombatEat.0, 0x1000_0110);
    assert_eq!(MotionCommand::CombatDrink.0, 0x1000_0111);
    assert_eq!(MotionCommand::NextMonster.0, 0x0900_0112);
    assert_eq!(MotionCommand::ClosestPlayer.0, 0x0900_0117);
}

// ---- motion -------------------------------------------------------------------------------

fn motion_of(v: &Value, wo: ObjectGuid) -> Motion {
    let state = arr(&v["state"]);
    let mut m = Motion::from_stance(MotionStance(u(&v["stance"])));
    m.motion_state.forward_command = MotionCommand(u(&state[0]));
    m.motion_state.sidestep_command = MotionCommand(u(&state[1]));
    m.motion_state.turn_command = MotionCommand(u(&state[2]));
    m.motion_state.forward_speed = f(&state[3]);
    m.motion_state.sidestep_speed = f(&state[4]);
    m.motion_state.turn_speed = f(&state[5]);
    if let Some(cmds) = state[6].as_array() {
        m.motion_state.commands = Some(
            cmds.iter()
                .map(|c| {
                    let c = arr(c);
                    MotionItem {
                        is_autonomous: b(&c[1]),
                        ..MotionItem::new(wo, MotionCommand(u(&c[0])), f(&c[2]))
                    }
                })
                .collect(),
        );
    }
    m.is_autonomous = b(&v["autonomous"]);
    if let Some(flags) = v.get("flags") {
        m.motion_flags = MotionFlags(u(flags) as u8);
        m.movement_type = MovementType(u(&v["movement_type"]) as u8);
        m.target_guid = g(u(&v["target"]));
        m.run_rate = f(&v["run_rate"]);
        m.desired_heading = f(&v["desired_heading"]);
        m.position = v["position"].as_array().map(|_| position(&v["position"]));
        let p = arr(&v["params"]);
        m.move_to_parameters.movement_parameters = MovementParams(u(&p[0]));
        m.move_to_parameters.distance_to_object = f(&p[1]);
        m.move_to_parameters.min_distance = f(&p[2]);
        m.move_to_parameters.fail_distance = f(&p[3]);
        m.move_to_parameters.speed = f(&p[4]);
        m.move_to_parameters.walk_run_threshold = f(&p[5]);
        m.move_to_parameters.desired_heading = f(&p[6]);
    }
    m
}

fn motion_seqs(s: &mut SequenceManager) -> Vec<String> {
    vec![
        hex(&s.get_current_sequence(SequenceType::ObjectMovement)),
        hex(&s.get_current_sequence(SequenceType::ObjectServerControl)),
        hex(&s.get_current_sequence(SequenceType::Motion)),
    ]
}

#[test]
fn movement_data_matches_aces_writer() {
    let file = vectors::load_named("messages", "serialization_motion");
    let mut checked = 0;
    for case in &file.cases {
        let wo = g(0x5000_0001);
        let mut seq = SequenceManager::new();
        let out = match case.input["kind"].as_str().expect("kind") {
            "motion" => {
                for _ in 0..i(&case.input["advance"]) {
                    let _ = seq.get_next_sequence(SequenceType::ObjectMovement);
                    let _ = seq.get_next_sequence(SequenceType::ObjectServerControl);
                    let _ = seq.get_next_sequence(SequenceType::Motion);
                }
                let motion = motion_of(&case.input["motion"], wo);
                let data = MovementData::from_motion(wo, &motion);
                let mut w = Vec::new();
                movement_data::write(
                    &mut w,
                    &data,
                    b(&case.input["header"]),
                    &mut seq,
                    dereth_world_data::command_numbering::CommandNumbering::Final,
                );
                w
            }
            "move_to_state" => {
                let bytes = unhex(case.input["hex"].as_str().expect("hex"));
                let state = MoveToState::read(
                    g(0x5000_0002),
                    &mut BinaryReader::new(&bytes),
                    dereth_world_data::command_numbering::CommandNumbering::Final,
                )
                .expect("read");
                let data = MovementData::from_move_to_state(&mut world(), g(0x5000_0002), &state);
                let mut w = Vec::new();
                movement_data::write(
                    &mut w,
                    &data,
                    true,
                    &mut seq,
                    dereth_world_data::command_numbering::CommandNumbering::Final,
                );
                w
            }
            k => panic!("unknown kind {k}"),
        };
        assert_eq!(hex(&out), hex_out(case), "{}", case.input);
        let expected: Vec<String> = arr(&case.output["seq"])
            .iter()
            .map(|s| s.as_str().expect("seq").to_owned())
            .collect();
        assert_eq!(motion_seqs(&mut seq), expected, "{}", case.input);
        checked += 1;
    }
    assert!(checked > 30, "{checked}");
}

#[test]
fn interpreted_motion_state_flags_follow_its_fields() {
    let mut s = InterpretedMotionState::new();
    assert_eq!(s.build_movement_flags(), MovementStateFlag::ForwardCommand);
    s.current_style = MotionStance::Invalid;
    s.forward_command = MotionCommand::Invalid;
    s.turn_speed = 1.5;
    assert_eq!(s.build_movement_flags(), MovementStateFlag::TurnSpeed);
    assert!(!s.has_movement());
    s.turn_command = MotionCommand::TurnRight;
    assert!(s.has_movement());
}

// ---- world objects ------------------------------------------------------------------------

fn class_of(name: &str) -> Class {
    match name {
        "GenericObject" => Class::GenericObject,
        "Container" => Class::Container,
        "Chest" => Class::Chest,
        "Door" => Class::Door,
        "Clothing" => Class::Clothing,
        "Creature" => Class::Creature,
        "Hook" => Class::Hook,
        "Player" => Class::Player,
        "Portal" => Class::Portal,
        "ManaStone" => Class::ManaStone,
        "CraftTool" => Class::CraftTool,
        "MeleeWeapon" => Class::MeleeWeapon,
        c => panic!("no class {c}"),
    }
}

/// Builds the vector's object (and its equipped items and inventory) into `w`.
fn build(w: &mut World, spec: &Value) -> ObjectGuid {
    let guid = g(u(&spec["guid"]));
    let mut o = WorldObject::allocate(class_of(spec["class"].as_str().expect("class")));
    o.guid = guid;
    let bi = &mut o.biota;
    bi.id = guid.full();
    bi.weenie_class_id = u(&spec["wcid"]);
    bi.weenie_type = WeenieType(u(&spec["weenie_type"]));
    let pairs = |k: &str| {
        arr(&spec[k])
            .iter()
            .map(|p| arr(p).clone())
            .collect::<Vec<_>>()
    };
    bi.properties_int = Some(
        pairs("int")
            .iter()
            .map(|p| (PropertyInt(u(&p[0]) as u16), i(&p[1])))
            .collect(),
    );
    bi.properties_int64 = Some(
        pairs("int64")
            .iter()
            .map(|p| (PropertyInt64(u(&p[0]) as u16), i64_of(&p[1]).expect("long")))
            .collect(),
    );
    bi.properties_bool = Some(
        pairs("bool")
            .iter()
            .map(|p| (PropertyBool(u(&p[0]) as u16), b(&p[1])))
            .collect(),
    );
    bi.properties_float = Some(
        pairs("float")
            .iter()
            .map(|p| (PropertyFloat(u(&p[0]) as u16), d(&p[1])))
            .collect(),
    );
    bi.properties_string = Some(
        pairs("string")
            .iter()
            .map(|p| {
                (
                    PropertyString(u(&p[0]) as u16),
                    p[1].as_str().expect("s").to_owned(),
                )
            })
            .collect(),
    );
    bi.properties_did = Some(
        pairs("did")
            .iter()
            .map(|p| (PropertyDataId(u(&p[0]) as u16), u(&p[1])))
            .collect(),
    );
    bi.properties_iid = Some(
        pairs("iid")
            .iter()
            .map(|p| (PropertyInstanceId(u(&p[0]) as u16), u(&p[1])))
            .collect(),
    );
    bi.properties_position = Some(
        pairs("positions")
            .iter()
            .map(|p| {
                (
                    PositionType(u(&p[0]) as u16),
                    PropertiesPosition {
                        obj_cell_id: u(&p[1]),
                        position_x: f(&p[2]),
                        position_y: f(&p[3]),
                        position_z: f(&p[4]),
                        rotation_w: f(&p[5]),
                        rotation_x: f(&p[6]),
                        rotation_y: f(&p[7]),
                        rotation_z: f(&p[8]),
                    },
                )
            })
            .collect(),
    );
    let anim = pairs("anim_parts");
    if !anim.is_empty() {
        bi.properties_anim_part = Some(
            anim.iter()
                .map(|p| PropertiesAnimPart {
                    index: u(&p[0]) as u8,
                    animation_id: u(&p[1]),
                })
                .collect(),
        );
    }
    let pal = pairs("palettes");
    if !pal.is_empty() {
        bi.properties_palette = Some(
            pal.iter()
                .map(|p| PropertiesPalette {
                    sub_palette_id: u(&p[0]),
                    offset: u(&p[1]) as u16,
                    length: u(&p[2]) as u16,
                })
                .collect(),
        );
    }
    let tex = pairs("textures");
    if !tex.is_empty() {
        bi.properties_texture_map = Some(
            tex.iter()
                .map(|p| PropertiesTextureMap {
                    part_index: u(&p[0]) as u8,
                    old_texture: u(&p[1]),
                    new_texture: u(&p[2]),
                })
                .collect(),
        );
    }
    let spells = pairs("spell_book");
    if !spells.is_empty() {
        bi.properties_spell_book = Some(spells.iter().map(|p| (i(&p[0]), f(&p[1]))).collect());
    }
    bi.properties_attribute = Some(DotNetDict::new());
    bi.properties_attribute_2nd = Some(DotNetDict::new());
    bi.properties_skill = Some(DotNetDict::new());
    for a in pairs("attributes") {
        let id = u(&a[0]);
        if id < 100 {
            bi.properties_attribute.as_mut().expect("set").insert(
                PropertyAttribute(id as u16),
                PropertiesAttribute {
                    init_level: u(&a[1]),
                    level_from_cp: u(&a[2]),
                    cp_spent: u(&a[3]),
                },
            );
        } else {
            bi.properties_attribute_2nd.as_mut().expect("set").insert(
                PropertyAttribute2nd((id - 100) as u16),
                PropertiesAttribute2nd {
                    init_level: u(&a[1]),
                    level_from_cp: u(&a[2]),
                    cp_spent: u(&a[3]),
                    current_level: u(&a[4]),
                },
            );
        }
    }
    for s in pairs("skills") {
        bi.properties_skill.as_mut().expect("set").insert(
            Skill(i(&s[0])),
            PropertiesSkill {
                level_from_pp: u(&s[1]) as u16,
                sac: SkillAdvancementClass(u(&s[2])),
                pp: u(&s[3]),
                init_level: u(&s[4]),
                ..Default::default()
            },
        );
    }
    o.wo.world_object.object_description_flags = ObjectDescriptionFlag(i(&spec["odf"]));
    if !spec["motion"].is_null() {
        o.wo.world_object_properties.current_motion_state = Some(motion_of(&spec["motion"], guid));
    }
    for c in pairs("children") {
        o.wo.world_object_properties.children.push(HeldItem {
            guid: u(&c[0]),
            location_id: i(&c[1]),
            equip_mask: EquipMask(u(&c[2])),
        });
    }
    if let Some(ch) = spec["character"].as_array() {
        let character = Character {
            character_options_1: i(&ch[0]),
            character_options_2: i(&ch[1]),
            default_hair_texture: u(&ch[2]),
            hair_texture: u(&ch[3]),
            is_plussed: u(&ch[4]) != 0,
            ..Default::default()
        };
        o.player.as_mut().expect("a player").player.character = Some(character);
    }
    w.objects.insert(o).expect("fresh guid");

    let equipped: Vec<ObjectGuid> = arr(&spec["equipped"]).iter().map(|e| build(w, e)).collect();
    let inventory: Vec<ObjectGuid> = arr(&spec["inventory"])
        .iter()
        .map(|e| build(w, e))
        .collect();
    let o = w.objects.get_mut(guid).expect("inserted");
    set_equipped_objects(o, &equipped);
    set_inventory(o, &inventory);
    guid
}

/// Fills `Creature.EquippedObjects` directly (the vectors give the dictionary's order).
fn set_equipped_objects(
    o: &mut empyrean_world::world_objects::world_object::WorldObject,
    items: &[ObjectGuid],
) {
    if items.is_empty() {
        return;
    }
    let equipped = &mut o
        .creature
        .as_mut()
        .expect("a creature")
        .creature_equipment
        .equipped_objects;
    for g in items {
        equipped.insert(*g, ());
    }
}

/// Fills `Container.Inventory` directly (the vectors give the dictionary's order).
fn set_inventory(
    o: &mut empyrean_world::world_objects::world_object::WorldObject,
    items: &[ObjectGuid],
) {
    if items.is_empty() {
        return;
    }
    let inventory = &mut o
        .container
        .as_mut()
        .expect("a container")
        .container
        .inventory;
    for g in items {
        inventory.insert(*g, ());
    }
}

const ALL_SEQUENCES: [SequenceType; 10] = [
    SequenceType::ObjectPosition,
    SequenceType::ObjectMovement,
    SequenceType::ObjectState,
    SequenceType::ObjectVector,
    SequenceType::ObjectTeleport,
    SequenceType::ObjectServerControl,
    SequenceType::ObjectForcePosition,
    SequenceType::ObjectVisualDesc,
    SequenceType::ObjectInstance,
    SequenceType::Motion,
];

/// A world whose portal dat holds the vector's synthetic files (an empty entry is a file the dat
/// does not have).
fn world_for(case: &vectors::Case) -> World {
    let mut dats = FakeDats::new();
    for e in arr(&case.input["dats"]) {
        let e = arr(e);
        let bytes = unhex(e[1].as_str().expect("hex"));
        if !bytes.is_empty() {
            dats = dats.with_raw(DatDatabaseType::Portal, u(&e[0]), bytes);
        }
    }
    world_with(dats)
}

#[test]
fn appraisals_match_aces_appraise_info() {
    let file = vectors::load_named("messages", "serialization_appraise");
    assert!(file.cases.len() >= 10);
    for case in &file.cases {
        let mut w = world();
        let guid = build(&mut w, &case.input["object"]);
        let info = appraise_info::appraise_info_new(
            &mut w,
            guid,
            g(0x5000_0FFF),
            b(&case.input["success"]),
        );
        let mut out = Vec::new();
        appraise_info::write(&mut out, &info);
        assert_eq!(hex(&out), hex_out(case), "{}", case.input);
    }
}

#[test]
fn object_descriptions_match_aces_writers() {
    let file = vectors::load_named("messages", "serialization_world_objects");
    assert!(file.cases.len() >= 20);
    let mut refused = Vec::new();
    for case in &file.cases {
        let mut w = world_for(case);
        let guid = build(&mut w, &case.input["object"]);
        for _ in 0..i(&case.input["advance"]) {
            for t in ALL_SEQUENCES {
                let _ = w
                    .objects
                    .get_mut(guid)
                    .expect("built")
                    .sequences
                    .get_next_sequence(t);
            }
        }
        let label = format!("{} 0x{:08X}", case.input["object"]["class"], guid.full());

        // V236, V237, V334.
        let before = refused.len();
        let ids = {
            let mut w2 = world_for(case);
            let guid = build(&mut w2, &case.input["object"]);
            let mut sent = |v: u32, base: u32| {
                if v != 0 && v & base == 0 && v < base {
                    base + v
                } else if v != 0 && v.wrapping_sub(base) >= 0x4000_0000 {
                    refused.push(format!("{label} 0x{v:08X}"));
                    0
                } else {
                    v
                }
            };
            // The object description first, as the serialiser computes it (it may set the icon).
            let d = empyrean_world::dispatch::calculate_obj_desc::calculate_obj_desc(&mut w2, guid);
            let o = w2.objects.get(guid).expect("built");
            let (icon, overlay, underlay) = (
                o.icon_id(),
                o.icon_overlay_id().unwrap_or(0),
                o.icon_underlay_id().unwrap_or(0),
            );
            crate::proto_identity::ObjectIds {
                icon: sent(icon, 0x0600_0000),
                overlay: sent(overlay, 0x0600_0000),
                underlay: sent(underlay, 0x0600_0000),
                palette: sent(d.palette_id, 0x0400_0000),
                subpalettes: d
                    .sub_palettes
                    .iter()
                    .map(|p| sent(p.sub_palette_id, 0x0400_0000))
                    .collect(),
                textures: d
                    .texture_changes
                    .iter()
                    .map(|t| {
                        (
                            sent(t.old_texture, 0x0500_0000),
                            sent(t.new_texture, 0x0500_0000),
                        )
                    })
                    .collect(),
                parts: d
                    .anim_part_changes
                    .iter()
                    .map(|a| sent(a.animation_id, 0x0100_0000))
                    .collect(),
                pscript: o
                    .get_property(PropertyDataId::RestrictionEffect)
                    .filter(|v| *v != 0)
                    .map(|v| v as u16),
            }
        };
        let want = |key: &str| {
            let ace = unhex(case.output[key].as_str().expect("hex"));
            let ruled = crate::proto_identity::retail_ruled_object_ids(key, &ace, &ids)
                .or_else(|| crate::proto_identity::retail_ruled_object(key, &ace));
            hex(&ruled.unwrap_or(ace))
        };
        let (create, errors) = crate::log_capture::capture(|| {
            game_message_create_object::game_message_create_object(&mut w, guid, false, false)
        });
        assert_eq!(
            errors.len(),
            refused.len() - before,
            "{label}: one content error per refused id: {errors:?}"
        );
        assert_eq!(hex(&create.data), want("create"), "{label}: create");
        let create_admin =
            game_message_create_object::game_message_create_object(&mut w, guid, true, true);
        assert_eq!(
            hex(&create_admin.data),
            want("create_admin"),
            "{label}: create adminvision"
        );
        let update =
            game_message_update_object::game_message_update_object(&mut w, guid, false, false);
        assert_eq!(hex(&update.data), want("update"), "{label}: update");
        let mut game_data = Vec::new();
        empyrean_world::dispatch::serialize_game_data_only::serialize_game_data_only(
            &mut w,
            guid,
            &mut game_data,
            true,
        );
        assert_eq!(hex(&game_data), want("game_data"), "{label}: game data");
        let obj_desc = game_message_obj_desc_event::game_message_obj_desc_event(&mut w, guid);
        assert_eq!(hex(&obj_desc.data), want("obj_desc"), "{label}: obj desc");

        let seqs: Vec<String> = ALL_SEQUENCES
            .iter()
            .map(|t| {
                hex(&w
                    .objects
                    .get_mut(guid)
                    .expect("built")
                    .sequences
                    .get_current_sequence(*t))
            })
            .collect();
        let expected: Vec<String> = arr(&case.output["seq"])
            .iter()
            .map(|s| s.as_str().expect("seq").to_owned())
            .collect();
        assert_eq!(seqs, expected, "{label}: sequences");
    }
    // ACE's vector objects take a synthetic icon, palettes and textures below their types' bases,
    // which ACE sent as bare offsets (the client added the base): V236/V237 amended sends each as the
    // id the client read, so none of these objects has an id refused.
    assert!(refused.is_empty(), "the refused ids: {refused:?}");
}

// ---- Shared rules: decode our bytes with the client's dereth-protocol ------------------------------------
//
// Each check decodes the bytes with `dereth-protocol` (tolerating at most three zero bytes of sender
// padding, as the retail archive does), re-encodes the decoded value and requires the same bytes
// back. A layout ACE gets differently from the retail client is `#[ignore = "Cross-check: ..."]`: kept as
// ACE writes it, and reported.

use dereth_protocol::{self as dp, Message as ProtoMessage};
use empyrean_world::network::game_event::events as ev;
use empyrean_world::network::game_messages::game_message::GameMessage;
use empyrean_world::network::game_messages::game_message_opcode::GameMessageOpcode;
use empyrean_world::sessions::SessionData;

// V236.
fn decode_client_body<M: ProtoMessage + std::fmt::Debug + PartialEq>(
    body: &[u8],
) -> Result<M, String> {
    let decoded = dp::read_body_padded::<M>(body)
        .map_err(|e| format!("dereth-protocol rejects {}: {e}", hex(body)))?;
    let again = dp::write_body(&decoded).map_err(|e| format!("re-encode: {e}"))?;
    if again.len() <= body.len() && hex(&body[..again.len()]) == hex(&again) {
        if !body[again.len()..].iter().all(|b| *b == 0) {
            return Err(format!("unread tail {}", hex(&body[again.len()..])));
        }
        return Ok(decoded);
    }
    let reread = dp::read_body_padded::<M>(&again)
        .map_err(|e| format!("re-encoding does not decode: {e}"))?;
    if reread != decoded {
        return Err(format!(
            "fields do not round-trip: {} vs {}",
            hex(&again),
            hex(body)
        ));
    }
    Ok(decoded)
}

/// The body of a message: after the opcode, or after the 0xF7B0 ordered-event header and type.
fn decode_client_message<M: ProtoMessage + std::fmt::Debug + PartialEq>(m: &GameMessage) -> M {
    let data = &m.data;
    let (ty, body) = if m.opcode == GameMessageOpcode::GameEvent {
        (
            u32::from_le_bytes(data[12..16].try_into().expect("type")),
            &data[16..],
        )
    } else {
        (
            u32::from_le_bytes(data[..4].try_into().expect("opcode")),
            &data[4..],
        )
    };
    assert_eq!(ty, M::OPCODE.0, "opcode / event type");
    decode_client_body(body).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn object_descriptions_decode_as_the_client_reads_them() {
    let file = vectors::load_named("messages", "serialization_world_objects");
    let mut failures = Vec::new();
    for case in &file.cases {
        let label = format!(
            "{} 0x{:08X}",
            case.input["object"]["class"],
            u(&case.input["object"]["guid"])
        );
        for key in ["create", "create_admin", "update"] {
            let data = unhex(case.output[key].as_str().expect("hex"));
            let r = if key == "update" {
                decode_client_body::<dp::objects::ItemUpdateObject>(&data[4..]).map(|d| d.0.id.0)
            } else {
                decode_client_body::<dp::objects::ItemCreateObject>(&data[4..]).map(|d| d.0.id.0)
            };
            match r {
                Ok(id) => assert_eq!(id, u(&case.input["object"]["guid"])),
                Err(e) => failures.push(format!("{label} {key}: {e}")),
            }
        }
        let data = unhex(case.output["obj_desc"].as_str().expect("hex"));
        if let Err(e) = decode_client_body::<dp::objects::ItemObjDescEvent>(&data[4..]) {
            failures.push(format!("{label} obj_desc: {e}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn appraisals_decode_as_the_client_reads_them() {
    let file = vectors::load_named("messages", "serialization_appraise");
    for case in &file.cases {
        let mut body = u(&case.input["object"]["guid"]).to_le_bytes().to_vec();
        body.extend(unhex(&hex_out(case)));
        let d: dp::objects::ItemSetAppraiseInfo = decode_client_body(&body)
            .unwrap_or_else(|e| panic!("{}: {e}", case.input["object"]["class"]));
        assert_eq!(d.object.0, u(&case.input["object"]["guid"]));
    }
    // and the full structure
    let mut body = 0x8000_0001u32.to_le_bytes().to_vec();
    appraise_info::write(&mut body, &full_appraisal());
    let _: dp::objects::ItemSetAppraiseInfo = decode_client_body(&body).expect("full appraisal");
}

#[test]
fn movement_data_decodes_as_the_client_reads_it() {
    use empyrean_world::network::game_messages::messages::game_message_update_motion;
    let file = vectors::load_named("messages", "serialization_motion");
    let mut checked = 0;
    for case in &file.cases {
        if case.input.get("header").is_some_and(|h| h == false) {
            continue;
        }
        // The real message: its alignment is relative to the message, not the vector's own writer.
        let mut o = SequencedObject::new(0x5000_0001);
        let data = match case.input["kind"].as_str().expect("kind") {
            "motion" => MovementData::from_motion(
                g(0x5000_0001),
                &motion_of(&case.input["motion"], g(0x5000_0001)),
            ),
            _ => {
                let bytes = unhex(case.input["hex"].as_str().expect("hex"));
                let state = MoveToState::read(
                    g(0x5000_0001),
                    &mut BinaryReader::new(&bytes),
                    dereth_world_data::command_numbering::CommandNumbering::Final,
                )
                .expect("read");
                MovementData::from_move_to_state(&mut world(), g(0x5000_0001), &state)
            }
        };
        let msg = game_message_update_motion::game_message_update_motion(
            &mut o,
            &data,
            dereth_world_data::command_numbering::CommandNumbering::Final,
        );
        let m: dp::movement::MovementSetObjectMovement = decode_client_message(&msg);
        m.decoded_movement()
            .unwrap_or_else(|e| panic!("{}: movement buffer: {e}", case.input));
        checked += 1;
    }
    assert!(checked > 15, "{checked}");
}

#[test]
fn position_pack_decodes_as_the_client_reads_it() {
    use empyrean_world::network::game_messages::messages::game_message_update_position;
    let mut w = world();
    let mut o = WorldObject::allocate(Class::GenericObject);
    o.guid = g(0x8000_0301);
    o.set_position(
        PositionType::Location,
        Some(Position::from_components(
            0xA9B4_0019,
            84.0,
            7.1,
            94.005,
            0.0,
            0.0,
            -0.0795,
            0.9968,
            false,
        )),
    );
    o.set_placement(Some(Placement::Resting));
    w.objects.insert(o).expect("fresh");
    for admin_move in [false, true] {
        let m = game_message_update_position::game_message_update_position(
            &mut w,
            g(0x8000_0301),
            admin_move,
        );
        let d: dp::movement::MovementPositionEvent = decode_client_message(&m);
        assert_eq!(d.id.0, 0x8000_0301);
    }
}

fn ses() -> SessionData {
    SessionData {
        game_event_sequence: 5,
        ..Default::default()
    }
}

#[test]
fn structure_events_decode_as_the_client_reads_them() {
    let file = vectors::load_named("messages", "serialization_structures");
    let fields = |kind: &str| -> Vec<Vec<Value>> {
        file.cases
            .iter()
            .filter(|c| c.input["kind"] == kind)
            .map(|c| arr(&c.input["fields"]).clone())
            .collect()
    };

    // enchantments
    for e in fields("enchantment") {
        let e = enchantment(&Value::Array(e));
        let _: dp::qualities::MagicUpdateEnchantment = decode_client_message(
            &ev::game_event_magic_update_enchantment::game_event_magic_update_enchantment(
                &mut ses(),
                &e,
            ),
        );
    }
    let list: Vec<Enchantment> = fields("enchantment")
        .into_iter()
        .map(|e| enchantment(&Value::Array(e)))
        .collect();
    let _: dp::qualities::MagicUpdateMultipleEnchantments =
        decode_client_message(&ev::game_event_magic_update_multiple_enchantments::game_event_magic_update_multiple_enchantments(&mut ses(), &list));

    // houses
    for x in fields("house_data") {
        let hd = house_data::HouseData {
            buy_time: u(&x[0]),
            rent_time: u(&x[1]),
            r#type: HouseType(i(&x[2])),
            maintenance_free: b(&x[3]),
            buy: payments(&x[4]),
            rent: payments(&x[5]),
            position: Some(position(&x[6])),
        };
        let _: dp::trade::HouseDataMessage = decode_client_message(
            &ev::game_event_house_data::game_event_house_data(&mut ses(), &hd),
        );
    }
    for (n, x) in fields("house_profile").into_iter().enumerate() {
        let hp = house_profile::HouseProfile {
            dwelling_id: u(&x[0]),
            owner_id: g(u(&x[1])),
            owner_name: os(&x[9]),
            buy: payments(&x[10]),
            ..Default::default()
        };
        let m = ev::game_event_house_profile::game_event_house_profile(
            &mut ses(),
            g(0x7000_0001 + n as u32),
            &hp,
        );
        let _: dp::trade::HouseProfileMessage = decode_client_message(&m);
    }
    let _: dp::trade::HouseUpdateRentPayment = decode_client_message(
        &ev::game_event_house_update_rent_payment::game_event_house_update_rent_payment(&mut ses()),
    );
    let mut r = restriction_db::RestrictionDB {
        house_owner: 0x5000_0001,
        open_status: true,
        ..Default::default()
    };
    r.table.add(g(0x5000_0059), 1);
    let mut holder = SequencedObject::new(0x7000_0018);
    let _: dp::trade::HouseUpdateRestrictions = decode_client_message(
        &ev::game_event_house_update_restrictions::game_event_house_update_restrictions(
            &mut ses(),
            &mut holder,
            &r,
        ),
    );

    // squelches
    let mut db = squelch_db::SquelchDB::default();
    db.characters.add(
        0x5000_0021,
        squelch_info::SquelchInfo::from_filter(SquelchMask(4), "A", false),
    );
    db.globals.filters.push(SquelchMask(0x10));
    let _: dp::comms::CommunicationSetSquelchDb = decode_client_message(
        &ev::game_event_communication_set_squelch::game_event_set_squelch_db(&mut ses(), &db),
    );

    // allegiance (no allegiance)
    let _: dp::social::AllegianceInfoResponse = decode_client_message(
        &ev::game_event_allegiance_info_response::game_event_allegiance_info_response(
            &mut ses(),
            0x5000_0001,
            &allegiance_profile::AllegianceProfile::default(),
        ),
    );

    // chess
    let data = chess_move_data::ChessMoveData {
        r#type: ChessMoveType::FromTo,
        player_guid: g(0x5000_0001),
        color: ChessColor(1),
        piece_guid: g(0x8000_0002),
        from: Some(chess_move_data::ChessPieceCoord { x: 1, y: 2 }),
        to: Some(chess_move_data::ChessPieceCoord { x: 3, y: 4 }),
    };
    let _: dp::trade::GameOpponentTurn = decode_client_message(
        &ev::game_event_opponent_turn::game_event_opponent_turn(&mut ses(), g(0x8000_0010), &data),
    );
}

/// A world object with its own sequences, for builders that take `&mut impl HasSequences`.
struct SequencedObject(WorldObject);

impl SequencedObject {
    fn new(guid: u32) -> Self {
        Self(WorldObject {
            guid: g(guid),
            ..Default::default()
        })
    }
}

impl empyrean_world::network::sequence::sequence_manager::HasSequences for SequencedObject {
    fn world_object(&self) -> &WorldObject {
        &self.0
    }
    fn sequences(&mut self) -> &mut SequenceManager {
        &mut self.0.sequences
    }
}

// ---- the player description (hand-derived: ACE's Player cannot be built in the harness) ------

const SESSION: empyrean_net::SessionId = empyrean_net::SessionId {
    client_id: 1,
    generation: 1,
};
const PLAYER: u32 = 0x5000_0001;

/// A player with a level, a name, strength, health and two skills, a `Character` with options,
/// and a session whose next event is `seq`.
fn player_world(seq: u32) -> World {
    let mut w = world();
    let mut p = WorldObject::allocate(Class::Player);
    p.guid = g(PLAYER);
    p.biota.id = PLAYER;
    p.biota.weenie_type = WeenieType::Creature;
    p.set_property(PropertyInt::Level, 25);
    p.set_property(PropertyString::Name, "Bob".to_owned());
    let mut attrs = DotNetDict::new();
    attrs.insert(
        PropertyAttribute::Strength,
        PropertiesAttribute {
            init_level: 10,
            level_from_cp: 5,
            cp_spent: 100,
        },
    );
    p.biota.properties_attribute = Some(attrs);
    let mut vitals = DotNetDict::new();
    vitals.insert(
        PropertyAttribute2nd::MaxHealth,
        PropertiesAttribute2nd {
            init_level: 0,
            level_from_cp: 2,
            cp_spent: 50,
            current_level: 40,
        },
    );
    p.biota.properties_attribute_2nd = Some(vitals);
    let mut skills = DotNetDict::new();
    skills.insert(
        Skill(38),
        PropertiesSkill {
            level_from_pp: 1,
            sac: SkillAdvancementClass(3),
            pp: 0,
            init_level: 5,
            ..Default::default()
        },
    );
    skills.insert(
        Skill(6),
        PropertiesSkill {
            level_from_pp: 3,
            sac: SkillAdvancementClass(2),
            pp: 70,
            init_level: 10,
            ..Default::default()
        },
    );
    p.biota.properties_skill = Some(skills);
    p.player.as_mut().expect("a player").player.character = Some(Character {
        character_options_1: 0x11,
        character_options_2: 0x22,
        spellbook_filters: 0x3FFF,
        ..Default::default()
    });
    w.objects.insert(p).expect("fresh");
    w.sessions.insert(
        SESSION,
        SessionData {
            player: Some(g(PLAYER)),
            game_event_sequence: seq,
            ..Default::default()
        },
    );
    w
}

#[test]
fn player_description_writes_every_section_in_aces_order() {
    use empyrean_world::network::game_event::events::game_event_player_description::game_event_player_description;

    let mut w = player_world(7);
    let m = game_event_player_description(&mut w, SESSION);

    // Hand-derived from GameEventPlayerDescription.WriteEventBody.
    let mut e = String::new();
    e += "B0F70000"; // GameEvent
    e += "01000050"; // the player
    e += "07000000"; // GameEventSequence
    e += "13000000"; // PlayerDescription
    e += "11000000"; // PropertyInt32 | PropertyString
    e += "0A000000"; // WeenieType.Creature
    e += "01004000"; // 1 int, 64 buckets
    e += "1900000019000000"; // Level = 25
    e += "01002000"; // 1 string, 32 buckets
    e += "010000000300426F62000000"; // Name = "Bob" (not plussed), padded to 4
    e += "03000000"; // Attribute | Skill (no spells, no enchantments)
    e += "01000000"; // Health != null
    e += "FF010000"; // AttributeCache.Full
    e += "050000000A00000064000000"; // Strength: ranks, starting value, xp
    for _ in 0..5 {
        e += "000000000000000000000000"; // Endurance .. Self: new, zero attributes
    }
    e += "02000000000000003200000028000000"; // Health: ranks, init, xp, current
    for _ in 0..2 {
        e += "00000000000000000000000000000000"; // Stamina, Mana
    }
    e += "02002000"; // 2 skills, 32 buckets
                     // Skill 6 and 38 share bucket 6: ordered by id.
                     // id, ranks (ushort), 1 (ushort), advancement class, xp, init level, 0, 0.0
    e += "06000000030001000200000046000000";
    e += "0A000000000000000000000000000000";
    e += "26000000010001000300000000000000";
    e += "05000000000000000000000000000000";
    e += "60040000"; // CharacterOptions2 | SpellLists8 | SpellbookFilters
    e += "11000000"; // CharacterOptions1
    for _ in 0..8 {
        e += "00000000"; // spell bars
    }
    e += "FF3F0000"; // SpellbookFilters
    e += "22000000"; // CharacterOptions2
    e += "00000000"; // inventory
    e += "00000000"; // equipped
    assert_eq!(hex(&m.data), e);
    assert_eq!(
        w.sessions
            .get(SESSION)
            .expect("session")
            .game_event_sequence,
        8
    );

    let d: dp::login::LoginPlayerDescription = decode_client_message(&m);
    assert_eq!(d.player_module.options, 0x11);
    assert_eq!(d.player_module.options2, 0x22);
}

#[test]
fn player_description_orders_inventory_spells_and_equipment_as_ace_does() {
    use empyrean_world::network::game_event::events::game_event_player_description::game_event_player_description;

    let mut w = player_world(1);
    // Items: a pack and a focus in the side slots, two plain items in the main pack.
    let mut add =
        |guid: u32, class: Class, wt: WeenieType, placement: Option<i32>, pack_slot: bool| {
            let mut o = WorldObject::allocate(class);
            o.guid = g(guid);
            o.biota.weenie_type = wt;
            if let Some(p) = placement {
                o.set_property(PropertyInt::PlacementPosition, p);
            }
            if pack_slot {
                o.set_property(PropertyBool::RequiresBackpackSlot, true);
            }
            o.set_property(PropertyInt::CurrentWieldedLocation, 0x10);
            o.set_property(PropertyInt::ClothingPriority, 0x8);
            w.objects.insert(o).expect("fresh");
        };
    add(
        0x8000_0010,
        Class::GenericObject,
        WeenieType::Generic,
        Some(2),
        false,
    );
    add(
        0x8000_0011,
        Class::Container,
        WeenieType::Container,
        Some(1),
        false,
    );
    add(
        0x8000_0012,
        Class::GenericObject,
        WeenieType::Generic,
        None,
        false,
    );
    add(
        0x8000_0013,
        Class::GenericObject,
        WeenieType::Generic,
        Some(0),
        true,
    );
    add(
        0x8000_0014,
        Class::Clothing,
        WeenieType::Clothing,
        None,
        false,
    );
    {
        let p = w.objects.get_mut(g(PLAYER)).expect("player");
        set_inventory(
            p,
            &[
                g(0x8000_0010),
                g(0x8000_0011),
                g(0x8000_0012),
                g(0x8000_0013),
            ],
        );
        set_equipped_objects(p, &[g(0x8000_0014)]);
        let mut spells = DotNetDict::new();
        for s in [65, 1, 64, 2] {
            spells.insert(s, 2.0f32);
        }
        p.biota.properties_spell_book = Some(spells);
        // a cooldown (no spell lookup) makes the enchantment registry appear
        p.biota.properties_enchantment_registry = Some(vec![PropertiesEnchantmentRegistry {
            spell_id: 0x8005,
            layer_id: 1,
            spell_category: SpellCategory(0x8000),
            duration: 10.0,
            ..Default::default()
        }]);
        p.set_position(
            PositionType::LastOutsideDeath,
            Some(Position::from_components(
                0xA9B4_0019,
                1.0,
                2.0,
                3.0,
                0.0,
                0.0,
                0.0,
                1.0,
                false,
            )),
        );
        let c = p
            .player
            .as_mut()
            .expect("a player")
            .player
            .character
            .as_mut()
            .expect("character");
        c.is_plussed = true;
        c.character_properties_shortcut_bar = vec![shortcut_row(3, 0x8000_0010)];
        c.character_properties_spell_bar = vec![
            empyrean_store::models::shard::CharacterPropertiesSpellBar {
                character_id: PLAYER,
                spell_bar_number: 1,
                spell_bar_index: 2,
                spell_id: 64,
            },
            empyrean_store::models::shard::CharacterPropertiesSpellBar {
                character_id: PLAYER,
                spell_bar_number: 1,
                spell_bar_index: 1,
                spell_id: 65,
            },
            empyrean_store::models::shard::CharacterPropertiesSpellBar {
                character_id: PLAYER,
                spell_bar_number: 8,
                spell_bar_index: 1,
                spell_id: 2,
            },
        ];
        c.character_properties_fill_comp_book = vec![
            CharacterPropertiesFillCompBook {
                character_id: PLAYER,
                spell_component_id: 257,
                quantity_to_rebuy: 5,
            },
            CharacterPropertiesFillCompBook {
                character_id: PLAYER,
                spell_component_id: 1,
                quantity_to_rebuy: 7,
            },
        ];
    }
    let m = game_event_player_description(&mut w, SESSION);
    let d: dp::login::LoginPlayerDescription = decode_client_message(&m);

    // "+" for a plussed character below cloak level Player.
    let names: Vec<_> = format!("{:?}", d.qualities.base)
        .match_indices("+Bob")
        .map(|(i, _)| i)
        .collect();
    assert_eq!(names.len(), 1, "{:?}", d.qualities.base);
    // Non-containers first by placement (null first), then the side slots.
    let placed: Vec<u32> = d.inventory_placements.iter().map(|p| p.iid.0).collect();
    let content: Vec<u32> = d.content_profiles.iter().map(|p| p.iid.0).collect();
    assert_eq!(
        content,
        vec![0x8000_0012, 0x8000_0010, 0x8000_0013, 0x8000_0011]
    );
    assert_eq!(placed, vec![0x8000_0014]);
    // Spell bars: bar 1 by index, bar 8.
    assert_eq!(d.player_module.spell_bars[0], vec![65, 64]);
    assert_eq!(d.player_module.spell_bars[7], vec![2]);
    assert!(d.qualities.enchantments.is_some());
    assert!(d.qualities.spell_book.is_some());
}

#[test]
fn a_players_object_description_has_its_characters_hair_and_the_login_physics_state() {
    let mut w = player_world(1);
    {
        let c = w
            .objects
            .get_mut(g(PLAYER))
            .expect("player")
            .player
            .as_mut()
            .expect("a player");
        let ch = c.player.character.as_mut().expect("character");
        ch.default_hair_texture = 0x0500_0900;
        ch.hair_texture = 0x0500_0901;
    }
    // Hand-derived: AddBaseModelData's player texture (part 0x10, packed of known type 0x5000000),
    // no equipment, no setup, aligned within the message (17 bytes, 3 of padding); then the
    // instance and the next visual description sequences.
    let m = game_message_obj_desc_event::game_message_obj_desc_event(&mut w, g(PLAYER));
    assert_eq!(
        hex(&m.data),
        "25F600000100005011000100100009010900000000000100"
    );

    // A player without a physics body is sent the login state IgnoreCollisions | Gravity |
    // Hidden | EdgeSlide; with adminvision and cloaked, the translucency is 0.5.
    w.objects
        .get_mut(g(PLAYER))
        .expect("player")
        .set_property(PropertyInt::CloakStatus, CloakStatus::On.0);
    let plain =
        game_message_create_object::game_message_create_object(&mut w, g(PLAYER), false, false);
    let admin =
        game_message_create_object::game_message_create_object(&mut w, g(PLAYER), true, false);
    let plain_d: dp::objects::ItemCreateObject = decode_client_message(&plain);
    let admin_d: dp::objects::ItemCreateObject = decode_client_message(&admin);
    assert_eq!(
        format!("{:?}", plain_d.0.physicsdesc)
            .matches("translucency: None")
            .count()
            + 1,
        2,
        "{:?}",
        plain_d.0.physicsdesc
    );
    assert!(
        format!("{:?}", admin_d.0.physicsdesc).contains("0.5"),
        "{:?}",
        admin_d.0.physicsdesc
    );
    // after the object description: flags, then the state
    let physics_flags_at = 4 + 4 + 12;
    assert_eq!(
        hex(&plain.data[physics_flags_at + 4..physics_flags_at + 8]),
        hex(&0x0040_4410u32.to_le_bytes())
    );
}

/// V334 (V334, retail): the description's script is data id 44 and the physics description's
/// default script is data id 30, each only from its own id. ACE wrote data id 30 in both.
#[test]
fn the_descriptions_script_is_data_id_44_and_the_default_script_data_id_30() {
    let mut w = world();
    let guid = g(0x8000_0A01);
    let mut o = WorldObject::allocate(Class::GenericObject);
    o.guid = guid;
    o.biota.id = guid.full();
    o.biota.weenie_class_id = 1499;
    o.biota.weenie_type = WeenieType::Generic;
    o.set_property(PropertyDataId::PhysicsScript, 0x5A);
    o.set_property(PropertyDataId::RestrictionEffect, 0x98);
    w.objects.insert(o).expect("fresh guid");
    let scripts = |w: &mut World| {
        let d: dp::objects::ItemCreateObject = decode_client_message(
            &game_message_create_object::game_message_create_object(w, guid, false, false),
        );
        (d.0.wdesc.pscript, d.0.physicsdesc.default_script)
    };
    assert_eq!(scripts(&mut w), (Some(0x98), Some(0x5A)), "both ids set");

    // Data id 30 alone (a spell projectile): no description script, as retail sent it.
    w.objects
        .get_mut(guid)
        .expect("inserted")
        .remove_property(PropertyDataId::RestrictionEffect);
    assert_eq!(scripts(&mut w), (None, Some(0x5A)), "data id 30 only");

    // Data id 44 alone (a house): the description script and no default script.
    let o = w.objects.get_mut(guid).expect("inserted");
    o.remove_property(PropertyDataId::PhysicsScript);
    o.set_property(
        PropertyDataId::RestrictionEffect,
        PlayScript::RestrictionEffectBlue.0,
    );
    assert_eq!(scripts(&mut w), (Some(152), None), "data id 44 only");
}

// ---- real content --------------------------------------------------------------------------

/// The content tier (`--features real-content`): every weenie in the real `world.pack`, built with
/// the real dats, yields a `CreateObject` that dereth-protocol decodes. Paths: `EMPYREAN_TEST_WORLD_PACK`
/// (default `world.pack` in the repository) and `DERETH_TEST_DAT_DIR`. Run in release.
#[cfg(feature = "real-content")]
mod real_content {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    use empyrean_content::PackContent;
    use empyrean_dat::{DatManager, RealDats};
    use empyrean_world::world_objects::world_object::CtorEnv;

    use super::*;

    fn pack_path() -> PathBuf {
        empyrean_common::test_paths::world_pack()
    }

    fn client_dir() -> PathBuf {
        dereth_dat::testing::dat_dir()
    }

    #[test]
    fn every_weenie_in_the_real_pack_yields_a_create_object_the_client_decodes() {
        let dats = DatManager::initialize(Arc::new(
            RealDats::open(&client_dir()).expect("the retail dats"),
        ))
        .expect("dats");
        let now = ClockSnapshot {
            portal_year_ticks: 0.0,
            unix_time: 1_000_000.0,
            utc: DotNetDateTime::new(2026, 1, 1),
            monotonic: Duration::ZERO,
        };
        let mut w = World::new(now, dats);
        w.content = Arc::new(PackContent::open(&pack_path()).expect("world.pack"));

        let wcids: Vec<u32> = w
            .content
            .get_all_weenie_class_names()
            .keys()
            .copied()
            .collect();
        let mut built = 0usize;
        let mut not_constructed = 0usize;
        let mut panics: BTreeMap<String, Vec<u32>> = BTreeMap::new();
        let mut failures: BTreeMap<String, Vec<u32>> = BTreeMap::new();
        let mut decoded = 0usize;
        for (n, wcid) in wcids.iter().enumerate() {
            let guid = g(0x8000_0000 + u32::try_from(n).expect("count"));
            let weenie = w.content.get_cached_weenie(*wcid);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                CtorEnv::with_world(&w, |env| {
                    empyrean_world::factories::world_object_factory::create_world_object(
                        env,
                        weenie.clone(),
                        guid,
                    )
                })
            }));
            let o = match result {
                Ok(Some(o)) => o,
                Ok(None) => {
                    not_constructed += 1;
                    continue;
                }
                Err(e) => {
                    let msg = e
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                        .unwrap_or_default();
                    panics
                        .entry(format!(
                            "construct: {}",
                            msg.chars().take(80).collect::<String>()
                        ))
                        .or_default()
                        .push(*wcid);
                    continue;
                }
            };
            let mut o = o;
            // The constructors' `CurrentMotionState` (a pointer in WorldObject/Creature
            // SetEphemeralValues): `new Motion(Invalid)` with a motion table, and a creature's
            // `new Motion(NonCombat, Ready)`.
            if o.motion_table_id() != 0 {
                o.wo.world_object_properties.current_motion_state =
                    Some(Motion::from_stance(MotionStance::Invalid));
            }
            if o.is_creature() {
                o.wo.world_object_properties.current_motion_state = Some(Motion::new(
                    MotionStance::NonCombat,
                    MotionCommand::Ready,
                    1.0,
                ));
            }
            w.objects.insert(o).expect("fresh guid");
            built += 1;

            let msg = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                game_message_create_object::game_message_create_object(&mut w, guid, false, false)
            }));
            let msg = match msg {
                Ok(m) => m,
                Err(e) => {
                    let msg = e
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                        .unwrap_or_default();
                    panics
                        .entry(format!(
                            "serialize: {}",
                            msg.chars().take(80).collect::<String>()
                        ))
                        .or_default()
                        .push(*wcid);
                    w.objects.remove(guid);
                    continue;
                }
            };
            match decode_client_body::<dp::objects::ItemCreateObject>(&msg.data[4..]) {
                Ok(_) => decoded += 1,
                Err(e) => failures
                    .entry(e.chars().take(60).collect())
                    .or_default()
                    .push(*wcid),
            }
            let desc = game_message_obj_desc_event::game_message_obj_desc_event(&mut w, guid);
            if let Err(e) = decode_client_body::<dp::objects::ItemObjDescEvent>(&desc.data[4..]) {
                failures
                    .entry(format!(
                        "obj desc: {}",
                        e.chars().take(60).collect::<String>()
                    ))
                    .or_default()
                    .push(*wcid);
            }
            w.objects.remove(guid);
        }
        eprintln!(
            "real-content sweep: {} weenies, {built} built, {not_constructed} not constructed (Undef type), {decoded} decoded",
            wcids.len()
        );
        for (k, v) in &panics {
            eprintln!(
                "  panic x{}: {k} (e.g. wcid {:?})",
                v.len(),
                &v[..v.len().min(5)]
            );
        }
        for (k, v) in &failures {
            eprintln!(
                "  decode failure x{}: {k} (e.g. wcid {:?})",
                v.len(),
                &v[..v.len().min(5)]
            );
        }
        assert!(
            failures.is_empty(),
            "{} weenies do not decode",
            failures.values().map(Vec::len).sum::<usize>()
        );
    }
}

/// A `CharacterPropertiesShortcutBar` row (1-based bar index).
fn shortcut_row(
    index: u32,
    object: u32,
) -> empyrean_store::models::shard::CharacterPropertiesShortcutBar {
    empyrean_store::models::shard::CharacterPropertiesShortcutBar {
        character_id: PLAYER,
        shortcut_bar_index: index,
        shortcut_object_id: object,
    }
}
