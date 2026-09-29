// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Portal_Properties.cs
//! Port of `Source/ACE.Server/WorldObjects/Portal_Properties.cs`.

/// Non-property fields declared in `Portal_Properties.cs`.
#[derive(Debug, Default)]
pub struct PortalPropertiesFields {}

impl crate::world_objects::world_object::WorldObject {
    // ACE: Portal.NoRecall
    /// `(PortalRestrictions & PortalBitmask.NoRecall) != 0`.
    #[must_use]
    pub fn no_recall(&self) -> bool {
        (self.portal_restrictions().0 & empyrean_entity::enums::PortalBitmask::NoRecall.0) != 0
    }

    // ACE: Portal.NoTie
    /// `NoRecall`.
    #[must_use]
    pub fn no_tie(&self) -> bool {
        self.no_recall()
    }

    // ACE: Portal.NoSummon
    /// `(PortalRestrictions & PortalBitmask.NoSummon) != 0`.
    #[must_use]
    pub fn no_summon(&self) -> bool {
        (self.portal_restrictions().0 & empyrean_entity::enums::PortalBitmask::NoSummon.0) != 0
    }

    // ACE: Portal.SocietyId
    /// Always 0 in ACE.
    #[must_use]
    pub fn society_id(&self) -> i32 {
        0
    }
}
