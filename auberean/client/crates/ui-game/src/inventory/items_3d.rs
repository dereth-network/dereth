//! Items and the world: an item from the pack dropped in the world goes to what is under the
//! pointer (given to someone, put in a container, dropped on the ground), and what is selected in
//! the world is picked up by pulling on it.
//!
//! [`Items3d`] listens to the world's field (the selection's, behind every panel). An item of the
//! player's pack (not worn) dropped there, or coins from the money bag, starts a search for what
//! is under the pointer; once found (the player itself counts as nothing), the item moves there
//! (a stack split first), or as many coins as the splitter says. Pulling on the world while
//! something is selected drags its icon, to be dropped in the pack.

use std::any::Any;

use auberean_ui::framework::FrameId;
use auberean_ui::manager::global;
use auberean_ui::system::Value;
use auberean_ui::{Handle, Mail, Ui};

use super::container::COINS;
use super::{attr, init_stack, message, notice, said, Facts, Request, ITEM_SHORTCUT};
use crate::gameplay::{self, Child, Notice, View};

mod name {
    use auberean_ui::Token;
    pub const ITEM_SELECTION_FIELD: Token = Token::of("ItemSelectionField");
    pub const ITEM_ICON: Token = Token::of("ItemIcon");
}

/// Whether the world takes a dropped element: coins, or an item from a container.
// D2-02964
#[must_use]
pub fn takes(ui: &Ui, item: Handle, _field: Handle) -> bool {
    let Some(n) = ui.node(item) else { return false };
    if n.id == COINS {
        return true;
    }
    if n.id_attr(attr::SHORTCUT_TYPE) == Some(ITEM_SHORTCUT) {
        return n
            .instance_id_attr(attr::FROM_ID)
            .is_some_and(|f| f != super::NO_OBJECT);
    }
    false
}

/// Items and the world.
#[derive(Debug)]
pub struct Items3d {
    frame: FrameId,
    field: Option<Handle>,
    icon: Option<Handle>,
    /// A search under the pointer under way, for an item or for coins.
    item_drop: bool,
    money_drop: bool,
    /// The splits asked for.
    item_context: u32,
    money_context: u32,
    /// The item dropped, what was found, its slot in the pack.
    item: u64,
    target: u64,
    old_index: u32,
    facts: Facts,
    /// The last answer heard from the world.
    answer: Option<u64>,
}

impl Items3d {
    /// The world's field found (its drop check given) and its icon; the clock, pulls and drops
    /// on the field heard.
    // D2-02965
    pub fn new(ui: &mut Ui, parent: FrameId, facts: Option<&Facts>) -> Self {
        let frame = ui.new_child_frame(parent);
        let field = ui.element(name::ITEM_SELECTION_FIELD);
        let mut icon = None;
        if let Some(f) = field {
            ui.set_drag_checks(f, None, Some(takes));
            icon = ui.child_recursive(f, name::ITEM_ICON);
        }
        if let Some(l) = ui.frame_listener(frame) {
            ui.listen_for_global_message(l, global::TIME);
            ui.listen_for_message(l, name::ITEM_SELECTION_FIELD, message::NOT_DRAGABLE);
            ui.listen_for_message(l, name::ITEM_SELECTION_FIELD, message::STOP_DRAG_ITEM);
        }
        Self {
            frame,
            field,
            icon,
            item_drop: false,
            money_drop: false,
            item_context: 0,
            money_context: 0,
            item: super::NO_OBJECT,
            target: super::NO_OBJECT,
            old_index: 0,
            facts: facts.cloned().unwrap_or_default(),
            answer: facts.and_then(|f| f.picked),
        }
    }

    /// The search under the pointer started.
    // D2-02962
    fn start_find_target(&mut self, out: &mut Vec<gameplay::Request>) -> bool {
        self.facts.picked = None;
        out.push(gameplay::Request::Inventory(Request::PickWorld));
        true
    }

    /// Whether the search found something: what it found (the player itself is nothing).
    // D2-02970
    fn finish_find_target(&mut self) -> bool {
        let Some(found) = self.facts.picked else {
            return false;
        };
        self.target = if found == self.facts.player {
            super::NO_OBJECT
        } else {
            found
        };
        true
    }

    /// The world pulled on while something is selected: its icon dragged.
    // D2-02963
    pub fn start_3d_drag(&mut self, ui: &mut Ui) {
        let Some(icon) = self.icon else { return };
        // The world is pulled on at every move until a drag starts; heard after the moves (the
        // original hears it at once), only the first pull starts one.
        if ui.dragged().is_some() {
            return;
        }
        if !self.facts.here(self.facts.player) {
            return;
        }
        let sel = self.facts.selection;
        if sel == super::NO_OBJECT {
            return;
        }
        // D2-01973
        let Some(it) = self.facts.item(sel) else {
            return;
        };
        if !it.icon_layers.is_empty() {
            ui.set_icon_desc(icon, &it.icon_layers);
        } else if it.icon != 0 {
            ui.set_image(icon, it.icon, 0);
        } else {
            return;
        }
        ui.set_attr(icon, auberean_ui::token::attr::DRAGABLE, Value::Bool(true));
        ui.set_attr(icon, attr::SHORTCUT_TYPE, Value::Id(ITEM_SHORTCUT));
        ui.set_attr(icon, attr::SHORTCUT_DATA, Value::InstanceId(sel));
        ui.start_drag(icon);
    }

    /// An item dropped on the world: one of the player's pack, not worn, searched for a target.
    // D2-02968
    fn start_item_drop(&mut self, ui: &Ui, dropped: Handle, out: &mut Vec<gameplay::Request>) {
        let Some(n) = ui.node(dropped) else { return };
        let item = n
            .instance_id_attr(attr::SHORTCUT_DATA)
            .unwrap_or(super::NO_OBJECT);
        let from = n
            .instance_id_attr(attr::FROM_ID)
            .unwrap_or(super::NO_OBJECT);
        if n.bool_attr(attr::EQUIPPED) || from != self.facts.player {
            return;
        }
        let old = n
            .int_attr(attr::OLD_INDEX)
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(0);
        if self.start_find_target(out) {
            self.item_drop = true;
            self.item = item;
            self.old_index = old;
        }
    }

    /// Coins dropped on the world: searched for a target.
    // D2-02969
    fn start_money_drop(&mut self, out: &mut Vec<gameplay::Request>) {
        if self.start_find_target(out) {
            self.money_drop = true;
        }
    }

    /// The target found for an item: moved there (a stack split first).
    // D2-02973
    fn mid_item_drop(&mut self, out: &mut Vec<gameplay::Request>) {
        if !self.finish_find_target() {
            return;
        }
        self.item_drop = false;
        let Some(it) = self.facts.item(self.item).cloned() else {
            return;
        };
        if it.stackable && it.quantity > 1 {
            self.item_context = super::next_stack_context();
            let amount = it.stack_size.min(it.quantity);
            out.push(gameplay::Request::Notice(init_stack(
                self.item_context,
                amount,
                amount,
                self.facts.player,
                super::NO_OBJECT,
                self.item,
            )));
            return;
        }
        self.finish_item_drop(it.quantity, out);
    }

    /// The item moved to the target.
    // D2-02961
    fn finish_item_drop(&mut self, quantity: u32, out: &mut Vec<gameplay::Request>) {
        if !self.facts.here(self.facts.player) {
            return;
        }
        out.push(gameplay::Request::Inventory(Request::Put {
            item: self.item,
            from: self.facts.player,
            to: self.target,
            old_slot: self.old_index,
            new_slot: 0,
            quantity,
        }));
        self.item = super::NO_OBJECT;
        self.target = super::NO_OBJECT;
        self.old_index = 0;
    }

    /// The target found for coins: the splitter asked how many (at most the money, at most a
    /// stack).
    // D2-02974
    fn mid_money_drop(&mut self, out: &mut Vec<gameplay::Request>) {
        if !self.finish_find_target() {
            return;
        }
        self.money_drop = false;
        if !self.facts.here(self.facts.player) {
            return;
        }
        self.money_context = super::next_stack_context();
        let money = u32::try_from(self.facts.money).unwrap_or(0);
        let amount = self.facts.coin_stack.min(money);
        out.push(gameplay::Request::Notice(init_stack(
            self.money_context,
            amount,
            amount,
            super::NO_OBJECT,
            super::NO_OBJECT,
            super::NO_OBJECT,
        )));
    }

    /// The splitter's answer: the item moved, or the coins taken out of the bag to the target.
    // D2-02967
    fn stack_done(&mut self, context: u32, amount: u32, out: &mut Vec<gameplay::Request>) {
        if context == self.item_context {
            self.finish_item_drop(amount, out);
        }
        if context == self.money_context && self.facts.here(self.facts.player) {
            out.push(gameplay::Request::Inventory(Request::OutOfMoney {
                amount,
                slot: 0,
                container: self.target,
            }));
        }
    }

    /// It goes: any split it asked for called off.
    // D2-02966, D2-02971
    pub fn close_with(self, ui: &mut Ui, out: &mut Vec<gameplay::Request>) {
        for c in [self.item_context, self.money_context] {
            if c != 0 {
                out.push(gameplay::Request::Notice(said(notice::CLOSE_STACK, &[c])));
            }
        }
        ui.drop_frame(self.frame);
    }
}

impl Items3d {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[0x45A];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for Items3d {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// The clock (a search found); an item or coins dropped on the world; the world pulled on.
    // D2-02972, D2-02975
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<gameplay::Request>) {
        match *mail {
            Mail::Global {
                what: global::TIME, ..
            } => {
                if self.item_drop {
                    self.mid_item_drop(out);
                }
                if self.money_drop {
                    self.mid_money_drop(out);
                }
            }
            Mail::Element {
                element,
                what,
                param,
                ..
            } if element == name::ITEM_SELECTION_FIELD => {
                if what == message::STOP_DRAG_ITEM && param != 0 {
                    let Some(d) = ui.dropped() else { return };
                    if ui.node(d).is_some_and(|n| n.id == COINS) {
                        self.start_money_drop(out);
                    } else {
                        self.start_item_drop(ui, d, out);
                    }
                } else if what == message::NOT_DRAGABLE {
                    self.start_3d_drag(ui);
                }
            }
            _ => {}
        }
    }

    fn notice(&mut self, _ui: &mut Ui, n: &Notice, out: &mut Vec<gameplay::Request>) {
        if n.native == notice::STACK_DONE {
            self.stack_done(n.word_at(0), n.word_at(1), out);
        }
    }

    /// A frame: the scripts' answers kept; the world's field found once it is made.
    fn update(&mut self, ui: &mut Ui, view: &View<'_>, _out: &mut Vec<gameplay::Request>) {
        if let Some(f) = view.inventory {
            if f.version != self.facts.version || f.player != self.facts.player {
                let picked = self.facts.picked;
                self.facts = f.clone();
                self.facts.picked = picked;
            }
            // An answer counts when it comes: the last search's, still there as this one is
            // asked, does not answer it.
            if f.picked != self.answer {
                self.answer = f.picked;
                self.facts.picked = f.picked;
            }
        }
        if self.field.is_none() {
            if let Some(f) = ui.element(name::ITEM_SELECTION_FIELD) {
                ui.set_drag_checks(f, None, Some(takes));
                self.icon = ui.child_recursive(f, name::ITEM_ICON);
                self.field = Some(f);
            }
        }
    }

    fn close(self: Box<Self>, ui: &mut Ui) {
        self.close_with(ui, &mut Vec::new());
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
