//! An item in a cell: the cell's ground and the item's icon in it.
//!
//! [`ItemCell`] gives the icon what a drop needs to know (it is an item, which one, the
//! container it is in, whether it is worn) and shows the item as the player's scripts answer for
//! it: its icon, its name for its quantity (or a quantity given) as the tooltip; while the client
//! does not know the item yet, "waiting for the server" and the cell's pending look; while a
//! request about it is under way, the pending look too. The icon may be dragged unless the item
//! is pending or the panel says it may not be. It shows selected while it is the player's
//! selection.
//!
//! Pressed on (the left button, twice, or the right), the item becomes the selection; pressed
//! twice it is used; with the right button the examination panel opens. While an item waits to be
//! used on a target, the icon shows the cursor its element names for it.

use auberean_ui::manager::Cursor;
use auberean_ui::system::Value;
use auberean_ui::token::{msg, state};
use auberean_ui::types::button_id;
use auberean_ui::{Handle, ListenerId, Mail, Token, Ui};

use super::{attr, notice, Facts, ItemFacts, Request, ITEM_SHORTCUT, NO_OBJECT};
use crate::gameplay::{self, Notice};

mod name {
    use auberean_ui::Token;
    /// The cell's look while the item is pending, and the icon's while it is selected.
    pub const PENDING: Token = Token::of("pending");
    pub const SELECTED: Token = Token::of("selected");
    /// The tooltip while the client does not know the item.
    pub const WAITING: &str = "ID_Item_TT_WaitingForServer";
}

/// An item in a cell.
#[derive(Debug)]
pub struct ItemCell {
    listener: ListenerId,
    background: Option<Handle>,
    icon: Option<Handle>,
    item: u64,
    selection: u64,
    pending: bool,
    draggable: bool,
    /// A quantity the panel shows instead of the item's own.
    quantity: Option<u32>,
    /// The client did not know the item when it was last shown.
    waiting: bool,
    /// What it last showed of the item (shown again when the scripts' answer changes).
    shown: Option<ItemFacts>,
}

impl ItemCell {
    /// Nothing shown yet; the player's selection taken.
    // D2-03008
    pub fn new(ui: &mut Ui, facts: Option<&Facts>) -> Self {
        Self {
            listener: ui.new_listener(),
            background: None,
            icon: None,
            item: NO_OBJECT,
            selection: facts.map_or(NO_OBJECT, |f| f.selection),
            pending: false,
            draggable: true,
            quantity: None,
            waiting: false,
            shown: None,
        }
    }

    /// Show `item` in the cell `background`, in its child `icon_name`: the icon marked as an item's,
    /// the item's, from `from`, worn or not; its presses heard.
    // D2-03024
    #[allow(clippy::too_many_arguments)]
    pub fn init(
        &mut self,
        ui: &mut Ui,
        background: Handle,
        icon_name: Token,
        item: u64,
        from: u64,
        equipped: bool,
        facts: Option<&Facts>,
    ) {
        self.pending = false;
        self.draggable = true;
        self.quantity = None;
        self.background = Some(background);
        self.icon = ui.child(background, icon_name);
        if let Some(icon) = self.icon {
            ui.set_attr(icon, attr::SHORTCUT_TYPE, Value::Id(ITEM_SHORTCUT));
            ui.set_attr(icon, attr::SHORTCUT_DATA, Value::InstanceId(item));
            ui.set_attr(icon, attr::FROM_ID, Value::InstanceId(from));
            ui.set_attr(icon, attr::EQUIPPED, Value::Bool(equipped));
            ui.listen_for_message(self.listener, icon_name, msg::MOUSE_PRESS_TOP);
            self.set_item(ui, item, facts);
        }
    }

    /// The item shown.
    #[must_use]
    pub const fn item(&self) -> u64 {
        self.item
    }

    /// The icon.
    #[must_use]
    pub const fn icon(&self) -> Option<Handle> {
        self.icon
    }

    /// Whether it shows pending.
    #[must_use]
    pub const fn pending(&self) -> bool {
        self.pending
    }

    /// Show another item.
    // D2-03016
    pub fn set_item(&mut self, ui: &mut Ui, item: u64, facts: Option<&Facts>) {
        self.item = item;
        self.update(ui, facts);
    }

    /// Pending while a request about the item is under way.
    // D2-03017
    pub fn set_pending(&mut self, ui: &mut Ui, pending: bool, facts: Option<&Facts>) {
        self.pending = pending;
        self.update(ui, facts);
    }

    /// Whether the icon may be dragged.
    // D2-03018
    pub fn set_draggable(&mut self, ui: &mut Ui, draggable: bool, facts: Option<&Facts>) {
        self.draggable = draggable;
        self.update(ui, facts);
    }

    /// Show this quantity instead of the item's own.
    // D2-03019
    pub fn set_quantity(&mut self, ui: &mut Ui, quantity: u32, facts: Option<&Facts>) {
        self.quantity = Some(quantity);
        self.update(ui, facts);
    }

    /// Mark the icon as another kind of shortcut.
    // D2-03020
    pub fn set_shortcut_type(&mut self, ui: &mut Ui, kind: u32, facts: Option<&Facts>) {
        if let Some(icon) = self.icon {
            ui.set_attr(icon, attr::SHORTCUT_TYPE, Value::Id(kind));
        }
        self.update(ui, facts);
    }

    /// Show the item as the scripts answer for it: unknown, "waiting for the server" and pending;
    /// known, its name for its quantity and its icon, pending while a request is under way; the
    /// icon dragable unless pending or kept from it; selected while it is the selection.
    // D2-03014
    pub fn update(&mut self, ui: &mut Ui, facts: Option<&Facts>) {
        let (Some(bg), Some(icon)) = (self.background, self.icon) else {
            return;
        };
        let mut pending = self.pending;
        let known = facts.and_then(|f| f.item(self.item));
        let tooltip;
        let look;
        if let Some(it) = known {
            tooltip = match (self.quantity, &it.offered_name) {
                (Some(_), Some(offered)) => offered.clone(),
                _ => it.name.clone(),
            };
            if it.icon_layers.is_empty() {
                ui.set_image(icon, it.icon, 0);
            } else {
                ui.set_icon_desc(icon, &it.icon_layers);
            }
            self.waiting = false;
            look = if pending {
                name::PENDING
            } else {
                state::STATELESS
            };
        } else {
            tooltip = super::write(ui, name::WAITING, &[]);
            pending = true;
            ui.clear_image(icon);
            look = name::PENDING;
            self.waiting = true;
        }
        self.shown = known.cloned();
        ui.set_state(bg, look);
        ui.set_attr(
            icon,
            auberean_ui::token::attr::DRAGABLE,
            Value::Bool(!pending && self.draggable),
        );
        let s = if self.selection == self.item && self.item != NO_OBJECT {
            name::SELECTED
        } else {
            state::STATELESS
        };
        ui.set_state(icon, s);
        ui.set_tooltip(icon, &tooltip);
    }

    /// The scripts' answers changed: shown again when what it shows of its item did (the item
    /// came, changed, or looks different).
    // D2-03021, D2-03022, D2-03023
    pub fn refresh(&mut self, ui: &mut Ui, facts: Option<&Facts>) {
        let now = facts.and_then(|f| f.item(self.item));
        if now != self.shown.as_ref() || (self.waiting && now.is_some()) {
            self.update(ui, facts);
        }
    }

    /// A press on the icon: the item selected; pressed twice, used; with the right button, the
    /// examination panel opened. Not while the client does not know it.
    // D2-03010
    pub fn listen(&mut self, mail: &Mail, out: &mut Vec<gameplay::Request>) {
        let Mail::Element {
            from, what, param, ..
        } = *mail
        else {
            return;
        };
        if Some(from) != self.icon || what != msg::MOUSE_PRESS_TOP || self.waiting {
            return;
        }
        if param != button_id::LEFT && param != button_id::DOUBLE_LEFT && param != button_id::RIGHT
        {
            return;
        }
        out.push(gameplay::Request::Inventory(Request::Select(self.item)));
        if param == button_id::DOUBLE_LEFT {
            out.push(gameplay::Request::UseSelected);
        } else if param == button_id::RIGHT {
            out.push(gameplay::Request::Notice(super::said(
                notice::OPEN_EXAMINATION,
                &[],
            )));
        }
    }

    /// The presses it heard.
    pub fn take(&mut self, ui: &mut Ui, out: &mut Vec<gameplay::Request>) {
        for m in ui.take_mail(self.listener) {
            self.listen(&m, out);
        }
    }

    /// The player's selection changed: the icon selected when it became this item, plain when it
    /// stopped being it.
    // D2-03011
    pub fn selection_changed(&mut self, ui: &mut Ui, selection: u64) {
        let s = if self.item == selection {
            (self.item != self.selection).then_some(name::SELECTED)
        } else {
            (self.item == self.selection).then_some(state::STATELESS)
        };
        if let (Some(s), Some(icon)) = (s, self.icon) {
            ui.set_state(icon, s);
        }
        self.selection = selection;
    }

    /// An item waits to be used on a target: the icon shows the cursor its element names for it
    /// (the one it had kept); no longer: the kept cursor back.
    // D2-03013
    pub fn set_targeting_pointer(&mut self, ui: &mut Ui, on: bool) {
        let Some(icon) = self.icon else { return };
        if !on {
            ui.restore_saved_cursor(icon);
            return;
        }
        let Some(n) = ui.node(icon) else { return };
        let (x, y, c) = (
            n.int_attr(attr::USAGE_CURSOR_X),
            n.int_attr(attr::USAGE_CURSOR_Y),
            n.data_id_attr(attr::USAGE_CURSOR),
        );
        if let (Some(x), Some(y), Some(c)) = (x, y, c) {
            if c != 0 {
                ui.save_cursor(icon);
                ui.set_element_cursor(
                    icon,
                    Cursor {
                        picture: c,
                        hot_x: x,
                        hot_y: y,
                    },
                );
            }
        }
    }

    /// The scripts' notices it listens to: the player's selection, an item waiting for a target.
    pub fn notice(&mut self, ui: &mut Ui, n: &Notice, player: u64) {
        if n.about.is_some_and(|a| a != player) {
            return;
        }
        match n.native {
            notice::SELECTION_CHANGED => self.selection_changed(ui, n.long(0)),
            notice::SET_TARGETING_POINTER => self.set_targeting_pointer(ui, n.word() != 0),
            _ => {}
        }
    }

    /// It goes: its listening, the icon plain again (a drag of it stopped), the cell's look
    /// plain.
    // D2-03009, D2-03015, D2-03012
    pub fn close(self, ui: &mut Ui) {
        ui.drop_listener(self.listener);
        if let Some(icon) = self.icon {
            ui.set_attr(icon, auberean_ui::token::attr::DRAGABLE, Value::Bool(false));
            ui.set_state(icon, state::STATELESS);
            ui.clear_image(icon);
            ui.clear_element_tooltip(icon);
            if let Some(d) = ui.dragged() {
                let dragging = ui
                    .node(d)
                    .and_then(|n| n.instance_id_attr(attr::SHORTCUT_DATA))
                    .is_some_and(|id| id == self.item);
                if dragging {
                    ui.stop_drag_and_drop();
                }
            }
        }
        if let Some(bg) = self.background {
            ui.set_state(bg, state::STATELESS);
        }
    }
}
