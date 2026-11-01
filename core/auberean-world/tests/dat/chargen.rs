//! Character generation's appearance choices from the September 2013 install, held to a creation
//! the 2017 client sent.

use auberean_world::chargen::{
    choices, middle, Avatar, Random, BUILD, CLOTHING, EMPYREAN, FEMALE, HEIGHT, HUMAN, MALE,
    TUMEROK, TYPES,
};
use auberean_world::look::Look;

use crate::common::need_world;

/// A human man's choices: build in 31 steps of 0.01 from 0.85 (the middle stop, where a new avatar
/// starts, a hair under 1), height in 16 from 0.95 (middle 1.02), skin tone, hairstyle, beard, hair
/// color, the shirt's, pants' and boots' colors and the face, each with its table key's stops
/// (11 skin tones); every race and sex's build and height stop counts; an Empyrean woman's boots
/// have their 73 colors twice over. A new avatar's choices are all stops, and its look puts
/// together with the body and wears its three garments.
///
/// Behaviour: world.chargen.a-race-and-sexs-choices-and-their-stops
#[test]
fn a_race_and_sexs_choices_and_their_stops() {
    let w = need_world!();
    let man = choices(&w.records, HUMAN, MALE);
    let types: Vec<u32> = man.iter().map(|c| c.physique_type).collect();
    assert_eq!(
        types,
        vec![1, 2, 4, 0x10, 0x20, 0x40, 0x100, 0x200, 0x400, 0x800]
    );
    let counts: Vec<usize> = man.iter().map(|c| c.values.len()).collect();
    assert_eq!(counts, vec![31, 16, 11, 12, 8, 12, 26, 26, 23, 11]);
    let build = &man[0].values;
    assert_eq!(middle(build.len()), 15);
    assert_eq!(build[0].to_bits(), 0x3F59_999A, "0.85");
    assert_eq!(build[15].to_bits(), 0x3F7F_FFFE, "the middle build");
    assert_eq!(middle(16), 7);
    assert!((man[1].values[7] - 1.02).abs() < 1e-6, "the middle height");
    // Build and height in every race and sex.
    for (race, sex, n_build, n_height) in [
        (HUMAN, MALE, 31, 16),
        (HUMAN, FEMALE, 31, 16),
        (2, MALE, 21, 16),
        (2, FEMALE, 21, 16),
        (TUMEROK, MALE, 21, 16),
        (TUMEROK, FEMALE, 21, 16),
        (8, MALE, 11, 11),
        (8, FEMALE, 21, 21),
        (EMPYREAN, MALE, 26, 26),
        (EMPYREAN, FEMALE, 26, 26),
    ] {
        let c = choices(&w.records, race, sex);
        assert_eq!(
            (c[0].values.len(), c[1].values.len()),
            (n_build, n_height),
            "race {race} sex {sex:#x}"
        );
    }
    let boots = choices(&w.records, EMPYREAN, FEMALE)
        .into_iter()
        .find(|c| c.physique_type == 0x400)
        .expect("boots");
    assert_eq!(boots.values.len(), 146);
    assert_eq!(boots.values[..73], boots.values[73..]);

    let mut random = Random::default();
    let a = Avatar::new(&man, HUMAN, MALE, &mut random);
    assert_eq!(a.value(BUILD), Some(build[15]));
    assert_eq!(a.value(HEIGHT), Some(man[1].values[7]));
    for c in &man {
        let v = a.value(c.physique_type).expect("every type chosen");
        assert!(c.values.contains(&v), "{:#x} = {v}", c.physique_type);
    }
    let body = auberean_world::place::PlaceRecords::entity(
        &w.records,
        auberean_primitives::DataId(0x4700_00CD),
    )
    .and_then(|e| e.object)
    .expect("the human man's body");
    let look = Look::of(&w.records, &a.look(&man, auberean_primitives::DataId(body)))
        .expect("the look puts together");
    for c in man
        .iter()
        .filter(|c| c.physique_type & CLOTHING == c.physique_type)
    {
        let worn = look.global.appearance.get(c.table).expect("the garment");
        assert_eq!(worn.get(0x10), Some(&1.0), "{:#x} worn", c.physique_type);
    }
}

/// A Tumerok woman created on 14 January 2017: each of the thirteen physique values her client
/// sent is one of her race and sex's stops for that type, and the types she does not have (beard,
/// boots, eyes) were sent as 0.
///
/// Behaviour: world.chargen.a-2017-creations-values-are-stops
#[test]
fn a_2017_creations_values_are_stops() {
    let w = need_world!();
    let her = choices(&w.records, TUMEROK, FEMALE);
    let sent: [(u32, f32); 13] = [
        (1, 0.939_999_9),
        (2, 1.009_999_9),
        (4, 0.4),
        (0x1000, 0.0),
        (8, 0.6),
        (0x10, 0.6),
        (0x40, 1.0),
        (0x100, 0.32),
        (0x800, 0.6),
        (0x20, 0.0),
        (0x80, 0.05),
        (0x400, 0.0),
        (0x200, 0.1),
    ];
    assert_eq!(sent.len(), TYPES.len());
    for (t, v) in sent {
        match her.iter().find(|c| c.physique_type == t) {
            Some(c) => assert!(
                c.values.iter().any(|x| x.to_bits() == v.to_bits()),
                "{t:#x} = {v} is not a stop: {:?}",
                c.values
            ),
            None => assert_eq!(v, 0.0, "{t:#x} she lacks"),
        }
    }
}
