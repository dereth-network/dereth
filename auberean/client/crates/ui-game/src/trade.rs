//! The trade window: two players' offers side by side, each accepted or not.
//!
//! The scripts register a trade between the player and a partner. Asked by the partner, the
//! player gets a question ("... wants to trade with you"); saying yes opens the trade, no closes
//! it. Asking, the player's window shows waiting for the partner. Opened, the window shows the
//! partner's name over the two lists:
//!
//! - an item dropped on a list from the player's pack (not one worn) is offered: a stack of more
//!   than one asks how many first, up to the most a stack holds;
//! - an item dragged out of the player's own list and dropped nowhere is taken back;
//! - each side's offers show as the scripts say they were made and taken back, the player's own
//!   draggable;
//! - accept and decline (each ghosted once pressed until the scripts say otherwise) mark each
//!   side's button accepted or not; a change clears both;
//! - close or cancel closes the trade.
//!
//! When the trade closes the lists empty. Closed by the player, the window hides; by the partner,
//! it says so for a while (the window's own delay), then hides.

use std::any::Any;

use auberean_text::{StringInfo, Value as Var};
use auberean_ui::framework::FrameId;
use auberean_ui::manager::global;
use auberean_ui::system::Value;
use auberean_ui::token::{attr, msg, state};
use auberean_ui::{Handle, Mail, Token, Ui};

use crate::gameplay::{self, Child, Notice, Record, Request, View, PLAY_LAYOUT_ENUM};
use crate::inventory::{self, item::ItemCell, Facts, ITEM_SHORTCUT, NO_OBJECT};

/// The scripts' notices to the trade window, by the native that sends each.
pub mod notice {
    /// A trade was registered: who asked (high and low), with whom (high and low), the partner's
    /// name (the record).
    pub const REGISTERED: u32 = 0x0090 << 16 | 0x0001;
    /// The trade opened.
    pub const OPENED: u32 = 0x0090 << 16 | 0x0002;
    /// The trade closed: who closed it (high and low).
    pub const CLOSED: u32 = 0x0090 << 16 | 0x0003;
    /// Someone accepted, or declined (high and low; none: the player's own accept may be pressed
    /// again).
    pub const ACCEPTED: u32 = 0x0090 << 16 | 0x0004;
    pub const DECLINED: u32 = 0x0090 << 16 | 0x0005;
    /// The offers were emptied.
    pub const RESET: u32 = 0x0090 << 16 | 0x0006;
    /// Both sides' acceptance was cleared.
    pub const ACCEPTANCE_CLEARED: u32 = 0x0090 << 16 | 0x0007;
    /// Someone offered an item: who (high and low), the item (high and low), how many.
    pub const OFFERED: u32 = 0x0090 << 16 | 0x0008;
    /// Someone took an item back: who (high and low), the item (high and low).
    pub const REVOKED: u32 = 0x0090 << 16 | 0x0009;
}

mod name {
    use auberean_ui::Token;
    pub const FIELD: Token = Token::of("SecureTradeField");
    pub const CLOSE: Token = Token::of("SecureTradeCloseButton");
    pub const CANCEL: Token = Token::of("SecureTradeCancelButton");
    pub const ACCEPT: Token = Token::of("AcceptTradeButton");
    pub const DECLINE: Token = Token::of("DeclineTradeButton");
    pub const LIST_SELF: Token = Token::of("TradeListBoxSelf");
    pub const LIST_OTHER: Token = Token::of("TradeListBoxOther");
    pub const ITEM_TEMPLATE: Token = Token::of("TradeItemTemplate");
    pub const ITEM_ICON: Token = Token::of("ItemIcon");
    pub const SELF: Token = Token::of("Self");
    pub const OTHER: Token = Token::of("Other");
    pub const BUTTON_INNER: Token = Token::of("TradeButtonInner");
    pub const PARTNER_LABEL: Token = Token::of("PartnerLabel");
    pub const ENDED_TEXT: Token = Token::of("EndedText");
    pub const STOP_DRAG_ITEM: Token = Token::of("stop_drag_item");
    /// The window's states.
    pub const TRADE: Token = Token::of("trade");
    pub const ACCEPT_PENDING: Token = Token::of("acceptpending");
    pub const PARTNER_CLOSED: Token = Token::of("partnerclosed");
    /// A side's button's states.
    pub const ACCEPTED: Token = Token::of("accepted");
    pub const NOT_ACCEPTED: Token = Token::of("notaccepted");
    /// How long the partner's closing is shown, seconds.
    pub const DELAY: Token = Token::of("delay");
    /// The text variables.
    pub const VALUE: Token = Token::of("value");
    pub const NAME: Token = Token::of("name");
}

/// What the trade window asks of the player's scripts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TradeRequest {
    /// Open the trade with the partner (yes to the question).
    Open(u64),
    /// Close the trade.
    Close,
    Accept,
    Decline,
    /// Offer so many of an item.
    Offer {
        item: u64,
        quantity: u32,
    },
    /// Take an offered item back.
    Revoke(u64),
}

/// Only an item may be dropped on a list.
// D2-02153
fn drop_check(ui: &Ui, item: Handle, _list: Handle) -> bool {
    ui.node(item)
        .is_some_and(|n| n.id_attr(inventory::attr::SHORTCUT_TYPE) == Some(ITEM_SHORTCUT))
}

/// The trade window.
#[derive(Debug)]
pub struct Trade {
    frame: FrameId,
    root: Option<Handle>,
    /// The partner, and the player.
    partner: u64,
    me: u64,
    partner_name: StringInfo,
    /// The question up (0: none).
    dialog: u32,
    /// Each side's offers.
    mine: Vec<ItemCell>,
    theirs: Vec<ItemCell>,
    /// The split asked for an item to offer, and the item.
    stack_context: u32,
    offering: u64,
    /// The partner closed the trade: shown until this time (the interface's clock).
    ended: bool,
    end_time: f64,
    facts: Option<Facts>,
}

impl Trade {
    /// The window from the play layout, its buttons, lists, the clock, the questions' answers
    /// and the splits heard; hidden.
    // D2-02172
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        let root = ui.frame_create_root(frame, PLAY_LAYOUT_ENUM, name::FIELD);
        let mut me = Self {
            frame,
            root,
            partner: NO_OBJECT,
            me: NO_OBJECT,
            partner_name: StringInfo::default(),
            dialog: 0,
            mine: Vec::new(),
            theirs: Vec::new(),
            stack_context: 0,
            offering: NO_OBJECT,
            ended: false,
            end_time: 0.0,
            facts: None,
        };
        if root.is_some() {
            if let Some(l) = ui.frame_listener(frame) {
                ui.listen_for_global_message(l, global::TIME);
                for b in [name::CLOSE, name::CANCEL, name::ACCEPT, name::DECLINE] {
                    ui.listen_for_message(l, b, msg::BUTTON_RELEASE_ON);
                }
                for e in [name::LIST_SELF, name::LIST_OTHER, name::ITEM_TEMPLATE] {
                    ui.listen_for_message(l, e, name::STOP_DRAG_ITEM);
                }
            }
            me.show(ui, false, &mut Vec::new());
        }
        me
    }

    /// The partner, and whether the partner closed the trade and it still says so.
    #[must_use]
    pub const fn partner(&self) -> (u64, bool) {
        (self.partner, self.ended)
    }

    /// The items each side offers, the player's first.
    #[must_use]
    pub fn offers(&self) -> (Vec<u64>, Vec<u64>) {
        let ids = |v: &[ItemCell]| v.iter().map(ItemCell::item).collect();
        (ids(&self.mine), ids(&self.theirs))
    }

    /// The question up (0: none).
    #[must_use]
    pub const fn dialog(&self) -> u32 {
        self.dialog
    }

    fn side(&self, who: u64) -> Token {
        if who == self.me {
            name::SELF
        } else {
            name::OTHER
        }
    }

    fn button_inner(&self, ui: &Ui, side: Token) -> Option<Handle> {
        let r = self.root?;
        let s = ui.child_recursive(r, side)?;
        ui.child_recursive(s, name::BUTTON_INNER)
    }

    /// The trade closes for the player: closed when the partner had closed it (the window hidden),
    /// else the scripts asked to close it.
    // D2-02149
    fn close_trade(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        if self.ended {
            self.show(ui, false, out);
            self.ended = false;
        } else {
            out.push(Request::Trade(TradeRequest::Close));
        }
    }

    /// The split done: the item offered, so many.
    // D2-02150
    fn finish_list_box_drop(&mut self, amount: u32, out: &mut Vec<Request>) {
        out.push(Request::Trade(TradeRequest::Offer {
            item: self.offering,
            quantity: amount,
        }));
        self.offering = NO_OBJECT;
    }

    /// A side's button: accepted or not.
    // D2-02151
    fn update_button(&self, ui: &mut Ui, who: u64, accepted: bool) {
        let Some(b) = self.button_inner(ui, self.side(who)) else {
            return;
        };
        ui.set_state(b, state::STATELESS);
        let s = if accepted {
            name::ACCEPTED
        } else {
            name::NOT_ACCEPTED
        };
        ui.set_state(b, s);
    }

    /// An offered item dragged out and dropped nowhere: taken back.
    // D2-02152
    fn handle_item_removal(ui: &Ui, dropped: Handle, out: &mut Vec<Request>) {
        let item = ui
            .node(dropped)
            .and_then(|n| n.instance_id_attr(inventory::attr::SHORTCUT_DATA))
            .unwrap_or(NO_OBJECT);
        out.push(Request::Trade(TradeRequest::Revoke(item)));
    }

    /// An item dropped on a list: one from the player's pack, not worn, is offered (a stack of
    /// more than one: how many asked first, up to the most a stack holds).
    // D2-02161
    fn handle_list_box_drop(&mut self, ui: &Ui, dropped: Handle, out: &mut Vec<Request>) {
        let Some(n) = ui.node(dropped) else { return };
        let item = n
            .instance_id_attr(inventory::attr::SHORTCUT_DATA)
            .unwrap_or(NO_OBJECT);
        let from = n
            .instance_id_attr(inventory::attr::FROM_ID)
            .unwrap_or(NO_OBJECT);
        let worn = n.bool_attr(inventory::attr::EQUIPPED);
        if from != self.me || worn {
            return;
        }
        self.offering = item;
        let Some(it) = self.facts.as_ref().and_then(|f| f.item(item)) else {
            return;
        };
        let quantity = it.quantity;
        if it.stackable && quantity > 1 {
            self.stack_context = inventory::next_stack_context();
            let most = it.stack_size.min(quantity);
            out.push(Request::Notice(inventory::init_stack(
                self.stack_context,
                most,
                most,
                self.me,
                NO_OBJECT,
                item,
            )));
            return;
        }
        self.finish_list_box_drop(quantity, out);
    }

    /// The question whether to trade with the partner (unless one is up).
    // D2-02164
    fn make_accept_trade_dialog(
        &mut self,
        ui: &Ui,
        name_info: &StringInfo,
        out: &mut Vec<Request>,
    ) {
        if self.dialog != 0 {
            return;
        }
        let table = ui
            .assets()
            .data_id_by_enum(crate::flow::MESSAGES, crate::flow::TABLE_MAPPING);
        let mut s = StringInfo::entry(table, Token::of("ID_SecureTrade_AcceptTradeText").raw());
        let partner: Vec<u16> = ui
            .assets()
            .write(name_info)
            .unwrap_or_default()
            .encode_utf16()
            .collect();
        s.add_variable_string(name::VALUE.raw(), &partner);
        let id = crate::dialog::new_context();
        out.push(Request::Dialog(gameplay::Dialog {
            id,
            kind: gameplay::dialog::CONFIRMATION,
            text: ui.assets().write(&s).unwrap_or_default(),
            modal: false,
            info: None,
        }));
        self.dialog = id;
    }

    /// The question answered: yes opens the trade; no closes it (or, when the partner had closed
    /// it, hides the window).
    // D2-02160
    fn do_accept_trade(&mut self, ui: &mut Ui, yes: bool, out: &mut Vec<Request>) {
        if yes {
            out.push(Request::Trade(TradeRequest::Open(self.partner)));
        } else if !self.ended {
            out.push(Request::Trade(TradeRequest::Close));
        } else {
            self.show(ui, false, out);
            self.ended = false;
        }
    }

    /// A trade registered: the partner (the one of the two who is not the player) and their
    /// name; a window still saying the last trade ended hidden; asked by the player, the window
    /// shows waiting; asked by the partner, the question.
    // D2-02165
    fn registered(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<Request>) {
        let (asker, other) = (n.long(0), n.long(2));
        self.partner = if asker == self.me { other } else { asker };
        self.partner_name = text_record(n).unwrap_or_default();
        if self.shown(ui) && self.ended {
            self.ended = false;
            self.show(ui, false, out);
        }
        if asker == self.me {
            self.show(ui, true, out);
            if let Some(r) = self.root {
                ui.set_state(r, name::ACCEPT_PENDING);
            }
            return;
        }
        let who = self.partner_name.clone();
        self.make_accept_trade_dialog(ui, &who, out);
    }

    /// The trade opened: shown, the lists taking items, the partner's name.
    // D2-02159
    fn opened(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        self.show(ui, true, out);
        let Some(r) = self.root else { return };
        ui.set_state(r, name::TRADE);
        let list = |ui: &Ui, side, l| {
            ui.child_recursive(r, side)
                .and_then(|s| ui.child_recursive(s, l))
        };
        let Some(mine) = list(ui, name::SELF, name::LIST_SELF) else {
            return;
        };
        ui.set_drag_checks(mine, None, Some(drop_check));
        let Some(theirs) = list(ui, name::OTHER, name::LIST_OTHER) else {
            return;
        };
        ui.set_drag_checks(theirs, None, Some(drop_check));
        if let Some(p) = ui.child_recursive(r, name::PARTNER_LABEL) {
            let t = ui.assets().write(&self.partner_name).unwrap_or_default();
            ui.set_text(p, &t);
        }
    }

    /// An item offered: in the offerer's list (once), the player's own draggable, showing how many.
    // D2-02166, D2-02167
    fn add_item(&mut self, ui: &mut Ui, who: u64, item: u64, quantity: u32) {
        let mine = who == self.me;
        let (side, list_name) = if mine {
            (name::SELF, name::LIST_SELF)
        } else {
            (name::OTHER, name::LIST_OTHER)
        };
        let Some(list) = self
            .root
            .and_then(|r| ui.child_recursive(r, side))
            .and_then(|s| ui.child_recursive(s, list_name))
        else {
            return;
        };
        let cells = if mine { &self.mine } else { &self.theirs };
        if cells.iter().any(|c| c.item() == item) {
            return;
        }
        let Some(row) = ui.frame_create_element(PLAY_LAYOUT_ENUM, name::ITEM_TEMPLATE) else {
            return;
        };
        ui.list_add(list, row);
        let facts = self.facts.as_ref();
        let mut cell = ItemCell::new(ui, facts);
        cell.init(ui, row, name::ITEM_ICON, item, NO_OBJECT, false, facts);
        cell.set_draggable(ui, mine, facts);
        cell.set_quantity(ui, quantity, facts);
        cell.set_shortcut_type(ui, 0, facts);
        if mine {
            self.mine.insert(0, cell);
        } else {
            self.theirs.insert(0, cell);
        }
    }

    /// An item taken back: out of the offerer's list.
    // D2-02168, D2-02169
    fn remove_item(&mut self, ui: &mut Ui, who: u64, item: u64) {
        let mine = who == self.me;
        let (side, list_name) = if mine {
            (name::SELF, name::LIST_SELF)
        } else {
            (name::OTHER, name::LIST_OTHER)
        };
        let Some(list) = self
            .root
            .and_then(|r| ui.child_recursive(r, side))
            .and_then(|s| ui.child_recursive(s, list_name))
        else {
            return;
        };
        let cells = if mine {
            &mut self.mine
        } else {
            &mut self.theirs
        };
        let Some(i) = cells.iter().position(|c| c.item() == item) else {
            return;
        };
        let cell = cells.remove(i);
        let row = cell
            .icon()
            .and_then(|ic| ui.node(ic))
            .and_then(|n| n.parent);
        cell.close(ui);
        if let Some(row) = row {
            ui.list_delete(list, row);
        }
    }

    /// A side emptied: its offers let go, its list flushed, its button not accepted.
    // D2-02170, D2-02171
    fn reset_internal(&mut self, ui: &mut Ui, side: Token) {
        let cells = if side == name::SELF {
            std::mem::take(&mut self.mine)
        } else {
            std::mem::take(&mut self.theirs)
        };
        for c in cells {
            c.close(ui);
        }
        let Some(s) = self.root.and_then(|r| ui.child_recursive(r, side)) else {
            return;
        };
        let list_name = if side == name::SELF {
            name::LIST_SELF
        } else {
            name::LIST_OTHER
        };
        if let Some(l) = ui.child_recursive(s, list_name) {
            ui.list_flush(l);
            if let Some(b) = ui.child_recursive(s, name::BUTTON_INNER) {
                ui.set_state(b, name::NOT_ACCEPTED);
            }
        }
    }

    /// Both lists emptied while the window trades.
    fn reset_both(&mut self, ui: &mut Ui) {
        let trading = self
            .root
            .and_then(|r| ui.node(r))
            .is_some_and(|n| n.state == name::TRADE);
        if trading {
            self.reset_internal(ui, name::SELF);
            self.reset_internal(ui, name::OTHER);
        }
    }

    /// The trade closed: the question down, the lists emptied; closed by the player, hidden; by
    /// the partner, the window says so (with their name) for its delay.
    // D2-02175
    fn closed(&mut self, ui: &mut Ui, who: u64, out: &mut Vec<Request>) {
        out.push(Request::DialogDown(self.dialog));
        self.dialog = 0;
        self.reset_both(ui);
        if who == self.me {
            self.show(ui, false, out);
            return;
        }
        self.end_time = ui.now();
        if let Some(r) = self.root {
            ui.set_state(r, name::PARTNER_CLOSED);
            if let Some(t) = ui.child_recursive(r, name::ENDED_TEXT) {
                let table = ui
                    .assets()
                    .data_id_by_enum(crate::flow::MESSAGES, crate::flow::TABLE_MAPPING);
                let mut s = StringInfo::entry(table, Token::of("ID_SecureTrade_Ended").raw());
                s.add_var(
                    name::NAME.raw(),
                    Var::Text(Box::new(self.partner_name.clone())),
                );
                let text = ui.assets().write(&s).unwrap_or_default();
                ui.set_text(t, &text);
            }
            let delay = ui
                .node(r)
                .and_then(|n| n.float_attr(name::DELAY))
                .unwrap_or(0.0);
            self.end_time += f64::from(delay);
        }
        self.ended = true;
    }

    /// Both sides' acceptance cleared.
    // D2-02156
    fn acceptance_cleared(&self, ui: &mut Ui) {
        for side in [name::SELF, name::OTHER] {
            if let Some(b) = self.button_inner(ui, side) {
                ui.set_state(b, state::STATELESS);
                ui.set_state(b, name::NOT_ACCEPTED);
            }
        }
    }

    /// Someone accepted or declined: their button marked; none named, the player's own button
    /// may be pressed again.
    // D2-02157, D2-02158
    fn accepted(&self, ui: &mut Ui, who: u64, accepted: bool) {
        if who == NO_OBJECT {
            let button = if accepted {
                name::ACCEPT
            } else {
                name::DECLINE
            };
            if let Some(b) = self
                .root
                .and_then(|r| ui.child_recursive(r, name::SELF))
                .and_then(|s| ui.child_recursive(s, button))
            {
                ui.set_attr(b, attr::GHOSTED, Value::Bool(false));
            }
            return;
        }
        self.update_button(ui, who, accepted);
    }
}

/// The string reference a notice names.
fn text_record(n: &Notice) -> Option<StringInfo> {
    n.records.iter().flatten().find_map(|r| match r {
        Record::Text(t) => Some(t.clone()),
        _ => None,
    })
}

impl Trade {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[
        0x436, 0x45A, 0x437, 0x438, 0x439, 0x43A, 0x43B, 0x43C, 0x43D, 0x43E, 0x43F, 0x1_040C,
        0x1_040B, 0x408, 0x42B, 0x454, 0x1_040E,
    ];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for Trade {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Shown or hidden: its window as the framework shows.
    // D2-02178
    fn show(&mut self, ui: &mut Ui, on: bool, _out: &mut Vec<Request>) {
        ui.frame_show(self.frame, on);
        if let Some(r) = self.root {
            let shown = ui.frame_shown(self.frame);
            ui.set_visible(r, shown);
        }
    }

    /// The clock (the partner's closing shown long enough: closed); close and cancel (hidden
    /// when the partner had closed, else the trade closed); accept and decline (asked, the button
    /// ghosted); an item dropped on a list, or dragged out of one and dropped nowhere.
    // D2-02154, D2-02162
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<Request>) {
        match *mail {
            Mail::Global {
                what: global::TIME, ..
            } => {
                if self.ended && self.end_time < ui.now() {
                    self.close_trade(ui, out);
                }
            }
            Mail::Element {
                from,
                element,
                what,
                param,
            } => {
                if element == name::CLOSE || element == name::CANCEL {
                    if self.ended {
                        self.show(ui, false, out);
                        self.ended = false;
                    } else {
                        out.push(Request::Trade(TradeRequest::Close));
                    }
                    return;
                }
                if element == name::ACCEPT || element == name::DECLINE {
                    out.push(Request::Trade(if element == name::ACCEPT {
                        TradeRequest::Accept
                    } else {
                        TradeRequest::Decline
                    }));
                    ui.set_attr(from, attr::GHOSTED, Value::Bool(true));
                    return;
                }
                if what != name::STOP_DRAG_ITEM {
                    return;
                }
                let Some(dropped) = ui.dropped() else { return };
                if element == name::LIST_SELF || element == name::LIST_OTHER {
                    if param != 0 {
                        self.handle_list_box_drop(ui, dropped, out);
                    }
                } else if element == name::ITEM_TEMPLATE && param == 0 {
                    Self::handle_item_removal(ui, dropped, out);
                }
            }
            _ => {}
        }
    }

    /// The scripts' trade notices; a question answered; a split done; the selection and an item
    /// waiting for a target, which each offered item shows.
    // D2-02163, D2-02155, D2-02176
    fn notice(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<Request>) {
        match n.native {
            notice::REGISTERED => self.registered(ui, n, out),
            notice::OPENED => self.opened(ui, out),
            notice::CLOSED => self.closed(ui, n.long(0), out),
            notice::ACCEPTED => self.accepted(ui, n.long(0), true),
            notice::DECLINED => self.accepted(ui, n.long(0), false),
            notice::RESET => self.reset_both(ui),
            notice::ACCEPTANCE_CLEARED => self.acceptance_cleared(ui),
            notice::OFFERED => self.add_item(ui, n.long(0), n.long(2), n.word_at(4)),
            notice::REVOKED => self.remove_item(ui, n.long(0), n.long(2)),
            crate::dialog::notice::CLOSE_CONFIRMATION_DIALOG => {
                if let Some((id, yes)) = crate::dialog::confirmation_closed(n) {
                    if id == self.dialog {
                        self.do_accept_trade(ui, yes, out);
                    }
                }
            }
            inventory::notice::STACK_DONE if n.word_at(0) == self.stack_context => {
                self.finish_list_box_drop(n.word_at(1), out);
            }
            inventory::notice::SELECTION_CHANGED | inventory::notice::SET_TARGETING_POINTER => {
                for c in self.mine.iter_mut().chain(self.theirs.iter_mut()) {
                    c.notice(ui, n, self.me);
                }
            }
            _ => {}
        }
    }

    /// The offers' presses (an item selected, used or examined); the player, and what the scripts
    /// answer of the items shown.
    fn update(&mut self, ui: &mut Ui, view: &View<'_>, out: &mut Vec<Request>) {
        for c in self.mine.iter_mut().chain(self.theirs.iter_mut()) {
            c.take(ui, out);
        }
        let Some(f) = view.inventory else { return };
        self.me = f.player;
        if self.facts.as_ref().is_some_and(|o| o.version == f.version) {
            return;
        }
        self.facts = Some(f.clone());
        let facts = self.facts.as_ref();
        for c in self.mine.iter_mut().chain(self.theirs.iter_mut()) {
            c.refresh(ui, facts);
        }
    }

    /// It goes: its window gone (its question with it), the offers let go.
    // D2-02173, D2-02174
    fn close(mut self: Box<Self>, ui: &mut Ui) {
        for c in std::mem::take(&mut self.mine)
            .into_iter()
            .chain(std::mem::take(&mut self.theirs))
        {
            c.close(ui);
        }
        if let Some(r) = self.root {
            ui.frame_remove_root(self.frame, r);
            ui.remove(r);
        }
        ui.drop_frame(self.frame);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
