// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/HeldItem.cs
//! Port of `Source/ACE.Server/Entity/HeldItem.cs`.

use empyrean_entity::enums::EquipMask;

// ACE: HeldItem
/// This Class is used to add children.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeldItem {
    // ACE: HeldItem.Guid
    pub guid: u32,
    /// The `ParentLocation`, as an `int`.
    // ACE: HeldItem.LocationId
    pub location_id: i32,
    // ACE: HeldItem.EquipMask
    pub equip_mask: EquipMask,
}

impl HeldItem {
    // ACE: HeldItem.HeldItem
    #[must_use]
    pub fn new(guid: u32, location_id: i32, equipmask: EquipMask) -> Self {
        Self {
            guid,
            location_id,
            equip_mask: equipmask,
        }
    }
}
