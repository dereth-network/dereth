// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/BiotaPropertiesEmote.cs
//! `BiotaPropertiesEmote`: a row of the `shard` database (Entity Framework model).

use super::BiotaPropertiesEmoteAction;

// ACE: BiotaPropertiesEmote
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BiotaPropertiesEmote {
    // ACE: BiotaPropertiesEmote.Id
    pub id: u32,
    // ACE: BiotaPropertiesEmote.ObjectId
    pub object_id: u32,
    // ACE: BiotaPropertiesEmote.Category
    pub category: u32,
    // ACE: BiotaPropertiesEmote.Probability
    pub probability: f32,
    // ACE: BiotaPropertiesEmote.WeenieClassId
    pub weenie_class_id: Option<u32>,
    // ACE: BiotaPropertiesEmote.Style
    pub style: Option<u32>,
    // ACE: BiotaPropertiesEmote.Substyle
    pub substyle: Option<u32>,
    // ACE: BiotaPropertiesEmote.Quest
    pub quest: Option<String>,
    // ACE: BiotaPropertiesEmote.VendorType
    pub vendor_type: Option<i32>,
    // ACE: BiotaPropertiesEmote.MinHealth
    pub min_health: Option<f32>,
    // ACE: BiotaPropertiesEmote.MaxHealth
    pub max_health: Option<f32>,
    // ACE: BiotaPropertiesEmote.BiotaPropertiesEmoteAction
    pub biota_properties_emote_action: Vec<BiotaPropertiesEmoteAction>,
}
