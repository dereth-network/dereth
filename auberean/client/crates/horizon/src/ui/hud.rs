//! The HUD: the parameter bar's gauges (health and vigor) along the foot of the screen and a
//! hero's focus gauge beside them, the target bar at its top under the experience band (the
//! selection's name and health, with no frame round it, attacking or not), the status row of the
//! effects on the player at its top right, and the menu bar at its bottom right. The experience
//! band, the link line and the vitae lamp are [`super::status`]'s.
//!
//! **The menu bar** holds a button for each of the windows (Map, Examine, Inventory, Skills,
//! Crafting, Quests, Social), its icon glowing while its window is open or the pointer is over it,
//! and a last one that opens a menu over it: Settings, HUD Layout, Log Out and Exit Game. A button
//! opens or closes its window; Social's holds the fellowship, the allegiance and the friends.

use dereth_ui_kit::art::Family;
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx};

use crate::draw::{Argb, Rect, WHITE};
use crate::state::{GameState, Vital};
use crate::ui::Tip;

/// The gauges' colors: health green, vigor gold, focus blue; the experience band's.
pub const HEALTH: Argb = 0xFF78_D85A;
pub const VIGOR: Argb = 0xFFF0_C050;
pub const FOCUS: Argb = 0xFF60_A8F0;
pub use dereth_ui_kit::hud::EXPERIENCE;

/// The red of the player attacking.
pub const ATTACKING: Argb = 0xFFE0_4030;

/// A gauge's width and the step between gauges, in layout units.
const GAUGE_W: f32 = 160.0;
const GAUGE_STEP: f32 = 164.0;

/// How many effects a row of the status row holds, and how many rows it has.
pub const EFFECTS_PER_ROW: usize = 9;
pub const EFFECT_ROWS: usize = 3;

/// The side of an effect's icon, in layout units.
const ICON: f32 = 32.0;

/// Where the parameter bar's gauges and the focus gauge beside them are on a screen drawn at `k`
/// pixels a layout unit, the three together along the foot: health, vigor and focus, each its
/// bar.
#[must_use]
pub fn gauge_rects(screen: (f32, f32), k: f32) -> [Rect; 3] {
    let x0 = screen.0 / 2.0 - 3.0 * GAUGE_STEP * k / 2.0;
    let y = screen.1 - 48.0 * k;
    #[allow(clippy::cast_precision_loss)]
    let at = |i: usize| {
        Rect::new(
            x0 + i as f32 * GAUGE_STEP * k,
            y + 18.0 * k,
            GAUGE_W * k,
            20.0 * k,
        )
    };
    [at(0), at(1), at(2)]
}

/// Where health's and vigor's bars are: beside the focus gauge as [`gauge_rects`] has them while
/// `focus_beside`, else the two closed up, in the middle of the foot.
#[must_use]
pub fn vital_rects(screen: (f32, f32), k: f32, focus_beside: bool) -> [Rect; 2] {
    let [health, vigor, _] = gauge_rects(screen, k);
    if focus_beside {
        return [health, vigor];
    }
    let shift = GAUGE_STEP * k / 2.0;
    [health.offset(shift, 0.0), vigor.offset(shift, 0.0)]
}

/// Where the focus gauge is at its default place, its label and value over its bar: beside
/// vigor.
#[must_use]
pub fn focus_rect(screen: (f32, f32), k: f32) -> Rect {
    let bar = gauge_rects(screen, k)[2];
    Rect::new(bar.x, bar.y - 18.0 * k, bar.w, bar.h + 18.0 * k)
}

/// One vital's gauge over `bar`: its label, its value and its bar; over it, its value of its
/// greatest.
fn vital_gauge(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    (name, v, colour): (&str, Vital, Argb),
    bar: Rect,
    tip: &mut Option<Tip>,
) {
    let k = p.scale;
    let label = TextStyle::new(Family::Numerals, 12.0, WHITE).edge(0xFF33_2211);
    let number = TextStyle::new(Family::BarNumbers, 14.0, WHITE).edge(0xFF33_2211);
    kit::gauge(p, bar, v.ratio(), colour);
    p.text(&label, bar.x + 6.0 * k, bar.y - 12.0 * k, name);
    let n = v.current.to_string();
    let w = p.measure(&number, &n);
    p.text(&number, bar.right() - 8.0 * k - w, bar.y - 14.0 * k, &n);
    let hit = Rect::new(bar.x, bar.y - 18.0 * k, bar.w, bar.h + 18.0 * k);
    if ctx.over_quiet(&hit) {
        *tip = Some(Tip {
            title: name.to_owned(),
            lines: vec![format!("{} / {}", v.current, v.max)],
        });
    }
}

/// The parameter bar: health's and vigor's gauges, beside the focus gauge while `focus_beside`,
/// else closed up.
pub fn parameter_bar(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    state: &GameState,
    focus_beside: bool,
    tip: &mut Option<Tip>,
) {
    let vitals = [
        ("Health", state.health, HEALTH),
        ("Vigor", state.vigor, VIGOR),
    ];
    for (vital, bar) in vitals
        .into_iter()
        .zip(vital_rects(p.screen, p.scale, focus_beside))
    {
        vital_gauge(p, ctx, vital, bar, tip);
    }
}

/// The focus gauge, for a hero alone: only a hero has focus.
pub fn focus_gauge(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    state: &GameState,
    tip: &mut Option<Tip>,
) {
    if !state.hero {
        return;
    }
    let bar = gauge_rects(p.screen, p.scale)[2];
    vital_gauge(p, ctx, ("Focus", state.focus, FOCUS), bar, tip);
}

/// Where the target bar's gauge is on a screen drawn at `k` pixels a layout unit: under the
/// experience band.
#[must_use]
pub fn target_gauge_rect(screen: (f32, f32), k: f32) -> Rect {
    Rect::new(screen.0 / 2.0 - 160.0 * k, 68.0 * k, 320.0 * k, 16.0 * k)
}

/// Where the target bar is at its default place: the selection's name over its gauge.
#[must_use]
pub fn target_rect(screen: (f32, f32), k: f32) -> Rect {
    let bar = target_gauge_rect(screen, k);
    Rect::new(bar.x, bar.y - 28.0 * k, bar.w, bar.h + 28.0 * k)
}

/// The target bar: the selection's name, and its health once the scripts told it; nothing round
/// it, whether or not the player attacks.
pub fn target_bar(p: &mut Painter<'_>, state: &GameState) {
    let Some(t) = &state.target else {
        return;
    };
    let k = p.scale;
    let bar = target_gauge_rect(p.screen, k);
    let name = TextStyle::new(Family::Heading, 16.0, 0xFFF4_E8C8).edge(0xFF1A_1008);
    p.text_in(
        &name,
        Rect::new(bar.x, bar.y - 28.0 * k, bar.w, 24.0 * k),
        Align::Centre,
        &t.name,
    );
    if let Some(h) = t.health {
        kit::gauge(p, bar, h.ratio(), HEALTH);
    }
}

/// Where the status row's `n`th effect is on a screen drawn at `k` pixels a layout unit: rows of
/// [`EFFECTS_PER_ROW`] from the top right, leftward.
#[must_use]
pub fn effect_rect(screen: (f32, f32), k: f32, n: usize) -> Rect {
    let (row, col) = (n / EFFECTS_PER_ROW, n % EFFECTS_PER_ROW);
    #[allow(clippy::cast_precision_loss)]
    let (row, col) = (row as f32, col as f32);
    let step = ICON + 6.0;
    Rect::new(
        screen.0 - 16.0 * k - (col + 1.0) * step * k + 6.0 * k,
        16.0 * k + row * (step + 12.0) * k,
        ICON * k,
        ICON * k,
    )
}

/// Where the status row is at its default place, full: every effect's icon and what is left of
/// it under the icon.
#[must_use]
pub fn effects_rect(screen: (f32, f32), k: f32) -> Rect {
    // The first effect stands at the top right, the last at the bottom left.
    let first = effect_rect(screen, k, 0);
    let last = effect_rect(screen, k, EFFECTS_PER_ROW * EFFECT_ROWS - 1);
    Rect::new(
        last.x,
        first.y,
        first.right() - last.x,
        last.bottom() + 12.0 * k - first.y,
    )
}

/// What is left of an effect, as the status row writes it under its icon.
#[must_use]
pub fn time_left(seconds: f64) -> String {
    let s = dereth_primitives::num::to_i32_f64(seconds.max(0.0).ceil());
    if s >= 3600 {
        format!("{}h", (s + 3599) / 3600)
    } else if s >= 60 {
        format!("{}m", (s + 59) / 60)
    } else {
        format!("{s}s")
    }
}

/// The status row: each effect's icon from the game's pictures (a plain tile where there is
/// none), and under it what is left of it; over one, its name.
pub fn effects(p: &mut Painter<'_>, ctx: &mut Ctx<'_>, state: &GameState, tip: &mut Option<Tip>) {
    let k = p.scale;
    let small = TextStyle::new(Family::Numerals, 10.0, 0xFFE8_E0D0).edge(0xFF10_0C08);
    for (n, e) in state
        .effects
        .iter()
        .take(EFFECTS_PER_ROW * EFFECT_ROWS)
        .enumerate()
    {
        let r = effect_rect(p.screen, k, n);
        match p.art.icon(e.icon) {
            Some(icon) => p.sprite(&icon, r, WHITE),
            None => p.fill(r, 0xC020_2830),
        }
        if let Some(left) = e.left {
            p.text_in(
                &small,
                Rect::new(r.x - 4.0 * k, r.bottom() + 1.0 * k, r.w + 8.0 * k, 11.0 * k),
                Align::Centre,
                &time_left(left),
            );
        }
        if ctx.over_quiet(&r) {
            *tip = Some(Tip {
                title: e.name.clone(),
                lines: e.left.map(|l| vec![time_left(l)]).unwrap_or_default(),
            });
        }
    }
}

/// What a button of the menu bar opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuButton {
    Map,
    Examine,
    Inventory,
    Skills,
    Crafting,
    Quests,
    /// The social window, on whichever tab it showed last.
    Social,
    /// The menu of Settings, HUD Layout, Log Out and Exit Game.
    System,
}

impl MenuButton {
    /// The buttons, left to right.
    pub const ALL: [Self; 8] = [
        Self::Map,
        Self::Examine,
        Self::Inventory,
        Self::Skills,
        Self::Crafting,
        Self::Quests,
        Self::Social,
        Self::System,
    ];

    /// Its name, as its tooltip gives it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Map => "Map",
            Self::Examine => "Examine",
            Self::Inventory => "Inventory",
            Self::Skills => "Skills",
            Self::Crafting => "Crafting",
            Self::Quests => "Quests",
            Self::Social => "Social",
            Self::System => "System",
        }
    }

    /// The art's pieces for its icon, the first the art has drawn.
    #[must_use]
    pub const fn icons(self) -> &'static [&'static str] {
        match self {
            Self::Map => &["menu.map"],
            Self::Examine => &["menu.examine"],
            Self::Inventory => &["menu.inventory"],
            Self::Skills => &["menu.skills", "menu.character"],
            Self::Crafting => &["menu.crafting"],
            Self::Quests => &["menu.quests", "menu.journal"],
            Self::Social => &["menu.fellowship", "menu.social"],
            Self::System => &["menu.settings"],
        }
    }
}

/// What the menu bar's last button's menu holds, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemItem {
    Settings,
    Layout,
    LogOut,
    Quit,
}

impl SystemItem {
    pub const ALL: [Self; 4] = [Self::Settings, Self::Layout, Self::LogOut, Self::Quit];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Settings => "Settings",
            Self::Layout => "HUD Layout",
            Self::LogOut => "Log Out",
            Self::Quit => "Exit Game",
        }
    }
}

/// The system menu's width, its rows' height and its padding above and below them, in layout
/// units.
const SYSTEM_W: f32 = 150.0;
const SYSTEM_ROW: f32 = 26.0;
const SYSTEM_PAD: f32 = 10.0;

/// The menu bar's margin from the screen's right edge and foot, in layout units: closer in than
/// Dereth's, which has fewer buttons, so the plate keeps clear of the hotbar.
const MENU_MARGIN: f32 = 8.0;

/// Where the menu bar is on a screen drawn at `k` pixels a layout unit: Dereth's Horizon's plate,
/// its buttons close together on it at the bottom right with room round them, the plate as far
/// from the screen's right edge as from its foot, each on a whole pixel.
#[must_use]
pub fn menu_rect(screen: (f32, f32), k: f32) -> Rect {
    dereth_ui_kit::hud::menu_plate(screen, k, MenuButton::ALL.len(), MENU_MARGIN)
}

/// Where the menu bar's `n`th button is (from the left).
#[must_use]
pub fn menu_button_rect(screen: (f32, f32), k: f32, n: usize) -> Rect {
    dereth_ui_kit::hud::menu_button(menu_rect(screen, k), k, n)
}

/// Where the system menu is, risen over its button and kept on the screen.
#[must_use]
pub fn system_menu_rect(screen: (f32, f32), k: f32) -> Rect {
    let bar = menu_rect(screen, k);
    let button = menu_button_rect(screen, k, MenuButton::ALL.len() - 1);
    #[allow(clippy::cast_precision_loss)]
    let h = (SystemItem::ALL.len() as f32 * SYSTEM_ROW + 2.0 * SYSTEM_PAD) * k;
    let w = SYSTEM_W * k;
    let x = (button.x + button.w / 2.0 - w / 2.0).min(screen.0 - MENU_MARGIN * k - w);
    Rect::new(x, bar.y - h - 6.0 * k, w, h)
}

/// Where the system menu's `n`th row is.
#[must_use]
pub fn system_row_rect(screen: (f32, f32), k: f32, n: usize) -> Rect {
    let menu = system_menu_rect(screen, k);
    #[allow(clippy::cast_precision_loss)]
    let y = menu.y + (SYSTEM_PAD + n as f32 * SYSTEM_ROW) * k;
    Rect::new(menu.x + 6.0 * k, y, menu.w - 12.0 * k, SYSTEM_ROW * k)
}

/// What the menu bar was asked this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuPick {
    Button(MenuButton),
    System(SystemItem),
}

/// The menu bar: its plate (dark, a little translucent, with a fine gold rim), and on it each
/// button its icon (or its name's first letter, where the art has none), lit (its icon glowing,
/// where the art has the glow) while `lit` says its window is open (in [`MenuButton::ALL`]'s order)
/// or the pointer is over it; the system menu over it while `system_open`. What was pressed, and
/// where the bar and the open menu are.
pub fn menu_bar(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    lit: [bool; MenuButton::ALL.len()],
    system_open: &mut bool,
    tip: &mut Option<Tip>,
) -> (Option<MenuPick>, Vec<Rect>) {
    let k = p.scale;
    let screen = p.screen;
    let bar = menu_rect(screen, k);
    let mut pick = None;
    let mut drawn = vec![bar];
    kit::plain_ground(p, bar);
    let letter = TextStyle::new(Family::Heading, 18.0, 0xFFF4_E8C8).edge(0xFF1A_1008);
    for (i, b) in MenuButton::ALL.into_iter().enumerate() {
        let r = menu_button_rect(screen, k, i);
        let over = ctx.over(&r);
        let on = lit[i] || (b == MenuButton::System && *system_open);
        let icon = b.icons().iter().find_map(|n| p.piece(n).map(|s| (*n, s)));
        if (over || on) && !icon.as_ref().is_some_and(|(n, _)| glow(p, n, r)) {
            p.fill(r, MENU_LIT);
        }
        let colour = if over || on { WHITE } else { 0xFFD0_D0D0 };
        match icon {
            Some((_, icon)) => p.sprite(&icon, r, colour),
            None => {
                p.text_in(&letter, r, Align::Centre, &b.name()[..1]);
            }
        }
        if over {
            if !*system_open {
                *tip = Some(Tip {
                    title: b.name().to_owned(),
                    lines: Vec::new(),
                });
            }
            if ctx.input.clicked(&r) {
                if b == MenuButton::System {
                    *system_open = !*system_open;
                } else {
                    *system_open = false;
                    pick = Some(MenuPick::Button(b));
                }
            }
        }
    }
    if *system_open {
        let menu = system_menu_rect(screen, k);
        kit::plain_ground(p, menu);
        let style = TextStyle::new(Family::Body, 14.0, WHITE).edge(0xFF00_0000);
        for (i, item) in SystemItem::ALL.into_iter().enumerate() {
            let row = system_row_rect(screen, k, i);
            if ctx.over(&row) {
                p.fill(row, MENU_LIT);
            }
            p.text_in(&style, row.offset(10.0 * k, 0.0), Align::Left, item.label());
            if ctx.input.clicked(&row) {
                pick = Some(MenuPick::System(item));
                *system_open = false;
            }
        }
        drawn.push(menu);
        // A press anywhere but the bar and the menu closes the menu.
        let (x, y) = ctx.input.mouse;
        if ctx.input.pressed[0] && !menu.contains(x, y) && !bar.contains(x, y) {
            *system_open = false;
        }
    }
    (pick, drawn)
}

/// The shade a menu bar button or the system menu's row takes while lit or under the pointer.
pub const MENU_LIT: Argb = 0x30C8_9646;

/// The glow behind the art's icon `icon` drawn over `r`: its piece `<icon>.glow`, made from the
/// icon's own shape and reaching as far round it as the art made it. Whether the art has it.
pub fn glow(p: &mut Painter<'_>, icon: &str, r: Rect) -> bool {
    let (Some(face), Some(glow)) = (p.piece(icon), p.piece(&format!("{icon}.glow"))) else {
        return false;
    };
    let (w, h) = (glow.w * r.w / face.w, glow.h * r.h / face.h);
    let at = Rect::new(r.x + (r.w - w) / 2.0, r.y + (r.h - h) / 2.0, w, h);
    p.sprite(&glow, at, WHITE);
    true
}

/// A number with its thousands marked.
pub use dereth_ui_kit::hud::grouped;
