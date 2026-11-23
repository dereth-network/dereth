//! A character that dies on a journey stays down until the server moves it: the journey ends
//! there, and nothing presses movement onto the dead body (the journey's keys, a frame hook's
//! held control, the keys pressed again when the window loses focus). Fixture: the jump key's (a
//! real `App` on real terrain with a recorded player description) stepped at ten frames a second,
//! with a hook that asks for the journey and holds a control, and the server's `Dead` motion for
//! the player constructed.

use std::cell::RefCell;
use std::rc::Rc;

use crate::common::sim_app::{body, frames, player_description};
use dereth_animation::MotionCommand;
use dereth_client_runtime::frame_hook::{FrameHook, HookFrame, NavKey, NavRequest};
use dereth_client_runtime::travel::{TravelAllow, TravelEvent, TravelRequest, TravelTarget};
use dereth_primitives::num::math;

use super::jump_charge_release::{server_state, setup};

/// What the hook asks of the next frame.
#[derive(Debug)]
enum Ask {
    Nav(NavRequest),
    Travel(TravelRequest),
}

/// Hands each frame what is queued for it and keeps what the travel told.
#[derive(Debug, Default)]
struct Hook {
    queue: Rc<RefCell<Vec<Ask>>>,
    told: Rc<RefCell<Vec<TravelEvent>>>,
}

impl FrameHook for Hook {
    fn step(&mut self, frame: &mut HookFrame<'_>) {
        self.told
            .borrow_mut()
            .extend(frame.travel.events.iter().cloned());
        for a in self.queue.borrow_mut().drain(..) {
            match a {
                Ask::Nav(r) => frame.nav(r),
                Ask::Travel(r) => frame.travel(r),
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

/// Behaviour: plugins.nav.a-character-who-dies-on-a-journey-stays-down
/// At ten frames a second, a journey running east dies with its character: the server's `Dead`
/// motion ends it (`dead`), and the body stays dead where it fell for three seconds while a hook
/// holds the forward control and sets the run lock and the window loses focus (which presses held
/// controls again): no control moves it and control is not taken back from the server, so it
/// never stands. Brought to its feet by the server, it runs again.
#[test]
fn a_character_who_dies_on_a_journey_stays_down_and_the_journey_ends() {
    let fps = 10.0;
    let mut app = setup();
    player_description(&mut app, "early-inventory-and-casting");
    app.clock = Box::new(dereth_client_runtime::platform::clock::FixedStepClock::new(
        1.0 / fps,
    ));
    let hook = Hook::default();
    let (queue, told) = (Rc::clone(&hook.queue), Rc::clone(&hook.told));
    app.install_frame_hook(Box::new(hook));
    app.probe_mut().mark_playable();
    // Each plan answered in the frame after it is asked for: the frames outrun the clock.
    app.travel.wait_for_plans = true;
    frames(&mut app, 2);
    let start = body(&app).position();
    let mut goal = start;
    goal.frame.origin.x += 40.0;
    queue.borrow_mut().push(Ask::Travel(TravelRequest::Go {
        id: 7,
        target: TravelTarget::Place(goal),
        allow: TravelAllow::default(),
    }));
    frames(&mut app, 30);
    let set_out = body(&app).position().frame.origin;
    assert!(
        math::hypotf(
            set_out.x - start.frame.origin.x,
            set_out.y - start.frame.origin.y
        ) > 2.0,
        "the journey is under way: {:?}",
        told.borrow()
    );
    // The server kills the character.
    server_state(&mut app, MotionCommand::DEAD, 0, false);
    frames(&mut app, 2);
    let dead = |app: &dereth_client::app::App| {
        app.world_state()
            .and_then(|w| w.character_motion())
            .map(|(_, _, forward)| forward)
    };
    assert_eq!(dead(&app), Some(MotionCommand::DEAD), "dead");
    assert!(
        told.borrow().iter().any(|e| matches!(
            e,
            TravelEvent::Failed { id: 7, reason, .. } if reason == "dead"
        )),
        "the journey ends on the death: {:?}",
        told.borrow()
    );
    // A hook holds forward and sets the run lock, and the window loses focus.
    queue
        .borrow_mut()
        .push(Ask::Nav(NavRequest::Hold(NavKey::Forward, true)));
    frames(&mut app, 2);
    queue.borrow_mut().push(Ask::Nav(NavRequest::Autorun(true)));
    frames(&mut app, 2);
    super::focus_loss::lose_focus(&mut app, 9_000);
    let fell = body(&app).position().frame.origin;
    let mut settled = fell;
    for i in 0..30 {
        frames(&mut app, 1);
        assert_eq!(dead(&app), Some(MotionCommand::DEAD), "stays down");
        assert!(
            app.probe().movement_commands().lists.substate.is_empty(),
            "the held forward key is dropped"
        );
        // The fall itself carries the body back a little in its first second.
        if i == 14 {
            settled = body(&app).position().frame.origin;
        }
    }
    let now = body(&app).position().frame.origin;
    let fallen = math::hypotf(now.x - fell.x, now.y - fell.y);
    let since = math::hypotf(now.x - settled.x, now.y - settled.y);
    assert!(fallen < 1.5, "no further than its fall: {fallen:.2} m");
    assert!(
        since < 0.05,
        "then lies still: moved {since:.2} m in a second and a half"
    );
    // Brought back to its feet by the server, as at the lifestone: the controls move it again.
    server_state(&mut app, MotionCommand::READY, 0, false);
    queue
        .borrow_mut()
        .push(Ask::Nav(NavRequest::Autorun(false)));
    queue
        .borrow_mut()
        .push(Ask::Nav(NavRequest::Hold(NavKey::Forward, false)));
    frames(&mut app, 2);
    queue
        .borrow_mut()
        .push(Ask::Nav(NavRequest::Hold(NavKey::Forward, true)));
    let up = body(&app).position().frame.origin;
    frames(&mut app, 20);
    let after = body(&app).position().frame.origin;
    let ran = math::hypotf(after.x - up.x, after.y - up.y);
    assert!(
        ran > 4.0,
        "on its feet, it runs again: {ran:.2} m in two seconds"
    );
    app.shutdown();
}
