//! A resolution change keeps the window's top-left corner (client divergence CD-001), on a running
//! client: picking a resolution, the forced 800x600 on logging out and back in, and a refused
//! change all leave the window where it was, moving it only as far as the work area requires.
//!
//! Fixture: a headless App on the retail dats whose computed window rectangle is read across
//! resolution changes. All monitor metrics are synthetic: the tests inspect the renderer size and
//! the App's maintained computed rectangle, not a real desktop window or monitor. They emit the
//! preference request the resolution selector sends, without clicking the selector itself, and
//! allow two frames for request processing and presentation polling. No datagram is sent. The
//! placement rule itself, over synthetic metrics with no device, is the cpu tier's
//! `presentation::window_placement`.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

// =============================================================================================
// The wiring: headless application and computed window rectangle
// =============================================================================================

mod wired {
    use std::path::PathBuf;

    use dereth_client::app::App;
    use dereth_render::window_proc::Rect;
    use dereth_ui::framework::mode;
    use dereth_ui_screens::options::store;
    use dereth_ui_screens::{PrefValue, UiRequest};
    use {dereth_client_runtime::config::Config, dereth_client_runtime::config::Preferences};

    const RESOLUTION: &str = "Display.Resolution";

    /// The platform's `headless_screen_metrics`: 1920x1080, origin zero, no frame/work area.
    /// Repeat its expected dimensions independently so a change breaks these numbers.
    const SCREEN: (i32, i32) = (1920, 1080);

    fn client_dir() -> PathBuf {
        dereth_dat::testing::dat_dir()
    }

    fn app_with(resolution: Option<&str>) -> App {
        assert!(
            dereth_dat::testing::have_dats(),
            "the retail dats are this file's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
            client_dir().display()
        );
        let mut cfg = Config {
            ui: true,
            headless: true,
            sound: false,
            connect: false,
            dat_dir: client_dir(),
            preferences_file: std::env::temp_dir()
                .join("dereth-window-position-not-created/prefs.ini"),
            ..Config::default()
        };
        if let Some(r) = resolution {
            cfg.apply_preferences(&Preferences::parse(&format!(
                "[Display]\r\nFullScreen=False\r\nResolution={r}\r\n"
            )));
        }
        // Display preferences default to fullscreen, matching the original registered default.
        // These tests explicitly request windowed mode so they measure a windowed rectangle
        // rather than the borderless whole-monitor placement.
        cfg.display.full_screen = false;
        let mut a = App::new(cfg).expect("an application");
        a.start_shell().expect("the shell starts");
        a.ui_mut().expect("the UI shell is up").ui.requests.clear();
        a
    }

    /// Allow two frames for request draining and the subsequent presentation poll (the apply lags
    /// one frame); these assertions inspect the settled result rather than separately checking
    /// the exact frame on which each stage runs.
    fn settle(a: &mut App) {
        a.frame();
        a.frame();
    }

    fn enter(a: &mut App, m: dereth_ui::UiMode) {
        a.queue_ui_mode(m);
        settle(a);
        assert_eq!(a.ui().expect("a shell").flow.current_mode(), Some(m));
    }

    fn rect(a: &App) -> Rect {
        a.window_rect()
            .expect("the application maintains a computed window rectangle")
    }

    fn top_left(a: &App) -> (i32, i32) {
        let r = rect(a);
        (r.left, r.top)
    }

    /// Creation still centers a new client, which has no prior windowed position to preserve.
    /// This checks the headless computed rectangle using the fixed screen fixture.
    #[test]
    fn a_new_client_is_created_centred() {
        let a = app_with(None);
        assert_eq!(a.renderer().size(), (800, 600));
        assert_eq!(
            top_left(&a),
            (SCREEN.0 / 2 - 800 / 2, SCREEN.1 / 2 - 600 / 2),
            "the creation-time center is preserved"
        );
        assert_eq!(top_left(&a), (560, 240));
    }

    /// Behaviour: presentation.window.a-resolution-change-keeps-the-windows-top-left
    ///
    /// Emit the resolution preference request used by the option UI. After the presentation
    /// poll, renderer extent changes and the computed top-left remains fixed. The request seam
    /// is exercised directly, without testing selector mouse handling or a real desktop window.
    #[test]
    fn picking_a_resolution_resizes_without_moving_the_window() {
        let mut a = app_with(None);
        let before = top_left(&a);
        assert_eq!(before, (560, 240));
        // Entering gameplay lifts the forced resolution, itself a presentation change. Client
        // Options is normally opened from this screen; no panel opening is needed for this request.
        enter(&mut a, mode::GAME_PLAY);
        assert_eq!(
            top_left(&a),
            before,
            "the un-force is itself a presentation change"
        );

        a.ui_mut()
            .expect("the UI shell is up")
            .ui
            .requests
            .emit(UiRequest::SetPreference(
                RESOLUTION,
                PrefValue::Int(store::mode_desc(1280, 720)),
            ));
        settle(&mut a);
        assert_eq!(
            a.renderer().size(),
            (1280, 720),
            "the resolution choice still applies"
        );
        assert_eq!(
            top_left(&a),
            before,
            "the extent changed, the position did not"
        );
        assert_eq!(
            rect(&a),
            Rect {
                left: 560,
                top: 240,
                right: 560 + 1280,
                bottom: 240 + 720
            }
        );
        // The retail answer, which CD-001 departs from.
        assert_ne!(
            top_left(&a),
            (SCREEN.0 / 2 - 1280 / 2, SCREEN.1 / 2 - 720 / 2)
        );
        assert_ne!(top_left(&a), (320, 180));

        // Change again to a narrower, taller extent to guard against a coincidental position match.
        a.ui_mut()
            .expect("the UI shell is up")
            .ui
            .requests
            .emit(UiRequest::SetPreference(
                RESOLUTION,
                PrefValue::Int(store::mode_desc(1024, 768)),
            ));
        settle(&mut a);
        assert_eq!(a.renderer().size(), (1024, 768));
        assert_eq!(
            top_left(&a),
            before,
            "the narrower, taller resolution does not re-center either"
        );
        assert_ne!(
            top_left(&a),
            (SCREEN.0 / 2 - 1024 / 2, SCREEN.1 / 2 - 768 / 2)
        );
    }

    /// The off-screen rule through a headless `App`: this extent overflows the bottom-right,
    /// so the computed rectangle moves by the minimum needed to fit. Compare against centering.
    #[test]
    fn a_resolution_that_would_run_off_the_screen_is_clamped() {
        let mut a = app_with(None);
        assert_eq!(top_left(&a), (560, 240));
        enter(&mut a, mode::GAME_PLAY);

        a.ui_mut()
            .expect("the UI shell is up")
            .ui
            .requests
            .emit(UiRequest::SetPreference(
                RESOLUTION,
                PrefValue::Int(store::mode_desc(1600, 900)),
            ));
        settle(&mut a);
        assert_eq!(a.renderer().size(), (1600, 900));
        assert_eq!(
            top_left(&a),
            (SCREEN.0 - 1600, SCREEN.1 - 900),
            "x = area.right - cx, y = area.bottom - cy: the smallest move that fits"
        );
        assert_eq!(top_left(&a), (320, 180));
        let r = rect(&a);
        assert!(
            r.right <= SCREEN.0 && r.bottom <= SCREEN.1,
            "fully on the monitor"
        );
        // Not the centre, which for 1600x900 would be (160, 90) — the clamp moved it 240 and 60
        // pixels, not 400 and 150.
        assert_ne!(
            top_left(&a),
            (SCREEN.0 / 2 - 1600 / 2, SCREEN.1 / 2 - 900 / 2)
        );
    }

    /// Exercise forced-resolution transitions by changing screens directly. Entering gameplay
    /// lifts the 800x600 override and applies saved resolution; leaving restores the override.
    /// Repeat entry to distinguish the initial clamp from later top-left preservation. These
    /// are UI mode transitions, not authenticated network login/logout exchanges.
    #[test]
    fn logging_out_and_back_in_keeps_the_window_where_it_is() {
        let mut a = app_with(Some("1600x900"));
        assert_eq!(
            a.renderer().size(),
            (800, 600),
            "client initialization forces 800x600"
        );
        assert_eq!(top_left(&a), (560, 240), "the creation centre");

        // Log in: the force is lifted and the saved 1600x900 reaches the presentation. It does
        // not fit at (560, 240), so the off-screen rule moves it the minimum — and no further.
        enter(&mut a, mode::GAME_PLAY);
        assert_eq!(a.renderer().size(), (1600, 900));
        assert!(!a.uses_forced_resolution());
        assert_eq!(top_left(&a), (320, 180), "clamped, not centred");
        assert_ne!(top_left(&a), (160, 90), "the 1600x900 screen centre");

        // Leaving gameplay restores 800x600. At this measured top-left that extent fits, so
        // the computed position stays fixed even though the render target shrinks.
        enter(&mut a, mode::CHARACTER_MANAGEMENT);
        assert_eq!(a.renderer().size(), (800, 600));
        assert!(a.uses_forced_resolution());
        assert_eq!(
            top_left(&a),
            (320, 180),
            "the top-left is kept on the log-out edge"
        );
        assert_ne!(
            top_left(&a),
            (560, 240),
            "the 800x600 screen centre, which is where it went"
        );

        // And log back in: 1600x900 from (320, 180) fits exactly, so this time not even the clamp
        // fires and the position is kept outright.
        enter(&mut a, mode::GAME_PLAY);
        assert_eq!(a.renderer().size(), (1600, 900));
        assert_eq!(
            top_left(&a),
            (320, 180),
            "the top-left is kept on the log-in edge"
        );
        assert_ne!(top_left(&a), (160, 90));
        assert_eq!(
            rect(&a),
            Rect {
                left: 320,
                top: 180,
                right: 1920,
                bottom: 1080
            }
        );
    }

    /// Reject a 640x480 request below the 800x600 minimum. Both renderer size and rectangle
    /// remain unchanged, separating a declined request from the accepted resize tests.
    /// This checks the returned state, not an internal count of presentation-change calls.
    #[test]
    fn a_refused_resolution_change_moves_nothing() {
        let mut a = app_with(None);
        enter(&mut a, mode::GAME_PLAY);
        let before = rect(&a);
        a.ui_mut()
            .expect("the UI shell is up")
            .ui
            .requests
            .emit(UiRequest::SetPreference(
                RESOLUTION,
                PrefValue::Int(store::mode_desc(640, 480)),
            ));
        settle(&mut a);
        assert_eq!(
            a.renderer().size(),
            (800, 600),
            "the minimum accepted resolution remains 800x600"
        );
        assert_eq!(rect(&a), before);
    }

    /// Behaviour: presentation.window.leaving-full-screen-puts-the-window-back-where-it-was
    ///
    /// Emit the full-screen preference request the options page sends, on and then off, in the
    /// world with the window moved off the screen centre. While full screen the computed rectangle
    /// is the whole monitor; leaving it restores the top-left the window had before, where retail
    /// centres the window on the screen.
    #[test]
    fn leaving_full_screen_puts_the_window_back_where_it_was() {
        let mut a = app_with(Some("1600x900"));
        enter(&mut a, mode::GAME_PLAY);
        // The saved 1600x900 does not fit at the creation centre, so the clamp moved it to
        // (320, 180): a position that is neither the creation centre nor the 1600x900 centre.
        assert_eq!(top_left(&a), (320, 180));

        let full_screen = |a: &mut App, on: bool| {
            a.ui_mut()
                .expect("the UI shell is up")
                .ui
                .requests
                .emit(UiRequest::SetPreference(
                    "Display.FullScreen",
                    PrefValue::Bool(on),
                ));
            settle(a);
        };

        full_screen(&mut a, true);
        assert_eq!(
            rect(&a),
            Rect {
                left: 0,
                top: 0,
                right: SCREEN.0,
                bottom: SCREEN.1
            },
            "full screen covers the monitor"
        );

        full_screen(&mut a, false);
        assert_eq!(
            a.renderer().size(),
            (1600, 900),
            "windowed at the saved size"
        );
        assert_eq!(
            top_left(&a),
            (320, 180),
            "leaving full screen puts the window back where it was"
        );
        assert_ne!(
            top_left(&a),
            (SCREEN.0 / 2 - 1600 / 2, SCREEN.1 / 2 - 900 / 2),
            "NOT retail's screen centre"
        );
        assert_ne!(top_left(&a), (160, 90));

        // A second round trip is the same: the remembered position is taken each time full
        // screen is entered, not once.
        full_screen(&mut a, true);
        full_screen(&mut a, false);
        assert_eq!(top_left(&a), (320, 180));
    }
}
