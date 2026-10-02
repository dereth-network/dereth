//! Steering with the right mouse button while right-click mouse look is off. The 3D view is cut
//! into zones: holding the button in the top part walks forward (runs near the top edge), in the
//! bottom part backs up (runs near the bottom edge) or sidesteps (runs near the side edges), and
//! the left and right fifths turn. The middle of the view does nothing. While the button is held
//! the zone follows the pointer, even outside the view.
//!
//! Turning is at full speed; the classic interface turned faster the farther the pointer sat
//! towards the edge, which the shared movement actions cannot express.

use dereth_client_contract::actions::{movement, Action, ActionId};

use crate::widgets::Rect;

/// One zone of the 3D view, numbered as the classic interface numbered them.
pub type Zone = u8;

/// The pointer is not in the view and nothing is held.
pub const NO_ZONE: Zone = 0x16;

/// The thresholds that cut a view into zones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Bands {
    turn_left: i32,
    turn_right: i32,
    run_forward: i32,
    walk_forward: i32,
    forward: i32,
    turn_only: i32,
    run_back_top: i32,
    run_back: i32,
    step_left_run: i32,
    step_left_far: i32,
    step_right_run: i32,
    step_right_far: i32,
}

impl Bands {
    fn of(view: Rect) -> Self {
        let (left, top) = (view.x, view.y);
        let (right, bottom) = (view.x + view.w, view.y + view.h);
        let fifth = (right - left) / 5;
        let tenth = fifth / 2;
        let row = (bottom - top) / 16;
        Self {
            turn_left: left + 2 * fifth,
            turn_right: right - 2 * fifth,
            run_forward: top + 2 * row,
            walk_forward: top + 7 * row,
            forward: top + 9 * row,
            turn_only: top + 12 * row,
            run_back_top: top + 13 * row,
            run_back: bottom - row,
            step_left_run: left + tenth,
            step_left_far: left + 3 * tenth,
            step_right_run: right - tenth,
            step_right_far: right - 3 * tenth,
        }
    }
}

/// The zone under the pointer. Before the button is held a pointer outside the view is in no
/// zone; once it is held every point has one.
pub fn zone_at(view: Rect, x: i32, y: i32, held: bool) -> Zone {
    if !held && !view.contains(x, y) {
        return NO_ZONE;
    }
    let b = Bands::of(view);
    // The classic interface compared unsigned: a point left of or above the window's origin
    // counts as far right or far down.
    let (x, y) = (x as u32, y as u32);
    let at = |v: i32| v as u32;
    let column = if x < at(b.turn_left) {
        0
    } else if x <= at(b.turn_right) {
        1
    } else {
        2
    };
    if y <= at(b.forward) {
        let band = if y < at(b.run_forward) {
            0
        } else if y <= at(b.walk_forward) {
            1
        } else {
            2
        };
        return column * 3 + band;
    }
    if y <= at(b.turn_only) && !(column == 1 && y == at(b.turn_only)) {
        return 9 + column;
    }
    match column {
        0 => {
            if x < at(b.step_left_run) {
                0x0D
            } else if x > at(b.step_left_far) {
                0x0E
            } else {
                0x0C
            }
        }
        1 => {
            if y < at(b.run_back_top) {
                0x0F
            } else if y < at(b.run_back) {
                0x10
            } else {
                0x11
            }
        }
        _ => {
            if x > at(b.step_right_run) {
                0x14
            } else if x < at(b.step_right_far) {
                0x13
            } else {
                0x12
            }
        }
    }
}

/// The walking motion a zone holds, if any.
fn motion(zone: Zone) -> Option<ActionId> {
    match zone {
        0..=8 => Some(movement::MOVE_FORWARD),
        0x0C..=0x0E => Some(movement::STRAFE_LEFT),
        0x0F..=0x11 => Some(movement::MOVE_BACKWARD),
        0x12..=0x14 => Some(movement::STRAFE_RIGHT),
        _ => None,
    }
}

/// The turn a zone holds, if any.
fn turn(zone: Zone) -> Option<ActionId> {
    match zone {
        0 | 1 | 2 | 9 => Some(movement::TURN_LEFT),
        6 | 7 | 8 | 0x0B => Some(movement::TURN_RIGHT),
        _ => None,
    }
}

/// Whether a zone runs: the outermost band of each motion.
pub fn runs(zone: Zone) -> bool {
    matches!(zone, 0 | 3 | 6 | 0x0D | 0x11 | 0x14)
}

/// What the held button is doing.
#[derive(Debug, Default)]
pub struct Steering {
    zone: Option<Zone>,
    /// Whether the walk key is being held on the steering's behalf.
    walking: bool,
}

impl Steering {
    pub fn active(&self) -> bool {
        self.zone.is_some()
    }

    /// The button went down at a point; `None` when the point is outside the view.
    /// `run_by_default` is the player's run-as-default setting: a walking zone holds the walk
    /// key when it is set, a running zone when it is not.
    pub fn press(&mut self, view: Rect, x: i32, y: i32, run_by_default: bool) -> Vec<Action> {
        let zone = zone_at(view, x, y, false);
        if zone == NO_ZONE {
            return vec![];
        }
        self.enter(zone, run_by_default)
    }

    /// The pointer moved while the button is held.
    pub fn moved(&mut self, view: Rect, x: i32, y: i32, run_by_default: bool) -> Vec<Action> {
        if self.zone.is_none() {
            return vec![];
        }
        self.enter(zone_at(view, x, y, true), run_by_default)
    }

    /// The button came up, or the window lost focus: everything held ends.
    pub fn release(&mut self) -> Vec<Action> {
        let Some(old) = self.zone.take() else {
            return vec![];
        };
        let mut out: Vec<Action> = [motion(old), turn(old)]
            .into_iter()
            .flatten()
            .map(Action::end)
            .collect();
        if std::mem::take(&mut self.walking) {
            out.push(Action::end(movement::TOGGLE_RUN_WALK));
        }
        out
    }

    fn enter(&mut self, zone: Zone, run_by_default: bool) -> Vec<Action> {
        let old = self.zone.replace(zone);
        if old == Some(zone) {
            return vec![];
        }
        let mut out = vec![];
        let (old_motion, old_turn) = (old.and_then(motion), old.and_then(turn));
        let (new_motion, new_turn) = (motion(zone), turn(zone));
        if old_motion.is_some() && old_motion != new_motion {
            out.extend(old_motion.map(Action::end));
        }
        if old_turn.is_some() && old_turn != new_turn {
            out.extend(old_turn.map(Action::end));
        }
        // The walk key flips run and walk; it is held while the zone's pace differs from the
        // player's default pace.
        let walking = new_motion.is_some() && runs(zone) != run_by_default;
        if walking != self.walking {
            self.walking = walking;
            out.push(if walking {
                Action::begin(movement::TOGGLE_RUN_WALK)
            } else {
                Action::end(movement::TOGGLE_RUN_WALK)
            });
        }
        if new_motion.is_some() && new_motion != old_motion {
            out.extend(new_motion.map(Action::begin));
        }
        if new_turn.is_some() && new_turn != old_turn {
            out.extend(new_turn.map(Action::begin));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;

    const VIEW: Rect = Rect {
        x: 0,
        y: 0,
        w: 500,
        h: 480,
    };

    #[test]
    fn the_view_is_cut_into_turning_columns_and_moving_bands() {
        // Columns: 0..200 turns left, 200..=300 straight, above 300 turns right. Rows of 30:
        // forward to 270, turning only to 360, then back and sidesteps.
        assert_eq!(zone_at(VIEW, 250, 10, false), 3);
        assert_eq!(zone_at(VIEW, 250, 100, false), 4);
        assert_eq!(zone_at(VIEW, 250, 250, false), 5);
        assert_eq!(zone_at(VIEW, 50, 10, false), 0);
        assert_eq!(zone_at(VIEW, 450, 100, false), 7);
        assert_eq!(zone_at(VIEW, 50, 300, false), 9);
        assert_eq!(zone_at(VIEW, 250, 300, false), 10);
        assert_eq!(zone_at(VIEW, 450, 300, false), 11);
        assert_eq!(zone_at(VIEW, 250, 370, false), 0x0F);
        assert_eq!(zone_at(VIEW, 250, 400, false), 0x10);
        assert_eq!(zone_at(VIEW, 250, 470, false), 0x11);
        assert_eq!(zone_at(VIEW, 20, 400, false), 0x0D);
        assert_eq!(zone_at(VIEW, 100, 400, false), 0x0C);
        assert_eq!(zone_at(VIEW, 180, 400, false), 0x0E);
        assert_eq!(zone_at(VIEW, 480, 400, false), 0x14);
        assert_eq!(zone_at(VIEW, 400, 400, false), 0x12);
        assert_eq!(zone_at(VIEW, 320, 400, false), 0x13);
    }

    #[test]
    fn outside_the_view_there_is_no_zone_until_the_button_is_held() {
        assert_eq!(zone_at(VIEW, 600, 100, false), NO_ZONE);
        assert_eq!(zone_at(VIEW, 600, 100, true), 7);
        // Left of the view's origin compares as far right.
        assert_eq!(zone_at(VIEW, -5, 100, true), 7);
    }

    #[test]
    fn holding_near_the_top_runs_forward_and_lower_down_walks() {
        let mut s = Steering::default();
        assert_eq!(
            s.press(VIEW, 250, 10, true),
            vec![Action::begin(movement::MOVE_FORWARD)]
        );
        assert_eq!(
            s.moved(VIEW, 250, 100, true),
            vec![Action::begin(movement::TOGGLE_RUN_WALK)]
        );
        assert_eq!(
            s.release(),
            vec![
                Action::end(movement::MOVE_FORWARD),
                Action::end(movement::TOGGLE_RUN_WALK)
            ]
        );
        assert!(!s.active());
    }

    #[test]
    fn moving_to_the_side_adds_a_turn_and_the_middle_stops_walking() {
        let mut s = Steering::default();
        s.press(VIEW, 250, 10, true);
        assert_eq!(
            s.moved(VIEW, 50, 10, true),
            vec![Action::begin(movement::TURN_LEFT)]
        );
        assert_eq!(
            s.moved(VIEW, 50, 300, true),
            vec![Action::end(movement::MOVE_FORWARD)]
        );
        assert_eq!(
            s.moved(VIEW, 250, 300, true),
            vec![Action::end(movement::TURN_LEFT)]
        );
        assert_eq!(s.release(), vec![]);
    }

    #[test]
    fn walking_zones_hold_the_walk_key_only_when_running_is_the_default() {
        let mut s = Steering::default();
        assert_eq!(
            s.press(VIEW, 250, 400, false),
            vec![Action::begin(movement::MOVE_BACKWARD)]
        );
        assert_eq!(
            s.moved(VIEW, 250, 470, false),
            vec![Action::begin(movement::TOGGLE_RUN_WALK)]
        );
    }

    #[test]
    fn a_press_outside_the_view_starts_nothing() {
        let mut s = Steering::default();
        assert!(s.press(VIEW, 600, 100, true).is_empty());
        assert!(!s.active());
        assert!(s.moved(VIEW, 250, 10, true).is_empty());
    }
}
