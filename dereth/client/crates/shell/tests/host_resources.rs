//! Behaviour: none (host readers reach the actual UI without a device or implicit disk access).

use dereth_client_runtime::{
    app::{Platform, StartupError},
    config::Config,
    present::NullPresentation,
};
use dereth_client_shell::platform::host::{Host, NullHost};
use dereth_client_shell::{app::App, hud::Hud, ui::UiShell};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
};

static BLINK: AtomicU32 = AtomicU32::new(250);
thread_local! {
    static READS: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
}

#[derive(Debug)]
struct ResourceHost;
impl Host for ResourceHost {
    const BUILD_ID: &'static str = "host-resource-test";
    type Clipboard = dereth_client_shell::clipboard::NoClipboard;
    fn open_platform(
        cfg: &Config,
        events: dereth_client_shell::platform::window::WindowEvents,
    ) -> Result<Platform, StartupError> {
        NullHost::open_platform(cfg, events)
    }
    fn local_utc_offset_secs(_: i64) -> i32 {
        0
    }
    fn launch_uri(_: &str) -> i32 {
        0
    }
    fn install_default_output() {}
    fn clipboard() -> Self::Clipboard {
        Self::Clipboard::default()
    }
    fn cursor_images(window: Option<isize>) -> Box<dyn dereth_client_shell::cursor::CursorImages> {
        NullHost::cursor_images(window)
    }
    fn caret_blink_secs() -> f64 {
        dereth_client_contract::window_proc::caret_blink_time_seconds_from_millis(
            BLINK.load(Ordering::Relaxed),
        )
    }
    fn movie_bytes(path: &Path) -> Option<Vec<u8>> {
        READS.with(|reads| reads.borrow_mut().push(path.to_owned()));
        match path.file_name()?.to_str()? {
            "valid.avi" => Some(
                std::fs::read(dereth_dat::testing::dat_dir().join("turbine_logo_ac.avi"))
                    .expect("the movie fixture"),
            ),
            "invalid.avi" => Some(vec![0; 12]),
            _ => None,
        }
    }
}

#[test]
fn constructing_receivers_does_not_preempt_the_host_and_media_reads_are_lazy_once() {
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let _early_hud = Hud::default();
    let _early_ui = UiShell::new(&store, (800, 600)).expect("UI before host registration");
    let cfg = Config {
        headless: true,
        sound: false,
        world: false,
        dat_dir: dereth_dat::testing::dat_dir(),
        preferences_file: std::env::temp_dir().join(format!(
            "dereth-host-resources-{}/preferences.ini",
            std::process::id()
        )),
        ..Config::default()
    };
    let movie_dir = cfg.dat_dir.clone();
    let mut app =
        App::<ResourceHost>::with_presentation(cfg, Box::new(NullPresentation::new(800, 600)))
            .expect("device-free App");
    app.start_shell().expect("UI");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    app.frame();
    assert_eq!(
        app.ui().unwrap().ui.caret_blink_time,
        dereth_client_contract::window_proc::caret_blink_time_seconds_from_millis(250)
    );
    BLINK.store(u32::MAX, Ordering::Relaxed);
    app.frame();
    assert_eq!(
        app.ui().unwrap().ui.caret_blink_time,
        dereth_client_contract::window_proc::caret_blink_time_seconds_from_millis(u32::MAX)
    );

    READS.with(|reads| reads.borrow_mut().clear());
    let initial = app.ui().unwrap().movie_stats;
    for name in ["missing.avi", "invalid.avi", "valid.avi"] {
        let shell = app.ui_mut().unwrap();
        let root = shell.ui.root();
        shell.ui.initialize(root);
        let node = shell.ui.node_mut(root).unwrap();
        assert!(node.flags.is_initialized());
        node.media.reset(&[dereth_ui::desc::MediaDesc {
            media_type: 1,
            type_echo_ok: true,
            fields: dereth_ui::desc::MediaFields::Movie {
                file_name: name.into(),
                stretch_to_full_screen: 0,
            },
        }]);
        node.media.registered_for_tick = true;
        app.frame();
        if name != "valid.avi" {
            assert!(
                app.ui()
                    .unwrap()
                    .ui
                    .node(root)
                    .unwrap()
                    .media
                    .movie_finished
            );
        }
    }
    let after = app.ui().unwrap().movie_stats;
    assert_eq!(after.started - initial.started, 3);
    assert_eq!(after.skipped - initial.skipped, 2);
    assert!(
        after.frames > initial.frames,
        "host movie bytes reach the real decoder"
    );
    app.frame();
    assert_eq!(
        READS.with(|reads| reads.borrow().clone()),
        ["missing.avi", "invalid.avi", "valid.avi"].map(|name| movie_dir.join(name))
    );
}
