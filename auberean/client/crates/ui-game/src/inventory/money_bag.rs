//! The money bag in the inventory window: the player's money, coins dragged out of it, items
//! dropped on it melted into money.
//!
//! [`MoneyBag`] shows the amount the scripts say the player has. Pulled on while there is money,
//! its coins are dragged out (into a pack, or into the world). An item from the player's pack
//! dropped on it (one the player has and does not wear) is melted: the kinds that melt at once
//! without a question; else a stack is split first; then a question ("melt this into money?")
//! unless the player turned it off or holds shift; a yes melts it.

use std::any::Any;

use auberean_ui::framework::FrameId;
use auberean_ui::{Handle, Mail, Ui};

use super::container::COINS;
use super::{attr, init_stack, message, notice, said, Facts, Request};
use crate::gameplay::{self, dialog, Child, Dialog, Notice, View};

mod name {
    use auberean_ui::Token;
    pub const MONEY_BAG: Token = Token::of("MoneyBag");
    pub const AMOUNT_LABEL: Token = Token::of("AmountLabel");
    /// The amount, and the question before melting.
    pub const AMOUNT: &str = "ID_Inventory_Amount";
    pub const DRAG_TO_MONEY: &str = "ID_Inventory_DragToMoney";
}

/// Whether the money bag takes a dropped element: an item the player has and does not wear.
// D2-02954
#[must_use]
pub fn takes(ui: &Ui, item: Handle, _bag: Handle) -> bool {
    let Some(n) = ui.node(item) else { return false };
    let (Some(kind), Some(id)) = (
        n.id_attr(attr::SHORTCUT_TYPE),
        n.instance_id_attr(attr::SHORTCUT_DATA),
    ) else {
        return false;
    };
    let Some(f) = FACTS.with(|f| f.borrow().as_ref().and_then(|f| f.item(id).cloned())) else {
        return false;
    };
    f.had && !f.worn && kind == super::ITEM_SHORTCUT
}

thread_local! {
    /// The scripts' answers as the money bag last saw them, for its drop check (which the
    /// framework calls with the element tree alone).
    static FACTS: std::cell::RefCell<Option<Facts>> = const { std::cell::RefCell::new(None) };
}

/// The money bag.
#[derive(Debug)]
pub struct MoneyBag {
    frame: FrameId,
    bag: Option<Handle>,
    label: Option<Handle>,
    /// The melt waiting on a split or a question: the item, how many, its slot, its container.
    item: u64,
    quantity: u32,
    slot: u32,
    from: u64,
    stack_context: u32,
    dialog: u32,
    /// The money shown.
    money: u32,
    /// Shift held this frame.
    shift: bool,
    facts: Facts,
}

impl MoneyBag {
    /// The bag found, its drop check given, its label; drops on it and pulls on it heard.
    // D2-02955
    pub fn new(ui: &mut Ui, parent: FrameId, facts: Option<&Facts>) -> Self {
        let frame = ui.new_child_frame(parent);
        let bag = ui.element(name::MONEY_BAG);
        let mut label = None;
        if let Some(b) = bag {
            ui.set_drag_checks(b, None, Some(takes));
            label = ui.child_recursive(b, name::AMOUNT_LABEL);
        }
        if let Some(l) = ui.frame_listener(frame) {
            ui.listen_for_message(l, name::MONEY_BAG, message::STOP_DRAG_ITEM);
            ui.listen_for_message(l, name::MONEY_BAG, message::NOT_DRAGABLE);
        }
        let facts = facts.cloned().unwrap_or_default();
        FACTS.with(|f| *f.borrow_mut() = Some(facts.clone()));
        Self {
            frame,
            bag,
            label,
            item: super::NO_OBJECT,
            quantity: 0,
            slot: 0,
            from: super::NO_OBJECT,
            stack_context: 0,
            dialog: 0,
            money: 0,
            shift: false,
            facts,
        }
    }

    /// The money shown.
    #[must_use]
    pub const fn money(&self) -> u32 {
        self.money
    }

    /// The player's money changed: the amount shown.
    // D2-02949
    pub fn update_money(&mut self, ui: &mut Ui, amount: u32) {
        let Some(l) = self.label else { return };
        let text = super::write(
            ui,
            name::AMOUNT,
            &[("value", auberean_text::Value::UInt(amount))],
        );
        ui.set_text(l, &text);
        self.money = amount;
    }

    /// The question taken down.
    // D2-02950
    pub fn close_dialog(&mut self, out: &mut Vec<gameplay::Request>) {
        out.push(gameplay::Request::DialogDown(self.dialog));
        self.dialog = 0;
    }

    /// Whether to ask before melting: unless the player turned the question off or holds shift.
    // D2-02951
    #[must_use]
    pub fn confirmation_appropriate(&self, shift: bool) -> bool {
        if self.facts.here(self.facts.player) && self.facts.no_melt_question {
            return false;
        }
        !shift
    }

    /// The question: melt this item (by its name) into money?
    // D2-02952
    pub fn make_confirmation_dialog(&mut self, ui: &Ui, out: &mut Vec<gameplay::Request>) {
        let Some(it) = self.facts.item(self.item) else {
            return;
        };
        let item =
            auberean_text::Value::Text(Box::new(auberean_text::StringInfo::literal(&it.name)));
        let text = super::write(ui, name::DRAG_TO_MONEY, &[("name", item)]);
        self.dialog = crate::dialog::new_context();
        out.push(gameplay::Request::Dialog(Dialog {
            id: self.dialog,
            kind: dialog::CONFIRMATION,
            text,
            modal: false,
        }));
    }

    /// The melt: with yes, the item melted; forgotten either way.
    // D2-02953
    pub fn do_drag_to(&mut self, yes: bool, out: &mut Vec<gameplay::Request>) {
        if yes && self.facts.here(self.facts.player) {
            out.push(gameplay::Request::Inventory(Request::IntoMoney {
                item: self.item,
                quantity: self.quantity,
                slot: self.slot,
                from: self.from,
            }));
        }
        self.dialog = 0;
        self.item = super::NO_OBJECT;
        self.quantity = 0;
        self.slot = 0;
        self.from = super::NO_OBJECT;
    }

    /// An item dropped on the bag: from the player's pack, not worn: the kinds that melt at once
    /// melted; else (a question up taken down) kept; a stack split first, else the question (or
    /// the melt when no question is asked).
    // D2-02959
    pub fn money_bag_drop(
        &mut self,
        ui: &Ui,
        dropped: Handle,
        shift: bool,
        out: &mut Vec<gameplay::Request>,
    ) {
        if !self.facts.here(self.facts.player) {
            return;
        }
        let Some(n) = ui.node(dropped) else { return };
        let item = n
            .instance_id_attr(attr::SHORTCUT_DATA)
            .unwrap_or(super::NO_OBJECT);
        let from = n
            .instance_id_attr(attr::FROM_ID)
            .unwrap_or(super::NO_OBJECT);
        if from != self.facts.player {
            return;
        }
        let equipped = n.bool_attr(attr::EQUIPPED);
        let Some(it) = self.facts.item(item).cloned() else {
            return;
        };
        if equipped {
            return;
        }
        let slot = n
            .int_attr(attr::OLD_INDEX)
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(0);
        if it.melts_at_once {
            out.push(gameplay::Request::Inventory(Request::IntoMoney {
                item,
                quantity: it.quantity,
                slot,
                from,
            }));
            return;
        }
        if self.dialog != 0 {
            self.close_dialog(out);
            self.do_drag_to(false, &mut Vec::new());
        }
        self.item = item;
        self.quantity = it.quantity;
        self.slot = slot;
        self.from = from;
        if it.stackable && it.quantity > 1 {
            self.stack_context = super::next_stack_context();
            let amount = it.stack_size.min(it.quantity);
            out.push(gameplay::Request::Notice(init_stack(
                self.stack_context,
                amount,
                amount,
                from,
                super::NO_OBJECT,
                item,
            )));
            return;
        }
        if self.confirmation_appropriate(shift) {
            self.make_confirmation_dialog(ui, out);
            return;
        }
        self.do_drag_to(true, out);
    }

    /// The splitter's answer: that many to melt; the question, unless none is asked.
    // D2-02958
    pub fn stack_done(
        &mut self,
        ui: &Ui,
        context: u32,
        amount: u32,
        shift: bool,
        out: &mut Vec<gameplay::Request>,
    ) {
        if context != self.stack_context {
            return;
        }
        self.quantity = amount;
        if self.confirmation_appropriate(shift) {
            self.make_confirmation_dialog(ui, out);
            return;
        }
        self.do_drag_to(true, out);
    }

    /// The question answered.
    // D2-02957
    pub fn close_confirmation_dialog(
        &mut self,
        dialog: u32,
        yes: bool,
        out: &mut Vec<gameplay::Request>,
    ) {
        if dialog == self.dialog {
            self.do_drag_to(yes, out);
        }
    }

    /// It goes: no longer listening.
    // D2-02956
    pub fn close_with(self, ui: &mut Ui, _out: &mut Vec<gameplay::Request>) {
        FACTS.with(|f| *f.borrow_mut() = None);
        ui.drop_frame(self.frame);
    }
}

impl Child for MoneyBag {
    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Something dropped on the bag; the bag pulled on while there is money (its coins dragged).
    // D2-02960
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<gameplay::Request>) {
        let Mail::Element { what, param, .. } = *mail else {
            return;
        };
        if what == message::STOP_DRAG_ITEM && param != 0 {
            if let Some(d) = ui.dropped() {
                let shift = self.shift;
                self.money_bag_drop(ui, d, shift, out);
            }
        }
        if what == message::NOT_DRAGABLE && self.money != 0 {
            if let Some(coins) = self.bag.and_then(|b| ui.child_recursive(b, COINS)) {
                ui.start_drag(coins);
            }
        }
    }

    /// The splitter's answer; the question answered.
    fn notice(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<gameplay::Request>) {
        match n.native {
            notice::STACK_DONE => {
                let shift = self.shift;
                self.stack_done(ui, n.word_at(0), n.word_at(1), shift, out);
            }
            notice::CLOSE_CONFIRMATION => {
                self.close_confirmation_dialog(n.word_at(0), n.word_at(1) != 0, out)
            }
            _ => {}
        }
    }

    /// A frame: the scripts' answers kept; the player's money.
    fn update(&mut self, ui: &mut Ui, view: &View<'_>, _out: &mut Vec<gameplay::Request>) {
        self.shift = view.shift;
        if let Some(f) = view.inventory {
            if f.version != self.facts.version || f.player != self.facts.player {
                if f.player != self.facts.player {
                    self.update_money(ui, u32::try_from(f.money).unwrap_or(0));
                }
                self.facts = f.clone();
                FACTS.with(|c| *c.borrow_mut() = Some(f.clone()));
            }
        }
        for n in view.everyone {
            if n.native == notice::UPDATE_MONEY && n.about == Some(self.facts.player) {
                self.update_money(ui, n.word());
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

/// A notice that a question closed, with its answer (what a dialog sends when it closes).
#[must_use]
pub fn closed(dialog: u32, yes: bool) -> Notice {
    said(notice::CLOSE_CONFIRMATION, &[dialog, u32::from(yes)])
}
