//! The loot window: shown while a container is open, its items carried to the pack, taken all at
//! once, and closed.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_client_sdk::contract::view::{Container, Contents};
use auberean_client_sdk::Action;
use auberean_horizon::keys::Keyed;

use crate::things::{carry, holding, item, mid, pack, without_selecting};
use crate::{Harness, PLAYER};

const CHEST: u64 = 0x7000_0000_0000_00C0;
const SWORD: u64 = 0x7000_0000_0000_0001;
const CAP: u64 = 0x7000_0000_0000_0002;

fn with_chest() -> auberean_horizon::state::GameState {
    let mut inv = pack(
        [&[CAP], &[], &[], &[]],
        &[(SWORD, item("Sword")), (CAP, item("Cap"))],
        &[],
        0,
    );
    inv.containers.insert(
        CHEST,
        Container {
            size: 12,
            segments: vec![Contents {
                items: vec![SWORD],
                first: 0,
                count: 1,
            }],
            slots: vec![(0, 1)],
        },
    );
    let mut s = holding(inv);
    s.goods.opened = vec![(CHEST, "Chest".to_owned())];
    s
}

#[test]
fn an_open_container_shows_and_take_all_takes_everything_out_of_it() {
    let mut h = Harness::new();
    let s = with_chest();
    h.frame(&s);
    let l = h.ui.goods.loot.layout(h.k()).expect("shown");
    assert_eq!(h.ui.goods.loot.shown(), Some(CHEST));
    let out = h.click(&s, mid(l.take_all));
    assert_eq!(out.acts, [Action::TakeAll { container: CHEST }]);
}

#[test]
fn an_item_carried_out_of_the_container_into_the_pack_moves_there() {
    let mut h = Harness::new();
    let s = with_chest();
    h.ui.keyed(Keyed::Inventory, &s);
    h.frame(&s);
    let k = h.k();
    let loot = h.ui.goods.loot.layout(k).expect("shown");
    let inv = h.ui.goods.inventory.layout(k).expect("open");
    let acts = without_selecting(carry(&mut h, &s, mid(loot.cell(0, k)), mid(inv.cell(3, k))));
    assert_eq!(
        acts,
        [Action::PutItem {
            item: SWORD,
            from: CHEST,
            to: PLAYER,
            old_slot: 0,
            new_slot: 1,
            quantity: 1,
        }]
    );
}

#[test]
fn closing_the_loot_window_closes_the_container() {
    let mut h = Harness::new();
    let s = with_chest();
    h.frame(&s);
    let mut asked = Vec::new();
    assert!(h.ui.close_top(&mut asked));
    assert_eq!(
        asked,
        [Action::SetOpen {
            container: CHEST,
            open: false
        }]
    );
}
