//! The shortcut bar (on the toolbar): its slots, the set in view, the peace and war buttons.
//!
//! The bar is a list of slots, ten to a row, one to five rows high, holding `10 x rows + 90`
//! slots: ten sets of ten, the last set's view filled out with slots that are never used (ghosted).
//! Each slot shows what the player's shortcut there holds: a skill's icon, an item's icon (or an
//! hourglass while the item is not here yet), a line of text, a recipe's icon; the slot's number
//! in its corner when the player numbers the slots; a skill waiting to be ready again shows a
//! shadow that draws back as the wait runs out (a skill the player cannot use with what it
//! carries stays shadowed). Its tooltip names what it holds.
//!
//! - **Using a slot**: a click on it, or its key (1 to 0, Shift with them for the second row): the
//!   player's scripts use what it holds. A right click examines what it holds.
//! - **Sets**: the arrows beside the bar, or the keys (Ctrl+1 to 0, - and Shift+-), move the view
//!   to another set of ten; the set's number shows by the arrows, and the scripts are told.
//! - **Dragging**: a slot dragged onto another moves its shortcut there, one dragged off the bar
//!   empties its slot (while the shortcuts are locked, only with Shift held); something with a
//!   shortcut kind dropped on a slot below the hundredth puts its shortcut there; a skill's icon
//!   double-clicked in the skills panel goes to the first empty slot in view.
//! - **Height**: the bar keeps the height in rows the player's options give; resized by hand, it
//!   takes the nearest whole number of rows and says so.
//! - **Peace and war**: the buttons (and the attack key) ask the scripts to change the attack
//!   state, and keep showing the state the player is in until the scripts say it changed.
//!
//! What the bar asks of the player's scripts (the shortcuts, each slot's tooltip and icon, a
//! skill's training, readiness and use, whether the player has an item, the options) comes as a
//! [`Source`] the client asks again when it may have changed; the notices that make the bar look
//! again are taken once the frame's [`Source`] is in.

use std::any::Any;

use auberean_ui::framework::FrameId;
use auberean_ui::manager::global;
use auberean_ui::token::{attr, msg, state};
use auberean_ui::types::button_id;
use auberean_ui::{Handle, Mail, Token, Ui, Value};

use crate::flow;
use crate::gameplay::{notice as play_notice, Child, Examination, Notice, Request, View};

/// The toolbar's layout's enumeration value (the toolbar, the shortcut bar, the chat windows, the
/// toolbar's windows).
pub const TOOLBAR_LAYOUT_ENUM: u32 = 0x4100_0003;

/// The slots the bar uses; the rest only fill out the last set's view.
pub const USABLE_SLOTS: u32 = 100;

/// The wait the bar gives a skill the player cannot use with what it carries.
const UNUSABLE_WAIT: f32 = 1_000_000.0;

mod name {
    use auberean_ui::Token;
    pub const SHORTCUT_FIELD: Token = Token::of("ShortcutField");
    pub const SHORTCUT_LIST_BOX: Token = Token::of("ShortcutListBox");
    pub const SHORTCUT_TEMPLATE: Token = Token::of("ShortcutTemplate");
    pub const SHORTCUT_BUTTON: Token = Token::of("ShortcutButton");
    pub const SHORTCUT_ICON: Token = Token::of("ShortcutIcon");
    pub const ALIAS_TEXT: Token = Token::of("AliasText");
    pub const NUMBER_DISPLAY: Token = Token::of("NumberDisplay");
    pub const SET_NUMBER_DISPLAY: Token = Token::of("SetNumberDisplay");
    pub const TIME_LEFT_METER: Token = Token::of("TimeLeftMeter");
    pub const SKILL_NODE_ICON: Token = Token::of("SkillNodeIcon");
    pub const START_ATTACK_BUTTON: Token = Token::of("StartAttackButton");
    pub const STOP_ATTACK_BUTTON: Token = Token::of("StopAttackButton");
    pub const SHORTCUT_TYPE: Token = Token::of("ShortcutType");
    pub const SHORTCUT_DATA: Token = Token::of("ShortcutData");
    pub const SHORTCUT_INDEX: Token = Token::of("ShortcutIndex");
    pub const MAX_ROWS: Token = Token::of("max_rows");
    pub const BORDER_SIZE: Token = Token::of("border_size");
    pub const ROW_HEIGHT: Token = Token::of("row_height");
    pub const SMOOTH_MOVEMENT: Token = Token::of("smooth_movement");
    pub const POSITION: Token = Token::of("position");
    pub const PENDING: Token = Token::of("pending");
    pub const ACTIVE: Token = Token::of("active");
    pub const TOGGLED: Token = Token::of("toggled");
    pub const STOP_DRAG_ITEM: Token = Token::of("stop_drag_item");
    pub const NEW_VIEW: Token = Token::of("new_view");
}

/// The interface actions the bar hears.
pub mod action {
    /// Start or stop attacking.
    pub const AUTOATTACK: u32 = 0x4100_0013;
    /// Use slots 1 to 10 of the set in view.
    pub const SHORTCUT_1: u32 = 0x4100_002B;
    pub const SHORTCUT_0: u32 = 0x4100_0034;
    /// Show sets 1 to 10.
    pub const SET_1: u32 = 0x4100_0035;
    pub const SET_0: u32 = 0x4100_003E;
    /// Show the next set, the one before.
    pub const SET_NEXT: u32 = 0x4100_003F;
    pub const SET_PREV: u32 = 0x4100_0040;
    /// Use slots 1 to 10 of the second row in view.
    pub const SHORTCUT_B_1: u32 = 0x4100_005F;
    pub const SHORTCUT_B_0: u32 = 0x4100_0068;
}

/// The player's scripts' notices the bar hears, by the native that sends each.
pub mod notice {
    /// The shortcuts came again (the set, the numbering, every slot).
    pub const RELOAD_SHORTCUTS: u32 = 0x008F << 16 | 0x0001;
    /// A recipe is gone (its data id).
    pub const REMOVE_RECIPE: u32 = 0x008F << 16 | 0x0002;
    /// A skill changed (its enumerated value), the reset times changed.
    pub const UPDATE_SKILL: u32 = 0x0085 << 16 | 0x0006;
    pub const UPDATE_RESET_TIME: u32 = 0x0085 << 16 | 0x0007;
    /// The inventory changed (how, the item, ...), a container changed (the container, how, the
    /// item, ...).
    pub const UPDATE_INVENTORY: u32 = 0x008B << 16 | 0x0001;
    pub const UPDATE_CONTAINER: u32 = 0x008B << 16 | 0x0002;
    /// A special attack asked for (its skill, its number), started, ended (its number); one
    /// toggled on or off (its skill, on).
    pub const REQUEST_SPECIAL_ATTACK: u32 = 0x0086 << 16 | 0x0002;
    pub const START_SPECIAL_ATTACK: u32 = 0x0086 << 16 | 0x0003;
    pub const END_SPECIAL_ATTACK: u32 = 0x0086 << 16 | 0x0004;
    pub const TOGGLE_SPECIAL_ATTACK: u32 = 0x0086 << 16 | 0x0005;
}

/// The shortcut kinds.
pub mod kind {
    /// An attack or another skill, by its enumerated value.
    pub const SKILL: u32 = 0x4000_0001;
    /// An item, by its instance id.
    pub const ITEM: u32 = 0x4000_0002;
    /// A line of text.
    pub const TEXT: u32 = 0x4000_0003;
    /// An emote, by its enumerated value.
    pub const EMOTE: u32 = 0x4000_0004;
    /// A recipe, by its data id.
    pub const RECIPE: u32 = 0x4000_0006;
}

/// How the inventory changed, as its notice says: everything, or an item pending.
const INVENTORY_ALL: u32 = 0x4000_0001;
const INVENTORY_PENDING: u32 = 0x4000_0005;

/// What the examination asks for, by what a slot holds.
pub mod examine {
    pub const ITEM: u32 = 0x4000_0002;
    pub const SKILL: u32 = 0x4000_0003;
    pub const RECIPE: u32 = 0x4000_0007;
}

/// One shortcut: its kind, and the datum its kind uses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shortcut {
    pub kind: u32,
    pub item: u64,
    pub data_enum: u32,
    pub data_id: u32,
    pub text: String,
}

/// A slot's tooltip and icon, as the scripts give them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SlotInfo {
    pub answer: u32,
    pub tooltip: String,
    pub icon: u32,
}

/// What the scripts say of a skill a slot holds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SkillState {
    pub skill: u32,
    pub trained: bool,
    pub usable: bool,
    /// While it waits to be ready: the whole wait and what is left, in seconds.
    pub waiting: Option<(f32, f32)>,
}

/// What the client knows of an item a slot holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemState {
    pub item: u64,
    pub have: bool,
    /// Its name and icon, while it is here.
    pub here: Option<(String, u32)>,
    /// The icon its look makes, while it is here: layers bottom first, each a picture and the
    /// color it is tinted by (none: the icon above is shown).
    pub icon_layers: Vec<(u32, u32)>,
}

/// What the bar asks of the player's scripts, as the client last asked it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Source {
    /// Counts the changes.
    pub version: u64,
    pub shortcuts: Vec<Shortcut>,
    /// The first hundred slots' tooltips and icons.
    pub slots: Vec<SlotInfo>,
    /// The set in view (from 1), whether the slots and sets are numbered, the height in rows,
    /// whether the shortcuts are locked, whether the bar moves between sets at once.
    pub set: u32,
    pub show_numbers: bool,
    pub height: i32,
    pub locked: bool,
    pub insta_scroll: bool,
    pub skills: Vec<SkillState>,
    pub items: Vec<ItemState>,
}

/// A skill's wait shown on a slot: the slot, the skill, the slot's shadow, the whole wait and
/// when it ends.
#[derive(Clone, Copy, Debug)]
struct TimeLeft {
    index: u32,
    skill: u32,
    meter: Option<Handle>,
    total: f64,
    end: f64,
}

/// A long argument of a notice: its high word, then its low.
fn long_at(words: &[u32], i: usize) -> u64 {
    let hi = words.get(i).copied().unwrap_or(0);
    let lo = words.get(i + 1).copied().unwrap_or(0);
    u64::from(hi) << 32 | u64::from(lo)
}

fn index_of(i: usize) -> u32 {
    u32::try_from(i).unwrap_or(u32::MAX)
}

/// The shortcut bar's panel.
#[derive(Debug)]
pub struct Shortcuts {
    frame: FrameId,
    /// The bar.
    pub root: Option<Handle>,
    /// The peace and war buttons, on the toolbar.
    start: Option<Handle>,
    stop: Option<Handle>,
    list: Option<Handle>,
    /// The bar's own copy of the shortcuts.
    shortcuts: Vec<Shortcut>,
    /// The attack state as the player's scripts last said.
    attacking: bool,
    /// The player targets automatically, as last seen.
    pub auto_target: bool,
    /// Special attacks asked for: their number and skill.
    special: Vec<(u32, u32)>,
    time_left: Vec<TimeLeft>,
    /// The first slot in view.
    offset: u32,
    show_numbers: bool,
    /// The bar was moved or resized by hand; the display is being set up again.
    moved: bool,
    refreshing: bool,
    /// Item slots waiting for their item to be here.
    waiting: Vec<(u64, Handle)>,
    /// What the scripts last said, and the notices waiting for this frame's word.
    source: Option<Source>,
    noticed: Vec<Notice>,
    /// The bar's height was set from the player's options.
    started: bool,
    /// The list's drag check is the locked one.
    locked: bool,
}

impl Shortcuts {
    /// The shortcut bar under the toolbar's framework, its buttons found on the toolbar
    /// (`toolbar`): the bar and its list, the list's drag checks, peace shown, the buttons, the
    /// slots, the list's drops and views, the skills panel's icons and the bar's moves heard, and
    /// the keys, the clock and the display's being set up again. (Its height from the player's
    /// options, with its slots, comes with the first word from the scripts.)
    // D2-02536
    pub fn new(ui: &mut Ui, parent: FrameId, toolbar: Option<Handle>) -> Self {
        let frame = ui.new_child_frame(parent);
        let find = |ui: &Ui, n| toolbar.and_then(|t| ui.child_recursive(t, n));
        let start = find(ui, name::START_ATTACK_BUTTON);
        let stop = find(ui, name::STOP_ATTACK_BUTTON);
        let root = ui.frame_create_root(frame, TOOLBAR_LAYOUT_ENUM, name::SHORTCUT_FIELD);
        let list = root.and_then(|r| ui.child_recursive(r, name::SHORTCUT_LIST_BOX));
        if let Some(l) = list {
            ui.set_drag_checks(l, Some(drag_out_unlocked), Some(drop_in));
        }
        let me = Self {
            frame,
            root,
            start,
            stop,
            list,
            shortcuts: Vec::new(),
            attacking: false,
            auto_target: false,
            special: Vec::new(),
            time_left: Vec::new(),
            offset: 0,
            show_numbers: false,
            moved: true,
            refreshing: false,
            waiting: Vec::new(),
            source: None,
            noticed: Vec::new(),
            started: false,
            locked: false,
        };
        if let Some(l) = ui.frame_listener(frame) {
            for (element, what) in [
                (name::START_ATTACK_BUTTON, msg::BUTTON_RELEASE_ON),
                (name::STOP_ATTACK_BUTTON, msg::BUTTON_RELEASE_ON),
                (name::SHORTCUT_BUTTON, msg::BUTTON_RELEASE_ON),
                (name::SHORTCUT_LIST_BOX, name::STOP_DRAG_ITEM),
                (name::SHORTCUT_LIST_BOX, name::NEW_VIEW),
                (name::SKILL_NODE_ICON, msg::MOUSE_PRESS_TOP),
                (name::SHORTCUT_FIELD, msg::LOCATION_CHANGED),
            ] {
                ui.listen_for_message(l, element, what);
            }
            for g in [
                global::KEY_DOWN,
                global::REFRESH_START,
                global::REFRESH_DONE,
                global::TIME,
            ] {
                ui.listen_for_global_message(l, g);
            }
        }
        me.check_attack_buttons(ui);
        me
    }

    /// Whether the buttons show war.
    #[must_use]
    pub const fn attacking(&self) -> bool {
        self.attacking
    }

    /// The bar's own copy of the shortcuts.
    #[must_use]
    pub fn shortcuts(&self) -> &[Shortcut] {
        &self.shortcuts
    }

    /// The first slot in view.
    #[must_use]
    pub const fn offset(&self) -> u32 {
        self.offset
    }

    /// The slots' elements, in order.
    #[must_use]
    pub fn slots(&self, ui: &Ui) -> Vec<Handle> {
        self.list.map(|l| ui.list_items(l)).unwrap_or_default()
    }

    // -----------------------------------------------------------------------------------------
    // What the scripts say
    // -----------------------------------------------------------------------------------------

    fn slot_info(&self, index: u32) -> SlotInfo {
        self.source
            .as_ref()
            .and_then(|s| s.slots.get(index as usize).cloned())
            .unwrap_or_default()
    }

    fn skill(&self, skill: u32) -> Option<SkillState> {
        self.source
            .as_ref()?
            .skills
            .iter()
            .find(|k| k.skill == skill)
            .copied()
    }

    fn item(&self, item: u64) -> Option<&ItemState> {
        self.source.as_ref()?.items.iter().find(|i| i.item == item)
    }

    fn height(&self) -> i32 {
        self.source.as_ref().map_or(1, |s| s.height)
    }

    fn insta_scroll(&self) -> bool {
        self.source.as_ref().is_some_and(|s| s.insta_scroll)
    }

    // -----------------------------------------------------------------------------------------
    // The attack state
    // -----------------------------------------------------------------------------------------

    /// The button for the state the player is in ghosted.
    // D2-02490
    fn check_attack_buttons(&self, ui: &mut Ui) {
        if let Some(b) = self.start {
            ui.set_attr(b, attr::GHOSTED, Value::Bool(self.attacking));
        }
        if let Some(b) = self.stop {
            ui.set_attr(b, attr::GHOSTED, Value::Bool(!self.attacking));
        }
    }

    /// The player's scripts say the player is attacking or not: the button for the state the
    /// player is in is ghosted.
    // D2-02491
    pub fn set_attack_state(&mut self, ui: &mut Ui, attacking: bool) {
        self.attacking = attacking;
        self.check_attack_buttons(ui);
    }

    /// A button asks to start or stop attacking: to start, a player who targets automatically, with
    /// nothing selected it could attack, selects the nearest foe first; the buttons go on showing the state the player is in until
    /// the scripts say it changed.
    // D2-02492
    pub fn request_attack_state_change(
        &mut self,
        ui: &mut Ui,
        start: bool,
        out: &mut Vec<Request>,
    ) {
        if start && self.auto_target {
            // D2-00170
            out.push(Request::Notice(Notice {
                native: crate::selection::notice::SELECT,
                words: vec![crate::selection::key::NEAREST_FOE],
                ..Notice::default()
            }));
        }
        out.push(Request::Attack { start });
        self.check_attack_buttons(ui);
    }

    // -----------------------------------------------------------------------------------------
    // Slots
    // -----------------------------------------------------------------------------------------

    /// A slot's element: the slot template with its kind, number and datum; dragable unless
    /// empty; its number in its corner when it holds something and the slots are numbered. A
    /// slot from the hundredth on is ghosted for good. Otherwise its tooltip and icon are the
    /// scripts' (an item's own come from the item), the icon over the slot's picture.
    // D2-02527
    fn make_element(&mut self, ui: &mut Ui, index: u32, info: &Shortcut) -> Option<Handle> {
        let e = ui.frame_create_element(TOOLBAR_LAYOUT_ENUM, name::SHORTCUT_TEMPLATE)?;
        let alias = ui.child_recursive(e, name::ALIAS_TEXT);
        if let Some(a) = alias {
            ui.clear_all_text(a);
        }
        let empty = info.kind == 0;
        ui.set_attr(e, name::SHORTCUT_TYPE, Value::Id(info.kind));
        #[allow(clippy::cast_possible_wrap)]
        ui.set_attr(e, name::SHORTCUT_INDEX, Value::Int(index as i32));
        ui.set_attr(e, attr::DRAGABLE, Value::Bool(!empty));
        if let Some(n) = ui.child_recursive(e, name::NUMBER_DISPLAY) {
            ui.clear_all_text(n);
            if !empty && self.show_numbers {
                ui.set_text(n, &format!("{}", (index + 1) % 10));
            }
        }
        let Some(icon) = ui.child_recursive(e, name::SHORTCUT_ICON) else {
            ui.remove(e);
            return None;
        };
        if index >= USABLE_SLOTS {
            if let Some(b) = ui.child_recursive(e, name::SHORTCUT_BUTTON) {
                ui.set_attr(b, attr::GHOSTED, Value::Bool(true));
                ui.set_attr(b, attr::BROADCAST_WHEN_GHOSTED, Value::Bool(false));
            }
            return Some(e);
        }
        let shown = self.slot_info(index);
        ui.set_tooltip(e, &shown.tooltip);
        match info.kind {
            0 | kind::SKILL => ui.set_attr(e, name::SHORTCUT_DATA, Value::Id(info.data_enum)),
            kind::ITEM => {
                self.setup_item_element(ui, info.item, e);
                ui.set_attr(e, name::SHORTCUT_DATA, Value::InstanceId(info.item));
            }
            kind::TEXT => {
                if let Some(a) = alias {
                    ui.set_text(a, &info.text);
                }
            }
            kind::RECIPE => ui.set_attr(e, name::SHORTCUT_DATA, Value::Id(info.data_id)),
            _ => {}
        }
        if !empty && shown.icon != 0 {
            ui.add_image_to_state(icon, shown.icon, 2, state::STATELESS);
        }
        Some(e)
    }

    /// An item slot's look: the item's name (for as many as there are) as its tooltip and the
    /// icon its look makes on the slot (its plain icon when its look makes none); an item not here
    /// yet waits, the slot pending ("waiting for the server").
    // D2-02522
    fn setup_item_element(&mut self, ui: &mut Ui, item: u64, e: Handle) {
        let Some(icon) = ui.child_recursive(e, name::SHORTCUT_ICON) else {
            return;
        };
        let known = self.item(item);
        let here = known.and_then(|i| i.here.clone());
        let layers = known.map(|i| i.icon_layers.clone()).unwrap_or_default();
        let pending = here.is_none();
        let tip = match here {
            None => {
                self.waiting.push((item, e));
                flow::message(ui, flow::MESSAGES, "ID_Item_TT_WaitingForServer")
            }
            Some((name, picture)) => {
                if layers.is_empty() {
                    ui.set_image(icon, picture, 0);
                } else {
                    ui.set_icon_desc(icon, &layers);
                }
                name
            }
        };
        ui.set_tooltip(e, &tip);
        ui.set_state(
            e,
            if pending {
                name::PENDING
            } else {
                state::STATELESS
            },
        );
    }

    /// Put a shortcut in a slot: with `tell`, the player's scripts keep it and the bar's copy
    /// changes, the slot's element is replaced; without, the element is added at the list's end.
    /// Either way the slot is checked.
    // D2-02529
    fn set_shortcut(
        &mut self,
        ui: &mut Ui,
        index: u32,
        info: &Shortcut,
        tell: bool,
        out: &mut Vec<Request>,
    ) {
        if tell {
            out.push(Request::PutInSlot {
                slot: index,
                shortcut: info.clone(),
            });
            if let Some(s) = self.shortcuts.get_mut(index as usize) {
                *s = info.clone();
            }
        }
        let Some(e) = self.make_element(ui, index, info) else {
            return;
        };
        let Some(l) = self.list else {
            ui.remove(e);
            return;
        };
        if tell {
            ui.list_set_item(l, e, index as usize);
        } else {
            ui.list_add(l, e);
        }
        self.check_shortcut(ui, index, info, out);
    }

    /// Empty a slot: its skill's wait forgotten, an empty shortcut put there.
    // D2-02498
    fn remove_shortcut(&mut self, ui: &mut Ui, index: u32, tell: bool, out: &mut Vec<Request>) {
        if let Some(i) = self.time_left.iter().position(|t| t.index == index) {
            self.time_left.remove(i);
        }
        self.set_shortcut(ui, index, &Shortcut::default(), tell, out);
    }

    /// What a slot holds checked: a skill (trained, its wait), an item (the player's), a recipe
    /// (one is named); an emote is taken off the bar.
    // D2-02512
    fn check_shortcut(&mut self, ui: &mut Ui, index: u32, info: &Shortcut, out: &mut Vec<Request>) {
        match info.kind {
            kind::SKILL => {
                self.check_skill_slot(ui, index, info, out);
            }
            kind::ITEM => {
                self.check_item_slot(ui, index, info, out);
            }
            kind::EMOTE => self.remove_shortcut(ui, index, true, out),
            kind::RECIPE => {
                self.check_recipe(ui, index, info, out);
            }
            _ => {}
        }
    }

    /// A recipe slot: emptied when it names no recipe.
    // D2-02500
    fn check_recipe(
        &mut self,
        ui: &mut Ui,
        index: u32,
        info: &Shortcut,
        out: &mut Vec<Request>,
    ) -> bool {
        if info.kind == kind::EMOTE {
            self.remove_shortcut(ui, index, true, out);
        }
        let ok = info.data_id != 0;
        if !ok {
            self.remove_shortcut(ui, index, true, out);
        }
        ok
    }

    /// An item slot: emptied when the player no longer has the item; a slot whose item is waited
    /// for is shown plainly again.
    // D2-02504
    fn check_item_slot(
        &mut self,
        ui: &mut Ui,
        index: u32,
        info: &Shortcut,
        out: &mut Vec<Request>,
    ) -> bool {
        if self.item(info.item).is_some_and(|i| !i.have) {
            self.remove_shortcut(ui, index, true, out);
            return false;
        }
        let waited = self.waiting.iter().any(|(i, _)| *i == info.item);
        if waited {
            let Some(e) = self.list.and_then(|l| ui.list_item(l, index as usize)) else {
                return false;
            };
            ui.set_state(e, state::STATELESS);
        }
        true
    }

    /// A skill's wait, by the skill and its slot.
    // D2-02501
    fn check_time_left_array(&self, skill: u32, index: u32) -> Option<usize> {
        self.time_left
            .iter()
            .position(|t| t.skill == skill && t.index == index)
    }

    /// A skill slot: emptied when the player no longer has the skill trained; a skill waiting to
    /// be ready (or one the player cannot use with what it carries) shows its wait on the slot's
    /// shadow; one that is ready ends a wait shown.
    // D2-02509
    fn check_skill_slot(
        &mut self,
        ui: &mut Ui,
        index: u32,
        info: &Shortcut,
        out: &mut Vec<Request>,
    ) -> bool {
        let skill = info.data_enum;
        let Some(said) = self.skill(skill) else {
            return true;
        };
        if !said.trained {
            self.remove_shortcut(ui, index, true, out);
            return false;
        }
        let (mut waiting, mut total, mut left) = match said.waiting {
            Some((t, l)) => (true, t, l),
            None => (false, 0.0, 0.0),
        };
        if !said.usable {
            waiting = true;
            total = UNUSABLE_WAIT;
            left = UNUSABLE_WAIT;
        }
        let k = self.check_time_left_array(skill, index);
        let now = ui.now();
        if !waiting {
            if let Some(k) = k {
                self.time_left[k].end = now;
            }
            return true;
        }
        let meter = self
            .list
            .and_then(|l| ui.list_item(l, index as usize))
            .and_then(|e| ui.child_recursive(e, name::TIME_LEFT_METER));
        if meter.is_none() {
            return false;
        }
        let total = if total == 0.0 { 0.0001 } else { total };
        let t = TimeLeft {
            index,
            skill,
            meter,
            total: f64::from(total),
            end: f64::from(left) + now,
        };
        match k {
            Some(k) => self.time_left[k] = t,
            None => self.time_left.push(t),
        }
        self.update_time_left(ui);
        true
    }

    /// The skills' waits shown: each slot's shadow draws back with its wait; a wait done, or one
    /// whose slot no longer holds its skill, is forgotten.
    // D2-02502
    fn update_time_left(&mut self, ui: &mut Ui) {
        let now = ui.now();
        let mut i = 0;
        while i < self.time_left.len() {
            let t = self.time_left[i];
            let holds = self
                .shortcuts
                .get(t.index as usize)
                .is_some_and(|s| s.kind == kind::SKILL && s.data_enum == t.skill);
            if !holds {
                self.time_left.remove(i);
                continue;
            }
            let share = ((t.end - now) / t.total).clamp(0.0, 1.0);
            if let Some(m) = t.meter {
                #[allow(clippy::cast_possible_truncation)]
                let p = (1.0 - share) as f32;
                ui.set_attr(m, name::POSITION, Value::Float(p));
                if share <= 0.0 {
                    self.time_left.remove(i);
                    continue;
                }
            }
            i += 1;
        }
    }

    /// Every item slot checked.
    // D2-02513
    fn check_all_items(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        for i in 0..self.shortcuts.len() {
            let s = self.shortcuts[i].clone();
            if s.kind == kind::ITEM {
                self.check_item_slot(ui, index_of(i), &s, out);
            }
        }
    }

    /// The slots holding an item checked; each, shown pending or plainly, waits for word of it.
    // D2-02514
    fn check_item(&mut self, ui: &mut Ui, item: u64, pending: bool, out: &mut Vec<Request>) {
        for i in 0..self.shortcuts.len() {
            let s = self.shortcuts[i].clone();
            if s.kind != kind::ITEM || s.item != item {
                continue;
            }
            self.check_shortcut(ui, index_of(i), &s, out);
            if let Some(e) = self.list.and_then(|l| ui.list_item(l, i)) {
                self.waiting.push((item, e));
                ui.set_state(
                    e,
                    if pending {
                        name::PENDING
                    } else {
                        state::STATELESS
                    },
                );
            }
        }
    }

    /// Every skill slot checked.
    // D2-02515
    fn check_all_skills(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        for i in 0..self.shortcuts.len() {
            let s = self.shortcuts[i].clone();
            if s.kind == kind::SKILL {
                self.check_skill_slot(ui, index_of(i), &s, out);
            }
        }
    }

    /// The slots holding a skill checked.
    // D2-02516
    fn check_skill(&mut self, ui: &mut Ui, skill: u32, out: &mut Vec<Request>) {
        for i in 0..self.shortcuts.len() {
            let s = self.shortcuts[i].clone();
            if s.kind == kind::SKILL && s.data_enum == skill {
                self.check_shortcut(ui, index_of(i), &s, out);
            }
        }
    }

    /// What a dragged slot (or anything with a shortcut kind) holds, read from its element:
    /// its kind, then its skill, item, text or recipe.
    // D2-02505
    #[must_use]
    pub fn from_element(ui: &Ui, e: Handle) -> Option<Shortcut> {
        let n = ui.node(e)?;
        let k = n.id_attr(name::SHORTCUT_TYPE)?;
        let mut s = Shortcut {
            kind: k,
            ..Shortcut::default()
        };
        match k {
            kind::SKILL => s.data_enum = n.id_attr(name::SHORTCUT_DATA)?,
            kind::ITEM => s.item = n.instance_id_attr(name::SHORTCUT_DATA)?,
            kind::TEXT => {
                let a = ui.child_recursive(e, name::ALIAS_TEXT)?;
                s.text = ui.text(a).unwrap_or_default();
            }
            kind::RECIPE => s.data_id = n.id_attr(name::SHORTCUT_DATA)?,
            _ => {}
        }
        Some(s)
    }

    /// The set in view written by the arrows (when sets are numbered), and the scripts told.
    // D2-02510
    fn update_set_number(&self, ui: &mut Ui, out: &mut Vec<Request>) {
        let Some(d) = ui.element(name::SET_NUMBER_DISPLAY) else {
            return;
        };
        let set = self.offset / 10 + 1;
        if self.show_numbers {
            ui.set_text(d, &format!("{set}"));
        } else {
            ui.clear_all_text(d);
        }
        out.push(Request::ShortcutSet(set));
    }

    /// The list filled out to `10 x rows + 90` slots with empty ones, or cut down to it.
    // D2-02534
    fn update_padding(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        let Some(l) = self.list else { return };
        let rows = ui
            .node(l)
            .and_then(|n| n.int_attr(name::MAX_ROWS))
            .unwrap_or(0);
        let want = usize::try_from((rows * 5 + 45) * 2).unwrap_or(0);
        loop {
            let have = ui.list_items(l).len();
            if have == want {
                return;
            }
            if have < want {
                let empty = Shortcut::default();
                match self.make_element(ui, index_of(have), &empty) {
                    Some(e) => {
                        ui.list_add(l, e);
                        self.check_shortcut(ui, index_of(have), &empty, out);
                    }
                    None => return,
                }
            } else if let Some(e) = ui.list_remove(l, have - 1) {
                ui.remove(e);
            }
        }
    }

    /// The bar's height from the player's options (1 to 5 rows): when the list's rows differ, the
    /// bar resized to them keeping its foot where it is, the list to the bar's inside, the slots
    /// filled out.
    // D2-02535
    fn refresh_shortcut_height(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        let (Some(l), Some(root)) = (self.list, self.root) else {
            return;
        };
        let rows = self.height().clamp(1, 5);
        let now = ui
            .node(l)
            .and_then(|n| n.int_attr(name::MAX_ROWS))
            .unwrap_or(0);
        if now == rows {
            return;
        }
        let border = ui.node(root).and_then(|n| n.int_attr(name::BORDER_SIZE));
        let row_h = ui.node(root).and_then(|n| n.int_attr(name::ROW_HEIGHT));
        if let (Some(r), Some(border), Some(row_h)) = (ui.screen_rect(root), border, row_h) {
            let w = r.width();
            let new_h = row_h * rows + border;
            let y = r.y0 - new_h + r.height();
            ui.move_to(root, r.x0, y);
            ui.resize_to(root, w, new_h);
            ui.move_to(root, r.x0, y);
            ui.set_attr(l, name::MAX_ROWS, Value::Int(rows));
            ui.resize_to(l, w, new_h - border);
            self.update_padding(ui, out);
        }
        self.moved = false;
    }

    /// The shortcuts again: the list emptied (the view kept where it was), the waits and the
    /// waiting items forgotten, every slot of the player's options made and checked, the list
    /// filled out, the view on the set, its number written.
    // D2-02538
    fn init(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        self.shortcuts.clear();
        self.time_left.clear();
        let offset = self.offset;
        let Some(l) = self.list else { return };
        ui.list_flush(l);
        ui.list_home(l);
        self.offset = offset;
        self.waiting.clear();
        self.shortcuts = self
            .source
            .as_ref()
            .map(|s| s.shortcuts.clone())
            .unwrap_or_default();
        for i in 0..self.shortcuts.len() {
            let s = self.shortcuts[i].clone();
            if let Some(e) = self.make_element(ui, index_of(i), &s) {
                ui.list_add(l, e);
                self.check_shortcut(ui, index_of(i), &s, out);
            }
        }
        self.update_padding(ui, out);
        #[allow(clippy::cast_possible_wrap)]
        ui.list_scroll_to_row(l, (self.offset / 10) as i32, true);
        self.update_set_number(ui, out);
    }

    // -----------------------------------------------------------------------------------------
    // Notices
    // -----------------------------------------------------------------------------------------

    /// The shortcuts came again: the set in view and the numbering from the player's options,
    /// then every slot.
    // D2-02539
    fn reload_shortcuts(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        let (set, numbers) = self
            .source
            .as_ref()
            .map_or((0, false), |s| (s.set, s.show_numbers));
        self.offset = (set * 10).saturating_sub(10);
        self.show_numbers = numbers;
        self.init(ui, out);
    }

    /// The options came again: the shortcuts again, then the bar's height from them.
    // D2-02540
    fn reload_options(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        self.reload_shortcuts(ui, out);
        self.refresh_shortcut_height(ui, out);
    }

    /// A recipe is gone: the slots holding it emptied.
    // D2-02499
    fn remove_recipe(&mut self, ui: &mut Ui, recipe: u32, out: &mut Vec<Request>) {
        for i in 0..self.shortcuts.len() {
            let s = &self.shortcuts[i];
            if s.kind == kind::RECIPE && s.data_id == recipe {
                self.remove_shortcut(ui, index_of(i), true, out);
            }
        }
    }

    /// The inventory changed: every item slot (all of it) or the item's slots checked, then every
    /// skill slot.
    // D2-02517
    fn update_inventory(&mut self, ui: &mut Ui, how: u32, item: u64, out: &mut Vec<Request>) {
        if how == INVENTORY_ALL {
            self.check_all_items(ui, out);
        } else {
            self.check_item(ui, item, how == INVENTORY_PENDING, out);
        }
        self.check_all_skills(ui, out);
    }

    /// A container changed: every item slot, or the item's.
    // D2-02518
    fn update_container(&mut self, ui: &mut Ui, how: u32, item: u64, out: &mut Vec<Request>) {
        if how == INVENTORY_ALL {
            self.check_all_items(ui, out);
        } else {
            self.check_item(ui, item, how == INVENTORY_PENDING, out);
        }
    }

    /// A skill changed: its slots checked.
    // D2-02519
    fn update_skill(&mut self, ui: &mut Ui, skill: u32, out: &mut Vec<Request>) {
        self.check_skill(ui, skill, out);
    }

    /// The reset times changed: every skill slot checked.
    // D2-02520
    fn update_reset_time(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        self.check_all_skills(ui, out);
    }

    /// An item went away: its slots checked.
    // D2-02521
    fn being_deleted(&mut self, ui: &mut Ui, item: u64, out: &mut Vec<Request>) {
        self.check_item(ui, item, false, out);
    }

    /// An item came: the slots waiting for it take its look.
    // D2-02526
    fn create_object(&mut self, ui: &mut Ui, item: u64) {
        let mut i = 0;
        while i < self.waiting.len() {
            let (id, e) = self.waiting[i];
            if id == item && ui.node(e).is_some() {
                self.waiting.remove(i);
                self.setup_item_element(ui, item, e);
                continue;
            }
            i += 1;
        }
    }

    fn skill_slots(&self, skill: u32) -> Vec<usize> {
        self.shortcuts
            .iter()
            .enumerate()
            .filter(|(_, s)| s.kind == kind::SKILL && s.data_enum == skill)
            .map(|(i, _)| i)
            .collect()
    }

    /// A special attack asked for (once per number): its skill's slots pending and not dragable.
    // D2-02530
    fn request_special_attack(&mut self, ui: &mut Ui, skill: u32, number: u32) {
        if self.special.iter().any(|(n, _)| *n == number) {
            return;
        }
        self.special.push((number, skill));
        for i in self.skill_slots(skill) {
            let Some(e) = self.list.and_then(|l| ui.list_item(l, i)) else {
                break;
            };
            ui.set_state(e, name::PENDING);
            ui.set_attr(e, attr::DRAGABLE, Value::Bool(false));
        }
    }

    /// A special attack started: its skill's slots active.
    // D2-02531
    fn start_special_attack(&mut self, ui: &mut Ui, number: u32) {
        let Some(&(_, skill)) = self.special.iter().find(|(n, _)| *n == number) else {
            return;
        };
        for i in self.skill_slots(skill) {
            let Some(e) = self.list.and_then(|l| ui.list_item(l, i)) else {
                break;
            };
            ui.set_state(e, name::ACTIVE);
        }
    }

    /// A special attack ended: its skill's slots back to toggled or plain, dragable; the attack
    /// forgotten.
    // D2-02532
    fn end_special_attack(&mut self, ui: &mut Ui, number: u32) {
        let Some(&(_, skill)) = self.special.iter().find(|(n, _)| *n == number) else {
            return;
        };
        for i in self.skill_slots(skill) {
            let Some(e) = self.list.and_then(|l| ui.list_item(l, i)) else {
                return;
            };
            let toggled = ui.node(e).is_some_and(|n| n.bool_attr(name::TOGGLED));
            ui.set_state(
                e,
                if toggled {
                    name::TOGGLED
                } else {
                    state::STATELESS
                },
            );
            ui.set_attr(e, attr::DRAGABLE, Value::Bool(true));
        }
        self.special.retain(|(n, _)| *n != number);
    }

    /// A special attack toggled on or off: its skill's slots toggled or plain.
    // D2-02503
    fn toggle_special_attack(&mut self, ui: &mut Ui, skill: u32, on: bool) {
        for i in self.skill_slots(skill) {
            let Some(e) = self.list.and_then(|l| ui.list_item(l, i)) else {
                break;
            };
            ui.set_state(e, if on { name::TOGGLED } else { state::STATELESS });
            ui.set_attr(e, name::TOGGLED, Value::Bool(on));
        }
    }

    /// A notice of the scripts, taken once the frame's word from them is in.
    fn take_notice(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<Request>) {
        let w = |i: usize| n.words.get(i).copied().unwrap_or(0);
        match n.native {
            play_notice::UPDATE_ATTACK_STATE => self.set_attack_state(ui, n.flag()),
            play_notice::RELOAD_OPTIONS | crate::options::notice::OPTIONS_CHANGED => {
                self.reload_options(ui, out);
            }
            notice::RELOAD_SHORTCUTS => self.reload_shortcuts(ui, out),
            notice::REMOVE_RECIPE => self.remove_recipe(ui, w(0), out),
            notice::UPDATE_SKILL => self.update_skill(ui, w(0), out),
            notice::UPDATE_RESET_TIME => self.update_reset_time(ui, out),
            notice::UPDATE_INVENTORY => {
                self.update_inventory(ui, w(0), long_at(&n.words, 1), out);
            }
            notice::UPDATE_CONTAINER => {
                self.update_container(ui, w(2), long_at(&n.words, 3), out);
            }
            notice::REQUEST_SPECIAL_ATTACK => self.request_special_attack(ui, w(0), w(1)),
            notice::START_SPECIAL_ATTACK => self.start_special_attack(ui, w(0)),
            notice::END_SPECIAL_ATTACK => self.end_special_attack(ui, w(0)),
            notice::TOGGLE_SPECIAL_ATTACK => {
                self.toggle_special_attack(ui, w(0), w(1) & 0xFF != 0);
            }
            _ => {}
        }
    }

    /// The scripts' word for this frame: the first word sets the bar's height from the player's
    /// options; the slots the bar put a shortcut in show what the scripts now say of them; items
    /// that came take their look, items that went have their slots checked.
    fn take_source(&mut self, ui: &mut Ui, s: &Source, out: &mut Vec<Request>) {
        if self.source.as_ref().is_some_and(|o| o.version == s.version) {
            return;
        }
        let before: Vec<u64> = self
            .source
            .as_ref()
            .map(|o| {
                o.items
                    .iter()
                    .filter(|i| i.here.is_some())
                    .map(|i| i.item)
                    .collect()
            })
            .unwrap_or_default();
        let old = self.source.replace(s.clone());
        self.set_drag_out_check(ui, s.locked);
        if !self.started {
            self.started = true;
            self.refresh_shortcut_height(ui, out);
        }
        if let Some(old) = old {
            self.show_answered(ui, &old, s, out);
        }
        for i in &s.items {
            let was = before.contains(&i.item);
            if i.here.is_some() && !was {
                self.create_object(ui, i.item);
            } else if i.here.is_none() && was {
                self.being_deleted(ui, i.item, out);
            }
        }
    }

    /// A slot is made with the tooltip and icon the scripts give for it once they hold its
    /// shortcut. The bar asks them after it puts a shortcut in a slot, and they answer with the
    /// next word: each slot whose shortcut they took from the bar since their last word is made
    /// again with what they now say of it.
    fn show_answered(&mut self, ui: &mut Ui, old: &Source, new: &Source, out: &mut Vec<Request>) {
        let Some(l) = self.list else { return };
        for i in 0..self.shortcuts.len().min(USABLE_SLOTS as usize) {
            let now = new.shortcuts.get(i);
            if now == old.shortcuts.get(i) || now != self.shortcuts.get(i) {
                continue;
            }
            let info = self.shortcuts[i].clone();
            if let Some(e) = self.make_element(ui, index_of(i), &info) {
                ui.list_set_item(l, e, i);
                self.check_shortcut(ui, index_of(i), &info, out);
            }
        }
    }

    fn set_drag_out_check(&mut self, ui: &mut Ui, locked: bool) {
        if self.locked == locked {
            return;
        }
        self.locked = locked;
        if let Some(l) = self.list {
            let check = if locked {
                drag_out_locked
            } else {
                drag_out_unlocked
            };
            ui.set_drag_checks(l, Some(check), Some(drop_in));
        }
    }

    // -----------------------------------------------------------------------------------------
    // Messages
    // -----------------------------------------------------------------------------------------

    /// What the bar heard from its elements: the bar moved by hand; a skill's icon double-clicked
    /// in the skills panel (to the first empty slot in view); the peace and war buttons; a slot let
    /// go on (the left button uses it, the right examines it); a drop on the list (the slot under
    /// the pointer takes it, a slot dragged off is emptied); the list's view moved (the first slot
    /// in view, the list's smooth moves as the options say, the set's number).
    // D2-02533
    fn listen_element(
        &mut self,
        ui: &mut Ui,
        from: Handle,
        element: Token,
        what: Token,
        param: u32,
        out: &mut Vec<Request>,
    ) {
        if element == name::SHORTCUT_FIELD && what == msg::LOCATION_CHANGED {
            self.moved = true;
            return;
        }
        if element == name::SKILL_NODE_ICON && param == button_id::DOUBLE_LEFT {
            if !ui.node(from).is_some_and(|n| n.bool_attr(attr::DRAGABLE)) {
                return;
            }
            if let Some(info) = Self::from_element(ui, from) {
                let end = (self.offset + 10).min(index_of(self.shortcuts.len()));
                let free = (self.offset..end).find(|i| self.shortcuts[*i as usize].kind == 0);
                if let Some(i) = free {
                    self.set_shortcut(ui, i, &info, true, out);
                }
            }
        }
        let Some(l) = self.list else { return };
        if element == name::START_ATTACK_BUTTON {
            self.request_attack_state_change(ui, true, out);
            return;
        }
        if element == name::STOP_ATTACK_BUTTON {
            self.request_attack_state_change(ui, false, out);
            return;
        }
        if element == name::SHORTCUT_BUTTON {
            self.slot_released(ui, l, from, param, out);
            return;
        }
        if element != name::SHORTCUT_LIST_BOX {
            return;
        }
        if what == name::STOP_DRAG_ITEM {
            self.dropped(ui, l, param != 0, out);
        }
        if what == name::NEW_VIEW {
            let first = ui
                .screen_rect(l)
                .and_then(|r| ui.list_index_at(l, r.x0, r.y0))
                .unwrap_or(0);
            self.offset = index_of(first);
            let smooth = ui
                .node(l)
                .is_some_and(|n| n.bool_attr(name::SMOOTH_MOVEMENT));
            let insta = self.insta_scroll();
            if smooth == insta {
                ui.set_attr(l, name::SMOOTH_MOVEMENT, Value::Bool(!insta));
            }
            self.update_set_number(ui, out);
        }
    }

    /// A slot's button let go on: the left button uses the slot, the right examines what it
    /// holds (a skill, an item, a recipe).
    fn slot_released(
        &mut self,
        ui: &mut Ui,
        l: Handle,
        button: Handle,
        param: u32,
        out: &mut Vec<Request>,
    ) {
        let Some(slot) = ui.node(button).and_then(|n| n.parent) else {
            return;
        };
        let Some(i) = ui.list_index_of(l, slot).map(index_of) else {
            return;
        };
        if i >= USABLE_SLOTS {
            return;
        }
        if param == button_id::LEFT {
            out.push(Request::UseSlot(i));
            return;
        }
        if param != button_id::RIGHT {
            return;
        }
        let s = self.shortcuts.get(i as usize).cloned().unwrap_or_default();
        let e = match s.kind {
            kind::SKILL => Examination {
                kind: examine::SKILL,
                data_enum: s.data_enum,
                ..Examination::default()
            },
            kind::ITEM => Examination {
                kind: examine::ITEM,
                item: s.item,
                selection: s.item,
                ..Examination::default()
            },
            kind::RECIPE => Examination {
                kind: examine::RECIPE,
                data_id: s.data_id,
                ..Examination::default()
            },
            _ => return,
        };
        out.push(Request::Examine(e));
        out.push(Request::OpenExaminationPanel);
    }

    /// Something dropped: taken by the list, the slot under the pointer gets its shortcut (the
    /// slot it came from, on the bar, emptied first); not taken, the bar's slot it came from is
    /// emptied.
    fn dropped(&mut self, ui: &mut Ui, l: Handle, taken: bool, out: &mut Vec<Request>) {
        let Some(item) = ui.dropped() else { return };
        let came_from = ui
            .node(item)
            .and_then(|n| n.int_attr(name::SHORTCUT_INDEX))
            .and_then(|i| u32::try_from(i).ok());
        if !taken {
            if let Some(i) = came_from {
                self.remove_shortcut(ui, i, true, out);
            }
            return;
        }
        if let Some(i) = came_from {
            self.remove_shortcut(ui, i, true, out);
        }
        let Some(under) = ui.listbox_item_under_mouse(l) else {
            return;
        };
        let Some(to) = ui.list_index_of(l, under).map(index_of) else {
            return;
        };
        if let Some(info) = Self::from_element(ui, item) {
            self.set_shortcut(ui, to, &info, true, out);
        }
    }

    /// What the bar heard of the manager: the keys (use a slot of the set in view, or of its
    /// second row; show a set, the next or the one before; the attack key), the clock (the waits;
    /// after a resize by hand, the nearest whole number of rows, the bar fitted to them and the
    /// rows said), and the display being set up again (after which the height is the options').
    // D2-02537
    fn listen_global(&mut self, ui: &mut Ui, what: u32, param: u32, out: &mut Vec<Request>) {
        let (Some(root), Some(l)) = (self.root, self.list) else {
            return;
        };
        match what {
            global::KEY_DOWN => self.key(ui, l, param, out),
            global::TIME => {
                self.update_time_left(ui);
                if !self.moved || self.refreshing {
                    return;
                }
                self.fit_rows(ui, root, l, out);
                self.moved = false;
            }
            global::REFRESH_START => self.refreshing = true,
            global::REFRESH_DONE => {
                self.refreshing = false;
                ui.set_attr(l, name::MAX_ROWS, Value::Int(0));
                self.refresh_shortcut_height(ui, out);
            }
            _ => {}
        }
    }

    /// After a resize by hand: the nearest whole number of rows, the bar fitted to them within its
    /// least and greatest height, the list to its inside, the slots filled out, the rows said.
    fn fit_rows(&mut self, ui: &mut Ui, root: Handle, l: Handle, out: &mut Vec<Request>) {
        if !ui.shown(root) {
            return;
        }
        let n = ui.node(root);
        let get = |a| n.and_then(|n| n.int_attr(a));
        let (Some(border), Some(row_h), Some(min_h), Some(max_h)) = (
            get(name::BORDER_SIZE),
            get(name::ROW_HEIGHT),
            get(attr::MIN_HEIGHT),
            get(attr::MAX_HEIGHT),
        ) else {
            return;
        };
        let Some(r) = ui.screen_rect(root) else {
            return;
        };
        if row_h == 0 {
            return;
        }
        let (w, h) = (r.width(), r.height());
        #[allow(clippy::cast_precision_loss)]
        let rows = dereth_primitives::num::to_i32((h - border) as f32 / row_h as f32 + 0.5);
        let mut off = h - rows * row_h - border;
        let mut new_h = h;
        if off != 0 {
            new_h = h - off;
            if new_h < min_h {
                off += min_h - new_h;
                new_h = min_h;
            }
            if new_h > max_h {
                off += new_h - max_h;
                new_h = max_h;
            }
            if off != 0 {
                let y = r.y0 + off;
                ui.move_to(root, r.x0, y);
                ui.resize_to(root, w, new_h);
                ui.move_to(root, r.x0, y);
            }
        }
        ui.set_attr(l, name::MAX_ROWS, Value::Int(rows));
        ui.resize_to(l, w, new_h - border);
        self.update_padding(ui, out);
        Self::rows_changed(rows, out);
    }

    /// The bar's height in rows changed by hand: said to those who keep it.
    // D2-00172
    fn rows_changed(rows: i32, out: &mut Vec<Request>) {
        out.push(Request::ShortcutRows(rows));
    }

    /// An interface key.
    fn key(&mut self, ui: &mut Ui, l: Handle, key: u32, out: &mut Vec<Request>) {
        match key {
            action::AUTOATTACK => {
                let start = !self.attacking;
                self.request_attack_state_change(ui, start, out);
            }
            k if (action::SHORTCUT_1..=action::SHORTCUT_0).contains(&k) => {
                out.push(Request::UseSlot(self.offset + (k - action::SHORTCUT_1)));
            }
            k if (action::SHORTCUT_B_1..=action::SHORTCUT_B_0).contains(&k) => {
                let i = self.offset + 10 + (k - action::SHORTCUT_B_1);
                let rows = ui
                    .node(l)
                    .and_then(|n| n.int_attr(name::MAX_ROWS))
                    .unwrap_or(0);
                if i < USABLE_SLOTS && rows > 1 {
                    out.push(Request::UseSlot(i));
                }
            }
            k if (action::SET_1..=action::SET_0).contains(&k) => {
                #[allow(clippy::cast_possible_wrap)]
                let set = (k - action::SET_1) as i32;
                ui.list_scroll_to_row(l, set, self.insta_scroll());
            }
            action::SET_NEXT | action::SET_PREV => {
                let step = if key == action::SET_NEXT { 1 } else { -1 };
                #[allow(clippy::cast_possible_wrap)]
                let set = (self.offset / 10) as i32 + step;
                let set = if set < 0 {
                    9
                } else if set > 9 {
                    0
                } else {
                    set
                };
                ui.list_scroll_to_row(l, set, self.insta_scroll());
            }
            _ => {}
        }
    }
}

/// The list lets a slot be dragged off unless it is not a real slot.
// D2-02494
fn drag_out_unlocked(ui: &Ui, item: Handle, _list: Handle) -> bool {
    ui.node(item)
        .and_then(|n| n.int_attr(name::SHORTCUT_INDEX))
        .is_some_and(|i| i < 100)
}

/// While the shortcuts are locked, a slot is dragged off only with Shift held.
fn drag_out_locked(ui: &Ui, item: Handle, list: Handle) -> bool {
    drag_out_unlocked(ui, item, list) && ui.shift_held()
}

/// The list takes in only something with a shortcut kind, over a real slot.
// D2-02493
fn drop_in(ui: &Ui, item: Handle, list: Handle) -> bool {
    let kind = ui
        .node(item)
        .and_then(|n| n.id_attr(name::SHORTCUT_TYPE))
        .unwrap_or(0);
    let Some(under) = ui.listbox_item_under_mouse(list) else {
        return false;
    };
    let index = ui
        .node(under)
        .and_then(|n| n.int_attr(name::SHORTCUT_INDEX))
        .unwrap_or(i32::MAX);
    index < 100 && kind != 0
}

impl Shortcuts {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[
        0x1_040B, 0x40F, 0x417, 0x46D, 0x497, 0x405, 0x406, 0x407, 0x465, 0x466, 0x413, 0x47C,
        0x415, 0x416, 0x1_040C,
    ];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for Shortcuts {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// The bar shown or hidden as asked.
    // D2-02489
    fn show(&mut self, ui: &mut Ui, on: bool, _out: &mut Vec<Request>) {
        if let Some(r) = self.root {
            ui.set_visible(r, on);
        }
    }

    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<Request>) {
        match *mail {
            Mail::Element {
                from,
                element,
                what,
                param,
            } => self.listen_element(ui, from, element, what, param, out),
            Mail::Global { what, param } => self.listen_global(ui, what, param, out),
            _ => {}
        }
    }

    /// Notices wait for this frame's word from the scripts (the bar asks them as it acts).
    fn notice(&mut self, _ui: &mut Ui, n: &Notice, _out: &mut Vec<Request>) {
        self.noticed.push(n.clone());
    }

    /// This frame's word from the scripts, then the notices that waited for it.
    fn update(&mut self, ui: &mut Ui, view: &View<'_>, out: &mut Vec<Request>) {
        if let Some(w) = view.world {
            self.auto_target = w.should_auto_target;
        }
        if let Some(s) = view.shortcuts {
            self.take_source(ui, s, out);
        }
        for n in std::mem::take(&mut self.noticed) {
            self.take_notice(ui, &n, out);
        }
    }

    /// The bar goes: its slots, its root, its listening.
    // D2-02525, D2-02528
    fn close(self: Box<Self>, ui: &mut Ui) {
        if let Some(l) = self.list {
            ui.list_flush(l);
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
