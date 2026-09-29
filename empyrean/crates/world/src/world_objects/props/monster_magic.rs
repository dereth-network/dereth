// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Monster_Magic.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Monster_Magic.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/Monster_Magic.cs`.

use empyrean_entity::enums::{PropertyBool, PropertyFloat};

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: Creature.AiUsesMana
    pub fn ai_uses_mana(&self) -> bool {
        self.get_property(PropertyBool::AiUsesMana).unwrap_or(true)
    }

    // ACE: Creature.AiUsesMana
    pub fn set_ai_uses_mana(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::AiUsesMana);
        } else {
            self.set_property(PropertyBool::AiUsesMana, value);
        }
    }

    // ACE: Creature.AiUseHumanMagicAnimations
    pub fn ai_use_human_magic_animations(&self) -> bool {
        self.get_property(PropertyBool::AiUseHumanMagicAnimations)
            .unwrap_or(false)
    }

    // ACE: Creature.AiUseHumanMagicAnimations
    pub fn set_ai_use_human_magic_animations(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::AiUseHumanMagicAnimations);
        } else {
            self.set_property(PropertyBool::AiUseHumanMagicAnimations, value);
        }
    }

    // ACE: Creature.AiUseMagicDelay
    pub fn ai_use_magic_delay(&self) -> Option<f64> {
        self.get_property(PropertyFloat::AiUseMagicDelay)
    }

    // ACE: Creature.AiUseMagicDelay
    pub fn set_ai_use_magic_delay(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::AiUseMagicDelay),
            Some(v) => self.set_property(PropertyFloat::AiUseMagicDelay, v),
        }
    }
}
