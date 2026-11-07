//! How a new character looks.

use auberean_primitives::Vec3;
use auberean_protocol::table::{Table, SMALL};
use yalain_world::chargen::{CharGenRules, FEMALE};

/// A Tumerok woman made on 14 January 2017 (her choices as her client sent them): the body's
/// look as the parent, a scale of build times height, and per appearance the modifiers her
/// physique chose, each choice among parts with its "shown" modifier, laid out as the server
/// sent them.
/// Behaviour: server.characters.look-from-the-physique
#[test]
fn a_new_character_looks_as_the_2017_server_made_her() {
    let mut bodies = std::collections::BTreeMap::new();
    bodies.insert((4, FEMALE), 0x1F00_007A);
    let rules = CharGenRules::new(bodies, yalain_testkit::test_physique());
    let build = 0.939_999_9_f32;
    let height = 1.009_999_9_f32;
    let physique = Table::from_inserted(
        SMALL,
        [
            (1, build),
            (2, height),
            (4, 0.4),
            (4096, 0.0),
            (8, 0.6),
            (16, 0.6),
            (64, 1.0),
            (256, 0.32),
            (2048, 0.6),
            (32, 0.0),
            (128, 0.05),
            (1024, 0.0),
            (512, 0.1),
        ],
    );
    let look = rules.look(4, FEMALE, &physique).expect("a body");
    assert_eq!((look.bits, look.parent), (0x122, 0x1F00_007A));
    assert_eq!(
        look.scale,
        Vec3::new(build * height, build * height, height)
    );
    assert_eq!((look.global.bits, look.global.key), (0x21, 0x4000_0002));
    let got: Vec<(u32, Vec<(u32, f32)>)> = look
        .global
        .appearance
        .entries
        .iter()
        .map(|(k, t)| (*k, t.entries.clone()))
        .collect();
    assert_eq!(
        got,
        vec![
            (0x2000_0038, vec![(2, 0.32), (16, 1.0)]),
            (0x2000_0009, vec![(1, 0.4)]),
            (0x2000_0039, vec![(2, 0.1), (16, 1.0)]),
            (0x2000_000A, vec![(3, 0.6), (11, 1.0), (0x4100_0003, 0.05)]),
            (0x2000_000B, vec![(15, 0.6), (0x4100_001C, 0.6)]),
        ]
    );
    assert!(
        rules.look(4, 0x4000, &physique).is_none(),
        "no body, no character"
    );
}
