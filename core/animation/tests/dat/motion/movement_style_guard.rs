//! A movement buffer repeating the current style issues no motion while a new style does; the split
//! unpack entry points compose into the original.
//! Fixture: the shipped retail DAT records and recorded inputs.

use super::common;

use std::sync::Arc;

use dereth_animation::data::AnimAssets;
use dereth_animation::{MotionCommand, MotionDriver};
use dereth_primitives::DataId;

/// `Character::new`'s own two ids.
const ALUVIAN_MALE_SETUP: DataId = DataId(0x0200_0001);
const ALUVIAN_MALE_MOTION_TABLE: DataId = DataId(0x0900_0001);

fn driver() -> MotionDriver {
    let assets = Arc::new(common::open());
    let setup = AnimAssets::setup(assets.as_ref(), ALUVIAN_MALE_SETUP).expect("the setup decodes");
    let mut d = MotionDriver::new(assets as Arc<dyn AnimAssets>);
    assert!(d.set_setup(setup), "the part array is built");
    assert!(
        d.set_motion_table(ALUVIAN_MALE_MOTION_TABLE),
        "the motion table loads"
    );
    d
}

/// Behaviour: movement.style.a-repeated-style-word-issues-no-motion-and-a-new-one-does
/// **Both directions of that comparison, from one driver.**
///
/// A style that differs is issued and lands; a style already in force is **not** issued, and the
/// proof of that is `pending_motions`, not the style word: a redundant `DoMotion` pushes a motion
/// node that then waits on. See the module header for the
/// measurement that corrected this explanation.
#[test]
fn a_repeated_style_word_issues_no_motion_and_a_new_one_does() {
    let mut d = driver();
    // Both motion-state constructors start at `NonCombat` (`command.rs`, and the interpreted
    // motion state's own constructor).
    assert_eq!(
        d.movement.interp.interpreted_state.current_style,
        MotionCommand::NON_COMBAT,
        "the client's own opening stance"
    );

    // --- the `je` is taken: the style is already in force -----------------------------------
    let before = (
        d.sequence.nodes().len(),
        d.sequence.curr(),
        d.sequence.first_cyclic(),
        d.sequence.frame_number(),
        d.movement.interp.pending_motions.len(),
    );
    assert!(
        before.0 > 0,
        "the default state queued a standing cycle to observe"
    );
    d.apply_movement_style(MotionCommand::NON_COMBAT);
    let after = (
        d.sequence.nodes().len(),
        d.sequence.curr(),
        d.sequence.first_cyclic(),
        d.sequence.frame_number(),
        d.movement.interp.pending_motions.len(),
    );
    assert_eq!(
        before, after,
        "a style word repeating the stance in force issued a DoMotion anyway: the equality \
         the only thing that stops every 0xF74C parking a motion node that the next approach then \
         waits on"
    );
    assert_eq!(
        d.movement.interp.interpreted_state.current_style,
        MotionCommand::NON_COMBAT
    );

    // --- the `je` is not taken: a different style ---------------------------------------------
    d.apply_movement_style(MotionCommand::BOW_COMBAT);
    assert_eq!(
        d.movement.interp.interpreted_state.current_style,
        MotionCommand::BOW_COMBAT,
        "the stance did not follow a style word that differs"
    );
    let bow = (
        d.sequence.nodes().len(),
        d.sequence.curr(),
        d.sequence.first_cyclic(),
        d.sequence.frame_number(),
        d.movement.interp.pending_motions.len(),
    );
    assert_ne!(before, bow, "entering BowCombat changed nothing at all");

    // ..and now *that* one repeats and must be inert in its turn, so the assertion above is not
    // satisfied by a driver that happens never to re-queue anything.
    d.apply_movement_style(MotionCommand::BOW_COMBAT);
    assert_eq!(
        bow,
        (
            d.sequence.nodes().len(),
            d.sequence.curr(),
            d.sequence.first_cyclic(),
            d.sequence.frame_number(),
            d.movement.interp.pending_motions.len()
        ),
        "the guard held for NonCombat and not for BowCombat"
    );
    assert_eq!(
        d.movement.interp.interpreted_state.current_style,
        MotionCommand::BOW_COMBAT
    );
}

/// The split entry points compose into the original.
#[test]
fn the_split_entry_points_compose_into_the_original() {
    use dereth_animation::motion::InterpretedMotionState;
    let mut whole = driver();
    let mut split = driver();
    let state = InterpretedMotionState {
        current_style: MotionCommand::HAND_COMBAT,
        forward_command: MotionCommand::RUN_FORWARD,
        forward_speed: 1.0,
        ..InterpretedMotionState::default()
    };
    whole.unpack_interpreted_movement(MotionCommand::HAND_COMBAT, &state, false);
    split.apply_movement_style(MotionCommand::HAND_COMBAT);
    split.move_to_interpreted_state(&state, false);

    let read = |d: &MotionDriver| {
        (
            d.movement.interp.interpreted_state.current_style,
            d.movement.interp.interpreted_state.forward_command,
            d.movement.interp.raw_state.current_style,
            d.sequence.nodes().len(),
            d.sequence.curr(),
            d.sequence.first_cyclic(),
            d.sequence.frame_number(),
            d.movement.interp.pending_motions.len(),
        )
    };
    assert_eq!(read(&whole), read(&split), "the split is not the whole");
    assert_eq!(
        whole.movement.interp.interpreted_state.current_style,
        MotionCommand::HAND_COMBAT
    );
    assert_eq!(
        whole.movement.interp.interpreted_state.forward_command,
        MotionCommand::RUN_FORWARD
    );
}
