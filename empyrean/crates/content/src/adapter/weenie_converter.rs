// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Adapter/WeenieConverter.cs
//! A world-database [`Weenie`](crate::models::world::Weenie) row to the entity
//! [`Weenie`](empyrean_entity::Weenie) the server works with.
//!
//! Casts follow C#: an unsigned column cast to a signed enum (`(EmoteCategory)uint`) keeps the bit
//! pattern, and `new Dictionary(...)`/indexer sets keep the rows' order, so a repeated key keeps its
//! first slot and takes its last value.

use std::sync::Arc;

use empyrean_common::dotnet::{DotNetDict, DotNetHashSet};
use empyrean_entity::enums::{
    CombatBodyPart, DamageType, DestinationType, EmoteCategory, MotionCommand, MotionStance,
    PlayScript, PositionType, PropertyAttribute, PropertyAttribute2nd, PropertyBool,
    PropertyDataId, PropertyFloat, PropertyInstanceId, PropertyInt, PropertyInt64, PropertyString,
    RegenLocationType, RegenerationType, Skill, SkillAdvancementClass, Sound, VendorType,
    WeenieType,
};
use empyrean_entity::models::{
    PropertiesAnimPart, PropertiesAttribute, PropertiesAttribute2nd, PropertiesBodyPart,
    PropertiesBook, PropertiesBookPageData, PropertiesCreateList, PropertiesEmote,
    PropertiesEmoteAction, PropertiesGenerator, PropertiesPalette, PropertiesPosition,
    PropertiesSkill, PropertiesTextureMap,
};

use crate::models::world;

/// C# `(int)uint`: the bit pattern.
#[allow(clippy::cast_possible_wrap)]
fn as_i32(v: u32) -> i32 {
    v as i32
}

/// C# `(uint)int`: the bit pattern.
#[allow(clippy::cast_sign_loss)]
fn as_u32(v: i32) -> u32 {
    v as u32
}

// ACE: WeenieConverter
// ACE: WeenieConverter.ConvertToEntityWeenie
#[must_use]
pub fn convert_to_entity_weenie(
    weenie: &world::Weenie,
    instantiate_empty_collections: bool,
) -> empyrean_entity::Weenie {
    let mut result = empyrean_entity::Weenie {
        weenie_class_id: weenie.class_id,
        class_name: Some(weenie.class_name.clone()),
        weenie_type: WeenieType(as_u32(weenie.r#type)),
        ..Default::default()
    };

    let want = |count: usize| instantiate_empty_collections || count > 0;

    if want(weenie.weenie_properties_bool.len()) {
        let mut d = DotNetDict::new();
        for value in &weenie.weenie_properties_bool {
            d.insert(PropertyBool(value.r#type), value.value);
        }
        result.properties_bool = Some(d);
    }
    if want(weenie.weenie_properties_did.len()) {
        let mut d = DotNetDict::new();
        for value in &weenie.weenie_properties_did {
            d.insert(PropertyDataId(value.r#type), value.value);
        }
        result.properties_did = Some(d);
    }
    if want(weenie.weenie_properties_float.len()) {
        let mut d = DotNetDict::new();
        for value in &weenie.weenie_properties_float {
            d.insert(PropertyFloat(value.r#type), value.value);
        }
        result.properties_float = Some(d);
    }
    if want(weenie.weenie_properties_iid.len()) {
        let mut d = DotNetDict::new();
        for value in &weenie.weenie_properties_iid {
            d.insert(PropertyInstanceId(value.r#type), value.value);
        }
        result.properties_iid = Some(d);
    }
    if want(weenie.weenie_properties_int.len()) {
        let mut d = DotNetDict::new();
        for value in &weenie.weenie_properties_int {
            d.insert(PropertyInt(value.r#type), value.value);
        }
        result.properties_int = Some(d);
    }
    if want(weenie.weenie_properties_int64.len()) {
        let mut d = DotNetDict::new();
        for value in &weenie.weenie_properties_int64 {
            d.insert(PropertyInt64(value.r#type), value.value);
        }
        result.properties_int64 = Some(d);
    }
    if want(weenie.weenie_properties_string.len()) {
        let mut d = DotNetDict::new();
        for value in &weenie.weenie_properties_string {
            d.insert(PropertyString(value.r#type), value.value.clone());
        }
        result.properties_string = Some(d);
    }

    if want(weenie.weenie_properties_position.len()) {
        let mut d = DotNetDict::new();
        for record in &weenie.weenie_properties_position {
            let new_entity = PropertiesPosition {
                obj_cell_id: record.obj_cell_id,
                position_x: record.origin_x,
                position_y: record.origin_y,
                position_z: record.origin_z,
                rotation_w: record.angles_w,
                rotation_x: record.angles_x,
                rotation_y: record.angles_y,
                rotation_z: record.angles_z,
            };
            d.insert(PositionType(record.position_type), new_entity);
        }
        result.properties_position = Some(d);
    }

    if want(weenie.weenie_properties_spell_book.len()) {
        let mut d = DotNetDict::new();
        for value in &weenie.weenie_properties_spell_book {
            d.insert(value.spell, value.probability);
        }
        result.properties_spell_book = Some(d);
    }

    if want(weenie.weenie_properties_anim_part.len()) {
        result.properties_anim_part = Some(
            weenie
                .weenie_properties_anim_part
                .iter()
                .map(|record| PropertiesAnimPart {
                    index: record.index,
                    animation_id: record.animation_id,
                })
                .collect(),
        );
    }

    if want(weenie.weenie_properties_palette.len()) {
        result.properties_palette = Some(
            weenie
                .weenie_properties_palette
                .iter()
                .map(|record| PropertiesPalette {
                    sub_palette_id: record.sub_palette_id,
                    offset: record.offset,
                    length: record.length,
                })
                .collect(),
        );
    }

    if want(weenie.weenie_properties_texture_map.len()) {
        result.properties_texture_map = Some(
            weenie
                .weenie_properties_texture_map
                .iter()
                .map(|record| PropertiesTextureMap {
                    part_index: record.index,
                    old_texture: record.old_id,
                    new_texture: record.new_id,
                })
                .collect(),
        );
    }

    // Properties for all world objects that typically aren't modified over the original weenie

    if want(weenie.weenie_properties_create_list.len()) {
        result.properties_create_list = Some(Arc::new(
            weenie
                .weenie_properties_create_list
                .iter()
                .map(|record| PropertiesCreateList {
                    destination_type: DestinationType(i32::from(record.destination_type)),
                    weenie_class_id: record.weenie_class_id,
                    stack_size: record.stack_size,
                    palette: record.palette,
                    shade: record.shade,
                    try_to_bond: record.try_to_bond,
                    ..Default::default()
                })
                .collect(),
        ));
    }

    if want(weenie.weenie_properties_emote.len()) {
        let mut list = Vec::new();
        for record in &weenie.weenie_properties_emote {
            let mut new_entity = PropertiesEmote {
                category: EmoteCategory(as_i32(record.category)),
                probability: record.probability,
                weenie_class_id: record.weenie_class_id,
                style: record.style.map(MotionStance),
                substyle: record.substyle.map(MotionCommand),
                quest: record.quest.clone(),
                vendor_type: record.vendor_type.map(VendorType),
                min_health: record.min_health,
                max_health: record.max_health,
                ..Default::default()
            };

            // LINQ `OrderBy` is a stable sort.
            let mut actions: Vec<&world::WeeniePropertiesEmoteAction> =
                record.weenie_properties_emote_action.iter().collect();
            actions.sort_by_key(|r| r.order);
            for record2 in actions {
                let new_entity2 = PropertiesEmoteAction {
                    r#type: record2.r#type,
                    delay: record2.delay,
                    extent: record2.extent,
                    motion: record2.motion.map(MotionCommand),
                    message: record2.message.clone(),
                    test_string: record2.test_string.clone(),
                    min: record2.min,
                    max: record2.max,
                    min_64: record2.min_64,
                    max_64: record2.max_64,
                    min_dbl: record2.min_dbl,
                    max_dbl: record2.max_dbl,
                    stat: record2.stat,
                    display: record2.display,
                    amount: record2.amount,
                    amount_64: record2.amount_64,
                    hero_xp_64: record2.hero_xp_64,
                    percent: record2.percent,
                    spell_id: record2.spell_id,
                    wealth_rating: record2.wealth_rating,
                    treasure_class: record2.treasure_class,
                    treasure_type: record2.treasure_type,
                    p_script: record2.p_script.map(|v| PlayScript(as_u32(v))),
                    sound: record2.sound.map(|v| Sound(as_u32(v))),
                    destination_type: record2.destination_type,
                    weenie_class_id: record2.weenie_class_id,
                    stack_size: record2.stack_size,
                    palette: record2.palette,
                    shade: record2.shade,
                    try_to_bond: record2.try_to_bond,
                    obj_cell_id: record2.obj_cell_id,
                    origin_x: record2.origin_x,
                    origin_y: record2.origin_y,
                    origin_z: record2.origin_z,
                    angles_w: record2.angles_w,
                    angles_x: record2.angles_x,
                    angles_y: record2.angles_y,
                    angles_z: record2.angles_z,
                    ..Default::default()
                };

                new_entity.properties_emote_action.push(new_entity2);
            }

            list.push(new_entity);
        }
        result.properties_emote = Some(Arc::new(list));
    }

    if want(weenie.weenie_properties_event_filter.len()) {
        let mut set = DotNetHashSet::new();
        for value in &weenie.weenie_properties_event_filter {
            set.insert(value.event);
        }
        result.properties_event_filter = Some(Arc::new(set));
    }

    if want(weenie.weenie_properties_generator.len()) {
        // TODO do we have the correct order? (ACE's own note)
        result.properties_generator = Some(Arc::new(
            weenie
                .weenie_properties_generator
                .iter()
                .map(|record| PropertiesGenerator {
                    probability: record.probability,
                    weenie_class_id: record.weenie_class_id,
                    delay: record.delay,
                    init_create: record.init_create,
                    max_create: record.max_create,
                    when_create: RegenerationType(record.when_create),
                    where_create: RegenLocationType(record.where_create),
                    stack_size: record.stack_size,
                    palette_id: record.palette_id,
                    shade: record.shade,
                    obj_cell_id: record.obj_cell_id,
                    origin_x: record.origin_x,
                    origin_y: record.origin_y,
                    origin_z: record.origin_z,
                    angles_w: record.angles_w,
                    angles_x: record.angles_x,
                    angles_y: record.angles_y,
                    angles_z: record.angles_z,
                    ..Default::default()
                })
                .collect(),
        ));
    }

    // Properties for creatures

    if want(weenie.weenie_properties_attribute.len()) {
        let mut d = DotNetDict::new();
        for record in &weenie.weenie_properties_attribute {
            let new_entity = PropertiesAttribute {
                init_level: record.init_level,
                level_from_cp: record.level_from_cp,
                cp_spent: record.cp_spent,
            };
            d.insert(PropertyAttribute(record.r#type), new_entity);
        }
        result.properties_attribute = Some(d);
    }

    if want(weenie.weenie_properties_attribute_2nd.len()) {
        let mut d = DotNetDict::new();
        for record in &weenie.weenie_properties_attribute_2nd {
            let new_entity = PropertiesAttribute2nd {
                init_level: record.init_level,
                level_from_cp: record.level_from_cp,
                cp_spent: record.cp_spent,
                current_level: record.current_level,
            };
            d.insert(PropertyAttribute2nd(record.r#type), new_entity);
        }
        result.properties_attribute_2nd = Some(d);
    }

    if want(weenie.weenie_properties_body_part.len()) {
        let mut d = DotNetDict::new();
        for record in &weenie.weenie_properties_body_part {
            let new_entity = PropertiesBodyPart {
                d_type: DamageType(record.d_type),
                d_val: record.d_val,
                d_var: record.d_var,
                base_armor: record.base_armor,
                armor_vs_slash: record.armor_vs_slash,
                armor_vs_pierce: record.armor_vs_pierce,
                armor_vs_bludgeon: record.armor_vs_bludgeon,
                armor_vs_cold: record.armor_vs_cold,
                armor_vs_fire: record.armor_vs_fire,
                armor_vs_acid: record.armor_vs_acid,
                armor_vs_electric: record.armor_vs_electric,
                armor_vs_nether: record.armor_vs_nether,
                bh: record.bh,
                hlf: record.hlf,
                mlf: record.mlf,
                llf: record.llf,
                hrf: record.hrf,
                mrf: record.mrf,
                lrf: record.lrf,
                hlb: record.hlb,
                mlb: record.mlb,
                llb: record.llb,
                hrb: record.hrb,
                mrb: record.mrb,
                lrb: record.lrb,
            };
            d.insert(CombatBodyPart(i32::from(record.key)), new_entity);
        }
        result.properties_body_part = Some(Arc::new(d));
    }

    if want(weenie.weenie_properties_skill.len()) {
        let mut d = DotNetDict::new();
        for record in &weenie.weenie_properties_skill {
            let new_entity = PropertiesSkill {
                level_from_pp: record.level_from_pp,
                sac: SkillAdvancementClass(record.sac),
                pp: record.pp,
                init_level: record.init_level,
                resistance_at_last_check: record.resistance_at_last_check,
                last_used_time: record.last_used_time,
            };
            d.insert(Skill(i32::from(record.r#type)), new_entity);
        }
        result.properties_skill = Some(d);
    }

    // Properties for books

    if let Some(book) = &weenie.weenie_properties_book {
        result.properties_book = Some(PropertiesBook {
            max_num_pages: book.max_num_pages,
            max_num_chars_per_page: book.max_num_chars_per_page,
        });
    }

    if want(weenie.weenie_properties_book_page_data.len()) {
        let mut pages: Vec<&world::WeeniePropertiesBookPageData> =
            weenie.weenie_properties_book_page_data.iter().collect();
        pages.sort_by_key(|r| r.page_id);
        result.properties_book_page_data = Some(
            pages
                .into_iter()
                .map(|record| PropertiesBookPageData {
                    author_id: record.author_id,
                    author_name: Some(record.author_name.clone()),
                    author_account: Some(record.author_account.clone()),
                    ignore_author: record.ignore_author,
                    page_text: Some(record.page_text.clone()),
                })
                .collect(),
        );
    }

    result
}

// ACE: WeenieConverter.ConvertFromEntityWeenie
/// ACE throws `NotImplementedException`; so does this.
///
/// # Panics
/// Always.
#[must_use]
pub fn convert_from_entity_weenie(_weenie: &empyrean_entity::Weenie) -> world::Weenie {
    panic!("NotImplementedException: WeenieConverter.ConvertFromEntityWeenie")
}
