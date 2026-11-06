//! The shops: a shop's sales, the catalog of a town's shops, a player's own consignments and the
//! window a new consignment is put up in.
//!
//! - [`Shop`]: a shop's window: its name, description and keeper's portrait, its sales a page at a
//!   time ([`Sale`]: what is sold, for what and how many of it, how many are left), bought with the
//!   purchase button or by dragging a sale into the pack.
//! - [`catalog::Catalog`]: the shops of a catalog, one entered by pressing it.
//! - [`consignment::Consignments`]: the player's consignments and the coins and items they made.
//! - [`new_sale::NewSale`]: a new consignment: an item dropped on it, its price, put up.
//!
//! What the scripts answer of a purchase or a consignment (how many the player may buy, what a
//! consignment costs) comes with the frame ([`Facts`]).

pub mod catalog;
pub mod consignment;
pub mod new_sale;

use std::any::Any;
use std::collections::BTreeMap;

use auberean_text::{StringInfo, Value as Var};
use auberean_ui::framework::FrameId;
use auberean_ui::manager::global;
use auberean_ui::system::Value;
use auberean_ui::token::{attr, msg, state};
use auberean_ui::types::{button_id, text::text_msg};
use auberean_ui::{Handle, Mail, Token, Ui};

use crate::gameplay::{self, Child, Notice, Record, Request, View, PLAY_LAYOUT_ENUM};
use crate::inventory::{self, NO_OBJECT};

/// The scripts' notices to the shop panels, by the native that sends each, and the panels' own.
pub mod notice {
    /// The catalog shown: its header (the record), its shops (the record).
    pub const VIEW_CATALOG: u32 = 0x1AD3 << 16 | 0x0001;
    /// The catalog left.
    pub const LEAVE_CATALOG: u32 = 0x1AD3 << 16 | 0x0002;
    /// A shop shown, or its sales changed (the record: the shop).
    pub const VIEW_STORE: u32 = 0x1AD3 << 16 | 0x0003;
    pub const UPDATE_STORE: u32 = 0x1AD3 << 16 | 0x0004;
    /// How many of a sale are left: the item (high and low), how many.
    pub const UPDATE_SALE: u32 = 0x1AD3 << 16 | 0x0005;
    /// The shop left.
    pub const LEAVE_STORE: u32 = 0x1AD3 << 16 | 0x0006;
    /// A purchase to confirm: the product (the record), how many, what it costs (the record), how
    /// much of that.
    pub const CONFIRM_PURCHASE: u32 = 0x1AD3 << 16 | 0x0007;
    /// The player's consignments shown, or changed (the record: them as a shop).
    pub const VIEW_CONSIGNMENT: u32 = 0x1AD3 << 16 | 0x0008;
    pub const UPDATE_CONSIGNMENT: u32 = 0x1AD3 << 16 | 0x0009;
    /// The consignments left.
    pub const LEAVE_CONSIGNMENT: u32 = 0x1AD3 << 16 | 0x000A;
    /// The shop's page and its size changed: the page's first sale, how many there are. (Nothing
    /// sends it.)
    pub const UPDATE_POSITION: u32 = 0x48D;
    /// A new consignment to put up: the item (high and low), how many (none: an empty window).
    // D2-00277
    pub const INIT_NEW_SALE: u32 = 0x494;
    /// The player's selection changed (high and low).
    pub const SELECTION_CHANGED: u32 = crate::inventory::notice::SELECTION_CHANGED;
}

mod name {
    use auberean_ui::Token;
    pub const SHOP: Token = Token::of("Shop");
    pub const SORT_NAME: Token = Token::of("SortName");
    pub const SORT_TEXT: Token = Token::of("SortText");
    pub const NEXT: Token = Token::of("ShopNextButton");
    pub const PREV: Token = Token::of("ShopPrevButton");
    pub const CLOSE: Token = Token::of("ShopCloseButton");
    pub const HELP: Token = Token::of("ShopHelpButton");
    pub const PURCHASE: Token = Token::of("PurchaseButton");
    pub const SHOP_NAME: Token = Token::of("ShopName");
    pub const SHOP_DESC: Token = Token::of("ShopDesc");
    pub const PORTRAIT: Token = Token::of("Portrait");
    pub const ITEM_LIST: Token = Token::of("ItemList");
    pub const SALE: Token = Token::of("Sale");
    pub const PRODUCT_ICON: Token = Token::of("ProductIcon");
    pub const ITEM_ICON: Token = Token::of("ItemIcon");
    pub const PRODUCT_DESC: Token = Token::of("ProductDesc");
    pub const COST_ICON: Token = Token::of("CostIcon");
    pub const COST_QUANTITY: Token = Token::of("CostQuantity");
    pub const STOCK_QUANTITY: Token = Token::of("StockQuantity");
    pub const ITEM_ID: Token = Token::of("ItemID");
    pub const UNAVAILABLE: Token = Token::of("unavailable");
    pub const SELECTED: Token = Token::of("selected");
    pub const QUANTITY: Token = Token::of("quantity");
    pub const PRODUCT: Token = Token::of("product");
    pub const TRADE: Token = Token::of("trade");
    pub const COST: Token = Token::of("cost");
}

/// The help page's table.
pub(crate) const HELP_TABLE: u32 = crate::inventory::HELP_TABLE;

/// How long a hidden shop window keeps its elements, and how long the page buttons wait after a
/// press, in seconds; how long after the last key the sort field is read.
const KEEP: f64 = 120.0;
const PAGE_WAIT: f64 = 1.0;
const SORT_WAIT: f64 = 3.0;
/// How many sales a page shows.
const PAGE: i32 = 7;

/// A sale's state.
pub mod sale_state {
    pub const RESTRICTED: u32 = 1;
    pub const EXPIRED: u32 = 2;
    pub const SOLD: u32 = 3;
}

/// One sale of a shop (or one of the player's consignments), as the scripts filled it.
// D2-00275
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sale {
    pub item: u64,
    /// The item's record (what kind of thing it is).
    pub item_did: u32,
    pub icon: u32,
    /// The layers of the item's icon (each a picture and its color); the icon alone when none.
    pub icon_layers: Vec<(u32, u32)>,
    pub description: StringInfo,
    pub cost_description: StringInfo,
    pub cost_icon: u32,
    pub cost_quantity: f32,
    /// How many are left (-1: no end of them; 0: sold out).
    pub stock: i32,
    pub ordinal: u32,
    /// Restricted, expired, sold ([`sale_state`]), or none.
    pub state: u32,
}

/// A shop (a catalog's shop, or the player's consignments) as the scripts filled it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Store {
    pub id: u32,
    pub name: StringInfo,
    pub description: StringInfo,
    /// What the sales are sorted by (none when it does not resolve).
    pub sorter: StringInfo,
    pub portrait: u32,
    /// Its sales in order of their ordinals.
    pub sales: Vec<Sale>,
    /// The page's first sale, and how many there are (a catalog's shop: how many it sells).
    pub position: i32,
    pub size: i32,
}

impl Store {
    /// A sale by its ordinal (the first with it, in order); none when it has no such sale.
    // D2-12381
    #[must_use]
    pub fn sale(&self, ordinal: u32) -> Option<&Sale> {
        self.sales.iter().find(|s| s.ordinal == ordinal)
    }
}

/// What the scripts answer for the shop panels: how many of each sale's item the player may buy,
/// and what share of a consignment's price it costs to put up.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    pub version: u64,
    pub max_purchase: BTreeMap<u64, u32>,
    pub consignment_charge: f32,
}

/// What the shop panels ask of the player's scripts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShopRequest {
    /// Buy so many of a sale's item, into a slot of the pack (0: wherever).
    Purchase {
        item: u64,
        quantity: u32,
        slot: u32,
    },
    /// A purchase confirmed, or not.
    Confirm(bool),
    /// Leave the shop, the catalog, the consignments.
    LeaveStore,
    LeaveCatalog,
    LeaveConsignment,
    /// See the player's consignments; enter a catalog's shop.
    EnterConsignment,
    EnterShop(u32),
    /// The shop's next or previous page; the page from a sale on.
    NextSales,
    PrevSales,
    Sales(u32),
    /// Add so many of an item to a consignment already up.
    Restock {
        item: u64,
        quantity: u32,
        sale: u32,
    },
    /// Put so many of an item up for a price.
    Sell {
        item: u64,
        quantity: u32,
        price: u32,
    },
    /// Collect a consignment's coins (sold) or its item.
    Collect {
        sale: u32,
        sold: bool,
    },
}

/// A string table entry of the messages, with its variables.
pub(crate) fn entry(ui: &Ui, id: &str, vars: Vec<(Token, Var)>) -> StringInfo {
    let table = ui
        .assets()
        .data_id_by_enum(crate::flow::MESSAGES, crate::flow::TABLE_MAPPING);
    let mut s = StringInfo::entry(table, Token::of(id).raw());
    for (k, v) in vars {
        s.add_var(k.raw(), v);
    }
    s
}

/// A string reference written.
pub(crate) fn write(ui: &Ui, s: &StringInfo) -> String {
    ui.assets().write(s).unwrap_or_default()
}

/// The shop a notice names.
pub(crate) fn store_of(n: &Notice) -> Option<Store> {
    n.records.iter().flatten().find_map(|r| match r {
        Record::Store(s) => Some(s.clone()),
        _ => None,
    })
}

/// The string references a notice names, in order.
pub(crate) fn texts_of(n: &Notice) -> Vec<StringInfo> {
    n.records
        .iter()
        .flatten()
        .filter_map(|r| match r {
            Record::Text(t) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

/// A message for a dialog of its own, as the scripts' message notice shows one, going up at once
/// in place of the dialog up (the shop's messages do).
pub(crate) fn display_message(text: StringInfo) -> Request {
    Request::Notice(Notice {
        native: gameplay::notice::DISPLAY_MESSAGE,
        words: vec![0, 1],
        records: vec![Some(Record::Text(text))],
        ..Notice::default()
    })
}

/// A cost or a quantity: whole, as a whole number; else to two places.
pub(crate) fn quantity_var(q: f32) -> Var {
    let whole = dereth_primitives::num::to_i32(q);
    #[allow(clippy::cast_precision_loss)]
    if (whole as f32 - q).abs() == 0.0 {
        Var::Int(whole)
    } else {
        Var::Float {
            value: f64::from(q),
            decimals: 2,
        }
    }
}

/// A shop's window.
#[derive(Debug)]
pub struct Shop {
    frame: FrameId,
    /// The sale selected.
    selected: Option<Handle>,
    field: Option<Handle>,
    sort_name: Option<Handle>,
    sort_text: Option<Handle>,
    /// The sort field typed into: read at this time.
    sort_typed: bool,
    sort_time: f64,
    /// The page buttons wait until this time (0: they do not).
    nav_wait: f64,
    next: Option<Handle>,
    prev: Option<Handle>,
    /// Each sale's row, by its item.
    sales: BTreeMap<u64, Handle>,
    /// A purchase under way: the item and the slot.
    buying: u64,
    buying_slot: u32,
    /// The question up, the split asked for, the context its sales are dragged in.
    dialog: u32,
    stack_context: u32,
    context: u32,
    /// When it was hidden.
    hidden_at: f64,
    facts: Facts,
}

impl Shop {
    /// The window's framework, the scripts' shop notices, the drags out of it, the questions and
    /// splits, its buttons and its sort field heard; hidden.
    // D2-02307
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        let context = crate::inventory::container::request_context();
        if let Some(l) = ui.frame_listener(frame) {
            for b in [
                name::NEXT,
                name::PREV,
                name::CLOSE,
                name::HELP,
                name::PURCHASE,
            ] {
                ui.listen_for_message(l, b, msg::BUTTON_RELEASE_ON);
            }
            for m in [
                text_msg::KEY_INPUT,
                text_msg::CHARACTER_INPUT,
                text_msg::DEACTIVATED,
            ] {
                ui.listen_for_message(l, name::SORT_TEXT, m);
            }
        }
        let mut me = Self {
            frame,
            selected: None,
            field: None,
            sort_name: None,
            sort_text: None,
            sort_typed: false,
            sort_time: 0.0,
            nav_wait: 0.0,
            next: None,
            prev: None,
            sales: BTreeMap::new(),
            buying: NO_OBJECT,
            buying_slot: 0,
            dialog: 0,
            stack_context: 0,
            context,
            hidden_at: 0.0,
            facts: Facts::default(),
        };
        me.show(ui, false, &mut Vec::new());
        me
    }

    /// The sale rows' items, in order, and the one selected.
    #[must_use]
    pub fn sales(&self, ui: &Ui) -> (Vec<u64>, Option<u64>) {
        let list = self
            .field
            .and_then(|f| ui.child_recursive(f, name::ITEM_LIST));
        let items = list.map_or_else(Vec::new, |l| {
            ui.list_items(l)
                .iter()
                .filter_map(|r| ui.node(*r).and_then(|n| n.instance_id_attr(name::ITEM_ID)))
                .collect()
        });
        let sel = self
            .selected
            .and_then(|s| ui.node(s))
            .and_then(|n| n.instance_id_attr(name::ITEM_ID));
        (items, sel)
    }

    /// The question up (0: none).
    #[must_use]
    pub const fn dialog(&self) -> u32 {
        self.dialog
    }

    /// Its window made when first shown: the sort name and field (hidden; the field takes
    /// numbers), the page buttons; the clock heard.
    // D2-02282
    fn create_managed_elements(&mut self, ui: &mut Ui) -> bool {
        if self.field.is_some() {
            return true;
        }
        self.field = ui.frame_create_root(self.frame, PLAY_LAYOUT_ENUM, name::SHOP);
        let Some(f) = self.field else { return false };
        self.sort_name = ui.child_recursive(f, name::SORT_NAME);
        if let Some(s) = self.sort_name {
            ui.set_visible(s, false);
        }
        self.sort_text = ui.child_recursive(f, name::SORT_TEXT);
        if let Some(s) = self.sort_text {
            ui.set_input_filter(s, Some(auberean_ui::types::text::number_input_filter));
            ui.set_visible(s, false);
        }
        self.next = ui.child_recursive(f, name::NEXT);
        self.prev = ui.child_recursive(f, name::PREV);
        if self.sort_name.is_some()
            && self.sort_text.is_some()
            && self.next.is_some()
            && self.prev.is_some()
        {
            if let Some(l) = ui.frame_listener(self.frame) {
                ui.listen_for_global_message(l, global::TIME);
            }
            return true;
        }
        false
    }

    /// Its window let go (made again when next shown).
    // D2-02295, D2-02299
    fn destroy_managed_elements(&mut self, ui: &mut Ui) {
        if let Some(f) = self.field.take() {
            ui.frame_remove_root(self.frame, f);
            ui.remove(f);
        }
        self.sort_name = None;
        self.sort_text = None;
        self.next = None;
        self.prev = None;
        self.selected = None;
        self.sales.clear();
        if let Some(l) = ui.frame_listener(self.frame) {
            ui.unregister_for_all_global_messages(l);
        }
    }

    /// The page buttons by the page: previous unavailable on the first page, next on the last
    /// (of seven sales); each lit again unless the buttons wait.
    // D2-02284
    fn update_position(&self, ui: &mut Ui, position: i32, size: i32) {
        let set = |ui: &mut Ui, h: Handle, a: Token, on: bool| ui.set_attr(h, a, Value::Bool(on));
        if let Some(p) = self.prev {
            if position == 0 {
                set(ui, p, name::UNAVAILABLE, true);
                set(ui, p, attr::GHOSTED, true);
            } else {
                set(ui, p, name::UNAVAILABLE, false);
                if self.nav_wait == 0.0 {
                    set(ui, p, attr::GHOSTED, false);
                }
            }
        }
        if let Some(n) = self.next {
            if size - PAGE <= position {
                set(ui, n, name::UNAVAILABLE, true);
                set(ui, n, attr::GHOSTED, true);
            } else {
                set(ui, n, name::UNAVAILABLE, false);
                if self.nav_wait == 0.0 {
                    set(ui, n, attr::GHOSTED, false);
                }
            }
        }
    }

    /// A purchase question for the scripts: so many of a product for so much of what it costs.
    // D2-02286
    fn confirm_purchase(&mut self, ui: &Ui, n: &Notice, out: &mut Vec<Request>) {
        if !self.shown(ui) {
            return;
        }
        let texts = texts_of(n);
        let (product, trade) = (
            texts.first().cloned().unwrap_or_default(),
            texts.get(1).cloned().unwrap_or_default(),
        );
        let quantity = n.word_at(1);
        let cost = n.word_at(3);
        let id = if quantity == 1 {
            "ID_Store_ConfirmPurchaseSingular"
        } else {
            "ID_Store_ConfirmPurchasePlural"
        };
        let s = entry(
            ui,
            id,
            vec![
                (name::PRODUCT, Var::Text(Box::new(product))),
                (name::QUANTITY, Var::UInt(quantity)),
                (name::TRADE, Var::Text(Box::new(trade))),
                (name::COST, Var::UInt(cost)),
            ],
        );
        let id = crate::dialog::new_context();
        out.push(Request::Dialog(gameplay::Dialog {
            id,
            kind: gameplay::dialog::CONFIRMATION,
            text: write(ui, &s),
            modal: false,
            info: None,
        }));
        self.dialog = id;
    }

    /// A sale selected (none: no sale): the one before plain, this one selected, the purchase
    /// button lit while one is; its item selected for the player.
    // D2-02288
    fn select(&mut self, ui: &mut Ui, row: Option<Handle>, out: &mut Vec<Request>) {
        if row != self.selected {
            if let Some(s) = self.selected {
                ui.set_state(s, state::STATELESS);
            }
            if let Some(r) = row {
                ui.set_state(r, name::SELECTED);
            }
            self.selected = row;
        }
        if let Some(b) = self
            .field
            .and_then(|f| ui.child_recursive(f, name::PURCHASE))
        {
            ui.set_attr(b, attr::GHOSTED, Value::Bool(self.selected.is_none()));
        }
        if let Some(item) = self
            .selected
            .and_then(|s| ui.node(s))
            .and_then(|n| n.instance_id_attr(name::ITEM_ID))
        {
            out.push(Request::SetSelection(item));
        }
    }

    /// So many asked for: the item bought, into the slot it was dragged to.
    // D2-02289
    fn finish_drag_from_shop(&self, quantity: u32, out: &mut Vec<Request>) {
        if self.buying != NO_OBJECT {
            out.push(Request::Shop(ShopRequest::Purchase {
                item: self.buying,
                quantity,
                slot: self.buying_slot,
            }));
        }
    }

    /// A sale's item to buy, into a slot: not a sale here, or unavailable, the player told it may
    /// not be bought; one the player cannot afford, told so; more than one to be had, how many
    /// asked; else one bought.
    // D2-02298
    fn purchase_item(&mut self, ui: &Ui, item: u64, slot: u32, out: &mut Vec<Request>) {
        let unavailable = self
            .sales
            .get(&item)
            .and_then(|r| ui.node(*r))
            .map(|n| n.bool_attr(name::UNAVAILABLE));
        if unavailable != Some(false) {
            out.push(display_message(entry(
                ui,
                "ID_Store_CantPurchase",
                Vec::new(),
            )));
            return;
        }
        self.buying = item;
        self.buying_slot = slot;
        let most = self.facts.max_purchase.get(&item).copied().unwrap_or(0);
        if most == 0 {
            out.push(display_message(entry(
                ui,
                "ID_Store_CantAfford",
                Vec::new(),
            )));
            return;
        }
        if most != 1 {
            self.stack_context = inventory::next_stack_context();
            let mut ask =
                inventory::init_stack(self.stack_context, 0, most, NO_OBJECT, NO_OBJECT, NO_OBJECT);
            ask.records = vec![Some(Record::Text(entry(
                ui,
                "ID_Store_ShopStack",
                Vec::new(),
            )))];
            out.push(Request::Notice(ask));
            return;
        }
        self.finish_drag_from_shop(1, out);
    }

    /// A sale's row filled from the sale: its item's icon (dragable out of the shop, as an item
    /// from this window), its description (also the icon's tooltip), what it costs and how much,
    /// how many are left (restricted and sold out make the row unavailable); the row knows its
    /// item, its presses heard.
    // D2-02302
    fn post_sale(&mut self, ui: &mut Ui, sale: &Sale, row: Handle) {
        if let Some(icon) = ui
            .child_recursive(row, name::PRODUCT_ICON)
            .and_then(|p| ui.child_recursive(p, name::ITEM_ICON))
        {
            if sale.icon_layers.is_empty() {
                ui.set_image(icon, sale.icon, 2);
            } else {
                ui.set_icon_desc(icon, &sale.icon_layers);
            }
            let tip = write(ui, &sale.description);
            ui.set_tooltip(icon, &tip);
            ui.set_attr(
                icon,
                inventory::attr::SHORTCUT_TYPE,
                Value::Id(inventory::ITEM_SHORTCUT),
            );
            ui.set_attr(
                icon,
                inventory::attr::SHORTCUT_DATA,
                Value::InstanceId(sale.item),
            );
            #[allow(clippy::cast_possible_wrap)]
            ui.set_attr(
                icon,
                inventory::attr::SOURCE_CONTEXT,
                Value::Int(self.context as i32),
            );
            ui.set_attr(icon, attr::DRAGABLE, Value::Bool(true));
        }
        if let Some(d) = ui.child_recursive(row, name::PRODUCT_DESC) {
            let t = write(ui, &sale.description);
            ui.set_text(d, &t);
        }
        if let Some(c) = ui.child_recursive(row, name::COST_ICON) {
            if sale.cost_icon != 0 {
                ui.add_image_to_state(c, sale.cost_icon, 0, state::STATELESS);
                ui.set_center_image(c, true);
                let tip = write(ui, &sale.cost_description);
                ui.set_tooltip(c, &tip);
            }
        }
        if let Some(q) = ui.child_recursive(row, name::COST_QUANTITY) {
            let s = entry(
                ui,
                "ID_Store_Quantity",
                vec![(name::QUANTITY, quantity_var(sale.cost_quantity))],
            );
            let t = write(ui, &s);
            ui.set_text(q, &t);
        }
        if let Some(q) = ui.child_recursive(row, name::STOCK_QUANTITY) {
            let gone = |ui: &mut Ui| {
                ui.set_state(q, name::UNAVAILABLE);
                ui.set_attr(row, name::UNAVAILABLE, Value::Bool(true));
            };
            let s = if sale.state == sale_state::RESTRICTED {
                gone(ui);
                entry(ui, "ID_Store_Restricted", Vec::new())
            } else if sale.stock == 0 {
                gone(ui);
                entry(ui, "ID_Store_SoldOut", Vec::new())
            } else if sale.stock < 0 {
                entry(ui, "ID_Store_Unlimited", Vec::new())
            } else {
                entry(
                    ui,
                    "ID_Store_Quantity",
                    vec![(name::QUANTITY, Var::Int(sale.stock))],
                )
            };
            let t = write(ui, &s);
            ui.set_text(q, &t);
        }
        ui.set_attr(row, name::ITEM_ID, Value::InstanceId(sale.item));
        if let (Some(l), Some(id)) = (ui.frame_listener(self.frame), ui.node(row).map(|n| n.id)) {
            ui.listen_for_message(l, id, msg::MOUSE_PRESS);
        }
        self.sales.insert(sale.item, row);
    }

    /// The sales shown again: the page buttons; the list emptied and a row put up per sale (the
    /// one that was selected selected again); the purchase button ghosted with none selected.
    // D2-02306
    fn update_sales(&mut self, ui: &mut Ui, store: &Store, out: &mut Vec<Request>) {
        self.update_position(ui, store.position, store.size);
        let Some(list) = self
            .field
            .and_then(|f| ui.child_recursive(f, name::ITEM_LIST))
        else {
            return;
        };
        let mut was = NO_OBJECT;
        if let Some(s) = self.selected {
            was = ui
                .node(s)
                .and_then(|n| n.instance_id_attr(name::ITEM_ID))
                .unwrap_or(NO_OBJECT);
            self.select(ui, None, out);
        }
        ui.list_flush(list);
        self.sales.clear();
        for sale in &store.sales {
            let Some(row) = ui.frame_create_element(PLAY_LAYOUT_ENUM, name::SALE) else {
                continue;
            };
            self.post_sale(ui, sale, row);
            ui.list_multi_add(list, row);
            if sale.item == was {
                self.select(ui, Some(row), out);
            }
        }
        ui.list_changes_done(list);
        if self.selected.is_none() {
            if let Some(b) = self
                .field
                .and_then(|f| ui.child_recursive(f, name::PURCHASE))
            {
                ui.set_attr(b, attr::GHOSTED, Value::Bool(true));
            }
        }
    }

    /// A shop shown: the window shown; its name, description and keeper's portrait; the sort
    /// name and field shown when the shop says what its sales are sorted by; the sales.
    // D2-02309
    fn view_store(&mut self, ui: &mut Ui, store: &Store, out: &mut Vec<Request>) {
        self.show(ui, true, out);
        let Some(f) = self.field else { return };
        if let Some(t) = ui.child_recursive(f, name::SHOP_NAME) {
            let s = write(ui, &store.name);
            ui.set_text(t, &s);
        }
        if let Some(t) = ui.child_recursive(f, name::SHOP_DESC) {
            let s = write(ui, &store.description);
            ui.set_text(t, &s);
        }
        if let Some(p) = ui.child_recursive(f, name::PORTRAIT) {
            ui.add_image_to_state(p, store.portrait, 0, state::STATELESS);
            ui.set_center_image(p, true);
        }
        if let (Some(n), Some(t)) = (self.sort_name, self.sort_text) {
            match ui.assets().write(&store.sorter) {
                Some(s) => {
                    ui.set_text(n, &s);
                    ui.set_visible(n, true);
                    ui.clear_all_text(t);
                    ui.set_visible(t, true);
                }
                None => {
                    ui.set_visible(n, false);
                    ui.set_visible(t, false);
                }
            }
        }
        self.update_sales(ui, store, out);
    }

    /// How many of a sale are left: sold out, the row unavailable.
    // D2-02296
    fn update_sale(&self, ui: &mut Ui, item: u64, stock: i32) {
        if !self.shown(ui) {
            return;
        }
        let Some(row) = self.sales.get(&item).copied() else {
            return;
        };
        let Some(q) = ui.child_recursive(row, name::STOCK_QUANTITY) else {
            return;
        };
        let s = if stock < 1 {
            let sold_out = entry(ui, "ID_Store_SoldOut", Vec::new());
            ui.set_state(q, name::UNAVAILABLE);
            ui.set_attr(row, name::UNAVAILABLE, Value::Bool(true));
            entry(
                ui,
                "ID_Store_Quantity",
                vec![(name::QUANTITY, Var::Text(Box::new(sold_out)))],
            )
        } else {
            entry(
                ui,
                "ID_Store_Quantity",
                vec![(name::QUANTITY, Var::Int(stock))],
            )
        };
        let t = write(ui, &s);
        ui.set_text(q, &t);
    }

    /// The selection changed to something not the sale selected: no sale selected.
    // D2-02292
    fn selection_changed(&mut self, ui: &mut Ui, selection: u64, out: &mut Vec<Request>) {
        if !self.shown(ui) {
            return;
        }
        let Some(s) = self.selected else { return };
        let item = ui
            .node(s)
            .and_then(|n| n.instance_id_attr(name::ITEM_ID))
            .unwrap_or(NO_OBJECT);
        if item != selection {
            self.select(ui, None, out);
        }
    }
}

impl Shop {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[
        0x486, 0x482, 0x483, 0x48D, 0x484, 0x48F, 0x485, 0x408, 0x493, 0x436, 0x45A,
    ];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for Shop {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Shown (its window made, shown as the framework shows) or hidden (a split or question it
    /// asked for called off; its window hidden and the time kept).
    // D2-02285
    fn show(&mut self, ui: &mut Ui, on: bool, out: &mut Vec<Request>) {
        ui.frame_show(self.frame, on);
        if on {
            if self.create_managed_elements(ui) {
                if let Some(f) = self.field {
                    let shown = ui.frame_shown(self.frame);
                    ui.set_visible(f, shown);
                }
            }
            return;
        }
        if self.stack_context != 0 {
            out.push(Request::Notice(inventory::said(
                inventory::notice::CLOSE_STACK,
                &[self.stack_context],
            )));
            self.stack_context = 0;
        }
        if self.dialog != 0 {
            out.push(Request::DialogDown(self.dialog));
            self.dialog = 0;
        }
        if let Some(f) = self.field {
            ui.set_visible(f, false);
            self.hidden_at = ui.now();
        }
    }

    /// The clock: hidden two minutes, its window let go; shown, the sort field read three seconds
    /// after its last key (the page from that sale on asked for, the buttons waiting a second), the
    /// page buttons lit again once the wait is over. Its buttons, its sort field, a sale pressed
    /// (selected; with the right button, examined).
    // D2-02297, D2-02303
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<Request>) {
        let now = ui.now();
        match *mail {
            Mail::Global {
                what: global::TIME, ..
            } => {
                if !self.shown(ui) {
                    if self.field.is_some() && now - self.hidden_at > KEEP {
                        self.destroy_managed_elements(ui);
                    }
                } else if self.nav_wait == 0.0 {
                    if self.sort_typed && self.sort_time < now {
                        self.nav_wait = now + PAGE_WAIT;
                        self.sort_typed = false;
                        let typed = self.sort_text.and_then(|t| ui.text(t)).unwrap_or_default();
                        let n: i32 = typed.trim().parse().unwrap_or(0);
                        out.push(Request::Shop(ShopRequest::Sales(n.max(0).cast_unsigned())));
                    }
                } else if self.nav_wait < now {
                    self.nav_wait = 0.0;
                    for b in [self.next, self.prev].into_iter().flatten() {
                        let unavailable =
                            ui.node(b).is_some_and(|n| n.bool_attr(name::UNAVAILABLE));
                        if !unavailable {
                            ui.set_attr(b, attr::GHOSTED, Value::Bool(false));
                        }
                    }
                }
            }
            Mail::Element {
                from,
                element,
                what,
                param,
            } => {
                if !self.shown(ui) {
                    return;
                }
                if element == name::CLOSE {
                    out.push(Request::Shop(ShopRequest::LeaveStore));
                    out.push(Request::Notice(inventory::said(notice::LEAVE_STORE, &[])));
                    return;
                }
                if element == name::SORT_TEXT {
                    self.sort_time = now;
                    self.sort_typed = true;
                    if what != text_msg::DEACTIVATED {
                        self.sort_time += SORT_WAIT;
                    }
                }
                if element == name::NEXT || element == name::PREV {
                    out.push(Request::Shop(if element == name::NEXT {
                        ShopRequest::NextSales
                    } else {
                        ShopRequest::PrevSales
                    }));
                    self.nav_wait = now + PAGE_WAIT;
                    if let (Some(n), Some(p)) = (self.next, self.prev) {
                        ui.set_attr(n, attr::GHOSTED, Value::Bool(true));
                        ui.set_attr(p, attr::GHOSTED, Value::Bool(true));
                    }
                    return;
                }
                if element == name::PURCHASE {
                    if let Some(item) = self
                        .selected
                        .and_then(|s| ui.node(s))
                        .and_then(|n| n.instance_id_attr(name::ITEM_ID))
                    {
                        self.purchase_item(ui, item, 0, out);
                    }
                }
                if element == name::HELP {
                    out.push(Request::Inventory(inventory::Request::Help {
                        entry: Token::of("ID_Help_Shop"),
                        table: HELP_TABLE,
                    }));
                    return;
                }
                if element != name::SALE || what != msg::MOUSE_PRESS {
                    return;
                }
                if param != button_id::LEFT && param != button_id::RIGHT {
                    return;
                }
                if self.selected != Some(from) {
                    self.select(ui, Some(from), out);
                }
                if param == button_id::RIGHT {
                    out.push(Request::Notice(inventory::said(
                        inventory::notice::OPEN_EXAMINATION,
                        &[],
                    )));
                }
            }
            _ => {}
        }
    }

    /// The scripts' shop notices; the questions and splits; an item dragged out of the shop into
    /// the pack (bought into that slot).
    // D2-02287, D2-02290, D2-02291, D2-02301, D2-02308, D2-02283
    fn notice(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<Request>) {
        match n.native {
            notice::VIEW_STORE => {
                if let Some(s) = store_of(n) {
                    self.view_store(ui, &s, out);
                }
            }
            notice::UPDATE_STORE => {
                if let Some(s) = store_of(n).filter(|_| self.shown(ui)) {
                    self.update_sales(ui, &s, out);
                }
            }
            notice::UPDATE_SALE => {
                self.update_sale(ui, n.long(0), n.word_at(2).cast_signed());
            }
            notice::UPDATE_POSITION if self.shown(ui) => {
                self.update_position(ui, n.word_at(0).cast_signed(), n.word_at(1).cast_signed());
            }
            notice::LEAVE_STORE | notice::LEAVE_CATALOG => self.show(ui, false, out),
            notice::CONFIRM_PURCHASE => self.confirm_purchase(ui, n, out),
            notice::SELECTION_CHANGED => self.selection_changed(ui, n.long(0), out),
            crate::dialog::notice::CLOSE_CONFIRMATION_DIALOG => {
                if let Some((id, yes)) = crate::dialog::confirmation_closed(n) {
                    if self.shown(ui) && id == self.dialog {
                        self.dialog = 0;
                        out.push(Request::Shop(ShopRequest::Confirm(yes)));
                    }
                }
            }
            inventory::notice::STACK_DONE => {
                if self.shown(ui) && n.word_at(0) == self.stack_context {
                    self.finish_drag_from_shop(n.word_at(1), out);
                }
            }
            inventory::notice::DRAG_FROM => {
                let (item, slot, context) = (n.long(0), n.word_at(2), n.word_at(3));
                if self.shown(ui) && context == self.context && self.sales.contains_key(&item) {
                    self.purchase_item(ui, item, slot, out);
                }
            }
            _ => {}
        }
    }

    /// What the scripts answer of the purchases.
    fn update(&mut self, _ui: &mut Ui, view: &View<'_>, _out: &mut Vec<Request>) {
        if let Some(f) = view.shop {
            if f.version != self.facts.version {
                self.facts = f.clone();
            }
        }
    }

    /// It goes: its window gone (a question it asked for with it).
    // D2-02300
    fn close(mut self: Box<Self>, ui: &mut Ui) {
        self.destroy_managed_elements(ui);
        ui.drop_frame(self.frame);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
