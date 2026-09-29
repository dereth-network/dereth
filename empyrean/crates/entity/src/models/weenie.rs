// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Models/Weenie.cs
//! `Weenie`: a template from the world database.
//!
//! As in ACE, only populated collections and dictionaries are initialized (`Some`), to conserve
//! memory: check for `None` first. The common-property collections are `Arc`s so that a biota can
//! share them (see [`crate::models`]).

use std::sync::Arc;

use empyrean_common::dotnet::{DotNetDict, DotNetHashSet};

use crate::enums::{
    CombatBodyPart, PositionType, PropertyAttribute, PropertyAttribute2nd, PropertyBool,
    PropertyDataId, PropertyFloat, PropertyInstanceId, PropertyInt, PropertyInt64, PropertyString,
    Skill, WeenieType,
};
use crate::models::i_weenie::impl_i_weenie;
use crate::models::{
    PropertiesAnimPart, PropertiesAttribute, PropertiesAttribute2nd, PropertiesBodyPart,
    PropertiesBook, PropertiesBookPageData, PropertiesCreateList, PropertiesEmote,
    PropertiesGenerator, PropertiesPalette, PropertiesPosition, PropertiesSkill,
    PropertiesTextureMap,
};

/// ACE: Weenie
#[derive(Debug, Clone, Default)]
pub struct Weenie {
    // ACE: Weenie.WeenieClassId
    pub weenie_class_id: u32,
    // ACE: Weenie.ClassName
    pub class_name: Option<String>,
    // ACE: Weenie.WeenieType
    pub weenie_type: WeenieType,

    // ACE: Weenie.PropertiesBool
    pub properties_bool: Option<DotNetDict<PropertyBool, bool>>,
    // ACE: Weenie.PropertiesDID
    pub properties_did: Option<DotNetDict<PropertyDataId, u32>>,
    // ACE: Weenie.PropertiesFloat
    pub properties_float: Option<DotNetDict<PropertyFloat, f64>>,
    // ACE: Weenie.PropertiesIID
    pub properties_iid: Option<DotNetDict<PropertyInstanceId, u32>>,
    // ACE: Weenie.PropertiesInt
    pub properties_int: Option<DotNetDict<PropertyInt, i32>>,
    // ACE: Weenie.PropertiesInt64
    pub properties_int64: Option<DotNetDict<PropertyInt64, i64>>,
    // ACE: Weenie.PropertiesString
    pub properties_string: Option<DotNetDict<PropertyString, String>>,

    // ACE: Weenie.PropertiesPosition
    pub properties_position: Option<DotNetDict<PositionType, PropertiesPosition>>,

    /// Spell id to probability.
    // ACE: Weenie.PropertiesSpellBook
    pub properties_spell_book: Option<DotNetDict<i32, f32>>,

    // ACE: Weenie.PropertiesAnimPart
    pub properties_anim_part: Option<Vec<PropertiesAnimPart>>,
    // ACE: Weenie.PropertiesPalette
    pub properties_palette: Option<Vec<PropertiesPalette>>,
    // ACE: Weenie.PropertiesTextureMap
    pub properties_texture_map: Option<Vec<PropertiesTextureMap>>,

    // Properties for all world objects that typically aren't modified over the original weenie
    // ACE: Weenie.PropertiesCreateList
    pub properties_create_list: Option<Arc<Vec<PropertiesCreateList>>>,
    // ACE: Weenie.PropertiesEmote
    pub properties_emote: Option<Arc<Vec<PropertiesEmote>>>,
    // ACE: Weenie.PropertiesEventFilter
    pub properties_event_filter: Option<Arc<DotNetHashSet<i32>>>,
    // ACE: Weenie.PropertiesGenerator
    pub properties_generator: Option<Arc<Vec<PropertiesGenerator>>>,

    // Properties for creatures
    // ACE: Weenie.PropertiesAttribute
    pub properties_attribute: Option<DotNetDict<PropertyAttribute, PropertiesAttribute>>,
    // ACE: Weenie.PropertiesAttribute2nd
    pub properties_attribute_2nd: Option<DotNetDict<PropertyAttribute2nd, PropertiesAttribute2nd>>,
    // ACE: Weenie.PropertiesBodyPart
    pub properties_body_part: Option<Arc<DotNetDict<CombatBodyPart, PropertiesBodyPart>>>,
    // ACE: Weenie.PropertiesSkill
    pub properties_skill: Option<DotNetDict<Skill, PropertiesSkill>>,

    // Properties for books
    // ACE: Weenie.PropertiesBook
    pub properties_book: Option<PropertiesBook>,
    // ACE: Weenie.PropertiesBookPageData
    pub properties_book_page_data: Option<Vec<PropertiesBookPageData>>,
}

impl_i_weenie!(Weenie);
