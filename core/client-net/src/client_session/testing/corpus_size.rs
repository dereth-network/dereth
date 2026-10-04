//! The recorded corpus's own sizes, read back through `dereth_client_net::client_session::testing`.
//!
//! Every figure here is the corpus counting itself: `session_names()` reads the index and `Corpus`
//! reads the blob streams, so a denominator is never an integer typed into a test and promoting a
//! recording re-measures it instead of reddening it. Counts over the retail dats are not here: the
//! installed data set is fixed, so those denominators stay with their tests.
//!
//! The client and model test binaries share these counts and the parsed corpus cache.

use super::{capture, session_names, Corpus, Direction, GAME_ACTION};

/// Whether `path` (a `fixtures/packet-captures/<slug>.jsonl`) is a recording that ends without a
/// clean `Disconnect`. Those sit in the folder beside the others, but the message corpus index
/// does not carry them, so a scan of the folder whose count is compared with
/// [`recorded_sessions`] skips them -- by the capture index's own list, which the corpus tool
/// writes, rather than by names written here.
pub fn is_unclean_logout_recording(path: &std::path::Path) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|s| capture::without_disconnect().contains(&s))
}

/// `0x0013 Login_PlayerDescription` -- the ordered game event that puts a character in the world.
/// It arrives inside the `0xF7B0` envelope, so it is counted with `count_event`, not `count`.
pub const PLAYER_DESCRIPTION: u32 = 0x0013;

/// `0xF74C Movement_SetObjectMovement`.
pub const MOVEMENT: u32 = 0xF74C;

/// Every scenario the corpus index names, parsed once per test binary.
pub fn corpus() -> Vec<&'static Corpus> {
    Corpus::shared_all()
}

/// How many recordings the corpus holds, from `fixtures/message-corpus/index.json`.
///
/// `session_names()` panics on an index that names nothing, so this cannot answer zero.
pub fn recorded_sessions() -> usize {
    session_names().len()
}

/// How many of them get a character into the world -- i.e. carry a `0x0013` PlayerDescription.
///
/// The floor is explicit: a corpus that answered zero here would let "eleven of the thirteen"
/// pass as "none of none".
pub fn recorded_world_sessions() -> usize {
    let n = corpus()
        .iter()
        .filter(|c| c.count_event(PLAYER_DESCRIPTION) > 0)
        .count();
    assert!(
        n > 0,
        "no recording reaches 0x0013; the corpus read nothing"
    );
    n
}

/// The complement of [`recorded_world_sessions`]: the login-only recordings.
pub fn recorded_login_only_sessions() -> usize {
    recorded_sessions() - recorded_world_sessions()
}

/// Recorded blobs travelling `dir` with `opcode`, over the whole corpus.
///
/// No floor: a zero is a legitimate answer for an opcode no recording carries, and a call site
/// that needs the zero to be a measurement rather than a dead reader asserts its own calibration.
pub fn recorded_blobs(dir: Direction, opcode: u32) -> usize {
    corpus().iter().map(|c| c.count(dir, opcode)).sum()
}

/// Every recorded server-to-client blob.
pub fn recorded_server_blobs() -> usize {
    recorded_in_dir(Direction::ServerToClient)
}

/// Every recorded client-to-server blob.
pub fn recorded_client_blobs() -> usize {
    recorded_in_dir(Direction::ClientToServer)
}

/// Every recorded client game action -- the `0xF7B1` envelope, client to server.
pub fn recorded_client_actions() -> usize {
    let n = recorded_blobs(Direction::ClientToServer, GAME_ACTION);
    assert!(
        n > 0,
        "the corpus carries no 0xF7B1 at all; the reader is dead"
    );
    n
}

/// Every recorded `0xF74C` movement body.
pub fn recorded_movement_events() -> usize {
    let n = recorded_blobs(Direction::ServerToClient, MOVEMENT);
    assert!(
        n > 0,
        "the corpus carries no 0xF74C at all; the reader is dead"
    );
    n
}

fn recorded_in_dir(dir: Direction) -> usize {
    let n: usize = corpus()
        .iter()
        .map(|c| c.blobs.iter().filter(|b| b.dir == dir).count())
        .sum();
    assert!(
        n > 0,
        "the corpus carries no blob in {dir:?}; the reader is dead"
    );
    n
}
