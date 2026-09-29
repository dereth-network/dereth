// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/HouseData.cs
//! Port of `Source/ACE.Server/Network/Structure/HouseData.cs`.

use empyrean_entity::enums::{HouseStatus, HouseType};
use empyrean_entity::{ObjectGuid, Position};

use super::house_payment::{self, HousePayment};
use crate::network::game_messages::game_message::write_record;
use crate::world_objects::world_object_networking::shims;
use crate::World;

// ACE: HouseData
/// Set of information related to owning a house.
#[derive(Debug, Clone, Default)]
pub struct HouseData {
    // ACE: HouseData.BuyTime
    /// When the house was purchased (Unix timestmap)
    pub buy_time: u32,
    // ACE: HouseData.RentTime
    /// When the current maintenance period began (Unix timestmap)
    pub rent_time: u32,
    // ACE: HouseData.Type
    /// The type of house (1=cottage, 2=villa, 3=mansion, 4=apartment)
    pub r#type: HouseType,
    // ACE: HouseData.MaintenanceFree
    /// Indicates maintenance is free this period, admin flag
    pub maintenance_free: bool,
    // ACE: HouseData.Buy
    /// The list of items required for purchasing a house
    pub buy: Vec<HousePayment>,
    // ACE: HouseData.Rent
    /// The list of items required for paying rent on a house
    pub rent: Vec<HousePayment>,
    // ACE: HouseData.Position
    /// House location (`null` until set: written by the `Position` writer, so a null throws).
    pub position: Option<Position>,
}

// ACE: HouseData.HouseData
/// `new HouseData()`: maintenance is free when `house_rent_enabled` is off.
pub fn house_data_new(w: &World) -> HouseData {
    let mut d = HouseData::default();

    if !shims::property_manager_get_bool(w, "house_rent_enabled", true) {
        d.maintenance_free = true;
    }
    d
}

impl HouseData {
    // ACE: HouseData.SetBuyItems
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

    // ACE: HouseData.SetRentItems
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

    // ACE: HouseData.SetPaidItems
    /// Sets the items that have already been paid for rent.
    pub fn set_paid_items(&mut self, w: &World, slumlord: ObjectGuid) {
        set_paid_items(w, slumlord, &mut self.buy, &mut self.rent);
    }
}

/// `HouseData.SetPaidItems` / `HouseProfile.SetPaidItems` (identical bodies in ACE).
pub(crate) fn set_paid_items(
    w: &World,
    slumlord: ObjectGuid,
    buy: &mut [HousePayment],
    rent: &mut [HousePayment],
) {
    let house = crate::world_objects::slum_lord::house(w, slumlord).and_then(|h| w.objects.get(h));

    if house.is_some_and(|h| h.house_owner().is_some()) {
        for item in buy.iter_mut() {
            item.paid = item.num;
        }
    }

    if house.is_some_and(|h| h.house_status() == HouseStatus::InActive) {
        for item in rent.iter_mut() {
            item.paid = item.num;
        }
        return;
    }

    for item in crate::world_objects::container::inventory_values(w, slumlord) {
        let o = w.objects.get(item).expect("ACE: null inventory item");
        let mut wcid = o.biota.weenie_class_id;
        let mut value = o.stack_size().unwrap_or(1);
        if shims::weenie_class_name(w, wcid).starts_with("tradenote") {
            wcid = 273;
            value = o
                .value()
                .expect("ACE: item.Value is null (InvalidOperationException)");
        }
        let Some(rent_item) = rent.iter_mut().find(|i| i.weenie_id == wcid) else {
            log::info!(
                "HouseData.SetPaidItems({}): couldn't find rent item {}",
                crate::dispatch::name::name(w, slumlord).unwrap_or_default(),
                o.biota.weenie_class_id
            );
            continue;
        };
        rent_item.paid = rent_item.num.min(rent_item.paid.wrapping_add(value));
    }
}

// ACE: HouseDataExtensions.Write
/// `writer.Write(HouseData data)`.
pub fn write(writer: &mut Vec<u8>, data: &HouseData) {
    let mut strings = house_payment::strings(&data.buy);
    strings.extend(house_payment::strings(&data.rent));
    let record = record(data);
    write_record(writer, &strings, |w| {
        dereth_protocol::Message::write(&record, w)
    });
}

/// The dereth-protocol record the `Write` extension below writes, field for field. (The message `HouseData` is exactly this structure.)
#[must_use]
pub fn record(data: &HouseData) -> dereth_protocol::trade::HouseDataMessage {
    let position = data
        .position
        .as_ref()
        .expect("ACE: data.Position is null (NullReferenceException)");
    dereth_protocol::trade::HouseDataMessage {
        buy_time: data.buy_time.cast_signed(),
        rent_time: data.rent_time.cast_signed(),
        house_type: data.r#type.0.cast_unsigned(),
        maintenance_free: i32::from(data.maintenance_free),
        buy: house_payment::record_list(&data.buy),
        rent: house_payment::record_list(&data.rent),
        // `writer.Write(Position)`: cell, origin, then the rotation W first.
        position: dereth_protocol::types::PositionWire {
            objcell_id: position.cell(),
            frame: dereth_protocol::types::Frame {
                origin: empyrean_entity::shared_types::wire_vec3(position.pos()),
                orientation: empyrean_entity::shared_types::wire_quat(position.rotation()),
            },
        },
    }
}
