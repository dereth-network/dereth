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
