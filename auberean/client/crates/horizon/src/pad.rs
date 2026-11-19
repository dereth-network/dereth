//! The pad read frame by frame for gamepad mode ([`crate::gamepad`]): the sticks past their dead
//! zones, the buttons that went down and came up, the d-pad's repeat, a bumper's tap, and the
//! triggers holding up the cross hotbars ([`crate::ui::cross`]), whose places the d-pad and the
//! face buttons then use. Nothing here decides what a button does in the game.

use auberean_shell::pad::{PadButton, PadButton as B, PadState};

pub use dereth_ui_kit::nav::Dir;
pub use dereth_ui_kit::pad::CrossSet;

/// The pad's settings, kept with Horizon's own ([`crate::own`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PadSettings {
    /// How far a stick must be pushed, from 0 to 1, before it does anything.
    pub dead_zone: f32,
    /// How fast the right stick turns the camera, against its usual pace.
    pub camera_speed: f32,
}

impl Default for PadSettings {
    fn default() -> Self {
        Self {
            dead_zone: 0.2,
            camera_speed: 1.0,
        }
    }
}

/// The dead zone's range in the settings.
pub const DEAD_ZONE_RANGE: (f32, f32) = (0.05, 0.5);
/// The camera speed's range in the settings.
pub const CAMERA_SPEED_RANGE: (f32, f32) = (0.25, 3.0);

/// How far a trigger is pulled, from 0 to 1, before it counts as held.
pub const TRIGGER_HELD: f32 = 0.5;

/// How long the d-pad is held before it repeats, and how often it repeats then, in seconds.
pub const REPEAT_DELAY: f64 = 0.35;
pub const REPEAT_EVERY: f64 = 0.09;

/// A stick's push with the dead zone taken out: `None` inside it; outside, the same direction with
/// the push measured from the dead zone's edge, so it still runs from 0 to 1. Measured round the
/// middle, so a diagonal is not cut short.
#[must_use]
pub fn dead_zone(stick: (f32, f32), zone: f32) -> Option<(f32, f32)> {
    let (x, y) = stick;
    let pushed = (x * x + y * y).sqrt();
    if !pushed.is_finite() || pushed <= zone {
        return None;
    }
    let past = ((pushed - zone) / (1.0 - zone).max(f32::EPSILON)).min(1.0);
    Some((x / pushed * past, y / pushed * past))
}

/// The look stick's push as the camera turns by it: past the dead zone, eased so a small push
/// turns it finely (the push squared), times the camera speed.
#[must_use]
pub fn look(stick: (f32, f32), settings: &PadSettings) -> (f32, f32) {
    dead_zone(stick, settings.dead_zone).map_or((0.0, 0.0), |(x, y)| {
        let pushed = (x * x + y * y).sqrt();
        (
            x * pushed * settings.camera_speed,
            y * pushed * settings.camera_speed,
        )
    })
}

/// The look stick's up and down as the camera zooms by it, while LB is held: past the dead zone,
/// eased as [`look`] is, pushed up bringing it closer (positive) and down taking it further.
#[must_use]
pub fn zoom(stick: (f32, f32), settings: &PadSettings) -> f32 {
    dead_zone(stick, settings.dead_zone).map_or(0.0, |(x, y)| y * (x * x + y * y).sqrt())
}

/// The buttons of the cross hotbar's eight places, in order: the d-pad's (up, right, down, left),
/// then the face buttons' (top, right, bottom, left).
pub const CROSS_BUTTONS: [PadButton; 8] = [
    PadButton::DPadUp,
    PadButton::DPadRight,
    PadButton::DPadDown,
    PadButton::DPadLeft,
    PadButton::North,
    PadButton::East,
    PadButton::South,
    PadButton::West,
];

/// The buttons that pick the shortcut bar's sets with RB held, in the sets' order: Y, B, A, X the
/// first four, the d-pad's up, right, down, left the last four.
pub const SET_BUTTONS: [PadButton; 8] = [
    PadButton::North,
    PadButton::East,
    PadButton::South,
    PadButton::West,
    PadButton::DPadUp,
    PadButton::DPadRight,
    PadButton::DPadDown,
    PadButton::DPadLeft,
];

/// What one frame of the pad asks.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PadStep {
    /// The left stick past its dead zone: right and ahead.
    pub movement: Option<(f32, f32)>,
    /// The right stick, as the camera turns by it: nothing while LB is held, when it zooms.
    pub look: (f32, f32),
    /// The right stick's up and down while LB is held, as the camera zooms by it ([`zoom`]).
    pub zoom: f32,
    /// The right stick past its dead zone, as it is pushed: what scrolls a list.
    pub scroll: Option<(f32, f32)>,
    /// The cross hotbar half up, while a trigger is held.
    pub set: Option<CrossSet>,
    /// The cross hotbar's places pressed this frame.
    pub fired: Vec<usize>,
    /// The buttons that went down this frame ([`PadButton::bit`]); the face buttons and the
    /// d-pad only while no trigger holds the cross hotbar up.
    pub pressed: u16,
    /// The buttons that came up this frame, whatever the triggers.
    pub released: u16,
    /// The d-pad pressed this frame, or held long enough to repeat, with no trigger held.
    pub dpad: Option<Dir>,
    /// Anything pressed or pulled this frame: what switches gamepad mode on.
    pub woke: bool,
    /// The buttons held now.
    pub held: u16,
    /// LB or RB let go without another button pressed while it was held: a tap.
    pub lb_tap: bool,
    pub rb_tap: bool,
    /// With RB held, a face button or the d-pad pressed: the cross hotbar set it picks, from 0
    /// (as [`SET_BUTTONS`]).
    pub rb_pick: Option<usize>,
}

impl PadStep {
    /// Whether `b` went down this frame.
    #[must_use]
    pub const fn pressed(&self, b: PadButton) -> bool {
        self.pressed & b.bit() != 0
    }

    /// Whether `b` came up this frame.
    #[must_use]
    pub const fn released(&self, b: PadButton) -> bool {
        self.released & b.bit() != 0
    }
}

/// The pad across frames: its state last frame, which its presses are read against, and the
/// d-pad's repeat.
#[derive(Debug, Clone, Default)]
pub struct PadDriver {
    last: PadState,
    /// Another button was pressed while LB, or RB, was held: its let-go is not a tap.
    lb_used: bool,
    rb_used: bool,
    /// The d-pad direction held, and how long until it next repeats.
    repeat: Option<(Dir, f64)>,
}

/// How far a stick must be pushed, from 0 to 1, to switch gamepad mode on.
pub const STICK_WAKES: f32 = 0.6;

/// Whether either stick is pushed far enough to switch gamepad mode on.
fn pushed(p: &PadState) -> bool {
    let far = |(x, y): (f32, f32)| x * x + y * y >= STICK_WAKES * STICK_WAKES;
    far(p.left) || far(p.right)
}

/// The d-pad's buttons, by direction.
const DPAD: [(PadButton, Dir); 4] = [
    (PadButton::DPadUp, Dir::Up),
    (PadButton::DPadDown, Dir::Down),
    (PadButton::DPadLeft, Dir::Left),
    (PadButton::DPadRight, Dir::Right),
];

impl PadDriver {
    /// One frame of `pad` (`None` with no pad), `dt` seconds after the last, under `settings`.
    /// With none, everything held is let go.
    pub fn step(&mut self, pad: Option<PadState>, settings: &PadSettings, dt: f64) -> PadStep {
        let now = pad.unwrap_or_default();
        let before = std::mem::replace(&mut self.last, now);
        let set = if now.left_trigger >= TRIGGER_HELD {
            Some(CrossSet::Left)
        } else if now.right_trigger >= TRIGGER_HELD {
            Some(CrossSet::Right)
        } else {
            None
        };
        let went_down = now.buttons & !before.buttons;
        let pulled = |n: f32, b: f32| n >= TRIGGER_HELD && b < TRIGGER_HELD;
        // LB held turns the right stick from turning the camera to zooming it.
        let zooming = now.held(B::LeftBumper);
        let mut step = PadStep {
            movement: dead_zone(now.left, settings.dead_zone),
            look: if zooming {
                (0.0, 0.0)
            } else {
                look(now.right, settings)
            },
            zoom: if zooming {
                zoom(now.right, settings)
            } else {
                0.0
            },
            scroll: dead_zone(now.right, settings.dead_zone),
            set,
            pressed: went_down,
            released: before.buttons & !now.buttons,
            woke: went_down != 0
                || pulled(now.left_trigger, before.left_trigger)
                || pulled(now.right_trigger, before.right_trigger)
                || (pushed(&now) && !pushed(&before)),
            held: now.buttons,
            ..PadStep::default()
        };
        let others = went_down & !(B::LeftBumper.bit() | B::RightBumper.bit());
        if now.held(B::LeftBumper) && others != 0 {
            self.lb_used = true;
        }
        if now.held(B::RightBumper) && (others != 0 || set.is_some()) {
            self.rb_used = true;
        }
        if went_down & B::LeftBumper.bit() != 0 {
            self.lb_used = false;
        }
        if went_down & B::RightBumper.bit() != 0 {
            self.rb_used = false;
        }
        // The right stick pushed while LB is held zooms: its let-go is not a tap either.
        if zooming && step.scroll.is_some() {
            self.lb_used = true;
        }
        step.lb_tap = step.released(B::LeftBumper) && !std::mem::take(&mut self.lb_used);
        step.rb_tap = step.released(B::RightBumper) && !std::mem::take(&mut self.rb_used);
        // RB held: a face button or the d-pad picks the cross hotbar set, and does nothing else.
        if now.held(B::RightBumper) && set.is_none() {
            if let Some(i) = SET_BUTTONS.iter().position(|b| went_down & b.bit() != 0) {
                step.rb_pick = Some(i);
            }
            for b in CROSS_BUTTONS {
                step.pressed &= !b.bit();
            }
            self.repeat = None;
            return step;
        }
        if set.is_some() {
            step.fired = CROSS_BUTTONS
                .iter()
                .enumerate()
                .filter(|(_, b)| went_down & b.bit() != 0)
                .map(|(i, _)| i)
                .collect();
            for b in CROSS_BUTTONS {
                step.pressed &= !b.bit();
            }
            self.repeat = None;
            return step;
        }
        // The d-pad: a press at once, then a repeat while it stays held.
        if let Some((_, dir)) = DPAD.iter().find(|(b, _)| went_down & b.bit() != 0) {
            step.dpad = Some(*dir);
            self.repeat = Some((*dir, REPEAT_DELAY));
        } else if let Some((dir, left)) = self.repeat {
            let held = DPAD.iter().any(|(b, d)| *d == dir && now.held(*b));
            if held {
                let left = left - dt;
                if left <= 0.0 {
                    step.dpad = Some(dir);
                    self.repeat = Some((dir, REPEAT_EVERY));
                } else {
                    self.repeat = Some((dir, left));
                }
            } else {
                self.repeat = None;
            }
        }
        step
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (this client's own pad support)
    use super::*;
    use auberean_shell::pad::PadState;

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    fn pad(buttons: &[PadButton]) -> PadState {
        let mut p = PadState::default();
        for b in buttons {
            p.set(*b, true);
        }
        p
    }

    const FRAME: f64 = 1.0 / 60.0;

    #[test]
    fn inside_the_dead_zone_a_stick_is_still_and_outside_it_runs_from_nothing_to_all_the_way() {
        assert_eq!(dead_zone((0.1, 0.1), 0.2), None);
        assert_eq!(dead_zone((0.0, 0.2), 0.2), None);
        let (x, y) = dead_zone((0.0, 0.6), 0.2).unwrap();
        assert!(near(x, 0.0) && near(y, 0.5), "half way past the edge: {y}");
        let (x, y) = dead_zone((-1.0, 0.0), 0.2).unwrap();
        assert!(near(x, -1.0) && near(y, 0.0));
        // Round the middle: a diagonal keeps its direction and is not cut short.
        let (x, y) = dead_zone(
            (
                std::f32::consts::FRAC_1_SQRT_2,
                std::f32::consts::FRAC_1_SQRT_2,
            ),
            0.2,
        )
        .unwrap();
        assert!(near(x, y) && near((x * x + y * y).sqrt(), 1.0));
        // A pad reading a little past the rim is held to it.
        let (x, y) = dead_zone((1.0, 1.0), 0.2).unwrap();
        assert!(near((x * x + y * y).sqrt(), 1.0));
        assert_eq!(dead_zone((f32::NAN, 0.0), 0.2), None);
    }

    #[test]
    fn the_look_stick_turns_finely_near_the_middle_and_by_the_camera_speed() {
        let s = PadSettings::default();
        assert_eq!(look((0.1, 0.0), &s), (0.0, 0.0));
        let (x, _) = look((0.6, 0.0), &s);
        assert!(
            near(x, 0.25),
            "half way past the edge turns at a quarter: {x}"
        );
        let (x, y) = look((0.0, -1.0), &s);
        assert!(near(x, 0.0) && near(y, -1.0));
        let fast = PadSettings {
            camera_speed: 2.0,
            ..s
        };
        assert!(near(look((1.0, 0.0), &fast).0, 2.0));
    }

    #[test]
    fn the_sticks_move_and_turn_and_a_pad_gone_lets_everything_go() {
        let s = PadSettings::default();
        let mut d = PadDriver::default();
        let pushed = PadState {
            left: (0.0, 1.0),
            right: (1.0, 0.0),
            ..PadState::default()
        };
        let step = d.step(Some(pushed), &s, FRAME);
        assert_eq!(step.movement, Some((0.0, 1.0)));
        assert!(near(step.look.0, 1.0));
        assert!(step.woke, "a stick pushed well over switches the mode on");
        let nudged = PadState {
            left: (0.0, 0.3),
            ..PadState::default()
        };
        let mut d2 = PadDriver::default();
        assert!(!d2.step(Some(nudged), &s, FRAME).woke, "a nudge does not");
        let _ = d.step(Some(pad(&[PadButton::South])), &s, FRAME);
        let gone = d.step(None, &s, FRAME);
        assert_eq!(gone.movement, None);
        assert!(gone.released(PadButton::South), "what was held is let go");
    }

    #[test]
    fn a_trigger_held_turns_the_face_buttons_and_the_d_pad_to_the_cross_hotbar() {
        let s = PadSettings::default();
        let mut d = PadDriver::default();
        let lt = PadState {
            left_trigger: 1.0,
            ..PadState::default()
        };
        let step = d.step(Some(lt), &s, FRAME);
        assert_eq!(step.set, Some(CrossSet::Left));
        assert!(step.fired.is_empty() && step.woke);
        let both_buttons = PadState {
            buttons: pad(&[PadButton::South, PadButton::DPadLeft]).buttons,
            ..lt
        };
        let step = d.step(Some(both_buttons), &s, FRAME);
        assert_eq!(step.fired, vec![3, 6]);
        assert!(
            !step.pressed(PadButton::South) && step.dpad.is_none(),
            "A and the d-pad fire their slots and nothing else"
        );
        // Held, they fire once.
        assert!(d.step(Some(both_buttons), &s, FRAME).fired.is_empty());
        // RT: the second set; LT over it when both are held.
        let rt = PadState {
            right_trigger: 0.8,
            ..PadState::default()
        };
        assert_eq!(d.step(Some(rt), &s, FRAME).set, Some(CrossSet::Right));
        let both = PadState {
            left_trigger: 0.9,
            ..rt
        };
        assert_eq!(d.step(Some(both), &s, FRAME).set, Some(CrossSet::Left));
        let light = PadState {
            right_trigger: 0.3,
            ..PadState::default()
        };
        assert_eq!(d.step(Some(light), &s, FRAME).set, None, "barely pulled");
    }

    #[test]
    fn rb_held_with_a_button_picks_a_set_and_a_bumper_let_go_alone_is_a_tap() {
        let s = PadSettings::default();
        let mut d = PadDriver::default();
        let _ = d.step(Some(pad(&[PadButton::RightBumper])), &s, FRAME);
        let step = d.step(
            Some(pad(&[PadButton::RightBumper, PadButton::West])),
            &s,
            FRAME,
        );
        assert_eq!(step.rb_pick, Some(3), "X picks the fourth set");
        assert!(!step.pressed(PadButton::West), "and does nothing else");
        let _ = d.step(Some(pad(&[PadButton::RightBumper])), &s, FRAME);
        let up = d.step(
            Some(pad(&[PadButton::RightBumper, PadButton::DPadUp])),
            &s,
            FRAME,
        );
        assert_eq!(up.rb_pick, Some(4), "up picks the fifth");
        let _ = d.step(Some(pad(&[PadButton::RightBumper])), &s, FRAME);
        let y = d.step(
            Some(pad(&[PadButton::RightBumper, PadButton::North])),
            &s,
            FRAME,
        );
        assert_eq!(y.rb_pick, Some(0), "Y the first");
        let up = d.step(Some(pad(&[])), &s, FRAME);
        assert!(!up.rb_tap, "RB used with a button is not a tap");
        let _ = d.step(Some(pad(&[PadButton::RightBumper])), &s, FRAME);
        assert!(d.step(Some(pad(&[])), &s, FRAME).rb_tap);
        let _ = d.step(Some(pad(&[PadButton::LeftBumper])), &s, FRAME);
        let _ = d.step(
            Some(pad(&[PadButton::LeftBumper, PadButton::DPadLeft])),
            &s,
            FRAME,
        );
        assert!(
            !d.step(Some(pad(&[])), &s, FRAME).lb_tap,
            "LB held as a modifier"
        );
        let _ = d.step(Some(pad(&[PadButton::LeftBumper])), &s, FRAME);
        assert!(d.step(Some(pad(&[])), &s, FRAME).lb_tap);
    }

    #[test]
    fn lb_held_turns_the_right_stick_from_turning_the_camera_to_zooming_it_and_is_then_no_tap() {
        let s = PadSettings::default();
        let mut d = PadDriver::default();
        let up = PadState {
            right: (0.3, 1.0),
            ..PadState::default()
        };
        let turning = d.step(Some(up), &s, FRAME);
        assert!(turning.look.1 > 0.5 && turning.look.0 > 0.0);
        assert!(turning.zoom.abs() < 1e-6, "without LB it only turns");
        let _ = d.step(Some(pad(&[PadButton::LeftBumper])), &s, FRAME);
        let held = PadState {
            buttons: pad(&[PadButton::LeftBumper]).buttons,
            ..up
        };
        let zooming = d.step(Some(held), &s, FRAME);
        assert_eq!(zooming.look, (0.0, 0.0), "no turn or tilt with LB held");
        assert!(zooming.zoom > 0.5, "up zooms in: {}", zooming.zoom);
        let down = PadState {
            right: (0.0, -1.0),
            ..held
        };
        assert!(d.step(Some(down), &s, FRAME).zoom < -0.5, "down zooms out");
        assert!(
            !d.step(Some(pad(&[])), &s, FRAME).lb_tap,
            "LB let go after zooming does not toggle autorun"
        );
        let _ = d.step(Some(pad(&[PadButton::LeftBumper])), &s, FRAME);
        assert!(d.step(Some(pad(&[])), &s, FRAME).lb_tap, "a tap still is");
    }

    #[test]
    fn a_button_counts_once_when_it_goes_down_and_once_when_it_comes_up() {
        let s = PadSettings::default();
        let mut d = PadDriver::default();
        let step = d.step(Some(pad(&[PadButton::East, PadButton::Start])), &s, FRAME);
        assert!(step.pressed(PadButton::East) && step.pressed(PadButton::Start) && step.woke);
        let held = d.step(Some(pad(&[PadButton::East, PadButton::Start])), &s, FRAME);
        assert_eq!(held.pressed, 0);
        let up = d.step(Some(pad(&[PadButton::Start])), &s, FRAME);
        assert!(up.released(PadButton::East) && !up.released(PadButton::Start));
    }

    #[test]
    fn the_d_pad_steps_at_once_then_repeats_while_held() {
        let s = PadSettings::default();
        let mut d = PadDriver::default();
        let down = pad(&[PadButton::DPadDown]);
        assert_eq!(d.step(Some(down), &s, FRAME).dpad, Some(Dir::Down));
        let mut steps = 0usize;
        // Held a second: the first repeat after the delay, then one every repeat.
        for _ in 0..60 {
            if d.step(Some(down), &s, FRAME).dpad.is_some() {
                steps += 1;
            }
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let expected = ((1.0 - REPEAT_DELAY) / REPEAT_EVERY).floor() as usize + 1;
        assert!(
            steps.abs_diff(expected) <= 1,
            "{steps} repeats, about {expected}"
        );
        assert_eq!(d.step(Some(pad(&[])), &s, FRAME).dpad, None);
        assert_eq!(
            d.step(Some(pad(&[])), &s, 1.0).dpad,
            None,
            "let go, no repeat"
        );
    }
}
