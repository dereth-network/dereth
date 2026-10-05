use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_primitives::LocalTime;
use dereth_protocol::actions::unpack_action;
use dereth_protocol::movement::{MovementMoveToState, RawMotionState};
use dereth_protocol::Message;
use {
    dereth_client_runtime::character::Character, dereth_client_runtime::character::CharacterInput,
};

/// The command a walk forward reports, as a literal: reading it back through the client's own
/// constant would not notice a wrong constant.
pub const WALK_FORWARD: u32 = 0x4500_0005;
/// The run hold key. Invalid is 0, None is 1.
pub const HOLD_KEY_RUN: u32 = 2;
/// The opcode a movement report travels under, inside a game action.
pub const MOVE_TO_STATE: u32 = 0xF61C;
const GAME_ACTION: u32 = 0xF7B1;

/// How many frames one [`hold`] consumes.
pub const HOLD: u32 = 60;

/// A body standing still on the default landblock's terrain, settled for two seconds.
pub fn settled_character(store: &std::sync::Arc<dereth_dat::RetailDatStore>) -> Character {
    let region = dereth_client_runtime::landblock::load_region(store).expect("the region decodes");
    let mut c = Character::new(
        store,
        &region,
        dereth_client_runtime::landblock::DEFAULT_LANDBLOCK,
        (96.0, 96.0),
    )
    .expect("the character is created");
    for i in 1..=60 {
        c.update(LocalTime(f64::from(i) / 30.0));
    }
    assert!(
        c.on_ground(),
        "the body must settle before anything is measured"
    );
    c
}

/// Hold `input` for two seconds of frames from `from`, and answer the distance covered in the
/// **second** of them -- the settled ground speed. The first second is discarded on purpose:
/// the animation and the physics ramp the velocity, so a window that starts at a key edge
/// measures the transition and not the speed.
pub fn hold(c: &mut Character, input: CharacterInput, from: u32) -> f32 {
    c.input = input;
    for i in from..from + 30 {
        c.update(LocalTime(f64::from(i + 1) / 30.0));
    }
    let before = c.position().frame.origin;
    for i in from + 30..from + 60 {
        c.update(LocalTime(f64::from(i + 1) / 30.0));
    }
    let after = c.position().frame.origin;
    ((after.x - before.x).powi(2) + (after.y - before.y).powi(2)).sqrt()
}

/// Every recorded movement report the whole corpus carries, with the blob it came from.
///
/// The corpus is enumerated through its own index, so promoting a recording changes what this
/// reads and pins no count anywhere.
pub fn recorded_move_to_states() -> Vec<(String, Vec<u8>, MovementMoveToState)> {
    let mut out = Vec::new();
    for corpus in Corpus::load_all() {
        let mut blobs: Vec<_> = corpus
            .blobs
            .iter()
            .filter(|b| b.dir == Direction::ClientToServer && b.opcode == GAME_ACTION)
            .collect();
        blobs.sort_by_key(|b| b.blob_id);
        for b in blobs {
            let mut action = unpack_action(&b.payload).expect("a recorded game action must unpack");
            if action.sub_type.0 != MOVE_TO_STATE {
                continue;
            }
            let m = MovementMoveToState::read(&mut action.body).expect("a recorded body decodes");
            action
                .body
                .expect_exhausted()
                .expect("a recorded body is fully consumed");
            out.push((corpus.name.clone(), b.payload.clone(), m));
        }
    }
    out
}

/// What the client would put on the wire for the body as it stands.
pub fn wire(c: &Character) -> RawMotionState {
    dereth_client_runtime::app::raw_motion_state_to_wire(&c.driver().movement.interp.raw_state)
}
