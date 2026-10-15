//! A pad on the launcher's page: what each of its buttons does there, and which control a move
//! reaches.
//!
//! The page is a set of controls on screen, one of them focused. A pad moves the focus with the
//! d-pad or the left stick (held, it moves once, again after [`REPEAT_DELAY`], then every
//! [`REPEAT_EVERY`]), presses the focused control with the bottom face button, steps back with the
//! right one, steps through the page's sections with the shoulder buttons, and scrolls the list
//! under the focus with the right stick. The host reads the pad, as the client's own
//! [`PadState`], and hands each reading here with the time it was read; what comes back is for the
//! page to do. The page hands [`reach`] where its controls are, and focuses the one it names.
//!
//! Nothing here reads a pad or a clock: a host does both, and a reading taken while the launcher's
//! window is not the one in front is `None`, so a pad played in a game is not read here too.

pub use dereth_input::pad::{PadButton, PadState};
use serde::{Deserialize, Serialize};

/// Seconds a held direction waits before it moves the focus again.
pub const REPEAT_DELAY: f64 = 0.4;

/// Seconds between the moves of a direction held past [`REPEAT_DELAY`].
pub const REPEAT_EVERY: f64 = 0.12;

/// How far the left stick goes over before it moves the focus, as the d-pad does.
pub const STICK_MOVES: f32 = 0.5;

/// How far the right stick goes over before it scrolls.
pub const STICK_SCROLLS: f32 = 0.2;

/// Pixels a second the right stick scrolls, pushed all the way.
pub const SCROLL_SPEED: f32 = 1200.0;

/// The longest gap between two readings a scroll is measured over: a host that stalls does not
/// scroll a page's length at once when it resumes.
const LONGEST_STEP: f64 = 0.1;

/// A direction the focus moves in, on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Up,
    Down,
    Left,
    Right,
}

/// What the page does for a reading of the pad.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PadCommand {
    /// Move the focus to the control [`reach`] names.
    Move {
        step: Step,
    },
    /// Press the focused control (a button's click; Enter in a text box).
    Press,
    /// Step back: close what is open, or go back a screen.
    Back,
    /// The page's next section, and its previous one.
    NextSection,
    PreviousSection,
    /// Scroll the list under the focus by this many pixels, down when positive.
    Scroll {
        pixels: f32,
    },
}

/// The pad's moves, from one reading to the next.
#[derive(Debug, Clone, Default)]
pub struct PadNav {
    /// The last reading, and when it was taken; none while no pad is read.
    last: Option<(PadState, f64)>,
    /// The direction held, and when it moves the focus again.
    held: Option<(Step, f64)>,
}

impl PadNav {
    /// The page's moves for `pad`, read at `now` seconds (any clock that only goes forward). `None`
    /// is no pad, or none to read: the pad's state is forgotten, so a button held through it does
    /// nothing when the pad is read again until it is let go and pressed.
    pub fn read(&mut self, pad: Option<&PadState>, now: f64) -> Vec<PadCommand> {
        let Some(pad) = pad else {
            *self = Self::default();
            return Vec::new();
        };
        // The first reading is the baseline: what is already held presses nothing.
        let (before, then) = self.last.unwrap_or((*pad, now));
        self.last = Some((*pad, now));
        let mut out = Vec::new();
        for (button, command) in [
            (PadButton::South, PadCommand::Press),
            (PadButton::East, PadCommand::Back),
            (PadButton::RightBumper, PadCommand::NextSection),
            (PadButton::LeftBumper, PadCommand::PreviousSection),
        ] {
            if pad.pressed_since(&before, button) {
                out.push(command);
            }
        }
        let held = direction(pad);
        let was = direction(&before);
        self.held = match (held, self.held) {
            // Held from before: it moves again when its time comes.
            (Some(step), Some((h, next))) if step == h && was == Some(step) => {
                if now >= next {
                    out.push(PadCommand::Move { step });
                    Some((step, now + REPEAT_EVERY))
                } else {
                    Some((step, next))
                }
            }
            // Newly held, and not just held through the first reading: it moves at once.
            (Some(step), _) if was != Some(step) => {
                out.push(PadCommand::Move { step });
                Some((step, now + REPEAT_DELAY))
            }
            (Some(_), held) => held,
            (None, _) => None,
        };
        let tilt = pad.right.1;
        if tilt.abs() > STICK_SCROLLS {
            #[allow(clippy::cast_possible_truncation)]
            let seconds = (now - then).clamp(0.0, LONGEST_STEP) as f32;
            if seconds > 0.0 {
                // Up on the stick is up the page.
                out.push(PadCommand::Scroll {
                    pixels: -tilt * SCROLL_SPEED * seconds,
                });
            }
        }
        out
    }
}

/// The direction the pad holds: the d-pad's, else the left stick's when it goes over far enough,
/// along its longer axis.
fn direction(pad: &PadState) -> Option<Step> {
    for (button, step) in [
        (PadButton::DPadUp, Step::Up),
        (PadButton::DPadDown, Step::Down),
        (PadButton::DPadLeft, Step::Left),
        (PadButton::DPadRight, Step::Right),
    ] {
        if pad.held(button) {
            return Some(step);
        }
    }
    let (x, y) = pad.left;
    if x.abs().max(y.abs()) <= STICK_MOVES {
        None
    } else if y.abs() >= x.abs() {
        Some(if y > 0.0 { Step::Up } else { Step::Down })
    } else {
        Some(if x > 0.0 { Step::Right } else { Step::Left })
    }
}

/// Where a control is on screen: its left and top edges and its size, in pixels, y down.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    fn right(&self) -> f32 {
        self.x + self.width
    }

    fn bottom(&self) -> f32 {
        self.y + self.height
    }

    fn center(&self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
}

/// The control a move one `step` from the control `from` reaches, among `controls` in the page's
/// order: the nearest one wholly past `from` that way and no further off to the side than it lies
/// ahead, a control in line with it before one off to the side, then the one whose middle is
/// nearest `from`'s, then the first in the page's order. A control drawn over `from`, or under it
/// (a row's button, the row it is on), is that way when its middle is, and as near as can be.
/// `None` when no control lies that way, and the focus stays. With nothing focused (`from` none,
/// or not one of `controls`), the first control.
#[must_use]
pub fn reach(controls: &[Rect], from: Option<usize>, step: Step) -> Option<usize> {
    let Some(at) = from.and_then(|i| controls.get(i)) else {
        return (!controls.is_empty()).then_some(0);
    };
    let (cx, cy) = at.center();
    let mut best: Option<(f32, f32, usize)> = None;
    for (i, c) in controls.iter().enumerate() {
        if Some(i) == from || c.width <= 0.0 || c.height <= 0.0 {
            continue;
        }
        let (ox, oy) = c.center();
        // How far past `from` it lies along the move, edge to edge, and how far off to the side.
        let (ahead, past, aside) = if overlap(c, at) {
            let ahead = match step {
                Step::Down => oy - cy,
                Step::Up => cy - oy,
                Step::Right => ox - cx,
                Step::Left => cx - ox,
            };
            (ahead - 1.0, 0.0, 0.0)
        } else {
            match step {
                Step::Down => (
                    c.y - at.y,
                    c.y - at.bottom(),
                    gap(c.x, c.right(), at.x, at.right()),
                ),
                Step::Up => (
                    at.y - c.y,
                    at.y - c.bottom(),
                    gap(c.x, c.right(), at.x, at.right()),
                ),
                Step::Right => (
                    c.x - at.x,
                    c.x - at.right(),
                    gap(c.y, c.bottom(), at.y, at.bottom()),
                ),
                Step::Left => (
                    at.x - c.x,
                    at.x - c.right(),
                    gap(c.y, c.bottom(), at.y, at.bottom()),
                ),
            }
        };
        // Wholly past it, give or take a pixel of overlap, and that way rather than off to the side.
        if ahead <= 0.0 || past < -1.0 || aside > past.max(0.0) + 1.0 {
            continue;
        }
        let off = match step {
            Step::Down | Step::Up => (ox - cx).abs(),
            Step::Left | Step::Right => (oy - cy).abs(),
        };
        let score = past.max(0.0) + 2.0 * aside;
        if best.is_none_or(|(s, o, _)| score < s || (score == s && off < o)) {
            best = Some((score, off, i));
        }
    }
    best.map(|(_, _, i)| i)
}

/// Whether `a` and `b` are drawn over each other by more than a pixel each way: one on the other,
/// as a row's button is on its row.
fn overlap(a: &Rect, b: &Rect) -> bool {
    a.x < b.right() - 1.0
        && b.x < a.right() - 1.0
        && a.y < b.bottom() - 1.0
        && b.y < a.bottom() - 1.0
}

/// The gap between the spans `a0..a1` and `b0..b1`: none where they overlap.
fn gap(a0: f32, a1: f32, b0: f32, b1: f32) -> f32 {
    (b0 - a1).max(a0 - b1).max(0.0)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the launcher's own pad controls)
    use super::*;

    fn pad(buttons: &[PadButton]) -> PadState {
        let mut p = PadState::default();
        for b in buttons {
            p.set(*b, true);
        }
        p
    }

    fn stick(x: f32, y: f32) -> PadState {
        PadState {
            left: (x, y),
            ..PadState::default()
        }
    }

    fn scroll(y: f32) -> PadState {
        PadState {
            right: (0.0, y),
            ..PadState::default()
        }
    }

    const UP: PadCommand = PadCommand::Move { step: Step::Up };
    const DOWN: PadCommand = PadCommand::Move { step: Step::Down };
    const RIGHT: PadCommand = PadCommand::Move { step: Step::Right };

    #[test]
    fn each_button_does_its_thing_once_as_it_goes_down() {
        let mut n = PadNav::default();
        assert!(n.read(Some(&pad(&[])), 0.0).is_empty());
        for (b, c) in [
            (PadButton::South, PadCommand::Press),
            (PadButton::East, PadCommand::Back),
            (PadButton::RightBumper, PadCommand::NextSection),
            (PadButton::LeftBumper, PadCommand::PreviousSection),
        ] {
            assert_eq!(n.read(Some(&pad(&[b])), 1.0), [c], "{b:?}");
            assert!(n.read(Some(&pad(&[b])), 2.0).is_empty(), "{b:?} held");
            assert!(n.read(Some(&pad(&[])), 3.0).is_empty(), "{b:?} let go");
        }
        assert!(
            n.read(Some(&pad(&[PadButton::North, PadButton::Start])), 4.0)
                .is_empty(),
            "the other buttons do nothing"
        );
    }

    #[test]
    fn a_held_direction_moves_at_once_then_after_a_pause_then_steadily() {
        let mut n = PadNav::default();
        n.read(Some(&pad(&[])), 0.0);
        let down = pad(&[PadButton::DPadDown]);
        assert_eq!(n.read(Some(&down), 1.0), [DOWN]);
        assert!(n.read(Some(&down), 1.0 + REPEAT_DELAY - 0.01).is_empty());
        assert_eq!(n.read(Some(&down), 1.0 + REPEAT_DELAY), [DOWN]);
        let t = 1.0 + REPEAT_DELAY;
        assert!(n.read(Some(&down), t + REPEAT_EVERY - 0.01).is_empty());
        assert_eq!(n.read(Some(&down), t + REPEAT_EVERY), [DOWN]);
        // Another direction moves at once, and letting go stops it.
        assert_eq!(
            n.read(Some(&pad(&[PadButton::DPadRight])), t + 0.2),
            [RIGHT]
        );
        assert!(n.read(Some(&pad(&[])), t + 0.3).is_empty());
        assert_eq!(n.read(Some(&down), t + 0.31), [DOWN], "pressed again");
    }

    #[test]
    fn the_left_stick_moves_as_the_d_pad_once_pushed_over_far_enough() {
        let mut n = PadNav::default();
        n.read(Some(&stick(0.0, 0.0)), 0.0);
        assert!(n.read(Some(&stick(0.0, STICK_MOVES)), 0.1).is_empty());
        assert_eq!(n.read(Some(&stick(0.2, 0.9)), 0.2), [UP], "up the stick");
        assert!(n.read(Some(&stick(0.0, 0.0)), 0.3).is_empty());
        assert_eq!(
            n.read(Some(&stick(0.9, -0.3)), 0.4),
            [RIGHT],
            "its longer axis"
        );
        assert!(n.read(Some(&stick(0.0, 0.0)), 0.5).is_empty());
        assert_eq!(n.read(Some(&stick(0.0, -1.0)), 0.6), [DOWN]);
    }

    #[test]
    fn what_is_held_as_the_pad_is_first_read_or_read_again_does_nothing_until_pressed_anew() {
        let mut n = PadNav::default();
        let held = pad(&[PadButton::South, PadButton::DPadDown]);
        assert!(
            n.read(Some(&held), 0.0).is_empty(),
            "held as it is first read"
        );
        assert!(n.read(Some(&held), 1.0).is_empty(), "and still held");
        assert!(n.read(None, 2.0).is_empty(), "the window loses the front");
        assert!(n.read(Some(&held), 3.0).is_empty(), "held through it");
        n.read(Some(&pad(&[])), 4.0);
        assert_eq!(n.read(Some(&held), 5.0), [PadCommand::Press, DOWN]);
    }

    #[test]
    fn the_right_stick_scrolls_by_its_tilt_and_the_time_between_readings() {
        let mut n = PadNav::default();
        n.read(Some(&scroll(0.0)), 0.0);
        assert!(n.read(Some(&scroll(STICK_SCROLLS)), 0.05).is_empty());
        assert_eq!(
            n.read(Some(&scroll(-1.0)), 0.1),
            [PadCommand::Scroll {
                pixels: SCROLL_SPEED * 0.05
            }],
            "down the stick, down the page"
        );
        let up = n.read(Some(&scroll(0.5)), 0.12);
        assert!(
            matches!(up[..], [PadCommand::Scroll { pixels }] if (pixels + 0.5 * SCROLL_SPEED * 0.02).abs() < 1e-3),
            "{up:?}"
        );
        let stalled = n.read(Some(&scroll(1.0)), 10.0);
        assert!(
            matches!(stalled[..], [PadCommand::Scroll { pixels }] if (pixels + SCROLL_SPEED * 0.1).abs() < 1e-3),
            "a stall scrolls no further than a short step: {stalled:?}"
        );
    }

    fn r(x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    /// The page as the launcher draws it: the rail's four buttons down the left, a search box
    /// over a list of three rows, and two buttons side by side under the list.
    fn page() -> Vec<Rect> {
        vec![
            r(10.0, 100.0, 120.0, 30.0),  // 0 HOME
            r(10.0, 140.0, 120.0, 30.0),  // 1 WORLDS
            r(10.0, 180.0, 120.0, 30.0),  // 2 LIBRARY
            r(10.0, 220.0, 120.0, 30.0),  // 3 ACCOUNTS
            r(200.0, 60.0, 400.0, 30.0),  // 4 the search box
            r(200.0, 110.0, 500.0, 40.0), // 5 a row
            r(200.0, 160.0, 500.0, 40.0), // 6 a row
            r(200.0, 210.0, 500.0, 40.0), // 7 a row
            r(200.0, 270.0, 100.0, 30.0), // 8 a button
            r(320.0, 270.0, 100.0, 30.0), // 9 the button beside it
        ]
    }

    #[test]
    fn a_move_reaches_the_nearest_control_that_way_in_line_before_one_off_to_the_side() {
        let p = page();
        assert_eq!(reach(&p, Some(1), Step::Down), Some(2), "down the rail");
        assert_eq!(reach(&p, Some(1), Step::Up), Some(0));
        assert_eq!(reach(&p, Some(5), Step::Down), Some(6), "down the list");
        assert_eq!(reach(&p, Some(6), Step::Up), Some(5));
        assert_eq!(
            reach(&p, Some(5), Step::Up),
            Some(4),
            "from the list to the search box"
        );
        assert_eq!(
            reach(&p, Some(7), Step::Down),
            Some(9),
            "from the list to the button nearest under its middle"
        );
        assert_eq!(
            reach(&p, Some(8), Step::Right),
            Some(9),
            "along the buttons"
        );
        assert_eq!(reach(&p, Some(9), Step::Left), Some(8));
        assert_eq!(
            reach(&p, Some(6), Step::Left),
            Some(2),
            "to the rail button most level with it"
        );
        assert_eq!(
            reach(&p, Some(1), Step::Right),
            Some(5),
            "and back to the list"
        );
    }

    #[test]
    fn a_button_on_a_row_is_reached_from_the_row_and_the_row_from_it() {
        let rows = vec![
            r(200.0, 110.0, 500.0, 40.0), // 0 a row
            r(660.0, 115.0, 30.0, 30.0),  // 1 its button, at its right end
            r(200.0, 160.0, 500.0, 40.0), // 2 the next row
            r(660.0, 165.0, 30.0, 30.0),  // 3 its button
        ];
        assert_eq!(
            reach(&rows, Some(0), Step::Right),
            Some(1),
            "onto the row's button"
        );
        assert_eq!(
            reach(&rows, Some(1), Step::Left),
            Some(0),
            "back to its row"
        );
        assert_eq!(
            reach(&rows, Some(0), Step::Down),
            Some(2),
            "down to the next row, not the button"
        );
        assert_eq!(
            reach(&rows, Some(1), Step::Down),
            Some(2),
            "from the button down to the next row, the nearer"
        );
        assert_eq!(
            reach(&rows, Some(1), Step::Right),
            None,
            "nothing past the button"
        );
    }

    #[test]
    fn a_move_with_nothing_that_way_leaves_the_focus_and_one_with_nothing_focused_takes_the_first()
    {
        let p = page();
        assert_eq!(
            reach(&p, Some(0), Step::Up),
            None,
            "the top of the rail: the search box is off to the side"
        );
        assert_eq!(reach(&p, Some(9), Step::Right), None, "the last button");
        assert_eq!(reach(&p, Some(0), Step::Left), None, "the left edge");
        assert_eq!(reach(&p, None, Step::Down), Some(0));
        assert_eq!(
            reach(&p, Some(99), Step::Up),
            Some(0),
            "focus on no control"
        );
        assert_eq!(reach(&[], None, Step::Down), None, "no controls");
        let mut hidden = page();
        hidden[6].height = 0.0;
        assert_eq!(
            reach(&hidden, Some(5), Step::Down),
            Some(7),
            "a control not drawn is passed over"
        );
    }

    #[test]
    fn the_commands_reach_the_page_as_it_reads_them() {
        let wire = |c: PadCommand| serde_json::to_string(&c).expect("encodes");
        assert_eq!(wire(DOWN), r#"{"kind":"move","step":"down"}"#);
        assert_eq!(wire(PadCommand::Press), r#"{"kind":"press"}"#);
        assert_eq!(wire(PadCommand::NextSection), r#"{"kind":"next_section"}"#);
        assert_eq!(
            wire(PadCommand::Scroll { pixels: 12.5 }),
            r#"{"kind":"scroll","pixels":12.5}"#
        );
        let rects: Vec<Rect> =
            serde_json::from_str(r#"[{"x":1,"y":2,"width":3,"height":4}]"#).expect("decodes");
        assert_eq!(rects, [r(1.0, 2.0, 3.0, 4.0)]);
        let step: Step = serde_json::from_str(r#""left""#).expect("decodes");
        assert_eq!(step, Step::Left);
    }
}
