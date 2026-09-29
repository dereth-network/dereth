// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Monster_Combat.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Monster_Combat.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/Monster_Combat.cs`.

use empyrean_entity::enums::{CombatStyle, PropertyBool, PropertyInt};

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: Creature.AiAllowedCombatStyle
    pub fn ai_allowed_combat_style(&self) -> CombatStyle {
        CombatStyle(
            self.get_property(PropertyInt::AiAllowedCombatStyle)
                .unwrap_or(0),
        )
    }

    // ACE: Creature.AiAllowedCombatStyle
    pub fn set_ai_allowed_combat_style(&mut self, value: CombatStyle) {
        if value.0 == 0 {
            self.remove_property(PropertyInt::AiAllowedCombatStyle);
        } else {
            self.set_property(PropertyInt::AiAllowedCombatStyle, value.0);
        }
    }

    // ACE: Creature.NeverAttack
    pub fn never_attack(&self) -> bool {
        self.get_property(PropertyBool::NeverAttack)
            .unwrap_or(false)
    }

    // ACE: Creature.NeverAttack
    pub fn set_never_attack(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::NeverAttack);
        } else {
            self.set_property(PropertyBool::NeverAttack, value);
        }
    }
}
