//! The plugins' windows and HUDs, drawn in Horizon's art from the trees the plugins declare
//! ([`auberean_shell::plugins::PluginWindow`]), laid out by the rules every host follows
//! ([`auberean_client_sdk::plugin_api::layout`]).
//!
//! - **The plugin bar** at the top left: a button for each window a plugin declares, lit while it
//!   is open; a press opens it or closes it.
//! - **A window** open is one of Horizon's windows, framed, with its title and close box, under
//!   Horizon's own windows and over the HUD; its close box closes it.
//! - **A HUD** open stands at its anchor on the screen, with no frame.
//!
//! A press on a button or a toggle is the plugin's ([`Request::PluginClick`]); opening or closing
//! a window is asked of the plugin host ([`Request::PluginWindow`]). The other kinds of node show
//! their text.

use auberean_client_sdk::plugin_api::layout::{self, Laid, Measure, Metrics};
use auberean_client_sdk::plugin_api::{NodeKind, UiNode, Value};
use auberean_shell::interface::Request;
use auberean_shell::plugins::PluginWindow;
use dereth_ui_kit::art::Family;
use dereth_ui_kit::paint::{Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx, WindowState};

use crate::draw::Rect;

/// The text the nodes are written in, and its size in points.
const TEXT_POINTS: f32 = 13.0;

/// The plugin bar's buttons, in layout units.
const BAR_BUTTON: (f32, f32) = (110.0, 24.0);
const BAR_AT: (f32, f32) = (12.0, 12.0);

/// The plugins' share of the interface.
#[derive(Debug, Default)]
pub struct PluginSeat {
    /// The plugins' windows and HUDs this frame, as the shell hands them over.
    pub windows: Vec<PluginWindow>,
    /// Each window's frame, by its plugin and its id.
    frames: Vec<((String, String), WindowState)>,
    /// Where the windows were drawn last frame.
    drawn: Vec<Rect>,
    /// Where each node with an id was drawn last frame, by its plugin, window and id.
    placed: Vec<((String, String, String), Rect)>,
}

/// Text measured in the windows' text, in layout units; a character's cell where the art has no
/// font.
struct KitMeasure<'a, 'p> {
    p: &'a Painter<'p>,
    style: TextStyle,
}

/// A character's width, in layout units, where the art has no font.
const CELL: f32 = 7.0;

impl Measure for KitMeasure<'_, '_> {
    fn text(&self, text: &str) -> f32 {
        let w = self.p.measure(&self.style, text) / self.p.scale.max(0.01);
        #[allow(clippy::cast_precision_loss)]
        let cells = text.chars().count() as f32 * CELL;
        if w > 0.0 {
            w
        } else {
            cells
        }
    }
}

/// The measures the windows are laid out by.
fn metrics() -> Metrics {
    Metrics::of(CELL, 16.0, 14.0, 12.0)
}

/// What `tree` needs and asks for, its text measured by `p`.
fn want(p: &Painter<'_>, tree: &UiNode, m: &Metrics) -> layout::Want {
    let measure = KitMeasure {
        p,
        style: TextStyle::new(Family::Body, TEXT_POINTS, 0xFFFF_FFFF),
    };
    layout::want(tree, m, &measure)
}

/// A window's title: its `title`, else its id.
fn title(w: &PluginWindow) -> String {
    let t = w.tree.text("title");
    if t.is_empty() {
        w.window.clone()
    } else {
        t.to_owned()
    }
}

impl PluginSeat {
    /// Where the plugin windows were drawn last frame: what lies under them does not answer the
    /// pointer.
    #[must_use]
    pub fn rects(&self) -> &[Rect] {
        &self.drawn
    }

    /// Where node `node` of `plugin`'s window `window` was drawn last frame.
    #[must_use]
    pub fn node_rect(&self, plugin: &str, window: &str, node: &str) -> Option<Rect> {
        self.placed
            .iter()
            .find(|((p, w, n), _)| p == plugin && w == window && n == node)
            .map(|(_, r)| *r)
    }

    /// Where the plugin bar's button for the `n`th window a plugin declares is.
    #[must_use]
    pub fn bar_button(k: f32, n: usize) -> Rect {
        #[allow(clippy::cast_precision_loss)]
        let n = n as f32;
        Rect::new(
            (BAR_AT.0 + n * (BAR_BUTTON.0 + 4.0)) * k,
            BAR_AT.1 * k,
            BAR_BUTTON.0 * k,
            BAR_BUTTON.1 * k,
        )
    }

    /// The bar, the windows open and the HUDs, asking for what the player does on them; where
    /// they are.
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        requests: &mut Vec<Request>,
    ) -> Vec<Rect> {
        let k = p.scale;
        let mut covers = Vec::new();
        self.drawn.clear();
        self.placed.clear();
        let windows = std::mem::take(&mut self.windows);
        let declared = windows.iter().filter(|w| w.tree.kind == NodeKind::Window);
        for (n, w) in declared.enumerate() {
            let r = Self::bar_button(k, n);
            if kit::button_lit(p, ctx, r, &title(w), true, w.open) {
                requests.push(Request::PluginWindow {
                    plugin: w.plugin.clone(),
                    window: w.window.clone(),
                    open: !w.open,
                });
            }
            covers.push(r);
        }
        for w in windows.iter().filter(|w| w.open) {
            let r = match w.tree.kind {
                NodeKind::Window => Some(self.window(p, ctx, w, requests)),
                NodeKind::Hud => Some(self.hud(p, ctx, w, requests)),
                _ => None,
            };
            covers.extend(r);
        }
        self.frames.retain(|(key, _)| {
            windows
                .iter()
                .any(|w| w.plugin == key.0 && w.window == key.1)
        });
        self.windows = windows;
        covers
    }

    /// A window, framed; where it is.
    fn window(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        w: &PluginWindow,
        requests: &mut Vec<Request>,
    ) -> Rect {
        let k = p.scale;
        let m = metrics();
        let want = want(p, &w.tree, &m);
        let body = layout::natural_body(&want);
        let key = (w.plugin.clone(), w.window.clone());
        let at = match self.frames.iter().position(|(k2, _)| *k2 == key) {
            Some(i) => i,
            None => {
                self.frames.push((key, WindowState::default()));
                self.frames.len() - 1
            }
        };
        let size = kit::window_size_for(p, body);
        let place = (
            ((p.screen.0 - size.0 * k) / 2.0).round(),
            ((p.screen.1 - size.1 * k) / 3.0).round(),
        );
        let state = &mut self.frames[at].1;
        state.open = true;
        let out = kit::window(p, ctx, state, &title(w), size, place);
        if out.closed {
            requests.push(Request::PluginWindow {
                plugin: w.plugin.clone(),
                window: w.window.clone(),
                open: false,
            });
        }
        let laid = layout::place_body(&w.tree, &want, &m, body);
        let mut drawing = Drawing {
            w,
            requests,
            placed: &mut self.placed,
        };
        drawing.nodes(
            p,
            ctx,
            &w.tree.children,
            &laid.children,
            (out.body.x, out.body.y),
        );
        self.drawn.push(out.rect);
        out.rect
    }

    /// A HUD, at its anchor with no frame; where it is.
    fn hud(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        w: &PluginWindow,
        requests: &mut Vec<Request>,
    ) -> Rect {
        let k = p.scale;
        let m = metrics();
        let want = want(p, &w.tree, &m);
        let body = layout::natural_body(&want);
        let (bw, bh) = (body.0 * k, body.1 * k);
        let margin = 12.0 * k;
        let (sw, sh) = p.screen;
        let anchor = w.tree.text("anchor");
        let x = if anchor.ends_with("left") {
            margin
        } else if anchor.ends_with("right") {
            sw - bw - margin
        } else {
            ((sw - bw) / 2.0).round()
        };
        let y = if anchor.starts_with("top") {
            margin + 48.0 * k
        } else if anchor.starts_with("bottom") {
            sh - bh - 160.0 * k
        } else {
            ((sh - bh) / 2.0).round()
        };
        let laid = layout::place_body(&w.tree, &want, &m, body);
        let mut drawing = Drawing {
            w,
            requests,
            placed: &mut self.placed,
        };
        drawing.nodes(p, ctx, &w.tree.children, &laid.children, (x, y));
        Rect::new(x, y, bw, bh)
    }
}

/// One window's nodes being drawn: what the presses on them ask, and where each was put.
struct Drawing<'a> {
    w: &'a PluginWindow,
    requests: &'a mut Vec<Request>,
    placed: &'a mut Vec<((String, String, String), Rect)>,
}

impl Drawing<'_> {
    /// A press on node `n`, for its plugin.
    fn click(&mut self, n: &UiNode) {
        if let Some(id) = &n.id {
            self.requests.push(Request::PluginClick {
                plugin: self.w.plugin.clone(),
                window: self.w.window.clone(),
                node: id.clone(),
            });
        }
    }

    /// `tree`'s nodes drawn as `laid` places them, from `origin` (screen pixels) at the painter's
    /// scale.
    fn nodes(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        tree: &[UiNode],
        laid: &[Laid],
        origin: (f32, f32),
    ) {
        let k = p.scale;
        let style =
            TextStyle::new(Family::Body, TEXT_POINTS, ctx.colours.text()).edge(ctx.colours.edge());
        let line = 1.0_f32.max(k.floor());
        for (n, l) in tree.iter().zip(laid) {
            let r = Rect::new(origin.0 + l.x * k, origin.1 + l.y * k, l.w * k, l.h * k);
            if let Some(id) = &n.id {
                self.placed.push((
                    (self.w.plugin.clone(), self.w.window.clone(), id.clone()),
                    r,
                ));
            }
            match n.kind {
                NodeKind::Row | NodeKind::Column | NodeKind::Tabs => {
                    self.nodes(p, ctx, &n.children, &l.children, (r.x, r.y));
                }
                NodeKind::Group => {
                    p.outline(r, line, 0x80C0_B090);
                    let t = n.text("title");
                    if !t.is_empty() {
                        p.text(&style, r.x + 6.0 * k, r.y + 2.0 * k, t);
                    }
                    self.nodes(p, ctx, &n.children, &l.children, (r.x, r.y));
                }
                NodeKind::Separator => {
                    let y = (r.y + r.h / 2.0).round();
                    p.fill(Rect::new(r.x, y, r.w, line), 0x80C0_B090);
                }
                NodeKind::Button => {
                    let enabled = !matches!(n.props.get("enabled"), Some(Value::Bool(false)));
                    if kit::button(p, ctx, r, n.text("text"), enabled) {
                        self.click(n);
                    }
                }
                NodeKind::Toggle => {
                    let on = matches!(n.props.get("value"), Some(Value::Bool(true)));
                    if kit::check_row(p, ctx, r, on, &style, n.text("text")).is_some() {
                        self.click(n);
                    }
                }
                NodeKind::Meter => {
                    let number = |name: &str| match n.props.get(name) {
                        #[allow(clippy::cast_precision_loss)]
                        Some(Value::Int(i)) => *i as f32,
                        #[allow(clippy::cast_possible_truncation)]
                        Some(Value::Num(f)) => *f as f32,
                        _ => 0.0,
                    };
                    let (value, max) = (number("value"), number("max"));
                    let share = if max > 0.0 {
                        (value / max).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    p.fill(r, 0xA018_1C22);
                    p.fill(Rect::new(r.x, r.y, (r.w * share).round(), r.h), 0xFFC0_A060);
                    p.text(&style, r.x + 4.0 * k, r.y + 1.0 * k, n.text("text"));
                }
                _ => {
                    let t = n.text("text");
                    let t = if t.is_empty() { n.text("value") } else { t };
                    p.text(&style, r.x, r.y, t);
                }
            }
        }
    }
}
