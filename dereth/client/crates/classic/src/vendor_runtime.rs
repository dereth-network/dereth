//! The classic vendor panel's totals and its Sell All button, which sells every object in the
//! sell basket in whole stacks.
use dereth_client_model::{NoticeSink, RequestSink, World};
use dereth_primitives::ServerTime;

/// The visible sell basket records one offered object per row. At submission,
/// rebuild amounts from current stacks; Sell Single keeps its literal one.
pub fn sell_all(
    world: &mut World,
    requests: &mut dyn RequestSink,
    notices: &mut dyn NoticeSink,
    now: ServerTime,
) -> bool {
    if world.shop.vendor_id.is_none() || world.shop.sell_list.is_empty() {
        return false;
    }
    let Some(rows) = world
        .shop
        .sell_list
        .iter()
        .map(|(id, _)| {
            world
                .weenie(*id)
                .map(|item| (*id, i32::from(item.pwd.stack_size.unwrap_or(1).max(1))))
        })
        .collect::<Option<Vec<_>>>()
    else {
        // An expired object cannot supply its current quantity. Keep the draft
        // intact rather than fabricate an amount for an absent object.
        return false;
    };
    world.shop.sell_list = rows;
    world.sell_all(requests, notices, now)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    use crate::int::u32_from;
    use dereth_client_model::{
        inventory::{requests::InventoryRequest, SplitState},
        weenie::Weenie,
        RecordingRequests, RecordingSink, Request,
    };
    use dereth_primitives::ObjectId;

    fn offered(stacks: &[Option<u16>]) -> World {
        let mut world = World::default();
        world.shop.vendor_id = Some(ObjectId(0x12345678));
        for (i, stack) in stacks.iter().enumerate() {
            let id = ObjectId(0x100 + u32_from(i));
            let mut item = Weenie::new(id);
            item.pwd.stack_size = *stack;
            item.sell_state = 1;
            world.tables.weenies.insert(id, item);
            world.shop.sell_list.push((id, 1));
        }
        world
    }

    #[test]
    fn sell_all_encodes_live_stacks_and_clears_marks_after_submission() {
        let mut world = offered(&[Some(12), Some(0), None]);
        world.weenie_mut(ObjectId(0x100)).unwrap().pwd.stack_size = Some(7);
        let mut requests = RecordingRequests::default();
        assert!(sell_all(
            &mut world,
            &mut requests,
            &mut RecordingSink::default(),
            ServerTime(4.0)
        ));
        let [Request::VendorSell(message)] = requests.0.as_slice() else {
            panic!("one Vendor_Sell request expected");
        };
        let expected: Vec<u8> = [0x60u32, 0x12345678, 3, 7, 0x100, 1, 0x101, 1, 0x102]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        assert_eq!(dereth_protocol::write_blob(message).unwrap(), expected);
        assert!(world.shop.sell_list.is_empty());
        for id in 0x100..=0x102 {
            assert_eq!(world.weenie(ObjectId(id)).unwrap().sell_state, 0);
        }
        assert_eq!(world.request_lock.pending, InventoryRequest::ShopEvent);
        assert_eq!(world.request_lock.object, Some(ObjectId(0x12345678)));
        assert_eq!(world.request_lock.at, ServerTime(4.0));
    }

    #[test]
    fn single_sale_retains_literal_one_for_a_whole_stack() {
        let mut world = offered(&[Some(12)]);
        let mut requests = RecordingRequests::default();
        assert_eq!(
            world.sell_single_item(
                ObjectId(0x100),
                SplitState::whole_stack(12),
                &mut requests,
                &mut RecordingSink::default(),
                ServerTime(4.0)
            ),
            Ok(true)
        );
        let [Request::VendorSell(message)] = requests.0.as_slice() else {
            panic!("one Vendor_Sell request expected");
        };
        assert_eq!(message.items[0].amount, 1);
        assert_eq!(
            world.weenie(ObjectId(0x100)).unwrap().pwd.stack_size,
            Some(12)
        );
    }

    #[test]
    fn no_vendor_or_expired_object_preserves_the_unsent_draft() {
        for remove_vendor in [false, true] {
            let mut world = offered(&[Some(12)]);
            if remove_vendor {
                world.shop.vendor_id = None;
            } else {
                world.tables.weenies.remove(ObjectId(0x100));
            }
            let mut requests = RecordingRequests::default();
            assert!(!sell_all(
                &mut world,
                &mut requests,
                &mut RecordingSink::default(),
                ServerTime(4.0)
            ));
            assert!(requests.0.is_empty());
            assert_eq!(world.shop.sell_list, vec![(ObjectId(0x100), 1)]);
            assert!(world.request_lock.is_idle());
        }
    }
}
