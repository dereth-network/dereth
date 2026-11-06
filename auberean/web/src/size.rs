//! The picture's size in a page: the page's own pixels at the render scale, never smaller than
//! 800 by 600, in the page's shape.

/// The picture for a page of `page` device pixels, drawing `scale` percent of them. A page or a
/// scale that would make it smaller than 800 by 600 is drawn at that size in the page's shape, and
/// the page scales it down to fit.
#[must_use]
pub fn drawn(page: (u32, u32), scale: u32) -> (u32, u32) {
    const LEAST: (u32, u32) = (800, 600);
    let (w, h) = (u64::from(page.0.max(1)), u64::from(page.1.max(1)));
    let scale = u64::from(scale.clamp(1, 100));
    let (mut dw, mut dh) = ((w * scale / 100).max(1), (h * scale / 100).max(1));
    if dw < u64::from(LEAST.0) || dh < u64::from(LEAST.1) {
        // The smallest size in the page's shape that holds 800 by 600.
        let grow = (u64::from(LEAST.0) * h).max(u64::from(LEAST.1) * w);
        dw = (w * grow).div_ceil(w * h);
        dh = (h * grow).div_ceil(w * h);
    }
    (
        u32::try_from(dw).unwrap_or(u32::MAX),
        u32::try_from(dh).unwrap_or(u32::MAX),
    )
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (how a browser page shows the picture: retail has no page)
    use super::*;

    #[test]
    fn a_large_page_is_drawn_at_its_own_pixels_and_at_its_render_scale() {
        assert_eq!(drawn((2560, 1440), 100), (2560, 1440));
        assert_eq!(drawn((2560, 1440), 50), (1280, 720));
    }

    #[test]
    fn a_small_page_or_a_low_scale_is_drawn_at_800_by_600_or_more_in_the_page_s_shape() {
        assert_eq!(drawn((1600, 1200), 50), (800, 600));
        let (w, h) = drawn((1000, 500), 50);
        assert!(w >= 800 && h >= 600, "{w} by {h}");
        assert_eq!(w, h * 2);
        let (w, h) = drawn((400, 800), 100);
        assert!(w >= 800 && h >= 600, "{w} by {h}");
        assert_eq!(h, w * 2);
    }
}
