// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/WeeniePropertiesEmote.cs
//! `WeeniePropertiesEmote`: one row of the world-database table `weenie_properties_emote`.

use super::WeeniePropertiesEmoteAction;

/// Emote Properties of Weenies
// ACE: WeeniePropertiesEmote
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WeeniePropertiesEmote {
    /// Unique Id of this Property
    pub id: u32,
    /// Id of the object this property belongs to
    pub object_id: u32,
    /// EmoteCategory
    pub category: u32,
    /// Probability of this EmoteSet being chosen
    pub probability: f32,
    pub weenie_class_id: Option<u32>,
    pub style: Option<u32>,
    pub substyle: Option<u32>,
    pub quest: Option<String>,
    pub vendor_type: Option<i32>,
    pub min_health: Option<f32>,
    pub max_health: Option<f32>,
    // ACE: WeeniePropertiesEmote.Object navigates back to the parent row, not carried.
    pub weenie_properties_emote_action: Vec<WeeniePropertiesEmoteAction>,
}
