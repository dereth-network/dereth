//! The crafting window (N): the player's craft skills down its left, the selected skill across its
//! top, and the skill's categories of recipes and the selected category's recipes side by side.
//!
//! - **A craft skill** shows its name (dimmed while its use earns less than the usual experience)
//!   and its rating (green while something raises it for now, red while something lowers it).
//!   Press one to select it: the game examines it, and its first category is selected.
//! - **The selected skill** shows its icon, its name and level (by how much it is raised or
//!   lowered for now), the experience to spend and what raising it costs, or that it can't be
//!   raised; Raise is lit while it may be raised now.
//! - **Its categories**, each once, in order of their numbers. Press one to select it: the game
//!   examines it.
//! - **The category's recipes** listed, each its icon (its skill's where it has none) and its
//!   name, dimmed until it may be made again. Press one to select it (the game examines it); press
//!   it again soon after, or Make, to open it for making ([`super::making`]). Drag one onto the
//!   hotbar to make a shortcut of it.

use auberean_client_sdk::contract::notice::examination::kind as examine;
use auberean_client_sdk::contract::{shortcut_kind, Action, CraftAction, ToolbarShortcut};
use dereth_ui_kit::art::Family;
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx, WindowState};

use crate::draw::{Argb, Rect, WHITE};
use crate::state::craft::{CraftSkill, Crafting, Recipe};
use crate::state::GameState;
use crate::ui::hotbar::{Drag, Examine};
use crate::ui::Tip;

/// The window's body, in layout units: its height, and the width of all but the list of craft
/// skills, which is as wide as its longest name needs, between the least and the most.
const BODY_H: f32 = 440.0;
const RIGHT: f32 = 480.0;
pub const SKILLS_LEAST: f32 = 200.0;
pub const SKILLS_MOST: f32 = 330.0;

/// How much of a craft skill's row its name may not take: its margin and its rating.
const NAME_SPARE: f32 = 74.0;

/// A row's height in the lists of skills and categories, and in the list of recipes.
pub const ROW: f32 = 26.0;
pub const RECIPE_ROW: f32 = 36.0;

/// How soon a second press on a recipe opens it, in seconds.
pub const DOUBLE_PRESS: f64 = 0.5;

/// A rating raised for now, lowered for now.
pub const BUFFED: Argb = 0xFF78_D85A;
pub const DEBUFFED: Argb = 0xFFE8_6050;

/// A name dimmed: a skill that earns less, a recipe that may not be made yet.
pub const DIMMED: Argb = 0xFF80_8080;

/// Where the window's parts are.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Parts {
    /// The list of craft skills.
    pub skills: Rect,
    /// The selected skill's display, and its Raise button.
    pub head: Rect,
    pub raise: Rect,
    /// The list of categories, and of recipes.
    pub categories: Rect,
    pub recipes: Rect,
    /// The button that opens the selected recipe for making.
    pub make: Rect,
}

impl Parts {
    /// The parts of a window drawn with `body` as its body, at `k` pixels a layout unit.
    #[must_use]
    pub fn of(body: Rect, k: f32) -> Self {
        let left = (body.w - RIGHT * k).max(SKILLS_LEAST * k);
        let gap = 10.0 * k;
        let right_x = body.x + left + gap;
        let right_w = body.right() - right_x;
        let head = Rect::new(right_x, body.y, right_w, 64.0 * k);
        let lists_y = head.bottom() + gap;
        let foot = body.bottom() - 36.0 * k;
        Self {
            skills: Rect::new(body.x, body.y, left, body.h),
            head,
            raise: Rect::new(
                head.right() - 100.0 * k,
                head.y + 6.0 * k,
                96.0 * k,
                28.0 * k,
            ),
            categories: Rect::new(right_x, lists_y, 150.0 * k, foot - gap - lists_y),
            recipes: Rect::new(
                right_x + 150.0 * k + gap,
                lists_y,
                right_w - 150.0 * k - gap,
                foot - gap - lists_y,
            ),
            make: Rect::new(body.right() - 120.0 * k, foot, 120.0 * k, 30.0 * k),
        }
    }

    /// The `n`th row of `list`, `height` layout units each, unscrolled.
    #[must_use]
    pub fn row(list: Rect, n: usize, height: f32, k: f32) -> Rect {
        #[allow(clippy::cast_precision_loss)]
        let y = list.y + n as f32 * height * k;
        Rect::new(list.x, y, list.w - 8.0 * k, height * k)
    }

    /// Where a skill's level is written in its row of `list`: at the row's right, clear of the
    /// list's scrollbar (`bar` layout units wide, at the list's right) by [`LEVEL_GAP`].
    #[must_use]
    pub fn level(list: Rect, row: Rect, bar: f32, k: f32) -> Rect {
        let right = (row.right() - 6.0 * k).min(list.right() - (bar + LEVEL_GAP) * k);
        Rect::new(right - LEVEL_W * k, row.y, LEVEL_W * k, row.h)
    }
}

/// A skill's level: how wide its place is, and how far it keeps from the list's scrollbar.
pub const LEVEL_W: f32 = 40.0;
pub const LEVEL_GAP: f32 = 4.0;

/// The crafting window.
#[derive(Debug, Default)]
pub struct CraftingWindow {
    window: WindowState,
    drawn: Option<(Rect, Rect)>,
    /// The craft skill, the category and the recipe selected (0 for none).
    pub skill: u32,
    pub category: u32,
    pub recipe: u32,
    /// How far each list is scrolled: the skills, the categories, the recipes.
    scroll: [f32; 3],
    /// The last recipe pressed, and when.
    last_press: Option<(u32, f64)>,
    /// The craft skills' names as their rows last read.
    shown: Vec<String>,
}

/// The categories of a skill's recipes the game names, each once, in order of their numbers.
#[must_use]
pub fn categories(c: &Crafting, skill: &CraftSkill) -> Vec<(u32, String)> {
    let mut out: Vec<(u32, String)> = Vec::new();
    for r in &skill.recipes {
        let Some(name) = c.categories.get(&r.category) else {
            continue;
        };
        if out.iter().any(|(n, _)| *n == r.category) {
            continue;
        }
        let at = out
            .iter()
            .position(|(n, _)| r.category < *n)
            .unwrap_or(out.len());
        out.insert(at, (r.category, name.clone()));
    }
    out
}

/// The recipes of a skill's category that are listed.
#[must_use]
pub fn recipes(skill: &CraftSkill, category: u32) -> Vec<&Recipe> {
    skill
        .recipes
        .iter()
        .filter(|r| r.shown && r.category == category)
        .collect()
}

/// A recipe's icon: its own, else its craft skill's.
#[must_use]
pub const fn recipe_icon(skill: &CraftSkill, r: &Recipe) -> u32 {
    if r.icon != 0 {
        r.icon
    } else {
        skill.icon
    }
}

/// The face a craft skill's name is written in, in `colour`.
fn name_style(colour: Argb, ctx: &Ctx<'_>) -> TextStyle {
    TextStyle::new(Family::Body, 13.0, colour).edge(ctx.colours.edge())
}

/// An examination of something of crafting, by its kind and its datum.
const fn examination(kind: u32, data_enum: u32, data_id: u32) -> Action {
    Examine {
        kind,
        data_enum,
        item: 0,
        data_id,
    }
    .action()
}

impl CraftingWindow {
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.window.open
    }

    pub fn toggle(&mut self) {
        if self.window.open {
            self.close();
        } else {
            self.window.open = true;
        }
    }

    pub fn close(&mut self) {
        self.window.open = false;
        self.drawn = None;
        self.shown.clear();
    }

    /// The craft skills' names as the rows shown last read, in order.
    #[must_use]
    pub fn shown_names(&self) -> &[String] {
        &self.shown
    }

    /// Where it was last drawn, while it is open.
    #[must_use]
    pub fn rect(&self) -> Option<Rect> {
        self.drawn.map(|(r, _)| r)
    }

    /// Its parts as last drawn, while it is open.
    #[must_use]
    pub fn parts(&self, k: f32) -> Option<Parts> {
        self.drawn.map(|(_, b)| Parts::of(b, k))
    }

    /// A craft skill selected: its first category with it, the recipe forgotten; the game
    /// examines it.
    fn select_skill(&mut self, c: &Crafting, id: u32, acts: &mut Vec<Action>) {
        self.skill = id;
        self.recipe = 0;
        self.category = c
            .skill(id)
            .and_then(|s| categories(c, s).first().map(|(n, _)| *n))
            .unwrap_or(0);
        self.scroll[1] = 0.0;
        self.scroll[2] = 0.0;
        acts.push(examination(examine::CRAFT_SKILL, 0, id));
    }

    /// Draw it while it is open; where it is.
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        drag: &mut Option<Drag>,
        tip: &mut Option<Tip>,
        acts: &mut Vec<Action>,
    ) -> Option<Rect> {
        if !self.window.open {
            return None;
        }
        let k = p.scale;
        let name = name_style(ctx.colours.text(), ctx);
        let widest = state
            .craft
            .skills
            .iter()
            .map(|s| p.measure(&name, &s.name) / k)
            .fold(0.0_f32, f32::max);
        let left = (widest + NAME_SPARE).clamp(SKILLS_LEAST, SKILLS_MOST);
        let size = kit::window_size_for(p, (RIGHT + left, BODY_H));
        let at = (
            ((p.screen.0 - size.0 * k) / 2.0).round(),
            ((p.screen.1 - size.1 * k) / 2.0 - 40.0 * k)
                .round()
                .max(0.0),
        );
        let at = crate::ui::place::clear(p, state, size, at);
        let w = kit::window(p, ctx, &mut self.window, "Crafting", size, at);
        if w.closed {
            self.close();
            return Some(w.rect);
        }
        self.drawn = Some((w.rect, w.body));
        let parts = Parts::of(w.body, k);
        let c = &state.craft;
        let body = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        if c.skills.is_empty() {
            p.text_in(&body, w.body, Align::Centre, "No craft skills yet.");
            return Some(w.rect);
        }
        if c.skill(self.skill).is_none() {
            self.skill = 0;
            self.category = 0;
            self.recipe = 0;
        }
        self.skills_list(p, ctx, c, &parts, tip, acts);
        let Some(skill) = c.skill(self.skill) else {
            p.text_in(&body, parts.head, Align::Centre, "Choose a craft skill.");
            return Some(w.rect);
        };
        self.head(p, ctx, c, skill, &parts, acts);
        self.categories_list(p, ctx, c, skill, &parts, acts);
        self.recipes_list(p, ctx, state.game_time, skill, &parts, drag, tip, acts);
        let chosen = recipes(skill, self.category)
            .into_iter()
            .any(|r| r.id == self.recipe);
        if kit::button(p, ctx, parts.make, "Make", chosen) {
            acts.push(Action::Craft(CraftAction::Open(self.recipe)));
        }
        Some(w.rect)
    }

    /// The craft skills down the left.
    fn skills_list(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        c: &Crafting,
        parts: &Parts,
        tip: &mut Option<Tip>,
        acts: &mut Vec<Action>,
    ) {
        let k = p.scale;
        let area = parts.skills;
        #[allow(clippy::cast_precision_loss)]
        let content = c.skills.len() as f32 * ROW * k;
        let offset = kit::scroll(p, ctx, area, content, &mut self.scroll[0]);
        let bar = kit::scrollbar_width(p).unwrap_or(4.0);
        let first = first_row(offset, ROW * k);
        let rows = rows_in(area.h, ROW * k);
        self.shown.clear();
        for (n, s) in c.skills.iter().enumerate().skip(first).take(rows) {
            let row = Parts::row(area, n - first, ROW, k);
            kit::list_highlight(p, row, s.id == self.skill);
            let colour = if s.earns { ctx.colours.text() } else { DIMMED };
            let name = name_style(colour, ctx);
            let rating = TextStyle::new(Family::Numerals, 12.0, rating_colour(s, colour))
                .edge(ctx.colours.edge());
            let text = Rect::new(
                row.x + 6.0 * k,
                row.y,
                row.w - (NAME_SPARE - 14.0) * k,
                row.h,
            );
            let fitted = p.fit(&name, &s.name, text.w);
            p.text_in(&name, text, Align::Left, &fitted);
            // A name still too long for the widest list is named whole under the pointer.
            if fitted != s.name && ctx.over_quiet(&row) {
                *tip = Some(Tip {
                    title: s.name.clone(),
                    lines: Vec::new(),
                });
            }
            self.shown.push(fitted);
            p.text_in(
                &rating,
                Parts::level(area, row, bar, k),
                Align::Right,
                &s.level.to_string(),
            );
            if ctx.over(&row) && ctx.input.clicked(&row) {
                self.select_skill(c, s.id, acts);
            }
        }
    }

    /// The selected skill's display and the Raise button.
    fn head(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        c: &Crafting,
        s: &CraftSkill,
        parts: &Parts,
        acts: &mut Vec<Action>,
    ) {
        let k = p.scale;
        let r = parts.head;
        let icon = Rect::new(r.x, r.y + 4.0 * k, 44.0 * k, 44.0 * k);
        kit::icon_slot(p, ctx, icon, p.art.icon(s.icon).as_ref(), WHITE);
        let title = TextStyle::new(Family::Body, 15.0, ctx.colours.text()).edge(ctx.colours.edge());
        let small = TextStyle::new(Family::Body, 12.0, 0xFFC8_C0B0).edge(ctx.colours.edge());
        let x = icon.right() + 10.0 * k;
        p.text(
            &title,
            x,
            r.y + 4.0 * k,
            &format!("{}  {}", s.name, level_text(s)),
        );
        let line = if s.raiseable {
            format!(
                "Craft XP {}    Cost to Raise {}",
                crate::ui::skills::grouped(c.experience),
                crate::ui::skills::grouped(s.cost_to_raise)
            )
        } else {
            "This can't be raised.".to_owned()
        };
        p.text(&small, x, r.y + 28.0 * k, &line);
        if kit::button(p, ctx, parts.raise, "Raise", s.can_raise) {
            acts.push(Action::Craft(CraftAction::RaiseCraftSkill(s.id)));
        }
    }

    /// The selected skill's categories.
    fn categories_list(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        c: &Crafting,
        s: &CraftSkill,
        parts: &Parts,
        acts: &mut Vec<Action>,
    ) {
        let k = p.scale;
        let area = parts.categories;
        let all = categories(c, s);
        if !all.iter().any(|(n, _)| *n == self.category) {
            self.category = all.first().map_or(0, |(n, _)| *n);
        }
        #[allow(clippy::cast_precision_loss)]
        let content = all.len() as f32 * ROW * k;
        let offset = kit::scroll(p, ctx, area, content, &mut self.scroll[1]);
        let first = first_row(offset, ROW * k);
        let rows = rows_in(area.h, ROW * k);
        let name = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        for (n, (cat, label)) in all.iter().enumerate().skip(first).take(rows) {
            let row = Parts::row(area, n - first, ROW, k);
            kit::list_highlight(p, row, *cat == self.category);
            let text = Rect::new(row.x + 6.0 * k, row.y, row.w - 8.0 * k, row.h);
            let fitted = p.fit(&name, label, text.w);
            p.text_in(&name, text, Align::Left, &fitted);
            if ctx.over(&row) && ctx.input.clicked(&row) {
                self.category = *cat;
                self.recipe = 0;
                self.scroll[2] = 0.0;
                acts.push(examination(examine::RECIPE_CATEGORY, *cat, 0));
            }
        }
    }

    /// The selected category's recipes: pressed, selected; pressed again soon after, opened;
    /// pulled, carried to the hotbar.
    #[allow(clippy::too_many_arguments)]
    fn recipes_list(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        now: f64,
        s: &CraftSkill,
        parts: &Parts,
        drag: &mut Option<Drag>,
        tip: &mut Option<Tip>,
        acts: &mut Vec<Action>,
    ) {
        let k = p.scale;
        let area = parts.recipes;
        let all = recipes(s, self.category);
        #[allow(clippy::cast_precision_loss)]
        let content = all.len() as f32 * RECIPE_ROW * k;
        let offset = kit::scroll(p, ctx, area, content, &mut self.scroll[2]);
        let first = first_row(offset, RECIPE_ROW * k);
        let rows = rows_in(area.h, RECIPE_ROW * k);
        for (n, r) in all.iter().enumerate().skip(first).take(rows) {
            let row = Parts::row(area, n - first, RECIPE_ROW, k);
            kit::list_highlight(p, row, r.id == self.recipe);
            let waits = r.use_again > now;
            let icon = recipe_icon(s, r);
            let tile = Rect::new(row.x + 2.0 * k, row.y + 2.0 * k, 32.0 * k, 32.0 * k);
            if let Some(i) = p.art.icon(icon) {
                p.sprite(&i, tile, if waits { DIMMED } else { WHITE });
            }
            let colour = if waits { DIMMED } else { ctx.colours.text() };
            let name = TextStyle::new(Family::Body, 13.0, colour).edge(ctx.colours.edge());
            let text = Rect::new(
                tile.right() + 8.0 * k,
                row.y,
                row.right() - tile.right() - 8.0 * k,
                row.h,
            );
            let fitted = p.fit(&name, &r.name, text.w);
            p.text_in(&name, text, Align::Left, &fitted);
            if ctx.over_quiet(&row) && drag.is_none() {
                *tip = Some(Tip {
                    title: r.name.clone(),
                    lines: if waits {
                        vec!["Not yet ready to make again.".to_owned()]
                    } else {
                        Vec::new()
                    },
                });
            }
            if ctx.over(&row) && ctx.input.clicked(&row) {
                let again = self
                    .last_press
                    .is_some_and(|(id, at)| id == r.id && ctx.time - at <= DOUBLE_PRESS);
                self.recipe = r.id;
                acts.push(examination(examine::RECIPE, 0, r.id));
                if again {
                    self.last_press = None;
                    acts.push(Action::Craft(CraftAction::Open(r.id)));
                } else {
                    self.last_press = Some((r.id, ctx.time));
                }
                *drag = Some(Drag {
                    shortcut: ToolbarShortcut {
                        kind: shortcut_kind::RECIPE,
                        data_id: r.id,
                        ..ToolbarShortcut::default()
                    },
                    from: None,
                    icon,
                    origin: ctx.input.mouse,
                    active: false,
                });
            }
        }
    }
}

/// A skill's level, and by how much something raises or lowers it for now.
#[must_use]
pub fn level_text(s: &CraftSkill) -> String {
    let by = i64::from(s.level) - i64::from(s.own_level);
    match by {
        0 => format!("Level {}", s.level),
        d if d > 0 => format!("Level {} (+{d})", s.level),
        d => format!("Level {} ({d})", s.level),
    }
}

/// A skill's rating's color: raised or lowered for now, else its name's.
const fn rating_colour(s: &CraftSkill, plain: Argb) -> Argb {
    if s.level > s.own_level {
        BUFFED
    } else if s.level < s.own_level {
        DEBUFFED
    } else {
        plain
    }
}

/// The first row a list scrolled by `offset` shows, its rows `row` pixels tall.
fn first_row(offset: f32, row: f32) -> usize {
    dereth_primitives::num::to_i32((offset / row).floor())
        .max(0)
        .unsigned_abs() as usize
}

/// How many rows `row` pixels tall a list `height` pixels tall shows.
fn rows_in(height: f32, row: f32) -> usize {
    dereth_primitives::num::to_i32((height / row).floor())
        .max(0)
        .unsigned_abs() as usize
}
