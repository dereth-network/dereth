//! The ingredient panel: a recipe opened for making, its ingredients' slots, the spinner, the
//! craft button.
//!
//! The scripts open it on a recipe (unless it is up and a making is under way): the recipe's name,
//! its charges (when it has some), its cost, a slot per ingredient it takes at the spinner's value
//! (in order of the ingredients' numbers, each named), the spinner set to the recipe's last value
//! (hidden when the recipe has none; its arrows ghosted at the ends and when it may not be turned;
//! typed into, the value is taken two seconds after the last key or when the field is left, kept
//! within its bounds), the fill-in button when the recipe has ingredients that fill themselves in,
//! the name colored by how hard the recipe is against the craft skill (from the difficulty less
//! the skill, -50 to 50, to five colors; the sixth when the recipe is past the skill's cutoff;
//! the recipe's own color when it has a coloring table), the panel as tall as its slots (from the
//! fourth on, a slot's height each, to its greatest). Its texts are string references its text
//! elements write: a recipe's or an ingredient's name whose string tables the data lacks shows
//! once they have come.
//!
//! - **An item dropped on a slot** (only an item from somewhere, not one already in the panel
//!   elsewhere than that slot) is asked to go in: the slot waits for the scripts, which say
//!   whether it went; it then shows the item's icon and may be dragged out (an item moved from
//!   another slot leaves that one). An item dragged out of a slot leaves it.
//! - **The spinner** changed: the cost and difficulty asked again when they move with it, the craft
//!   button, the slots (those the recipe no longer takes go, new ones come; a slot's item checked
//!   again: one that no longer fits, or the player no longer has, leaves).
//! - **Fill in**: for the player's items in order, the first that fits each empty slot that fills
//!   itself in goes in (one item per slot).
//! - **The craft button** (ghosted while a making is under way, while mounted, when the spinner is
//!   out of its bounds or the recipe is past the skill's cutoff, each with a tip saying why)
//!   asks to make the recipe from the items in the slots; while it is under way the slots wait and
//!   their items may not be dragged out, and the panel will not close. When it is done: closed if
//!   it went and the recipe has no charges left, else its charges shown again.
//! - An item in a slot changed, moved or went: checked again; one that is gone leaves its slot.
//!
//! What the panel reads of the scripts comes with the frame (the recipe at a spinner value: its
//! slots, cost, difficulty, which of the player's items fit which slot); a spinner value it has
//! no answer for yet is asked for, and the slots follow when the answer comes.

use std::any::Any;
use std::collections::BTreeMap;

use auberean_text::{StringInfo, Value as Var};
use auberean_ui::framework::FrameId;
use auberean_ui::manager::global;
use auberean_ui::token::{attr, msg, state};
use auberean_ui::types::text::{number_input_filter, text_msg};
use auberean_ui::{Handle, Mail, Token, Ui, Value};

use crate::flow;
use crate::gameplay::{self, Child, Notice, Request, PLAY_LAYOUT_ENUM};
use crate::inventory;

/// The scripts' notices to the panel.
pub mod notice {
    /// An ingredient was put in (or not): the recipe, the slot, the item (high and low), the
    /// spinner's value, 0 when it went.
    pub const ADD_INGREDIENT_DONE: u32 = 0x0088 << 16 | 0x0002;
    /// The making is done: the recipe, 0 when it went.
    pub const EXECUTE_CRAFT_DONE: u32 = 0x0088 << 16 | 0x0003;
    /// Open the panel on a recipe.
    pub const OPEN_INGREDIENT_PANEL: u32 = 0x0088 << 16 | 0x0005;
    /// The player mounted (1) or got off (0).
    pub const UPDATE_MOUNT_STATE: u32 = crate::gameplay::notice::UPDATE_MOUNT_STATE;
    /// An object is going away.
    pub const BEING_DELETED: u32 = 0x1_040C;
}

mod name {
    use auberean_ui::Token;
    pub const FIELD: Token = Token::of("RealIngredientField");
    pub const LABEL: Token = Token::of("RecipeLabel");
    pub const MISC: Token = Token::of("RecipeMiscText");
    pub const COST: Token = Token::of("CostField");
    pub const LIST: Token = Token::of("IngredientListBox");
    pub const SPINNER: Token = Token::of("SpinnerField");
    pub const UP: Token = Token::of("SpinnerUpButton");
    pub const DOWN: Token = Token::of("SpinnerDownButton");
    pub const VALUE: Token = Token::of("SpinnerValueField");
    pub const AUTO: Token = Token::of("AutoPopulateButton");
    pub const CRAFT: Token = Token::of("CraftButton");
    pub const CRAFT_TEXT: Token = Token::of("CraftButtonText");
    pub const CLOSE: Token = Token::of("IngredientCloseButton");
    pub const TEMPLATE: Token = Token::of("IngredientTemplate");
    pub const ITEM: Token = Token::of("IngredientItem");
    pub const ITEM_ICON: Token = Token::of("ItemIcon");
    pub const ITEM_LABEL: Token = Token::of("IngredientLabel");
    pub const ID: Token = Token::of("id");
    pub const PENDING: Token = Token::of("pending");
    pub const NORMAL: Token = Token::of("normal");
    pub const VAR_VALUE: Token = Token::of("value");
    pub const STOP_DRAG_ITEM: Token = Token::of("stop_drag_item");
    pub const SHORTCUT_TYPE: Token = Token::of("ShortcutType");
    pub const SHORTCUT_DATA: Token = Token::of("ShortcutData");
}

/// An ingredient a recipe takes: its number (its slot), and what it is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ingredient {
    pub id: u32,
    pub description: StringInfo,
}

/// A recipe as the panel reads it at a spinner value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecipeAt {
    pub recipe: u32,
    pub spinner: i32,
    pub name: StringInfo,
    /// -1 for none.
    pub charges: i32,
    pub min_spinner: i32,
    pub max_spinner: i32,
    pub last_spinner: i32,
    pub auto_populating: bool,
    pub max_ingredients: u32,
    pub name_coloring: bool,
    pub name_color: u32,
    pub difficulty: u32,
    pub dynamic_difficulty: bool,
    /// The difficulty less the skill's cutoff, when it has one.
    pub cutoff: Option<u32>,
    pub cost: i32,
    pub dynamic_cost: bool,
    pub craft_skill: u32,
    pub read_only_spinner: bool,
    /// The craft skill's level as it stands.
    pub skill_level: u32,
    /// The slots the recipe takes at this value (a bit per slot number from 1), and the
    /// ingredients.
    pub mask: u32,
    pub ingredients: Vec<Ingredient>,
    /// The cost, and the difficulty and name's color, at this value (when the scripts answer).
    pub spinner_cost: Option<i32>,
    pub spinner_difficulty: Option<(u32, u32)>,
    /// Whether each slot fills itself in.
    pub auto_populate: BTreeMap<u32, bool>,
    /// The player's items, in the scripts' order.
    pub items: Vec<u64>,
    /// Whether an item (the player having it) fits a slot.
    pub fits: BTreeMap<(u32, u64), bool>,
}

/// What the panel asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IngredientRequest {
    /// The recipe at a spinner value (0 for the recipe: the panel closed).
    Inquire { recipe: u32, spinner: i32 },
    /// Put an item in a slot.
    Add {
        recipe: u32,
        ordinal: u32,
        item: u64,
        spinner: i32,
    },
    /// Make the recipe from the items in the slots, by slot.
    Execute {
        recipe: u32,
        spinner: i32,
        ingredients: Vec<(u32, u64)>,
    },
}

/// The messages' string table.
fn entry(ui: &Ui, id: &str) -> StringInfo {
    let table = ui
        .assets()
        .data_id_by_enum(flow::MESSAGES, flow::TABLE_MAPPING);
    StringInfo::entry(table, Token::of(id).raw())
}

/// A value kept between two bounds, mapped onto another range.
// D2-02190
fn scale(v: f32, from_lo: f32, from_hi: f32, to_lo: f32, to_hi: f32) -> f32 {
    let v = if v < from_lo {
        from_lo
    } else if from_hi < v {
        from_hi
    } else {
        v
    };
    (to_hi - to_lo) * ((v - from_lo) / (from_hi - from_lo)) + to_lo
}

/// A slot's bit in the mask of slots.
fn bit(ordinal: u32) -> u32 {
    ordinal
        .checked_sub(1)
        .and_then(|b| 1u32.checked_shl(b))
        .unwrap_or(0)
}

/// Only an item from somewhere may be dropped on a slot.
// D2-02188
fn drop_check(ui: &Ui, item: Handle, _list: Handle) -> bool {
    let Some(n) = ui.node(item) else { return false };
    n.id_attr(name::SHORTCUT_TYPE) == Some(crate::shortcuts::kind::ITEM)
        && n.instance_id_attr(inventory::attr::FROM_ID)
            .is_some_and(|f| f != inventory::NO_OBJECT)
}

/// The ingredient panel.
#[derive(Debug)]
pub struct Ingredients {
    frame: FrameId,
    root: Option<Handle>,
    label: Option<Handle>,
    misc: Option<Handle>,
    cost_field: Option<Handle>,
    list: Option<Handle>,
    spinner_field: Option<Handle>,
    up: Option<Handle>,
    down: Option<Handle>,
    value: Option<Handle>,
    auto: Option<Handle>,
    craft: Option<Handle>,
    craft_text: Option<Handle>,
    /// The close question up.
    dialog: u32,
    recipe: u32,
    skill: u32,
    min: i32,
    max: i32,
    spinner: i32,
    auto_populating: bool,
    max_ingredients: u32,
    name_coloring: bool,
    name_color: u32,
    difficulty: u32,
    dynamic_difficulty: bool,
    cutoff: Option<u32>,
    skill_level: u32,
    cost: i32,
    dynamic_cost: bool,
    read_only: bool,
    /// The items in the slots, by slot, and the slot of each.
    slot_items: BTreeMap<u32, u64>,
    item_slots: BTreeMap<u64, u32>,
    /// The slots the panel has, and each slot's row.
    mask: u32,
    rows: BTreeMap<u32, Handle>,
    /// The spinner typed into: taken at this game time.
    typed: Option<f64>,
    mounted: bool,
    /// A making is under way.
    pending: bool,
    /// What the scripts last answered of the recipe.
    facts: Option<RecipeAt>,
    /// The game time.
    now: f64,
    /// Waiting for the answer at a spinner value, and what then follows: the panel opened, or the
    /// spinner changed.
    waiting: Option<(i32, Waiting)>,
}

/// What waits for the scripts' answer at a spinner value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Waiting {
    Open,
    Spinner,
}

impl Ingredients {
    /// The panel under the play screen's framework, its root and elements found, its buttons,
    /// the spinner and the slots' drops heard; hidden.
    // D2-02229
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        let root = ui.frame_create_root(frame, PLAY_LAYOUT_ENUM, name::FIELD);
        let find = |ui: &Ui, n| root.and_then(|r| ui.child_recursive(r, n));
        let me = Self {
            frame,
            root,
            label: find(ui, name::LABEL),
            misc: find(ui, name::MISC),
            cost_field: find(ui, name::COST),
            list: find(ui, name::LIST),
            spinner_field: find(ui, name::SPINNER),
            up: find(ui, name::UP),
            down: find(ui, name::DOWN),
            value: find(ui, name::VALUE),
            auto: find(ui, name::AUTO),
            craft: find(ui, name::CRAFT),
            craft_text: find(ui, name::CRAFT_TEXT),
            dialog: 0,
            recipe: 0,
            skill: 0,
            min: 0,
            max: 0,
            spinner: 0,
            auto_populating: false,
            max_ingredients: 0,
            name_coloring: false,
            name_color: 0,
            difficulty: 0,
            dynamic_difficulty: false,
            cutoff: None,
            skill_level: 0,
            cost: 0,
            dynamic_cost: false,
            read_only: false,
            slot_items: BTreeMap::new(),
            item_slots: BTreeMap::new(),
            mask: 0,
            rows: BTreeMap::new(),
            typed: None,
            mounted: false,
            pending: false,
            facts: None,
            now: 0.0,
            waiting: None,
        };
        if let Some(v) = me.value {
            ui.set_input_filter(v, Some(number_input_filter));
        }
        if let Some(l) = me.list {
            ui.set_drag_checks(l, None, Some(drop_check));
        }
        if let Some(l) = ui.frame_listener(frame) {
            ui.listen_for_global_message(l, global::TIME);
            for (e, m) in [
                (name::CLOSE, msg::BUTTON_RELEASE_ON),
                (name::UP, msg::HOT_CLICK),
                (name::DOWN, msg::HOT_CLICK),
                (name::AUTO, msg::BUTTON_RELEASE_ON),
                (name::CRAFT, msg::BUTTON_RELEASE_ON),
                (name::LIST, name::STOP_DRAG_ITEM),
                (name::ITEM, name::STOP_DRAG_ITEM),
                (name::VALUE, text_msg::KEY_INPUT),
                (name::VALUE, text_msg::CHARACTER_INPUT),
                (name::VALUE, text_msg::DEACTIVATED),
            ] {
                ui.listen_for_message(l, e, m);
            }
        }
        let mut me = me;
        me.show(ui, false, &mut Vec::new());
        me
    }

    /// The recipe open (0 for none), the spinner's value, the items in the slots by slot.
    #[must_use]
    pub fn state(&self) -> (u32, i32, &BTreeMap<u32, u64>) {
        (self.recipe, self.spinner, &self.slot_items)
    }

    /// The scripts' answer as it stands this frame, taken in when it is new.
    fn take_answer(&mut self, ui: &mut Ui, view: &gameplay::View<'_>, out: &mut Vec<Request>) {
        let Some(k) = view.craft else { return };
        self.now = k.now;
        let fresh = k
            .opened
            .as_ref()
            .filter(|f| Some(*f) != self.facts.as_ref())
            .cloned();
        if let Some(f) = fresh {
            let level_changed = self
                .facts
                .as_ref()
                .is_some_and(|o| o.recipe == f.recipe && o.skill_level != f.skill_level);
            self.facts = Some(f.clone());
            match self.waiting {
                Some((_, Waiting::Open)) if f.recipe == self.recipe => {
                    self.waiting = None;
                    self.init(ui, &f, out);
                }
                Some((s, Waiting::Spinner)) if f.recipe == self.recipe && f.spinner == s => {
                    self.waiting = None;
                    self.recipe_at_spinner(ui, out);
                }
                _ if f.recipe == self.recipe && f.spinner == self.spinner => {
                    if level_changed {
                        self.skill_level = f.skill_level;
                        self.update_craft_button(ui);
                        if let Some((d, c)) = f.spinner_difficulty {
                            self.difficulty = d;
                            self.name_color = c;
                        }
                        self.update_apparent_difficulty(ui);
                    }
                    for (ordinal, item) in self.slot_items.clone() {
                        self.check_item(ui, item, ordinal, out);
                    }
                }
                _ => {}
            }
        }
    }

    /// The answer at the spinner's value, when it is in.
    fn at(&self, spinner: i32) -> Option<&RecipeAt> {
        self.facts
            .as_ref()
            .filter(|f| f.recipe == self.recipe && f.spinner == spinner)
    }

    /// A making's under-way look: the slots wait (their items not to be dragged), the craft
    /// button follows.
    // D2-02195
    fn set_pending(&mut self, ui: &mut Ui, on: bool) {
        if self.root.is_none() {
            return;
        }
        self.pending = on;
        let look = if on { name::PENDING } else { state::STATELESS };
        for row in self.rows.values().copied().collect::<Vec<_>>() {
            ui.set_state(row, look);
            let Some(icon) = ui.child_recursive(row, name::ITEM_ICON) else {
                return;
            };
            ui.set_attr(icon, attr::DRAGABLE, Value::Bool(!on));
        }
        self.update_craft_button(ui);
    }

    /// The cost shown.
    // D2-02189
    fn update_cost(&mut self, ui: &mut Ui, cost: i32) {
        self.cost = cost;
        let Some(f) = self.cost_field else { return };
        let mut s = entry(ui, "ID_Craft_Cost");
        s.add_var(name::VAR_VALUE.raw(), Var::Int(cost));
        ui.set_string_info(f, &s);
    }

    /// The craft button: ghosted while a making is under way; while mounted, when the spinner is
    /// out of its bounds or the recipe is past the skill's cutoff, ghosted with why as its tip;
    /// else lit (unless it already is).
    // D2-02187
    fn update_craft_button(&mut self, ui: &mut Ui) {
        let Some(b) = self.craft else { return };
        let ghosted = ui.node(b).is_some_and(|n| n.bool_attr(attr::GHOSTED));
        if self.pending {
            ui.set_attr(b, attr::GHOSTED, Value::Bool(true));
            return;
        }
        let why = if self.mounted {
            Some("ID_Craft_TT_Mounted")
        } else if self.spinner < self.min || self.max < self.spinner {
            Some("ID_Craft_TT_InvalidSpinner")
        } else if self.past_cutoff() {
            Some("ID_Craft_TT_TooHard")
        } else {
            None
        };
        let label = |ui: &mut Ui, id| {
            if let Some(t) = self.craft_text {
                let s = entry(ui, id);
                ui.set_string_info(t, &s);
            }
        };
        match why {
            Some(tip) => {
                ui.set_attr(b, attr::GHOSTED, Value::Bool(true));
                label(ui, "ID_Craft_CraftButton_Disabled");
                let tip = entry(ui, tip);
                ui.set_tooltip_info(b, &tip);
            }
            None if !ghosted => {}
            None => {
                ui.set_attr(b, attr::GHOSTED, Value::Bool(false));
                label(ui, "ID_Craft_CraftButton");
                ui.clear_element_tooltip(b);
            }
        }
    }

    fn past_cutoff(&self) -> bool {
        #[allow(clippy::cast_possible_wrap)]
        let over = self.difficulty.wrapping_sub(self.skill_level) as i32;
        self.cutoff.is_some_and(|c| {
            #[allow(clippy::cast_possible_wrap)]
            let c = c as i32;
            c < over
        })
    }

    /// The recipe's name colored by how hard it is against the craft skill.
    // D2-02197
    fn update_apparent_difficulty(&mut self, ui: &mut Ui) {
        let Some(l) = self.label else { return };
        if self.past_cutoff() {
            self.name_color = 6;
        } else if !self.name_coloring {
            if self.difficulty == 0 {
                self.name_color = 0;
            } else {
                #[allow(clippy::cast_precision_loss)]
                let diff = self.difficulty as f32 - self.skill_level as f32;
                let c = scale(diff + 42.0, -50.0, 50.0, 0.0, 5.0);
                // Out of range, the conversion's indefinite value: the last color.
                let c = dereth_primitives::num::to_i32(c).cast_unsigned();
                self.name_color = c.min(5);
            }
        }
        let t = ui.text(l).unwrap_or_default();
        ui.set_string_with_font(l, &t, 0, self.name_color);
    }

    /// The panel as tall as its slots: its least height, and from the fourth slot on a slot's
    /// height more each, to its greatest; its least width.
    // D2-02194
    fn auto_size(&mut self, ui: &mut Ui) {
        let Some(root) = self.root else { return };
        let int = |ui: &Ui, a| ui.node(root).and_then(|n| n.int_attr(a)).unwrap_or(0);
        let w = int(ui, attr::MIN_WIDTH);
        let mut h = int(ui, attr::MIN_HEIGHT);
        let most = int(ui, attr::MAX_HEIGHT);
        if self.max_ingredients > 3 {
            let Some(t) = ui.frame_create_element(PLAY_LAYOUT_ENUM, name::TEMPLATE) else {
                return;
            };
            let row = ui.node(t).map_or(0, |n| n.rect.height());
            ui.remove(t);
            #[allow(clippy::cast_possible_wrap)]
            let more = row * (self.max_ingredients - 3) as i32;
            h = (more + h).min(most);
        }
        ui.resize_to(root, w, h);
    }

    /// Opened on a recipe: everything from the scripts' answer at the recipe's last spinner
    /// value; shown.
    // D2-02224
    fn init(&mut self, ui: &mut Ui, f: &RecipeAt, out: &mut Vec<Request>) {
        if self.label.is_none() || self.misc.is_none() {
            return;
        }
        let Some(list) = self.list else { return };
        self.pending = false;
        ui.list_flush(list);
        self.rows.clear();
        self.slot_items.clear();
        self.item_slots.clear();
        self.recipe = f.recipe;
        self.skill = f.craft_skill;
        self.spinner = f.last_spinner;
        self.min = f.min_spinner;
        self.max = f.max_spinner;
        self.read_only = f.read_only_spinner;
        if let Some(s) = self.spinner_field {
            ui.set_visible(s, !(f.min_spinner == 0 && f.max_spinner == 0));
        }
        if !(f.min_spinner == 0 && f.max_spinner == 0) {
            if let Some(v) = self.value {
                ui.set_text(v, &self.spinner.to_string());
                ui.set_attr(v, attr::EDITABLE, Value::Bool(!self.read_only));
            }
            if let Some(d) = self.down {
                let g = self.spinner == self.min || self.read_only;
                ui.set_attr(d, attr::GHOSTED, Value::Bool(g));
            }
            if let Some(u) = self.up {
                let g = self.spinner == self.max || self.read_only;
                ui.set_attr(u, attr::GHOSTED, Value::Bool(g));
            }
        }
        self.auto_populating = f.auto_populating;
        if f.auto_populating {
            if let Some(a) = self.auto {
                ui.set_state(a, name::NORMAL);
                ui.set_visible(a, true);
            }
        }
        self.max_ingredients = f.max_ingredients;
        self.name_color = f.name_color;
        self.name_coloring = f.name_coloring;
        self.difficulty = f.difficulty;
        self.cutoff = f.cutoff;
        self.dynamic_difficulty = f.dynamic_difficulty;
        self.dynamic_cost = f.dynamic_cost;
        self.update_cost(ui, f.cost);
        if let Some(m) = self.misc {
            if f.charges > 0 {
                let mut s = entry(ui, "ID_Craft_Charges");
                s.add_var(name::VAR_VALUE.raw(), Var::Int(f.charges));
                ui.set_string_info(m, &s);
                ui.set_visible(m, true);
            } else {
                ui.set_visible(m, false);
            }
        }
        if let Some(l) = self.label {
            ui.set_string_info(l, &f.name);
        }
        self.skill_level = f.skill_level;
        self.mask = f.mask;
        for ing in &f.ingredients {
            self.add_slot(ui, ing);
        }
        ui.list_changes_done(list);
        self.update_craft_button(ui);
        self.update_apparent_difficulty(ui);
        self.auto_size(ui);
        self.show(ui, true, out);
    }

    /// A slot for an ingredient, among the slots in order of their numbers: its number on it and
    /// its icon, its name.
    // D2-02216
    fn add_slot(&mut self, ui: &mut Ui, ing: &Ingredient) {
        let Some(list) = self.list else { return };
        let Some(row) = ui.frame_create_element(PLAY_LAYOUT_ENUM, name::TEMPLATE) else {
            return;
        };
        let (Some(icon), Some(label)) = (
            ui.child_recursive(row, name::ITEM_ICON),
            ui.child_recursive(row, name::ITEM_LABEL),
        ) else {
            return;
        };
        ui.set_attr(row, name::ID, Value::Id(ing.id));
        ui.set_attr(icon, name::ID, Value::Id(ing.id));
        ui.set_string_info(label, &ing.description);
        let at = self
            .rows
            .range(ing.id + 1..)
            .next()
            .and_then(|(_, r)| ui.list_index_of(list, *r))
            .unwrap_or_else(|| ui.list_items(list).len());
        ui.list_insert(list, row, at);
        self.rows.insert(ing.id, row);
        self.mask |= bit(ing.id);
    }

    /// A slot taken away (its item leaving it first).
    // D2-02225
    fn remove_slot(&mut self, ui: &mut Ui, ordinal: u32) {
        if let Some(item) = self.slot_items.get(&ordinal).copied() {
            self.item_removal(ui, item);
        }
        let Some(row) = self.rows.remove(&ordinal) else {
            return;
        };
        if let Some(list) = self.list {
            if let Some(i) = ui.list_index_of(list, row) {
                ui.list_remove(list, i);
            }
        }
        self.mask &= !bit(ordinal);
    }

    /// A slot's name again, and its item checked again.
    // D2-02226
    fn update_slot(&mut self, ui: &mut Ui, ing: &Ingredient, out: &mut Vec<Request>) {
        let Some(row) = self.rows.get(&ing.id).copied() else {
            return;
        };
        let (Some(icon), Some(label)) = (
            ui.child_recursive(row, name::ITEM_ICON),
            ui.child_recursive(row, name::ITEM_LABEL),
        ) else {
            return;
        };
        ui.set_attr(row, name::ID, Value::Id(ing.id));
        ui.set_attr(icon, name::ID, Value::Id(ing.id));
        ui.set_string_info(label, &ing.description);
        if let Some(item) = self.slot_items.get(&ing.id).copied() {
            self.check_item(ui, item, ing.id, out);
        }
    }

    /// The slots the recipe takes at the spinner's value: each it takes kept (named again) or
    /// added, each it no longer takes gone.
    // D2-02228
    fn update_ingredients(&mut self, ui: &mut Ui, f: &RecipeAt, out: &mut Vec<Request>) {
        for ing in &f.ingredients {
            if self.mask & bit(ing.id) == 0 {
                self.add_slot(ui, ing);
            } else {
                self.update_slot(ui, ing, out);
            }
        }
        let had = self.mask;
        for ordinal in 1..=32 {
            if had & !f.mask & bit(ordinal) != 0 {
                self.remove_slot(ui, ordinal);
            }
        }
        if let Some(list) = self.list {
            ui.list_changes_done(list);
        }
    }

    /// Whether an item fits a slot: the player has it and the recipe takes it there; one in a
    /// slot that does not fit leaves it.
    // D2-02218
    fn check_item(
        &mut self,
        ui: &mut Ui,
        item: u64,
        ordinal: u32,
        _out: &mut Vec<Request>,
    ) -> bool {
        let fits = self
            .at(self.spinner)
            .and_then(|f| f.fits.get(&(ordinal, item)).copied())
            .unwrap_or(false);
        if !fits && self.item_slots.contains_key(&item) {
            self.item_removal(ui, item);
        }
        fits
    }

    /// An item leaves its slot: its icon cleared, no longer dragable.
    // D2-02217
    fn item_removal(&mut self, ui: &mut Ui, item: u64) {
        let Some(ordinal) = self.item_slots.get(&item).copied() else {
            return;
        };
        let Some(row) = self.rows.get(&ordinal).copied() else {
            return;
        };
        ui.set_state(row, state::STATELESS);
        let Some(icon) = ui.child_recursive(row, name::ITEM_ICON) else {
            return;
        };
        ui.clear_image(icon);
        ui.set_attr(icon, attr::DRAGABLE, Value::Bool(false));
        self.slot_items.remove(&ordinal);
        self.item_slots.remove(&item);
    }

    /// The spinner's value taken: written when the field reads otherwise (both as unsigned
    /// words: the field read as `wcstoul` reads, the value written unsigned, so a negative one
    /// shows as its word, -1 as 4294967295), its arrows ghosted at its ends, then the recipe at
    /// the value (asked for when its answer is not in).
    // D2-02231
    fn spinner_changed(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        if let Some(v) = self.value {
            let text: Vec<u16> = ui.text(v).unwrap_or_default().encode_utf16().collect();
            #[allow(clippy::cast_possible_truncation)]
            let shown = auberean_text::wide::scan(&text, 0, 32, true).value as u32;
            let value = self.spinner.cast_unsigned();
            if shown != value {
                ui.set_text(v, &value.to_string());
            }
        }
        let ghost = |ui: &mut Ui, h: Option<Handle>, on: bool| {
            if let Some(h) = h {
                ui.set_attr(h, attr::GHOSTED, Value::Bool(on));
            }
        };
        if self.spinner == self.max {
            ghost(ui, self.up, true);
        }
        if self.spinner == self.min {
            ghost(ui, self.down, true);
        }
        if self.min < self.spinner && !self.read_only {
            ghost(ui, self.down, false);
        }
        if self.spinner < self.max && !self.read_only {
            ghost(ui, self.up, false);
        }
        if self.at(self.spinner).is_none() {
            self.waiting = Some((self.spinner, Waiting::Spinner));
            out.push(Request::Ingredients(IngredientRequest::Inquire {
                recipe: self.recipe,
                spinner: self.spinner,
            }));
            return;
        }
        self.recipe_at_spinner(ui, out);
    }

    /// The recipe at the spinner's value: its cost and difficulty when they move with it, the
    /// craft button, the slots.
    fn recipe_at_spinner(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        let Some(f) = self.at(self.spinner).cloned() else {
            return;
        };
        if self.dynamic_cost {
            if let Some(c) = f.spinner_cost {
                self.update_cost(ui, c);
            }
        }
        if self.dynamic_difficulty {
            if let Some((d, c)) = f.spinner_difficulty {
                self.difficulty = d;
                self.name_color = c;
                self.update_apparent_difficulty(ui);
            }
        }
        self.update_craft_button(ui);
        self.update_ingredients(ui, &f, out);
    }

    /// Fill in: for the player's items in order, the first that fits each empty slot that fills
    /// itself in is asked to go there.
    // D2-02232
    fn auto_populate(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        let Some(f) = self.at(self.spinner).cloned() else {
            return;
        };
        let mut open: Vec<u32> = self.rows.keys().copied().collect();
        for item in &f.items {
            if self.item_slots.contains_key(item) {
                continue;
            }
            for (k, ordinal) in open.clone().into_iter().enumerate() {
                if !f.auto_populate.get(&ordinal).copied().unwrap_or(false) {
                    continue;
                }
                if self.slot_items.contains_key(&ordinal) {
                    continue;
                }
                if !self.check_item(ui, *item, ordinal, out) {
                    continue;
                }
                let Some(icon) = self
                    .rows
                    .get(&ordinal)
                    .and_then(|r| ui.child_recursive(*r, name::ITEM_ICON))
                else {
                    return;
                };
                ui.set_attr(icon, name::ID, Value::Id(ordinal));
                out.push(Request::Ingredients(IngredientRequest::Add {
                    recipe: self.recipe,
                    ordinal,
                    item: *item,
                    spinner: self.spinner,
                }));
                open.remove(k);
                break;
            }
        }
    }

    /// Closed: every item out of its slot, the slots forgotten; the scripts told nothing is open.
    // D2-02233
    fn do_close(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        self.show(ui, false, out);
        for item in self.item_slots.keys().copied().collect::<Vec<_>>() {
            self.item_removal(ui, item);
        }
        self.pending = false;
        self.slot_items.clear();
        self.item_slots.clear();
        self.rows.clear();
        out.push(Request::Ingredients(IngredientRequest::Inquire {
            recipe: 0,
            spinner: 0,
        }));
    }

    /// An item dropped on the list: on the slot under the pointer, unless it is already there; an
    /// item already in another slot leaves it, an item in this slot leaves it; the slot waits and
    /// the item is asked to go in.
    // D2-02227
    fn list_drop(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        let Some(list) = self.list else { return };
        let Some(dropped) = ui.dropped() else { return };
        let Some(item) = ui
            .node(dropped)
            .and_then(|n| n.instance_id_attr(name::SHORTCUT_DATA))
        else {
            return;
        };
        let Some(row) = ui.listbox_item_under_mouse(list) else {
            return;
        };
        let Some(icon) = ui.child_recursive(row, name::ITEM_ICON) else {
            return;
        };
        let ordinal = ui.node(icon).and_then(|n| n.id_attr(name::ID)).unwrap_or(0);
        if ordinal == 0 {
            return;
        }
        if let Some(was) = self.item_slots.get(&item).copied() {
            if was == ordinal {
                return;
            }
            self.item_removal(ui, item);
        }
        if let Some(there) = self.slot_items.get(&ordinal).copied() {
            self.item_removal(ui, there);
        }
        ui.set_state(row, name::PENDING);
        ui.set_attr(icon, name::ID, Value::Id(ordinal));
        out.push(Request::Ingredients(IngredientRequest::Add {
            recipe: self.recipe,
            ordinal,
            item,
            spinner: self.spinner,
        }));
    }

    /// An item went in (or not): the spinner's value as the scripts had it; the slot no longer
    /// waits, and when it went, it holds the item (which leaves any other slot), showing its icon,
    /// dragable.
    // D2-02236
    fn add_done(
        &mut self,
        ui: &mut Ui,
        n: &Notice,
        inv: Option<&inventory::Facts>,
        out: &mut Vec<Request>,
    ) {
        let ordinal = n.word_at(1);
        let item = n.long(2);
        #[allow(clippy::cast_possible_wrap)]
        let spinner = n.word_at(4) as i32;
        let error = n.word_at(5);
        if spinner != self.spinner {
            self.spinner = spinner;
            self.spinner_changed(ui, out);
        }
        let Some(row) = self.rows.get(&ordinal).copied() else {
            return;
        };
        ui.set_state(row, state::STATELESS);
        let Some(icon) = ui.child_recursive(row, name::ITEM_ICON) else {
            return;
        };
        if error != 0 {
            return;
        }
        if self.item_slots.contains_key(&item) {
            self.item_removal(ui, item);
        }
        self.slot_items.insert(ordinal, item);
        self.item_slots.insert(item, ordinal);
        if let Some(it) = inv.and_then(|f| f.items.get(&item)) {
            if it.icon_layers.is_empty() {
                ui.set_image(icon, it.icon, 2);
            } else {
                ui.set_icon_desc(icon, &it.icon_layers);
            }
            ui.set_attr(icon, attr::DRAGABLE, Value::Bool(true));
        }
    }

    /// The making is done: no longer under way; closed when it went and the recipe has no charges
    /// left, else its charges shown again.
    // D2-02237
    fn execute_done(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<Request>) {
        let went = n.word_at(1) == 0;
        let charges = self
            .facts
            .as_ref()
            .filter(|f| f.recipe == self.recipe)
            .map_or(-1, |f| f.charges);
        self.set_pending(ui, false);
        if went && charges == 0 {
            self.do_close(ui, out);
            return;
        }
        let Some(m) = self.misc else { return };
        if charges > 0 {
            let mut s = entry(ui, "ID_Craft_Charges");
            s.add_var(name::VAR_VALUE.raw(), Var::Int(charges));
            ui.set_string_info(m, &s);
        } else {
            ui.set_visible(m, false);
        }
    }

    /// The spinner's typed value taken, read as `wcstol` reads in base 0 (a leading number,
    /// `0x` hexadecimal, a leading 0 octal): within its bounds (one outside goes to the bound it
    /// passed).
    fn take_typed(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        let Some(v) = self.value else { return };
        let text: Vec<u16> = ui.text(v).unwrap_or_default().encode_utf16().collect();
        let t = auberean_text::wide::to_int32(&text);
        self.spinner = if t < self.min {
            self.min
        } else if self.max < t {
            self.max
        } else {
            t
        };
        self.spinner_changed(ui, out);
    }
}

impl Ingredients {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[
        0x436, 0x448, 0x44A, 0x47A, 0x44B, 0x405, 0x406, 0x1_040C, 0x46E,
    ];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for Ingredients {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Shown or hidden (its root as the framework shows): the close question goes.
    // D2-02192
    fn show(&mut self, ui: &mut Ui, on: bool, out: &mut Vec<Request>) {
        ui.frame_show(self.frame, on);
        if let Some(r) = self.root {
            let shown = ui.frame_shown(self.frame);
            ui.set_visible(r, shown);
            out.push(Request::DialogDown(self.dialog));
            self.dialog = 0;
        }
    }

    /// The spinner typed into (taken two seconds after the last key, or when left) and its
    /// arrows; the close, fill-in and craft buttons; drops on the slots and items dragged out.
    // D2-02234, D2-02235
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<Request>) {
        match *mail {
            Mail::Global {
                what: global::TIME, ..
            } => {
                if self.typed.is_some_and(|due| due < self.now) {
                    self.typed = None;
                    self.take_typed(ui, out);
                }
            }
            Mail::Element {
                element,
                what,
                param,
                ..
            } => {
                if element == name::VALUE {
                    if what == text_msg::DEACTIVATED {
                        self.take_typed(ui, out);
                        return;
                    }
                    self.typed = Some(self.now + 2.0);
                    return;
                }
                if element == name::UP || element == name::DOWN {
                    self.spinner = self.spinner.clamp(self.min, self.max.max(self.min));
                    if element == name::UP && self.spinner < self.max {
                        self.spinner += 1;
                    } else if element == name::DOWN && self.min < self.spinner {
                        self.spinner -= 1;
                    } else {
                        return;
                    }
                    self.spinner_changed(ui, out);
                    return;
                }
                if element == name::CLOSE {
                    if !self.pending {
                        self.do_close(ui, out);
                    }
                } else if element == name::AUTO {
                    self.auto_populate(ui, out);
                } else if element == name::CRAFT {
                    self.set_pending(ui, true);
                    out.push(Request::Ingredients(IngredientRequest::Execute {
                        recipe: self.recipe,
                        spinner: self.spinner,
                        ingredients: self.slot_items.iter().map(|(o, i)| (*o, *i)).collect(),
                    }));
                } else if what == name::STOP_DRAG_ITEM {
                    if element == name::LIST && param != 0 {
                        self.list_drop(ui, out);
                    } else if element == name::ITEM && param == 0 {
                        let item = ui
                            .dropped()
                            .and_then(|d| ui.node(d))
                            .and_then(|n| n.id_attr(name::ID))
                            .and_then(|o| self.slot_items.get(&o).copied());
                        if let Some(item) = item {
                            self.item_removal(ui, item);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// The scripts open the panel on a recipe, say an item went in, the making is done; a close
    /// question answered; mounted or not; an item in a slot going away.
    // D2-02230, D2-02238, D2-02193, D2-02220
    fn notice(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<Request>) {
        match n.native {
            notice::OPEN_INGREDIENT_PANEL => {
                if !ui.frame_shown(self.frame) || !self.pending {
                    let recipe = n.word_at(0);
                    self.recipe = recipe;
                    self.waiting = Some((i32::MIN, Waiting::Open));
                    if let Some(f) = self.facts.clone().filter(|f| f.recipe == recipe) {
                        self.waiting = None;
                        self.init(ui, &f, out);
                    }
                }
            }
            notice::EXECUTE_CRAFT_DONE => self.execute_done(ui, n, out),
            notice::UPDATE_MOUNT_STATE => {
                self.mounted = n.word_at(0) != 0;
                self.update_craft_button(ui);
            }
            crate::dialog::notice::CLOSE_CONFIRMATION_DIALOG
                if n.word_at(0) == self.dialog && self.dialog != 0 =>
            {
                self.dialog = 0;
                if n.word_at(1) != 0 {
                    self.do_close(ui, out);
                }
            }
            notice::BEING_DELETED => {
                if let Some(item) = n.about {
                    self.item_removal(ui, item);
                }
            }
            _ => {}
        }
    }

    /// The scripts' answers taken: the panel opened, the spinner's slots, an item that went in, an
    /// item in a slot checked again after it changed; the craft skill's level.
    // D2-02198, D2-02221, D2-02222, D2-02223
    // D2-02237, D2-02230
    fn notice_seen(
        &mut self,
        ui: &mut Ui,
        view: &gameplay::View<'_>,
        n: &Notice,
        out: &mut Vec<Request>,
    ) {
        if matches!(
            n.native,
            notice::OPEN_INGREDIENT_PANEL | notice::EXECUTE_CRAFT_DONE
        ) {
            self.take_answer(ui, view, out);
        }
        self.notice(ui, n, out);
    }

    fn update(&mut self, ui: &mut Ui, view: &gameplay::View<'_>, out: &mut Vec<Request>) {
        self.take_answer(ui, view, out);
        for n in view.everyone {
            if n.native == notice::ADD_INGREDIENT_DONE {
                self.add_done(ui, n, view.inventory, out);
            }
        }
    }

    // D2-02206, D2-02196
    fn close(self: Box<Self>, ui: &mut Ui) {
        ui.drop_frame(self.frame);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
