// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Extensions/WorldDbExtensions.cs
//! `Clone()` for the three treasure-material rows the cache normalizes in place.

use crate::models::world::{TreasureMaterialBase, TreasureMaterialColor, TreasureMaterialGroups};

// ACE: WorldDbExtensions
impl TreasureMaterialBase {
    // ACE: WorldDbExtensions.Clone
    #[must_use]
    pub fn ace_clone(&self) -> TreasureMaterialBase {
        TreasureMaterialBase {
            id: self.id,
            material_code: self.material_code,
            material_id: self.material_id,
            probability: self.probability,
            tier: self.tier,
        }
    }
}

impl TreasureMaterialGroups {
    // ACE: WorldDbExtensions.Clone
    #[must_use]
    pub fn ace_clone(&self) -> TreasureMaterialGroups {
        TreasureMaterialGroups {
            id: self.id,
            material_group: self.material_group,
            material_id: self.material_id,
            probability: self.probability,
            tier: self.tier,
        }
    }
}

impl TreasureMaterialColor {
    // ACE: WorldDbExtensions.Clone
    #[must_use]
    pub fn ace_clone(&self) -> TreasureMaterialColor {
        TreasureMaterialColor {
            color_code: self.color_code,
            id: self.id,
            material_id: self.material_id,
            palette_template: self.palette_template,
            probability: self.probability,
        }
    }
}
