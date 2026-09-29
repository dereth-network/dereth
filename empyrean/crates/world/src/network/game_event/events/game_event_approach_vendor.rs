// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventApproachVendor.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventApproachVendor.cs`.

use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;
use empyrean_net::SessionId;

use crate::network::game_event::game_event_message::game_event_message_with_capacity;
use crate::network::game_event::game_event_message::{session_data, session_player};
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::{BinaryWriter, GameMessage};
use crate::world_objects::vendor;
use crate::World;

// ACE: GameEventApproachVendor.GameEventApproachVendor
/// The vendor's profile and every item for sale (the default items, then the unique ones), each as
/// its packed stack size and `SerializeGameDataOnly`.
///
/// # Panics
/// As ACE throws: a vendor missing `MerchandiseItemTypes`/`MinValue`/`MaxValue`/`BuyPrice`/
/// `SellPrice` (`InvalidOperationException`), or whose alternate currency weenie is absent
/// (`NullReferenceException`).
#[must_use]
pub fn game_event_approach_vendor(
    w: &mut World,
    session: SessionId,
    vendor: ObjectGuid,
    alt_currency_spent: u32,
) -> GameMessage {
    // 5,376 is the average seen in retail pcaps, 15,272 is the max seen in retail pcaps
    let mut msg = game_event_message_with_capacity(
        GameEventType::ApproachVendor,
        GameMessageGroup::UIQueue,
        session_data(w, session),
        8192,
    );

    let v = w
        .objects
        .get(vendor)
        .expect("System.NullReferenceException: vendor");
    const NULLABLE: &str = "System.InvalidOperationException: Nullable object must have a value.";

    msg.data.write_u32(vendor.full());

    // the types of items vendor will purchase
    msg.data
        .write_u32(v.merchandise_item_types().expect(NULLABLE).cast_unsigned());
    msg.data
        .write_u32(v.merchandise_min_value().expect(NULLABLE).cast_unsigned());
    msg.data
        .write_u32(v.merchandise_max_value().expect(NULLABLE).cast_unsigned());

    msg.data
        .write_u32(u32::from(v.deal_magical_items().unwrap_or(false)));

    #[allow(clippy::cast_possible_truncation)] // `(float)vendor.BuyPrice`
    msg.data.write_f32(v.buy_price().expect(NULLABLE) as f32);
    #[allow(clippy::cast_possible_truncation)] // `(float)vendor.SellPrice`
    msg.data.write_f32(v.sell_price().expect(NULLABLE) as f32);

    // the wcid of the alternate currency
    let alternate_currency = v.alternate_currency();
    msg.data.write_u32(alternate_currency.unwrap_or(0));

    // if this vendor accepts items as alternate currency, instead of pyreals
    if let Some(alternate_currency) = alternate_currency {
        let alt_currency = w
            .content
            .get_cached_weenie(alternate_currency)
            .expect("System.NullReferenceException: altCurrency");
        let plural_name = alt_currency.get_plural_name();

        // the total amount of alternate currency the player currently has
        let player = session_player(w, session);
        let alt_currency_in_inventory =
            crate::world_objects::container::get_num_inventory_items_of_wcid(
                w,
                player,
                alternate_currency,
            )
            .cast_unsigned();
        msg.data
            .write_u32(alt_currency_in_inventory.wrapping_add(alt_currency_spent));

        // the plural name of alt currency
        msg.data.write_string16l(plural_name.as_str());
    } else {
        msg.data.write_i32(0);
        msg.data.write_string16l("");
    }

    let items = vendor::for_each_item(w, vendor);
    let num_items = items.len();

    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    // `DefaultItemsForSale.Count + UniqueItemsForSale.Count`
    msg.data.write_i32(num_items as i32);

    for obj in items {
        // -1 = unlimited supply
        let o = w.objects.get(obj);
        let stack_size: i32 = o
            .and_then(|o| {
                o.wo.world_object_properties
                    .vendor_shop_create_list_stack_size
            })
            .or_else(|| o.and_then(|o| o.stack_size()))
            .unwrap_or(1);

        // packed value: (stackSize & 0xFFFFFF) | (pwdType << 24)
        // pwdType: flag indicating whether the new or old PublicWeenieDesc is used; -1 = PublicWeenieDesc, 1 = OldPublicWeenieDesc; -1 always used.
        msg.data.write_i32(stack_size & 0x00FF_FFFF | -1 << 24);

        crate::dispatch::serialize_game_data_only::serialize_game_data_only(
            w,
            obj,
            &mut msg.data,
            false,
        );
    }

    msg.data.align();
    msg
}
