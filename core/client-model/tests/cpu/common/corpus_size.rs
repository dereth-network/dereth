//! Shared fixtures for corpus size.

#![allow(dead_code)]

use dereth_client_net::client_session::testing::{session_names, Corpus, Direction, GAME_ACTION};

pub const PLAYER_DESCRIPTION: u32 = 0x0013;

fn corpus() -> Vec<&'static Corpus> {
    Corpus::shared_all()
}

pub fn recorded_sessions() -> usize {
    session_names().len()
}

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

pub fn recorded_client_actions() -> usize {
    let n: usize = corpus()
        .iter()
        .map(|c| c.count(Direction::ClientToServer, GAME_ACTION))
        .sum();
    assert!(
        n > 0,
        "the corpus carries no 0xF7B1 at all; the reader is dead"
    );
    n
}
