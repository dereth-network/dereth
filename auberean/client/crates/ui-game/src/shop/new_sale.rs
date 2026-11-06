//! A new consignment: an item from the pack dropped on the window (how many asked when it is a
//! stack), its icon and its name for that many shown, the price typed. Put up: with no item, no
//! amount or no price the player is told what is missing; else a question says what putting it
//! up costs (the price times the amount times the consignment charge, at least one coin), and
//! yes puts it up and closes the window. While an item is dragged over the window it shows
//! selected.

use std::any::Any;

use auberean_text::{StringInfo, Value as Var};
use auberean_ui::framework::FrameId;
use auberean_ui::manager::global;
use auberean_ui::token::{msg, state};
use auberean_ui::types::text::number_input_filter;
use auberean_ui::{Handle, Mail, Token, Ui};

use super::{entry, notice, write, Facts, ShopRequest, HELP_TABLE};
use crate::gameplay::{self, Child, Notice, Request, View, PLAY_LAYOUT_ENUM};
use crate::inventory::{self, NO_OBJECT};

mod name {
    use auberean_ui::Token;
    pub const FIELD: Token = Token::of("NewSaleField");
    pub const NEW_SALE: Token = Token::of("NewSale");
    pub const PRICE: Token = Token::of("NewSalePrice");
    pub const ICON: Token = Token::of("NewSaleIcon");
    pub const DESC: Token = Token::of("NewSaleDesc");
    pub const CLOSE: Token = Token::of("NewSaleCloseButton");
    pub const HELP: Token = Token::of("NewSaleHelpButton");
    pub const ADD: Token = Token::of("AddSaleButton");
    pub const SELECTED: Token = Token::of("selected");
    pub const PRODUCT: Token = Token::of("product");
    pub const COST: Token = Token::of("cost");
    pub const PRICE_VAR: Token = Token::of("price");
    pub const STOP_DRAG_ITEM: Token = Token::of("stop_drag_item");
    pub const MOUSE_OVER: Token = Token::of("mouse_over");
}

/// How long a hidden window keeps its elements, in seconds.
const KEEP: f64 = 120.0;

/// Only an item from somewhere may be dropped on the window.
// D2-02357
fn drop_check(ui: &Ui, item: Handle, _on: Handle) -> bool {
    ui.node(item).is_some_and(|n| {
        n.id_attr(inventory::attr::SHORTCUT_TYPE) == Some(inventory::ITEM_SHORTCUT)
            && n.instance_id_attr(inventory::attr::FROM_ID)
                .is_some_and(|f| f != NO_OBJECT)
    })
}

/// The window a new consignment is put up in.
#[derive(Debug)]
pub struct NewSale {
    frame: FrameId,
    field: Option<Handle>,
    new_sale: Option<Handle>,
    price: Option<Handle>,
    /// The item, how many, the price.
    item: u64,
    quantity: u32,
    /// The name for that many has been asked of the scripts and is not known yet.
    awaiting_name: bool,
    chosen_price: u32,
    stack_context: u32,
    dialog: u32,
    hidden_at: f64,
    facts: Option<inventory::Facts>,
    shop: Facts,
}

impl NewSale {
    /// Its framework, its buttons and the drops on it heard; hidden.
    // D2-02363
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        if let Some(l) = ui.frame_listener(frame) {
            for b in [name::CLOSE, name::HELP, name::ADD] {
                ui.listen_for_message(l, b, msg::BUTTON_RELEASE_ON);
            }
            ui.listen_for_message(l, name::NEW_SALE, name::MOUSE_OVER);
            ui.listen_for_message(l, name::NEW_SALE, name::STOP_DRAG_ITEM);
        }
        let mut me = Self {
            frame,
            field: None,
            new_sale: None,
            price: None,
            item: NO_OBJECT,
            quantity: 0,
            awaiting_name: false,
            chosen_price: 0,
            stack_context: 0,
            dialog: 0,
            hidden_at: 0.0,
            facts: None,
            shop: Facts::default(),
        };
        me.show(ui, false, &mut Vec::new());
        me
    }

    /// The item and how many of it, the question up (0: none).
    #[must_use]
    pub const fn chosen(&self) -> (u64, u32, u32) {
        (self.item, self.quantity, self.dialog)
    }

    /// Its window made when first shown: the drop target, the price field (numbers only); the
    /// clock heard.
    // D2-02359
    fn create_managed_elements(&mut self, ui: &mut Ui) -> bool {
        if self.field.is_some() {
            return true;
        }
        self.field = ui.frame_create_root(self.frame, PLAY_LAYOUT_ENUM, name::FIELD);
        let Some(f) = self.field else { return false };
        self.new_sale = ui.child_recursive(f, name::NEW_SALE);
        if let Some(n) = self.new_sale {
            ui.set_drag_checks(n, None, Some(drop_check));
        }
        self.price = ui.child_recursive(f, name::PRICE);
        if let Some(p) = self.price {
            ui.set_input_filter(p, Some(number_input_filter));
        }
        if let Some(l) = ui.frame_listener(self.frame) {
            ui.listen_for_global_message(l, global::TIME);
        }
        self.new_sale.is_some() && self.price.is_some()
    }

    /// Its window let go.
    // D2-02352
    fn destroy_managed_elements(&mut self, ui: &mut Ui) {
        if let Some(f) = self.field.take() {
            ui.frame_remove_root(self.frame, f);
            ui.remove(f);
            self.new_sale = None;
            self.price = None;
        }
        if let Some(l) = ui.frame_listener(self.frame) {
            ui.unregister_for_all_global_messages(l);
        }
    }

    /// Shown with the item: its icon and its name (none: cleared); the price cleared.
    // D2-02353
    fn update_item(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        if !self.shown(ui) {
            self.show(ui, true, out);
        }
        let Some(f) = self.field else { return };
        let it = self
            .facts
            .as_ref()
            .and_then(|fa| fa.item(self.item))
            .cloned();
        let Some(icon) = ui.child_recursive(f, name::ICON) else {
            return;
        };
        match &it {
            None => ui.clear_image(icon),
            Some(i) if i.icon_layers.is_empty() => ui.set_image(icon, i.icon, 2),
            Some(i) => ui.set_icon_desc(icon, &i.icon_layers),
        }
        let Some(d) = ui.child_recursive(f, name::DESC) else {
            return;
        };
        let name = self
            .facts
            .as_ref()
            .and_then(|fa| fa.name_of_amount(self.item, self.quantity));
        match name {
            Some(n) => ui.set_text(d, n),
            None => ui.clear_all_text(d),
        }
        self.awaiting_name = it.is_some() && name.is_none();
        if self.awaiting_name {
            out.push(Request::Inventory(inventory::Request::NameAmount {
                item: self.item,
                quantity: self.quantity,
            }));
        }
        if let Some(p) = self.price {
            ui.clear_all_text(p);
        }
    }

    /// The name for the amount has come: the window shows it.
    fn name_came(&mut self, ui: &mut Ui) {
        if !self.awaiting_name {
            return;
        }
        let Some(name) = self
            .facts
            .as_ref()
            .and_then(|f| f.name_of_amount(self.item, self.quantity))
        else {
            return;
        };
        self.awaiting_name = false;
        if let Some(d) = self.field.and_then(|f| ui.child_recursive(f, name::DESC)) {
            ui.set_text(d, name);
        }
    }

    /// An item dropped on the window: how many asked (one: at once).
    // D2-02356
    fn handle_drag_drop(&mut self, ui: &mut Ui, dropped: Handle, out: &mut Vec<Request>) {
        let Some(item) = ui
            .node(dropped)
            .and_then(|n| n.instance_id_attr(inventory::attr::SHORTCUT_DATA))
            .filter(|i| *i != NO_OBJECT)
        else {
            return;
        };
        let Some(quantity) = self
            .facts
            .as_ref()
            .and_then(|f| f.item(item))
            .map(|i| i.quantity)
        else {
            return;
        };
        self.item = item;
        self.stack_context = inventory::next_stack_context();
        if quantity == 1 {
            self.stack_done(ui, self.stack_context, 1, out);
            return;
        }
        let mut ask = inventory::init_stack(
            self.stack_context,
            quantity,
            quantity,
            NO_OBJECT,
            NO_OBJECT,
            NO_OBJECT,
        );
        ask.records = vec![Some(gameplay::Record::Text(entry(
            ui,
            "ID_Store_ConsignmentSellStack",
            Vec::new(),
        )))];
        out.push(Request::Notice(ask));
    }

    /// A split done for the item dropped: the window shows that many.
    // D2-02360
    fn stack_done(&mut self, ui: &mut Ui, context: u32, amount: u32, out: &mut Vec<Request>) {
        if self.shown(ui) && context == self.stack_context && self.item != NO_OBJECT && amount != 0
        {
            self.quantity = amount;
            self.update_item(ui, out);
        }
    }

    /// Put up: what is missing told (no item, no amount, no price); else the question with what it
    /// costs.
    // D2-02365
    fn add_sale(&mut self, ui: &Ui, out: &mut Vec<Request>) {
        let missing = |id: &str| display_dialog(ui, id);
        if self.item == NO_OBJECT {
            out.push(missing("ID_Shop_NewSale_Err_NoItem"));
            return;
        }
        if self.quantity == 0 {
            out.push(missing("ID_Shop_NewSale_Err_NoQuantity"));
            return;
        }
        let Some(p) = self.price else { return };
        let typed = ui.text(p).unwrap_or_default();
        let price = read_long(&typed);
        if price < 1 {
            out.push(missing("ID_Shop_NewSale_Err_NoPrice"));
            return;
        }
        self.chosen_price = price.cast_unsigned();
        let id = if self.quantity == 1 {
            "ID_Store_ConsignmentConfirmSellSingular"
        } else {
            "ID_Store_ConsignmentConfirmSellPlural"
        };
        let Some(it) = self.facts.as_ref().and_then(|f| f.item(self.item)) else {
            return;
        };
        #[allow(clippy::cast_precision_loss)]
        let total = f64::from(self.chosen_price as f32)
            * f64::from(self.quantity as f32)
            * f64::from(self.shop.consignment_charge);
        let cost = dereth_primitives::num::to_i32_f64(total.ceil())
            .cast_unsigned()
            .max(1);
        let s = entry(
            ui,
            id,
            vec![
                (
                    name::PRODUCT,
                    Var::Text(Box::new(StringInfo::literal(
                        self.facts
                            .as_ref()
                            .and_then(|f| f.name_of_amount(self.item, self.quantity))
                            .unwrap_or(&it.name),
                    ))),
                ),
                (name::COST, Var::UInt(cost)),
                (name::PRICE_VAR, Var::UInt(self.chosen_price)),
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
}

/// A message of the window's own in a dialog.
fn display_dialog(ui: &Ui, id: &str) -> Request {
    Request::Dialog(gameplay::Dialog {
        id: crate::dialog::new_context(),
        kind: gameplay::dialog::MESSAGE,
        text: write(ui, &entry(ui, id, Vec::new())),
        modal: false,
        info: None,
    })
}

/// A number typed as the C library reads one (decimal, `0x` hexadecimal, a leading `0` octal),
/// as far as it reads; out of range, the nearest end.
fn read_long(s: &str) -> i32 {
    let t = s.trim_start();
    let (neg, t) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let (radix, digits) = if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        (16, h)
    } else if t.starts_with('0') {
        (8, t)
    } else {
        (10, t)
    };
    let mut v: i64 = 0;
    for c in digits.chars() {
        let Some(d) = c.to_digit(radix) else { break };
        v = (v * i64::from(radix) + i64::from(d)).min(i64::from(i32::MAX) + 1);
    }
    let v = if neg { -v } else { v };
    i32::try_from(v.clamp(i64::from(i32::MIN), i64::from(i32::MAX))).unwrap_or(0)
}

impl NewSale {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[0x48F, 0x492, 0x494, 0x436, 0x45A];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for NewSale {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Shown (its window made) or hidden (a split or question it asked for called off, the time
    /// kept); its window as the framework shows.
    // D2-02362
    fn show(&mut self, ui: &mut Ui, on: bool, out: &mut Vec<Request>) {
        ui.frame_show(self.frame, on);
        if on {
            if !self.create_managed_elements(ui) {
                return;
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
        if let Some(f) = self.field {
            let shown = ui.frame_shown(self.frame);
            ui.set_visible(f, shown);
        }
    }

    /// The clock (hidden two minutes, its window let go); close (hidden, the shop left), help, put
    /// up; an item dragged over the window (selected while it is one) or dropped on it.
    // D2-02354, D2-02366
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
                element,
                what,
                param,
                ..
            } => {
                if !self.shown(ui) {
                    return;
                }
                if element == name::CLOSE {
                    self.show(ui, false, out);
                    out.push(Request::Shop(ShopRequest::LeaveStore));
                    return;
                }
                if element == name::HELP {
                    out.push(Request::Inventory(inventory::Request::Help {
                        entry: Token::of("ID_Help_ShopConsignment"),
                        table: HELP_TABLE,
                    }));
                    return;
                }
                if element == name::ADD {
                    self.add_sale(ui, out);
                }
                if element != name::NEW_SALE {
                    return;
                }
                if what == name::MOUSE_OVER {
                    let Some(n) = self.new_sale else { return };
                    if param == 0 {
                        ui.set_state(n, state::STATELESS);
                    } else {
                        let item_dragged = ui.dragged().and_then(|d| ui.node(d)).is_some_and(|d| {
                            d.id_attr(inventory::attr::SHORTCUT_TYPE)
                                == Some(inventory::ITEM_SHORTCUT)
                        });
                        if item_dragged {
                            ui.set_state(n, name::SELECTED);
                        }
                    }
                } else if what == name::STOP_DRAG_ITEM && param != 0 {
                    if let Some(d) = ui.dropped() {
                        self.handle_drag_drop(ui, d, out);
                    }
                }
            }
            _ => {}
        }
    }

    /// A new consignment asked for (the window shown with the item and the amount); the
    /// consignments or the catalog left; a split done (the amount); the question answered (yes:
    /// put up, the window hidden).
    // D2-02355, D2-02361
    fn notice(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<Request>) {
        match n.native {
            notice::INIT_NEW_SALE => {
                self.item = n.long(0);
                self.quantity = n.word_at(2);
                self.update_item(ui, out);
            }
            notice::LEAVE_CONSIGNMENT | notice::LEAVE_CATALOG => self.show(ui, false, out),
            inventory::notice::STACK_DONE => self.stack_done(ui, n.word_at(0), n.word_at(1), out),
            crate::dialog::notice::CLOSE_CONFIRMATION_DIALOG => {
                let Some((id, yes)) = crate::dialog::confirmation_closed(n) else {
                    return;
                };
                if self.shown(ui) && yes && id == self.dialog {
                    self.dialog = 0;
                    if self.item != NO_OBJECT && self.quantity != 0 && self.chosen_price != 0 {
                        out.push(Request::Shop(ShopRequest::Sell {
                            item: self.item,
                            quantity: self.quantity,
                            price: self.chosen_price,
                        }));
                        self.show(ui, false, out);
                    }
                }
            }
            _ => {}
        }
    }

    /// What the scripts answer of the items and of consignments.
    fn update(&mut self, ui: &mut Ui, view: &View<'_>, _out: &mut Vec<Request>) {
        if let Some(f) = view.inventory {
            if self.facts.as_ref().is_none_or(|o| o.version != f.version) {
                self.facts = Some(f.clone());
                self.name_came(ui);
            }
        }
        if let Some(s) = view.shop {
            if s.version != self.shop.version {
                self.shop = s.clone();
            }
        }
    }

    /// It goes: its window gone (a question it asked for with it).
    // D2-02358, D2-02364
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
