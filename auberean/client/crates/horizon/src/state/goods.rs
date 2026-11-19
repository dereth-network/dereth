//! The player's things as Horizon draws them: the pack, what is worn, the money, the containers
//! the player has open, a trade with another player, and the shops, the market's catalog and the
//! player's consignments, followed each frame from the view's inventory and store and the
//! scripts' notices.

use std::collections::BTreeMap;

use auberean_client_sdk::contract::notice::{
    Notice, Said, Sale, SelectionNotice, Store, StoreNotice, TradeNotice,
};
use auberean_client_sdk::contract::text::TextRef;
use auberean_client_sdk::contract::view::{Inventory, Item, View};

/// The places of the pack's four segments a player's pack starts with: the pack itself, then
/// three bags. The last bag takes up whatever the pack holds beyond them.
pub const ROOMS: [u32; 4] = [42, 12, 12, 12];

/// The segments of the pack, as the inventory window's tabs name them.
pub const SEGMENTS: usize = 4;

/// A sale's state, as the scripts give it.
pub mod sale_state {
    /// The player may not buy it.
    pub const RESTRICTED: u32 = 1;
    /// A consignment whose time ran out unsold, and one sold.
    pub const EXPIRED: u32 = 2;
    pub const SOLD: u32 = 3;
}

/// An item offered in a trade: which, and how many.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Offer {
    pub item: u64,
    pub quantity: u32,
}

/// A trade with another player, from when the scripts set it up until it closes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Trade {
    pub partner: u64,
    pub partner_name: String,
    /// The player asked for it (and waits for the partner).
    pub asked_by_player: bool,
    /// The trade is open: both sides may offer.
    pub open: bool,
    pub mine: Vec<Offer>,
    pub theirs: Vec<Offer>,
    /// Each side accepted the trade as it stands.
    pub mine_accepted: bool,
    pub theirs_accepted: bool,
    /// The player pressed accept or decline and waits for the scripts to say how it stands.
    pub answering: bool,
    /// The partner closed it: the window says so until the player closes it.
    pub ended_by_partner: bool,
}

/// A sale of a shop as Horizon shows it: the sale, its words written.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SaleShown {
    pub sale: Sale,
    pub description: String,
    pub cost: String,
}

impl SaleShown {
    /// Whether it may not be bought: restricted, or none left (a stock below none has no end).
    #[must_use]
    pub const fn unavailable(&self) -> bool {
        self.sale.state == sale_state::RESTRICTED || self.sale.stock == 0
    }
}

/// A shop, or the player's consignments, as Horizon shows it: the shop, its words written.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShopShown {
    pub store: Store,
    pub name: String,
    pub description: String,
    pub sales: Vec<SaleShown>,
}

/// The market's catalog: its header and its shops.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Catalog {
    pub header: String,
    pub shops: Vec<ShopShown>,
}

/// A purchase the scripts ask the player to confirm: so many of a product for so much of what it
/// costs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Purchase {
    pub product: String,
    pub quantity: u32,
    pub cost: String,
    pub cost_quantity: u32,
}

/// The player's things, as Horizon draws them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Goods {
    /// The pack, what is worn, the money and the containers open, as the view last gave them.
    pub inventory: Option<Inventory>,
    /// What is selected.
    pub selection: u64,
    /// An item waits to be used on a target.
    pub targeting: bool,
    /// The containers other than the pack the player has open, by when they opened, each with its
    /// name.
    pub opened: Vec<(u64, String)>,
    pub trade: Option<Trade>,
    pub shop: Option<ShopShown>,
    pub catalog: Option<Catalog>,
    pub consignments: Option<ShopShown>,
    /// A purchase waiting to be confirmed.
    pub purchase: Option<Purchase>,
    /// A sale of the player's waits to be collected.
    pub sale_waiting: bool,
    /// The most of each sale's item the player may buy, as the scripts last answered.
    pub max_purchase: BTreeMap<u64, u32>,
    /// What a consignment costs to put up, a share of its price.
    pub consignment_charge: f32,
    /// The shops as the notices gave them, written again each frame.
    shop_store: Option<Store>,
    catalog_store: Option<(Option<TextRef>, Vec<Store>)>,
    consignment_store: Option<Store>,
    purchase_said: Option<(Option<TextRef>, u32, Option<TextRef>, u32)>,
}

/// What `t` says, or nothing while its strings are not at hand.
fn written(t: &TextRef, write: &dyn Fn(&TextRef) -> Option<String>) -> String {
    write(t).unwrap_or_default()
}

/// A shop with its words written.
fn shown(s: &Store, write: &dyn Fn(&TextRef) -> Option<String>) -> ShopShown {
    ShopShown {
        store: s.clone(),
        name: written(&s.name, write),
        description: written(&s.description, write),
        sales: s
            .sales
            .iter()
            .map(|sale| SaleShown {
                sale: sale.clone(),
                description: written(&sale.description, write),
                cost: written(&sale.cost_description, write),
            })
            .collect(),
    }
}

impl Goods {
    /// The player's id, while its pack is here.
    #[must_use]
    pub fn player(&self) -> Option<u64> {
        self.inventory
            .as_ref()
            .map(|i| i.player)
            .filter(|p| *p != 0)
    }

    /// An item, as the view gives it.
    #[must_use]
    pub fn item(&self, id: u64) -> Option<&Item> {
        self.inventory.as_ref()?.items.get(&id)
    }

    /// The money the player has.
    #[must_use]
    pub fn money(&self) -> u32 {
        self.inventory
            .as_ref()
            .map_or(0, |i| u32::try_from(i.money).unwrap_or(0))
    }

    /// What is worn at a place.
    #[must_use]
    pub fn worn_at(&self, location: u32) -> Option<u64> {
        self.inventory
            .as_ref()?
            .worn
            .iter()
            .find(|(l, _)| *l == location)
            .map(|(_, id)| *id)
    }

    /// How many items a segment of the pack holds now, as the scripts say.
    #[must_use]
    pub fn segment_count(&self, segment: usize) -> u32 {
        self.inventory
            .as_ref()
            .and_then(|i| i.segments.get(segment))
            .map_or(0, |(_, n)| *n)
    }

    /// The places of each of the pack's segments: the pack's own and the first two bags' as they
    /// always are, and the last bag the rest of what the pack holds (its own when the pack's size
    /// is not known).
    #[must_use]
    pub fn rooms(&self) -> [u32; SEGMENTS] {
        let mut rooms = ROOMS;
        let size = self
            .inventory
            .as_ref()
            .and_then(|i| i.containers.get(&i.player))
            .map_or(0, |c| c.size);
        let before: u32 = rooms[..SEGMENTS - 1].iter().sum();
        if size > before {
            rooms[SEGMENTS - 1] = size - before;
        }
        rooms
    }

    /// The slot a segment's first place is: the places of the segments before it.
    #[must_use]
    pub fn first_slot(&self, segment: usize) -> u32 {
        self.rooms()[..segment.min(SEGMENTS)].iter().sum()
    }

    /// The items a container shows in a segment, in their order (the pack's four segments; any
    /// other container's one).
    #[must_use]
    pub fn contents(&self, container: u64, segment: usize) -> Vec<u64> {
        let Some(c) = self
            .inventory
            .as_ref()
            .and_then(|i| i.containers.get(&container))
            .and_then(|c| c.segments.get(segment))
        else {
            return Vec::new();
        };
        let first = usize::try_from(c.first).unwrap_or(usize::MAX);
        let count = usize::try_from(c.count).unwrap_or(0);
        c.items.iter().skip(first).take(count).copied().collect()
    }

    /// How many places an opened container has.
    #[must_use]
    pub fn size(&self, container: u64) -> u32 {
        self.inventory
            .as_ref()
            .and_then(|i| i.containers.get(&container))
            .map_or(0, |c| c.size)
    }

    /// A shop's sale of an item.
    #[must_use]
    pub fn sale(&self, item: u64) -> Option<&SaleShown> {
        self.shop
            .as_ref()?
            .sales
            .iter()
            .find(|s| s.sale.item == item)
    }

    /// Follow one frame: the view's inventory and store as last refreshed, the names of the
    /// containers open, and the scripts' notices about trades and shops.
    pub fn follow(
        &mut self,
        view: Option<&View>,
        notices: &[Notice],
        write: &dyn Fn(&TextRef) -> Option<String>,
    ) {
        let Some(view) = view else {
            *self = Self::default();
            return;
        };
        match &view.inventory {
            Some(inv) => {
                if self.inventory.as_ref().map(|i| i.version) != Some(inv.version) {
                    self.inventory = Some(inv.clone());
                }
            }
            None => self.inventory = None,
        }
        if let Some(store) = &view.store {
            self.max_purchase.clone_from(&store.max_purchase);
            self.consignment_charge = store.consignment_charge;
        }
        let world = view.world.as_ref();
        self.selection = world.map_or(0, |w| w.selection.current);
        let player = self.player();
        let open: Vec<u64> = self.inventory.as_ref().map_or_else(Vec::new, |i| {
            i.containers
                .keys()
                .copied()
                .filter(|c| Some(*c) != player)
                .collect()
        });
        self.opened.retain(|(c, _)| open.contains(c));
        for c in open {
            if !self.opened.iter().any(|(o, _)| *o == c) {
                self.opened.push((c, String::new()));
            }
        }
        for (c, name) in &mut self.opened {
            let thing = world.and_then(|w| w.things.iter().find(|t| t.id == *c));
            if let Some(t) = thing {
                *name = t
                    .name
                    .clone()
                    .filter(|n| !n.is_empty())
                    .or_else(|| t.name_ref.as_ref().and_then(write))
                    .or_else(|| write(&t.own_name))
                    .unwrap_or_default();
            }
        }
        for n in notices {
            self.notice(n, player.unwrap_or(0));
        }
        self.shop = self.shop_store.as_ref().map(|s| shown(s, write));
        self.consignments = self.consignment_store.as_ref().map(|s| shown(s, write));
        self.catalog = self.catalog_store.as_ref().map(|(h, shops)| Catalog {
            header: h.as_ref().map(|h| written(h, write)).unwrap_or_default(),
            shops: shops.iter().map(|s| shown(s, write)).collect(),
        });
        self.purchase = self.purchase_said.as_ref().map(|(p, n, c, q)| Purchase {
            product: p.as_ref().map(|p| written(p, write)).unwrap_or_default(),
            quantity: *n,
            cost: c.as_ref().map(|c| written(c, write)).unwrap_or_default(),
            cost_quantity: *q,
        });
        if let Some(t) = &mut self.trade {
            if t.partner_name.is_empty() {
                if let Some(thing) = world.and_then(|w| w.things.iter().find(|x| x.id == t.partner))
                {
                    t.partner_name = thing
                        .name
                        .clone()
                        .or_else(|| thing.name_ref.as_ref().and_then(write))
                        .unwrap_or_default();
                }
            }
        }
    }

    /// The purchase question answered: it goes.
    pub fn purchase_answered(&mut self) {
        self.purchase_said = None;
        self.purchase = None;
    }

    /// The trade's window closed by the player after the partner closed the trade.
    pub fn trade_dismissed(&mut self) {
        if self.trade.as_ref().is_some_and(|t| t.ended_by_partner) {
            self.trade = None;
        }
    }

    /// The player pressed accept or decline in the trade.
    pub fn trade_answering(&mut self) {
        if let Some(t) = &mut self.trade {
            t.answering = true;
        }
    }

    fn notice(&mut self, n: &Notice, player: u64) {
        match &n.said {
            Said::Selection(SelectionNotice::Targeting { waiting }) => self.targeting = *waiting,
            Said::Trade(t) => self.trade_notice(t, player),
            Said::Store(s) => self.store_notice(s),
            _ => {}
        }
    }

    fn trade_notice(&mut self, n: &TradeNotice, player: u64) {
        match n {
            TradeNotice::Registered {
                asker,
                asked,
                partner: _,
            } => {
                let asked_by_player = *asker == player;
                self.trade = Some(Trade {
                    partner: if asked_by_player { *asked } else { *asker },
                    asked_by_player,
                    ..Trade::default()
                });
            }
            TradeNotice::Opened => {
                let t = self.trade.get_or_insert_with(Trade::default);
                t.open = true;
                t.ended_by_partner = false;
            }
            TradeNotice::Closed { by, .. } => {
                if *by == player || self.trade.as_ref().is_none_or(|t| !t.open) {
                    self.trade = None;
                } else if let Some(t) = &mut self.trade {
                    t.open = false;
                    t.ended_by_partner = true;
                    t.mine.clear();
                    t.theirs.clear();
                    t.mine_accepted = false;
                    t.theirs_accepted = false;
                    t.answering = false;
                }
            }
            TradeNotice::Accepted { by } | TradeNotice::Declined { by } => {
                let accepted = matches!(n, TradeNotice::Accepted { .. });
                if let Some(t) = &mut self.trade {
                    if *by == player {
                        t.mine_accepted = accepted;
                        t.answering = false;
                    } else if *by == 0 {
                        t.answering = false;
                    } else {
                        t.theirs_accepted = accepted;
                    }
                }
            }
            TradeNotice::Emptied => {
                if let Some(t) = &mut self.trade {
                    t.mine.clear();
                    t.theirs.clear();
                }
            }
            TradeNotice::AcceptanceCleared => {
                if let Some(t) = &mut self.trade {
                    t.mine_accepted = false;
                    t.theirs_accepted = false;
                    t.answering = false;
                }
            }
            TradeNotice::Offered { by, item, quantity } => {
                if let Some(t) = &mut self.trade {
                    let side = if *by == player {
                        &mut t.mine
                    } else {
                        &mut t.theirs
                    };
                    side.retain(|o| o.item != *item);
                    side.push(Offer {
                        item: *item,
                        quantity: *quantity,
                    });
                }
            }
            TradeNotice::Withdrawn { by, item } => {
                if let Some(t) = &mut self.trade {
                    let side = if *by == player {
                        &mut t.mine
                    } else {
                        &mut t.theirs
                    };
                    side.retain(|o| o.item != *item);
                }
            }
            _ => {}
        }
    }

    fn store_notice(&mut self, n: &StoreNotice) {
        match n {
            StoreNotice::Catalog { header, shops } => {
                self.catalog_store = Some((header.clone(), shops.clone()));
            }
            StoreNotice::CatalogLeft => self.catalog_store = None,
            StoreNotice::Shown(s) | StoreNotice::Changed(s) => {
                if let Some(s) = s {
                    self.shop_store = Some((**s).clone());
                }
            }
            StoreNotice::Stock { item, stock } => {
                if let Some(sale) = self
                    .shop_store
                    .as_mut()
                    .and_then(|s| s.sales.iter_mut().find(|s| s.item == *item))
                {
                    sale.stock = *stock;
                }
            }
            StoreNotice::Left => {
                self.shop_store = None;
                self.purchase_said = None;
            }
            StoreNotice::Purchase {
                product,
                quantity,
                cost,
                cost_quantity,
            } => {
                self.purchase_said =
                    Some((product.clone(), *quantity, cost.clone(), *cost_quantity));
            }
            StoreNotice::Consignments(s) | StoreNotice::ConsignmentsChanged(s) => {
                if let Some(s) = s {
                    self.consignment_store = Some((**s).clone());
                }
            }
            StoreNotice::ConsignmentsLeft => self.consignment_store = None,
            StoreNotice::SaleWaiting { waiting } => self.sale_waiting = *waiting,
            _ => {}
        }
    }
}
