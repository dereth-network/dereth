//! A journey out of the Training Academy, planned in a session recorded there: the Navigator's
//! plan from the start of Shoushi's Academy hands Jonathan's token back, where the server's
//! published portal table knows only the exit portal in the Academy's far part. Fixture: the
//! shipped retail DAT records, long-solo-play's recorded login into the Academy (its
//! character, Jonathan and the rest as the server showed them), a published table of one portal
//! (the Academy's exit, as the world places it), and the Navigator package as it ships.

use dereth_headless::run::Options;
use dereth_headless::{parse, run};

/// The server's table with the Academy's exit portal alone: it stands in a part of the dungeon no
/// walk from the start reaches, and sends to Shoushi.
const PUBLISHED: &str = "dereth-navigation-store\t1\nworld\tRecorded\n\
    portal\tpublished\t29337\tExit to Shoushi\t0x7F030169 158.641 -149.516 -6.063\t\
    0xDA55001D 84.800 99.000 20.000\t0\t0\t0\t-\t-\t-\n";

/// The Navigator's words, from a replayed login into Shoushi's Academy, a published table (or
/// none), and a plan asked for.
fn plan(published: Option<&str>) -> Vec<String> {
    dereth_client_sdk::dat::testing::require_dats();
    let navigator = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../plugins/navigator");
    let table = published.map(|text| {
        let dir = std::env::temp_dir().join(format!(
            "dereth-headless-academy-exit-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("a temporary folder");
        let path = dir.join("portals.tsv");
        std::fs::write(&path, text).expect("the table is written");
        path
    });
    let opts = Options {
        dat_dir: dereth_client_sdk::dat::testing::dat_dir(),
        world: true,
        plugins: vec![navigator],
        nav_published: table,
        ..Options::default()
    };
    let commands = parse(
        "login long-solo-play +Brann\n\
         replay 9\n\
         tick 30\n\
         position\n\
         say /nav plan Shoushi\n\
         tick 120\n\
         chat\n\
         quit\n",
    )
    .expect("the script parses");
    let mut out: Vec<u8> = Vec::new();
    run(&commands, &opts, &mut out).expect("the run finishes");
    let text = String::from_utf8(out).expect("text");
    assert!(
        text.lines()
            .any(|l| l.starts_with("position cell=0x7F0301AD ")),
        "the character stands at the Academy's start: {text}"
    );
    text.lines()
        .filter_map(|l| l.strip_prefix("chat 0 [Nav] "))
        .map(str::to_owned)
        .collect()
}

/// Behaviour: plugins.nav.a-journey-out-of-the-academy-hands-jonathans-token-back
/// Asked for a plan to Shoushi from the Academy's first room, the client plans a walk to
/// Jonathan (through the door between his room and the first), the token handed back to him, and
/// a walk on from where he sends the character;
/// without the server's table, which says where the Academy's exit leads, there is no way.
///
/// Falsified by a planner that offers no hand-back (the exit portal's part is not walked to from
/// the start, so the plan is "no_path"), or one that offers it with nowhere known to arrive.
#[test]
fn a_plan_out_of_a_recorded_academy_hands_jonathans_token_back() {
    let said = plan(Some(PUBLISHED));
    // The walk to him is split at the door between the first room and his.
    assert!(
        said.iter()
            .any(|l| l
                .starts_with("To Shoushi: walk, door, walk, hand back to Jonathan, walk (about ")),
        "{said:?}"
    );
    let without = plan(None);
    assert!(
        without.iter().any(|l| l == "No way to Shoushi: no_path."),
        "{without:?}"
    );
}
