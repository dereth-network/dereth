//! [`Hands`] -- the keyboard, the modifier state and the wheel, for the shell's own stations.
//!
//! The shell is the options pages, the key bindings, the character-creation wizard, the dialog
//! boxes and the text entry. Most of its scenarios drive the client from the *keyboard*, and each
//! needs the same three helpers -- a [`Pump`] with `is_ready` and `is_active_app` set, a millisecond clock that only moves forwards,
//! and a `send` that dispatches each message twice: once through the pump's own state and once into
//! the client's real input manager, which is the pair of calls the window loop makes.
//!
//! [`crate::adapters_chat::Hand`] is the same gesture for the chat entry and is deliberately
//! left alone here: it owns its pump privately, so it cannot say *Alt is held* -- which is the whole
//! subject of the system keys and of the full-screen toggle -- and it has no wheel at all. What is
//! added here is those two plus the holds: a key held down across frames is how the client's own
//! repeat sweep is reached, and a tap can never reach it.
//!
//! It is the shell subject's adapter, so it lives in its own file rather than on
//! [`crate::HeadlessClient`] or [`crate::Player`].
//!
//! # Naming a key
//!
//! [`Hands::key`] takes the host's own resolved [`Key`] -- the virtual key and the scan code -- and
//! not a name, for [`crate::adapters_chat`]'s reason: naming a key is the window layer's job and
//! this crate must not take a second opinion on it. A scenario in the test tiers gets one from
//! `dereth_client::platform::window::key_from_key_code`, which is where `winit` stops.
//!
//! `Assets::Retail` with the shell up, always: the input manager is part of the shell.

use dereth_client::platform::keys::Key;
use dereth_client::pump::{Pump, Win32Message};
use dereth_ui::{ElemHandle, ElementId};

use crate::HeadlessClient;

/// The player's hands: one on the keyboard, one on the wheel.
///
/// It owns a [`Pump`] because a Windows keyboard message carries state the pump keeps -- whether
/// Alt is held decides between the `WM_KEY*` and `WM_SYSKEY*` families -- and a message built
/// without it is not the message the client would have received.
#[derive(Debug)]
pub struct Hands {
    pump: Pump,
    /// The message clock, in milliseconds. It only ever moves forwards, which is what lets the
    /// client tell one press from a repeat of it.
    time: u32,
    /// Whether the host has reported Alt as held. It is the pump's own bit -- set through
    /// [`Hands::alt`] and read back by [`Hands::alt_is_down`] -- and it decides both the
    /// `WM_SYSKEY*` family and the `WM_SYSCHAR` one.
    alt: bool,
}

impl Default for Hands {
    fn default() -> Self {
        Self::new()
    }
}

impl Hands {
    /// Hands on a client that is up and has the focus.
    #[must_use]
    pub fn new() -> Self {
        let mut pump = Pump::new();
        pump.state.is_ready = true;
        pump.state.is_active_app = true;
        Self {
            pump,
            time: 400_000,
            alt: false,
        }
    }

    /// The millisecond stamp of the last message sent.
    #[must_use]
    pub const fn now_ms(&self) -> u32 {
        self.time
    }

    /// One Windows message, through the pump and then into the client's own input manager --
    /// the pair of calls the real message loop makes.
    ///
    /// # Panics
    /// Panics on a client built without the UI shell, which is where the input manager is.
    pub fn send(&mut self, c: &mut HeadlessClient, m: Win32Message) {
        self.pump.dispatch(m);
        c.app_mut()
            .input_manager_mut()
            .expect("the keyboard needs the input shell, so build the scenario with the shell up")
            .on_message(m);
    }

    /// Whether Alt is held, which is what chooses `WM_SYSKEY*` over `WM_KEY*`.
    #[must_use]
    pub const fn alt_is_down(&self) -> bool {
        self.alt
    }

    /// Say that Alt went down or came up, the way the host's modifier report does.
    ///
    /// It is not a key press: `ModifiersChanged` produces no message at all, it only sets the bit
    /// the next key message is built against. A scenario that wants the Alt key *itself* pressed
    /// sends [`Hands::key`] with the left-Alt key as well.
    pub fn alt(&mut self, down: bool) {
        self.alt = down;
        self.time += 10;
        let msgs = self.pump.map_window_event(
            &dereth_client::platform::window::HostEvent::ModifiersChanged { alt: down },
            self.time,
        );
        debug_assert!(msgs.is_empty(), "a modifier report is not itself a message");
    }

    /// One key transition, as a key going down or coming up.
    pub fn key(&mut self, c: &mut HeadlessClient, k: Key, pressed: bool) {
        self.time += 10;
        let m = self.pump.key_message_for_key(k, pressed, self.time);
        self.send(c, m);
    }

    /// A key pressed and released, with a frame after each edge.
    ///
    /// **A tap is not a hold.** A down and an up inside one frame cancel each other before the
    /// frame's sweep looks, so a scenario whose claim is about a key that *does* something while
    /// it is held has to use [`Hands::hold`] instead -- which is the trap every movement
    /// scenario has to know about.
    pub fn tap(&mut self, c: &mut HeadlessClient, k: Key) {
        self.key(c, k, true);
        c.tick(1);
        self.key(c, k, false);
        c.tick(1);
    }

    /// A key held down for `frames` whole frames and then released, with one frame after the
    /// release. The client's own repeat sweep is reached this way and no other.
    pub fn hold(&mut self, c: &mut HeadlessClient, k: Key, frames: u64) {
        self.key(c, k, true);
        c.tick(frames);
        self.key(c, k, false);
        c.tick(1);
    }

    /// The key down, with nothing released -- for the claims whose subject is what happens
    /// *during* the hold and which therefore read the frames themselves.
    pub fn press(&mut self, c: &mut HeadlessClient, k: Key) {
        self.key(c, k, true);
    }

    /// The key let go.
    pub fn release(&mut self, c: &mut HeadlessClient, k: Key) {
        self.key(c, k, false);
    }

    /// One character, as the message a key press produces after `TranslateMessage`.
    ///
    /// `WM_SYSCHAR` while Alt is held, which is the family the client's own character handler
    /// distinguishes.
    pub fn character(&mut self, c: &mut HeadlessClient, ch: char) {
        self.time += 10;
        let message = if self.alt {
            dereth_input::win32::msg::WM_SYSCHAR
        } else {
            dereth_input::win32::msg::WM_CHAR
        };
        let m = Win32Message::new(message, ch as usize, 0, self.time);
        self.send(c, m);
    }

    /// Type `text` into whatever holds the keyboard, one character at a time, then run a frame.
    ///
    /// # Panics
    /// Panics on a character outside plain ASCII, for [`crate::adapters_chat::Hand::type_text`]'s
    /// reason: the wide-character path is a claim of its own, and a scenario that typed one here
    /// would be asserting over the code page of whatever machine ran it.
    pub fn type_text(&mut self, c: &mut HeadlessClient, text: &str) {
        for ch in text.chars() {
            assert!(
                ch.is_ascii(),
                "this gesture is deliberately ASCII: {text:?}"
            );
            self.character(c, ch);
        }
        c.tick(1);
    }

    /// The wheel turned `notches` clicks -- positive away from the player, as Windows signs it.
    ///
    /// The `wParam` is built by the client's own mapping (`WHEEL_DELTA` is 120 and rides in the
    /// high word), so a scenario never writes that arithmetic out.
    pub fn wheel(&mut self, c: &mut HeadlessClient, notches: f32) {
        self.time += 10;
        let msgs = self.pump.map_window_event(
            &dereth_client::platform::window::HostEvent::MouseWheel { notches },
            self.time,
        );
        assert_eq!(msgs.len(), 1, "one wheel report is one WM_MOUSEWHEEL");
        for m in msgs {
            self.send(c, m);
        }
    }

    /// Put the pointer somewhere without pressing anything, which is what the wheel and the
    /// tooltip both need: they act on whatever is under the cursor.
    pub fn move_to(&mut self, c: &mut HeadlessClient, x: i32, y: i32) {
        self.time += 10;
        let m = self
            .pump
            .mouse_move_message(f64::from(x), f64::from(y), self.time);
        self.send(c, m);
    }
}

/// The handle of `id` under whichever screen is current -- any screen, not only the gameplay one.
///
/// The shell's own stations run on the login screen, the character-creation wizard, the options
/// pages and the dialog boxes as often as on gameplay, so the walk starts from the current
/// screen's roots and not from a downcast to one screen type.
///
/// # Panics
/// Panics when no screen is current, or when the shipped layout does not carry `id`: an element a
/// scenario names and the layout does not have is a wrong id, and answering `None` would let the
/// scenario read it as "absent, as expected".
#[must_use]
pub fn element(c: &HeadlessClient, id: ElementId) -> ElemHandle {
    let app = c.view().expect_app();
    let shell = app.ui().expect("the UI shell is up");
    let roots = shell
        .flow
        .current()
        .expect("a screen is current")
        .roots()
        .to_vec();
    roots
        .iter()
        .find_map(|r| {
            if shell.ui.node(*r).is_some_and(|n| n.element_id() == id) {
                Some(*r)
            } else {
                shell.ui.get_child_recursive(*r, id)
            }
        })
        .unwrap_or_else(|| panic!("{id:?} is not under this screen's roots"))
}

/// Whether the shipped layout carries `id` under the current screen at all.
#[must_use]
pub fn has_element(c: &HeadlessClient, id: ElementId) -> bool {
    let app = c.view().expect_app();
    let Some(shell) = app.ui() else { return false };
    let Some(screen) = shell.flow.current() else {
        return false;
    };
    screen.roots().iter().any(|r| {
        shell.ui.node(*r).is_some_and(|n| n.element_id() == id)
            || shell.ui.get_child_recursive(*r, id).is_some()
    })
}

// ---------------------------------------------------------------------------------------------
// The client a shell scenario has to own outright
// ---------------------------------------------------------------------------------------------

/// How to build an `App` a scenario owns by value.
///
/// **Why this exists beside [`crate::ClientSpec`].** Three of the shell's claims are about the
/// client's own *lifetime* rather than about what a frame did, and [`crate::HeadlessClient`]
/// deliberately cannot express any of them:
///
/// | the claim | what the harness does instead |
/// |---|---|
/// | a quit request makes the frame answer false and stop early | [`crate::HeadlessClient::tick`] asserts the frame continued, because for every other scenario a client that shut itself down is a defect |
/// | the bounded run loop stops after the frames it was asked for | the loop is `App::run`, which the harness has no step for |
/// | the shutdown sequence's last step released the presentation, and the keymap step wrote the file | [`crate::HeadlessClient::shutdown`] takes the `App` by value and drops the log it answers with |
///
/// Each is a harness gap, stated rather than hidden. What is *not*
/// duplicated is the configuration: this is the same `App::with_platform(cfg, NullPresentation,
/// Platform::headless(w, h))` the harness builds, with the two fields these claims need -- the
/// frame bound and a real preferences directory -- opened up.
#[derive(Debug, Clone)]
pub struct AppSpec {
    /// Bring the UI shell up.
    pub shell: bool,
    /// Put the gameplay screen up and settle it for this many frames.
    pub gameplay_frames: Option<u64>,
    /// Load the default static scene before the screens.
    pub static_scene: bool,
    /// `Config::frames` -- the `--frames n` bound `App::run` stops at.
    pub frames: Option<u64>,
    /// The player's `Display.FullScreen` preference, as the shipped settings file carries it.
    ///
    /// It is a preference and not a state: the client keeps it and applies it on
    /// entering the world, so a scenario about full screen has to be able to start a client that
    /// *asked* for it and then watch what the client does with the request. [`crate::ClientSpec`]
    /// has no field for it, which is why this one does.
    pub full_screen: bool,
    /// Where the client keeps the player's preferences.
    ///
    /// `None` is **nowhere**: the empty path, which is the client's own "no preferences file"
    /// state -- it loads no key map and no saved layout of whoever is running the test, and it
    /// writes neither on the way out. A scenario whose claim is that something *was* written
    /// passes a scratch directory of its own; see [`scratch_preferences`].
    pub preferences_file: Option<std::path::PathBuf>,
    pub width: u32,
    pub height: u32,
}

impl Default for AppSpec {
    fn default() -> Self {
        Self {
            shell: false,
            gameplay_frames: None,
            static_scene: false,
            frames: None,
            full_screen: false,
            preferences_file: None,
            width: 800,
            height: 600,
        }
    }
}

impl AppSpec {
    /// The shell up and the gameplay screen settled.
    #[must_use]
    pub fn in_gameplay(frames: u64) -> Self {
        Self {
            shell: true,
            gameplay_frames: Some(frames),
            ..Self::default()
        }
    }
}

/// Build the `App` [`AppSpec`] describes.
///
/// # Panics
/// Panics when the retail dats under `$DERETH_TEST_DAT_DIR` are absent, which is a scenario with no
/// oracle rather than a client in a state worth asserting over.
#[must_use]
pub fn build_app(spec: &AppSpec) -> dereth_client::app::App {
    use dereth_client::app::{App, Platform};
    use dereth_client::config::Config;
    use dereth_client::present::NullPresentation;

    let mut cfg = Config {
        headless: true,
        frames: spec.frames,
        // No socket is ever opened, exactly as the harness's own builder promises.
        connect: false,
        sound: false,
        ui: spec.shell,
        width: spec.width,
        height: spec.height,
        dat_dir: dereth_dat::testing::dat_dir(),
        preferences_file: spec.preferences_file.clone().unwrap_or_default(),
        ..Config::default()
    };
    cfg.display.full_screen = spec.full_screen;
    let mut app = App::with_platform(
        cfg,
        Box::new(NullPresentation::new(spec.width, spec.height)),
        Platform::headless(spec.width, spec.height),
    )
    .unwrap_or_else(|e| {
        panic!("a headless client needs the retail dats under $DERETH_TEST_DAT_DIR and nothing else: {e}")
    });
    if spec.shell {
        app.start_shell().expect("the UI shell comes up");
    }
    if spec.static_scene {
        let s = dereth_client::world::SceneConfig {
            landblock: app.config().landblock,
            land_radius: app.config().land_radius,
            scenery_radius: app.config().scenery_radius,
            ..dereth_client::world::SceneConfig::default()
        };
        app.load_static_scene(s).expect("the static scene loads");
    }
    if let Some(frames) = spec.gameplay_frames {
        app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
        for _ in 0..frames {
            assert!(
                app.frame(),
                "the client shut itself down while settling the gameplay screen"
            );
        }
    }
    app
}

/// A scratch preferences file under the temporary directory, namespaced by `name`.
///
/// The client puts its `.keymap` and its layout beside `UserPreferences.ini`, so a scenario whose
/// claim is that one of them was written points the configuration at a directory of its own --
/// never at the player's own preferences folder.
///
/// # Panics
/// Panics when the directory cannot be created.
#[must_use]
pub fn scratch_preferences(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("dere-scenario-shell").join(name);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir.join("UserPreferences.ini")
}

// ---------------------------------------------------------------------------------------------
// The keyboard with no client behind it
// ---------------------------------------------------------------------------------------------

/// A keyboard wired to the shipped key maps and to nothing else.
///
/// **Why it is not a client.** A claim about *what a key is bound to* has to be read off the
/// registrations the client makes at start-up and no others: a whole client with a screen up has
/// that screen's maps registered on top, and an action that arrives is then evidence about the
/// screen rather than about the shipped map. So this is the client's own input shell over the
/// retail tables, driven by the client's own pump, and the messages go nowhere else.
///
/// It opens the retail data files, so it belongs to the `dat` tier.
#[derive(Debug)]
pub struct BareKeyboard {
    pump: Pump,
    /// The client's own input shell over the shipped `ActionMap` and both key maps.
    pub input: dereth_client::input::InputShell,
    time: u32,
}

impl Default for BareKeyboard {
    fn default() -> Self {
        Self::new()
    }
}

impl BareKeyboard {
    /// Open the shipped tables and put a pump in front of them.
    ///
    /// # Panics
    /// Panics when the retail data files under `$DERETH_TEST_DAT_DIR` are absent or the shipped tables
    /// will not decode -- a keyboard with no key map is a scenario with no oracle.
    #[must_use]
    pub fn new() -> Self {
        let store = dereth_dat::testing::open_store()
            .expect("the shipped key maps live in the retail data files: set DERETH_TEST_DAT_DIR");
        let input = dereth_client::input::InputShell::new(&store, None)
            .expect("the shipped action map and both key maps decode");
        let mut pump = Pump::new();
        pump.state.is_ready = true;
        pump.state.is_active_app = true;
        Self {
            pump,
            input,
            time: 1_000,
        }
    }

    /// Press and release `k`, and answer the actions the press produced, in order.
    ///
    /// The release is delivered and swept too, so that the next call starts from a keyboard with
    /// nothing held -- which a station driving a dozen keys in a row depends on.
    pub fn tap(&mut self, k: Key) -> Vec<u32> {
        let started = self.edge(k, true);
        let _ = self.edge(k, false);
        started
    }

    /// One transition, and the actions that *began* with it.
    pub fn edge(&mut self, k: Key, pressed: bool) -> Vec<u32> {
        self.time += 100;
        let m = self.pump.key_message_for_key(k, pressed, self.time);
        self.pump.dispatch(m);
        self.input.on_message(m);
        self.input
            .use_time(dereth_primitives::LocalTime(f64::from(self.time) / 1000.0));
        self.input
            .take_events()
            .into_iter()
            .filter(|e| e.start)
            .map(|e| e.action.0)
            .collect()
    }
}

// ---------------------------------------------------------------------------------------------
// The options panel
// ---------------------------------------------------------------------------------------------

impl Hands {
    /// Click an element the scenario already holds, at the middle of its drawn box.
    ///
    /// **The hit test has to resolve to that element exactly**, and that is not pedantry: a press
    /// is delivered to whatever the hit test names, and a widget's own message handler opens by
    /// refusing anything that is not itself, so a press that lands on a child never presses the
    /// widget. A gesture that accepted a descendant would be measuring something that cannot reach
    /// the thing under test either way.
    ///
    /// # Panics
    /// Panics when the element is clipped to nothing, or when the middle of it hit-tests to
    /// something else -- with what it did hit, so the failure names the element in the way.
    pub fn click_handle(&mut self, c: &mut HeadlessClient, h: ElemHandle) {
        let (x, y) = {
            let app = c.view().expect_app();
            let ui = &app.ui().expect("the UI shell is up").ui;
            let b = ui.screen_clip_box(h);
            assert!(
                b.is_valid(),
                "the element is clipped to nothing, so no pointer can reach it"
            );
            let (x, y) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
            let hit = ui.hit_test_screen(x, y);
            assert_eq!(
                hit.and_then(|x| ui.node(x))
                    .map(dereth_ui::ElementNode::element_id),
                ui.node(h).map(dereth_ui::ElementNode::element_id),
                "the pointer at ({x}, {y}) must land on the element it aims at"
            );
            (x, y)
        };
        self.click_at(c, x, y);
    }

    /// The pointer leaving the window altogether.
    ///
    /// It is the one message that clears what the pointer is over, and a
    /// scenario about the hit test has to be able to say it; building it by hand needs the
    /// gesture clock this type owns.
    pub fn leave(&mut self, c: &mut HeadlessClient) {
        self.time += 10;
        let m = Win32Message::new(dereth_input::win32::msg::WM_MOUSELEAVE, 0, 0, self.time);
        self.send(c, m);
    }
    /// One mouse-button transition, as a message, for a gesture that has to read the client
    /// between the press and the release -- which is the only way to see what an element looks
    /// like *while* it is pressed.
    ///
    /// # Panics
    /// Panics on a button the client's own table does not name.
    pub fn button_message(
        &mut self,
        button: dereth_client::platform::keys::MouseButton,
        pressed: bool,
    ) -> Win32Message {
        self.time += 10;
        self.pump
            .button_message(button, pressed, self.time)
            .expect("that button is one of the messages the client's table names")
    }

    /// Press a row of a list: the pointer goes to the middle of the row's drawn box, and the hit
    /// has to land inside `list`.
    ///
    /// **A row is not what the hit test names** -- the list is, and it works out from where the
    /// pointer was which of its rows was pressed. So this is the one press whose requirement is
    /// the container and not the element, which is why it cannot go through
    /// [`Hands::click_handle`].
    ///
    /// # Panics
    /// Panics when the row is clipped to nothing, or when the press would land outside the list.
    pub fn click_row(&mut self, c: &mut HeadlessClient, list: ElemHandle, row: ElemHandle) {
        let (x, y) = {
            let app = c.view().expect_app();
            let ui = &app.ui().expect("the UI shell is up").ui;
            let b = ui.screen_clip_box(row);
            assert!(
                b.is_valid(),
                "the row is clipped to nothing, so no pointer can reach it"
            );
            let (x, y) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
            let hit = ui
                .hit_test_screen(x, y)
                .expect("the pointer lands on something");
            assert!(
                hit == list || ui.is_ancestor_of(list, hit),
                "a press in the middle of the row must land inside the list"
            );
            (x, y)
        };
        self.click_at(c, x, y);
    }

    /// Click one point of the screen, with no hit test of its own -- for the gestures whose point
    /// is a position rather than an element, where the scenario has already proved what is there.
    pub fn click_at(&mut self, c: &mut HeadlessClient, x: i32, y: i32) {
        use dereth_client::platform::keys::MouseButton;
        self.time += 10;
        let m = self
            .pump
            .mouse_move_message(f64::from(x), f64::from(y), self.time);
        self.send(c, m);
        for down in [true, false] {
            self.time += 10;
            let m = self
                .pump
                .button_message(MouseButton::Left, down, self.time)
                .expect("the left button is one of the messages the client's table names");
            self.send(c, m);
        }
        // Two frames: one takes the gesture and raises what it asked for, the next runs it and
        // lets a page's own per-frame poll read the model again.
        c.tick(2);
    }
}

/// Open the toolbar panel that holds `page`, then that page's own tab, by clicking what a player
/// clicks -- and answer the page's handle.
///
/// **Neither the panel nor the tab is named.** The page's handle is walked up to whichever panel
/// contains it, the toolbar button for that panel is found by the panel it opens, and the tab
/// comes out of the panel's own page-to-tab table. A scenario that named a button id would be
/// asserting over a number rather than over the route a player takes.
///
/// # Panics
/// Panics when the page is not bound, when it is in no toolbar panel, when no toolbar button opens
/// that panel, when the panel's table does not name the page, or when the page does not come up.
pub fn open_options_page(
    c: &mut HeadlessClient,
    hands: &mut Hands,
    page: ElemHandle,
) -> ElemHandle {
    use dereth_ui_screens::screens::gameplay::GamePlayScreen;

    let with_screen =
        |c: &mut HeadlessClient,
         f: &mut dyn FnMut(&mut dereth_ui::UiSystem, &mut GamePlayScreen)| {
            let shell = c.app_mut().ui_mut().expect("the UI shell is up");
            let screen = shell.flow.current_mut().expect("a screen is current");
            let any: &mut dyn std::any::Any = &mut **screen;
            let gameplay = any
                .downcast_mut::<GamePlayScreen>()
                .expect("the gameplay screen");
            f(&mut shell.ui, gameplay);
        };

    let mut found: Option<(u32, ElemHandle)> = None;
    with_screen(c, &mut |ui, s| {
        let info = s
            .panels
            .pages
            .iter()
            .copied()
            .find(|p| {
                let mut h = Some(page);
                while let Some(cur) = h {
                    if cur == p.handle {
                        return true;
                    }
                    h = ui.parent(cur);
                }
                false
            })
            .expect("the page sits inside a toolbar panel");
        found = Some((info.panel_id, info.handle));
    });
    let (panel_id, container) = found.expect("the panel");

    let mut button = None;
    with_screen(c, &mut |_, s| {
        button = s
            .toolbar
            .buttons
            .iter()
            .find(|b| b.panel_id == panel_id)
            .map(|b| b.handle);
    });
    hands.click_handle(c, button.expect("the toolbar has a button for that panel"));

    let mut tab = None;
    with_screen(c, &mut |ui, _| {
        let id = ui
            .node(container)
            .and_then(|n| {
                n.behaviour
                    .as_ref()?
                    .as_any()?
                    .downcast_ref::<dereth_ui::widgets::panel::Panel>()
            })
            .and_then(|p| p.page_to_tab.get(&page_element_of(ui, page)).copied())
            .expect("the panel's tab table names this page");
        tab = ui.get_child_recursive(container, id);
    });
    hands.click_handle(c, tab.expect("the tab caption element"));

    // The page fills its rows from the model when it is shown, and this client queues that off the
    // visibility message and drains it in the host, so the values are not there until a frame or
    // two after the tab click. Everything a scenario reads is one of those values.
    c.tick(4);
    assert!(
        c.view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .is_visible(page),
        "the page must be up before anything read off it means anything"
    );
    page
}

/// The shipped element id of a live handle.
fn page_element_of(ui: &dereth_ui::UiSystem, h: ElemHandle) -> ElementId {
    ui.node(h)
        .map(dereth_ui::ElementNode::element_id)
        .expect("the page is live")
}
