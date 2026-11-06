//! The trade window on the retail layouts: a trade asked by a partner, opened, items offered and
//! taken back, each side's acceptance, the partner closing it.

use auberean_text::StringInfo;
use auberean_ui::{Cursor, Token, Value};
use auberean_ui_game::gameplay::{Record, Request};
use auberean_ui_game::inventory::{attr, notice as item_notice};
use auberean_ui_game::trade::{notice, Trade, TradeRequest};

use super::stage::{answer, dialog, long, said, World, ORE, PARTNER, PLAYER, SWORD};

fn trades(out: &[Request]) -> Vec<TradeRequest> {
    out.iter()
        .filter_map(|r| match r {
            Request::Trade(t) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

fn ids(v: &[u64]) -> Vec<u32> {
    v.iter().flat_map(|i| long(*i)).collect()
}

/// A trade the partner asked for puts up the question naming them; yes opens it. Opened, the
/// window shows the partner's name; an item dragged from the pack onto the player's list is
/// offered (a stack asks how many first); each side's offers show as the scripts say; accept is
/// asked and its button ghosted, and the side the scripts say accepted shows accepted until the
/// acceptance is cleared; an offer taken back leaves its list. Closed by the partner, the window
/// says so with their name for its delay, then hides.
///
/// Behaviour: ui.trade.the-trade-window-offers-accepts-and-closes
#[test]
fn the_trade_window_offers_accepts_and_closes() {
    let mut w = World::new();
    let field = w.el("SecureTradeField");
    assert!(!w.shown(field), "hidden at first");

    // Asked by the partner: the question, naming them.
    let name = Some(Record::Text(StringInfo::literal("Bartleby")));
    let words = ids(&[PARTNER, PLAYER]);
    let out = w.tick(&[said(
        notice::REGISTERED,
        [words, vec![0]].concat(),
        vec![None, None, None, None, name],
    )]);
    let (q, text) = dialog(&out).expect("the question");
    assert!(text.contains("Bartleby"), "{text}");
    let out = w.tick(&[answer(q, true)]);
    assert_eq!(trades(&out), vec![TradeRequest::Open(PARTNER)]);

    // Opened.
    w.tick(&[said(notice::OPENED, Vec::new(), Vec::new())]);
    assert!(w.shown(field), "shown");
    assert_eq!(w.state(field), Token::of("trade"));
    assert_eq!(w.text(w.el("PartnerLabel")), "Bartleby");

    // The sword offered from the pack.
    let sword = w.pack_icon(SWORD);
    let mine = w.el("TradeListBoxSelf");
    let out = w.drag(sword, mine);
    assert_eq!(
        trades(&out),
        vec![TradeRequest::Offer {
            item: SWORD,
            quantity: 1
        }]
    );
    // The ore is a stack: how many asked first.
    let ore = w.pack_icon(ORE);
    let out = w.drag(ore, mine);
    assert!(trades(&out).is_empty(), "{out:?}");
    let (label, out) = w.split(5);
    assert_eq!(label, "Ore");
    assert_eq!(
        trades(&out),
        vec![TradeRequest::Offer {
            item: ORE,
            quantity: 5
        }]
    );

    // Both sides' offers as the scripts say.
    let offered = |who: u64, item: u64, n: u32| {
        said(
            notice::OFFERED,
            [ids(&[who, item]), vec![n]].concat(),
            Vec::new(),
        )
    };
    w.tick(&[
        offered(PLAYER, SWORD, 1),
        offered(PARTNER, 0x5000_0000_0000_0099, 3),
    ]);
    let trade = w.play.child::<Trade>().expect("the trade window");
    assert_eq!(trade.offers(), (vec![SWORD], vec![0x5000_0000_0000_0099]));
    assert_eq!(w.ui.list_items(mine).len(), 1);
    assert_eq!(w.ui.list_items(w.el("TradeListBoxOther")).len(), 1);

    // Accept: asked, the button ghosted; the partner's acceptance shown, then cleared.
    let accept = w.el("AcceptTradeButton");
    let out = w.click(accept);
    assert_eq!(trades(&out), vec![TradeRequest::Accept]);
    assert!(w.ghosted(accept));
    let other = w.under(w.el("Other"), "TradeButtonInner");
    w.tick(&[said(notice::ACCEPTED, ids(&[PARTNER]), Vec::new())]);
    assert_eq!(w.state(other), Token::of("accepted"));
    w.tick(&[said(notice::ACCEPTANCE_CLEARED, Vec::new(), Vec::new())]);
    assert_eq!(w.state(other), Token::of("notaccepted"));
    w.tick(&[said(notice::ACCEPTED, vec![0, 0], Vec::new())]);
    assert!(!w.ghosted(accept), "the player's own accept lit again");

    // The sword taken back.
    w.tick(&[said(notice::REVOKED, ids(&[PLAYER, SWORD]), Vec::new())]);
    let trade = w.play.child::<Trade>().expect("the trade window");
    assert!(trade.offers().0.is_empty());
    assert!(w.ui.list_items(mine).is_empty());

    // Closed by the partner: said so for the window's delay, then hidden.
    w.tick(&[said(notice::CLOSED, ids(&[PARTNER]), Vec::new())]);
    assert_eq!(w.state(field), Token::of("partnerclosed"));
    assert!(w.text(w.el("EndedText")).contains("Bartleby"));
    assert!(w.shown(field), "still shown");
    w.wait(30.0);
    assert!(!w.shown(field), "hidden after the delay");
}

/// An offered item's tooltip names it for the number offered, not the number its owner holds.
///
/// Behaviour: ui.trade.an-offered-item-is-named-for-the-number-offered
#[test]
fn an_offered_item_is_named_for_the_number_offered() {
    let mut w = World::new();
    w.inventory.version += 1;
    w.inventory
        .items
        .get_mut(&ORE)
        .expect("the ore")
        .offered_name = Some("5 Ore".to_string());
    w.tick(&[said(notice::OPENED, Vec::new(), Vec::new())]);
    w.tick(&[said(
        notice::OFFERED,
        [ids(&[PLAYER, ORE]), vec![5]].concat(),
        Vec::new(),
    )]);
    let row = w.ui.list_items(w.el("TradeListBoxSelf"))[0];
    let icon = w.under(row, "ItemIcon");
    assert_eq!(w.ui.element_tooltip(icon), Some("5 Ore"));
}

/// An offered item shows the player's selection as a pack's does: its icon takes the selected look
/// when it becomes the selection and loses it when the selection moves on, on either side of the
/// trade. An item waiting to be used on a target gives the icon the cursor its element names, and
/// the kept cursor comes back after.
///
/// Behaviour: ui.trade.an-offered-item-shows-the-selection-and-the-targeting-cursor
#[test]
fn an_offered_item_shows_the_selection_and_the_targeting_cursor() {
    let mut w = World::new();
    let theirs = 0x5000_0000_0000_0099;
    let offered = |who: u64, item: u64| {
        said(
            notice::OFFERED,
            [ids(&[who, item]), vec![1]].concat(),
            Vec::new(),
        )
    };
    w.tick(&[said(notice::OPENED, Vec::new(), Vec::new())]);
    w.tick(&[offered(PLAYER, SWORD), offered(PARTNER, theirs)]);
    let icon = |w: &World, list: &str| {
        let row = w.ui.list_items(w.el(list))[0];
        w.under(row, "ItemIcon")
    };
    let (mine, other) = (icon(&w, "TradeListBoxSelf"), icon(&w, "TradeListBoxOther"));
    let selected = Token::of("selected");
    let selection = |id: u64| said(item_notice::SELECTION_CHANGED, ids(&[id]), Vec::new());

    w.tick(&[selection(SWORD)]);
    assert_eq!(w.state(mine), selected, "the player's offer, selected");
    assert_ne!(w.state(other), selected, "the partner's, not");
    w.tick(&[selection(theirs)]);
    assert_ne!(w.state(mine), selected, "the selection moved on");
    assert_eq!(w.state(other), selected, "the partner's offer, selected");

    let targeting = |on: u32| said(item_notice::SET_TARGETING_POINTER, vec![on], Vec::new());
    let cursor = Cursor {
        picture: 0x0600_0001,
        hot_x: 1,
        hot_y: 2,
    };
    w.ui.set_attr(mine, attr::USAGE_CURSOR, Value::DataId(cursor.picture));
    w.ui.set_attr(mine, attr::USAGE_CURSOR_X, Value::Int(1));
    w.ui.set_attr(mine, attr::USAGE_CURSOR_Y, Value::Int(2));
    let shown = |w: &World| w.ui.node(mine).and_then(|n| n.cursor).unwrap_or_default();
    let before = shown(&w);
    w.tick(&[targeting(1)]);
    assert_eq!(shown(&w), cursor, "the waiting cursor");
    w.tick(&[targeting(0)]);
    assert_eq!(shown(&w), before, "the kept one back");
}
