// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/Weenie.cs
//! `Weenie`: one row of the world-database table `weenie`.

use empyrean_common::dotnet::DotNetDateTime;

use super::{
    WeeniePropertiesAnimPart, WeeniePropertiesAttribute, WeeniePropertiesAttribute2nd,
    WeeniePropertiesBodyPart, WeeniePropertiesBook, WeeniePropertiesBookPageData,
    WeeniePropertiesBool, WeeniePropertiesCreateList, WeeniePropertiesDID, WeeniePropertiesEmote,
    WeeniePropertiesEventFilter, WeeniePropertiesFloat, WeeniePropertiesGenerator,
    WeeniePropertiesIID, WeeniePropertiesInt, WeeniePropertiesInt64, WeeniePropertiesPalette,
    WeeniePropertiesPosition, WeeniePropertiesSkill, WeeniePropertiesSpellBook,
    WeeniePropertiesString, WeeniePropertiesTextureMap,
};

/// Weenies
// ACE: Weenie
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Weenie {
    /// Weenie Class Id (wcid) / (WCID) / (weenieClassId)
    pub class_id: u32,
    /// Weenie Class Name (W_????_CLASS)
    pub class_name: String,
    /// WeenieType
    pub r#type: i32,
    pub last_modified: DotNetDateTime,
    pub weenie_properties_anim_part: Vec<WeeniePropertiesAnimPart>,
    pub weenie_properties_attribute: Vec<WeeniePropertiesAttribute>,
    pub weenie_properties_attribute_2nd: Vec<WeeniePropertiesAttribute2nd>,
    pub weenie_properties_body_part: Vec<WeeniePropertiesBodyPart>,
    pub weenie_properties_book: Option<WeeniePropertiesBook>,
    pub weenie_properties_book_page_data: Vec<WeeniePropertiesBookPageData>,
    pub weenie_properties_bool: Vec<WeeniePropertiesBool>,
    pub weenie_properties_create_list: Vec<WeeniePropertiesCreateList>,
    pub weenie_properties_did: Vec<WeeniePropertiesDID>,
    pub weenie_properties_emote: Vec<WeeniePropertiesEmote>,
    pub weenie_properties_event_filter: Vec<WeeniePropertiesEventFilter>,
    pub weenie_properties_float: Vec<WeeniePropertiesFloat>,
    pub weenie_properties_generator: Vec<WeeniePropertiesGenerator>,
    pub weenie_properties_iid: Vec<WeeniePropertiesIID>,
    pub weenie_properties_int: Vec<WeeniePropertiesInt>,
    pub weenie_properties_int64: Vec<WeeniePropertiesInt64>,
    pub weenie_properties_palette: Vec<WeeniePropertiesPalette>,
    pub weenie_properties_position: Vec<WeeniePropertiesPosition>,
    pub weenie_properties_skill: Vec<WeeniePropertiesSkill>,
    pub weenie_properties_spell_book: Vec<WeeniePropertiesSpellBook>,
    pub weenie_properties_string: Vec<WeeniePropertiesString>,
    pub weenie_properties_texture_map: Vec<WeeniePropertiesTextureMap>,
}
