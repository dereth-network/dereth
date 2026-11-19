//! The controls in force: the action map's actions pressed and let go (by a window's keys, an
//! interface, a script, a headless driver or a plugin, all alike), carried as the action map says
//! each toggles, and summed each frame into what they ask of the player's body and camera.
//!
//! A control pressed is put in force, in the place of the one its source began before: a held one
//! until its release, a latched one until it is pressed again (autorun), a single press for its
//! frame (a jump). Going forward or backward, or stopping, ends a latched run. A release finds the
//! press its source began, whatever was pressed in between.
//!
//! The modifier controls in force (Shift, Ctrl, Alt) choose each control's values: Shift with
//! forward walks (0.339 of running), with the camera's controls turns them faster. Each frame the
//! movement controls in force sum into the frame's movement (forward and backward, a strafe each
//! way, a turn each way, a jump); in mouse look, when the mouse steers the player, a turn strafes
//! instead. The camera's controls rotate, tilt and zoom it while held and switch its view when
//! pressed. The interface's controls that reach here ask for the camera's first person, mouse look
//! and zoom steps, and for using and selecting, which the client then carries out as their
//! actions.
//!
//! Diagonals are not faster: the body's motion cuts what is asked to length 1.

use auberean_assets::ui::{ActionBindings, ActionValues};
use auberean_client_contract::control::{
    camera, class, interface, motion, movement, toggle, CameraHeld, CameraPress, Control,
    ControlFrame, ControlId, ControlSource,
};
use auberean_client_contract::Action;

/// The action map's record.
pub const ACTION_MAP: u32 = 0x5100_0000;

/// A control in force: the control, its extent, the source that began it, how it toggles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InForce {
    pub id: ControlId,
    pub extent: f32,
    pub source: ControlSource,
    pub toggle: u32,
}

/// The action map, the controls in force, and what was pressed since the last frame.
#[derive(Clone, Debug, Default)]
// D2-03433, D2-03458, D2-03432, D2-03457
pub struct Controls {
    /// The action map (empty until it is read), and whether it has been.
    map: ActionBindings,
    has_map: bool,
    in_force: Vec<InForce>,
    presses: Vec<CameraPress>,
    /// The movement controls begun since the last frame.
    started: Vec<ControlId>,
    /// What the interface's controls pressed ask besides the camera, for the client to carry out.
    asked: Vec<Action>,
    /// The behaviors controls began (`true`) or ended (`false`) since the last frame.
    behaviors: Vec<(u32, bool)>,
    /// Mouse look is on: the camera's switch, which a mouse look control's press flips and the
    /// host's camera sets.
    pub mouse_look: bool,
    /// In mouse look the mouse steers the player (the camera's option): turns strafe.
    pub steers: bool,
}

impl Controls {
    /// Controls over an action map.
    #[must_use]
    pub fn over(map: ActionBindings) -> Self {
        Self {
            map,
            has_map: true,
            ..Self::default()
        }
    }

    /// Whether the action map has been read.
    #[must_use]
    pub const fn has_map(&self) -> bool {
        self.has_map
    }

    /// The action map the controls carry out from now on.
    pub fn set_map(&mut self, map: ActionBindings) {
        self.map = map;
        self.has_map = true;
    }

    /// The action map (empty before it is read).
    #[must_use]
    pub const fn map(&self) -> &ActionBindings {
        &self.map
    }

    /// How a control toggles; 0 for one the action map does not list.
    #[must_use]
    pub fn toggle(&self, id: ControlId) -> u32 {
        self.map().toggle_type(id.0)
    }

    /// What a control does with these modifier bits held: the entry for exactly them, else
    /// without the bit 8, else with none.
    // D2-03449
    #[must_use]
    pub fn values(&self, id: ControlId, modifiers: u32) -> Option<ActionValues> {
        values(self.map(), id.0, modifiers)
    }

    /// A control pressed or let go: a held one let go ends; a latched one pressed again ends;
    /// otherwise it is put in force (in the place of the one its source began before) and begun.
    /// Forward, backward and stop end a latched run first. A control the action map does not
    /// list does nothing.
    // D2-03462, D2-03439
    pub fn control(&mut self, c: Control) {
        let toggle = self.toggle(c.id);
        if toggle == 0 {
            return;
        }
        let value = c.value();
        if value == 0.0 {
            if toggle == toggle::HELD {
                if let Some(i) = self.find(c.source) {
                    self.in_force.remove(i);
                    self.stop(c.id);
                }
            }
            return;
        }
        let found = if toggle == toggle::LATCH {
            if let Some(i) = self.find(c.source) {
                self.in_force.remove(i);
                self.stop(c.id);
                return;
            }
            None
        } else {
            if matches!(
                c.id,
                movement::FORWARD | movement::BACKWARD | movement::STOP
            ) {
                self.turn_off_run_lock();
            }
            self.find(c.source)
        };
        // The extent kept is the value's whole part, as a short.
        let whole = i16::try_from(dereth_primitives::num::to_i32(value.trunc())).unwrap_or(0);
        let info = InForce {
            id: c.id,
            extent: f32::from(whole),
            source: c.source,
            toggle,
        };
        match found {
            Some(i) => self.in_force[i] = info,
            None => self.in_force.push(info),
        }
        self.start(c.id);
    }

    /// Every control in force, in the order they began, let go as its source's release would be;
    /// with `typing` (a text entry has the keys), all but the modifier and interface classes'. A
    /// held control ends (and the movement read from the controls in force stops, so the body's
    /// next report says so); a latched run and a single press for this frame stay, as letting go
    /// leaves them.
    // D2-03459
    pub fn release(&mut self, typing: bool) {
        for a in self.in_force.clone() {
            let Some(values) = self.values(a.id, self.modifiers()) else {
                continue;
            };
            if !typing || !matches!(values.class, class::MODIFIER | class::INTERFACE) {
                self.control(Control::end(a.id).from(a.source));
            }
        }
    }

    /// The control in force begun by this source, if any.
    // D2-03441
    fn find(&self, source: ControlSource) -> Option<usize> {
        self.in_force.iter().position(|a| a.source == source)
    }

    /// Whether a control is in force.
    // D2-03443
    #[must_use]
    pub fn is_down(&self, id: ControlId) -> bool {
        self.in_force.iter().any(|a| a.id == id)
    }

    /// The controls in force, in the order they began.
    #[must_use]
    pub fn in_force(&self) -> &[InForce] {
        &self.in_force
    }

    /// The modifiers held, as the modifier controls in force set them (Shift 1, Ctrl 2, Alt 4).
    // D2-03444
    #[must_use]
    pub fn modifiers(&self) -> u32 {
        self.in_force
            .iter()
            .filter_map(|a| self.map().action_values(a.id.0, 0))
            .filter(|v| v.class == class::MODIFIER)
            .fold(0, |m, v| m | v.value)
    }

    /// A control begun: a movement control noted as begun (it interrupts a move to a place); a
    /// behavior asked of the player; a camera control that switches the view, pressed; an
    /// interface control, carried out.
    // D2-03451
    fn start(&mut self, id: ControlId) {
        let Some(v) = self.values(id, self.modifiers()) else {
            return;
        };
        match v.class {
            class::MOVEMENT => self.started.push(id),
            class::BEHAVIOR => self.behaviors.push((v.value, true)),
            // D2-09642
            class::CAMERA => {
                let press = match v.value {
                    camera::RESET => CameraPress::Reset,
                    camera::SHOULDER => CameraPress::Shoulder,
                    camera::LOOK_AT_GROUND => CameraPress::TopDown,
                    camera::LOOK_AT_SELF => CameraPress::Reverse,
                    camera::SWING_OUT_LEFT => CameraPress::SwingLeft,
                    camera::SWING_OUT_RIGHT => CameraPress::SwingRight,
                    camera::MAP_VIEW => CameraPress::MapView,
                    _ => return,
                };
                self.presses.push(press);
            }
            class::INTERFACE => self.interface_value(v.value),
            _ => {}
        }
    }

    /// A control ended: by its plain values, whatever modifiers are held, a behavior is asked to
    /// stop. The camera's held controls and the movement controls are read from the controls in
    /// force each frame, so nothing more is needed for them.
    // D2-03435, D2-09643
    fn stop(&mut self, id: ControlId) {
        if let Some(v) = self.map().action_values(id.0, 0) {
            if v.class == class::BEHAVIOR {
                self.behaviors.push((v.value, false));
            }
        }
    }

    /// An interface control that reached the controls: the camera's first person, mouse look and
    /// zoom steps; using, selecting nothing and the selection keys, asked as their actions.
    fn interface_value(&mut self, value: u32) {
        match value {
            camera::FIRST_PERSON => self.presses.push(CameraPress::FirstPerson),
            camera::TOGGLE_MOUSE_LOOK => self.presses.push(CameraPress::MouseLook),
            camera::INSTANT_ZOOM_IN => self.presses.push(CameraPress::Zoom(1)),
            camera::INSTANT_ZOOM_OUT => self.presses.push(CameraPress::Zoom(-1)),
            v => self.asked.extend(interface::action(v)),
        }
    }

    /// What the interface's controls pressed since this was last taken ask, in order: using and
    /// selecting, as their actions.
    pub fn take_asked(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.asked)
    }

    /// A latched run ended (its control dropped): by going forward, backward or stopping, by a
    /// teleport, and each frame the player's body makes a move on its own, so the run does not go
    /// on when it arrives.
    // D2-03454
    pub fn turn_off_run_lock(&mut self) {
        self.in_force.retain(|a| a.id != movement::RUN_LOCK);
    }

    /// Whether a latched run is in force.
    #[must_use]
    pub fn autorun(&self) -> bool {
        self.is_down(movement::RUN_LOCK)
    }

    /// The controls in force for a single press dropped, once the frame has used them.
    // D2-03453, D2-03452
    fn purge_set_actions(&mut self) {
        self.in_force.retain(|a| a.toggle != toggle::IMPULSE);
    }

    /// This frame's movement and camera from the controls in force: each movement control's value
    /// by its extent and its press's (forward and backward summed, strafes summed, a turn the last
    /// one's, in mouse look with steering a strafe), the camera's held controls; what was pressed
    /// since the last frame taken, a mouse look press flipping mouse look, and the single presses
    /// dropped.
    // D2-09649
    pub fn frame(&mut self) -> ControlFrame {
        let modifiers = self.modifiers();
        let (mut forward, mut sideways, mut turn) = (0.0f32, 0.0f32, 0.0f32);
        let mut jump = false;
        let mut held = CameraHeld {
            fast: modifiers & 1 != 0,
            ..CameraHeld::default()
        };
        for a in &self.in_force {
            let Some(v) = values(self.map(), a.id.0, modifiers) else {
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
                        // control in the data sets.
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
            if *p == CameraPress::MouseLook {
                self.mouse_look = !self.mouse_look;
            }
        }
        self.purge_set_actions();
        ControlFrame {
            forward: forward.clamp(-1.0, 1.0),
            sideways: sideways.clamp(-1.0, 1.0),
            turn,
            jump,
            camera: held,
            presses,
            started: std::mem::take(&mut self.started),
            behaviors: std::mem::take(&mut self.behaviors),
        }
    }
}

/// What an action does in an action map with these modifier bits held: the entry for exactly
/// them, else without the bit 8, else with none.
#[must_use]
pub fn values(map: &ActionBindings, action: u32, modifiers: u32) -> Option<ActionValues> {
    map.action_values(action, modifiers)
        .or_else(|| {
            (modifiers & 8 != 0)
                .then(|| map.action_values(action, modifiers & !8))
                .flatten()
        })
        .or_else(|| map.action_values(action, 0))
}
