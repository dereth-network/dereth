//! The pointer during a camera drag, through the whole client on a host that records what it is
//! asked: the game camera's mouse look and the Horizon interface's orbit drags hide and hold the
//! pointer once the drag has moved it, the mouse's own movement turns the camera meanwhile, and
//! the pointer is shown again where the drag began when the drag ends, the window loses the focus
//! or another interface is shown. A host that holds the pointer by putting it back reads the
//! mouse's movement off its reports of the pointer, at the pointer's own rate. A drag of the
//! interface itself, and any drag in gamepad mode, keeps the pointer shown. Fixture: the retail
//! dats, with no server, at the gameplay screen.

use super::*;
use crate::{
    app::CoreApp,
    platform::{
        host::{Host, NullHost},
        keys::MouseButton,
        window::HostEvent,
    },
    pointer::{HostPointer, PutBack},
};
use dereth_client_contract::options::{
    interface::{Interface, INTERFACE},
    store,
};
use dereth_client_contract::PrefValue;
use std::cell::{Cell, RefCell};

/// What the host was asked.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Asked {
    /// Hide the pointer and hold it, where it is.
    Hold((f64, f64)),
    /// Show the pointer again, here.
    LetGo((f64, f64)),
}

thread_local! {
    static ASKED: RefCell<Vec<Asked>> = const { RefCell::new(Vec::new()) };
    /// Whether the host can hold the pointer; one that cannot leaves the drag as it was.
    static HOLDS: Cell<bool> = const { Cell::new(true) };
    /// Whether the host holds the pointer by putting it back, as the desktop does.
    static PUTS_BACK: Cell<bool> = const { Cell::new(false) };
    /// The host's hold while it holds the pointer by putting it back.
    static PUT_BACK: RefCell<Option<PutBack>> = const { RefCell::new(None) };
}

/// The middle of the 800 by 600 window, where a host that puts the pointer back puts it.
const MIDDLE: (f64, f64) = (400.0, 300.0);

/// Everything the host has been asked on this thread.
fn asked() -> Vec<Asked> {
    ASKED.with(|a| a.borrow().clone())
}

/// A pointer hold that records what it is asked and holds when [`HOLDS`] says it can, putting
/// the pointer back in the middle of the window when [`PUTS_BACK`] says so.
struct Recorder;

impl HostPointer for Recorder {
    fn capture(&mut self, at: (f64, f64)) -> bool {
        ASKED.with(|a| a.borrow_mut().push(Asked::Hold(at)));
        if !HOLDS.get() {
            return false;
        }
        if PUTS_BACK.get() {
            PUT_BACK.set(Some(PutBack::new(at, MIDDLE, true).0));
        }
        true
    }

    fn release(&mut self, at: (f64, f64)) {
        ASKED.with(|a| a.borrow_mut().push(Asked::LetGo(at)));
        PUT_BACK.set(None);
    }
}

/// The host with nothing under it but a pointer hold that records what it is asked.
#[derive(Debug, Clone, Copy, Default)]
struct PointerHost;

impl Host for PointerHost {
    const BUILD_ID: &'static str = "pointer-hold-test";
    type Clipboard = crate::clipboard::NoClipboard;
    fn open_platform(
        cfg: &dereth_client_runtime::config::Config,
        events: crate::platform::window::WindowEvents,
    ) -> Result<dereth_client_runtime::app::Platform, dereth_client_runtime::app::StartupError>
    {
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
        crate::clipboard::NoClipboard
    }
    fn cursor_images(window: Option<isize>) -> Box<dyn crate::cursor::CursorImages> {
        NullHost::cursor_images(window)
    }
    fn pointer(_window: Option<isize>) -> Box<dyn HostPointer> {
        Box::new(Recorder)
    }
}

struct Client {
    app: CoreApp<PointerHost>,
    shell: ClientShell<PointerHost>,
    state: std::path::PathBuf,
}

impl Client {
    /// The client at the gameplay screen in the modern interface, its settings kept in a folder
    /// of its own named after `name`.
    fn new(name: &str) -> Self {
        Self::with_presentation(name, || {
            Box::new(crate::present::NullPresentation::new(800, 600))
        })
    }

    /// [`Self::new`], drawing with what `make` makes.
    fn with_presentation(
        name: &str,
        make: fn() -> Box<dyn crate::present::ClientPresentation>,
    ) -> Self {
        ASKED.with(|a| a.borrow_mut().clear());
        let state =
            std::env::temp_dir().join(format!("dereth-pointer-hold-{name}-{}", std::process::id()));
        let cfg = dereth_client_runtime::config::Config {
            headless: true,
            connect: false,
            sound: false,
            world: false,
            width: 800,
            height: 600,
            dat_dir: dereth_dat::testing::dat_dir(),
            preferences_file: state.join("prefs.ini"),
            ..Default::default()
        };
        let mut app = CoreApp::<PointerHost>::bring_up_with_store(
            cfg,
            None,
            |_| Ok(dereth_client_runtime::app::Platform::headless(800, 600)),
            |_, _, _, _| Ok(make()),
        )
        .expect("real tables and a device-free presentation");
        let mut shell =
            ClientShell::with_window_events(app.window.raw_handle(), Default::default());
        app.start_shell(&mut shell).expect("real input and UI");
        for _ in 0..2 {
            assert!(app.frame(&mut shell));
        }
        shell
            .modern
            .ui
            .as_mut()
            .expect("UI")
            .queue(dereth_ui::framework::mode::GAME_PLAY);
        let mut c = Self { app, shell, state };
        for _ in 0..6 {
            c.frame();
        }
        c
    }

    /// The Horizon interface shown, at its game screen, with `pad` its gamepad mode.
    fn horizon(&mut self, pad: bool) {
        let mut ui = dereth_horizon::runtime::HorizonFrontEnd::new(
            std::sync::Arc::new(dereth_horizon::art::Art::empty()),
            dereth_horizon::options::HorizonOptions {
                screen: dereth_horizon::options::StartScreen::Game,
                ..Default::default()
            },
            None,
        );
        ui.start(&mut self.app.ui_context());
        ui.ui.options.pad.enabled = pad;
        self.shell.horizon.ui = Some(ui);
        self.choose(Interface::Horizon);
        self.frame();
    }

    /// The classic interface shown, over the early portal its tests read.
    fn classic(&mut self) {
        struct Fonts;
        impl dereth_classic_dat::fonts::FontSource for Fonts {
            fn rasterize(
                &self,
                _: &dereth_classic_dat::fonts::FontSpec,
            ) -> Result<dereth_classic_dat::fonts::FontAtlas, String> {
                Ok(Default::default())
            }
        }
        let portal = std::path::PathBuf::from(
            std::env::var_os("DERETH_CLASSIC_PORTAL").expect("classic portal"),
        );
        let art = std::sync::Arc::new(
            dereth_classic_ui::art::ClassicArt::new(
                dereth_classic_dat::ClassicPortal::open(&portal).unwrap(),
                &Fonts,
            )
            .unwrap(),
        );
        let mut ui = dereth_classic_ui::runtime::ClassicUi::new(
            dereth_classic_ui::resources::Resources::new(
                art,
                Err("World creation tables unavailable".into()),
                None,
            ),
            dereth_classic_ui::art::ClassicPaths {
                state: self.state.join("classic"),
            },
            dereth_classic_ui::panels::factory,
            (800, 600),
        );
        ui.start(&mut self.app.ui_context())
            .expect("the classic interface starts");
        self.shell.classic.ui = Some(ui);
        self.choose(Interface::Classic);
        self.frame();
    }

    /// A body in the world for the camera to turn about, on a presentation that keeps a world.
    fn body(&mut self) {
        let store = std::sync::Arc::clone(self.app.probe().dat_store());
        let cfg = dereth_client_runtime::scene::SceneConfig {
            land_radius: 0,
            scenery_radius: 0,
            ..Default::default()
        };
        self.app
            .probe_mut()
            .load_world(&store, cfg)
            .expect("a world with a body");
        self.frame();
        self.frame();
    }

    fn choose(&mut self, interface: Interface) {
        store::set_value(INTERFACE, PrefValue::Int(interface.value()));
        self.frame();
    }

    fn frame(&mut self) {
        assert!(self.app.frame(&mut self.shell));
    }

    /// One of the window's events, and a frame for it.
    fn send(&mut self, event: HostEvent) {
        self.shell.queue_window_event(event);
        self.frame();
    }

    fn pointer_to(&mut self, x: f64, y: f64) {
        self.send(HostEvent::CursorMoved { x, y });
    }

    /// The window reports the pointer at `(x, y)`, which a host holding the pointer by putting it
    /// back reads as the mouse's movement since its last report, and as no movement when it is
    /// its report of the pointer put back; after the frame's drain such a host puts the pointer
    /// back.
    fn report(&mut self, x: f64, y: f64) {
        let moved = PUT_BACK.with(|p| p.borrow_mut().as_mut().map(|p| p.moved((x, y))));
        match moved {
            None => self.pointer_to(x, y),
            Some(Some((dx, dy))) => self.send(HostEvent::PointerMotion { dx, dy }),
            Some(None) => self.frame(),
        }
        PUT_BACK.with(|p| {
            if let Some(p) = p.borrow_mut().as_mut() {
                p.put_back();
            }
        });
    }

    fn button(&mut self, button: MouseButton, pressed: bool) {
        self.send(HostEvent::MouseInput { button, pressed });
    }

    /// One of the window's events routed to the classic interface with the game's world entered
    /// and on show, as its right button needs to look around and to examine, and then a frame.
    /// (With no server the world is never entered, each frame says so again before the window's
    /// events are routed, and the login screen is over the world; so the event is routed here,
    /// between frames, with the login screen taken down.)
    fn send_in_world(&mut self, event: HostEvent) {
        self.shell.queue_window_event(event);
        self.app.probe_mut().host_state_mut().in_world = true;
        {
            let ui = self
                .shell
                .classic
                .ui
                .as_mut()
                .expect("the classic interface");
            let cx = self.app.ui_context();
            let view = cx.hud().view(cx.objects());
            let context = dereth_classic_ui::panels::Context {
                resources: &ui.resources,
                layout: ui.desktop.layout(),
                now: dereth_primitives::LocalTime(0.0),
                game: &view,
                pregame: cx.pregame(),
                keyboard: &ui.keyboard,
                settings: &ui.settings,
                map_teleport_allowed: false,
                classic: &ui.classic,
            };
            ui.desktop.remove_visual("login", &context);
        }
        let now = self.app.clock.tick_ms();
        self.shell.window_input(&mut self.app.ui_context(), now);
        self.frame();
    }

    /// The right button's releases the world has taken: for the end of a camera turn, which
    /// examines nothing, and for a click, which examines what is under the pointer.
    fn releases(&self) -> (u64, u64) {
        let stats = self.app.interaction().stats;
        (stats.mouse_look_releases, stats.examine_searches)
    }

    /// A click of the right button at `(x, y)`, the pointer wobbling within the drag threshold:
    /// it examines.
    fn right_click_examines(&mut self, x: f64, y: f64) {
        let (turns, examines) = self.releases();
        self.pointer_to(x, y);
        self.button(MouseButton::Right, true);
        self.pointer_to(x + 2.0, y + 1.0);
        self.button(MouseButton::Right, false);
        assert_eq!(
            self.releases(),
            (turns, examines + 1),
            "a click at ({x}, {y}) examines"
        );
    }

    /// The right button let go now, after a drag: it examines nothing.
    fn let_go_examines_nothing(&mut self, what: &str) {
        let (turns, examines) = self.releases();
        self.button(MouseButton::Right, false);
        assert_eq!(
            self.releases(),
            (turns + 1, examines),
            "{what}: the release examines nothing"
        );
    }

    /// Where the device input has the pointer: where the interface sees it.
    fn interface_pointer(&self) -> (i32, i32) {
        self.shell
            .shared
            .input
            .as_ref()
            .expect("device input")
            .mouse_pos()
    }

    /// The orbit camera's turn, and its settings.
    fn orbit(&mut self) -> dereth_client_runtime::orbit::OrbitCamera {
        self.app.ui_context().orbit_camera().expect("a body")
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

/// Behaviour: camera.mouse-look.hides-the-pointer-and-shows-it-again-where-it-began
///
/// The game camera's mouse look, held on the right button: a click holds nothing; a drag holds
/// the pointer where the drag has taken it once past the drag threshold, the mouse's movement
/// then moves only the camera's pointer, and letting go shows the pointer where the drag began
/// and tells the interface it is there.
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_game_cameras_mouse_look_holds_the_pointer_once_dragged_and_shows_it_where_it_began() {
    let mut c = Client::new("modern");
    c.pointer_to(400.0, 300.0);
    c.button(MouseButton::Right, true);
    assert!(c.app.mouse_look, "the right button turns the camera");
    c.pointer_to(402.0, 301.0);
    c.button(MouseButton::Right, false);
    assert_eq!(asked(), [], "a click holds nothing");

    c.pointer_to(400.0, 300.0);
    c.button(MouseButton::Right, true);
    c.pointer_to(403.0, 300.0);
    assert_eq!(asked(), [], "within the drag threshold");
    c.pointer_to(420.0, 300.0);
    assert_eq!(asked(), [Asked::Hold((420.0, 300.0))]);
    c.send(HostEvent::PointerMotion { dx: 25.0, dy: 5.0 });
    c.send(HostEvent::PointerMotion { dx: 5.0, dy: 0.0 });
    assert_eq!(
        c.app.last_cursor,
        Some((450.0, 305.0)),
        "the camera follows the mouse"
    );
    assert_eq!(
        c.interface_pointer(),
        (420, 300),
        "the interface's pointer stays where it was held"
    );
    c.pointer_to(700.0, 500.0);
    assert_eq!(
        c.app.last_cursor,
        Some((450.0, 305.0)),
        "the held pointer's own movement is not the camera's"
    );
    c.button(MouseButton::Right, false);
    assert_eq!(
        asked(),
        [Asked::Hold((420.0, 300.0)), Asked::LetGo((400.0, 300.0))]
    );
    assert!(!c.app.mouse_look);
    assert_eq!(c.app.last_cursor, Some((400.0, 300.0)));
    assert_eq!(
        c.interface_pointer(),
        (400, 300),
        "the interface is told the pointer is back where the drag began"
    );
    c.finish();
}

/// The classic interface's mouse look, held on its look key, holds the pointer as the modern
/// interface's does: the camera follows the mouse through the classic interface's own handling of
/// the pointer, and the pointer is shown again where the look began.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn the_classic_interfaces_mouse_look_holds_the_pointer_and_shows_it_where_it_began() {
    let mut c = Client::new("classic");
    c.classic();
    assert_eq!(c.shell.shown_interface(), Interface::Classic);
    c.pointer_to(400.0, 300.0);
    // What the classic interface's look key does, held.
    c.app.mouse_look_button(true);
    c.frame();
    c.pointer_to(440.0, 300.0);
    assert_eq!(asked(), [Asked::Hold((440.0, 300.0))]);
    c.send(HostEvent::PointerMotion { dx: 10.0, dy: -4.0 });
    assert_eq!(
        c.app.last_cursor,
        Some((450.0, 296.0)),
        "the camera follows the mouse"
    );
    c.app.mouse_look_button(false);
    c.frame();
    assert_eq!(
        asked(),
        [Asked::Hold((440.0, 300.0)), Asked::LetGo((400.0, 300.0))]
    );
    assert_eq!(c.app.last_cursor, Some((400.0, 300.0)));
    c.finish();
}

/// A drag of the interface turns no camera and keeps the pointer shown: the left button in the
/// modern interface, and a piece of the Horizon interface's HUD dragged while it is laid out.
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_drag_of_the_interface_keeps_the_pointer() {
    use dereth_horizon::ui::panels::WindowId;
    let mut c = Client::new("interface-drag");
    c.pointer_to(200.0, 200.0);
    c.button(MouseButton::Left, true);
    c.pointer_to(300.0, 260.0);
    c.button(MouseButton::Left, false);
    assert!(!c.app.mouse_look);
    assert_eq!(asked(), []);

    c.horizon(false);
    c.shell
        .horizon
        .ui
        .as_mut()
        .unwrap()
        .ui
        .windows
        .open(WindowId::Layout, 0.0);
    c.frame();
    c.frame();
    let r = c
        .shell
        .horizon
        .ui
        .as_ref()
        .unwrap()
        .ui
        .hud
        .layout
        .outlines
        .iter()
        .find(|(name, _)| *name == "minimap")
        .map(|(_, r)| *r)
        .expect("the minimap's outline");
    let (x, y) = (f64::from(r.x + r.w / 2.0), f64::from(r.y + r.h / 2.0));
    c.pointer_to(x, y);
    c.button(MouseButton::Left, true);
    c.pointer_to(x - 60.0, y + 40.0);
    assert!(!c.app.mouse_look, "a drag of the HUD turns no camera");
    c.button(MouseButton::Left, false);
    assert_eq!(asked(), []);
    c.finish();
}

/// In gamepad mode a drag over the world still turns the camera with the mouse, and the pointer
/// stays shown.
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_drag_in_gamepad_mode_keeps_the_pointer() {
    let mut c = Client::new("gamepad");
    c.horizon(true);
    c.pointer_to(400.0, 250.0);
    c.button(MouseButton::Left, true);
    assert!(c.app.mouse_look, "the drag turns the camera");
    c.pointer_to(480.0, 250.0);
    c.pointer_to(560.0, 260.0);
    c.button(MouseButton::Left, false);
    assert_eq!(asked(), []);
    c.finish();
}

/// The window losing the focus ends the drag: the pointer is shown again where the drag began,
/// and it is not held again until a new drag.
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn losing_the_focus_lets_the_pointer_go() {
    let mut c = Client::new("focus");
    c.pointer_to(400.0, 300.0);
    c.button(MouseButton::Right, true);
    c.pointer_to(440.0, 300.0);
    assert_eq!(asked(), [Asked::Hold((440.0, 300.0))]);
    c.send(HostEvent::Focused(false));
    assert_eq!(
        asked(),
        [Asked::Hold((440.0, 300.0)), Asked::LetGo((400.0, 300.0))]
    );
    assert!(
        !c.app.mouse_look,
        "the camera stops turning with the pointer"
    );
    c.send(HostEvent::Focused(true));
    c.pointer_to(600.0, 400.0);
    assert_eq!(asked().len(), 2, "not held again without a new drag");
    c.finish();
}

/// Another interface shown during a drag ends it: the pointer is shown again where the drag
/// began, the camera stops turning with it, and the interface shown now is told where it is.
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn another_interface_shown_during_a_drag_lets_the_pointer_go() {
    let mut c = Client::new("switch");
    c.horizon(false);
    c.pointer_to(400.0, 250.0);
    c.button(MouseButton::Left, true);
    c.pointer_to(430.0, 250.0);
    assert_eq!(asked(), [Asked::Hold((430.0, 250.0))]);
    c.choose(Interface::Modern);
    assert_eq!(c.shell.shown_interface(), Interface::Modern);
    assert_eq!(
        asked(),
        [Asked::Hold((430.0, 250.0)), Asked::LetGo((400.0, 250.0))]
    );
    assert!(
        !c.app.mouse_look,
        "the camera stops turning with the pointer"
    );
    c.frame();
    assert_eq!(c.app.last_cursor, Some((400.0, 250.0)));
    assert_eq!(c.interface_pointer(), (400, 250));
    c.pointer_to(600.0, 400.0);
    assert_eq!(asked().len(), 2, "not held again without a new drag");
    c.finish();
}

/// The camera's turn from `before` to `after`, in radians, the way a pointer moving right turns
/// it.
fn turned(before: f32, after: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    (before - after + PI).rem_euclid(TAU) - PI
}

/// A host that holds the pointer by putting it back, as the desktop does, reads the mouse's
/// movement off the window's reports of the pointer: the orbit camera turns as far for a pixel of
/// it as for a pixel of the pointer's own movement before it was held, and the report of the
/// pointer put back turns nothing.
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_pointer_held_by_putting_it_back_turns_the_camera_at_the_rate_the_pointer_did() {
    PUTS_BACK.set(true);
    let mut c = Client::with_presentation("put-back", || {
        Box::new(dereth_client_runtime::sim_present::SimPresentation::new(
            800, 600,
        ))
    });
    c.horizon(false);
    c.body();
    c.report(400.0, 250.0);
    c.button(MouseButton::Left, true);
    // The drag's first report is where the camera's turn is measured from; the next, still inside
    // the drag threshold, turns it by the pointer's own movement.
    c.report(401.0, 250.0);
    let yaw = c.orbit().yaw;
    c.report(403.0, 250.0);
    assert_eq!(asked(), [], "inside the drag threshold");
    let per_pixel = turned(yaw, c.orbit().yaw) / 2.0;
    assert!(
        per_pixel.abs() > 1e-6,
        "the pointer's own movement turns the camera"
    );
    c.report(420.0, 250.0);
    assert_eq!(asked(), [Asked::Hold((420.0, 250.0))]);

    // The host put the pointer in the middle of the window when it held it.
    let yaw = c.orbit().yaw;
    c.report(MIDDLE.0, MIDDLE.1);
    assert!(
        turned(yaw, c.orbit().yaw).abs() < 1e-6,
        "the report of the pointer put back turns nothing"
    );
    c.report(MIDDLE.0 + 30.0, MIDDLE.1);
    let held = turned(yaw, c.orbit().yaw) / 30.0;
    assert!(
        (held - per_pixel).abs() < 1e-6,
        "{held} a pixel held, {per_pixel} a pixel before"
    );
    // Put back again after that drain.
    let yaw = c.orbit().yaw;
    c.report(MIDDLE.0, MIDDLE.1);
    assert!(
        turned(yaw, c.orbit().yaw).abs() < 1e-6,
        "the report of the pointer put back turns nothing"
    );
    c.report(MIDDLE.0 - 12.0, MIDDLE.1 + 4.0);
    let back = turned(yaw, c.orbit().yaw) / -12.0;
    assert!(
        (back - per_pixel).abs() < 1e-6,
        "{back} a pixel held, {per_pixel} a pixel before"
    );
    c.button(MouseButton::Left, false);
    assert_eq!(
        asked(),
        [Asked::Hold((420.0, 250.0)), Asked::LetGo((400.0, 250.0))]
    );
    c.finish();
}

/// The client closing during a drag shows the pointer again.
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn closing_the_client_during_a_drag_lets_the_pointer_go() {
    let mut c = Client::new("close");
    c.pointer_to(400.0, 300.0);
    c.button(MouseButton::Right, true);
    c.pointer_to(440.0, 300.0);
    c.finish();
    assert_eq!(
        asked(),
        [Asked::Hold((440.0, 300.0)), Asked::LetGo((400.0, 300.0))]
    );
}

/// Behaviour: ui.pointer.turning-the-camera-with-the-right-button-is-not-an-appraisal
///
/// In the modern interface a click of the right button examines, and a drag of it examines
/// nothing when let go: held and shown again where it began, or, on a host that cannot hold the
/// pointer, brought back there by hand.
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_right_drag_in_the_modern_interface_examines_nothing_where_it_is_let_go() {
    let mut c = Client::new("modern-examine");
    c.right_click_examines(200.0, 150.0);

    c.pointer_to(400.0, 300.0);
    c.button(MouseButton::Right, true);
    c.pointer_to(420.0, 300.0);
    c.send(HostEvent::PointerMotion { dx: 30.0, dy: 4.0 });
    assert_eq!(asked(), [Asked::Hold((420.0, 300.0))]);
    c.let_go_examines_nothing("held");
    assert_eq!(c.interface_pointer(), (400, 300), "shown where it began");

    HOLDS.set(false);
    c.pointer_to(600.0, 200.0);
    c.button(MouseButton::Right, true);
    c.pointer_to(660.0, 210.0);
    c.pointer_to(601.0, 200.0);
    c.let_go_examines_nothing("brought back by hand");
    c.right_click_examines(300.0, 400.0);
    c.finish();
}

/// Behaviour: ui.pointer.turning-the-camera-with-the-right-button-is-not-an-appraisal
///
/// In the Horizon interface, under character-based and camera-based movement alike, a click of
/// the right button over the world examines, and nothing that turns the view examines when the
/// right button is let go: a drag of it, held and shown again where it began; the right button
/// pressed while a drag of the left one holds the pointer, the mouse still; both held and dragged
/// together; and a drag brought back by hand, on a host that cannot hold the pointer and in
/// gamepad mode, where nothing holds it.
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_right_drag_in_the_horizon_interface_examines_nothing_where_it_is_let_go() {
    use dereth_client_runtime::orbit::MovementMode;
    for movement in [MovementMode::Character, MovementMode::Camera] {
        HOLDS.set(true);
        let mut c = Client::new("horizon-examine");
        c.horizon(false);
        let options = &mut c.shell.horizon.ui.as_mut().unwrap().ui.options;
        options.orbit.movement = movement;
        options.pad.movement = movement;
        c.frame();
        c.right_click_examines(200.0, 150.0);

        // The right button dragged: the pointer is held, and shown again where it began.
        c.pointer_to(400.0, 250.0);
        c.button(MouseButton::Right, true);
        c.pointer_to(420.0, 250.0);
        c.send(HostEvent::PointerMotion { dx: 30.0, dy: 4.0 });
        assert_eq!(asked(), [Asked::Hold((420.0, 250.0))], "{movement:?}");
        c.let_go_examines_nothing(&format!("{movement:?}, held"));
        assert_eq!(c.interface_pointer(), (400, 250), "shown where it began");

        // The left button's drag holds the pointer; the right one pressed and let go meanwhile,
        // the mouse still, runs and examines nothing under the hidden pointer.
        c.pointer_to(600.0, 200.0);
        c.button(MouseButton::Left, true);
        c.pointer_to(630.0, 200.0);
        c.button(MouseButton::Right, true);
        c.let_go_examines_nothing(&format!("{movement:?}, with the left held"));
        c.button(MouseButton::Left, false);

        // Both buttons, dragged together, let go the left first.
        c.pointer_to(300.0, 350.0);
        c.button(MouseButton::Right, true);
        c.button(MouseButton::Left, true);
        c.pointer_to(330.0, 350.0);
        c.send(HostEvent::PointerMotion { dx: -40.0, dy: 0.0 });
        c.button(MouseButton::Left, false);
        c.let_go_examines_nothing(&format!("{movement:?}, both"));

        // A host that cannot hold the pointer: the drag goes on with it shown, and it is brought
        // back to where it began.
        HOLDS.set(false);
        c.pointer_to(500.0, 400.0);
        c.button(MouseButton::Right, true);
        c.pointer_to(560.0, 410.0);
        c.pointer_to(501.0, 400.0);
        c.let_go_examines_nothing(&format!("{movement:?}, brought back by hand"));
        c.right_click_examines(250.0, 450.0);
        c.finish();

        // Gamepad mode holds nothing; the same drag brought back by hand.
        HOLDS.set(true);
        let mut c = Client::new("horizon-pad-examine");
        c.horizon(true);
        c.shell.horizon.ui.as_mut().unwrap().ui.options.pad.movement = movement;
        c.frame();
        c.pointer_to(400.0, 250.0);
        c.button(MouseButton::Right, true);
        c.pointer_to(470.0, 250.0);
        c.pointer_to(401.0, 251.0);
        c.let_go_examines_nothing(&format!("{movement:?}, gamepad mode"));
        assert_eq!(asked(), [], "gamepad mode holds nothing");
        c.right_click_examines(200.0, 150.0);
        c.finish();
    }
}

/// Behaviour: ui.pointer.turning-the-camera-with-the-right-button-is-not-an-appraisal
///
/// In the classic interface, with the right button set to look around, a click of it in the
/// world examines and a look examines nothing when let go, though the interface keeps the
/// pointer where the look began.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn a_right_button_look_in_the_classic_interface_examines_nothing_when_let_go() {
    use dereth_classic_ui::keyboard_runtime::{set_classic_bits, RIGHT_CLICK_LOOK};
    let mut c = Client::new("classic-examine");
    c.classic();
    set_classic_bits(RIGHT_CLICK_LOOK);
    let right = |pressed| HostEvent::MouseInput {
        button: MouseButton::Right,
        pressed,
    };

    let (turns, examines) = c.releases();
    c.pointer_to(250.0, 200.0);
    c.send_in_world(right(true));
    c.send_in_world(right(false));
    assert_eq!(c.releases(), (turns, examines + 1), "a click examines");

    c.pointer_to(350.0, 250.0);
    c.send_in_world(right(true));
    assert!(c.app.mouse_look, "the right button looks around");
    c.pointer_to(380.0, 250.0);
    assert_eq!(asked(), [Asked::Hold((380.0, 250.0))]);
    c.send(HostEvent::PointerMotion { dx: 25.0, dy: -5.0 });
    c.send_in_world(right(false));
    assert!(!c.app.mouse_look);
    assert_eq!(
        c.releases(),
        (turns + 1, examines + 1),
        "the look examines nothing"
    );
    c.finish();
}

/// Behaviour: camera.mouse-look.a-drag-is-never-the-first-click-of-a-double-click
///
/// A click at once after a drag, where the drag's held pointer is shown again, is a click and not
/// the second of a double-click: after a drag of the right button it examines, in the modern
/// interface as in Horizon, and after a drag of the left in Horizon it uses nothing, where a
/// second click at once after that click does.
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_click_at_once_after_a_drag_is_a_click_and_not_a_double_click() {
    let mut c = Client::new("drag-then-click");
    let drag = |c: &mut Client, button: MouseButton, x: f64, y: f64| {
        c.pointer_to(x, y);
        c.button(button, true);
        c.pointer_to(x + 30.0, y);
        c.send(HostEvent::PointerMotion { dx: 20.0, dy: 0.0 });
    };
    drag(&mut c, MouseButton::Right, 400.0, 300.0);
    c.let_go_examines_nothing("modern");
    c.right_click_examines(400.0, 300.0);

    c.horizon(false);
    drag(&mut c, MouseButton::Right, 300.0, 250.0);
    c.let_go_examines_nothing("Horizon");
    c.right_click_examines(300.0, 250.0);

    let uses = |c: &Client| c.app.interaction().stats.use_searches;
    let before = uses(&c);
    drag(&mut c, MouseButton::Left, 500.0, 200.0);
    c.button(MouseButton::Left, false);
    assert_eq!(c.interface_pointer(), (500, 200), "shown where it began");
    c.button(MouseButton::Left, true);
    c.button(MouseButton::Left, false);
    assert_eq!(uses(&c), before, "a click after the drag uses nothing");
    c.button(MouseButton::Left, true);
    c.button(MouseButton::Left, false);
    assert_eq!(
        uses(&c),
        before + 1,
        "a second click at once is a double-click"
    );
    c.finish();
}
