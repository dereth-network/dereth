//! `Display.SyncToRefresh` reaches the sync interval of a real swap-chain present: on only in
//! full screen, off when windowed, and live when the option changes.
//!
//! Fixture: the options-to-application test runs a headless App on a software device, so it
//! asserts only the interval stored for the next presentation and that no swap-chain present was
//! fabricated. The swap-chain test owns an invisible native window and reads the interval the
//! real present call used, on the default device and on the `wgpu` device, whose window surface
//! is configured to present on the refresh or at once. Neither changes a saved profile or a
//! monitor mode.

#![cfg(gpu)]

use crate::common::client_dir;

use dereth_client::app::App;
use dereth_ui_screens::{PrefValue, UiRequest};
use {dereth_client_runtime::config::Config, dereth_client_runtime::config::Preferences};

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
        preferences_file: std::env::temp_dir().join("dereth-sync-to-refresh-not-created/prefs.ini"),
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
    assert!(p.compatibility.fs_sync_to_display_refresh);
}

/// It opens an invisible window: on Linux on the display the run inherits, so a machine with none
/// runs it under Xvfb.
///
/// Behaviour: presentation.sync-to-refresh.reaches-the-swap-chain-present-interval
#[test]
#[cfg_attr(
    target_os = "macos",
    ignore = "macOS makes an event loop only on the process's main thread, which no test runs on"
)]
fn a_real_hidden_swap_chain_presents_with_the_configured_interval() {
    let handles = crate::common::invisible_window();
    // The default device, then the `wgpu` device, one at a time: a
    // window holds one swap chain.
    for backend in [None, Some(dereth_render::device::Backend::Wgpu)] {
        let mut renderer = match backend {
            None => dereth_client_shell::gpu::Renderer::new(Some(handles), 800, 600),
            Some(b) => dereth_client_shell::gpu::Renderer::new_on(b, Some(handles), 800, 600),
        }
        .expect("a real swap-chain renderer");

        renderer.set_presentation_sync(true, true);
        renderer.start_frame().expect("begin interval-one frame");
        renderer.end_frame().expect("Present(1, 0)");
        assert_eq!(
            renderer.last_present_sync_interval(),
            Some(1),
            "{backend:?}"
        );

        renderer.set_presentation_sync(false, true);
        renderer.start_frame().expect("begin immediate frame");
        renderer.end_frame().expect("Present(0, 0)");
        assert_eq!(
            renderer.last_present_sync_interval(),
            Some(0),
            "{backend:?}"
        );
    }
}
