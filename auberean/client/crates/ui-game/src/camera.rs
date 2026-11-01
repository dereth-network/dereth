//! The camera's part of the play screen: mouse look and first person as the player switches them
//! (held, toggled) and the game allows them, the crosshair shown in mouse look, the wheel over the
//! world zooming, and the movement the arrow keys and the left button ask for.
//!
//! The player's keys reach it as the interface's actions: the arrows (turn left and right, run
//! forward, back up), the key held for mouse look, the mouse look and first person switches, and
//! the left button. What it decides leaves as [`Request`]s for the client: the camera's switches,
//! a zoom step, a movement action with its extent. A movement key does nothing while an element
//! that takes the keyboard's arrows has it (a text entry, a list).

use auberean_ui::framework::FrameId;
use auberean_ui::manager::global;
use auberean_ui::token::msg;
use auberean_ui::types::button_id;
use auberean_ui::{Handle, Mail, Ui};

/// The play layout's enumeration value (the radar, the selection, the dialogs, the crosshair).
pub const GAMEPLAY_LAYOUT_ENUM: u32 = 0x4100_0009;

mod name {
    use auberean_ui::Token;
    /// The crosshair shown in the middle of the screen in mouse look.
    pub const MOUSE_LOOK_CURSOR_FIELD: Token = Token::of("MouseLookCursorField");
    /// The field behind every panel that covers the world (the selection's).
    pub const ITEM_SELECTION_FIELD: Token = Token::of("ItemSelectionField");
}

/// The interface actions the camera hears.
pub mod action {
    /// The arrows.
    pub const LEFT: u32 = 0x4000_0013;
    pub const RIGHT: u32 = 0x4000_0014;
    pub const UP: u32 = 0x4000_0015;
    pub const DOWN: u32 = 0x4000_0016;
    /// Held for mouse look.
    pub const INSTANT_MOUSE_LOOK: u32 = 0x4000_002C;
    /// The first person switch.
    pub const FIRST_PERSON: u32 = 0x4000_00A2;
    /// The mouse look switch.
    pub const TOGGLE_MOUSE_LOOK: u32 = 0x4000_00BA;
    /// The movement actions it asks for: forward, backward, turn left, turn right.
    pub const FORWARD: u32 = 0x4000_0020;
    pub const BACKWARD: u32 = 0x4000_0021;
    pub const TURN_LEFT: u32 = 0x4000_0024;
    pub const TURN_RIGHT: u32 = 0x4000_0025;
}

/// What the camera asks of the client.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Request {
    /// Mouse look on or off.
    MouseLook(bool),
    /// First person on or off.
    FirstPerson(bool),
    /// A step of the camera's zoom: in (toward the player) or out.
    Zoom { inward: bool },
    /// A movement action with its extent (1 while its key is held, 0 when let go).
    Act { action: u32, extent: f32 },
}

/// The camera's panel: a child framework of the play screen.
#[derive(Debug)]
pub struct Camera {
    frame: FrameId,
    /// The crosshair.
    cursor: Option<Handle>,
    /// How many of the game's reasons forbid mouse look now (it shows only at none).
    cant_mouse_look: i32,
    /// Whether the game allows first person.
    can_first_person: bool,
    /// What the player switched on.
    want_mouse_look: bool,
    want_first_person: bool,
    /// The left button is running forward in mouse look.
    running: bool,
}

impl Camera {
    /// The camera's framework under the play screen's: the crosshair made, the world's field and
    /// the player's buttons and keys listened to, the camera's switches set.
    // D2-01996
    pub fn new(ui: &mut Ui, parent: FrameId, out: &mut Vec<Request>) -> Self {
        let frame = ui.new_child_frame(parent);
        let cursor =
            ui.frame_create_root(frame, GAMEPLAY_LAYOUT_ENUM, name::MOUSE_LOOK_CURSOR_FIELD);
        if let Some(l) = ui.frame_listener(frame) {
            ui.listen_for_message(l, name::ITEM_SELECTION_FIELD, msg::MOUSE_PRESS_TOP);
            for g in [
                global::MOUSE_DOWN,
                global::MOUSE_UP,
                global::KEY_DOWN,
                global::KEY_UP,
            ] {
                ui.listen_for_global_message(l, g);
            }
        }
        let mut me = Self {
            frame,
            cursor,
            cant_mouse_look: 0,
            can_first_person: true,
            want_mouse_look: false,
            want_first_person: false,
            running: false,
        };
        me.update_camera(ui, out);
        me
    }

    /// The camera's framework.
    #[must_use]
    pub const fn frame(&self) -> FrameId {
        self.frame
    }

    /// Whether mouse look is on: switched on, and nothing forbids it.
    #[must_use]
    pub const fn mouse_look(&self) -> bool {
        self.cant_mouse_look == 0 && self.want_mouse_look
    }

    /// Whether first person is on: switched on, and allowed.
    #[must_use]
    pub const fn first_person(&self) -> bool {
        self.can_first_person && self.want_first_person
    }

    /// The camera's switches set from what the player wants and the game allows, and the
    /// crosshair shown only in mouse look.
    // D2-01995
    fn update_camera(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        let look = self.mouse_look();
        out.push(Request::MouseLook(look));
        out.push(Request::FirstPerson(self.first_person()));
        if let Some(c) = self.cursor {
            ui.set_visible(c, look);
        }
    }

    /// A movement action, unless the element with the keyboard takes the arrows itself (a text
    /// entry or a list).
    // D2-01994
    fn send_key(ui: &Ui, action: u32, extent: f32, out: &mut Vec<Request>) {
        if let Some(a) = ui.active() {
            if takes_arrows(ui, a) {
                return;
            }
        }
        out.push(Request::Act { action, extent });
    }

    /// What the camera's listener heard: the wheel over the world zooms; the arrows ask to move
    /// and turn; the left button runs forward in mouse look; the mouse look and first person
    /// switches and the key held for mouse look.
    // D2-01993, D2-01998
    pub fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<Request>) {
        match *mail {
            Mail::Element {
                element,
                what,
                param,
                ..
            } => {
                if element == name::ITEM_SELECTION_FIELD && what == msg::MOUSE_PRESS_TOP {
                    match param {
                        button_id::WHEEL_UP => out.push(Request::Zoom { inward: true }),
                        button_id::WHEEL_DOWN => out.push(Request::Zoom { inward: false }),
                        _ => {}
                    }
                }
            }
            Mail::Global {
                what: global::MOUSE_DOWN,
                param: button_id::LEFT,
            } if self.want_mouse_look => {
                self.running = true;
                Self::send_key(ui, action::FORWARD, 1.0, out);
            }
            Mail::Global {
                what: global::MOUSE_UP,
                param: button_id::LEFT,
            } if self.running => {
                self.running = false;
                Self::send_key(ui, action::FORWARD, 0.0, out);
            }
            Mail::Global { what, param } if what == global::KEY_DOWN || what == global::KEY_UP => {
                let down = what == global::KEY_DOWN;
                let extent = if down { 1.0 } else { 0.0 };
                match param {
                    action::LEFT => Self::send_key(ui, action::TURN_LEFT, extent, out),
                    action::RIGHT => Self::send_key(ui, action::TURN_RIGHT, extent, out),
                    action::UP => Self::send_key(ui, action::FORWARD, extent, out),
                    action::DOWN => Self::send_key(ui, action::BACKWARD, extent, out),
                    // Held: on when pressed (if it is off); let go, switched over, so letting it
                    // go also ends mouse look switched on while it was held.
                    action::INSTANT_MOUSE_LOOK if down => {
                        if !self.want_mouse_look {
                            self.want_mouse_look = true;
                            self.update_camera(ui, out);
                        }
                    }
                    action::INSTANT_MOUSE_LOOK => {
                        self.want_mouse_look = !self.want_mouse_look;
                        self.update_camera(ui, out);
                    }
                    action::TOGGLE_MOUSE_LOOK if down => {
                        self.want_mouse_look = !self.want_mouse_look;
                        self.update_camera(ui, out);
                    }
                    action::FIRST_PERSON if down => {
                        self.want_first_person = !self.want_first_person;
                        self.update_camera(ui, out);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// The game raised (`allowed` false) or lowered one reason that forbids mouse look.
    // D2-01999
    pub fn update_can_mouse_look(&mut self, ui: &mut Ui, allowed: bool, out: &mut Vec<Request>) {
        self.cant_mouse_look += if allowed { -1 } else { 1 };
        self.update_camera(ui, out);
    }

    /// The camera's framework goes: mouse look off, its crosshair and listening gone.
    // D2-01992, D2-01997
    pub fn close(self, ui: &mut Ui, out: &mut Vec<Request>) {
        out.push(Request::MouseLook(false));
        if let Some(c) = self.cursor {
            ui.frame_remove_root(self.frame, c);
            ui.remove(c);
        }
        ui.drop_frame(self.frame);
    }
}

/// Whether an element uses the arrows itself when it has the keyboard: a text element or a list.
#[must_use]
pub fn takes_arrows(ui: &Ui, h: Handle) -> bool {
    use auberean_ui::token::kind;
    ui.node(h).is_some_and(|n| {
        n.kind == kind::TEXT || n.kind == kind::LISTBOX || n.kind == kind::LISTBOX_MULTI_SIZE
    })
}

/// The mail a framework heard, taken.
pub(crate) fn take(ui: &mut Ui, frame: FrameId) -> Vec<Mail> {
    ui.frame_listener(frame)
        .map(|l| ui.take_mail(l))
        .unwrap_or_default()
}

impl Camera {
    /// What the camera's listener heard since the last tick, acted on.
    pub fn tick(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        for m in take(ui, self.frame) {
            self.listen(ui, &m, out);
        }
    }
}
