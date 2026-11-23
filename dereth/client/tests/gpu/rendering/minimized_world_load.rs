//! A world loaded while the window is minimized presents nothing: the empty frames that size the
//! world's upload space wait for the first frame drawn after the window is restored.
//! Fixture: the retail dats in `$DERETH_TEST_DAT_DIR` and a headless device.
//!
//! Behaviour: none (a world loaded while minimized presents nothing until the restore)

#![cfg(gpu)]

use crate::common::client_dir;
use dereth_client::app::App;
use dereth_client_runtime::config::Config;
use dereth_input::host::HostEvent;

#[test]
fn a_world_loaded_while_minimized_presents_nothing_until_the_window_is_restored() {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are required at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
    let cfg = Config {
        headless: true,
        sound: false,
        dat_dir: client_dir(),
        ..Config::default()
    };
    let mut app = App::new(cfg).expect("the device");
    let (width, height) = app.renderer_mut().size();
    app.handle_window_event(
        &HostEvent::Resized {
            width: 0,
            height: 0,
        },
        0,
    );
    let before = app.renderer_mut().gpu.frame_stamp();
    let s = dereth_client_runtime::scene::SceneConfig {
        landblock: app.config().landblock,
        land_radius: 1,
        scenery_radius: 0,
        ..dereth_client_runtime::scene::SceneConfig::default()
    };
    app.load_static_scene(s).expect("the static scene loads");
    assert_eq!(
        app.renderer_mut().gpu.frame_stamp(),
        before,
        "loading the world presented frames while the window was minimized"
    );
    assert!(app.frame());
    assert_eq!(
        app.renderer_mut().gpu.frame_stamp(),
        before,
        "a minimized frame presented"
    );

    app.handle_window_event(&HostEvent::Resized { width, height }, 0);
    assert!(app.frame());
    assert_eq!(
        app.renderer_mut().gpu.frame_stamp() - before,
        u64::try_from(dereth_render::device::FRAME_COUNT).unwrap() + 1,
        "the first frame drawn sizes the upload space, then draws"
    );
    app.shutdown();
}
