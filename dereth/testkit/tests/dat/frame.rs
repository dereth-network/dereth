//! The `dat` tier's frame scenarios: what one frame of a whole retail-backed client does -- the
//! fixed step order, a quit request, a bounded run, the shutdown wait and an owned frame snapshot.
//!
//! Every scenario here builds a whole client -- `App::with_platform(cfg, NullPresentation,
//! Platform::headless(w, h))` -- and **this binary must run serially**, because two headless
//! clients in one process share the UI request globals.
//!
//! Each scenario is a `pub fn` with a `#[test]` beside it that runs it and checks its declaration.
//! `ALL` is this file's own list, concatenated with the other subjects' in `census.rs`, so a
//! scenario that is written and not listed shows up as a shortfall rather than as a silent gap.

use dereth_client::frame::FrameStep;
use dereth_testkit::adapters_shell::{build_app, AppSpec};
use dereth_testkit::{ClientSpec, HeadlessClient};

// -------------------------------------------------------------------------------------------
// 11. frame.fourteen-steps-in-order-every-frame
// -------------------------------------------------------------------------------------------

/// One tick does the same fixed list of jobs in the same order, and so does the next.
pub fn the_frame_runs_its_steps_in_order() {
    let mut c = HeadlessClient::new(ClientSpec::retail());
    c.tick(1);
    let first: Vec<FrameStep> = c.view().expect_app().last_frame_steps().to_vec();
    c.tick(1);

    c.assert_behaviour("frame.fourteen-steps-in-order-every-frame", move |v| {
        let second = v.expect_app().last_frame_steps();
        first == FrameStep::ORDER && second == FrameStep::ORDER && v.frames() == 2
    });
    c.shutdown();
}

dereth_testkit::scenarios! {
    scenario_the_frame_runs_its_steps_in_order => the_frame_runs_its_steps_in_order ["frame.fourteen-steps-in-order-every-frame"],
    scenario_a_quit_request_stops_the_frame_early => a_quit_request_stops_the_frame_early ["frame.quit.a-quit-request-stops-the-frame-where-the-message-queue-is-drained"],
    scenario_a_bounded_run_draws_what_it_was_asked_for => a_bounded_run_draws_what_it_was_asked_for ["frame.run.a-bounded-run-draws-exactly-the-frames-it-was-asked-for"],
    scenario_the_shutdown_waits_for_the_drawing_to_finish => the_shutdown_waits_for_the_drawing_to_finish ["frame.shutdown.the-last-step-waits-for-the-drawing-to-finish"],
    scenario_an_owned_frame_outlives_the_live_one => an_owned_frame_outlives_the_live_one ["frame.snapshot.an-owned-frame-agrees-with-the-live-one-and-outlives-it"],
}

// ---------------------------------------------------------------------------------------------
// frame.quit.* and frame.run.*
//
// Both own their `App` outright rather than going through `HeadlessClient`, and the reason is in
// `dereth_testkit::adapters_shell::AppSpec`: a frame that answers *false* is what these two claims
// are about, and `HeadlessClient::tick` asserts it answered true -- which is right for every other
// scenario in the crate and is exactly wrong for these.
// ---------------------------------------------------------------------------------------------

/// A quit request stops the frame where the window's messages are drained.
pub fn a_quit_request_stops_the_frame_early() {
    use dereth_client::app::AppState;

    let mut app = build_app(&AppSpec::default());
    // One whole frame first, so that "the frame stopped early" is measured against this client's
    // own full frame and not against a table alone.
    assert!(
        app.frame(),
        "a frame with nothing asking to quit continues the loop"
    );
    let whole = app.last_frame_steps().to_vec();

    app.done();
    let continued = app.frame();
    let cut = app.last_frame_steps().to_vec();
    let drawn = app.frames_drawn();
    let state = app.state();
    app.shutdown();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "frame.quit.a-quit-request-stops-the-frame-where-the-message-queue-is-drained",
        move |_| {
            whole == FrameStep::ORDER
                // It stopped, and it stopped at the message drain: the cut list is a prefix of
                // the full one and is shorter than it, so this is a truncation and not a
                // different order.
                && !continued
                && cut.len() < whole.len()
                && whole.starts_with(&cut)
                && cut.last() == Some(&FrameStep::ProcessWindowEvents)
                // Nothing was drawn by the cut frame -- the count is still the first frame's one.
                && drawn == 1
                && state == AppState::ShuttingDown
        },
    );
}

/// Told to run three frames, the client's own loop runs three and stops.
pub fn a_bounded_run_draws_what_it_was_asked_for() {
    use dereth_client::app::AppState;

    const WANT: u64 = 3;

    let mut app = build_app(&AppSpec {
        frames: Some(WANT),
        ..AppSpec::default()
    });
    let ran = app.run();
    let drawn = app.frames_drawn();
    let state = app.state();
    app.shutdown();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "frame.run.a-bounded-run-draws-exactly-the-frames-it-was-asked-for",
        move |_| ran == WANT && drawn == WANT && state == AppState::ShuttingDown,
    );
}

// ---------------------------------------------------------------------------------------------
// frame.shutdown.* and frame.snapshot.*
//
// Three neighbouring claims -- a client with no device builds and runs a frame, the step list is
// the same without one, and the four device steps reach the presentation once per frame -- are
// asserted by `headless.a-whole-client-with-no-device-or-window` in `login.rs` and by the scenario
// above, so they are not written twice here.
// ---------------------------------------------------------------------------------------------

/// The shutdown sequence ends by waiting for the drawing surface.
pub fn the_shutdown_waits_for_the_drawing_to_finish() {
    use dereth_client::present::{NullPresentation, NullPresentationCounts};
    use dereth_client::shutdown::{Outcome, Step};

    let mut app = build_app(&AppSpec::default());
    assert!(app.frame());
    let idle_before = app
        .presentation()
        .as_any()
        .downcast_ref::<NullPresentation>()
        .expect("the presentation this client was built with")
        .counts();
    let released_early = |c: &NullPresentationCounts| c.wait_idle;

    // `App::shutdown` takes the client by value, so the release is read off the log it answers
    // with rather than off the counter -- which is the same fact, and the only one still
    // readable afterwards.
    let log = app.shutdown();
    let outcome = log.outcome(Step::WaitIdle);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "frame.shutdown.the-last-step-waits-for-the-drawing-to-finish",
        move |_| released_early(&idle_before) == 0 && outcome == Some(Outcome::Ran),
    );
}

/// The frame taken away as a value says what the live one says, and survives it.
pub fn an_owned_frame_outlives_the_live_one() {
    use dereth_client_contract::GameView as _;

    let mut c = HeadlessClient::new(ClientSpec::retail());
    c.tick(1);
    // The owned value, and the live client's own answers beside it.
    let snap = c.snapshot();
    let (live_player, live_name, live_mode) = {
        let app = c.view().expect_app();
        let objects = app.objects();
        let view = app.hud().view(objects);
        (
            view.player(),
            view.character_name().map(str::to_owned),
            view.combat_mode(),
        )
    };
    let agrees = snap.player() == live_player
        && snap.character_name().map(str::to_owned) == live_name
        && snap.combat_mode() == live_mode;

    // The point of the type: the client is mutable again with the snapshot still in hand, which a
    // borrowing view could not be.
    c.tick(2);
    let still_readable =
        snap.player().is_none() && snap.character_name().map(str::to_owned) == live_name;
    let frames = c.frames();

    c.assert_behaviour(
        "frame.snapshot.an-owned-frame-agrees-with-the-live-one-and-outlives-it",
        move |_| agrees && still_readable && frames == 3,
    );
    c.shutdown();
}
