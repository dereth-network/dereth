// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Allegiance.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Allegiance.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/Allegiance.cs`.

use empyrean_entity::enums::PropertyString;

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: Allegiance.AllegianceName
    pub fn allegiance_name(&self) -> Option<String> {
        self.get_property(PropertyString::AllegianceName)
    }

    // ACE: Allegiance.AllegianceName
    pub fn set_allegiance_name(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::AllegianceName),
            Some(v) => self.set_property(PropertyString::AllegianceName, v),
        }
    }

    // ACE: Allegiance.AllegianceMotd
    pub fn allegiance_motd(&self) -> Option<String> {
        self.get_property(PropertyString::AllegianceMotd)
    }

    // ACE: Allegiance.AllegianceMotd
    pub fn set_allegiance_motd(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::AllegianceMotd),
            Some(v) => self.set_property(PropertyString::AllegianceMotd, v),
        }
    }

    // ACE: Allegiance.AllegianceMotdSetBy
    pub fn allegiance_motd_set_by(&self) -> Option<String> {
        self.get_property(PropertyString::AllegianceMotdSetBy)
    }

    // ACE: Allegiance.AllegianceMotdSetBy
    pub fn set_allegiance_motd_set_by(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::AllegianceMotdSetBy),
            Some(v) => self.set_property(PropertyString::AllegianceMotdSetBy, v),
        }
    }

    // ACE: Allegiance.AllegianceSpeakerTitle
    pub fn allegiance_speaker_title(&self) -> Option<String> {
        self.get_property(PropertyString::AllegianceSpeakerTitle)
    }

    // ACE: Allegiance.AllegianceSpeakerTitle
    pub fn set_allegiance_speaker_title(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::AllegianceSpeakerTitle),
            Some(v) => self.set_property(PropertyString::AllegianceSpeakerTitle, v),
        }
    }

    // ACE: Allegiance.AllegianceSeneschalTitle
    pub fn allegiance_seneschal_title(&self) -> Option<String> {
        self.get_property(PropertyString::AllegianceSeneschalTitle)
    }

    // ACE: Allegiance.AllegianceSeneschalTitle
    pub fn set_allegiance_seneschal_title(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::AllegianceSeneschalTitle),
            Some(v) => self.set_property(PropertyString::AllegianceSeneschalTitle, v),
        }
    }

    // ACE: Allegiance.AllegianceCastellanTitle
    pub fn allegiance_castellan_title(&self) -> Option<String> {
        self.get_property(PropertyString::AllegianceCastellanTitle)
    }

    // ACE: Allegiance.AllegianceCastellanTitle
    pub fn set_allegiance_castellan_title(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::AllegianceCastellanTitle),
            Some(v) => self.set_property(PropertyString::AllegianceCastellanTitle, v),
        }
    }
}
