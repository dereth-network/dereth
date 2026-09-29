// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Portal_Properties.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Portal_Properties.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/Portal_Properties.cs`.

use empyrean_entity::enums::{
    PortalBitmask, PropertyBool, PropertyDataId, PropertyInt, PropertyString, SubscriptionStatus,
};

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: Portal.PortalRestrictions
    pub fn portal_restrictions(&self) -> PortalBitmask {
        PortalBitmask(
            self.get_property(PropertyInt::PortalBitmask)
                .unwrap_or(PortalBitmask::Unrestricted.0),
        )
    }

    // ACE: Portal.PortalRestrictions
    pub fn set_portal_restrictions(&mut self, value: PortalBitmask) {
        if value == PortalBitmask::Undef {
            self.remove_property(PropertyInt::PortalBitmask);
        } else {
            self.set_property(PropertyInt::PortalBitmask, value.0);
        }
    }

    // ACE: Portal.OriginalPortal
    pub fn original_portal(&self) -> Option<u32> {
        self.get_property(PropertyDataId::OriginalPortal)
    }

    // ACE: Portal.OriginalPortal
    pub fn set_original_portal(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::OriginalPortal),
            Some(v) => self.set_property(PropertyDataId::OriginalPortal, v),
        }
    }

    // ACE: Portal.PortalShowDestination
    pub fn portal_show_destination(&self) -> Option<bool> {
        self.get_property(PropertyBool::PortalShowDestination)
    }

    // ACE: Portal.PortalShowDestination
    pub fn set_portal_show_destination(&mut self, value: Option<bool>) {
        match value {
            None => self.remove_property(PropertyBool::PortalShowDestination),
            Some(v) => self.set_property(PropertyBool::PortalShowDestination, v),
        }
    }

    // ACE: Portal.AppraisalPortalDestination
    pub fn appraisal_portal_destination(&self) -> Option<String> {
        self.get_property(PropertyString::AppraisalPortalDestination)
    }

    // ACE: Portal.AppraisalPortalDestination
    pub fn set_appraisal_portal_destination(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::AppraisalPortalDestination),
            Some(v) => self.set_property(PropertyString::AppraisalPortalDestination, v),
        }
    }

    // ACE: Portal.PortalIgnoresPkAttackTimer
    pub fn portal_ignores_pk_attack_timer(&self) -> bool {
        self.get_property(PropertyBool::PortalIgnoresPkAttackTimer)
            .unwrap_or(false)
    }

    // ACE: Portal.PortalIgnoresPkAttackTimer
    pub fn set_portal_ignores_pk_attack_timer(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::PortalIgnoresPkAttackTimer);
        } else {
            self.set_property(PropertyBool::PortalIgnoresPkAttackTimer, value);
        }
    }

    // ACE: Portal.AccountRequirements
    pub fn account_requirements_portal(&self) -> Option<SubscriptionStatus> {
        self.get_property(PropertyInt::AccountRequirements)
            .map(SubscriptionStatus)
    }

    // ACE: Portal.AccountRequirements
    pub fn set_account_requirements_portal(&mut self, value: Option<SubscriptionStatus>) {
        match value {
            None => self.remove_property(PropertyInt::AccountRequirements),
            Some(v) => self.set_property(PropertyInt::AccountRequirements, v.0),
        }
    }

    // ACE: Portal.AdvocateQuest
    pub fn advocate_quest_portal(&self) -> Option<bool> {
        self.get_property(PropertyBool::AdvocateQuest)
    }

    // ACE: Portal.AdvocateQuest
    pub fn set_advocate_quest_portal(&mut self, value: Option<bool>) {
        match value {
            None => self.remove_property(PropertyBool::AdvocateQuest),
            Some(v) => self.set_property(PropertyBool::AdvocateQuest, v),
        }
    }
}
