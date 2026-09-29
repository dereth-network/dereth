// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/WeenieExtensions.cs
//! Lookups on a world-database [`Weenie`] row (the EF model, before `WeenieConverter`). Each is
//! LINQ `FirstOrDefault` over the child rows in their stored order.

use empyrean_entity::enums::{
    PositionType, PropertyAttribute, PropertyAttribute2nd, PropertyBool, PropertyDataId,
    PropertyFloat, PropertyInstanceId, PropertyInt, PropertyInt64, PropertyString, Skill,
};
use empyrean_entity::Position;

use super::{
    Weenie, WeeniePropertiesAttribute, WeeniePropertiesAttribute2nd, WeeniePropertiesBodyPart,
    WeeniePropertiesBookPageData, WeeniePropertiesCreateList, WeeniePropertiesEmote,
    WeeniePropertiesEventFilter, WeeniePropertiesPalette, WeeniePropertiesPosition,
    WeeniePropertiesSkill, WeeniePropertiesSpellBook, WeeniePropertiesTextureMap,
};

// ACE: WeenieExtensions
impl Weenie {
    // =====================================
    // Get
    // Bool, DID, Float, IID, Int, Int64, String, Position
    // =====================================

    // ACE: WeenieExtensions.GetProperty
    #[must_use]
    pub fn get_property_bool(&self, property: PropertyBool) -> Option<bool> {
        self.weenie_properties_bool
            .iter()
            .find(|x| u32::from(x.r#type) == u32::from(property.0))
            .map(|x| x.value)
    }

    // ACE: WeenieExtensions.GetProperty
    #[must_use]
    pub fn get_property_did(&self, property: PropertyDataId) -> Option<u32> {
        self.weenie_properties_did
            .iter()
            .find(|x| u32::from(x.r#type) == u32::from(property.0))
            .map(|x| x.value)
    }

    // ACE: WeenieExtensions.GetProperty
    #[must_use]
    pub fn get_property_float(&self, property: PropertyFloat) -> Option<f64> {
        self.weenie_properties_float
            .iter()
            .find(|x| x.r#type == property.0)
            .map(|x| x.value)
    }

    // ACE: WeenieExtensions.GetProperty
    #[must_use]
    pub fn get_property_iid(&self, property: PropertyInstanceId) -> Option<u32> {
        self.weenie_properties_iid
            .iter()
            .find(|x| u32::from(x.r#type) == u32::from(property.0))
            .map(|x| x.value)
    }

    // ACE: WeenieExtensions.GetProperty
    #[must_use]
    pub fn get_property_int(&self, property: PropertyInt) -> Option<i32> {
        self.weenie_properties_int
            .iter()
            .find(|x| u32::from(x.r#type) == u32::from(property.0))
            .map(|x| x.value)
    }

    // ACE: WeenieExtensions.GetProperty
    #[must_use]
    pub fn get_property_int64(&self, property: PropertyInt64) -> Option<i64> {
        self.weenie_properties_int64
            .iter()
            .find(|x| u32::from(x.r#type) == u32::from(property.0))
            .map(|x| x.value)
    }

    // ACE: WeenieExtensions.GetProperty
    #[must_use]
    pub fn get_property_string(&self, property: PropertyString) -> Option<&str> {
        self.weenie_properties_string
            .iter()
            .find(|x| u32::from(x.r#type) == u32::from(property.0))
            .map(|x| x.value.as_str())
    }

    // ACE: WeenieExtensions.GetProperty
    #[must_use]
    pub fn get_property_position(
        &self,
        position_type: PositionType,
    ) -> Option<&WeeniePropertiesPosition> {
        self.weenie_properties_position
            .iter()
            .find(|x| u32::from(x.position_type) == u32::from(position_type.0))
    }

    // ACE: WeenieExtensions.GetPosition
    #[must_use]
    pub fn get_position(&self, position_type: PositionType) -> Option<Position> {
        let result = self
            .weenie_properties_position
            .iter()
            .find(|x| u32::from(x.position_type) == u32::from(position_type.0))?;

        Some(Position::from_components(
            result.obj_cell_id,
            result.origin_x,
            result.origin_y,
            result.origin_z,
            result.angles_x,
            result.angles_y,
            result.angles_z,
            result.angles_w,
            false,
        ))
    }

    // =====================================
    // Get
    // All Else
    // =====================================

    // ACE: WeenieExtensions.GetSpell
    #[must_use]
    pub fn get_spell(&self, spell: i32) -> Option<&WeeniePropertiesSpellBook> {
        self.weenie_properties_spell_book
            .iter()
            .find(|x| x.spell == spell)
    }

    // ACE: WeenieExtensions.GetAnimationId
    #[must_use]
    pub fn get_animation_id(&self, index: u8) -> Option<u32> {
        self.weenie_properties_anim_part
            .iter()
            .find(|x| x.index == index)
            .map(|x| x.animation_id)
    }

    // ACE: WeenieExtensions.GetPalette
    #[must_use]
    pub fn get_palette(&self, sub_palette_id: u32) -> Option<&WeeniePropertiesPalette> {
        self.weenie_properties_palette
            .iter()
            .find(|x| x.sub_palette_id == sub_palette_id)
    }

    // ACE: WeenieExtensions.GetTextureMap
    #[must_use]
    pub fn get_texture_map(&self, index: u8) -> Option<&WeeniePropertiesTextureMap> {
        self.weenie_properties_texture_map
            .iter()
            .find(|x| x.index == index)
    }

    // ACE: WeenieExtensions.GetCreateList
    #[must_use]
    pub fn get_create_list(&self, destination_type: i8) -> Option<&WeeniePropertiesCreateList> {
        self.weenie_properties_create_list
            .iter()
            .find(|x| x.destination_type == destination_type)
    }

    // ACE: WeenieExtensions.GetEmote
    #[must_use]
    pub fn get_emote(&self, category: u32) -> Option<&WeeniePropertiesEmote> {
        self.weenie_properties_emote
            .iter()
            .find(|x| x.category == category)
    }

    // ACE: WeenieExtensions.GetEventFilter
    #[must_use]
    pub fn get_event_filter(&self, event_id: i32) -> Option<&WeeniePropertiesEventFilter> {
        self.weenie_properties_event_filter
            .iter()
            .find(|x| x.event == event_id)
    }

    // WeeniePropertiesGenerator

    // ACE: WeenieExtensions.GetProperty
    #[must_use]
    pub fn get_property_attribute(
        &self,
        property: PropertyAttribute,
    ) -> Option<&WeeniePropertiesAttribute> {
        self.weenie_properties_attribute
            .iter()
            .find(|x| u32::from(x.r#type) == u32::from(property.0))
    }

    // ACE: WeenieExtensions.GetProperty
    #[must_use]
    pub fn get_property_attribute_2nd(
        &self,
        property: PropertyAttribute2nd,
    ) -> Option<&WeeniePropertiesAttribute2nd> {
        self.weenie_properties_attribute_2nd
            .iter()
            .find(|x| u32::from(x.r#type) == u32::from(property.0))
    }

    // ACE: WeenieExtensions.GetBodyPart
    #[must_use]
    pub fn get_body_part(&self, key: u16) -> Option<&WeeniePropertiesBodyPart> {
        self.weenie_properties_body_part
            .iter()
            .find(|x| x.key == key)
    }

    // ACE: WeenieExtensions.GetProperty
    #[must_use]
    pub fn get_property_skill(&self, skill: Skill) -> Option<&WeeniePropertiesSkill> {
        // C# `(uint)skill` of an `int` enum: the bit pattern.
        #[allow(clippy::cast_sign_loss)]
        let skill = skill.0 as u32;
        self.weenie_properties_skill
            .iter()
            .find(|x| u32::from(x.r#type) == skill)
    }

    // ACE: WeenieExtensions.GetBookPageData
    #[must_use]
    pub fn get_book_page_data(&self, page_id: u32) -> Option<&WeeniePropertiesBookPageData> {
        self.weenie_properties_book_page_data
            .iter()
            .find(|x| x.page_id == page_id)
    }
}
