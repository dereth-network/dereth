//! The settings window, a tab a page. **General:** which interface the client shows, the orbit
//! camera's preferences (how the movement keys move the player under it, and whether the mouse
//! turns it the other way across and up and down), whether the map coordinates show under the
//! minimap, the interface scale, the HUD layout's editor, logging out, and the view behind
//! character select ([`crate::vista`]). **Graphics**,
//! **Sound** and **Camera:** the client's own preferences, as the game's own options window lists
//! them ([`crate::ui::options`]). **Character:** the player's options. **Chat:** the chat windows'
//! options. **Key Bindings:** Horizon's key bindings ([`crate::ui::bindings`]).
//!
//! The interface and the camera's are the client's own preferences: choosing the game's own
//! interface switches to it from the next frame and keeps the choice, and the camera's are applied
//! and kept as they are set, as the options window's client page does with its own. The map
//! coordinates are one of the player's options, kept with them as the radar's button keeps it.
//! The scale and the view are Horizon's own ([`crate::own`]): taken from the next frame and kept in
//! its file.

use auberean_client_sdk::Action;
use auberean_shell::interface::{Choice, MovementMode};
use dereth_ui_kit::art::Family;
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx, WindowState};

use crate::draw::Rect;
use crate::ui::bindings::Bindings;
use crate::ui::options::{PageData, Pages, PREFERENCE_PAGES};
use crate::ui::Outcome;
use crate::vista::Choice as VistaChoice;

/// The window's body, in layout units.
const BODY: (f32, f32) = (660.0, 470.0);

/// The pages' tabs, in order.
pub const TABS: [&str; 7] = [
    "General",
    "Graphics",
    "Sound",
    "Camera",
    "Character",
    "Chat",
    "Key Bindings",
];

/// The tabs' places in [`TABS`].
pub mod tab {
    pub const GENERAL: usize = 0;
    pub const CHARACTER: usize = 4;
    pub const CHAT: usize = 5;
    pub const KEYS: usize = 6;
}

/// The room the tabs take at the top of the body, and a page's inset from the body's left, in
/// layout units: the tabs run the body's width and the page starts under them, as Dereth's
/// Horizon lays out its settings.
const TAB_BAR: f32 = 36.0;
const PAGE_INSET: f32 = 8.0;

/// The room the view's name takes on its row, and the width of its box, in layout units.
const VISTA_LABEL: f32 = 180.0;
const VISTA_BOX: f32 = 220.0;

/// The interface scale's choices as the window lists them: Auto, then each step.
pub const SCALES: [&str; 5] = ["Auto", "100%", "150%", "200%", "300%"];

/// The interfaces as the window lists them.
pub const INTERFACES: [&str; 2] = ["Retail", "Horizon"];

/// The movement modes as the window lists them, in [`MovementMode::ALL`]'s order.
pub const MOVEMENTS: [&str; 2] = ["Camera-based", "Character-based"];

/// The orbit camera's preferences as the client last listed them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Orbit {
    pub movement: MovementMode,
    pub reverse_x: bool,
    pub reverse_y: bool,
}

/// One of the orbit camera's preferences set in the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrbitChange {
    Movement(MovementMode),
    ReverseX(bool),
    ReverseY(bool),
}

/// Where the window's controls are.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Controls {
    /// The interface's row of choices.
    pub interface: Rect,
    /// The movement's row of choices.
    pub movement: Rect,
    pub reverse_x: Rect,
    pub reverse_y: Rect,
    /// The map coordinates under the minimap.
    pub coordinates: Rect,
    /// The interface scale's row of choices ([`SCALES`]).
    pub scale: Rect,
    /// Opens the HUD layout's editor.
    pub layout: Rect,
    pub log_out: Rect,
}

/// The settings window.
#[derive(Debug, Default)]
pub struct Settings {
    window: WindowState,
    /// Where it was last drawn, and its body, while it is open.
    drawn: Option<(Rect, Rect)>,
    /// The page shown, by its place in [`TABS`].
    pub tab: usize,
    /// The pages of options, and what they read.
    pub pages: Pages,
    pub data: PageData,
    /// The key bindings page.
    pub bindings: Bindings,
    /// The player's options and the chat options asked for this frame.
    pub acts: Vec<Action>,
    /// Horizon's own settings, as the window shows and changes them.
    pub own: crate::own::Own,
    /// The window changed Horizon's own settings since this was last taken.
    own_changed: bool,
    /// HUD layout was pressed since this was last taken.
    layout_asked: bool,
    /// Log Out was pressed.
    log_out_asked: bool,
    /// The views' list is open under their box, and the first of them it shows.
    vista_menu: bool,
    vista_top: usize,
}

impl Settings {
    /// Whether it is open.
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.window.open
    }

    /// Open it, or close it.
    pub fn toggle(&mut self) {
        if self.window.open {
            self.close();
        } else {
            self.window.open = true;
        }
    }

    /// Whether a box in it has the keys (a chat tab's name).
    #[must_use]
    pub const fn typing(&self) -> bool {
        self.window.open && self.pages.typing()
    }

    /// Close it: the key bindings not applied are let go of, and a box typed in lets go.
    pub fn close(&mut self) {
        self.pages.let_go();
        self.window.open = false;
        self.drawn = None;
        self.bindings.maps = None;
        self.bindings.cancel();
        self.bindings.changed = false;
        self.vista_menu = false;
    }

    /// The sections of the client's preferences the page shown lists, while it is open.
    #[must_use]
    pub fn sections(&self) -> &'static [u32] {
        if !self.window.open {
            return &[];
        }
        self.tab
            .checked_sub(1)
            .and_then(|i| PREFERENCE_PAGES.get(i))
            .map_or(&[], |(_, s)| s)
    }

    /// Where the tabs are in a window drawn with `body` as its body, at `k` pixels a layout unit.
    #[must_use]
    pub fn tab_bar(body: Rect, k: f32) -> Rect {
        Rect::new(body.x, body.y, body.w, 26.0 * k)
    }

    /// Where a page is drawn in a window drawn with `body` as its body.
    #[must_use]
    pub fn page(body: Rect, k: f32) -> Rect {
        Rect::new(
            body.x + PAGE_INSET * k,
            body.y + TAB_BAR * k,
            body.w - PAGE_INSET * k,
            body.h - (TAB_BAR + 4.0) * k,
        )
    }

    /// Where it was last drawn, while it is open.
    #[must_use]
    pub fn rect(&self) -> Option<Rect> {
        self.drawn.map(|(r, _)| r)
    }

    /// Where its body was last drawn, while it is open.
    #[must_use]
    pub fn body(&self) -> Option<Rect> {
        self.drawn.map(|(_, b)| b)
    }

    /// Horizon's own settings changed outside the window (the map's Follow), to be kept.
    pub fn own_set(&mut self) {
        self.own_changed = true;
    }

    /// Whether the window changed Horizon's own settings since this was last asked.
    pub fn take_own_changed(&mut self) -> bool {
        std::mem::take(&mut self.own_changed)
    }

    /// Whether HUD layout was pressed since this was last asked.
    pub fn take_layout_asked(&mut self) -> bool {
        std::mem::take(&mut self.layout_asked)
    }

    /// Whether Log Out was pressed since this was last asked.
    pub fn take_log_out_asked(&mut self) -> bool {
        std::mem::take(&mut self.log_out_asked)
    }

    /// Where the controls are in a window drawn with `body` as its body, at `k` pixels a layout
    /// unit.
    #[must_use]
    pub fn controls(body: Rect, k: f32) -> Controls {
        let page = Self::page(body, k);
        let row = |y: f32| Rect::new(page.x, page.y + y * k, page.w - 12.0 * k, 26.0 * k);
        let button = |x: f32| Rect::new(page.x + x * k, page.y + 332.0 * k, 140.0 * k, 30.0 * k);
        Controls {
            interface: row(26.0),
            movement: row(94.0),
            reverse_x: row(126.0),
            reverse_y: row(154.0),
            coordinates: row(212.0),
            scale: row(270.0),
            layout: button(0.0),
            log_out: button(156.0),
        }
    }

    /// Where the General page's heading `y` layout units down the page is written.
    fn heading_at(body: Rect, k: f32, y: f32) -> (f32, f32) {
        let page = Self::page(body, k);
        (page.x, page.y + y * k)
    }

    /// Draw it while it is open, `shown` the interface the client shows, `orbit` the camera's
    /// preferences (none until the client lists them) and `coordinates` whether the map
    /// coordinates show (none outside the world); where it is.
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        shown: Choice,
        orbit: Option<Orbit>,
        coordinates: Option<bool>,
        out: &mut Outcome,
    ) -> Option<Rect> {
        if !self.window.open {
            return None;
        }
        let k = p.scale;
        let size = kit::window_size_for(p, BODY);
        let at = (
            ((p.screen.0 - size.0 * k) / 2.0).round(),
            ((p.screen.1 - size.1 * k) / 2.0).round(),
        );
        let w = kit::window(p, ctx, &mut self.window, "Settings", size, at);
        if w.closed {
            self.close();
            return Some(w.rect);
        }
        self.drawn = Some((w.rect, w.body));
        let before = self.tab;
        kit::tabs(p, ctx, Self::tab_bar(w.body, k), &TABS, &mut self.tab);
        if self.tab != before {
            self.pages.top();
            self.bindings.cancel();
            self.vista_menu = false;
        }
        let page = Self::page(w.body, k);
        match self.tab {
            tab::GENERAL => {}
            tab::CHARACTER => {
                let options = self.data.options.clone();
                self.pages
                    .character(p, ctx, page, options.as_ref(), &mut out.acts);
                return Some(w.rect);
            }
            tab::CHAT => {
                let chat = self.data.options.as_ref().map(|o| o.chat.clone());
                if self
                    .pages
                    .chat(p, ctx, page, chat.as_ref(), &mut self.own, &mut out.acts)
                {
                    self.own_changed = true;
                }
                return Some(w.rect);
            }
            tab::KEYS => {
                self.bindings.frame(p, ctx, page);
                return Some(w.rect);
            }
            _ => {
                let sections = self.sections();
                self.pages
                    .preferences(p, ctx, page, &mut self.data, sections);
                return Some(w.rect);
            }
        }
        let heading = TextStyle::new(Family::Heading, 15.0, ctx.colours.heading());
        let body = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let c = Self::controls(w.body, k);
        // The views' open list takes the presses over it before anything under it is drawn, and a
        // press anywhere else closes it.
        let occluders = ctx.input.occluders.len();
        if self.vista_menu {
            let (_, vista_box) = Self::vista_rows(&c, w.body, k);
            let count = VistaChoice::ALL.len();
            if kit::dropdown_dismissed(p, ctx, vista_box, count) {
                self.vista_menu = false;
            } else {
                let r = kit::dropdown_list_rect(p, vista_box, count);
                ctx.input.occluders.push(r);
            }
        }
        let (x, y) = Self::heading_at(w.body, k, 0.0);
        p.text(&heading, x, y, "Interface");
        let at = Choice::ALL.iter().position(|c| *c == shown);
        if let Some(i) = kit::radios(p, ctx, c.interface, &INTERFACES, at) {
            if Some(i) != at {
                out.choose = Choice::ALL.get(i).copied();
            }
        }
        let (x, y) = Self::heading_at(w.body, k, 68.0);
        p.text(&heading, x, y, "Camera");
        if let Some(o) = orbit {
            let at = MovementMode::ALL.iter().position(|m| *m == o.movement);
            if let Some(i) = kit::radios(p, ctx, c.movement, &MOVEMENTS, at) {
                if Some(i) != at {
                    out.orbit = MovementMode::ALL.get(i).copied().map(OrbitChange::Movement);
                }
            }
            let across = "Reverse Horizontal Turning";
            if let Some(v) = kit::check_row(p, ctx, c.reverse_x, o.reverse_x, &body, across) {
                out.orbit = Some(OrbitChange::ReverseX(v));
            }
            let down = "Reverse Vertical Turning";
            if let Some(v) = kit::check_row(p, ctx, c.reverse_y, o.reverse_y, &body, down) {
                out.orbit = Some(OrbitChange::ReverseY(v));
            }
        }
        let (x, y) = Self::heading_at(w.body, k, 186.0);
        p.text(&heading, x, y, "Minimap");
        if let Some(on) = coordinates {
            let line = "Show Coordinates By the Radar";
            if let Some(v) = kit::check_row(p, ctx, c.coordinates, on, &body, line) {
                out.acts.push(Action::ShowRadarCoordinates { on: v });
                out.acts.push(Action::KeepOptions);
            }
        }
        self.scale_rows(p, ctx, &c, w.body);
        if kit::button(p, ctx, c.layout, "HUD Layout", true) {
            self.layout_asked = true;
            self.close();
        }
        if kit::button(p, ctx, c.log_out, "Log Out", true) {
            self.log_out_asked = true;
        }
        self.vista_row(p, ctx, &c, w.body, occluders);
        Some(w.rect)
    }

    /// Where the view behind character select is chosen, in a window drawn with `body` as its body
    /// and its controls at `c`, at `k` pixels a layout unit: one row under the buttons, its name on
    /// the left and after it the box that opens the list of choices ([`VistaChoice::ALL`]).
    #[must_use]
    pub fn vista_rows(c: &Controls, body: Rect, k: f32) -> (Rect, Rect) {
        let y = c.layout.bottom() + 14.0 * k;
        let label = Rect::new(c.layout.x, y, VISTA_LABEL * k, 26.0 * k);
        let room = body.right() - 12.0 * k - label.right();
        let choices = Rect::new(label.right(), y, room.min(VISTA_BOX * k), 26.0 * k);
        (label, choices)
    }

    /// The view behind character select: its name, and its box, whose list of choices opens
    /// under it (over it where the screen has no room below), drawn over the page; `occluders`
    /// is how many the page had before the list took its own.
    fn vista_row(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        c: &Controls,
        body: Rect,
        occluders: usize,
    ) {
        let k = p.scale;
        let style = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let (label, choices) = Self::vista_rows(c, body, k);
        p.text_in(&style, label, Align::Left, "Character Select View");
        let names = VistaChoice::ALL.map(VistaChoice::name);
        let at = VistaChoice::ALL
            .iter()
            .position(|v| *v == self.own.vista)
            .unwrap_or(0);
        if kit::dropdown_box(p, ctx, choices, names[at], self.vista_menu, true) {
            self.vista_menu = !self.vista_menu;
            self.vista_top = 0;
        }
        ctx.input.occluders.truncate(occluders);
        if self.vista_menu {
            if let Some(i) = kit::dropdown_list(p, ctx, choices, &names, at, &mut self.vista_top) {
                self.vista_menu = false;
                if let Some(v) = VistaChoice::ALL.get(i).filter(|v| **v != self.own.vista) {
                    self.own.vista = *v;
                    self.own_changed = true;
                }
            }
        }
    }

    /// The interface scale's choices, and under them why the step chosen is not drawn or which
    /// steps a larger window would offer.
    fn scale_rows(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, c: &Controls, body: Rect) {
        let k = p.scale;
        let heading = TextStyle::new(Family::Heading, 15.0, ctx.colours.heading());
        let small = TextStyle::new(Family::Body, 12.0, ctx.colours.text()).edge(ctx.colours.edge());
        let (x, y) = Self::heading_at(body, k, 244.0);
        p.text(&heading, x, y, "Interface Scale");
        let chosen = self.own.scale;
        let at = match chosen {
            None => Some(0),
            Some(s) => crate::scale::STEPS
                .iter()
                .position(|t| (t - s).abs() < f32::EPSILON)
                .map(|i| i + 1),
        };
        if let Some(i) = kit::radios(p, ctx, c.scale, &SCALES, at) {
            if Some(i) != at {
                self.own.scale = i
                    .checked_sub(1)
                    .and_then(|i| crate::scale::STEPS.get(i))
                    .copied();
                self.own_changed = true;
            }
        }
        if let Some(line) = crate::scale::note(chosen, p.screen) {
            let (x, y) = Self::heading_at(body, k, 302.0);
            p.text(&small, x, y, &line);
        }
    }
}
