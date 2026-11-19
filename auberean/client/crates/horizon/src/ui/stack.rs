//! The windows' stacking: which of the windows the player opens is drawn over which.
//!
//! A window opened comes to the front, over every window open before it, whatever opened it (its
//! key, a press in another window, the game); a press on a window brings it to the front before
//! anything sees the press. The settings and the game's question stay over them all.
//!
//! The windows are translucent over the world. Where a window lies over another, a backing at
//! least [`OVER_WINDOW`] opaque is drawn under it first, so what the window below says does not
//! show through what the window over it says.

use crate::draw::{DrawList, Quad, Rect};

/// The windows that stack, in the order they are drawn until one is opened or pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Window {
    Skills,
    Examine,
    Social,
    Map,
    Journal,
    Book,
    /// The windows of the player's things together.
    Goods,
    Crafting,
    Making,
}

impl Window {
    pub const ALL: [Self; 9] = [
        Self::Skills,
        Self::Examine,
        Self::Social,
        Self::Map,
        Self::Journal,
        Self::Book,
        Self::Goods,
        Self::Crafting,
        Self::Making,
    ];
}

/// How opaque a window's backing is at least where it lies over another window.
pub const OVER_WINDOW: f32 = 0.95;

/// The backing's alpha: the first byte at or over [`OVER_WINDOW`] (243 of 255).
const BACKING_ALPHA: u32 = 0xF3;

/// The windows' order, the bottom first, and how much of each was open last frame.
#[derive(Debug)]
pub struct Stack {
    order: Vec<Window>,
    shown: Vec<(Window, usize)>,
    /// The windows drawn this frame, in the order drawn, each with its quads in the draw list.
    pub drawn: Vec<(Window, std::ops::Range<usize>)>,
}

impl Default for Stack {
    fn default() -> Self {
        Self {
            order: Window::ALL.to_vec(),
            shown: Vec::new(),
            drawn: Vec::new(),
        }
    }
}

impl Stack {
    /// The windows, the bottom first.
    #[must_use]
    pub fn order(&self) -> &[Window] {
        &self.order
    }

    /// Bring `w` to the front.
    pub fn raise(&mut self, w: Window) {
        self.order.retain(|o| *o != w);
        self.order.push(w);
    }

    /// The windows as open now, each with how many of its parts are open (the windows of things
    /// count each of theirs): one more open than last frame comes to the front, several in their
    /// own order.
    pub fn follow(&mut self, shown: &[(Window, usize)]) {
        let before = std::mem::take(&mut self.shown);
        let was = |w: Window| before.iter().find(|(o, _)| *o == w).map_or(0, |(_, n)| *n);
        let order = self.order.clone();
        for w in order {
            let now = shown.iter().find(|(o, _)| *o == w).map_or(0, |(_, n)| *n);
            if now > was(w) {
                self.raise(w);
            }
        }
        self.shown = shown.to_vec();
    }

    /// The window a press at `(x, y)` falls on, of those drawn at `rects` last frame: the front
    /// one there.
    #[must_use]
    pub fn pressed_on(&self, rects: &[(Window, Vec<Rect>)], (x, y): (f32, f32)) -> Option<Window> {
        self.order.iter().rev().copied().find(|w| {
            rects
                .iter()
                .filter(|(o, _)| o == w)
                .any(|(_, rs)| rs.iter().any(|r| r.contains(x, y)))
        })
    }
}

/// Back a window drawn at `window`, whose quads begin at `from` in `list`, where it lies over any
/// of `below`: a fill of `colour`, at least [`OVER_WINDOW`] opaque, under it there, put in before
/// its quads.
pub fn back(list: &mut DrawList, from: usize, window: Rect, below: &[Rect], colour: u32) {
    let fills: Vec<Quad> = below
        .iter()
        .filter_map(|b| window.intersect(b))
        .map(|r| Quad {
            tex: None,
            dst: r,
            src: r,
            colour: (colour & 0x00FF_FFFF) | (BACKING_ALPHA << 24),
            clip: None,
            turn: 0.0,
            depth: None,
        })
        .collect();
    if fills.is_empty() {
        return;
    }
    let n = fills.len();
    let from = from.min(list.quads.len());
    list.quads.splice(from..from, fills);
    for a in &mut list.anchored {
        if a.quads.start >= from {
            a.quads = a.quads.start + n..a.quads.end + n;
        }
    }
}
