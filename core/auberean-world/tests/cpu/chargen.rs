//! Character generation's appearance choices on synthetic lists: the slider's stops, the middle,
//! the random pick by page, what is sent, and the look the choices give.

use auberean_primitives::{DataId, Vec3};
use auberean_world::chargen::{
    by_steps, middle, stop_at, stop_of, Avatar, Choice, Random, BODY_PAGE, BUILD, HEIGHT, MALE,
    SHIRT, SKIN_TONE, TYPES, WORN,
};
use auberean_world::look::GLOBAL_KEY;

fn choice(t: u32, values: &[f32], key: u32, table: u32) -> Choice {
    Choice {
        physique_type: t,
        label: "x",
        key,
        table,
        values: values.to_vec(),
    }
}

/// A slider of n stops stands at stop k at k/(n-1); a position maps back to the stop at
/// (n-1)·position truncated after a small nudge, so a stop's own position gives that stop; build
/// and height stop every 0.01 from their low to their high bound; a new avatar's height and build
/// stand at the middle stop (the upper of two middles rounds down).
///
/// Behaviour: world.chargen.a-slider-stops-at-its-values
#[test]
fn a_slider_stops_at_its_values() {
    for n in [2usize, 5, 16, 31, 73, 146] {
        for k in 0..n {
            #[allow(clippy::cast_precision_loss)]
            let p = k as f32 / (n - 1) as f32;
            assert_eq!(stop_at(p, n), k, "n {n} stop {k}");
        }
    }
    assert_eq!(stop_at(0.99, 2), 0);
    assert_eq!(stop_at(1.5, 4), 3);
    let steps = by_steps(0.95, 1.1);
    assert_eq!(steps.len(), 16);
    assert_eq!(stop_of(&steps, steps[7]), Some(7));
    assert_eq!(stop_of(&steps, 2.0), None);
    assert_eq!(
        (middle(16), middle(31), middle(11), middle(1)),
        (7, 15, 5, 0)
    );
}

/// Randomizing a page picks each of its types among their stops and leaves the other pages'
/// types alone; a type the avatar does not have is set to 0; what is sent is all thirteen types
/// in order. A new avatar has every type picked and then its height and build at their middles.
///
/// Behaviour: world.chargen.randomize-picks-a-pages-types-among-their-stops
#[test]
fn randomize_picks_a_pages_types_among_their_stops() {
    let choices = vec![
        choice(BUILD, &by_steps(0.85, 1.15), 0, 0),
        choice(HEIGHT, &by_steps(0.95, 1.1), 0, 0),
        choice(SKIN_TONE, &[0.0, 0.05, 0.1, 0.15], 1, 0x2000_000C),
        choice(SHIRT, &[0.0, 0.04, 0.08], 2, 0x2000_0016),
    ];
    let mut random = Random::default();
    let mut a = Avatar::new(&choices, 1, MALE, &mut random);
    assert_eq!(a.value(BUILD), Some(choices[0].values[15]));
    assert_eq!(a.value(HEIGHT), Some(choices[1].values[7]));
    let shirt = a.value(SHIRT);
    for _ in 0..50 {
        a.randomize(&choices, BODY_PAGE, &mut random);
        for c in &choices {
            let v = a.value(c.physique_type).expect("chosen");
            assert!(c.values.contains(&v));
        }
        assert_eq!(a.value(SHIRT), shirt, "the clothing page is left alone");
        assert_eq!(a.value(0x8), Some(0.0), "a type it lacks is 0");
    }
    let sent: Vec<u32> = a.physique().iter().map(|(t, _)| *t).collect();
    assert_eq!(sent, TYPES.to_vec());
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..200 {
        seen.insert(random.roll_int(0, 3));
    }
    assert_eq!(seen.into_iter().collect::<Vec<_>>(), vec![0, 1, 2, 3]);
}

/// The look of the choices: the body's description as the parent, a scale of build × height
/// across and height up, and in the whole avatar's changes each type's value in its table, a
/// garment worn as well.
///
/// Behaviour: world.chargen.the-choices-look-is-the-body-scaled-with-its-values
#[test]
fn the_choices_look_is_the_body_scaled_with_its_values() {
    let choices = vec![
        choice(BUILD, &[0.9], 0, 0),
        choice(HEIGHT, &[1.1], 0, 0),
        choice(SKIN_TONE, &[0.25], 1, 0x2000_000C),
        choice(SHIRT, &[0.08], 2, 0x2000_0016),
    ];
    let mut random = Random::default();
    let a = Avatar::new(&choices, 1, MALE, &mut random);
    let d = a.look(&choices, DataId(0x1F00_0023));
    assert_eq!(d.parent, DataId(0x1F00_0023));
    assert_eq!(d.scale, Vec3::new(0.9 * 1.1, 0.9 * 1.1, 1.1));
    assert_eq!(d.global.key, GLOBAL_KEY);
    let tables: Vec<(u32, Vec<(u32, f32)>)> = d
        .global
        .appearance
        .entries
        .iter()
        .map(|(t, k)| (*t, k.entries.clone()))
        .collect();
    assert_eq!(
        tables,
        vec![
            (0x2000_000C, vec![(1, 0.25)]),
            (0x2000_0016, vec![(2, 0.08), (WORN, 1.0)]),
        ]
    );
}

/// A new character's look is picked from the client's one generator after whatever the rest of
/// the program drew from it: with nothing drawn before, every run picks the same look; a draw by
/// anything else that holds the generator moves the look picked next, as that many draws would.
///
/// Behaviour: world.chargen.a-new-look-is-picked-after-whatever-drew-before
#[test]
fn a_new_look_is_picked_after_whatever_drew_before() {
    let choices = vec![
        choice(BUILD, &by_steps(0.85, 1.15), 0, 0),
        choice(HEIGHT, &by_steps(0.95, 1.1), 0, 0),
        choice(SKIN_TONE, &by_steps(0.0, 0.99), 1, 0x2000_000C),
        choice(SHIRT, &by_steps(0.0, 0.99), 2, 0x2000_0016),
    ];
    let look = |drawn_before: u32| {
        let random = Random::new();
        // Something else in the program holds the same generator and draws first.
        let elsewhere = random.clone();
        for _ in 0..drawn_before {
            elsewhere.draw();
        }
        let mut mine = random;
        Avatar::new(&choices, 1, MALE, &mut mine).physique()
    };
    assert_eq!(
        look(0),
        look(0),
        "nothing drawn before: the same look every run"
    );
    let after_draws = |n: u32| {
        let mut random = Random::new();
        for _ in 0..n {
            random.draw();
        }
        Avatar::new(&choices, 1, MALE, &mut random).physique()
    };
    assert_eq!(look(37), after_draws(37));
    let looks: std::collections::BTreeSet<Vec<u32>> = (0..20)
        .map(|k| {
            look(k * 13)
                .iter()
                .map(|(_, v)| v.to_bits())
                .collect::<Vec<u32>>()
        })
        .collect();
    assert!(looks.len() > 15, "{} looks of 20", looks.len());
}
