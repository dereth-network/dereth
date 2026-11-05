//! The stack splitter: how many of a stack (or of the player's coins) to move.
//!
//! [`StackSplitter`] comes up centered on the pointer, kept on the screen, when another panel
//! asks for an amount: its label names the item (or the coins), its entry holds the amount
//! offered and takes only digits. The toggle halves the amount (on) or gives the whole (off). OK
//! with a whole number no greater than the most allowed closes it and tells the panel that asked;
//! anything else shows why under the entry ("a whole number, please", "at most so many"). Cancel
//! closes it. Asked again, the split before is closed first; it closes on its own when the panel
//! that asked calls it off, when the object it watches closes as a container, or goes.

use std::any::Any;

use auberean_ui::framework::FrameId;
use auberean_ui::token::msg;
use auberean_ui::types::text::number_input_filter;
use auberean_ui::{Handle, Mail, Token, Ui};

use super::{notice, said, Facts, PLAY_LAYOUT_ENUM};
use crate::gameplay::{self, Child, Notice, View};

mod name {
    use auberean_ui::Token;
    pub const STACKABLE_FIELD: Token = Token::of("StackableField");
    pub const LABEL: Token = Token::of("Label");
    pub const AMOUNT_ENTRY: Token = Token::of("AmountEntry");
    pub const ERROR_TEXT: Token = Token::of("ErrorText");
    pub const STACK_OK_BUTTON: Token = Token::of("StackOkButton");
    pub const STACK_CANCEL_BUTTON: Token = Token::of("StackCancelButton");
    pub const AMOUNT_TOGGLE_BUTTON: Token = Token::of("AmountToggleButton");
    /// The toggle's attribute.
    pub const HIGHLIGHTED: Token = Token::of("highlighted");
    /// Why OK was refused.
    pub const UINT_PLEASE: &str = "ID_Stack_UIntPlease";
    pub const MAX_QUANTITY: &str = "ID_Stack_MaxQuantity";
}

/// The stack splitter.
#[derive(Debug)]
pub struct StackSplitter {
    frame: FrameId,
    root: Option<Handle>,
    label: Option<Handle>,
    entry: Option<Handle>,
    error: Option<Handle>,
    /// The amount offered and the most allowed.
    amount: u32,
    max: u32,
    /// The object whose loss calls it off, and the other object of the request.
    watch: u64,
    other: u64,
    /// The context of the request it answers (0: none).
    context: u32,
    facts: Facts,
}

impl StackSplitter {
    /// Its window made from the play layout, its label, entry (digits only) and error line
    /// found; its buttons heard; hidden.
    // D2-02242
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        let root = ui.frame_create_root(frame, PLAY_LAYOUT_ENUM, name::STACKABLE_FIELD);
        let find = |ui: &Ui, n: Token| root.and_then(|r| ui.child_recursive(r, n));
        let label = find(ui, name::LABEL);
        let entry = find(ui, name::AMOUNT_ENTRY);
        if let Some(e) = entry {
            ui.set_input_filter(e, Some(number_input_filter));
        }
        let error = find(ui, name::ERROR_TEXT);
        if let Some(l) = ui.frame_listener(frame) {
            for b in [
                name::STACK_OK_BUTTON,
                name::STACK_CANCEL_BUTTON,
                name::AMOUNT_TOGGLE_BUTTON,
            ] {
                ui.listen_for_message(l, b, msg::BUTTON_RELEASE_ON);
            }
        }
        let mut me = Self {
            frame,
            root,
            label,
            entry,
            error,
            amount: 0,
            max: 0,
            watch: super::NO_OBJECT,
            other: super::NO_OBJECT,
            context: 0,
            facts: Facts::default(),
        };
        me.show(ui, false, &mut Vec::new());
        me
    }

    /// The context of the request it shows (0: none).
    #[must_use]
    pub const fn context(&self) -> u32 {
        self.context
    }

    /// Centered on the pointer, kept on the screen.
    // D2-02240
    pub fn position_window(&self, ui: &mut Ui) {
        let Some(r) = self.root else { return };
        let Some(rect) = ui.node(r).map(|n| n.rect) else {
            return;
        };
        let (w, h) = (rect.width(), rect.height());
        let (px, py) = ui.pointer();
        let (dw, dh) = ui.display();
        let mut x = px - w / 2;
        let mut y = py - h / 2;
        if x < 0 {
            x = 0;
        } else if dw < x + w {
            x = dw - w;
        }
        if y < 0 {
            y = 0;
        } else if dh < y + h {
            y = dh - h;
        }
        ui.move_to(r, x, y);
    }

    /// Closed: with `done`, the panel that asked told the amount; the watched object let go,
    /// the request forgotten; hidden.
    // D2-02241, D2-08106
    pub fn close_with(&mut self, ui: &mut Ui, done: Option<u32>, out: &mut Vec<gameplay::Request>) {
        if let Some(amount) = done {
            out.push(gameplay::Request::Notice(said(
                notice::STACK_DONE,
                &[self.context, amount],
            )));
        }
        self.context = 0;
        self.watch = super::NO_OBJECT;
        self.other = super::NO_OBJECT;
        self.show(ui, false, out);
    }

    /// Another panel asks for an amount: the split before closed; the request kept; the error
    /// cleared, the label written, the toggle off, the amount offered in the entry; placed at the
    /// pointer; the watched object listened to; shown.
    // D2-02252
    pub fn init(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<gameplay::Request>) {
        if self.context != 0 {
            self.close_with(ui, None, out);
        }
        self.context = n.word_at(0);
        self.amount = n.word_at(1);
        self.max = n.word_at(2);
        self.watch = n.long(3);
        self.other = n.long(5);
        let label_item = n.long(7);
        if let Some(e) = self.error {
            ui.clear_all_text(e);
        }
        // A label the asking panel wrote (a shop's) before the item's name.
        let written = n.records.iter().flatten().find_map(|r| match r {
            gameplay::Record::Text(t) => ui.assets().write(t),
            _ => None,
        });
        if let Some(l) = self.label {
            let text = if let Some(w) = written {
                w
            } else if label_item == super::NO_OBJECT {
                super::write(ui, super::container::MONEY_STACK, &[])
            } else {
                self.facts
                    .item(label_item)
                    .map(|i| i.name.clone())
                    .unwrap_or_default()
            };
            ui.set_text(l, &text);
        }
        if let Some(t) = self
            .root
            .and_then(|r| ui.child_recursive(r, name::AMOUNT_TOGGLE_BUTTON))
        {
            ui.set_attr(t, name::HIGHLIGHTED, auberean_ui::Value::Bool(false));
        }
        self.set_amount(ui, self.amount);
        self.position_window(ui);
        self.show(ui, true, out);
    }

    /// The entry holds `amount`, written as `%d` writes it: signed, so from 2^31 up it shows
    /// negative.
    // D2-02250
    pub fn set_amount(&self, ui: &mut Ui, amount: u32) {
        if let Some(e) = self.entry {
            ui.set_text(e, &amount.cast_signed().to_string());
        }
    }

    /// OK: a whole number no greater than the most closes it with that amount; else why, under
    /// the entry.
    // D2-02249
    pub fn ok(&mut self, ui: &mut Ui, out: &mut Vec<gameplay::Request>) {
        let text = self.entry.and_then(|e| ui.text(e)).unwrap_or_default();
        let why = match parse_whole(&text) {
            Some(v) if v <= self.max => {
                self.close_with(ui, Some(v), out);
                return;
            }
            Some(_) => super::write(
                ui,
                name::MAX_QUANTITY,
                &[("size", auberean_text::Value::UInt(self.max))],
            ),
            None => super::write(ui, name::UINT_PLEASE, &[]),
        };
        if let Some(e) = self.error {
            ui.set_text(e, &why);
        }
    }
}

/// A whole number as the client reads one: after any spaces and a sign, `0x` and hexadecimal
/// digits, a leading `0` and octal digits, or decimal digits; the whole text read, and no
/// greater than 32 bits hold (a minus sign counts back from 2^32). Nothing at all reads as 0.
// D2-02247
fn parse_whole(s: &str) -> Option<u32> {
    let body = s.trim_start_matches([' ', '\t', '\n', '\r', '\u{b}', '\u{c}']);
    let (neg, rest) = match body.chars().next() {
        Some('-') => (true, &body[1..]),
        Some('+') => (false, &body[1..]),
        _ => (false, body),
    };
    let hex = rest.len() > 2
        && (rest.starts_with("0x") || rest.starts_with("0X"))
        && rest[2..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_hexdigit());
    let (radix, digits) = if hex {
        (16, &rest[2..])
    } else if rest.starts_with('0') {
        (8, rest)
    } else {
        (10, rest)
    };
    if digits.is_empty() {
        // Nothing read: the whole text is read only when there was none.
        return s.is_empty().then_some(0);
    }
    if !digits.chars().all(|c| c.is_digit(radix)) {
        return None;
    }
    let v = u32::try_from(u64::from_str_radix(digits, radix).ok()?).ok()?;
    Some(if neg { v.wrapping_neg() } else { v })
}

impl StackSplitter {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[0x458, 0x459];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for StackSplitter {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Shown or hidden: its window with it.
    // D2-02239
    fn show(&mut self, ui: &mut Ui, on: bool, _out: &mut Vec<gameplay::Request>) {
        ui.frame_show(self.frame, on);
        if let Some(r) = self.root {
            let shown = ui.frame_shown(self.frame);
            ui.set_visible(r, shown);
        }
    }

    /// OK, Cancel, and the toggle (on: half the most; off: the most), the error cleared.
    // D2-02251
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<gameplay::Request>) {
        let Mail::Element {
            from,
            element,
            what,
            ..
        } = *mail
        else {
            return;
        };
        if what != msg::BUTTON_RELEASE_ON {
            return;
        }
        if element == name::STACK_OK_BUTTON {
            self.ok(ui, out);
        } else if element == name::STACK_CANCEL_BUTTON {
            self.close_with(ui, None, out);
        } else if element == name::AMOUNT_TOGGLE_BUTTON {
            let half = ui
                .node(from)
                .is_some_and(|n| n.bool_attr(name::HIGHLIGHTED));
            let amount = if half {
                u32::try_from(dereth_primitives::num::to_i32_f64(
                    f64::from(self.max) * 0.5,
                ))
                .unwrap_or(0)
            } else {
                self.max
            };
            self.set_amount(ui, amount);
            if let Some(e) = self.error {
                ui.clear_all_text(e);
            }
        }
    }

    /// Asked for an amount; called off (its request's context).
    // D2-02244
    fn notice(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<gameplay::Request>) {
        match n.native {
            notice::INIT_STACK => self.init(ui, n, out),
            notice::CLOSE_STACK if n.word() != 0 && n.word() == self.context => {
                self.close_with(ui, None, out);
            }
            _ => {}
        }
    }

    /// The watched object closed as a container, or gone: closed.
    // D2-02245, D2-02246
    fn update(&mut self, ui: &mut Ui, view: &View<'_>, out: &mut Vec<gameplay::Request>) {
        if let Some(f) = view.inventory {
            if f.version != self.facts.version || self.facts.player != f.player {
                self.facts = f.clone();
            }
        }
        if self.context == 0 || self.watch == super::NO_OBJECT {
            return;
        }
        for n in view.everyone {
            if n.native == notice::CONTAINER_STATE_UPDATE
                && n.about == Some(self.watch)
                && n.long(0) == self.watch
                && n.word_at(2) & 0xFF == 0
            {
                self.close_with(ui, None, out);
                return;
            }
        }
        if view.inventory.is_some() && !self.facts.here(self.watch) {
            self.close_with(ui, None, out);
        }
    }

    /// It goes: closed, its window gone.
    // D2-02243, D2-02248
    fn close(mut self: Box<Self>, ui: &mut Ui) {
        self.close_with(ui, None, &mut Vec::new());
        if let Some(r) = self.root.take() {
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
