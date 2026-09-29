// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/HouseProfile.cs
//! Port of `Source/ACE.Server/Network/Structure/HouseProfile.cs`.

use empyrean_entity::enums::{HouseBitfield, HouseType};
use empyrean_entity::ObjectGuid;

use super::house_data;
use super::house_payment::{self, HousePayment};
use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::write_record;
use crate::World;

// ACE: HouseProfile
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HouseProfile {
    // ACE: HouseProfile.DwellingID
    /// The house ID
    pub dwelling_id: u32,
    // ACE: HouseProfile.OwnerID
    /// The object ID of the current owner
    pub owner_id: ObjectGuid,
    // ACE: HouseProfile.Bitmask
    pub bitmask: HouseBitfield,
    // ACE: HouseProfile.MinLevel
    /// The minimum level requirement to purchase this dwelling (-1 if no requirement)
    pub min_level: i32,
    // ACE: HouseProfile.MaxLevel
    /// The maximum level requirement to purchase this dewlling (-1 if no requirement)
    pub max_level: i32,
    // ACE: HouseProfile.MinAllegRank
    /// The minimum allegiance rank requirement to purchase this dwelling (-1 if no requirement)
    pub min_alleg_rank: i32,
    // ACE: HouseProfile.MaxAllegRank
    /// The maximum allegiance rank requirement to purchase this dwelling (-1 if no requirement)
    pub max_alleg_rank: i32,
    // ACE: HouseProfile.MaintenanceFree
    /// Indicates maintenance is free this period, admin flag
    pub maintenance_free: bool,
    // ACE: HouseProfile.Type
    /// The type of dwelling (1=cottage, 2=villa, 3=mansion, 4=apartment)
    pub r#type: HouseType,
    // ACE: HouseProfile.OwnerName
    /// The name of the current owner
    pub owner_name: Option<String>,
    // ACE: HouseProfile.Buy
    /// The list of items required for purchasing a house (`null` until set: written empty)
    pub buy: Vec<HousePayment>,
    // ACE: HouseProfile.Rent
    /// The list of items required for paying rent on a house (`null` until set: written empty)
    pub rent: Vec<HousePayment>,
}

impl Default for HouseProfile {
    // ACE: HouseProfile.HouseProfile
    /// `new HouseProfile()`: no level or rank requirement, active.
    fn default() -> Self {
        HouseProfile {
            dwelling_id: 0,
            owner_id: ObjectGuid::default(),
            // set defaults
            bitmask: HouseBitfield::Active,
            min_level: -1,
            max_level: -1,
            min_alleg_rank: -1,
            max_alleg_rank: -1,
            maintenance_free: false,
            r#type: HouseType::default(),
            owner_name: None,
            buy: Vec::new(),
            rent: Vec::new(),
        }
    }
}

impl HouseProfile {
    // ACE: HouseProfile.SetBuyItems
    /// Sets the list of items required to purchase this dwelling.
    pub fn set_buy_items(
        &mut self,
        _w: &World,
        buy_items: &[crate::world_objects::world_object::WorldObject],
    ) {
        self.buy = buy_items
            .iter()
            .map(house_payment::house_payment_from_detached)
            .collect();
    }

    // ACE: HouseProfile.SetRentItems
    /// Sets the list of items required to pay rent for this dwelling.
    pub fn set_rent_items(
        &mut self,
        _w: &World,
        rent_items: &[crate::world_objects::world_object::WorldObject],
    ) {
        self.rent = rent_items
            .iter()
            .map(house_payment::house_payment_from_detached)
            .collect();
    }

    // ACE: HouseProfile.SetPaidItems
    /// Sets the items that have already been paid for rent.
    pub fn set_paid_items(&mut self, w: &World, slumlord: ObjectGuid) {
        house_data::set_paid_items(w, slumlord, &mut self.buy, &mut self.rent);
    }
}

// ACE: HouseProfileExtensions.Write
/// `writer.Write(HouseProfile profile)`.
pub fn write(writer: &mut Vec<u8>, profile: &HouseProfile) {
    let mut strings = vec![profile.owner_name.as_deref().unwrap_or("")];
    strings.extend(house_payment::strings(&profile.buy));
    strings.extend(house_payment::strings(&profile.rent));
    write_record(writer, &strings, |w| record(profile).write(w));
}

/// The dereth-protocol record the `Write` extension below writes, field for field.
#[must_use]
pub fn record(profile: &HouseProfile) -> dereth_protocol::trade::HouseProfile {
    dereth_protocol::trade::HouseProfile {
        id: dereth_primitives::ObjectId(profile.dwelling_id),
        owner: profile.owner_id.into(),
        bitmask: profile.bitmask.0.cast_unsigned(),
        min_level: profile.min_level,
        max_level: profile.max_level,
        min_alleg_rank: profile.min_alleg_rank,
        max_alleg_rank: profile.max_alleg_rank,
        maintenance_free: i32::from(profile.maintenance_free),
        house_type: profile.r#type.0.cast_unsigned(),
        name: ace_str(profile.owner_name.as_deref()),
        buy: house_payment::record_list(&profile.buy),
        rent: house_payment::record_list(&profile.rent),
    }
}
