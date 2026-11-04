//! Where an element is, as the game keeps it for a character: whether it shows and its corners in
//! the screen's clip space (from -1 to 1 across, 1 to -1 down), so a place kept at one size of
//! window lands in the same part of another.

use dereth_primitives::num::to_i32_f64;

use crate::system::{Handle, Ui};
use crate::token::{attr, Token};

/// A kept place: whether the element shows, and its upper left and lower right corners in clip
/// space.
// D2-13090, D2-13088
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Location {
    pub visible: bool,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

/// A new kept place: hidden, every corner at -3 (off the screen, whose clip space runs from -1 to
/// 1).
impl Default for Location {
    // D2-13089
    fn default() -> Self {
        Self {
            visible: false,
            x0: -3.0,
            y0: -3.0,
            x1: -3.0,
            y1: -3.0,
        }
    }
}

/// A pixel `(x, y)` of a screen `w` by `h` in clip space: twice it over the size less one, the
/// vertical turned over, each less one pixel's share.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn pixel_to_clip(x: i32, y: i32, w: i32, h: i32) -> (f32, f32) {
    let (xf, yf, wf, hf) = (f64::from(x), f64::from(y), f64::from(w), f64::from(h));
    let cx = ((xf + xf) / wf - 1.0) as f32;
    let cy = (-((yf + yf) / hf - 1.0)) as f32;
    let cx = (f64::from(cx) - 1.0 / wf) as f32;
    let cy = (f64::from(cy) - 1.0 / hf) as f32;
    (cx, cy)
}

/// A point in clip space as a pixel of a screen `w` by `h`: the point plus one pixel's share, plus
/// one, times half the size, cut toward zero (the vertical turned over).
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn clip_to_pixel(cx: f32, cy: f32, w: i32, h: i32) -> (i32, i32) {
    let (wf, hf) = (f64::from(w), f64::from(h));
    let x = to_i32_f64((1.0 / wf + f64::from(cx) + 1.0) * wf * 0.5);
    let y = to_i32_f64((1.0 - (1.0 / hf + f64::from(cy))) * hf * 0.5);
    (x, y)
}

impl Ui {
    /// Where an element whose place is kept is: whether it shows and its corners in clip space.
    /// `None` for an element whose place is not kept.
    // D2-11367
    #[must_use]
    pub fn location(&self, h: Handle) -> Option<Location> {
        let n = self.node(h)?;
        if !n.bool_attr(attr::SAVE_LOC) {
            return None;
        }
        let r = self.screen_rect(h)?;
        let (w, hh) = self.display();
        let (x0, y0) = pixel_to_clip(r.x0, r.y0, w, hh);
        let (x1, y1) = pixel_to_clip(r.x1, r.y1, w, hh);
        Some(Location {
            visible: n.visible,
            x0,
            y0,
            x1,
            y1,
        })
    }

    /// Whether an element's place is kept.
    // D2-11368
    #[must_use]
    pub fn keeps_location(&self, h: Handle) -> bool {
        self.node(h).is_some_and(|n| n.bool_attr(attr::SAVE_LOC))
    }

    /// Put an element whose place is kept where a kept place says: shown or not, moved to its
    /// upper left corner, resized to it when the element may be resized; false for an element
    /// whose place is not kept.
    // D2-11369
    pub fn apply_location(&mut self, h: Handle, l: &Location) -> bool {
        if !self.keeps_location(h) {
            return false;
        }
        let (w, hh) = self.display();
        let (x0, y0) = clip_to_pixel(l.x0, l.y0, w, hh);
        let (x1, y1) = clip_to_pixel(l.x1, l.y1, w, hh);
        self.set_visible(h, l.visible);
        self.move_to(h, x0, y0);
        if self
            .node(h)
            .is_some_and(|n| n.bool_attr(Token::of("allow_resize")))
        {
            self.resize_to(h, x1 - x0 + 1, y1 - y0 + 1);
        }
        self.move_to(h, x0, y0);
        true
    }
}
