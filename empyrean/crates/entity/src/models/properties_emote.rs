// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Models/PropertiesEmote.cs
//! `PropertiesEmote`: one emote (a category, its conditions and its actions).

use std::sync::Arc;

use crate::enums::{EmoteCategory, MotionCommand, MotionStance, VendorType};
use crate::models::properties_emote_action::PropertiesEmoteAction;
use crate::models::weenie::Weenie;

/// ACE: PropertiesEmote. `Clone` copies every field; ACE's own `Clone()` is
/// [`PropertiesEmote::ace_clone`].
#[derive(Debug, Clone, Default)]
pub struct PropertiesEmote {
    /// Only used to tie this record back to a specific database row.
    // ACE: PropertiesEmote.DatabaseRecordId
    pub database_record_id: u32,
    // ACE: PropertiesEmote.Category
    pub category: EmoteCategory,
    // ACE: PropertiesEmote.Probability
    pub probability: f32,
    // ACE: PropertiesEmote.WeenieClassId
    pub weenie_class_id: Option<u32>,
    // ACE: PropertiesEmote.Style
    pub style: Option<MotionStance>,
    // ACE: PropertiesEmote.Substyle
    pub substyle: Option<MotionCommand>,
    // ACE: PropertiesEmote.Quest
    pub quest: Option<String>,
    // ACE: PropertiesEmote.VendorType
    pub vendor_type: Option<VendorType>,
    // ACE: PropertiesEmote.MinHealth
    pub min_health: Option<f32>,
    // ACE: PropertiesEmote.MaxHealth
    pub max_health: Option<f32>,
    /// A reference in ACE (shared, never copied by `Clone()`).
    // ACE: PropertiesEmote.Object
    pub object: Option<Arc<Weenie>>,
    /// Initialised to an empty list, as in ACE.
    // ACE: PropertiesEmote.PropertiesEmoteAction
    pub properties_emote_action: Vec<PropertiesEmoteAction>,
}

impl PropertiesEmote {
    /// ACE's `Clone()`: `DatabaseRecordId` reset to 0, `Object` left `null`, and every action
    /// cloned with [`PropertiesEmoteAction::ace_clone`].
    // ACE: PropertiesEmote.Clone
    #[must_use]
    pub fn ace_clone(&self) -> PropertiesEmote {
        let mut result = PropertiesEmote {
            database_record_id: 0,
            category: self.category,
            probability: self.probability,
            weenie_class_id: self.weenie_class_id,
            style: self.style,
            substyle: self.substyle,
            quest: self.quest.clone(),
            vendor_type: self.vendor_type,
            min_health: self.min_health,
            max_health: self.max_health,
            object: None,
            properties_emote_action: Vec::new(),
        };

        for action in &self.properties_emote_action {
            result.properties_emote_action.push(action.ace_clone());
        }

        result
    }
}
