//! The key map ("Controls"), reached from the character list's Options: every action the player
//! can bind that has a name, a row each, its name and its two keys (a key held with Shift or Ctrl
//! written with it, `Ctrl - F5`). A key cell pressed waits for a key: the key pressed is bound
//! there (one bound to another action asks first; one the game keeps for itself is refused); Clear
//! empties the cell; Clear All empties every cell, Undo goes back to the maps as last applied, a
//! preset button (Defaults, WoW, FK, EQ, DAoC) puts its maps in place, each after asking; Apply
//! keeps the maps; Back returns to the character list (asking first when there are changes not
//! applied).
//!
//! The maps are the client's: the player's for keys alone, with Shift and with Ctrl, over the base
//! map the game keeps for itself; an action's keys are found from the bottom map up.

use std::any::Any;

use auberean_text::{StringInfo, Value as Var};
use auberean_ui::framework::FrameId;
use auberean_ui::token::{attr, msg};
use auberean_ui::{Handle, Mail, Token, Ui, Value};

use crate::flow::mode;
use crate::pregame::{self, message, Data, Notice, Request, Screen};

/// The layout the screen is built from, by enumeration value.
pub const LAYOUT_ENUM: u32 = 0x4100_0001;

mod name {
    use auberean_ui::Token;
    pub const KEYBOARD_FIELD: Token = Token::of("KeyboardField");
    pub const LIST_BOX: Token = Token::of("list_box");
    pub const CLEAR_BUTTON: Token = Token::of("clear_button");
    pub const CLEAR_ALL_BUTTON: Token = Token::of("clear_all_button");
    pub const SAVE_BUTTON: Token = Token::of("save_button");
    pub const UNDO_BUTTON: Token = Token::of("undo_button");
    pub const QUIT_BUTTON: Token = Token::of("quit_button");
    pub const ACTION_BOX_ITEM: Token = Token::of("action_box_item");
    pub const KEY_BOX_ITEM: Token = Token::of("key_box_item");
    /// A list item's own text element is named by this attribute.
    pub const TEXT_ID: Token = Token::of("textid");
    /// An action row's action.
    pub const ID: Token = Token::of("id");
    /// The element a refused key's reason is written to (none in this layout).
    pub const ERROR_TEXT: Token = Token::of("error_text");
}

/// The preset buttons: Defaults, WoW, FK, EQ, DAoC.
pub mod preset {
    use auberean_ui::Token;
    pub const DEFAULTS: Token = Token::of("restore_button");
    pub const WOW: Token = Token::of("restore_wow_button");
    pub const FK: Token = Token::of("restore_fk_button");
    pub const EQ: Token = Token::of("restore_eq_button");
    pub const DAOC: Token = Token::of("restore_daoc_button");
    /// Each with its maps' enumeration values: keys alone, with Shift, with Ctrl.
    pub const MAPS: [(Token, [u32; 3]); 5] = [
        (DEFAULTS, [0x4100_0001, 0x4100_0003, 0x4100_0002]),
        (WOW, [0x4100_000D, 0x4100_000F, 0x4100_000E]),
        (FK, [0x4100_000A, 0x4100_000C, 0x4100_000B]),
        (EQ, [0x4100_0004, 0x4100_0006, 0x4100_0005]),
        (DAOC, [0x4100_0007, 0x4100_0009, 0x4100_0008]),
    ];
}

/// The key cells' attributes: the key bound there and the modifier held with it.
mod cell {
    use auberean_ui::Token;
    pub const HIT_KEY: Token = Token(0xAE37_197F);
    pub const HIT_META: Token = Token(0x2A34_1964);
}

/// The input words of the keys that modify others: left and right Shift, left and right Ctrl.
pub mod meta {
    pub const LEFT_SHIFT: u32 = 0x8100_042A;
    pub const RIGHT_SHIFT: u32 = 0x8100_0436;
    pub const LEFT_CTRL: u32 = 0x8100_041D;
    pub const RIGHT_CTRL: u32 = 0x8100_049D;
}

/// Clear All's question (the data holds no words for it: it is asked empty).
const CLEAR_ALL_QUESTION: u32 = 0x1291_8D52;

/// The input words the screen never binds: the pointer's two axes.
const POINTER_X: u32 = 0x8200_0300;
const POINTER_Y: u32 = 0x8200_0304;
/// The left button: pressed while a cell waits, the cell stops waiting.
const LEFT_BUTTON: u32 = 0x8200_040C;

/// One key map: its bindings, an input word to an action each.
pub type KeyMap = Vec<(u32, u32)>;

/// The client's key maps: the base map the game keeps for itself, then the player's for keys
/// alone, with Shift, with Ctrl.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyMaps {
    pub base: KeyMap,
    pub normal: KeyMap,
    pub shift: KeyMap,
    pub control: KeyMap,
}

impl KeyMaps {
    /// The keys an action is bound to, each with the modifier its map is held with: from the
    /// bottom map up, each once.
    // D2-04954
    #[must_use]
    pub fn mapped_keys(&self, action: u32) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        for (map, m) in [
            (&self.base, 0),
            (&self.normal, 0),
            (&self.shift, meta::LEFT_SHIFT),
            (&self.control, meta::LEFT_CTRL),
        ] {
            for &(input, a) in map {
                if a == action && !out.contains(&(input, m)) {
                    out.push((input, m));
                }
            }
        }
        out
    }

    /// Whether the game keeps a key for itself (the base map binds it).
    #[must_use]
    pub fn reserved(&self, input: u32) -> bool {
        self.base.iter().any(|(i, _)| *i == input)
    }
}

/// What the key map screen shows and changes: the actions the player can bind (each with its
/// name, in the game's order) and those it cannot, the maps as last applied, and each preset's.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyData {
    pub actions: Vec<(u32, String)>,
    pub unnamed: Vec<u32>,
    pub maps: KeyMaps,
    pub presets: Vec<(Token, KeyMaps)>,
}

/// A key's name as the keyboard writes it (a US keyboard's), by its input word; none for a key
/// with no name.
#[must_use]
pub fn key_name(input: u32) -> Option<&'static str> {
    if !(0x8100_0401..=0x8100_04ED).contains(&input) {
        return None;
    }
    let code = input - 0x8100_0400;
    Some(match code {
        0x01 => "Esc",
        0x02 => "1",
        0x03 => "2",
        0x04 => "3",
        0x05 => "4",
        0x06 => "5",
        0x07 => "6",
        0x08 => "7",
        0x09 => "8",
        0x0A => "9",
        0x0B => "0",
        0x0C => "-",
        0x0D => "=",
        0x0E => "Backspace",
        0x0F => "Tab",
        0x10 => "Q",
        0x11 => "W",
        0x12 => "E",
        0x13 => "R",
        0x14 => "T",
        0x15 => "Y",
        0x16 => "U",
        0x17 => "I",
        0x18 => "O",
        0x19 => "P",
        0x1A => "[",
        0x1B => "]",
        0x1C => "Enter",
        0x1D => "Ctrl",
        0x1E => "A",
        0x1F => "S",
        0x20 => "D",
        0x21 => "F",
        0x22 => "G",
        0x23 => "H",
        0x24 => "J",
        0x25 => "K",
        0x26 => "L",
        0x27 => ";",
        0x28 => "'",
        0x29 => "`",
        0x2A => "Shift",
        0x2B => "\\",
        0x2C => "Z",
        0x2D => "X",
        0x2E => "C",
        0x2F => "V",
        0x30 => "B",
        0x31 => "N",
        0x32 => "M",
        0x33 => ",",
        0x34 => ".",
        0x35 => "/",
        0x36 => "Right Shift",
        0x37 => "Num *",
        0x38 => "Alt",
        0x39 => "Space",
        0x3A => "Caps Lock",
        0x3B => "F1",
        0x3C => "F2",
        0x3D => "F3",
        0x3E => "F4",
        0x3F => "F5",
        0x40 => "F6",
        0x41 => "F7",
        0x42 => "F8",
        0x43 => "F9",
        0x44 => "F10",
        0x45 => "Num Lock",
        0x46 => "Scroll Lock",
        0x47 => "Num 7",
        0x48 => "Num 8",
        0x49 => "Num 9",
        0x4A => "Num -",
        0x4B => "Num 4",
        0x4C => "Num 5",
        0x4D => "Num 6",
        0x4E => "Num +",
        0x4F => "Num 1",
        0x50 => "Num 2",
        0x51 => "Num 3",
        0x52 => "Num 0",
        0x53 => "Num Del",
        0x57 => "F11",
        0x58 => "F12",
        0x9C => "Num Enter",
        0x9D => "Right Ctrl",
        0xB5 => "Num /",
        0xB7 => "Prnt Scrn",
        0xB8 => "Right Alt",
        0xC5 => "Pause",
        0xC7 => "Home",
        0xC8 => "Up",
        0xC9 => "Page Up",
        0xCB => "Left",
        0xCD => "Right",
        0xCF => "End",
        0xD0 => "Down",
        0xD1 => "Page Down",
        0xD2 => "Insert",
        0xD3 => "Delete",
        0xDB => "Left Windows",
        0xDC => "Right Windows",
        0xDD => "Application",
        _ => return None,
    })
}

/// The key map screen.
#[derive(Debug)]
pub struct Keyboard {
    frame: FrameId,
    field: Option<Handle>,
    list: Option<Handle>,
    clear_button: Option<Handle>,
    save_button: Option<Handle>,
    undo_button: Option<Handle>,
    /// The maps shown and changed.
    maps: KeyMaps,
    /// The key cell waiting for a key, and the one a key pressed is already bound in.
    selected: Option<Handle>,
    occupied: Option<Handle>,
    /// A key cell selected waits for a key.
    waiting: bool,
    /// The key pressed and the modifier held with it, until it is bound.
    sem: u32,
    meta: u32,
    /// There are changes not applied.
    modified: bool,
    /// The preset button last pressed.
    last_restore: Token,
    /// The questions up (0: none), and please wait.
    clear_all_context: u32,
    already_mapped: u32,
    restore_context: u32,
    undo_context: u32,
    exit_context: u32,
    wait_context: u32,
}

impl Keyboard {
    /// The screen made: its field, its list and buttons, what it hears; the client's maps; the
    /// list filled; the buttons as nothing changed.
    // D2-01770, D2-01771, D2-01774
    pub fn new(ui: &mut Ui, data: &mut Data, _out: &mut Vec<Request>) -> Self {
        let frame = ui.new_main_frame();
        let field = ui.frame_create_root(frame, LAYOUT_ENUM, name::KEYBOARD_FIELD);
        let child = |ui: &Ui, n| field.and_then(|f| ui.child_recursive(f, n));
        let mut me = Self {
            frame,
            field,
            list: child(ui, name::LIST_BOX),
            clear_button: child(ui, name::CLEAR_BUTTON),
            save_button: child(ui, name::SAVE_BUTTON),
            undo_button: child(ui, name::UNDO_BUTTON),
            maps: data
                .keys
                .as_ref()
                .map(|k| k.maps.clone())
                .unwrap_or_default(),
            selected: None,
            occupied: None,
            waiting: false,
            sem: 0,
            meta: 0,
            modified: false,
            last_restore: preset::DEFAULTS,
            clear_all_context: 0,
            already_mapped: 0,
            restore_context: 0,
            undo_context: 0,
            exit_context: 0,
            wait_context: 0,
        };
        pregame::listen(ui, frame, name::LIST_BOX, msg::NEW_SELECTION);
        for b in [
            name::SAVE_BUTTON,
            name::CLEAR_BUTTON,
            name::CLEAR_ALL_BUTTON,
            name::QUIT_BUTTON,
            preset::DEFAULTS,
            preset::WOW,
            preset::FK,
            preset::EQ,
            preset::DAOC,
            name::UNDO_BUTTON,
        ] {
            pregame::listen(ui, frame, b, msg::BUTTON_RELEASE_ON);
        }
        me.initialize_keyboard_list(ui, data);
        me.update_buttons(ui);
        me
    }

    /// The maps as the screen shows them.
    #[must_use]
    pub const fn maps(&self) -> &KeyMaps {
        &self.maps
    }

    /// Whether there are changes not applied.
    #[must_use]
    pub const fn modified(&self) -> bool {
        self.modified
    }

    /// Whether any question is up.
    // D2-01750
    fn confirmation_boxes_open(&self) -> bool {
        self.clear_all_context != 0
            || self.already_mapped != 0
            || self.restore_context != 0
            || self.undo_context != 0
            || self.exit_context != 0
    }

    /// Clear is lit while a key cell is selected; Apply and Undo while there are changes.
    // D2-01751
    fn update_buttons(&self, ui: &mut Ui) {
        let (Some(c), Some(a), Some(u)) = (self.clear_button, self.save_button, self.undo_button)
        else {
            return;
        };
        ui.set_attr(c, attr::GHOSTED, Value::Bool(self.selected.is_none()));
        ui.set_attr(a, attr::GHOSTED, Value::Bool(!self.modified));
        ui.set_attr(u, attr::GHOSTED, Value::Bool(!self.modified));
    }

    /// A list item's own text element.
    fn text_of(ui: &Ui, item: Handle) -> Option<Handle> {
        let id = ui.node(item)?.id_attr(name::TEXT_ID)?;
        ui.child_recursive(item, Token(id))
    }

    fn set_cell(ui: &mut Ui, item: Handle, key: u32, m: u32) {
        ui.set_attr(item, cell::HIT_KEY, Value::Int(key.cast_signed()));
        ui.set_attr(item, cell::HIT_META, Value::Int(m.cast_signed()));
    }

    fn cell_of(ui: &Ui, item: Handle) -> (u32, u32) {
        let n = ui.node(item);
        (
            n.and_then(|n| n.int_attr(cell::HIT_KEY))
                .unwrap_or(0)
                .cast_unsigned(),
            n.and_then(|n| n.int_attr(cell::HIT_META))
                .unwrap_or(0)
                .cast_unsigned(),
        )
    }

    /// Every key cell emptied: a change, nothing selected.
    // D2-01753
    fn do_clear_all(&mut self, ui: &mut Ui) {
        self.modified = true;
        self.update_buttons(ui);
        let Some(list) = self.list else { return };
        ui.list_select_item(list, None, true);
        for (i, item) in ui.list_items(list).into_iter().enumerate() {
            if i % 3 == 0 {
                continue;
            }
            Self::set_cell(ui, item, 0, 0);
            let Some(t) = Self::text_of(ui, item) else {
                return;
            };
            ui.clear_all_text(t);
        }
    }

    /// Every question and please wait down.
    // D2-01754
    fn close_all_dialogs(&mut self, out: &mut Vec<Request>) {
        for c in [
            &mut self.clear_all_context,
            &mut self.already_mapped,
            &mut self.restore_context,
            &mut self.undo_context,
            &mut self.exit_context,
            &mut self.wait_context,
        ] {
            pregame::dialog_down(*c, out);
            *c = 0;
        }
    }

    /// How a key is written, with the modifier held with it (`Ctrl - F5`); none for a key with no
    /// name.
    // D2-01756, D2-14175
    fn inq_key_string_info(ui: &Ui, key: u32, m: u32) -> Option<String> {
        let k = key_name(key)?;
        let literal = |s: &str| Var::Text(Box::new(StringInfo::literal(s)));
        if m == 0 {
            return Some(message(ui, "ID_Keyboard_key", &[("key", literal(k))]));
        }
        let k2 = key_name(m)?;
        Some(message(
            ui,
            "ID_Keyboard_key_meta",
            &[("key", literal(k)), ("key2", literal(k2))],
        ))
    }

    /// Back: please wait, the character list.
    // D2-01760
    fn close_screen(&mut self, ui: &Ui, out: &mut Vec<Request>) {
        self.wait_context = pregame::please_wait(ui, self.frame, out);
        pregame::queue(mode::CHARACTER_SELECTION, out);
    }

    /// The key pressed is bound in the cell selected: the cell it was in before (a question said
    /// yes) emptied, the cell written and keeping the key, nothing selected.
    // D2-01761
    fn finish_set(&mut self, ui: &mut Ui) {
        let (Some(list), Some(selected)) = (self.list, self.selected) else {
            return;
        };
        self.modified = true;
        self.update_buttons(ui);
        if let Some(o) = self.occupied.take() {
            let Some(t) = Self::text_of(ui, o) else {
                return;
            };
            ui.clear_all_text(t);
            Self::set_cell(ui, o, 0, 0);
        }
        if let Some(text) = Self::inq_key_string_info(ui, self.sem, self.meta) {
            let Some(t) = Self::text_of(ui, selected) else {
                return;
            };
            ui.set_text(t, &text);
        }
        Self::set_cell(ui, selected, self.sem, self.meta);
        ui.list_select_item(list, None, true);
        self.sem = 0;
        self.meta = 0;
    }

    /// A key pressed for the selected cell: one bound in another cell asks whether to move it
    /// there; one the game keeps for an action without a name is refused; else it is bound.
    // D2-01766
    fn start_set(&mut self, ui: &mut Ui, data: &Data, key: u32, m: u32, out: &mut Vec<Request>) {
        self.sem = key;
        self.meta = m;
        let Some(list) = self.list else { return };
        let items = ui.list_items(list);
        for (i, &item) in items.iter().enumerate() {
            let k = i % 3;
            if k == 0 || Some(item) == self.selected || Self::cell_of(ui, item) != (key, m) {
                continue;
            }
            let Some(action_row) = items.get(i - k).copied() else {
                continue;
            };
            let action_text = Self::text_of(ui, action_row)
                .and_then(|t| ui.text(t))
                .unwrap_or_default();
            if let Some(key_text) = Self::inq_key_string_info(ui, key, m) {
                let literal = |s: &str| Var::Text(Box::new(StringInfo::literal(s)));
                let q = message(
                    ui,
                    "ID_Keyboard_ReplaceKeyConfirm",
                    &[
                        ("key", literal(&key_text)),
                        ("action", literal(&action_text)),
                    ],
                );
                self.occupied = Some(item);
                self.already_mapped = pregame::question(q, self.frame, out);
            }
            return;
        }
        if let Some(d) = &data.keys {
            for &action in &d.unnamed {
                if self.maps.mapped_keys(action).first() == Some(&(key, m)) {
                    let text = message(ui, "ID_Keyboard_Error_Reserved", &[]);
                    if let Some(e) = ui.find(name::ERROR_TEXT) {
                        ui.set_text(e, &text);
                    }
                    return;
                }
            }
        }
        self.finish_set(ui);
    }

    /// The list: an action's row and its two key cells for each action that has a name, the
    /// action's name and the keys it is bound to (the first two found from the bottom map up).
    // D2-01767
    fn initialize_keyboard_list(&mut self, ui: &mut Ui, data: &Data) {
        let Some(list) = self.list else { return };
        let Some(d) = &data.keys else { return };
        ui.list_flush(list);
        for (action, text) in &d.actions {
            let Some(row) = ui.frame_create_element(LAYOUT_ENUM, name::ACTION_BOX_ITEM) else {
                continue;
            };
            ui.list_add(list, row);
            let Some(t) = Self::text_of(ui, row) else {
                continue;
            };
            ui.set_text(t, text);
            ui.set_attr(row, name::ID, Value::Int(action.cast_signed()));
            let keys = self.maps.mapped_keys(*action);
            for k in 0..2 {
                let Some(c) = ui.frame_create_element(LAYOUT_ENUM, name::KEY_BOX_ITEM) else {
                    continue;
                };
                ui.list_add(list, c);
                let Some(&(key, m)) = keys.get(k) else {
                    continue;
                };
                let Some(t) = Self::text_of(ui, c) else {
                    continue;
                };
                if let Some(s) = Self::inq_key_string_info(ui, key, m) {
                    ui.set_text(t, &s);
                }
                Self::set_cell(ui, c, key, m);
            }
        }
    }

    /// Apply: the maps made from the list (a key alone to the plain map, held with Shift to the
    /// Shift map, with Ctrl to the Ctrl map) and kept by the client; nothing left to apply.
    // D2-01768
    fn save_keyboard_list(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        let Some(list) = self.list else { return };
        let items = ui.list_items(list);
        let mut maps = KeyMaps {
            base: self.maps.base.clone(),
            ..KeyMaps::default()
        };
        let mut i = 0;
        while i < items.len() {
            let action = ui
                .node(items[i])
                .and_then(|n| n.int_attr(name::ID))
                .unwrap_or(0)
                .cast_unsigned();
            for _ in 0..2 {
                i += 1;
                let Some(&c) = items.get(i) else { break };
                let (key, m) = Self::cell_of(ui, c);
                if key == 0 {
                    continue;
                }
                match m {
                    0 => maps.normal.push((key, action)),
                    meta::LEFT_CTRL | meta::RIGHT_CTRL => maps.control.push((key, action)),
                    meta::LEFT_SHIFT | meta::RIGHT_SHIFT => maps.shift.push((key, action)),
                    _ => {}
                }
            }
            i += 1;
        }
        self.maps = maps.clone();
        out.push(Request::Keys(maps));
        self.modified = false;
        self.update_buttons(ui);
    }

    /// A question answered: Clear All (every cell emptied), a key already bound (moved, or the key
    /// let go), a preset (its maps in place, the list again), Undo (the maps as last applied, the
    /// list again), Back with changes (the character list).
    // D2-01769
    fn close_confirmation_dialog(
        &mut self,
        ui: &mut Ui,
        data: &Data,
        context: u32,
        yes: bool,
        out: &mut Vec<Request>,
    ) {
        if context == self.clear_all_context {
            self.clear_all_context = 0;
            if yes {
                self.do_clear_all(ui);
            }
        } else if context == self.already_mapped {
            self.already_mapped = 0;
            if yes {
                self.finish_set(ui);
            } else {
                self.selected = None;
                self.sem = 0;
                self.meta = 0;
            }
        } else if context == self.restore_context {
            self.restore_context = 0;
            if !yes {
                return;
            }
            if let Some((_, m)) = data
                .keys
                .as_ref()
                .and_then(|d| d.presets.iter().find(|(b, _)| *b == self.last_restore))
            {
                self.maps = KeyMaps {
                    base: self.maps.base.clone(),
                    ..m.clone()
                };
            }
            self.modified = true;
            self.update_buttons(ui);
            self.initialize_keyboard_list(ui, data);
        } else if context == self.undo_context {
            self.undo_context = 0;
            if !yes {
                return;
            }
            self.modified = false;
            if let Some(d) = &data.keys {
                self.maps = d.maps.clone();
            }
            self.initialize_keyboard_list(ui, data);
            self.update_buttons(ui);
        } else if context == self.exit_context {
            self.exit_context = 0;
            if yes {
                self.close_screen(ui, out);
            }
        }
    }

    /// What the screen heard, unless a question is up: a preset (asks), Undo with changes (asks),
    /// Apply, Back (asks with changes), Clear (the selected cell emptied), Clear All (asks), the
    /// list (an action's row selects its first key cell; a key cell waits for a key).
    // D2-01772
    fn listen_to_message(&mut self, ui: &mut Ui, element: Token, out: &mut Vec<Request>) {
        if self.clear_all_context != 0
            || self.already_mapped != 0
            || self.restore_context != 0
            || self.undo_context != 0
            || self.exit_context != 0
        {
            return;
        }
        let ask = |ui: &Ui, s: &str, f: FrameId, out: &mut Vec<Request>| {
            pregame::question(message(ui, s, &[]), f, out)
        };
        if preset::MAPS.iter().any(|(b, _)| *b == element) {
            self.last_restore = element;
            self.restore_context = ask(ui, "ID_Keyboard_RestoreConfirm", self.frame, out);
        }
        if element == name::UNDO_BUTTON && self.modified {
            self.undo_context = ask(ui, "ID_Keyboard_UndoConfirm", self.frame, out);
        }
        if element == name::SAVE_BUTTON {
            self.save_keyboard_list(ui, out);
        }
        if element == name::QUIT_BUTTON {
            if self.modified {
                self.exit_context = ask(ui, "ID_Keyboard_ExitConfirm", self.frame, out);
            } else {
                self.close_screen(ui, out);
            }
        }
        if element == name::CLEAR_BUTTON {
            if let Some(s) = self.selected {
                let Some(t) = Self::text_of(ui, s) else {
                    return;
                };
                ui.clear_all_text(t);
                Self::set_cell(ui, s, 0, 0);
                let Some(list) = self.list else { return };
                ui.list_select_item(list, None, true);
                self.modified = true;
                self.update_buttons(ui);
            }
        }
        if element == name::CLEAR_ALL_BUTTON {
            let q = pregame::message_by_token(ui, CLEAR_ALL_QUESTION, &[]);
            self.clear_all_context = pregame::question(q, self.frame, out);
        }
        if element == name::LIST_BOX {
            let Some(list) = self.list else { return };
            let index = ui.list_selected_index(list);
            if let Some(i) = index.filter(|i| i % 3 == 0) {
                ui.list_select_index(list, i + 1, true);
                return;
            }
            self.selected = ui.list_selected_item(list);
            if self.selected.is_some() {
                self.waiting = true;
            }
            self.update_buttons(ui);
        }
    }

    /// A key pressed (its input word, the modifier held): while the left button has not ended
    /// the wait, no question is up, the key is not the pointer's and not one the game keeps, a
    /// cell is selected and the key has a name, it is set there.
    // D2-01773
    pub fn key_hit(&mut self, ui: &mut Ui, data: &Data, key: u32, m: u32, out: &mut Vec<Request>) {
        if !self.waiting {
            if key == LEFT_BUTTON {
                return;
            }
        } else if key == LEFT_BUTTON {
            self.waiting = false;
            return;
        }
        if self.confirmation_boxes_open()
            || key == POINTER_X
            || key == POINTER_Y
            || self.maps.reserved(key)
            || self.selected.is_none()
            || key_name(key).is_none()
        {
            return;
        }
        self.start_set(ui, data, key, m, out);
    }

    /// The list's cells, for a check: each row's text (an action's name, or a key's).
    #[must_use]
    pub fn rows(&self, ui: &Ui) -> Vec<String> {
        let Some(list) = self.list else {
            return Vec::new();
        };
        ui.list_items(list)
            .into_iter()
            .map(|i| {
                Self::text_of(ui, i)
                    .and_then(|t| ui.text(t))
                    .unwrap_or_default()
            })
            .collect()
    }
}

impl Screen for Keyboard {
    fn frame(&self) -> FrameId {
        self.frame
    }

    fn tick(&mut self, ui: &mut Ui, data: &mut Data, out: &mut Vec<Request>) {
        for m in pregame::take(ui, self.frame) {
            if let Mail::Element { element, .. } = m {
                self.listen_to_message(ui, element, out);
            }
        }
        for (key, m) in std::mem::take(&mut data.key_hits) {
            self.key_hit(ui, data, key, m, out);
        }
    }

    fn notice(&mut self, ui: &mut Ui, data: &mut Data, n: &Notice, out: &mut Vec<Request>) {
        if let Notice::ConfirmationClosed { id, yes } = n {
            self.close_confirmation_dialog(ui, data, *id, *yes, out);
        }
    }

    /// The screen goes: its questions and please wait down, the list emptied, its field.
    // D2-01755
    fn close(mut self: Box<Self>, ui: &mut Ui, _data: &mut Data, out: &mut Vec<Request>) {
        self.close_all_dialogs(out);
        if let Some(l) = self.list.take() {
            ui.list_flush(l);
        }
        let _ = self.field.take();
        ui.drop_frame(self.frame);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
