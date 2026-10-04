//! Vendors: `VendorProfile`, the exact price formulas and the acceptability test.
//!
//! Three things matter for fidelity in the price formulas:
//!
//! 1. the stored `f32` rate is widened before multiplication; the arithmetic uses **`f64`**
//!    through `floor`/`ceil`, then truncates to an integer;
//! 2. the `± 0.1` fudge is applied **inside** the rounding, not outside;
//! 3. trade notes ignore the vendor's rate entirely — bought at face value, sold at **1.15×**.
//!
//! The pure rules of this module live in [`dereth_rules::vendor`]; they are re-exported
//! here, so every `dereth_client_model::vendor::{buy_price, sell_price}` path resolves.

use crate::weenie::bitfield;
use dereth_primitives::{DataId, ObjectId};
use dereth_protocol::types::PublicWeenieDesc;

/// `ShopEvent`.
pub const SE_BUY: u32 = 0;
pub const SE_SELL: u32 = 1;

/// **`PropertyInt` 20 `CoinValue`** — the player's purse, and the only quality the vendor window
/// reads. The window registers a player quality handler for the same `0x14`.
pub const COIN_VALUE: u32 = 20;

/// `ShopMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum ShopMode {
    #[default]
    Undef = 0,
    None = 1,
    Buy = 2,
    Sell = 3,
}

/// `VendorProfile` (0x28).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VendorProfile {
    /// An `ITEM_TYPE` mask of what the shop deals in.
    pub item_types: u32,
    /// Minimum unit value it will buy; `-1` = no limit.
    pub min_value: i32,
    /// Maximum unit value it will buy; `-1` = no limit.
    pub max_value: i32,
    pub magic: u32,
    /// The multiplier applied when the **vendor buys from you**.
    pub buy_price: f32,
    /// The multiplier applied when the **vendor sells to you**.
    pub sell_price: f32,
    /// An alternative currency's `DataID`; `INVALID_DID` = pyreals.
    pub trade_id: DataId,
    pub trade_num: u32,
    pub trade_name: String,
}

/// `ItemProfile` (0x10).
#[derive(Debug, Clone, PartialEq)]
pub struct ItemProfile {
    pub amount: i32,
    pub iid: ObjectId,
    pub pwd: PublicWeenieDesc,
}

pub use dereth_rules::vendor::*;

impl VendorProfile {
    /// The price charged to the player.
    ///
    /// `value` on a stack is the **whole stack's** value, so the unit value is divided out first
    /// and the count passed separately.
    #[must_use]
    pub fn vendor_sell_price(&self, pwd: &PublicWeenieDesc, count: i32) -> i32 {
        let value = i32::try_from(pwd.value.unwrap_or(0)).unwrap_or(i32::MAX);
        let n = i32::from(pwd.stack_size.unwrap_or(0));
        let unit = if n != 0 { value / n } else { value };
        sell_price(unit, pwd.obj_type, self.sell_price, count)
    }

    /// The price the vendor pays the player.
    ///
    /// A refused item prices at **0**.
    #[must_use]
    pub fn vendor_buy_price(&self, pwd: &PublicWeenieDesc) -> i32 {
        if self.inq_acceptability(pwd) != 0 {
            return 0;
        }
        let value = i32::try_from(pwd.value.unwrap_or(0)).unwrap_or(i32::MAX);
        let n = i32::from(pwd.stack_size.unwrap_or(0));
        if n != 0 {
            buy_price(value / n, pwd.obj_type, self.buy_price, n)
        } else {
            buy_price(value, pwd.obj_type, self.buy_price, 1)
        }
    }

    /// Determine whether the vendor accepts this item.
    ///
    /// Returns 0 for "acceptable" and a non-zero reason code otherwise. The "too valuable" arm's
    /// return value is the client's own odd expression `~(pwd.obj_type >> 16) & 4`, not a constant —
    /// No known case distinguishes it from a plain 4. Transcribed
    /// literally rather than simplified.
    #[must_use]
    pub fn inq_acceptability(&self, pwd: &PublicWeenieDesc) -> u32 {
        if pwd.obj_type & self.item_types == 0 && pwd.bitfield & bitfield::CANNOT_BE_SALVAGED == 0 {
            return 1; // wrong type for this shop
        }
        let value = i32::try_from(pwd.value.unwrap_or(0)).unwrap_or(i32::MAX);
        let n = i32::from(pwd.stack_size.unwrap_or(0));
        let unit = if n != 0 { value / n } else { value };
        if unit == 0 {
            return 2; // no value
        }
        if self.max_value != -1 && unit > self.max_value {
            return !(pwd.obj_type >> 16) & 4; // too valuable
        }
        if self.min_value != -1 && unit < self.min_value {
            return 3; // too cheap
        }
        0
    }

    /// The boolean wrapper around [`Self::inq_acceptability`].
    #[must_use]
    pub fn is_acceptable(&self, pwd: &PublicWeenieDesc) -> bool {
        self.inq_acceptability(pwd) == 0
    }

    /// The client's message for a reason code.
    #[must_use]
    pub fn refusal_message(&self, pwd: &PublicWeenieDesc) -> Option<&'static str> {
        match self.inq_acceptability(pwd) {
            0 => None,
            1 => Some("That item cannot be sold here"),
            2 => Some("That item has no value and cannot be sold"),
            3 => Some("That item is too cheap to sell here"),
            _ => Some("That item is too valuable to sell here"),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The open shop and its three lists.
//
// This is the state that `dereth_protocol::trade::VendorInfo`'s handler fills, that the
// `VendorBuy`/`VendorSell` senders read, and that `toolbar::splitter`'s vendor arm and
// `use_object`'s two vendor guards consult; it is also `Weenie::sell_state`'s writer.
// ---------------------------------------------------------------------------------------------

/// Open-shop state, stock and the buy/sell baskets.
///
/// The pending vendor/item ids and open-vendor id share a model with the profile, stock and
/// baskets. The panel lives in `dereth-ui-screens`, and both interaction and display use this state.
///
/// **`vendor_id` has three readers.**
/// The selection-change splitter seed, the use-object
/// "a vendor's stock is bought, not used" guard and the client's
/// vendor-closing step all read it; the `0x0062` handler writes it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Shop {
    /// `None` is the client's 0, i.e. no vendor window open.
    pub vendor_id: Option<ObjectId>,
    /// Buying normally, selling when the window was
    /// opened by dragging an item onto the vendor.
    pub mode: ShopMode,
    /// The profile carried by `0x0062`.
    pub profile: VendorProfile,
    /// Stock in the order `0x0062` listed it.
    pub stock: Vec<ItemProfile>,
    /// Buy basket: object ids and requested counts.
    pub buy_list: Vec<(ObjectId, i32)>,
    /// Sell basket.
    pub sell_list: Vec<(ObjectId, i32)>,
    /// Descriptions retained with basket rows after stock or owned objects disappear.
    pub basket_descriptions: std::collections::BTreeMap<ObjectId, PublicWeenieDesc>,
    /// the vendor a pending open was aimed at.
    pub attempt_open_vendor: Option<ObjectId>,
    /// the item dragged onto that vendor.
    pub attempt_sale_object: Option<ObjectId>,
    /// the client's cached count of the player's coin.
    ///
    /// **Advisory only**: the server repeats the affordability and capacity checks; the client
    /// checks them so the panel can refuse immediately.
    pub total_value: i32,
    /// how much of an **alternate currency** this session has already
    /// committed at this vendor. Read only on the alternate-currency arm of the money test, and
    /// reset to 0 by every sell. Both recorded vendors are pyreal shops, so the corpus cannot
    /// witness it.
    pub last_sale: i32,
    /// A part of a stack dropped on the sell list, waiting for the server to make it.
    ///
    /// The drop asks for the split and puts the **source** stack on the list as the row's
    /// placeholder; when an object of the same class and exactly the split size is declared, it
    /// takes the placeholder's row. Closing the shop drops the wait with the rest of the state.
    pub pending_sell_split: Option<PendingSellSplit>,
}

/// The three things the sell list remembers about a split it asked for: the row standing in for
/// it, and the class and stack size that identify the object the server makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingSellSplit {
    pub placeholder: ObjectId,
    pub wcid: u32,
    pub stack_size: u32,
}

impl Shop {
    /// Whether a vendor window is open — `vendor_id != 0`.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.vendor_id.is_some()
    }

    /// Resolve a stocked class id to its object id and clamp the desired count to available stock.
    ///
    /// It searches by class id, not object id.
    /// The client takes a **WCID** and compares it against each stock
    /// row's `pwd.wcid`, which is a different number entirely — a stock row's `iid` is the
    /// vendor's own instance id and is never a class id. Both outputs matter. The
    /// body is five lines:
    ///
    /// ```text
    /// for each stock row n, in order:
    ///     w = the object for n.iid
    ///     w exists and w.pwd.wcid == wcid:
    ///         n.amount != -1 and n.amount < wanted  -> wanted = n.amount
    ///         out iid = n.iid                       -> true
    /// -> false
    /// ```
    ///
    /// The **clamp** is what stops
    /// `World::fill_component_list` asking for 500 tapers from a vendor holding 12; `amount == -1` is
    /// the "unlimited stock" 29 of the corpus's 30 recorded stock rows carry (the thirtieth is a
    /// Sack with `amount == 1`), and it is exempt. The
    /// **resolved `iid`** is what the buy basket is keyed on, because the buy list holds instance
    /// ids and the desired-component hash holds class ids.
    ///
    /// The original path gets the description from the object table; the stock
    /// row carries its own description here (decoding drops a row without one),
    /// so the lookup is local. The two agree because `0x0062`'s rows are exactly what the client
    /// then creates objects from.
    ///
    /// Returns `(iid, wanted-after-clamp)`.
    #[must_use]
    pub fn shop_has_item(&self, wcid: u32, wanted: i32) -> Option<(ObjectId, i32)> {
        let p = self.stock.iter().find(|p| p.pwd.wcid == wcid)?;
        let clamped = if p.amount != -1 && p.amount < wanted {
            p.amount
        } else {
            wanted
        };
        Some((p.iid, clamped))
    }

    /// The stock row for an object id, including the description carried by `0x0062` that
    /// supplies its price.
    #[must_use]
    pub fn stock_item(&self, id: ObjectId) -> Option<&ItemProfile> {
        self.stock.iter().find(|p| p.iid == id)
    }

    /// Behavior: whether the stock holds at least one item of
    /// a given `ITEM_TYPE`. A type-filter tab is created only when this answers true.
    ///
    /// **This has no production caller, and that is deliberate rather than an oversight.**
    /// The client's version reads the *live object*'s item type, and which object that is depends on
    /// the ownership fork — a question a `Shop` cannot answer without player and object-table
    /// access. The production reader is therefore
    /// `dereth_client::vendor_view`'s `type_filters`, which walks the same eighteen masks over types
    /// resolved through that fork. This stays as the `Shop`-local form the vendor tests use to
    /// pick the recorded grocer out of the corpus by its contents.
    #[must_use]
    pub fn list_contains_type(&self, item_type: u32) -> bool {
        self.stock.iter().any(|p| p.pwd.obj_type & item_type != 0)
    }

    /// `INVALID_DID` means the shop deals in pyreals.
    #[must_use]
    pub fn trade_currency(&self) -> Option<DataId> {
        (self.profile.trade_id.0 != 0).then_some(self.profile.trade_id)
    }

    /// Turn a basket into the item-profile list carried by a shop event.
    ///
    /// **It merges.** Its inner loop is
    /// `if (profile.iid == item.iid) { profile.amount += count; found = true; }` over the list
    /// built so far, so two rows of the *same* object id become one profile whose amount is their
    /// sum — and a new node is appended only when nothing matched.
    ///
    /// **The corpus is what says so.** `long-solo-play`'s fourth `0x005F` carries `amount = 2` for
    /// `0x80000A6E`, whose stock row advertises `stack_size == 1` and `max_stack_size == 1`. So it
    /// cannot be the immediate purchase's "split size when the stack is 2 or more, else 1" — that
    /// arm is unreachable
    /// for an unstackable item. It is **"Add to List" twice and then "Buy All"**, and this merge is
    /// the only thing that turns two rows into one; a plain-list basket cannot produce that buy.
    ///
    /// The wire form carries the amount and the id and **not** the `PublicWeenieDesc` (the
    /// has-description flag is 0), so each item is exactly eight bytes — as all six recorded shop
    /// actions are.
    fn wire_items(basket: &[(ObjectId, i32)]) -> Vec<dereth_protocol::trade::ItemProfile> {
        let mut out: Vec<dereth_protocol::trade::ItemProfile> = Vec::new();
        for (iid, amount) in basket {
            if let Some(p) = out.iter_mut().find(|p| p.iid == *iid) {
                p.amount += *amount;
            } else {
                out.push(dereth_protocol::trade::ItemProfile {
                    amount: *amount,
                    iid: *iid,
                    pwd: None,
                });
            }
        }
        out
    }

    /// Build the `0x005F` purchase request.
    ///
    /// `None` when no vendor is open or the basket is empty, which are the two states
    /// the shop-event send cannot send from.
    #[must_use]
    pub fn event_buy(&self) -> Option<dereth_protocol::trade::VendorBuy> {
        let vendor_id = self.vendor_id?;
        if self.buy_list.is_empty() {
            return None;
        }
        Some(dereth_protocol::trade::VendorBuy {
            vendor_id,
            items: Self::wire_items(&self.buy_list),
            // The alternate currency spent. Zero for a pyreal shop, which is every vendor in the
            // corpus: both recorded `VendorProfile`s carry `trade_id == 0`.
            alternate_currency_id: 0,
        })
    }

    /// Build the `0x0060` sale request, which has **no** currency field.
    #[must_use]
    pub fn event_sell(&self) -> Option<dereth_protocol::trade::VendorSell> {
        let vendor_id = self.vendor_id?;
        if self.sell_list.is_empty() {
            return None;
        }
        Some(dereth_protocol::trade::VendorSell {
            vendor_id,
            items: Self::wire_items(&self.sell_list),
        })
    }
}

impl ItemProfile {
    /// A stock row retained by the shop.
    ///
    /// A stock row without a `PublicWeenieDesc` cannot be priced or drawn: the item list reads
    /// `pwd` for the icon, the name and the value — so a row that arrives without one is
    /// **dropped and counted** rather than defaulted into a nameless zero-value item. All 8
    /// `0x0062` in the recorded corpus carry the `0xFF` flag on every row.
    #[must_use]
    pub fn from_wire(p: &dereth_protocol::trade::ItemProfile) -> Option<Self> {
        Some(Self {
            amount: p.amount,
            iid: p.iid,
            pwd: p.pwd.clone()?,
        })
    }
}

impl VendorProfile {
    /// Behavior: the wire form into the in-memory one.
    ///
    /// The two differ only in signedness and in `trade_id` being a `DataID` on this side; the
    /// field order and meaning are unchanged.
    #[must_use]
    pub fn from_wire(p: &dereth_protocol::trade::VendorProfile) -> Self {
        Self {
            item_types: p.item_types,
            min_value: p.min_value,
            max_value: p.max_value,
            #[allow(clippy::cast_sign_loss)]
            magic: p.magic as u32,
            buy_price: p.buy_price,
            sell_price: p.sell_price,
            trade_id: DataId(p.trade_id),
            #[allow(clippy::cast_sign_loss)]
            trade_num: p.trade_num as u32,
            trade_name: p.trade_name.clone(),
        }
    }
}

/// The client's type-filter tabs — **name and mask**, in the order it
/// adds them.
///
/// Five tabs use masks that differ from the single item-type bit their names might suggest:
///
/// | tab | mask | what it really is |
/// |---|---|---|
/// | Keys, Tools | `0x20004000` | `KEY \| TINKERING_TOOL` |
/// | Miscellaneous | `0x490` | `MISC \| CREATURE \| USELESS` |
/// | Magic Items | `0x8000` | `CASTER`, not `MAGIC_WIELDABLE` |
/// | Alchemical Items | `0x4800000` | alchemy **base and intermediate** |
/// | Fletching Items | `0x9000000` | fletching **base and intermediate** |
///
/// A tab is created only when at least one stock item matches its mask, so a shop that sells
/// only food has one tab. Production resolves the live item type through `vendor_view`.
pub const TYPE_FILTERS: [(&str, u32); 18] = [
    ("Armor", 0x0000_0002),
    ("Books, Paper", 0x0000_2000),
    ("Clothing", 0x0000_0004),
    ("Containers", 0x0000_0200),
    ("Food", 0x0000_0020),
    ("Gems", 0x0000_0800),
    ("Jewelry", 0x0000_0008),
    ("Keys, Tools", 0x2000_4000),
    ("Miscellaneous", 0x0000_0490),
    ("Services", 0x0010_0000),
    ("Spell Components", 0x0000_1000),
    ("Trade Notes", 0x0004_0000),
    ("Weapons", 0x0000_0101),
    ("Mana Stones", 0x0008_0000),
    ("Magic Items", 0x0000_8000),
    ("Alchemical Items", 0x0480_0000),
    ("Cooking Items", 0x0040_0000),
    ("Fletching Items", 0x0900_0000),
];

/// Append one unavailable component to the warning line.
///
/// One line of string building, and the whole of it:
///
/// ```text
/// name = the component's name for wcid
/// msg += (msg is empty) ? "There was not enough: " : ", "
/// msg += name
/// ```
///
/// The original length counts the terminator, so **1 means empty**. The separator appears
/// between entries and never before the first, and the caller appends a `"\n"` of its own after the
/// whole line. Reproduced with the trailing space of `"There was not enough: "` intact.
///
/// A component whose name the component table does not know contributes an **empty** entry
/// rather than being skipped, because the name lookup leaves the caller's string empty and
/// the append still runs. Reproduced: the separator is still added.
fn add_missing_comp(cat: &crate::magic::ComponentCatalogue, wcid: u32, msg: &mut String) {
    if msg.is_empty() {
        msg.push_str(NOT_ENOUGH_COMPONENTS);
    } else {
        msg.push_str(", ");
    }
    msg.push_str(cat.comp_name_from_wcid(wcid));
}

/// The client's outcome, counted rather than assumed.
///
/// The three "missing" counters are separate on purpose: *not stocked at all*, *stocked but short*
/// and *bought* are three different things and a single number cannot tell them apart — which is
/// exactly why the client raises the same message from two places and the reader cannot tell which.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FillComponents {
    /// How many rows `desired_comps_` held. The denominator; a run over an empty hash is not the
    /// same as a run that found everything in stock.
    pub desired_rows: usize,
    /// Rows where `have < desired` and the category matched.
    pub short_rows: usize,
    /// Rows added to the buy basket.
    pub added: usize,
    /// Units added, after both the stock clamp and the already-in-basket subtraction.
    pub bought_units: i32,
    /// Rows the shop does not stock at all — `shop_has_item` answered `None`.
    pub not_stocked: usize,
    /// Rows the shop stocks but in fewer than the shortfall.
    pub short_stocked: usize,
    /// `vendor_sell_price` summed over what was added.
    pub total: i32,
    /// Whether `max_price` cut the walk short.
    pub aborted: bool,
    /// The exact `add_missing_comp` line, or `None` when nothing was missing.
    pub missing_line: Option<String>,
}

/// Why an immediate purchase was refused.
///
/// Both arms are the client's own, and both are **advisory**: the server re-runs
/// them. The strings are the client's, printed to the scroll before the `false` return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuyRefusal {
    /// [`NOT_ENOUGH_MONEY`]. The **pyreal** arm tests `total_value`; the alternate-currency arm
    /// tests `trade_num - last_sale`.
    NotEnoughMoney,
    /// [`EMPTY_SOME_SLOTS`].
    NoFreeSlot,
}

impl BuyRefusal {
    /// The text the client prints. Both are literals in the client, not string-table rows.
    #[must_use]
    pub fn message(self) -> &'static str {
        match self {
            Self::NotEnoughMoney => NOT_ENOUGH_MONEY,
            Self::NoFreeSlot => EMPTY_SOME_SLOTS,
        }
    }
}

/// The sell-list drop's containment refusal.
pub const NOT_CARRIED: &str = "You can only sell items you are carrying";
/// Refusal to sell a nonempty container.
pub const CONTAINER_NOT_EMPTY: &str = "Cannot sell container that isn't empty";
/// Refusal to sell part of a stack.
pub const PART_OF_A_STACK: &str = "Cannot sell part of a stack";
/// Affordability refusal shared by immediate purchase and Buy All.
pub const NOT_ENOUGH_MONEY: &str = "You don't have enough money";
/// Capacity refusal shared by immediate purchase and Buy All.
pub const EMPTY_SOME_SLOTS: &str = "You must empty some slots in your backpack first";
/// The client's guard, when no vendor window is open.
pub const NEED_AN_OPEN_VENDOR: &str = "You need an open vendor.";
/// The client's `max_price` abort. The trailing newline is the
/// client's own and is part of the literal.
pub const BUYING_ABORTED: &str = "Buying aborted; max price reached.
";
/// The client's prefix. The trailing space is the client's.
pub const NOT_ENOUGH_COMPONENTS: &str = "There was not enough: ";
/// The sell list's refusal when the split it asked for cannot be placed.
pub const CANNOT_SPLIT_TO_SELL: &str = "Cannot split the stack to sell it";

/// The sell list's announcement of a split it asked for, formatted with the stack's appropriate
/// name.
#[must_use]
pub fn splitting_before_selling(name: &str) -> String {
    format!("Splitting the {name} before selling them")
}

/// The client's container announcement — a wide literal, formatted with the object's
/// appropriate name (`NameType` 2).
pub const SELLING_CONTENTS_OF: &str = "Selling contents of ";
/// The button handler's `case 0x100000D6` — the close button's confirmation, raised only when a
/// basket is not empty.
pub const UNFINISHED_TRANSACTIONS: &str =
    "You have not completed all transactions. Are you sure you want to leave this vendor?";

impl crate::world::World {
    /// the reader three call sites use.
    ///
    /// The selection-changed handler's splitter seed,
    /// the client's "a vendor's stock is bought, not used" guard and
    /// the client's vendor-closing step all read it.
    #[must_use]
    pub fn vendor_id(&self) -> Option<ObjectId> {
        self.shop.vendor_id
    }

    /// Handle the vendor advertisement `0x0062`.
    ///
    /// ```text
    /// clear the ground object (0, notify)
    /// vendor_id = merchant_id
    /// mode = buy mode + (merchant_id == attempt_open_vendor ? 1 : 0)
    /// open_window(id, profile, items, mode)
    /// if (mode == sell mode) add_item_to_sell(attempt_sale_object)
    /// attempt_open_vendor = 0; attempt_sale_object = 0
    /// ```
    ///
    /// The corpus carries **8** `0x0062` messages whose bodies parse as a vendor profile plus a
    /// stock list to the exact end.
    ///
    /// Return the number of stock rows retained; a row without a description is dropped, so
    /// the count can be below `m.items.len()`.
    pub fn handle_vendor_info(
        &mut self,
        m: &dereth_protocol::trade::VendorInfo,
        out: &mut dyn crate::NoticeSink,
        req: &mut dyn crate::RequestSink,
        now: dereth_primitives::ServerTime,
    ) -> usize {
        // Clear the ground object, notifying — opening a shop closes whatever ground container
        // was open, and the `true` is the no-longer-viewing-contents message this crate already
        // sends.
        self.set_ground_object(req, out, None, true, now);
        let sell_mode = self.shop.attempt_open_vendor == Some(m.merchant_id);
        let sale_object = self.shop.attempt_sale_object;
        let same_vendor = self.shop.vendor_id == Some(m.merchant_id);
        if !same_vendor {
            self.flush_sell_list_sell_state();
        }
        let buy_list = if same_vendor {
            std::mem::take(&mut self.shop.buy_list)
        } else {
            Vec::new()
        };
        let sell_list = if same_vendor {
            std::mem::take(&mut self.shop.sell_list)
        } else {
            Vec::new()
        };
        let basket_descriptions = if same_vendor {
            std::mem::take(&mut self.shop.basket_descriptions)
        } else {
            Default::default()
        };
        let pending_sell_split = same_vendor
            .then_some(self.shop.pending_sell_split)
            .flatten();
        let stock: Vec<ItemProfile> = m.items.iter().filter_map(ItemProfile::from_wire).collect();
        let taken = stock.len();
        self.shop = Shop {
            vendor_id: Some(m.merchant_id),
            mode: if same_vendor {
                self.shop.mode
            } else if sell_mode {
                ShopMode::Sell
            } else {
                ShopMode::Buy
            },
            profile: VendorProfile::from_wire(&m.profile),
            stock,
            buy_list,
            sell_list,
            basket_descriptions,
            // "Both deferred fields are then cleared."
            attempt_open_vendor: None,
            attempt_sale_object: None,
            total_value: self.shop.total_value,
            last_sale: 0,
            pending_sell_split,
        };
        // **The client's purse refresh.**
        // The purse has to be filled *before* the panel is told the window opened, because the
        // two total-value updates that render it are that function's own callees. Carried across
        // the `Shop` replacement above and then refreshed, so an open with no player description
        // reads 0 rather than whatever the last shop left behind.
        self.update_total_value();
        // **Release the matching shop request before refreshing the lists.**
        //
        // **The shop's own re-advertisement is the reply that releases the shop's own request.**
        // All four senders here — [`Self::buy_single_item`], [`Self::buy_all`],
        // [`Self::sell_single_item`] and [`Self::sell_all`] — end in
        // [`Self::record_shop_request`]. This is the matching response path; ground-container
        // responses use their own request kinds.
        //
        // ACE sends it: `Player_Commerce.FinalizeBuyTransaction` ends in
        // `vendor.ApproachVendor(this, VendorType.Buy, altCurrencySpent)` and
        // `Vendor.SellItems_FinalTransaction` in `ApproachVendor(player, VendorType.Sell)`, both
        // of which enqueue `GameEventApproachVendor` — this message. `long-solo-play` records all
        // three: the open at idx 4736, the purchase's reply at 4748, the sale's at 4784.
        //
        // Without this release, the request lock would refuse every later
        // inventory request with *"You can only move or use one item at a time"* and **there is no
        // timeout** to recover from it — the request time is written and read by
        // nothing. One purchase would wedge the character permanently.
        //
        // Both halves of the guard are transcribed. The enum test keeps a `0x0062` from clearing a
        // pickup or a wield that happens to be outstanding on the same id. The id test implemented
        // by [`crate::inventory::requests::RequestLock::clear_if_matches`] keeps one merchant's
        // reply from releasing a request outstanding on another.
        if self.request_lock.pending == crate::inventory::requests::InventoryRequest::ShopEvent {
            self.request_lock.clear_if_matches(m.merchant_id);
        }
        // **The client's stock loop**, without which a vendor's items would draw on bare panel
        // art.
        //
        // Materialize stock after refreshing the purse and before the three list updates, so by the
        // time any slot is drawn every stock row is already an object.
        self.open_vendor_stock_objects(now, out);
        out.emit(crate::Notice::OpenVendor {
            vendor: m.merchant_id,
            mode: self.shop.mode,
            items: taken,
        });
        // **Range registrant #2.** Opening the vendor registers a range check that closes
        // the window when the player leaves the vendor's use radius. Registration lives with
        // the world model rather than in the panel; see [`crate::range`].
        self.register_vendor_range_check(m.merchant_id, now);
        if sell_mode {
            if let Some(item) = sale_object {
                // Complete the deferred drag by offering its item to the sell basket.
                self.add_item_to_sell(item, out);
            }
        }
        taken
    }

    /// Materialize vendor stock in the game-object table.
    ///
    /// For an existing object, retain its description if the player owns it; otherwise copy the
    /// stock description and cancel pending destruction. For a new object, create it, copy the
    /// description, determine its position state and mark it valid.
    ///
    /// **This supplies the objects used by item decoration.** Decoration returns early if
    /// the object lookup fails; without this
    /// function every stock slot would take the null arm, `ItemListWidget::decorate` would skip it,
    /// and the slot would keep the raw icon instead of the composite background/effect decoration.
    /// The same object description supplies basket prices and
    /// what the button handler's case `0x100000C3` reads `stack_size` from.
    ///
    /// **The stock description is deliberately retained.** The original path drops the
    /// profile's `pwd` because after this point every reader goes through the object table; this
    /// build keeps two stores for that one desc — [`Shop::stock`]'s profile and
    /// the world object table — and every stock reader here (`stock_item`, the two price
    /// functions, `vendor_view::inq_type`) uses the profile. Clearing it would empty the window.
    /// [`Self::set_object_stack_size`] writes both stores for exactly this reason.
    ///
    /// Marking a new object valid raises an item-attributes notice with `kind: 1`, which the
    /// item-list consumer uses to refresh its decoration.
    ///
    /// Returns how many objects were manufactured.
    fn open_vendor_stock_objects(
        &mut self,
        now: dereth_primitives::ServerTime,
        out: &mut dyn crate::NoticeSink,
    ) -> usize {
        // Cloned out of the shop because the loop writes the object table, which the borrow of
        // `self.shop.stock` would hold. The original description assignment is a copy too.
        let rows: Vec<(ObjectId, PublicWeenieDesc)> = self
            .shop
            .stock
            .iter()
            .map(|p| (p.iid, p.pwd.clone()))
            .collect();
        let mut made = 0;
        for (iid, pwd) in rows {
            if self.tables.weenies.contains_key(iid) {
                // Only an object the player does not own takes the profile's desc — an object the player is
                // carrying keeps the desc the create stream gave it, so a vendor cannot rewrite
                // the player's own copy of an item it also stocks.
                //
                // Existing objects skip position-state determination and the valid-object notice.
                // Updating the description and cancelling destruction completes this arm.
                if !self.is_owned_by_player(iid) {
                    if let Some(w) = self.tables.weenies.get_mut(iid) {
                        w.pwd = pwd;
                    }
                }
                self.remove_object_to_be_destroyed(iid);
            } else {
                let mut w = crate::weenie::Weenie::new(iid);
                w.pwd = pwd;
                w.determine_position_state();
                self.tables.weenies.insert(iid, w);
                self.declare_valid(iid, now, out);
                made += 1;
            }
        }
        made
    }

    /// Close the vendor window and clear its sale marks.
    ///
    /// There is **no** close-vendor message on the wire — the client just stops talking to it
    /// (`\[verified\]`). The sell list's marks come off here, which is
    /// [`Self::flush_sell_list_sell_state`].
    ///
    /// Returns true when a vendor was actually open.
    pub fn close_vendor(&mut self, out: &mut dyn crate::NoticeSink) -> bool {
        if !self.shop.is_open() {
            return false;
        }
        self.flush_sell_list_sell_state();
        // The client's hide arm unregisters the
        // object-range check before it calls this; every path that closes the window goes through
        // it, so the unregister belongs here.
        if let Some(v) = self.shop.vendor_id {
            self.object_range_checks
                .unregister(crate::range::RangeHandler::Vendor, v);
        }
        self.shop = Shop {
            total_value: self.shop.total_value,
            ..Shop::default()
        };
        out.emit(crate::Notice::CloseVendor);
        true
    }

    /// The button handler's `case 0x100000D6` — the close button.
    ///
    /// ```text
    /// if (buy_list is empty && sell_list is empty) { close; return; }
    /// if (no dialog is already up) raise the close-vendor confirmation dialog
    /// ```
    ///
    /// So a shop with anything in either basket asks first. Returns `Ok(())` when the window
    /// closed and `Err(text)` with the confirmation the client raises when it did not; the
    /// callback is [`Self::close_vendor`], exactly as the client's close-vendor dialog callback is.
    ///
    /// # Errors
    /// [`UNFINISHED_TRANSACTIONS`] when either basket is non-empty.
    pub fn close_vendor_button(
        &mut self,
        out: &mut dyn crate::NoticeSink,
    ) -> Result<bool, &'static str> {
        if !self.shop.buy_list.is_empty() || !self.shop.sell_list.is_empty() {
            return Err(UNFINISHED_TRANSACTIONS);
        }
        Ok(self.close_vendor(out))
    }

    /// Behavior: how many **container** slots and how many
    /// **item** slots a basket would need, in that order.
    ///
    /// The split is the same one [`Self::needs_pack_slot`] makes for a single object.
    #[must_use]
    pub fn inq_list_slot_count(&self, list: &[(ObjectId, i32)]) -> (i32, i32) {
        let mut containers = 0;
        let mut items = 0;
        for (id, _) in list {
            if self.needs_pack_slot(*id) {
                containers += 1;
            } else {
                items += 1;
            }
        }
        (containers, items)
    }

    /// Whether the pack-slot bit `0x800000` is set or either item/container capacity is nonzero.
    /// Both slot-count paths use this test.
    ///
    /// The bit is [`crate::inventory::use_object::use_bitfield::REQUIRES_PACK_SLOT`], named there
    /// at its only other reader. An object that answers true takes a **container**
    /// slot on the player rather than an item slot.
    #[must_use]
    pub fn needs_pack_slot(&self, id: ObjectId) -> bool {
        let Some(w) = self.weenie(id) else {
            return false;
        };
        w.pwd.bitfield & crate::inventory::use_object::use_bitfield::REQUIRES_PACK_SLOT != 0
            || w.pwd.items_capacity.unwrap_or(0) != 0
            || w.pwd.containers_capacity.unwrap_or(0) != 0
    }

    /// The same test directly on a stock row's description.
    #[must_use]
    fn stock_needs_pack_slot(p: &PublicWeenieDesc) -> bool {
        p.bitfield & crate::inventory::use_object::use_bitfield::REQUIRES_PACK_SLOT != 0
            || p.items_capacity.unwrap_or(0) != 0
            || p.containers_capacity.unwrap_or(0) != 0
    }

    /// The immediate purchase's free-slot test, which is a **`!=`, not a `<`**:
    ///
    /// ```text
    /// (not needs_pack_slot or the player's contained-container count != containers_capacity)
    ///   and (needs_pack_slot or the player's contained-item count != items_capacity)
    /// ```
    ///
    /// Reproduced as written. A player whose count has somehow passed the capacity therefore
    /// reads as *having* room, which is the client's own behaviour and not a simplification.
    #[must_use]
    fn has_room_for(&self, needs_pack_slot: bool) -> bool {
        let Some(player) = self.player else {
            return false;
        };
        let Some(p) = self.weenie(player) else {
            return false;
        };
        #[allow(clippy::cast_possible_wrap)]
        let cap = |v: Option<u8>| i32::from(v.unwrap_or(0) as i8);
        let inv = self.inventory(player);
        let containers = inv.map_or(0, |i| i32::try_from(i.containers.len()).unwrap_or(i32::MAX));
        let items = inv.map_or(0, |i| i32::try_from(i.items.len()).unwrap_or(i32::MAX));
        if needs_pack_slot {
            containers != cap(p.pwd.containers_capacity)
        } else {
            items != cap(p.pwd.items_capacity)
        }
    }

    /// Behavior: refill [`Shop::total_value`] from the player's
    /// **`PropertyInt` 20 `CoinValue`**, and answer what it now holds.
    ///
    /// Read player integer quality `0x14`. Failure writes **0**, rather than retaining the
    /// previous value, so a session with no player description
    /// shows an empty purse and refuses every pyreal purchase — reproduced as written.
    ///
    /// **This is [`Shop::total_value`]'s production writer.** Without it
    /// [`Self::can_afford`]'s pyreal arm would compare every price against **0** and the
    /// immediate purchase and `Buy All` would refuse with *"You don't have enough money"* however
    /// rich the player was.
    ///
    /// **The cadence.** The original path has three callers:
    /// vendor opening and two quality hooks.
    /// The first quality hook starts by testing whether the changed property is stack size; both
    /// are registered at initialisation as player quality handlers `(1, 0x14)`. With only the open
    /// path the purse would be a snapshot taken when the window opened rather than a live figure,
    /// so the player-quality handler is the subscription, reached from both writers of the
    /// player's qualities.
    pub fn update_total_value(&mut self) -> i32 {
        let v = self.player_qualities().map_or(0, |q| q.inq_int(COIN_VALUE));
        self.shop.total_value = v;
        v
    }

    // Public and private quality updates write the same player record, and its integer-quality
    // `0x14` subscription refreshes the purse; there is no second store to bridge.

    /// The two currency-dependent affordability tests shared by purchase paths.
    ///
    /// ```text
    /// if (vendor trade currency == INVALID_DID)  ok = total_value >= price
    /// else                                       ok = trade_num - last_sale >= price
    /// ```
    ///
    /// The *branch structure* is unambiguous and is what is transcribed. `\[verified\]`
    /// structurally, the operands `[inferred]` from the two arms' contents.
    #[must_use]
    fn can_afford(&self, price: i32) -> bool {
        match self.shop.trade_currency() {
            None => self.shop.total_value >= price,
            #[allow(clippy::cast_possible_wrap)]
            Some(_) => (self.shop.profile.trade_num as i32) - self.shop.last_sale >= price,
        }
    }

    /// Use the selected split amount when the stack size is at least two, otherwise one.
    /// Immediate purchase and Add to List share this rule, written
    /// once because the client writes it twice identically.
    ///
    /// **Unreachable for every item in the recorded corpus**: each of the 30 stock rows across the
    /// eight `0x0062` advertises `stack_size` below 2, so the recorded traffic can only ever take
    /// the `else` arm, so only a synthesised test can tell this from a constant 1.
    #[must_use]
    fn buy_count(pwd: &PublicWeenieDesc, split: i32) -> i32 {
        let stack = i32::from(pwd.stack_size.unwrap_or(0));
        if stack >= 2 {
            split
        } else {
            1
        }
    }

    /// Purchase one selected stock item immediately.
    ///
    /// **This function sends.** It does not add to the buy basket — that is `case 0x100000C3`,
    /// [`Self::add_to_buy_list`]. The client builds a one-entry `PackableList<ItemProfile>`
    /// and puts it on the wire immediately, which is why **all
    /// five `0x005F` in the recorded corpus carry exactly one item**.
    ///
    /// ```text
    /// w = weenie(obj_id); if (!w) return false
    /// count = (w.pwd.stack_size >= 2) ? split_amount(w) : 1
    /// price = the shop profile's vendor_sell_price(w.pwd, count)
    /// if (!can_afford(price))  { print "You don't have enough money"; return false }
    /// if (!has_room_for(...))  { print "You must empty some slots..."; return false }
    /// if (currency != INVALID_DID) last_sale = count
    /// send_buy(vendor_id, [{count, obj_id}], the trade currency)
    /// record a shop-event request on vendor_id; increment the busy count
    /// ```
    ///
    /// `split` is the selected split amount. **This is where the splitter's
    /// vendor arm is visible**: an item being sold by an open vendor seeds the splitter at 1
    /// rather than at the whole stack, so `split` normally arrives as 1. A recorded multi-unit
    /// purchase alone does not prove slider use: repeated basket additions can also merge counts.
    ///
    /// # Errors
    /// The two advisory refusals of [`BuyRefusal`]. `Ok(false)` when the id is not in the stock.
    pub fn buy_single_item(
        &mut self,
        item: ObjectId,
        split: i32,
        req: &mut dyn crate::RequestSink,
        out: &mut dyn crate::NoticeSink,
        now: dereth_primitives::ServerTime,
    ) -> Result<bool, BuyRefusal> {
        let Some(vendor) = self.shop.vendor_id else {
            return Ok(false);
        };
        // The client's object lookup: a vendor's stock **is** in the object tables in retail
        // (the server creates the objects), and it is in `Shop::stock` here because `0x0062` is
        // the only description of them this build receives. Both are consulted, stock first.
        let Some((pwd, _)) = self.priced_row(item) else {
            return Ok(false);
        };
        let count = Self::buy_count(&pwd, split);
        let price = self.shop.profile.vendor_sell_price(&pwd, count);
        if !self.can_afford(price) {
            self.refuse_shop(out, NOT_ENOUGH_MONEY);
            return Err(BuyRefusal::NotEnoughMoney);
        }
        if !self.has_room_for(Self::stock_needs_pack_slot(&pwd)) {
            self.refuse_shop(out, EMPTY_SOME_SLOTS);
            return Err(BuyRefusal::NoFreeSlot);
        }
        if self.shop.trade_currency().is_some() {
            self.shop.last_sale = count;
        }
        req.send(crate::Request::VendorBuy(
            dereth_protocol::trade::VendorBuy {
                vendor_id: vendor,
                items: vec![dereth_protocol::trade::ItemProfile {
                    amount: count,
                    iid: item,
                    pwd: None,
                }],
                alternate_currency_id: self.shop.profile.trade_id.0,
            },
        ));
        self.record_shop_request(vendor, now);
        Ok(true)
    }

    /// Resize the object's stack and rescale its total value together.
    ///
    /// Compute `unit = value / max(old_stack_size, 1)` with unsigned division, then set
    /// `value = unit * size` and `stack_size = size`.
    ///
    /// **Nothing about this is a UI element**, despite the name. It rewrites the *object's own*
    /// `PublicWeenieDesc`, and the two writes are a pair: `value` on a stack is the whole stack's
    /// value, so rescaling it by the same factor keeps
    /// the price function's unit-value quotient invariant. The unit price stays fixed; what moves is
    /// `stack_size`, and that is read by the button handler's case `0x100000C3` (add to the buy
    /// list with the split size when `stack_size >= 2`, else 1) and immediate purchase.
    /// This call is what lets the split slider mean anything at a
    /// vendor: without it a stackable stock row adds exactly one per press.
    ///
    /// **Where the write lands here.** The original stock-loading path copies each description
    /// into the game object it
    /// manufactures (see `dereth_client::vendor_view`'s `inq_type`). This build keeps
    /// two stores for that one desc — [`Shop::stock`]'s profile, which every stock reader uses,
    /// and the world object table, populated from vendor stock as well as object creates —
    /// so the client's single write is applied to both. That is a deviation only in the case
    /// `inq_type` already names (an object the client held *and* the player owns, whose weenie
    /// the original path leaves un-copied). In that path nothing reads the stock profile after open at
    /// all, so writing it is unobservable there.
    ///
    /// Idempotent, which matters because the panel re-runs its item-list update on every refresh:
    /// `unit = (unit*n)/n`.
    ///
    /// Returns whether any desc was found to write.
    pub fn set_object_stack_size(&mut self, item: ObjectId, size: i32) -> bool {
        let Ok(size) = u16::try_from(size) else {
            return false;
        };
        let mut wrote = false;
        if let Some(p) = self.shop.stock.iter_mut().find(|p| p.iid == item) {
            Self::apply_stack_size(&mut p.pwd, size);
            wrote = true;
        }
        if let Some(w) = self.weenie_mut(item) {
            Self::apply_stack_size(&mut w.pwd, size);
            wrote = true;
        }
        wrote
    }

    /// Apply the paired stack-size and total-value update to one description.
    ///
    /// `stack_size == 0` divides by **1**, and the divide is unsigned, so an absent `stack_size` — the
    /// wire's optional field — takes the same arm the literal 0 does.
    fn apply_stack_size(pwd: &mut PublicWeenieDesc, size: u16) {
        let value = pwd.value.unwrap_or(0);
        let divisor = u32::from(pwd.stack_size.unwrap_or(0)).max(1);
        pwd.value = Some((value / divisor).saturating_mul(u32::from(size)));
        pwd.stack_size = Some(size);
    }

    /// The button handler's `case 0x100000C3` — the "Add to List" button.
    ///
    /// ```text
    /// w = the selected object's weenie; if (!w) return
    /// add to the buy list (w, w.pwd.stack_size >= 2 ? the split size : 1)
    /// ```
    ///
    /// Local only: nothing goes on the wire until "Buy All" ([`Self::buy_all`]).
    pub fn add_to_buy_list(&mut self, item: ObjectId, split: i32) -> bool {
        let Some((pwd, _)) = self.priced_row(item) else {
            return false;
        };
        let count = Self::buy_count(&pwd, split);
        self.shop.basket_descriptions.insert(item, pwd);
        self.shop.buy_list.push((item, count));
        true
    }

    /// The button handler's `case 0x100000CA` — the "Buy All" button.
    ///
    /// The same two refusals as [`Self::buy_single_item`], but against the **basket's total**
    ///  and against [`Self::inq_list_slot_count`]'s two counts:
    ///
    /// ```text
    /// if (!can_afford(transaction value))            print "You don't have enough money"; return
    /// record the basket's contents; (containers, items) = inq_list_slot_count(basket)
    /// if (player.containers_free < containers || player.items_free < items)
    ///                                              print "You must empty some slots..."; return
    /// send the buy shop event (vendor, buy_list, trade currency); retain buy_list
    /// ```
    ///
    /// Note the polarity difference from [`Self::buy_single_item`]: here it is a `<` on the *free*
    /// count, there a `!=` on the *used* count against the capacity. Both are reproduced as written.
    ///
    /// # Errors
    /// The two advisory refusals of [`BuyRefusal`].
    pub fn buy_all(
        &mut self,
        req: &mut dyn crate::RequestSink,
        out: &mut dyn crate::NoticeSink,
        now: dereth_primitives::ServerTime,
    ) -> Result<bool, BuyRefusal> {
        let Some(vendor) = self.shop.vendor_id else {
            return Ok(false);
        };
        if self.shop.buy_list.is_empty() {
            return Ok(false);
        }
        let total = self.transaction_value();
        if !self.can_afford(total) {
            self.refuse_shop(out, NOT_ENOUGH_MONEY);
            return Err(BuyRefusal::NotEnoughMoney);
        }
        let (containers, items) = self.inq_list_slot_count(&self.shop.buy_list.clone());
        if self.free_container_slots() < containers || self.free_item_slots() < items {
            self.refuse_shop(out, EMPTY_SOME_SLOTS);
            return Err(BuyRefusal::NoFreeSlot);
        }
        if self.shop.trade_currency().is_some() {
            self.shop.last_sale = total;
        }
        let m = self
            .shop
            .event_buy()
            .expect("the basket is non-empty and a vendor is open");
        req.send(crate::Request::VendorBuy(m));
        self.record_shop_request(vendor, now);
        Ok(true)
    }

    /// Price the buy basket through the vendor profile's sell-price formula.
    #[must_use]
    pub fn transaction_value(&self) -> i32 {
        self.shop
            .buy_list
            .iter()
            .filter_map(|(id, n)| {
                self.priced_row(*id)
                    .map(|(pwd, _)| self.shop.profile.vendor_sell_price(&pwd, *n))
            })
            .sum()
    }

    /// What the vendor would pay for the sell basket — per row.
    #[must_use]
    pub fn sell_value(&self) -> i32 {
        self.shop
            .sell_list
            .iter()
            .filter_map(|(id, _)| {
                self.vendor_basket_description(*id)
                    .map(|pwd| self.shop.profile.vendor_buy_price(pwd))
            })
            .sum()
    }

    /// The latest live description, or the description saved with a retained basket row.
    #[must_use]
    pub fn vendor_basket_description(&self, id: ObjectId) -> Option<&PublicWeenieDesc> {
        self.weenie(id)
            .map(|w| &w.pwd)
            .or_else(|| self.shop.basket_descriptions.get(&id))
    }

    /// The `PublicWeenieDesc` a price is computed from: the shop's stock row first, then the
    /// object tables. Also returns whether it came from the stock.
    #[must_use]
    fn priced_row(&self, id: ObjectId) -> Option<(PublicWeenieDesc, bool)> {
        if let Some(p) = self.shop.stock_item(id) {
            return Some((p.pwd.clone(), true));
        }
        self.vendor_basket_description(id)
            .map(|pwd| (pwd.clone(), false))
    }

    #[allow(clippy::cast_possible_wrap)]
    fn free_container_slots(&self) -> i32 {
        let Some(player) = self.player else { return 0 };
        let Some(p) = self.weenie(player) else {
            return 0;
        };
        let cap = i32::from(p.pwd.containers_capacity.unwrap_or(0) as i8);
        let used = self
            .inventory(player)
            .map_or(0, |i| i32::try_from(i.containers.len()).unwrap_or(i32::MAX));
        cap - used
    }

    #[allow(clippy::cast_possible_wrap)]
    fn free_item_slots(&self) -> i32 {
        let Some(player) = self.player else { return 0 };
        let Some(p) = self.weenie(player) else {
            return 0;
        };
        let cap = i32::from(p.pwd.items_capacity.unwrap_or(0) as i8);
        let used = self
            .inventory(player)
            .map_or(0, |i| i32::try_from(i.items.len()).unwrap_or(i32::MAX));
        cap - used
    }

    /// Behavior: the message a refused item produces.
    ///
    /// `None` means the vendor takes it. Four refusal strings come from the profile's reason
    /// codes; the fifth, [`NOT_CARRIED`], is this function's own containment test and
    /// precedes the profile entirely.
    ///
    /// **The container escape** is the same `if` the *return-value* half
    /// ([`Self::drag_item_accepted`]) carries:
    ///
    /// ```text
    /// the object holds at least one item, or the shop profile's acceptability reason for
    /// w.pwd is 0  -> true
    /// ```
    ///
    /// A pack with something in it is accepted **with no message**, whatever the vendor thinks of
    /// the pack itself — because the drop is not about the pack, it is about what is inside it.
    /// Without this the two halves would disagree on every full container the player carries and
    /// the gesture would end in *"That item cannot be sold here"*.
    ///
    /// **The ownership test** has `pwd.container_id == player` as its first disjunct, which is
    /// what `is_owned_by_player` implements. A walk of the exhaustive contained-items list is not
    /// a substitute: it holds *items*, naming neither the player's own side packs nor anything he
    /// is wielding, so all five containers `long-solo-play`'s player carries — the Sack and the
    /// four Foci — would answer *false* and be refused with [`NOT_CARRIED`] before the container
    /// path could run at all.
    #[must_use]
    pub fn drag_item_acceptable(&self, item: ObjectId) -> Option<&'static str> {
        let Some(w) = self.weenie(item) else {
            return Some(NOT_CARRIED);
        };
        if !self.is_owned_by_player(item) {
            return Some(NOT_CARRIED);
        }
        // This counts contained items; the side packs are the contained-container count and
        // are **not** part of this test.
        if self.inventory(item).is_some_and(|i| !i.items.is_empty()) {
            return None;
        }
        self.shop.profile.refusal_message(&w.pwd)
    }

    /// Whether the vendor drop target should show an accepting drag cursor.
    ///
    /// [`Self::drag_item_acceptable`] above is the same function's *other* out-channel, the
    /// display-string notice on `0x1A`, and retail's `quiet` argument suppresses
    /// only that. For an unknown object this returns `false`; the message helper instead
    /// supplies [`NOT_CARRIED`]. Real sell-list rows have known objects.
    ///
    /// An owned container with items is accepted even if the vendor would refuse the container
    /// itself. The message helper follows the same rule: insertion offers eligible contents
    /// while omitting the nonempty container. The count here is contained items; side packs are a
    /// separate list.
    #[must_use]
    pub fn drag_item_accepted(&self, item: ObjectId) -> bool {
        // No weenie for the id -> false.
        let Some(w) = self.weenie(item) else {
            return false;
        };
        // Not owned by the player -> "You can only sell items you are carrying", false.
        if !self.is_owned_by_player(item) {
            return false;
        }
        if self.inventory(item).is_some_and(|i| !i.items.is_empty()) {
            return true;
        }
        self.shop.profile.is_acceptable(&w.pwd)
    }

    /// Sell one selected item immediately.
    ///
    /// ```text
    /// w = the weenie for obj_id; if (!w) return false
    /// if (needs_pack_slot(w) && (num_items(w) || num_containers(w)))
    ///     print "Cannot sell container that isn't empty"; return false
    /// if (w.pwd.stack_size >= 2 && split_size < max_split_size)
    ///     print "Cannot sell part of a stack"; return false
    /// last_sale = 0
    /// send_sell(vendor_id, [{amount: 1, iid: obj_id}])
    /// record a shop-event request on vendor_id; increment the busy count
    /// ```
    ///
    /// **`amount` is the literal 1**, whatever the stack size, and the one recorded `0x0060` carries two rows both of amount 1.
    ///
    /// # Errors
    /// The two refusal strings above.
    pub fn sell_single_item(
        &mut self,
        item: ObjectId,
        split: crate::inventory::SplitState,
        req: &mut dyn crate::RequestSink,
        out: &mut dyn crate::NoticeSink,
        now: dereth_primitives::ServerTime,
    ) -> Result<bool, &'static str> {
        let Some(vendor) = self.shop.vendor_id else {
            return Ok(false);
        };
        let Some(w) = self.weenie(item) else {
            return Ok(false);
        };
        let stack = i32::from(w.pwd.stack_size.unwrap_or(0));
        if self.needs_pack_slot(item) {
            let held = self
                .inventory(item)
                .map_or(0, |i| i.items.len() + i.containers.len());
            if held != 0 {
                self.refuse_shop(out, CONTAINER_NOT_EMPTY);
                return Err(CONTAINER_NOT_EMPTY);
            }
        }
        if stack >= 2 && !split.is_whole_stack() {
            self.refuse_shop(out, PART_OF_A_STACK);
            return Err(PART_OF_A_STACK);
        }
        self.shop.last_sale = 0;
        req.send(crate::Request::VendorSell(
            dereth_protocol::trade::VendorSell {
                vendor_id: vendor,
                items: vec![dereth_protocol::trade::ItemProfile {
                    amount: 1,
                    iid: item,
                    pwd: None,
                }],
            },
        ));
        self.record_shop_request(vendor, now);
        if let Some(w) = self.weenie_mut(item) {
            w.sell_state = 0;
        }
        Ok(true)
    }

    /// The button handler's `case 0x100000D3` — the "Sell All" button, and **the one shape the
    /// corpus's single `0x0060` has**: two rows in one message.
    ///
    /// ```text
    /// if (the sell window's list records into sell_list) {
    ///     last_sale = 0
    ///     send_sell(vendor_id, sell_list)
    ///     record a shop-event request on vendor_id; increment the busy count
    ///     clear each row's sell state; retain sell_list
    /// }
    /// refresh_sell_list()
    /// ```
    pub fn sell_all(
        &mut self,
        req: &mut dyn crate::RequestSink,
        out: &mut dyn crate::NoticeSink,
        now: dereth_primitives::ServerTime,
    ) -> bool {
        let _ = out;
        let Some(vendor) = self.shop.vendor_id else {
            return false;
        };
        let Some(m) = self.shop.event_sell() else {
            return false;
        };
        self.shop.last_sale = 0;
        req.send(crate::Request::VendorSell(m));
        self.record_shop_request(vendor, now);
        self.clear_sell_marks();
        true
    }

    /// Add a dragged item to the sell list and mark its sale state.
    ///
    /// **This is `Weenie::sell_state`'s production writer.** `dereth_client::hud` and
    /// `items::widget` read the field; without this writer they would always see zero.
    ///
    /// Returns false when the vendor refuses the item, in which case nothing is marked and
    /// [`Self::drag_item_acceptable`]'s text is emitted.
    pub fn add_item_to_sell(&mut self, item: ObjectId, out: &mut dyn crate::NoticeSink) -> bool {
        if let Some(text) = self.drag_item_acceptable(item) {
            self.refuse_shop(out, text);
            return false;
        }
        // The sell list's add-item call with position `-1`, whose
        // `recurse` is **1** and whose `check_acceptable` is **0**: `drag_item_acceptable` above has
        // already spoken for the thing the player dragged, and the children get their own gate.
        self.vendor_add_item(item, true, false, out)
    }

    /// The sell list's drop with only part of a stack dialled in.
    ///
    /// The same acceptability test as a whole stack comes first. Then the split is asked for:
    /// the dialled amount is placed beside the source, in the source's own container. If that
    /// cannot be asked, the drop is refused with [`CANNOT_SPLIT_TO_SELL`]. Otherwise the player is
    /// told the stack is being split, the source stack goes on the list as the row's placeholder,
    /// and the class and size of the part are remembered so that
    /// [`Self::vendor_split_item_attributes_changed`] can put the new object in that row.
    ///
    /// Returns whether the split was asked for.
    #[allow(clippy::too_many_arguments)]
    pub fn split_item_to_sell(
        &mut self,
        item: ObjectId,
        split: crate::inventory::SplitState,
        now: dereth_primitives::ServerTime,
        out: &mut dyn crate::NoticeSink,
        req: &mut dyn crate::RequestSink,
    ) -> bool {
        if let Some(text) = self.drag_item_acceptable(item) {
            self.refuse_shop(out, text);
            return false;
        }
        let Some((container, wcid, name)) = self.weenie(item).map(|w| {
            (
                w.pwd.container_id.unwrap_or_default(),
                w.pwd.wcid,
                w.object_name(crate::weenie::NameType::Appropriate),
            )
        }) else {
            return false;
        };
        let Some(player) = self.player else {
            return false;
        };
        if !self
            .attempt_to_place_in_container(req, out, item, player, container, false, 0, split, now)
        {
            self.refuse_shop(out, CANNOT_SPLIT_TO_SELL);
            return false;
        }
        let stack_size = self.object_split_size(item, split);
        self.refuse_shop_owned(out, splitting_before_selling(&name));
        self.vendor_add_item(item, true, false, out);
        self.shop.pending_sell_split = Some(PendingSellSplit {
            placeholder: item,
            wcid,
            stack_size,
        });
        true
    }

    /// Match an attribute-change notice against the sell list's pending split.
    ///
    /// Only a declared object (`kind` bit 0) of the remembered class whose stack size, counting no
    /// stack as one, is exactly the split size answers it. The new object then takes the
    /// placeholder's row -- marked for sale when it holds nothing -- and the placeholder leaves the
    /// list with its mark taken off. The wait ends with the match. Returns whether it matched.
    pub fn vendor_split_item_attributes_changed(
        &mut self,
        item: ObjectId,
        kind: u32,
        out: &mut dyn crate::NoticeSink,
    ) -> bool {
        let Some(pending) = self.shop.pending_sell_split else {
            return false;
        };
        if kind & 1 == 0 {
            return false;
        }
        let Some(w) = self.weenie(item) else {
            return false;
        };
        if w.pwd.wcid != pending.wcid
            || u32::from(w.pwd.stack_size.unwrap_or(0).max(1)) != pending.stack_size
        {
            return false;
        }
        let description = w.pwd.clone();
        self.shop.pending_sell_split = None;
        let empty = self
            .inventory(item)
            .is_none_or(|i| i.items.is_empty() && i.containers.is_empty());
        if let Some(at) = self
            .shop
            .sell_list
            .iter()
            .position(|(id, _)| *id == pending.placeholder)
        {
            if empty {
                self.shop.basket_descriptions.insert(item, description);
                self.shop.sell_list.insert(at, (item, 1));
                if self.selected == Some(pending.placeholder) {
                    self.set_selected_object(Some(item), false, out);
                }
                if let Some(w) = self.weenie_mut(item) {
                    w.sell_state = 1;
                }
            }
            self.remove_from_sell_list(pending.placeholder);
        }
        true
    }

    /// Insert into the sell list, including one level of container contents: *"drag your whole
    /// backpack as a proxy for every item in it"*.
    ///
    /// The self-call passes eight arguments, and the function returns `-1` unless an insert
    /// happens:
    ///
    /// ```text
    /// ret = -1
    /// remove_existing and the list already holds id:
    ///     if that row's position < insert_at, insert_at -= 1; delete the row
    /// check_acceptable and not is_acceptable(profile, w.pwd) and not is_player(w)  -> -1
    /// the object holds no items and no containers:
    ///     the list is element 0x100000CE -> the object's sell state = 1
    ///     insert id at insert_at; ret = insert_at
    /// not recurse                     -> ret
    /// the object holds no items       -> ret
    /// display-string notice (0x1A, "Selling contents of %s" % name)
    /// for each child in its contained-items list:
    ///     n = add child (insert_at, remove_existing, 0, 1, 1, -1); if n != -1, insert_at = n + 1
    /// -> ret
    /// ```
    ///
    /// Four things decide the observable behaviour and each is exactly one line above:
    ///
    /// * **The container is never listed.** It fails the leaf gate, so what the
    ///   player ends up offering is its contents and not the pack.
    /// * **Each child is filtered on its own `pwd`** — `check_acceptable = 1` on the recursive
    ///   call — through the profile's acceptance predicate, which is
    ///   `inq_acceptability(..) == 0`. The is-player test is the escape beside it and cannot fire
    ///   for a contained object; it is reproduced because it is the guard's second term and a build
    ///   that dropped it would be reading a different `if`.
    /// * **One level deep.** The children are called with `recurse = 0`, and
    ///   the traversal uses the contained-items list; side packs are a separate list
    ///   and are not walked, so a pouch inside the pack contributes nothing either way.
    /// * **The notice comes first**, before any child is looked at, and it names the container.
    ///
    /// `insert_at` is `-1` from the public add-item path, and insertion returns the passed index, so
    /// an appending insert returns `-1`, the caller's `-1` check treats it as "not inserted" and
    /// every child appends in contained-items order. That is why this build's `Vec` push is the
    /// whole of the position arithmetic.
    ///
    /// **One declared deviation:** retail's `remove_existing`
    /// deletes a row the list already holds and re-appends it, *moving* it to the end. This build
    /// leaves it where it is, which matters only when a container is dropped over a basket that
    /// already names one of its contents.
    ///
    /// Returns whether anything was added.
    fn vendor_add_item(
        &mut self,
        item: ObjectId,
        recurse: bool,
        check_acceptable: bool,
        out: &mut dyn crate::NoticeSink,
    ) -> bool {
        let Some(w) = self.weenie(item) else {
            return false;
        };
        // Checking acceptability, an unacceptable non-player object is refused (`-1`).
        if check_acceptable && !self.shop.profile.is_acceptable(&w.pwd) && !w.is_player() {
            return false;
        }
        let (items, containers) = self
            .inventory(item)
            .map_or((Vec::new(), 0), |i| (i.items.clone(), i.containers.len()));
        let mut added = false;
        // The leaf gate, which guards setting the sell state to 1 as well as the insert.
        if items.is_empty()
            && containers == 0
            && !self.shop.sell_list.iter().any(|(id, _)| *id == item)
        {
            // The amount on a sell row is 1; see [`Self::sell_single_item`].
            self.shop.basket_descriptions.insert(item, w.pwd.clone());
            self.shop.sell_list.push((item, 1));
            self.set_selected_object(Some(item), false, out);
            if let Some(w) = self.weenie_mut(item) {
                w.sell_state = 1;
            }
            out.emit(crate::Notice::AddItemToSell(item));
            added = true;
        }
        if !recurse || items.is_empty() {
            return added;
        }
        let name = self.weenie(item).map_or_else(String::new, |w| {
            w.object_name(crate::weenie::NameType::Appropriate)
        });
        self.refuse_shop_owned(out, format!("{SELLING_CONTENTS_OF}{name}"));
        for child in items {
            added |= self.vendor_add_item(child, false, true, out);
        }
        added
    }

    /// Remove a sell-list row and clear its sale mark: the Clear Item button.
    pub fn remove_from_sell_list(&mut self, item: ObjectId) -> bool {
        let before = self.shop.sell_list.len();
        self.shop.sell_list.retain(|(id, _)| *id != item);
        if self.shop.sell_list.len() == before {
            return false;
        }
        if let Some(w) = self.weenie_mut(item) {
            w.sell_state = 0;
        }
        self.prune_vendor_basket_descriptions();
        true
    }

    /// Behavior: every row of the sell list goes back
    /// to `sell_state = 0`. Returns how many marks were taken off.
    ///
    /// Explicitly clearing or closing also discards the rows.
    pub fn flush_sell_list_sell_state(&mut self) -> usize {
        let n = self.clear_sell_marks();
        self.shop.sell_list.clear();
        self.prune_vendor_basket_descriptions();
        n
    }

    /// Forget descriptions no remaining cart row refers to.
    pub fn prune_vendor_basket_descriptions(&mut self) {
        let shop = &mut self.shop;
        shop.basket_descriptions.retain(|id, _| {
            shop.buy_list
                .iter()
                .chain(&shop.sell_list)
                .any(|(row, _)| row == id)
        });
    }

    /// Submitting a sale clears its pending marks without discarding the basket.
    fn clear_sell_marks(&mut self) -> usize {
        let ids: Vec<ObjectId> = self.shop.sell_list.iter().map(|(id, _)| *id).collect();
        let mut n = 0;
        for id in ids {
            if let Some(w) = self.weenie_mut(id) {
                if w.sell_state != 0 {
                    w.sell_state = 0;
                    n += 1;
                }
            }
        }
        n
    }

    /// Record the outstanding shop request and increment the busy count after sending.
    fn record_shop_request(&mut self, vendor: ObjectId, now: dereth_primitives::ServerTime) {
        self.request_lock.record(
            vendor,
            crate::inventory::requests::InventoryRequest::ShopEvent,
            now,
        );
        // The shop's answer ends with the use-done acknowledgement, which lowers it again.
        self.magic.busy_count += 1;
    }

    fn refuse_shop(&mut self, out: &mut dyn crate::NoticeSink, text: &str) {
        self.refuse_shop_owned(out, text.to_string());
    }

    /// The same `0x1A` display-string notice for a line this window builds rather than
    /// quotes — the client's *"Selling contents of %s"* is the only one.
    fn refuse_shop_owned(&mut self, out: &mut dyn crate::NoticeSink, text: String) {
        out.emit(crate::Notice::DisplayString {
            channel: crate::inventory::requests::FEEDBACK_CHANNEL,
            text,
            feedback: dereth_client_contract::feedback::Feedback::WARNING,
        });
    }

    /// Fill the buy basket with missing spell components, subject to stock and price limits.
    ///
    /// This is `/fillcomps`. It walks (WCID -> the level the player
    /// wants kept in stock), asks the [`crate::magic::ComponentTracker`] how many are already held,
    /// and adds the shortfall to the **buy basket** — it does not send. The button handler's
    /// "Buy All" is what puts `0x005F` on the wire afterwards.
    ///
    /// ```text
    /// if (vendor_id == 0) { print "You need an open vendor."; return }
    /// refresh the items window with filter 0x1000, false          // TYPE_SPELL_COMPONENTS filter
    /// for ((wcid, desired) in desired_comps_) {
    ///     if (max_price != 0 && running >= max_price) {
    ///         drop buy_list's last row; print "Buying aborted; max price reached.\n"; break }
    ///     have = num_component(wcid)
    ///     cat  = determine_component_category(wcid)
    ///     if ((cat == category || category == Undef) && have < desired) {
    ///         want = desired - have
    ///         clamped = want
    ///         if (shop_has_item(wcid) gives iid and clamped) {
    ///             already = amount of iid already in buy_list           // subtracted from want
    ///             want -= already
    ///             if (clamped < want) { add_missing_comp(wcid, msg); want = clamped }
    ///             w = the weenie for iid
    ///             if (w) { add to the buy list (w, want)
    ///                      running += vendor_sell_price(w.pwd, want) }
    ///         } else add_missing_comp(wcid, msg)
    ///     }
    /// }
    /// if (msg is non-empty) { add msg to the scroll; add "\n" to the scroll }
    /// rebuild the buy list's contents; refresh the buy panel, transaction value and total value
    /// last_sale = 0; open the vendor panel's tab 0x100000BA           // the Buying tab
    /// ```
    ///
    /// Three details that are easy to get wrong and are reproduced as written:
    ///
    /// * the **abort test is `>=` against the running total *before* the row is priced**, and it
    ///   then removes the row it had *already* added on the previous pass (the basket's tail), so a
    ///   `max_price` run always ends one row short of the limit rather than at it;
    /// * `max_price == 0` means **no limit**, not "spend nothing";
    /// * the "not enough" message is raised in **two** places — once when the shop does not stock
    ///   the component at all, and once when it stocks fewer than the shortfall — and in the second
    ///   case the row is *still* bought, at the clamped count.
    ///
    /// `category` is `None` for the client's undefined component category, which means "every
    /// category". Returns [`FillComponents`].
    pub fn fill_component_list(
        &mut self,
        category: Option<u32>,
        max_price: i32,
        out: &mut dyn crate::NoticeSink,
    ) -> FillComponents {
        let mut r = FillComponents::default();
        if !self.shop.is_open() {
            self.refuse_shop(out, NEED_AN_OPEN_VENDOR);
            return r;
        }
        let desired: Vec<(u32, i32)> = self
            .player_system
            .desired_comps
            .iter()
            .map(|(k, v)| (*k, *v))
            .collect();
        r.desired_rows = desired.len();
        let mut missing = String::new();
        let mut running = 0i32;
        for (wcid, want_level) in desired {
            if max_price != 0 && running >= max_price {
                self.shop.buy_list.pop();
                r.aborted = true;
                out.emit(crate::Notice::DisplayString {
                    feedback: dereth_client_contract::feedback::Feedback::LOCAL,
                    channel: crate::inventory::requests::FEEDBACK_CHANNEL,
                    text: BUYING_ABORTED.to_string(),
                });
                break;
            }
            let have = self
                .magic
                .components
                .num_component(&self.magic.catalogue, wcid);
            let cat = self.magic.catalogue.determine_component_category(wcid);
            if category.is_some_and(|c| c != cat) || have >= i64::from(want_level) {
                continue;
            }
            r.short_rows += 1;
            let mut want = i32::try_from(i64::from(want_level) - have).unwrap_or(i32::MAX);
            let Some((iid, clamped)) = self.shop.shop_has_item(wcid, want) else {
                add_missing_comp(&self.magic.catalogue, wcid, &mut missing);
                r.not_stocked += 1;
                continue;
            };
            // The first buy-list row for `iid` is subtracted from `want`.
            if let Some((_, already)) = self.shop.buy_list.iter().find(|(id, _)| *id == iid) {
                want -= *already;
            }
            if clamped < want {
                add_missing_comp(&self.magic.catalogue, wcid, &mut missing);
                r.short_stocked += 1;
                want = clamped;
            }
            let Some(pwd) = self.shop.stock_item(iid).map(|p| p.pwd.clone()) else {
                continue;
            };
            self.shop.basket_descriptions.insert(iid, pwd.clone());
            self.shop.buy_list.push((iid, want));
            running += self.shop.profile.vendor_sell_price(&pwd, want);
            r.added += 1;
            r.bought_units += want;
        }
        r.total = running;
        if !missing.is_empty() {
            out.emit(crate::Notice::DisplayString {
                feedback: dereth_client_contract::feedback::Feedback::LOCAL,
                channel: crate::inventory::requests::FEEDBACK_CHANNEL,
                text: missing.clone(),
            });
            r.missing_line = Some(missing);
        }
        // `last_sale = 0`, then open tab `0x100000BA` — the Buying tab. The tab itself is
        // `dereth_ui_screens`'; what this crate owns is the reset.
        self.shop.last_sale = 0;
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weenie::item_type;

    fn item(value: u32, stack: u16, obj_type: u32) -> PublicWeenieDesc {
        PublicWeenieDesc {
            value: Some(value),
            stack_size: if stack == 0 { None } else { Some(stack) },
            obj_type,
            ..PublicWeenieDesc::default()
        }
    }

    fn shop() -> VendorProfile {
        VendorProfile {
            item_types: item_type::MISC | item_type::PROMISSORY_NOTE,
            min_value: -1,
            max_value: -1,
            magic: 0,
            buy_price: 0.5,
            sell_price: 1.5,
            ..VendorProfile::default()
        }
    }

    /// The price wrappers treat a stack's value as the **whole stack's** value.
    #[test]
    fn stack_values_are_divided_before_the_count_is_reapplied() {
        let s = shop();
        let stack = item(1000, 10, item_type::MISC);
        assert_eq!(s.vendor_sell_price(&stack, 10), 1500);
        // The vendor pays 0.5 * 100 * 10 = 500.
        assert_eq!(s.vendor_buy_price(&stack), 500);
        // An unstacked item: unit is the whole value, count 1.
        let single = item(100, 0, item_type::MISC);
        assert_eq!(s.vendor_buy_price(&single), 50);
    }

    /// Vendor acceptability checks, in order, with the four reason codes.
    #[test]
    fn acceptability_returns_the_documented_reason_codes_in_order() {
        let mut s = shop();
        assert_eq!(s.inq_acceptability(&item(100, 0, item_type::MISC)), 0);
        assert_eq!(
            s.inq_acceptability(&item(100, 0, item_type::ARMOR)),
            1,
            "wrong type for this shop"
        );
        // The retained bit rescues a wrong-typed item from reason 1.
        let mut retained = item(100, 0, item_type::ARMOR);
        retained.bitfield = bitfield::CANNOT_BE_SALVAGED;
        assert_eq!(s.inq_acceptability(&retained), 0);

        assert_eq!(
            s.inq_acceptability(&item(0, 0, item_type::MISC)),
            2,
            "no value"
        );
        s.min_value = 50;
        assert_eq!(
            s.inq_acceptability(&item(10, 0, item_type::MISC)),
            3,
            "too cheap"
        );
        s.min_value = -1;
        s.max_value = 50;
        assert_ne!(
            s.inq_acceptability(&item(1000, 0, item_type::MISC)),
            0,
            "too valuable"
        );
        // A refused item prices at zero.
        assert_eq!(s.vendor_buy_price(&item(1000, 0, item_type::MISC)), 0);
    }

    /// Messages for each refusal reason.
    #[test]
    fn the_refusal_messages_match_the_reason_codes() {
        let mut s = shop();
        assert_eq!(s.refusal_message(&item(100, 0, item_type::MISC)), None);
        assert_eq!(
            s.refusal_message(&item(100, 0, item_type::ARMOR)),
            Some("That item cannot be sold here")
        );
        assert_eq!(
            s.refusal_message(&item(0, 0, item_type::MISC)),
            Some("That item has no value and cannot be sold")
        );
        s.min_value = 50;
        assert_eq!(
            s.refusal_message(&item(10, 0, item_type::MISC)),
            Some("That item is too cheap to sell here")
        );
        s.min_value = -1;
        s.max_value = 50;
        assert_eq!(
            s.refusal_message(&item(1000, 0, item_type::MISC)),
            Some("That item is too valuable to sell here")
        );
    }
}
