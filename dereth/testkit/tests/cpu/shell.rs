//! The part of the shell that is arithmetic rather than a shipped layout: where the client puts its
//! window (a size, a position and a set of frame bits worked out from what the desktop reports
//! about itself), and where a press lands in the view. The placement function is pure, every
//! `ScreenMetrics` below is one the scenario made up, and **no real monitor is ever read**, so
//! these hold on any machine.
//!
//! The shipped key map, option page and wizard, and the claims that need a running client, read the
//! retail data and are in `tests/dat/shell.rs`. `ALL` is this file's own list, concatenated with
//! the other subjects' in `census.rs`.

use dereth_testkit::HeadlessClient;
use {
    dereth_client_contract::window_proc::placement, dereth_client_contract::window_proc::Placement,
    dereth_client_contract::window_proc::Rect, dereth_client_contract::window_proc::ScreenMetrics,
    dereth_client_contract::window_proc::WS_POPUP,
};

// The desktop's own frame bits, by the names a player would use for them. They are the host's
// numbers and not this client's, which is why they are written out here rather than imported: a
// caption bar, a system menu, a minimise button, a resize grip and a maximise button.
const CAPTION_BAR: u32 = 0x00C0_0000;
const SYSTEM_MENU: u32 = 0x0008_0000;
const MINIMISE_BUTTON: u32 = 0x0002_0000;
const RESIZE_GRIP: u32 = 0x0004_0000;
const MAXIMISE_BUTTON: u32 = 0x0001_0000;
const ON_SCREEN: u32 = 0x1000_0000;

/// A 1920x1080 primary monitor with a 40-pixel task bar at the bottom, and the desktop's usual
/// frame. Made up, like every other set of metrics in this file.
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

/// A second monitor to the right of the primary one and a little above it, which is the arm the
/// monitor's own origin exists for.
fn secondary() -> ScreenMetrics {
    ScreenMetrics {
        cx_screen: 2560,
        cy_screen: 1440,
        origin: (1920, -120),
        work_area: None,
        ..primary()
    }
}

// =============================================================================================
// window.full-screen.windowed-has-a-frame-and-full-screen-has-none
// =============================================================================================

/// The frame bits, by what a player can do with each of them.
///
/// The two whole style words are not asserted as literals beside the client's own constants for
/// them, which would be a transcription; what is asserted here is the decomposition, which is the
/// part a player sees -- a caption to drag, a menu, a minimise button, no resize grip.
pub fn a_window_has_its_frame_and_a_full_screen_one_has_none() {
    let w = placement(false, true, 800, 600, &primary());
    let windowed = w.style & CAPTION_BAR == CAPTION_BAR
        && w.style & SYSTEM_MENU == SYSTEM_MENU
        && w.style & MINIMISE_BUTTON == MINIMISE_BUTTON
        && w.style & (RESIZE_GRIP | MAXIMISE_BUTTON) == 0
        && w.style & WS_POPUP == 0;

    let f = placement(true, true, 800, 600, &primary());
    let full = f.style & WS_POPUP == WS_POPUP
        && f.style & (CAPTION_BAR | SYSTEM_MENU | MINIMISE_BUTTON) == 0;

    // Asked for, or not shown: the one bit that says "put this on the screen" is the only thing
    // being visible adds, in either mode.
    let shown = w.style & ON_SCREEN == ON_SCREEN
        && f.style & ON_SCREEN == ON_SCREEN
        && placement(false, false, 800, 600, &primary()).style & ON_SCREEN == 0
        && placement(true, false, 800, 600, &primary()).style & ON_SCREEN == 0;

    // And the two are not the same window: a round-trip guard folded in here rather than kept
    // as a row of its own.
    let different: bool = {
        let m = primary();
        let a: Placement = placement(false, true, 1920, 1080, &m);
        let b: Placement = placement(true, true, 1920, 1080, &m);
        a != b && a.style != b.style && (a.x, a.y, a.cx, a.cy) != (b.x, b.y, b.cx, b.cy)
    };

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "window.full-screen.windowed-has-a-frame-and-full-screen-has-none",
        move |_| windowed && full && shown && different,
    );
}

dereth_testkit::scenarios! {
    scenario_a_window_has_its_frame_and_a_full_screen_one_has_none => a_window_has_its_frame_and_a_full_screen_one_has_none ["window.full-screen.windowed-has-a-frame-and-full-screen-has-none"],
    scenario_a_window_is_the_asked_for_picture_plus_its_frame_centred => a_window_is_the_asked_for_picture_plus_its_frame_centred ["window.full-screen.windowed-is-the-picture-the-player-asked-for-plus-its-frame-centred"],
    scenario_a_window_too_big_for_the_desktop_is_pulled_back_onto_it => a_window_too_big_for_the_desktop_is_pulled_back_onto_it ["window.full-screen.a-window-too-big-for-the-desktop-is-pulled-back-onto-it"],
    scenario_full_screen_fills_the_monitor_the_window_is_on => full_screen_fills_the_monitor_the_window_is_on ["window.full-screen.it-fills-the-monitor-the-window-is-on-and-not-the-resolution-that-was-asked-for"],
    scenario_full_screen_never_asks_to_float_over_everything_else => full_screen_never_asks_to_float_over_everything_else ["window.full-screen.it-never-floats-over-the-players-other-windows"],
    scenario_a_press_outside_the_view_is_refused_though_it_is_inside_the_window => a_press_outside_the_view_is_refused_though_it_is_inside_the_window ["viewport.a-press-outside-the-view-is-refused-though-it-is-inside-the-window"],
    scenario_the_armed_point_is_measured_from_the_views_own_corner => the_armed_point_is_measured_from_the_views_own_corner ["viewport.the-armed-point-is-measured-from-the-views-own-corner"],
    scenario_a_world_press_is_measured_against_the_view_the_frame_pushed_in => a_world_press_is_measured_against_the_view_the_frame_pushed_in ["viewport.a-world-press-is-measured-against-the-view-the-frame-pushed-in"],
}

// =============================================================================================
// window.full-screen.windowed-is-the-picture-the-player-asked-for-plus-its-frame-centred
// =============================================================================================

/// 800x600 of picture inside the desktop's frame is an 806x629 window, centred.
///
/// That outer size is the number the project's own runtime validation measured the original
/// client reporting, which is why this scenario uses those metrics.
pub fn a_window_is_the_asked_for_picture_plus_its_frame_centred() {
    let p = placement(false, true, 800, 600, &primary());
    let outer = (p.cx, p.cy) == (806, 629);
    let centred =
        p.x == 1920 / 2 - 806 / 2 && p.y == 1080 / 2 - 629 / 2 && (p.x, p.y) == (557, 226);

    // On another monitor it centres on *that* one, not on the desktop's origin.
    let elsewhere = {
        let w = placement(false, true, 800, 600, &secondary());
        w.x == 1920 + 2560 / 2 - 806 / 2 && w.y == -120 + 1440 / 2 - 629 / 2
    };

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "window.full-screen.windowed-is-the-picture-the-player-asked-for-plus-its-frame-centred",
        move |_| outer && centred && elsewhere,
    );
}

// =============================================================================================
// window.full-screen.a-window-too-big-for-the-desktop-is-pulled-back-onto-it
// =============================================================================================

/// The three arms of the clamp, and the fourth case where there is nothing to clamp against.
pub fn a_window_too_big_for_the_desktop_is_pulled_back_onto_it() {
    // Wider than the usable desktop: flush against its left edge, with only the frame overhanging.
    let narrow = ScreenMetrics {
        work_area: Some(Rect {
            left: 100,
            top: 0,
            right: 700,
            bottom: 1040,
        }),
        ..primary()
    };
    let too_wide = placement(false, true, 800, 600, &narrow).x == 100 - 3;

    // Past the right-hand edge: pulled back by the width of the usable desktop, which is **not**
    // the same as pulling it back to that edge once the desktop does not start at zero. The
    // distinction is kept, because the two differ by the left inset and a reader would assume
    // the other one.
    let offset = ScreenMetrics {
        work_area: Some(Rect {
            left: 100,
            top: 0,
            right: 1300,
            bottom: 1040,
        }),
        ..primary()
    };
    let past_the_edge = {
        let p = placement(false, true, 800, 600, &offset);
        // (1300 - 100) - 806 is 394.
        p.x == (1300 - 100) - 806 && p.x != 1300 - 806 && p.x >= 100
    };

    // And when that pull lands left of the usable desktop, it is pulled back to its left edge.
    let tight = ScreenMetrics {
        work_area: Some(Rect {
            left: 200,
            top: 0,
            right: 1200,
            bottom: 1040,
        }),
        ..primary()
    };
    let pulled_back = placement(false, true, 800, 600, &tight).x == 200;

    // Taller than the usable desktop: flush against its top.
    let short = ScreenMetrics {
        work_area: Some(Rect {
            left: 0,
            top: 60,
            right: 1920,
            bottom: 500,
        }),
        ..primary()
    };
    let too_tall = placement(false, true, 800, 600, &short).y == 60;

    // A desktop that will not say where its usable part is: the centred position stands, which is
    // what a host with no such notion gets.
    let none = ScreenMetrics {
        work_area: None,
        ..narrow
    };
    let unclamped = placement(false, true, 800, 600, &none).x == 1920 / 2 - 806 / 2;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "window.full-screen.a-window-too-big-for-the-desktop-is-pulled-back-onto-it",
        move |_| too_wide && past_the_edge && pulled_back && too_tall && unclamped,
    );
}

// =============================================================================================
// window.full-screen.it-fills-the-monitor-the-window-is-on-and-not-the-resolution-that-was-asked-for
// =============================================================================================

/// A deliberate divergence from retail: full screen is a borderless window over the monitor, so
/// it is the monitor's own rectangle and never the requested resolution.
pub fn full_screen_fills_the_monitor_the_window_is_on() {
    let m = primary();
    // A deliberately *smaller* requested resolution, so a client that used it -- which is what a
    // real display-mode switch would do -- is caught here.
    let p = placement(true, true, 1024, 768, &m);
    let fills = (p.x, p.y, p.cx, p.cy) == (0, 0, 1920, 1080)
        && (p.cx, p.cy) != (1024, 768)
        && p.style & WS_POPUP == WS_POPUP;

    // The task bar is covered rather than avoided: the clamp is a windowed-only arm.
    let covers_the_task_bar = {
        let clamped = ScreenMetrics {
            work_area: Some(Rect {
                left: 0,
                top: 0,
                right: 900,
                bottom: 500,
            }),
            ..m
        };
        placement(true, true, 1024, 768, &clamped) == p
    };

    // The monitor the window is on, not the primary one.
    let second = {
        let q = placement(true, true, 800, 600, &secondary());
        (q.x, q.y, q.cx, q.cy) == (1920, -120, 2560, 1440)
    };

    let mut c = HeadlessClient::model();
    c.assert_behaviour("window.full-screen.it-fills-the-monitor-the-window-is-on-and-not-the-resolution-that-was-asked-for", move |_| {
        fills && covers_the_task_bar && second
    });
}

// =============================================================================================
// window.full-screen.it-never-floats-over-the-players-other-windows
// =============================================================================================

/// Every shape of desktop the placement has an arm for, and none of them asks to float.
///
/// That nothing *but* this moved in the full-screen arm is not a row of its own: it is the two
/// scenarios above, asserted from the other side, and a third copy of the same numbers would not
/// add a claim.
pub fn full_screen_never_asks_to_float_over_everything_else() {
    let no_work_area = ScreenMetrics {
        work_area: None,
        ..primary()
    };
    let tiny_work_area = ScreenMetrics {
        work_area: Some(Rect {
            left: 0,
            top: 0,
            right: 640,
            bottom: 480,
        }),
        ..primary()
    };
    let metrics = [primary(), secondary(), no_work_area, tiny_work_area];

    let mut checked = 0_usize;
    let mut floated = false;
    for m in &metrics {
        for full in [false, true] {
            for visible in [false, true] {
                for (w, h) in [(800, 600), (1024, 768), (1920, 1080), (3840, 2160)] {
                    floated |= placement(full, visible, w, h, m).topmost;
                    checked += 1;
                }
            }
        }
    }
    let whole_matrix = checked == 4 * 2 * 2 * 4;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "window.full-screen.it-never-floats-over-the-players-other-windows",
        move |_| whole_matrix && !floated,
    );
}

// =============================================================================================
// viewport.* -- the press is measured against the view the player looks through
//
// Five rows: three here, which are arithmetic over a rectangle and need no data file at all, and
// two in the `dat` tier beside them, which need the shipped geometry and a whole client. The two
// controls -- the window's centre missing when the view is moved, and the default full-screen
// view being unchanged -- are arms of the two dat rows rather than rows of their own.
// =============================================================================================

use dereth_primitives::viewport::Viewport;

/// The window, throughout. Every press below is given in **window** coordinates, which is what
/// the client is handed.
const WINDOW: (u32, u32) = (800, 600);

/// The whole window as a view: the rectangle everything used to assume.
const WHOLE: Viewport = Viewport {
    x: 0,
    y: 0,
    width: WINDOW.0,
    height: WINDOW.1,
};

/// A moved and resized view, deliberately **off-centre**, so that a client that centred the view
/// rather than placing it, or that used its size and not its corner, fails here rather than
/// coincidentally agreeing.
const INSET: Viewport = Viewport {
    x: 240,
    y: 90,
    width: 400,
    height: 300,
};

/// The inset view's own middle, in window coordinates. The middle of a 400x300 view is 199 across
/// and 149 down, the same whole pixel that 399/299 is for an 800x600 one.
const INSET_MIDDLE: (i32, i32) = (240 + 199, 90 + 149);

/// One press on the world, over no part of the interface.
fn a_world_press(x: i32, y: i32) -> dereth_client_shell::ui::UiMouseEvent {
    dereth_client_shell::ui::UiMouseEvent {
        action: dereth_ui::focus::action::PRIMARY_CLICK,
        start: true,
        x,
        y,
        over: None,
    }
}

// =============================================================================================
// viewport.a-press-outside-the-view-is-refused-though-it-is-inside-the-window
// =============================================================================================

/// Five presses inside the window and outside the view, each paired with the same press taken by
/// the full-window control -- which is what makes the refusal the view's doing and not the
/// window's.
pub fn a_press_outside_the_view_is_refused_though_it_is_inside_the_window() {
    let outside = [
        (INSET.x as i32 - 1, 200), // one pixel left of the view
        (300, INSET.y as i32 - 1), // one pixel above it
        (640, 200),                // one pixel past its right edge
        (300, 390),                // one pixel past its bottom edge
        (799, 599),                // the window's own last pixel
    ];

    let mut refused = dereth_client_runtime::pick::WorldPicker::new();
    let all_refused = outside
        .iter()
        .all(|(x, y)| !refused.find_object(*x, *y, INSET));
    let counted = refused.stats.outside_viewport == 5 && refused.stats.requests == 0;

    // The control: every one of those five is inside the window, and a full-window view takes
    // them all.
    let mut taken = dereth_client_runtime::pick::WorldPicker::new();
    let all_taken = outside
        .iter()
        .all(|(x, y)| taken.find_object(*x, *y, WHOLE));
    let none_refused = taken.stats.requests == 5 && taken.stats.outside_viewport == 0;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "viewport.a-press-outside-the-view-is-refused-though-it-is-inside-the-window",
        move |_| all_refused && counted && all_taken && none_refused,
    );
}

// =============================================================================================
// viewport.the-armed-point-is-measured-from-the-views-own-corner
// =============================================================================================

/// The value that is *used*, not the one that was handed over.
pub fn the_armed_point_is_measured_from_the_views_own_corner() {
    let mut p = dereth_client_runtime::pick::WorldPicker::new();
    let armed = p.find_object(INSET_MIDDLE.0, INSET_MIDDLE.1, INSET);
    let from_the_views_corner = armed && p.selection_cursor() == Some((199.0, 149.0));

    // The control: with the whole window as the view the two corners are one, which is exactly
    // what every reader of this number used to be able to assume.
    let mut q = dereth_client_runtime::pick::WorldPicker::new();
    let same_press = q.find_object(INSET_MIDDLE.0, INSET_MIDDLE.1, WHOLE);
    let from_the_windows_corner = same_press && q.selection_cursor() == Some((439.0, 239.0));

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "viewport.the-armed-point-is-measured-from-the-views-own-corner",
        move |_| from_the_views_corner && from_the_windows_corner,
    );
}

// =============================================================================================
// viewport.a-world-press-is-measured-against-the-view-the-frame-pushed-in
// =============================================================================================

/// The seam itself: three clients, one with the view pushed in and pressed inside it, one pressed
/// outside it, and one with nothing pushed in at all.
pub fn a_world_press_is_measured_against_the_view_the_frame_pushed_in() {
    use {dereth_client_runtime::interaction, dereth_client_runtime::interaction::Interaction};

    let mut inside = Interaction::new();
    inside.note_game_viewport(Some(INSET));
    let e = a_world_press(INSET_MIDDLE.0, INSET_MIDDLE.1);
    let armed_at_the_views_point =
        inside.wrapper_mouse(e, WINDOW, interaction::is_world_click(e.over))
            && inside.pick.selection_cursor() == Some((199.0, 149.0));

    // Inside the window, outside the view: nothing is armed and the refusal is counted.
    let mut outside = Interaction::new();
    outside.note_game_viewport(Some(INSET));
    let e = a_world_press(20, 20);
    let nothing_armed = !outside.wrapper_mouse(e, WINDOW, interaction::is_world_click(e.over))
        && !outside.pick.looking_for_object()
        && outside.pick.stats.outside_viewport == 1;

    // The control: with no view pushed in -- which is every client before it is looking at the
    // world at all -- the window is the view and the same press behaves as it always did.
    let mut plain = Interaction::new();
    let e = a_world_press(20, 20);
    let the_window_is_the_view =
        plain.wrapper_mouse(e, WINDOW, interaction::is_world_click(e.over))
            && plain.pick.selection_cursor() == Some((20.0, 20.0))
            && plain.pick.stats.outside_viewport == 0;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "viewport.a-world-press-is-measured-against-the-view-the-frame-pushed-in",
        move |_| armed_at_the_views_point && nothing_armed && the_window_is_the_view,
    );
}
