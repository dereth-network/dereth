//! The inventory window: its key, what a press on an item asks, and what carrying an item, a worn
//! item or the coins to each place asks of the game.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_client_sdk::Action;
use auberean_horizon::keys::{self, Keyed};
use auberean_horizon::ui::inventory::{coins, place, Layout};

use crate::things::{carry, enter_amount, holding, item, mid, pack, stack, without_selecting};
use crate::{Harness, PLAYER};

const SWORD: u64 = 0x7000_0000_0000_0001;
const CAP: u64 = 0x7000_0000_0000_0002;
const ROPE: u64 = 0x7000_0000_0000_0003;
const GEM: u64 = 0x7000_0000_0000_0004;

/// The window open over a pack, drawn once; where its parts are.
fn opened(h: &mut Harness, s: &auberean_horizon::state::GameState) -> Layout {
    h.ui.keyed(Keyed::Inventory, s);
    h.frame(s);
    h.ui.goods
        .inventory
        .layout(h.k())
        .expect("the window is drawn")
}

#[test]
fn the_inventory_key_opens_the_window_and_closes_it_again() {
    assert_eq!(keys::keyed(0x4100_0002), Some(Keyed::Inventory));
    let mut h = Harness::new();
    let s = holding(pack([&[], &[], &[], &[]], &[], &[], 0));
    h.ui.keyed(Keyed::Inventory, &s);
    h.frame(&s);
    assert!(h.ui.goods.inventory.rect().is_some(), "open");
    assert!(h.ui.window_open());
    h.ui.keyed(Keyed::Inventory, &s);
    h.frame(&s);
    assert!(h.ui.goods.inventory.rect().is_none(), "closed");
}

#[test]
fn a_press_on_an_item_selects_it_and_a_second_soon_after_uses_it() {
    let mut h = Harness::new();
    let s = holding(pack(
        [&[SWORD], &[], &[], &[]],
        &[(SWORD, item("Sword"))],
        &[],
        0,
    ));
    let l = opened(&mut h, &s);
    let first = h.click(&s, mid(l.cell(0, h.k())));
    assert_eq!(first.acts, [Action::SetSelection { target: SWORD }]);
    let second = h.click(&s, mid(l.cell(0, h.k())));
    assert_eq!(
        second.acts,
        [Action::SetSelection { target: SWORD }, Action::UseSelected]
    );
}

/// While an item waits for its target, the usage pointer shows over an item's icon in the
/// window, worn or in the pack, as the game's own pack shows it; over an empty tile or the window
/// elsewhere, the default; over the world, the usage one.
#[test]
fn the_usage_pointer_shows_over_an_items_icon_and_not_over_the_rest_of_the_window() {
    let mut h = Harness::new();
    let s = holding(pack(
        [&[SWORD], &[], &[], &[]],
        &[(SWORD, item("Sword")), (CAP, item("Cap"))],
        &[(place::LEFT[0], CAP)],
        0,
    ));
    let l = opened(&mut h, &s);
    let k = h.k();
    let worn = l
        .worn
        .iter()
        .find(|(at, _)| *at == place::LEFT[0])
        .map(|(_, r)| *r)
        .expect("the place");
    let at = |h: &mut Harness, (x, y): (f32, f32)| {
        h.input.mouse = (x, y);
        h.frame(&s);
        h.ui.usage_pointer_at(x, y)
    };
    assert!(at(&mut h, mid(l.cell(0, k))), "over the sword in the pack");
    assert!(at(&mut h, mid(worn)), "over the cap worn");
    assert!(!at(&mut h, mid(l.cell(1, k))), "over an empty tile");
    assert!(!at(&mut h, mid(l.figure)), "over the figure");
    assert!(at(&mut h, (960.0, 400.0)), "over the world");
}

#[test]
fn an_item_carried_along_its_segment_goes_to_that_slot() {
    let mut h = Harness::new();
    let s = holding(pack(
        [&[SWORD, CAP, ROPE], &[], &[], &[]],
        &[
            (SWORD, item("Sword")),
            (CAP, item("Cap")),
            (ROPE, item("Rope")),
        ],
        &[],
        0,
    ));
    let l = opened(&mut h, &s);
    let k = h.k();
    let acts = without_selecting(carry(&mut h, &s, mid(l.cell(0, k)), mid(l.cell(1, k))));
    assert_eq!(
        acts,
        [Action::PutItem {
            item: SWORD,
            from: PLAYER,
            to: PLAYER,
            old_slot: 0,
            new_slot: 1,
            quantity: 1,
        }]
    );
}

#[test]
fn a_whole_item_carried_past_the_last_of_its_segment_lands_a_slot_sooner() {
    let mut h = Harness::new();
    let s = holding(pack(
        [&[SWORD, CAP, ROPE], &[], &[], &[]],
        &[
            (SWORD, item("Sword")),
            (CAP, item("Cap")),
            (ROPE, item("Rope")),
        ],
        &[],
        0,
    ));
    let l = opened(&mut h, &s);
    let k = h.k();
    // Past the last item: just past it, and the place the sword leaves closes up.
    let acts = without_selecting(carry(&mut h, &s, mid(l.cell(0, k)), mid(l.cell(9, k))));
    assert_eq!(
        acts,
        [Action::PutItem {
            item: SWORD,
            from: PLAYER,
            to: PLAYER,
            old_slot: 0,
            new_slot: 2,
            quantity: 1,
        }]
    );
}

#[test]
fn an_item_carried_onto_a_bags_tab_goes_after_that_bags_last_item() {
    let mut h = Harness::new();
    let s = holding(pack(
        [&[SWORD], &[CAP, ROPE], &[], &[]],
        &[
            (SWORD, item("Sword")),
            (CAP, item("Cap")),
            (ROPE, item("Rope")),
        ],
        &[],
        0,
    ));
    let l = opened(&mut h, &s);
    let k = h.k();
    let tabs = h.ui.goods.inventory.tabs().to_vec();
    let acts = without_selecting(carry(&mut h, &s, mid(l.cell(0, k)), mid(tabs[1])));
    // The first bag's places start past the pack's own 42; it holds two.
    assert_eq!(
        acts,
        [Action::PutItem {
            item: SWORD,
            from: PLAYER,
            to: PLAYER,
            old_slot: 0,
            new_slot: 44,
            quantity: 1,
        }]
    );
}

#[test]
fn a_stack_carried_asks_how_many_up_to_what_a_stack_holds() {
    let mut h = Harness::new();
    let s = holding(pack(
        [&[GEM, SWORD], &[], &[], &[]],
        &[(GEM, stack("Gems", 9, 5)), (SWORD, item("Sword"))],
        &[],
        0,
    ));
    let l = opened(&mut h, &s);
    let k = h.k();
    let acts = without_selecting(carry(&mut h, &s, mid(l.cell(0, k)), mid(l.cell(1, k))));
    assert!(acts.is_empty(), "nothing moves before the amount: {acts:?}");
    let box_ = h.ui.goods.amount.as_ref().expect("asked");
    assert_eq!((box_.value(), box_.most), (5, 5));
    assert_eq!(
        enter_amount(&mut h, &s),
        [Action::PutItem {
            item: GEM,
            from: PLAYER,
            to: PLAYER,
            old_slot: 0,
            new_slot: 1,
            quantity: 5,
        }]
    );
}

#[test]
fn a_worn_item_carried_to_the_pack_is_taken_off_into_that_slot() {
    let mut h = Harness::new();
    let mut cap = item("Cap");
    cap.worn = true;
    let s = holding(pack(
        [&[SWORD], &[], &[], &[]],
        &[(SWORD, item("Sword")), (CAP, cap)],
        &[(place::HEAD, CAP)],
        0,
    ));
    let l = opened(&mut h, &s);
    let k = h.k();
    let head = l
        .worn
        .iter()
        .find(|(p, _)| *p == place::HEAD)
        .expect("head")
        .1;
    let acts = without_selecting(carry(&mut h, &s, mid(head), mid(l.cell(0, k))));
    assert_eq!(acts, [Action::TakeOff { item: CAP, slot: 0 }]);
}

#[test]
fn an_item_carried_to_a_place_is_worn_there_and_onto_the_figure_at_its_own() {
    let mut h = Harness::new();
    let mut cap = item("Cap");
    cap.preferred_location = place::HEAD;
    let s = holding(pack([&[CAP], &[], &[], &[]], &[(CAP, cap)], &[], 0));
    let l = opened(&mut h, &s);
    let k = h.k();
    let back = l
        .worn
        .iter()
        .find(|(p, _)| *p == place::BACK)
        .expect("back")
        .1;
    let acts = without_selecting(carry(&mut h, &s, mid(l.cell(0, k)), mid(back)));
    assert_eq!(
        acts,
        [Action::Wear {
            item: CAP,
            location: place::BACK
        }]
    );
    let acts = without_selecting(carry(&mut h, &s, mid(l.cell(0, k)), mid(l.figure)));
    assert_eq!(
        acts,
        [Action::Wear {
            item: CAP,
            location: place::HEAD
        }]
    );
}

#[test]
fn an_item_carried_onto_the_money_bag_melts_after_a_question() {
    let mut h = Harness::new();
    let s = holding(pack(
        [&[SWORD], &[], &[], &[]],
        &[(SWORD, item("Sword"))],
        &[],
        0,
    ));
    let l = opened(&mut h, &s);
    let k = h.k();
    let acts = without_selecting(carry(&mut h, &s, mid(l.cell(0, k)), mid(l.money)));
    assert!(acts.is_empty(), "asked first: {acts:?}");
    let q = h.ui.goods.question.as_ref().expect("the question");
    assert_eq!(q.text, "Turn Sword into gold?");
    let yes = h.key(&s, auberean_horizon::ui::vk::ENTER).acts;
    assert_eq!(
        yes,
        [Action::IntoMoney {
            item: SWORD,
            quantity: 1,
            slot: 0,
            from: PLAYER,
        }]
    );
}

#[test]
fn an_item_melts_without_a_question_when_the_player_turned_it_off() {
    let mut h = Harness::new();
    let mut inv = pack([&[SWORD], &[], &[], &[]], &[(SWORD, item("Sword"))], &[], 0);
    inv.no_melt_question = true;
    let s = holding(inv);
    let l = opened(&mut h, &s);
    let k = h.k();
    let acts = without_selecting(carry(&mut h, &s, mid(l.cell(0, k)), mid(l.money)));
    assert_eq!(
        acts,
        [Action::IntoMoney {
            item: SWORD,
            quantity: 1,
            slot: 0,
            from: PLAYER,
        }]
    );
    assert!(h.ui.goods.question.is_none());
}

#[test]
fn coins_carried_into_the_pack_ask_how_many_up_to_a_stack() {
    let mut h = Harness::new();
    let s = holding(pack(
        [&[SWORD], &[], &[], &[]],
        &[(SWORD, item("Sword"))],
        &[],
        1200,
    ));
    let l = opened(&mut h, &s);
    let k = h.k();
    let acts = without_selecting(carry(&mut h, &s, mid(l.money), mid(l.cell(4, k))));
    assert!(acts.is_empty());
    let box_ = h.ui.goods.amount.as_ref().expect("asked");
    assert_eq!(box_.most, 500, "no more than a stack of coins");
    assert_eq!(
        enter_amount(&mut h, &s),
        [Action::OutOfMoney {
            amount: 500,
            slot: 1,
            container: PLAYER,
        }]
    );
}

#[test]
fn an_item_carried_onto_the_world_goes_to_what_is_under_the_pointer() {
    let mut h = Harness::new();
    let s = holding(pack(
        [&[SWORD], &[], &[], &[]],
        &[(SWORD, item("Sword"))],
        &[],
        0,
    ));
    let l = opened(&mut h, &s);
    let k = h.k();
    let acts = without_selecting(carry(&mut h, &s, mid(l.cell(0, k)), (900.0, 400.0)));
    assert!(acts.is_empty());
    assert_eq!(h.ui.goods.asks.pick_world, Some((900.0, 400.0)));
    let mut out = auberean_horizon::ui::Outcome::default();
    // The player itself under the pointer is the ground.
    h.ui.goods.world_found(PLAYER, &s, &mut out);
    assert_eq!(
        out.acts,
        [Action::PutItem {
            item: SWORD,
            from: PLAYER,
            to: 0,
            old_slot: 0,
            new_slot: 0,
            quantity: 1,
        }]
    );
}

#[test]
fn an_item_carried_onto_another_window_that_takes_nothing_goes_nowhere() {
    let mut h = Harness::new();
    let s = holding(pack(
        [&[SWORD], &[], &[], &[]],
        &[(SWORD, item("Sword"))],
        &[],
        0,
    ));
    let l = opened(&mut h, &s);
    let k = h.k();
    // The window's own title bar takes nothing.
    let title = (l.figure.x, l.figure.y - 20.0 * k);
    let acts = without_selecting(carry(&mut h, &s, mid(l.cell(0, k)), title));
    assert!(acts.is_empty(), "{acts:?}");
    assert!(h.ui.goods.asks.pick_world.is_none());
}

#[test]
fn the_money_reads_in_coins_with_thousands_marked() {
    assert_eq!(coins(1), "1 coin");
    assert_eq!(coins(1_234_567), "1,234,567 coins");
}

#[test]
fn the_money_bag_is_smaller_than_an_item_s_tile_with_its_amount_beside_it_under_the_pack() {
    let mut h = Harness::new();
    let s = holding(pack([&[], &[], &[], &[]], &[], &[], 1_234));
    let l = opened(&mut h, &s);
    let k = h.k();
    let tile = auberean_horizon::ui::items::TILE * k;
    assert!(l.money.w < tile && l.money.h < tile, "{:?}", l.money);
    assert!(l.amount.x >= l.money.right(), "beside it");
    assert!((l.amount.y - l.money.y).abs() < 0.01 && (l.amount.h - l.money.h).abs() < 0.01);
    assert!(
        l.money.y >= l.cell(41, k).bottom(),
        "under the pack's six rows"
    );
}

/// The money bag shows the game's own picture of gold, as large as the bag holds it with its
/// shape kept.
#[test]
fn the_money_bag_shows_the_games_gold_picture() {
    use std::sync::Arc;

    use auberean_horizon::ui::items::GOLD;
    use dereth_ui_kit::art::{Art, ArtSource, Image};

    /// The game's records, holding its gold alone.
    #[derive(Debug)]
    struct Gold;
    impl ArtSource for Gold {
        fn icon(&self, did: u32) -> Option<Image> {
            (did == GOLD).then(|| Image {
                width: 25,
                height: 23,
                bgra: vec![0xFF; 25 * 23 * 4],
            })
        }
        fn fresh(&self) -> Arc<dyn ArtSource> {
            Arc::new(Self)
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }
    let art = Art::empty();
    art.set_source(Arc::new(Gold));
    let gold = art.icon(GOLD).expect("the gold");
    let mut h = Harness::with(art);
    let s = holding(pack([&[], &[], &[], &[]], &[], &[], 1_234));
    let l = opened(&mut h, &s);
    let drawn = h
        .list
        .quads
        .iter()
        .find(|q| q.tex == Some(gold.tex))
        .expect("the gold drawn");
    let r = drawn.dst;
    assert!(
        r.x >= l.money.x && r.right() <= l.money.right() + 0.01,
        "{r:?} in {:?}",
        l.money
    );
    assert!(r.y >= l.money.y && r.bottom() <= l.money.bottom() + 0.01);
    assert!((r.w / r.h - 25.0 / 23.0).abs() < 0.01, "its shape kept");
}

#[test]
fn the_figure_stands_over_the_windows_ground_and_under_what_follows() {
    let mut h = Harness::new();
    let s = holding(pack([&[], &[], &[], &[]], &[], &[], 0));
    let l = opened(&mut h, &s);
    let (r, mark) = h.ui.goods.inventory.figure().expect("shown");
    assert_eq!(r, l.figure);
    assert!(
        mark > 0 && mark < h.list.quads.len(),
        "{mark} of {}",
        h.list.quads.len()
    );
    // What lies under it is drawn before it: the window and the figure's own ground.
    assert!(h.list.quads[..mark]
        .iter()
        .any(|q| q.dst.x <= r.x && q.dst.right() >= r.right() && q.dst.y <= r.y));
}

#[test]
fn an_item_put_on_the_shown_tab_or_a_full_segments_tab_stays_where_it_is() {
    let mut h = Harness::new();
    let bag: Vec<u64> = (0..12).map(|i| 0x7100_0000_0000_0000 + i).collect();
    let mut known: Vec<(u64, auberean_client_sdk::contract::view::Item)> =
        bag.iter().map(|id| (*id, item("Pebble"))).collect();
    known.push((SWORD, item("Sword")));
    let s = holding(pack([&[SWORD], &bag, &[], &[]], &known, &[], 0));
    let l = opened(&mut h, &s);
    let k = h.k();
    let tabs = h.ui.goods.inventory.tabs().to_vec();
    for tab in [0, 1] {
        let acts = without_selecting(carry(&mut h, &s, mid(l.cell(0, k)), mid(tabs[tab])));
        assert!(acts.is_empty(), "tab {tab}: {acts:?}");
    }
    let acts = without_selecting(carry(&mut h, &s, mid(l.cell(0, k)), mid(tabs[2])));
    assert_eq!(
        acts,
        [Action::PutItem {
            item: SWORD,
            from: PLAYER,
            to: PLAYER,
            old_slot: 0,
            new_slot: 54,
            quantity: 1,
        }]
    );
}

#[test]
fn a_right_press_on_an_item_examines_it_in_the_examine_window() {
    let mut h = Harness::new();
    let s = holding(pack(
        [&[SWORD], &[], &[], &[]],
        &[(SWORD, item("Sword"))],
        &[],
        0,
    ));
    let l = opened(&mut h, &s);
    h.input.mouse = mid(l.cell(0, h.k()));
    h.input.pressed[1] = true;
    let out = h.frame(&s);
    assert_eq!(
        out.acts.first(),
        Some(&Action::Examine {
            kind: 0x4000_0002,
            data_enum: 0,
            item: SWORD,
            data_id: 0,
            selection: SWORD,
        })
    );
    assert!(h.ui.examine.is_open());
}
