//! A headless run names itself, with Dereth's version, when the game asks for the client's version.
//! Fixture: the shipped retail DAT records.
//!
//! Behaviour: none (this client's own name for itself in a headless run)

use dereth_headless::run::{Options, BUILD_ID};
use dereth_headless::{parse, run};

/// `@version` in a headless run answers with the headless client and the version it carries,
/// Dereth's, not the version of the libraries under it.
#[test]
fn a_headless_run_answers_version_with_its_own_name_and_dereth_s_version() {
    dereth_client_sdk::dat::testing::require_dats();
    let options = Options {
        dat_dir: dereth_client_sdk::dat::testing::dat_dir(),
        ..Options::default()
    };
    let commands = parse("say @version\nquit\n").expect("the script parses");
    let mut out: Vec<u8> = Vec::new();
    run(&commands, &options, &mut out).expect("the run finishes");
    let text = String::from_utf8(out).expect("the run prints text");
    assert_eq!(
        BUILD_ID,
        concat!("dereth-headless ", env!("CARGO_PKG_VERSION"))
    );
    assert_ne!(
        env!("CARGO_PKG_VERSION"),
        "0.0.0",
        "it carries Dereth's version"
    );
    let answer = format!("chat Client version {BUILD_ID}");
    assert!(text.lines().any(|l| l == answer), "{text}");
}
