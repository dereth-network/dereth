//! The frame -- what one frame of this client is.
//!
//! The fourteen steps in their order, and the log that counts them.
//!
//! One file per subject, so that two changes adding rows at the same time do not edit the same
//! file. [`ROWS`] is in id order; the registry's own test asserts that, and that no id and no
//! evidence handle is repeated anywhere in it.

// `behaviour!` is `#[macro_export]`ed by `mod.rs` above this module's declaration, so it is in
// textual scope here and needs no import.
use super::{Behaviour, Evidence, Tier, RETAIL, THIS_CLIENT};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "frame.fourteen-steps-in-order-every-frame",
        says: "One tick of the client performs the same fixed list of per-frame jobs in the same \
               order, and it is the same order on the second frame as on the first.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-FRAME-ORDER"),
        station: "dereth-testkit::dat::frame::scenario_the_frame_runs_its_steps_in_order",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "frame.log.is-the-counters-and-keeps-them-whole",
        says: "Everything the client counts about its own frames is one log: a whole frame writes its \
               fourteen steps in order, a frame cut short writes a shorter list rather than a different \
               one, every kind of entry has its own one-line form carrying no address, a total keeps \
               counting after the log has forgotten the entries themselves, and the most recent entry \
               of a kind outlives them too.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-R2-5"),
        station: "dereth-testkit::cpu::frame::scenario_the_frame_log_is_the_counters",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "frame.physics-tick.a-display-at-a-multiple-of-thirty-hertz-steps-the-world-on-a-steady-cadence",
        says: "On a display refreshing at 30, 60, 120 or 240 Hz the world is stepped every first, \
               second, fourth or eighth frame, every time, 30 times a second, whether the clock \
               is read exactly, added up frame by frame, or off a counter with a server's time \
               added and up to half a millisecond of jitter.",
        since: THIS_CLIENT,
        divergence: "CD-042",
        evidence: Evidence::Private("AC-EVID-PHYSICS-TICK-STEADY"),
        station: "dereth-physics::lib::step::tests::a_display_at_a_multiple_of_thirty_hertz_steps_the_world_every_whole_quantum_of_frames",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "frame.physics-tick.a-tick-short-of-the-quantum-steps-the-world-by-the-time-that-passed",
        says: "A step taken a little before a thirtieth of a second has passed covers the time \
               that did pass: every body's clock lands on the frame's, and a falling body's speed \
               is gravity times the time simulated, so bodies cover the same ground each second \
               whatever the beat.",
        since: THIS_CLIENT,
        divergence: "CD-042",
        evidence: Evidence::Private("AC-EVID-PHYSICS-TICK-SPEED"),
        station: "dereth-physics::lib::step::tests::a_tick_opened_short_of_the_quantum_steps_the_world_by_the_time_that_passed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "frame.quit.a-quit-request-stops-the-frame-where-the-message-queue-is-drained",
        says: "A frame that finds the player has asked to quit stops where the window's messages \
               are drained: it does not pump the network and it draws nothing, it reports to the \
               loop that there is no next frame, and the client is shutting down from then on.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-FRAME-ORDER-QUIT"),
        station: "dereth-testkit::dat::frame::scenario_a_quit_request_stops_the_frame_early",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "frame.run.a-bounded-run-draws-exactly-the-frames-it-was-asked-for",
        says: "Told to run a fixed number of frames, the client's own loop draws exactly that \
               many and then stops and shuts itself down, with nobody closing a window.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-FRAME-ORDER-RUN"),
        station: "dereth-testkit::dat::frame::scenario_a_bounded_run_draws_what_it_was_asked_for",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "frame.shutdown.the-last-step-waits-for-the-drawing-to-finish",
        says: "Shutting the client down ends by waiting for the drawing surface to go idle, and \
               that step is reached and really runs -- nothing else has released it earlier in \
               the sequence.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-R2-1-SHUTDOWN"),
        station: "dereth-testkit::dat::frame::scenario_the_shutdown_waits_for_the_drawing_to_finish",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "frame.snapshot.an-owned-frame-agrees-with-the-live-one-and-outlives-it",
        says: "The frame a reader takes away as a value answers the same as the live client does \
               for the character, the body and the combat mode, and it is still readable after \
               the client has run whole frames on top of it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-R3-1-SNAPSHOT"),
        station: "dereth-testkit::dat::frame::scenario_an_owned_frame_outlives_the_live_one",
        tier: Tier::Dat,
    },
];
