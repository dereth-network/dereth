//! The manager: what the interface does with the player's input and the clock as a whole.
//!
//! The pointer's moves update which elements it is over and which is topmost (and so the cursor);
//! its buttons reach the elements under it, the topmost root first; the interface keys go to the
//! element that has the keyboard, and to whoever listens for keys; the clock reaches whoever
//! listens for it (a held hot button, an element moving with the pointer). Elements take the
//! keyboard in their activation order. A moving or resizing element and a dragged item follow the
//! pointer.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::listener::{Listener, ListenerId, Mail, Registrations};
use crate::system::{Handle, Ui};
use crate::token::Token;
use crate::types::{button_id, key_id};

/// The messages the manager sends its global listeners.
pub mod global {
    /// A pointer button pressed, and let go (the parameter is the button).
    pub const MOUSE_DOWN: u32 = 0xFFFF_0000;
    pub const MOUSE_UP: u32 = 0xFFFF_0001;
    /// The pointer moved.
    pub const MOUSE_MOVE: u32 = 0xFFFF_0002;
    /// An interface key pressed, and let go (the parameter is the key).
    pub const KEY_DOWN: u32 = 0xFFFF_0003;
    pub const KEY_UP: u32 = 0xFFFF_0004;
    /// The clock moved on.
    pub const TIME: u32 = 0xFFFF_0005;
    /// The display is being set up again (a new size), and has been.
    pub const REFRESH_START: u32 = 0xFFFF_0007;
    pub const REFRESH_DONE: u32 = 0xFFFF_0008;
}

/// The mapping layout enumeration values are named in.
pub const LAYOUT_MAPPING: u32 = 0x4000_0003;

/// A cursor: its picture and the point of it that is the pointer's position.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cursor {
    pub picture: u32,
    pub hot_x: i32,
    pub hot_y: i32,
}

/// The manager's state, dropped with the interface.
// D2-11280
#[derive(Clone, Debug, Default)]
pub(crate) struct Manager {
    /// Where the pointer is.
    pub x: i32,
    pub y: i32,
    /// The topmost element under the pointer.
    pub top: Option<Handle>,
    /// The element whose tooltip would show.
    pub tooltip_owner: Option<Handle>,
    /// When the pointer last moved and last went down, on the clock.
    pub last_move: f64,
    pub last_press: f64,
    /// The element with the keyboard, and its root.
    pub active: Option<Handle>,
    pub active_root: Option<Handle>,
    /// The elements that can take the keyboard, by activation order.
    pub activatables: Vec<Handle>,
    /// Listeners for the manager's global messages, by message.
    pub globals: HashMap<u32, Vec<Listener>>,
    /// Listeners to an element's messages, by the sender's name and message.
    pub listeners: HashMap<(Token, Token), Vec<Listener>>,
    /// The cursor shown, and the default.
    pub cursor: Option<Cursor>,
    pub default_cursor: Option<Cursor>,
    /// The element resizing with the pointer.
    pub resizing: Option<Handle>,
    /// The item being dragged and the element it was dragged from.
    pub drag: Option<Handle>,
    pub drag_owner: Option<Handle>,
    /// The clock.
    pub now: f64,
    /// The pointer moved since the elements under it were last worked out.
    pub moved: bool,
    // The screen flow's part: listeners outside the tree, removals waiting, elements by name,
    // the drag in progress.
    /// Listeners outside the tree: the next number, what each registered for, its mail.
    pub next_listener: u32,
    pub(crate) registrations: HashMap<ListenerId, Registrations>,
    pub mail: HashMap<ListenerId, Vec<Mail>>,
    /// Registrations for a global message beyond the first, by listener and message.
    pub global_counts: HashMap<(Listener, u32), i32>,
    /// Which listeners are registered for which element's message.
    pub element_marks: HashSet<(Listener, Token, Token)>,
    /// Registrations to undo at the next delivery: element messages and global messages.
    pub element_clean: VecDeque<(Listener, Token, Token)>,
    pub global_clean: VecDeque<(Listener, u32)>,
    /// Listeners for a top-level element's own messages, by the element and message, and the
    /// undoings waiting. Nothing is ever delivered to them.
    pub object_listeners: HashMap<(Handle, u32), Vec<Listener>>,
    pub object_clean: VecDeque<(Listener, Handle, u32)>,
    /// Elements by name, oldest first.
    pub by_name: HashMap<Token, Vec<Handle>>,
    /// The use key is down.
    pub use_down: bool,
    /// The left button is down; where it last went down.
    pub left_down: bool,
    pub press_x: i32,
    pub press_y: i32,
    /// Where the dragged item stands from the pointer, and the size it is shown in (the item it
    /// was copied from's).
    pub drag_dx: i32,
    pub drag_dy: i32,
    pub drag_size: (i32, i32),
    /// The copy of the item last dropped, kept (hidden) until the next drag for the listeners
    /// that hear where it went: a listener outside the tree reads its mail after the drop.
    pub dropped: Option<Handle>,
    /// The frameworks (a removed one leaves an empty slot).
    pub(crate) frames: Vec<Option<crate::framework::frame::FrameData>>,
}

impl Ui {
    /// Whether the left button is held.
    #[must_use]
    pub fn left_down(&self) -> bool {
        self.mgr.left_down
    }

    /// The clock as the interface last saw it, in seconds.
    #[must_use]
    pub fn now(&self) -> f64 {
        self.mgr.now
    }

    /// Where the pointer is.
    #[must_use]
    pub fn pointer(&self) -> (i32, i32) {
        (self.mgr.x, self.mgr.y)
    }

    /// The topmost element under the pointer.
    #[must_use]
    pub fn top(&self) -> Option<Handle> {
        self.mgr.top.filter(|h| self.node(*h).is_some())
    }

    /// The topmost element under `(x, y)` that sees the pointer, as a move there would find it.
    #[must_use]
    pub fn hit(&self, x: i32, y: i32) -> Option<Handle> {
        fn walk(ui: &Ui, h: Handle, x: i32, y: i32, inside: bool) -> Option<Handle> {
            let n = ui.node(h)?;
            let contains = inside && n.visible && n.rect.contains(x, y);
            for c in &n.children {
                if let Some(f) = walk(ui, *c, x - n.rect.x0, y - n.rect.y0, contains) {
                    return Some(f);
                }
            }
            (contains && n.mouse_visible).then_some(h)
        }
        self.roots().iter().find_map(|r| walk(self, *r, x, y, true))
    }

    // -----------------------------------------------------------------------------------------
    // Listeners
    // -----------------------------------------------------------------------------------------

    /// Have a listener hear one of the manager's global messages. Registering again counts once
    /// more, and needs its own undoing; registering while an undoing waits keeps the count.
    // D2-11257
    pub fn register_for_global_message(&mut self, l: impl Into<Listener>, what: u32) {
        let who = l.into();
        let list = self.mgr.globals.entry(what).or_default();
        if list.contains(&who) {
            self.increment_message(who, what);
            return;
        }
        list.push(who);
    }

    /// Stop a listener hearing a global message, at the next delivery.
    // D2-11258
    pub fn unregister_for_global_message(&mut self, l: impl Into<Listener>, what: u32) {
        let who = l.into();
        if self.mgr.globals.contains_key(&what) {
            self.mgr.global_clean.push_back((who, what));
        }
    }

    /// Have a listener hear what elements named `element` say (`what`), once however often it
    /// asks; asking while an undoing of the same waits cancels the undoing.
    // D2-11279
    pub fn register_for_message(&mut self, l: impl Into<Listener>, element: Token, what: Token) {
        let who = l.into();
        let list = self.mgr.listeners.entry((element, what)).or_default();
        if list.contains(&who) {
            let pending = self
                .mgr
                .element_clean
                .iter()
                .position(|x| *x == (who, element, what));
            if let Some(i) = pending {
                self.mgr.element_clean.remove(i);
                self.decrement_element_message(who, element, what);
            }
            return;
        }
        self.increment_element_message(who, element, what);
        if let Some(list) = self.mgr.listeners.get_mut(&(element, what)) {
            list.push(who);
        }
    }

    /// Stop a listener hearing an element's message, at the next delivery.
    // D2-11250, D2-11225
    pub fn unregister_for_message(&mut self, l: impl Into<Listener>, element: Token, what: Token) {
        let who = l.into();
        if self
            .mgr
            .listeners
            .get(&(element, what))
            .is_some_and(|list| !list.is_empty())
        {
            self.mgr.element_clean.push_back((who, element, what));
        }
    }

    /// Have a listener hear one of a top-level element's own messages (`what`, as the manager
    /// numbers its global ones), once however often it asks; asking while an undoing of the same
    /// waits cancels the undoing. Only a top-level element on the screen keeps listeners. The
    /// manager never sends a top-level element's messages: the listener hears nothing by it.
    // D2-11252
    pub fn register_for_object_message(&mut self, l: impl Into<Listener>, root: Handle, what: u32) {
        let who = l.into();
        if !self.roots().contains(&root) {
            return;
        }
        let list = self.mgr.object_listeners.entry((root, what)).or_default();
        if list.contains(&who) {
            if let Some(i) = self
                .mgr
                .object_clean
                .iter()
                .position(|x| *x == (who, root, what))
            {
                self.mgr.object_clean.remove(i);
            }
            return;
        }
        list.push(who);
    }

    /// Stop a listener hearing a top-level element's message, at the next delivery.
    // D2-11253
    pub fn unregister_for_object_message(
        &mut self,
        l: impl Into<Listener>,
        root: Handle,
        what: u32,
    ) {
        let who = l.into();
        if self.mgr.object_listeners.contains_key(&(root, what)) {
            self.mgr.object_clean.push_back((who, root, what));
        }
    }

    /// Whether a listener is registered for a top-level element's message.
    #[must_use]
    pub fn is_registered_for_object_message(
        &self,
        l: impl Into<Listener>,
        root: Handle,
        what: u32,
    ) -> bool {
        let who = l.into();
        self.mgr
            .object_listeners
            .get(&(root, what))
            .is_some_and(|list| list.contains(&who))
    }

    /// Undo the registrations for top-level elements' messages waiting.
    fn clean_object_list(&mut self) {
        while let Some((who, root, what)) = self.mgr.object_clean.pop_front() {
            let Some(list) = self.mgr.object_listeners.get_mut(&(root, what)) else {
                continue;
            };
            if let Some(i) = list.iter().position(|x| *x == who) {
                list.swap_remove(i);
                if list.is_empty() {
                    self.mgr.object_listeners.remove(&(root, what));
                }
            }
        }
    }

    /// Undo the global registrations waiting: a listener registered more than once loses one
    /// count; otherwise it leaves the list (the last listener takes its place).
    // D2-11259
    fn clean_global_list(&mut self) {
        while let Some((who, what)) = self.mgr.global_clean.pop_front() {
            if !self.mgr.globals.contains_key(&what) {
                continue;
            }
            if self.extra_registrations(who, what) >= 1 {
                self.decrement_message(who, what);
                continue;
            }
            let Some(list) = self.mgr.globals.get_mut(&what) else {
                continue;
            };
            if let Some(i) = list.iter().position(|x| *x == who) {
                list.swap_remove(i);
                if list.is_empty() {
                    self.mgr.globals.remove(&what);
                }
            }
        }
    }

    /// Undo the element registrations waiting: the mark cleared, the listener out of the list
    /// (the last listener takes its place).
    // D2-11251
    fn clean_element_list(&mut self) {
        while let Some((who, element, what)) = self.mgr.element_clean.pop_front() {
            self.decrement_element_message(who, element, what);
            let Some(list) = self.mgr.listeners.get_mut(&(element, what)) else {
                continue;
            };
            if let Some(i) = list.iter().position(|x| *x == who) {
                list.swap_remove(i);
                if list.is_empty() {
                    self.mgr.listeners.remove(&(element, what));
                }
            }
        }
    }

    /// Every undoing waiting, done.
    // D2-11256
    pub(crate) fn clean_queues(&mut self) {
        self.clean_global_list();
        self.clean_object_list();
        self.clean_element_list();
    }

    /// Forget a listener that is going away: it leaves every list at once, and its waiting
    /// undoings go.
    // D2-11255, D2-11254
    pub(crate) fn delete_listener_from_queues(&mut self, who: Listener) {
        self.mgr.global_clean.retain(|(w, _)| *w != who);
        self.mgr.element_clean.retain(|(w, _, _)| *w != who);
        self.mgr.object_clean.retain(|(w, _, _)| *w != who);
        for l in self.mgr.object_listeners.values_mut() {
            l.retain(|x| *x != who);
        }
        for l in self.mgr.globals.values_mut() {
            l.retain(|x| *x != who);
        }
        for l in self.mgr.listeners.values_mut() {
            l.retain(|x| *x != who);
        }
        self.mgr.global_counts.retain(|(w, _), _| *w != who);
        self.mgr.element_marks.retain(|(w, _, _)| *w != who);
    }

    /// A global message to everyone listening for it, in their order; undoings waiting are done
    /// before and after. A listener added while it goes out hears it too.
    // D2-11273
    pub(crate) fn broadcast_global(&mut self, what: u32, param: u32) {
        self.clean_queues();
        let mut i = 0;
        while let Some(who) = self.mgr.globals.get(&what).and_then(|l| l.get(i)).copied() {
            match who {
                Listener::Element(h) => {
                    if self.node(h).is_some() {
                        self.listen(h, what, param);
                    }
                }
                Listener::Other(l) => self.post(l, Mail::Global { what, param }),
            }
            i += 1;
        }
        self.clean_queues();
    }

    /// What an element said, to the listeners registered for its name and that message, in
    /// their order; undoings waiting are done before and after.
    // D2-11272
    pub(crate) fn deliver(&mut self, from: Handle, element: Token, what: Token, param: u32) {
        self.clean_queues();
        let mut i = 0;
        while let Some(who) = self
            .mgr
            .listeners
            .get(&(element, what))
            .and_then(|l| l.get(i))
            .copied()
        {
            match who {
                Listener::Element(h) => {
                    if self.node(h).is_some() {
                        self.hear(h, from, what, param);
                    }
                }
                Listener::Other(l) => self.post(
                    l,
                    Mail::Element {
                        from,
                        element,
                        what,
                        param,
                    },
                ),
            }
            i += 1;
        }
        self.clean_queues();
    }

    // -----------------------------------------------------------------------------------------
    // Elements by name, roots and the delete queue
    // -----------------------------------------------------------------------------------------

    /// The oldest element named `name` still there.
    // D2-11260
    #[must_use]
    pub fn element(&self, name: Token) -> Option<Handle> {
        self.mgr
            .by_name
            .get(&name)
            .and_then(|l| l.iter().copied().find(|h| self.node(*h).is_some()))
    }

    /// Note a new element by its name.
    pub(crate) fn note_name(&mut self, h: Handle) {
        if let Some(n) = self.node(h) {
            let id = n.id;
            let l = self.mgr.by_name.entry(id).or_default();
            if !l.contains(&h) {
                l.push(h);
            }
        }
    }

    /// Make the top-level element `name` of the layout record `layout` and put it on the screen,
    /// then let it and its kind finish (a panel listening to its pages, a page making its page).
    // D2-11247
    pub fn create_root_element_by_data_id(&mut self, layout: u32, name: Token) -> Option<Handle> {
        let l = self.load_layout(layout)?;
        let h = self.create_root(l, name)?;
        if name.raw() != 0 {
            self.initialized(h);
        }
        Some(h)
    }

    /// Take a root off the screen without removing it.
    // D2-11195
    pub fn remove_root_element(&mut self, h: Handle) {
        if self.node(h).is_some_and(|n| n.parent.is_none()) {
            self.take_root_off_screen(h);
        }
    }

    // -----------------------------------------------------------------------------------------
    // The pointer
    // -----------------------------------------------------------------------------------------

    /// The pointer moved to `(x, y)`: whoever listens for the move hears it (an element moving or
    /// resizing with the pointer); a press becomes a drag of what it was on, however far this move
    /// went; then the elements under it are worked out again. Returns whether the pointer is over
    /// the interface.
    // D2-11206
    pub fn pointer_move(&mut self, x: i32, y: i32) -> bool {
        self.mgr.x = x;
        self.mgr.y = y;
        self.broadcast_global(global::MOUSE_MOVE, 0);
        self.drag_follows_pointer();
        self.mgr.moved = true;
        self.check_drag_and_drop();
        self.update_mouse();
        self.top().is_some()
    }

    /// Which elements the pointer is over: each root from the nearest, until one takes the move;
    /// the topmost element under it sets the cursor (else the default cursor shows); a new
    /// tooltip owner drops the tooltip shown.
    // D2-11207
    pub fn update_mouse(&mut self) {
        self.mgr.moved = false;
        self.mgr.last_move = self.mgr.now;
        self.mgr.top = None;
        let (x, y) = (self.mgr.x, self.mgr.y);
        let mut top = None;
        let mut tip = None;
        let mut handled = false;
        for r in self.roots().to_vec() {
            if handled {
                break;
            }
            self.notify_mouse_move(r, x, y, true, &mut top, &mut tip, &mut handled);
        }
        self.mgr.top = top;
        let top_has_cursor = top.is_some_and(|t| self.has_cursor(t));
        if !top_has_cursor {
            self.use_default_cursor();
        }
        if self.mgr.tooltip_owner != tip {
            self.mgr.tooltip_owner = tip;
            self.clear_tooltip();
        }
    }

    /// The left button went down; returns whether the interface took the press.
    pub fn pointer_press(&mut self) -> bool {
        self.pointer_button(button_id::LEFT, true)
    }

    /// The left button came up; returns whether the interface had taken the press.
    pub fn pointer_release(&mut self) -> bool {
        self.pointer_button(button_id::LEFT, false)
    }

    /// A pointer button pressed or let go (a wheel step is a press alone). Returns whether the
    /// pointer is over the interface, or the interface held the button.
    // D2-11221, D2-11227
    pub fn pointer_button(&mut self, button: u32, down: bool) -> bool {
        if down {
            let over = self.top().is_some();
            self.mouse_down_event(button);
            return over;
        }
        let wheel = button == button_id::WHEEL_UP || button == button_id::WHEEL_DOWN;
        if wheel {
            return false;
        }
        let held = self.held_anywhere(button);
        self.mouse_up_event(button);
        held
    }

    fn held_anywhere(&self, button: u32) -> bool {
        fn walk(ui: &Ui, h: Handle, b: u32) -> bool {
            ui.node(h)
                .is_some_and(|n| n.holds(b) || n.children.iter().any(|c| walk(ui, *c, b)))
        }
        self.roots().iter().any(|r| walk(self, *r, button))
    }

    /// A press: global listeners hear it; the element with the keyboard loses it unless the
    /// pointer is on it; each root from the nearest passes the press down to the elements the
    /// pointer is over, until one takes it (the roots behind never hear it, nor hold the button);
    /// then the pointer's elements are worked out again and the tooltip goes.
    // D2-11208
    fn mouse_down_event(&mut self, button: u32) {
        self.broadcast_global(global::MOUSE_DOWN, button);
        self.mgr.last_press = self.mgr.now;
        self.mgr.press_x = self.mgr.x;
        self.mgr.press_y = self.mgr.y;
        if button == button_id::LEFT {
            self.mgr.left_down = true;
        }
        if let Some(a) = self.mgr.active {
            if !self.node(a).is_some_and(|n| n.over_top) {
                self.deactivate_element(a);
                self.mgr.active = None;
            }
        }
        let (x, y) = (self.mgr.x, self.mgr.y);
        let mut handled = false;
        for r in self.roots().to_vec() {
            if handled {
                break;
            }
            self.notify_mouse_down(r, x, y, button, &mut handled);
        }
        self.flush_pending();
        self.update_mouse();
        self.clear_tooltip();
    }

    /// A release: global listeners hear it; each root from the nearest passes it to the elements
    /// holding the button until one takes it; the pointer's elements are worked out again; a
    /// dragged item is dropped on the topmost element.
    // D2-11222
    fn mouse_up_event(&mut self, button: u32) {
        self.broadcast_global(global::MOUSE_UP, button);
        if button == button_id::LEFT {
            self.mgr.left_down = false;
        }
        let (x, y) = (self.mgr.x, self.mgr.y);
        let mut handled = false;
        for r in self.roots().to_vec() {
            if handled {
                break;
            }
            self.notify_mouse_up(r, x, y, button, &mut handled);
        }
        self.flush_pending();
        self.update_mouse();
        if let Some(item) = self.mgr.drag {
            let caught = self.top().is_some_and(|t| self.catch_dropped_item(t, item));
            if let Some(owner) = self.mgr.drag_owner {
                self.drag_and_drop_complete(owner, item, caught);
            }
            self.clear_drag_and_drop();
        }
    }

    // -----------------------------------------------------------------------------------------
    // The keyboard
    // -----------------------------------------------------------------------------------------

    /// An interface key pressed or let go. The element with the keyboard takes the arrows and the
    /// use key first; whatever it does not take, the global listeners hear. Returns whether the
    /// element took it.
    // D2-11198, D2-11199
    pub fn key(&mut self, key: u32, down: bool) -> bool {
        if key == key_id::USE {
            self.mgr.use_down = down;
        }
        let used = match (self.mgr.active, key) {
            (Some(a), key_id::LEFT | key_id::RIGHT | key_id::UP | key_id::DOWN | key_id::USE) => {
                self.element_key(a, key, down)
            }
            _ => false,
        };
        if used {
            return true;
        }
        let what = if down {
            global::KEY_DOWN
        } else {
            global::KEY_UP
        };
        self.broadcast_global(what, key);
        false
    }

    /// The element with the keyboard.
    // D2-11275
    #[must_use]
    pub fn active(&self) -> Option<Handle> {
        self.mgr.active.filter(|h| self.node(*h).is_some())
    }

    /// The top-level element of the element that last took the keyboard, while it has it (none
    /// once any element gives the keyboard up).
    // D2-11276
    #[must_use]
    pub fn active_root(&self) -> Option<Handle> {
        self.mgr.active_root.filter(|h| self.node(*h).is_some())
    }

    /// Whether Shift and Ctrl are held, as the host last said.
    // D2-03447, D2-03446
    #[must_use]
    pub fn modifiers(&self) -> (bool, bool) {
        (self.keyboard.shift, self.keyboard.ctrl)
    }

    /// An element took the keyboard (`on`) or gave it up: the one that had it gives it up.
    // D2-11203
    pub(crate) fn activation_alert(&mut self, h: Handle, on: bool) {
        self.mgr.active_root = None;
        if on {
            if let Some(before) = self.mgr.active {
                if before != h {
                    self.deactivate_element(before);
                }
            }
            self.mgr.active = Some(h);
            self.mgr.active_root = Some(self.root_of(h));
            return;
        }
        if self.mgr.active == Some(h) {
            self.mgr.active = None;
        }
    }

    /// Count an element among those that take the keyboard, by its activation order (after
    /// those of the same order).
    // D2-11228
    pub(crate) fn register_activatable(&mut self, h: Handle) {
        if self.mgr.activatables.contains(&h) {
            return;
        }
        let order = self.activate_order(h);
        let at = self
            .mgr
            .activatables
            .iter()
            .position(|e| self.activate_order(*e) > order)
            .unwrap_or(self.mgr.activatables.len());
        self.mgr.activatables.insert(at, h);
    }

    // D2-11213
    pub(crate) fn unregister_activatable(&mut self, h: Handle) {
        self.mgr.activatables.retain(|e| *e != h);
    }

    // -----------------------------------------------------------------------------------------
    // The clock
    // -----------------------------------------------------------------------------------------

    /// The clock moved on to `now` (seconds): elements under a pointer that moved are worked out,
    /// a resizing element follows the pointer, and whoever listens for the clock hears it.
    // D2-11277
    pub fn use_time(&mut self, now: f64) {
        self.mgr.now = now;
        if self.mgr.moved {
            self.update_mouse();
        }
        self.clean_queues();
        self.tooltip_use_time();
        self.check_drag_and_drop();
        self.broadcast_global(global::TIME, 0);
        self.flush_pending();
    }

    // -----------------------------------------------------------------------------------------
    // Cursors
    // -----------------------------------------------------------------------------------------

    /// Show a cursor (a picture of 0 keeps the one shown); with `default`, it is also the cursor
    /// shown where no element sets one.
    // D2-11196
    pub fn set_cursor(&mut self, cursor: Cursor, default: bool) {
        if default {
            self.mgr.default_cursor = Some(cursor);
        }
        if cursor.picture != 0 && self.mgr.cursor != Some(cursor) {
            self.mgr.cursor = Some(cursor);
            self.touch();
        }
    }

    /// Show the default cursor.
    // D2-11197
    pub fn use_default_cursor(&mut self) {
        if let Some(c) = self.mgr.default_cursor {
            self.set_cursor(c, true);
        }
    }

    /// The cursor to show.
    #[must_use]
    pub fn cursor(&self) -> Option<Cursor> {
        self.mgr.cursor
    }

    // -----------------------------------------------------------------------------------------
    // The display set up again; everything let go
    // -----------------------------------------------------------------------------------------

    /// The display was set up again at `width` by `height`: listeners are told it starts, every
    /// root is placed again by its edges (and everything redrawn), the cursor is shown again, and
    /// listeners are told it is done.
    // D2-11244, D2-11264
    pub fn refresh(&mut self, width: i32, height: i32) {
        self.broadcast_global(global::REFRESH_START, 0);
        self.set_display(width, height);
        for r in self.roots().to_vec() {
            self.place_tree(r);
        }
        self.touch();
        if let Some(c) = self.mgr.cursor.take() {
            self.set_cursor(c, false);
        }
        self.broadcast_global(global::REFRESH_DONE, 0);
    }

    /// Let everything go: the drag dropped, the tooltip cleared, the elements waiting for removal
    /// removed, and every listener's registrations forgotten.
    // D2-11278, D2-11214, D2-11215, D2-11216, D2-11217, D2-11218, D2-11246
    pub fn cleanup(&mut self) {
        self.clear_drag_and_drop();
        if let Some(d) = self.mgr.dropped.take() {
            self.remove(d);
        }
        self.clear_tooltip();
        let m = &mut self.mgr;
        m.globals.clear();
        m.listeners.clear();
        m.global_counts.clear();
        m.element_marks.clear();
        m.global_clean.clear();
        m.element_clean.clear();
        m.by_name.clear();
    }

    // -----------------------------------------------------------------------------------------
    // Depth
    // -----------------------------------------------------------------------------------------
}
