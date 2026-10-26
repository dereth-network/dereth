//! A frame hook's held movement controls, what a plugin's jumps and held motions press, carried
//! out on a real body: the jump it holds charges as the jump key does and leaps when let go, the
//! charge it puts down ends without a leap, a sidestep it holds moves the body sideways until let
//! go, and none of it is the player's own movement. Fixture: the jump key's (a real `App` on real
//! terrain with a recorded player description), with a hook that hands the frame a queued list
//! of requests each step.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use crate::common::sim_app::{body, frames, player_description};
use dereth_client_runtime::frame_hook::{FrameHook, HookFrame, NavKey, NavRequest};
use dereth_primitives::num::math;

use super::jump_charge_release::setup;

/// Hands each frame the next queued list of requests, and notes whether the frame saw the
/// player's own movement.
#[derive(Debug, Default)]
struct Holder {
    queue: Rc<RefCell<VecDeque<Vec<NavRequest>>>>,
    seen: Rc<RefCell<Vec<bool>>>,
}

impl FrameHook for Holder {
    fn step(&mut self, frame: &mut HookFrame<'_>) {
        self.seen.borrow_mut().push(frame.player_input);
        if let Some(asked) = self.queue.borrow_mut().pop_front() {
            for r in asked {
                frame.nav(r);
            }
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

type Shared = (
    Rc<RefCell<VecDeque<Vec<NavRequest>>>>,
    Rc<RefCell<Vec<bool>>>,
);

fn holder(app: &mut dereth_client::app::App) -> Shared {
    let h = Holder::default();
    let shared = (Rc::clone(&h.queue), Rc::clone(&h.seen));
    app.install_frame_hook(Box::new(h));
    shared
}

/// Behaviour: plugins.nav.a-jump-faces-its-heading-charges-for-the-time-asked-and-leaps-the-way-held
#[test]
fn a_hooks_held_jump_charges_standing_and_leaps_when_let_go() {
    let mut app = setup();
    player_description(&mut app, "early-inventory-and-casting");
    let (queue, seen) = holder(&mut app);
    frames(&mut app, 2);
    seen.borrow_mut().clear();
    queue
        .borrow_mut()
        .push_back(vec![NavRequest::Hold(NavKey::Jump, true)]);
    frames(&mut app, 3);
    assert!(
        app.objects().world.combat.jump_pending,
        "the held jump charges"
    );
    assert!(body(&app).driver().movement.interp.standing_longjump);
    assert!(body(&app).on_ground(), "a charge stands on the ground");
    assert_eq!(
        app.probe().jump_counts(),
        (0, 0),
        "no leap while it charges"
    );
    frames(&mut app, 12);
    queue
        .borrow_mut()
        .push_back(vec![NavRequest::Hold(NavKey::Jump, false)]);
    frames(&mut app, 2);
    assert!(!app.objects().world.combat.jump_pending);
    assert_eq!(app.probe().jump_counts().0, 1, "let go: the leap");
    let extent = app
        .probe()
        .last_jump_request()
        .expect("the leap was asked of the server")
        .0
        .extent;
    assert!(extent > 0.1, "charged for a while: {extent}");
    assert!(
        seen.borrow().iter().all(|s| !*s),
        "the hook's own controls are not the player's movement: {:?}",
        seen.borrow()
    );
    app.shutdown();
}

/// Behaviour: plugins.nav.a-jump-faces-its-heading-charges-for-the-time-asked-and-leaps-the-way-held
#[test]
fn a_hooks_jump_put_down_while_it_charges_ends_without_a_leap() {
    let mut app = setup();
    player_description(&mut app, "early-inventory-and-casting");
    let (queue, _) = holder(&mut app);
    frames(&mut app, 2);
    queue
        .borrow_mut()
        .push_back(vec![NavRequest::Hold(NavKey::Jump, true)]);
    frames(&mut app, 6);
    assert!(app.objects().world.combat.jump_pending);
    queue.borrow_mut().push_back(vec![NavRequest::CancelJump]);
    frames(&mut app, 2);
    assert!(!app.objects().world.combat.jump_pending, "put down");
    queue
        .borrow_mut()
        .push_back(vec![NavRequest::Hold(NavKey::Jump, false)]);
    frames(&mut app, 4);
    assert_eq!(app.probe().jump_counts(), (0, 0), "no leap, even let go");
    assert!(body(&app).on_ground());
    app.shutdown();
}

/// Behaviour: plugins.nav.a-held-motion-presses-a-movement-control-for-the-time-asked
#[test]
fn a_hooks_held_sidestep_moves_the_body_sideways_until_let_go() {
    let mut app = setup();
    player_description(&mut app, "early-inventory-and-casting");
    let (queue, seen) = holder(&mut app);
    frames(&mut app, 2);
    seen.borrow_mut().clear();
    let before = body(&app).position().frame.origin;
    queue
        .borrow_mut()
        .push_back(vec![NavRequest::Hold(NavKey::StrafeRight, true)]);
    frames(&mut app, 2);
    let input = app.probe().char_input();
    assert!(
        input.step_right && !input.forward && !input.turn_right,
        "a sidestep: {input:?}"
    );
    frames(&mut app, 60);
    let moved = body(&app).position().frame.origin;
    assert!(
        math::hypotf(moved.x - before.x, moved.y - before.y) > 0.25,
        "the body stepped: {before:?} -> {moved:?}"
    );
    queue
        .borrow_mut()
        .push_back(vec![NavRequest::Hold(NavKey::StrafeRight, false)]);
    frames(&mut app, 30);
    assert!(!app.probe().char_input().step_right, "let go");
    let stopped = body(&app).position().frame.origin;
    frames(&mut app, 10);
    let later = body(&app).position().frame.origin;
    assert!(
        math::hypotf(later.x - stopped.x, later.y - stopped.y) < 0.01,
        "let go, it stands"
    );
    assert!(seen.borrow().iter().all(|s| !*s), "{:?}", seen.borrow());
    app.shutdown();
}
