//! The world's controls on the device's side: the keys and buttons turned into the game's controls
//! through the key maps, and the device's own motion (the mouse, the wheel) kept for the frame.
//!
//! A key or button is an input (a key's scan code word, a button's word). Pressed with the
//! modifier keys held (Shift, Ctrl), it names a control in the key maps ([`crate::keys`]); let go,
//! it lets go of the control it pressed, whatever was let go of in between. The controls go to
//! the client as actions ([`Controls::take_actions`]), which carries them as the action map says
//! each toggles and sums them into each frame's movement and camera; this side never decides what
//! a control does. The window losing the keyboard lets every control in force go: the held ones
//! end, a latched run goes on. A text entry taking the keys lets go of the controls its held keys
//! pressed, then the others but the modifiers' and the interface's.
//!
//! A key bound to one of this client's own actions ([`crate::interface_keys::own`]), which no
//! action map lists, is kept here while held rather than sent to the client: the look-at-front
//! control the orbit camera hears.
//!
//! Each frame the client's [`ControlFrame`], the mouse's motion and the wheel's steps, the mouse
//! buttons held and the own actions held make the player's intent for the body and the camera
//! ([`Controls::intent`]). The interface hears the
//! keys first (its camera asks for the arrows' movement and the left button's run in mouse look
//! as the controls themselves).

use std::collections::HashMap;

use auberean_client_sdk::contract::control::{
    class, CameraPress, Control, ControlFrame, ControlId, ControlSource,
};
use auberean_client_sdk::Action;
use auberean_scene::camera::Press;
use auberean_scene::Intent;

use crate::input::{Button, Event};
use crate::interface_keys::own;
use crate::keys::{is_meta_key, meta_mode, Keys};

/// The devices a control's input comes from: the keyboard and the mouse (the interface's, a
/// control pressed as itself, is [`ControlSource::direct`]'s).
const KEYBOARD: u32 = 0;
const MOUSE: u32 = 2;

/// The buttons' input words: left, right, middle.
pub const LEFT_BUTTON: u32 = 0x8200_040C;
pub const RIGHT_BUTTON: u32 = 0x8200_040D;
pub const MIDDLE_BUTTON: u32 = 0x8200_040E;

/// The modifier keys held, each other input's modifiers at its press, the controls asked since
/// they were last taken, and the device's motion since the last frame.
#[derive(Clone, Debug, Default)]
pub struct Controls {
    meta: u32,
    pressed: HashMap<u32, u32>,
    actions: Vec<Action>,
    /// The wheel's steps since the last frame.
    wheel: Vec<i32>,
    mouse: (f32, f32),
    /// A text entry has the keys.
    text_mode: bool,
    /// The mouse buttons the world's controls heard held: left, right.
    buttons: (bool, bool),
    /// This client's own actions held, each by the input that pressed it.
    own: Vec<(u32, u32)>,
    /// The interface's actions the inputs began (`true`) and ended since they were last taken, in
    /// order: the game's interface actions, and this client's own.
    keyed: Vec<(u32, bool)>,
}

/// Whether an input word is one of the mouse's.
const fn is_mouse_input(input: u32) -> bool {
    input >> 24 == LEFT_BUTTON >> 24
}

const fn button_input(b: Button) -> u32 {
    match b {
        Button::Left => LEFT_BUTTON,
        Button::Right => RIGHT_BUTTON,
        Button::Middle => MIDDLE_BUTTON,
    }
}

/// A camera control the client pressed, as the scene's camera takes it.
const fn press_of(p: CameraPress) -> Press {
    match p {
        CameraPress::Reset => Press::Reset,
        CameraPress::Shoulder => Press::Shoulder,
        CameraPress::FirstPerson => Press::FirstPerson,
        CameraPress::TopDown => Press::TopDown,
        CameraPress::Reverse => Press::Reverse,
        CameraPress::SwingLeft => Press::SwingLeft,
        CameraPress::SwingRight => Press::SwingRight,
        CameraPress::MapView => Press::MapView,
        CameraPress::MouseLook => Press::MouseLook,
        CameraPress::Zoom(n) => Press::Wheel(n),
    }
}

impl Controls {
    /// Take one event. Returns whether the world's controls used it.
    pub fn event(&mut self, e: &Event, keys: &Keys) -> bool {
        match *e {
            Event::Key { key, down, repeat } => {
                if !repeat {
                    self.input(keys, KEYBOARD, key.input_word(), down);
                }
                true
            }
            Event::Button { button, down } => {
                match button {
                    Button::Left => self.buttons.0 = down,
                    Button::Right => self.buttons.1 = down,
                    Button::Middle => {}
                }
                self.input(keys, MOUSE, button_input(button), down);
                true
            }
            Event::Wheel { notches } => {
                let n = dereth_primitives::num::to_i32(notches.round());
                if n != 0 {
                    self.wheel.push(n);
                }
                true
            }
            // D2-03438, D2-09639
            Event::Motion { dx, dy } => {
                self.mouse.0 += dx;
                self.mouse.1 += dy;
                true
            }
            Event::Focus(false) => {
                self.let_go();
                false
            }
            Event::Focus(true) if self.text_mode => {
                self.actions.push(Action::ReleaseControls { typing: true });
                false
            }
            Event::Focus(_)
            | Event::Text(_)
            | Event::Composing(_)
            | Event::Pointer { .. }
            | Event::Resized { .. }
            | Event::Close => false,
        }
    }

    /// A text entry takes the keys, or gives them back. Taking them: each keyboard key held that
    /// names a control lets it go as its release would, and is forgotten (its release later finds
    /// nothing), then the controls in force but the modifiers' and the interface's are let go,
    /// with the keyboard still the window's. Giving them back changes nothing in force.
    // D2-04871, D2-04945, D2-04868
    pub fn set_text_mode(&mut self, keys: &Keys, on: bool) {
        if on == self.text_mode {
            return;
        }
        self.text_mode = on;
        if !on {
            return;
        }
        let held: Vec<(u32, u32)> = self
            .pressed
            .iter()
            .map(|(input, meta)| (*input, *meta))
            .filter(|(input, _)| !is_mouse_input(*input))
            .collect();
        for (input, meta) in held {
            if let Some(action) = keys.stack.action(input, meta) {
                self.pressed.remove(&input);
                self.send(action, false, KEYBOARD, input);
            }
        }
        self.actions.push(Action::ReleaseControls { typing: true });
    }

    /// Everything held let go, as when the window loses the keyboard: the keys and buttons held
    /// forgotten (a key let go meanwhile never comes back up), and every control in force let go.
    pub fn let_go(&mut self) {
        self.meta = 0;
        self.pressed.clear();
        self.buttons = (false, false);
        self.own.clear();
        self.actions.push(Action::ReleaseControls { typing: false });
    }

    /// Mouse look switched: with a text entry having the keys, the controls in force but the
    /// modifiers' and the interface's are let go.
    pub fn mouse_look_switched(&mut self) {
        if self.text_mode {
            self.actions.push(Action::ReleaseControls { typing: true });
        }
    }

    /// An input pressed or let go: its control looked up in the key maps with the modifiers held
    /// at its press (a modifier key's with the others held before it, its release's with none),
    /// then pressed or let go.
    // D2-04947, D2-04944, D2-04961
    fn input(&mut self, keys: &Keys, device: u32, input: u32, down: bool) {
        let meta = if is_meta_key(input) {
            let before = self.meta;
            if down {
                self.meta |= meta_mode(input);
                before
            } else {
                self.meta &= !meta_mode(input);
                0
            }
        } else if down {
            *self.pressed.entry(input).or_insert(self.meta)
        } else {
            match self.pressed.remove(&input) {
                Some(m) => m,
                None => return,
            }
        };
        let Some(action) = keys.stack.action(input, meta) else {
            return;
        };
        let interface = own::is_own(action)
            || keys
                .records
                .values(action, meta)
                .is_some_and(|v| v.class == class::INTERFACE);
        if interface {
            self.keyed.push((action, down));
        }
        self.send(action, down, device, input);
    }

    /// The interface's actions the inputs began and ended since this was last taken, in order:
    /// each action and whether it began.
    pub fn take_keyed(&mut self) -> Vec<(u32, bool)> {
        std::mem::take(&mut self.keyed)
    }

    /// A control pressed or let go by an input; one of this client's own actions kept while held.
    fn send(&mut self, action: u32, down: bool, device: u32, input: u32) {
        if own::is_own(action) {
            self.own.retain(|&(_, i)| i != input);
            if down {
                self.own.push((action, input));
            }
            return;
        }
        let id = ControlId(action);
        let c = if down {
            Control::begin(id)
        } else {
            Control::end(id)
        };
        self.actions
            .push(Action::Control(c.from(ControlSource { device, input })));
    }

    /// The controls asked since they were last taken, in order, for the client.
    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }

    /// The intent of this frame: the client's movement and camera from its controls in force, the
    /// camera controls it pressed and then the wheel's steps, and the mouse's motion, taken.
    // D2-09649
    pub fn intent(&mut self, frame: ControlFrame) -> Intent {
        let mut presses: Vec<Press> = frame.presses.into_iter().map(press_of).collect();
        presses.extend(self.wheel.drain(..).map(Press::Wheel));
        Intent {
            forward: frame.forward,
            sideways: frame.sideways,
            turn: frame.turn,
            jump: frame.jump,
            held: frame.camera,
            presses,
            mouse: std::mem::take(&mut self.mouse),
            started: frame.started,
            behaviors: frame.behaviors,
            buttons: self.buttons,
            front_view: self.own_held(own::LOOK_AT_FRONT),
        }
    }

    /// A mouse button let go where the world's controls did not hear it (over the interface): no
    /// longer held for the orbit camera.
    pub fn button_released(&mut self, button: Button) {
        match button {
            Button::Left => self.buttons.0 = false,
            Button::Right => self.buttons.1 = false,
            Button::Middle => {}
        }
    }

    /// Whether one of this client's own actions is held.
    #[must_use]
    pub fn own_held(&self, action: u32) -> bool {
        self.own.iter().any(|&(a, _)| a == action)
    }
}
