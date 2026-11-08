//! The market: brokers' catalogs of stores that sell what players put up, and players'
//! consignments.
//!
//! **Using a broker** (walked to within five meters): the use answered, then its catalog (each of
//! its stores with how many sales it holds); a shopper still in one of the broker's stores leaves
//! it first. While the shopper stays near, the catalog is sent again about every half minute; it
//! ends when the shopper walks off.
//!
//! **A vendor's store** is followed the same way: while the shopper stays in it and near, the
//! store's size is sent again about every half minute (as a catalog of none); leaving the store
//! ends it, and walking off past eight meters ends it with the catalog's leaving. A vendor's
//! stock never runs out, so nothing in it changes.
//!
//! **A store chosen** from the catalog shows the sales players put up in it.
//!
//! **Consignments**: entering them shows the player's own (and its reminder of them at that
//! broker). Putting an item up for a price takes a charge of a hundredth of what it would fetch
//! (at least 1), moves the item to the broker and shows the consignment. Collecting an unsold one
//! gives the item back as a new item holding all it held; collecting a sold one pays its price. A
//! player entering the world is reminded of its consignments, one reminder per broker.
//!
//! **What may not be sold**: an item attuned to its holder, and one bound by use in combat: refused,
//! nothing taken and nothing moved. Nor anything by a character holding [`MOST_CONSIGNMENTS`] already,
//! which is told so.
//!
//! **A page** shows as many sales as a storekeeper's and names the whole listing's size; the client
//! turns the pages as it does a storekeeper's (2017: no page of any store showed more than 7 of 510
//! sales; no broker's store listed more than 3). So what one player's asking holds up is never more
//! than a moment of its own connection's messages.
//!
//! **What a page costs.** A connection's pages are drawn from a store of [`PAGE_ROWS_HELD`] rows,
//! filled at [`PAGE_ROWS_A_SECOND`], by the sales shown and the listing read for them ([`LISTED_A_ROW`]);
//! a page past that is not answered.
//!
//! **Buying a player's sale**: the price taken, the item moved from the broker into the buyer's
//! pack, the sale's stock told as gone; the seller collects the price. A seller in the world is told
//! of the sale, which its client marks on its reminder of that broker. A client shopping at that
//! broker doesn't take that word, so it is sent the reminder whole, and the consignments page if it
//! has it open.
//!
//! **A sale's time.** What is unsold [`ON_SALE_SECONDS`] after it went up is expired: it leaves its
//! store, can't be bought, and its seller's lists mark it, the seller told if in the world. Nothing
//! moves and nothing is written when that happens: it comes of the time the record already holds, so
//! a stop or a crash can't touch it. The item stays with its broker, whole, for as long as it takes
//! its seller to collect it, with room in the pack as any collection needs.
//!
//! **Restocking**: more of what a consignment holds, put up with it at its price. The client's window
//! offers it for any consignment not sold out, and asks once it has said what it will cost: a hundredth
//! of what those would fetch, at least 1, as a sale. They must be of the sale's template and numbers,
//! and with what is left make less than a full stack. What is left and what comes make one sale, on
//! sale from now, an expired one too. Anything else is refused, nothing taken and nothing moved. This
//! rests on the client's code and its script library: no recording has one.
//!
//! **A seller that is gone.** A character deleted keeps what it has with brokers through the hour it
//! may come back in, on sale as before. When it is gone, that goes with it as its pack does: what is
//! unsold leaves its store and the world, and the price of what it sold and did not collect is paid
//! to no one. What a buyer bought stays the buyer's.

use std::collections::BTreeMap;

use auberean_protocol::physics::ObjectOnly;
use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::library::Elem;
use auberean_script::native_object::{Item, NativeObject, TextInfo};
use yalain_net::{BlobKind, ConnId, LocalTime, ServerNet};
use yalain_store::Store;

use crate::call_limit::Bucket;
use crate::calls::Value;
use crate::graph::{hash, list, tag, Builder, Slot};
use crate::inventory::{self, Move};
use crate::state::Held;
use crate::{progress, session, store, Clock, Server};

/// The market's calls.
pub mod function {
    /// The client's.
    pub const ENTER_STORE: u32 = 0x9B5E_0029;
    pub const LEAVE_CATALOG: u32 = 0x9B5E_0022;
    pub const ENTER_CONSIGNMENT: u32 = 0x9B5E_002B;
    pub const LEAVE_CONSIGNMENT: u32 = 0x9B5E_002A;
    pub const SELL: u32 = 0x9B5E_0028;
    pub const COLLECT: u32 = 0x9B5E_003F;
    pub const RESTOCK: u32 = 0x9B5E_0016;
    pub const SALES: u32 = 0x9B5E_0009;
    /// The server's.
    pub const ENTER_CATALOG: u32 = 0x9B5E_0052;
    pub const UPDATE_CATALOG: u32 = 0x9B5E_0062;
    pub const LEAVE_CATALOG_DONE: u32 = 0x9B5E_005E;
    pub const LEAVE_STORE: u32 = 0x9B5E_004D;
    pub const ENTER_CONSIGNMENT_DONE: u32 = 0x9B5E_0053;
    pub const UPDATE_CONSIGNMENT: u32 = 0x9B5E_0057;
    pub const SALE_REMINDERS: u32 = 0x9B5E_005D;
    pub const UPDATE_SALE_REMINDERS: u32 = 0x9B5E_0060;
    pub const UPDATE_SALE: u32 = 0x9B5E_0046;
}

/// Script packages: a consignment (9 slots), a player's sale (10), a consigner's reminder (3).
pub const CONSIGNMENT: (u16, usize) = (0x1B65, 9);
pub const PLAYER_SALE: (u16, usize) = (0x1B69, 10);
pub const CONSIGNER: (u16, usize) = (0x1B64, 3);
/// What has become of one consignment, by its sale: its package and size.
pub const CONSIGNMENT_DESC: (u16, usize) = (0x1B66, 3);
/// Native types: a table of words to words, a list of references, an icon, a string reference.
const WORD_WORD_HASH: u16 = 0x1;
const REF_LIST: u16 = 0x65;
const ICON: u16 = 0x2E;
const TEXT: u16 = 0x6A;

/// What a sale is priced in.
pub const COIN: u32 = 0x4700_0538;
/// The consignment charge, a fraction of what the consignment would fetch.
pub const CHARGE: f64 = 0.01;
/// A move into a broker's keeping, and out of it.
pub const MOVE_IN: u32 = 0x8000_0020;
pub const MOVE_OUT: u32 = 0x4000_0004;
/// How near a shopper must stay, and how often its catalog is sent again (seconds).
pub const STAY_NEAR: f32 = 8.0;
pub const CATALOG_EVERY: f64 = 34.0;
pub const FIRST_UPDATE: f64 = 2.5;
/// How far a broker is walked to.
pub const BROKER_REACH: f32 = 5.0;

/// How long a consignment stays on sale, in seconds. The client counts down to ten days and forgets
/// its reminder of one at eleven; the one sale recorded that long was unexpired at 10.91 days.
pub const ON_SALE_SECONDS: f64 = 11.0 * 86_400.0;
/// How often the game clock is compared with the sales' times, in seconds.
pub const EXPIRY_LOOK: f64 = 60.0;
/// A consignment's flag: expired.
const FLAG_EXPIRED: u32 = 1;

/// The most consignments a character holds at once. The client has words for a character's maximum,
/// so the 2017 servers had one; its number was never recorded (of 2 sales put up in the recordings,
/// a character's most is 2), so this is ours.
pub const MOST_CONSIGNMENTS: usize = 100;
/// The words for a character at that maximum.
pub const AT_THE_MOST: (u32, u32) = (0xCF60_26D2, 0x2500_0CEE);
/// A connection's rows of pages, at once and each second (18 recorded asks, none of more than 13).
pub const PAGE_ROWS_HELD: f64 = 2000.0;
pub const PAGE_ROWS_A_SECOND: f64 = 1000.0;
/// How many of a listing's sales read for a page cost as much as a row it shows.
pub const LISTED_A_ROW: usize = 10;
/// The world's record of the consignments.
pub const RECORD: &str = "consignments";
/// The record's encoding (1 held no item's own numbers).
const RECORD_VERSION: u8 = 2;
/// The most of one stack a player sale offers.
pub const MOST_STACK: u32 = 50;

/// A broker: the catalog it shows (a store group) and where it stands (a string reference).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Broker {
    pub catalog: u32,
    pub location: (u32, u32),
}

/// One consignment: the sale, its owner and broker, the store it sells in, the item, its price,
/// how many were put up and how many sold, when.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Consignment {
    pub sale: u32,
    pub owner: u64,
    pub keeper: u64,
    pub catalog: u32,
    pub store: u32,
    pub item: u64,
    pub template: u32,
    pub price: f32,
    pub offered: u32,
    pub sold: u32,
    pub entered: f64,
}

impl Consignment {
    /// Are any of it left?
    #[must_use]
    pub const fn in_stock(&self) -> bool {
        self.sold < self.offered
    }

    /// Is what is left of it past its time on sale, at this game time?
    #[must_use]
    pub fn expired(&self, now: f64) -> bool {
        self.in_stock() && now - self.entered >= ON_SALE_SECONDS
    }

    /// May it be seen and bought in its store, at this game time?
    #[must_use]
    pub fn on_sale(&self, now: f64) -> bool {
        self.in_stock() && !self.expired(now)
    }
}

/// Every consignment, read from the world's record once; who shops at which broker.
#[derive(Debug, Clone, Default)]
pub struct Market {
    pub loaded: bool,
    pub list: Vec<Consignment>,
    pub next_sale: u32,
    /// What the items on sale hold of their own, as the record keeps it.
    pub item_states: Vec<crate::state::ItemState>,
    /// Shoppers at brokers: player, broker, when its catalog is next sent.
    pub shoppers: Vec<(ConnId, u64, f64)>,
    /// When the shoppers were last looked at.
    pub last_look: f64,
    /// The game time the sales' times were last compared with.
    pub expiry_look: Option<f64>,
}

impl Market {
    /// The record in the server's encoding.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = vec![RECORD_VERSION];
        out.extend(self.next_sale.to_le_bytes());
        out.extend(u32::try_from(self.list.len()).unwrap_or(0).to_le_bytes());
        for c in &self.list {
            out.extend(c.sale.to_le_bytes());
            out.extend(c.owner.to_le_bytes());
            out.extend(c.keeper.to_le_bytes());
            out.extend(c.catalog.to_le_bytes());
            out.extend(c.store.to_le_bytes());
            out.extend(c.item.to_le_bytes());
            out.extend(c.template.to_le_bytes());
            out.extend(c.price.to_bits().to_le_bytes());
            out.extend(c.offered.to_le_bytes());
            out.extend(c.sold.to_le_bytes());
            out.extend(c.entered.to_bits().to_le_bytes());
            // Its length, 0 for none: a sold item is its buyer's to keep.
            let state = self
                .item_states
                .iter()
                .find(|k| k.id == c.item)
                .map_or_else(Vec::new, crate::state::ItemState::to_bytes);
            out.extend(u32::try_from(state.len()).unwrap_or(0).to_le_bytes());
            out.extend(state);
        }
        out
    }

    /// Read the record back; none when it does not read.
    #[must_use]
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        let (&version, mut r) = b.split_first()?;
        if !(1..=RECORD_VERSION).contains(&version) {
            return None;
        }
        let u32_ = |r: &mut &[u8]| -> Option<u32> {
            let (a, rest) = r.split_first_chunk::<4>()?;
            *r = rest;
            Some(u32::from_le_bytes(*a))
        };
        let next_sale = u32_(&mut r)?;
        let n = u32_(&mut r)?;
        let mut m = Self {
            next_sale,
            ..Self::default()
        };
        let mut words = Vec::new();
        for _ in 0..n {
            // A consignment's 15 words.
            words.clear();
            for _ in 0..15 {
                words.push(u32_(&mut r)?);
            }
            let long = |i: usize| u64::from(words[i]) | u64::from(words[i + 1]) << 32;
            m.list.push(Consignment {
                sale: words[0],
                owner: long(1),
                keeper: long(3),
                catalog: words[5],
                store: words[6],
                item: long(7),
                template: words[9],
                price: f32::from_bits(words[10]),
                offered: words[11],
                sold: words[12],
                entered: f64::from_bits(long(13)),
            });
            if version >= 2 {
                let n = usize::try_from(u32_(&mut r)?).ok()?;
                if n > 0 {
                    let (state, rest) = r.split_at_checked(n)?;
                    m.item_states
                        .push(crate::state::ItemState::from_bytes(state)?);
                    r = rest;
                }
            }
        }
        Some(m)
    }
}

/// Read the consignments once, making their items again where the record keeps them. Whether they
/// are read: while the store cannot answer, nothing about them may be done or kept.
pub fn ensure<S: Store>(s: &mut Server<S>) -> bool {
    if s.market.loaded {
        return true;
    }
    let Some(read) = crate::saves::read_record(s, RECORD, Market::from_bytes) else {
        return false;
    };
    let read = read.unwrap_or_default();
    s.market.list = read.list;
    s.market.next_sale = read.next_sale;
    s.market.item_states.clone_from(&read.item_states);
    s.market.loaded = true;
    let items: Vec<Consignment> = s.market.list.clone();
    for c in items {
        // A sold item is its buyer's: it comes back with its buyer.
        if c.sold >= c.offered {
            continue;
        }
        if s.things.get(c.item).is_none() {
            if let Some(t) = s.catalog.get(c.template).cloned() {
                let mut x = crate::things::Thing::of(c.item, &t, crate::things::Kind::Item);
                x.quantity = c.offered.saturating_sub(c.sold).max(1);
                x.container = Some((c.keeper, 1));
                if let Some(kept) = read.item_states.iter().find(|k| k.id == c.item) {
                    kept.restore(&mut x);
                }
                s.things.insert(x);
            }
        }
    }
    true
}

fn keep<S: Store>(s: &mut Server<S>) {
    if !s.market.loaded {
        return;
    }
    // An item its template no longer makes is not in the world: as it was last kept.
    s.market.item_states = s
        .market
        .list
        .iter()
        .filter(|c| c.sold < c.offered)
        .filter_map(|c| match s.things.get(c.item) {
            Some(t) => Some(crate::state::ItemState::of(t)),
            None => s
                .market
                .item_states
                .iter()
                .find(|k| k.id == c.item)
                .cloned(),
        })
        .collect();
    let b = s.market.to_bytes();
    s.saves.hold_record(RECORD, b);
}

fn call(f: u32, args: &[Arg]) -> ServerMessage {
    session::call(f, args).expect("values pack")
}

fn long_of(v: &[Value], i: usize) -> Option<u64> {
    v.iter()
        .filter_map(|x| match x {
            Value::Long(l) => Some(*l),
            _ => None,
        })
        .nth(i)
}

fn ints(v: &[Value]) -> Vec<u32> {
    v.iter()
        .filter_map(|x| match x {
            Value::Int(i) => Some(*i),
            _ => None,
        })
        .collect()
}

/// A catalog: each store holding sales, with how many; a vendor's (no catalog), its one store with
/// all its sales.
fn view<S: Store>(s: &Server<S>, keeper: u64, catalog: u32) -> Arg {
    let mut counts: BTreeMap<u32, u32> = BTreeMap::new();
    if catalog == 0 {
        if let Some(k) = crate::presence::content_by_id(&s.content.objects, keeper)
            .and_then(|i| s.stores.keeper(s.content.objects[i].template))
        {
            let n = u32::try_from(s.stores.sales_of(k.store).len()).unwrap_or(0);
            counts.insert(k.store, n);
        }
    }
    let now = s.game_now;
    for c in s
        .market
        .list
        .iter()
        .filter(|c| c.catalog == catalog && c.on_sale(now))
    {
        *counts.entry(c.store).or_default() += 1;
    }
    let entries: Vec<(Item, Item)> = counts
        .into_iter()
        .map(|(k, n)| (Item::Word(k), Item::Word(n)))
        .collect();
    let mut b = Builder::new();
    b.native(
        WORD_WORD_HASH,
        hash(Elem::Word, Elem::Word, session::TABLE_SIZE, &entries),
    );
    Arg::Object(b.graph())
}

fn catalog_message<S: Store>(s: &Server<S>, f: u32, keeper: u64, catalog: u32) -> ServerMessage {
    call(
        f,
        &[
            view(s, keeper, catalog),
            Arg::Int(catalog),
            Arg::LongInt(keeper),
        ],
    )
}

/// The broker a content object is, if it is one.
#[must_use]
pub fn broker<S: Store>(s: &Server<S>, template: u32) -> Option<Broker> {
    s.brokers.get(&template).copied()
}

/// A broker used, the player near: the use answered, then its catalog.
pub fn use_broker<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    keeper: u64,
    b: Broker,
    done: &ServerMessage,
    clock: Clock,
) {
    if !ensure(s) {
        return;
    }
    progress::private(net, world, done);
    if let Some(p) = s.players.get_mut(&world) {
        if p.shopping.is_some_and(|(k, _, _)| k == keeper) {
            p.shopping = None;
            progress::private(
                net,
                world,
                &call(function::LEAVE_STORE, &[Arg::LongInt(keeper)]),
            );
        }
    }
    let m = catalog_message(s, function::ENTER_CATALOG, keeper, b.catalog);
    progress::private(net, world, &m);
    follow(s, world, keeper, clock);
}

/// A shopper at a storekeeper: its catalog (a vendor's: its store) sent again while it stays near.
pub fn follow<S: Store>(s: &mut Server<S>, world: ConnId, keeper: u64, clock: Clock) {
    s.market.shoppers.retain(|(w, _, _)| *w != world);
    s.market
        .shoppers
        .push((world, keeper, clock.local.0 + FIRST_UPDATE));
}

/// A shopper leaves a vendor's store: its store no longer sent again (a broker's catalog goes on,
/// the shopper back in it).
pub fn left_store<S: Store>(s: &mut Server<S>, world: ConnId) {
    let vendor = |s: &Server<S>, keeper: u64| {
        crate::presence::content_by_id(&s.content.objects, keeper)
            .is_none_or(|i| broker(s, s.content.objects[i].template).is_none())
    };
    let gone: Vec<u64> = s
        .market
        .shoppers
        .iter()
        .filter(|(w, k, _)| *w == world && vendor(s, *k))
        .map(|(_, k, _)| *k)
        .collect();
    s.market
        .shoppers
        .retain(|(w, k, _)| !(*w == world && gone.contains(k)));
}

/// The shoppers looked at about once a second: the catalog sent again when due, or ended for one
/// who walked off.
pub fn look<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock) {
    if s.market.shoppers.is_empty() || clock.local.0 - s.market.last_look < 1.0 {
        return;
    }
    s.market.last_look = clock.local.0;
    let shoppers = s.market.shoppers.clone();
    let mut keep_on = Vec::new();
    for (world, keeper, next) in shoppers {
        let Some(p) = s.players.get(&world) else {
            continue;
        };
        let at = crate::presence::content_by_id(&s.content.objects, keeper)
            .map(|i| &s.content.objects[i]);
        let near = at.is_some_and(|o| {
            crate::places::reach(&p.position, o.cell, o.origin).is_some_and(|d| d <= STAY_NEAR)
        });
        let catalog = at
            .and_then(|o| broker(s, o.template))
            .map_or(0, |b| b.catalog);
        if !near {
            progress::private(
                net,
                world,
                &call(function::LEAVE_CATALOG_DONE, &[Arg::LongInt(keeper)]),
            );
            if let Some(p) = s.players.get_mut(&world) {
                p.at_consignments = None;
            }
            continue;
        }
        if next <= clock.local.0 {
            let m = catalog_message(s, function::UPDATE_CATALOG, keeper, catalog);
            progress::private(net, world, &m);
            keep_on.push((world, keeper, clock.local.0 + CATALOG_EVERY));
        } else {
            keep_on.push((world, keeper, next));
        }
    }
    s.market.shoppers = keep_on;
}

/// A player sale's profile: the product, its icon, the item, its name, its price in coins, its
/// stack, how many are left.
fn sale_profile(b: &mut Builder, c: &Consignment, name: (u32, u32), icon: &[[u32; 3]]) -> u32 {
    let me = b.reserve(PLAYER_SALE.0);
    let icon_ref = (!icon.is_empty()).then(|| {
        let mut bytes = u32::try_from(icon.len())
            .unwrap_or(0)
            .to_le_bytes()
            .to_vec();
        for l in icon {
            for w in l {
                bytes.extend(w.to_le_bytes());
            }
        }
        b.native(
            ICON,
            NativeObject::Packed {
                ty: ICON.into(),
                bytes,
            },
        )
    });
    let n = b.native(TEXT, NativeObject::TextInfo(progress::text(name)));
    #[allow(clippy::cast_possible_truncation)]
    b.fill(
        me,
        &[
            Slot::Tagged(4, c.template),
            Slot::Ref(icon_ref),
            Slot::Tagged(0x84, c.item as u32),
            Slot::Tagged(tag::HIGH, (c.item >> 32) as u32),
            Slot::Int(0),
            Slot::Ref(Some(n)),
            Slot::Tagged(tag::FLOAT, c.price.to_bits()),
            Slot::Tagged(4, COIN),
            Slot::Tagged(4, MOST_STACK),
            Slot::Tagged(4, c.offered.saturating_sub(c.sold)),
        ],
    );
    me
}

fn name_and_icon<S: Store>(s: &Server<S>, c: &Consignment) -> ((u32, u32), Vec<[u32; 3]>) {
    let name = s
        .catalog
        .get(c.template)
        .and_then(|t| t.facts.name)
        .unwrap_or((0, 0));
    let icon = s.things.get(c.item).map_or_else(
        || {
            s.catalog
                .icons
                .get(&c.template)
                .cloned()
                .unwrap_or_default()
        },
        |t| inventory::icon_of(s, t),
    );
    (name, icon)
}

/// A consignment as the client holds it.
fn consignment(
    b: &mut Builder,
    c: &Consignment,
    name: (u32, u32),
    icon: &[[u32; 3]],
    now: f64,
) -> u32 {
    let me = b.reserve(CONSIGNMENT.0);
    let profile = sale_profile(b, c, name, icon);
    let t = c.entered.to_bits();
    #[allow(clippy::cast_possible_truncation)]
    b.fill(
        me,
        &[
            Slot::Ref(Some(profile)),
            Slot::Long(c.owner),
            Slot::Int(c.sale),
            // How many are left: the client adds what has sold to it for how many went up.
            Slot::Int(c.offered.saturating_sub(c.sold)),
            Slot::Int(c.sold),
            Slot::Tagged(0x84, t as u32),
            Slot::Tagged(tag::HIGH, (t >> 32) as u32),
            Slot::Int(if c.expired(now) { FLAG_EXPIRED } else { 0 }),
        ],
    );
    me
}

/// A list of a player's consignments at a broker.
fn consignments_list<S: Store>(s: &Server<S>, b: &mut Builder, owner: u64, keeper: u64) -> u32 {
    let mine: Vec<Consignment> = s
        .market
        .list
        .iter()
        .filter(|c| c.owner == owner && c.keeper == keeper)
        .copied()
        .collect();
    let list_id = b.next_id();
    b.native(REF_LIST, list(Elem::Ref, Vec::new()));
    let mut refs = Vec::new();
    for c in &mine {
        let (name, icon) = name_and_icon(s, c);
        refs.push(Item::Ref(consignment(b, c, name, &icon, s.game_now)));
    }
    b.replace_native(list_id, list(Elem::Ref, refs));
    list_id
}

fn consignment_message<S: Store>(s: &Server<S>, f: u32, owner: u64, keeper: u64) -> ServerMessage {
    let mut b = Builder::new();
    consignments_list(s, &mut b, owner, keeper);
    call(f, &[Arg::Object(b.graph()), Arg::LongInt(keeper)])
}

fn reminders<S: Store>(s: &Server<S>, owner: u64, keeper: u64, br: Broker) -> ServerMessage {
    let mut b = Builder::new();
    let me = b.reserve(CONSIGNER.0);
    let at = b.native(TEXT, NativeObject::TextInfo(progress::text(br.location)));
    let l = consignments_list(s, &mut b, owner, keeper);
    b.fill(
        me,
        &[
            Slot::Ref(Some(at)),
            Slot::Ref(Some(l)),
            Slot::Tagged(4, br.catalog),
        ],
    );
    call(
        function::SALE_REMINDERS,
        &[Arg::Object(b.graph()), Arg::LongInt(keeper)],
    )
}

/// Tells a reminder its client holds of these consignments at a broker what has become of each: has
/// it expired, how many have sold.
fn reminder_of(changed: &[Consignment], keeper: u64, now: f64) -> ServerMessage {
    let mut b = Builder::new();
    let list_id = b.next_id();
    b.native(REF_LIST, list(Elem::Ref, Vec::new()));
    let mut refs = Vec::new();
    for c in changed {
        let desc = b.reserve(CONSIGNMENT_DESC.0);
        b.fill(
            desc,
            &[
                Slot::Int(c.sale),
                Slot::Int(u32::from(c.expired(now))),
                Slot::Int(c.sold),
            ],
        );
        refs.push(Item::Ref(desc));
    }
    b.replace_native(list_id, list(Elem::Ref, refs));
    call(
        function::UPDATE_SALE_REMINDERS,
        &[Arg::Object(b.graph()), Arg::LongInt(keeper)],
    )
}

/// A seller in the world told what has become of these of its consignments at a broker. A client
/// shopping at that broker doesn't take the word: it takes the reminder whole, and its page shows
/// them only when sent again.
fn tell_seller<S: Store>(
    s: &Server<S>,
    net: &mut ServerNet,
    seller: ConnId,
    keeper: u64,
    changed: &[Consignment],
) {
    let Some(p) = s.players.get(&seller) else {
        return;
    };
    let me = p.character;
    let on_page = p.at_consignments == Some(keeper);
    progress::private(net, seller, &reminder_of(changed, keeper, s.game_now));
    let shopping = s
        .market
        .shoppers
        .iter()
        .any(|(w, k, _)| *w == seller && *k == keeper);
    if shopping || on_page {
        if let Some(br) = broker_of(s, keeper) {
            progress::private(net, seller, &reminders(s, me, keeper, br));
        }
    }
    if on_page {
        let m = consignment_message(s, function::UPDATE_CONSIGNMENT, me, keeper);
        progress::private(net, seller, &m);
    }
}

/// Sellers in the world told of what has gone past its time on sale since this was last looked at.
/// Nothing changes for that: a sale's time comes of the record as it stands.
pub fn look_for_expired<S: Store>(s: &mut Server<S>, net: &mut ServerNet) {
    if !s.market.loaded {
        return;
    }
    let now = s.game_now;
    let Some(last) = s.market.expiry_look else {
        s.market.expiry_look = Some(now);
        return;
    };
    if now - last < EXPIRY_LOOK {
        return;
    }
    s.market.expiry_look = Some(now);
    let mut told: BTreeMap<(u64, u64), Vec<Consignment>> = BTreeMap::new();
    for c in &s.market.list {
        if c.expired(now) && !c.expired(last) {
            told.entry((c.owner, c.keeper)).or_default().push(*c);
        }
    }
    for ((owner, keeper), changed) in told {
        let seller = s
            .players
            .iter()
            .find(|(_, q)| q.character == owner)
            .map(|(w, _)| *w);
        if let Some(seller) = seller {
            tell_seller(s, net, seller, keeper, &changed);
        }
    }
}

fn broker_of<S: Store>(s: &Server<S>, keeper: u64) -> Option<Broker> {
    let i = crate::presence::content_by_id(&s.content.objects, keeper)?;
    broker(s, s.content.objects[i].template)
}

/// Is a player near enough to a keeper of a store or a broker to deal with it?
#[must_use]
pub fn near<S: Store>(s: &Server<S>, world: ConnId, keeper: u64) -> bool {
    let Some(p) = s.players.get(&world) else {
        return false;
    };
    crate::presence::content_by_id(&s.content.objects, keeper)
        .map(|i| &s.content.objects[i])
        .and_then(|o| crate::places::reach(&p.position, o.cell, o.origin))
        .is_some_and(|d| d <= STAY_NEAR)
}

/// A store chosen from a catalog: its player sales.
pub fn enter_store<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[Value],
    now: LocalTime,
) {
    if !ensure(s) {
        return;
    }
    let (Some(keeper), Some(&store)) = (long_of(v, 0), ints(v).first()) else {
        return;
    };
    let Some(br) = broker_of(s, keeper) else {
        return;
    };
    if !near(s, world, keeper) {
        return;
    }
    let Some(m) = page_of(
        s,
        world,
        keeper,
        br.catalog,
        store,
        0,
        store::function::ENTER_STORE,
        now,
    ) else {
        return;
    };
    progress::private(net, world, &m);
    if let Some(p) = s.players.get_mut(&world) {
        p.shopping = Some((keeper, store, 0));
    }
}

/// A store's page of sales at a place in its listing, with the whole listing's size; none if the
/// connection's allowance doesn't stretch to it. A place past the listing's end is its last page.
#[allow(clippy::too_many_arguments)]
fn page_of<S: Store>(
    s: &mut Server<S>,
    world: ConnId,
    keeper: u64,
    catalog: u32,
    store: u32,
    pos: u32,
    f: u32,
    now: LocalTime,
) -> Option<ServerMessage> {
    let game_now = s.game_now;
    let listed: Vec<&Consignment> = s
        .market
        .list
        .iter()
        .filter(|c| c.catalog == catalog && c.store == store && c.on_sale(game_now))
        .collect();
    let size = listed.len();
    let pos = (pos as usize).min(size.saturating_sub(1));
    let sales: Vec<Consignment> = listed
        .into_iter()
        .skip(pos)
        .take(store::PAGE as usize)
        .copied()
        .collect();
    if !page_allowed(s, world, size, sales.len(), now) {
        return None;
    }
    let mut b = Builder::new();
    let viewr = b.reserve(store::package::STORE_VIEW);
    let l = b.next_id();
    b.native(REF_LIST, list(Elem::Ref, Vec::new()));
    let mut refs = Vec::new();
    for c in &sales {
        let (name, icon) = name_and_icon(s, c);
        refs.push(Item::Ref(sale_profile(&mut b, c, name, &icon)));
    }
    b.replace_native(l, list(Elem::Ref, refs));
    b.fill(
        viewr,
        &[
            Slot::Ref(Some(l)),
            Slot::Tagged(4, store),
            Slot::Tagged(4, u32::try_from(size).unwrap_or(0)),
            Slot::Int(u32::try_from(pos).unwrap_or(0)),
        ],
    );
    Some(call(f, &[Arg::Object(b.graph()), Arg::LongInt(keeper)]))
}

/// The client turns a broker's store's page: the next or the previous, by the page's sales; past
/// the listing's end it stays. Answered as a storekeeper's turn is.
pub fn turn_page<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    (keeper, store, pos): (u64, u32, u32),
    forward: bool,
    now: LocalTime,
) {
    let Some(br) = broker_of(s, keeper) else {
        return;
    };
    if !near(s, world, keeper) {
        return;
    }
    let game_now = s.game_now;
    let size = s
        .market
        .list
        .iter()
        .filter(|c| c.catalog == br.catalog && c.store == store && c.on_sale(game_now))
        .count();
    let size = u32::try_from(size).unwrap_or(u32::MAX);
    let next = if !forward {
        pos.saturating_sub(store::PAGE)
    } else if pos.saturating_add(store::PAGE) < size {
        pos + store::PAGE
    } else {
        pos
    };
    let f = store::function::UPDATE_STORE;
    let Some(m) = page_of(s, world, keeper, br.catalog, store, next, f, now) else {
        return;
    };
    progress::private(net, world, &m);
    if let Some(p) = s.players.get_mut(&world) {
        p.shopping = Some((keeper, store, next));
    }
}

/// Is this one a broker, whose store's sales are this market's?
#[must_use]
pub fn is_broker<S: Store>(s: &Server<S>, keeper: u64) -> bool {
    broker_of(s, keeper).is_some()
}

/// May a connection have a page now, of a store listing so many sales? What it costs goes by the
/// sales it shows and the listing read for it.
fn page_allowed<S: Store>(
    s: &mut Server<S>,
    world: ConnId,
    listed: usize,
    shown: usize,
    now: LocalTime,
) -> bool {
    rows_allowed(s, world, shown + 1 + listed / LISTED_A_ROW, now)
}

/// May a connection be sent so many rows now, out of its allowance? A request for more than it can
/// ever hold takes all it holds.
pub(crate) fn rows_allowed<S: Store>(
    s: &mut Server<S>,
    world: ConnId,
    rows: usize,
    now: LocalTime,
) -> bool {
    let Some(p) = s.players.get_mut(&world) else {
        return false;
    };
    #[allow(clippy::cast_precision_loss)]
    let cost = (rows as f64).min(PAGE_ROWS_HELD);
    p.page_rows
        .get_or_insert_with(|| Bucket::with(PAGE_ROWS_HELD, PAGE_ROWS_A_SECOND, now))
        .allow_for(cost, now)
}

/// The player's consignments at a broker, with its reminder.
pub fn enter_consignment<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[Value],
    now: LocalTime,
) {
    if !ensure(s) {
        return;
    }
    let Some(keeper) = long_of(v, 0) else { return };
    let Some(br) = broker_of(s, keeper) else {
        return;
    };
    if !near(s, world, keeper) {
        return;
    }
    let Some(me) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let mine = s.market.list.iter().filter(|c| c.owner == me).count();
    if !page_allowed(s, world, 0, mine, now) {
        return;
    }
    progress::private(net, world, &reminders(s, me, keeper, br));
    let m = consignment_message(s, function::ENTER_CONSIGNMENT_DONE, me, keeper);
    progress::private(net, world, &m);
    if let Some(p) = s.players.get_mut(&world) {
        p.at_consignments = Some(keeper);
    }
}

/// The client leaves the consignments page, or its catalog (not answered).
pub fn left_consignments<S: Store>(s: &mut Server<S>, world: ConnId) {
    if let Some(p) = s.players.get_mut(&world) {
        p.at_consignments = None;
    }
}

/// An item put up for sale: the charge taken, the item moved to the broker, the consignment shown.
pub fn sell<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[Value],
    clock: Clock,
) {
    if !ensure(s) {
        return;
    }
    // The values come last parameter first: the store, the price, how many, the item, the keeper.
    let w = ints(v);
    let (Some(keeper), Some(item)) = (long_of(v, 1), long_of(v, 0)) else {
        return;
    };
    let (store_id, price, quantity) = match w.as_slice() {
        [store_id, price, quantity] => (*store_id, *price, *quantity),
        _ => return,
    };
    #[allow(clippy::cast_precision_loss)]
    let price = price as f32;
    let Some(br) = broker_of(s, keeper) else {
        return;
    };
    if !near(s, world, keeper) {
        return;
    }
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let me = p.character;
    let Some(held) = p
        .play
        .items
        .iter()
        .find(|h| h.id == item && h.location == 0)
        .copied()
    else {
        return;
    };
    if crate::places::pack_spoken_for(s, world) {
        let m = call(
            store::function::REQUEST_DONE,
            &[Arg::Int(store::error::SALE_REFUSED)],
        );
        progress::private(net, world, &m);
        return;
    }
    if s.market.list.iter().filter(|c| c.owner == me).count() >= MOST_CONSIGNMENTS {
        let m = call(
            store::function::REQUEST_DONE,
            &[Arg::Int(store::error::SALE_REFUSED)],
        );
        progress::private(net, world, &m);
        let told = crate::chat::show_text(1, progress::text(AT_THE_MOST));
        progress::private(net, world, &told);
        return;
    }
    // An item bound to its seller stays with it, and so does one used in combat: its sale profile
    // does not make, and the client's own check answers the same.
    if s.things
        .get(item)
        .is_some_and(|t| inventory::attuned_now(s, t) || t.own.bound_to != 0)
    {
        let m = call(
            store::function::REQUEST_DONE,
            &[Arg::Int(store::error::SALE_REFUSED)],
        );
        progress::private(net, world, &m);
        return;
    }
    let quantity = quantity.clamp(1, held.quantity.max(1));
    // A part of a stack goes as a new item, which only its template can make.
    if quantity < held.quantity && s.catalog.get(held.template).is_none() {
        return;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let charge = ((f64::from(quantity) * f64::from(price) * CHARGE).ceil() as u32).max(1);
    let money = store::money(&p.qualities);
    if price <= 0.0 || money < charge {
        return;
    }
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let left = money - charge;
    crate::quals::set_int(&mut p.qualities, store::MONEY, left.cast_signed());
    progress::send_int(net, world, store::MONEY, left.cast_signed());
    // The item leaves the pack (a part of a stack goes as a new item).
    let consigned = if quantity >= held.quantity {
        p.play.items.retain(|h| h.id != item);
        inventory::close_up(&mut p.play.items);
        item
    } else {
        if let Some(h) = p.play.items.iter_mut().find(|h| h.id == item) {
            h.quantity -= quantity;
        }
        if let Some(t) = s.things.get_mut(item) {
            t.quantity -= quantity;
        }
        match inventory::split_off(s, item, quantity) {
            Some(n) => n,
            None => return,
        }
    };
    if let Some(t) = s.things.get_mut(consigned) {
        t.container = Some((keeper, 1));
    }
    let m = Move {
        move_type: MOVE_IN,
        item,
        split_item: consigned,
        quantity,
        from: me,
        actual_from: me,
        actual_from_slot: i32::try_from(held.slot).unwrap_or(-1),
        target: keeper,
        actual_target: keeper,
        target_slot: 1,
        actual_target_slot: 1,
        merge_container: keeper,
        merge_slot: 1,
        all_taken: quantity >= held.quantity,
        unlock: true,
        no_anim: true,
        grab: true,
        quiet: true,
        no_move: true,
        contain: true,
        auto_merge: true,
        template: held.template,
        split_quantity: quantity,
        ..Move::default()
    };
    progress::private(net, world, &inventory::move_done(&m));
    s.market.next_sale += 1;
    let sale = s.market.next_sale;
    s.market.list.push(Consignment {
        sale,
        owner: me,
        keeper,
        catalog: br.catalog,
        store: store_id,
        item: consigned,
        template: held.template,
        price,
        offered: quantity,
        sold: 0,
        entered: net.game_time(clock.local),
    });
    keep(s);
    progress::private(net, world, &reminders(s, me, keeper, br));
    let m = consignment_message(s, function::UPDATE_CONSIGNMENT, me, keeper);
    progress::private(net, world, &m);
    crate::world::keep_player(s, world);
}

/// More of a consignment's item put up with it, for the charge a sale of those would take.
pub fn restock<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[Value],
    clock: Clock,
) {
    if !ensure(s) {
        return;
    }
    // The values come last parameter first: the sale, how many, the item, the keeper.
    let w = ints(v);
    let (Some(keeper), Some(item)) = (long_of(v, 1), long_of(v, 0)) else {
        return;
    };
    let (sale, quantity) = match w.as_slice() {
        [sale, quantity] => (*sale, *quantity),
        _ => return,
    };
    let Some(br) = broker_of(s, keeper) else {
        return;
    };
    if !near(s, world, keeper) {
        return;
    }
    let refuse = |net: &mut ServerNet, error: u32| {
        progress::private(
            net,
            world,
            &call(store::function::REQUEST_DONE, &[Arg::Int(error)]),
        );
    };
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let me = p.character;
    let money = store::money(&p.qualities);
    let held = p
        .play
        .items
        .iter()
        .find(|h| h.id == item && h.location == 0)
        .copied();
    let row = s
        .market
        .list
        .iter()
        .position(|c| c.sale == sale && c.owner == me && c.keeper == keeper && c.in_stock());
    let (Some(held), Some(row)) = (held, row) else {
        return refuse(net, store::error::SALE_REFUSED);
    };
    let c = s.market.list[row];
    let left = c.offered - c.sold;
    let most = s
        .catalog
        .get(c.template)
        .and_then(|t| t.props.int("Item_MaxStackSize"))
        .unwrap_or(0);
    if crate::places::pack_spoken_for(s, world)
        || quantity == 0
        || quantity > held.quantity.max(1)
        || held.template != c.template
        || !inventory::same_numbers(s, item, c.item)
        || i64::from(left) + i64::from(quantity) >= most
    {
        return refuse(net, store::error::SALE_REFUSED);
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let charge = ((f64::from(quantity) * f64::from(c.price) * CHARGE).ceil() as u32).max(1);
    if money < charge {
        return refuse(net, store::error::NOT_ENOUGH_COIN);
    }
    let all_taken = quantity >= held.quantity.max(1);
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let left_money = money - charge;
    crate::quals::set_int(&mut p.qualities, store::MONEY, left_money.cast_signed());
    progress::send_int(net, world, store::MONEY, left_money.cast_signed());
    if all_taken {
        p.play.items.retain(|h| h.id != item);
        inventory::close_up(&mut p.play.items);
    } else if let Some(h) = p.play.items.iter_mut().find(|h| h.id == item) {
        h.quantity -= quantity;
    }
    if let Some(t) = s.things.get_mut(c.item) {
        t.quantity = left + quantity;
    }
    if !all_taken {
        if let Some(t) = s.things.get_mut(item) {
            t.quantity -= quantity;
        }
    }
    s.market.list[row].offered += quantity;
    s.market.list[row].entered = net.game_time(clock.local);
    let m = Move {
        move_type: MOVE_IN,
        item,
        split_item: c.item,
        quantity,
        from: me,
        actual_from: me,
        actual_from_slot: i32::try_from(held.slot).unwrap_or(-1),
        target: keeper,
        actual_target: keeper,
        target_slot: 1,
        actual_target_slot: 1,
        merge_container: keeper,
        merge_slot: 1,
        all_taken,
        unlock: true,
        no_anim: true,
        grab: true,
        quiet: true,
        no_move: true,
        contain: true,
        auto_merge: true,
        template: held.template,
        split_quantity: quantity,
        ..Move::default()
    };
    progress::private(net, world, &inventory::move_done(&m));
    if all_taken {
        // What was its own stack is the sale's now.
        crate::death::vanish(s, net, item);
    } else if s.things.get(item).is_some() {
        let told = s.things.get(item).map_or(0, |t| t.quantity);
        let m = ServerMessage::UpdateIntVisual(auberean_protocol::quality::VisualQuality {
            object: inventory::visual(s, item),
            quality: crate::craft::QUANTITY,
            value: i32::try_from(told).unwrap_or(i32::MAX),
        });
        inventory::to_holder(s, net, world, item, &m);
    }
    keep(s);
    progress::private(net, world, &reminders(s, me, keeper, br));
    let m = consignment_message(s, function::UPDATE_CONSIGNMENT, me, keeper);
    progress::private(net, world, &m);
    crate::world::keep_player(s, world);
}

/// A consignment collected: an unsold one's item back as a new item holding all the old one held,
/// a sold one's price paid.
pub fn collect<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, v: &[Value]) {
    if !ensure(s) {
        return;
    }
    // The values come last parameter first: the profits flag, the sale, the keeper.
    let w = ints(v);
    let Some(keeper) = long_of(v, 0) else { return };
    let (profits, sale) = match w.as_slice() {
        [profits, sale] => (*profits != 0, *sale),
        _ => return,
    };
    let Some(me) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    if !near(s, world, keeper) {
        return;
    }
    let Some(i) = s
        .market
        .list
        .iter()
        .position(|c| c.sale == sale && c.owner == me && c.keeper == keeper)
    else {
        return;
    };
    let c = s.market.list[i];
    if profits {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let paid = (f64::from(c.sold) * f64::from(c.price)).floor() as u32;
        if let Some(p) = s.players.get_mut(&world) {
            let money = store::money(&p.qualities).saturating_add(paid);
            crate::quals::set_int(&mut p.qualities, store::MONEY, money.cast_signed());
            progress::send_int(net, world, store::MONEY, money.cast_signed());
        }
        if c.sold >= c.offered {
            // The item itself is its buyer's now.
            s.market.list.remove(i);
        } else {
            s.market.list[i].offered -= c.sold;
            s.market.list[i].sold = 0;
        }
    } else {
        let left = c.offered.saturating_sub(c.sold);
        // A sold item is its buyer's, and its price stays to be collected. What comes back needs
        // its template to be made from and a place in the pack. Anything else leaves the sale up.
        let made = s.catalog.get(c.template).is_some();
        let room = s
            .players
            .get(&world)
            .is_some_and(|p| inventory::fits_in_pack(&p.play.items, 1));
        let refused = if left == 0 || !made {
            Some(store::error::COLLECT_FAILED)
        } else if room {
            None
        } else {
            Some(store::error::NO_ROOM)
        };
        if let Some(error) = refused {
            let m = call(store::function::REQUEST_DONE, &[Arg::Int(error)]);
            progress::private(net, world, &m);
            let m = consignment_message(s, function::UPDATE_CONSIGNMENT, me, keeper);
            progress::private(net, world, &m);
            return;
        }
        // What sold of it stays to be paid.
        if c.sold > 0 {
            s.market.list[i].offered = c.sold;
        } else {
            s.market.list.remove(i);
        }
        let stamp = s
            .things
            .get(c.item)
            .map(|t| ObjectOnly(inventory::stamp_of_thing(t)));
        let kept = s
            .things
            .get(c.item)
            .map(crate::state::ItemState::of)
            .or_else(|| {
                s.market
                    .item_states
                    .iter()
                    .find(|k| k.id == c.item)
                    .cloned()
            });
        s.things.remove(c.item);
        if let Some(stamp) = stamp {
            net.send(
                world,
                queue::EVENT,
                BlobKind::UNORDERED,
                ServerMessage::ForgetObject(stamp).encode(),
            );
        }
        inventory::collected(s, net, world, c.template, left, keeper, kept.as_ref());
    }
    keep(s);
    let m = consignment_message(s, function::UPDATE_CONSIGNMENT, me, keeper);
    progress::private(net, world, &m);
    crate::world::keep_player(s, world);
}

/// A character is gone for good: what it has with brokers goes with it. While the store can't answer
/// nothing is done, and its rows only wait for no one.
pub fn seller_gone<S: Store>(s: &mut Server<S>, net: &mut ServerNet, character: u64) {
    if !ensure(s) {
        return;
    }
    let unsold: Vec<u64> = s
        .market
        .list
        .iter()
        .filter(|c| c.owner == character && c.in_stock())
        .map(|c| c.item)
        .collect();
    let before = s.market.list.len();
    s.market.list.retain(|c| c.owner != character);
    if s.market.list.len() == before {
        return;
    }
    // A sold row's item is its buyer's, and stays.
    for item in unsold {
        crate::death::vanish(s, net, item);
    }
    keep(s);
}

/// A player entering the world is reminded of its consignments, one reminder per broker.
pub fn enter<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    if !ensure(s) {
        return;
    }
    let Some(me) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let mut keepers: Vec<u64> = s
        .market
        .list
        .iter()
        .filter(|c| c.owner == me)
        .map(|c| c.keeper)
        .collect();
    keepers.sort_unstable();
    keepers.dedup();
    for keeper in keepers {
        if let Some(br) = broker_of(s, keeper) {
            progress::private(net, world, &reminders(s, me, keeper, br));
        }
    }
}

/// A purchase of a player's sale, when the item named is one: the price taken, the item moved from
/// the broker into the buyer's pack, its stock told as gone. Returns whether it was one.
pub fn purchase<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    item: u64,
    keeper: u64,
) -> bool {
    if !ensure(s) {
        return false;
    }
    // A sold row keeps its item's number until its price is collected, and the buyer may have put
    // the item up again.
    let of_item = |c: &Consignment| c.item == item && c.keeper == keeper;
    let Some(i) = s
        .market
        .list
        .iter()
        .position(|c| of_item(c) && c.in_stock())
        .or_else(|| s.market.list.iter().position(of_item))
    else {
        return false;
    };
    let c = s.market.list[i];
    let left = c.offered.saturating_sub(c.sold);
    if !near(s, world, keeper) {
        let m = call(
            store::function::REQUEST_DONE,
            &[Arg::Int(store::error::NOT_FOUND)],
        );
        progress::private(net, world, &m);
        return true;
    }
    let full = s
        .players
        .get(&world)
        .is_none_or(|p| !inventory::fits_in_pack(&p.play.items, 1));
    if full {
        let m = call(
            store::function::REQUEST_DONE,
            &[Arg::Int(store::error::NO_ROOM)],
        );
        progress::private(net, world, &m);
        return true;
    }
    // An item its template no longer makes isn't in the world to be bought, and one past its time
    // isn't on sale.
    if left == 0 || s.things.get(item).is_none() || c.expired(s.game_now) {
        progress::private(
            net,
            world,
            &call(
                store::function::REQUEST_DONE,
                &[Arg::Int(store::error::NOT_FOUND)],
            ),
        );
        progress::private(
            net,
            world,
            &call(
                function::UPDATE_SALE,
                &[Arg::Int(0), Arg::LongInt(item), Arg::LongInt(keeper)],
            ),
        );
        return true;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let price = (f64::from(left) * f64::from(c.price)).ceil() as u32;
    let Some(p) = s.players.get_mut(&world) else {
        return true;
    };
    let money = store::money(&p.qualities);
    if money < price {
        progress::private(
            net,
            world,
            &call(
                store::function::REQUEST_DONE,
                &[Arg::Int(store::error::NOT_ENOUGH_COIN)],
            ),
        );
        return true;
    }
    crate::quals::set_int(
        &mut p.qualities,
        store::MONEY,
        (money - price).cast_signed(),
    );
    progress::send_int(net, world, store::MONEY, (money - price).cast_signed());
    s.market.list[i].sold = c.offered;
    // The item moves from the broker into the buyer's pack.
    let me = p.character;
    if let Some(t) = s.things.get_mut(item) {
        t.container = None;
    }
    let mut slot = 0;
    if let Some(p) = s.players.get_mut(&world) {
        slot = inventory::room_for_one(&mut p.play.items);
        p.play.items.push(Held {
            id: item,
            template: c.template,
            slot,
            location: 0,
            quantity: left,
        });
        if let Some(t) = s.things.get_mut(item) {
            t.container = Some((me, slot));
        }
    }
    let stamp = inventory::contain_stamp(s, item);
    net.send(
        world,
        queue::EVENT,
        BlobKind::UNORDERED,
        ServerMessage::Contain(ObjectOnly(stamp)).encode(),
    );
    let m = Move {
        move_type: MOVE_OUT,
        item,
        split_item: item,
        quantity: left,
        from: keeper,
        actual_from: keeper,
        actual_from_slot: 1,
        from_slot: -1,
        target: me,
        actual_target: me,
        target_slot: i32::try_from(slot).unwrap_or(0),
        actual_target_slot: i32::try_from(slot).unwrap_or(0),
        merge_slot: -1,
        all_taken: true,
        no_anim: true,
        grab: true,
        unlock: true,
        contain: true,
        template: c.template,
        ..Move::default()
    };
    progress::private(net, world, &inventory::move_done(&m));
    progress::private(
        net,
        world,
        &call(
            function::UPDATE_SALE,
            &[Arg::Int(0), Arg::LongInt(item), Arg::LongInt(keeper)],
        ),
    );
    if let Some(t) = s.things.get(item) {
        let m = ServerMessage::CreateObject(inventory::item_create(t));
        net.send(world, queue::EVENT, BlobKind::UNORDERED, m.encode());
    }
    let seller = s
        .players
        .iter()
        .find(|(_, q)| q.character == c.owner)
        .map(|(w, _)| *w);
    if let Some(seller) = seller {
        let sold = Consignment {
            sold: c.offered,
            ..c
        };
        tell_seller(s, net, seller, keeper, &[sold]);
    }
    keep(s);
    crate::world::keep_player(s, world);
    true
}

/// A text of the market's location, as a string reference.
#[must_use]
pub fn location_text(b: Broker) -> TextInfo {
    progress::text(b.location)
}
