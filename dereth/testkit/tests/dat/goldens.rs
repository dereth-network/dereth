//! The two committed goldens.
//!
//! Each is the frame log of one run, in the log's own one-line text form, diffed against a file
//! under `tests/golden/`. `DERETH_TEST_GOLDEN=1` rewrites instead of comparing; the rewrite's output is a
//! diff to read, not a way to make a red test green.
//!
//! What they are for is in [`dereth_testkit::golden`]: a run that says *this is every event it
//! produced* fails on a new event, a missing event and a reordered pair alike, where a counter
//! per thing counted only fails on the one it counts.

use dereth_testkit::{golden, ClientSpec, Given, HeadlessClient, Inbound};

/// **Golden 1 -- the headless login.** The steps the script-driven headless client runs to take a
/// recorded session from nothing to the character-select screen.
pub fn the_headless_login() {
    let mut c = HeadlessClient::new(ClientSpec::retail());
    c.given(Given::EnteredWorld {
        session: "first-login-walk-jump",
        character: None,
    });
    let text = c.events_text();
    assert!(
        text.lines().count() > 14,
        "a login that produced one frame of events has not replayed anything: {text}"
    );
    golden::check("headless_login", &text);
    c.shutdown();
}

#[test]
fn golden_the_headless_login() {
    the_headless_login();
}

/// **Golden 2 -- a corpus slice.** A slice of a recorded world session delivered into a client
/// whose screens are up, and every event the frames around it produced.
///
/// The recording is the Academy, which is where a new character starts, so the slice carries
/// object creations, physics state and the tutorial's own messages rather than only a handshake.
/// The client is built with its shell so that the panels those messages reach are there to reach.
pub fn a_corpus_slice() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    // Two frames of nothing first, so that the golden begins with what an idle frame does and a
    // change to *that* is visible here too.
    c.tick(2)
        .when(Inbound::from_corpus("long-solo-play", 0..600))
        .tick(6);
    let text = c.events_text();
    assert!(
        !c.view().objects().is_empty(),
        "the slice put nothing in the world, so this golden is of an idle client"
    );
    golden::check("corpus_slice", &text);
    c.shutdown();
}

#[test]
fn golden_a_corpus_slice() {
    a_corpus_slice();
}
