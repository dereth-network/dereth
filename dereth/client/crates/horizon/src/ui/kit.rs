//! The interface's widgets, drawn from the game's own window art: windows, buttons, tabs, icon
//! slots, gauges and tooltips.

use crate::art::{Family, Sprite};
use crate::draw::{Argb, Rect, WHITE};
use crate::ui::colours::Colours;
use crate::ui::input::InputFrame;
use crate::ui::paint::{Align, Painter, TextStyle};

/// The per-frame context every widget is drawn with.
#[derive(Debug)]
pub struct Ctx<'a> {
    /// Seconds since the interface came up.
    pub time: f64,
    /// Seconds since the last frame.
    pub dt: f64,
    pub input: &'a mut InputFrame,
    pub colours: &'a Colours,
    /// Set when the pointer is over any widget this frame.
    pub hot: bool,
    /// What the player has in hand.
    pub drag: &'a mut Option<crate::ui::Drag>,
    /// Where a drag can land this frame, in drawing order: what a drop there asks for, or
    /// `None` for a window that takes the drop and does nothing with it. The last one drawn
    /// under the pointer wins.
    pub drops: Vec<(Rect, Option<Drop>)>,
}

/// What a drop asks for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Drop {
    /// The game's own drop: a slot, a container, the character, the shortcut bar, the world.
    Target(dereth_client_contract::view::DropTarget),
    /// Onto the vendor's sell list.
    Sell,
    /// Onto the secure trade's offer, at this position.
    Trade(u32),
    /// Onto a spell bar's slot: spells only.
    SpellSlot { tab: usize, index: usize },
    /// Onto the salvage window.
    Salvage,
    /// Onto a window that takes what is dropped on it itself.
    Window,
    /// Onto a slot of the pad's cross hotbars, which binds it there.
    CrossSlot(usize),
}

impl From<dereth_client_contract::view::DropTarget> for Drop {
    fn from(t: dereth_client_contract::view::DropTarget) -> Self {
        Self::Target(t)
    }
}

impl Drop {
    /// The request a drop of `item` here raises, if any.
    #[must_use]
    pub fn request(
        self,
        item: dereth_primitives::ObjectId,
    ) -> Option<dereth_client_contract::UiRequest> {
        use dereth_client_contract::UiRequest;
        match self {
            Self::Target(target) => Some(UiRequest::DragDrop { item, target }),
            Self::Sell => Some(UiRequest::VendorAddToSell { item }),
            Self::Trade(position) => Some(UiRequest::TradeAddItem { item, position }),
            Self::Salvage => Some(UiRequest::SalvageList(
                dereth_client_contract::panels::salvage::SalvageAction::Add(item),
            )),
            // A spell bar takes spells; an item dropped there goes nowhere.
            Self::SpellSlot { .. } | Self::Window | Self::CrossSlot(_) => None,
        }
    }
}

impl Ctx<'_> {
    /// Note that the pointer is over `r`, if it is, and say so, without offering `r` to the pad's
    /// focus: a scrollbar, a label beside its box, a close button the pad's cancel stands for.
    pub fn over_quiet(&mut self, r: &Rect) -> bool {
        let over = self.input.hover(r);
        if over {
            self.hot = true;
        }
        over
    }

    /// Offer `r` to the pad's focus without answering the pointer: a control shown disabled,
    /// which the focus can still rest on (so moving it does not jump past controls as they
    /// enable and disable) though it does nothing.
    pub fn stop(&mut self, r: &Rect) {
        let input = &mut *self.input;
        input
            .nav
            .note(*r, crate::ui::nav::Kind::Plain, &input.occluders);
    }

    /// Note that the pointer is over `r`, if it is, and say so; and offer `r` to the pad's focus.
    pub fn over(&mut self, r: &Rect) -> bool {
        let input = &mut *self.input;
        input
            .nav
            .note(*r, crate::ui::nav::Kind::Plain, &input.occluders);
        let over = self.input.hover(r);
        if over {
            self.hot = true;
        }
        over
    }
}

/// How much of the window behind the frame shows through the interface's own window fill.
const OWN_FILL_ALPHA: f32 = 0.8;

/// The scale a panel inside a window draws the own frame at, against a window's: slimmer bars
/// and smaller plates, so a panel within a window does not weigh as much as the window.
const PANEL_FRAME: f32 = 0.7;

/// A window background of the interface's own art over `r`: the fill, partly clear, under the
/// frame; with the title bar along its top when `titled`. Whether the pieces were there.
fn own_panel(p: &mut Painter<'_>, r: Rect, alpha: f32, titled: bool) -> bool {
    if !p.art.has_piece("window.tl") {
        return false;
    }
    if !titled {
        let scale = p.scale;
        p.scale *= PANEL_FRAME;
        let drawn = own_panel_at(p, r, alpha, false);
        p.scale = scale;
        return drawn;
    }
    own_panel_at(p, r, alpha, true)
}

fn own_panel_at(p: &mut Painter<'_>, r: Rect, alpha: f32, titled: bool) -> bool {
    let k = p.scale;
    let inset = p
        .art
        .piece_value(if titled {
            "window.fill_inset"
        } else {
            "window.plain_fill_inset"
        })
        .filter(|v| v.len() == 4)
        .unwrap_or_else(|| vec![0.0; 4]);
    if let Some(fill) = p.piece("window.fill") {
        let body = Rect::new(
            r.x + inset[0] * k,
            r.y + inset[1] * k,
            r.w - (inset[0] + inset[2]) * k,
            r.h - (inset[1] + inset[3]) * k,
        );
        p.tiled(
            &fill,
            body,
            k,
            crate::draw::with_alpha(WHITE, alpha * OWN_FILL_ALPHA),
        );
    }
    p.frame(
        "window",
        if titled { "" } else { "plain." },
        r,
        crate::draw::with_alpha(WHITE, alpha),
    )
}

/// How far content inside a [`panel`] keeps from its edges, in screen pixels (left, top, right,
/// bottom): clear of the own frame's corner plates when it is drawn, else a small margin.
#[must_use]
pub fn panel_inset(p: &Painter<'_>) -> (f32, f32, f32, f32) {
    let k = p.scale;
    match p.art.piece_value("window.body").filter(|v| v.len() == 4) {
        Some(v) => {
            let k = k * PANEL_FRAME;
            (v[0] * k, v[3] * k, v[2] * k, v[3] * k)
        }
        None => (10.0 * k, 8.0 * k, 10.0 * k, 8.0 * k),
    }
}

/// A plain ground over `r`, as a tooltip's: dark, a fine bronze rule round it; a panel without
/// the own art.
pub fn plain_ground(p: &mut Painter<'_>, r: Rect) {
    if p.art.has_piece("window.tl") {
        let k = p.scale;
        p.fill(r, 0xEC12_161C);
        p.outline(r, 1.0_f32.max(k), 0xC0A0_7C48);
        p.outline(r.inset(1.0_f32.max(k)), 1.0_f32.max(k), 0x6000_0000);
    } else {
        panel(p, r, 0.97);
    }
}

/// The part of a [`figure_ground`] over `r` inside its frame's bars, where a figure standing on
/// it is seen.
#[must_use]
pub fn figure_inside(p: &Painter<'_>, r: Rect) -> Rect {
    let k = p.scale;
    let inset = if p.art.has_piece("window.tl") {
        p.art
            .piece_value("window.plain_fill_inset")
            .filter(|v| v.len() == 4)
            .map_or_else(
                || vec![0.0; 4],
                |v| v.iter().map(|d| d * k * PANEL_FRAME).collect(),
            )
    } else {
        vec![1.0_f32.max(k); 4]
    };
    Rect::new(
        r.x + inset[0],
        r.y + inset[1],
        r.w - inset[0] - inset[2],
        r.h - inset[1] - inset[3],
    )
}

/// A ground for a figure to stand on over `r`: a grey that darkens downward, in a panel's own
/// frame (a fine bronze rule without the own art).
pub fn figure_ground(p: &mut Painter<'_>, r: Rect) {
    const TOP: (f32, f32, f32) = (92.0, 96.0, 102.0);
    const BOTTOM: (f32, f32, f32) = (24.0, 26.0, 30.0);
    let k = p.scale;
    let own = p.art.has_piece("window.tl");
    // Halfway under the frame's bars, so no gap shows between the grey and the frame.
    let inset = if own {
        p.art
            .piece_value("window.plain_fill_inset")
            .filter(|v| v.len() == 4)
            .map_or_else(|| vec![0.0; 4], |v| v.iter().map(|d| d * 0.5).collect())
    } else {
        vec![0.0; 4]
    };
    let f = k * PANEL_FRAME;
    let inner = Rect::new(
        r.x + inset[0] * f,
        r.y + inset[1] * f,
        r.w - (inset[0] + inset[2]) * f,
        r.h - (inset[1] + inset[3]) * f,
    );
    // Bands two pixels tall: smooth enough at any scale, few enough to cost nothing.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let bands = ((inner.h / 2.0).ceil().max(1.0) as u32).min(512);
    #[allow(clippy::cast_precision_loss)]
    let band_h = inner.h / bands as f32;
    let channel = |a: f32, b: f32, t: f32| -> u32 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let v = (a + (b - a) * t).round().clamp(0.0, 255.0) as u32;
        v
    };
    for i in 0..bands {
        #[allow(clippy::cast_precision_loss)]
        let t = (i as f32 + 0.5) / bands as f32;
        // Eased, so the light holds through the upper half where the face is.
        let t = t * t;
        let colour = 0xFF00_0000
            | (channel(TOP.0, BOTTOM.0, t) << 16)
            | (channel(TOP.1, BOTTOM.1, t) << 8)
            | channel(TOP.2, BOTTOM.2, t);
        #[allow(clippy::cast_precision_loss)]
        let y = inner.y + i as f32 * band_h;
        // The last band ends on the ground's edge; the others reach a hair under the next.
        let h = if i + 1 == bands {
            inner.bottom() - y
        } else {
            band_h + 0.5
        };
        p.fill(Rect::new(inner.x, y, inner.w, h), colour);
    }
    if own {
        let scale = p.scale;
        p.scale *= PANEL_FRAME;
        p.frame("window", "plain.", r, WHITE);
        p.scale = scale;
    } else {
        p.outline(r, 1.0_f32.max(k), 0xC0A0_7C48);
    }
}

/// A plain window background over `r`, no title.
pub fn panel(p: &mut Painter<'_>, r: Rect, alpha: f32) {
    if own_panel(p, r, alpha, false) {
        return;
    }
    p.fill(r, crate::draw::with_alpha(0xFF20_2020, alpha));
}

/// A window that moves when its header is dragged.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WindowState {
    pub open: bool,
    /// Top-left, in screen pixels; `None` until first placed.
    pub pos: Option<(f32, f32)>,
    /// Where in the header the drag took hold.
    pub grab: Option<(f32, f32)>,
    /// Seconds since it was opened, for the opening fade.
    pub opened_at: f64,
    /// A window closed by its own buttons (a notice), with no close button on its frame.
    pub no_close: bool,
}

/// What a window's frame reports.
#[derive(Debug, Clone, Copy)]
pub struct WindowOut {
    /// The space inside the frame, below the header.
    pub body: Rect,
    /// The whole window.
    pub rect: Rect,
    pub closed: bool,
}

/// The height of a window's header, in layout units.
pub const HEADER: f32 = 40.0;

/// Draw a window's frame: the background, the title in the heading font, the hairline and the
/// close button; drag it by its header. `size` is in layout units.
pub fn window(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    state: &mut WindowState,
    title: &str,
    size: (f32, f32),
    default_pos: (f32, f32),
) -> WindowOut {
    let k = p.scale;
    let (w, h) = (size.0 * k, size.1 * k);
    let (mut x, mut y) = state.pos.unwrap_or(default_pos);
    let header = Rect::new(x, y, w, HEADER * k);
    // Drag by the header.
    if let Some((gx, gy)) = state.grab {
        if ctx.input.down[0] {
            x = ctx.input.mouse.0 - gx;
            y = ctx.input.mouse.1 - gy;
            x = x.clamp(-w + 60.0, p.screen.0 - 60.0);
            y = y.clamp(0.0, p.screen.1 - 30.0);
            ctx.input.captured = true;
        } else {
            state.grab = None;
        }
    }
    // Whatever the scale, the window stays on the screen.
    if w < p.screen.0 {
        x = x.clamp(0.0, p.screen.0 - w);
    } else {
        x = 0.0;
    }
    if h < p.screen.1 {
        y = y.clamp(0.0, p.screen.1 - h);
    } else {
        y = 0.0;
    }
    state.pos = Some((x, y));
    let rect = Rect::new(x, y, w, h);
    ctx.input
        .nav
        .layer(rect, title, crate::ui::nav::PanelKind::Window);
    // A drop on a window lands in it, not on what is under it.
    ctx.drops.push((rect, None));
    let own = p.art.has_piece("window.tl");
    // The own frame's title bar: the band its title and close button are centred on, and the
    // close button's place, from the window's right edge.
    let band = p
        .art
        .piece_value("window.band")
        .filter(|v| v.len() == 2 && own)
        .map(|v| (v[0] * k, v[1] * k));
    let plate = p
        .art
        .piece_value("window.plate_tr")
        .filter(|v| v.len() == 3 && own);
    let close = match (
        band,
        p.art.piece_value("window.close").filter(|v| v.len() == 2),
    ) {
        // Over the top-right corner plate, centred on it.
        (Some(_), _) if plate.is_some() => {
            let v = plate.unwrap_or_default();
            let s = v[2] * k;
            Rect::new(
                rect.right() - v[0] * k - s / 2.0,
                y + v[1] * k - s / 2.0,
                s,
                s,
            )
        }
        (Some((b0, b1)), Some(c)) => Rect::new(
            rect.right() - c[0] * k,
            y + (b0 + b1 - c[1] * k) / 2.0,
            c[1] * k,
            c[1] * k,
        ),
        _ => Rect::new(rect.right() - 34.0 * k, y + 8.0 * k, 24.0 * k, 24.0 * k),
    };
    // The opening fade and scale-up.
    let age = (ctx.time - state.opened_at).max(0.0);
    let t = crate::ui::narrow((age / 0.12).min(1.0));
    let fade_before = p.fade;
    p.fade *= t;
    let grow = 0.95 + 0.05 * t;
    let shown = Rect::new(
        rect.x + rect.w * (1.0 - grow) / 2.0,
        rect.y + rect.h * (1.0 - grow) / 2.0,
        rect.w * grow,
        rect.h * grow,
    );
    let own = own && own_panel(p, shown, 1.0, true);
    if !own {
        panel(p, shown, 1.0);
    }
    if let (true, Some((b0, b1))) = (own, band) {
        // The own title bar carries its ornament at its ends: the title starts past the left
        // one, centred on the bar's band, a little smaller than the overlay's.
        let heading =
            TextStyle::new(Family::Heading, 20.0, ctx.colours.heading()).edge(0xFF00_0000);
        p.text_in(
            &heading,
            Rect::new(x, y + b0, w, b1 - b0),
            Align::Centre,
            title,
        );
    } else {
        let heading =
            TextStyle::new(Family::Heading, 23.0, ctx.colours.heading()).edge(0xFF00_0000);
        p.text(
            &heading,
            (x + 14.0 * k).round(),
            (y + 8.0 * k).round(),
            title,
        );
    }
    let mut closed = false;
    let own_close = if own && state.no_close {
        Some(false)
    } else if own {
        let over = ctx.over_quiet(&close);
        let state = if over && ctx.input.down[0] {
            "pressed"
        } else if over {
            "hover"
        } else {
            "normal"
        };
        p.piece(&format!("window.close.{state}")).map(|b| {
            p.sprite(&b, close, WHITE);
            over
        })
    } else {
        None
    };
    if let Some(over) = own_close {
        if over && ctx.input.clicked(&close) {
            closed = true;
        }
    }
    // The pad's cancel closes the window it has the focus in, as its close button does.
    if !state.no_close
        && ctx
            .input
            .pad
            .close
            .is_some_and(|r| (r.x - rect.x).abs() < 1.0 && (r.y - rect.y).abs() < 1.0)
    {
        ctx.input.pad.close = None;
        closed = true;
    }
    p.fade = fade_before;
    // Over the window, but not a place for the pad's focus to rest: what it holds is.
    if ctx.input.hover(&rect) {
        ctx.hot = true;
        if ctx.input.pressed[0] && header.contains(ctx.input.mouse.0, ctx.input.mouse.1) {
            state.grab = Some((ctx.input.mouse.0 - x, ctx.input.mouse.1 - y));
        }
        if ctx.input.pressed[0] || ctx.input.pressed[1] {
            ctx.input.captured = true;
        }
    }
    if closed {
        state.open = false;
    }
    // The own frame's contents keep inside its side bars and clear of its bottom corner plates.
    let inset = p
        .art
        .piece_value("window.body")
        .filter(|v| v.len() == 4 && own)
        .unwrap_or_else(|| vec![12.0, HEADER + 6.0, 12.0, 10.0]);
    WindowOut {
        body: Rect::new(
            x + inset[0] * k,
            y + inset[1] * k,
            w - (inset[0] + inset[2]) * k,
            h - (inset[1] + inset[3]) * k,
        ),
        rect,
        closed,
    }
}

/// A rounded button with a label; `true` when clicked.
pub fn button(p: &mut Painter<'_>, ctx: &mut Ctx<'_>, r: Rect, label: &str, enabled: bool) -> bool {
    button_lit(p, ctx, r, label, enabled, false)
}

/// A [`button`] that stays lit while `lit` (the one chosen of several), its label in gold.
pub fn button_lit(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    r: Rect,
    label: &str,
    enabled: bool,
    lit: bool,
) -> bool {
    let over = if enabled {
        ctx.over(&r)
    } else {
        ctx.stop(&r);
        false
    };
    let state = if !enabled {
        "disabled"
    } else if over && ctx.input.down[0] {
        "pressed"
    } else if over || lit {
        "hover"
    } else {
        "normal"
    };
    // The own button's states carry their own light.
    let own = p.halves(&format!("button.{state}"), r, WHITE);
    if !own {
        p.fill(r, 0xFF30_3030);
    }
    if over && !own {
        // The amber inner glow of a lit button.
        p.fill(r.inset(3.0 * p.scale), 0x20FF_C060);
    }
    let colour = if !enabled {
        0xFF80_8080
    } else if lit {
        0xFFFF_D878
    } else {
        0xFFEE_E1C5
    };
    let mut style = TextStyle::new(Family::Body, 14.0, colour).edge(0xFF00_0000);
    // A label wider than the button is drawn smaller until it fits.
    let room = r.w - 12.0 * p.scale;
    let wide = p.measure(&style, label);
    if wide > room && wide > 0.0 {
        style =
            TextStyle::new(Family::Body, (14.0 * room / wide).max(9.0), colour).edge(0xFF00_0000);
    }
    let pressed_offset = if over && ctx.input.down[0] { 1.0 } else { 0.0 };
    p.text_in(&style, r.offset(0.0, pressed_offset), Align::Centre, label);
    enabled && over && ctx.input.clicked(&r)
}

/// A row of tabs over `r`; `selected` follows a click. Returns whether it changed.
/// Where [`tabs`] draws each tab.
#[must_use]
pub fn tab_rects(p: &Painter<'_>, r: Rect, labels: &[&str]) -> Vec<Rect> {
    let k = p.scale;
    let style = TextStyle::new(Family::Body, 12.0, 0xFFEE_E1C5).edge(0xFF00_0000);
    let mut x = r.x;
    labels
        .iter()
        .map(|l| {
            let w = (p.measure(&style, l) + 34.0 * k).max(70.0 * k);
            let tr = Rect::new(x, r.y, w, r.h);
            x += w;
            tr
        })
        .collect()
}

/// Each tab's width in a row of tabs over `r`, `selected` the one shown: each as wide as its name
/// needs, unless they do not fit, when the selected keeps its width and the others share the rest.
fn tab_widths(p: &Painter<'_>, r: Rect, labels: &[&str], selected: usize, fill: bool) -> Vec<f32> {
    let k = p.scale;
    let style = TextStyle::new(Family::Body, 12.0, 0xFFEE_E1C5).edge(0xFF00_0000);
    #[allow(clippy::cast_precision_loss)]
    let share = r.w / labels.len().max(1) as f32;
    let mut widths: Vec<f32> = labels
        .iter()
        .map(|l| {
            if fill {
                share
            } else {
                (p.measure(&style, l) + 34.0 * k).max(70.0 * k)
            }
        })
        .collect();
    #[allow(clippy::cast_precision_loss)]
    let overlap = 4.0 * k * (labels.len().saturating_sub(1)) as f32;
    let total: f32 = widths.iter().sum::<f32>() - overlap;
    if !fill && total > r.w && labels.len() > 1 {
        let keep = widths.get(selected).copied().unwrap_or(0.0).min(r.w * 0.5);
        #[allow(clippy::cast_precision_loss)]
        let rest = ((r.w + overlap - keep) / (labels.len() - 1) as f32).max(28.0 * k);
        for (i, w) in widths.iter_mut().enumerate() {
            *w = if i == selected { keep } else { rest };
        }
    }
    widths
}

/// Where [`tabs`] draws each tab when `selected` is shown.
#[must_use]
pub fn tab_rects_for(p: &Painter<'_>, r: Rect, labels: &[&str], selected: usize) -> Vec<Rect> {
    let k = p.scale;
    let mut x = r.x;
    tab_widths(p, r, labels, selected, false)
        .into_iter()
        .map(|w| {
            let tr = Rect::new(x, r.y, w, r.h);
            x += w - 4.0 * k;
            tr
        })
        .collect()
}

pub fn tabs(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    r: Rect,
    labels: &[&str],
    selected: &mut usize,
) -> bool {
    tabs_sized(p, ctx, r, labels, selected, false)
}

/// A row of tabs sharing the whole of `r` between them.
pub fn tabs_filling(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    r: Rect,
    labels: &[&str],
    selected: &mut usize,
) -> bool {
    tabs_sized(p, ctx, r, labels, selected, true)
}

fn tabs_sized(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    r: Rect,
    labels: &[&str],
    selected: &mut usize,
    fill: bool,
) -> bool {
    if labels.is_empty() {
        return false;
    }
    let mut changed = false;
    let k = p.scale;
    {
        let input = &mut *ctx.input;
        input.nav.tabs(r, &input.occluders);
    }
    // A shoulder button of the pad steps to the tab beside the one shown.
    if let Some((bar, by)) = ctx.input.pad.tab_step {
        if (bar.x - r.x).abs() < 1.0 && (bar.y - r.y).abs() < 1.0 {
            ctx.input.pad.tab_step = None;
            let n = i32::try_from(labels.len()).unwrap_or(1);
            let at = i32::try_from(*selected).unwrap_or(0);
            *selected = usize::try_from((at + by).rem_euclid(n)).unwrap_or(0);
            changed = true;
        }
    }
    let style = TextStyle::new(Family::Body, 12.0, 0xFFEE_E1C5).edge(0xFF00_0000);
    let widths = tab_widths(p, r, labels, *selected, fill);
    let mut x = r.x;
    for (i, label) in labels.iter().enumerate() {
        let tr = Rect::new(x, r.y, widths[i], r.h);
        let on = i == *selected;
        // Not a place for the pad's focus to rest: the shoulder buttons turn the tabs.
        let over = ctx.over_quiet(&tr);
        let state = if on {
            "selected"
        } else if over {
            "hover"
        } else {
            "unselected"
        };
        // The own tab's states carry their own light.
        p.halves(&format!("tab.{state}"), tr, WHITE);
        let c = if on { 0xFFFF_FFFF } else { 0xFFA0_A0A0 };
        let shown = p.fit(&style, label, tr.w - 16.0 * k);
        p.text_in(&style.colour(c), tr, Align::Centre, &shown);
        if over && ctx.input.clicked(&tr) && !on {
            *selected = i;
            changed = true;
        }
        x += widths[i] - 4.0 * k;
    }
    changed
}

/// Where an icon sits inside a slot frame of `r`.
#[must_use]
pub fn icon_rect(r: Rect) -> Rect {
    let k = r.w / 44.0;
    Rect::new(r.x + 2.0 * k, r.y + 3.0 * k, 40.0 * k, 40.0 * k)
}

/// An icon frame with an icon in it: the hotbar and inventory slot.
pub fn icon_slot(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    r: Rect,
    icon: Option<&Sprite>,
    tint: Argb,
) -> bool {
    let over = ctx.over(&r);
    let state = if over && ctx.input.down[0] {
        "pressed"
    } else if over {
        "hover"
    } else {
        "empty"
    };
    if own_slot(p, r, icon, tint, state) {
        return over && ctx.input.clicked(&r);
    }
    if let Some(i) = icon {
        p.sprite(i, icon_rect(r), tint);
    }
    over && ctx.input.clicked(&r)
}

/// Where the own slot's frame goes for a slot over `r`: placed so its well is where the icon
/// sits.
fn own_slot_rect(p: &Painter<'_>, r: Rect, frame: &Sprite) -> Rect {
    let ir = icon_rect(r);
    let well = p
        .art
        .piece_value("slot.icon")
        .filter(|v| v.len() == 3)
        .unwrap_or_else(|| vec![4.0, 5.0, 40.0]);
    let s = ir.w / well[2].max(1.0);
    Rect::new(
        ir.x - well[0] * s,
        ir.y - well[1] * s,
        frame.w * s,
        frame.h * s,
    )
}

/// A slot of the interface's own art over `r`, in `state`: a dark recess, the icon in it, then
/// the frame over both. Whether the pieces were there.
fn own_slot(p: &mut Painter<'_>, r: Rect, icon: Option<&Sprite>, tint: Argb, state: &str) -> bool {
    let Some(frame) = p.piece(&format!("slot.{state}")) else {
        return false;
    };
    let ir = icon_rect(r);
    p.fill(ir, 0x9608_0A0C);
    if let Some(i) = icon {
        p.sprite(i, ir, tint);
    }
    let fr = own_slot_rect(p, r, &frame);
    p.sprite(&frame, fr, WHITE);
    true
}

/// The mark of the selected object's tile: the slot's highlight, lit, under a gold rim.
pub fn selected_tile(p: &mut Painter<'_>, r: Rect) {
    let k = r.w / 44.0;
    if let Some(lit) = p.piece("slot.drag") {
        let fr = own_slot_rect(p, r, &lit);
        p.sprite(&lit, fr, WHITE);
        return;
    }
    p.outline(r, 2.0 * k, SELECTED_RIM);
}

/// The colour of the rim [`selected_tile`] draws.
pub const SELECTED_RIM: Argb = 0xFFFF_D860;

/// A tooltip near the pointer: a title and lines of text.
pub fn tooltip(p: &mut Painter<'_>, ctx: &Ctx<'_>, title: &str, lines: &[String]) {
    let k = p.scale;
    let head = TextStyle::new(Family::Body, 14.0, 0xFFEE_E1C5).edge(0xFF00_0000);
    let body = TextStyle::new(Family::Body, 12.0, 0xFFFF_FFFF).edge(0xFF00_0000);
    // Long lines are broken to a readable measure, never wider than the screen allows.
    let measure = (420.0 * k).min(p.screen.0 - 44.0 * k).max(80.0 * k);
    let lines: Vec<String> = lines
        .iter()
        .flat_map(|l| p.wrap(&body, l, measure))
        .collect();
    let mut w = p.measure(&head, title).min(measure);
    for l in &lines {
        w = w.max(p.measure(&body, l));
    }
    let lh = p.line_height(&body);
    let hh = p.line_height(&head);
    #[allow(clippy::cast_precision_loss)]
    let h = hh + lh * lines.len() as f32 + 22.0 * k;
    let w = w + 28.0 * k;
    // Beside the pointer, on the other side where that runs off the screen, and held on it.
    let (sw, sh) = p.screen;
    let mut x = ctx.input.mouse.0 + 18.0 * k;
    let mut y = ctx.input.mouse.1 + 18.0 * k;
    if x + w > sw {
        x = ctx.input.mouse.0 - w - 8.0 * k;
    }
    if y + h > sh {
        y = ctx.input.mouse.1 - h - 8.0 * k;
    }
    let x = x.clamp(0.0, (sw - w).max(0.0));
    let y = y.clamp(0.0, (sh - h).max(0.0));
    let r = Rect::new(x, y, w, h);
    if p.art.has_piece("window.tl") {
        // With the own art: no frame, a dark ground with a fine bronze rule round it.
        p.fill(r, 0xEC12_161C);
        p.outline(r, 1.0_f32.max(k), 0xC0A0_7C48);
        p.outline(r.inset(1.0_f32.max(k)), 1.0_f32.max(k), 0x6000_0000);
    } else {
        panel(p, r, 0.97);
    }
    p.text(&head, x + 14.0 * k, y + 9.0 * k, title);
    let mut ly = y + 9.0 * k + hh;
    for l in &lines {
        p.text(&body, x + 14.0 * k, ly, l);
        ly += lh;
    }
}

/// A gauge over `r`, filled to `fill` in `colour`: the own gauge, else a plain track and fill.
pub fn gauge(p: &mut Painter<'_>, r: Rect, fill: f32, colour: Argb) {
    if own_gauge(p, r, fill, colour) {
        return;
    }
    p.fill(r, 0xFF20_1810);
    p.fill(Rect::new(r.x, r.y, r.w * fill.clamp(0.0, 1.0), r.h), colour);
}

/// A gauge of the interface's own art over `r`: a dark channel, the neutral fill coloured
/// `colour` to `fill` of it, then the frame over both. Whether the pieces were there.
pub fn own_gauge(p: &mut Painter<'_>, r: Rect, fill: f32, colour: Argb) -> bool {
    own_gauge_layers(p, r, &[(fill, colour)])
}

/// A gauge of the interface's own art with several fills over the one channel, each `(fill,
/// colour)` drawn over the one before (a drained chunk behind the value, a rested run ahead of
/// it). Whether the pieces were there.
pub fn own_gauge_layers(p: &mut Painter<'_>, r: Rect, layers: &[(f32, Argb)]) -> bool {
    let (Some(left), Some(strip)) = (
        p.art.piece("gauge.frame.left", 1.0),
        p.art.piece("gauge.fill", 1.0),
    ) else {
        return false;
    };
    let channel = p
        .art
        .piece_value("gauge.channel")
        .filter(|v| v.len() == 4)
        .unwrap_or_else(|| vec![14.0, 5.0, 14.0, 5.0]);
    let kk = r.h / left.h.max(1.0);
    let inner = Rect::new(
        r.x + channel[0] * kk,
        r.y + channel[1] * kk,
        r.w - (channel[0] + channel[2]) * kk,
        r.h - (channel[1] + channel[3]) * kk,
    );
    p.fill(inner, 0xAA08_0A0C);
    let fk = inner.h / strip.h.max(1.0);
    if let Some(strip) = p.art.piece("gauge.fill", fk) {
        for (fill, colour) in layers {
            let fill = fill.clamp(0.0, 1.0);
            if fill > 0.0 {
                let shown = Rect::new(inner.x, inner.y, inner.w * fill, inner.h);
                p.tiled(&strip, shown, fk, *colour);
            }
        }
    }
    p.halves("gauge.frame", r, WHITE)
}

/// A check box: a small dark box with a cream tick. The new state when it is clicked.
pub fn checkbox(p: &mut Painter<'_>, ctx: &mut Ctx<'_>, r: Rect, on: bool) -> Option<bool> {
    let k = p.scale;
    // On whole pixels, so the frame's edges each land on a full pixel.
    let side = (20.0 * k).round();
    let b = Rect::new(r.x.round(), (r.y + (r.h - side) / 2.0).round(), side, side);
    let over = ctx.over(&b);
    let frame = p
        .piece(if over {
            "control.box-hover"
        } else {
            "control.box"
        })
        .or_else(|| p.piece(if over { "slot.hover" } else { "slot.empty" }));
    if let Some(frame) = frame {
        // The own art: the slot's frame, small, its well dark, the check mark in it when on.
        p.fill(b.inset(2.0 * k), 0x9608_0A0C);
        p.sprite(&frame, b, WHITE);
        if on {
            if let Some(mark) = p.piece("control.check") {
                p.sprite(&mark, b.inset(2.0 * k), WHITE);
            }
        }
        return (over && ctx.input.clicked(&b)).then_some(!on);
    }
    p.fill(b, if over { 0xFF40_3A30 } else { 0xFF20_1C18 });
    p.fill(Rect::new(b.x, b.y, b.w, 1.0), 0xFFC8_B27A);
    if on {
        p.fill(b.inset(5.0 * k), 0xFFEE_E1C5);
    }
    (over && ctx.input.clicked(&b)).then_some(!on)
}

/// A check box that cannot be changed now, greyed: its box dim, and its tick, where it is
/// ticked, faint, since what it ticks is not drawn.
#[cfg(feature = "hifi")]
pub fn checkbox_greyed(p: &mut Painter<'_>, r: Rect, on: bool) {
    let k = p.scale;
    let side = (20.0 * k).round();
    let b = Rect::new(r.x.round(), (r.y + (r.h - side) / 2.0).round(), side, side);
    if let Some(frame) = p.piece("control.box").or_else(|| p.piece("slot.empty")) {
        p.fill(b.inset(2.0 * k), 0x9608_0A0C);
        p.sprite(&frame, b, 0x80FF_FFFF);
        if on {
            if let Some(mark) = p.piece("control.check") {
                p.sprite(&mark, b.inset(2.0 * k), 0x50FF_FFFF);
            }
        }
        return;
    }
    p.fill(b, 0xFF18_1612);
    p.fill(Rect::new(b.x, b.y, b.w, 1.0), 0xFF64_5A40);
    if on {
        p.fill(b.inset(5.0 * k), 0xFF5A_554C);
    }
}

/// A slider: a thin track, its fill, the knob and the value. The new value while it is dragged.
pub fn slider(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    r: Rect,
    value: f32,
    lo: f32,
    hi: f32,
) -> Option<f32> {
    let k = p.scale;
    let span = (hi - lo).max(f32::EPSILON);
    let t = ((value - lo) / span).clamp(0.0, 1.0);
    let track = Rect::new(r.x, r.y, r.w - 60.0 * k, r.h);
    let style = TextStyle::new(Family::Body, 12.0, ctx.colours.text()).edge(ctx.colours.edge());
    let shown = if span > 20.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    };
    p.text_in(
        &style,
        Rect::new(track.right() + 10.0 * k, r.y, 50.0 * k, r.h),
        Align::Left,
        &shown,
    );
    let t = track_slider(p, ctx, track, t)?;
    let v = lo + t * span;
    ((v - value).abs() > span / 1000.0).then_some(v)
}

/// A slider's track, its fill and its knob across `r`, at `t` from `0` (left) to `1` (right).
/// Where along it the pointer holds it, while it does.
pub fn track_slider(p: &mut Painter<'_>, ctx: &mut Ctx<'_>, r: Rect, t: f32) -> Option<f32> {
    let k = p.scale;
    let t = t.clamp(0.0, 1.0);
    if p.art.has_piece("gauge.frame.left") && p.art.has_piece("control.knob") {
        // The own art: the gauge as the track, filled to the value in bronze, the knob on it.
        let gh = (r.h * 0.8).min(20.0 * k);
        let track = Rect::new(r.x, r.y + (r.h - gh) / 2.0, r.w, gh);
        own_gauge_layers(p, track, &[(t, 0xFFC8_A060)]);
        let channel = p
            .art
            .piece_value("gauge.channel")
            .filter(|v| v.len() == 4)
            .unwrap_or_else(|| vec![14.0, 5.0, 14.0, 5.0]);
        let kk = gh / 20.0;
        let (x0, x1) = (track.x + channel[0] * kk, track.right() - channel[2] * kk);
        let ks = gh * 1.2;
        if let Some(knob) = p.piece("control.knob") {
            let kx = x0 + (x1 - x0) * t;
            p.sprite(
                &knob,
                Rect::new(kx - ks / 2.0, r.y + r.h / 2.0 - ks / 2.0, ks, ks),
                WHITE,
            );
        }
        let hit = Rect::new(track.x - 8.0 * k, r.y, track.w + 16.0 * k, r.h);
        note_slider(ctx, hit, x0, x1, t);
        if hold(ctx, hit) {
            return Some(((ctx.input.mouse.0 - x0) / (x1 - x0).max(1.0)).clamp(0.0, 1.0));
        }
        return None;
    }
    let track = Rect::new(r.x, r.y + r.h / 2.0 - 3.0 * k, r.w, 6.0 * k);
    p.fill(track, 0xFF20_1C18);
    p.fill(
        Rect::new(track.x, track.y, track.w * t, track.h),
        0xFFC8_B27A,
    );
    let knob = Rect::new(
        track.x + track.w * t - 7.0 * k,
        r.y + r.h / 2.0 - 8.0 * k,
        14.0 * k,
        16.0 * k,
    );
    p.fill(knob, 0xFFEE_E1C5);
    let hit = Rect::new(track.x - 8.0 * k, r.y, track.w + 16.0 * k, r.h);
    note_slider(ctx, hit, track.x, track.right(), t);
    if hold(ctx, hit) {
        return Some(((ctx.input.mouse.0 - track.x) / track.w).clamp(0.0, 1.0));
    }
    None
}

/// Note a slider for the pad's focus: left and right move its knob along `x0` to `x1`.
fn note_slider(ctx: &mut Ctx<'_>, hit: Rect, x0: f32, x1: f32, t: f32) {
    let input = &mut *ctx.input;
    input.nav.note(
        hit,
        crate::ui::nav::Kind::Slider { x0, x1, t },
        &input.occluders,
    );
}

/// Whether the control over `hit` holds the pointer this frame: from a press on it until the
/// left button comes up, wherever the pointer goes meanwhile.
fn hold(ctx: &mut Ctx<'_>, hit: Rect) -> bool {
    let key = (u64::from(hit.x.to_bits()) << 32) | u64::from(hit.y.to_bits() ^ hit.w.to_bits());
    if !ctx.input.down[0] && ctx.input.held == Some(key) {
        ctx.input.held = None;
    }
    let holding = ctx.input.held == Some(key) && ctx.input.down[0];
    if holding || (ctx.over(&hit) && ctx.input.pressed[0]) {
        ctx.input.held = Some(key);
        ctx.input.pressed[0] = false;
        ctx.input.captured = true;
        return true;
    }
    false
}

/// A choice stepped through with arrows either side of its name. The index chosen when an arrow
/// is clicked.
pub fn stepper(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    r: Rect,
    names: &[&str],
    at: usize,
) -> Option<usize> {
    if names.is_empty() {
        return None;
    }
    let k = p.scale;
    let left = Rect::new(r.x, r.y, 26.0 * k, r.h);
    let right = Rect::new(r.right() - 26.0 * k, r.y, 26.0 * k, r.h);
    let style = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
    p.fill(
        Rect::new(left.right(), r.y, right.x - left.right(), r.h),
        0x6020_1C18,
    );
    p.text_in(
        &style,
        Rect::new(left.right(), r.y, right.x - left.right(), r.h),
        Align::Centre,
        names[at.min(names.len() - 1)],
    );
    let mut chosen = None;
    for (b, glyph, step) in [(left, "<", names.len() - 1), (right, ">", 1)] {
        let over = ctx.over(&b);
        p.fill(b, if over { 0xFF50_4638 } else { 0xFF30_2A22 });
        p.text_in(&style, b, Align::Centre, glyph);
        if over && ctx.input.clicked(&b) {
            chosen = Some((at + step) % names.len());
        }
    }
    chosen
}

/// A dropdown box over `r`: the field with `name` in it and the down arrow at its right end, lit
/// while its list is `open`. `true` when it is clicked (to open or close its list).
pub fn dropdown_box(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    r: Rect,
    name: &str,
    open: bool,
    enabled: bool,
) -> bool {
    let k = p.scale;
    let over = if enabled {
        ctx.over(&r)
    } else {
        ctx.stop(&r);
        false
    };
    let lit = open || over;
    let own = p.halves("field", r, if lit { WHITE } else { 0xD0FF_FFFF });
    if !own {
        p.fill(r, if lit { 0xE030_2A22 } else { 0xC020_1C18 });
        p.outline(r, 1.0_f32.max(k), 0xA0C8_B27A);
    }
    // The arrow: the scrollbar's lower cap, square at the right end, else a drawn mark.
    let side = r.h;
    let arrow = Rect::new(r.right() - side, r.y, side, side);
    let tint = if !enabled {
        0x70FF_FFFF
    } else if lit {
        0xFFFF_F0D0
    } else {
        WHITE
    };
    // The down-arrow button cut whole, else the track's lower end.
    let drawn = p
        .art
        .piece("scroll.down", 1.0)
        .and_then(|probe| p.art.piece("scroll.down", side / probe.w.max(1.0)))
        .map(|piece| {
            let h = side * piece.h / piece.w.max(1.0);
            p.sprite(
                &piece,
                Rect::new(arrow.x, arrow.y + (side - h) / 2.0, side, h),
                tint,
            );
        });
    let drawn = drawn.or_else(|| {
        p.art.piece("scroll.track-bottom", 1.0).and_then(|probe| {
            let cap = p
                .art
                .piece_value("scroll.cap")
                .and_then(|v| v.first().copied())
                .unwrap_or(probe.w);
            let kk = side / probe.w.max(1.0);
            let piece = p.art.piece("scroll.track-bottom", kk)?;
            let part = piece.sub(0.0, piece.h - cap, piece.w, cap);
            p.sprite(&part, arrow, tint);
            Some(())
        })
    });
    if drawn.is_none() {
        let style = TextStyle::new(Family::Body, 11.0, 0xFFEE_E1C5).edge(0xFF00_0000);
        p.text_in(&style, arrow, Align::Centre, "\u{25BC}");
    }
    let colour = if enabled {
        ctx.colours.text()
    } else {
        0xFF80_8080
    };
    let style = TextStyle::new(Family::Body, 13.0, colour).edge(ctx.colours.edge());
    let room = Rect::new(r.x + 8.0 * k, r.y, r.w - side - 12.0 * k, r.h);
    let shown = p.fit(&style, name, room.w);
    p.text_in(&style, room, Align::Left, &shown);
    enabled && over && ctx.input.clicked(&r)
}

/// The most rows a dropdown's list shows at once; a longer list scrolls.
pub const DROPDOWN_ROWS: usize = 10;

/// Where the list of a dropdown box over `anchor` opens with `count` choices: under the box, or
/// over it when there is not room below on the screen.
#[must_use]
pub fn dropdown_list_rect(p: &Painter<'_>, anchor: Rect, count: usize) -> Rect {
    let k = p.scale;
    let row = dropdown_row_height(p, anchor);
    #[allow(clippy::cast_precision_loss)]
    let h = row * count.clamp(1, DROPDOWN_ROWS) as f32 + 6.0 * k;
    let below = anchor.bottom() + 2.0 * k;
    let y = if below + h <= p.screen.1 || anchor.y - h - 2.0 * k < 0.0 {
        below
    } else {
        anchor.y - h - 2.0 * k
    };
    Rect::new(anchor.x, y, anchor.w, h)
}

fn dropdown_row_height(p: &Painter<'_>, anchor: Rect) -> f32 {
    anchor.h.max(24.0 * p.scale)
}

/// The open list of a dropdown box over `anchor`, drawn over everything drawn before it: the
/// plain dark ground, a row for each of `names`, `at` marked; the wheel moves `top`, the first
/// row shown, through a long list. The choice clicked; a press on the list is taken, so it acts
/// on nothing under it.
pub fn dropdown_list(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    anchor: Rect,
    names: &[&str],
    at: usize,
    top: &mut usize,
) -> Option<usize> {
    let k = p.scale;
    let r = dropdown_list_rect(p, anchor, names.len());
    // The open list is a box of its own for the pad's focus, which starts on the choice shown;
    // the pad's cancel (Escape) closes it.
    ctx.input
        .nav
        .layer(r, "Dropdown", crate::ui::nav::PanelKind::Box);
    let row_h = dropdown_row_height(p, anchor);
    let shown = names.len().min(DROPDOWN_ROWS);
    let last = names.len().saturating_sub(shown);
    if ctx.input.hover(&r) && ctx.input.wheel != 0.0 {
        let step = if ctx.input.wheel > 0.0 { -1 } else { 1 };
        *top = top.saturating_add_signed(step).min(last);
        ctx.input.wheel = 0.0;
    }
    *top = (*top).min(last);
    // Opaque, so nothing of the page under it shows through.
    p.fill(r, 0xFF12_161C);
    plain_ground(p, r);
    let style = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
    let mut act = None;
    for (n, i) in (*top..*top + shown).enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let row = Rect::new(
            r.x + 3.0 * k,
            r.y + 3.0 * k + n as f32 * row_h,
            r.w - 6.0 * k,
            row_h,
        );
        let over = ctx.over(&row);
        if i == at {
            ctx.input.nav.home(row);
        }
        if i == at {
            crate::ui::pregame::list_highlight(p, row, true);
        } else if over {
            p.fill(row, 0x30C8_9646);
        }
        let label = p.fit(&style, names[i], row.w - 16.0 * k);
        p.text_in(&style, row.offset(8.0 * k, 0.0), Align::Left, &label);
        if over && ctx.input.clicked(&row) {
            act = Some(i);
        }
    }
    if names.len() > shown {
        // Where the rows shown are in the whole list: a thin bar down the right edge.
        #[allow(clippy::cast_precision_loss)]
        let (h, y) = (
            (r.h - 6.0 * k) * shown as f32 / names.len() as f32,
            (r.h - 6.0 * k) * *top as f32 / names.len() as f32,
        );
        p.fill(
            Rect::new(r.right() - 4.0 * k, r.y + 3.0 * k + y, 2.0 * k, h),
            0xC0C8_B27A,
        );
    }
    if ctx.input.hover(&r) && (ctx.input.pressed[0] || ctx.input.pressed[1]) {
        ctx.input.pressed = [false; 3];
        ctx.input.captured = true;
    }
    ctx.input.nav.end_layer();
    act
}

/// Whether the open list of a dropdown box over `anchor` with `count` choices closes this frame:
/// a press anywhere off it, taken so it acts on nothing else (a press on the box itself closes
/// it too), or Escape. Asked before anything under the list is drawn.
pub fn dropdown_dismissed(p: &Painter<'_>, ctx: &mut Ctx<'_>, anchor: Rect, count: usize) -> bool {
    let r = dropdown_list_rect(p, anchor, count);
    let pressed = ctx.input.pressed[0] || ctx.input.pressed[1];
    if pressed && !r.contains(ctx.input.mouse.0, ctx.input.mouse.1) {
        ctx.input.pressed = [false; 3];
        ctx.input.captured = true;
        return true;
    }
    ctx.input.take_key(crate::ui::input::vk::ESCAPE)
}

/// A round mark of `colour`, `radius` about `(cx, cy)`, drawn a pixel row at a time.
fn disc(p: &mut Painter<'_>, cx: f32, cy: f32, radius: f32, colour: Argb) {
    let mut y = (cy - radius).floor();
    while y < cy + radius {
        let dy = (y + 0.5 - cy).abs();
        let half = (radius * radius - dy * dy).max(0.0).sqrt();
        if half > 0.0 {
            p.fill(Rect::new(cx - half, y, half * 2.0, 1.0), colour);
        }
        y += 1.0;
    }
}

/// A radio button in `r` (its round mark centred down the left of `r`), set when `on`: a dark
/// well in a bronze ring, a cream dot in it when set. `true` when clicked.
pub fn radio(p: &mut Painter<'_>, ctx: &mut Ctx<'_>, r: Rect, on: bool) -> bool {
    let k = p.scale;
    let d = (18.0 * k).min(r.h).round();
    let b = Rect::new(r.x, (r.y + (r.h - d) / 2.0).round(), d, d);
    let over = ctx.over(&b);
    let (cx, cy, rr) = (b.x + d / 2.0, b.y + d / 2.0, d / 2.0);
    let rim = if over { 0xFFE8_C890 } else { 0xFFA0_7C48 };
    disc(p, cx, cy, rr, rim);
    disc(p, cx, cy, rr - 1.5_f32.max(1.5 * k), 0xFF08_0A0C);
    if on {
        disc(p, cx, cy, rr * 0.45, 0xFFEE_E1C5);
    }
    over && ctx.input.clicked(&b)
}

/// A row of radio buttons across `r`, one for each of `names`, each with its name beside its
/// mark; `at` is the one set, if any. The index clicked, mark or name.
pub fn radios(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    r: Rect,
    names: &[&str],
    at: Option<usize>,
) -> Option<usize> {
    if names.is_empty() {
        return None;
    }
    let k = p.scale;
    let style = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
    #[allow(clippy::cast_precision_loss)]
    let each = r.w / names.len() as f32;
    let mut chosen = None;
    for (i, name) in names.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let cell = Rect::new(r.x + i as f32 * each, r.y, each, r.h);
        if radio(p, ctx, cell, at == Some(i)) {
            chosen = Some(i);
        }
        let d = (18.0 * k).min(r.h).round();
        let label = Rect::new(cell.x + d + 6.0 * k, cell.y, cell.w - d - 6.0 * k, cell.h);
        p.text_in(&style, label, Align::Left, name);
        if ctx.over_quiet(&label) && ctx.input.clicked(&label) {
            chosen = Some(i);
        }
    }
    chosen
}

/// A check box at the left of `r` with `label` beside it, centred down `r` together. The new
/// state when the box or its label is clicked.
pub fn check_row(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    r: Rect,
    on: bool,
    style: &TextStyle,
    label: &str,
) -> Option<bool> {
    let k = p.scale;
    let changed = checkbox(p, ctx, Rect::new(r.x, r.y, 20.0 * k, r.h), on);
    let w = p.measure(style, label);
    let text = Rect::new(r.x + 28.0 * k, r.y, w.min(r.w - 28.0 * k), r.h);
    p.text_in(style, text, Align::Left, label);
    changed.or_else(|| (ctx.over_quiet(&text) && ctx.input.clicked(&text)).then_some(!on))
}

/// What a press on the scrollbar asks for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollAct {
    /// One step up (`-1`) or down (`1`): an arrow was clicked.
    Step(i32),
    /// A place along the content, 0 at the top and 1 at the end: the thumb or the track was
    /// pressed or dragged there.
    To(f32),
}

/// How wide the interface's own scrollbar is, in layout units, when its pieces are there.
#[must_use]
pub fn scrollbar_width(p: &Painter<'_>) -> Option<f32> {
    p.art.piece("scroll.track-top", 1.0).map(|s| s.w)
}

/// The interface's own scrollbar over `bar` (as tall as the area it scrolls): the track with its
/// arrow caps, and the thumb in its channel, `shown` of the content's height long and `at` of the
/// way down. `None` when its pieces are not there; else what a press on it asked for, if any.
pub fn own_scrollbar(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    bar: Rect,
    shown: f32,
    at: f32,
) -> Option<Option<ScrollAct>> {
    let top = p.art.piece("scroll.track-top", 1.0)?;
    let k = bar.w / top.w.max(1.0);
    let piece = |p: &Painter<'_>, n: &str| p.art.piece(n, k);
    let (top, mid, bottom) = (
        piece(p, "scroll.track-top")?,
        piece(p, "scroll.track-mid")?,
        piece(p, "scroll.track-bottom")?,
    );
    let (th, bh) = (top.h * k, bottom.h * k);
    p.tiled(
        &mid,
        Rect::new(bar.x, bar.y + th, bar.w, (bar.h - th - bh).max(0.0)),
        k,
        WHITE,
    );
    p.sprite(
        &top,
        Rect::new(bar.x, bar.y, bar.w, th.min(bar.h / 2.0)),
        WHITE,
    );
    p.sprite(
        &bottom,
        Rect::new(
            bar.x,
            bar.bottom() - bh.min(bar.h / 2.0),
            bar.w,
            bh.min(bar.h / 2.0),
        ),
        WHITE,
    );
    let cap = p
        .art
        .piece_value("scroll.cap")
        .and_then(|v| v.first().copied())
        .unwrap_or(top.w)
        * k;
    let side = p
        .art
        .piece_value("scroll.channel")
        .filter(|v| v.len() == 2)
        .unwrap_or_else(|| vec![top.w / 4.0, top.w / 4.0]);
    let up = Rect::new(bar.x, bar.y, bar.w, cap);
    let down = Rect::new(bar.x, bar.bottom() - cap, bar.w, cap);
    let channel = Rect::new(
        bar.x + side[0] * k,
        up.bottom(),
        bar.w - (side[0] + side[1]) * k,
        (down.y - up.bottom()).max(0.0),
    );
    // The thumb: its ends, its tile, its grip centred.
    let tk = channel.w
        / p.art
            .piece("scroll.thumb-mid", 1.0)
            .map_or(1.0, |s| s.w.max(1.0));
    let thumb_h = (channel.h * shown.clamp(0.0, 1.0))
        .max(channel.w * 3.0)
        .min(channel.h);
    let ty = channel.y + (channel.h - thumb_h) * at.clamp(0.0, 1.0);
    if let (Some(t), Some(m), Some(g), Some(b)) = (
        piece(p, "scroll.thumb-top"),
        piece(p, "scroll.thumb-mid"),
        piece(p, "scroll.thumb-grip"),
        piece(p, "scroll.thumb-bottom"),
    ) {
        let (eh, bh) = ((t.h * tk).min(thumb_h / 2.0), (b.h * tk).min(thumb_h / 2.0));
        p.tiled(
            &m,
            Rect::new(channel.x, ty + eh, channel.w, (thumb_h - eh - bh).max(0.0)),
            tk,
            WHITE,
        );
        p.sprite(
            &t.sub(0.0, 0.0, t.w, eh / tk),
            Rect::new(channel.x, ty, channel.w, eh),
            WHITE,
        );
        p.sprite(
            &b.sub(0.0, b.h - bh / tk, b.w, bh / tk),
            Rect::new(channel.x, ty + thumb_h - bh, channel.w, bh),
            WHITE,
        );
        let gh = (g.h * tk).min(thumb_h - eh - bh);
        if gh > 0.0 {
            p.sprite(
                &g,
                Rect::new(channel.x, ty + (thumb_h - gh) / 2.0, channel.w, gh),
                WHITE,
            );
        }
    }
    let mut act = None;
    // This bar, as the control that holds the pointer through a drag.
    let key = (u64::from(bar.x.to_bits()) << 32) | u64::from(bar.w.to_bits() ^ bar.h.to_bits());
    if !ctx.input.down[0] && ctx.input.held == Some(key) {
        ctx.input.held = None;
    }
    let holding = ctx.input.held == Some(key) && ctx.input.down[0];
    if holding || (ctx.over_quiet(&channel) && ctx.input.pressed[0]) {
        // Pressed on the channel, the bar keeps the pointer until the button comes up, wherever
        // the pointer goes.
        ctx.input.held = Some(key);
        ctx.input.pressed[0] = false;
        ctx.input.captured = true;
        if channel.h > thumb_h {
            let t = (ctx.input.mouse.1 - channel.y - thumb_h / 2.0) / (channel.h - thumb_h);
            act = Some(ScrollAct::To(t.clamp(0.0, 1.0)));
        }
    } else if ctx.over_quiet(&up) && ctx.input.clicked(&up) {
        act = Some(ScrollAct::Step(-1));
    } else if ctx.over_quiet(&down) && ctx.input.clicked(&down) {
        act = Some(ScrollAct::Step(1));
    }
    Some(act)
}

/// A scrolling area: the wheel over `area` moves `offset` through `content` pixels of height, and
/// a bar at the right shows where it is (the interface's own scrollbar when its pieces are there,
/// its arrows stepping and its thumb dragged). Returns the offset to draw at.
pub fn scroll(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    area: Rect,
    content: f32,
    offset: &mut f32,
) -> f32 {
    let k = p.scale;
    let max = (content - area.h).max(0.0);
    if ctx.input.hover(&area) && ctx.input.wheel != 0.0 {
        *offset -= ctx.input.wheel * 48.0 * k;
        ctx.input.wheel = 0.0;
    }
    // The pad's focus moving through the list scrolls it just as far as the focus needs.
    if let Some((r, by)) = ctx.input.pad.scroll_list {
        if (r.x - area.x).abs() < 1.0 && (r.y - area.y).abs() < 1.0 && (r.w - area.w).abs() < 1.0 {
            *offset += by;
            ctx.input.pad.scroll_list = None;
        }
    }
    *offset = offset.clamp(0.0, max);
    if max > 0.0 {
        // For the pad, the rows are stops, and the list scrolls with them.
        let input = &mut *ctx.input;
        input
            .nav
            .list(area, *offset > 0.0, *offset < max, &input.occluders);
        if let Some(w) = scrollbar_width(p) {
            let bar = Rect::new(area.right() - w * k, area.y, w * k, area.h);
            let shown = area.h / content;
            if let Some(act) = own_scrollbar(p, ctx, bar, shown, *offset / max).flatten() {
                *offset = match act {
                    ScrollAct::Step(n) => *offset + n as f32 * 48.0 * k,
                    ScrollAct::To(t) => t * max,
                }
                .clamp(0.0, max);
            }
            return *offset;
        }
        let track = Rect::new(area.right() - 4.0 * k, area.y, 3.0 * k, area.h);
        p.fill(track, 0x4000_0000);
        let h = (area.h * area.h / content).max(16.0 * k);
        let y = area.y + (area.h - h) * (*offset / max);
        p.fill(Rect::new(track.x, y, track.w, h), 0xC0C8_B27A);
    }
    *offset
}

/// A step button: the scrollbar's arrow cap (up, or down when `up` is false) with `label`
/// beside it, over `r`; dimmed when it cannot be pressed. `true` when clicked. Without the own
/// art, a plain button reading the arrow and the label.
pub fn step_button(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    r: Rect,
    up: bool,
    label: &str,
    enabled: bool,
) -> bool {
    let k = p.scale;
    let cap_piece = if up {
        "scroll.track-top"
    } else {
        "scroll.track-bottom"
    };
    let Some(probe) = p.art.piece(cap_piece, 1.0) else {
        let glyph = if up { "\u{25B2}" } else { "\u{25BC}" };
        return button(p, ctx, r, &format!("{glyph} {label}"), enabled);
    };
    let cap = p
        .art
        .piece_value("scroll.cap")
        .and_then(|v| v.first().copied())
        .unwrap_or(probe.w);
    let side = r.h;
    let kk = side / probe.w.max(1.0);
    let over = if enabled {
        ctx.over(&r)
    } else {
        ctx.stop(&r);
        false
    };
    let tint = if !enabled {
        0x70FF_FFFF
    } else if over && ctx.input.down[0] {
        0xFFB0_B0B0
    } else if over {
        0xFFFF_F0D0
    } else {
        WHITE
    };
    let button = if up { "scroll.up" } else { "scroll.down" };
    if let Some(piece) = p.art.piece(button, kk) {
        // The arrow button cut out on its own, centred in the square, keeping its shape.
        let h = side * piece.h / piece.w.max(1.0);
        p.sprite(
            &piece,
            Rect::new(r.x, r.y + (side - h) / 2.0, side, h),
            tint,
        );
    } else if let Some(piece) = p.art.piece(cap_piece, kk) {
        // The cap is the square end of the track: its top for up, its bottom for down.
        let part = if up {
            piece.sub(0.0, 0.0, piece.w, cap)
        } else {
            piece.sub(0.0, piece.h - cap, piece.w, cap)
        };
        p.sprite(&part, Rect::new(r.x, r.y, side, side), tint);
    }
    let colour = if enabled { 0xFFEE_E1C5 } else { 0xFF80_8080 };
    let style = TextStyle::new(Family::Numerals, 13.0, colour).edge(0xFF00_0000);
    p.text_in(
        &style,
        Rect::new(r.x + side + 4.0 * k, r.y, r.w - side - 4.0 * k, r.h),
        Align::Left,
        label,
    );
    enabled && over && ctx.input.clicked(&r)
}

/// A text caret after text of `style` whose line starts at `(x, top)`: as tall as the face's
/// letters, not its whole line, so it sits on the text.
pub fn caret(p: &mut Painter<'_>, style: &TextStyle, x: f32, top: f32) {
    let lh = p.line_height(style);
    let w = (1.2 * p.scale).max(1.0);
    p.fill(Rect::new(x, top + lh * 0.2, w, lh * 0.62), style.colour);
}

/// A one-line text box, focused while `focus` holds its `id`: a click in it focuses it and a
/// click elsewhere lets it go; typing and the editing keys edit it ([`crate::ui::edit`]), and
/// Escape lets it go. `true` when Enter is pressed in it.
pub fn text_box(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    r: Rect,
    text: &mut String,
    focus: &mut u32,
    id: u32,
) -> bool {
    text_box_aligned(p, ctx, r, text, focus, id, Align::Left)
}

/// [`text_box`] with its text placed by `align`: against its right end for [`Align::Right`]
/// (a number), while it fits; a line wider than the box scrolls as a left-aligned one does.
pub fn text_box_aligned(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    r: Rect,
    text: &mut String,
    focus: &mut u32,
    id: u32,
    align: Align,
) -> bool {
    use crate::ui::edit;
    use crate::ui::input::vk;
    let k = p.scale;
    let mut pressed_in = false;
    {
        let input = &mut *ctx.input;
        input
            .nav
            .note(r, crate::ui::nav::Kind::Text, &input.occluders);
    }
    if ctx.input.pressed[0] {
        if ctx.input.hover(&r) {
            *focus = id;
            pressed_in = true;
            ctx.input.pressed[0] = false;
            ctx.input.captured = true;
        } else if *focus == id {
            *focus = 0;
        }
    }
    ctx.over(&r);
    let focused = *focus == id;
    if !p.art.has_piece("field.left") {
        p.fill(r, if focused { 0xE020_1C14 } else { 0x9010_0C08 });
        p.fill(
            Rect::new(r.x, r.bottom() - 1.0 * k, r.w, 1.0 * k),
            if focused { 0xFFC8_B27A } else { 0x60C8_B27A },
        );
    }
    let style = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
    // The own field under the text: its rim lit while it has the keyboard.
    let own_pad = if p.halves("field", r, if focused { WHITE } else { 0xD0FF_FFFF }) {
        p.art
            .piece("field.left", 1.0)
            .map_or(6.0 * k, |l| l.w * r.h / l.h.max(1.0) + 4.0 * k)
    } else {
        6.0 * k
    };
    let mut submitted = false;
    let inner = Rect::new(r.x + own_pad, r.y, (r.w - 2.0 * own_pad).max(0.0), r.h);
    // Where the text runs: the whole box, or for text placed against the right or the middle
    // and narrower than the box, just the text's own width there.
    let placed = |p: &Painter<'_>, text: &str| -> Rect {
        let w = p.measure(&style, text);
        if w + 3.0 * k >= inner.w {
            return inner;
        }
        match align {
            Align::Left => inner,
            // Room kept at the end for the caret after the last letter.
            Align::Right => Rect::new(inner.right() - w - 3.0 * k, inner.y, w, inner.h),
            Align::Centre => Rect::new(inner.x + (inner.w - w) / 2.0, inner.y, w, inner.h),
        }
    };
    if !focused {
        let area = placed(p, text);
        p.text_in(&style, area, Align::Left, text);
        return false;
    }
    ctx.input.text_focus = true;
    let area = placed(p, text);
    edit::begin(ctx.input, edit::line_key(r, id), text);
    let double = pressed_in && ctx.input.double;
    edit::pointer(p, ctx.input, &style, text, area, pressed_in, double);
    edit::keys(ctx.input, text, 64);
    if ctx.input.take_key(vk::ENTER) {
        submitted = true;
    }
    if ctx.input.take_key(vk::ESCAPE) {
        *focus = 0;
    }
    let top = p.text_top(&style, r.y, r.h);
    let caret_top = (r.y + (r.h - p.line_height(&style)) / 2.0).round();
    let blink = *focus == id && (ctx.time * 2.0).fract() < 0.5;
    let area = placed(p, text);
    edit::draw(
        p,
        &mut ctx.input.edit,
        &style,
        text,
        area,
        top,
        caret_top,
        blink,
    );
    submitted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art::Art;
    use crate::draw::DrawList;
    use crate::ui::input::{vk, InputFrame};

    /// One frame of a text box at (10, 10, 200, 24) with no art behind it.
    fn frame(input: &mut InputFrame, text: &mut String, focus: &mut u32) -> bool {
        frame_aligned(input, text, focus, Align::Left).0
    }

    /// One frame of a text box placing its text by `align`, and the flat quads it drew.
    fn frame_aligned(
        input: &mut InputFrame,
        text: &mut String,
        focus: &mut u32,
        align: Align,
    ) -> (bool, Vec<Rect>) {
        let art = Art::empty();
        let colours = crate::ui::colours::Colours::default();
        let mut list = DrawList::default();
        let mut p = Painter {
            list: &mut list,
            art: &art,
            scale: 1.0,
            screen: (800.0, 600.0),
            fade: 1.0,
        };
        let mut drag = None;
        let mut ctx = Ctx {
            time: 0.0,
            dt: 1.0 / 60.0,
            input,
            colours: &colours,
            hot: false,
            drag: &mut drag,
            drops: Vec::new(),
        };
        let submitted = text_box_aligned(
            &mut p,
            &mut ctx,
            Rect::new(10.0, 10.0, 200.0, 24.0),
            text,
            focus,
            7,
            align,
        );
        input.next_frame();
        let flat = list
            .quads
            .iter()
            .filter(|q| q.tex.is_none())
            .map(|q| q.dst)
            .collect();
        (submitted, flat)
    }

    /// Behaviour: none (this client's own text box)
    #[test]
    fn a_right_aligned_text_box_keeps_its_caret_at_the_right_end_and_edits_at_it() {
        let mut input = InputFrame::default();
        let (mut text, mut focus) = (String::from("12"), 0);
        input.mouse = (100.0, 20.0);
        input.pressed[0] = true;
        frame_aligned(&mut input, &mut text, &mut focus, Align::Right);
        let (_, flat) = frame_aligned(&mut input, &mut text, &mut focus, Align::Right);
        // The caret: a thin upright bar, at the box's right end, inside its padding.
        let caret = flat
            .iter()
            .find(|r| r.w <= 2.0 && r.h > 4.0)
            .expect("the caret is drawn");
        assert!(caret.x > 190.0 && caret.x < 210.0, "{caret:?}");
        input.keys = vec![vk::HOME];
        frame_aligned(&mut input, &mut text, &mut focus, Align::Right);
        input.chars = vec!['3'];
        frame_aligned(&mut input, &mut text, &mut focus, Align::Right);
        assert_eq!(text, "312");
        // Left-aligned, the same caret sits at the left end.
        let mut text = String::new();
        let mut focus = 7;
        let (_, flat) = frame_aligned(&mut input, &mut text, &mut focus, Align::Left);
        let caret = flat.iter().find(|r| r.w <= 2.0 && r.h > 4.0).unwrap();
        assert!(caret.x < 30.0, "{caret:?}");
    }

    /// Behaviour: none (this client's own text box)
    #[test]
    fn a_text_box_takes_the_keyboard_on_a_click_and_answers_enter() {
        let mut input = InputFrame::default();
        let (mut text, mut focus) = (String::new(), 0);
        input.chars = vec!['x'];
        frame(&mut input, &mut text, &mut focus);
        assert!(
            text.is_empty(),
            "typing before it has the keyboard goes elsewhere"
        );
        input.mouse = (20.0, 20.0);
        input.pressed[0] = true;
        frame(&mut input, &mut text, &mut focus);
        assert_eq!(focus, 7);
        input.chars = "holt".chars().collect();
        frame(&mut input, &mut text, &mut focus);
        input.keys = vec![vk::BACK];
        frame(&mut input, &mut text, &mut focus);
        assert_eq!(text, "hol");
        input.keys = vec![vk::ENTER];
        assert!(frame(&mut input, &mut text, &mut focus));
        input.keys = vec![vk::ESCAPE];
        frame(&mut input, &mut text, &mut focus);
        assert_eq!(focus, 0, "Escape lets it go");
    }

    /// Behaviour: none (this client's own text box)
    #[test]
    fn a_drag_in_a_text_box_selects_and_typing_replaces_the_selection() {
        let mut input = InputFrame::default();
        let (mut text, mut focus) = (String::from("Holtburg"), 0);
        input.mouse = (100.0, 20.0);
        input.down[0] = true;
        input.pressed[0] = true;
        frame(&mut input, &mut text, &mut focus);
        assert_eq!(focus, 7);
        // Dragged out past the box's left end: the selection runs to the start.
        input.mouse = (0.0, 20.0);
        frame(&mut input, &mut text, &mut focus);
        input.down[0] = false;
        frame(&mut input, &mut text, &mut focus);
        assert_eq!(input.edit.selection(), 0..8);
        input.chars = vec!['Y'];
        frame(&mut input, &mut text, &mut focus);
        assert_eq!(text, "Y");
    }

    /// One frame of a slider over (100, 100, 200, 24) at `value` in 0..1, with no art behind it.
    fn slider_frame(input: &mut InputFrame, value: f32) -> Option<f32> {
        let art = Art::empty();
        let colours = crate::ui::colours::Colours::default();
        let mut list = DrawList::default();
        let mut p = Painter {
            list: &mut list,
            art: &art,
            scale: 1.0,
            screen: (800.0, 600.0),
            fade: 1.0,
        };
        let mut drag = None;
        let mut ctx = Ctx {
            time: 0.0,
            dt: 1.0 / 60.0,
            input,
            colours: &colours,
            hot: false,
            drag: &mut drag,
            drops: Vec::new(),
        };
        let r = track_slider(
            &mut p,
            &mut ctx,
            Rect::new(100.0, 100.0, 200.0, 24.0),
            value,
        );
        input.next_frame();
        r
    }

    /// Behaviour: none (this client's own slider)
    #[test]
    fn a_slider_keeps_the_pointer_from_its_press_until_the_button_comes_up() {
        let mut input = InputFrame {
            mouse: (150.0, 112.0),
            down: [true, false, false],
            pressed: [true, false, false],
            ..InputFrame::default()
        };
        let t = slider_frame(&mut input, 0.5).expect("a press on it takes it");
        assert!((t - 0.25).abs() < 0.01, "{t}");
        // Dragged well off the slider, below and past its right end: it still follows.
        input.mouse = (400.0, 300.0);
        assert_eq!(slider_frame(&mut input, t), Some(1.0));
        input.mouse = (100.0, 40.0);
        assert_eq!(slider_frame(&mut input, 1.0), Some(0.0));
        // The button up lets it go.
        input.down[0] = false;
        assert_eq!(slider_frame(&mut input, 0.0), None);
        // Pressed elsewhere and dragged over it, it does not take hold.
        input.mouse = (150.0, 300.0);
        input.down[0] = true;
        input.pressed[0] = true;
        slider_frame(&mut input, 0.0);
        input.mouse = (150.0, 112.0);
        assert_eq!(slider_frame(&mut input, 0.0), None);
    }
}
