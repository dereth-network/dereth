//! Plugin windows on the desktop: each declared window tree laid out and drawn by the widget kit
//! in the skin of the interface it is drawn under, and the player's input handed back.
//!
//! **Depends on** the widget kit (`dereth-ui-kit`), which draws them ([`window`], [`layer`]); the
//! declared tree (`dereth-plugin-api`); the plugin host's question and its answer
//! (`dereth-client-contract`); the identifiers (`dereth-primitives`); the game's font records
//! (`dereth-assets`), which the plain skin draws its text in ([`skin`]); and a PNG decoder (`png`)
//! for a plugin's pictures. **Used by** the
//! client shell, which reads the plugin host's windows each frame, draws them in a layer over the
//! modern and the classic interface and hands the player's input back to it, and by the Horizon
//! interface, which seats them among its own windows.
//!
//! **Must never** reach the runtime or the plugin host (`cargo xtask seams`, `seam: client
//! crates`): it is given plain views of the windows and answers plain inputs.
//!
//! # How a window is drawn
//!
//! A plugin window is laid out by the plugin interface's own rules (`dereth_plugin_api::layout`,
//! the same for every host) in its interface's font, again whenever it changes or its size does,
//! and drawn every frame with the kit's widgets ([`window::PluginUi`]): a frame the player drags
//! by its title bar, a close box, and the nodes filling its body, top to bottom (a `row` left to
//! right); a body shorter than its nodes scrolls. A button is the kit's button; a toggle a check box
//! with its caption; a list scrolls, its rows chosen by a click; a choice opens a list of its
//! options; a number steps and a slider is dragged; a line or a box of text takes typing; a
//! picture is drawn from its file, a game icon from the game's data.
//!
//! An interface with windows of its own (Horizon) seats each plugin window among them. Under an
//! interface without, the windows, HUDs, bar and marks are drawn as a layer ([`layer::Layer`]):
//! a press on a window brings it in front of the others, a new window opens in front, and a
//! window taken down and opened again comes back where it was left. Either way the player sizes a
//! window by its left, right and bottom edges, its bottom corners and the grip in its bottom right
//! corner ([`PluginUi::resize`]), as its declaration allows ([`Sizing`]), and every window is
//! kept on the screen as the screen and the scale change.
//!
//! # The host's own tier
//!
//! Over every plugin window stand the plugin host's own windows (the plugin panel) and its
//! question to the player (a file a plugin asks for) ([`host`]), drawn in the interface's own look
//! as any window is. The question's answer carries its id.
//!
//! # The plugin bar
//!
//! A strip of one button per plugin active here, as Decal's bar was: the button shows the
//! plugin's name and, by its colour, what it is doing (running, paused, idle, or stopped on an
//! error), framed in gold while the plugin's main window is open; a click opens or closes that
//! window. The bar is moved by its handle at its left end and sized by its grip at its right
//! end, and is never wider than the screen; where its buttons do not all fit they scroll along it
//! ([`bar`]).

#![forbid(unsafe_code)]

pub mod bar;
pub mod host;
pub mod layer;
pub mod marks;
pub mod skin;
pub mod window;

pub use bar::{BarLook, BarPlace};
pub use layer::{Drawn, Layer, Scene};
pub use marks::{ArrowMark, MapMark, Marks, RadarMark};
pub use window::{HudMode, Place, Places, PluginUi, Size, Sizes, Sizing};

use std::collections::BTreeMap;

use dereth_plugin_api::{NodeKind, UiNode, Value};
use dereth_primitives::num::{math, to_i32_f64};

/// The width of a character and the height of a line of a label in the world, as the labels are
/// kept apart.
const CELL_W: i32 = 7;
const LINE_H: i32 = 16;

/// Where a window stands among the others.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tier {
    /// A plugin's window or HUD.
    #[default]
    Plugin,
    /// The host's own window (the plugin panel), drawn above every plugin window, HUD and mark
    /// ([`host`]).
    Host,
}

/// A PNG file an image node shows, by its `src`, with the hash it was given as it arrived: the
/// picture is told from another by that hash, its file never read again until it changes.
#[derive(Debug, Clone)]
pub struct Picture {
    pub src: String,
    pub bytes: std::sync::Arc<[u8]>,
    pub hash: u64,
}

/// One window as the plugin host keeps it: a copy the presentation can keep while the host goes on
/// (the tree and the pictures are shared, made again only when the window changes).
#[derive(Debug, Clone)]
pub struct WindowView {
    /// Where it stands among the others.
    pub tier: Tier,
    /// The owning plugin's id.
    pub plugin: String,
    /// The window's id.
    pub window: String,
    pub tree: std::sync::Arc<UiNode>,
    pub open: bool,
    /// Changes whenever the tree or `open` does.
    pub revision: u64,
    /// The PNG files the tree's image nodes show (see [`image_sources`]).
    pub images: Vec<Picture>,
    /// The size the player gave a resizable window, in character cells and lines.
    pub size: Option<(i64, i64)>,
    /// The player opened it (the plugin bar, or the plugin answering one of the player's
    /// gestures): it may take the pad's focus as it opens. One a plugin opened of its own accord
    /// never does.
    pub by_player: bool,
}

/// One plugin on the bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarItem {
    pub plugin: String,
    pub name: String,
    /// `running`, `paused`, `idle` or `error`.
    pub state: String,
    /// Whether its main window is open.
    pub open: bool,
}

/// The `src` of every image node in `tree`, for the caller to hand over the files.
#[must_use]
pub fn image_sources(tree: &UiNode) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![tree];
    while let Some(n) = stack.pop() {
        if n.kind == NodeKind::Image && !n.text("src").is_empty() {
            out.push(n.text("src").to_owned());
        }
        stack.extend(n.children.iter());
    }
    out
}

/// What the player did on a plugin window, or on the game's map for the plugins listening.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    Click {
        plugin: String,
        window: String,
        node: String,
    },
    /// A toggle flipped to `value`.
    Toggle {
        plugin: String,
        window: String,
        node: String,
        value: bool,
    },
    /// A list's row chosen, from 1.
    Select {
        plugin: String,
        window: String,
        node: String,
        row: i64,
    },
    Close {
        plugin: String,
        window: String,
    },
    /// A number stepped or a choice moved on, to `value`.
    Set {
        plugin: String,
        window: String,
        node: String,
        value: Value,
    },
    /// A resizable window let go at a new size, in character cells and lines.
    Resize {
        plugin: String,
        window: String,
        width: i64,
        height: i64,
    },
    /// A plugin's button on the bar clicked.
    Bar {
        plugin: String,
    },
    /// A plugin's command run from one of the interface's own controls (a hotbar slot), as the
    /// player typing `/verb` would.
    Command {
        plugin: String,
        verb: String,
    },
    /// A press on the game's own map: no plugin's, told to every plugin listening.
    Map(dereth_client_contract::panels::map::MapClick),
}

impl Input {
    /// The window it was done on, `(plugin, window)`; none for the bar, a command or the map.
    #[must_use]
    pub fn window(&self) -> Option<(&str, &str)> {
        match self {
            Self::Click { plugin, window, .. }
            | Self::Toggle { plugin, window, .. }
            | Self::Select { plugin, window, .. }
            | Self::Close { plugin, window }
            | Self::Set { plugin, window, .. }
            | Self::Resize { plugin, window, .. } => Some((plugin, window)),
            Self::Bar { .. } | Self::Command { .. } | Self::Map(_) => None,
        }
    }
}

/// One thing on the markers' layer, in screen pixels: a square dot, or a line of text. The caller
/// works out where each marker falls (in the world view, on the radar, on the map) and hands the
/// layer these; the layer takes no mouse.
#[derive(Debug, Clone, PartialEq)]
pub enum Glyph {
    Dot {
        x: i32,
        y: i32,
        size: i32,
        /// `0xRRGGBB`.
        color: u32,
    },
    Text {
        x: i32,
        y: i32,
        text: String,
        color: u32,
    },
    /// Text standing at a place in the world, centred on `x`: `color` is `0xAARRGGBB`, so a label
    /// can be faded.
    Label {
        x: i32,
        y: i32,
        text: String,
        color: u32,
    },
}

/// How far in front of the world's surface at its anchor a label may be and still count as in
/// front of it, in the depth buffer's own units.
const DEPTH_SLACK: f32 = 1e-6;

/// Which of the world's labels the world hides, answered by the depth at each label's anchor.
///
/// A label is tested once, at its anchor pixel: shown whole when nothing nearer stands there,
/// hidden whole when something does. The depths come back a frame or two after they are asked
/// for, so each frame asks for the labels it has and shows what the newest answer said; a label
/// no answer has covered yet is not shown.
#[derive(Debug, Default)]
pub struct LabelOcclusion {
    next_tag: u64,
    /// The requests not yet answered: their tag, and each label's key and anchor depth.
    asked: std::collections::VecDeque<(u64, Vec<(String, f32)>)>,
    /// The newest answer: whether each label's anchor was in front of the world.
    seen: BTreeMap<String, bool>,
}

/// How many unanswered requests are remembered.
const ASKED: usize = 8;

impl LabelOcclusion {
    /// Remember the labels about to be probed, each by its key and its anchor's depth, and answer
    /// the tag to send with the probe.
    pub fn ask(&mut self, labels: Vec<(String, f32)>) -> u64 {
        self.next_tag += 1;
        self.asked.push_back((self.next_tag, labels));
        while self.asked.len() > ASKED {
            self.asked.pop_front();
        }
        self.next_tag
    }

    /// Take in the depths read at the anchors of the request `tag` (in the order asked), and drop
    /// any older request still waiting.
    pub fn answer(&mut self, tag: u64, depths: &[f32]) {
        while self.asked.front().is_some_and(|(t, _)| *t < tag) {
            self.asked.pop_front();
        }
        let Some((_, labels)) = self.asked.pop_front().filter(|(t, _)| *t == tag) else {
            return;
        };
        self.seen = labels
            .into_iter()
            .zip(depths)
            .map(|((key, anchor), world)| (key, anchor <= world + DEPTH_SLACK))
            .collect();
    }

    /// Whether the newest answer showed this label in front of the world.
    #[must_use]
    pub fn visible(&self, key: &str) -> bool {
        self.seen.get(key).copied().unwrap_or(false)
    }
}

/// A label that wants a place on screen: where its text is centred, its text, how far away its
/// place is, and whether it goes before every label of lower rank whatever their distance.
#[derive(Debug, Clone, PartialEq)]
pub struct LabelSpot {
    pub x: i32,
    pub y: i32,
    pub text: String,
    pub distance: f64,
    pub rank: u8,
}

/// How much of a label may lie under labels already placed before it is left out.
const COVERED: f64 = 0.35;

/// Which labels to show, and at what opacity, so a crowd stays readable: higher rank first, then
/// nearer first. A label mostly under ones already placed is left out; one only clipping them is
/// shown at half opacity. Returns `(index, alpha)` in drawing order, farthest first, so nearer
/// labels draw over farther ones.
#[must_use]
pub fn declutter(spots: &[LabelSpot]) -> Vec<(usize, u8)> {
    let mut order: Vec<usize> = (0..spots.len()).collect();
    order.sort_by(|&a, &b| {
        spots[b]
            .rank
            .cmp(&spots[a].rank)
            .then(spots[a].distance.total_cmp(&spots[b].distance))
    });
    let bounds = |s: &LabelSpot| {
        let w = chars(&s.text) * CELL_W + 4;
        (s.x - w / 2, s.y, s.x - w / 2 + w, s.y + LINE_H)
    };
    let mut placed: Vec<(i32, i32, i32, i32)> = Vec::new();
    let mut shown = Vec::new();
    for i in order {
        let b = bounds(&spots[i]);
        let area = f64::from((b.2 - b.0) * (b.3 - b.1)).max(1.0);
        let covered: f64 = placed
            .iter()
            .map(|p| {
                let w = (b.2.min(p.2) - b.0.max(p.0)).max(0);
                let h = (b.3.min(p.3) - b.1.max(p.1)).max(0);
                f64::from(w * h)
            })
            .sum();
        if covered / area > COVERED {
            continue;
        }
        placed.push(b);
        shown.push((i, if covered > 0.0 { 0x80 } else { 0xFF }));
    }
    shown.reverse();
    shown
}

/// The dots of a straight line from `a` to `b`, every `step` pixels: how a map line is drawn.
#[must_use]
pub fn line_glyphs(a: (i32, i32), b: (i32, i32), step: i32, color: u32) -> Vec<Glyph> {
    wide_line_glyphs(a, b, step, 2, color)
}

/// As [`line_glyphs`], with dots `size` pixels square: a shape's line in the world.
#[must_use]
pub fn wide_line_glyphs(
    a: (i32, i32),
    b: (i32, i32),
    step: i32,
    size: i32,
    color: u32,
) -> Vec<Glyph> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    // A line so long it runs far off the screen is left out rather than drawn dot by dot.
    if dx.abs().max(dy.abs()) > 8_000 {
        return Vec::new();
    }
    let n = (dx.abs().max(dy.abs()) / step.max(1)).max(1);
    (0..=n)
        .map(|i| Glyph::Dot {
            x: a.0 + dx * i / n,
            y: a.1 + dy * i / n,
            size,
            color,
        })
        .collect()
}

/// A direction indicator around `(cx, cy)`: a ring of sixteen dim dots with the one nearest
/// `turn` (degrees clockwise from straight ahead, which is up) lit, and a short spoke toward it.
#[must_use]
pub fn arrow_glyphs(cx: i32, cy: i32, radius: i32, turn: f64, color: u32) -> Vec<Glyph> {
    let mut out = Vec::new();
    let lit = to_i32_f64((turn.rem_euclid(360.0) / 22.5).round()).rem_euclid(16);
    for i in 0..16 {
        let a = f64::from(i) * 22.5_f64.to_radians();
        #[allow(clippy::cast_possible_truncation)]
        let (x, y) = (
            cx + to_i32_f64((math::sin(a) * f64::from(radius)).round()),
            cy - to_i32_f64((math::cos(a) * f64::from(radius)).round()),
        );
        out.push(Glyph::Dot {
            x,
            y,
            size: if i == lit { 5 } else { 2 },
            color: if i == lit { color } else { 0x60_60_60 },
        });
    }
    let a = turn.to_radians();
    for r in 1..radius - 3 {
        #[allow(clippy::cast_possible_truncation)]
        out.push(Glyph::Dot {
            x: cx + to_i32_f64((math::sin(a) * f64::from(r)).round()),
            y: cy - to_i32_f64((math::cos(a) * f64::from(r)).round()),
            size: 2,
            color,
        });
    }
    out
}

/// How many character cells `s` takes.
fn chars(s: &str) -> i32 {
    i32::try_from(s.chars().count()).unwrap_or(i32::MAX / CELL_W)
}

#[cfg(test)]
mod frame_tests;
#[cfg(test)]
mod host_tests;
#[cfg(test)]
mod kit_tests;
#[cfg(test)]
mod skin_tests;
#[cfg(test)]
mod tests;
