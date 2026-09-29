// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/WeeniePropertiesEmoteAction.cs
//! `WeeniePropertiesEmoteAction`: one row of the world-database table `weenie_properties_emote_action`.

/// EmoteAction Properties of Weenies
// ACE: WeeniePropertiesEmoteAction
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WeeniePropertiesEmoteAction {
    /// Unique Id of this Property
    pub id: u32,
    /// Id of the emote this property belongs to
    pub emote_id: u32,
    /// Emote Action Sequence Order
    pub order: u32,
    /// EmoteType
    pub r#type: u32,
    /// Time to wait before EmoteAction starts execution
    pub delay: f32,
    /// ?
    pub extent: f32,
    pub motion: Option<u32>,
    pub message: Option<String>,
    pub test_string: Option<String>,
    pub min: Option<i32>,
    pub max: Option<i32>,
    pub min_64: Option<i64>,
    pub max_64: Option<i64>,
    pub min_dbl: Option<f64>,
    pub max_dbl: Option<f64>,
    pub stat: Option<i32>,
    pub display: Option<bool>,
    pub amount: Option<i32>,
    pub amount_64: Option<i64>,
    pub hero_xp_64: Option<i64>,
    pub percent: Option<f64>,
    pub spell_id: Option<i32>,
    pub wealth_rating: Option<i32>,
    pub treasure_class: Option<i32>,
    pub treasure_type: Option<i32>,
    pub p_script: Option<i32>,
    pub sound: Option<i32>,
    /// Type of Destination the value applies to (DestinationType.????)
    pub destination_type: Option<i8>,
    /// Weenie Class Id of object to Create
    pub weenie_class_id: Option<u32>,
    /// Stack Size of object to create (-1 = infinite)
    pub stack_size: Option<i32>,
    /// Palette Color of Object
    pub palette: Option<i32>,
    /// Shade of Object&apos;s Palette
    pub shade: Option<f32>,
    /// Unused?
    pub try_to_bond: Option<bool>,
    pub obj_cell_id: Option<u32>,
    pub origin_x: Option<f32>,
    pub origin_y: Option<f32>,
    pub origin_z: Option<f32>,
    pub angles_w: Option<f32>,
    pub angles_x: Option<f32>,
    pub angles_y: Option<f32>,
    pub angles_z: Option<f32>,
    // ACE: WeeniePropertiesEmoteAction.Emote navigates back to the parent row, not carried.
}
