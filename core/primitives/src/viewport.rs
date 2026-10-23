//! The 3D viewport rectangle.
//!
//! Plain data, and here rather than in `dereth-render` because two of `dereth-client`'s logic modules
//! (`interaction` and `pick`) need the rectangle and nothing else the renderer owns.
//! `dereth_render::camera::Viewport` re-exports it, so every existing import keeps working.

/// A viewport rectangle in back-buffer pixels. `IDirect3DDevice9::SetViewport` also carries
/// `MinZ = 0.0`, `MaxZ = 1.0`, which the client never changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// A part of the back buffer that what is drawn into it takes for a whole screen of its own: a
/// view inside a tool's window, drawn by a renderer that knows nothing of the window around it.
///
/// `rect` is where the part is, in back-buffer pixels; `extent` is the size of the screen it
/// stands for. A rectangle of that screen lands in the same place of `rect`, scaled when the two
/// sizes differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Inset {
    pub rect: Viewport,
    pub extent: (u32, u32),
}

impl Inset {
    /// `v`, a rectangle of the screen this region stands for (cut to that screen first), as a
    /// rectangle of the back buffer.
    #[must_use]
    pub fn place(&self, v: Viewport) -> Viewport {
        let (ew, eh) = self.extent;
        let (x0, x1) = (v.x.min(ew), v.x.saturating_add(v.width).min(ew));
        let (y0, y1) = (v.y.min(eh), v.y.saturating_add(v.height).min(eh));
        let along = |at: u32, length: u32, n: u32, of: u32| -> u32 {
            let scaled = u64::from(n) * u64::from(length) / u64::from(of.max(1));
            at.saturating_add(u32::try_from(scaled).unwrap_or(u32::MAX))
        };
        let (left, right) = (
            along(self.rect.x, self.rect.width, x0, ew),
            along(self.rect.x, self.rect.width, x1, ew),
        );
        let (top, bottom) = (
            along(self.rect.y, self.rect.height, y0, eh),
            along(self.rect.y, self.rect.height, y1, eh),
        );
        Viewport {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        }
    }

    /// A point of the back buffer, in pixels, as a point of the screen this region stands for;
    /// `None` outside the region.
    #[must_use]
    pub fn point_inside(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        let r = self.rect;
        let (left, top) = (f64::from(r.x), f64::from(r.y));
        let (width, height) = (f64::from(r.width), f64::from(r.height));
        if width <= 0.0 || height <= 0.0 {
            return None;
        }
        let inside = x >= left && y >= top && x < left + width && y < top + height;
        inside.then(|| self.point_from(x, y))
    }

    /// A point of the back buffer, in pixels, as a point of the screen this region stands for,
    /// wherever it is.
    #[must_use]
    pub fn point_from(&self, x: f64, y: f64) -> (f64, f64) {
        let r = self.rect;
        let sx = f64::from(self.extent.0) / f64::from(r.width.max(1));
        let sy = f64::from(self.extent.1) / f64::from(r.height.max(1));
        ((x - f64::from(r.x)) * sx, (y - f64::from(r.y)) * sy)
    }

    /// A point of the screen this region stands for as a point of the back buffer, in pixels.
    #[must_use]
    pub fn point_to(&self, x: f64, y: f64) -> (f64, f64) {
        let r = self.rect;
        let sx = f64::from(r.width) / f64::from(self.extent.0.max(1));
        let sy = f64::from(r.height) / f64::from(self.extent.1.max(1));
        (f64::from(r.x) + x * sx, f64::from(r.y) + y * sy)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: a renderer's view drawn inside part of another window)
    use super::*;

    const fn vp(x: u32, y: u32, width: u32, height: u32) -> Viewport {
        Viewport {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn a_region_the_size_of_its_screen_moves_each_rectangle_by_its_corner() {
        let r = Inset {
            rect: vp(300, 40, 1000, 700),
            extent: (1000, 700),
        };
        assert_eq!(r.place(vp(0, 0, 1000, 700)), vp(300, 40, 1000, 700));
        assert_eq!(r.place(vp(10, 20, 30, 40)), vp(310, 60, 30, 40));
        assert_eq!(r.point_from(310.0, 60.0), (10.0, 20.0));
        assert_eq!(r.point_to(10.0, 20.0), (310.0, 60.0));
    }

    #[test]
    fn a_region_smaller_than_its_screen_scales_each_rectangle_into_it_and_cuts_what_lies_outside() {
        let r = Inset {
            rect: vp(100, 50, 400, 300),
            extent: (800, 600),
        };
        assert_eq!(r.place(vp(0, 0, 800, 600)), vp(100, 50, 400, 300));
        assert_eq!(r.place(vp(400, 300, 400, 300)), vp(300, 200, 200, 150));
        // A rectangle reaching past the screen is cut to it; one wholly past it is empty.
        assert_eq!(r.place(vp(600, 0, 400, 600)), vp(400, 50, 100, 300));
        assert_eq!(r.place(vp(900, 0, 10, 10)).width, 0);
        assert_eq!(r.point_inside(300.0, 200.0), Some((400.0, 300.0)));
        assert_eq!(r.point_inside(99.0, 200.0), None);
        assert_eq!(r.point_inside(500.0, 200.0), None);
    }
}
