//! Camera input: the camera action set, mouse-look, mouse turning and the mouse-input filter.
//!
//! It covers the camera manager's action handler, the mouse-look handler, the mouse-input filter,
//! the rotate step and the mouse-look toggle, and the four mouse-look preferences.
//!
//! The camera *geometry* — the swept sphere, FOV, znear/zfar — lives elsewhere. This module owns
//! the camera **input**.

use dereth_primitives::LocalTime;

use dereth_client_contract::actions::Action;

/// The camera action ids, which are the shared vocabulary's.
pub use dereth_client_contract::actions::camera as action;

/// What the camera manager asks of the current camera set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CameraCommand {
    Closer {
        extent: f32,
    },
    StopCloser,
    Farther {
        extent: f32,
    },
    StopFarther,
    /// Rotate left by `extent` with both mode flags set to 1.
    Rotate {
        left: bool,
        extent: f32,
    },
    StopRotating {
        left: bool,
    },
    Raise {
        extent: f32,
    },
    StopRaising,
    Lower {
        extent: f32,
    },
    StopLowering,
    SetDefaultOffsets,
    SetInHead,
    ToggleLookDown,
    ToggleMapMode,
    ToggleMouseLook(bool),
    /// Action `0x3E`: register (on press) or unregister (on release) input map `6`, then fall
    /// through to the `0x3D` behaviour.
    AlternateMode {
        on: bool,
    },
    /// Hold to look at the character's front: this client's own action, which the orbit camera
    /// answers and the game's camera does not.
    FrontView(bool),
    NotHandled,
}

/// The camera manager's action handler.
#[must_use]
pub fn on_action(event: &Action) -> CameraCommand {
    let e = event.extent;
    match (event.id, event.is_start()) {
        (action::ZOOM_IN, true) => CameraCommand::Closer { extent: e },
        (action::ZOOM_IN, false) => CameraCommand::StopCloser,
        (action::ZOOM_OUT, true) => CameraCommand::Farther { extent: e },
        (action::ZOOM_OUT, false) => CameraCommand::StopFarther,
        (action::ROTATE_LEFT, true) => CameraCommand::Rotate {
            left: true,
            extent: e,
        },
        (action::ROTATE_LEFT, false) => CameraCommand::StopRotating { left: true },
        (action::ROTATE_RIGHT, true) => CameraCommand::Rotate {
            left: false,
            extent: e,
        },
        (action::ROTATE_RIGHT, false) => CameraCommand::StopRotating { left: false },
        (action::ROTATE_UP, true) => CameraCommand::Raise { extent: e },
        (action::ROTATE_UP, false) => CameraCommand::StopRaising,
        (action::ROTATE_DOWN, true) => CameraCommand::Lower { extent: e },
        (action::ROTATE_DOWN, false) => CameraCommand::StopLowering,
        // The four one-shots (toggle type 3) act only on the start.
        (action::MOVE_TO_DEFAULT, true) => CameraCommand::SetDefaultOffsets,
        (action::FIRST_PERSON, true) => CameraCommand::SetInHead,
        (action::OVERHEAD_VIEW, true) => CameraCommand::ToggleLookDown,
        (action::MAP_VIEW, true) => CameraCommand::ToggleMapMode,
        (action::TOGGLE_MOUSELOOK, s) => CameraCommand::ToggleMouseLook(s),
        (action::TOGGLE_ALTERNATE_MODE, s) => CameraCommand::AlternateMode { on: s },
        (dereth_client_contract::actions::dereth::LOOK_AT_FRONT, s) => CameraCommand::FrontView(s),
        _ => CameraCommand::NotHandled,
    }
}

/// The four `Input.*` preferences the input manager registers, with the defaults its constructor
/// sets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MouseLookPreferences {
    /// `Input.MouseLookSensitivity`, default **0.25**.
    pub sensitivity: f32,
    /// `Input.MouseLookSmoothingAmount`, default **0.0** — the filter is then a pass-through.
    pub smoothing: f32,
    /// `Input.InvertMouseLookYAxis`, default false. **It negates both axes.**
    pub invert_y: bool,
    /// `Input.UseMouseTurning`, default false.
    pub use_mouse_turning: bool,
}

impl MouseLookPreferences {
    /// The registry names the input manager registers, spelled
    /// once. If nothing writes these four, `Input.UseMouseTurning` stays permanently false and
    /// the camera's mouse-turning arm is unreachable at run time.
    pub const SENSITIVITY: &'static str =
        dereth_client_contract::options::names::MOUSE_LOOK_SENSITIVITY;
    pub const SMOOTHING: &'static str =
        dereth_client_contract::options::names::MOUSE_LOOK_SMOOTHING_AMOUNT;
    pub const INVERT_Y: &'static str =
        dereth_client_contract::options::names::INVERT_MOUSE_LOOK_Y_AXIS;
    pub const USE_MOUSE_TURNING: &'static str =
        dereth_client_contract::options::names::USE_MOUSE_TURNING;
}

impl Default for MouseLookPreferences {
    fn default() -> Self {
        Self {
            sensitivity: 0.25,
            smoothing: 0.0,
            invert_y: false,
            use_mouse_turning: false,
        }
    }
}

/// The mouse-input filter's smoothing state.
///
/// The placement of the three smoothing values remains unverified: they are **file-static** and
/// shared by every `CameraState`, not per-camera. Reproduced here as module-level state (one
/// instance, held by the caller) rather than per-camera, exactly as the client behaves: if two
/// camera sets were switched inside 0.25 s they would smooth against each other's samples.
#[derive(Debug, Default, Clone, Copy)]
pub struct MouseInputFilter {
    prev_x: f32,
    prev_y: f32,
    last_time: LocalTime,
}

/// The window inside which the filter averages against the previous sample.
pub const FILTER_WINDOW_SECONDS: f64 = 0.25;

impl MouseInputFilter {
    /// ```text
    /// `now` is the current local time.
    /// if (now - last_time <= 0.25) { ax = (prev_x + x) * 0.5; ay = (prev_y + y) * 0.5; }
    /// else                         { ax = x; ay = y; }
    /// out_x = ax * smoothing + x * (1 - smoothing)
    /// out_y = ay * smoothing + y * (1 - smoothing)
    /// prev_x = out_x; prev_y = out_y; last_time = now
    /// ```
    pub fn filter(&mut self, x: f32, y: f32, smoothing: f32, now: LocalTime) -> (f32, f32) {
        let (ax, ay) = if now.0 - self.last_time.0 <= FILTER_WINDOW_SECONDS {
            ((self.prev_x + x) * 0.5, (self.prev_y + y) * 0.5)
        } else {
            (x, y)
        };
        let out_x = ax * smoothing + x * (1.0 - smoothing);
        let out_y = ay * smoothing + y * (1.0 - smoothing);
        self.prev_x = out_x;
        self.prev_y = out_y;
        self.last_time = now;
        (out_x, out_y)
    }
}

/// The `1/15` in `fx *= sens * 0.06666667`. With the default sensitivity 0.25 that is `1/60` of a
/// pixel per unit of camera rotation input.
pub const MOUSE_LOOK_SCALE: f32 = 0.066_666_67;

/// The per-axis warm-up: the camera only starts moving from the **sixth** consecutive frame with a
/// non-zero delta on that axis: the counter is incremented and the frame skipped while it is
/// at most 5.
pub const MOUSE_LOOK_WARMUP: i16 = 5;

/// What one mouse-look handler call decided.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MouseLookResult {
    /// `Rotate(fx > 0 ? in_head : !in_head, 0, |fx|, 0)` — `None` while the axis is warming up
    /// or the delta filtered to zero.
    pub rotate: Option<(bool, f32)>,
    /// `Lower(0, fy)` for a positive `fy`, `Raise(0, |fy|)` for a negative one.
    pub pitch: Option<(PitchDirection, f32)>,
    /// The 0.2 s idle tick, with mouse turning on: the body's turn stops (the command
    /// interpreter's stop-drift), the mouse having stopped.
    pub stop_drift: bool,
    /// The idle tick's throttled movement event is owed: half a second since the last.
    pub send_movement_event: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PitchDirection {
    Raise,
    Lower,
}

/// Mouse look, with its two per-axis warm-up counters (each a `short` on the camera set).
#[derive(Debug, Default)]
pub struct MouseLook {
    pub filter: MouseInputFilter,
    x_extent: i16,
    y_extent: i16,
    /// Whether mouse-look mode is active.
    pub active: bool,
    /// The time of the last server message.
    pub last_server_message: LocalTime,
}

impl MouseLook {
    /// Toggle mouse look -- entering resets both warm-up counters.
    pub fn toggle(&mut self, on: bool) -> bool {
        if self.active == on {
            return false;
        }
        self.active = on;
        if on {
            self.x_extent = 0;
            self.y_extent = 0;
        }
        true
    }

    /// One mouse-look handler call with `(dx, dy)`.
    ///
    /// Three things a rebuild must reproduce exactly: the `sensitivity / 15` scale,
    /// `Input.InvertMouseLookYAxis` negating **both** axes, and the five-sample per-axis warm-up.
    /// `in_head` records whether the camera is inside the player's head.
    pub fn handle(
        &mut self,
        dx: i32,
        dy: i32,
        prefs: &MouseLookPreferences,
        in_head: bool,
        now: LocalTime,
    ) -> MouseLookResult {
        if dx == 0 && dy == 0 {
            // The 0.2 s idle tick from the per-frame input poll.
            if !prefs.use_mouse_turning {
                return MouseLookResult {
                    rotate: None,
                    pitch: None,
                    stop_drift: false,
                    send_movement_event: false,
                };
            }
            let resend = self.last_server_message.0 + 0.5 < now.0;
            if resend {
                self.last_server_message = now;
            }
            return MouseLookResult {
                rotate: None,
                pitch: None,
                stop_drift: true,
                send_movement_event: resend,
            };
        }

        // A mouse delta is a small integer; f32 holds every value a device can report exactly.
        #[allow(clippy::cast_precision_loss)]
        let (mut fx, mut fy) = self
            .filter
            .filter(dx as f32, dy as f32, prefs.smoothing, now);
        fx *= prefs.sensitivity * MOUSE_LOOK_SCALE;
        fy *= prefs.sensitivity * MOUSE_LOOK_SCALE;
        if prefs.invert_y {
            // NOTE: **both** axes. The client negates x and y under one branch. If you "fix"
            // this to invert only Y, mouse-look will not match the original.
            fx = -fx;
            fy = -fy;
        }

        let rotate = if fx == 0.0 {
            self.x_extent = 0;
            None
        } else {
            self.x_extent += 1;
            if self.x_extent > MOUSE_LOOK_WARMUP {
                Some((if fx > 0.0 { in_head } else { !in_head }, fx.abs()))
            } else {
                None
            }
        };
        let pitch = if fy == 0.0 {
            self.y_extent = 0;
            None
        } else {
            self.y_extent += 1;
            if self.y_extent > MOUSE_LOOK_WARMUP {
                Some(if fy > 0.0 {
                    (PitchDirection::Lower, fy)
                } else {
                    (PitchDirection::Raise, fy.abs())
                })
            } else {
                None
            }
        };
        MouseLookResult {
            rotate,
            pitch,
            stop_drift: false,
            send_movement_event: false,
        }
    }
}

/// The rotate step refuses to run more often than every 0.0002 s.
pub const ROTATE_MIN_INTERVAL: f64 = 0.0002;

/// The default camera offset at which mouse turning applies to the **character** instead of the
/// camera.
pub const DEFAULT_CAMERA_OFFSET: (f32, f32, f32) = (0.0, 0.18, 0.0);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::ActionId;

    fn ev(action: ActionId, start: bool) -> Action {
        if start {
            Action::begin(action)
        } else {
            Action::end(action).with_extent(1.0)
        }
    }

    /// Oracle: the recovered movement-input behavior §4's action table.
    #[test]
    fn the_camera_actions_match_the_documented_table() {
        assert_eq!(
            on_action(&ev(action::ZOOM_IN, true)),
            CameraCommand::Closer { extent: 1.0 }
        );
        assert_eq!(
            on_action(&ev(action::ZOOM_IN, false)),
            CameraCommand::StopCloser
        );
        assert_eq!(
            on_action(&ev(action::ROTATE_LEFT, true)),
            CameraCommand::Rotate {
                left: true,
                extent: 1.0
            }
        );
        assert_eq!(
            on_action(&ev(action::ROTATE_RIGHT, false)),
            CameraCommand::StopRotating { left: false }
        );
        assert_eq!(
            on_action(&ev(action::FIRST_PERSON, true)),
            CameraCommand::SetInHead
        );
        // The one-shots do nothing on the release.
        assert_eq!(
            on_action(&ev(action::FIRST_PERSON, false)),
            CameraCommand::NotHandled
        );
        assert_eq!(
            on_action(&ev(action::TOGGLE_ALTERNATE_MODE, true)),
            CameraCommand::AlternateMode { on: true }
        );
    }

    /// Oracle: the recovered movement-input behavior §5 point 3 — the camera only starts moving from
    /// the sixth consecutive non-zero frame, and a zero frame resets that axis.
    #[test]
    fn the_five_sample_warmup_is_per_axis() {
        let mut ml = MouseLook::default();
        let prefs = MouseLookPreferences::default();
        for i in 1..=5 {
            let r = ml.handle(10, 0, &prefs, false, LocalTime(f64::from(i) * 0.01));
            assert!(r.rotate.is_none(), "frame {i} must still be warming up");
        }
        let r = ml.handle(10, 0, &prefs, false, LocalTime(0.06));
        assert!(r.rotate.is_some(), "the sixth frame moves the camera");
        // The Y axis never had a non-zero delta, so it never armed.
        assert!(r.pitch.is_none());
        // A zero X delta resets X.
        let r = ml.handle(0, 10, &prefs, false, LocalTime(0.07));
        assert!(r.rotate.is_none());
        let r = ml.handle(10, 0, &prefs, false, LocalTime(0.08));
        assert!(r.rotate.is_none(), "X restarted its warm-up");
    }

    /// Oracle: the recovered movement-input behavior §5 points 1 and 2 — the `sensitivity / 15` scale
    /// and the invert that negates **both** axes.
    #[test]
    fn the_scale_is_sensitivity_over_fifteen_and_invert_negates_both() {
        let prefs = MouseLookPreferences::default();
        let mut ml = MouseLook::default();
        // Warm both axes up, then read the sixth frame's magnitudes.
        for i in 1..=5 {
            ml.handle(60, 60, &prefs, false, LocalTime(f64::from(i) * 0.01));
        }
        let r = ml.handle(60, 60, &prefs, false, LocalTime(0.06));
        // 60 * 0.25 * (1/15) = 1.0
        let (_, mag) = r.rotate.expect("armed");
        assert!((mag - 1.0).abs() < 1e-5, "{mag}");
        assert_eq!(r.pitch.expect("armed").0, PitchDirection::Lower);

        let inv = MouseLookPreferences {
            invert_y: true,
            ..MouseLookPreferences::default()
        };
        let mut ml = MouseLook::default();
        for i in 1..=5 {
            ml.handle(60, 60, &inv, false, LocalTime(f64::from(i) * 0.01));
        }
        let r = ml.handle(60, 60, &inv, false, LocalTime(0.06));
        // Both axes flipped: pitch becomes Raise *and* the horizontal direction flips too.
        assert_eq!(r.pitch.expect("armed").0, PitchDirection::Raise);
        let (dir_inverted, _) = r.rotate.expect("armed");
        let mut plain = MouseLook::default();
        for i in 1..=5 {
            plain.handle(60, 60, &prefs, false, LocalTime(f64::from(i) * 0.01));
        }
        let (dir_plain, _) = plain
            .handle(60, 60, &prefs, false, LocalTime(0.06))
            .rotate
            .expect("armed");
        assert_ne!(dir_inverted, dir_plain, "invert flips the horizontal too");
    }

    /// Oracle: the recovered movement-input behavior §5's description of the mouse-input filter. With the
    /// default smoothing 0.0 the filter is a pass-through; with smoothing it averages against the
    /// previous sample only inside the 0.25 s window.
    #[test]
    fn the_filter_matches_the_pseudocode() {
        let mut f = MouseInputFilter::default();
        assert_eq!(f.filter(10.0, -4.0, 0.0, LocalTime(1.0)), (10.0, -4.0));

        let mut f = MouseInputFilter::default();
        // First call: last_time is 0.0 and now is 0.1, so 0.1 <= 0.25 and it averages against the
        // zero-initialised previous sample. ax = (0 + 10) * 0.5 = 5; out = 5*0.5 + 10*0.5 = 7.5.
        assert_eq!(f.filter(10.0, 0.0, 0.5, LocalTime(0.1)).0, 7.5);
        // Outside the window: no averaging, so the output is the input.
        assert_eq!(f.filter(10.0, 0.0, 0.5, LocalTime(1.0)).0, 10.0);
    }

    /// Oracle: the recovered movement-input behavior §5 — the idle tick keeps the turn command alive
    /// only when `Input.UseMouseTurning` is on.
    #[test]
    fn the_idle_tick_only_matters_with_mouse_turning() {
        let mut ml = MouseLook::default();
        let off = MouseLookPreferences::default();
        assert!(!ml.handle(0, 0, &off, false, LocalTime(1.0)).stop_drift);
        let on = MouseLookPreferences {
            use_mouse_turning: true,
            ..MouseLookPreferences::default()
        };
        assert!(ml.handle(0, 0, &on, false, LocalTime(1.0)).stop_drift);
    }
}
