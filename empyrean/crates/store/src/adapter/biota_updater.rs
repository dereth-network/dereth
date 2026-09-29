// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Adapter/BiotaUpdater.cs
//! `BiotaUpdater`: brings a loaded database biota ([`db::Biota`]) up to date with the entity
//! biota, row by row, the way ACE lets Entity Framework compute the changes.
//!
//! DIVERGE (arch): there is no `ShardDbContext`. ACE marks a stale row deleted with
//! `context.X.Remove(row)` and Entity Framework drops it from the collection when the save
//! succeeds; here the row is dropped from the collection at the end of the section that found it,
//! and the save writes the collections as they stand (see `ShardDatabase::do_save_biota`).
//!
//! As in ACE, the entity biota is written to as well: create-list, emote and generator records
//! learn the id of the row they were matched to (`DatabaseRecordId`). The caller's `source_biota`
//! is the database thread's owned snapshot, so this reaches the world only if the world sends that
//! snapshot back; either way the rows written are the same.

use empyrean_entity::enums::{
    CombatBodyPart, PositionType, PropertyAttribute, PropertyAttribute2nd, Skill,
};
use empyrean_entity::models::{
    PropertiesCreateList, PropertiesEmote, PropertiesEmoteAction, PropertiesGenerator,
};

use crate::adapter::biota_converter::{body_part_row, enchantment_row};
use crate::cast::from_index;
use crate::models::shard as db;
use empyrean_common::dotnet::CsCast;

// ACE: BiotaUpdater
/// `BiotaUpdater` (a static class in ACE).
#[derive(Debug)]
pub struct BiotaUpdater;

/// Drops the rows whose flag is set (Entity Framework's delete fix-up).
fn remove_marked<T>(rows: &mut Vec<T>, remove: &[bool]) {
    let mut i = 0;
    rows.retain(|_| {
        let keep = !remove[i];
        i += 1;
        keep
    });
}

/// The index-matched lists (anim parts, palettes, texture maps): the row whose `order` is `i`
/// carries entry `i`; rows with no order, or an order past the end, are deleted.
macro_rules! update_ordered_list {
    ($source:expr, $target:expr, $object_id:expr, |$value:ident, $existing:ident| $copy:block) => {{
        if let Some(list) = $source {
            for (i, $value) in list.iter().enumerate() {
                let found = $target.iter().position(|r| r.order.map(i32::from) == Some(from_index::<i32>(i)));
                let idx = match found {
                    Some(idx) => idx,
                    None => {
                        $target.push(Default::default());
                        $target.last_mut().unwrap().object_id = $object_id;
                        $target.len() - 1
                    }
                };
                let $existing = &mut $target[idx];
                $copy
                $existing.order = Some(from_index::<u8>(i));
            }
        }
        let count = $source.map(|l| l.len());
        let remove: Vec<bool> = $target
            .iter()
            .map(|value| match (count, value.order) {
                (None, _) | (_, None) => true,
                (Some(c), Some(o)) => usize::from(o) >= c,
            })
            .collect();
        remove_marked($target, &remove);
    }};
}

impl BiotaUpdater {
    // ACE: BiotaUpdater.UpdateDatabaseBiota
    /// Updates `target_biota` (the rows as loaded) from `source_biota` (the entity biota).
    #[allow(clippy::too_many_lines)]
    pub fn update_database_biota(
        source_biota: &mut empyrean_entity::Biota,
        target_biota: &mut db::Biota,
    ) {
        target_biota.weenie_class_id = source_biota.weenie_class_id;
        target_biota.weenie_type = source_biota.weenie_type.0.cs_cast();

        let id = source_biota.id;

        // The seven plain property bags: set every source value, then delete rows the source lacks.
        macro_rules! update_bag {
            ($bag:ident, $rows:ident, $set:ident, $key:path, $val:expr) => {{
                if let Some(d) = &source_biota.$bag {
                    for (k, v) in d.iter() {
                        target_biota.$set(*k, $val(v));
                    }
                }
                let remove: Vec<bool> = target_biota
                    .$rows
                    .iter()
                    .map(|value| {
                        source_biota
                            .$bag
                            .as_ref()
                            .is_none_or(|d| !d.contains_key(&$key(value.r#type)))
                    })
                    .collect();
                remove_marked(&mut target_biota.$rows, &remove);
            }};
        }

        update_bag!(
            properties_bool,
            biota_properties_bool,
            set_property_bool,
            empyrean_entity::enums::PropertyBool,
            |v: &bool| *v
        );
        update_bag!(
            properties_did,
            biota_properties_did,
            set_property_did,
            empyrean_entity::enums::PropertyDataId,
            |v: &u32| *v
        );
        update_bag!(
            properties_float,
            biota_properties_float,
            set_property_float,
            empyrean_entity::enums::PropertyFloat,
            |v: &f64| *v
        );
        update_bag!(
            properties_iid,
            biota_properties_iid,
            set_property_iid,
            empyrean_entity::enums::PropertyInstanceId,
            |v: &u32| *v
        );
        update_bag!(
            properties_int,
            biota_properties_int,
            set_property_int,
            empyrean_entity::enums::PropertyInt,
            |v: &i32| *v
        );
        update_bag!(
            properties_int64,
            biota_properties_int64,
            set_property_int64,
            empyrean_entity::enums::PropertyInt64,
            |v: &i64| *v
        );
        update_bag!(
            properties_string,
            biota_properties_string,
            set_property_string,
            empyrean_entity::enums::PropertyString,
            String::as_str
        );

        if let Some(d) = &source_biota.properties_position {
            for (k, v) in d.iter() {
                let rows = &mut target_biota.biota_properties_position;
                let idx = match rows.iter().position(|r| r.position_type == k.0) {
                    Some(idx) => idx,
                    None => {
                        rows.push(db::BiotaPropertiesPosition {
                            object_id: id,
                            ..Default::default()
                        });
                        rows.len() - 1
                    }
                };
                let existing_value = &mut rows[idx];

                existing_value.position_type = k.0;
                existing_value.obj_cell_id = v.obj_cell_id;
                existing_value.origin_x = v.position_x;
                existing_value.origin_y = v.position_y;
                existing_value.origin_z = v.position_z;
                existing_value.angles_w = v.rotation_w;
                existing_value.angles_x = v.rotation_x;
                existing_value.angles_y = v.rotation_y;
                existing_value.angles_z = v.rotation_z;

                // Entity Framework is unable to store NaN floats in the database and results in an error of:
                // ERROR 1054: Unknown column 'NaN' in 'field list'
                if existing_value.angles_x.is_nan()
                    || existing_value.angles_y.is_nan()
                    || existing_value.angles_z.is_nan()
                    || existing_value.angles_w.is_nan()
                {
                    existing_value.angles_w = 1.0;
                    existing_value.angles_x = 0.0;
                    existing_value.angles_y = 0.0;
                    existing_value.angles_z = 0.0;
                }
            }
        }
        let remove: Vec<bool> = target_biota
            .biota_properties_position
            .iter()
            .map(|value| {
                source_biota
                    .properties_position
                    .as_ref()
                    .is_none_or(|d| !d.contains_key(&PositionType(value.position_type)))
            })
            .collect();
        remove_marked(&mut target_biota.biota_properties_position, &remove);

        if let Some(d) = &source_biota.properties_spell_book {
            // Optimization to help characters with very large spell books and avoid full iterations inside the foreach
            let mut existing_values: std::collections::HashMap<i32, usize> = target_biota
                .biota_properties_spell_book
                .iter()
                .enumerate()
                .map(|(i, r)| (r.spell, i))
                .collect();

            for (k, v) in d.iter() {
                let idx = match existing_values.get(k) {
                    Some(&idx) => idx,
                    None => {
                        target_biota.biota_properties_spell_book.push(
                            db::BiotaPropertiesSpellBook {
                                object_id: id,
                                ..Default::default()
                            },
                        );
                        let idx = target_biota.biota_properties_spell_book.len() - 1;
                        // ToDictionary was built before the loop, so a later duplicate key cannot
                        // see this row; the source dictionary has unique keys anyway.
                        existing_values.entry(*k).or_insert(idx);
                        idx
                    }
                };
                let existing_value = &mut target_biota.biota_properties_spell_book[idx];
                existing_value.spell = *k;
                existing_value.probability = *v;
            }
        }
        let remove: Vec<bool> = target_biota
            .biota_properties_spell_book
            .iter()
            .map(|value| {
                source_biota
                    .properties_spell_book
                    .as_ref()
                    .is_none_or(|d| !d.contains_key(&value.spell))
            })
            .collect();
        remove_marked(&mut target_biota.biota_properties_spell_book, &remove);

        update_ordered_list!(
            source_biota.properties_anim_part.as_ref(),
            &mut target_biota.biota_properties_anim_part,
            id,
            |value, existing_value| {
                existing_value.index = value.index;
                existing_value.animation_id = value.animation_id;
            }
        );

        update_ordered_list!(
            source_biota.properties_palette.as_ref(),
            &mut target_biota.biota_properties_palette,
            id,
            |value, existing_value| {
                existing_value.sub_palette_id = value.sub_palette_id;
                existing_value.offset = value.offset;
                existing_value.length = value.length;
            }
        );

        update_ordered_list!(
            source_biota.properties_texture_map.as_ref(),
            &mut target_biota.biota_properties_texture_map,
            id,
            |value, existing_value| {
                existing_value.index = value.part_index;
                existing_value.old_id = value.old_texture;
                existing_value.new_id = value.new_texture;
            }
        );

        // Properties for all world objects that typically aren't modified over the original Biota

        // This is a cluster... because there is no key per record, just the record id.
        // That poses a problem because when we add a new record to be saved, we don't know what the record id is yet.
        // It's not until we try to save the record a second time that we will then have the database persisted record (with a valid id), and the entity record (that still has a DatabaseRecordId of 0)
        // We then need to match up the record that was saved with it's entity counterpart
        let mut used_target_create_list =
            vec![false; target_biota.biota_properties_create_list.len()];
        if let Some(list) = source_biota.properties_create_list_mut() {
            let targets = &mut target_biota.biota_properties_create_list;
            let mut processed_source = vec![false; list.len()];
            // Process matched up records first
            for (si, value) in list.iter().enumerate() {
                if value.database_record_id == 0 {
                    continue;
                }

                // Source record should already exist in the target
                // If the existingValue was not found, the database was likely modified outside of ACE after our last save
                let Some(ti) = targets
                    .iter()
                    .position(|r| r.id == value.database_record_id)
                else {
                    continue;
                };

                copy_create_list(value, &mut targets[ti]);

                processed_source[si] = true;
                used_target_create_list[ti] = true;
            }
            for (si, value) in list.iter_mut().enumerate() {
                if processed_source[si] {
                    continue;
                }

                // For simplicity, just find the first unused target
                let ti = match (0..targets.len()).find(|&t| !used_target_create_list[t]) {
                    Some(ti) => ti,
                    None => {
                        targets.push(db::BiotaPropertiesCreateList {
                            object_id: id,
                            ..Default::default()
                        });
                        used_target_create_list.push(false);
                        targets.len() - 1
                    }
                };

                value.database_record_id = targets[ti].id;

                copy_create_list(value, &mut targets[ti]);

                used_target_create_list[ti] = true;
            }
        }
        let remove: Vec<bool> = used_target_create_list.iter().map(|used| !used).collect();
        remove_marked(&mut target_biota.biota_properties_create_list, &remove);

        // Same cluster for emotes: emote_map pairs a source emote (index) with a target row (index).
        let mut emote_map: Vec<(usize, usize)> = Vec::new();
        if let Some(list) = source_biota.properties_emote_mut() {
            let targets = &mut target_biota.biota_properties_emote;
            // Process matched up records first
            for (si, value) in list.iter().enumerate() {
                if value.database_record_id == 0 {
                    continue;
                }

                // Source record should already exist in the target
                let Some(ti) = targets
                    .iter()
                    .position(|r| r.id == value.database_record_id)
                else {
                    continue;
                };

                copy_emote(value, &mut targets[ti]);

                // emoteMap[value] = existingValue (the same source is visited once)
                emote_map.push((si, ti));
            }
            for (si, value) in list.iter_mut().enumerate() {
                if emote_map.iter().any(|&(s, _)| s == si) {
                    continue;
                }

                // For simplicity, just find the first unused target
                let ti = match (0..targets.len()).find(|&t| !emote_map.iter().any(|&(_, u)| u == t))
                {
                    Some(ti) => ti,
                    None => {
                        targets.push(db::BiotaPropertiesEmote {
                            object_id: id,
                            ..Default::default()
                        });
                        targets.len() - 1
                    }
                };

                value.database_record_id = targets[ti].id;

                copy_emote(value, &mut targets[ti]);

                emote_map.push((si, ti));
            }

            // Now process the emote actions
            for &(si, ti) in &emote_map {
                let actions = &list[si].properties_emote_action;
                let target = &mut targets[ti];
                for (i, action) in actions.iter().enumerate() {
                    let order: u32 = from_index(i);
                    let ai = match target
                        .biota_properties_emote_action
                        .iter()
                        .position(|r| r.order == order)
                    {
                        Some(ai) => ai,
                        None => {
                            let emote_id = target.id;
                            target.biota_properties_emote_action.push(
                                db::BiotaPropertiesEmoteAction {
                                    emote_id,
                                    ..Default::default()
                                },
                            );
                            target.biota_properties_emote_action.len() - 1
                        }
                    };

                    copy_emote_action(action, &mut target.biota_properties_emote_action[ai], order);
                }
                let count = actions.len();
                let remove: Vec<bool> = target
                    .biota_properties_emote_action
                    .iter()
                    .map(|value| value.order as usize >= count)
                    .collect();
                remove_marked(&mut target.biota_properties_emote_action, &remove);
            }
        }
        let remove: Vec<bool> = (0..target_biota.biota_properties_emote.len())
            .map(|t| !emote_map.iter().any(|&(_, u)| u == t))
            .collect();
        remove_marked(&mut target_biota.biota_properties_emote, &remove);

        if let Some(set) = &source_biota.properties_event_filter {
            for value in set.iter() {
                if !target_biota
                    .biota_properties_event_filter
                    .iter()
                    .any(|r| r.event == *value)
                {
                    target_biota.biota_properties_event_filter.push(
                        db::BiotaPropertiesEventFilter {
                            object_id: id,
                            event: *value,
                        },
                    );
                }
            }
        }
        let remove: Vec<bool> = target_biota
            .biota_properties_event_filter
            .iter()
            .map(|value| {
                source_biota
                    .properties_event_filter
                    .as_ref()
                    .is_none_or(|s| !s.iter().any(|p| *p == value.event))
            })
            .collect();
        remove_marked(&mut target_biota.biota_properties_event_filter, &remove);

        // Same cluster for generators.
        let mut used_target_generators = vec![false; target_biota.biota_properties_generator.len()];
        if let Some(list) = source_biota.properties_generator_mut() {
            let targets = &mut target_biota.biota_properties_generator;
            let mut processed_source = vec![false; list.len()];
            // Process matched up records first
            for (si, value) in list.iter().enumerate() {
                if value.database_record_id == 0 {
                    continue;
                }

                // Source record should already exist in the target
                let Some(ti) = targets
                    .iter()
                    .position(|r| r.id == value.database_record_id)
                else {
                    continue;
                };

                copy_generator(value, &mut targets[ti]);

                processed_source[si] = true;
                used_target_generators[ti] = true;
            }
            for (si, value) in list.iter_mut().enumerate() {
                if processed_source[si] {
                    continue;
                }

                // For simplicity, just find the first unused target
                let ti = match (0..targets.len()).find(|&t| !used_target_generators[t]) {
                    Some(ti) => ti,
                    None => {
                        targets.push(db::BiotaPropertiesGenerator {
                            object_id: id,
                            ..Default::default()
                        });
                        used_target_generators.push(false);
                        targets.len() - 1
                    }
                };

                value.database_record_id = targets[ti].id;

                copy_generator(value, &mut targets[ti]);

                used_target_generators[ti] = true;
            }
        }
        let remove: Vec<bool> = used_target_generators.iter().map(|used| !used).collect();
        remove_marked(&mut target_biota.biota_properties_generator, &remove);

        // Properties for creatures

        if let Some(d) = &source_biota.properties_attribute {
            for (k, v) in d.iter() {
                let rows = &mut target_biota.biota_properties_attribute;
                let idx = rows
                    .iter()
                    .position(|r| r.r#type == k.0)
                    .unwrap_or_else(|| {
                        rows.push(db::BiotaPropertiesAttribute {
                            object_id: id,
                            ..Default::default()
                        });
                        rows.len() - 1
                    });
                let existing_value = &mut rows[idx];
                existing_value.r#type = k.0;
                existing_value.init_level = v.init_level;
                existing_value.level_from_cp = v.level_from_cp;
                existing_value.cp_spent = v.cp_spent;
            }
        }
        let remove: Vec<bool> = target_biota
            .biota_properties_attribute
            .iter()
            .map(|value| {
                source_biota
                    .properties_attribute
                    .as_ref()
                    .is_none_or(|d| !d.contains_key(&PropertyAttribute(value.r#type)))
            })
            .collect();
        remove_marked(&mut target_biota.biota_properties_attribute, &remove);

        if let Some(d) = &source_biota.properties_attribute_2nd {
            for (k, v) in d.iter() {
                let rows = &mut target_biota.biota_properties_attribute_2nd;
                let idx = rows
                    .iter()
                    .position(|r| r.r#type == k.0)
                    .unwrap_or_else(|| {
                        rows.push(db::BiotaPropertiesAttribute2nd {
                            object_id: id,
                            ..Default::default()
                        });
                        rows.len() - 1
                    });
                let existing_value = &mut rows[idx];
                existing_value.r#type = k.0;
                existing_value.init_level = v.init_level;
                existing_value.level_from_cp = v.level_from_cp;
                existing_value.cp_spent = v.cp_spent;
                existing_value.current_level = v.current_level;
            }
        }
        let remove: Vec<bool> = target_biota
            .biota_properties_attribute_2nd
            .iter()
            .map(|value| {
                source_biota
                    .properties_attribute_2nd
                    .as_ref()
                    .is_none_or(|d| !d.contains_key(&PropertyAttribute2nd(value.r#type)))
            })
            .collect();
        remove_marked(&mut target_biota.biota_properties_attribute_2nd, &remove);

        if let Some(d) = &source_biota.properties_body_part {
            for (k, v) in d.iter() {
                let rows = &mut target_biota.biota_properties_body_part;
                // C# compares the ushort Key with (uint)kvp.Key, both widened to long.
                let key_u32: u32 = k.0.cs_cast();
                let idx = rows
                    .iter()
                    .position(|r| u32::from(r.key) == key_u32)
                    .unwrap_or_else(|| {
                        rows.push(db::BiotaPropertiesBodyPart {
                            object_id: id,
                            ..Default::default()
                        });
                        rows.len() - 1
                    });
                let row_id = rows[idx].id;
                rows[idx] = body_part_row(id, *k, v, row_id);
                // An existing row keeps its object_Id (ACE never rewrites it).
            }
        }
        let remove: Vec<bool> = target_biota
            .biota_properties_body_part
            .iter()
            .map(|value| {
                source_biota
                    .properties_body_part
                    .as_ref()
                    .is_none_or(|d| !d.contains_key(&CombatBodyPart(i32::from(value.key))))
            })
            .collect();
        remove_marked(&mut target_biota.biota_properties_body_part, &remove);

        if let Some(d) = &source_biota.properties_skill {
            for (k, v) in d.iter() {
                let rows = &mut target_biota.biota_properties_skill;
                let key: u16 = k.0.cs_cast();
                let idx = rows
                    .iter()
                    .position(|r| r.r#type == key)
                    .unwrap_or_else(|| {
                        rows.push(db::BiotaPropertiesSkill {
                            object_id: id,
                            ..Default::default()
                        });
                        rows.len() - 1
                    });
                let existing_value = &mut rows[idx];
                existing_value.r#type = key;
                existing_value.level_from_pp = v.level_from_pp;
                existing_value.sac = v.sac.0;
                existing_value.pp = v.pp;
                existing_value.init_level = v.init_level;
                existing_value.resistance_at_last_check = v.resistance_at_last_check;
                existing_value.last_used_time = v.last_used_time;
            }
        }
        let remove: Vec<bool> = target_biota
            .biota_properties_skill
            .iter()
            .map(|value| {
                source_biota
                    .properties_skill
                    .as_ref()
                    .is_none_or(|d| !d.contains_key(&Skill(i32::from(value.r#type))))
            })
            .collect();
        remove_marked(&mut target_biota.biota_properties_skill, &remove);

        // Properties for books

        if let Some(book) = &source_biota.properties_book {
            let target =
                target_biota
                    .biota_properties_book
                    .get_or_insert_with(|| db::BiotaPropertiesBook {
                        object_id: id,
                        ..Default::default()
                    });

            target.max_num_pages = book.max_num_pages;
            target.max_num_chars_per_page = book.max_num_chars_per_page;
        } else if target_biota.biota_properties_book.is_some() {
            target_biota.biota_properties_book = None;
        }

        if let Some(list) = &source_biota.properties_book_page_data {
            for (i, value) in list.iter().enumerate() {
                let page: u32 = from_index(i);
                let rows = &mut target_biota.biota_properties_book_page_data;
                let idx = rows
                    .iter()
                    .position(|r| r.page_id == page)
                    .unwrap_or_else(|| {
                        rows.push(db::BiotaPropertiesBookPageData {
                            object_id: id,
                            ..Default::default()
                        });
                        rows.len() - 1
                    });
                let existing_value = &mut rows[idx];
                existing_value.page_id = page;
                existing_value.author_id = value.author_id;
                existing_value.author_name.clone_from(&value.author_name);
                existing_value
                    .author_account
                    .clone_from(&value.author_account);
                existing_value.ignore_author = value.ignore_author;
                existing_value.page_text.clone_from(&value.page_text);
            }
        }
        let count = source_biota
            .properties_book_page_data
            .as_ref()
            .map(Vec::len);
        let remove: Vec<bool> = target_biota
            .biota_properties_book_page_data
            .iter()
            .map(|value| count.is_none_or(|c| value.page_id as usize >= c))
            .collect();
        remove_marked(&mut target_biota.biota_properties_book_page_data, &remove);

        // Biota additions over Weenie

        if let Some(d) = &source_biota.properties_allegiance {
            for (k, v) in d.iter() {
                let rows = &mut target_biota.biota_properties_allegiance;
                let idx = rows
                    .iter()
                    .position(|r| r.character_id == *k)
                    .unwrap_or_else(|| {
                        rows.push(db::BiotaPropertiesAllegiance {
                            allegiance_id: id,
                            ..Default::default()
                        });
                        rows.len() - 1
                    });
                let existing_value = &mut rows[idx];
                existing_value.character_id = *k;
                existing_value.banned = v.banned;
                existing_value.approved_vassal = v.approved_vassal;
            }
        }
        let remove: Vec<bool> = target_biota
            .biota_properties_allegiance
            .iter()
            .map(|value| {
                source_biota
                    .properties_allegiance
                    .as_ref()
                    .is_none_or(|d| !d.contains_key(&value.character_id))
            })
            .collect();
        remove_marked(&mut target_biota.biota_properties_allegiance, &remove);

        if let Some(list) = &source_biota.properties_enchantment_registry {
            for value in list {
                let rows = &mut target_biota.biota_properties_enchantment_registry;
                let found = rows.iter().position(|r| {
                    r.spell_id == value.spell_id
                        && r.layer_id == value.layer_id
                        && r.caster_object_id == value.caster_object_id
                });
                match found {
                    // An existing row keeps its object_Id (ACE never rewrites it).
                    Some(idx) => {
                        let object_id = rows[idx].object_id;
                        rows[idx] = enchantment_row(object_id, value);
                    }
                    None => rows.push(enchantment_row(id, value)),
                }
            }
        }
        let remove: Vec<bool> = target_biota
            .biota_properties_enchantment_registry
            .iter()
            .map(|value| {
                source_biota
                    .properties_enchantment_registry
                    .as_ref()
                    .is_none_or(|l| {
                        !l.iter().any(|p| {
                            p.spell_id == value.spell_id
                                && p.layer_id == value.layer_id
                                && p.caster_object_id == value.caster_object_id
                        })
                    })
            })
            .collect();
        remove_marked(
            &mut target_biota.biota_properties_enchantment_registry,
            &remove,
        );

        if let Some(d) = &source_biota.house_permissions {
            for (k, v) in d.iter() {
                let rows = &mut target_biota.house_permission;
                let idx = rows
                    .iter()
                    .position(|r| r.player_guid == *k)
                    .unwrap_or_else(|| {
                        rows.push(db::HousePermission {
                            house_id: id,
                            ..Default::default()
                        });
                        rows.len() - 1
                    });
                rows[idx].player_guid = *k;
                rows[idx].storage = *v;
            }
        }
        let remove: Vec<bool> = target_biota
            .house_permission
            .iter()
            .map(|value| {
                source_biota
                    .house_permissions
                    .as_ref()
                    .is_none_or(|d| !d.contains_key(&value.player_guid))
            })
            .collect();
        remove_marked(&mut target_biota.house_permission, &remove);
    }
}

// ACE: BiotaUpdater.CopyValueInto
fn copy_create_list(
    value: &PropertiesCreateList,
    existing_value: &mut db::BiotaPropertiesCreateList,
) {
    existing_value.destination_type = value.destination_type.0.cs_cast();
    existing_value.weenie_class_id = value.weenie_class_id;
    existing_value.stack_size = value.stack_size;
    existing_value.palette = value.palette;
    existing_value.shade = value.shade;
    existing_value.try_to_bond = value.try_to_bond;
}

// ACE: BiotaUpdater.CopyValueInto
fn copy_emote(value: &PropertiesEmote, existing_value: &mut db::BiotaPropertiesEmote) {
    existing_value.category = value.category.0.cs_cast();
    existing_value.probability = value.probability;
    existing_value.weenie_class_id = value.weenie_class_id;
    existing_value.style = value.style.map(|s| s.0);
    existing_value.substyle = value.substyle.map(|s| s.0);
    existing_value.quest.clone_from(&value.quest);
    existing_value.vendor_type = value.vendor_type.map(|v| v.0);
    existing_value.min_health = value.min_health;
    existing_value.max_health = value.max_health;
}

// ACE: BiotaUpdater.CopyValueInto
fn copy_emote_action(
    value: &PropertiesEmoteAction,
    existing_value: &mut db::BiotaPropertiesEmoteAction,
    order: u32,
) {
    //existingValue.EmoteId = value.EmoteId;
    existing_value.order = order;
    existing_value.r#type = value.r#type;
    existing_value.delay = value.delay;
    existing_value.extent = value.extent;
    existing_value.motion = value.motion.map(|m| m.0);
    existing_value.message.clone_from(&value.message);
    existing_value.test_string.clone_from(&value.test_string);
    existing_value.min = value.min;
    existing_value.max = value.max;
    existing_value.min_64 = value.min_64;
    existing_value.max_64 = value.max_64;
    existing_value.min_dbl = value.min_dbl;
    existing_value.max_dbl = value.max_dbl;
    existing_value.stat = value.stat;
    existing_value.display = value.display;
    existing_value.amount = value.amount;
    existing_value.amount_64 = value.amount_64;
    existing_value.hero_xp_64 = value.hero_xp_64;
    existing_value.percent = value.percent;
    existing_value.spell_id = value.spell_id;
    existing_value.wealth_rating = value.wealth_rating;
    existing_value.treasure_class = value.treasure_class;
    existing_value.treasure_type = value.treasure_type;
    existing_value.p_script = value.p_script.map(|p| p.0.cs_cast());
    existing_value.sound = value.sound.map(|s| s.0.cs_cast());
    existing_value.destination_type = value.destination_type;
    existing_value.weenie_class_id = value.weenie_class_id;
    existing_value.stack_size = value.stack_size;
    existing_value.palette = value.palette;
    existing_value.shade = value.shade;
    existing_value.try_to_bond = value.try_to_bond;
    existing_value.obj_cell_id = value.obj_cell_id;
    existing_value.origin_x = value.origin_x;
    existing_value.origin_y = value.origin_y;
    existing_value.origin_z = value.origin_z;
    existing_value.angles_w = value.angles_w;
    existing_value.angles_x = value.angles_x;
    existing_value.angles_y = value.angles_y;
    existing_value.angles_z = value.angles_z;
}

// ACE: BiotaUpdater.CopyValueInto
fn copy_generator(value: &PropertiesGenerator, existing_value: &mut db::BiotaPropertiesGenerator) {
    existing_value.probability = value.probability;
    existing_value.weenie_class_id = value.weenie_class_id;
    existing_value.delay = value.delay;
    existing_value.init_create = value.init_create;
    existing_value.max_create = value.max_create;
    existing_value.when_create = value.when_create.0;
    existing_value.where_create = value.where_create.0;
    existing_value.stack_size = value.stack_size;
    existing_value.palette_id = value.palette_id;
    existing_value.shade = value.shade;
    existing_value.obj_cell_id = value.obj_cell_id;
    existing_value.origin_x = value.origin_x;
    existing_value.origin_y = value.origin_y;
    existing_value.origin_z = value.origin_z;
    existing_value.angles_w = value.angles_w;
    existing_value.angles_x = value.angles_x;
    existing_value.angles_y = value.angles_y;
    existing_value.angles_z = value.angles_z;
}
