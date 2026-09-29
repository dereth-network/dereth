// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Entity/HouseListResults.cs
//! `HouseListResults`: one row of `WorldDatabase.GetHousesAll` (a slum-lord weenie and one of its
//! landblock instances).

use empyrean_entity::enums::HouseType;

use crate::models::world::{LandblockInstance, Weenie};

// ACE: HouseListResults
#[derive(Debug, Clone)]
pub struct HouseListResults {
    pub weenie: Weenie,
    pub landblock_instance: LandblockInstance,
    pub house_type: HouseType,
}

impl HouseListResults {
    // ACE: HouseListResults.HouseListResults
    #[must_use]
    pub fn new(weenie: Weenie, landblock_instance: LandblockInstance) -> Self {
        let house_type = Self::get_house_type(&weenie.class_name);
        Self {
            weenie,
            landblock_instance,
            house_type,
        }
    }

    /// `IndexOf(.., StringComparison.OrdinalIgnoreCase) != -1`. The needles are ASCII, so an ASCII
    /// case fold is the same test.
    // ACE: HouseListResults.GetHouseType
    #[must_use]
    pub fn get_house_type(classname: &str) -> HouseType {
        let lower = classname.to_ascii_lowercase();
        if lower.contains("apartment") {
            HouseType::Apartment
        } else if lower.contains("cottage") {
            HouseType::Cottage
        } else if lower.contains("villa") {
            HouseType::Villa
        } else if lower.contains("mansion") {
            HouseType::Mansion
        } else {
            HouseType::Undef
        }
    }
}
