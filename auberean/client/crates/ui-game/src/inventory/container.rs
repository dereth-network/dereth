//! A container's cells: a list of empty cells, as many as the container shows, with the items
//! of its segment in their slots.
//!
//! [`ContainerCells`] is made for a container's element (a pack of the inventory window, an
//! opened container's window): its list box gets as many cells as the element names (or as the
//! container holds), and each item of the container's segment goes in the cell of its slot. The
//! scripts' notices about the container keep it current: everything again, an item added at a
//! slot or removed, a request about one under way or failed (the item pending, or not).
//!
//! An item dropped on the cells is moved to the slot under the pointer (past the segment's last
//! item: after it), out of the container it came from; a stack is split first (the stack splitter
//! asked for the amount, at most a full stack), unless it is worn, when it is taken off into that
//! slot instead. Coins dropped from the money bag are taken out of it into the slot, as many as
//! the splitter says. An item dragged from another panel of the player's (the shortcut bar) is
//! told to that panel with the slot.

use auberean_ui::framework::FrameId;
use auberean_ui::system::Value;
use auberean_ui::{Handle, Mail, Token, Ui};

use super::item::ItemCell;
use super::{attr, init_stack, message, notice, said, Facts, Request, TOOLBAR_LAYOUT_ENUM};
use crate::gameplay::{self, Notice};

mod name {
    use auberean_ui::Token;
    pub const CONTAINER_LIST_BOX: Token = Token::of("ContainerListBox");
    pub const ITEM_BACKGROUND: Token = Token::of("ItemBackground");
    pub const ITEM_ICON: Token = Token::of("ItemIcon");
    /// The money bag's coins, dragged out of it.
    pub const COINS_FIELD: Token = Token::of("CoinsField");
}

/// The coins' element: what a container takes besides items.
pub const COINS: Token = name::COINS_FIELD;

/// The name of the coins' stack in the stack splitter.
pub const MONEY_STACK: &str = "ID_Inventory_MoneyStack";

/// What the scripts' container notice says changed.
pub mod change {
    pub const ALL: u32 = 0x4000_0001;
    pub const ADDED: u32 = 0x4000_0002;
    pub const REMOVED: u32 = 0x4000_0003;
    pub const FAILED: u32 = 0x4000_0004;
    pub const UNDER_WAY: u32 = 0x4000_0005;
}

/// The count of request contexts (the containers' own).
static CONTEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// A new request context.
// D2-02987
pub fn request_context() -> u32 {
    CONTEXT
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        .wrapping_add(1)
}

/// Whether a container's cells take a dropped element: the money bag's coins, or an item's icon.
// D2-02985
#[must_use]
pub fn takes(ui: &Ui, item: Handle, _container: Handle) -> bool {
    let Some(n) = ui.node(item) else { return false };
    if n.id == name::COINS_FIELD {
        return true;
    }
    n.id_attr(attr::SHORTCUT_TYPE) == Some(super::ITEM_SHORTCUT)
}

/// A container's cells.
#[derive(Debug)]
// D2-03007
pub struct ContainerCells {
    frame: FrameId,
    container: u64,
    /// The items shown, and the cells the list has.
    count: u32,
    max_size: u32,
    list_name: Token,
    root: Option<Handle>,
    list: Option<Handle>,
    /// The items, in their cells' order.
    items: Vec<ItemCell>,
    /// The splits asked for: an item dropped, coins dropped.
    item_context: u32,
    coins_context: u32,
    /// The move waiting on a split.
    moving: u64,
    moving_from: u64,
    old_index: u32,
    new_index: u32,
    /// The container's segment shown and its first slot; how many a split moves at most.
    first_slot: u32,
    segment: u32,
    move_quantity: u32,
    /// The version of the scripts' answers the cells were last laid out from.
    laid_out: Option<u64>,
}

impl ContainerCells {
    /// Nothing shown yet, under a panel's framework.
    // D2-03001
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        Self {
            frame: ui.new_child_frame(parent),
            container: super::NO_OBJECT,
            count: 0,
            max_size: 0,
            list_name: Token(0),
            root: None,
            list: None,
            items: Vec::new(),
            item_context: 0,
            coins_context: 0,
            moving: super::NO_OBJECT,
            moving_from: super::NO_OBJECT,
            old_index: 0,
            new_index: 0,
            first_slot: 0,
            segment: 0,
            move_quantity: 0,
            laid_out: None,
        }
    }

    /// The container shown.
    #[must_use]
    pub const fn container(&self) -> u64 {
        self.container
    }

    /// How many cells it has.
    #[must_use]
    pub const fn max_size(&self) -> u32 {
        self.max_size
    }

    /// The segment's first slot.
    #[must_use]
    pub const fn first_slot(&self) -> u32 {
        self.first_slot
    }

    /// The items shown, in their cells' order.
    #[must_use]
    pub fn items(&self) -> Vec<u64> {
        self.items.iter().map(ItemCell::item).collect()
    }

    /// The item shown in a cell.
    #[must_use]
    pub fn item_display(&self, item: u64) -> Option<&ItemCell> {
        self.items.iter().find(|d| d.item() == item)
    }

    /// The list box of cells.
    #[must_use]
    pub const fn list(&self) -> Option<Handle> {
        self.list
    }

    /// Its framework.
    #[must_use]
    pub const fn frame(&self) -> FrameId {
        self.frame
    }

    /// Show `container`'s segment `segment`, whose first slot is `first`, in the element
    /// `element`: its list box's cells made (as many as the element names, else as the container
    /// holds), drops on them heard, its items put in. Returns how many cells it has.
    // D2-03004
    #[allow(clippy::too_many_arguments)]
    pub fn init(
        &mut self,
        ui: &mut Ui,
        container: u64,
        element: Option<Handle>,
        first: u32,
        segment: u32,
        facts: &Facts,
        out: &mut Vec<gameplay::Request>,
    ) -> u32 {
        self.cleanup(ui, out);
        self.first_slot = first;
        self.segment = segment;
        self.container = container;
        let Some(element) = element else {
            return self.max_size;
        };
        self.root = Some(element);
        self.list = ui.child(element, name::CONTAINER_LIST_BOX);
        let Some(list) = self.list else {
            return self.max_size;
        };
        ui.set_drag_checks(list, None, Some(takes));
        self.list_name = ui.node(list).map_or(Token(0), |n| n.id);
        if let Some(l) = ui.frame_listener(self.frame) {
            ui.listen_for_message(l, self.list_name, message::STOP_DRAG_ITEM);
        }
        if facts.here(container) {
            self.max_size = ui
                .node(element)
                .and_then(|n| n.int_attr(attr::NUM_SLOTS))
                .map_or_else(|| facts.size(container), |n| u32::try_from(n).unwrap_or(0));
            for _ in 0..self.max_size {
                if let Some(cell) =
                    ui.frame_create_element(TOOLBAR_LAYOUT_ENUM, name::ITEM_BACKGROUND)
                {
                    ui.list_multi_add(list, cell);
                }
            }
            self.get_contents(ui, facts);
            ui.list_changes_done(list);
        }
        self.max_size
    }

    /// Everything let go: any split it asked for called off, the move forgotten, its items and
    /// cells gone, drops no longer heard.
    // D2-03002
    pub fn cleanup(&mut self, ui: &mut Ui, out: &mut Vec<gameplay::Request>) {
        for c in [self.item_context, self.coins_context] {
            if c != 0 {
                out.push(gameplay::Request::Notice(said(notice::CLOSE_STACK, &[c])));
            }
        }
        self.item_context = 0;
        self.coins_context = 0;
        self.moving = super::NO_OBJECT;
        self.moving_from = super::NO_OBJECT;
        self.old_index = 0;
        self.new_index = 0;
        self.container = super::NO_OBJECT;
        self.flush(ui);
        if let Some(list) = self.list {
            ui.list_flush(list);
        }
        if let Some(l) = ui.frame_listener(self.frame) {
            ui.unregister_for_message(
                auberean_ui::Listener::Other(l),
                self.list_name,
                message::STOP_DRAG_ITEM,
            );
        }
        self.list_name = Token(0);
        self.list = None;
        self.max_size = 0;
        self.laid_out = None;
    }

    /// The items go; cells made past the container's size go too.
    // D2-03000
    pub fn flush(&mut self, ui: &mut Ui) {
        for d in self.items.drain(..) {
            d.close(ui);
        }
        if let Some(list) = self.list {
            while self.max_size < self.count {
                if let Some(cell) = ui.list_multi_remove(list, 0) {
                    ui.remove(cell);
                }
                self.count -= 1;
            }
        }
        self.count = 0;
    }

    /// The container's segment again: its items, each in the cell of its slot.
    // D2-03003
    pub fn get_contents(&mut self, ui: &mut Ui, facts: &Facts) {
        self.flush(ui);
        self.laid_out = Some(facts.version);
        let Some(c) = facts.contents(self.container, self.segment).cloned() else {
            return;
        };
        for (i, item) in c.items.iter().enumerate() {
            let i = u32::try_from(i).unwrap_or(u32::MAX);
            if c.first <= i && i < c.first + c.count {
                self.add_item(ui, *item, i - c.first, facts);
            }
        }
    }

    /// An item into the cell at `index`: an empty cell from the end (or a new one, past the
    /// container's size), the item shown in it, its slot noted on its icon.
    // D2-02995
    pub fn add_item(&mut self, ui: &mut Ui, item: u64, index: u32, facts: &Facts) {
        let Some(list) = self.list else { return };
        let cell = if self.count + 1 > self.max_size {
            ui.frame_create_element(TOOLBAR_LAYOUT_ENUM, name::ITEM_BACKGROUND)
        } else {
            ui.list_multi_remove(list, self.max_size as usize - 1)
        };
        let Some(cell) = cell else { return };
        ui.list_multi_insert(list, cell, index as usize);
        let mut d = ItemCell::new(ui, Some(facts));
        d.init(
            ui,
            cell,
            name::ITEM_ICON,
            item,
            self.container,
            false,
            Some(facts),
        );
        let Some(icon) = d.icon() else {
            d.close(ui);
            return;
        };
        let slot = self.first_slot + index;
        ui.set_attr(
            icon,
            attr::OLD_INDEX,
            Value::Int(i32::try_from(slot).unwrap_or(i32::MAX)),
        );
        let at = (index as usize).min(self.items.len());
        self.items.insert(at, d);
        self.count += 1;
    }

    /// Where an item is among the cells.
    // D2-02996
    #[must_use]
    pub fn find_item(&self, item: u64) -> Option<usize> {
        self.items.iter().position(|d| d.item() == item)
    }

    /// An item taken out: its cell emptied and put back at the end (a cell past the container's
    /// size goes). Returns whether its cell was found.
    // D2-02997
    pub fn remove_item(&mut self, ui: &mut Ui, item: u64) -> bool {
        let Some(list) = self.list else { return false };
        let Some(i) = self.find_item(item) else {
            return false;
        };
        let d = self.items.remove(i);
        d.close(ui);
        let past = self.max_size < self.count;
        let Some(cell) = ui.list_multi_remove(list, i) else {
            return false;
        };
        if past {
            ui.remove(cell);
            self.count -= 1;
            return true;
        }
        ui.list_multi_insert(list, cell, self.max_size as usize - 1);
        self.count -= 1;
        true
    }

    /// A request about an item under way: it shows pending.
    // D2-02998
    pub fn begin_request(&mut self, ui: &mut Ui, item: u64, facts: &Facts) -> bool {
        let Some(i) = self.find_item(item) else {
            return false;
        };
        self.items[i].set_pending(ui, true, Some(facts));
        true
    }

    /// A request about an item failed: no longer pending.
    // D2-02999
    pub fn request_failed(&mut self, ui: &mut Ui, item: u64, facts: &Facts) -> bool {
        let Some(i) = self.find_item(item) else {
            return false;
        };
        self.items[i].set_pending(ui, false, Some(facts));
        true
    }

    /// The scripts' notice about the container: everything again, an item added at a slot of
    /// the segment, one removed, a request about one failed or under way; the cells laid out
    /// again when something changed.
    ///
    /// The client's own panel lays itself out as each notice comes, asking the scripts then. Here
    /// a frame's notices come together with the scripts' answers after all of them, so cells laid
    /// out from those answers already show an item added or removed before them: such a notice
    /// changes nothing (else an item given as the pack is sent again would show twice).
    // D2-03005
    pub fn update_container(&mut self, ui: &mut Ui, n: &Notice, facts: &Facts) {
        if n.long(0) != self.container || self.container == super::NO_OBJECT {
            return;
        }
        let item = n.long(3);
        let shown = self.laid_out == Some(facts.version);
        let changed = match n.word_at(2) {
            change::ALL => {
                self.get_contents(ui, facts);
                true
            }
            change::ADDED | change::REMOVED if shown => false,
            change::ADDED => {
                let slot = n.word_at(5);
                // The answers are the frame's end: an item coming back as another leaves (a swap
                // of what is wielded) was within the segment when it was added, though the
                // segment holds no more of them by now. Its cells reach at least that far.
                let (_, count) = facts
                    .containers
                    .get(&self.container)
                    .and_then(|c| c.slots.get(self.segment as usize))
                    .copied()
                    .unwrap_or((0, 0));
                let reach = count.max(self.max_size);
                if slot < self.first_slot || reach + self.first_slot <= slot {
                    return;
                }
                self.add_item(ui, item, slot - self.first_slot, facts);
                true
            }
            change::REMOVED => self.remove_item(ui, item),
            change::FAILED => self.request_failed(ui, item, facts),
            change::UNDER_WAY => self.begin_request(ui, item, facts),
            _ => false,
        };
        if changed {
            if let Some(list) = self.list {
                ui.list_changes_done(list);
            }
        }
    }

    /// Show `n` cells instead: the element told, and made again.
    // D2-03006
    pub fn resize(&mut self, ui: &mut Ui, n: u32, facts: &Facts, out: &mut Vec<gameplay::Request>) {
        if self.max_size == n {
            return;
        }
        let Some(root) = self.root else { return };
        ui.set_attr(
            root,
            attr::NUM_SLOTS,
            Value::Int(i32::try_from(n).unwrap_or(i32::MAX)),
        );
        let (c, first, seg) = (self.container, self.first_slot, self.segment);
        self.init(ui, c, Some(root), first, seg, facts, out);
    }

    /// Shown or hidden: its element with it.
    // D2-02986
    pub fn show(&mut self, ui: &mut Ui, on: bool) {
        ui.frame_show(self.frame, on);
        if let Some(r) = self.root {
            ui.set_visible(r, on);
        }
    }

    /// The slot under the pointer: the segment's first slot plus the cell's place, no further
    /// than just past the segment's last item.
    // D2-02989
    #[must_use]
    pub fn new_index(&self, ui: &Ui, facts: &Facts) -> u32 {
        let Some(list) = self.list else { return 0 };
        let mut at = 0;
        if let Some(cell) = ui.listbox_item_under_mouse(list) {
            let (_, count) = facts.segment(self.segment);
            let n = ui
                .list_index_of(list, cell)
                .and_then(|i| u32::try_from(i).ok())
                .unwrap_or(0);
            at = n.min(count);
        }
        self.first_slot + at
    }

    /// What the cells heard: an element dropped on them (taken), from the money bag (coins),
    /// from another panel of the player's, or an item.
    // D2-02994
    pub fn listen(
        &mut self,
        ui: &mut Ui,
        mail: &Mail,
        facts: &Facts,
        out: &mut Vec<gameplay::Request>,
    ) {
        let Mail::Element {
            from,
            element,
            what,
            param,
        } = *mail
        else {
            return;
        };
        let Some(list) = self.list else { return };
        if what != message::STOP_DRAG_ITEM
            || element != self.list_name
            || from != list
            || param == 0
        {
            return;
        }
        let Some(dropped) = ui.dropped() else { return };
        let Some(n) = ui.node(dropped) else { return };
        if n.id == name::COINS_FIELD {
            self.drag_from_money_bag(ui, facts, out);
            return;
        }
        match n.int_attr(attr::SOURCE_CONTEXT) {
            None => self.list_box_drop(ui, dropped, facts, out),
            Some(context) => {
                if facts.player == self.container {
                    self.drag_from(ui, dropped, context, facts, out);
                }
            }
        }
    }

    /// An item from another panel dropped: that panel told the item and the slot.
    // D2-02990
    fn drag_from(
        &mut self,
        ui: &Ui,
        dropped: Handle,
        context: i32,
        facts: &Facts,
        out: &mut Vec<gameplay::Request>,
    ) {
        let item = ui
            .node(dropped)
            .and_then(|n| n.instance_id_attr(attr::SHORTCUT_DATA))
            .unwrap_or(super::NO_OBJECT);
        let slot = self.new_index(ui, facts);
        let w = |v: u64| [(v >> 32) as u32, (v & 0xFFFF_FFFF) as u32];
        let [hi, lo] = w(item);
        #[allow(clippy::cast_sign_loss)]
        out.push(gameplay::Request::Notice(said(
            notice::DRAG_FROM,
            &[hi, lo, slot, context as u32],
        )));
    }

    /// An item dropped on the cells: worn, taken off into the slot (when this is the player's);
    /// else, unless it would stay where it is, moved there: a stack of more than one split first.
    // D2-02992
    fn list_box_drop(
        &mut self,
        ui: &Ui,
        dropped: Handle,
        facts: &Facts,
        out: &mut Vec<gameplay::Request>,
    ) {
        if self.list.is_none() || !facts.here(facts.player) {
            return;
        }
        let Some(n) = ui.node(dropped) else { return };
        let item = n
            .instance_id_attr(attr::SHORTCUT_DATA)
            .unwrap_or(super::NO_OBJECT);
        let from = n
            .instance_id_attr(attr::FROM_ID)
            .unwrap_or(super::NO_OBJECT);
        let equipped = n.bool_attr(attr::EQUIPPED);
        let new_index = self.new_index(ui, facts);
        let Some(it) = facts.item(item) else { return };
        let (stackable, quantity, max_stack) = (it.stackable, it.quantity, it.stack_size);
        if equipped {
            if self.container == facts.player {
                out.push(gameplay::Request::Inventory(Request::TakeOff {
                    item,
                    slot: new_index,
                }));
            }
            return;
        }
        let old_index = if from == super::NO_OBJECT {
            if self.container != facts.player {
                return;
            }
            0
        } else {
            n.int_attr(attr::OLD_INDEX)
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(0)
        };
        if old_index == new_index {
            return;
        }
        self.moving = item;
        self.moving_from = from;
        self.new_index = new_index;
        self.old_index = old_index;
        self.move_quantity = if max_stack < quantity {
            max_stack
        } else {
            quantity
        };
        if stackable && quantity > 1 {
            self.item_context = super::next_stack_context();
            out.push(gameplay::Request::Notice(init_stack(
                self.item_context,
                self.move_quantity,
                self.move_quantity,
                from,
                self.container,
                item,
            )));
            return;
        }
        self.finish_list_box_drop(quantity, facts, out);
    }

    /// The move asked for: unless it stays where it was, `quantity` moved to the slot (a whole
    /// stack moved further on in its own container lands one slot sooner, the slot it left
    /// emptied).
    // D2-02984
    pub fn finish_list_box_drop(
        &mut self,
        quantity: u32,
        facts: &Facts,
        out: &mut Vec<gameplay::Request>,
    ) {
        if !facts.here(facts.player) {
            return;
        }
        if self.moving_from != self.container || self.old_index != self.new_index {
            if self.moving_from == self.container {
                let (_, count) = facts.segment(self.segment);
                if count <= self.new_index.wrapping_sub(self.first_slot)
                    && self.move_quantity == quantity
                {
                    self.new_index -= 1;
                }
            }
            out.push(gameplay::Request::Inventory(Request::Put {
                item: self.moving,
                from: self.moving_from,
                to: self.container,
                old_slot: self.old_index,
                new_slot: self.new_index,
                quantity,
            }));
        }
        self.moving = super::NO_OBJECT;
        self.moving_from = super::NO_OBJECT;
        self.old_index = 0;
        self.new_index = 0;
        self.move_quantity = 0;
    }

    /// Coins dropped from the money bag: the stack splitter asked how many (at most the player's
    /// money, and at most a stack), for the slot under the pointer.
    // D2-02993
    fn drag_from_money_bag(&mut self, ui: &Ui, facts: &Facts, out: &mut Vec<gameplay::Request>) {
        if !facts.here(facts.player) {
            return;
        }
        self.new_index = self.new_index(ui, facts);
        self.moving = super::NO_OBJECT;
        self.moving_from = super::NO_OBJECT;
        self.old_index = 0;
        self.coins_context = super::next_stack_context();
        let money = u32::try_from(facts.money).unwrap_or(0);
        let amount = money.min(facts.coin_stack);
        out.push(gameplay::Request::Notice(init_stack(
            self.coins_context,
            amount,
            amount,
            super::NO_OBJECT,
            self.container,
            super::NO_OBJECT,
        )));
    }

    /// The stack splitter's answer: for a dropped item, the move with that many; for coins, that
    /// many taken out of the money bag into the slot.
    // D2-02988
    pub fn stack_done(
        &mut self,
        context: u32,
        amount: u32,
        facts: &Facts,
        out: &mut Vec<gameplay::Request>,
    ) {
        if context == self.item_context {
            self.finish_list_box_drop(amount, facts, out);
        }
        if context == self.coins_context && facts.here(facts.player) {
            out.push(gameplay::Request::Inventory(Request::OutOfMoney {
                amount,
                slot: self.new_index,
                container: self.container,
            }));
        }
    }

    /// The notices it hears: the splitter's answer.
    pub fn notice(&mut self, n: &Notice, facts: &Facts, out: &mut Vec<gameplay::Request>) {
        if n.native == notice::STACK_DONE {
            self.stack_done(n.word_at(0), n.word_at(1), facts, out);
        }
    }

    /// A frame: drops on its cells and presses on its items; the scripts' notices about the
    /// container and the player; its items shown again when the scripts' answers changed.
    pub fn update(
        &mut self,
        ui: &mut Ui,
        notices: &[Notice],
        facts: &Facts,
        changed: bool,
        out: &mut Vec<gameplay::Request>,
    ) {
        for m in super::take(ui, self.frame) {
            self.listen(ui, &m, facts, out);
        }
        for d in &mut self.items {
            d.take(ui, out);
        }
        for n in notices {
            if n.native == notice::UPDATE_CONTAINER && n.about == Some(self.container) {
                self.update_container(ui, n, facts);
            }
            for d in &mut self.items {
                d.notice(ui, n, facts.player);
            }
        }
        if changed {
            for d in &mut self.items {
                d.refresh(ui, Some(facts));
            }
        }
    }

    /// It goes: everything let go, its framework.
    pub fn close(mut self, ui: &mut Ui, out: &mut Vec<gameplay::Request>) {
        self.cleanup(ui, out);
        ui.drop_frame(self.frame);
    }
}
