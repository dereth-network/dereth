//! Ordered offered objects and working payment totals for one housing session.

use super::{HouseOp, HouseProfile};
use crate::World;
use dereth_client_contract::panels::slumlord::{PaymentAction, PaymentItem, PaymentListsView};
use dereth_client_contract::view::SlumlordPayment;
use dereth_primitives::ObjectId;

#[derive(Debug, Clone, Default)]
pub struct PaymentLists {
    pub original: Option<(ObjectId, HouseProfile)>,
    pub current: Option<HouseProfile>,
    state: PaymentListsView,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaymentEffect {
    Notice(String, dereth_client_contract::feedback::Feedback),
    Split {
        item: ObjectId,
        split: u32,
        max: u32,
    },
    Submit {
        slumlord: ObjectId,
        rent: bool,
        items: Vec<ObjectId>,
    },
}

impl PaymentLists {
    pub fn receive(&mut self, slumlord: ObjectId, profile: HouseProfile) {
        let op = if profile.owner.0 == 0 {
            HouseOp::Buy
        } else {
            HouseOp::Rent
        };
        self.original = Some((slumlord, profile.clone()));
        self.current = Some(profile);
        self.state = PaymentListsView {
            op,
            visible: true,
            ..Default::default()
        };
        self.refresh();
    }

    pub fn view(&self) -> PaymentListsView {
        self.state.clone()
    }

    pub fn select(&mut self, op: HouseOp) {
        self.state.op = op;
    }

    fn items(&self) -> &[PaymentItem] {
        match self.state.op {
            HouseOp::Buy => &self.state.buy,
            HouseOp::Rent => &self.state.rent,
            HouseOp::Undef => &[],
        }
    }

    pub fn allowed(&self) -> bool {
        self.current.as_ref().is_some_and(|p| match self.state.op {
            HouseOp::Buy => p.owner.0 == 0,
            HouseOp::Rent => p.owner.0 != 0,
            HouseOp::Undef => false,
        })
    }

    pub fn wants(&self, wcid: u32, note: Option<i32>) -> bool {
        self.allowed()
            && self
                .current
                .as_ref()
                .is_some_and(|p| p.needs_more(self.state.op, wcid, note) != 0)
    }

    pub fn add(&mut self, item: PaymentItem) -> bool {
        if self.items().iter().any(|i| i.id == item.id)
            || !self.wants(item.wcid, item.trade_note_value)
        {
            return false;
        }
        if !self
            .current
            .as_mut()
            .is_some_and(|p| p.pay(self.state.op, item.wcid, item.amount, item.trade_note_value))
        {
            return false;
        }
        match self.state.op {
            HouseOp::Buy => self.state.buy.push(item),
            HouseOp::Rent => self.state.rent.push(item),
            HouseOp::Undef => return false,
        }
        self.refresh();
        true
    }

    pub fn remove(&mut self, id: ObjectId) {
        match self.state.op {
            HouseOp::Buy => self.state.buy.retain(|i| i.id != id),
            HouseOp::Rent => self.state.rent.retain(|i| i.id != id),
            HouseOp::Undef => return,
        }
        self.recompute();
    }

    /// An authoritative inventory departure removes an offered object from either list.
    pub fn remove_unowned(&mut self, id: ObjectId) {
        self.state.buy.retain(|i| i.id != id);
        self.state.rent.retain(|i| i.id != id);
        self.recompute();
    }

    pub fn clear(&mut self) {
        self.state.buy.clear();
        self.state.rent.clear();
        self.recompute();
    }

    pub fn close(&mut self) {
        self.clear();
        self.state.visible = false;
    }

    pub fn submit(&mut self) -> Option<PaymentEffect> {
        if !self.allowed() || self.items().is_empty() {
            return None;
        }
        if self.state.op == HouseOp::Buy && !self.state.buy_payment.paid_in_full {
            return None;
        }
        let slumlord = self.original.as_ref()?.0;
        if slumlord.0 == 0 {
            return None;
        }
        let effect = PaymentEffect::Submit {
            slumlord,
            rent: self.state.op.is_rent(),
            items: self.items().iter().map(|i| i.id).collect(),
        };
        self.clear();
        Some(effect)
    }

    fn recompute(&mut self) {
        self.current = self.original.as_ref().map(|(_, p)| p.clone());
        if let Some(p) = self.current.as_mut() {
            for (op, rows) in [
                (HouseOp::Buy, &self.state.buy),
                (HouseOp::Rent, &self.state.rent),
            ] {
                for i in rows {
                    p.pay(op, i.wcid, i.amount, i.trade_note_value);
                }
            }
        }
        self.refresh();
    }

    fn refresh(&mut self) {
        let payment = |op| {
            self.current
                .as_ref()
                .map_or_else(SlumlordPayment::default, |p| SlumlordPayment {
                    requirements: if op == HouseOp::Rent {
                        p.compose_text2(op)
                    } else {
                        p.compose_text(op)
                    },
                    paid_in_full: p.op_is_paid_in_full(op),
                })
        };
        self.state.buy_payment = payment(HouseOp::Buy);
        self.state.rent_payment = payment(HouseOp::Rent);
    }
}

impl World {
    pub fn payment_lists_view(&self) -> PaymentListsView {
        self.payments.view()
    }

    pub fn payment_action(
        &mut self,
        action: PaymentAction,
        note_value: impl Fn(u32) -> Option<i32>,
    ) -> Vec<PaymentEffect> {
        let mut list = std::mem::take(&mut self.payments);
        let mut effects = Vec::new();
        match action {
            PaymentAction::Select(op) => list.select(op),
            PaymentAction::Remove(id) => list.remove(id),
            PaymentAction::Clear => list.clear(),
            PaymentAction::Close => list.close(),
            PaymentAction::Submit => effects.extend(list.submit()),
            PaymentAction::Add(id) => {
                if !self.is_owned_by_player(id) {
                    if self.weenie(id).is_some() && list.state.op != HouseOp::Undef {
                        effects.push(PaymentEffect::Notice(
                            "You can only trade items you are carrying".into(),
                            dereth_client_contract::feedback::Feedback::WARNING,
                        ));
                    }
                } else if list.allowed() {
                    if self.split.split_size != self.split.max_split_size {
                        if self
                            .weenie(id)
                            .is_some_and(|w| list.wants(w.pwd.wcid, note_value(w.pwd.wcid)))
                            && !list.items().iter().any(|i| i.id == id)
                        {
                            effects.push(PaymentEffect::Split {
                                item: id,
                                split: self.split.split_size,
                                max: self.split.max_split_size,
                            });
                        }
                    } else {
                        self.offer_payment_tree(&mut list, id, &note_value, &mut Vec::new());
                    }
                }
            }
        }
        self.payments = list;
        effects
    }

    fn offer_payment_tree(
        &self,
        list: &mut PaymentLists,
        id: ObjectId,
        note_value: &impl Fn(u32) -> Option<i32>,
        seen: &mut Vec<ObjectId>,
    ) {
        if seen.contains(&id) || !self.is_owned_by_player(id) {
            return;
        }
        seen.push(id);
        let Some(w) = self.weenie(id) else { return };
        let contents = self.inventory(id).map_or(&[][..], |i| i.items.as_slice());
        if !contents.is_empty() {
            for &child in contents {
                self.offer_payment_tree(list, child, note_value, seen);
            }
        } else {
            list.add(PaymentItem {
                id,
                wcid: w.pwd.wcid,
                amount: w.pwd.stack_size.map_or(1, |n| i32::from(n.max(1))),
                trade_note_value: note_value(w.pwd.wcid),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::housing::{HousePayment, HousePaymentList};

    fn list(rent: bool) -> PaymentLists {
        let mut list = PaymentLists::default();
        let price = HousePaymentList(vec![HousePayment {
            wcid: 273,
            num: 30,
            ..Default::default()
        }]);
        list.receive(
            ObjectId(9),
            HouseProfile {
                owner: ObjectId(u32::from(rent)),
                buy: price.clone(),
                rent: price,
                ..Default::default()
            },
        );
        list
    }
    fn item(id: u32, amount: i32) -> PaymentItem {
        PaymentItem {
            id: ObjectId(id),
            wcid: 273,
            amount,
            trade_note_value: None,
        }
    }

    /// Behaviour: panels.house-purchase.confirmations-use-current-payment-and-handle-every-answer
    #[test]
    fn ownership_departure_removes_both_lists_without_reordering_survivors() {
        let mut list = list(false);
        list.state.buy = vec![item(7, 10), item(2, 10), item(6, 10)];
        list.state.rent = vec![item(6, 10), item(2, 10), item(7, 10)];
        list.recompute();
        list.remove_unowned(ObjectId(2));
        assert_eq!(
            list.view().buy.iter().map(|i| i.id).collect::<Vec<_>>(),
            vec![ObjectId(7), ObjectId(6)]
        );
        assert_eq!(
            list.view().rent.iter().map(|i| i.id).collect::<Vec<_>>(),
            vec![ObjectId(6), ObjectId(7)]
        );
        assert_eq!(list.current.as_ref().unwrap().buy.0[0].paid, 20);
        assert_eq!(list.current.as_ref().unwrap().rent.0[0].paid, 20);
    }

    /// Behaviour: panels.house-purchase.paying-sends-buy-house-with-the-windows-items
    #[test]
    fn partial_drop_splits_without_offering_and_whole_drop_uses_shared_state() {
        let mut world = World::new();
        world.player = Some(ObjectId(1));
        world.payments = list(false);
        let mut coin = crate::Weenie::new(ObjectId(2));
        coin.valid = true;
        coin.pwd.container_id = world.player;
        coin.pwd.wcid = 273;
        coin.pwd.stack_size = Some(30);
        world.tables.weenies.insert(coin.id, coin);
        world.split = crate::inventory::SplitState {
            split_size: 10,
            max_split_size: 30,
        };
        assert_eq!(
            world.payment_action(PaymentAction::Add(ObjectId(2)), |_| None),
            vec![PaymentEffect::Split {
                item: ObjectId(2),
                split: 10,
                max: 30
            }]
        );
        assert!(world.payment_lists_view().buy.is_empty());
        world.split.split_size = 30;
        assert!(world
            .payment_action(PaymentAction::Add(ObjectId(2)), |_| None)
            .is_empty());
        assert_eq!(world.payment_lists_view().buy, vec![item(2, 30)]);
        assert_eq!(
            world.payment_action(PaymentAction::Submit, |_| None),
            vec![PaymentEffect::Submit {
                slumlord: ObjectId(9),
                rent: false,
                items: vec![ObjectId(2)],
            }]
        );
    }

    /// Behaviour: panels.house-purchase.paying-sends-buy-house-with-the-windows-items
    #[test]
    fn offered_rows_preserve_append_order_and_submit_clears_both_lists() {
        for rent in [false, true] {
            let mut list = list(rent);
            assert!(list.add(item(7, 10)));
            assert!(list.add(item(2, 10)));
            assert!(!list.add(item(7, 10)));
            assert!(list.add(item(6, 10)));
            assert_eq!(
                list.submit(),
                Some(PaymentEffect::Submit {
                    slumlord: ObjectId(9),
                    rent,
                    items: vec![ObjectId(7), ObjectId(2), ObjectId(6)]
                })
            );
            assert!(list.view().buy.is_empty() && list.view().rent.is_empty());
            assert!(list.submit().is_none());
        }
    }

    /// Behaviour: panels.house-purchase.each-profile-opens-the-payment-window-once
    #[test]
    fn identical_receipt_reopens_and_resets_but_viewing_does_not() {
        let mut list = list(false);
        let (id, profile) = list.original.clone().unwrap();
        list.add(item(7, 10));
        list.close();
        for _ in 0..3 {
            assert!(!list.view().visible);
        }
        list.add(item(8, 10));
        list.receive(id, profile);
        assert!(list.view().visible);
        assert!(list.view().buy.is_empty());
    }

    /// Behaviour: panels.house-purchase.confirmations-use-current-payment-and-handle-every-answer
    #[test]
    fn removing_an_offered_item_recomputes_from_original_before_submit() {
        let mut list = list(false);
        list.add(item(7, 10));
        list.add(item(2, 20));
        assert!(list.view().buy_payment.paid_in_full);
        list.remove_unowned(ObjectId(2));
        assert!(!list.view().buy_payment.paid_in_full);
        assert!(list.submit().is_none());
        assert_eq!(list.current.as_ref().unwrap().buy.0[0].paid, 10);
        assert_eq!(list.original.as_ref().unwrap().1.buy.0[0].paid, 0);
    }

    /// Behaviour: panels.house-purchase.each-payment-is-paid-in-full-by-its-own-price
    #[test]
    fn each_payment_is_paid_in_full_only_by_its_own_price() {
        for rent in [false, true] {
            // Both prices are thirty Pyreals; only the open tab's list is paid.
            let mut list = list(rent);
            assert!(list.add(item(7, 30)));
            let view = list.view();
            assert_eq!(
                view.buy_payment.paid_in_full, !rent,
                "the purchase follows the purchase price (rent paid: {rent})"
            );
            assert_eq!(
                view.rent_payment.paid_in_full, rent,
                "the maintenance follows the rent (rent paid: {rent})"
            );
        }
    }
}
