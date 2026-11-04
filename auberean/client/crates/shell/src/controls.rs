//! The world's controls: the device's keys and buttons turned into actions through the key maps,
//! and the actions in force turned into the player's intent for the body and the camera.
//!
//! A key or button is an input (a key's scan code word, a button's word). Pressed with the
//! modifier keys held (Shift, Ctrl), it names an action in the key maps ([`crate::keys`]); let go,
//! it ends the action it began, whatever was let go of in between. The action map says how each
//! action toggles: a held one is in force from press to release, a latched one from one press to
//! the next (autorun), a single press for its frame (a jump). Going forward or backward, or
//! stopping, ends a latched run. The window losing the keyboard lets every action in force go:
//! the held ones end, a latched run goes on.
//!
//! The modifier actions in force (Shift, Ctrl, Alt) choose each action's values: Shift with
//! forward walks (0.339 of running), with the camera's keys turns them faster. Each frame the
//! movement actions in force sum into the intent (forward and backward, a strafe each way, a turn
//! each way, a jump); in mouse look, when the mouse steers the player, a turn strafes instead. The
//! camera's actions rotate, tilt and zoom it while held and switch its view when pressed. The
//! interface's actions are the interface's, which hears the keys first (its camera asks for the
//! arrows' movement and the left button's run in mouse look as keys would,
//! [`Controls::action_handler`]); one that reaches the world's controls asks for the selection and
//! use the shell makes without an interface.
//!
//! Diagonals are not faster: the body's motion cuts what is asked to length 1.

use std::collections::HashMap;

use auberean_scene::camera::{Held, Press};
use auberean_scene::{movement, Intent};

use crate::input::{Button, Event};
use crate::keys::{is_meta_key, meta_mode, Keys};

/// How an action toggles.
pub mod toggle {
    /// In force from press to release.
    pub const HELD: u32 = 0x4000_0001;
    /// In force from one press to the next.
    pub const LATCH: u32 = 0x4000_0002;
    /// In force for the frame of the press.
    pub const IMPULSE: u32 = 0x4000_0003;
}

/// What an action does, by its class.
pub mod class {
    pub const MOVEMENT: u32 = 0x4000_0001;
    pub const BEHAVIOR: u32 = 0x4000_0002;
    pub const CAMERA: u32 = 0x4000_0004;
    pub const MODIFIER: u32 = 0x4000_0005;
    pub const INTERFACE: u32 = 0x4000_0006;
}

/// A movement action's value: what it moves.
pub mod motion {
    pub const FORWARD: u32 = 0x4000_0001;
    pub const TURN_RIGHT: u32 = 0x4000_0003;
    pub const TURN_LEFT: u32 = 0x4000_0004;
    pub const STRAFE_RIGHT: u32 = 0x4000_0005;
    pub const STRAFE_LEFT: u32 = 0x4000_0006;
    pub const JUMP: u32 = 0x4000_0007;
}

/// The camera actions' values.
pub mod camera {
    pub const SWING_OUT_LEFT: u32 = 0x4000_0028;
    pub const SWING_OUT_RIGHT: u32 = 0x4000_0029;
    pub const SHOULDER: u32 = 0x4000_002A;
    pub const ROTATE_LEFT: u32 = 0x4000_002E;
    pub const ROTATE_RIGHT: u32 = 0x4000_002F;
    pub const RESET: u32 = 0x4000_0030;
    pub const LOOK_AT_GROUND: u32 = 0x4000_00AE;
    pub const LOOK_AT_SELF: u32 = 0x4000_00B1;
    pub const MAP_VIEW: u32 = 0x4000_00B3;
    pub const ZOOM_IN: u32 = 0x4000_00B4;
    pub const ZOOM_OUT: u32 = 0x4000_00B5;
    pub const TILT_UP: u32 = 0x4000_00B6;
    pub const TILT_DOWN: u32 = 0x4000_00B7;
    /// The interface's camera actions the world's controls take when the interface does not.
    pub const FIRST_PERSON: u32 = 0x4000_00A2;
    pub const TOGGLE_MOUSE_LOOK: u32 = 0x4000_00BA;
    pub const INSTANT_ZOOM_IN: u32 = 0x4000_00B8;
    pub const INSTANT_ZOOM_OUT: u32 = 0x4000_00B9;
}

/// The stop action: with forward and backward, pressed it ends a latched run.
const STOP_KEY: u32 = 0x4000_00A3;

/// The interface's actions the world's controls answer when no interface took them.
mod interface {
    pub const ESC: u32 = 0x4000_001E;
    pub const USE: u32 = 0x4100_000D;
    pub const SELECT_NEXT: [u32; 4] = [0x4100_001A, 0x4100_001D, 0x4100_0020, 0x4100_0023];
    pub const SELECT_OFF: u32 = 0x4100_0029;
}

/// The devices an action's input comes from: the keyboard, the mouse, and the interface (an action
/// it asks for as a key would, its input the action itself).
const KEYBOARD: u32 = 0;
const MOUSE: u32 = 2;
pub const INTERFACE: u32 = 1;

/// The buttons' input words: left, right, middle.
pub const LEFT_BUTTON: u32 = 0x8200_040C;
pub const RIGHT_BUTTON: u32 = 0x8200_040D;
pub const MIDDLE_BUTTON: u32 = 0x8200_040E;

/// What the world's controls ask besides moving: selecting and using.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Select the next object around the player (nearest first, then farther each press).
    SelectNext,
    /// Clear the selection.
    SelectNone,
    /// Use the selected object.
    Use,
}

/// An action in force: the action, its extent, the device and input that began it, how it
/// toggles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InForce {
    pub action: u32,
    pub extent: f32,
    pub device: u32,
    pub input: u32,
    pub toggle: u32,
}

/// The modifier keys held, each other input's modifiers at its press, the actions in force, and
/// what was pressed since the last frame.
#[derive(Clone, Debug, Default)]
// D2-03433, D2-03458, D2-03432, D2-03457
pub struct Controls {
    meta: u32,
    pressed: HashMap<u32, u32>,
    actions: Vec<InForce>,
    presses: Vec<Press>,
    /// The movement actions begun since the last frame.
    started: Vec<u32>,
    commands: Vec<Command>,
    mouse: (f32, f32),
    /// Mouse look is on (the camera's switch).
    pub mouse_look: bool,
    /// In mouse look the mouse steers the player (the camera's option): turns strafe.
    pub steers: bool,
    /// The behaviors actions began (`true`) or ended (`false`) since the last frame.
    behaviors: Vec<(u32, bool)>,
    /// A text entry has the keys.
    text_mode: bool,
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
                self.input(keys, MOUSE, button_input(button), down);
                true
            }
            Event::Wheel { notches } => {
                let n = dereth_primitives::num::to_i32(notches.round());
                if n != 0 {
                    self.presses.push(Press::Wheel(n));
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
                // Keys let go while the window had no keyboard never come back up.
                self.meta = 0;
                self.pressed.clear();
                self.focus_switch(keys, false);
                false
            }
            Event::Focus(true) if self.text_mode => {
                self.focus_switch(keys, true);
                false
            }
            _ => false,
        }
    }

    /// A text entry takes the keys, or gives them back. Taking them: each keyboard key held that
    /// names an action is let go as its release would be, and forgotten (its release later finds
    /// nothing), then the actions in force are let go ([`Self::focus_switch`]) with the keyboard
    /// still the window's. Giving them back changes nothing in force.
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
                self.action_handler(keys, action, 0.0, KEYBOARD, input);
            }
        }
        self.focus_switch(keys, true);
    }

    /// Mouse look switched: the focus handlers hear it, and with a text entry having the keys the
    /// actions in force are let go ([`Self::focus_switch`]).
    pub fn mouse_look_switched(&mut self, keys: &Keys) {
        if self.text_mode {
            self.focus_switch(keys, true);
        }
    }

    /// The window's keyboard gone, or (`focus` kept) a text entry having the keys: each action in
    /// force, in the order they began, let go through the action handler as its input would be;
    /// with the keyboard kept, all but the modifier and interface classes'. A held action ends
    /// (and the movement read from the actions in force stops, so the body's next report says so);
    /// a latched run and a single press for this frame stay, as letting go leaves them.
    // D2-03459
    fn focus_switch(&mut self, keys: &Keys, focus: bool) {
        for a in self.actions.clone() {
            let modifiers = self.modifier_bitfield(keys);
            let Some(values) = keys.records.values(a.action, modifiers) else {
                continue;
            };
            if !focus || !matches!(values.class, class::MODIFIER | class::INTERFACE) {
                self.action_handler(keys, a.action, 0.0, a.device, a.input);
            }
        }
    }

    /// An input pressed or let go: its action looked up in the key maps with the modifiers held
    /// at its press (a modifier key's with the others held before it, its release's with none),
    /// then handled.
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
        let value = if down { 1.0 } else { 0.0 };
        self.action_handler(keys, action, value, device, input);
    }

    /// The action in force begun by this input, if any.
    // D2-03441
    fn find(&self, device: u32, input: u32) -> Option<usize> {
        self.actions
            .iter()
            .position(|a| a.device == device && a.input == input)
    }

    /// Whether an action is in force.
    // D2-03443
    #[must_use]
    pub fn action_down(&self, action: u32) -> bool {
        self.actions.iter().any(|a| a.action == action)
    }

    /// The actions in force, in the order they began.
    #[must_use]
    pub fn actions(&self) -> &[InForce] {
        &self.actions
    }

    /// An action pressed (`value` nonzero) or let go: a held action let go ends; a latched one
    /// pressed again ends; otherwise the action is put in force (in the place of the one its input
    /// began before) and begun. Forward, backward and stop end a latched run first.
    // D2-03462, D2-03439
    pub fn action_handler(
        &mut self,
        keys: &Keys,
        action: u32,
        value: f32,
        device: u32,
        input: u32,
    ) {
        let toggle = keys.records.action_map.toggle_type(action);
        if toggle == 0 {
            return;
        }
        if value == 0.0 {
            if toggle == toggle::HELD {
                if let Some(i) = self.find(device, input) {
                    self.actions.remove(i);
                    self.stop_action(keys, action);
                }
            }
            return;
        }
        let found = if toggle == toggle::LATCH {
            if let Some(i) = self.find(device, input) {
                self.actions.remove(i);
                self.stop_action(keys, action);
                return;
            }
            None
        } else {
            if matches!(action, movement::FORWARD | movement::BACKWARD | STOP_KEY) {
                self.turn_off_run_lock();
            }
            self.find(device, input)
        };
        // The extent kept is the value's whole part, as a short.
        let whole = i16::try_from(dereth_primitives::num::to_i32(value.trunc())).unwrap_or(0);
        let info = InForce {
            action,
            extent: f32::from(whole),
            device,
            input,
            toggle,
        };
        match found {
            Some(i) => self.actions[i] = info,
            None => self.actions.push(info),
        }
        self.start_action(keys, action);
    }

    /// The modifiers held, as the modifier actions in force set them (Shift 1, Ctrl 2, Alt 4).
    // D2-03444
    #[must_use]
    pub fn modifier_bitfield(&self, keys: &Keys) -> u32 {
        self.actions
            .iter()
            .filter_map(|a| keys.records.action_map.action_values(a.action, 0))
            .filter(|v| v.class == class::MODIFIER)
            .fold(0, |m, v| m | v.value)
    }

    /// An action begun: a movement action noted as begun (it interrupts a move to a place); a
    /// behavior asked of the player; a camera action that switches the view, pressed; an interface
    /// action, the interface's.
    // D2-03451
    fn start_action(&mut self, keys: &Keys, action: u32) {
        let modifiers = self.modifier_bitfield(keys);
        let Some(v) = keys.records.values(action, modifiers) else {
            return;
        };
        match v.class {
            class::MOVEMENT => self.started.push(action),
            class::BEHAVIOR => self.behaviors.push((v.value, true)),
            // D2-09642
            class::CAMERA => {
                let press = match v.value {
                    camera::RESET => Press::Reset,
                    camera::SHOULDER => Press::Shoulder,
                    camera::LOOK_AT_GROUND => Press::TopDown,
                    camera::LOOK_AT_SELF => Press::Reverse,
                    camera::SWING_OUT_LEFT => Press::SwingLeft,
                    camera::SWING_OUT_RIGHT => Press::SwingRight,
                    camera::MAP_VIEW => Press::MapView,
                    _ => return,
                };
                self.presses.push(press);
            }
            class::INTERFACE => self.interface_action(v.value),
            _ => {}
        }
    }

    /// An action ended: by its plain values, whatever modifiers are held, a behavior is asked to
    /// stop. The camera's held actions and the movement actions are read from the actions in force
    /// each frame, so nothing more is needed for them.
    // D2-03435, D2-09643
    fn stop_action(&mut self, keys: &Keys, action: u32) {
        if let Some(v) = keys.records.action_map.action_values(action, 0) {
            if v.class == class::BEHAVIOR {
                self.behaviors.push((v.value, false));
            }
        }
    }

    /// An interface action no interface took: the camera's first person, mouse look and wheel
    /// steps, and selecting and using, as the shell makes them.
    fn interface_action(&mut self, value: u32) {
        match value {
            camera::FIRST_PERSON => self.presses.push(Press::FirstPerson),
            camera::TOGGLE_MOUSE_LOOK => self.presses.push(Press::MouseLook),
            camera::INSTANT_ZOOM_IN => self.presses.push(Press::Wheel(1)),
            camera::INSTANT_ZOOM_OUT => self.presses.push(Press::Wheel(-1)),
            interface::USE => self.commands.push(Command::Use),
            interface::ESC | interface::SELECT_OFF => self.commands.push(Command::SelectNone),
            v if interface::SELECT_NEXT.contains(&v) => self.commands.push(Command::SelectNext),
            _ => {}
        }
    }

    /// A latched run ended (its action dropped).
    // D2-03454
    fn turn_off_run_lock(&mut self) {
        self.actions.retain(|a| a.action != movement::RUN_LOCK);
    }

    /// Whether a latched run is in force.
    #[must_use]
    pub fn autorun(&self) -> bool {
        self.action_down(movement::RUN_LOCK)
    }

    /// The actions in force for a single press dropped, once the frame has used them.
    // D2-03453, D2-03452
    fn purge_set_actions(&mut self) {
        self.actions.retain(|a| a.toggle != toggle::IMPULSE);
    }

    /// The commands pressed since the last frame.
    pub fn take_commands(&mut self) -> Vec<Command> {
        std::mem::take(&mut self.commands)
    }

    /// The intent of this frame, from the actions in force: each movement action's value by its
    /// extent and its input's (forward and backward summed, strafes summed, a turn the last
    /// one's, in mouse look with steering a strafe), the camera's held actions; the presses and
    /// the mouse motion taken, and the single presses dropped.
    // D2-09649
    pub fn intent(&mut self, keys: &Keys) -> Intent {
        let modifiers = self.modifier_bitfield(keys);
        let (mut forward, mut sideways, mut turn) = (0.0f32, 0.0f32, 0.0f32);
        let mut jump = false;
        let mut held = Held {
            fast: modifiers & 1 != 0,
            ..Held::default()
        };
        for a in &self.actions {
            let Some(v) = keys.records.values(a.action, modifiers) else {
                continue;
            };
            match v.class {
                class::MOVEMENT => {
                    let mut code = v.value;
                    if self.mouse_look && self.steers {
                        code = match code {
                            motion::TURN_RIGHT => motion::STRAFE_RIGHT,
                            motion::TURN_LEFT => motion::STRAFE_LEFT,
                            c => c,
                        };
                    }
                    let e = v.extent * a.extent;
                    match code {
                        motion::FORWARD => forward += e,
                        motion::TURN_RIGHT => turn = -1.0,
                        motion::TURN_LEFT => turn = 1.0,
                        motion::STRAFE_RIGHT => sideways += e,
                        motion::STRAFE_LEFT => sideways -= e,
                        motion::JUMP => jump = true,
                        // The other values are the rows for modifier bit 8, which no modifier
                        // action in the data sets.
                        _ => {}
                    }
                }
                class::CAMERA => match v.value {
                    camera::ROTATE_LEFT => held.rotate_left = true,
                    camera::ROTATE_RIGHT => held.rotate_right = true,
                    camera::TILT_UP => held.tilt_up = true,
                    camera::TILT_DOWN => held.tilt_down = true,
                    camera::ZOOM_IN => held.zoom_in = true,
                    camera::ZOOM_OUT => held.zoom_out = true,
                    _ => {}
                },
                _ => {}
            }
        }
        let presses = std::mem::take(&mut self.presses);
        for p in &presses {
            if *p == Press::MouseLook {
                self.mouse_look = !self.mouse_look;
            }
        }
        self.purge_set_actions();
        Intent {
            forward: forward.clamp(-1.0, 1.0),
            sideways: sideways.clamp(-1.0, 1.0),
            turn,
            jump,
            held,
            presses,
            mouse: std::mem::take(&mut self.mouse),
            started: std::mem::take(&mut self.started),
            behaviors: std::mem::take(&mut self.behaviors),
        }
    }
}
