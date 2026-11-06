//! The skill window's crafting page: the player's craft skills, a craft skill's recipe
//! categories, a category's recipes.
//!
//! The page has three views. **The craft skills**: a row per craft skill (its name, cut with an
//! ellipsis when it does not fit, the whole name its tooltip; enabled while its use earns at least
//! the usual experience, else disabled; its rating, marked buffed or debuffed when its level as it
//! stands differs from its own). **A craft skill's categories**: the craft skill at the head, a
//! row per category of its recipes (in order of the categories' numbers, each named once), and
//! the craft skill's display (its icon, name and level, buffed or debuffed with by how much, and
//! the experience to spend and what raising costs, or that it cannot be raised; the raise button
//! ghosted unless it may be raised now). **A category's recipes**: the category at the head, a row
//! per recipe of the category that is shown (its icon, its craft skill's when it has none; its
//! name; enabled once it may be used again), and the craft skill's display.
//!
//! A row's forward button goes a view deeper, selecting what the row is (the recipe's opens the
//! recipe); the back button goes up. Pressing a row selects what it is (twice: goes deeper, or
//! opens the recipe); pressing the head selects it (twice: goes up). Selecting asks the
//! examination for the craft skill, the category or the recipe, and when the examination's request
//! names a row's craft skill, category or recipe, that row is marked selected (the row marked
//! before back to its own look). A recipe dragged from its row is dragged as a shortcut to it.
//! The crafting tab starts at the craft skills, nothing selected. The scripts' notice that the
//! craft panel changed lays the view out again with what they answer.

use std::any::Any;
use std::collections::BTreeMap;

use auberean_text::{StringInfo, Value as Var};
use auberean_ui::framework::FrameId;
use auberean_ui::token::{attr, msg, state};
use auberean_ui::types::button_id;
use auberean_ui::{Handle, Mail, Token, Ui, Value};

use crate::flow;
use crate::gameplay::{self, notice, Child, Examination, Notice, Record, Request};
use crate::shortcuts::{kind as shortcut_kind, TOOLBAR_LAYOUT_ENUM};

/// The scripts' notice that the craft panel changed.
pub const UPDATE_CRAFT_PANEL: u32 = 0x0088 << 16 | 0x0001;

/// The examination's requests for a craft skill, a recipe category and a recipe.
pub mod examine {
    pub const CRAFT_SKILL: u32 = 0x4000_000B;
    pub const CATEGORY: u32 = 0x4000_000C;
    pub const RECIPE: u32 = 0x4000_0007;
}

mod name {
    use auberean_ui::Token;
    pub const PAGE: Token = Token::of("CraftPage");
    pub const DRAG_FIELD: Token = Token::of("DragField");
    pub const FORWARD: Token = Token::of("ForwardButton");
    pub const BACK: Token = Token::of("BackButton");
    pub const RAISE: Token = Token::of("RaiseCraftButton");
    pub const TAB: Token = Token::of("CraftTab");
    pub const RECIPE_BUTTON: Token = Token::of("RecipeButton");
    pub const RECIPE_ICON: Token = Token::of("RecipeIcon");
    pub const ENTRY: Token = Token::of("EntryTemplate");
    pub const CATEGORY: Token = Token::of("CategoryTemplate");
    pub const RECIPE: Token = Token::of("RecipeTemplate");
    pub const SKILL_HEADER: Token = Token::of("CraftSkillCategoryHeader");
    pub const RECIPE_HEADER: Token = Token::of("RecipeHeader");
    pub const SKILL_LIST: Token = Token::of("CraftSkillList");
    pub const CATEGORY_LIST: Token = Token::of("CraftSkillCategoryList");
    pub const RECIPE_LIST: Token = Token::of("RecipeList");
    pub const NAME: Token = Token::of("Name");
    pub const LEVEL: Token = Token::of("Level");
    pub const SELECTED_NAME: Token = Token::of("SelectedName");
    pub const SELECTED_LEVEL: Token = Token::of("SelectedLevel");
    pub const MISC_TEXT: Token = Token::of("MiscText");
    pub const SELECTED_SKILL: Token = Token::of("SelectedCraftSkill");
    pub const SELECTED_ICON: Token = Token::of("SelectedIcon");
    pub const BUTTON_FIELD: Token = Token::of("ButtonField");
    pub const SKILLS_VIEW: Token = Token::of("craftskillview");
    pub const CATEGORIES_VIEW: Token = Token::of("craftskillcategoryview");
    pub const RECIPES_VIEW: Token = Token::of("recipeview");
    pub const DID: Token = Token::of("did");
    pub const SELECTED: Token = Token::of("selected");
    pub const ENABLED: Token = Token::of("enabled");
    pub const DISABLED: Token = Token::of("disabled");
    pub const BUFF: Token = Token::of("buff");
    pub const DEBUFF: Token = Token::of("debuff");
    pub const LEVEL_VAR: Token = Token::of("level");
    pub const VALUE: Token = Token::of("value");
    pub const XP: Token = Token::of("xp");
    pub const COST: Token = Token::of("cost");
    pub const SHORTCUT_TYPE: Token = Token::of("ShortcutType");
    pub const SHORTCUT_DATA: Token = Token::of("ShortcutData");
    pub const NOT_DRAGABLE: Token = Token::of("not_dragable");
}

/// A craft skill as the scripts' adaptor gives it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CraftSkill {
    pub did: u32,
    pub name: StringInfo,
    /// Its level as it stands, and its own.
    pub level: u32,
    pub raw_level: u32,
    pub cost_to_raise: u64,
    pub icon: u32,
    /// The factor of the experience its use earns.
    pub usage_xp_mod: f32,
}

/// A recipe as the scripts' adaptor gives it (what the page reads of it).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recipe {
    pub did: u32,
    pub name: StringInfo,
    pub icon: u32,
    pub shown: bool,
    pub craft_skill: u32,
    pub category: u32,
    /// The game time from which it may be used again.
    pub time_use_again: f64,
}

/// A craft skill with what the page asks of it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SkillFacts {
    /// As the list gives it, and as asked on its own (`None` when the scripts said no).
    pub listed: CraftSkill,
    pub own: Option<CraftSkill>,
    /// Its level: its own, and as it stands.
    pub raw_level: u32,
    pub level: u32,
    /// Whether it may ever be raised; whether it may be raised now.
    pub raiseable: bool,
    pub can_raise: bool,
    /// Its recipes, in the scripts' order.
    pub recipes: Vec<Recipe>,
}

/// What the player's scripts answer for the crafting page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    /// Moves when the scripts were asked again.
    pub version: u64,
    pub skills: Vec<SkillFacts>,
    /// The experience the player may spend on craft skills.
    pub experience: u64,
    /// The recipe categories' names.
    pub categories: BTreeMap<u32, StringInfo>,
    /// The game time now.
    pub now: f64,
    /// The recipe the ingredient panel reads, at a spinner value, as last answered.
    pub opened: Option<crate::ingredients::RecipeAt>,
}

impl Facts {
    fn skill(&self, did: u32) -> Option<&SkillFacts> {
        self.skills.iter().find(|s| s.listed.did == did)
    }
}

/// What the crafting page asks of crafting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CraftRequest {
    /// Open a recipe (the scripts open the ingredient panel on it).
    Open(u32),
    /// Raise a craft skill.
    RaiseCraftSkill(u32),
}

/// The messages' string table.
fn entry(ui: &Ui, id: &str) -> StringInfo {
    let table = ui
        .assets()
        .data_id_by_enum(flow::MESSAGES, flow::TABLE_MAPPING);
    StringInfo::entry(table, Token::of(id).raw())
}

fn text_of(ui: &Ui, info: &StringInfo) -> String {
    ui.assets().write(info).unwrap_or_default()
}

/// A name written into a text element, cut with an ellipsis when it does not fit, the whole name
/// its tooltip then.
fn name_cut_to_fit(ui: &mut Ui, h: Handle, info: &StringInfo) {
    let text = text_of(ui, info);
    ui.set_text(h, &text);
    if ui.text_too_wide(h) {
        let ellipsis = text_of(ui, &entry(ui, "ID_Ellipsis"));
        ui.truncate_text(h, &ellipsis);
        ui.set_tooltip(h, &text);
    }
}

/// The crafting page's panel.
#[derive(Debug)]
pub struct Craft {
    frame: FrameId,
    page: Option<Handle>,
    drag: Option<Handle>,
    /// The view shown.
    view: Token,
    /// The craft skill, category and recipe selected.
    skill: u32,
    category: u32,
    recipe: u32,
    /// The row marked selected.
    selected: Option<Handle>,
    /// The selected craft skill's recipe categories (by number) and recipes.
    categories: Vec<(u32, StringInfo)>,
    recipes: Vec<Recipe>,
    /// The rows, by craft skill, category and recipe.
    skill_rows: BTreeMap<u32, Handle>,
    category_rows: BTreeMap<u32, Handle>,
    recipe_rows: BTreeMap<u32, Handle>,
    facts: Facts,
    /// The scripts said the craft panel changed: laid out again once their answers are in.
    refresh_due: bool,
}

/// The layout's two lower views stretch the head with the page and leave the craft skill's display
/// where it stands, so on a window taller than the layout's the head covers the list and takes its
/// clicks. Here the head keeps its height and the display stays at the page's foot, and the list
/// alone takes what the page grows by. A page at the layout's height is laid out as before.
fn give_the_list_what_the_page_grows_by(ui: &mut Ui, page: Handle) {
    for (view, head) in [
        (name::CATEGORIES_VIEW, name::SKILL_HEADER),
        (name::RECIPES_VIEW, name::RECIPE_HEADER),
    ] {
        ui.set_child_edges(page, view, head, [1, 1, 1, 2]);
        ui.set_child_edges(page, view, name::SELECTED_SKILL, [0, 2, 0, 1]);
    }
}

impl Craft {
    /// The page under the skill window's framework: its page and drag field found, its buttons,
    /// rows and tab heard.
    // D2-02660
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        let page = ui.element(name::PAGE);
        if let Some(p) = page {
            give_the_list_what_the_page_grows_by(ui, p);
        }
        let drag = page.and_then(|p| ui.child_recursive(p, name::DRAG_FIELD));
        if let Some(l) = ui.frame_listener(frame) {
            for b in [name::FORWARD, name::BACK, name::RAISE, name::RECIPE_BUTTON] {
                ui.listen_for_message(l, b, msg::BUTTON_RELEASE_ON);
            }
            ui.listen_for_message(l, name::TAB, msg::MOUSE_PRESS);
            for e in [
                name::ENTRY,
                name::CATEGORY,
                name::RECIPE,
                name::SKILL_HEADER,
                name::RECIPE_HEADER,
            ] {
                ui.listen_for_message(l, e, msg::MOUSE_PRESS_TOP);
            }
            ui.listen_for_message(l, name::RECIPE, name::NOT_DRAGABLE);
        }
        Self {
            frame,
            page,
            drag,
            view: Token(0),
            skill: 0,
            category: 0,
            recipe: 0,
            selected: None,
            categories: Vec::new(),
            recipes: Vec::new(),
            skill_rows: BTreeMap::new(),
            category_rows: BTreeMap::new(),
            recipe_rows: BTreeMap::new(),
            facts: Facts::default(),
            refresh_due: false,
        }
    }

    /// The view shown.
    #[must_use]
    pub const fn view(&self) -> Token {
        self.view
    }

    /// The craft skill, category and recipe selected.
    #[must_use]
    pub const fn selection(&self) -> (u32, u32, u32) {
        (self.skill, self.category, self.recipe)
    }

    /// The row marked selected: the one before back to its own look, this one selected.
    // D2-02634
    fn select_element(&mut self, ui: &mut Ui, e: Option<Handle>) {
        if let Some(old) = self.selected {
            if let Some(s) = ui.default_state(old) {
                ui.set_state(old, s);
            }
        }
        self.selected = e;
        if let Some(e) = e {
            ui.set_state(e, name::SELECTED);
        }
    }

    /// A category selected (none for 0): the recipe forgotten; the examination asks for it.
    // D2-02635
    fn select_category(&mut self, did: u32, out: &mut Vec<Request>) {
        self.category = did;
        self.recipe = 0;
        if did != 0 {
            out.push(Request::Examine(Examination {
                kind: examine::CATEGORY,
                data_enum: did,
                ..Examination::default()
            }));
        }
    }

    /// A recipe selected; the examination asks for it.
    // D2-02636
    fn select_recipe(&mut self, did: u32, out: &mut Vec<Request>) {
        self.recipe = did;
        if did != 0 {
            out.push(Request::Examine(Examination {
                kind: examine::RECIPE,
                data_id: did,
                ..Examination::default()
            }));
        }
    }

    /// A recipe's icon: its own, else its craft skill's (0 when the skill is not known).
    // D2-02637
    fn recipe_icon(&self, r: &Recipe) -> u32 {
        if r.icon != 0 {
            return r.icon;
        }
        self.facts
            .skill(self.skill)
            .and_then(|s| s.own.as_ref())
            .map_or(0, |a| a.icon)
    }

    /// A craft skill's level text: its level as it stands; with `value`, when buffs or debuffs move
    /// it, the buffed or debuffed form with by how much; and its look (buffed, debuffed, or none).
    // D2-02638
    fn make_level_text(&self, ui: &Ui, skill: u32, value: bool) -> (StringInfo, Token) {
        let (raw, level) = self
            .facts
            .skill(skill)
            .map_or((0, 0), |s| (s.raw_level, s.level));
        #[allow(clippy::cast_possible_wrap)]
        let diff = level.wrapping_sub(raw) as i32;
        let mut info = entry(ui, "ID_Craft_SkillLevelNormal");
        info.add_var(name::LEVEL_VAR.raw(), Var::UInt(level));
        let mut look = state::STATELESS;
        if diff != 0 {
            if value {
                let id = if diff < 0 {
                    "ID_Craft_SkillLevelDebuffed"
                } else {
                    "ID_Craft_SkillLevelBuffed"
                };
                info.string_id = Token::of(id).raw();
                #[allow(clippy::cast_possible_wrap)]
                info.add_var(name::VALUE.raw(), Var::Int(diff.unsigned_abs() as i32));
            }
            look = if diff < 0 { name::DEBUFF } else { name::BUFF };
        }
        (info, look)
    }

    /// The craft skill's display (in the categories' and recipes' views): its icon, name and level,
    /// the buttons' field selected; with the raise button, what raising costs against the
    /// experience to spend (or that it cannot be raised), the button ghosted unless it may be
    /// raised now.
    // D2-02639
    fn refresh_craft_skill_display(&self, ui: &mut Ui, a: &CraftSkill) {
        let Some(page) = self.page else { return };
        if self.view != name::CATEGORIES_VIEW && self.view != name::RECIPES_VIEW {
            return;
        }
        let find = |ui: &Ui, n| ui.child_recursive(page, n);
        let (Some(sel_name), Some(sel_level), Some(misc), Some(icon), Some(buttons)) = (
            find(ui, name::SELECTED_NAME),
            find(ui, name::SELECTED_LEVEL),
            find(ui, name::MISC_TEXT),
            find(ui, name::SELECTED_ICON),
            find(ui, name::BUTTON_FIELD),
        ) else {
            return;
        };
        if a.icon == 0 {
            ui.clear_image(icon);
        } else {
            ui.set_image(icon, a.icon, 2);
        }
        let n = text_of(ui, &a.name);
        ui.set_text(sel_name, &n);
        let (level, _) = self.make_level_text(ui, a.did, true);
        let level = text_of(ui, &level);
        ui.set_text(sel_level, &level);
        ui.set_state(buttons, name::SELECTED);
        let Some(raise) = find(ui, name::RAISE) else {
            return;
        };
        let facts = self.facts.skill(a.did);
        let mut info = if facts.is_some_and(|s| s.raiseable) {
            let mut i = entry(ui, "ID_Craft_AvailableXPRaise");
            i.add_var(name::XP.raw(), Var::ULong(self.facts.experience));
            i.add_var(name::COST.raw(), Var::ULong(a.cost_to_raise));
            i
        } else {
            entry(ui, "ID_Craft_CantRaise")
        };
        let can = facts.is_some_and(|s| s.can_raise);
        ui.set_attr(raise, attr::GHOSTED, Value::Bool(!can));
        let t = text_of(ui, &std::mem::take(&mut info));
        ui.set_text(misc, &t);
    }

    /// A craft skill's row (or the head, `header`): its number on it and on its button (back for
    /// the head, forward for a row); its name, enabled while its use earns at least the usual
    /// experience; its rating, marked by how its level stands against its own; selected when it
    /// is the craft skill selected.
    // D2-02642
    fn make_craft_skill(&mut self, ui: &mut Ui, a: &CraftSkill, row: Handle, header: bool) {
        ui.set_attr(row, name::DID, Value::Id(a.did));
        let button = if header { name::BACK } else { name::FORWARD };
        let Some(b) = ui.child_recursive(row, button) else {
            return;
        };
        ui.set_attr(b, name::DID, Value::Id(a.did));
        let Some(n) = ui.child_recursive(row, name::NAME) else {
            return;
        };
        let look = if a.usage_xp_mod >= 1.0 {
            name::ENABLED
        } else {
            name::DISABLED
        };
        ui.set_state(n, look);
        name_cut_to_fit(ui, n, &a.name);
        let Some(level) = ui.child_recursive(row, name::LEVEL) else {
            return;
        };
        #[allow(clippy::cast_possible_wrap)]
        let diff = a.level.wrapping_sub(a.raw_level) as i32;
        let look = match diff {
            d if d < 0 => name::DEBUFF,
            d if d > 0 => name::BUFF,
            _ => state::STATELESS,
        };
        ui.set_state(level, look);
        let mut rating = entry(ui, "ID_Craft_Rating");
        rating.add_var(name::VALUE.raw(), Var::UInt(a.level));
        let rating = text_of(ui, &rating);
        ui.set_text(level, &rating);
        if self.skill == a.did {
            self.select_element(ui, Some(row));
        }
    }

    /// A category's row (or the head): its number on it and on its button; its name; selected when
    /// it is the category selected.
    // D2-02643
    fn make_category(
        &mut self,
        ui: &mut Ui,
        category: u32,
        label: &StringInfo,
        row: Handle,
        header: bool,
    ) {
        ui.set_attr(row, name::DID, Value::Id(category));
        let button = if header { name::BACK } else { name::FORWARD };
        let Some(b) = ui.child_recursive(row, button) else {
            return;
        };
        ui.set_attr(b, name::DID, Value::Id(category));
        let Some(n) = ui.child_recursive(row, name::NAME) else {
            return;
        };
        name_cut_to_fit(ui, n, label);
        if self.category == category {
            self.select_element(ui, Some(row));
        }
    }

    /// A recipe's row: its number on it and on its button; its icon; its name, enabled once it may
    /// be used again; selected when it is the recipe selected.
    // D2-02644
    fn make_recipe(&mut self, ui: &mut Ui, r: &Recipe, row: Handle) {
        ui.set_attr(row, name::DID, Value::Id(r.did));
        let Some(icon) = ui.child_recursive(row, name::RECIPE_ICON) else {
            return;
        };
        let Some(b) = ui.child_recursive(row, name::RECIPE_BUTTON) else {
            return;
        };
        ui.set_attr(b, name::DID, Value::Id(r.did));
        match self.recipe_icon(r) {
            0 => ui.clear_image(icon),
            i => ui.set_image(icon, i, 0),
        }
        let Some(n) = ui.child_recursive(row, name::NAME) else {
            return;
        };
        let look = if r.time_use_again > self.facts.now {
            name::DISABLED
        } else {
            name::ENABLED
        };
        ui.set_state(n, look);
        name_cut_to_fit(ui, n, &r.name);
        if self.recipe == r.did {
            self.select_element(ui, Some(row));
        }
    }

    /// The rows forgotten and nothing marked.
    // D2-02646
    fn clear_selection_info(&mut self) {
        self.skill_rows.clear();
        self.category_rows.clear();
        self.recipe_rows.clear();
        self.selected = None;
    }

    /// The selected craft skill's categories again: each category of its recipes the scripts
    /// name, once, in order of their numbers.
    // D2-02648, D2-12360, D2-12388, D2-12357, D2-12361, D2-12356, D2-12358, D2-12359
    fn update_categories(&mut self) {
        self.categories.clear();
        for r in &self.recipes {
            let Some(label) = self.facts.categories.get(&r.category) else {
                continue;
            };
            if self.categories.iter().any(|(c, _)| *c == r.category) {
                continue;
            }
            let at = self
                .categories
                .iter()
                .position(|(c, _)| r.category < *c)
                .unwrap_or(self.categories.len());
            self.categories.insert(at, (r.category, label.clone()));
        }
    }

    /// A craft skill selected (none for 0): the category and recipe forgotten, its recipes and
    /// their categories taken; the examination asks for it.
    // D2-02654
    fn select_craft_skill(&mut self, did: u32, out: &mut Vec<Request>) {
        self.skill = did;
        self.category = 0;
        self.recipe = 0;
        self.recipes = self
            .facts
            .skill(did)
            .filter(|_| did != 0)
            .map(|s| s.recipes.clone())
            .unwrap_or_default();
        self.update_categories();
        if did != 0 {
            out.push(Request::Examine(Examination {
                kind: examine::CRAFT_SKILL,
                data_id: did,
                ..Examination::default()
            }));
        }
    }

    /// The craft skills' view: a row per craft skill.
    // D2-02655
    fn refresh_craft_skill_page(&mut self, ui: &mut Ui) {
        let Some(page) = self.page else { return };
        if self.view != name::SKILLS_VIEW {
            return;
        }
        self.skill_rows.clear();
        self.category_rows.clear();
        self.recipe_rows.clear();
        self.selected = None;
        let Some(list) = ui.child_recursive(page, name::SKILL_LIST) else {
            return;
        };
        ui.list_flush(list);
        let skills: Vec<CraftSkill> = self.facts.skills.iter().map(|s| s.listed.clone()).collect();
        for a in &skills {
            let Some(row) = ui.frame_create_element(TOOLBAR_LAYOUT_ENUM, name::ENTRY) else {
                continue;
            };
            self.make_craft_skill(ui, a, row, false);
            ui.list_multi_add(list, row);
            self.skill_rows.insert(a.did, row);
        }
        ui.list_changes_done(list);
    }

    /// A craft skill's view: it at the head with its display, a row per category.
    // D2-02656
    fn refresh_category_page(&mut self, ui: &mut Ui) {
        let Some(page) = self.page else { return };
        if self.view != name::CATEGORIES_VIEW || self.skill == 0 {
            return;
        }
        self.clear_selection_info();
        if let Some(a) = self.facts.skill(self.skill).and_then(|s| s.own.clone()) {
            let Some(head) = ui.child_recursive(page, name::SKILL_HEADER) else {
                return;
            };
            self.make_craft_skill(ui, &a, head, true);
            self.skill_rows.insert(a.did, head);
            self.refresh_craft_skill_display(ui, &a);
        }
        let Some(list) = ui.child_recursive(page, name::CATEGORY_LIST) else {
            return;
        };
        ui.list_flush(list);
        for (c, label) in self.categories.clone() {
            let Some(row) = ui.frame_create_element(TOOLBAR_LAYOUT_ENUM, name::CATEGORY) else {
                continue;
            };
            self.make_category(ui, c, &label, row, false);
            ui.list_multi_add(list, row);
            self.category_rows.insert(c, row);
        }
        ui.list_changes_done(list);
    }

    /// A category's view: it at the head, a row per recipe of it that is shown, the craft skill's
    /// display.
    // D2-02657
    fn refresh_recipe_page(&mut self, ui: &mut Ui) {
        let Some(page) = self.page else { return };
        if self.view != name::RECIPES_VIEW || self.skill == 0 || self.category == 0 {
            return;
        }
        self.clear_selection_info();
        let Some(list) = ui.child_recursive(page, name::RECIPE_LIST) else {
            return;
        };
        ui.list_flush(list);
        let label = self
            .categories
            .iter()
            .filter(|(c, _)| *c == self.category)
            .map(|(_, l)| l.clone())
            .next_back();
        if let Some(label) = label {
            let Some(head) = ui.child_recursive(page, name::RECIPE_HEADER) else {
                return;
            };
            self.make_category(ui, self.category, &label, head, true);
            self.category_rows.insert(self.category, head);
        }
        for r in self.recipes.clone() {
            if !r.shown || r.category != self.category {
                continue;
            }
            let Some(row) = ui.frame_create_element(TOOLBAR_LAYOUT_ENUM, name::RECIPE) else {
                continue;
            };
            self.make_recipe(ui, &r, row);
            ui.list_multi_add(list, row);
            self.recipe_rows.insert(r.did, row);
        }
        ui.list_changes_done(list);
        if let Some(a) = self.facts.skill(self.skill).and_then(|s| s.own.clone()) {
            self.refresh_craft_skill_display(ui, &a);
        }
    }

    /// The view shown laid out again (the selected craft skill's recipes taken again first).
    // D2-02659
    fn refresh_craft_panel(&mut self, ui: &mut Ui) {
        if self.skill != 0 {
            self.recipes = self
                .facts
                .skill(self.skill)
                .map(|s| s.recipes.clone())
                .unwrap_or_default();
        }
        if self.view == name::SKILLS_VIEW {
            self.refresh_craft_skill_page(ui);
        } else if self.view == name::CATEGORIES_VIEW {
            self.refresh_category_page(ui);
        } else if self.view == name::RECIPES_VIEW {
            self.refresh_recipe_page(ui);
        }
    }

    /// A view shown and laid out.
    // D2-02661
    fn set_view(&mut self, ui: &mut Ui, view: Token) {
        let Some(page) = self.page else { return };
        ui.set_state(page, view);
        self.view = view;
        self.refresh_craft_panel(ui);
    }

    /// The examination's request changed: the row of the craft skill, category or recipe it names
    /// marked selected (none for anything else).
    // D2-02653
    fn examination_request_changed(
        &mut self,
        ui: &mut Ui,
        kind: u32,
        data_enum: u32,
        data_id: u32,
    ) {
        let row = match kind {
            examine::CRAFT_SKILL => self.skill_rows.get(&data_id),
            examine::CATEGORY => self.category_rows.get(&data_enum),
            examine::RECIPE => self.recipe_rows.get(&data_id),
            _ => None,
        }
        .copied();
        self.select_element(ui, row);
    }

    /// A recipe's row dragged: the drag field made a shortcut to the recipe with its icon, and
    /// dragged.
    fn drag_recipe(&self, ui: &mut Ui, did: u32) {
        let r = self
            .recipes
            .iter()
            .find(|r| r.did == did)
            .cloned()
            .unwrap_or(Recipe {
                did,
                ..Recipe::default()
            });
        let icon = self.recipe_icon(&r);
        let Some(d) = self.drag else { return };
        ui.set_attr(d, name::SHORTCUT_DATA, Value::Id(did));
        ui.set_attr(d, name::SHORTCUT_TYPE, Value::Id(shortcut_kind::RECIPE));
        ui.add_image_to_state(d, icon, 2, state::STATELESS);
        ui.start_drag(d);
    }
}

/// A press the rows answer: the left or right button, once or twice.
fn row_press(param: u32) -> bool {
    matches!(
        param,
        button_id::LEFT | button_id::DOUBLE_LEFT | button_id::RIGHT | button_id::DOUBLE_RIGHT
    )
}

impl Craft {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[0x45D, 0x447, 0x1_040C];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for Craft {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// The buttons, the rows, the heads, a recipe dragged, the tab.
    // D2-02663
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<Request>) {
        let Mail::Element {
            from,
            element,
            what,
            param,
        } = *mail
        else {
            return;
        };
        let did = ui
            .node(from)
            .and_then(|n| n.id_attr(name::DID))
            .unwrap_or(0);
        let released = what == msg::BUTTON_RELEASE_ON;
        let pressed = what == msg::MOUSE_PRESS_TOP && row_press(param);
        let twice = param == button_id::DOUBLE_LEFT;
        if element == name::FORWARD && released {
            if self.view == name::SKILLS_VIEW {
                self.select_craft_skill(did, out);
                self.set_view(ui, name::CATEGORIES_VIEW);
            } else if self.view == name::CATEGORIES_VIEW {
                self.select_category(did, out);
                self.set_view(ui, name::RECIPES_VIEW);
            } else if self.view == name::RECIPES_VIEW {
                self.select_recipe(did, out);
                out.push(Request::Craft(CraftRequest::Open(did)));
            }
            return;
        }
        if element == name::BACK && released {
            if self.view == name::CATEGORIES_VIEW {
                self.set_view(ui, name::SKILLS_VIEW);
            } else if self.view == name::RECIPES_VIEW {
                self.set_view(ui, name::CATEGORIES_VIEW);
            }
            return;
        }
        if element == name::RECIPE_HEADER && pressed {
            self.select_category(did, out);
            if twice {
                self.set_view(ui, name::CATEGORIES_VIEW);
            }
            return;
        }
        if element == name::SKILL_HEADER && pressed {
            self.select_craft_skill(did, out);
            if twice {
                self.set_view(ui, name::SKILLS_VIEW);
            }
            return;
        }
        if element == name::ENTRY && pressed && self.view == name::SKILLS_VIEW {
            self.select_craft_skill(did, out);
            if twice {
                self.set_view(ui, name::CATEGORIES_VIEW);
                return;
            }
        }
        if element == name::CATEGORY && pressed && self.view == name::CATEGORIES_VIEW {
            self.select_category(did, out);
            if twice {
                self.set_view(ui, name::RECIPES_VIEW);
                return;
            }
        }
        if element == name::RECIPE && pressed && self.view == name::RECIPES_VIEW {
            self.select_recipe(did, out);
            if twice {
                out.push(Request::Craft(CraftRequest::Open(did)));
                return;
            }
        }
        if element == name::RECIPE_BUTTON && released {
            self.select_recipe(did, out);
            out.push(Request::Craft(CraftRequest::Open(did)));
        }
        if element == name::RECIPE && what == name::NOT_DRAGABLE {
            self.drag_recipe(ui, did);
        }
        if element == name::RAISE && released {
            out.push(Request::Craft(CraftRequest::RaiseCraftSkill(self.skill)));
            return;
        }
        if element == name::TAB {
            self.clear_selection_info();
            self.select_craft_skill(0, out);
            self.set_view(ui, name::SKILLS_VIEW);
        }
    }

    /// The scripts said the craft panel changed (laid out again once their answers are in), or
    /// the examination's request changed.
    // D2-02662
    fn notice(&mut self, ui: &mut Ui, n: &Notice, _out: &mut Vec<Request>) {
        match n.native {
            UPDATE_CRAFT_PANEL => self.refresh_due = true,
            notice::EXAMINATION_REQUEST_CHANGED => {
                let r = n.records.iter().flatten().find_map(|r| match r {
                    Record::Request(r) => Some(*r),
                    _ => None,
                });
                if let Some(r) = r {
                    self.examination_request_changed(ui, r.kind, r.data_enum, r.data_id);
                }
            }
            _ => {}
        }
    }

    /// The scripts' answers taken, and the view laid out again when they said so.
    fn update(&mut self, ui: &mut Ui, view: &gameplay::View<'_>, _out: &mut Vec<Request>) {
        if let Some(f) = view.craft {
            if f.version == self.facts.version {
                self.facts.now = f.now;
            } else {
                self.facts = f.clone();
            }
        }
        if self.refresh_due {
            self.refresh_due = false;
            self.refresh_craft_panel(ui);
        }
    }

    /// It goes: its rows forgotten and its lists of categories and recipes emptied.
    // D2-02652, D2-02658, D2-02649, D2-02650, D2-02651
    fn close(mut self: Box<Self>, ui: &mut Ui) {
        self.clear_selection_info();
        self.categories.clear();
        self.recipes.clear();
        ui.drop_frame(self.frame);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
