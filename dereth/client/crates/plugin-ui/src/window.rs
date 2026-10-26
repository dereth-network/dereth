//! Plugin windows drawn with the widget kit: each declared node by the kit's widget for it, laid
//! out by the plugin interface's own rules ([`dereth_plugin_api::layout`]) in the painter's font,
//! in whatever skin the painter's art is.
//!
//! [`PluginUi`] keeps what the declared tree does not: where each window is, how far each list and
//! text box is scrolled, what the player is typing, which choice list is open, the pictures made
//! for each window's images. It is drawn every frame. An interface with windows of its own seats
//! each plugin window among them and calls [`PluginUi::body`] inside the frame it draws; one with
//! none draws the whole window with [`PluginUi::window`]. Either way the controls answer the pointer
//! and note themselves for a pad's focus as the kit's widgets do, so a pad moves over a plugin
//! window as over any other.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use dereth_plugin_api::layout::{self, Laid, Measure, Metrics, Want};
use dereth_plugin_api::{NodeKind, UiNode, Value};
use dereth_ui_kit::art::{Family, TexId};
use dereth_ui_kit::draw::{Argb, Rect};
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx, WindowState};

use crate::{Input, WindowView};

/// How long the pointer rests on a control before its tooltip shows, in seconds.
const TOOLTIP_DELAY: f64 = 0.6;

/// The most characters a line or a box of lines takes.
const MAX_TEXT: usize = 4_000;

/// A HUD's margin from the screen's edge, in layout units.
const HUD_MARGIN: f32 = 12.0;

/// A HUD's plate.
const HUD_PLATE: Argb = 0x9E10_0C08;

/// The least width a window asks for, in cells.
const NATURAL_CELLS: f32 = 24.0;

/// The side of a window's bottom right corner a press takes hold of to resize it, in layout
/// units: the grip, part of the window's frame.
const GRIP: f32 = 12.0;

/// How far in from a window's left, right and bottom edges a press takes hold of the edge to
/// resize it, in layout units.
const EDGE: f32 = 6.0;

/// How a window takes the player's size. Either way a window the player sizes has its nodes laid
/// out again at that size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sizing {
    /// Declared `resizable = true`: its plugin hears the size in cells and lines, and keeps it.
    Plugin,
    /// Declared neither way: the host keeps the size the player gave it, and its plugin hears
    /// nothing.
    Player,
    /// Declared `resizable = false`, and every HUD: the size its nodes ask for, always.
    Fixed,
}

/// Where a HUD stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudMode {
    /// At the place its anchor names, for an interface that moves it among its own (Horizon).
    Anchored,
    /// Where the player put it, else at its anchor; while `unlocked`, a press on it moves it.
    Placed { unlocked: bool },
}

/// How window `view` takes the player's size.
#[must_use]
pub fn sizing(view: &WindowView) -> Sizing {
    if view.tree.kind != NodeKind::Window {
        return Sizing::Fixed;
    }
    match view.tree.props.get("resizable").and_then(Value::as_bool) {
        Some(true) => Sizing::Plugin,
        Some(false) => Sizing::Fixed,
        None => Sizing::Player,
    }
}

/// An edge or corner of a window held by the player: which edges move, where the pointer took
/// hold, and where the window stood then (screen pixels).
#[derive(Debug, Clone, Copy)]
struct Resizing {
    left: bool,
    right: bool,
    bottom: bool,
    from: (f32, f32),
    rect: Rect,
}

/// A choice's list, open: the window and node it belongs to, where its box is, its options, the
/// one chosen and the first one shown.
#[derive(Debug, Clone)]
struct OpenChoice {
    key: (String, String),
    node: String,
    id: u32,
    anchor: Rect,
    options: Vec<String>,
    at: usize,
    top: usize,
}

/// What a window's tree needs and asks for, and what it was measured for.
#[derive(Debug, Clone)]
struct WantFor {
    revision: u64,
    metrics: Metrics,
    want: Arc<Want>,
}

/// A window's tree laid out in its body, and the body's size it was laid out in.
#[derive(Debug, Clone)]
struct LaidFor {
    revision: u64,
    metrics: Metrics,
    size: (f32, f32),
    laid: Arc<Laid>,
}

/// The painter's measures of text, in layout units: the windows' text, and a button's label in
/// the face the kit draws it in.
struct PainterMeasure<'a, 'p> {
    p: &'a Painter<'p>,
    style: TextStyle,
}

impl Measure for PainterMeasure<'_, '_> {
    fn text(&self, text: &str) -> f32 {
        self.p.measure(&self.style, text) / self.p.scale.max(0.01)
    }

    fn label(&self, text: &str) -> f32 {
        kit::button_label_width(self.p, text) / self.p.scale.max(0.01)
    }
}

/// What is kept for one window beside its declared tree.
#[derive(Debug, Default)]
struct Win {
    want: Option<WantFor>,
    laid: Option<LaidFor>,
    /// How far its body is scrolled, where it is shorter than its nodes.
    body_scroll: f32,
    frame: WindowState,
    /// How far each list and text box is scrolled, by node.
    scroll: HashMap<u32, f32>,
    /// How many rows each list had when it was drawn last, by node.
    rows: HashMap<u32, usize>,
    /// What the player is typing in each line and box, by node, while it has the keyboard.
    drafts: HashMap<u32, String>,
    /// Each image's picture, by its `src`: the hash its file was given as it arrived, and the
    /// picture made of it.
    pictures: HashMap<String, (u64, Option<TexId>)>,
    /// Each canvas's and chart's drawing as rectangles and placed text, by node: the window's
    /// revision it was last seen at, the hash of what it draws, and the drawing.
    drawings: HashMap<u32, (u64, u64, Arc<Drawing>)>,
    /// Where it was drawn last, in screen pixels.
    rect: Rect,
    /// An edge or corner the player holds.
    resizing: Option<Resizing>,
    /// A window its plugin declares resizable: the body the player gave it (layout units), until
    /// the plugin's own size for it arrives.
    live: Option<(f32, f32)>,
    /// A window declared neither way: the size the player gave it, frame and all (layout units).
    sized: Option<(f32, f32)>,
}

/// A canvas's or a chart's drawing: rectangles and placed text, in its own pixels.
type Drawing = (
    Vec<dereth_plugin_api::draw::Rect>,
    Vec<dereth_plugin_api::draw::Placed>,
);

/// Where a window stands: `((plugin, window), (x, y))`, in screen pixels.
pub type Place = ((String, String), (f32, f32));

/// Where windows stand.
pub type Places = Vec<Place>;

/// The size the player gave a window: `((plugin, window), (width, height))`, frame and all, in
/// layout units.
pub type Size = ((String, String), (f32, f32));

/// The sizes the player gave windows.
pub type Sizes = Vec<Size>;

/// The plugin windows' presentation state.
#[derive(Debug, Default)]
pub struct PluginUi {
    wins: BTreeMap<(String, String), Win>,
    inputs: Vec<Input>,
    /// The line or box with the keyboard, by its id; `0` for none.
    focus: u32,
    open: Option<OpenChoice>,
    /// The control the pointer rests on that has a tooltip: its id, since when, and its text.
    hover: Option<(u32, f64, String)>,
    /// Which art the pictures were made in.
    art: usize,
    /// How many windows have been placed with no place of their own, for the next one's.
    placed: usize,
    /// A picture has been made this frame: one a frame, the others showing their caption (or the
    /// picture they had) until their turn.
    made_picture: bool,
    /// The HUD the player is moving, and where on it the press took hold.
    hud_drag: Option<((String, String), (f32, f32))>,
    /// How many canvas and chart drawings have been made, for the tests.
    #[cfg(test)]
    pub(crate) drawings_made: usize,
}

/// A node's id in its window, for the state kept for it: its own id, else its place in the tree.
fn node_id(view: &WindowView, path: &str) -> u32 {
    let mut h: u32 = 0x811C_9DC5;
    for b in view
        .plugin
        .bytes()
        .chain([0])
        .chain(view.window.bytes())
        .chain([0])
        .chain(path.bytes())
    {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    h.max(1)
}

/// The largest a picture may be, a side.
const MAX_PICTURE: u32 = 1024;

/// Whether `r` is in view: on the screen and inside what the clip in force shows.
fn in_view(p: &Painter<'_>, r: Rect) -> bool {
    let screen = Rect::new(0.0, 0.0, p.screen.0, p.screen.1);
    let shown = dereth_ui_kit::draw::current_clip()
        .and_then(|c| c.intersect(&screen))
        .unwrap_or(screen);
    // A node of no size stands where it is: it is in view when that place is.
    let r = Rect::new(r.x, r.y, r.w.max(1.0), r.h.max(1.0));
    r.intersect(&shown).is_some()
}

/// A hash of what node `n` holds, its properties as the tree shows them.
fn content_hash(n: &UiNode) -> u64 {
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    for b in n.dump().bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01B3);
    }
    h
}

/// A PNG file as a picture of the kit's, or `None` for anything that is not one or is larger
/// than [`MAX_PICTURE`] a side.
fn picture(bytes: &[u8]) -> Option<dereth_ui_kit::art::Image> {
    let mut d = png::Decoder::new(std::io::Cursor::new(bytes));
    d.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = d.read_info().ok()?;
    let (w, h) = (r.info().width, r.info().height);
    if w == 0 || h == 0 || w > MAX_PICTURE || h > MAX_PICTURE {
        return None;
    }
    let mut buf = vec![0; r.output_buffer_size()];
    let frame = r.next_frame(&mut buf).ok()?;
    let px = &buf[..frame.buffer_size()];
    let bgra: Vec<u8> = match frame.color_type {
        png::ColorType::Rgba => px
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|c| [c[2], c[1], c[0], c[3]])
            .collect(),
        png::ColorType::Rgb => px
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|c| [c[2], c[1], c[0], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => px
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[0], c[0], c[1]])
            .collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    Some(dereth_ui_kit::art::Image {
        width: w,
        height: h,
        bgra,
    })
}

/// A number property, if it is one.
fn num(n: &UiNode, k: &str) -> Option<f64> {
    n.props.get(k).and_then(Value::as_f64)
}

/// Whether a control may be used: its `enabled`, true when left out.
fn enabled(n: &UiNode) -> bool {
    n.props
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// A number as a node's value: whole when it is whole.
#[must_use]
pub fn number_value(v: f64) -> Value {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: a whole number well inside an i64.
        Value::Int(v as i64)
    } else {
        Value::Num((v * 1e6).round() / 1e6)
    }
}

/// A number as a field shows it: whole numbers without a point, others to four places.
#[must_use]
pub fn number_text(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        let s = format!("{v:.4}");
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    }
}

/// `v` moved onto the steps of `step` from `lo`, kept between `lo` and `hi`.
fn snap(v: f64, lo: f64, hi: f64, step: f64) -> f64 {
    let v = if step > 0.0 {
        lo + ((v - lo) / step).round() * step
    } else {
        v
    };
    v.clamp(lo, hi.max(lo))
}

/// A meter's colour for its style: the game's vital bars', or a plain bronze.
#[must_use]
pub fn meter_colour(style: &str) -> Argb {
    match style {
        "health" => 0xFFB0_2020,
        "stamina" => 0xFFC0_A020,
        "mana" => 0xFF20_40C0,
        _ => 0xFFA0_8850,
    }
}

/// A plugin's colour, `0xRRGGBB`, made opaque.
fn opaque(c: u32) -> Argb {
    c | 0xFF00_0000
}

impl PluginUi {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The text style the plugin windows' text is drawn in.
    #[must_use]
    pub fn body_style(ctx: &Ctx<'_>) -> TextStyle {
        TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge())
    }

    /// The measures the windows are laid out by, from the painter's font and skin.
    #[must_use]
    pub fn metrics(p: &Painter<'_>, style: &TextStyle) -> Metrics {
        let k = p.scale.max(0.01);
        let ten = p.measure(style, "0000000000") / k;
        let cell = if ten > 0.0 { ten / 10.0 } else { 7.0 };
        let line = p.line_height(style) / k;
        let check = p.art.palette().check_side;
        let bar = kit::scrollbar_width(p).unwrap_or(6.0);
        Metrics::of(cell, line, check, bar)
    }

    /// Keep the state of the windows in `views` only, and start afresh for another art: pictures
    /// made in one art are none of another's.
    pub fn retain(&mut self, art: &dereth_ui_kit::art::Art, views: &[WindowView]) {
        let id = std::ptr::from_ref(art) as usize;
        if id != self.art {
            for w in self.wins.values_mut() {
                w.pictures.clear();
            }
            self.art = id;
        }
        let keep: std::collections::BTreeSet<(String, String)> = views
            .iter()
            .filter(|v| v.open)
            .map(|v| (v.plugin.clone(), v.window.clone()))
            .collect();
        let gone: Vec<(String, String)> = self
            .wins
            .keys()
            .filter(|k| !keep.contains(*k))
            .cloned()
            .collect();
        for k in gone {
            if let Some(w) = self.wins.get_mut(&k) {
                for (_, id) in w.pictures.values() {
                    if let Some(id) = id {
                        art.forget_image(*id);
                    }
                }
                w.pictures.clear();
                w.drawings.clear();
                w.drafts.clear();
                w.want = None;
                w.laid = None;
                w.frame.open = false;
            }
        }
        if self.open.as_ref().is_some_and(|o| !keep.contains(&o.key)) {
            self.open = None;
        }
    }

    /// Where a window that is not open will open: the place the player left it, else the next
    /// of a staggered row of places. An open window stays where it is.
    pub fn place_hint(&mut self, plugin: &str, window: &str, x: f32, y: f32) {
        let w = self
            .wins
            .entry((plugin.to_owned(), window.to_owned()))
            .or_default();
        if w.frame.pos.is_none() || !w.frame.open {
            w.frame.pos = Some((x, y));
        }
    }

    /// The size a window declared neither way will open at, frame and all (layout units): the
    /// size the player left it at. An open window keeps the size it has.
    pub fn size_hint(&mut self, plugin: &str, window: &str, width: f32, height: f32) {
        let w = self
            .wins
            .entry((plugin.to_owned(), window.to_owned()))
            .or_default();
        if w.sized.is_none() || !w.frame.open {
            w.sized = Some((width, height));
        }
    }

    /// The size the player gave each window declared neither way, for the caller to remember.
    #[must_use]
    pub fn sizes(&self) -> Sizes {
        self.wins
            .iter()
            .filter_map(|(k, w)| w.sized.map(|s| (k.clone(), s)))
            .collect()
    }

    /// Where each window stands, `((plugin, window), (x, y))` in screen pixels, for the caller to
    /// remember.
    #[must_use]
    pub fn places(&self) -> Places {
        self.wins
            .iter()
            .filter_map(|(k, w)| w.frame.pos.map(|p| (k.clone(), p)))
            .collect()
    }

    /// Where window `(plugin, window)` was drawn last, in screen pixels.
    #[must_use]
    pub fn rect_of(&self, plugin: &str, window: &str) -> Option<Rect> {
        self.wins
            .get(&(plugin.to_owned(), window.to_owned()))
            .filter(|w| w.frame.open)
            .map(|w| w.rect)
    }

    /// Note something the player did on a plugin surface beside the windows (the bar).
    pub fn push_input(&mut self, input: Input) {
        self.inputs.push(input);
    }

    /// What the player did since the last call.
    pub fn take_inputs(&mut self) -> Vec<Input> {
        std::mem::take(&mut self.inputs)
    }

    /// Whether a plugin's line or box has the keyboard.
    #[must_use]
    pub fn typing(&self) -> bool {
        self.focus != 0
    }

    /// Whether a choice's list is open: Escape and a press off it close it.
    #[must_use]
    pub fn choosing(&self) -> bool {
        self.open.is_some()
    }

    /// Let the keyboard go and close any choice's list (the interface is put away, or a question
    /// stands over the windows).
    pub fn let_go(&mut self) {
        self.focus = 0;
        self.open = None;
        self.hover = None;
    }

    /// What window `view`'s tree needs and asks for, measured in this painter's font: again only
    /// when the tree or the font changes.
    fn want_of(win: &mut Win, p: &Painter<'_>, ctx: &Ctx<'_>, view: &WindowView) -> Arc<Want> {
        let style = Self::body_style(ctx);
        let metrics = Self::metrics(p, &style);
        let fresh = win
            .want
            .as_ref()
            .is_none_or(|w| w.revision != view.revision || w.metrics != metrics);
        if fresh {
            let measure = PainterMeasure { p, style };
            win.want = Some(WantFor {
                revision: view.revision,
                metrics,
                want: Arc::new(layout::want(&view.tree, &metrics, &measure)),
            });
        }
        Arc::clone(&win.want.as_ref().expect("measured").want)
    }

    /// Window `view`'s tree laid out in a body of `size` (layout units): again only when the tree,
    /// the font or the size changes.
    fn laid_in(
        win: &mut Win,
        p: &Painter<'_>,
        ctx: &Ctx<'_>,
        view: &WindowView,
        size: (f32, f32),
    ) -> Arc<Laid> {
        let want = Self::want_of(win, p, ctx, view);
        let metrics = win
            .want
            .as_ref()
            .map_or_else(fallback_metrics, |w| w.metrics);
        let size = (size.0.round(), size.1.round());
        let fresh = win
            .laid
            .as_ref()
            .is_none_or(|l| l.revision != view.revision || l.metrics != metrics || l.size != size);
        if fresh {
            win.laid = Some(LaidFor {
                revision: view.revision,
                metrics,
                size,
                laid: Arc::new(layout::place_body(&view.tree, &want, &metrics, size)),
            });
        }
        Arc::clone(&win.laid.as_ref().expect("laid out").laid)
    }

    /// The body of window `view`, in layout units: the size its plugin keeps for it or the player
    /// is giving it now, else what its nodes ask for (at least as wide as its title needs); within
    /// its bounds and the screen, and wide enough for its nodes and a scrollbar where it is
    /// shorter than they are.
    fn body_size(&mut self, p: &Painter<'_>, ctx: &Ctx<'_>, view: &WindowView) -> (f32, f32) {
        let key = (view.plugin.clone(), view.window.clone());
        let win = self.wins.entry(key).or_default();
        let want = Self::want_of(win, p, ctx, view);
        let metrics = win
            .want
            .as_ref()
            .map_or_else(fallback_metrics, |w| w.metrics);
        let (lo, hi) = body_bounds(p, view, &metrics, &want);
        #[allow(clippy::cast_precision_loss)]
        let kept = match sizing(view) {
            Sizing::Plugin => win.live.or_else(|| {
                view.size.map(|(w, h)| {
                    (
                        w.clamp(1, 400) as f32 * metrics.cell,
                        h.clamp(1, 200) as f32 * metrics.line,
                    )
                })
            }),
            _ => None,
        };
        // A window asks for at least a few words' width, however little its nodes ask for.
        let (w, h) = kept.unwrap_or_else(|| {
            let (w, h) = layout::natural_body(&want);
            match view.tree.kind {
                NodeKind::Window => (w.max(NATURAL_CELLS * metrics.cell), h),
                _ => (w, h),
            }
        });
        let h = h.clamp(lo.1, hi.1.max(lo.1));
        let mut w = w.clamp(lo.0, hi.0.max(lo.0));
        if h + 0.5 < want.h.least {
            w = w.max((want.w.least + metrics.bar_room()).min(hi.0.max(lo.0)));
        }
        (w, h)
    }

    /// The size of window `view`, frame and all, in layout units: its body ([`Self::body_size`]),
    /// or for a window declared neither way the size the player gave it, within its bounds.
    pub fn outer_size(&mut self, p: &Painter<'_>, ctx: &Ctx<'_>, view: &WindowView) -> (f32, f32) {
        let body = self.body_size(p, ctx, view);
        let sizing = sizing(view);
        let key = (view.plugin.clone(), view.window.clone());
        let win = self.wins.entry(key).or_default();
        match (sizing, win.sized) {
            (Sizing::Player, Some(size)) => {
                let want = Self::want_of(win, p, ctx, view);
                let metrics = win
                    .want
                    .as_ref()
                    .map_or_else(fallback_metrics, |w| w.metrics);
                let (lo, hi) = body_bounds(p, view, &metrics, &want);
                let least = kit::window_size_for(p, lo);
                let most = kit::window_size_for(p, hi);
                (
                    size.0.clamp(least.0, most.0.max(least.0)),
                    size.1.clamp(least.1, most.1.max(least.1)),
                )
            }
            _ => kit::window_size_for(p, body),
        }
    }

    /// Before window `view`'s frame is drawn where `frame` puts it (else at `default_pos`, screen
    /// pixels): a press on its left, right or bottom edge, a bottom corner or its grip takes hold
    /// of it, and while the player holds it the window takes the size the pointer gives it,
    /// within its bounds and the screen, the edges not held staying where they were; its nodes
    /// are laid out again at each size. Let go, a window its plugin declares resizable tells its
    /// plugin the size in cells and lines. A window declared `resizable = false` is not resized.
    /// The size to draw the window at, in layout units.
    pub fn resize(
        &mut self,
        p: &Painter<'_>,
        ctx: &mut Ctx<'_>,
        view: &WindowView,
        frame: &mut WindowState,
        default_pos: (f32, f32),
    ) -> (f32, f32) {
        let size = self.outer_size(p, ctx, view);
        let sizing = sizing(view);
        if sizing == Sizing::Fixed {
            return size;
        }
        let k = p.scale;
        let key = (view.plugin.clone(), view.window.clone());
        let (x, y) = frame.pos.unwrap_or(default_pos);
        let rect = Rect::new(x, y, size.0 * k, size.1 * k);
        let inset = kit::window_body_inset(p);
        let Some(win) = self.wins.get_mut(&key) else {
            return size;
        };
        if win.resizing.is_none() && ctx.input.pressed[0] && ctx.input.hover(&rect) {
            let (mx, my) = ctx.input.mouse;
            let grip = grip_rect(rect, k).contains(mx, my);
            let (band, corner) = (EDGE * k, GRIP * k);
            let low = my >= rect.bottom() - corner;
            let left = mx < rect.x + band || (low && mx < rect.x + corner);
            let right =
                !left && (mx >= rect.right() - band || (low && mx >= rect.right() - corner));
            let bottom = my >= rect.bottom() - band || (low && (left || right));
            // The title bar is the header's: a press there moves the window.
            let below_header = my >= rect.y + p.art.palette().header * k;
            if grip || ((left || right || bottom) && below_header) {
                ctx.input.pressed[0] = false;
                ctx.input.captured = true;
                ctx.hot = true;
                win.resizing = Some(Resizing {
                    left: left && !grip,
                    right: right || grip,
                    bottom: bottom || grip,
                    from: (mx, my),
                    rect,
                });
            }
        }
        let Some(held) = win.resizing else {
            return size;
        };
        ctx.input.captured = true;
        let (mx, my) = ctx.input.mouse;
        let (dx, dy) = (mx - held.from.0, my - held.from.1);
        let w = held.rect.w
            + if held.right {
                dx
            } else if held.left {
                -dx
            } else {
                0.0
            };
        let h = held.rect.h + if held.bottom { dy } else { 0.0 };
        let want = Self::want_of(win, p, ctx, view);
        let metrics = win
            .want
            .as_ref()
            .map_or_else(fallback_metrics, |l| l.metrics);
        let (lo, hi) = body_bounds(p, view, &metrics, &want);
        // The body the pointer asks for, within the window's bounds.
        let body = (
            (w / k - inset[0] - inset[2]).clamp(lo.0, hi.0.max(lo.0)),
            (h / k - inset[1] - inset[3]).clamp(lo.1, hi.1.max(lo.1)),
        );
        match sizing {
            Sizing::Plugin => win.live = Some(body),
            _ => win.sized = Some(kit::window_size_for(p, body)),
        }
        let released = !ctx.input.down[0];
        if released {
            win.resizing = None;
        }
        let size = self.outer_size(p, ctx, view);
        // A held left edge moves the window's left side; its right side stays.
        let x = if held.left {
            held.rect.right() - size.0 * k
        } else {
            held.rect.x
        };
        frame.pos = Some((x, held.rect.y));
        if released && sizing == Sizing::Plugin {
            #[allow(clippy::cast_possible_truncation)]
            // LINT-OK: a window's size in cells and lines, a few hundred at most.
            let (cw, ch) = (
                (body.0 / metrics.cell).round().max(1.0) as i64,
                (body.1 / metrics.line).round().max(1.0) as i64,
            );
            self.inputs.push(Input::Resize {
                plugin: view.plugin.clone(),
                window: view.window.clone(),
                width: cw,
                height: ch,
            });
        }
        size
    }

    /// Draw window `view` whole, its frame from the kit, and answer where it is: for an
    /// interface that seats plugin windows in no windows of its own. `None` once it is closed.
    pub fn window(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        view: &WindowView,
    ) -> Option<Rect> {
        let key = (view.plugin.clone(), view.window.clone());
        let n = self.placed;
        let win = self.wins.entry(key.clone()).or_default();
        if !win.frame.open {
            win.frame.open = true;
            win.frame.opened_at = ctx.time - 1.0;
            if win.frame.pos.is_none() {
                #[allow(clippy::cast_precision_loss)]
                let step = (n % 8) as f32 * 24.0;
                win.frame.pos = Some((40.0 + step, 60.0 + step));
                self.placed += 1;
            }
        }
        let mut frame = win.frame;
        let size = self.resize(p, ctx, view, &mut frame, (40.0, 60.0));
        let out = kit::window(
            p,
            ctx,
            &mut frame,
            view.tree.text("title"),
            size,
            (40.0, 60.0),
        );
        // One its plugin opened of its own accord does not take the pad's focus.
        if !view.by_player {
            ctx.input.nav.quiet();
        }
        if let Some(w) = self.wins.get_mut(&key) {
            w.frame = frame;
            w.rect = out.rect;
        }
        if sizing(view) != Sizing::Fixed {
            grip(p, out.rect);
        }
        self.body(p, ctx, view, out.body);
        if out.closed {
            self.inputs.push(Input::Close {
                plugin: view.plugin.clone(),
                window: view.window.clone(),
            });
            if let Some(w) = self.wins.get_mut(&key) {
                w.frame.open = true;
            }
        }
        Some(out.rect)
    }

    /// Draw HUD `view` on a screen of `p.screen` pixels: a dark, half-clear plate with no frame,
    /// its nodes on it. Where it stands is `mode`'s: at the place its anchor names, or where the
    /// player put it (kept on the screen); while the player has the HUDs unlocked a press on it
    /// moves it, it is outlined, and its controls take no input. Otherwise only its controls take
    /// the pointer. Its place on screen.
    pub fn hud(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        view: &WindowView,
        mode: HudMode,
    ) -> Rect {
        let key = (view.plugin.clone(), view.window.clone());
        let win = self.wins.entry(key.clone()).or_default();
        let size = layout::natural_body(&Self::want_of(win, p, ctx, view));
        let k = p.scale;
        let pad = laid_pad();
        let (w, h) = ((size.0 + 2.0 * pad) * k, (size.1 + 2.0 * pad) * k);
        let (sw, sh) = p.screen;
        let m = HUD_MARGIN * k;
        let anchor = view
            .tree
            .props
            .get("anchor")
            .and_then(Value::as_str)
            .unwrap_or("top_left");
        let x = if anchor.ends_with("left") || anchor == "left" {
            m
        } else if anchor.ends_with("right") {
            sw - w - m
        } else {
            (sw - w) / 2.0
        };
        let y = if anchor.starts_with("top") {
            m
        } else if anchor.starts_with("bottom") {
            sh - h - m
        } else {
            (sh - h) / 2.0
        };
        let placed = match mode {
            HudMode::Anchored => None,
            HudMode::Placed { .. } => win.frame.pos,
        };
        let (mut x, mut y) = placed.unwrap_or((x, y));
        let unlocked = matches!(mode, HudMode::Placed { unlocked: true });
        let mut moved = false;
        if unlocked {
            let (mx, my) = ctx.input.mouse;
            let here = Rect::new(x, y, w, h);
            if self.hud_drag.is_none()
                && ctx.input.pressed[0]
                && !ctx.input.captured
                && ctx.input.hover(&here)
            {
                self.hud_drag = Some((key.clone(), (mx - x, my - y)));
                ctx.input.pressed[0] = false;
            }
            if let Some((held, (gx, gy))) = self.hud_drag.clone() {
                if held == key {
                    if ctx.input.down[0] {
                        (x, y) = (mx - gx, my - gy);
                        ctx.input.captured = true;
                    } else {
                        self.hud_drag = None;
                    }
                    moved = true;
                }
            }
        }
        // Wholly on the screen, whatever the screen's size and the scale.
        let x = x.clamp(0.0, (sw - w).max(0.0)).round();
        let y = y.clamp(0.0, (sh - h).max(0.0)).round();
        let r = Rect::new(x, y, w, h);
        if let Some(win) = self.wins.get_mut(&key) {
            win.rect = r;
            win.frame.open = true;
            if placed.is_some() || moved {
                win.frame.pos = Some((x, y));
            }
        }
        p.fill(r, HUD_PLATE);
        if unlocked {
            // Its controls take no input while it can be moved.
            let mouse = ctx.input.mouse;
            ctx.input.mouse = (-10_000.0, -10_000.0);
            self.body(p, ctx, view, r.inset(pad * k));
            ctx.input.mouse = mouse;
            let line = 1.0_f32.max(k.floor());
            let colour = p.art.palette().label_lit | 0xFF00_0000;
            for side in [
                Rect::new(r.x, r.y, r.w, line),
                Rect::new(r.x, r.bottom() - line, r.w, line),
                Rect::new(r.x, r.y, line, r.h),
                Rect::new(r.right() - line, r.y, line, r.h),
            ] {
                p.fill(side, colour);
            }
            if ctx.input.hover(&r) {
                ctx.hot = true;
            }
        } else {
            self.body(p, ctx, view, r.inset(pad * k));
        }
        r
    }

    /// Before any plugin window is drawn this frame: a press off an open choice's list, or
    /// Escape, closes it, and the press goes no further.
    pub fn begin(&mut self, p: &Painter<'_>, ctx: &mut Ctx<'_>) {
        self.made_picture = false;
        if let Some(open) = &self.open {
            if kit::dropdown_dismissed(p, ctx, open.anchor, open.options.len()) {
                self.open = None;
            }
        }
    }

    /// What stands over every plugin window: the open choice's list, and the tooltip of the
    /// control the pointer rests on. After every window is drawn.
    pub fn top(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>) {
        if let Some(mut open) = self.open.take() {
            let names: Vec<&str> = open.options.iter().map(String::as_str).collect();
            let picked = kit::dropdown_list(p, ctx, open.anchor, &names, open.at, &mut open.top);
            match picked {
                Some(i) => {
                    if let Some(choice) = open.options.get(i) {
                        self.inputs.push(Input::Set {
                            plugin: open.key.0.clone(),
                            window: open.key.1.clone(),
                            node: open.node.clone(),
                            value: Value::from(choice.as_str()),
                        });
                    }
                }
                None => self.open = Some(open),
            }
        }
        if let Some((_, since, text)) = &self.hover {
            if ctx.time - since >= TOOLTIP_DELAY && self.open.is_none() {
                let text = text.clone();
                kit::tooltip(p, ctx, &text, &[]);
            }
        }
    }

    /// The occluder the open choice's list makes over the windows under it.
    #[must_use]
    pub fn choice_rect(&self, p: &Painter<'_>) -> Option<Rect> {
        self.open
            .as_ref()
            .map(|o| kit::dropdown_list_rect(p, o.anchor, o.options.len()))
    }

    /// Draw window `view`'s nodes into `body` (screen pixels), inside the frame the interface
    /// drew, laid out at the body's size. A body shorter than its nodes scrolls through them (the
    /// wheel, its scrollbar, the pad's focus moving through them).
    pub fn body(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, view: &WindowView, body: Rect) {
        let k = p.scale.max(0.01);
        let size = (body.w / k, (body.h / k).max(0.0));
        let key = (view.plugin.clone(), view.window.clone());
        let win = self.wins.entry(key.clone()).or_default();
        let laid = Self::laid_in(win, p, ctx, view, size);
        win.frame.open = true;
        let at = laid.scroll.map_or(0.0, |tall| {
            win.body_scroll.clamp(0.0, (tall - laid.h).max(0.0))
        });
        // An open choice's list stands over every window: nothing under it takes the pointer.
        let occluded = self.choice_rect(p);
        let before = ctx.input.occluders.len();
        ctx.input.occluders.extend(occluded);
        let area = Rect::new(body.x, body.y, body.w, laid.h * k);
        p.list.push_clip(area);
        self.node(
            p,
            ctx,
            view,
            &view.tree,
            &laid,
            (body.x, body.y - at * k),
            "",
        );
        // After the nodes, so a list in the body takes the wheel over it first.
        let scrolled = laid.scroll.map(|tall| {
            let mut offset = at * k;
            kit::scroll(p, ctx, area, tall * k, &mut offset);
            offset / k
        });
        p.list.pop_clip();
        if let Some(w) = self.wins.get_mut(&key) {
            w.body_scroll = scrolled.unwrap_or(0.0);
        }
        ctx.input.occluders.truncate(before);
    }

    /// Note a control's tooltip while the pointer rests on it.
    fn tip(&mut self, ctx: &Ctx<'_>, n: &UiNode, id: u32, r: Rect) {
        let text = n.text("tooltip");
        if text.is_empty() || !ctx.input.hover(&r) {
            if self.hover.as_ref().is_some_and(|h| h.0 == id) && !ctx.input.hover(&r) {
                self.hover = None;
            }
            return;
        }
        if self.hover.as_ref().is_none_or(|h| h.0 != id) {
            self.hover = Some((id, ctx.time, text.to_owned()));
        }
    }

    /// Draw node `n`, laid out as `l`, its parent's top left at `origin` (screen pixels).
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn node(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        view: &WindowView,
        n: &UiNode,
        l: &Laid,
        origin: (f32, f32),
        path: &str,
    ) {
        let k = p.scale;
        let r = Rect::new(origin.0 + l.x * k, origin.1 + l.y * k, l.w * k, l.h * k);
        // A node out of view (past the screen's edge, or outside what the window shows) is not
        // drawn: what it holds is out of view with it. A window's own place is its body's.
        if !matches!(n.kind, NodeKind::Window | NodeKind::Hud) && !in_view(p, r) {
            return;
        }
        let path = match &n.id {
            Some(id) => format!("#{id}"),
            None => path.to_owned(),
        };
        let id = node_id(view, &path);
        let style = Self::body_style(ctx);
        let node_name = n.id.clone().unwrap_or_default();
        let (plugin, window) = (view.plugin.clone(), view.window.clone());
        match n.kind {
            NodeKind::Window | NodeKind::Hud | NodeKind::Column | NodeKind::Row => {
                for (i, (c, cl)) in n.children.iter().zip(&l.children).enumerate() {
                    self.node(p, ctx, view, c, cl, (r.x, r.y), &format!("{path}/{i}"));
                }
            }
            NodeKind::Group => {
                let title = n.text("title");
                let line = 1.0_f32.max(k.floor());
                let head = if title.is_empty() {
                    0.0
                } else {
                    p.line_height(&style)
                };
                let frame = Rect::new(r.x, r.y + head / 2.0, r.w, r.h - head / 2.0);
                let rule = p.art.palette().field_rule | 0xFF00_0000;
                // The rule round it, its top left open where the title stands in it.
                let (gap_from, gap_to) = if title.is_empty() {
                    (frame.x, frame.x)
                } else {
                    let tw = p.measure(&style, title);
                    (r.x + 6.0 * k, (r.x + 12.0 * k + tw).min(frame.right()))
                };
                for side in [
                    Rect::new(frame.x, frame.y, gap_from - frame.x, line),
                    Rect::new(gap_to, frame.y, frame.right() - gap_to, line),
                    Rect::new(frame.x, frame.bottom() - line, frame.w, line),
                    Rect::new(frame.x, frame.y, line, frame.h),
                    Rect::new(frame.right() - line, frame.y, line, frame.h),
                ] {
                    if side.w > 0.0 && side.h > 0.0 {
                        p.fill(side, rule);
                    }
                }
                if !title.is_empty() {
                    p.text(&style, (r.x + 9.0 * k).round(), r.y.round(), title);
                }
                for (i, (c, cl)) in n.children.iter().zip(&l.children).enumerate() {
                    self.node(p, ctx, view, c, cl, (r.x, r.y), &format!("{path}/{i}"));
                }
            }
            // Down a row, a separator is a line from its top to its bottom.
            NodeKind::Separator if r.h > r.w => {
                let rule = p.art.palette().field_rule | 0xFF00_0000;
                let line = 1.0_f32.max(k.floor());
                p.fill(Rect::new((r.x + r.w / 2.0).floor(), r.y, line, r.h), rule);
            }
            NodeKind::Separator => {
                if let Some(divider) = p.piece("rule") {
                    // The skin's own divider, stretched along, as tall as it is where there is room.
                    let h = (divider.h * k).min(r.h.max(1.0));
                    p.sprite(
                        &divider,
                        Rect::new(r.x, (r.y + (r.h - h) / 2.0).floor(), r.w, h),
                        dereth_ui_kit::draw::WHITE,
                    );
                } else {
                    let rule = p.art.palette().field_rule | 0xFF00_0000;
                    let line = 1.0_f32.max(k.floor());
                    p.fill(Rect::new(r.x, (r.y + r.h / 2.0).floor(), r.w, line), rule);
                }
            }
            NodeKind::Label => {
                let colour = n
                    .props
                    .get("color")
                    .and_then(|c| dereth_plugin_api::draw::color(c).ok())
                    .map_or(ctx.colours.text(), opaque);
                p.text_in(&style.colour(colour), r, Align::Left, n.text("text"));
                self.tip(ctx, n, id, r);
            }
            NodeKind::Hotkey => {
                p.text_in(&style, r, Align::Left, &layout::hotkey_line(n));
                self.tip(ctx, n, id, r);
            }
            NodeKind::Button => {
                if kit::button(p, ctx, r, n.text("text"), enabled(n)) {
                    self.inputs.push(Input::Click {
                        plugin,
                        window,
                        node: node_name,
                    });
                }
                self.tip(ctx, n, id, r);
            }
            NodeKind::Toggle => {
                let on = n
                    .props
                    .get("value")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let check = l.h.min(p.art.palette().check_side * k + 4.0 * k);
                let boxr = Rect::new(r.x, r.y, check, r.h);
                let text_x = r.x + p.art.palette().check_side * k + 6.0 * k;
                let label = Rect::new(text_x, r.y, (r.right() - text_x).max(0.0), r.h);
                let colour = if enabled(n) {
                    ctx.colours.text()
                } else {
                    ctx.colours.dim()
                };
                p.text_in(&style.colour(colour), label, Align::Left, n.text("text"));
                if enabled(n) {
                    let flipped = kit::checkbox(p, ctx, boxr, on).or_else(|| {
                        (ctx.over_quiet(&label) && ctx.input.clicked(&label)).then_some(!on)
                    });
                    if let Some(value) = flipped {
                        self.inputs.push(Input::Toggle {
                            plugin,
                            window,
                            node: node_name,
                            value,
                        });
                    }
                } else {
                    kit::checkbox_greyed(p, boxr, on);
                }
                self.tip(ctx, n, id, r);
            }
            NodeKind::Meter => {
                let max = num(n, "max").unwrap_or(1.0);
                let value = num(n, "value").unwrap_or(0.0);
                let frac = if max > 0.0 {
                    (value / max).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                #[allow(clippy::cast_possible_truncation)]
                // LINT-OK: a fraction from 0 to 1.
                let frac = frac as f32;
                kit::gauge(p, r, frac, meter_colour(n.text("style")));
                if !n.text("text").is_empty() {
                    p.text_in(&style, r, Align::Centre, n.text("text"));
                }
                self.tip(ctx, n, id, r);
            }
            NodeKind::Canvas | NodeKind::Chart => {
                self.drawing(p, view, n, id, r);
            }
            NodeKind::Image => {
                self.image(p, ctx, view, n, r);
                self.tip(ctx, n, id, r);
            }
            NodeKind::List => {
                self.list(p, ctx, view, n, l, id, r);
            }
            NodeKind::Tabs => {
                let titles: Vec<&str> = n.children.iter().map(|g| g.text("title")).collect();
                let chosen = n
                    .props
                    .get("selected")
                    .and_then(Value::as_i64)
                    .and_then(|s| usize::try_from(s - 1).ok())
                    .unwrap_or(0)
                    .min(titles.len().saturating_sub(1));
                let mut selected = chosen;
                let bar = Rect::new(
                    r.x,
                    r.y,
                    r.w,
                    l.children.first().map_or(r.h, |p| p.y * k) - 3.0 * k,
                );
                if kit::tabs(p, ctx, bar, &titles, &mut selected) && selected != chosen {
                    self.inputs.push(Input::Select {
                        plugin,
                        window,
                        node: node_name,
                        row: i64::try_from(selected).unwrap_or(0) + 1,
                    });
                }
                if let (Some(page), Some(pl)) = (n.children.get(chosen), l.children.get(chosen)) {
                    let at = (r.x + pl.x * k, r.y + pl.y * k);
                    let page_path = format!("{path}/{chosen}");
                    match pl.scroll {
                        None => {
                            for (i, (c, cl)) in page.children.iter().zip(&pl.children).enumerate() {
                                self.node(p, ctx, view, c, cl, at, &format!("{page_path}/{i}"));
                            }
                        }
                        // A page shorter than its nodes scrolls through them under the row of
                        // tabs, which stays where it is.
                        Some(tall) => {
                            let page_id = node_id(view, &page_path);
                            let key = (view.plugin.clone(), view.window.clone());
                            let area = Rect::new(at.0, at.1, pl.w * k, pl.h * k);
                            let kept = self
                                .wins
                                .get(&key)
                                .and_then(|w| w.scroll.get(&page_id))
                                .copied()
                                .unwrap_or(0.0)
                                .clamp(0.0, (tall - pl.h).max(0.0));
                            p.list.push_clip(area);
                            for (i, (c, cl)) in page.children.iter().zip(&pl.children).enumerate() {
                                let shifted = (at.0, at.1 - kept * k);
                                self.node(
                                    p,
                                    ctx,
                                    view,
                                    c,
                                    cl,
                                    shifted,
                                    &format!("{page_path}/{i}"),
                                );
                            }
                            let mut offset = kept * k;
                            kit::scroll(p, ctx, area, tall * k, &mut offset);
                            p.list.pop_clip();
                            if let Some(w) = self.wins.get_mut(&key) {
                                w.scroll.insert(page_id, offset / k);
                            }
                        }
                    }
                }
            }
            NodeKind::Number => {
                let label = l.parts.first().copied().unwrap_or(0.0) * k;
                p.text_in(
                    &style,
                    Rect::new(r.x, r.y, label, r.h),
                    Align::Left,
                    n.text("text"),
                );
                let (lo, hi) = (
                    num(n, "min").unwrap_or(f64::MIN),
                    num(n, "max").unwrap_or(f64::MAX),
                );
                let step = num(n, "step").filter(|s| *s > 0.0).unwrap_or(1.0);
                let value = num(n, "value").unwrap_or(lo.max(0.0)).clamp(lo, hi.max(lo));
                let side = r.h;
                let down = Rect::new(r.x + label, r.y, side, side);
                let up = Rect::new(r.right() - side, r.y, side, side);
                let field = Rect::new(
                    down.right() + 2.0 * k,
                    r.y,
                    (up.x - down.right() - 4.0 * k).max(0.0),
                    r.h,
                );
                kit::field_ground(p, field, false);
                p.text_in(&style, field, Align::Centre, &number_text(value));
                let usable = enabled(n);
                let mut set = None;
                if kit::button(p, ctx, down, "<", usable && value > lo) {
                    set = Some(snap(value - step, lo, hi, 0.0));
                }
                if kit::button(p, ctx, up, ">", usable && value < hi) {
                    set = Some(snap(value + step, lo, hi, 0.0));
                }
                if let Some(v) = set {
                    self.inputs.push(Input::Set {
                        plugin,
                        window,
                        node: node_name,
                        value: number_value(v),
                    });
                }
                self.tip(ctx, n, id, r);
            }
            NodeKind::Choice => {
                let label = l.parts.first().copied().unwrap_or(0.0) * k;
                p.text_in(
                    &style,
                    Rect::new(r.x, r.y, label, r.h),
                    Align::Left,
                    n.text("text"),
                );
                let options: Vec<String> = n
                    .props
                    .get("options")
                    .and_then(Value::as_list)
                    .unwrap_or_default()
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect();
                let value = n.text("value").to_owned();
                let field = Rect::new(r.x + label, r.y, r.w - label, r.h);
                let open = self.open.as_ref().is_some_and(|o| o.id == id);
                let usable = enabled(n) && !options.is_empty();
                if kit::dropdown_box(p, ctx, field, &value, open, usable) {
                    if open {
                        self.open = None;
                    } else {
                        let at = options.iter().position(|o| *o == value).unwrap_or(0);
                        self.open = Some(OpenChoice {
                            key: (plugin, window),
                            node: node_name,
                            id,
                            anchor: field,
                            top: at.saturating_sub(kit::DROPDOWN_ROWS / 2),
                            at,
                            options,
                        });
                    }
                } else if open {
                    // The list follows its box as the window moves.
                    if let Some(o) = &mut self.open {
                        o.anchor = field;
                    }
                }
                self.tip(ctx, n, id, r);
            }
            NodeKind::Input => {
                let label = l.parts.first().copied().unwrap_or(0.0) * k;
                p.text_in(
                    &style,
                    Rect::new(r.x, r.y, label, r.h),
                    Align::Left,
                    n.text("text"),
                );
                let field = Rect::new(r.x + label, r.y, r.w - label, r.h);
                let current = n.text("value").to_owned();
                if enabled(n) {
                    let had = self.focus == id;
                    let mut text = self
                        .wins
                        .get(&(plugin.clone(), window.clone()))
                        .and_then(|w| w.drafts.get(&id))
                        .filter(|_| had)
                        .cloned()
                        .unwrap_or_else(|| current.clone());
                    let before = text.clone();
                    kit::text_box_holding(
                        p,
                        ctx,
                        field,
                        &mut text,
                        &mut self.focus,
                        id,
                        Align::Left,
                        MAX_TEXT,
                    );
                    let has = self.focus == id;
                    if let Some(w) = self.wins.get_mut(&(plugin.clone(), window.clone())) {
                        if has {
                            w.drafts.insert(id, text.clone());
                        } else {
                            w.drafts.remove(&id);
                        }
                    }
                    if has && text != before {
                        self.inputs.push(Input::Set {
                            plugin,
                            window,
                            node: node_name,
                            value: Value::Str(text.clone()),
                        });
                    }
                    if !has && text.is_empty() {
                        let hint = n.text("placeholder");
                        p.text_in(
                            &style.colour(ctx.colours.dim()),
                            field.inset(6.0 * k),
                            Align::Left,
                            hint,
                        );
                    }
                } else {
                    kit::field_ground(p, field, false);
                    p.text_in(
                        &style.colour(ctx.colours.dim()),
                        field.inset(6.0 * k),
                        Align::Left,
                        &current,
                    );
                }
                self.tip(ctx, n, id, r);
            }
            NodeKind::TextBox => {
                let editable = n.props.get("editable").and_then(Value::as_bool) == Some(true);
                let current = n.text("value").to_owned();
                let key = (plugin.clone(), window.clone());
                let had = self.focus == id;
                let (mut text, mut scroll) = {
                    let w = self.wins.entry(key.clone()).or_default();
                    (
                        w.drafts
                            .get(&id)
                            .filter(|_| had)
                            .cloned()
                            .unwrap_or(current),
                        w.scroll.get(&id).copied().unwrap_or(0.0),
                    )
                };
                let changed = kit::text_area(
                    p,
                    ctx,
                    r,
                    &mut text,
                    &mut self.focus,
                    id,
                    &mut scroll,
                    editable,
                    "",
                    MAX_TEXT,
                );
                let has = self.focus == id;
                if let Some(w) = self.wins.get_mut(&key) {
                    w.scroll.insert(id, scroll);
                    if has {
                        w.drafts.insert(id, text.clone());
                    } else {
                        w.drafts.remove(&id);
                    }
                }
                if changed && editable {
                    self.inputs.push(Input::Set {
                        plugin,
                        window,
                        node: node_name,
                        value: Value::Str(text),
                    });
                }
                self.tip(ctx, n, id, r);
            }
            NodeKind::Slider => {
                let label = l.parts.first().copied().unwrap_or(0.0) * k;
                p.text_in(
                    &style,
                    Rect::new(r.x, r.y, label, r.h),
                    Align::Left,
                    n.text("text"),
                );
                let (lo, hi) = (num(n, "min").unwrap_or(0.0), num(n, "max").unwrap_or(100.0));
                let hi = hi.max(lo);
                let step = num(n, "step").unwrap_or(((hi - lo) / 10.0).max(f64::MIN_POSITIVE));
                let value = num(n, "value").unwrap_or(lo).clamp(lo, hi);
                let cell = self
                    .wins
                    .get(&(plugin.clone(), window.clone()))
                    .and_then(|w| w.laid.as_ref())
                    .map_or(7.0, |l| l.metrics.cell);
                let value_w = 7.0 * cell * k;
                let track = Rect::new(r.x + label, r.y, (r.w - label - value_w).max(0.0), r.h);
                let span = hi - lo;
                #[allow(clippy::cast_possible_truncation)]
                // LINT-OK: a fraction from 0 to 1.
                let t = if span > 0.0 {
                    ((value - lo) / span) as f32
                } else {
                    0.0
                };
                p.text_in(
                    &style,
                    Rect::new(track.right() + 6.0 * k, r.y, value_w - 6.0 * k, r.h),
                    Align::Left,
                    &number_text(value),
                );
                if enabled(n) {
                    if let Some(t) = kit::track_slider(p, ctx, track, t) {
                        let v = snap(lo + f64::from(t) * span, lo, hi, step);
                        if (v - value).abs() > f64::EPSILON * value.abs().max(1.0) {
                            self.inputs.push(Input::Set {
                                plugin,
                                window,
                                node: node_name,
                                value: number_value(v),
                            });
                        }
                    }
                } else {
                    let pal = *p.art.palette();
                    let bar =
                        Rect::new(track.x, track.y + track.h / 2.0 - 3.0 * k, track.w, 6.0 * k);
                    p.fill(bar, pal.track);
                    p.fill(
                        Rect::new(bar.x, bar.y, bar.w * t, bar.h),
                        pal.track_fill & 0x80FF_FFFF,
                    );
                }
                self.tip(ctx, n, id, r);
            }
        }
    }

    /// A list: its column headings, then its rows, scrolled by the wheel, its bar or the pad's
    /// focus moving through them; a click on a row chooses it.
    #[allow(clippy::too_many_arguments)]
    fn list(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        view: &WindowView,
        n: &UiNode,
        l: &Laid,
        id: u32,
        r: Rect,
    ) {
        let k = p.scale;
        let style = Self::body_style(ctx);
        let key = (view.plugin.clone(), view.window.clone());
        let metrics = self
            .wins
            .get(&key)
            .and_then(|w| w.want.as_ref())
            .map_or_else(fallback_metrics, |w| w.metrics);
        let heads = layout::headings(n);
        let rows = layout::rows(n);
        let row_h = metrics.row() * k;
        let head_h = if heads.is_empty() {
            0.0
        } else {
            (metrics.line + 2.0) * k
        };
        // Inside the well's rim.
        let rim = kit::well_rim(p);
        kit::well(p, r);
        let icon_w = if layout::has_icons(n) {
            (metrics.line + metrics.gap) * k
        } else {
            0.0
        };
        // The columns from inside the rim and a gap, a cell apart.
        let col_x = |c: usize| -> f32 {
            #[allow(clippy::cast_precision_loss)]
            let between = metrics.cell * c as f32;
            r.x + (metrics.rim + metrics.gap + between + l.parts.iter().take(c).sum::<f32>()) * k
                + icon_w
        };
        if !heads.is_empty() {
            let dim = style.colour(ctx.colours.dim());
            for (c, h) in heads.iter().enumerate() {
                let w = l.parts.get(c).copied().unwrap_or(0.0) * k;
                let cell = Rect::new(col_x(c), r.y + rim, w, head_h);
                let shown = p.fit(&dim, h, w);
                p.text_in(&dim, cell, Align::Left, &shown);
            }
        }
        let area = Rect::new(
            r.x + rim,
            r.y + rim + head_h,
            r.w - 2.0 * rim,
            r.h - head_h - 2.0 * rim,
        );
        #[allow(clippy::cast_precision_loss)]
        let content = rows.len() as f32 * row_h;
        let (mut offset, had) = self
            .wins
            .get(&key)
            .map(|w| {
                (
                    w.scroll.get(&id).copied().unwrap_or(0.0),
                    w.rows.get(&id).copied(),
                )
            })
            .unwrap_or_default();
        // Scrolled to its end (or with every row in sight), it keeps its last row in sight as
        // rows are added: a log shows its newest line.
        if let Some(had) = had.filter(|had| rows.len() > *had) {
            #[allow(clippy::cast_precision_loss)]
            let end = (had as f32 * row_h - area.h).max(0.0);
            if offset >= end - 1.0 {
                offset = (content - area.h).max(0.0);
            }
        }
        let at = kit::scroll(p, ctx, area, content, &mut offset);
        if let Some(w) = self.wins.get_mut(&key) {
            w.scroll.insert(id, offset);
            w.rows.insert(id, rows.len());
        }
        let bar = if content > area.h {
            kit::scrollbar_width(p).map_or(6.0, |w| w + 2.0) * k
        } else {
            0.0
        };
        let selected = n
            .props
            .get("selected")
            .and_then(Value::as_i64)
            .and_then(|s| usize::try_from(s - 1).ok());
        // The rows in sight, and one either side for the pad's focus to step on to.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: a row index from a non-negative offset, at most the list's ten thousand rows.
        let first = ((at / row_h).floor().max(0.0) as usize).saturating_sub(1);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let last = (((at + area.h) / row_h).ceil().max(0.0) as usize + 1).min(rows.len());
        p.list.push_clip(area);
        for i in first..last {
            let Some(cells) = rows.get(i).and_then(Value::as_list) else {
                continue;
            };
            #[allow(clippy::cast_precision_loss)]
            let y = area.y + i as f32 * row_h - at;
            let row = Rect::new(area.x, y, area.w - bar, row_h);
            let over = ctx.over(&row);
            if selected == Some(i) {
                kit::list_highlight(p, row, true);
            } else if over {
                p.fill(row, p.art.palette().row_hover);
            }
            let colour = cells
                .iter()
                .find_map(|c| dereth_plugin_api::ui::cell_parts(c).1)
                .map_or(ctx.colours.text(), opaque);
            if let Some(icon) = cells
                .iter()
                .find_map(|c| dereth_plugin_api::ui::cell_parts(c).2)
                .and_then(|did| p.art.icon(did))
            {
                let s = row_h - 2.0 * k;
                p.sprite(
                    &icon,
                    Rect::new(area.x + 2.0 * k, y + 1.0 * k, s, s),
                    dereth_ui_kit::draw::WHITE,
                );
            }
            for (c, cell) in cells.iter().enumerate() {
                let w = l.parts.get(c).copied().unwrap_or(0.0) * k;
                let text = layout::cell_text(cell);
                let cellr = Rect::new(col_x(c), y, w, row_h);
                let shown = p.fit(&style, &text, w);
                p.text_in(&style.colour(colour), cellr, Align::Left, &shown);
            }
            if over && ctx.input.clicked(&row) {
                self.inputs.push(Input::Select {
                    plugin: view.plugin.clone(),
                    window: view.window.clone(),
                    node: n.id.clone().unwrap_or_default(),
                    row: i64::try_from(i).unwrap_or(0) + 1,
                });
            }
        }
        p.list.pop_clip();
        self.tip(ctx, n, id, r);
    }

    /// An image: its picture (made once from its file) scaled to the node, or the game's icon,
    /// with its caption in place of a picture it has not got.
    fn image(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &Ctx<'_>,
        view: &WindowView,
        n: &UiNode,
        r: Rect,
    ) {
        let style = Self::body_style(ctx);
        let icon = n
            .props
            .get("icon")
            .and_then(Value::as_i64)
            .and_then(|i| u32::try_from(i).ok())
            .filter(|i| *i != 0);
        if let Some(did) = icon {
            if let Some(s) = p.art.icon(did) {
                p.sprite(&s, r, dereth_ui_kit::draw::WHITE);
                return;
            }
        }
        let src = n.text("src");
        let key = (view.plugin.clone(), view.window.clone());
        let file = view.images.iter().find(|pic| pic.src == src);
        let made_one = self.made_picture;
        let mut made = false;
        let tex = match file {
            Some(pic) => {
                let win = self.wins.entry(key).or_default();
                match win.pictures.get(src) {
                    // Told apart by the hash it was given as it arrived: its file is not read
                    // again.
                    Some((h, tex)) if *h == pic.hash => *tex,
                    // One picture is made a frame; this one waits its turn, showing what it had.
                    old if made_one => old.and_then(|(_, tex)| *tex),
                    old => {
                        if let Some((_, Some(old))) = old {
                            p.art.forget_image(*old);
                        }
                        let tex = picture(&pic.bytes).map(|im| p.art.add_image(im));
                        win.pictures.insert(src.to_owned(), (pic.hash, tex));
                        made = true;
                        tex
                    }
                }
            }
            None => None,
        };
        self.made_picture |= made;
        match tex.and_then(|t| p.art.image(t).map(|im| (t, im))) {
            Some((t, im)) => {
                #[allow(clippy::cast_precision_loss)]
                let src_r = Rect::new(0.0, 0.0, im.width as f32, im.height as f32);
                p.list.image(t, src_r, r, dereth_ui_kit::draw::WHITE);
                if !n.text("text").is_empty() {
                    p.text(
                        &style,
                        r.x + 2.0 * p.scale,
                        r.y + 2.0 * p.scale,
                        n.text("text"),
                    );
                }
            }
            None => {
                kit::well(p, r);
                let caption = if n.text("text").is_empty() {
                    src
                } else {
                    n.text("text")
                };
                // Inside the well's rim, cut to it.
                let room = r.inset(kit::well_rim(p) + 2.0 * p.scale);
                let dim = style.colour(ctx.colours.dim());
                let shown = p.fit(&dim, caption, room.w);
                p.list.push_clip(room);
                p.text(&dim, room.x, room.y, &shown);
                p.list.pop_clip();
            }
        }
    }

    /// A canvas or a chart: its drawing's rectangles and text, scaled to the node and cut to it.
    fn drawing(&mut self, p: &mut Painter<'_>, view: &WindowView, n: &UiNode, id: u32, r: Rect) {
        let key = (view.plugin.clone(), view.window.clone());
        let win = self.wins.entry(key).or_default();
        // Made again only when what it draws changes, not whenever something else in its window
        // does: a new revision of the window is looked at once, by what this node holds.
        let drawing = match win.drawings.get(&id) {
            Some((rev, _, d)) if *rev == view.revision => Arc::clone(d),
            seen => {
                let what = content_hash(n);
                match seen {
                    Some((_, h, d)) if *h == what => {
                        let d = Arc::clone(d);
                        win.drawings
                            .insert(id, (view.revision, what, Arc::clone(&d)));
                        d
                    }
                    _ => {
                        let (w, h) = n.pixel_size();
                        let d = Arc::new(dereth_plugin_api::draw::rasterize(
                            &n.drawing(),
                            i32::try_from(w).unwrap_or(160),
                            i32::try_from(h).unwrap_or(80),
                        ));
                        win.drawings
                            .insert(id, (view.revision, what, Arc::clone(&d)));
                        #[cfg(test)]
                        {
                            self.drawings_made += 1;
                        }
                        d
                    }
                }
            }
        };
        let k = p.scale;
        if let Some(bg) = n
            .props
            .get("background")
            .and_then(|c| dereth_plugin_api::draw::color(c).ok())
        {
            p.fill(r, opaque(bg));
        }
        p.list.push_clip(r);
        #[allow(clippy::cast_precision_loss)]
        for &(x, y, w, h, c) in &drawing.0 {
            p.fill(
                Rect::new(
                    r.x + x as f32 * k,
                    r.y + y as f32 * k,
                    w as f32 * k,
                    h as f32 * k,
                ),
                opaque(c),
            );
        }
        let style = TextStyle::new(Family::Body, 11.0, 0xFFFF_FFFF);
        #[allow(clippy::cast_precision_loss)]
        for (x, y, text, c) in &drawing.1 {
            p.text(
                &style.colour(opaque(*c)),
                (r.x + *x as f32 * k).round(),
                (r.y + *y as f32 * k).round(),
                text,
            );
        }
        p.list.pop_clip();
    }
}

/// A window's grip, drawn as part of its frame in its bottom right corner, inside the frame's
/// border: the skin's own mark (`window.grip`: how far in from the corner it stands and its side,
/// in layout units), a few dots stepping up to the corner. A skin with a frame of its own and no
/// mark has none, its frame's corner being the handle; one with no frame of its own (the plain
/// one) has the mark in its border's colour.
fn grip(p: &mut Painter<'_>, rect: Rect) {
    let Some((inset, side)) = grip_mark(p) else {
        return;
    };
    let colour = p.art.palette().panel_border | 0xFF00_0000;
    let k = p.scale;
    let dot = (side / 5.0 * k).round().max(1.0);
    let step = ((side * k - dot) / 2.0).max(dot);
    let (right, bottom) = (rect.right() - inset * k, rect.bottom() - inset * k);
    // Three dots along the bottom, two over them and one in the corner: a step up to it.
    for row in 0..3_u8 {
        for i in 0..3 - row {
            let r = Rect::new(
                (right - dot - f32::from(i) * step).round(),
                (bottom - dot - f32::from(row) * step).round(),
                dot,
                dot,
            );
            if in_view(p, r) {
                p.fill(r, colour);
            }
        }
    }
}

/// The skin's grip mark, `(inset, side)` in layout units, or none.
fn grip_mark(p: &Painter<'_>) -> Option<(f32, f32)> {
    if let Some(v) = p.art.piece_value("window.grip").filter(|v| v.len() == 2) {
        return Some((v[0], v[1]));
    }
    let plain = !p.art.has_piece("window.tl");
    (plain && p.art.palette().panel_border >> 24 != 0).then_some((2.0, 6.0))
}

/// Where a window at `rect` has its grip: the square of its bottom right corner.
fn grip_rect(rect: Rect, k: f32) -> Rect {
    Rect::new(
        rect.right() - GRIP * k,
        rect.bottom() - GRIP * k,
        GRIP * k,
        GRIP * k,
    )
}

/// The room a window's title needs across its body: the title in the heading's face, a cell
/// either side of it and two more, and the close box (layout units).
fn title_room(p: &Painter<'_>, metrics: &Metrics, tree: &UiNode) -> f32 {
    let k = p.scale.max(0.01);
    let title = TextStyle::new(
        Family::Heading,
        p.art.palette().title_points,
        p.art.palette().title,
    );
    p.measure(&title, tree.text("title")) / k + 4.0 * metrics.cell + p.art.palette().close_side
}

/// The least and the most body the player may give window `view`, in layout units: never
/// narrower than its nodes' least width or its title's room, and within the sizes it declares
/// (`min_width` and `max_width` in cells, `min_height` and `max_height` in lines) and the screen.
/// A body shorter than its nodes scrolls through them, so its least height is a line.
fn body_bounds(
    p: &Painter<'_>,
    view: &WindowView,
    m: &Metrics,
    want: &Want,
) -> ((f32, f32), (f32, f32)) {
    let k = p.scale.max(0.01);
    #[allow(clippy::cast_precision_loss)]
    let bound = |key: &str, unit: f32| {
        view.tree
            .props
            .get(key)
            .and_then(Value::as_i64)
            .map(|v| v.clamp(1, 400) as f32 * unit)
    };
    let inset = kit::window_body_inset(p);
    let screen = (
        p.screen.0 / k - inset[0] - inset[2],
        p.screen.1 / k - inset[1] - inset[3],
    );
    let least_w = if view.tree.kind == NodeKind::Window {
        title_room(p, m, &view.tree)
    } else {
        0.0
    };
    let lo = (
        least_w.max(want.w.least).ceil(),
        m.line.max(bound("min_height", m.line).unwrap_or(0.0)),
    );
    let hi = (
        bound("max_width", m.cell)
            .unwrap_or(f32::MAX)
            .min(screen.0)
            .max(lo.0),
        bound("max_height", m.line)
            .unwrap_or(f32::MAX)
            .min(screen.1)
            .max(lo.1),
    );
    (lo, hi)
}

/// The measures a window not yet laid out is sized by.
fn fallback_metrics() -> Metrics {
    Metrics::of(7.0, 16.0, 11.0, 6.0)
}

/// A HUD's padding round its nodes, in layout units.
fn laid_pad() -> f32 {
    6.0
}
