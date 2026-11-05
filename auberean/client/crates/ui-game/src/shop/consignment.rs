//! The player's consignments: what the player put up for sale, each active, expired or sold.
//!
//! The window shows its name and description, the player's figure, and a row per consignment
//! (its item's icon, or the sold mark once sold; its description; its price; its state), then an
//! empty row. A row pressed selects it (its item selected for the player); with the right button
//! the item is examined. Collect (or a row dragged into the pack) collects the consignment's
//! coins, or its item when it did not sell. An item from the pack dropped on the list asks how
//! many: dropped on a consignment of the same kind of thing (not sold), the consignment is
//! restocked after a question that says what it costs; else the new-sale window opens with it.
//! While an item is dragged over the list, the row it would restock shows selected. New opens
//! the new-sale window empty; the page buttons scroll the list a page.

use std::any::Any;
use std::collections::BTreeMap;

use auberean_text::Value as Var;
use auberean_ui::framework::FrameId;
use auberean_ui::manager::global;
use auberean_ui::system::Value;
use auberean_ui::token::{attr, msg, state};
use auberean_ui::types::button_id;
use auberean_ui::{Handle, Mail, Token, Ui};

use super::{
    entry, notice, quantity_var, sale_state, write, Facts, Sale, ShopRequest, Store, HELP_TABLE,
};
use crate::gameplay::{self, Child, Notice, Request, View, PLAY_LAYOUT_ENUM};
use crate::inventory::{self, NO_OBJECT};

mod name {
    use auberean_ui::Token;
    pub const CONSIGNMENT: Token = Token::of("Consignment");
    pub const PORTRAIT: Token = Token::of("Portrait");
    pub const ITEM_LIST: Token = Token::of("ItemList");
    pub const CLOSE: Token = Token::of("ConsignmentCloseButton");
    pub const HELP: Token = Token::of("ConsignmentHelpButton");
    pub const NEXT: Token = Token::of("ConsignmentNextButton");
    pub const PREV: Token = Token::of("ConsignmentPrevButton");
    pub const NEW: Token = Token::of("NewConsignmentButton");
    pub const COLLECT: Token = Token::of("CollectConsignmentButton");
    pub const NAME: Token = Token::of("ConsignmentName");
    pub const DESC: Token = Token::of("ConsignmentDesc");
    pub const SALE: Token = Token::of("Sale");
    pub const PRODUCT_ICON: Token = Token::of("ProductIcon");
    pub const ITEM_ICON: Token = Token::of("ItemIcon");
    pub const PRODUCT_DESC: Token = Token::of("ProductDesc");
    pub const COST_ICON: Token = Token::of("CostIcon");
    pub const COST_QUANTITY: Token = Token::of("CostQuantity");
    pub const STATUS: Token = Token::of("StockQuantity");
    pub const SOLD_ICON: Token = Token::of("sold_icon");
    pub const ITEM_ID: Token = Token::of("ItemID");
    pub const SALE_ID: Token = Token::of("SaleID");
    pub const SOLD: Token = Token::of("sold");
    pub const EXPIRED: Token = Token::of("expired");
    pub const ACTIVE: Token = Token::of("active");
    pub const SELECTED: Token = Token::of("selected");
    pub const QUANTITY: Token = Token::of("quantity");
    pub const PRODUCT: Token = Token::of("product");
    pub const COST: Token = Token::of("cost");
    pub const STOP_DRAG_ITEM: Token = Token::of("stop_drag_item");
    pub const LOCATION_CHANGED: Token = Token::of("location_changed");
    pub const MOUSE_OVER: Token = Token::of("mouse_over");
    pub const RELATIVE_X: Token = Token::of("relative_x_position");
    pub const RELATIVE_Y: Token = Token::of("relative_y_position");
}

/// How long a hidden window keeps its elements, in seconds; how many rows a page shows.
const KEEP: f64 = 120.0;
const PAGE: i32 = 7;

/// Only an item from somewhere may be dropped on the list.
// D2-02325
fn drop_check(ui: &Ui, item: Handle, _list: Handle) -> bool {
    ui.node(item).is_some_and(|n| {
        n.id_attr(inventory::attr::SHORTCUT_TYPE) == Some(inventory::ITEM_SHORTCUT)
            && n.instance_id_attr(inventory::attr::FROM_ID)
                .is_some_and(|f| f != NO_OBJECT)
    })
}

/// The player's consignments.
#[derive(Debug)]
pub struct Consignments {
    frame: FrameId,
    selected: Option<Handle>,
    field: Option<Handle>,
    list: Option<Handle>,
    portrait: Option<Handle>,
    /// Each row by the context its icon is dragged in.
    rows: BTreeMap<u32, Handle>,
    /// The consignments as the scripts last gave them.
    store: Store,
    /// An item dropped on the list, the consignment it restocks (0: a new one), how many.
    dropped: u64,
    restock: u32,
    /// The restock's question waits on the scripts' name for the amount.
    awaiting_name: bool,
    amount: u32,
    stack_context: u32,
    dialog: u32,
    hidden_at: f64,
    facts: Option<inventory::Facts>,
    shop: Facts,
}

impl Consignments {
    /// Its framework, its buttons, its list's drops and its moves heard; hidden.
    // D2-02349
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        if let Some(l) = ui.frame_listener(frame) {
            for b in [
                name::NEXT,
                name::PREV,
                name::CLOSE,
                name::HELP,
                name::NEW,
                name::COLLECT,
            ] {
                ui.listen_for_message(l, b, msg::BUTTON_RELEASE_ON);
            }
            ui.listen_for_message(l, name::CONSIGNMENT, name::LOCATION_CHANGED);
            ui.listen_for_message(l, name::ITEM_LIST, name::STOP_DRAG_ITEM);
        }
        let mut me = Self {
            frame,
            selected: None,
            field: None,
            list: None,
            portrait: None,
            rows: BTreeMap::new(),
            store: Store::default(),
            dropped: NO_OBJECT,
            restock: 0,
            awaiting_name: false,
            amount: 0,
            stack_context: 0,
            dialog: 0,
            hidden_at: 0.0,
            facts: None,
            shop: Facts::default(),
        };
        me.show(ui, false, &mut Vec::new());
        me
    }

    /// The rows' consignments (by ordinal) in order, and the one selected.
    #[must_use]
    pub fn sales(&self, ui: &Ui) -> (Vec<i32>, Option<i32>) {
        let id = |h: Handle| ui.node(h).and_then(|n| n.int_attr(name::SALE_ID));
        let ids = self.list.map_or_else(Vec::new, |l| {
            ui.list_items(l).into_iter().filter_map(id).collect()
        });
        (ids, self.selected.and_then(id))
    }

    /// The question up (0: none).
    #[must_use]
    pub const fn dialog(&self) -> u32 {
        self.dialog
    }

    /// The figure's viewport placed where the window says, from the window's corner.
    // D2-02324
    fn sync_portrait(&self, ui: &mut Ui) {
        let (Some(f), Some(p)) = (self.field, self.portrait) else {
            return;
        };
        let rel = |a| ui.node(p).and_then(|n| n.int_attr(a)).unwrap_or(0);
        let (x, y) = (rel(name::RELATIVE_X), rel(name::RELATIVE_Y));
        let Some(r) = ui.screen_rect(f) else { return };
        ui.move_to(p, x + r.x0, y + r.y0);
    }

    /// The player's figure in its viewport, as the player looks: its head and shoulders (the host
    /// draws it, framed by the figure's height and race).
    // D2-02329
    fn init_portrait(&self, out: &mut Vec<Request>) {
        if let Some(viewport) = self.portrait {
            out.push(Request::Inventory(inventory::Request::ShowDoll {
                viewport,
                portrait: true,
            }));
        }
    }

    /// The page buttons by the list's top row: previous ghosted at the top, next on the last page.
    // D2-02332
    fn update_position(&self, ui: &mut Ui) {
        let (Some(l), Some(f)) = (self.list, self.field) else {
            return;
        };
        let top = ui.list_top_row(l);
        let n = i32::try_from(ui.list_items(l).len()).unwrap_or(i32::MAX);
        if let Some(p) = ui.child_recursive(f, name::PREV) {
            ui.set_attr(p, attr::GHOSTED, Value::Bool(top == 0));
        }
        if let Some(b) = ui.child_recursive(f, name::NEXT) {
            ui.set_attr(b, attr::GHOSTED, Value::Bool(n - PAGE <= top));
        }
    }

    /// Its window, the figure's viewport and the list made when first shown; the clock heard.
    // D2-02334
    fn create_managed_elements(&mut self, ui: &mut Ui, out: &mut Vec<Request>) -> bool {
        if self.field.is_some() {
            return true;
        }
        self.field = ui.frame_create_root(self.frame, PLAY_LAYOUT_ENUM, name::CONSIGNMENT);
        let Some(f) = self.field else { return false };
        self.portrait = ui.frame_create_root(self.frame, PLAY_LAYOUT_ENUM, name::PORTRAIT);
        if self.portrait.is_none() {
            return false;
        }
        self.init_portrait(out);
        self.list = ui.child_recursive(f, name::ITEM_LIST);
        if let Some(l) = self.list {
            ui.set_drag_checks(l, None, Some(drop_check));
        }
        if let Some(l) = ui.frame_listener(self.frame) {
            ui.listen_for_global_message(l, global::TIME);
        }
        self.list.is_some()
    }

    /// Its window let go.
    // D2-02337, D2-02333, D2-02342
    fn destroy_managed_elements(&mut self, ui: &mut Ui) {
        self.rows.clear();
        if let Some(l) = self.list.take() {
            ui.list_flush(l);
        }
        for r in [self.portrait.take(), self.field.take()]
            .into_iter()
            .flatten()
        {
            ui.frame_remove_root(self.frame, r);
            ui.remove(r);
        }
        self.selected = None;
        if let Some(l) = ui.frame_listener(self.frame) {
            ui.unregister_for_all_global_messages(l);
        }
    }

    /// Whether an item would restock a consignment: a stack of more than one of the same kind
    /// of thing as the consignment's, not sold.
    // D2-02328
    fn can_stack_with(&self, sale: u32, item: u64) -> bool {
        let Some(it) = self.facts.as_ref().and_then(|f| f.item(item)) else {
            return false;
        };
        if !it.stackable || it.stack_size <= 1 {
            return false;
        }
        self.store
            .sale(sale)
            .is_some_and(|s| s.state != sale_state::SOLD && it.data_id == s.item_did)
    }

    /// A row selected (one not the window's: none): the one before plain; collect lit while one
    /// is; its item selected for the player.
    // D2-02336
    fn select(&mut self, ui: &mut Ui, row: Option<Handle>, out: &mut Vec<Request>) {
        let row = row.filter(|r| self.rows.values().any(|x| x == r));
        if row != self.selected {
            if let Some(s) = self.selected {
                ui.set_state(s, state::STATELESS);
            }
            if let Some(r) = row {
                ui.set_state(r, name::SELECTED);
            }
            self.selected = row;
        }
        if let Some(c) = self
            .field
            .and_then(|f| ui.child_recursive(f, name::COLLECT))
        {
            ui.set_attr(c, attr::GHOSTED, Value::Bool(self.selected.is_none()));
        }
        if let Some(item) = self
            .selected
            .and_then(|s| ui.node(s))
            .and_then(|n| n.instance_id_attr(name::ITEM_ID))
        {
            out.push(Request::SetSelection(item));
        }
    }

    /// How many asked: a restock asks first what it costs (at least one coin); a new
    /// consignment opens the new-sale window with the item and the amount.
    // D2-02326
    fn stack_done(&mut self, ui: &Ui, amount: u32, out: &mut Vec<Request>) {
        if self.restock == 0 {
            let [hi, lo] = split(self.dropped);
            out.push(Request::Notice(inventory::said(
                notice::INIT_NEW_SALE,
                &[hi, lo, amount],
            )));
            return;
        }
        self.amount = amount;
        if self.facts.as_ref().is_some_and(|f| {
            f.item(self.dropped).is_some() && f.name_of_amount(self.dropped, amount).is_none()
        }) {
            self.awaiting_name = true;
            out.push(Request::Inventory(inventory::Request::NameAmount {
                item: self.dropped,
                quantity: amount,
            }));
            return;
        }
        self.ask_restock(ui, out);
    }

    /// What restocking the amount costs, asked, the item called by its name for that many.
    // D2-02326
    fn ask_restock(&mut self, ui: &Ui, out: &mut Vec<Request>) {
        let amount = self.amount;
        let Some(sale) = self.store.sale(self.restock) else {
            return;
        };
        let Some(f) = self.facts.as_ref() else {
            return;
        };
        let Some(it) = f.item(self.dropped) else {
            return;
        };
        let name = f.name_of_amount(self.dropped, amount).unwrap_or(&it.name);
        #[allow(clippy::cast_precision_loss)]
        let price = f64::from(amount as f32)
            * f64::from(sale.cost_quantity)
            * f64::from(self.shop.consignment_charge);
        let cost = dereth_primitives::num::to_i32_f64(price.ceil())
            .cast_unsigned()
            .max(1);
        let s = entry(
            ui,
            "ID_Store_ConsignmentConfirmRestock",
            vec![
                (
                    name::PRODUCT,
                    Var::Text(Box::new(auberean_text::StringInfo::literal(name))),
                ),
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

    /// An item dropped on the list: not one of the window's own rows; on a consignment it
    /// restocks, a restock, else a new consignment; how many asked (one: at once).
    // D2-02341
    fn handle_list_box_drop(&mut self, ui: &Ui, dropped: Handle, out: &mut Vec<Request>) {
        let Some(n) = ui.node(dropped) else { return };
        if let Some(ctx) = n.int_attr(inventory::attr::SOURCE_CONTEXT) {
            if self.rows.contains_key(&ctx.cast_unsigned()) {
                return;
            }
        }
        let item = n
            .instance_id_attr(inventory::attr::SHORTCUT_DATA)
            .unwrap_or(NO_OBJECT);
        if item == NO_OBJECT {
            return;
        }
        if let Some(row) = self.list.and_then(|l| ui.listbox_item_under_mouse(l)) {
            self.restock = ui
                .node(row)
                .and_then(|r| r.int_attr(name::SALE_ID))
                .unwrap_or(0)
                .cast_unsigned();
        }
        if !self.can_stack_with(self.restock, item) {
            self.restock = 0;
        }
        let Some(it) = self.facts.as_ref().and_then(|f| f.item(item)).cloned() else {
            return;
        };
        let label = if self.restock != 0 {
            "ID_Store_ConsignmentRestockStack"
        } else {
            "ID_Store_ConsignmentSellStack"
        };
        self.dropped = item;
        self.stack_context = inventory::next_stack_context();
        if it.quantity == 1 {
            self.stack_done(ui, 1, out);
            return;
        }
        let mut ask = inventory::init_stack(
            self.stack_context,
            it.quantity,
            it.quantity,
            NO_OBJECT,
            NO_OBJECT,
            NO_OBJECT,
        );
        ask.records = vec![Some(gameplay::Record::Text(entry(ui, label, Vec::new())))];
        out.push(Request::Notice(ask));
    }

    /// A consignment's row: its icon (the sold mark once sold), dragable out of the window in a
    /// context of its own; its description; its price; its state (expired and sold marked on the
    /// row); the row knows its item and consignment, its presses and the drags over it heard.
    // D2-02344
    fn post_sale(&mut self, ui: &mut Ui, sale: &Sale, row: Handle) {
        let ctx = crate::inventory::container::request_context();
        if let Some(p) = ui.child_recursive(row, name::PRODUCT_ICON) {
            if let Some(icon) = ui.child_recursive(p, name::ITEM_ICON) {
                if sale.state == sale_state::SOLD {
                    if let Some(i) = ui.node(p).and_then(|n| n.data_id_attr(name::SOLD_ICON)) {
                        ui.set_image(icon, i, 2);
                    }
                } else if sale.icon_layers.is_empty() {
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
                ui.set_attr(
                    icon,
                    inventory::attr::SOURCE_CONTEXT,
                    Value::Int(ctx.cast_signed()),
                );
                ui.set_attr(icon, attr::DRAGABLE, Value::Bool(true));
            }
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
        if let Some(st) = ui.child_recursive(row, name::STATUS) {
            let s = match sale.state {
                sale_state::EXPIRED => {
                    ui.set_state(st, name::EXPIRED);
                    ui.set_attr(row, name::EXPIRED, Value::Bool(true));
                    entry(ui, "ID_Store_Expired", Vec::new())
                }
                sale_state::SOLD => {
                    ui.set_state(st, name::SOLD);
                    ui.set_attr(row, name::SOLD, Value::Bool(true));
                    entry(ui, "ID_Store_Sold", Vec::new())
                }
                _ => {
                    ui.set_state(st, name::ACTIVE);
                    entry(ui, "ID_Store_Active", Vec::new())
                }
            };
            let t = write(ui, &s);
            ui.set_text(st, &t);
        }
        ui.set_attr(row, name::ITEM_ID, Value::InstanceId(sale.item));
        ui.set_attr(row, name::SALE_ID, Value::Int(sale.ordinal.cast_signed()));
        if let Some(l) = ui.frame_listener(self.frame) {
            ui.listen_for_message(l, name::SALE, msg::MOUSE_PRESS);
            ui.listen_for_message(l, name::SALE, name::MOUSE_OVER);
        }
        self.rows.insert(ctx, row);
    }

    /// The consignments shown again: the list emptied, a row per consignment (the one that was
    /// selected selected again) and an empty row; collect ghosted with none selected; the page
    /// buttons.
    // D2-02348
    fn update_sales(&mut self, ui: &mut Ui, store: &Store, out: &mut Vec<Request>) {
        let Some(list) = self.list else { return };
        let mut was = NO_OBJECT;
        if let Some(s) = self.selected {
            was = ui
                .node(s)
                .and_then(|n| n.instance_id_attr(name::ITEM_ID))
                .unwrap_or(NO_OBJECT);
            self.select(ui, None, out);
        }
        ui.list_flush(list);
        self.rows.clear();
        self.store = store.clone();
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
        if let Some(empty) = ui.frame_create_element(PLAY_LAYOUT_ENUM, name::SALE) {
            ui.list_multi_add(list, empty);
        }
        ui.list_changes_done(list);
        if self.selected.is_none() {
            if let Some(c) = self
                .field
                .and_then(|f| ui.child_recursive(f, name::COLLECT))
            {
                ui.set_attr(c, attr::GHOSTED, Value::Bool(true));
            }
        }
        self.update_position(ui);
    }

    /// The consignments shown: the window shown, its name and description, the rows.
    // D2-02351
    fn view_consignment(&mut self, ui: &mut Ui, store: &Store, out: &mut Vec<Request>) {
        self.show(ui, true, out);
        let Some(f) = self.field else { return };
        if let Some(t) = ui.child_recursive(f, name::NAME) {
            let s = write(ui, &store.name);
            ui.set_text(t, &s);
        }
        if let Some(t) = ui.child_recursive(f, name::DESC) {
            let s = write(ui, &store.description);
            ui.set_text(t, &s);
        }
        self.update_sales(ui, store, out);
    }

    /// A row's consignment collected: its coins when it sold, else its item.
    fn collect(ui: &Ui, row: Handle, out: &mut Vec<Request>) {
        let Some(n) = ui.node(row) else { return };
        let Some(sale) = n.int_attr(name::SALE_ID) else {
            return;
        };
        out.push(Request::Shop(ShopRequest::Collect {
            sale: sale.cast_unsigned(),
            sold: n.bool_attr(name::SOLD),
        }));
    }

    /// An item dragged over a row (`over`) or off it: one it would restock selects the row; else
    /// no row is selected. The window's own rows dragged over it select nothing.
    fn dragged_over(&mut self, ui: &mut Ui, row: Handle, over: bool, out: &mut Vec<Request>) {
        if !over && self.selected != Some(row) {
            return;
        }
        if over && self.selected == Some(row) {
            return;
        }
        let Some(dragged) = ui.dragged() else { return };
        let ctx = ui
            .node(dragged)
            .and_then(|n| n.int_attr(inventory::attr::SOURCE_CONTEXT))
            .unwrap_or(0);
        if self.rows.contains_key(&ctx.cast_unsigned()) {
            return;
        }
        if !over {
            if self.selected != Some(row) {
                return;
            }
        } else {
            let Some(sale) = ui.node(row).and_then(|n| n.int_attr(name::SALE_ID)) else {
                return;
            };
            let Some(item) = ui
                .node(dragged)
                .and_then(|n| n.instance_id_attr(inventory::attr::SHORTCUT_DATA))
            else {
                return;
            };
            if self.can_stack_with(sale.cast_unsigned(), item) {
                self.select(ui, Some(row), out);
                return;
            }
        }
        self.select(ui, None, out);
    }
}

fn split(v: u64) -> [u32; 2] {
    [(v >> 32) as u32, (v & 0xFFFF_FFFF) as u32]
}

impl Consignments {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[
        0x490, 0x491, 0x492, 0x48F, 0x408, 0x1_040C, 0x1_040E, 0x46E, 0x493, 0x436, 0x45A,
    ];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for Consignments {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Shown (its window made, the figure placed) or hidden (a split or question it asked for
    /// called off, the time kept); its window and the figure as the framework shows.
    // D2-02335
    fn show(&mut self, ui: &mut Ui, on: bool, out: &mut Vec<Request>) {
        ui.frame_show(self.frame, on);
        if on {
            if self.create_managed_elements(ui, out) {
                self.sync_portrait(ui);
            }
        } else {
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
            if self.field.is_some() {
                self.hidden_at = ui.now();
            }
        }
        let shown = ui.frame_shown(self.frame);
        for e in [self.field, self.portrait].into_iter().flatten() {
            ui.set_visible(e, shown);
        }
    }

    /// The clock (hidden two minutes, its window let go); the window moved (the figure follows);
    /// a drop on the list; close, the page buttons, new, collect, help; a row pressed (selected;
    /// the right button examines) or dragged over.
    // D2-02340, D2-02345
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<Request>) {
        match *mail {
            Mail::Global {
                what: global::TIME, ..
            } => {
                if !self.shown(ui) && self.field.is_some() && ui.now() - self.hidden_at > KEEP {
                    self.destroy_managed_elements(ui);
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
                if element == name::CONSIGNMENT && what == name::LOCATION_CHANGED {
                    self.sync_portrait(ui);
                } else if element == name::ITEM_LIST && what == name::STOP_DRAG_ITEM {
                    if param != 0 {
                        if let Some(d) = ui.dropped() {
                            self.handle_list_box_drop(ui, d, out);
                        }
                    }
                } else if element == name::CLOSE {
                    out.push(Request::Shop(ShopRequest::LeaveConsignment));
                    out.push(Request::Notice(inventory::said(
                        notice::LEAVE_CONSIGNMENT,
                        &[],
                    )));
                } else if element == name::NEXT || element == name::PREV {
                    if let Some(l) = self.list {
                        ui.list_page(l, element == name::NEXT);
                        self.update_position(ui);
                    }
                } else if element == name::NEW {
                    out.push(Request::Notice(inventory::said(
                        notice::INIT_NEW_SALE,
                        &[0, 0, 0],
                    )));
                } else if element == name::COLLECT {
                    if let Some(s) = self.selected {
                        Self::collect(ui, s, out);
                    }
                } else if element == name::HELP {
                    out.push(Request::Inventory(inventory::Request::Help {
                        entry: Token::of("ID_Help_ShopConsignment"),
                        table: HELP_TABLE,
                    }));
                } else if element == name::SALE {
                    if what == msg::MOUSE_PRESS {
                        if param == button_id::LEFT || param == button_id::RIGHT {
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
                    } else if what == name::MOUSE_OVER {
                        self.dragged_over(ui, from, param != 0, out);
                    }
                }
            }
            _ => {}
        }
    }

    /// The scripts' consignment notices; leaving the catalog; the selection; the questions and
    /// splits; a row dragged into the pack (collected).
    // D2-02327, D2-02338, D2-02339, D2-02350
    fn notice(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<Request>) {
        match n.native {
            notice::VIEW_CONSIGNMENT => {
                if let Some(s) = super::store_of(n) {
                    self.view_consignment(ui, &s, out);
                }
            }
            notice::UPDATE_CONSIGNMENT => {
                if let Some(s) = super::store_of(n).filter(|_| self.shown(ui)) {
                    self.update_sales(ui, &s, out);
                }
            }
            notice::LEAVE_CONSIGNMENT | notice::LEAVE_CATALOG => self.show(ui, false, out),
            notice::SELECTION_CHANGED => {
                if !self.shown(ui) {
                    return;
                }
                let Some(s) = self.selected else { return };
                let item = ui
                    .node(s)
                    .and_then(|x| x.instance_id_attr(name::ITEM_ID))
                    .unwrap_or(NO_OBJECT);
                if item != n.long(0) {
                    self.select(ui, None, out);
                }
            }
            crate::dialog::notice::CLOSE_CONFIRMATION_DIALOG => {
                let Some((id, yes)) = crate::dialog::confirmation_closed(n) else {
                    return;
                };
                if self.shown(ui) && yes && id == self.dialog {
                    self.dialog = 0;
                    if self.restock != 0 && self.dropped != NO_OBJECT && self.amount != 0 {
                        out.push(Request::Shop(ShopRequest::Restock {
                            item: self.dropped,
                            quantity: self.amount,
                            sale: self.restock,
                        }));
                    }
                }
            }
            inventory::notice::STACK_DONE => {
                if self.shown(ui) && n.word_at(0) == self.stack_context {
                    self.stack_done(ui, n.word_at(1), out);
                }
            }
            inventory::notice::DRAG_FROM => {
                if !self.shown(ui) {
                    return;
                }
                if let Some(row) = self.rows.get(&n.word_at(3)).copied() {
                    Self::collect(ui, row, out);
                }
            }
            _ => {}
        }
    }

    /// What the scripts answer of the items and of consignments; while the player is on foot,
    /// a change of the player's look reframes the figure.
    fn update(&mut self, ui: &mut Ui, view: &View<'_>, out: &mut Vec<Request>) {
        if let Some(f) = view.inventory {
            if self.facts.as_ref().is_none_or(|o| o.version != f.version) {
                self.facts = Some(f.clone());
                let came = self
                    .facts
                    .as_ref()
                    .is_some_and(|f| f.name_of_amount(self.dropped, self.amount).is_some());
                if self.awaiting_name && came {
                    self.awaiting_name = false;
                    self.ask_restock(ui, out);
                }
            }
        }
        if let Some(s) = view.shop {
            if s.version != self.shop.version {
                self.shop = s.clone();
            }
        }
    }

    /// It goes: its window and the figure gone (a question it asked for with them).
    // D2-02343
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
