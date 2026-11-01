//! The pack in the inventory window: four packs, each a segment of the player's pack shown in
//! its own cells, one at a time by its tab.
//!
//! [`Backpack`] makes the four packs' cells ([`ContainerCells`]: the main pack, then packs one
//! to three), each starting at the slot after the one before; when the player's pack holds more
//! than they show, the last pack shows the rest. The main pack shows first. A tab pressed shows
//! its pack. An item dropped on a pack's tab (not the one showing) goes into that pack after its
//! last item: taken off when worn, a stack split first; coins dropped on a tab are taken out of
//! the money bag. When the scripts say the pack grew or shrank, the last pack takes up the
//! difference.

use std::any::Any;

use auberean_ui::framework::FrameId;
use auberean_ui::token::{msg, state};
use auberean_ui::{Handle, Mail, Token, Ui};

use super::container::{self, ContainerCells};
use super::{attr, init_stack, message, notice, said, Facts, Request};
use crate::gameplay::{self, Child, Notice, View};

/// The packs: their cells' element and their tab.
pub const PACKS: [(&str, &str); 4] = [
    ("PackMainContainer", "PackMainButton"),
    ("PackOneContainer", "PackOneButton"),
    ("PackTwoContainer", "PackTwoButton"),
    ("PackThreeContainer", "PackThreeButton"),
];

/// The pack in the inventory window.
#[derive(Debug)]
pub struct Backpack {
    frame: FrameId,
    packs: Vec<ContainerCells>,
    tabs: Vec<Handle>,
    /// The splits asked for: an item dropped on a tab, coins dropped on one.
    item_context: u32,
    coins_context: u32,
    /// The move waiting on a split.
    container: u64,
    moving: u64,
    old_index: u32,
    new_index: u32,
    facts: Facts,
}

impl Backpack {
    /// The four packs, each from the slot after the last one before; their tabs and drops on
    /// them heard; the last pack showing what the others do not; the main pack shown.
    // D2-02947
    pub fn new(ui: &mut Ui, parent: FrameId, facts: Option<&Facts>) -> Self {
        let frame = ui.new_child_frame(parent);
        let facts = facts.cloned().unwrap_or_default();
        let mut me = Self {
            frame,
            packs: Vec::new(),
            tabs: Vec::new(),
            item_context: 0,
            coins_context: 0,
            container: super::NO_OBJECT,
            moving: super::NO_OBJECT,
            old_index: 0,
            new_index: 0,
            facts,
        };
        for (cells, tab) in PACKS {
            if ui.element(Token::of(cells)).is_some() {
                me.packs.push(ContainerCells::new(ui, frame));
            }
            if let Some(t) = ui.element(Token::of(tab)) {
                me.tabs.push(t);
            }
            if let Some(l) = ui.frame_listener(frame) {
                ui.listen_for_message(l, Token::of(tab), msg::BUTTON_RELEASE_ON);
                ui.listen_for_message(l, Token::of(tab), message::STOP_DRAG_ITEM);
            }
        }
        me.init_packs(ui, &mut Vec::new());
        me.show_pack(ui, 0);
        me
    }

    /// Each pack shows its segment of the player's pack, from the slot after the last one
    /// before; the last pack shows what the others do not.
    fn init_packs(&mut self, ui: &mut Ui, out: &mut Vec<gameplay::Request>) {
        let mut first = 0;
        let player = self.facts.player;
        let facts = self.facts.clone();
        for (segment, (cells, _)) in (0u32..).zip(PACKS) {
            let e = ui.element(Token::of(cells));
            if let Some(d) = self.packs.get_mut(segment as usize) {
                first += d.init(ui, player, e, first, segment, &facts, out);
            }
        }
        if facts.here(player) {
            let size = facts.size(player);
            if size != first {
                if let Some(last) = self.packs.last_mut() {
                    let n = size.wrapping_sub(last.first_slot());
                    if i32::try_from(n).is_ok_and(|n| n > 0) {
                        last.resize(ui, n, &facts, out);
                    }
                }
            }
        }
    }

    /// The packs' cells.
    #[must_use]
    pub fn packs(&self) -> &[ContainerCells] {
        &self.packs
    }

    /// Show one pack, the others hidden; its tab pressed in, the others out.
    // D2-02942
    pub fn show_pack(&mut self, ui: &mut Ui, which: usize) {
        if which >= PACKS.len() {
            return;
        }
        for (i, p) in self.packs.iter_mut().enumerate() {
            p.show(ui, i == which);
        }
        for (i, t) in self.tabs.iter().enumerate() {
            let s = if i == which {
                state::NORMAL_PRESSED
            } else {
                state::NORMAL
            };
            ui.set_state(*t, s);
        }
    }

    /// The pack grew or shrank: the last pack (the one starting furthest on) shows what the
    /// others do not (at least one cell).
    // D2-02941
    pub fn update_backpack(&mut self, ui: &mut Ui, out: &mut Vec<gameplay::Request>) {
        if !self.facts.here(self.facts.player) || self.packs.len() < PACKS.len() {
            return;
        }
        let size = self.facts.size(self.facts.player);
        let shown: u32 = self.packs.iter().map(ContainerCells::max_size).sum();
        let Some(last) = (0..self.packs.len()).max_by_key(|i| self.packs[*i].first_slot()) else {
            return;
        };
        let mut n = i64::from(self.packs[last].max_size()) - i64::from(shown) + i64::from(size);
        if n < 0 {
            n = 1;
        }
        let facts = self.facts.clone();
        self.packs[last].resize(ui, u32::try_from(n).unwrap_or(u32::MAX), &facts, out);
    }

    /// An item dropped on a tab: moved there (unless it stays where it is).
    // D2-02937
    pub fn finish_item_drop(&mut self, quantity: u32, out: &mut Vec<gameplay::Request>) {
        if self.old_index != self.new_index {
            if !self.facts.here(self.facts.player) {
                return;
            }
            out.push(gameplay::Request::Inventory(Request::Put {
                item: self.moving,
                from: self.container,
                to: self.container,
                old_slot: self.old_index,
                new_slot: self.new_index,
                quantity,
            }));
        }
        self.forget();
    }

    /// Coins dropped on a tab: that many taken out of the money bag.
    // D2-02938
    pub fn finish_money_bag_drop(&mut self, amount: u32, out: &mut Vec<gameplay::Request>) {
        if !self.facts.here(self.facts.player) {
            return;
        }
        out.push(gameplay::Request::Inventory(Request::OutOfMoney {
            amount,
            slot: self.new_index,
            container: self.container,
        }));
        self.forget();
    }

    fn forget(&mut self) {
        self.moving = super::NO_OBJECT;
        self.container = super::NO_OBJECT;
        self.old_index = 0;
        self.new_index = 0;
    }

    /// The stack splitter's answer.
    // D2-02939
    pub fn stack_done(&mut self, context: u32, amount: u32, out: &mut Vec<gameplay::Request>) {
        if context == self.item_context {
            self.finish_item_drop(amount, out);
        } else if context == self.coins_context {
            self.finish_money_bag_drop(amount, out);
        }
    }

    /// Something dropped on pack `i`'s tab, unless it is full: worn, taken off into its next
    /// slot; coins, the splitter asked how many (at most the money, at most a stack); an item, when
    /// the pack is not the one showing, moved after its last item (a stack split first).
    fn tab_drop(&mut self, ui: &Ui, i: usize, dropped: Handle, out: &mut Vec<gameplay::Request>) {
        let Some(n) = ui.node(dropped) else { return };
        let item = n
            .instance_id_attr(attr::SHORTCUT_DATA)
            .unwrap_or(super::NO_OBJECT);
        let old = n
            .int_attr(attr::OLD_INDEX)
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(0);
        if !self.facts.here(self.facts.player) {
            return;
        }
        let Some(pack) = self.packs.get(i) else {
            return;
        };
        let count = u32::try_from(pack.items().len()).unwrap_or(u32::MAX);
        if count >= pack.max_size() {
            return;
        }
        let next = pack.first_slot() + count;
        if n.bool_attr(attr::EQUIPPED) {
            out.push(gameplay::Request::Inventory(Request::TakeOff {
                item,
                slot: next,
            }));
            return;
        }
        if n.id == container::COINS {
            self.container = super::NO_OBJECT;
            self.old_index = 0;
            self.new_index = next;
            self.moving = super::NO_OBJECT;
            self.coins_context = super::next_stack_context();
            let money = u32::try_from(self.facts.money).unwrap_or(0);
            let amount = self.facts.coin_stack.min(money);
            out.push(gameplay::Request::Notice(init_stack(
                self.coins_context,
                amount,
                amount,
                super::NO_OBJECT,
                super::NO_OBJECT,
                super::NO_OBJECT,
            )));
            return;
        }
        let pressed = self
            .tabs
            .get(i)
            .and_then(|t| ui.node(*t))
            .is_some_and(|t| t.state == state::NORMAL_PRESSED);
        if pressed {
            return;
        }
        self.container = pack.container();
        self.old_index = old;
        self.new_index = next;
        self.moving = item;
        let Some(it) = self.facts.item(item) else {
            return;
        };
        let quantity = it.quantity;
        if it.stackable && quantity > 1 {
            self.item_context = super::next_stack_context();
            let amount = it.stack_size.min(quantity);
            out.push(gameplay::Request::Notice(init_stack(
                self.item_context,
                amount,
                amount,
                self.container,
                self.container,
                item,
            )));
            return;
        }
        self.finish_item_drop(quantity, out);
    }

    /// It goes: any split it asked for called off; its packs.
    // D2-02946
    pub fn close_with(mut self, ui: &mut Ui, out: &mut Vec<gameplay::Request>) {
        for c in [self.item_context, self.coins_context] {
            if c != 0 {
                out.push(gameplay::Request::Notice(said(notice::CLOSE_STACK, &[c])));
            }
        }
        for p in self.packs.drain(..) {
            p.close(ui, out);
        }
        ui.drop_frame(self.frame);
    }
}

impl Child for Backpack {
    fn frame(&self) -> FrameId {
        self.frame
    }

    /// A tab pressed (its pack shown), or something dropped on it.
    // D2-02944
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<gameplay::Request>) {
        let Mail::Element {
            element,
            what,
            param,
            ..
        } = *mail
        else {
            return;
        };
        let Some(i) = PACKS.iter().position(|(_, t)| Token::of(t) == element) else {
            return;
        };
        if what == msg::BUTTON_RELEASE_ON {
            self.show_pack(ui, i);
            return;
        }
        if what == message::STOP_DRAG_ITEM && param != 0 {
            if let Some(d) = ui.dropped() {
                self.tab_drop(ui, i, d, out);
            }
        }
    }

    /// The splitter's answer (its own and its packs').
    fn notice(&mut self, _ui: &mut Ui, n: &Notice, out: &mut Vec<gameplay::Request>) {
        if n.native == notice::STACK_DONE {
            self.stack_done(n.word_at(0), n.word_at(1), out);
        }
        for p in &mut self.packs {
            p.notice(n, &self.facts, out);
        }
    }

    /// A frame: the scripts' answers kept; the pack grown or shrunk; its packs' own.
    // D2-02943
    fn update(&mut self, ui: &mut Ui, view: &View<'_>, out: &mut Vec<gameplay::Request>) {
        let mut changed = false;
        if let Some(f) = view.inventory {
            if f.player != self.facts.player {
                // The player is known now: its pack shown.
                self.facts = f.clone();
                self.init_packs(ui, out);
                let shown = (0..self.tabs.len())
                    .find(|i| {
                        ui.node(self.tabs[*i])
                            .is_some_and(|t| t.state == state::NORMAL_PRESSED)
                    })
                    .unwrap_or(0);
                self.show_pack(ui, shown);
            } else if f.version != self.facts.version {
                self.facts = f.clone();
                changed = true;
            }
        }
        for n in view.everyone {
            if n.native == notice::UPDATE_BACKPACK && n.about == Some(self.facts.player) {
                self.update_backpack(ui, out);
            }
        }
        for p in &mut self.packs {
            p.update(ui, view.everyone, &self.facts, changed, out);
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
