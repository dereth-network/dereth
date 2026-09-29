//! [`Hand`] -- typing a line into the shipped chat entry, the way a player's hand does it.
//!
//! Many chat scenarios drive the client through the chat entry, and the gesture is always the
//! same: click the entry so that hit-testing
//! decides it was hit, one `WM_CHAR` per character, then the return key down and up, with a frame
//! between the runs. The reason not to use the shorter road -- [`crate::Player::Say`], which
//! queues the chat line directly -- is worth keeping:
//!
//! > a test that hands the request straight to the interaction cannot tell a wired command from a
//! > command whose entry box the player cannot reach.
//!
//! So the gesture is worth having, and it is worth having **once**. It is a subject adapter, so it
//! lives here rather than on [`crate::HeadlessClient`].
//!
//! It is `Assets::Retail` only, with the shell up: the input manager and the chat entry are both
//! part of it.

use dereth_client::platform::keys::{Key, MouseButton};
use dereth_client::pump::{Pump, Win32Message};
use dereth_ui::{ElemHandle, ElementId};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

use crate::HeadlessClient;

/// The shipped chat entry -- the element the client itself fills when a tell is started.
pub const CHAT_ENTRY: ElementId = ElementId(0x1000_0016);

/// A point a player could really put the pointer on to hit `h`: the middle of its drawn, clipped
/// box when the hit test resolves there, otherwise the first point of a coarse grid over that box
/// which does.
///
/// **The scan is not a convenience.** A shipped element is often partly covered by a sibling drawn
/// after it, and its middle can be under that sibling while most of it is clickable -- the social
/// page's toolbar button is exactly that case. Clicking the middle alone would report an element a
/// player reaches every day as unreachable, which is a false negative and not a finding. What is
/// not relaxed is the requirement: the point must hit `h` or something inside it, and this panics
/// with what it did hit when no point does.
///
/// # Panics
/// Panics when the element is clipped to nothing, or when nothing in its box reaches it.
#[must_use]
pub fn point_on(c: &HeadlessClient, h: ElemHandle) -> (i32, i32) {
    let app = c.view().expect_app();
    let ui = &app.ui().expect("the UI shell is up").ui;
    let b = ui.screen_clip_box(h);
    assert!(
        b.is_valid(),
        "the element is clipped to nothing, so no pointer can reach it"
    );
    let ok = |x: i32, y: i32| {
        ui.hit_test_screen(x, y)
            .is_some_and(|hit| hit == h || ui.is_ancestor_of(h, hit))
    };
    let (cx, cy) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
    if ok(cx, cy) {
        return (cx, cy);
    }
    for j in 0..=8 {
        for i in 0..=8 {
            let (x, y) = (b.x0 + (b.x1 - b.x0) * i / 8, b.y0 + (b.y1 - b.y0) * j / 8);
            if ok(x, y) {
                return (x, y);
            }
        }
    }
    panic!(
        "no point in {:?}'s drawn box {b:?} hit-tests to it; its middle hits {:?}",
        ui.node(h).map(dereth_ui::ElementNode::element_id),
        ui.hit_test_screen(cx, cy)
            .and_then(|x| ui.node(x))
            .map(dereth_ui::ElementNode::element_id),
    );
}

/// The player's hand on the keyboard and the mouse.
///
/// It owns a [`Pump`] because a Windows message carries a time and a state that the pump keeps: a
/// mouse message built without one is not the message the client would have received.
#[derive(Debug)]
pub struct Hand {
    pump: Pump,
    /// The message clock, in milliseconds. It only ever moves forwards, which is what lets the
    /// client tell one click from a double one.
    time: u32,
}

impl Default for Hand {
    fn default() -> Self {
        Self::new()
    }
}

impl Hand {
    /// A hand on a client that is up and has the focus.
    #[must_use]
    pub fn new() -> Self {
        let mut pump = Pump::new();
        pump.state.is_ready = true;
        pump.state.is_active_app = true;
        Self {
            pump,
            time: 400_000,
        }
    }

    /// One Windows message, through the pump and then into the client's own input manager --
    /// which is the pair of calls the real message loop makes.
    ///
    /// # Panics
    /// Panics on a client built without the UI shell, which is where the input manager is.
    pub fn send(&mut self, c: &mut HeadlessClient, m: Win32Message) {
        self.pump.dispatch(m);
        c.app_mut()
            .input_manager_mut()
            .expect("typing needs the input shell, so build the scenario with the shell up")
            .on_message(m);
    }

    /// Click the middle of an element: the pointer moves on to it and the left button goes down
    /// and up. Hit-testing decides what was hit, exactly as it does for a player.
    ///
    /// # Panics
    /// Panics when the element is not in the shipped tree.
    pub fn click_element(&mut self, c: &mut HeadlessClient, id: ElementId) {
        let h = {
            let app = c.view().expect_app();
            let shell = app.ui().expect("the UI shell is up");
            let any: &dyn std::any::Any = shell.flow.current().expect("a screen is up");
            let root = any
                .downcast_ref::<GamePlayScreen>()
                .expect("the gameplay screen")
                .root()
                .expect("the gameplay screen's root");
            shell
                .ui
                .get_child_recursive(root, id)
                .expect("the element is in the shipped layout")
        };
        self.click_handle(c, h);
    }

    /// The same gesture on an element the scenario already holds -- a row of a live list, say,
    /// which has no id of its own to look up.
    ///
    /// # Panics
    /// Panics when the element is not alive.
    pub fn click_handle(&mut self, c: &mut HeadlessClient, h: ElemHandle) {
        let (x, y) = point_on(c, h);
        let (x, y) = (f64::from(x), f64::from(y));
        self.time += 10;
        let m = self.pump.mouse_move_message(x, y, self.time);
        self.send(c, m);
        for down in [true, false] {
            self.time += 10;
            let m = self
                .pump
                .button_message(MouseButton::Left, down, self.time)
                .expect("the left button has a message");
            self.send(c, m);
        }
        c.tick(1);
    }

    /// One key transition, as a key going down or coming up.
    ///
    /// It takes the host's own resolved key -- the virtual key and the scan code -- rather than a
    /// name, because naming a key is the window layer's job and this crate must not take a second
    /// opinion on it. A scenario gets one from `dereth_client::platform::window::key_from_key_code`.
    pub fn key(&mut self, c: &mut HeadlessClient, key: Key, pressed: bool) {
        self.time += 10;
        let m = self.pump.key_message_for_key(key, pressed, self.time);
        self.send(c, m);
    }

    /// A key pressed and released, with a frame after each edge.
    pub fn tap(&mut self, c: &mut HeadlessClient, k: Key) {
        self.key(c, k, true);
        c.tick(1);
        self.key(c, k, false);
        c.tick(1);
    }

    /// One printable character, as the message a key press produces after it is translated.
    pub fn character(&mut self, c: &mut HeadlessClient, ch: char) {
        self.time += 10;
        let m = Win32Message::new(dereth_input::win32::msg::WM_CHAR, ch as usize, 0, self.time);
        self.send(c, m);
    }

    /// The pointer moved to a point and the left button put down, with no release.
    ///
    /// It is the half of a press a scenario needs when something has to change between the two
    /// edges -- which is the only way to reach a guard that is tested on the release.
    pub fn mouse_down_at(&mut self, c: &mut HeadlessClient, x: i32, y: i32) {
        self.time += 10;
        let m = self
            .pump
            .mouse_move_message(f64::from(x), f64::from(y), self.time);
        self.send(c, m);
        self.time += 10;
        let m = self
            .pump
            .button_message(MouseButton::Left, true, self.time)
            .expect("the left button has a message");
        self.send(c, m);
        c.tick(1);
    }

    /// The left button let go where it was last put down.
    pub fn mouse_up(&mut self, c: &mut HeadlessClient) {
        self.time += 10;
        let m = self
            .pump
            .button_message(MouseButton::Left, false, self.time)
            .expect("the left button has a message");
        self.send(c, m);
        c.tick(1);
    }

    /// A press at one point on the screen, with no hit test of its own.
    ///
    /// It is for the gestures whose point is a position rather than an element -- a slider pressed
    /// a tenth of the way along, say -- where the scenario has already proved what is under it.
    pub fn press_at(&mut self, c: &mut HeadlessClient, x: i32, y: i32) {
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
                .expect("the left button has a message");
            self.send(c, m);
        }
        // One frame takes the gesture, the next runs whatever it asked for.
        c.tick(2);
    }

    /// Press a row of a list: the pointer goes to the middle of the row's drawn box, and the hit
    /// has to land inside `list`.
    ///
    /// A row is not itself what the hit test names -- the list is, and it works out which row was
    /// pressed from where the pointer was -- so this is the one press whose requirement is the
    /// container and not the element.
    ///
    /// # Panics
    /// Panics when the row is clipped to nothing or the press would land outside the list.
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
            (f64::from(x), f64::from(y))
        };
        self.time += 10;
        let m = self.pump.mouse_move_message(x, y, self.time);
        self.send(c, m);
        c.tick(1);
        for down in [true, false] {
            self.time += 10;
            let m = self
                .pump
                .button_message(MouseButton::Left, down, self.time)
                .expect("the left button has a message");
            self.send(c, m);
        }
        c.tick(2);
    }

    /// Type `text` into whatever has the keyboard, one character at a time.
    ///
    /// # Panics
    /// Panics on a character outside plain ASCII. The wide-character path is a claim of its own and
    /// a scenario that typed one here would be asserting over the code page of whatever machine ran
    /// it -- the same reason the chat-room scenarios are deliberately ASCII.
    pub fn type_text(&mut self, c: &mut HeadlessClient, text: &str) {
        for byte in text.bytes() {
            assert!(
                byte.is_ascii(),
                "this gesture is deliberately ASCII: {text:?}"
            );
            self.time += 10;
            let m = Win32Message::new(
                dereth_input::win32::msg::WM_CHAR,
                usize::from(byte),
                0,
                self.time,
            );
            self.send(c, m);
        }
        c.tick(1);
    }

    /// Press and release the return key, which is what sends the line.
    pub fn press_return(&mut self, c: &mut HeadlessClient) {
        for (msg, lparam) in [
            (dereth_input::win32::msg::WM_KEYDOWN, 0x001c_0001_u32),
            (dereth_input::win32::msg::WM_KEYUP, 0xc01c_0001_u32),
        ] {
            self.time += 10;
            #[allow(clippy::cast_possible_wrap)]
            let m = Win32Message::new(msg, 13, lparam as isize, self.time);
            self.send(c, m);
        }
        c.tick(1);
    }

    /// The whole gesture: click the chat entry, type the line, press return.
    pub fn say(&mut self, c: &mut HeadlessClient, text: &str) {
        self.click_element(c, CHAT_ENTRY);
        self.type_text(c, text);
        self.press_return(c);
    }
}

// ---------------------------------------------------------------------------------------------
// The private messages the recordings carry
// ---------------------------------------------------------------------------------------------

/// Every private message the recorded shards sent, as `(recording, blob)` -- the bytes the shard
/// put on the wire, with the leading opcode dword, which is `SessionEvent::UiEvent`'s own contract.
///
/// **Why this reads the raw recordings and not the decoded corpus.** `dereth_client_net::client_session::testing::Corpus`
/// is the index every count in this crate is read from, and it carries **no** `0x02BD` at all: the
/// derivation that builds it leaves private messages out, which is the right thing for an index
/// that is read by name. The claims about *who* a private message is addressed to have no other
/// oracle -- a synthesised message would be this crate agreeing with itself -- so they are taken
/// off the datagrams, one layer below, through the same reader every replaying scenario uses.
///
/// Nothing of what the messages *say* leaves this function: the scenarios that call it assert over
/// names, ids and byte equality.
///
/// The whole set is replayed once per test binary and held, because eighteen recordings through a
/// real session is seconds rather than milliseconds.
///
/// # Panics
/// Panics when a recording named by the corpus index is missing or does not parse, which is a
/// broken checkout rather than a state the client could be in.
#[must_use]
pub fn recorded_private_messages() -> &'static [(String, Vec<u8>)] {
    static ALL: std::sync::OnceLock<Vec<(String, Vec<u8>)>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        let mut out = Vec::new();
        for name in dereth_client_net::client_session::testing::session_names() {
            let records = dereth_client_net::client_session::testing::capture::load_session(name)
                .unwrap_or_else(|e| panic!("the recording {name} does not parse: {e}"));
            let sequence =
                dereth_headless::capture::connection_sequence_number(&records).unwrap_or(0);
            let mut net = dereth_client::net::ClientNetwork::new(
                &dereth_client_net::client_session::testing::capture::peer(0).to_string(),
                7304,
                "dereth-testkit",
                "unused",
                sequence,
            )
            .expect("the replay endpoint");
            let mut objects = dereth_client::objects::ObjectStream::new();
            let mut entered = false;
            for r in &records {
                let now = dereth_primitives::LocalTime(r.t);
                if !r.c2s {
                    net.feed(
                        &r.raw,
                        dereth_client_net::client_session::testing::capture::peer(r.pair),
                        now,
                    );
                }
                net.tick(now);
                let _ = net.take_outgoing();
                for e in objects.pump(&mut net, now) {
                    if let dereth_client_net::client_session::SessionEvent::CharacterSet(set) = &e {
                        if !entered {
                            if let Some(c) = set.characters.first() {
                                let account = set.account.clone();
                                net.enter_world(c.gid, &account);
                                entered = true;
                            }
                        }
                    }
                    if let dereth_client_net::client_session::SessionEvent::UiEvent {
                        opcode,
                        blob,
                    } = e
                    {
                        if opcode == dereth_protocol::Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH {
                            out.push(((*name).to_owned(), blob));
                        }
                    }
                }
            }
        }
        out
    })
}
