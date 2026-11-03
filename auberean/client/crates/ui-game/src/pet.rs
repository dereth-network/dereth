//! The pet panel: the player's pets, each with its icon, its name and its mode, and the buttons
//! that command them.
//!
//! The scripts add a pet (its name, its icon, its mode), take one away, rename one, change its
//! mode, and say one is under attack. The panel lists them in a row each (as tall as its rows,
//! and shown only while the player has pets and the player's options show it):
//!
//! - a pet's row pressed selects the pet; with the right button its name may be typed over,
//!   asked of the scripts when the field is left with another name;
//! - its next and previous mode buttons turn its mode through the nine modes (aggressive,
//!   opportunistic, courageous, protective, assisting, defensive, supporting, cooperative,
//!   idle), shown at once and asked of the scripts within five seconds, and asked again every
//!   five seconds until the scripts say the pet took it;
//! - attack (on the selection), return, buff and heal command the pets (ghosted with none);
//! - a pet under attack shows unsafe until five seconds after the last attack.
//!
//! A pet's name as the scripts give it loses the part in parentheses after its first letter (the
//! owner's name).

use std::any::Any;
use std::collections::BTreeMap;

use auberean_ui::framework::FrameId;
use auberean_ui::manager::global;
use auberean_ui::system::Value;
use auberean_ui::token::{attr, msg, state};
use auberean_ui::types::{button_id, text::text_msg};
use auberean_ui::{Handle, Mail, Token, Ui};

use crate::gameplay::{Child, GamePlay, Notice, Record, Request, View, PLAY_LAYOUT_ENUM};
use crate::inventory::NO_OBJECT;

/// The scripts' notices to the pet panel, by the native that sends each.
pub mod notice {
    /// A pet added: the pet (high and low), its name (the record), its icon, its mode.
    pub const ADD_PET: u32 = 0x0085 << 16 | 0x0017;
    /// A pet gone: the pet (high and low).
    pub const REMOVE_PET: u32 = 0x0085 << 16 | 0x0018;
    /// A pet renamed: the pet (high and low), its name (the record).
    pub const UPDATE_PET_NAME: u32 = 0x0085 << 16 | 0x0019;
    /// A pet's mode changed: the pet (high and low), the mode.
    pub const UPDATE_PET_MODE: u32 = 0x0085 << 16 | 0x001A;
    /// A pet under attack: the pet (high and low).
    pub const PET_IS_UNDER_ATTACK: u32 = 0x0085 << 16 | 0x001B;
    /// The player's selection changed (high and low).
    pub const SELECTION_CHANGED: u32 = crate::inventory::notice::SELECTION_CHANGED;
    /// The options window changed the player's options.
    pub const OPTIONS_CHANGED: u32 = crate::options::notice::OPTIONS_CHANGED;
}

mod name {
    use auberean_ui::Token;
    pub const PANEL: Token = Token::of("PetPanel");
    pub const LIST: Token = Token::of("PetPanelListBox");
    pub const TEMPLATE: Token = Token::of("PetTemplate");
    pub const ICON: Token = Token::of("Icon");
    pub const NAME: Token = Token::of("Name");
    pub const MODE: Token = Token::of("Mode");
    pub const NEXT_MODE: Token = Token::of("NextMode");
    pub const PREVIOUS_MODE: Token = Token::of("PreviousMode");
    pub const ATTACK: Token = Token::of("AttackButton");
    pub const RETURN: Token = Token::of("ReturnButton");
    pub const BUFF: Token = Token::of("BuffButton");
    pub const HEAL: Token = Token::of("HealButton");
    /// The pet a row is.
    pub const PET_ID: Token = Token::of("petid");
    /// The panel's states.
    pub const NO_PETS: Token = Token::of("nopets");
    /// A row's states, and its name's.
    pub const SELECTED: Token = Token::of("selected");
    pub const UNSELECTED: Token = Token::of("unselected");
    pub const SAFE: Token = Token::of("safe");
    pub const UNSAFE: Token = Token::of("unsafe");
    /// A mode the table does not hold.
    pub const UNIQUE: Token = Token::of("Unique");
    /// The panel's messages to its listener.
    pub const MOUSE_RELEASE: Token = Token::of("mouse_release");
    pub const SET_SHOW_STATE: Token = Token::of("set_show_state");
    pub const LOCATION_CHANGED: Token = Token::of("location_changed");
}

/// The bit of the player's options that shows the pet panel.
pub const SHOW_PET_UI: u32 = 0x1_0000;

/// How long between asking again for the modes the pets have not taken, and how long a pet
/// shows unsafe after an attack, in seconds.
const REFRESH: f64 = 5.0;

/// The modes in their order, each with its look.
pub const MODES: [(u32, &str); 9] = [
    (0x20, "Aggressive"),
    (0x1000, "Opportunistic"),
    (0x2000, "Courageous"),
    (0x4000_0001, "Protective"),
    (0x4000_0003, "Assisting"),
    (0x4000_0004, "Defensive"),
    (0x4000_0002, "Supporting"),
    (0x4000_0006, "Cooperative"),
    (0x4000_0005, "Idle"),
];

/// What the pet panel asks of the player's scripts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PetRequest {
    /// The pets attack an object.
    Attack(u64),
    Return,
    Buff,
    Heal,
    /// A pet to take a mode.
    Mode {
        pet: u64,
        mode: u32,
    },
    /// A pet renamed.
    Name {
        pet: u64,
        name: String,
    },
}

/// The mode after one (the first after the last, or after one not in the table).
// D2-02368
#[must_use]
pub fn next_mode(mode: u32) -> u32 {
    match MODES.iter().position(|(m, _)| *m == mode) {
        Some(i) => MODES[(i + 1) % MODES.len()].0,
        None => MODES[0].0,
    }
}

/// The mode before one (the last before the first, or before one not in the table).
// D2-02369
#[must_use]
pub fn previous_mode(mode: u32) -> u32 {
    match MODES.iter().position(|(m, _)| *m == mode) {
        Some(0) | None => MODES[MODES.len() - 1].0,
        Some(i) => MODES[i - 1].0,
    }
}

/// A pet's name without the part in parentheses (from an opening one after the first letter to
/// the closing one after it, every place that part occurs).
#[must_use]
pub fn shown_name(name: &str) -> String {
    let units: Vec<u16> = name.encode_utf16().collect();
    let open = units.iter().position(|c| *c == u16::from(b'('));
    let close = units.iter().position(|c| *c == u16::from(b')'));
    match (open, close) {
        (Some(o), Some(c)) if o > 0 && c > o => {
            let part = String::from_utf16_lossy(&units[o..=c]);
            name.replace(&part, "")
        }
        _ => name.to_owned(),
    }
}

/// A pet as the panel keeps it.
// D2-02373
#[derive(Clone, Debug, PartialEq)]
struct PetInfo {
    name: String,
    row: Handle,
    mode: u32,
    next_mode: u32,
    /// When it was last attacked (the interface's clock; 0: not lately).
    last_attacked: f64,
}

/// The pet panel.
#[derive(Debug)]
pub struct Pets {
    frame: FrameId,
    root: Option<Handle>,
    list: Option<Handle>,
    pets: BTreeMap<u64, PetInfo>,
    /// The pet selected (none: no pet), and the player's selection as the scripts last said.
    selected: u64,
    selection: u64,
    /// When the modes were last asked again.
    last_refresh: f64,
    /// The player's options show the panel.
    show_option: bool,
}

impl Pets {
    /// The panel from the play layout, its list, its buttons and moves heard; its state set.
    // D2-02391
    pub fn new(ui: &mut Ui, parent: FrameId, out: &mut Vec<Request>) -> Self {
        let frame = ui.new_child_frame(parent);
        let root = ui.frame_create_root(frame, PLAY_LAYOUT_ENUM, name::PANEL);
        let list = root.and_then(|r| ui.child_recursive(r, name::LIST));
        if let (Some(l), Some(r)) = (ui.frame_listener(frame), root) {
            let id = ui.node(r).map_or(Token(0), |n| n.id);
            ui.listen_for_message(l, id, name::LOCATION_CHANGED);
            ui.listen_for_message(l, id, name::SET_SHOW_STATE);
            for b in [
                name::NEXT_MODE,
                name::PREVIOUS_MODE,
                name::ATTACK,
                name::RETURN,
                name::BUFF,
                name::HEAL,
            ] {
                ui.listen_for_message(l, b, msg::BUTTON_RELEASE_ON);
            }
        }
        let mut me = Self {
            frame,
            root,
            list,
            pets: BTreeMap::new(),
            selected: NO_OBJECT,
            selection: NO_OBJECT,
            last_refresh: 0.0,
            show_option: false,
        };
        me.update_state(ui, out);
        me
    }

    /// The pets, and the one selected.
    #[must_use]
    pub fn pets(&self) -> (Vec<u64>, u64) {
        (self.pets.keys().copied().collect(), self.selected)
    }

    /// A pet's name, mode and the mode it was asked to take.
    #[must_use]
    pub fn pet(&self, pet: u64) -> Option<(&str, u32, u32)> {
        self.pets
            .get(&pet)
            .map(|p| (p.name.as_str(), p.mode, p.next_mode))
    }

    fn button(&self, ui: &Ui, b: Token) -> Option<Handle> {
        self.root.and_then(|r| ui.child_recursive(r, b))
    }

    /// The panel's look: with pets and the option on, shown, as tall as its rows, its buttons lit;
    /// else its no-pets look, hidden, the buttons ghosted.
    // D2-02371, D2-02372
    fn update_state(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        let Some(r) = self.root else { return };
        let n = self.pets.len();
        let shown = n >= 1 && self.show_option;
        ui.set_state(
            r,
            if shown {
                state::STATELESS
            } else {
                name::NO_PETS
            },
        );
        let want = i32::try_from(n)
            .unwrap_or(i32::MAX)
            .saturating_mul(0x14)
            .saturating_add(0x6E);
        if let Some(rect) = ui.node(r).map(|nd| nd.rect) {
            if rect.height() != want {
                ui.resize_to(r, rect.width(), want);
            }
        }
        for b in [name::ATTACK, name::RETURN, name::HEAL, name::BUFF] {
            if let Some(h) = self.button(ui, b) {
                ui.set_attr(h, attr::GHOSTED, Value::Bool(!shown));
            }
        }
        self.show(ui, shown, out);
    }

    /// A pet's row gone.
    // D2-02377
    fn remove_pet(&mut self, ui: &mut Ui, pet: u64, out: &mut Vec<Request>) {
        if let Some(p) = self.pets.remove(&pet) {
            if let Some(l) = self.list {
                ui.list_delete(l, p.row);
            }
        }
        self.update_state(ui, out);
    }

    /// The selection changed: the selected pet's row plain again (its name no longer being typed
    /// over when another is selected); a pet newly selected, its row selected.
    // D2-02378
    fn selection_changed(&mut self, ui: &mut Ui, selection: u64) {
        if self.selected != NO_OBJECT {
            if let Some(p) = self.pets.get(&self.selected) {
                ui.set_state(p.row, name::UNSELECTED);
                if selection != self.selected {
                    let Some(field) = ui.child_recursive(p.row, name::NAME) else {
                        return;
                    };
                    if ui.node(field).is_some_and(|n| n.bool_attr(attr::EDITABLE)) {
                        ui.set_attr(field, attr::EDITABLE, Value::Bool(false));
                        if let Some(l) = ui.frame_listener(self.frame) {
                            ui.unregister_for_message(l, name::NAME, text_msg::DEACTIVATED);
                        }
                    }
                }
            }
            self.selected = NO_OBJECT;
        }
        if let Some(p) = self.pets.get(&selection) {
            ui.set_state(p.row, name::SELECTED);
            self.selected = selection;
        }
    }

    /// Every five seconds: a mode a pet has not taken asked again; a pet not attacked for five
    /// seconds safe again.
    // D2-02382
    fn refresh(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        let now = ui.now();
        for (pet, p) in &mut self.pets {
            if p.mode != p.next_mode {
                out.push(Request::Pet(PetRequest::Mode {
                    pet: *pet,
                    mode: p.next_mode,
                }));
            }
            if p.last_attacked > 0.0 && now - p.last_attacked > REFRESH {
                let Some(field) = ui.child_recursive(p.row, name::NAME) else {
                    return;
                };
                ui.set_state(field, name::SAFE);
                p.last_attacked = 0.0;
            }
        }
        self.last_refresh = now;
    }

    /// A pet added: its row (the pet named on it, its presses heard) unless it has one; its mode
    /// and name; its icon; the panel's state.
    // D2-02383
    fn add_pet(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<Request>) {
        let Some(list) = self.list else { return };
        let pet = n.long(0);
        let icon = n.word_at(3);
        let mode = n.word_at(4);
        let Some(row) = ui.frame_create_element(PLAY_LAYOUT_ENUM, name::TEMPLATE) else {
            return;
        };
        ui.set_attr(row, name::PET_ID, Value::InstanceId(pet));
        if let (Some(l), Some(id)) = (ui.frame_listener(self.frame), ui.node(row).map(|x| x.id)) {
            ui.listen_for_message(l, id, msg::MOUSE_PRESS);
        }
        if !self.pets.contains_key(&pet) {
            ui.list_add(list, row);
        }
        self.pets.insert(
            pet,
            PetInfo {
                name: String::new(),
                row,
                mode,
                next_mode: mode,
                last_attacked: 0.0,
            },
        );
        self.update_pet_mode(ui, pet, mode);
        let given = text_of(ui, n);
        self.update_pet_name(ui, pet, &given);
        if icon != 0 {
            let Some(i) = ui.child_recursive(row, name::ICON) else {
                return;
            };
            ui.add_image_to_state(i, icon, 0, state::STATELESS);
            ui.set_attr(i, attr::SIZE_IMAGE_TO_OWNER, Value::Bool(true));
        }
        self.update_state(ui, out);
    }

    /// A pet under attack: unsafe, from now.
    // D2-02384
    fn pet_is_under_attack(&mut self, ui: &mut Ui, pet: u64) {
        let now = ui.now();
        let Some(p) = self.pets.get_mut(&pet) else {
            return;
        };
        p.last_attacked = now;
        if let Some(field) = ui.child_recursive(p.row, name::NAME) {
            ui.set_state(field, name::UNSAFE);
        }
    }

    /// A pet asked to take a mode: kept as the mode to ask for, its mode shown.
    // D2-02386
    fn request_set_pet_mode(&mut self, ui: &mut Ui, pet: u64, mode: u32) {
        let Some(p) = self.pets.get_mut(&pet) else {
            return;
        };
        p.next_mode = mode;
        let Some(m) = ui.child_recursive(p.row, name::MODE) else {
            return;
        };
        let look = MODES
            .iter()
            .find(|(md, _)| *md == mode)
            .map_or(name::UNIQUE, |(_, s)| Token::of(s));
        ui.set_state(m, look);
    }

    /// A pet took a mode: kept; when it had taken the mode it was asked for, the new one is shown
    /// and asked for.
    // D2-02389
    fn update_pet_mode(&mut self, ui: &mut Ui, pet: u64, mode: u32) {
        let Some(p) = self.pets.get_mut(&pet) else {
            return;
        };
        let settled = p.mode == p.next_mode;
        p.mode = mode;
        if settled {
            self.request_set_pet_mode(ui, pet, mode);
        }
    }

    /// A pet renamed: its name without its owner's part, shown on its row.
    // D2-02392, D2-03038
    fn update_pet_name(&mut self, ui: &mut Ui, pet: u64, given: &str) {
        let Some(p) = self.pets.get_mut(&pet) else {
            return;
        };
        p.name = shown_name(given);
        if let Some(field) = ui.child_recursive(p.row, name::NAME) {
            ui.set_text(field, &p.name);
        }
    }

    /// A row pressed: the pet selected; with the right button, its name to be typed over (the
    /// field made editable, every letter of it selected, given the keyboard, its leaving heard).
    fn row_pressed(&mut self, ui: &mut Ui, row: Handle, button: u32, out: &mut Vec<Request>) {
        let Some(pet) = ui
            .node(row)
            .and_then(|n| n.instance_id_attr(name::PET_ID))
            .filter(|p| *p != NO_OBJECT)
        else {
            return;
        };
        out.push(Request::Select(pet));
        if button != button_id::RIGHT {
            return;
        }
        let Some(p) = self.pets.get(&pet) else { return };
        let Some(field) = ui.child_recursive(p.row, name::NAME) else {
            return;
        };
        ui.set_attr(field, attr::EDITABLE, Value::Bool(true));
        ui.set_attr(field, name::PET_ID, Value::InstanceId(pet));
        ui.select_all(field);
        ui.activate(field);
        if let Some(l) = ui.frame_listener(self.frame) {
            ui.listen_for_message(l, name::NAME, text_msg::DEACTIVATED);
        }
    }

    /// A pet's name typed over and left: no longer editable; another name than the selected
    /// pet's is asked of the scripts; the field shows the pet's name again.
    fn name_left(&mut self, ui: &mut Ui, field: Handle, out: &mut Vec<Request>) {
        ui.set_attr(field, attr::EDITABLE, Value::Bool(false));
        if let Some(l) = ui.frame_listener(self.frame) {
            ui.unregister_for_message(l, name::NAME, text_msg::DEACTIVATED);
        }
        let typed = ui.text(field).unwrap_or_default();
        let Some(p) = self.pets.get(&self.selected) else {
            return;
        };
        if typed.to_lowercase() != p.name.to_lowercase() {
            out.push(Request::Pet(PetRequest::Name {
                pet: self.selected,
                name: typed,
            }));
        }
        ui.clear_all_text(field);
        ui.set_text(field, &p.name);
    }

    /// The next or previous mode button of the row under the pointer.
    fn turn_mode(&mut self, ui: &mut Ui, next: bool) {
        let Some(row) = self.list.and_then(|l| ui.listbox_item_under_mouse(l)) else {
            return;
        };
        let Some(pet) = ui
            .node(row)
            .and_then(|n| n.instance_id_attr(name::PET_ID))
            .filter(|p| *p != NO_OBJECT)
        else {
            return;
        };
        let Some(mode) = self.pets.get(&pet).map(|p| p.mode) else {
            return;
        };
        let to = if next {
            next_mode(mode)
        } else {
            previous_mode(mode)
        };
        self.request_set_pet_mode(ui, pet, to);
    }
}

/// The name a notice's record gives.
fn text_of(ui: &Ui, n: &Notice) -> String {
    n.records
        .iter()
        .flatten()
        .find_map(|r| match r {
            Record::Text(t) => ui.assets().write(t),
            _ => None,
        })
        .unwrap_or_default()
}

impl Pets {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[0x49E, 0x49F, 0x4A0, 0x4A1, 0x4A2, 0x408, 0x46D];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for Pets {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Shown or hidden: its panel as the framework shows; the clock heard while shown.
    // D2-02367
    fn show(&mut self, ui: &mut Ui, on: bool, _out: &mut Vec<Request>) {
        ui.frame_show(self.frame, on);
        let Some(r) = self.root else { return };
        let shown = ui.frame_shown(self.frame);
        ui.set_visible(r, shown);
        let Some(l) = ui.frame_listener(self.frame) else {
            return;
        };
        if on {
            ui.listen_for_global_message(l, global::TIME);
        } else {
            ui.unregister_for_all_global_messages(l);
        }
    }

    /// The panel let go after a move (its place kept), told to show with no pets (hidden); a
    /// row pressed; the mode buttons; attack, return, buff and heal; a name typed over and left;
    /// the clock (every five seconds, the refresh).
    // D2-02385, D2-02390
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<Request>) {
        match *mail {
            Mail::Global {
                what: global::TIME, ..
            } => {
                if ui.now() - self.last_refresh > REFRESH {
                    self.refresh(ui, out);
                }
            }
            Mail::Element {
                from,
                element,
                what,
                param,
            } => {
                if Some(from) == self.root {
                    if what == name::MOUSE_RELEASE {
                        GamePlay::save_locations_changed(out);
                    }
                    if what == name::SET_SHOW_STATE {
                        if param != 0 && self.pets.is_empty() {
                            self.show(ui, false, out);
                        }
                        return;
                    }
                }
                if what == msg::MOUSE_PRESS {
                    self.row_pressed(ui, from, param, out);
                    return;
                }
                if what == msg::BUTTON_RELEASE_ON
                    && (element == name::NEXT_MODE || element == name::PREVIOUS_MODE)
                {
                    self.turn_mode(ui, element == name::NEXT_MODE);
                    return;
                }
                if element == name::ATTACK {
                    out.push(Request::Pet(PetRequest::Attack(self.selection)));
                } else if element == name::RETURN {
                    out.push(Request::Pet(PetRequest::Return));
                } else if element == name::BUFF {
                    out.push(Request::Pet(PetRequest::Buff));
                } else if element == name::HEAL {
                    out.push(Request::Pet(PetRequest::Heal));
                } else if element == name::NAME && what == text_msg::DEACTIVATED {
                    self.name_left(ui, from, out);
                }
            }
            _ => {}
        }
    }

    /// The scripts' pet notices; the selection; the options changed (the panel's state again).
    fn notice(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<Request>) {
        match n.native {
            notice::ADD_PET => self.add_pet(ui, n, out),
            notice::REMOVE_PET => self.remove_pet(ui, n.long(0), out),
            notice::UPDATE_PET_NAME => {
                let given = text_of(ui, n);
                self.update_pet_name(ui, n.long(0), &given);
            }
            notice::UPDATE_PET_MODE => self.update_pet_mode(ui, n.long(0), n.word_at(2)),
            notice::PET_IS_UNDER_ATTACK => self.pet_is_under_attack(ui, n.long(0)),
            notice::SELECTION_CHANGED => {
                self.selection = n.long(0);
                self.selection_changed(ui, n.long(0));
            }
            notice::OPTIONS_CHANGED => self.update_state(ui, out),
            _ => {}
        }
    }

    /// The player's options: whether they show the panel (its state again when that changed).
    fn update(&mut self, ui: &mut Ui, view: &View<'_>, out: &mut Vec<Request>) {
        let Some(o) = view.options else { return };
        let on = o.bits & SHOW_PET_UI != 0;
        if on != self.show_option {
            self.show_option = on;
            self.update_state(ui, out);
        }
    }

    /// It goes: hidden, its listening undone, its rows let go, its panel gone.
    // D2-02381, D2-02370
    fn close(mut self: Box<Self>, ui: &mut Ui) {
        self.show(ui, false, &mut Vec::new());
        self.pets.clear();
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
