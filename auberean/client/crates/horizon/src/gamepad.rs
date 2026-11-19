//! Gamepad mode: the game played with a pad. Out of it the pad does nothing but switch it on (any
//! button pressed or trigger pulled, or a stick pushed well over: that press does nothing else);
//! a key pressed switches it off again. The mouse may be used beside it.
//!
//! In the world (no window open):
//! * **Left stick**: the movement controls, as the keys would press them, so the orbit camera's
//!   movement scheme moves the player by them. **Right stick**: the camera's rotate and tilt
//!   controls; with **LB** held, its up and down zoom the camera in and out instead.
//! * **LT / RT held**: the left or the right cross hotbar ([`crate::ui::cross`]): the d-pad and
//!   the face buttons use its eight places, the first eight slots of the shortcut bar's set shown
//!   (LT) or of the row over it (RT). **RB held** with a face button or the d-pad shows the set it
//!   picks (Y, B, A, X the first four, the d-pad up, right, down, left the next four).
//! * **A**: use the selection; with none, select the nearest creature. **B**: let go of the
//!   selection and stop. **X**: the map. **Y**: jump, for as long as it is held.
//! * **D-pad up and down**: the fellowship's members in turn (the player, with none). **Left and
//!   right**: the creature before or after the one selected.
//! * **LB** tapped: autorun on or off. **The left stick pressed in**: start or stop attacking.
//! * **Start**: the settings.
//!
//! On a window (or a question), the d-pad moves the focus to the nearest control that way within
//! it, and the pointer with it; **A** presses what the focus is on, **B** closes the window on top
//! (or the chat entry), **LB / RB** turn its tabs, the **right stick** scrolls it, and **Back**
//! takes the focus to the next window open.
//!
//! On the screens before the world, the d-pad moves the focus over the screen's controls and the
//! characters' cards, and the pointer with it; **A** presses what the focus is on, **B** is
//! Escape and **Start** is Enter.
//!
//! Every control the pad presses is pressed from the pad's own source, so a key held at once is
//! not let go by it; leaving the world or gamepad mode lets go of all of them.

use auberean_client_sdk::contract::control::{movement, Control, ControlId, ControlSource};
use auberean_client_sdk::contract::{SelectKind, TOOLBAR_SLOTS};
use auberean_client_sdk::Action;
use auberean_shell::interface::{CameraRequest, Request};
use auberean_shell::pad::{PadButton as B, PadState};
use dereth_ui_kit::input::{vk, InputFrame};
use dereth_ui_kit::nav::{first_in, Cursor, Dir, Stepped};

use crate::pad::{CrossSet, PadDriver, PadSettings, PadStep};
use crate::state::GameState;
use crate::ui::HorizonUi;

/// The device a control the pad presses comes from: neither the keyboard's nor the mouse's, nor
/// the interface's own.
pub const PAD_DEVICE: u32 = 3;

/// The camera's controls the right stick holds.
pub mod camera {
    use auberean_client_sdk::contract::control::ControlId;

    pub const ROTATE_LEFT: ControlId = ControlId(0x4000_002E);
    pub const ROTATE_RIGHT: ControlId = ControlId(0x4000_002F);
    pub const TILT_UP: ControlId = ControlId(0x4000_00B6);
    pub const TILT_DOWN: ControlId = ControlId(0x4000_00B7);
}

/// How far along an axis, of the push's whole length, a stick must point for that axis's control
/// to be held: so a diagonal holds two, a push nearly straight one.
pub const AXIS: f32 = 0.38;

/// How far the right stick, eased, must be pushed to turn the camera.
pub const LOOK: f32 = 0.05;

/// How far the right stick must be pushed up or down, with LB held, to zoom; and how often, in
/// seconds, it zooms a step while it is.
pub const ZOOM: f32 = 0.2;
pub const ZOOM_EVERY: f64 = 0.08;

/// Wheel notches a second the right stick pushed all the way scrolls a window.
pub const SCROLL_RATE: f32 = 12.0;

/// How many places a cross hotbar has.
pub const CROSS_PLACES: u32 = 8;

/// Gamepad mode's state across frames.
#[derive(Debug, Default)]
pub struct PadMode {
    /// Gamepad mode is on.
    pub on: bool,
    pub settings: PadSettings,
    driver: PadDriver,
    /// The focus over a window.
    pub cursor: Cursor,
    /// The controls the pad holds.
    held: Vec<ControlId>,
    /// The fellowship's member the d-pad selected last, by its place.
    fellow: Option<usize>,
    /// Seconds until the right stick zooms again.
    zoom_wait: f64,
    /// The right stick's scroll short of a whole notch.
    scroll: f32,
    /// The cross hotbar half held up last frame.
    pub half: Option<CrossSet>,
}

/// What a frame of the pad asks: the client's actions and what is asked of the shell.
#[derive(Debug, Default, PartialEq)]
pub struct PadAsked {
    pub acts: Vec<Action>,
    pub requests: Vec<Request>,
}

/// The source of a control the pad presses.
const fn source(id: ControlId) -> ControlSource {
    ControlSource {
        device: PAD_DEVICE,
        input: id.0,
    }
}

impl PadMode {
    /// One frame of `pad` (none with no pad), `dt` seconds after the last, over the interface
    /// `ui` showing `state` in the world, its pointer and buttons in `input`.
    pub fn frame(
        &mut self,
        pad: Option<PadState>,
        dt: f64,
        ui: &mut HorizonUi,
        state: &GameState,
        input: &mut InputFrame,
    ) -> PadAsked {
        let step = self.driver.step(pad, &self.settings, dt);
        let mut out = PadAsked::default();
        if !self.on {
            // The press that switches it on does nothing else.
            self.on = step.woke;
            return out;
        }
        let panel = ui.window_open() || ui.typing() || ui.questions.is_up(state);
        if panel {
            self.let_go(&mut out.acts);
            self.half = None;
            self.panel(&step, dt, ui, input, &mut out.acts);
        } else {
            self.cursor.focus = None;
            self.world(&step, dt, ui, state, &mut out);
        }
        out
    }

    /// One frame of `pad` on the screens before the world, `dt` seconds after the last: the
    /// focus moved over what the last frame drew and the pointer with it, A as the left button,
    /// B as Escape and Start as Enter. Out of gamepad mode, a press switches it on and does
    /// nothing else.
    pub fn screens(&mut self, pad: Option<PadState>, dt: f64, input: &mut InputFrame) {
        let step = self.driver.step(pad, &self.settings, dt);
        self.half = None;
        if !self.on {
            self.on = step.woke;
            return;
        }
        let snap = input.nav.last.clone();
        self.cursor.follow(&snap);
        if self.cursor.focus.is_none() {
            let layer = snap.panels().last().copied().unwrap_or(0);
            self.cursor.focus = snap.home_in(layer).or_else(|| first_in(&snap, layer));
        }
        if let Some(dir) = step.dpad {
            if let Stepped::Scroll(d) = self.cursor.step(&snap, dir) {
                input.wheel += if d == Dir::Up { 1.0 } else { -1.0 };
            }
        }
        if let Some(f) = self.cursor.focus {
            input.mouse = f.centre();
        }
        if step.pressed(B::South) {
            input.down[0] = true;
            input.pressed[0] = true;
        }
        if step.released(B::South) && input.down[0] {
            input.down[0] = false;
            input.released[0] = true;
        }
        if step.pressed(B::East) {
            input.keys.push(vk::ESCAPE);
        }
        if step.pressed(B::Start) {
            input.keys.push(vk::ENTER);
        }
    }

    /// Gamepad mode off (a key was pressed): every control the pad holds let go.
    pub fn off(&mut self, acts: &mut Vec<Action>) {
        self.on = false;
        self.half = None;
        self.cursor.focus = None;
        self.let_go(acts);
    }

    /// Every control the pad holds let go (leaving the world, a window opening).
    pub fn let_go(&mut self, acts: &mut Vec<Action>) {
        for id in self.held.drain(..) {
            acts.push(Action::Control(Control::end(id).from(source(id))));
        }
    }

    /// The controls `wanted` held from now: each newly wanted pressed, each no longer let go.
    fn hold(&mut self, wanted: &[ControlId], acts: &mut Vec<Action>) {
        let (kept, gone): (Vec<_>, Vec<_>) = self.held.iter().partition(|c| wanted.contains(c));
        for id in gone {
            acts.push(Action::Control(Control::end(id).from(source(id))));
        }
        for id in wanted.iter().filter(|c| !kept.contains(c)) {
            acts.push(Action::Control(Control::begin(*id).from(source(*id))));
        }
        self.held = wanted.to_vec();
    }

    /// The pad in the world.
    fn world(
        &mut self,
        step: &PadStep,
        dt: f64,
        ui: &mut HorizonUi,
        state: &GameState,
        out: &mut PadAsked,
    ) {
        let acts = &mut out.acts;
        let mut wanted = Vec::new();
        if let Some((x, y)) = step.movement {
            let len = (x * x + y * y).sqrt().max(f32::EPSILON);
            let (x, y) = (x / len, y / len);
            if y > AXIS {
                wanted.push(movement::FORWARD);
            } else if y < -AXIS {
                wanted.push(movement::BACKWARD);
            }
            if x > AXIS {
                wanted.push(movement::STRAFE_RIGHT);
            } else if x < -AXIS {
                wanted.push(movement::STRAFE_LEFT);
            }
        }
        let (lx, ly) = step.look;
        if lx > LOOK {
            wanted.push(camera::ROTATE_RIGHT);
        } else if lx < -LOOK {
            wanted.push(camera::ROTATE_LEFT);
        }
        if ly > LOOK {
            wanted.push(camera::TILT_UP);
        } else if ly < -LOOK {
            wanted.push(camera::TILT_DOWN);
        }
        if step.held & B::North.bit() != 0 {
            wanted.push(movement::JUMP);
        }
        self.hold(&wanted, acts);
        // LB held: the right stick zooms a step at a time.
        self.zoom_wait = (self.zoom_wait - dt).max(0.0);
        if step.zoom.abs() >= ZOOM && self.zoom_wait <= 0.0 {
            out.requests.push(Request::Camera(CameraRequest::Zoom {
                inward: step.zoom > 0.0,
            }));
            self.zoom_wait = ZOOM_EVERY;
        } else if step.zoom.abs() < ZOOM {
            self.zoom_wait = 0.0;
        }
        self.half = step.set;
        if let (Some(half), Some(bar)) = (step.set, &state.bar) {
            for place in &step.fired {
                if let Some(slot) = cross_slot(bar.first(), half, *place) {
                    if bar.slot(slot).is_some_and(|s| !s.is_empty()) {
                        acts.push(Action::UseToolbarSlot { slot });
                    }
                }
            }
        }
        if let Some(i) = step.rb_pick.and_then(|i| u32::try_from(i).ok()) {
            acts.push(Action::ToolbarSet { set: i + 1 });
        }
        if step.pressed(B::South) {
            acts.push(if state.target.is_some() {
                Action::UseSelected
            } else {
                Action::SelectNearest {
                    kind: SelectKind::Creature,
                }
            });
        }
        if step.pressed(B::East) {
            acts.push(Action::Select { target: 0 });
            acts.push(Action::Control(
                Control::begin(movement::STOP).from(source(movement::STOP)),
            ));
            acts.push(Action::Control(
                Control::end(movement::STOP).from(source(movement::STOP)),
            ));
        }
        if step.pressed(B::West) {
            ui.toggle_map();
        }
        if step.pressed(B::Start) {
            ui.settings.toggle();
        }
        if step.lb_tap {
            let run = movement::RUN_LOCK;
            acts.push(Action::Control(Control::begin(run).from(source(run))));
            acts.push(Action::Control(Control::end(run).from(source(run))));
        }
        if step.pressed(B::LeftStick) {
            acts.push(Action::Attack {
                start: !state.combat.attacking,
            });
        }
        match step.dpad {
            Some(dir @ (Dir::Up | Dir::Down)) => {
                let rows = crate::ui::party::rows(state);
                if rows.is_empty() {
                    acts.push(Action::SelectSelf);
                } else {
                    let n = rows.len();
                    let at = match (self.fellow, dir) {
                        (None, Dir::Up) => n - 1,
                        (None, _) => 0,
                        (Some(i), Dir::Up) => (i + n - 1) % n,
                        (Some(i), _) => (i + 1) % n,
                    };
                    self.fellow = Some(at);
                    acts.push(Action::SetSelection {
                        target: rows[at].id,
                    });
                }
            }
            Some(dir) => acts.push(Action::SelectCycle {
                kind: SelectKind::Creature,
                previous: dir == Dir::Left,
            }),
            None => {}
        }
    }

    /// The pad on a window: the focus moved over its controls, the pointer with it, and A as the
    /// left button.
    fn panel(
        &mut self,
        step: &PadStep,
        dt: f64,
        ui: &mut HorizonUi,
        input: &mut InputFrame,
        acts: &mut Vec<Action>,
    ) {
        let snap = input.nav.last.clone();
        self.cursor.follow(&snap);
        if self.cursor.focus.is_none() {
            if let Some(&layer) = snap.panels().last() {
                self.cursor.enter(&snap, layer);
            }
        }
        if let Some(dir) = step.dpad {
            match self.cursor.step(&snap, dir) {
                Stepped::Scroll(d) => {
                    input.wheel += if d == Dir::Up { 1.0 } else { -1.0 };
                }
                Stepped::ScrollBy { area, by } => input.pad.scroll_list = Some((area, by)),
                Stepped::Moved | Stepped::Stayed => {}
            }
        }
        if step.pressed(B::Back) {
            let panels = snap.panels();
            let at = self
                .cursor
                .focus
                .and_then(|f| panels.iter().position(|l| *l == f.layer));
            let next = at.map_or(0, |i| (i + 1) % panels.len().max(1));
            if let Some(&layer) = panels.get(next) {
                self.cursor.enter(&snap, layer);
            }
        }
        if let Some(f) = self.cursor.focus {
            input.mouse = f.centre();
        }
        if step.pressed(B::South) {
            input.down[0] = true;
            input.pressed[0] = true;
        }
        if step.released(B::South) && input.down[0] {
            input.down[0] = false;
            input.released[0] = true;
        }
        if step.pressed(B::East) {
            if ui.chat.typing() {
                ui.chat.close();
            } else {
                ui.close_top(acts);
            }
        }
        let layer = self.cursor.focus.map_or(0, |f| f.layer);
        for (tap, by) in [(step.lb_tap, -1), (step.rb_tap, 1)] {
            if tap {
                if let Some(r) = snap.tabs_in(layer) {
                    input.pad.tab_step = Some((r, by));
                }
            }
        }
        if let Some((_, y)) = step.scroll {
            #[allow(clippy::cast_possible_truncation)]
            let notches = y * SCROLL_RATE * dt as f32;
            self.scroll += notches;
            let whole = self.scroll.trunc();
            if whole != 0.0 {
                input.wheel += whole;
                self.scroll -= whole;
            }
        } else {
            self.scroll = 0.0;
        }
    }
}

/// The shortcut bar's slot at `place` of cross hotbar `half`, the set shown starting at slot
/// `first`: LT's the set's first eight slots, RT's the first eight of the row over it. None past
/// the bar's last slot.
#[must_use]
pub fn cross_slot(first: u32, half: CrossSet, place: usize) -> Option<u32> {
    let place = u32::try_from(place).ok().filter(|p| *p < CROSS_PLACES)?;
    let row = match half {
        CrossSet::Left => 0,
        CrossSet::Right => crate::ui::hotbar::PER_ROW,
    };
    Some(first + row + place).filter(|s| *s < TOOLBAR_SLOTS)
}
