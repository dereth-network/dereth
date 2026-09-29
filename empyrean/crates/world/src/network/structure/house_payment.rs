// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/HousePayment.cs
//! Port of `Source/ACE.Server/Network/Structure/HousePayment.cs`.

use empyrean_common::extensions::string_extensions::pluralize;
use empyrean_entity::enums::{ItemType, PropertyString};
use empyrean_entity::{ObjectGuid, Weenie};
use std::sync::Arc;

use crate::entity::world_object_info::WorldObjectInfoOf;
use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::write_record;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::world_object_networking::shims;
use crate::World;

// ACE: HousePayment
/// Contains information about a house purchase or maintenance item.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HousePayment {
    // ACE: HousePayment.Num
    /// The quantity required
    pub num: i32,
    // ACE: HousePayment.Paid
    /// The quantity paid
    pub paid: i32,
    // ACE: HousePayment.WeenieID
    /// The item weenie class ID
    pub weenie_id: u32,
    // ACE: HousePayment.Name
    /// The item name
    pub name: Option<String>,
    // ACE: HousePayment.PluralName
    /// The pluralized item name (if not specified, use `<Name>` followed by 's' or 'es')
    pub plural_name: Option<String>,
}

impl HousePayment {
    // ACE: HousePayment.Remaining
    /// The quantity remaining to be paid for this item.
    #[must_use]
    pub fn remaining(&self) -> i32 {
        self.num.wrapping_sub(self.paid).max(0)
    }

    // ACE: HousePayment.Init
    /// `Init(Weenie weenie, ushort stackSize = 1)`: a fresh object of the weenie's class with
    /// the stack size, read by [`HousePayment::init_world_object`]; then `WeenieID` is the
    /// weenie's. The object is never added to the world (its dynamic guid is spent, as in ACE).
    pub fn init_weenie(&mut self, w: &mut World, weenie: &Arc<Weenie>, stack_size: u16) {
        let mut wo = shims::world_object_factory_create_new_world_object_by_name(
            w,
            weenie.class_name.as_deref().unwrap_or(""),
        )
        .expect("ACE: CreateNewWorldObject returned null (NullReferenceException)");
        shims::set_stack_size(&mut wo, Some(i32::from(stack_size)));
        self.weenie_id = weenie.weenie_class_id;
        self.init_object(&wo);
    }

    // ACE: HousePayment.Init
    /// `Init(WorldObject wo)` for an object in the world.
    pub fn init_world_object(&mut self, w: &World, wo: ObjectGuid) {
        let o = w.objects.get(wo).expect("ACE: wo is null");
        self.weenie_id = o.biota.weenie_class_id;
        self.name = crate::dispatch::name::name(w, wo);
        self.plural_name = Some(crate::world_objects::world_object::get_plural_name(w, wo));
        self.num = o.stack_size().unwrap_or(1);
    }

    /// `Init(WorldObject wo)` for an object that is not in the world (the fresh one of
    /// [`HousePayment::init_weenie`], or a slumlord's create-list object; never a player, so
    /// `Name` is the property).
    pub fn init_object(&mut self, o: &WorldObject) {
        self.weenie_id = o.biota.weenie_class_id;
        self.name = o.get_property(PropertyString::Name);
        self.plural_name = Some(match o.plural_name() {
            Some(p) => p,
            None => pluralize(
                self.name
                    .as_deref()
                    .expect("System.NullReferenceException: Name is null"),
            ),
        });
        self.num = o.stack_size().unwrap_or(1);
    }

    // ACE: HousePayment.GetConsumeItems
    /// The amount of items to consume for this payment item: the matching items, smallest stacks
    /// first; for pyreals (wcid 273) followed by the trade notes, smallest denomination first.
    #[must_use]
    pub fn get_consume_items(
        &self,
        w: &World,
        items: &[ObjectGuid],
    ) -> Vec<WorldObjectInfoOf<i32>> {
        if self.remaining() == 0 {
            return Vec::new();
        }

        let get = |g: &ObjectGuid| w.objects.get(*g).expect("ACE: null item");

        // filter to payment tab items that apply to this HousePayment wcid
        // consume from smallest stacks first, to clear out the clutter
        let mut wcid_items: Vec<ObjectGuid> = items
            .iter()
            .copied()
            .filter(|i| get(i).biota.weenie_class_id == self.weenie_id)
            .collect();
        wcid_items.sort_by_key(|i| get(i).stack_size().unwrap_or(1));

        // house monetary payments have pyreal wcid, and can be also be paid with trade notes
        if self.weenie_id == 273 {
            // append trade notes
            // consume from smallest denomination stacks first, with multiple stacks of the same denomination sorted by stack size

            // note that this does not produce an optimal solution for minimizing the amount of trade notes consumed

            // a slightly better solution would be to iteratively consume the highest denomination trade note that is <= the remaining amount,
            // but there are still some sets where even that wouldn't be optimized..

            let mut trade_notes: Vec<ObjectGuid> = items
                .iter()
                .copied()
                .filter(|i| get(i).item_type() == ItemType::PromissoryNote)
                .collect();
            // `.OrderBy(StackSize ?? 1).OrderBy(StackUnitValue)`: the second stable sort wins, ties
            // keep the first's order. `int?` keys: null sorts first.
            trade_notes.sort_by_key(|i| get(i).stack_size().unwrap_or(1));
            trade_notes.sort_by_key(|i| get(i).stack_unit_value());

            let coins_and_trade_notes: Vec<ObjectGuid> =
                wcid_items.into_iter().chain(trade_notes).collect();

            self.get_consume_items_inner(w, &coins_and_trade_notes)
        } else {
            self.get_consume_items_inner(w, &wcid_items)
        }
    }

    // ACE: HousePayment.GetConsumeItems_Inner
    /// The amount of each item to consume, in order, until the remaining amount is covered.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    pub fn get_consume_items_inner(
        &self,
        w: &World,
        items: &[ObjectGuid],
    ) -> Vec<WorldObjectInfoOf<i32>> {
        let mut consume_items = Vec::new();

        let mut remaining = self.remaining();

        for item in items {
            let o = w.objects.get(*item).expect("ACE: null item");
            let stack_size = o.stack_size().unwrap_or(1);

            let stack_value = o.value().unwrap_or(0);
            let base_value = o.stack_unit_value().unwrap_or(0);

            let amount = if o.is_trade_note() {
                stack_value
            } else {
                stack_size
            };

            if amount <= remaining {
                consume_items.push(WorldObjectInfoOf::with_value(w, *item, stack_size));
                remaining = remaining.wrapping_sub(amount);
            } else {
                let consume_amount = if o.is_trade_note() {
                    // `(int)Math.Ceiling((float)remaining / baseValue)`
                    empyrean_common::dotnet::CsCast::<i32>::cs_cast(
                        f64::from((remaining as f32) / (base_value as f32)).ceil(),
                    )
                } else {
                    remaining
                };

                consume_items.push(WorldObjectInfoOf::with_value(w, *item, consume_amount));
                remaining = 0;
            }

            if remaining <= 0 {
                break;
            }
        }

        consume_items
    }
}

// ACE: HousePayment.HousePayment
/// `new HousePayment(string weenieName, ushort stackSize = 1)`.
pub fn house_payment_from_weenie_name(
    w: &mut World,
    weenie_name: &str,
    stack_size: u16,
) -> HousePayment {
    let weenie = w
        .content
        .get_cached_weenie_by_class_name(weenie_name)
        .expect("ACE: weenie is null");
    let mut p = HousePayment::default();
    p.init_weenie(w, &weenie, stack_size);
    p
}

// ACE: HousePayment.HousePayment
/// `new HousePayment(Weenie weenie, ushort stackSize = 1)`.
pub fn house_payment_from_weenie(
    w: &mut World,
    weenie: &Arc<Weenie>,
    stack_size: u16,
) -> HousePayment {
    let mut p = HousePayment::default();
    p.init_weenie(w, weenie, stack_size);
    p
}

// ACE: HousePayment.HousePayment
/// `new HousePayment(WorldObject wo)`.
pub fn house_payment_from_world_object(w: &World, wo: ObjectGuid) -> HousePayment {
    let mut p = HousePayment::default();
    p.init_world_object(w, wo);
    p
}

/// `new HousePayment(WorldObject wo)` for an object outside the world (a slumlord's buy or rent
/// item: `SlumLord.GetBuyItems`).
#[must_use]
pub fn house_payment_from_detached(o: &WorldObject) -> HousePayment {
    let mut p = HousePayment::default();
    p.init_object(o);
    p
}

// ACE: HousePaymentExtensions.Write
/// `writer.Write(HousePayment payment)`.
pub fn write(writer: &mut Vec<u8>, payment: &HousePayment) {
    write_record(writer, &strings(std::slice::from_ref(payment)), |w| {
        record(payment).write(w)
    });
}

// ACE: HousePaymentExtensions.Write
/// `writer.Write(List<HousePayment> payments)`: a null list is written empty.
pub fn write_list(writer: &mut Vec<u8>, payments: &[HousePayment]) {
    let list = record_list(payments);
    write_record(writer, &strings(payments), |w| {
        w.packed_list(&list, |w, p| p.write(w))
    });
}

/// The dereth-protocol record the `Write` extension below writes, field for field.
#[must_use]
pub fn record(payment: &HousePayment) -> dereth_protocol::trade::HousePayment {
    dereth_protocol::trade::HousePayment {
        num: payment.num,
        paid: payment.paid,
        wcid: payment.weenie_id,
        name: ace_str(payment.name.as_deref()),
        plural_name: ace_str(payment.plural_name.as_deref()),
    }
}

/// [`record`] of each payment (a null list is written empty).
#[must_use]
pub fn record_list(payments: &[HousePayment]) -> Vec<dereth_protocol::trade::HousePayment> {
    payments.iter().map(record).collect()
}

/// The payments' strings, for `write_record`.
#[must_use]
pub fn strings(payments: &[HousePayment]) -> Vec<&str> {
    payments
        .iter()
        .flat_map(|p| {
            [
                p.name.as_deref().unwrap_or(""),
                p.plural_name.as_deref().unwrap_or(""),
            ]
        })
        .collect()
}
