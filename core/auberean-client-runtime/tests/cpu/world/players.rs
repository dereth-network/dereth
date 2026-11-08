//! How many players a world says it has, as the client passes it on.

use auberean_client_runtime::data::{DataFiles, ENGLISH};
use auberean_client_runtime::{Client, ClientData, ClientSettings};
use auberean_protocol::login::Notice;
use auberean_protocol::text::StringInfo;
use auberean_protocol::ServerMessage;

/// The blobs of the world connection's game queue, numbered from 1.
const BLOB: u64 = 0x0300_0000_0000_0000;

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

fn line(c: &mut Client, blob_id: u64, kind: u32, text: StringInfo) {
    c.take_message(blob_id, ServerMessage::Notice(Notice { kind, text }));
}

/// Behaviour: session.players.a-worlds-line-of-how-many-others-are-on-counts-its-players
#[test]
fn a_worlds_welcome_line_of_how_many_others_are_on_is_taken_once() {
    let mut c = client();
    assert_eq!(c.take_world_players(), None, "nothing said yet");
    line(
        &mut c,
        BLOB | 1,
        8,
        StringInfo::literal("-- Welcome to the server --\n"),
    );
    line(
        &mut c,
        BLOB | 2,
        8,
        StringInfo::literal("There are currently 2 other players logged in.\n"),
    );
    assert_eq!(c.take_world_players(), Some(3), "the two and the player");
    assert_eq!(c.take_world_players(), None, "taken once");

    let mut from_a_table = StringInfo::literal("There are currently 5 other players logged in.");
    from_a_table.string_id = 0x7E00_0001;
    line(&mut c, BLOB | 3, 8, from_a_table);
    assert_eq!(
        c.take_world_players(),
        None,
        "a line of the game's own strings"
    );
}
