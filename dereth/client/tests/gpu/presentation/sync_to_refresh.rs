//! `Display.SyncToRefresh` reaches the sync interval of a real swap-chain present: on only in
//! full screen, off when windowed, and live when the option changes.
//!
//! Fixture: the options-to-application test runs a headless App on a software device, so it
//! asserts only the interval stored for the next presentation and that no swap-chain present was
//! fabricated. The swap-chain test owns an invisible native window and reads the interval the
//! real present call used. Neither changes a saved profile or a monitor mode.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;

use dereth_client::app::App;
use dereth_client::config::{Config, Preferences};
use dereth_ui_screens::{PrefValue, UiRequest};

const SYNC: &str = "Display.SyncToRefresh";

fn app() -> App {
    let dir = client_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "retail dats are required at {}",
        dir.display()
    );
    let mut cfg = Config {
        ui: true,
        headless: true,
        sound: false,
        connect: false,
        dat_dir: dir,
        preferences_file: std::env::temp_dir().join("dere-p1-82b-not-created/prefs.ini"),
        ..Config::default()
    };
    // Display-preference initialization defaults FullScreen to true. This station
    // needs to exercise both policy arms, so make its initial arm explicitly windowed.
    cfg.display.full_screen = false;
    let mut app = App::new(cfg).expect("an application");
    app.start_shell().expect("the shell starts");
    // Full screen is carried only in gameplay: the preference is stored in any mode, but the
    // presentation goes full screen only while the gameplay screen is up.
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    settle(&mut app);
    assert_eq!(
        app.ui_mut()
            .expect("the UI shell is up")
            .flow
            .current_mode(),
        Some(dereth_ui::framework::mode::GAME_PLAY),
        "the station is in gameplay, where full screen applies"
    );
    app.ui_mut()
        .expect("the UI shell is up")
        .ui
        .requests
        .clear();
    app
}

fn settle(app: &mut App) {
    app.frame();
    app.frame();
}

#[test]
fn the_option_write_reaches_the_native_fullscreen_present_policy() {
    let mut app = app();
    assert_eq!(app.renderer().present_sync_interval(), 0);

    app.ui_mut()
        .expect("the UI shell is up")
        .ui
        .requests
        .emit(UiRequest::SetPreference(SYNC, PrefValue::Bool(true)));
    settle(&mut app);
    assert!(
        app.config().display.sync_to_refresh,
        "the options write reaches DisplayPrefs"
    );
    assert_eq!(
        app.renderer().present_sync_interval(),
        0,
        "the presentation setup leaves a windowed present immediate even with the preference on"
    );

    app.ui_mut()
        .expect("the UI shell is up")
        .ui
        .requests
        .emit(UiRequest::SetPreference(
            "Display.FullScreen",
            PrefValue::Bool(true),
        ));
    settle(&mut app);
    assert_eq!(
        app.renderer().present_sync_interval(),
        1,
        "logical full screen plus SyncToRefresh selects the supported interval-one path"
    );

    app.ui_mut()
        .expect("the UI shell is up")
        .ui
        .requests
        .emit(UiRequest::SetPreference(SYNC, PrefValue::Bool(false)));
    settle(&mut app);
    assert_eq!(
        app.renderer().present_sync_interval(),
        0,
        "turning synchronization off is live"
    );
    assert_eq!(
        app.renderer().last_present_sync_interval(),
        None,
        "headless rendering is offscreen and must not masquerade as a swap-chain Present"
    );
}

#[test]
fn a_saved_sync_preference_reaches_the_presentation_record() {
    let mut cfg = Config::default();
    cfg.apply_preferences(&Preferences::parse(
        "[Display]\r\nFullScreen=True\r\nSyncToRefresh=True\r\n",
    ));
    let p = cfg
        .load_display_preferences(None, true)
        .expect("a legal presentation");
    assert!(p.full_screen);
    assert!(p.fs_sync_to_display_refresh);
}

/// Behaviour: presentation.sync-to-refresh.reaches-the-swap-chain-present-interval
#[test]
fn a_real_hidden_swap_chain_presents_with_the_configured_interval() {
    use winit::dpi::PhysicalSize;

    let mut builder = winit::event_loop::EventLoopBuilder::new();
    #[cfg(windows)]
    {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        builder.with_any_thread(true);
    }
    let event_loop = builder.build().expect("an event loop");
    let window = winit::window::WindowBuilder::new()
        .with_visible(false)
        .with_inner_size(PhysicalSize::new(800, 600))
        .build(&event_loop)
        .expect("an invisible native window");
    let handles = dereth_client::platform::window::window_handles(&window).expect("window handles");
    let mut renderer = dereth_client::gpu::Renderer::new(Some(handles), 800, 600)
        .expect("a real swap-chain renderer");

    renderer.set_presentation_sync(true, true);
    renderer.start_frame().expect("begin interval-one frame");
    renderer.end_frame().expect("Present(1, 0)");
    assert_eq!(renderer.last_present_sync_interval(), Some(1));

    renderer.set_presentation_sync(false, true);
    renderer.start_frame().expect("begin immediate frame");
    renderer.end_frame().expect("Present(0, 0)");
    assert_eq!(renderer.last_present_sync_interval(), Some(0));
}
