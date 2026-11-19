//! The player's things as Horizon follows them from the view and the scripts' notices: the pack's
//! segments and their places, the containers open by name, a trade from its setting up to its
//! closing, and the shops' notices.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_client_sdk::contract::notice::{Notice, Said, Sale, Store, StoreNotice, TradeNotice};
use auberean_client_sdk::contract::text::TextRef;
use auberean_client_sdk::contract::view::{Container, Contents, Surroundings, Thing, View};
use auberean_horizon::state::goods::{Goods, Offer};

use crate::things::{item, pack};
use crate::PLAYER;

const PARTNER: u64 = 0x2130_0000_0000_0009;
const CHEST: u64 = 0x7000_0000_0000_00C0;
const SWORD: u64 = 0x7000_0000_0000_0001;

fn no_strings(_: &TextRef) -> Option<String> {
    None
}

fn literal(s: &str) -> TextRef {
    TextRef {
        literal: Some(s.encode_utf16().collect()),
        ..TextRef::default()
    }
}

fn written(t: &TextRef) -> Option<String> {
    t.literal.as_ref().map(|l| String::from_utf16_lossy(l))
}

fn view() -> View {
    View {
        inventory: Some(pack(
            [&[SWORD], &[], &[], &[]],
            &[(SWORD, item("Sword"))],
            &[],
            7,
        )),
        world: Some(Surroundings::default()),
        ..View::default()
    }
}

fn trade(n: TradeNotice) -> Notice {
    Notice {
        about: Some(PLAYER),
        said: Said::Trade(n),
    }
}

#[test]
fn the_last_bag_takes_what_the_pack_holds_past_the_others_places() {
    let mut g = Goods::default();
    g.follow(Some(&view()), &[], &no_strings);
    // The pack holds 78: 42, then 12 and 12, and the last bag the 12 left.
    assert_eq!(g.rooms(), [42, 12, 12, 12]);
    assert_eq!((g.first_slot(1), g.first_slot(3)), (42, 66));
    let mut v = view();
    let inv = v.inventory.as_mut().unwrap();
    inv.version = 2;
    inv.containers.get_mut(&PLAYER).unwrap().size = 86;
    g.follow(Some(&v), &[], &no_strings);
    assert_eq!(g.rooms(), [42, 12, 12, 20]);
    assert_eq!(g.money(), 7);
}

#[test]
fn a_container_opened_shows_by_its_name_until_the_view_lets_it_go() {
    let mut g = Goods::default();
    let mut v = view();
    v.inventory.as_mut().unwrap().containers.insert(
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
    v.world.as_mut().unwrap().things.push(Thing {
        id: CHEST,
        own_name: literal("Chest"),
        ..Thing::default()
    });
    g.follow(Some(&v), &[], &written);
    assert_eq!(g.opened, [(CHEST, "Chest".to_owned())]);
    assert_eq!(g.contents(CHEST, 0), [SWORD]);
    let mut v = view();
    v.inventory.as_mut().unwrap().version = 3;
    g.follow(Some(&v), &[], &written);
    assert!(g.opened.is_empty());
}

#[test]
fn a_trade_follows_its_notices_from_its_setting_up_to_its_closing() {
    let mut g = Goods::default();
    let v = view();
    g.follow(
        Some(&v),
        &[trade(TradeNotice::Registered {
            asker: PLAYER,
            asked: PARTNER,
            partner: None,
        })],
        &no_strings,
    );
    let t = g.trade.as_ref().expect("set up");
    assert!(t.asked_by_player && !t.open);
    assert_eq!(t.partner, PARTNER);
    g.follow(
        Some(&v),
        &[
            trade(TradeNotice::Opened),
            trade(TradeNotice::Offered {
                by: PLAYER,
                item: SWORD,
                quantity: 1,
            }),
            trade(TradeNotice::Offered {
                by: PARTNER,
                item: 0x99,
                quantity: 3,
            }),
            trade(TradeNotice::Accepted { by: PARTNER }),
        ],
        &no_strings,
    );
    let t = g.trade.as_ref().expect("open");
    assert!(t.open && t.theirs_accepted && !t.mine_accepted);
    assert_eq!(
        t.mine,
        [Offer {
            item: SWORD,
            quantity: 1
        }]
    );
    assert_eq!(
        t.theirs,
        [Offer {
            item: 0x99,
            quantity: 3
        }]
    );
    g.follow(
        Some(&v),
        &[
            trade(TradeNotice::Withdrawn {
                by: PLAYER,
                item: SWORD,
            }),
            trade(TradeNotice::AcceptanceCleared),
        ],
        &no_strings,
    );
    let t = g.trade.as_ref().expect("open");
    assert!(t.mine.is_empty() && !t.theirs_accepted);
    // Closed by the partner, the trade stays to say so until the player closes it.
    g.follow(
        Some(&v),
        &[trade(TradeNotice::Closed {
            by: PARTNER,
            reason: 0,
        })],
        &no_strings,
    );
    let t = g.trade.as_ref().expect("still shown");
    assert!(t.ended_by_partner && !t.open && t.theirs.is_empty());
    g.trade_dismissed();
    assert!(g.trade.is_none());
}

#[test]
fn a_trade_closed_by_the_player_goes_at_once() {
    let mut g = Goods::default();
    let v = view();
    g.follow(
        Some(&v),
        &[
            trade(TradeNotice::Opened),
            trade(TradeNotice::Closed {
                by: PLAYER,
                reason: 0,
            }),
        ],
        &no_strings,
    );
    assert!(g.trade.is_none());
}

#[test]
fn a_shop_shows_its_sales_written_and_follows_their_stock_until_it_is_left() {
    let mut g = Goods::default();
    let v = view();
    let store = Store {
        id: 4,
        name: literal("Smithy"),
        sales: vec![Sale {
            item: 0x50,
            description: literal("Iron sword"),
            cost_description: literal("coins"),
            cost_quantity: 30.0,
            stock: 2,
            ..Sale::default()
        }],
        ..Store::default()
    };
    let shop = |n: StoreNotice| Notice {
        about: None,
        said: Said::Store(n),
    };
    g.follow(
        Some(&v),
        &[shop(StoreNotice::Shown(Some(Box::new(store))))],
        &written,
    );
    let s = g.shop.as_ref().expect("shown");
    assert_eq!(s.name, "Smithy");
    assert_eq!(
        (s.sales[0].description.as_str(), s.sales[0].cost.as_str()),
        ("Iron sword", "coins")
    );
    g.follow(
        Some(&v),
        &[shop(StoreNotice::Stock {
            item: 0x50,
            stock: 0,
        })],
        &written,
    );
    assert!(g.sale(0x50).expect("the sale").unavailable(), "none left");
    g.follow(
        Some(&v),
        &[
            shop(StoreNotice::Purchase {
                product: Some(literal("Iron sword")),
                quantity: 1,
                cost: Some(literal("coins")),
                cost_quantity: 30,
            }),
            shop(StoreNotice::SaleWaiting { waiting: true }),
        ],
        &written,
    );
    assert_eq!(g.purchase.as_ref().map(|p| p.cost_quantity), Some(30));
    assert!(g.sale_waiting);
    g.follow(Some(&v), &[shop(StoreNotice::Left)], &written);
    assert!(g.shop.is_none() && g.purchase.is_none());
}
