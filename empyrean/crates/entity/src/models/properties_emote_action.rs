// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Models/PropertiesEmoteAction.cs
//! `PropertiesEmoteAction`: one action of an emote.

use crate::enums::{MotionCommand, PlayScript, Sound};

/// ACE: PropertiesEmoteAction. `Clone` copies every field; ACE's own `Clone()` is [`PropertiesEmoteAction::ace_clone`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PropertiesEmoteAction {
    /// Only used to tie this record back to a specific database row.
    // ACE: PropertiesEmoteAction.DatabaseRecordId
    pub database_record_id: u32,
    // ACE: PropertiesEmoteAction.Type
    pub r#type: u32,
    // ACE: PropertiesEmoteAction.Delay
    pub delay: f32,
    // ACE: PropertiesEmoteAction.Extent
    pub extent: f32,
    // ACE: PropertiesEmoteAction.Motion
    pub motion: Option<MotionCommand>,
    // ACE: PropertiesEmoteAction.Message
    pub message: Option<String>,
    // ACE: PropertiesEmoteAction.TestString
    pub test_string: Option<String>,
    // ACE: PropertiesEmoteAction.Min
    pub min: Option<i32>,
    // ACE: PropertiesEmoteAction.Max
    pub max: Option<i32>,
    // ACE: PropertiesEmoteAction.Min64
    pub min_64: Option<i64>,
    // ACE: PropertiesEmoteAction.Max64
    pub max_64: Option<i64>,
    // ACE: PropertiesEmoteAction.MinDbl
    pub min_dbl: Option<f64>,
    // ACE: PropertiesEmoteAction.MaxDbl
    pub max_dbl: Option<f64>,
    // ACE: PropertiesEmoteAction.Stat
    pub stat: Option<i32>,
    // ACE: PropertiesEmoteAction.Display
    pub display: Option<bool>,
    // ACE: PropertiesEmoteAction.Amount
    pub amount: Option<i32>,
    // ACE: PropertiesEmoteAction.Amount64
    pub amount_64: Option<i64>,
    // ACE: PropertiesEmoteAction.HeroXP64
    pub hero_xp_64: Option<i64>,
    // ACE: PropertiesEmoteAction.Percent
    pub percent: Option<f64>,
    // ACE: PropertiesEmoteAction.SpellId
    pub spell_id: Option<i32>,
    // ACE: PropertiesEmoteAction.WealthRating
    pub wealth_rating: Option<i32>,
    // ACE: PropertiesEmoteAction.TreasureClass
    pub treasure_class: Option<i32>,
    // ACE: PropertiesEmoteAction.TreasureType
    pub treasure_type: Option<i32>,
    // ACE: PropertiesEmoteAction.PScript
    pub p_script: Option<PlayScript>,
    // ACE: PropertiesEmoteAction.Sound
    pub sound: Option<Sound>,
    // ACE: PropertiesEmoteAction.DestinationType
    pub destination_type: Option<i8>,
    // ACE: PropertiesEmoteAction.WeenieClassId
    pub weenie_class_id: Option<u32>,
    // ACE: PropertiesEmoteAction.StackSize
    pub stack_size: Option<i32>,
    // ACE: PropertiesEmoteAction.Palette
    pub palette: Option<i32>,
    // ACE: PropertiesEmoteAction.Shade
    pub shade: Option<f32>,
    // ACE: PropertiesEmoteAction.TryToBond
    pub try_to_bond: Option<bool>,
    // ACE: PropertiesEmoteAction.ObjCellId
    pub obj_cell_id: Option<u32>,
    // ACE: PropertiesEmoteAction.OriginX
    pub origin_x: Option<f32>,
    // ACE: PropertiesEmoteAction.OriginY
    pub origin_y: Option<f32>,
    // ACE: PropertiesEmoteAction.OriginZ
    pub origin_z: Option<f32>,
    // ACE: PropertiesEmoteAction.AnglesW
    pub angles_w: Option<f32>,
    // ACE: PropertiesEmoteAction.AnglesX
    pub angles_x: Option<f32>,
    // ACE: PropertiesEmoteAction.AnglesY
    pub angles_y: Option<f32>,
    // ACE: PropertiesEmoteAction.AnglesZ
    pub angles_z: Option<f32>,
}

impl PropertiesEmoteAction {
    /// ACE's `Clone()`: a copy with `DatabaseRecordId` reset to 0.
    // ACE: PropertiesEmoteAction.Clone
    #[must_use]
    pub fn ace_clone(&self) -> PropertiesEmoteAction {
        PropertiesEmoteAction {
            r#type: self.r#type,
            delay: self.delay,
            extent: self.extent,
            motion: self.motion,
            message: self.message.clone(),
            test_string: self.test_string.clone(),
            min: self.min,
            max: self.max,
            min_64: self.min_64,
            max_64: self.max_64,
            min_dbl: self.min_dbl,
            max_dbl: self.max_dbl,
            stat: self.stat,
            display: self.display,
            amount: self.amount,
            amount_64: self.amount_64,
            hero_xp_64: self.hero_xp_64,
            percent: self.percent,
            spell_id: self.spell_id,
            wealth_rating: self.wealth_rating,
            treasure_class: self.treasure_class,
            treasure_type: self.treasure_type,
            p_script: self.p_script,
            sound: self.sound,
            destination_type: self.destination_type,
            weenie_class_id: self.weenie_class_id,
            stack_size: self.stack_size,
            palette: self.palette,
            shade: self.shade,
            try_to_bond: self.try_to_bond,
            obj_cell_id: self.obj_cell_id,
            origin_x: self.origin_x,
            origin_y: self.origin_y,
            origin_z: self.origin_z,
            angles_w: self.angles_w,
            angles_x: self.angles_x,
            angles_y: self.angles_y,
            angles_z: self.angles_z,
            database_record_id: 0,
        }
    }
}
