//! A resolution change keeps the window's top-left corner (client divergence CD-001): only the
//! size changes, and the window moves only as far as it must to stay inside the work area of its
//! monitor. Leaving full screen puts it back at the top-left it had before full screen, and full
//! screen is never kept on top (CD-003).
//!
//! Fixture: the placement function over synthetic monitor metrics; no device, no dats, no window.
//! The same rule on a running headless App is the gpu tier's `presentation::window_position`.
//!
//! # Retail presentation-change behavior
//!
//! After restarting rendering, the original client reread the resulting width, height and
//! fullscreen state. Fullscreen used position (0, 0) and that presentation's extent. Windowed
//! extent included decorations: `cx = Width + 2*cxDlgFrame` and
//! `cy = Height + cyCaption + 2*cyDlgFrame`. Leaving fullscreen centered it on the screen.
//! Otherwise a successful window-rectangle query preserved the old center via
//! `x = (left + right - cx)/2`, `y = (top + bottom - cy)/2`; query failure left (0, 0).
//!
//! A failed work-area query skipped the original clamp. With an area, an oversized window used
//! `work.left - cxDlgFrame` horizontally; the vertical clamp likewise split oversized and
//! fitting extents.
//! Otherwise the far-edge arm used `x = (work.right - work.left) - cx`, then enforced
//! `x >= work.left`, with the analogous vertical rule. That width-as-right-edge expression is
//! wrong for a nonzero monitor origin. Final placement selected topmost for fullscreen and
//! not-topmost for windowed, with the public `SWP_NOCOPYBITS` flag (0x100).
//!
//! For 800x600 -> 1280x720 on a 1920x1080 screen with frame metrics 3/23/3, outer sizes are
//! 806x629 -> 1286x749. Starting at (100, 100), the original center-preserving formula produces
//! (-140, 40), then clamps to (0, 40). It preserves the window's center, not the screen center.
//! Creation placement through `window_proc::placement` would produce (317, 166). Both differ from
//! the kept top-left (100, 100).
//!
//! # Current clamp and coverage
//!
//! `window_proc::change_presentation_placement` preserves an existing windowed top-left, then
//! applies the minimum correction needed to fit a fitting extent inside the reported work area,
//! or the monitor rectangle when no work area is supplied:
//!
//! ```text
//! if (x + cx > area.right)  x = area.right - cx
//! if (x < area.left)        x = area.left
//! if (y + cy > area.bottom) y = area.bottom - cy
//! if (y < area.top)         y = area.top
//! ```
//!
//! Left/top checks win for an oversized extent, keeping its top-left at the area's origin while
//! it extends past the far edges. This preserves access near the caption's left edge; it does not
//! prove that a close button at the far right remains onscreen. Current Windows hosts can supply
//! the selected monitor's work area; the headless host and platforms without that query use None.
//!
//! Leaving fullscreen keeps the top-left of the rectangle the window had before it went full
//! screen, which the caller remembers; lacking any rectangle uses creation centering before this
//! clamp. Fullscreen delegates to the current borderless monitor placement, including CD-003's
//! not-topmost rule.

use dereth_render::window_proc::{change_presentation_placement, placement, Rect, ScreenMetrics};

/// Synthetic 1920x1080 primary monitor, 40 px bottom taskbar and frame metrics 3/23/3.
/// These are supplied inputs, not measurements from the running desktop.
fn primary() -> ScreenMetrics {
    ScreenMetrics {
        cx_screen: 1920,
        cy_screen: 1080,
        cx_dlg_frame: 3,
        cy_caption: 23,
        cy_dlg_frame: 3,
        work_area: Some(Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        }),
        origin: (0, 0),
    }
}

/// The outer rectangle of a `w` x `h` client window whose top-left is `(x, y)`, with
/// [`primary`]'s frame.
fn at(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect {
        left: x,
        top: y,
        right: x + w + 6,
        bottom: y + h + 29,
    }
}

// =============================================================================================
// 1. A size change keeps the top-left
// =============================================================================================

/// Behaviour: presentation.window.a-resolution-change-keeps-the-windows-top-left
///
/// 800x600 -> 1280x720 with the window at `(100, 100)`: the extent changes and the
/// position does not.
///
/// The two readings we are NOT taking are asserted by name, because "it is not centred" is only
/// meaningful next to the two centres it could have been.
#[test]
fn a_size_change_keeps_the_windows_top_left() {
    let m = primary();
    let before = at(100, 100, 800, 600);
    assert_eq!(
        (before.right - before.left, before.bottom - before.top),
        (806, 629)
    );

    let p = change_presentation_placement(false, true, 1280, 720, Some(before), &m);
    assert_eq!(
        (p.cx, p.cy),
        (1286, 749),
        "cx = Width + 2*cxDlgFrame, cy = Height + cyCaption + 2*cyDlgFrame"
    );
    assert_eq!(
        (p.x, p.y),
        (100, 100),
        "client divergence CD-001: the top-left is kept"
    );

    // Retail's own answer: the window's current centre, `(503, 414)`, minus half the new extent,
    // then its work-area clamp. It moves the window, which CD-001 rules out.
    assert_eq!(
        (
            (before.left + before.right - p.cx) / 2,
            (before.top + before.bottom - p.cy) / 2
        ),
        (-140, 40)
    );
    assert_ne!(
        (p.x, p.y),
        (0, 40),
        "the original center-preserving rectangle, clamped"
    );

    // The creation-centered `window_proc::placement` answer is a second control, beside the
    // original center-preserving one.
    let old = placement(false, true, 1280, 720, &m);
    assert_eq!(
        (old.x, old.y),
        (317, 166),
        "the creation-time screen center"
    );
    assert_ne!(
        (p.x, p.y),
        (old.x, old.y),
        "the kept top-left differs from the creation-time centre"
    );

    // Reduce width but increase height: another extent change must retain the top-left too.
    let after = Rect {
        left: p.x,
        top: p.y,
        right: p.x + p.cx,
        bottom: p.y + p.cy,
    };
    let back = change_presentation_placement(false, true, 1024, 768, Some(after), &m);
    assert_eq!((back.cx, back.cy), (1030, 797));
    assert_eq!(
        (back.x, back.y),
        (100, 100),
        "the narrower, taller extent does not re-center either"
    );
    assert_ne!(
        (back.x, back.y),
        (
            placement(false, true, 1024, 768, &m).x,
            placement(false, true, 1024, 768, &m).y
        )
    );
}

/// The style and the Z order are the two things a size change must not disturb: CD-003's
/// not-topmost rule and CD-002's borderless full screen both live in the same [`placement`] the
/// change function delegates to.
#[test]
fn the_style_and_the_z_order_are_the_ones_placement_already_answered() {
    let m = primary();
    for (full, keep) in [
        (false, None),
        (false, Some(at(10, 10, 800, 600))),
        (true, None),
        (true, Some(at(10, 10, 800, 600))),
    ] {
        let p = change_presentation_placement(full, true, 1024, 768, keep, &m);
        let base = placement(full, true, 1024, 768, &m);
        assert_eq!(p.style, base.style, "full={full} keep={keep:?}");
        assert!(!p.topmost, "CD-003 (never topmost) holds beside CD-001");
        assert_eq!((p.cx, p.cy), (base.cx, base.cy), "only x and y ever differ");
    }
}

// =============================================================================================
// 2. The off-screen rule
// =============================================================================================

/// A window near the bottom-right corner, grown: the smallest move that puts it back inside the
/// work area, and no more.
#[test]
fn a_new_extent_that_runs_off_the_screen_is_pushed_back_the_minimum() {
    let m = primary();
    let before = at(1500, 900, 800, 600);
    let p = change_presentation_placement(false, true, 1280, 720, Some(before), &m);
    assert_eq!((p.cx, p.cy), (1286, 749));
    assert_eq!(p.x, 1920 - 1286, "x = area.right - cx");
    assert_eq!(
        p.y,
        1040 - 749,
        "y = area.bottom - cy -- the work area, so the task bar is spared"
    );
    assert_eq!((p.x, p.y), (634, 291));
    assert!(p.x + p.cx <= 1920 && p.y + p.cy <= 1040, "fully inside");
    // It moved because it had to, not because a size changed: it moved the minimum.
    assert!(p.x < 1500 && p.y < 900);
    assert_ne!(
        (p.x, p.y),
        (
            placement(false, true, 1280, 720, &m).x,
            placement(false, true, 1280, 720, &m).y
        )
    );
}

/// Oversized extent: left/top checks win and the far edges overflow. The assertions pin the
/// computed rectangle, not reachability of a close button or any other real desktop control.
#[test]
fn a_window_larger_than_the_work_area_is_pinned_to_its_top_left() {
    let m = primary();
    let p =
        change_presentation_placement(false, true, 1920, 1080, Some(at(100, 100, 800, 600)), &m);
    assert_eq!(
        (p.cx, p.cy),
        (1926, 1109),
        "the frame makes it larger than the monitor"
    );
    assert_eq!(
        (p.x, p.y),
        (0, 0),
        "area.left / area.top, because they are tested last"
    );
    assert!(
        p.x + p.cx > 1920 && p.y + p.cy > 1040,
        "it bleeds off the far edge, by design"
    );
    // Retail's first arm answers `work.left - cxDlgFrame` here, i.e. -3: it bleeds off the NEAR
    // edge by the frame width. That is a divergence of three pixels and it is stated, not hidden.
    assert_ne!(p.x, -3);
}

/// Without a reported work area, clamp to the monitor rectangle. This covers the headless
/// host's fallback; it is no longer a claim that every production host lacks work-area data.
/// A fitting window remains unmoved, the negative control for the clamp.
#[test]
fn with_no_work_area_the_clamp_is_the_monitors_own_rectangle() {
    let m = ScreenMetrics {
        work_area: None,
        ..primary()
    };
    // Fits: untouched.
    let p = change_presentation_placement(false, true, 1024, 768, Some(at(200, 150, 800, 600)), &m);
    assert_eq!((p.x, p.y), (200, 150), "nothing to clamp, so nothing moves");

    // Past the bottom edge, and now the task bar is not spared because nothing reported one.
    let q =
        change_presentation_placement(false, true, 1280, 1000, Some(at(200, 150, 800, 600)), &m);
    assert_eq!((q.cx, q.cy), (1286, 1029));
    assert_eq!(
        (q.x, q.y),
        (200, 1080 - 1029),
        "y = cy_screen - cy, not work.bottom - cy"
    );
    assert_eq!(q.y, 51);
}

/// A nonzero monitor origin exposes the original width-as-right-edge mistake. The fallback
/// clamp uses the selected monitor's absolute right/bottom coordinates instead.
#[test]
fn the_clamp_is_in_the_monitors_own_coordinate_space() {
    let m = ScreenMetrics {
        cx_screen: 2560,
        cy_screen: 1440,
        origin: (1920, -120),
        work_area: None,
        ..primary()
    };
    let before = at(2000, 0, 800, 600);
    let p = change_presentation_placement(false, true, 1280, 720, Some(before), &m);
    assert_eq!(
        (p.x, p.y),
        (2000, 0),
        "on the second monitor, and it stays there"
    );

    // Retail's `x = (work.right - work.left) - cx` is a width where a right edge belongs: with a
    // work area starting at 1920 it answers 2560 - 1286 = 1274, which is on the OTHER monitor.
    let pushed =
        change_presentation_placement(false, true, 1280, 720, Some(at(3200, 900, 800, 600)), &m);
    assert_eq!(
        pushed.x,
        1920 + 2560 - 1286,
        "area.right - cx, in the monitor's own space"
    );
    assert_eq!(pushed.x, 3194);
    assert!(
        pushed.x >= 1920,
        "it did not fall back onto the primary monitor"
    );
    assert_eq!(pushed.y, -120 + 1440 - 749);
}

// =============================================================================================
// 3. Leaving full screen, and the arms that are NOT the divergence
// =============================================================================================

/// Behaviour: presentation.window.leaving-full-screen-puts-the-window-back-where-it-was
///
/// Leaving full screen keeps the top-left the window had before it went full screen, the same
/// rule as every other resolution change (client divergence CD-001): the caller hands in the
/// remembered windowed rectangle, and only the extent is new. Retail centres the window on the
/// screen instead, and the monitor's own top-left, the rectangle the window has while full
/// screen, is not a windowed position at all.
#[test]
fn leaving_full_screen_keeps_the_top_left_it_had_before() {
    let m = primary();
    let before_full_screen = at(100, 150, 1024, 768);
    let p = change_presentation_placement(false, true, 800, 600, Some(before_full_screen), &m);
    assert_eq!((p.cx, p.cy), (806, 629));
    assert_eq!(
        (p.x, p.y),
        (100, 150),
        "the top-left from before full screen is kept"
    );
    assert_ne!(
        (p.x, p.y),
        (1920 / 2 - 806 / 2, 1080 / 2 - 629 / 2),
        "NOT retail's screen centre"
    );
    assert_ne!((p.x, p.y), (557, 226));
    assert_ne!(
        (p.x, p.y),
        (0, 0),
        "NOT the monitor's top-left, which is where the full-screen window was"
    );

    // The remembered position is still held to the work area: a window remembered near the
    // bottom-right that comes back larger is pushed in by the minimum, as on any other change.
    let q =
        change_presentation_placement(false, true, 1280, 720, Some(at(1500, 900, 800, 600)), &m);
    assert_eq!((q.x, q.y), (1920 - 1286, 1040 - 749));
}

/// With no current rectangle, the original branch seeded (0, 0). The current helper instead
/// falls back to creation centering and clamps it. This is an explicit additional divergence.
#[test]
fn no_current_rectangle_falls_back_to_the_centre() {
    let m = primary();
    let p = change_presentation_placement(false, true, 800, 600, None, &m);
    assert_eq!((p.x, p.y), (557, 226));
    assert_ne!(
        (p.x, p.y),
        (0, 0),
        "the missing-rectangle fallback differs from the original zero position"
    );
}

/// Full screen is the borderless arm (CD-002) untouched: the monitor's whole rectangle, whatever the window was
/// doing and wherever it was.
#[test]
fn full_screen_is_the_monitor_whatever_the_window_was_doing() {
    let m = primary();
    let base = placement(true, true, 1024, 768, &m);
    for cur in [None, Some(at(700, 400, 800, 600))] {
        let p = change_presentation_placement(true, true, 1024, 768, cur, &m);
        assert_eq!(p, base, "cur={cur:?}");
        assert_eq!((p.x, p.y, p.cx, p.cy), (0, 0, 1920, 1080));
    }
}
