// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Monster_Awareness.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Monster_Awareness.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/Monster_Awareness.cs`.

use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::{PropertyFloat, PropertyInt, TargetingTactic, Tolerance};

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: Creature.Tolerance
    pub fn tolerance(&self) -> Tolerance {
        Tolerance(self.get_property(PropertyInt::Tolerance).unwrap_or(0))
    }

    // ACE: Creature.Tolerance
    pub fn set_tolerance(&mut self, value: Tolerance) {
        if value.0 == 0 {
            self.remove_property(PropertyInt::Tolerance);
        } else {
            self.set_property(PropertyInt::Tolerance, value.0);
        }
    }

    // ACE: Creature.TargetingTactic
    pub fn targeting_tactic(&self) -> TargetingTactic {
        TargetingTactic(
            self.get_property(PropertyInt::TargetingTactic)
                .unwrap_or(0)
                .cs_cast(),
        )
    }

    // ACE: Creature.TargetingTactic
    // ACE-BUG: the setter stores the current TargetingTactic instead of `value`, so a non-zero value never changes the stored tactic (it can only be cleared with 0).
    pub fn set_targeting_tactic(&mut self, value: TargetingTactic) {
        if value.0 == 0 {
            self.remove_property(PropertyInt::TargetingTactic);
        } else {
            let current = self.targeting_tactic();
            self.set_property(PropertyInt::TargetingTactic, current.0.cs_cast());
        }
    }

    // ACE: Creature.VisualAwarenessRange
    pub fn visual_awareness_range(&self) -> Option<f64> {
        self.get_property(PropertyFloat::VisualAwarenessRange)
    }

    // ACE: Creature.VisualAwarenessRange
    pub fn set_visual_awareness_range(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::VisualAwarenessRange),
            Some(v) => self.set_property(PropertyFloat::VisualAwarenessRange, v),
        }
    }

    // ACE: Creature.AuralAwarenessRange
    pub fn aural_awareness_range(&self) -> Option<f64> {
        self.get_property(PropertyFloat::AuralAwarenessRange)
    }

    // ACE: Creature.AuralAwarenessRange
    pub fn set_aural_awareness_range(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::AuralAwarenessRange),
            Some(v) => self.set_property(PropertyFloat::AuralAwarenessRange, v),
        }
    }
}
