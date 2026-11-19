//! The making window: a recipe the game opened for making, its ingredients' slots, the spinner
//! and the Craft button, as the game's own ingredient panel works them.
//!
//! - **It opens** when the game opens a recipe for making (from the crafting window's Make, a
//!   recipe on the hotbar, or the game itself), unless a making is under way, once the game has
//!   read the recipe: its name colored by how hard it is for the craft skill, its charges when it
//!   has some, what making it costs, a slot for each ingredient it takes, and the spinner set to
//!   the recipe's last value (hidden when the recipe has none).
//! - **An item put down on a slot** (from the pack, an open container or what the player wears)
//!   is asked to go in, the slot waiting until the game says whether it went; then the slot holds
//!   it, out of any other slot. An item carried out of its slot to anywhere but another slot
//!   leaves it.
//! - **The spinner** steps between its bounds (or takes a number typed and entered): what making
//!   costs and how hard it is follow when they move with it, and the slots follow the recipe at
//!   its value; an item that no longer fits, or the player no longer has, leaves its slot.
//! - **Fill in**, for a recipe whose ingredients fill themselves in: for the player's items in
//!   the game's order, the first that fits each empty slot that fills itself in is asked to go
//!   there.
//! - **Craft** asks to make the recipe from the items in the slots. It is not lit while a making
//!   is under way, while the player is mounted, while the spinner is out of its bounds or the
//!   recipe is past what the skill may make (its tip says which). While it is under way the
//!   slots wait, and the window will not close. When it is done the window says how it went, and
//!   closes when it went and the recipe has no charges left.
//!
//! Closing it tells the game nothing is open; leaving the world closes it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use auberean_client_sdk::contract::view::RecipeAt;
use auberean_client_sdk::contract::{Action, CraftAction};
use dereth_ui_kit::art::Family;
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx, WindowState};

use crate::draw::{Argb, Rect};
use crate::state::craft::{cost_at, difficulty_colour, past_cutoff, Opened};
use crate::state::GameState;
use crate::ui::items::{self, Carried, Carry, Icon, TileLook, Zone, TILE};
use crate::ui::Tip;

/// The window's body's width, in layout units; its height follows its slots.
const WIDTH: f32 = 420.0;

/// A slot's row, in layout units.
pub const SLOT_ROW: f32 = 50.0;

/// The name's colors by how hard the recipe is: none, far below the skill to far above it, past
/// what the skill may make.
pub const DIFFICULTY: [Argb; 7] = [
    0xFFE8_E0D0,
    0xFFA0_A0A0,
    0xFF78_D85A,
    0xFF60_A8F0,
    0xFFF0_C050,
    0xFFF0_8040,
    0xFFC0_3030,
];

/// The spinner's number box, for the kit's focus.
const SPINNER_BOX: u32 = 0x4142_0601;

/// Why Craft is not lit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unlit {
    /// A making is under way.
    Making,
    Mounted,
    /// The spinner is out of its bounds.
    OutOfBounds,
    /// The recipe is past what the skill may make.
    TooHard,
}

impl Unlit {
    /// What Craft's tip says of it (none while a making is under way).
    #[must_use]
    pub const fn tip(self) -> Option<&'static str> {
        match self {
            Self::Making => None,
            Self::Mounted => Some("You cannot craft while mounted."),
            Self::OutOfBounds => Some("That number is out of range."),
            Self::TooHard => Some("That is too hard for you to make."),
        }
    }
}

/// What waits for the game to read the recipe: the window to open, or the spinner's value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wait {
    Open,
    Spinner(i32),
}

/// Where the window's parts are.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub body: Rect,
    /// The spinner's buttons and its box.
    pub down: Rect,
    pub up: Rect,
    pub value: Rect,
    pub fill_in: Rect,
    pub craft: Rect,
}

impl Layout {
    #[must_use]
    pub fn of(body: Rect, k: f32) -> Self {
        let foot = body.bottom() - 40.0 * k;
        let spin = body.y + 50.0 * k;
        Self {
            body,
            down: Rect::new(body.x + 12.0 * k, spin, 28.0 * k, 26.0 * k),
            value: Rect::new(body.x + 44.0 * k, spin, 70.0 * k, 26.0 * k),
            up: Rect::new(body.x + 118.0 * k, spin, 28.0 * k, 26.0 * k),
            fill_in: Rect::new(body.x + 12.0 * k, foot, 120.0 * k, 30.0 * k),
            craft: Rect::new(body.right() - 132.0 * k, foot, 120.0 * k, 30.0 * k),
        }
    }

    /// The tile of the `n`th slot.
    #[must_use]
    pub fn slot(&self, n: usize, k: f32) -> Rect {
        #[allow(clippy::cast_precision_loss)]
        let y = self.body.y + (86.0 + n as f32 * SLOT_ROW) * k;
        Rect::new(self.body.x + 12.0 * k, y, TILE * k, TILE * k)
    }
}

/// The making window.
#[derive(Debug, Default)]
pub struct MakingWindow {
    window: WindowState,
    drawn: Option<(Rect, Rect)>,
    /// The recipe open (0: none) and the spinner's value.
    pub recipe: u32,
    pub spinner: i32,
    wait: Option<Wait>,
    /// The items in the slots, by slot; the slots waiting for the game.
    pub slots: BTreeMap<u32, u64>,
    pub waiting: BTreeSet<u32>,
    /// A making is under way.
    pub making: bool,
    /// How the last making went: whether it went.
    pub went: Option<bool>,
    /// The recipe as the game last read it at the spinner's value, and the last reading seen.
    shown: Option<Opened>,
    seen: Option<RecipeAt>,
    /// The number typed into the spinner's box, and the kit's focus.
    typed: String,
    focus: u32,
}

impl MakingWindow {
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.recipe != 0 && self.window.open
    }

    /// Where it was last drawn, while it is open.
    #[must_use]
    pub fn rect(&self) -> Option<Rect> {
        self.drawn.map(|(r, _)| r)
    }

    /// Its parts as last drawn, while it is open.
    #[must_use]
    pub fn layout(&self, k: f32) -> Option<Layout> {
        self.drawn.map(|(_, b)| Layout::of(b, k))
    }

    /// The recipe as last read at the spinner's value, while it is open.
    #[must_use]
    pub fn shown(&self) -> Option<&Opened> {
        self.shown.as_ref().filter(|_| self.is_open())
    }

    /// Whether its spinner's box has the keys.
    #[must_use]
    pub const fn typing(&self) -> bool {
        self.is_open() && self.focus == SPINNER_BOX
    }

    /// Why Craft is not lit, the player mounted or not; none when it is.
    #[must_use]
    pub fn unlit(&self, mounted: bool) -> Option<Unlit> {
        let at = &self.shown.as_ref()?.at;
        if self.making {
            Some(Unlit::Making)
        } else if mounted {
            Some(Unlit::Mounted)
        } else if self.spinner < at.min_spinner || at.max_spinner < self.spinner {
            Some(Unlit::OutOfBounds)
        } else if past_cutoff(at) {
            Some(Unlit::TooHard)
        } else {
            None
        }
    }

    /// Closed, unless a making is under way: every slot emptied, the game told nothing is open.
    pub fn close(&mut self, acts: &mut Vec<Action>) {
        if self.recipe == 0 || self.making {
            return;
        }
        *self = Self {
            seen: self.seen.take(),
            ..Self::default()
        };
        acts.push(Action::Craft(CraftAction::Inquire {
            recipe: 0,
            spinner: 0,
        }));
    }

    /// Follow one frame of the game: a recipe opened, the game's reading of it, the ingredients
    /// it said went in, the makings it said were done.
    pub fn follow(&mut self, state: &GameState, acts: &mut Vec<Action>) {
        // Out of the world nothing is open for making.
        if !state.in_world {
            *self = Self::default();
            return;
        }
        let c = &state.craft;
        for &r in &c.open_asked {
            if !self.is_open() || !self.making {
                self.recipe = r;
                self.wait = Some(Wait::Open);
                if let Some(o) = c.opened.as_ref().filter(|o| o.at.recipe == r) {
                    self.seen = Some(o.at.clone());
                    self.open_on(o);
                }
            }
        }
        if let Some(o) = &c.opened {
            if self.seen.as_ref() != Some(&o.at) {
                self.seen = Some(o.at.clone());
                if o.at.recipe == self.recipe && self.recipe != 0 {
                    match self.wait {
                        Some(Wait::Open) => self.open_on(o),
                        Some(Wait::Spinner(s)) if o.at.spinner == s => {
                            self.wait = None;
                            self.at_spinner(o);
                        }
                        None if o.at.spinner == self.spinner => self.at_spinner(o),
                        _ => {}
                    }
                }
            }
        } else {
            self.seen = None;
        }
        for a in &c.added {
            if self.recipe == 0 || self.wait == Some(Wait::Open) {
                continue;
            }
            if a.spinner != self.spinner {
                self.spinner = a.spinner;
                self.spinner_changed(acts);
            }
            if !self.has_slot(a.slot) {
                continue;
            }
            self.waiting.remove(&a.slot);
            if a.error != 0 {
                continue;
            }
            self.slots.retain(|_, item| *item != a.item);
            self.slots.insert(a.slot, a.item);
        }
        for &(_, error) in &c.made {
            if self.recipe == 0 {
                continue;
            }
            let charges = self
                .seen
                .as_ref()
                .filter(|s| s.recipe == self.recipe)
                .map_or(-1, |s| s.charges);
            self.making = false;
            self.waiting.clear();
            self.went = Some(error == 0);
            if error == 0 && charges == 0 {
                self.close(acts);
            }
        }
    }

    /// Opened on the game's reading of a recipe: at the recipe's last spinner value, every slot
    /// empty.
    fn open_on(&mut self, o: &Opened) {
        self.wait = None;
        self.making = false;
        self.went = None;
        self.slots.clear();
        self.waiting.clear();
        self.recipe = o.at.recipe;
        self.spinner = o.at.last_spinner;
        self.typed = self.spinner.to_string();
        self.shown = Some(o.clone());
        if !self.window.open {
            self.window = WindowState {
                open: true,
                ..WindowState::default()
            };
        }
    }

    /// Whether the recipe as shown takes an ingredient in a slot.
    fn has_slot(&self, slot: u32) -> bool {
        self.shown
            .as_ref()
            .is_some_and(|o| o.ingredients.iter().any(|(id, _)| *id == slot))
    }

    /// The recipe as read at the spinner's value: the slots it no longer takes go, and an item
    /// that no longer fits its slot leaves it.
    fn at_spinner(&mut self, o: &Opened) {
        self.shown = Some(o.clone());
        let fits = |slot: u32, item: u64| o.at.fits.get(&(slot, item)).copied().unwrap_or(false);
        let takes = |slot: u32| o.ingredients.iter().any(|(id, _)| *id == slot);
        self.slots
            .retain(|slot, item| takes(*slot) && fits(*slot, *item));
        self.waiting.retain(|slot| takes(*slot));
    }

    /// The spinner moved: the box shows it, and the recipe at its value follows (asked of the
    /// game when it has not read it there yet).
    fn spinner_changed(&mut self, acts: &mut Vec<Action>) {
        self.typed = self.spinner.to_string();
        let read = self
            .seen
            .as_ref()
            .filter(|s| s.recipe == self.recipe && s.spinner == self.spinner)
            .cloned();
        match (read, &self.shown) {
            (Some(at), Some(shown)) => {
                let mut o = shown.clone();
                o.at = at;
                self.at_spinner(&o);
            }
            _ => {
                self.wait = Some(Wait::Spinner(self.spinner));
                acts.push(Action::Craft(CraftAction::Inquire {
                    recipe: self.recipe,
                    spinner: self.spinner,
                }));
            }
        }
    }

    /// The spinner stepped up or down within its bounds.
    pub fn step(&mut self, up: bool, acts: &mut Vec<Action>) {
        let Some(at) = self.shown.as_ref().map(|o| o.at.clone()) else {
            return;
        };
        if at.read_only_spinner {
            return;
        }
        self.spinner = self
            .spinner
            .clamp(at.min_spinner, at.max_spinner.max(at.min_spinner));
        if up && self.spinner < at.max_spinner {
            self.spinner += 1;
        } else if !up && at.min_spinner < self.spinner {
            self.spinner -= 1;
        } else {
            return;
        }
        self.spinner_changed(acts);
    }

    /// The number typed into the spinner's box taken, kept within its bounds.
    fn take_typed(&mut self, acts: &mut Vec<Action>) {
        let Some(at) = self.shown.as_ref().map(|o| o.at.clone()) else {
            return;
        };
        let Ok(n) = self.typed.trim().parse::<i64>() else {
            self.typed = self.spinner.to_string();
            return;
        };
        let n = n.clamp(
            i64::from(at.min_spinner),
            i64::from(at.max_spinner.max(at.min_spinner)),
        );
        let n = i32::try_from(n).unwrap_or(at.min_spinner);
        if n != self.spinner {
            self.spinner = n;
            self.spinner_changed(acts);
        } else {
            self.typed = n.to_string();
        }
    }

    /// An item asked to go into a slot: none while a making is under way, or onto a slot it is
    /// in; out of any other slot, and what the slot held out of it.
    pub fn put_in(&mut self, item: u64, slot: u32, acts: &mut Vec<Action>) {
        if self.making || !self.has_slot(slot) || self.slots.get(&slot) == Some(&item) {
            return;
        }
        self.slots.retain(|_, i| *i != item);
        self.slots.remove(&slot);
        self.waiting.insert(slot);
        acts.push(Action::Craft(CraftAction::AddIngredient {
            recipe: self.recipe,
            ordinal: slot,
            item,
            spinner: self.spinner,
        }));
    }

    /// An item carried out of its slot: it leaves it.
    pub fn take_out(&mut self, slot: u32) {
        if !self.making {
            self.slots.remove(&slot);
        }
    }

    /// Fill in: for the player's items in order, the first that fits each empty slot that fills
    /// itself in is asked to go there.
    fn fill_in(&mut self, acts: &mut Vec<Action>) {
        let Some(at) = self
            .seen
            .as_ref()
            .filter(|s| s.recipe == self.recipe && s.spinner == self.spinner)
            .cloned()
        else {
            return;
        };
        let mut open: Vec<u32> = self
            .shown
            .as_ref()
            .map(|o| o.ingredients.iter().map(|(id, _)| *id).collect())
            .unwrap_or_default();
        for item in &at.items {
            if self.slots.values().any(|i| i == item) {
                continue;
            }
            let found = open.iter().position(|slot| {
                at.auto_populate.get(slot).copied().unwrap_or(false)
                    && !self.slots.contains_key(slot)
                    && at.fits.get(&(*slot, *item)).copied().unwrap_or(false)
            });
            if let Some(k) = found {
                let slot = open.remove(k);
                self.waiting.insert(slot);
                acts.push(Action::Craft(CraftAction::AddIngredient {
                    recipe: self.recipe,
                    ordinal: slot,
                    item: *item,
                    spinner: self.spinner,
                }));
            }
        }
    }

    /// Draw it while it is open; where it is.
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        carry: &mut Carry,
        tip: &mut Option<Tip>,
        acts: &mut Vec<Action>,
    ) -> Option<Rect> {
        if !self.is_open() {
            self.drawn = None;
            return None;
        }
        let Some(shown) = self.shown.clone() else {
            self.drawn = None;
            return None;
        };
        let k = p.scale;
        #[allow(clippy::cast_precision_loss)]
        let slots = shown.ingredients.len().max(1) as f32;
        let size = kit::window_size_for(p, (WIDTH, 86.0 + slots * SLOT_ROW + 60.0));
        let at = (
            (p.screen.0 / 2.0 + 60.0 * k).round(),
            ((p.screen.1 - size.1 * k) / 2.0).round().max(0.0),
        );
        let at = crate::ui::place::clear(p, state, size, at);
        let w = kit::window(p, ctx, &mut self.window, "Making", size, at);
        if w.closed {
            self.window.open = true;
            self.close(acts);
            if self.recipe == 0 {
                return Some(w.rect);
            }
        }
        self.drawn = Some((w.rect, w.body));
        carry.zone(w.rect, Zone::Window);
        let l = Layout::of(w.body, k);
        let a = &shown.at;
        let colour = DIFFICULTY[difficulty_colour(a).min(6) as usize];
        let title = TextStyle::new(Family::Body, 15.0, colour).edge(ctx.colours.edge());
        let small = TextStyle::new(Family::Body, 12.0, 0xFFC8_C0B0).edge(ctx.colours.edge());
        let name = p.fit(&title, &shown.name, w.body.w - 24.0 * k);
        p.text(&title, w.body.x + 12.0 * k, w.body.y + 6.0 * k, &name);
        let mut line = format!("Cost {}", cost_at(a));
        if a.charges > 0 {
            let _ = write!(line, "    Charges {}", a.charges);
        }
        if let Some(went) = self.went {
            line.push_str(if went { "    Made." } else { "    Not made." });
        }
        p.text(&small, w.body.x + 12.0 * k, w.body.y + 28.0 * k, &line);
        if !(a.min_spinner == 0 && a.max_spinner == 0) {
            self.spinner_row(p, ctx, a, &l, acts);
        }
        self.slots_rows(p, ctx, state, &shown, &l, carry, tip);
        if a.auto_populating && kit::button(p, ctx, l.fill_in, "Auto-Fill", !self.making) {
            self.fill_in(acts);
        }
        let unlit = self.unlit(state.pets.mounted);
        if kit::button(p, ctx, l.craft, "Craft", unlit.is_none()) {
            self.making = true;
            self.went = None;
            acts.push(Action::Craft(CraftAction::ExecuteCraft {
                recipe: self.recipe,
                spinner: self.spinner,
                ingredients: self.slots.iter().map(|(s, i)| (*s, *i)).collect(),
            }));
        }
        if let Some(t) = unlit.and_then(Unlit::tip) {
            if ctx.input.hover(&l.craft) {
                *tip = Some(Tip {
                    title: "Craft".to_owned(),
                    lines: vec![t.to_owned()],
                });
            }
        }
        Some(w.rect)
    }

    /// The spinner: down, its box, up.
    fn spinner_row(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        a: &RecipeAt,
        l: &Layout,
        acts: &mut Vec<Action>,
    ) {
        let fixed = a.read_only_spinner;
        if kit::button(p, ctx, l.down, "-", !fixed && a.min_spinner < self.spinner) {
            self.step(false, acts);
        }
        if kit::button(p, ctx, l.up, "+", !fixed && self.spinner < a.max_spinner) {
            self.step(true, acts);
        }
        if fixed {
            let style =
                TextStyle::new(Family::Numerals, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
            p.text_in(&style, l.value, Align::Centre, &self.spinner.to_string());
            return;
        }
        let entered = kit::text_box_aligned(
            p,
            ctx,
            l.value,
            &mut self.typed,
            &mut self.focus,
            SPINNER_BOX,
            Align::Right,
        );
        self.typed.retain(|c| c.is_ascii_digit() || c == '-');
        if entered {
            self.focus = 0;
            self.take_typed(acts);
        }
    }

    /// The slots: each its tile (the item in it), and what it takes.
    #[allow(clippy::too_many_arguments)]
    fn slots_rows(
        &self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        shown: &Opened,
        l: &Layout,
        carry: &mut Carry,
        tip: &mut Option<Tip>,
    ) {
        let k = p.scale;
        let body = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        for (n, (slot, what)) in shown.ingredients.iter().enumerate() {
            let r = l.slot(n, k);
            let item = self
                .slots
                .get(slot)
                .and_then(|id| state.goods.item(*id).map(|it| (*id, it)));
            let icon = item.map(|(_, it)| Icon::of(it));
            let t = items::tile(
                p,
                ctx,
                r,
                TileLook {
                    icon: icon.as_ref(),
                    quantity: item.map(|(_, it)| it.quantity),
                    dim: self.making || self.waiting.contains(slot),
                    ..TileLook::default()
                },
            );
            carry.zone(r, Zone::Ingredient { slot: *slot });
            let text = Rect::new(
                r.right() + 10.0 * k,
                r.y,
                l.body.right() - r.right() - 22.0 * k,
                r.h,
            );
            let label = match item {
                Some((_, it)) => format!("{what}: {}", it.name),
                None => what.clone(),
            };
            let fitted = p.fit(&body, &label, text.w);
            p.text_in(&body, text, Align::Left, &fitted);
            if t.over {
                *tip = Some(Tip {
                    title: what.clone(),
                    lines: item
                        .map(|(_, it)| vec![it.name.clone()])
                        .unwrap_or_default(),
                });
            }
            if let (true, Some((id, _))) = (t.examine, item) {
                carry.examine = Some(id);
            }
            if let (true, Some((id, it)), false) = (t.pressed, item, self.making) {
                carry.press(
                    ctx,
                    Carried::Ingredient {
                        item: id,
                        slot: *slot,
                    },
                    Icon::of(it),
                    ctx.time,
                );
            }
        }
    }

    /// A thing carried put down on a slot, or an item carried out of a slot put down anywhere
    /// else: whether this window took it.
    pub fn put(&mut self, carried: Carried, onto: Option<Zone>, acts: &mut Vec<Action>) -> bool {
        match (carried, onto) {
            (
                Carried::Item { item, .. }
                | Carried::Worn { item, .. }
                | Carried::Ingredient { item, .. },
                Some(Zone::Ingredient { slot }),
            ) => {
                self.put_in(item, slot, acts);
                true
            }
            (Carried::Ingredient { slot, .. }, _) => {
                self.take_out(slot);
                true
            }
            (_, Some(Zone::Ingredient { .. })) => true,
            _ => false,
        }
    }
}
