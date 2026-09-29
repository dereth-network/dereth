// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/Biota.cs
//! `Biota`: a row of the `shard` database (Entity Framework model).

use super::BiotaPropertiesAllegiance;
use super::BiotaPropertiesAnimPart;
use super::BiotaPropertiesAttribute;
use super::BiotaPropertiesAttribute2nd;
use super::BiotaPropertiesBodyPart;
use super::BiotaPropertiesBook;
use super::BiotaPropertiesBookPageData;
use super::BiotaPropertiesBool;
use super::BiotaPropertiesCreateList;
use super::BiotaPropertiesDID;
use super::BiotaPropertiesEmote;
use super::BiotaPropertiesEnchantmentRegistry;
use super::BiotaPropertiesEventFilter;
use super::BiotaPropertiesFloat;
use super::BiotaPropertiesGenerator;
use super::BiotaPropertiesIID;
use super::BiotaPropertiesInt;
use super::BiotaPropertiesInt64;
use super::BiotaPropertiesPalette;
use super::BiotaPropertiesPosition;
use super::BiotaPropertiesSkill;
use super::BiotaPropertiesSpellBook;
use super::BiotaPropertiesString;
use super::BiotaPropertiesTextureMap;
use super::HousePermission;

// ACE: Biota
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Biota {
    // ACE: Biota.Id
    pub id: u32,
    // ACE: Biota.WeenieClassId
    pub weenie_class_id: u32,
    // ACE: Biota.WeenieType
    pub weenie_type: i32,
    // ACE: Biota.PopulatedCollectionFlags
    pub populated_collection_flags: u32,
    // ACE: Biota.BiotaPropertiesAllegiance
    pub biota_properties_allegiance: Vec<BiotaPropertiesAllegiance>,
    // ACE: Biota.BiotaPropertiesAnimPart
    pub biota_properties_anim_part: Vec<BiotaPropertiesAnimPart>,
    // ACE: Biota.BiotaPropertiesAttribute
    pub biota_properties_attribute: Vec<BiotaPropertiesAttribute>,
    // ACE: Biota.BiotaPropertiesAttribute2nd
    pub biota_properties_attribute_2nd: Vec<BiotaPropertiesAttribute2nd>,
    // ACE: Biota.BiotaPropertiesBodyPart
    pub biota_properties_body_part: Vec<BiotaPropertiesBodyPart>,
    // ACE: Biota.BiotaPropertiesBook
    pub biota_properties_book: Option<BiotaPropertiesBook>,
    // ACE: Biota.BiotaPropertiesBookPageData
    pub biota_properties_book_page_data: Vec<BiotaPropertiesBookPageData>,
    // ACE: Biota.BiotaPropertiesBool
    pub biota_properties_bool: Vec<BiotaPropertiesBool>,
    // ACE: Biota.BiotaPropertiesCreateList
    pub biota_properties_create_list: Vec<BiotaPropertiesCreateList>,
    // ACE: Biota.BiotaPropertiesDID
    pub biota_properties_did: Vec<BiotaPropertiesDID>,
    // ACE: Biota.BiotaPropertiesEmote
    pub biota_properties_emote: Vec<BiotaPropertiesEmote>,
    // ACE: Biota.BiotaPropertiesEnchantmentRegistry
    pub biota_properties_enchantment_registry: Vec<BiotaPropertiesEnchantmentRegistry>,
    // ACE: Biota.BiotaPropertiesEventFilter
    pub biota_properties_event_filter: Vec<BiotaPropertiesEventFilter>,
    // ACE: Biota.BiotaPropertiesFloat
    pub biota_properties_float: Vec<BiotaPropertiesFloat>,
    // ACE: Biota.BiotaPropertiesGenerator
    pub biota_properties_generator: Vec<BiotaPropertiesGenerator>,
    // ACE: Biota.BiotaPropertiesIID
    pub biota_properties_iid: Vec<BiotaPropertiesIID>,
    // ACE: Biota.BiotaPropertiesInt
    pub biota_properties_int: Vec<BiotaPropertiesInt>,
    // ACE: Biota.BiotaPropertiesInt64
    pub biota_properties_int64: Vec<BiotaPropertiesInt64>,
    // ACE: Biota.BiotaPropertiesPalette
    pub biota_properties_palette: Vec<BiotaPropertiesPalette>,
    // ACE: Biota.BiotaPropertiesPosition
    pub biota_properties_position: Vec<BiotaPropertiesPosition>,
    // ACE: Biota.BiotaPropertiesSkill
    pub biota_properties_skill: Vec<BiotaPropertiesSkill>,
    // ACE: Biota.BiotaPropertiesSpellBook
    pub biota_properties_spell_book: Vec<BiotaPropertiesSpellBook>,
    // ACE: Biota.BiotaPropertiesString
    pub biota_properties_string: Vec<BiotaPropertiesString>,
    // ACE: Biota.BiotaPropertiesTextureMap
    pub biota_properties_texture_map: Vec<BiotaPropertiesTextureMap>,
    // ACE: Biota.HousePermission
    pub house_permission: Vec<HousePermission>,
}
