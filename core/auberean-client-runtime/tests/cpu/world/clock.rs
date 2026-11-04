//! The game clock and the server's time-sync headers.

use auberean_client_runtime::data::{DataFiles, ENGLISH};
use auberean_client_runtime::{Client, ClientData, ClientSettings};

fn client() -> Client {
    Client::new(
        ClientSettings {
            login_server: "127.0.0.1:9000".parse().expect("an address"),
            account: b"a".to_vec(),
            ticket: Vec::new(),
            timestamp: 0,
            session_key: [0; 64],
            script_sends: false,
        },
        ClientData {
            files: DataFiles {
                files: Vec::new(),
                language: ENGLISH,
            },
            scripts: None,
        },
    )
}

fn at(c: &mut Client, t: f64) {
    c.set_time(auberean_client_net::LocalTime(t));
}

/// The server's time-sync clock sets the game clock, which then runs on with the client's own;
/// a time-sync clock behind the game clock leaves it as it is.
///
/// Behaviour: session.time.the-game-clock-moves-forward-to-the-servers-time-sync
#[test]
fn the_game_clock_moves_forward_to_the_servers_time_sync() {
    let mut c = client();
    at(&mut c, 10.0);
    c.sync_time(1000.0);
    assert!((c.game_time() - 1000.0).abs() < 1e-9);
    at(&mut c, 12.0);
    assert!((c.game_time() - 1002.0).abs() < 1e-9, "runs on");
    c.sync_time(1001.0);
    assert!((c.game_time() - 1002.0).abs() < 1e-9, "never back");
    c.sync_time(1005.0);
    assert!((c.game_time() - 1005.0).abs() < 1e-9, "forward");
}

/// Before the server's first time the game clock is the client's own elapsed time, each frame's;
/// it never runs back, though the host's clock may.
///
/// Behaviour: session.time.the-game-clock-is-the-elapsed-time-until-the-server-moves-it
#[test]
fn the_game_clock_is_the_elapsed_time_until_the_server_moves_it() {
    let mut c = client();
    at(&mut c, 5.0);
    assert_eq!(c.game_time(), 5.0, "the elapsed time");
    c.frame(auberean_client_net::LocalTime(7.0));
    assert_eq!(c.game_time(), 7.0, "each frame's");
    at(&mut c, 3.0);
    assert_eq!(c.game_time(), 7.0, "never back");
    c.sync_time(100.0);
    at(&mut c, 9.0);
    assert_eq!(
        c.game_time(),
        102.0,
        "the server's time, then on with the client's"
    );
    at(&mut c, 8.0);
    assert_eq!(c.game_time(), 102.0, "never back");
}

/// A position the server gives for the player places its body and leaves the game clock alone,
/// before the server's first time-sync and after it, whether the position's time is ahead of the
/// game clock or behind it.
///
/// Behaviour: session.time.a-players-position-leaves-the-game-clock-alone
#[test]
fn a_players_position_leaves_the_game_clock_alone() {
    use auberean_protocol::physics::{CreateObject, ObjectPosition};
    use auberean_protocol::types::ObjectStamp;
    use auberean_protocol::ServerMessage;
    const PLAYER: u64 = 0x5000_0001;
    const CELL: u32 = 0x0298_0109;
    let mut c = client();
    c.model.player = Some(PLAYER);
    at(&mut c, 10.0);
    let position = |time, teleport_stamp| {
        ServerMessage::Position(ObjectPosition {
            sender: ObjectStamp {
                id: PLAYER,
                ..ObjectStamp::default()
            },
            time,
            cell: CELL,
            teleport_stamp,
            ..ObjectPosition::default()
        })
    };
    // Blobs posted as they come: the player's object, then its positions.
    const POSTED: u64 = 0x0300_0000_0000_0000;
    c.take_message(
        POSTED,
        ServerMessage::CreateObject(CreateObject {
            id: PLAYER,
            ..CreateObject::default()
        }),
    );
    c.take_message(POSTED | 1, position(5000.0, 1));
    assert!(c.body.placed() && c.body.cell == CELL, "the body placed");
    assert_eq!(c.game_time(), 10.0, "still the elapsed time");
    c.sync_time(1000.0);
    c.take_message(POSTED | 2, position(1100.0, 2));
    assert_eq!(c.body.teleport_stamp, 2, "the position taken");
    assert_eq!(c.game_time(), 1000.0, "not forward");
    c.take_message(POSTED | 3, position(900.0, 3));
    assert_eq!(c.body.teleport_stamp, 3, "the position taken");
    assert_eq!(c.game_time(), 1000.0, "not back");
}

/// A position of the player's own moves the body only when newer: a newer teleport puts it there
/// facing the position's heading; a newer forced position puts it there facing as it faced; one
/// with no newer stamp, or an old stamp, leaves it.
///
/// Behaviour: session.time.a-position-of-the-players-own-moves-it-by-its-stamps
#[test]
fn a_position_of_the_players_own_moves_it_by_its_stamps() {
    use auberean_primitives::Vec3;
    use auberean_protocol::physics::{CreateObject, ObjectPosition};
    use auberean_protocol::types::ObjectStamp;
    use auberean_protocol::ServerMessage;
    const PLAYER: u64 = 0x5000_0001;
    const POSTED: u64 = 0x0300_0000_0000_0000;
    let mut c = client();
    c.model.player = Some(PLAYER);
    let at_x = |x: f32, byte: u32, teleport_stamp, force_position_stamp| {
        ServerMessage::Position(ObjectPosition {
            sender: ObjectStamp {
                id: PLAYER,
                ..ObjectStamp::default()
            },
            cell: 0x0298_0109,
            origin: Vec3::new(x, 0.0, 0.0),
            heading_motion: byte << 24,
            teleport_stamp,
            force_position_stamp,
            ..ObjectPosition::default()
        })
    };
    let quarter = 64.0 * 360.0 / 255.0;
    c.take_message(
        POSTED,
        ServerMessage::CreateObject(CreateObject {
            id: PLAYER,
            ..CreateObject::default()
        }),
    );
    c.take_message(POSTED | 1, at_x(1.0, 64, 1, 1));
    assert_eq!((c.body.origin.x, c.body.heading), (1.0, quarter), "placed");
    c.take_message(POSTED | 2, at_x(2.0, 128, 1, 1));
    assert_eq!(
        (c.body.origin.x, c.body.heading),
        (1.0, quarter),
        "no newer"
    );
    c.take_message(POSTED | 3, at_x(3.0, 128, 1, 2));
    assert_eq!(
        (c.body.origin.x, c.body.heading),
        (3.0, quarter),
        "forced there, facing as it faced"
    );
    c.take_message(POSTED | 4, at_x(4.0, 128, 0, 2));
    assert_eq!(c.body.origin.x, 3.0, "an old teleport moves nothing");
    c.take_message(POSTED | 5, at_x(5.0, 128, 2, 2));
    assert_eq!(
        (c.body.origin.x, c.body.heading),
        (5.0, 128.0 * 360.0 / 255.0),
        "a teleport takes its heading"
    );
}
