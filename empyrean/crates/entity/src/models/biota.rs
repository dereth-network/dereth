// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Models/Biota.cs
//! `Biota`: the property state of one world object.
//!
//! As in ACE, only populated collections and dictionaries are initialized (`Some`), to conserve
//! memory: check for `None` first. The five common-property collections may be shared with the
//! weenie the biota came from; they are `Arc`s and are written through the `*_mut` accessors
//! below, which copy on write (see [`crate::models`]).

use std::sync::Arc;

use empyrean_common::dotnet::{DotNetDict, DotNetHashSet};

use crate::enums::{
    CombatBodyPart, PositionType, PropertyAttribute, PropertyAttribute2nd, PropertyBool,
    PropertyDataId, PropertyFloat, PropertyInstanceId, PropertyInt, PropertyInt64, PropertyString,
    Skill, WeenieType,
};
use crate::models::i_weenie::impl_i_weenie;
use crate::models::{
    PropertiesAllegiance, PropertiesAnimPart, PropertiesAttribute, PropertiesAttribute2nd,
    PropertiesBodyPart, PropertiesBook, PropertiesBookPageData, PropertiesCreateList,
    PropertiesEmote, PropertiesEnchantmentRegistry, PropertiesGenerator, PropertiesPalette,
    PropertiesPosition, PropertiesSkill, PropertiesTextureMap,
};

/// ACE: Biota
#[derive(Debug, Clone, Default)]
pub struct Biota {
    // ACE: Biota.Id
    pub id: u32,
    // ACE: Biota.WeenieClassId
    pub weenie_class_id: u32,
    // ACE: Biota.WeenieType
    pub weenie_type: WeenieType,

    // ACE: Biota.PropertiesBool
    pub properties_bool: Option<DotNetDict<PropertyBool, bool>>,
    // ACE: Biota.PropertiesDID
    pub properties_did: Option<DotNetDict<PropertyDataId, u32>>,
    // ACE: Biota.PropertiesFloat
    pub properties_float: Option<DotNetDict<PropertyFloat, f64>>,
    // ACE: Biota.PropertiesIID
    pub properties_iid: Option<DotNetDict<PropertyInstanceId, u32>>,
    // ACE: Biota.PropertiesInt
    pub properties_int: Option<DotNetDict<PropertyInt, i32>>,
    // ACE: Biota.PropertiesInt64
    pub properties_int64: Option<DotNetDict<PropertyInt64, i64>>,
    // ACE: Biota.PropertiesString
    pub properties_string: Option<DotNetDict<PropertyString, String>>,

    // ACE: Biota.PropertiesPosition
    pub properties_position: Option<DotNetDict<PositionType, PropertiesPosition>>,

    /// Spell id to probability.
    // ACE: Biota.PropertiesSpellBook
    pub properties_spell_book: Option<DotNetDict<i32, f32>>,

    // ACE: Biota.PropertiesAnimPart
    pub properties_anim_part: Option<Vec<PropertiesAnimPart>>,
    // ACE: Biota.PropertiesPalette
    pub properties_palette: Option<Vec<PropertiesPalette>>,
    // ACE: Biota.PropertiesTextureMap
    pub properties_texture_map: Option<Vec<PropertiesTextureMap>>,

    // Properties for all world objects that typically aren't modified over the original weenie
    // ACE: Biota.PropertiesCreateList
    pub properties_create_list: Option<Arc<Vec<PropertiesCreateList>>>,
    // ACE: Biota.PropertiesEmote
    pub properties_emote: Option<Arc<Vec<PropertiesEmote>>>,
    // ACE: Biota.PropertiesEventFilter
    pub properties_event_filter: Option<Arc<DotNetHashSet<i32>>>,
    // ACE: Biota.PropertiesGenerator
    pub properties_generator: Option<Arc<Vec<PropertiesGenerator>>>,

    // Properties for creatures
    // ACE: Biota.PropertiesAttribute
    pub properties_attribute: Option<DotNetDict<PropertyAttribute, PropertiesAttribute>>,
    // ACE: Biota.PropertiesAttribute2nd
    pub properties_attribute_2nd: Option<DotNetDict<PropertyAttribute2nd, PropertiesAttribute2nd>>,
    // ACE: Biota.PropertiesBodyPart
    pub properties_body_part: Option<Arc<DotNetDict<CombatBodyPart, PropertiesBodyPart>>>,
    // ACE: Biota.PropertiesSkill
    pub properties_skill: Option<DotNetDict<Skill, PropertiesSkill>>,

    // Properties for books
    // ACE: Biota.PropertiesBook
    pub properties_book: Option<PropertiesBook>,
    // ACE: Biota.PropertiesBookPageData
    pub properties_book_page_data: Option<Vec<PropertiesBookPageData>>,

    // Biota additions over Weenie
    /// Character id to allegiance record.
    // ACE: Biota.PropertiesAllegiance
    pub properties_allegiance: Option<DotNetDict<u32, PropertiesAllegiance>>,
    // ACE: Biota.PropertiesEnchantmentRegistry
    pub properties_enchantment_registry: Option<Vec<PropertiesEnchantmentRegistry>>,
    /// Player guid to storage permission.
    // ACE: Biota.HousePermissions
    pub house_permissions: Option<DotNetDict<u32, bool>>,
}

impl_i_weenie!(Biota);

impl Biota {
    /// The create list for writing: copies it first if it is still shared with the weenie.
    pub fn properties_create_list_mut(&mut self) -> Option<&mut Vec<PropertiesCreateList>> {
        self.properties_create_list.as_mut().map(Arc::make_mut)
    }

    /// The emotes for writing: copies them first if they are still shared with the weenie.
    pub fn properties_emote_mut(&mut self) -> Option<&mut Vec<PropertiesEmote>> {
        self.properties_emote.as_mut().map(Arc::make_mut)
    }

    /// The event filter for writing: copies it first if it is still shared with the weenie.
    pub fn properties_event_filter_mut(&mut self) -> Option<&mut DotNetHashSet<i32>> {
        self.properties_event_filter.as_mut().map(Arc::make_mut)
    }

    /// The generator profiles for writing: copies them first if they are still shared with the
    /// weenie.
    pub fn properties_generator_mut(&mut self) -> Option<&mut Vec<PropertiesGenerator>> {
        self.properties_generator.as_mut().map(Arc::make_mut)
    }

    /// The body parts for writing: copies them first if they are still shared with the weenie.
    pub fn properties_body_part_mut(
        &mut self,
    ) -> Option<&mut DotNetDict<CombatBodyPart, PropertiesBodyPart>> {
        self.properties_body_part.as_mut().map(Arc::make_mut)
    }
}
