//! Menus: a button-like element that opens a popup list of choices under (or over) itself, sized
//! to its rows; choosing a row closes it and the menu says what was chosen; a press anywhere else
//! closes it.

use crate::manager::global;
use crate::system::{Handle, Ui};
use crate::token::{msg, Token};
use crate::types::button_id;

/// What a menu keeps between events.
#[derive(Clone, Debug, Default)]
pub struct MenuState {
    /// The popup, made from the menu's `popup` template, and the list inside it.
    pub popup: Option<Handle>,
    pub list: Option<Handle>,
    pub open: bool,
}

mod name {
    use crate::Token;
    pub const POPUP: Token = Token::of("popup");
    pub const LISTBOX: Token = Token::of("listbox");
    pub const TEXT_ITEM: Token = Token::of("text_item");
    pub const DEFAULT_SELECTED_ITEM: Token = Token::of("default_selected_item");
    pub const SELECTION_DISPLAY: Token = Token::of("selection_display");
    pub const GHOSTED: Token = Token::of("ghosted");
    pub const OPEN_UP: Token = Token::of("open_up");
    /// The menu's states.
    pub const CLOSED: Token = Token::of("closed");
    pub const OPEN: Token = Token::of("open");
    pub const ROLLOVER: Token = Token::of("rollover");
    /// The menu's messages.
    pub const NEW_SELECTION: Token = Token::of("new_selection");
    pub const DISPLAY_SELECTION: Token = Token::of("display_selection");
    pub const OPENED: Token = Token::of("opened");
    pub const CLOSED_MSG: Token = Token::of("closed");
}

impl Ui {
    fn menu(&self, h: Handle) -> MenuState {
        self.node(h)
            .map(|n| n.lists.menu.clone())
            .unwrap_or_default()
    }

    /// A new menu: its popup made from the `popup` template of its layout (not shown), the
    /// pointer's presses and releases heard, its list found in the popup by the `listbox`
    /// attribute and listened to, and the default choice selected.
    // D2-11602, D2-11608, D2-11612
    pub(crate) fn menu_created(&mut self, h: Handle) {
        let Some(n) = self.node(h) else { return };
        let Some(popup_id) = n.id_attr(name::POPUP) else {
            return;
        };
        let layout = self.library().get(n.refdesc).layout;
        let Some(popup) = self.create(layout, Token(popup_id)) else {
            return;
        };
        self.set_visible(popup, false);
        if let Some(n) = self.node_mut(h) {
            n.lists.menu.popup = Some(popup);
        }
        self.register_for_global_message(h, global::MOUSE_UP);
        self.register_for_global_message(h, global::MOUSE_DOWN);
        let list = self
            .node(h)
            .and_then(|n| n.id_attr(name::LISTBOX))
            .and_then(|id| self.child_recursive(popup, Token(id)));
        if let Some(l) = list {
            if let Some(n) = self.node_mut(h) {
                n.lists.menu.list = Some(l);
            }
            let lid = self.node(l).map_or(Token(0), |n| n.id);
            self.register_for_message(h, lid, msg::NEW_SELECTION);
            if let Some(d) = self
                .node(h)
                .and_then(|n| n.id_attr(name::DEFAULT_SELECTED_ITEM))
            {
                self.list_select_by_name(l, Token(d), true);
            }
        }
        self.menu_update_state(h);
    }

    /// Empty the menu's list.
    // D2-11603
    pub fn menu_flush(&mut self, h: Handle) {
        if let Some(l) = self.menu(h).list {
            self.list_flush(l);
        }
    }

    /// The menu's row at `index`.
    // D2-11604
    #[must_use]
    pub fn menu_item(&self, h: Handle, index: usize) -> Option<Handle> {
        self.list_item(self.menu(h).list?, index)
    }

    /// The menu's chosen row.
    // D2-11605
    #[must_use]
    pub fn menu_selected_item(&self, h: Handle) -> Option<Handle> {
        self.list_selected_item(self.menu(h).list?)
    }

    /// Choose the menu's row at `index` (the menu says so).
    // D2-11606
    pub fn menu_select_index(&mut self, h: Handle, index: usize) {
        if let Some(l) = self.menu(h).list {
            self.list_select_index(l, index, true);
        }
    }

    /// Choose a row of the menu (the menu says so).
    // D2-11607
    pub fn menu_select_item(&mut self, h: Handle, item: Option<Handle>) {
        if let Some(l) = self.menu(h).list {
            self.list_select_item(l, item, true);
        }
    }

    /// Add a row of text, made from the menu's `text_item` template, at `index`; returns it.
    // D2-11609
    pub fn menu_insert_text(&mut self, h: Handle, text: &str, index: usize) -> Option<Handle> {
        let n = self.node(h)?;
        let template = n.id_attr(name::TEXT_ITEM)?;
        let layout = self.library().get(n.refdesc).layout;
        let row = self.create(layout, Token(template))?;
        if self
            .node(row)
            .is_none_or(|r| r.kind != crate::token::kind::TEXT)
        {
            self.remove(row);
            return None;
        }
        self.set_text(row, text);
        let Some(l) = self.menu(h).list else {
            self.remove(row);
            return None;
        };
        self.list_insert(l, row, index);
        Some(row)
    }

    /// How many rows the menu has.
    // D2-11610
    #[must_use]
    pub fn menu_len(&self, h: Handle) -> usize {
        self.menu(h).list.map_or(0, |l| self.list_items(l).len())
    }

    /// The chosen row's place.
    // D2-11611
    #[must_use]
    pub fn menu_selected_index(&self, h: Handle) -> Option<usize> {
        self.list_selected_index(self.menu(h).list?)
    }

    /// The list chose a row: the element the menu shows its choice in hears it (told directly,
    /// not said to every listener), and the menu says what was chosen (the row's name, or none).
    // D2-11613
    fn menu_new_selection(&mut self, h: Handle) {
        let Some(l) = self.menu(h).list else { return };
        let index = self
            .list_selected_index(l)
            .map_or(u32::MAX, |i| u32::try_from(i).unwrap_or(u32::MAX));
        if let Some(d) = self
            .node(h)
            .and_then(|n| n.id_attr(name::SELECTION_DISPLAY))
        {
            if let Some(display) = self.child_recursive(h, Token(d)) {
                self.hear(display, h, name::DISPLAY_SELECTION, index);
            }
        }
        let chosen = self
            .list_selected_item(l)
            .and_then(|i| self.node(i))
            .map_or(0, |n| n.id.raw());
        self.broadcast(h, name::NEW_SELECTION, chosen);
    }

    /// The menu's look: ghosted, open, rolled over (while topmost under the pointer), or closed.
    // D2-11614
    fn menu_update_state(&mut self, h: Handle) {
        let Some(n) = self.node(h) else { return };
        let s = if n.bool_attr(name::GHOSTED) {
            crate::token::state::GHOSTED
        } else if n.lists.menu.open {
            name::OPEN
        } else if n.over_top {
            name::ROLLOVER
        } else {
            name::CLOSED
        };
        self.set_state(h, s);
        self.flush_pending();
    }

    // D2-11615
    pub(crate) fn menu_mouse_over_top(&mut self, h: Handle, on: bool, handled: &mut bool) {
        self.element_mouse_over_top(h, on, handled);
        self.menu_update_state(h);
    }

    /// What the menu's list said: a new choice.
    // D2-11616
    pub(crate) fn menu_hear(&mut self, h: Handle, from: Handle, what: Token) {
        if what == msg::NEW_SELECTION && self.menu(h).list == Some(from) {
            self.menu_new_selection(h);
        }
    }

    /// Open the popup: shown as a window of its own, as wide as its widest row and as tall as its
    /// rows (with the popup's own border around the list), centered under the menu (or over it,
    /// when the menu opens up); the menu says it opened.
    // D2-11617
    pub fn menu_open(&mut self, h: Handle) {
        let m = self.menu(h);
        let (Some(popup), Some(list), false) = (m.popup, m.list, m.open) else {
            return;
        };
        if !self.roots().contains(&popup) {
            self.add_root(popup);
        }
        self.set_visible(popup, true);
        let (Some(pr), Some(lr)) = (
            self.node(popup).map(|n| n.rect),
            self.node(list).map(|n| n.rect),
        ) else {
            return;
        };
        let extra_w = (pr.width() - lr.width()).max(0);
        let extra_h = (pr.height() - lr.height()).max(0);
        let (mut tall, mut wide) = (0, 0);
        for i in self.list_items(list) {
            if let Some(n) = self.node(i) {
                tall += n.rect.height();
                wide = wide.max(n.rect.width());
            }
        }
        self.resize_to(popup, wide + extra_w, tall + extra_h);
        let Some(mr) = self.screen_rect(h) else {
            return;
        };
        let pw = self.node(popup).map_or(0, |n| n.rect.width());
        let ph = self.node(popup).map_or(0, |n| n.rect.height());
        let open_up = self.node(h).is_some_and(|n| n.bool_attr(name::OPEN_UP));
        let y = if open_up {
            mr.y0 - ph
        } else {
            mr.y0 + mr.height() - 1
        };
        let x = mr.x0 + (mr.width() - pw) / 2;
        self.move_to(popup, x, y);
        self.place_children(popup);
        if let Some(n) = self.node_mut(h) {
            n.lists.menu.open = true;
        }
        self.menu_update_state(h);
        self.broadcast(h, name::OPENED, 0);
    }

    /// Close the popup; the menu says it closed.
    // D2-11618
    pub fn menu_close(&mut self, h: Handle) {
        let m = self.menu(h);
        let (Some(popup), true) = (m.popup, m.open) else {
            return;
        };
        self.set_visible(popup, false);
        if let Some(n) = self.node_mut(h) {
            n.lists.menu.open = false;
        }
        self.menu_update_state(h);
        self.broadcast(h, name::CLOSED_MSG, 0);
    }

    /// Hidden, the menu closes.
    // D2-11620
    pub(crate) fn menu_visibility_changed(&mut self, h: Handle) {
        if !self.is_visible(h) && self.menu(h).open {
            self.menu_close(h);
        }
    }

    /// Ghosted, the menu closes; its look follows.
    // D2-11621
    pub(crate) fn menu_bool_changed(&mut self, h: Handle, a: Token, v: bool) {
        if a == name::GHOSTED {
            if v && self.menu(h).open {
                self.menu_close(h);
            }
            self.menu_update_state(h);
        }
        if a == crate::token::attr::HIDE {
            self.menu_visibility_changed(h);
        }
    }

    /// The left button on an unghosted menu opens it, or closes it when open; the press stops
    /// here.
    // D2-11622
    pub(crate) fn menu_mouse_down(
        &mut self,
        h: Handle,
        x: i32,
        y: i32,
        b: u32,
        handled: &mut bool,
    ) {
        self.element_mouse_down(h, x, y, b, handled);
        let Some(n) = self.node(h) else { return };
        if n.moving || n.resizing || n.bool_attr(name::GHOSTED) || b != button_id::LEFT {
            return;
        }
        if n.lists.menu.open {
            self.menu_close(h);
        } else {
            self.menu_open(h);
        }
        *handled = true;
    }

    /// The pointer's buttons anywhere: while open, a press off the menu and its popup closes it;
    /// a release off the menu closes it unless it is on the popup off its list.
    // D2-11623
    pub(crate) fn menu_listen(&mut self, h: Handle, what: u32, param: u32) {
        self.element_listen(h, what, param);
        let m = self.menu(h);
        let (true, Some(popup), Some(list)) = (m.open, m.popup, m.list) else {
            return;
        };
        let over_menu = self.node(h).is_some_and(|n| n.over);
        let over_popup = self.node(popup).is_some_and(|n| n.over);
        let over_list = self.node(list).is_some_and(|n| n.over);
        if what == global::MOUSE_DOWN {
            if over_menu || over_popup {
                return;
            }
        } else if what == global::MOUSE_UP {
            if over_menu {
                return;
            }
            if over_popup && !over_list {
                return;
            }
        } else {
            return;
        }
        self.menu_close(h);
    }

    /// The menu going away takes its popup with it.
    // D2-11619
    pub(crate) fn menu_deleting(&mut self, h: Handle) {
        if let Some(p) = self.menu(h).popup {
            self.remove(p);
        }
    }
}
