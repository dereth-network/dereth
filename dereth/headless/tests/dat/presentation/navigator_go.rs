//! The Navigator sample's Go in a session the player entered (not a scripted run): the journey it
//! asks for sets out from where the character stands, walks it to its target and says so. The
//! desktop client runs the same runtime and the same plugin host, so this is its Go too.
//! Fixture: the shipped retail DAT records, first-login-walk-jump's recorded login into the world
//! (fed on to the moment the server shows the character), and the Navigator package as it ships.

use dereth_headless::run::Options;
use dereth_headless::{parse, run};

/// Behaviour: plugins.nav.a-journey-sets-out-in-a-session-the-player-entered
/// `/nav Holtburg` then `/nav go` in a replayed login: the client plans a walk from the
/// character's place, carries it out, and the Navigator says it arrived; the character ends a few
/// metres south, where the Navigator's list puts Holtburg.
///
/// Falsified by a client that carries journeys out only in a scripted run: the journey is never
/// begun, and the Navigator says nothing after pointing.
#[test]
fn the_navigators_go_walks_the_character_to_its_target_in_a_players_own_session() {
    dereth_client_sdk::dat::testing::require_dats();
    let navigator = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../plugins/navigator");
    let opts = Options {
        dat_dir: dereth_client_sdk::dat::testing::dat_dir(),
        world: true,
        plugins: vec![navigator],
        ..Options::default()
    };
    let commands = parse(
        "login first-login-walk-jump +Aldis\n\
         replay 9\n\
         tick 30\n\
         position\n\
         say /nav Holtburg\n\
         tick 2\n\
         say /nav go\n\
         tick 600\n\
         chat\n\
         position\n\
         quit\n",
    )
    .expect("the script parses");
    let mut out: Vec<u8> = Vec::new();
    run(&commands, &opts, &mut out).expect("the run finishes");
    let text = String::from_utf8(out).expect("text");
    let said: Vec<&str> = text
        .lines()
        .filter_map(|l| l.strip_prefix("chat 0 [Nav] "))
        .collect();
    assert_eq!(
        said,
        [
            "Pointing at Holtburg (42.2N 33.8E).",
            "Going by Holtburg: walk (about 4 s).",
            "Arrived at Holtburg.",
        ],
        "{text}"
    );
    let places: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with("position "))
        .collect();
    assert_eq!(places.len(), 2, "{text}");
    assert!(
        places[0].starts_with("position cell=0xA9B4002A origin=126.644,37.098,")
            && places[1].starts_with("position cell=0xA9B40021 "),
        "from the recorded place to Holtburg's, a cell south: {places:?}"
    );
}
