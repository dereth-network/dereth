// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/EmoteSet.cs
//! `EmoteSet`: the legacy emote-set record (fields only).

use crate::emote::Emote;
use crate::enums::EmoteCategory;

/// ACE: EmoteSet
#[derive(Debug, Clone, Default)]
pub struct EmoteSet {
    // ACE: EmoteSet.Category
    pub category: EmoteCategory,
    // ACE: EmoteSet.Probability
    pub probability: f32,
    // ACE: EmoteSet.ClassID
    pub class_id: u32,
    // ACE: EmoteSet.Quest
    pub quest: Option<String>,
    // ACE: EmoteSet.Style
    pub style: u32,
    // ACE: EmoteSet.Substate
    pub substate: u32,
    // ACE: EmoteSet.VendorType
    pub vendor_type: u32,
    // ACE: EmoteSet.MinHealth
    pub min_health: f32,
    // ACE: EmoteSet.MaxHealth
    pub max_health: f32,
    // ACE: EmoteSet.Emotes
    pub emotes: Option<Vec<Emote>>,
}
