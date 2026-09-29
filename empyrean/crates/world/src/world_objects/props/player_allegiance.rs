// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Allegiance.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Player_Allegiance.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/Player_Allegiance.cs`.

use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::{PropertyBool, PropertyInt, PropertyInt64};

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: Player.AllegianceXPCached
    pub fn allegiance_xp_cached(&self) -> u64 {
        self.get_property(PropertyInt64::AllegianceXPCached)
            .unwrap_or(0)
            .cs_cast()
    }

    // ACE: Player.AllegianceXPCached
    pub fn set_allegiance_xp_cached(&mut self, value: u64) {
        if value == 0 {
            self.remove_property(PropertyInt64::AllegianceXPCached);
        } else {
            self.set_property(PropertyInt64::AllegianceXPCached, value.cs_cast());
        }
    }

    // ACE: Player.AllegianceXPGenerated
    pub fn allegiance_xp_generated(&self) -> u64 {
        self.get_property(PropertyInt64::AllegianceXPGenerated)
            .unwrap_or(0)
            .cs_cast()
    }

    // ACE: Player.AllegianceXPGenerated
    pub fn set_allegiance_xp_generated(&mut self, value: u64) {
        if value == 0 {
            self.remove_property(PropertyInt64::AllegianceXPGenerated);
        } else {
            self.set_property(PropertyInt64::AllegianceXPGenerated, value.cs_cast());
        }
    }

    // ACE: Player.AllegianceXPReceived
    pub fn allegiance_xp_received(&self) -> u64 {
        self.get_property(PropertyInt64::AllegianceXPReceived)
            .unwrap_or(0)
            .cs_cast()
    }

    // ACE: Player.AllegianceXPReceived
    pub fn set_allegiance_xp_received(&mut self, value: u64) {
        if value == 0 {
            self.remove_property(PropertyInt64::AllegianceXPReceived);
        } else {
            self.set_property(PropertyInt64::AllegianceXPReceived, value.cs_cast());
        }
    }

    // ACE: Player.AllegianceRank
    pub fn allegiance_rank(&self) -> Option<i32> {
        self.get_property(PropertyInt::AllegianceRank)
    }

    // ACE: Player.AllegianceRank
    pub fn set_allegiance_rank(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::AllegianceRank),
            Some(v) => self.set_property(PropertyInt::AllegianceRank, v),
        }
    }

    // ACE: Player.AllegianceOfficerRank
    pub fn allegiance_officer_rank(&self) -> Option<i32> {
        self.get_property(PropertyInt::AllegianceOfficerRank)
    }

    // ACE: Player.AllegianceOfficerRank
    pub fn set_allegiance_officer_rank(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::AllegianceOfficerRank),
            Some(v) => self.set_property(PropertyInt::AllegianceOfficerRank, v),
        }
    }

    // ACE: Player.ExistedBeforeAllegianceXpChanges
    pub fn existed_before_allegiance_xp_changes(&self) -> bool {
        self.get_property(PropertyBool::ExistedBeforeAllegianceXpChanges)
            .unwrap_or(true)
    }

    // ACE: Player.ExistedBeforeAllegianceXpChanges
    pub fn set_existed_before_allegiance_xp_changes(&mut self, value: bool) {
        if value {
            self.remove_property(PropertyBool::ExistedBeforeAllegianceXpChanges);
        } else {
            self.set_property(PropertyBool::ExistedBeforeAllegianceXpChanges, value);
        }
    }
}
