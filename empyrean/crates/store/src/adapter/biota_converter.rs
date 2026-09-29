// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Adapter/BiotaConverter.cs
//! `BiotaConverter`: the database row model [`db::Biota`] to and from the entity model
//! [`empyrean_entity::Biota`].
//!
//! Enum casts are C#'s unchecked reinterpretations (`(PlayScript?)int`, `(ushort)SpellCategory`).
//! The five collections the entity model shares with its weenie are built as fresh `Arc`s.

use std::sync::Arc;

use empyrean_common::dotnet::{CsCast, DotNetDict, DotNetHashSet};
use empyrean_entity::enums::{
    CombatBodyPart, DamageType, DestinationType, EmoteCategory, EnchantmentTypeFlags, EquipmentSet,
    MotionCommand, MotionStance, PlayScript, PositionType, PropertyAttribute, PropertyAttribute2nd,
    PropertyBool, PropertyDataId, PropertyFloat, PropertyInstanceId, PropertyInt, PropertyInt64,
    PropertyString, RegenLocationType, RegenerationType, Skill, SkillAdvancementClass, Sound,
    SpellCategory, VendorType, WeenieType,
};
use empyrean_entity::models::{
    PropertiesAllegiance, PropertiesAnimPart, PropertiesAttribute, PropertiesAttribute2nd,
    PropertiesBodyPart, PropertiesBook, PropertiesBookPageData, PropertiesCreateList,
    PropertiesEmote, PropertiesEmoteAction, PropertiesEnchantmentRegistry, PropertiesGenerator,
    PropertiesPalette, PropertiesPosition, PropertiesSkill, PropertiesTextureMap,
};

use crate::cast::from_index;
use crate::models::shard as db;

// ACE: BiotaConverter
/// `BiotaConverter` (a static class in ACE).
#[derive(Debug)]
pub struct BiotaConverter;

/// `list.OrderBy(key)`: a stable sort of references (LINQ's `OrderBy` is stable).
fn order_by<T, K: Ord>(list: &[T], key: impl Fn(&T) -> K) -> Vec<&T> {
    let mut v: Vec<&T> = list.iter().collect();
    v.sort_by_key(|r| key(r));
    v
}

impl BiotaConverter {
    // ACE: BiotaConverter.ConvertToEntityBiota
    /// The entity model of a database biota. With `instantiate_empty_collections` false (ACE's
    /// default), an empty row list becomes a `None` collection.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn convert_to_entity_biota(
        biota: &db::Biota,
        instantiate_empty_collections: bool,
    ) -> empyrean_entity::Biota {
        let mut result = empyrean_entity::Biota {
            id: biota.id,
            weenie_class_id: biota.weenie_class_id,
            weenie_type: WeenieType(biota.weenie_type.cs_cast()),
            ..Default::default()
        };
        let take = |len: usize| instantiate_empty_collections || len > 0;

        if take(biota.biota_properties_bool.len()) {
            let mut d = DotNetDict::new();
            for value in &biota.biota_properties_bool {
                d.insert(PropertyBool(value.r#type), value.value);
            }
            result.properties_bool = Some(d);
        }
        if take(biota.biota_properties_did.len()) {
            let mut d = DotNetDict::new();
            for value in &biota.biota_properties_did {
                d.insert(PropertyDataId(value.r#type), value.value);
            }
            result.properties_did = Some(d);
        }
        if take(biota.biota_properties_float.len()) {
            let mut d = DotNetDict::new();
            for value in &biota.biota_properties_float {
                d.insert(PropertyFloat(value.r#type), value.value);
            }
            result.properties_float = Some(d);
        }
        if take(biota.biota_properties_iid.len()) {
            let mut d = DotNetDict::new();
            for value in &biota.biota_properties_iid {
                d.insert(PropertyInstanceId(value.r#type), value.value);
            }
            result.properties_iid = Some(d);
        }
        if take(biota.biota_properties_int.len()) {
            let mut d = DotNetDict::new();
            for value in &biota.biota_properties_int {
                d.insert(PropertyInt(value.r#type), value.value);
            }
            result.properties_int = Some(d);
        }
        if take(biota.biota_properties_int64.len()) {
            let mut d = DotNetDict::new();
            for value in &biota.biota_properties_int64 {
                d.insert(PropertyInt64(value.r#type), value.value);
            }
            result.properties_int64 = Some(d);
        }
        if take(biota.biota_properties_string.len()) {
            let mut d = DotNetDict::new();
            for value in &biota.biota_properties_string {
                d.insert(PropertyString(value.r#type), value.value.clone());
            }
            result.properties_string = Some(d);
        }

        if take(biota.biota_properties_position.len()) {
            let mut d = DotNetDict::new();
            for record in &biota.biota_properties_position {
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

        if take(biota.biota_properties_spell_book.len()) {
            let mut d = DotNetDict::new();
            for value in &biota.biota_properties_spell_book {
                d.insert(value.spell, value.probability);
            }
            result.properties_spell_book = Some(d);
        }

        if take(biota.biota_properties_anim_part.len()) {
            result.properties_anim_part = Some(
                order_by(&biota.biota_properties_anim_part, |r| r.order)
                    .into_iter()
                    .map(|record| PropertiesAnimPart {
                        index: record.index,
                        animation_id: record.animation_id,
                    })
                    .collect(),
            );
        }

        if take(biota.biota_properties_palette.len()) {
            result.properties_palette = Some(
                order_by(&biota.biota_properties_palette, |r| r.order)
                    .into_iter()
                    .map(|record| PropertiesPalette {
                        sub_palette_id: record.sub_palette_id,
                        offset: record.offset,
                        length: record.length,
                    })
                    .collect(),
            );
        }

        if take(biota.biota_properties_texture_map.len()) {
            result.properties_texture_map = Some(
                order_by(&biota.biota_properties_texture_map, |r| r.order)
                    .into_iter()
                    .map(|record| PropertiesTextureMap {
                        part_index: record.index,
                        old_texture: record.old_id,
                        new_texture: record.new_id,
                    })
                    .collect(),
            );
        }

        // Properties for all world objects that typically aren't modified over the original Biota

        if take(biota.biota_properties_create_list.len()) {
            let list = biota
                .biota_properties_create_list
                .iter()
                .map(|record| PropertiesCreateList {
                    database_record_id: record.id,
                    destination_type: DestinationType(i32::from(record.destination_type)),
                    weenie_class_id: record.weenie_class_id,
                    stack_size: record.stack_size,
                    palette: record.palette,
                    shade: record.shade,
                    try_to_bond: record.try_to_bond,
                })
                .collect();
            result.properties_create_list = Some(Arc::new(list));
        }

        if take(biota.biota_properties_emote.len()) {
            let mut list = Vec::new();
            for record in &biota.biota_properties_emote {
                let mut new_entity = PropertiesEmote {
                    database_record_id: record.id,
                    category: EmoteCategory(record.category.cs_cast()),
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

                for record2 in order_by(&record.biota_properties_emote_action, |r| r.order) {
                    new_entity
                        .properties_emote_action
                        .push(PropertiesEmoteAction {
                            database_record_id: record2.id,
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
                            p_script: record2.p_script.map(|v| PlayScript(v.cs_cast())),
                            sound: record2.sound.map(|v| Sound(v.cs_cast())),
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
                        });
                }

                list.push(new_entity);
            }
            result.properties_emote = Some(Arc::new(list));
        }

        if take(biota.biota_properties_event_filter.len()) {
            let mut set = DotNetHashSet::new();
            for value in &biota.biota_properties_event_filter {
                set.insert(value.event);
            }
            result.properties_event_filter = Some(Arc::new(set));
        }

        if take(biota.biota_properties_generator.len()) {
            // TODO do we have the correct order? (ACE's own note: the rows come back in id order.)
            let list = biota
                .biota_properties_generator
                .iter()
                .map(|record| PropertiesGenerator {
                    database_record_id: record.id,
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
                })
                .collect();
            result.properties_generator = Some(Arc::new(list));
        }

        // Properties for creatures

        if take(biota.biota_properties_attribute.len()) {
            let mut d = DotNetDict::new();
            for record in &biota.biota_properties_attribute {
                let new_entity = PropertiesAttribute {
                    init_level: record.init_level,
                    level_from_cp: record.level_from_cp,
                    cp_spent: record.cp_spent,
                };
                d.insert(PropertyAttribute(record.r#type), new_entity);
            }
            result.properties_attribute = Some(d);
        }

        if take(biota.biota_properties_attribute_2nd.len()) {
            let mut d = DotNetDict::new();
            for record in &biota.biota_properties_attribute_2nd {
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

        if take(biota.biota_properties_body_part.len()) {
            let mut d = DotNetDict::new();
            for record in &biota.biota_properties_body_part {
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

        if take(biota.biota_properties_skill.len()) {
            let mut d = DotNetDict::new();
            for record in &biota.biota_properties_skill {
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

        if let Some(book) = &biota.biota_properties_book {
            result.properties_book = Some(PropertiesBook {
                max_num_pages: book.max_num_pages,
                max_num_chars_per_page: book.max_num_chars_per_page,
            });
        }

        if take(biota.biota_properties_book_page_data.len()) {
            result.properties_book_page_data = Some(
                order_by(&biota.biota_properties_book_page_data, |r| r.page_id)
                    .into_iter()
                    .map(|record| PropertiesBookPageData {
                        author_id: record.author_id,
                        author_name: record.author_name.clone(),
                        author_account: record.author_account.clone(),
                        ignore_author: record.ignore_author,
                        page_text: record.page_text.clone(),
                    })
                    .collect(),
            );
        }

        // Biota additions over Weenie

        if take(biota.biota_properties_allegiance.len()) {
            let mut d = DotNetDict::new();
            for record in &biota.biota_properties_allegiance {
                d.insert(
                    record.character_id,
                    PropertiesAllegiance {
                        banned: record.banned,
                        approved_vassal: record.approved_vassal,
                    },
                );
            }
            result.properties_allegiance = Some(d);
        }

        if take(biota.biota_properties_enchantment_registry.len()) {
            result.properties_enchantment_registry = Some(
                biota
                    .biota_properties_enchantment_registry
                    .iter()
                    .map(|record| PropertiesEnchantmentRegistry {
                        enchantment_category: record.enchantment_category,
                        spell_id: record.spell_id,
                        layer_id: record.layer_id,
                        has_spell_set_id: record.has_spell_set_id,
                        spell_category: SpellCategory(u32::from(record.spell_category)),
                        power_level: record.power_level,
                        start_time: record.start_time,
                        duration: record.duration,
                        caster_object_id: record.caster_object_id,
                        degrade_modifier: record.degrade_modifier,
                        degrade_limit: record.degrade_limit,
                        last_time_degraded: record.last_time_degraded,
                        stat_mod_type: EnchantmentTypeFlags(record.stat_mod_type.cs_cast()),
                        stat_mod_key: record.stat_mod_key,
                        stat_mod_value: record.stat_mod_value,
                        spell_set_id: EquipmentSet(record.spell_set_id.cs_cast()),
                    })
                    .collect(),
            );
        }

        if take(biota.house_permission.len()) {
            let mut d = DotNetDict::new();
            for record in &biota.house_permission {
                d.insert(record.player_guid, record.storage);
            }
            result.house_permissions = Some(d);
        }

        result
    }

    // ACE: BiotaConverter.ConvertFromEntityBiota
    /// The database row model of an entity biota. With `include_database_record_ids`, the rows that
    /// have a surrogate id take the entity records' `database_record_id`.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn convert_from_entity_biota(
        biota: &empyrean_entity::Biota,
        include_database_record_ids: bool,
    ) -> db::Biota {
        let mut result = db::Biota {
            id: biota.id,
            weenie_class_id: biota.weenie_class_id,
            weenie_type: biota.weenie_type.0.cs_cast(),
            ..Default::default()
        };

        if let Some(d) = &biota.properties_bool {
            for (k, v) in d.iter() {
                result.set_property_bool(*k, *v);
            }
        }
        if let Some(d) = &biota.properties_did {
            for (k, v) in d.iter() {
                result.set_property_did(*k, *v);
            }
        }
        if let Some(d) = &biota.properties_float {
            for (k, v) in d.iter() {
                result.set_property_float(*k, *v);
            }
        }
        if let Some(d) = &biota.properties_iid {
            for (k, v) in d.iter() {
                result.set_property_iid(*k, *v);
            }
        }
        if let Some(d) = &biota.properties_int {
            for (k, v) in d.iter() {
                result.set_property_int(*k, *v);
            }
        }
        if let Some(d) = &biota.properties_int64 {
            for (k, v) in d.iter() {
                result.set_property_int64(*k, *v);
            }
        }
        if let Some(d) = &biota.properties_string {
            for (k, v) in d.iter() {
                result.set_property_string(*k, v);
            }
        }

        if let Some(d) = &biota.properties_position {
            for (k, v) in d.iter() {
                let mut entity = db::BiotaPropertiesPosition {
                    object_id: biota.id,
                    position_type: k.0,
                    obj_cell_id: v.obj_cell_id,
                    origin_x: v.position_x,
                    origin_y: v.position_y,
                    origin_z: v.position_z,
                    angles_w: v.rotation_w,
                    angles_x: v.rotation_x,
                    angles_y: v.rotation_y,
                    angles_z: v.rotation_z,
                };

                // Entity Framework is unable to store NaN floats in the database and results in an error of:
                // ERROR 1054: Unknown column 'NaN' in 'field list'
                if entity.angles_x.is_nan()
                    || entity.angles_y.is_nan()
                    || entity.angles_z.is_nan()
                    || entity.angles_w.is_nan()
                {
                    entity.angles_w = 1.0;
                    entity.angles_x = 0.0;
                    entity.angles_y = 0.0;
                    entity.angles_z = 0.0;
                }

                result.biota_properties_position.push(entity);
            }
        }

        if let Some(d) = &biota.properties_spell_book {
            for (k, v) in d.iter() {
                result
                    .biota_properties_spell_book
                    .push(db::BiotaPropertiesSpellBook {
                        object_id: biota.id,
                        spell: *k,
                        probability: *v,
                    });
            }
        }

        if let Some(list) = &biota.properties_anim_part {
            for (i, value) in list.iter().enumerate() {
                result
                    .biota_properties_anim_part
                    .push(db::BiotaPropertiesAnimPart {
                        object_id: biota.id,
                        index: value.index,
                        animation_id: value.animation_id,
                        order: Some(from_index(i)),
                        ..Default::default()
                    });
            }
        }

        if let Some(list) = &biota.properties_palette {
            for (i, value) in list.iter().enumerate() {
                result
                    .biota_properties_palette
                    .push(db::BiotaPropertiesPalette {
                        object_id: biota.id,
                        sub_palette_id: value.sub_palette_id,
                        offset: value.offset,
                        length: value.length,
                        order: Some(from_index(i)),
                        ..Default::default()
                    });
            }
        }

        if let Some(list) = &biota.properties_texture_map {
            for (i, value) in list.iter().enumerate() {
                result
                    .biota_properties_texture_map
                    .push(db::BiotaPropertiesTextureMap {
                        object_id: biota.id,
                        index: value.part_index,
                        old_id: value.old_texture,
                        new_id: value.new_texture,
                        order: Some(from_index(i)),
                        ..Default::default()
                    });
            }
        }

        // Properties for all world objects that typically aren't modified over the original weenie

        if let Some(list) = &biota.properties_create_list {
            for value in list.iter() {
                let mut entity = db::BiotaPropertiesCreateList {
                    object_id: biota.id,
                    destination_type: value.destination_type.0.cs_cast(),
                    weenie_class_id: value.weenie_class_id,
                    stack_size: value.stack_size,
                    palette: value.palette,
                    shade: value.shade,
                    try_to_bond: value.try_to_bond,
                    ..Default::default()
                };

                if include_database_record_ids {
                    entity.id = value.database_record_id;
                }

                result.biota_properties_create_list.push(entity);
            }
        }

        if let Some(list) = &biota.properties_emote {
            for value in list.iter() {
                let mut entity = db::BiotaPropertiesEmote {
                    object_id: biota.id,
                    category: value.category.0.cs_cast(),
                    probability: value.probability,
                    weenie_class_id: value.weenie_class_id,
                    style: value.style.map(|s| s.0),
                    substyle: value.substyle.map(|s| s.0),
                    quest: value.quest.clone(),
                    vendor_type: value.vendor_type.map(|v| v.0),
                    min_health: value.min_health,
                    max_health: value.max_health,
                    ..Default::default()
                };

                if include_database_record_ids {
                    entity.id = value.database_record_id;
                }

                // IndexOf compares the (reference-type) actions by identity, so it is the position.
                for (index, value2) in value.properties_emote_action.iter().enumerate() {
                    let mut entity2 = db::BiotaPropertiesEmoteAction {
                        // EmoteId is a foreign key to Emote.Id. ACE sets uint.MaxValue so that Entity
                        // Framework must replace it with the parent's id when the biota is added.
                        emote_id: u32::MAX,

                        order: from_index(index),
                        r#type: value2.r#type,
                        delay: value2.delay,
                        extent: value2.extent,
                        motion: value2.motion.map(|m| m.0),
                        message: value2.message.clone(),
                        test_string: value2.test_string.clone(),
                        min: value2.min,
                        max: value2.max,
                        min_64: value2.min_64,
                        max_64: value2.max_64,
                        min_dbl: value2.min_dbl,
                        max_dbl: value2.max_dbl,
                        stat: value2.stat,
                        display: value2.display,
                        amount: value2.amount,
                        amount_64: value2.amount_64,
                        hero_xp_64: value2.hero_xp_64,
                        percent: value2.percent,
                        spell_id: value2.spell_id,
                        wealth_rating: value2.wealth_rating,
                        treasure_class: value2.treasure_class,
                        treasure_type: value2.treasure_type,
                        p_script: value2.p_script.map(|p| p.0.cs_cast()),
                        sound: value2.sound.map(|s| s.0.cs_cast()),
                        destination_type: value2.destination_type,
                        weenie_class_id: value2.weenie_class_id,
                        stack_size: value2.stack_size,
                        palette: value2.palette,
                        shade: value2.shade,
                        try_to_bond: value2.try_to_bond,
                        obj_cell_id: value2.obj_cell_id,
                        origin_x: value2.origin_x,
                        origin_y: value2.origin_y,
                        origin_z: value2.origin_z,
                        angles_w: value2.angles_w,
                        angles_x: value2.angles_x,
                        angles_y: value2.angles_y,
                        angles_z: value2.angles_z,
                        ..Default::default()
                    };

                    if include_database_record_ids {
                        entity2.id = value2.database_record_id;
                    }

                    entity.biota_properties_emote_action.push(entity2);
                }

                result.biota_properties_emote.push(entity);
            }
        }

        if let Some(set) = &biota.properties_event_filter {
            for value in set.iter() {
                result
                    .biota_properties_event_filter
                    .push(db::BiotaPropertiesEventFilter {
                        object_id: biota.id,
                        event: *value,
                    });
            }
        }

        if let Some(list) = &biota.properties_generator {
            for value in list.iter() {
                let mut entity = db::BiotaPropertiesGenerator {
                    object_id: biota.id,
                    probability: value.probability,
                    weenie_class_id: value.weenie_class_id,
                    delay: value.delay,
                    init_create: value.init_create,
                    max_create: value.max_create,
                    when_create: value.when_create.0,
                    where_create: value.where_create.0,
                    stack_size: value.stack_size,
                    palette_id: value.palette_id,
                    shade: value.shade,
                    obj_cell_id: value.obj_cell_id,
                    origin_x: value.origin_x,
                    origin_y: value.origin_y,
                    origin_z: value.origin_z,
                    angles_w: value.angles_w,
                    angles_x: value.angles_x,
                    angles_y: value.angles_y,
                    angles_z: value.angles_z,
                    ..Default::default()
                };

                if include_database_record_ids {
                    entity.id = value.database_record_id;
                }

                result.biota_properties_generator.push(entity);
            }
        }

        // Properties for creatures

        if let Some(d) = &biota.properties_attribute {
            for (k, v) in d.iter() {
                result
                    .biota_properties_attribute
                    .push(db::BiotaPropertiesAttribute {
                        object_id: biota.id,
                        r#type: k.0,
                        init_level: v.init_level,
                        level_from_cp: v.level_from_cp,
                        cp_spent: v.cp_spent,
                    });
            }
        }

        if let Some(d) = &biota.properties_attribute_2nd {
            for (k, v) in d.iter() {
                result
                    .biota_properties_attribute_2nd
                    .push(db::BiotaPropertiesAttribute2nd {
                        object_id: biota.id,
                        r#type: k.0,
                        init_level: v.init_level,
                        level_from_cp: v.level_from_cp,
                        cp_spent: v.cp_spent,
                        current_level: v.current_level,
                    });
            }
        }

        if let Some(d) = &biota.properties_body_part {
            for (k, v) in d.iter() {
                result
                    .biota_properties_body_part
                    .push(body_part_row(biota.id, *k, v, 0));
            }
        }

        if let Some(d) = &biota.properties_skill {
            for (k, v) in d.iter() {
                result
                    .biota_properties_skill
                    .push(db::BiotaPropertiesSkill {
                        object_id: biota.id,
                        r#type: k.0.cs_cast(),
                        level_from_pp: v.level_from_pp,
                        sac: v.sac.0,
                        pp: v.pp,
                        init_level: v.init_level,
                        resistance_at_last_check: v.resistance_at_last_check,
                        last_used_time: v.last_used_time,
                    });
            }
        }

        // Properties for books

        if let Some(book) = &biota.properties_book {
            result.biota_properties_book = Some(db::BiotaPropertiesBook {
                object_id: biota.id,
                max_num_pages: book.max_num_pages,
                max_num_chars_per_page: book.max_num_chars_per_page,
            });
        }

        if let Some(list) = &biota.properties_book_page_data {
            // IndexOf compares the (reference-type) pages by identity, so it is the position.
            for (page_id, value) in list.iter().enumerate() {
                result
                    .biota_properties_book_page_data
                    .push(db::BiotaPropertiesBookPageData {
                        object_id: biota.id,
                        page_id: from_index(page_id),
                        author_id: value.author_id,
                        author_name: value.author_name.clone(),
                        author_account: value.author_account.clone(),
                        ignore_author: value.ignore_author,
                        page_text: value.page_text.clone(),
                        ..Default::default()
                    });
            }
        }

        // Biota additions over Weenie

        if let Some(d) = &biota.properties_allegiance {
            for (k, v) in d.iter() {
                result
                    .biota_properties_allegiance
                    .push(db::BiotaPropertiesAllegiance {
                        allegiance_id: biota.id,
                        character_id: *k,
                        banned: v.banned,
                        approved_vassal: v.approved_vassal,
                    });
            }
        }

        if let Some(list) = &biota.properties_enchantment_registry {
            for value in list {
                result
                    .biota_properties_enchantment_registry
                    .push(enchantment_row(biota.id, value));
            }
        }

        if let Some(d) = &biota.house_permissions {
            for (k, v) in d.iter() {
                result.house_permission.push(db::HousePermission {
                    house_id: biota.id,
                    player_guid: *k,
                    storage: *v,
                });
            }
        }

        result
    }
}

/// A body part row from the entity record (shared with `BiotaUpdater`).
pub(crate) fn body_part_row(
    object_id: u32,
    key: CombatBodyPart,
    v: &PropertiesBodyPart,
    id: u32,
) -> db::BiotaPropertiesBodyPart {
    db::BiotaPropertiesBodyPart {
        id,
        object_id,
        key: key.0.cs_cast(),
        d_type: v.d_type.0,
        d_val: v.d_val,
        d_var: v.d_var,
        base_armor: v.base_armor,
        armor_vs_slash: v.armor_vs_slash,
        armor_vs_pierce: v.armor_vs_pierce,
        armor_vs_bludgeon: v.armor_vs_bludgeon,
        armor_vs_cold: v.armor_vs_cold,
        armor_vs_fire: v.armor_vs_fire,
        armor_vs_acid: v.armor_vs_acid,
        armor_vs_electric: v.armor_vs_electric,
        armor_vs_nether: v.armor_vs_nether,
        bh: v.bh,
        hlf: v.hlf,
        mlf: v.mlf,
        llf: v.llf,
        hrf: v.hrf,
        mrf: v.mrf,
        lrf: v.lrf,
        hlb: v.hlb,
        mlb: v.mlb,
        llb: v.llb,
        hrb: v.hrb,
        mrb: v.mrb,
        lrb: v.lrb,
    }
}

/// An enchantment row from the entity record (shared with `BiotaUpdater`).
pub(crate) fn enchantment_row(
    object_id: u32,
    value: &PropertiesEnchantmentRegistry,
) -> db::BiotaPropertiesEnchantmentRegistry {
    db::BiotaPropertiesEnchantmentRegistry {
        object_id,
        enchantment_category: value.enchantment_category,
        spell_id: value.spell_id,
        layer_id: value.layer_id,
        has_spell_set_id: value.has_spell_set_id,
        spell_category: value.spell_category.0.cs_cast(),
        power_level: value.power_level,
        start_time: value.start_time,
        duration: value.duration,
        caster_object_id: value.caster_object_id,
        degrade_modifier: value.degrade_modifier,
        degrade_limit: value.degrade_limit,
        last_time_degraded: value.last_time_degraded,
        stat_mod_type: value.stat_mod_type.0.cs_cast(),
        stat_mod_key: value.stat_mod_key,
        stat_mod_value: value.stat_mod_value,
        spell_set_id: value.spell_set_id.0.cs_cast(),
    }
}
