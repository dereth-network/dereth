//! Explore mode (`--explore`) through the whole client, on a host whose clipboard records what it
//! is given: the camera starts where the location says, its keys fly it and step its speed, and
//! its two copy keys put its spot and the report's line on the clipboard; no key reaches the game.
//! Fixture: the retail dats, with no server and no body, in a device-free world.
//!
//! Behaviour: none (tooling: a developer's camera flown through the world with no server).

use super::report_command_tests::{ReportHost, COPIED};
use super::*;
use crate::app::CoreApp;
use dereth_input::host::HostEvent;
use dereth_input::keys::Key;

struct Client {
    app: CoreApp<ReportHost>,
    shell: ClientShell<ReportHost>,
    state: std::path::PathBuf,
}

/// Where the camera starts: over the Holtburg road, facing east.
const START: &str = "camera 0xA9B4 [96 96 120] yaw -1.5707964 pitch 0";

impl Client {
    fn new() -> Self {
        COPIED.with(|c| c.borrow_mut().clear());
        let state = std::env::temp_dir().join(format!("dereth-explore-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&state);
        let mut cfg = dereth_client_runtime::config::Config {
            headless: true,
            sound: false,
            world: false,
            width: 800,
            height: 600,
            dat_dir: dereth_dat::testing::dat_dir(),
            preferences_file: state.join("prefs.ini"),
            ..Default::default()
        };
        cfg.explore = true;
        cfg.connect = false;
        cfg.character = false;
        cfg.plugins = false;
        cfg.explore_start =
            Some(dereth_client_runtime::explore::parse_start(START).expect("the start reads"));
        let mut app = CoreApp::<ReportHost>::bring_up_with_store(
            cfg,
            None,
            |_| Ok(dereth_client_runtime::app::Platform::headless(800, 600)),
            |_, _, _, _| {
                Ok(Box::new(
                    dereth_client_runtime::sim_present::SimPresentation::new(800, 600),
                ))
            },
        )
        .expect("real tables and a device-free world");
        let mut shell =
            ClientShell::with_window_events(app.window.raw_handle(), Default::default());
        app.start_shell(&mut shell).expect("real input and UI");
        let scene = dereth_client_runtime::scene::SceneConfig {
            land_radius: 1,
            scenery_radius: 0,
            character: false,
            ..app.cfg.scene_config()
        };
        app.load_static_scene(scene).expect("a world");
        let mut c = Self { app, shell, state };
        c.frames(2);
        c
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            assert!(self.app.frame(&mut self.shell));
        }
    }

    fn key(&mut self, virtual_key: usize, pressed: bool) {
        self.shell.queue_window_event(HostEvent::KeyboardInput {
            key: Key::new(virtual_key, 0),
            pressed,
            text: None,
        });
    }

    fn press(&mut self, virtual_key: usize) {
        self.key(virtual_key, true);
        self.key(virtual_key, false);
        self.frames(1);
    }

    fn camera(&self) -> dereth_client_runtime::camera::FreeCamera {
        self.app.world.as_ref().expect("a world").camera
    }

    /// How far east, in meters, the camera flies in `frames` frames with W held.
    fn fly_east(&mut self, frames: usize) -> f32 {
        let block = |c: &Self| c.app.world.as_ref().and_then(|w| w.viewer_block());
        let (from, from_block) = (self.camera().position, block(self));
        self.key(0x57, true);
        self.frames(frames);
        self.key(0x57, false);
        self.frames(1);
        let (to, to_block) = (self.camera().position, block(self));
        let crossed = to_block.expect("a window").0 - from_block.expect("a window").0;
        // LINT-OK: a few blocks.
        #[allow(clippy::cast_precision_loss)]
        let crossed = crossed as f32 * 192.0;
        to.x + crossed - from.x
    }

    fn finish(self) {
        let Self {
            app,
            mut shell,
            state,
        } = self;
        app.shutdown(&mut shell);
        let _ = std::fs::remove_dir_all(state);
    }
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn explore_mode_flies_its_camera_and_copies_where_it_is() {
    let mut c = Client::new();
    // Where the location said.
    let start = c.camera();
    let block = c
        .app
        .world
        .as_ref()
        .and_then(|w| w.viewer_block())
        .expect("a window");
    assert_eq!(block, (0xA9, 0xB4));
    assert_eq!((start.position.x, start.position.y), (96.0, 96.0));
    assert!(c.app.world.as_ref().expect("a world").character.is_none());

    // W flies it the way it faces, and a step faster flies it 2.5 times as far.
    let slow = c.fly_east(5);
    assert!(slow > 1.0, "{slow}");
    assert!(
        (c.camera().position.y - start.position.y).abs() < 1e-2,
        "level and east"
    );
    c.shell
        .queue_window_event(HostEvent::MouseWheel { notches: 1.0 });
    c.frames(1);
    let fast = c.fly_east(5);
    assert!((fast / slow - 2.5).abs() < 0.05, "{slow} then {fast}");
    // E rises.
    let z = c.camera().position.z;
    c.key(0x45, true);
    c.frames(3);
    c.key(0x45, false);
    c.frames(1);
    assert!(c.camera().position.z > z + 1.0);
    // The game's own keys are not the game's in this mode: nothing moved a body or ran.
    assert_eq!(c.app.char_input, Default::default());

    // C copies the camera's spot as a location line, which reads back as where it is.
    c.press(0x43);
    let copied = COPIED.with(|c| c.borrow().clone());
    assert_eq!(copied.len(), 1, "{copied:?}");
    assert!(
        copied[0].starts_with("Your location is: 0x"),
        "{}",
        copied[0]
    );
    let back = dereth_client_runtime::explore::parse_start(&copied[0]).expect("reads back");
    let block = c
        .app
        .world
        .as_ref()
        .and_then(|w| w.viewer_block())
        .expect("a window");
    let here = c.camera();
    let at = back.to_render(block);
    assert!(
        (at.position.x - here.position.x).abs() < 1e-3,
        "{}",
        copied[0]
    );
    assert!(
        (at.position.y - here.position.y).abs() < 1e-3,
        "{}",
        copied[0]
    );
    // R copies the visual report's line, with the camera in it.
    c.press(0x52);
    let copied = COPIED.with(|c| c.borrow().clone());
    assert_eq!(copied.len(), 2, "{copied:?}");
    let report =
        dereth_client_runtime::report::VisualReport::parse(&copied[1]).expect("a report line");
    assert_eq!(report.mode, "free");
    assert!(report.camera.is_some());
    // And the corner line says so.
    assert!(
        !c.shell.explore.items.is_empty(),
        "the corner line is drawn"
    );
    c.finish();
}
