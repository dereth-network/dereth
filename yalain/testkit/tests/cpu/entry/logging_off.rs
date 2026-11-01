//! Logging off: the character stays in the world about nineteen seconds before it leaves, frozen
//! and glowing, its leaving behavior played near the end; then into portal space, its own look,
//! out of the world for those who saw it, and the login told it is out with the character list.
//! Fixture: two players of the test world side by side.

use auberean_protocol::login::CharacterRef;
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use yalain_testkit::bot::Received;
use yalain_testkit::Harness;
use yalain_world::world::{log_off, ARRIVAL_FX};

/// Run the world until `seconds` after `since`.
fn until(h: &mut Harness, since: f64, seconds: f64) {
    let t = since + seconds;
    while h.now.0 < t {
        h.step();
    }
}

/// Every message a client received since `from`, on either connection.
fn all(h: &Harness, b: usize, from: usize) -> Vec<ServerMessage> {
    h.bots[b].inbox[from..]
        .iter()
        .filter_map(Received::message)
        .collect()
}

/// The calls a client received on its world connection since `from`, by function.
fn calls(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    h.calls(b, from).into_iter().map(|(f, _)| f).collect()
}

/// A player logs off: at once it is frozen and glows for all who see it; at 15.2 s it plays its
/// leaving behavior; nothing more until 18.9 s, when it goes into portal space, its own look is
/// shown, those who saw it see it leave, and its login is told it is out with the character
/// list.
/// Behaviour: server.world.log-off-lingers
#[test]
fn a_player_logging_off_lingers_in_the_world_before_it_leaves() {
    let mut h = Harness::new();
    let (a, ida) = h.player_in_world("one", "Elara");
    let (c, _) = h.player_in_world("two", "Torvald");
    h.run(10);
    let (from_a, from_c) = (h.bots[a].inbox.len(), h.bots[c].inbox.len());
    let asked = h.now.0;
    h.bots[a].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: ida }),
    );
    h.run(4);
    assert!(calls(&h, a, from_a).contains(&yalain_world::session::function::MOVEMENT_FROZEN));
    let glows = |m: &ServerMessage| matches!(m, ServerMessage::DoFx(e) if e.sender.id == ida && e.action.fx == ARRIVAL_FX);
    assert!(
        h.world_messages(a, from_a).iter().any(glows),
        "glows for itself"
    );
    assert!(
        h.world_messages(c, from_c).iter().any(glows),
        "and for others"
    );
    let behaves = |m: &ServerMessage| matches!(m, ServerMessage::PlayBehavior(e) if e.sender.id == ida && e.action.behavior == log_off::BEHAVIOR);
    until(&mut h, asked, log_off::BEHAVIOR_AFTER - 0.3);
    assert!(!h.world_messages(c, from_c).iter().any(behaves), "not yet");
    until(&mut h, asked, log_off::BEHAVIOR_AFTER + 0.3);
    assert!(
        h.world_messages(c, from_c).iter().any(behaves),
        "the leaving behavior"
    );
    until(&mut h, asked, log_off::LEAVES_AFTER - 0.3);
    let gone =
        |m: &ServerMessage| matches!(m, ServerMessage::ObjectLeaves(e) if e.sender.id == ida);
    assert!(
        !h.world_messages(c, from_c).iter().any(gone),
        "still in the world"
    );
    assert!(
        !all(&h, a, from_a)
            .iter()
            .any(|m| matches!(m, ServerMessage::CharExitGame(_))),
        "not out yet"
    );
    assert!(h.server.players.values().any(|p| p.character == ida));
    until(&mut h, asked, log_off::LEAVES_AFTER + 0.5);
    assert!(calls(&h, a, from_a).contains(&yalain_world::places::function::ENTER_PORTAL_SPACE));
    assert!(h
        .world_messages(a, from_a)
        .iter()
        .any(|m| matches!(m, ServerMessage::UpdateVisualDesc(e) if e.sender.id == ida)));
    assert!(
        h.world_messages(c, from_c).iter().any(gone),
        "seen to leave"
    );
    let login: Vec<ServerMessage> = all(&h, a, from_a);
    let out = login
        .iter()
        .position(|m| matches!(m, ServerMessage::CharExitGame(_)))
        .expect("told it is out");
    assert!(login[out..]
        .iter()
        .any(|m| matches!(m, ServerMessage::CharacterList(_))));
    assert!(!h.server.players.values().any(|p| p.character == ida));
}
