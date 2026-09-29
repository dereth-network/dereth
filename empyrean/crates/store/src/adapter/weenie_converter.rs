// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Adapter/WeenieConverter.cs
//! `WeenieConverter.ConvertToDatabaseBiota`: a shard row model [`db::Biota`] built straight from
//! a weenie. (The file's other members, world model to and from entity model, are empyrean-content's.)
//!
//! DIVERGE (arch): ACE's input is the world-database row model (`Models.World.Weenie`), which lives
//! in empyrean-content; here it is the entity [`empyrean_entity::Weenie`] that empyrean-content builds from
//! those rows in the same order. The world rows' `Order` (emote actions) and `PageId` (book pages)
//! are the entity lists' positions, as `ConvertToEntityWeenie` orders them. As in ACE, anim parts,
//! palettes and texture maps get no `order` (null), and positions are copied without the NaN check
//! `BiotaConverter` applies. ACE has no caller of this method.

use empyrean_common::dotnet::CsCast;

use crate::adapter::biota_converter::body_part_row;
use crate::cast::from_index;
use crate::models::shard as db;

// ACE: WeenieConverter
/// ACE's `WeenieConverter` (the member that produces a shard biota).
#[derive(Debug)]
pub struct WeenieConverter;

impl WeenieConverter {
    // ACE: WeenieConverter.ConvertToDatabaseBiota
    /// The shard rows of a new biota `id` made from `weenie`.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn convert_to_database_biota(weenie: &empyrean_entity::Weenie, id: u32) -> db::Biota {
        let mut result = db::Biota {
            id,
            weenie_class_id: weenie.weenie_class_id,
            weenie_type: weenie.weenie_type.0.cs_cast(),
            ..Default::default()
        };

        for (k, v) in weenie.properties_bool.iter().flat_map(|d| d.iter()) {
            result.biota_properties_bool.push(db::BiotaPropertiesBool {
                object_id: result.id,
                r#type: k.0,
                value: *v,
            });
        }
        for (k, v) in weenie.properties_did.iter().flat_map(|d| d.iter()) {
            result.biota_properties_did.push(db::BiotaPropertiesDID {
                object_id: result.id,
                r#type: k.0,
                value: *v,
            });
        }
        for (k, v) in weenie.properties_float.iter().flat_map(|d| d.iter()) {
            result
                .biota_properties_float
                .push(db::BiotaPropertiesFloat {
                    object_id: result.id,
                    r#type: k.0,
                    value: *v,
                });
        }
        for (k, v) in weenie.properties_iid.iter().flat_map(|d| d.iter()) {
            result.biota_properties_iid.push(db::BiotaPropertiesIID {
                object_id: result.id,
                r#type: k.0,
                value: *v,
            });
        }
        for (k, v) in weenie.properties_int.iter().flat_map(|d| d.iter()) {
            result.biota_properties_int.push(db::BiotaPropertiesInt {
                object_id: result.id,
                r#type: k.0,
                value: *v,
            });
        }
        for (k, v) in weenie.properties_int64.iter().flat_map(|d| d.iter()) {
            result
                .biota_properties_int64
                .push(db::BiotaPropertiesInt64 {
                    object_id: result.id,
                    r#type: k.0,
                    value: *v,
                });
        }
        for (k, v) in weenie.properties_string.iter().flat_map(|d| d.iter()) {
            result
                .biota_properties_string
                .push(db::BiotaPropertiesString {
                    object_id: result.id,
                    r#type: k.0,
                    value: v.clone(),
                });
        }

        for (k, v) in weenie.properties_position.iter().flat_map(|d| d.iter()) {
            result
                .biota_properties_position
                .push(db::BiotaPropertiesPosition {
                    object_id: result.id,
                    position_type: k.0,
                    obj_cell_id: v.obj_cell_id,
                    origin_x: v.position_x,
                    origin_y: v.position_y,
                    origin_z: v.position_z,
                    angles_w: v.rotation_w,
                    angles_x: v.rotation_x,
                    angles_y: v.rotation_y,
                    angles_z: v.rotation_z,
                });
        }

        for (k, v) in weenie.properties_spell_book.iter().flat_map(|d| d.iter()) {
            result
                .biota_properties_spell_book
                .push(db::BiotaPropertiesSpellBook {
                    object_id: result.id,
                    spell: *k,
                    probability: *v,
                });
        }

        for value in weenie.properties_anim_part.iter().flatten() {
            result
                .biota_properties_anim_part
                .push(db::BiotaPropertiesAnimPart {
                    object_id: result.id,
                    index: value.index,
                    animation_id: value.animation_id,
                    ..Default::default()
                });
        }
        for value in weenie.properties_palette.iter().flatten() {
            result
                .biota_properties_palette
                .push(db::BiotaPropertiesPalette {
                    object_id: result.id,
                    sub_palette_id: value.sub_palette_id,
                    offset: value.offset,
                    length: value.length,
                    ..Default::default()
                });
        }
        for value in weenie.properties_texture_map.iter().flatten() {
            result
                .biota_properties_texture_map
                .push(db::BiotaPropertiesTextureMap {
                    object_id: result.id,
                    index: value.part_index,
                    old_id: value.old_texture,
                    new_id: value.new_texture,
                    ..Default::default()
                });
        }

        // Properties for all world objects that typically aren't modified over the original weenie

        for value in weenie.properties_create_list.iter().flat_map(|l| l.iter()) {
            result
                .biota_properties_create_list
                .push(db::BiotaPropertiesCreateList {
                    object_id: result.id,
                    destination_type: value.destination_type.0.cs_cast(),
                    weenie_class_id: value.weenie_class_id,
                    stack_size: value.stack_size,
                    palette: value.palette,
                    shade: value.shade,
                    try_to_bond: value.try_to_bond,
                    ..Default::default()
                });
        }

        for value in weenie.properties_emote.iter().flat_map(|l| l.iter()) {
            let mut emote = db::BiotaPropertiesEmote {
                object_id: result.id,
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

            for (order, value2) in value.properties_emote_action.iter().enumerate() {
                emote
                    .biota_properties_emote_action
                    .push(db::BiotaPropertiesEmoteAction {
                        // EmoteId is a foreign key to Emote.Id; uint.MaxValue forces Entity Framework
                        // to set it from the parent when the biota is added (Mag-nus 2018-08-04).
                        emote_id: u32::MAX,

                        order: from_index(order),
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
                    });
            }

            result.biota_properties_emote.push(emote);
        }

        for value in weenie.properties_event_filter.iter().flat_map(|s| s.iter()) {
            result
                .biota_properties_event_filter
                .push(db::BiotaPropertiesEventFilter {
                    object_id: result.id,
                    event: *value,
                });
        }

        for value in weenie.properties_generator.iter().flat_map(|l| l.iter()) {
            result
                .biota_properties_generator
                .push(db::BiotaPropertiesGenerator {
                    object_id: result.id,
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
                });
        }

        // Properties for creatures

        for (k, v) in weenie.properties_attribute.iter().flat_map(|d| d.iter()) {
            result
                .biota_properties_attribute
                .push(db::BiotaPropertiesAttribute {
                    object_id: result.id,
                    r#type: k.0,
                    init_level: v.init_level,
                    level_from_cp: v.level_from_cp,
                    cp_spent: v.cp_spent,
                });
        }
        for (k, v) in weenie
            .properties_attribute_2nd
            .iter()
            .flat_map(|d| d.iter())
        {
            result
                .biota_properties_attribute_2nd
                .push(db::BiotaPropertiesAttribute2nd {
                    object_id: result.id,
                    r#type: k.0,
                    init_level: v.init_level,
                    level_from_cp: v.level_from_cp,
                    cp_spent: v.cp_spent,
                    current_level: v.current_level,
                });
        }
        for (k, v) in weenie.properties_body_part.iter().flat_map(|d| d.iter()) {
            result
                .biota_properties_body_part
                .push(body_part_row(result.id, *k, v, 0));
        }
        for (k, v) in weenie.properties_skill.iter().flat_map(|d| d.iter()) {
            result
                .biota_properties_skill
                .push(db::BiotaPropertiesSkill {
                    object_id: result.id,
                    r#type: k.0.cs_cast(),
                    level_from_pp: v.level_from_pp,
                    sac: v.sac.0,
                    pp: v.pp,
                    init_level: v.init_level,
                    resistance_at_last_check: v.resistance_at_last_check,
                    last_used_time: v.last_used_time,
                });
        }

        // Properties for books

        if let Some(book) = &weenie.properties_book {
            result.biota_properties_book = Some(db::BiotaPropertiesBook {
                object_id: result.id,
                max_num_pages: book.max_num_pages,
                max_num_chars_per_page: book.max_num_chars_per_page,
            });
        }

        for (page_id, value) in weenie
            .properties_book_page_data
            .iter()
            .flatten()
            .enumerate()
        {
            result
                .biota_properties_book_page_data
                .push(db::BiotaPropertiesBookPageData {
                    object_id: result.id,
                    page_id: from_index(page_id),
                    author_id: value.author_id,
                    author_name: value.author_name.clone(),
                    author_account: value.author_account.clone(),
                    ignore_author: value.ignore_author,
                    page_text: value.page_text.clone(),
                    ..Default::default()
                });
        }

        result
    }
}
