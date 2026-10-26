//! The pad on the plugins' layer under the modern and the classic interface, neither of which takes
//! the pad itself: the d-pad moves a focus among the layer's controls (a plugin window's, a HUD's,
//! the plugin bar's), A presses the control in focus (and takes hold of a slider, for the d-pad's
//! left and right), B lets go of a slider, steps back out of a list, closes the window the focus
//! is in or refuses the host's dialog for a file, the shoulder buttons turn the tabs of the window
//! the focus is in, the right stick scrolls under it, and View moves the focus from window to
//! window and to the bar.
//!
//! The focus is the player's: it starts nowhere, and the first press of the d-pad or A brings it
//! to the topmost window. A window a plugin opens of its own accord never takes it from where it
//! is; one the player opened (from the plugin bar, or by one of the plugin's commands) does, and
//! so does the host's dialog.

use dereth_horizon::pad::{PadDriver, PadSettings, PadStep};
use dereth_input::pad::{PadButton as B, PadState};
use dereth_ui_kit::draw::{DrawList, Rect};
use dereth_ui_kit::input::{vk, InputFrame};
use dereth_ui_kit::nav::{first_in, Cursor, Dir, Kind, PanelKind, Snapshot, Stepped};

/// How far a press of the d-pad moves a slider held, of its whole length.
const SLIDER_STEP: f32 = 0.05;

/// The colour the focus is outlined in.
const FOCUS: u32 = 0xFFFF_D860;

/// The pad on the layer, from frame to frame.
#[derive(Debug, Default)]
pub(crate) struct LayerPad {
    driver: PadDriver,
    pub cursor: Cursor,
    /// The panels the layer drew last frame, by name: one not among them has just opened.
    names: Vec<String>,
    /// A press made for one frame (a slider moved), let go the next.
    release_next: bool,
    /// The slider A has taken hold of.
    adjusting: Option<Rect>,
    /// The pad has been used since the layer came up: the host's dialog takes the focus only
    /// then.
    engaged: bool,
}

impl LayerPad {
    /// One frame of `pad` (`None` with none) for the layer, `dt` seconds after the last: what it
    /// asks of `input`, the layer's input this frame, read against `snap`, what the layer drew
    /// last frame. `asking`: the host's question is up. `free` says whether nothing of the
    /// interface's stands over a place on screen, where a press of A may land.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        pad: Option<PadState>,
        dt: f64,
        time_ms: u32,
        snap: &Snapshot,
        input: &mut InputFrame,
        asking: bool,
        free: &dyn Fn(f32, f32) -> bool,
    ) {
        let step = self.driver.step(pad, &PadSettings::default(), dt);
        if std::mem::take(&mut self.release_next) {
            input.down[0] = false;
            input.released[0] = true;
        }
        self.engaged |= step.woke;
        self.follow(snap, asking);
        if !self.engaged {
            return;
        }
        let focus = self.cursor.focus;
        let adjusting = focus.is_some_and(|t| Some(t.rect) == self.adjusting);
        if !adjusting {
            self.adjusting = None;
        }
        // The first press brings the focus in: to the question, the topmost window, else the bar.
        if focus.is_none() && (step.dpad.is_some() || step.pressed(B::South)) {
            let layer = snap
                .panels()
                .into_iter()
                .rev()
                .find(|l| snap.layer(*l).is_some_and(|p| p.kind != PanelKind::Chat))
                .unwrap_or(0);
            self.cursor.enter(snap, layer);
            if self.cursor.focus.is_none() {
                self.cursor.focus = first_in(snap, 0);
            }
            return;
        }
        let Some(t) = focus else {
            return;
        };
        let mut pointer = None;
        if let Some(dir) = step.dpad {
            if let (true, Kind::Slider { x0, x1, t: at }) = (adjusting, t.kind) {
                if matches!(dir, Dir::Left | Dir::Right) {
                    let by = if dir == Dir::Left {
                        -SLIDER_STEP
                    } else {
                        SLIDER_STEP
                    };
                    let to = (at + by).clamp(0.0, 1.0);
                    pointer = Some((x0 + (x1 - x0) * to, t.centre().1));
                }
            } else {
                let mut stepped = self.cursor.step(snap, dir);
                if stepped == Stepped::Stayed && matches!(dir, Dir::Up | Dir::Down) {
                    stepped = self.cursor.wrap(snap, dir);
                }
                match stepped {
                    Stepped::Scroll(dir) => {
                        input.wheel += if dir == Dir::Up { 1.0 } else { -1.0 };
                    }
                    Stepped::ScrollBy { area, by } => input.pad.scroll_list = Some((area, by)),
                    Stepped::Moved | Stepped::Stayed => {}
                }
            }
        }
        let focus = self.cursor.focus;
        let layer = focus.map_or(0, |f| f.layer);
        let panel = snap.layer(layer);
        input.pad.focus = focus.map(|f| f.rect);
        input.pad.raise = panel
            .filter(|l| l.kind == PanelKind::Window)
            .map(|l| l.rect);
        let at = pointer.or_else(|| focus.map(|f| f.centre()));
        if step.dpad.is_some() || step.pressed(B::South) || step.scroll.is_some() {
            if let Some(at) = at {
                input.mouse = at;
            }
        }
        if let Some(at) = pointer {
            if free(at.0, at.1) {
                press(input, time_ms);
                self.release_next = true;
            }
        }
        self.buttons(&step, snap, input, time_ms, asking, free);
    }

    /// Keep the focus on its target as the layer changes under it; a window that has just opened
    /// takes it only when the player opened it (one a plugin opened of its own accord is quiet,
    /// and the focus never goes into it), and the host's question only once the pad is in use.
    fn follow(&mut self, snap: &Snapshot, asking: bool) {
        let was = self.cursor.focus;
        self.cursor.follow(snap);
        let names: Vec<String> = snap.layers.iter().map(|l| l.name.clone()).collect();
        let opened_into = self.cursor.focus.and_then(|f| {
            let l = snap.layer(f.layer)?;
            (!self.names.contains(&l.name)).then_some(l.kind)
        });
        self.names = names;
        if let Some(kind) = opened_into {
            let question = asking && kind == PanelKind::Box;
            let window = kind == PanelKind::Window;
            if !(question && self.engaged || window) {
                // Not the player's: the focus stays where it was.
                self.cursor.focus =
                    was.and_then(|w| snap.targets.iter().find(|t| t.rect == w.rect).copied());
            }
        }
    }

    /// A, B, the shoulders, View and the right stick.
    fn buttons(
        &mut self,
        step: &PadStep,
        snap: &Snapshot,
        input: &mut InputFrame,
        time_ms: u32,
        asking: bool,
        free: &dyn Fn(f32, f32) -> bool,
    ) {
        let focus = self.cursor.focus;
        let layer = focus.map_or(0, |f| f.layer);
        let panel = snap.layer(layer);
        if step.pressed(B::South) {
            if let Some(t) = focus {
                if matches!(t.kind, Kind::Slider { .. }) {
                    // A takes hold of a slider, for the d-pad's left and right, and lets it go.
                    self.adjusting = if self.adjusting == Some(t.rect) {
                        None
                    } else {
                        Some(t.rect)
                    };
                } else if t.kind == Kind::List {
                    self.cursor.enter_list(snap);
                } else if free(t.centre().0, t.centre().1) {
                    press(input, time_ms);
                }
            }
        }
        if step.released(B::South) && input.down[0] && !self.release_next {
            input.down[0] = false;
            input.released[0] = true;
        }
        if step.pressed(B::East) {
            if self.adjusting.take().is_some() {
                // The slider let go.
            } else if asking {
                // The host's question answered no.
                input.keys.push(vk::ESCAPE);
            } else if self.cursor.back(snap) {
                // Out of a list, or back to where the focus came from.
            } else if let Some(cancel) = snap.cancel_of(layer) {
                let (x, y) = (cancel.x + cancel.w / 2.0, cancel.y + cancel.h / 2.0);
                input.mouse = (x, y);
                press(input, time_ms);
                self.release_next = true;
            } else if let Some(window) = panel.filter(|l| l.kind == PanelKind::Window) {
                input.pad.close = Some(window.rect);
            } else {
                self.cursor.resign();
            }
        }
        for (tap, by) in [(step.pressed(B::LeftBumper), -1), (step.rb_tap, 1)] {
            if tap {
                if let Some(bar) = snap.tabs_in(layer) {
                    input.pad.tab_step = Some((bar, by));
                }
            }
        }
        // View: the next window round, and after the last the plugin bar and the HUDs.
        if step.pressed(B::Back) {
            let panels: Vec<usize> = snap
                .panels()
                .into_iter()
                .filter(|l| snap.layer(*l).is_some_and(|p| p.kind == PanelKind::Window))
                .collect();
            let next = panels
                .iter()
                .position(|l| *l == layer)
                .map_or(panels.first().copied(), |i| panels.get(i + 1).copied());
            match next {
                Some(l) => self.cursor.enter(snap, l),
                None => self.cursor.focus = first_in(snap, 0),
            }
        }
        if let (Some((_, y)), Some(f)) = (step.scroll, focus) {
            input.mouse = f.centre();
            input.wheel += y.signum();
        }
    }

    /// The focus's outline, over everything the layer draws, while the pad is in use.
    pub fn draw(&self, list: &mut DrawList) {
        let Some(f) = self.cursor.focus.filter(|_| self.engaged) else {
            return;
        };
        let r = f.rect;
        for side in [
            Rect::new(r.x - 2.0, r.y - 2.0, r.w + 4.0, 2.0),
            Rect::new(r.x - 2.0, r.bottom(), r.w + 4.0, 2.0),
            Rect::new(r.x - 2.0, r.y, 2.0, r.h),
            Rect::new(r.right(), r.y, 2.0, r.h),
        ] {
            list.fill(side, FOCUS);
        }
    }

    /// The layer is put away: nothing is held, and the focus goes.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// A press of the left button where the pointer is, let go by the caller.
fn press(input: &mut InputFrame, time_ms: u32) {
    input.down[0] = true;
    input.pressed[0] = true;
    input.note_left_press(time_ms);
}
