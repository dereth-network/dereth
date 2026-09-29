// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Networking.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject_Networking.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/WorldObject_Networking.cs`.

use empyrean_entity::enums::PropertyBool;

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: WorldObject.IgnoreCloIcons
    pub fn ignore_clo_icons(&self) -> Option<bool> {
        self.get_property(PropertyBool::IgnoreCloIcons)
    }

    // ACE: WorldObject.IgnoreCloIcons
    pub fn set_ignore_clo_icons(&mut self, value: Option<bool>) {
        match value {
            None => self.remove_property(PropertyBool::IgnoreCloIcons),
            Some(v) => self.set_property(PropertyBool::IgnoreCloIcons, v),
        }
    }

    // ACE: WorldObject.Dyable
    pub fn dyable(&self) -> Option<bool> {
        self.get_property(PropertyBool::Dyable)
    }

    // ACE: WorldObject.Dyable
    pub fn set_dyable(&mut self, value: Option<bool>) {
        match value {
            None => self.remove_property(PropertyBool::Dyable),
            Some(v) => self.set_property(PropertyBool::Dyable, v),
        }
    }
}
