//! Talking to people and shopping.
//!
//! **Using a person** (in reach: the use answered first) runs the quest step it triggers, if one
//! does; else it says what it says when used ([`crate::talk`]): a storekeeper its greeting, if its
//! row gives one, and it opens its store: the store's first page
//! of sales (seven to a page), each sale with its product, icon, name, price and currency, stack
//! size and place in the store, and a stock id the client buys it by. A sale for some players only
//! (a race, a least level, quests at a status) is left off the page of a player who may not have
//! it where its store hides such sales, else shown restricted; buying it finds no sale.
//!
//! **Paging** answers with the next or the previous page. **Leaving** is not answered (the client
//! closes the store itself).
//!
//! **Buying** (the client checks the price against the player's money before it asks): the money
//! goes down by the price times the quantity, the storekeeper opens, the item is made in the
//! player's pack, the page is sent again, the storekeeper closes. A vendor's stock never runs
//! out. With no room in the pack, the storekeeper opens and closes, the purchase is refused for
//! want of room and the page sent again; nothing is paid.

use auberean_protocol::quality::VisualQuality;
use auberean_protocol::ServerMessage;
use auberean_script::args::Arg;
use auberean_script::native_object::{Item, NativeObject};
use yalain_content::quests::Trigger;
use yalain_content::stores::Sale;
use yalain_content::ContentObject;
use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::calls::{Node, Value};
use crate::graph::{tag, Builder, Slot};
use crate::progress;
use crate::{session, Clock, Server};

/// The store functions.
pub mod function {
    pub const ENTER_STORE: u32 = 0x9B5E_0056;
    pub const UPDATE_STORE: u32 = 0x9B5E_0049;
    pub const REQUEST_DONE: u32 = 0x9B5E_004C;
    /// The client's requests.
    pub const NEXT_SALES: u32 = 0x9B5E_0023;
    pub const PREV_SALES: u32 = 0x9B5E_0024;
    pub const LEAVE_STORE: u32 = 0x9B5E_000C;
    pub const PURCHASE: u32 = 0x9B5E_0031;
}

/// Script packages and native types of a store's view.
pub mod package {
    pub const STORE_VIEW: u16 = 0x1B75;
    pub const SALE_PROFILE: u16 = 0x1B68;
    pub const TRANSACTION: u16 = 0x1B76;
}
pub const REF_LIST: u16 = 0x65;
pub const ICON_DESC: u16 = 0x2E;
pub const TEXT: u16 = 106;

/// Sales to a page.
pub const PAGE: u32 = 7;

/// The quality saying a storekeeper is busy with a sale, seen by all.
pub const OPEN: u32 = 702;

/// The money a player has.
pub const MONEY: u32 = 3100;

/// A store's error answers.
pub mod error {
    pub const NOT_FOUND: u32 = 45001;
    pub const NOT_ENOUGH_COIN: u32 = 45006;
    pub const SALE_REFUSED: u32 = 45009;
    pub const COLLECT_FAILED: u32 = 45014;
    pub const NO_ROOM: u32 = 45016;
}

/// The first stock id: a sale's is this plus its store's low 16 bits times 256 plus its place.
pub const STOCK_BASE: u64 = 0x8130_0000_2000_0000;

/// A sale's stock id.
#[must_use]
pub fn stock_id(sale: &Sale) -> u64 {
    STOCK_BASE | u64::from(sale.store & 0xFFFF) << 8 | u64::from(sale.ordinal & 0xFF)
}

/// A player has used a person (in reach, the use answered): the quest step it triggers, else its
/// store.
pub fn talk<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    id: u64,
    o: &ContentObject,
    clock: Clock,
) {
    let name = o.description.name.or(o.facts.name);
    if crate::quests::triggered(
        s,
        net,
        world,
        Trigger::Use(o.template),
        Some((id, name)),
        clock,
    ) {
        return;
    }
    let object = crate::presence::content_by_id(&s.content.objects, id);
    if let Some(quest) = crate::quests::too_low(s, world, Trigger::Use(o.template), clock.unix) {
        if let Some(line) = crate::talk::refusal_for(s, o.template, object, quest) {
            let speaker = object.map_or_else(
                || crate::talk::Speaker {
                    id,
                    name,
                    name_text: None,
                    stamp: auberean_protocol::types::ObjectStamp {
                        id,
                        instance_stamp: 1,
                        other_stamp: 1,
                    },
                },
                |i| crate::talk::content_speaker(s, i),
            );
            crate::talk::say(s, net, world, &speaker, &line.words, line.how);
            return;
        }
    }
    if crate::quests::too_soon(s, world, Trigger::Use(o.template), clock.unix) {
        if let Some(line) = crate::talk::too_soon_line(s, o.template, object) {
            let speaker = object.map_or_else(
                || crate::talk::Speaker {
                    id,
                    name,
                    name_text: None,
                    stamp: auberean_protocol::types::ObjectStamp {
                        id,
                        instance_stamp: 1,
                        other_stamp: 1,
                    },
                },
                |i| crate::talk::content_speaker(s, i),
            );
            crate::talk::say(s, net, world, &speaker, &line.words, line.how);
            return;
        }
    }
    let keeper = s.stores.keeper(o.template).cloned();
    let greeting = keeper.as_ref().and_then(|k| k.greeting.clone());
    if let Some(g) = &greeting {
        let told = object.map_or_else(
            || name.map_or_else(Default::default, crate::progress::text),
            |i| crate::talk::content_speaker(s, i).told_as(),
        );
        crate::quests::tell(s, net, world, id, told, g);
    } else if let Some(line) = crate::talk::at_use(s, world, o.template, object) {
        let speaker = object.map_or(
            crate::talk::Speaker {
                id,
                name,
                name_text: None,
                stamp: auberean_protocol::types::ObjectStamp {
                    id,
                    instance_stamp: 1,
                    other_stamp: 1,
                },
            },
            |i| crate::talk::content_speaker(s, i),
        );
        crate::talk::say(s, net, world, &speaker, &line.words, line.how);
    }
    let Some(k) = keeper else {
        return;
    };
    let view = page(s, world, id, k.store, 0, function::ENTER_STORE);
    progress::private(net, world, &view);
    if let Some(p) = s.players.get_mut(&world) {
        p.shopping = Some((id, k.store, 0));
    }
    crate::market::follow(s, world, id, clock);
}

/// A list of references.
fn refs(items: Vec<Item>) -> NativeObject {
    NativeObject::Sequence {
        item: auberean_script::library::Elem::Ref,
        items,
    }
}

/// A page of a store's sales, as the call `f` (entering or updating) sends it.
#[must_use]
pub fn page<S: Store>(
    s: &Server<S>,
    world: ConnId,
    keeper: u64,
    store: u32,
    pos: u32,
    f: u32,
) -> ServerMessage {
    let sales = s.stores.sales_of(store);
    let size = u32::try_from(sales.len()).unwrap_or(0);
    let mut b = Builder::new();
    let view = b.reserve(package::STORE_VIEW);
    // The page's sales the player may have; the others hidden where the store hides them, else
    // shown restricted. The store's size counts them all (2017: a store of four showing two).
    let shown: Vec<(&Sale, bool)> = sales
        .iter()
        .skip(pos as usize)
        .take(PAGE as usize)
        .map(|sale| (*sale, may_have(s, world, sale)))
        .filter(|(_, ok)| *ok || !s.stores.hides(store))
        .collect();
    // The list first, its references once the sales are in: a sale is its profile, its icon when
    // it has layers, and its name (2017: 400 of 400 sales with layers, 110 of 110 without).
    let list = b.native(REF_LIST, refs(Vec::new()));
    let mut profiles = Vec::with_capacity(shown.len());
    for (sale, ok) in shown {
        let profile = b.reserve(package::SALE_PROFILE);
        profiles.push(Item::Ref(profile));
        let icon = (!sale.icon.is_empty()).then(|| {
            let mut bytes = u32::try_from(sale.icon.len())
                .unwrap_or(0)
                .to_le_bytes()
                .to_vec();
            for (l, p, c) in &sale.icon {
                bytes.extend(l.to_le_bytes());
                bytes.extend(p.to_le_bytes());
                bytes.extend(c.to_le_bytes());
            }
            b.native(
                ICON_DESC,
                NativeObject::Packed {
                    ty: ICON_DESC.into(),
                    bytes,
                },
            )
        });
        // A sale without layers names its product's icon instead (110 of 110 in 2017).
        let picture = match icon {
            None => s
                .catalog
                .get(sale.product)
                .and_then(|t| t.props.word("Icon"))
                .map_or(Slot::Int(0), |d| Slot::Tagged(0x04, d)),
            Some(_) => Slot::Int(0),
        };
        let name = b.native(TEXT, NativeObject::TextInfo(progress::text(sale.name)));
        let stock = stock_id(sale);
        // A stack of one goes as the whole number it starts as, a larger one as the value set
        // (510 of 510).
        let stack = if sale.stack == 1 {
            Slot::Int(1)
        } else {
            Slot::Tagged(0x04, sale.stack)
        };
        #[allow(clippy::cast_possible_truncation)]
        b.fill(
            profile,
            &[
                Slot::Tagged(0x04, sale.product),
                Slot::Ref(icon),
                Slot::Tagged(0x84, stock as u32),
                Slot::Tagged(tag::HIGH, (stock >> 32) as u32),
                picture,
                Slot::Ref(Some(name)),
                Slot::Tagged(0x04, sale.cost.to_bits()),
                Slot::Tagged(0x04, sale.currency),
                stack,
                Slot::Tagged(0x04, sale.ordinal),
                Slot::Int(u32::from(!ok)),
            ],
        );
    }
    b.replace_native(list, refs(profiles));
    b.fill(
        view,
        &[
            Slot::Ref(Some(list)),
            Slot::Tagged(0x04, store),
            Slot::Tagged(0x04, size),
            Slot::Int(pos),
        ],
    );
    session::call(f, &[Arg::Object(b.graph()), Arg::LongInt(keeper)]).expect("a store view packs")
}

/// Whether a player may have a sale: of its race, of its level, holding its quests at their status.
fn may_have<S: Store>(s: &Server<S>, world: ConnId, sale: &Sale) -> bool {
    let Some(p) = s.players.get(&world) else {
        return false;
    };
    let held = &p.play.quests;
    sale.gate
        .admits(p.race, crate::examine::level_of(&p.qualities), &|q| {
            held.iter().find(|h| h.quest == q).map(|h| h.status)
        })
}

/// The client turns the store's page.
pub fn turn_page<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    forward: bool,
    now: dereth_primitives::LocalTime,
) {
    let Some((keeper, store, pos)) = s.players.get(&world).and_then(|p| p.shopping) else {
        s.unhandled.note("a store page turned outside a store");
        return;
    };
    if crate::market::is_broker(s, keeper) {
        crate::market::turn_page(s, net, world, (keeper, store, pos), forward, now);
        return;
    }
    let size = u32::try_from(s.stores.sales_of(store).len()).unwrap_or(0);
    let pos = if forward {
        if pos + PAGE < size {
            pos + PAGE
        } else {
            pos
        }
    } else {
        pos.saturating_sub(PAGE)
    };
    let m = page(s, world, keeper, store, pos, function::UPDATE_STORE);
    progress::private(net, world, &m);
    if let Some(p) = s.players.get_mut(&world) {
        p.shopping = Some((keeper, store, pos));
    }
}

/// The client leaves the store (not answered): a vendor's store is no longer sent again.
pub fn leave<S: Store>(s: &mut Server<S>, world: ConnId) {
    if let Some(p) = s.players.get_mut(&world) {
        p.shopping = None;
    }
    crate::market::left_store(s, world);
}

/// The money a player has.
#[must_use]
pub fn money(qualities: &[u8]) -> u32 {
    crate::quals::int(qualities, MONEY).map_or(0, i32::cast_unsigned)
}

/// The client buys a sale: read its transaction (the stock id, the quantity, the storekeeper).
#[must_use]
pub fn read_purchase(v: &[Value]) -> Option<(u64, u32, u64)> {
    let Some(Node::Script { package, slots }) = v.first().and_then(Value::root) else {
        return None;
    };
    if *package != package::TRANSACTION || slots.len() < 8 {
        return None;
    }
    let long = |i: usize| u64::from(slots[i + 1].1) << 32 | u64::from(slots[i].1);
    Some((long(0), slots[5].1, long(6)))
}

/// A purchase.
pub fn purchase<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[Value],
    clock: Clock,
) {
    let Some((stock, quantity, keeper)) = read_purchase(v) else {
        s.unhandled.note("a purchase that does not read");
        return;
    };
    // A player's sale, bought from a broker's store.
    if crate::market::purchase(s, net, world, stock, keeper) {
        return;
    }
    let Some((_, store, pos)) = s.players.get(&world).and_then(|p| p.shopping) else {
        s.unhandled.note("a purchase outside a store");
        return;
    };
    let Some(sale) = s
        .stores
        .sales_of(store)
        .into_iter()
        .find(|x| stock_id(x) == stock && may_have(s, world, x))
        .cloned()
    else {
        let m =
            session::call(function::REQUEST_DONE, &[Arg::Int(error::NOT_FOUND)]).expect("packs");
        progress::private(net, world, &m);
        return;
    };
    // What the world can't make isn't sold: nothing is paid for it. Nor what is bought for what its
    // acquiring does, when the world can't do that, or not for this player now: a contract hires one
    // mercenary, so it goes one at a time.
    let hires = crate::pets::is_contract(sale.product);
    let refused = if hires {
        quantity > 1 || !crate::pets::may_hire(s, world, sale.product)
    } else {
        s.catalog
            .get(sale.product)
            .is_none_or(|t| crate::quests::acquiring_does_more_than_quests(&t.props))
    };
    if refused || s.catalog.get(sale.product).is_none() {
        s.unhandled.note(format!(
            "a purchase of template {:08X}, which the world can't make or carry out",
            sale.product
        ));
        let m =
            session::call(function::REQUEST_DONE, &[Arg::Int(error::NOT_FOUND)]).expect("packs");
        progress::private(net, world, &m);
        return;
    }
    let quantity = quantity.max(1);
    // Of the sale's stack at most, in reach of the storekeeper (2017: 23 purchases of 1 to 300).
    let most = s
        .catalog
        .get(sale.product)
        .and_then(|t| t.props.int("Item_MaxStackSize"))
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(1)
        .max(sale.stack)
        .max(1);
    if quantity > most || !crate::market::near(s, world, keeper) {
        let m =
            session::call(function::REQUEST_DONE, &[Arg::Int(error::NOT_FOUND)]).expect("packs");
        progress::private(net, world, &m);
        return;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let price = (sale.cost * quantity as f32).ceil() as u32;
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let have = money(&p.qualities);
    if have < price {
        let m = session::call(function::REQUEST_DONE, &[Arg::Int(error::NOT_ENOUGH_COIN)])
            .expect("packs");
        progress::private(net, world, &m);
        return;
    }
    let room = crate::inventory::fits_in_pack(&p.play.items, 1);
    let open = |s: &mut Server<S>, v: i32| {
        ServerMessage::UpdateBoolVisual(VisualQuality {
            object: crate::places::object_stamp(s, keeper),
            quality: OPEN,
            value: v,
        })
    };
    // No room in the pack: the storekeeper opens and closes, the purchase is refused and the page
    // sent again; nothing is paid.
    if !room {
        for v in [1, 0] {
            let m = open(s, v);
            crate::places::to_knowers(s, net, keeper, &m);
        }
        let m = session::call(function::REQUEST_DONE, &[Arg::Int(error::NO_ROOM)]).expect("packs");
        progress::private(net, world, &m);
        let m = page(s, world, keeper, store, pos, function::UPDATE_STORE);
        progress::private(net, world, &m);
        return;
    }
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let left = have - price;
    crate::quals::set_int(&mut p.qualities, MONEY, left.cast_signed());
    // The money first (2017: before the storekeeper opened in 9 of 10 purchases).
    progress::send_int(net, world, MONEY, left.cast_signed());
    let opened = open(s, 1);
    crate::places::to_knowers(s, net, keeper, &opened);
    crate::inventory::bought(s, net, world, sale.product, quantity, keeper, clock);
    if hires {
        crate::pets::hire(s, net, world, sale.product, clock);
    }
    let m = page(s, world, keeper, store, pos, function::UPDATE_STORE);
    progress::private(net, world, &m);
    let closed = open(s, 0);
    crate::places::to_knowers(s, net, keeper, &closed);
    crate::world::keep_player(s, world);
}
