//! The pointer during a camera drag: hidden, held still, and shown again where the drag began.
//!
//! While a held button or key makes the pointer turn the camera -- the game camera's mouse look,
//! the orbit camera's drags -- the pointer is of no use to the player and only wanders off over
//! the window. The host is asked to hide it and hold it still ([`HostPointer::capture`]), and
//! reports the mouse's own movement instead ([`HostEvent::PointerMotion`]), which turns the camera
//! as the pointer's movement did. When the drag ends the host shows the pointer again where the
//! drag began ([`HostPointer::release`]).
//!
//! The pointer is held once it has moved past the client's drag threshold, so a click of a button
//! that would begin a drag (the right-click that examines, the click on the world that selects)
//! never hides it. A drag that turns no camera -- a window, an item, a slider, the chat window --
//! never holds it, and neither does any drag while gamepad mode is on.
//!
//! While the pointer is held the interface sees it where it was held: only the camera follows the
//! mouse. When the drag ends the interface is told the pointer is back where the drag began.
//!
//! A drag is never a click. The right button's click examines what is under the pointer when it
//! comes up, and the interface sees a drag's release where the drag began once a held pointer is
//! shown again there, or wherever a drag brought back by hand ends; and a click there at once
//! after the drag would be taken with the drag's press for a double-click, which uses. So a press
//! of the left or right button is followed through every event, held or not: once the pointer has
//! moved past the drag threshold from where the button went down, or a camera drag has held it,
//! the press is a drag, and the interface is told so before the release reaches it
//! ([`PointerHold::take_release`]).
//!
//! A host that can place the pointer holds it as the final client held it under mouse look
//! ([`PutBack`]): hidden, put back in one place whenever it has moved, and its movement read off
//! the window's own reports of it, so the camera turns at the speed and with the acceleration the
//! player set for the pointer. A host that cannot place it locks it and reads the mouse itself.

use dereth_input::host::HostEvent;
use dereth_input::keys::MouseButton;

/// The host's hold on the pointer.
pub trait HostPointer {
    /// Hide the pointer and hold it still where it is, at `at` in client pixels, for a camera
    /// drag. Until [`Self::release`] the host reports the mouse's movement as
    /// [`HostEvent::PointerMotion`] and not as the pointer moving. `false` when the host cannot
    /// hold it: the drag goes on with the pointer shown.
    fn capture(&mut self, at: (f64, f64)) -> bool;
    /// Show the pointer again at `at`, in client pixels, and report its movement as before. A
    /// host that has let go of it meanwhile (the window lost the focus) leaves it where it is.
    fn release(&mut self, at: (f64, f64));
}

/// A host that never holds the pointer: every drag keeps it shown.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoPointerHold;

impl HostPointer for NoPointerHold {
    fn capture(&mut self, _at: (f64, f64)) -> bool {
        false
    }

    fn release(&mut self, _at: (f64, f64)) {}
}

/// A pointer held by putting it back: hidden, put back at one place (its anchor) whenever it has
/// moved, and its movement read off the window's own reports of it.
///
/// A window system moves the pointer when asked either at once, every later report being made
/// from where it was put, or in its own time, reporting it there once it has. Until that report
/// the reports already on their way were made from where the pointer was, and are measured from
/// there; the report of the pointer put back is no movement either way.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PutBack {
    /// Where the pointer is put back, in client pixels.
    anchor: (f64, f64),
    /// Where the pointer was last reported or put: its next report is measured from here.
    from: (f64, f64),
    /// The window system moves the pointer at once when asked.
    at_once: bool,
    /// The pointer has been put back, and the window system's report of it there is to come.
    report_due: bool,
}

/// Whether two places in client pixels are the same pixel.
fn same_place(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 0.5 && (a.1 - b.1).abs() < 0.5
}

impl PutBack {
    /// The pointer held where it is, at `at`, to be put back at `anchor` (both in client
    /// pixels), on a window system that moves it at once when asked when `at_once`. Answers the
    /// hold, and where to put the pointer now if it is not at the anchor already.
    #[must_use]
    pub fn new(at: (f64, f64), anchor: (f64, f64), at_once: bool) -> (Self, Option<(f64, f64)>) {
        let mut hold = Self {
            anchor,
            from: at,
            at_once,
            report_due: false,
        };
        let now = hold.put_back();
        (hold, now)
    }

    /// The window reported the pointer at `at`, in client pixels: the mouse's movement since the
    /// last report, or `None` for no movement and for the report of the pointer put back.
    pub fn moved(&mut self, at: (f64, f64)) -> Option<(f64, f64)> {
        if self.report_due && same_place(at, self.anchor) {
            self.report_due = false;
            self.from = self.anchor;
            return None;
        }
        let moved = (at.0 - self.from.0, at.1 - self.from.1);
        self.from = at;
        (moved != (0.0, 0.0)).then_some(moved)
    }

    /// Where to put the pointer now, once the window's reports so far are read: the anchor, when
    /// the pointer has moved from it and is not already on its way back.
    pub fn put_back(&mut self) -> Option<(f64, f64)> {
        if self.report_due || same_place(self.from, self.anchor) {
            return None;
        }
        if self.at_once {
            self.from = self.anchor;
        } else {
            self.report_due = true;
        }
        Some(self.anchor)
    }
}

/// What one of the window's events is to the interface while a drag may hold the pointer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gate {
    /// Routed as it came.
    Route,
    /// The mouse moved while the pointer is held: the camera's pointer is now at this point, in
    /// client pixels, and only the camera is told.
    Camera(f64, f64),
    /// Not routed: the held pointer's own movement, or the mouse's while nothing is held.
    Drop,
}

/// A press of the left or right button under way.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Press {
    /// Where the pointer was when the button went down, as the camera has it; `None` while the
    /// window has not yet reported it, when the first report is taken for it.
    at: Option<(f64, f64)>,
    /// The press is a drag: the pointer has moved past the drag threshold from where it went
    /// down, or was held for a camera drag, while the button was down.
    dragged: bool,
}

/// A camera drag under way.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Drag {
    /// Where the pointer was when the drag began, and where it is shown again.
    start: (f64, f64),
    /// The host holds the pointer.
    held: bool,
    /// The pointer is not held during this drag: gamepad mode is on, the host could not hold it,
    /// or it was let go before the drag ended.
    refused: bool,
    /// Where the camera has been told the pointer is: where it was held, moved by the mouse since.
    camera: (f64, f64),
}

/// The pointer during camera drags, over the host's hold.
pub struct PointerHold {
    host: Box<dyn HostPointer>,
    /// Where the window last reported the pointer, in client pixels.
    pointer: Option<(f64, f64)>,
    drag: Option<Drag>,
    /// Where a drag that held the pointer showed it again, until the interface is told.
    returned: Option<(f64, f64)>,
    /// The left and right buttons' presses under way, in that order.
    presses: [Option<Press>; 2],
    /// A button came up with the last event: which, and whether its press was a drag, until the
    /// interface is told.
    released: Option<(MouseButton, bool)>,
}

impl std::fmt::Debug for PointerHold {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PointerHold")
            .field("pointer", &self.pointer)
            .field("drag", &self.drag)
            .field("returned", &self.returned)
            .field("presses", &self.presses)
            .finish_non_exhaustive()
    }
}

impl Default for PointerHold {
    fn default() -> Self {
        Self::new(Box::new(NoPointerHold))
    }
}

/// The client's drag threshold, as a squared distance in client pixels: a press and a release
/// further apart than this are a drag.
fn past_drag_threshold(from: (f64, f64), to: (f64, f64)) -> bool {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    dx * dx + dy * dy > f64::from(dereth_client_contract::actions::ui::DRAG_THRESHOLD_SQUARED)
}

impl PointerHold {
    /// The pointer over the host's hold, with no drag under way.
    #[must_use]
    pub fn new(host: Box<dyn HostPointer>) -> Self {
        Self {
            host,
            pointer: None,
            drag: None,
            returned: None,
            presses: [None; 2],
            released: None,
        }
    }

    /// Whether the host holds the pointer now.
    #[must_use]
    pub fn held(&self) -> bool {
        self.drag.is_some_and(|d| d.held)
    }

    /// One of the window's events, before it is routed: what the interface is to make of it.
    ///
    /// A press of the left or right button is followed meanwhile, for whether its release ends a
    /// click or a drag ([`Self::take_release`]).
    pub fn gate(&mut self, event: &HostEvent) -> Gate {
        let was_held = self.held();
        let held = self.drag.as_mut().filter(|d| d.held);
        let gate = match (event, held) {
            (HostEvent::CursorMoved { x, y }, None) => {
                self.pointer = Some((*x, *y));
                Gate::Route
            }
            (HostEvent::PointerMotion { dx, dy }, Some(drag)) => {
                drag.camera = (drag.camera.0 + dx, drag.camera.1 + dy);
                Gate::Camera(drag.camera.0, drag.camera.1)
            }
            (HostEvent::CursorMoved { .. } | HostEvent::PointerMotion { .. }, _) => Gate::Drop,
            _ => Gate::Route,
        };
        self.follow_presses(event, gate, was_held);
        gate
    }

    /// The left or right button came up with the last event gated: which, and whether its press
    /// was a drag, once. The interface is to be told before the release reaches it, so that a
    /// drag does nothing a click does, wherever the pointer is let go -- a drag that held the
    /// pointer shows it again where the drag began, and a drag can be brought back there by hand:
    /// its release examines nothing, and its press is no first click of a double-click.
    pub fn take_release(&mut self) -> Option<(MouseButton, bool)> {
        self.released.take()
    }

    /// Where the camera has the pointer now: where the mouse has moved it from where it was held,
    /// or where the window last reported it.
    fn camera_pointer(&self) -> Option<(f64, f64)> {
        match self.drag {
            Some(drag) if drag.held => Some(drag.camera),
            _ => self.pointer,
        }
    }

    /// Follow the left and right buttons' presses through one event, gated as `gate`, the
    /// pointer held when it came when `held`: a press is a drag once the pointer has moved past
    /// the drag threshold from where it went down, or once a camera drag has held the pointer
    /// while the button is down.
    fn follow_presses(&mut self, event: &HostEvent, gate: Gate, held: bool) {
        let button = match event {
            HostEvent::MouseInput { button, pressed } => match button {
                MouseButton::Left => Some((0, *button, *pressed)),
                MouseButton::Right => Some((1, *button, *pressed)),
                _ => None,
            },
            _ => None,
        };
        if let Some((i, _, true)) = button {
            self.presses[i] = Some(Press {
                at: self.camera_pointer(),
                dragged: false,
            });
        }
        let now = match (event, gate) {
            (HostEvent::CursorMoved { x, y }, Gate::Route) => Some((*x, *y)),
            (_, Gate::Camera(x, y)) => Some((x, y)),
            _ => None,
        };
        for press in self.presses.iter_mut().flatten() {
            if let Some(now) = now {
                let at = *press.at.get_or_insert(now);
                press.dragged |= past_drag_threshold(at, now);
            }
            press.dragged |= held;
        }
        if let Some((i, button, false)) = button {
            self.released = Some((button, self.presses[i].take().is_some_and(|p| p.dragged)));
        }
    }

    /// Follow the camera drag, after each of the window's events and at the start of a frame:
    /// `looking` is whether the pointer turns the camera now, and `allowed` whether a drag may
    /// hold the pointer (gamepad mode is off). A drag begins where the pointer is when the camera
    /// starts to turn with it, holds the pointer once it has moved past the drag threshold, and
    /// ends when the camera stops turning with it.
    pub fn follow(&mut self, looking: bool, allowed: bool) {
        if !looking {
            self.end();
            return;
        }
        let Some(drag) = self.drag.as_mut() else {
            // Without a place to show it again, the pointer is not held.
            let start = self.pointer.unwrap_or_default();
            self.drag = Some(Drag {
                start,
                held: false,
                refused: !allowed || self.pointer.is_none(),
                camera: start,
            });
            return;
        };
        if drag.held || drag.refused {
            return;
        }
        if !allowed {
            drag.refused = true;
            return;
        }
        let Some(at) = self
            .pointer
            .filter(|at| past_drag_threshold(drag.start, *at))
        else {
            return;
        };
        drag.held = self.host.capture(at);
        drag.refused = !drag.held;
        drag.camera = at;
    }

    /// Let the pointer go now, whatever the drag: the client is closing, or the interface is
    /// changing. The pointer is not held again until the camera stops turning with it.
    pub fn let_go(&mut self) {
        let start = self.drag.map(|d| d.start);
        self.end();
        if let Some(start) = start {
            self.drag = Some(Drag {
                start,
                held: false,
                refused: true,
                camera: start,
            });
        }
    }

    /// Where a drag that held the pointer showed it again, once: the interface is to be told the
    /// pointer is there.
    pub fn take_returned(&mut self) -> Option<(f64, f64)> {
        self.returned.take()
    }

    /// The drag under way ends: a held pointer is shown again where the drag began.
    fn end(&mut self) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        if drag.held {
            self.host.release(drag.start);
            self.pointer = Some(drag.start);
            self.returned = Some(drag.start);
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the requests a camera drag makes of the host's pointer hold).
    use super::*;

    use std::cell::RefCell;
    use std::rc::Rc;

    /// One request the host was given.
    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Asked {
        Capture((f64, f64)),
        Release((f64, f64)),
    }

    /// A host that records what it is asked, and holds the pointer when `holds`.
    struct Recorder {
        asked: Rc<RefCell<Vec<Asked>>>,
        holds: bool,
    }

    impl HostPointer for Recorder {
        fn capture(&mut self, at: (f64, f64)) -> bool {
            self.asked.borrow_mut().push(Asked::Capture(at));
            self.holds
        }

        fn release(&mut self, at: (f64, f64)) {
            self.asked.borrow_mut().push(Asked::Release(at));
        }
    }

    fn hold(holds: bool) -> (PointerHold, Rc<RefCell<Vec<Asked>>>) {
        let asked = Rc::new(RefCell::new(Vec::new()));
        let recorder = Recorder {
            asked: Rc::clone(&asked),
            holds,
        };
        (PointerHold::new(Box::new(recorder)), asked)
    }

    fn moved(h: &mut PointerHold, x: f64, y: f64) -> Gate {
        h.gate(&HostEvent::CursorMoved { x, y })
    }

    /// `button` going down or up, and what its release was said to end.
    fn button(
        h: &mut PointerHold,
        button: MouseButton,
        pressed: bool,
    ) -> Option<(MouseButton, bool)> {
        assert_eq!(h.gate(&HostEvent::MouseInput { button, pressed }), Gate::Route);
        h.take_release()
    }

    /// The right button going down or up, and whether its release was said to end a drag.
    fn right(h: &mut PointerHold, pressed: bool) -> Option<bool> {
        button(h, MouseButton::Right, pressed).map(|(b, dragged)| {
            assert_eq!(b, MouseButton::Right);
            dragged
        })
    }

    /// Behaviour: ui.pointer.turning-the-camera-with-the-right-button-is-not-an-appraisal
    ///
    /// The right button's release ends a click while the pointer has stayed within the drag
    /// threshold of where the button went down, and a drag once it has gone past it, however far
    /// it comes back; each release is told once.
    #[test]
    fn a_right_press_is_a_drag_once_the_pointer_passes_the_threshold_wherever_it_is_let_go() {
        let (mut h, _) = hold(false);
        moved(&mut h, 100.0, 50.0);
        assert_eq!(right(&mut h, true), None, "told on the release");
        moved(&mut h, 103.0, 50.0);
        assert_eq!(right(&mut h, false), Some(false), "three pixels is a click");
        assert_eq!(h.take_release(), None, "told once");
        moved(&mut h, 100.0, 50.0);
        right(&mut h, true);
        moved(&mut h, 104.0, 50.0);
        moved(&mut h, 100.0, 50.0);
        assert_eq!(right(&mut h, false), Some(true), "four is a drag, brought back");
        right(&mut h, true);
        assert_eq!(right(&mut h, false), Some(false), "the next press starts afresh");
    }

    /// Behaviour: ui.pointer.turning-the-camera-with-the-right-button-is-not-an-appraisal
    ///
    /// While a camera drag holds the pointer, a press of the right button is a drag however still
    /// the mouse, as is the press of the left that began it, and a press whose own drag holds the
    /// pointer stays one however far the mouse brings the camera back.
    #[test]
    fn a_right_press_while_a_camera_drag_holds_the_pointer_is_a_drag() {
        let (mut h, _) = hold(true);
        moved(&mut h, 10.0, 10.0);
        button(&mut h, MouseButton::Left, true);
        h.follow(true, true);
        moved(&mut h, 30.0, 10.0);
        h.follow(true, true);
        assert!(h.held(), "the left button's drag holds the pointer");
        right(&mut h, true);
        assert_eq!(right(&mut h, false), Some(true), "the mouse still");
        assert_eq!(
            button(&mut h, MouseButton::Left, false),
            Some((MouseButton::Left, true))
        );
        h.follow(false, true);

        moved(&mut h, 200.0, 200.0);
        right(&mut h, true);
        h.follow(true, true);
        moved(&mut h, 204.0, 200.0);
        h.follow(true, true);
        assert!(h.held());
        assert_eq!(
            h.gate(&HostEvent::PointerMotion { dx: -4.0, dy: 0.0 }),
            Gate::Camera(200.0, 200.0)
        );
        assert_eq!(right(&mut h, false), Some(true));
    }

    /// Behaviour: camera.mouse-look.the-pointer-is-held-once-the-drag-moves-it
    ///
    /// The pointer is held once the drag has moved it past the drag threshold, at the place it
    /// had reached, and shown again where the drag began when the camera stops turning with it.
    #[test]
    fn a_drag_holds_the_pointer_past_the_threshold_and_shows_it_where_it_began() {
        let (mut h, asked) = hold(true);
        assert_eq!(moved(&mut h, 100.0, 50.0), Gate::Route);
        h.follow(true, true);
        assert_eq!(
            moved(&mut h, 103.0, 50.0),
            Gate::Route,
            "within the threshold"
        );
        h.follow(true, true);
        assert!(asked.borrow().is_empty(), "three pixels is still a click");
        assert_eq!(moved(&mut h, 104.0, 50.0), Gate::Route);
        h.follow(true, true);
        assert_eq!(*asked.borrow(), [Asked::Capture((104.0, 50.0))]);
        assert!(h.held());
        h.follow(false, true);
        assert_eq!(
            *asked.borrow(),
            [Asked::Capture((104.0, 50.0)), Asked::Release((100.0, 50.0))]
        );
        assert!(!h.held());
        assert_eq!(h.take_returned(), Some((100.0, 50.0)));
        assert_eq!(h.take_returned(), None, "told once");
    }

    /// A press and release without a drag asks nothing of the host.
    #[test]
    fn a_click_holds_nothing() {
        let (mut h, asked) = hold(true);
        moved(&mut h, 10.0, 10.0);
        h.follow(true, true);
        moved(&mut h, 11.0, 12.0);
        h.follow(true, true);
        h.follow(false, true);
        assert!(asked.borrow().is_empty());
        assert_eq!(h.take_returned(), None);
    }

    /// While the pointer is held, the window's reports of the pointer are not routed and the
    /// mouse's own movement moves the camera's pointer on from where it was held; with nothing
    /// held the mouse's movement is not routed.
    #[test]
    fn while_held_the_mouse_motion_moves_the_cameras_pointer_and_the_pointer_reports_stop() {
        let (mut h, _) = hold(true);
        let motion = HostEvent::PointerMotion { dx: 5.0, dy: -2.0 };
        assert_eq!(h.gate(&motion), Gate::Drop, "nothing held");
        moved(&mut h, 200.0, 200.0);
        h.follow(true, true);
        moved(&mut h, 210.0, 200.0);
        h.follow(true, true);
        assert_eq!(h.gate(&motion), Gate::Camera(215.0, 198.0));
        assert_eq!(h.gate(&motion), Gate::Camera(220.0, 196.0));
        assert_eq!(moved(&mut h, 600.0, 600.0), Gate::Drop);
        let wheel = HostEvent::MouseWheel { notches: 1.0 };
        assert_eq!(h.gate(&wheel), Gate::Route);
        h.follow(false, true);
        assert_eq!(moved(&mut h, 201.0, 200.0), Gate::Route);
    }

    /// With gamepad mode on a drag never holds the pointer, however far it goes, and gamepad mode
    /// coming on before the drag reaches the threshold keeps it from holding it.
    #[test]
    fn a_drag_in_gamepad_mode_holds_nothing() {
        let (mut h, asked) = hold(true);
        moved(&mut h, 10.0, 10.0);
        h.follow(true, false);
        moved(&mut h, 300.0, 10.0);
        h.follow(true, false);
        h.follow(true, true);
        h.follow(false, true);
        moved(&mut h, 10.0, 10.0);
        h.follow(true, true);
        h.follow(true, false);
        moved(&mut h, 300.0, 10.0);
        h.follow(true, true);
        h.follow(false, true);
        assert!(asked.borrow().is_empty());
    }

    /// A host that cannot hold the pointer is asked once a drag, and the drag goes on with the
    /// pointer's own movement routed.
    #[test]
    fn a_host_that_cannot_hold_the_pointer_leaves_the_drag_as_it_was() {
        let (mut h, asked) = hold(false);
        moved(&mut h, 10.0, 10.0);
        h.follow(true, true);
        moved(&mut h, 30.0, 10.0);
        h.follow(true, true);
        assert_eq!(moved(&mut h, 50.0, 10.0), Gate::Route);
        h.follow(true, true);
        h.follow(false, true);
        assert_eq!(*asked.borrow(), [Asked::Capture((30.0, 10.0))]);
        assert_eq!(h.take_returned(), None);
    }

    /// Where the window system moves the pointer at once, each report is measured from where the
    /// pointer was put back, and its report there is no movement.
    #[test]
    fn a_pointer_put_back_at_once_moves_by_its_reports_and_its_put_back_is_no_movement() {
        let anchor = (400.0, 300.0);
        let (mut hold, now) = PutBack::new((120.0, 80.0), anchor, true);
        assert_eq!(now, Some(anchor), "put in the middle at once");
        assert_eq!(hold.moved(anchor), None, "the report of it there");
        assert_eq!(hold.moved((410.0, 297.0)), Some((10.0, -3.0)));
        assert_eq!(hold.moved((412.0, 297.0)), Some((2.0, 0.0)));
        assert_eq!(hold.put_back(), Some(anchor));
        assert_eq!(hold.put_back(), None, "already there");
        assert_eq!(hold.moved(anchor), None, "the report of the put-back");
        assert_eq!(
            hold.moved((395.0, 300.0)),
            Some((-5.0, 0.0)),
            "a report after the put-back, with no report of it between, is from the anchor"
        );
        let (mut still, now) = PutBack::new(anchor, anchor, true);
        assert_eq!(now, None, "held where it is put back");
        assert_eq!(still.moved((401.0, 300.0)), Some((1.0, 0.0)));
    }

    /// Where the window system moves the pointer in its own time, the reports on their way when it
    /// was put back are measured from where it was, until the report of it put back, which is no
    /// movement; it is not put back again while that report is to come.
    #[test]
    fn a_pointer_put_back_in_its_own_time_is_measured_from_its_reports_until_the_put_back_is_seen()
    {
        let anchor = (400.0, 300.0);
        let (mut hold, now) = PutBack::new((120.0, 80.0), anchor, false);
        assert_eq!(now, Some(anchor));
        assert_eq!(
            hold.moved((124.0, 80.0)),
            Some((4.0, 0.0)),
            "made before it moved"
        );
        assert_eq!(hold.put_back(), None, "still on its way");
        assert_eq!(hold.moved(anchor), None, "the report of it put back");
        assert_eq!(hold.moved((403.0, 302.0)), Some((3.0, 2.0)));
        assert_eq!(hold.put_back(), Some(anchor));
        assert_eq!(
            hold.moved((406.0, 302.0)),
            Some((3.0, 0.0)),
            "made before it moved"
        );
        assert_eq!(hold.moved(anchor), None);
        assert_eq!(hold.moved((399.0, 300.0)), Some((-1.0, 0.0)));
    }

    /// Letting go shows the pointer where the drag began, and the pointer is not held again until
    /// the camera has stopped turning with it.
    #[test]
    fn letting_go_shows_the_pointer_and_holds_it_no_more_in_that_drag() {
        let (mut h, asked) = hold(true);
        moved(&mut h, 10.0, 10.0);
        h.follow(true, true);
        moved(&mut h, 30.0, 10.0);
        h.follow(true, true);
        h.let_go();
        assert!(!h.held());
        assert_eq!(h.take_returned(), Some((10.0, 10.0)));
        moved(&mut h, 60.0, 10.0);
        h.follow(true, true);
        h.follow(false, true);
        assert_eq!(
            *asked.borrow(),
            [Asked::Capture((30.0, 10.0)), Asked::Release((10.0, 10.0))]
        );
    }
}
