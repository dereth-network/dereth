//! The Navigator sample's Go to someone a body cannot stand beside: Shopkeeper Renald the Elder in
//! Holtburg keeps a booth whose only openings are a window onto the street and counters into his
//! shop. Fixture: the shipped retail DAT records, first-login-walk-jump's recorded login into the
//! world (fed on until the server has shown the shopkeeper), and the Navigator package as it ships.

use dereth_headless::run::Options;
use dereth_headless::{parse, run};

/// Behaviour: plugins.nav.a-journey-to-someone-behind-a-counter-ends-within-reach-of-them
/// `/nav obj Renald the Elder` then `/nav go` from the street: the journey walks to the street
/// outside his window, within reach of him, and the Navigator says it arrived.
///
/// Falsified by a client that plans only to the shopkeeper's own place, which no walk reaches: the
/// Navigator says it stopped, and the character never moves.
#[test]
fn the_navigators_go_to_a_shopkeeper_in_a_booth_walks_to_his_window_and_arrives() {
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
         replay 58\n\
         tick 30\n\
         position\n\
         say /nav obj Renald the Elder\n\
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
            "Pointing at Shopkeeper Renald the Elder (following it).",
            "Going by Shopkeeper Renald the Elder: walk (about 6 s).",
            "Arrived at Shopkeeper Renald the Elder.",
        ],
        "{text}"
    );
    let places: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with("position "))
        .collect();
    assert_eq!(places.len(), 2, "{text}");
    assert!(
        places[0].starts_with("position cell=0xA9B4002A "),
        "the walk starts in the street: {places:?}"
    );
    // He stands at (139.23, 18.33) inside the booth; the window's outer face is at y = 19.5.
    let end = places[1];
    let origin: Vec<f32> = end
        .split_whitespace()
        .find_map(|w| w.strip_prefix("origin="))
        .expect("an origin")
        .split(',')
        .map(|v| v.parse().expect("a number"))
        .collect();
    let cell = end
        .split_whitespace()
        .find_map(|w| w.strip_prefix("cell=0x"))
        .and_then(|c| u32::from_str_radix(c, 16).ok())
        .expect("a cell");
    let across = ((origin[0] - 139.234).powi(2) + (origin[1] - 18.329).powi(2)).sqrt();
    assert!(
        cell >> 16 == 0xA9B4 && cell & 0xFFFF < 0x0100 && origin[1] > 19.5,
        "the character ends in the street at his window: {end}"
    );
    assert!(across < 2.5, "within reach of him: {across} m, {end}");
}
