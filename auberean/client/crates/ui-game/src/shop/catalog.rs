//! A catalog of shops: its header and a row per shop (its icon with its description as the tip,
//! its name, how many things it sells). A row pressed selects its shop; pressed twice, or the
//! enter button, enters it. The consignments button shows the player's own consignments.

use std::any::Any;

use auberean_ui::framework::FrameId;
use auberean_ui::manager::global;
use auberean_ui::system::Value;
use auberean_ui::token::{attr, msg, state};
use auberean_ui::types::button_id;
use auberean_ui::{Handle, Mail, Token, Ui};

use super::{notice, write, ShopRequest, Store, HELP_TABLE};
use crate::gameplay::{Child, Notice, Record, Request, PLAY_LAYOUT_ENUM};
use crate::inventory;

mod name {
    use auberean_ui::Token;
    pub const CATALOG: Token = Token::of("Catalog");
    pub const ENTER_STORE: Token = Token::of("EnterStoreButton");
    pub const CONSIGNMENT: Token = Token::of("ConsignmentButton");
    pub const CLOSE: Token = Token::of("CatalogCloseButton");
    pub const HELP: Token = Token::of("CatalogHelpButton");
    pub const STORE_HEADER: Token = Token::of("StoreHeader");
    pub const STORE_LIST: Token = Token::of("StoreList");
    pub const STORE_ELEMENT: Token = Token::of("StoreElement");
    pub const STORE_ICON: Token = Token::of("StoreIcon");
    pub const STORE_NAME: Token = Token::of("StoreName");
    pub const STORE_SIZE: Token = Token::of("StoreSize");
    pub const SELECTION_FIELD: Token = Token::of("SelectionField");
    pub const STORE_ID: Token = Token::of("StoreID");
    pub const SELECTED: Token = Token::of("selected");
}

/// How long a hidden catalog keeps its window, in seconds.
const KEEP: f64 = 120.0;

/// A catalog of shops.
#[derive(Debug)]
pub struct Catalog {
    frame: FrameId,
    /// The shop row selected (its selection field).
    selected: Option<Handle>,
    root: Option<Handle>,
    /// When it was hidden.
    hidden_at: f64,
}

impl Catalog {
    /// Its framework, its buttons heard; hidden.
    // D2-02315
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        if let Some(l) = ui.frame_listener(frame) {
            for b in [
                name::ENTER_STORE,
                name::CONSIGNMENT,
                name::CLOSE,
                name::HELP,
            ] {
                ui.listen_for_message(l, b, msg::BUTTON_RELEASE_ON);
            }
        }
        let mut me = Self {
            frame,
            selected: None,
            root: None,
            hidden_at: 0.0,
        };
        me.show(ui, false, &mut Vec::new());
        me
    }

    /// The shops listed, by id, and the one selected.
    #[must_use]
    pub fn stores(&self, ui: &Ui) -> (Vec<u32>, Option<u32>) {
        let id = |h: Handle| ui.node(h).and_then(|n| n.data_id_attr(name::STORE_ID));
        let list = self
            .root
            .and_then(|r| ui.child_recursive(r, name::STORE_LIST));
        let ids = list.map_or_else(Vec::new, |l| {
            ui.list_items(l)
                .iter()
                .filter_map(|row| ui.child_recursive(*row, name::SELECTION_FIELD))
                .filter_map(id)
                .collect()
        });
        (ids, self.selected.and_then(id))
    }

    fn enter_button(&self, ui: &Ui) -> Option<Handle> {
        self.root
            .and_then(|r| ui.child_recursive(r, name::ENTER_STORE))
    }

    /// Its window let go.
    // D2-02310
    fn destroy_managed_elements(&mut self, ui: &mut Ui) {
        if let Some(r) = self.root.take() {
            ui.frame_remove_root(self.frame, r);
            ui.remove(r);
        }
        self.selected = None;
        if let Some(l) = ui.frame_listener(self.frame) {
            ui.unregister_for_all_global_messages(l);
        }
    }

    /// A shop's row selected (none: none): the one before plain; the enter button lit while one
    /// is.
    // D2-02312
    fn select(&mut self, ui: &mut Ui, field: Option<Handle>) {
        if !self.shown(ui) {
            return;
        }
        if let Some(s) = self.selected {
            ui.set_state(s, state::STATELESS);
        }
        let Some(b) = self.enter_button(ui) else {
            return;
        };
        if let Some(f) = field {
            ui.set_state(f, name::SELECTED);
        }
        ui.set_attr(b, attr::GHOSTED, Value::Bool(field.is_none()));
        self.selected = field;
    }

    /// The selected shop entered.
    // D2-02314
    fn enter_store(&self, ui: &Ui, out: &mut Vec<Request>) {
        if let Some(id) = self
            .selected
            .and_then(|s| ui.node(s))
            .and_then(|n| n.data_id_attr(name::STORE_ID))
        {
            out.push(Request::Shop(ShopRequest::EnterShop(id)));
        }
    }

    /// The catalog left: no shop selected, hidden, its list emptied.
    // D2-02317
    fn leave_catalog(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        if !self.shown(ui) {
            return;
        }
        self.select(ui, None);
        self.show(ui, false, out);
        if let Some(l) = self
            .root
            .and_then(|r| ui.child_recursive(r, name::STORE_LIST))
        {
            ui.list_flush(l);
        }
    }

    /// A shop's row: its icon (its description the tip), its name, how many things it sells; its
    /// selection field knows the shop, its presses heard.
    // D2-02320
    fn post_shop(&self, ui: &mut Ui, shop: &Store, row: Handle) {
        if let Some(i) = ui.child_recursive(row, name::STORE_ICON) {
            if shop.portrait != 0 {
                ui.set_image(i, shop.portrait, 2);
                ui.set_attr(i, attr::SIZE_IMAGE_TO_OWNER, Value::Bool(true));
                let tip = write(ui, &shop.description);
                ui.set_tooltip(i, &tip);
            }
        }
        if let Some(n) = ui.child_recursive(row, name::STORE_NAME) {
            let t = write(ui, &shop.name);
            ui.set_text(n, &t);
        }
        if shop.size > 0 {
            if let Some(s) = ui.child_recursive(row, name::STORE_SIZE) {
                ui.set_text(s, &shop.size.to_string());
            }
        }
        if let Some(f) = ui.child_recursive(row, name::SELECTION_FIELD) {
            ui.set_attr(f, name::STORE_ID, Value::DataId(shop.id));
            if let Some(l) = ui.frame_listener(self.frame) {
                ui.listen_for_message(l, name::SELECTION_FIELD, msg::MOUSE_PRESS);
            }
        }
    }

    /// The catalog shown: its header; its shops listed (the list at the row it was at, the shop
    /// that was selected selected again); the enter button lit while one is.
    // D2-02321
    fn view_catalog(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<Request>) {
        self.show(ui, true, out);
        let Some(r) = self.root else { return };
        let Some(h) = ui.child_recursive(r, name::STORE_HEADER) else {
            return;
        };
        if let Some(t) = super::texts_of(n).first() {
            let s = write(ui, t);
            ui.set_text(h, &s);
        }
        let was = self
            .selected
            .and_then(|s| ui.node(s))
            .and_then(|nd| nd.data_id_attr(name::STORE_ID));
        if was.is_some() {
            self.select(ui, None);
        }
        let Some(list) = ui.child_recursive(r, name::STORE_LIST) else {
            return;
        };
        let top = ui.list_top_row(list);
        ui.list_flush(list);
        let shops = n
            .records
            .iter()
            .flatten()
            .find_map(|x| match x {
                Record::Stores(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_default();
        for shop in &shops {
            let Some(row) = ui.frame_create_element(PLAY_LAYOUT_ENUM, name::STORE_ELEMENT) else {
                continue;
            };
            self.post_shop(ui, shop, row);
            ui.list_multi_add(list, row);
            if Some(shop.id) == was {
                let f = ui.child_recursive(row, name::SELECTION_FIELD);
                if f.is_some() {
                    self.select(ui, f);
                }
            }
        }
        ui.list_changes_done(list);
        ui.list_scroll_to_row(list, top, true);
        if let Some(b) = self.enter_button(ui) {
            ui.set_attr(b, attr::GHOSTED, Value::Bool(self.selected.is_none()));
        }
    }
}

impl Child for Catalog {
    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Shown (its window made the first time, the clock heard) or hidden (the time kept); its
    /// window as the framework shows.
    // D2-02311
    fn show(&mut self, ui: &mut Ui, on: bool, _out: &mut Vec<Request>) {
        ui.frame_show(self.frame, on);
        if !on {
            if self.root.is_some() {
                self.hidden_at = ui.now();
            }
        } else if self.root.is_none() {
            self.root = ui.frame_create_root(self.frame, PLAY_LAYOUT_ENUM, name::CATALOG);
            if self.root.is_none() {
                return;
            }
            if let Some(l) = ui.frame_listener(self.frame) {
                ui.listen_for_global_message(l, global::TIME);
            }
        }
        if let Some(r) = self.root {
            let shown = ui.frame_shown(self.frame);
            ui.set_visible(r, shown);
        }
    }

    /// The clock (hidden two minutes, its window let go); close (the catalog left), enter, the
    /// consignments, help; a shop's row pressed (selected; twice, entered).
    // D2-02313, D2-02318
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<Request>) {
        match *mail {
            Mail::Global {
                what: global::TIME, ..
            } => {
                if !self.shown(ui) && self.root.is_some() && ui.now() - self.hidden_at > KEEP {
                    self.destroy_managed_elements(ui);
                }
            }
            Mail::Element {
                from,
                element,
                param,
                ..
            } => {
                if !self.shown(ui) {
                    return;
                }
                if element == name::CLOSE {
                    out.push(Request::Shop(ShopRequest::LeaveCatalog));
                    out.push(Request::Notice(inventory::said(notice::LEAVE_CATALOG, &[])));
                } else if element == name::ENTER_STORE {
                    self.enter_store(ui, out);
                } else if element == name::CONSIGNMENT {
                    out.push(Request::Shop(ShopRequest::EnterConsignment));
                } else if element == name::HELP {
                    out.push(Request::Inventory(inventory::Request::Help {
                        entry: Token::of("ID_Help_ShopCatalog"),
                        table: HELP_TABLE,
                    }));
                } else if element == name::SELECTION_FIELD
                    && [
                        button_id::LEFT,
                        button_id::RIGHT,
                        button_id::DOUBLE_LEFT,
                        button_id::DOUBLE_RIGHT,
                    ]
                    .contains(&param)
                {
                    self.select(ui, Some(from));
                    if param == button_id::DOUBLE_LEFT || param == button_id::DOUBLE_RIGHT {
                        self.enter_store(ui, out);
                    }
                }
            }
            _ => {}
        }
    }

    /// The scripts' catalog notices.
    fn notice(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<Request>) {
        match n.native {
            notice::VIEW_CATALOG => self.view_catalog(ui, n, out),
            notice::LEAVE_CATALOG => self.leave_catalog(ui, out),
            _ => {}
        }
    }

    /// It goes: its window gone.
    // D2-02316, D2-02319
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
