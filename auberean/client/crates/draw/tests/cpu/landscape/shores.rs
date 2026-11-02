//! The shores: where the water meets made-up ground, the strips laid from the waterline out over
//! the water.

use auberean_draw::shore::{generate, Shore, ShoreBlock, FOAM_LEAST_LENGTH};
use auberean_primitives::random::Random;

/// Ground of 5 m squares, 16 a side, its height `z(x, y)`, each square's two triangles as the
/// ground draws an uncut square: south-west, south-east, north-west; south-east, north-east,
/// north-west.
fn ground(z: impl Fn(f32, f32) -> f32) -> Vec<[[f32; 3]; 3]> {
    let mut out = Vec::new();
    for i in 0..16u8 {
        for j in 0..16u8 {
            let (x, y) = (f32::from(i) * 5.0, f32::from(j) * 5.0);
            let c = |dx: f32, dy: f32| [x + dx, y + dy, z(x + dx, y + dy)];
            let (sw, se, nw, ne) = (c(0.0, 0.0), c(5.0, 0.0), c(0.0, 5.0), c(5.0, 5.0));
            out.push([sw, se, nw]);
            out.push([se, ne, nw]);
        }
    }
    out
}

/// A bowl about (40, 40), its sides rising a meter by 10 m out and four by 20 m (its corners never
/// within a centimeter of the waters below, where an edge's end would count as on the water).
fn bowl(x: f32, y: f32) -> f32 {
    ((x - 40.0).powi(2) + (y - 40.0).powi(2)) / 100.0
}

fn pond(water: f32) -> Shore {
    let block = ShoreBlock {
        water: Some(water),
        own: true,
        triangles: ground(bowl),
    };
    generate(&[block], &Random::new()).expect("water")
}

fn radius(p: [f32; 3]) -> f32 {
    dereth_primitives::num::math::hypotf(p[0] - 40.0, p[1] - 40.0)
}

/// A pond's shore runs round it as one closed strip: its points where the ground's edges cross
/// the water (each pulled 5 cm up the bank), its far edge out over the water toward the pond's
/// middle, twelve times the ground's upward slant there (about 11 m on a bank rising 0.4 m a
/// meter); the strip ends where it began, measured along its length, one offset across it.
///
/// Behaviour: draw.shore.a-ponds-shore-runs-round-it-out-over-the-water
#[test]
fn a_ponds_shore_runs_round_it_out_over_the_water() {
    let shore = pond(4.1);
    assert_eq!(shore.strips.len(), 1, "one strip");
    let s = &shore.strips[0];
    assert!(s.closed, "it closes on itself");
    let n = s.points.len();
    assert!(n > 30, "{n} points");
    let (first, last) = (shore.points[s.points[0]], shore.points[s.points[n - 1]]);
    for k in 0..3 {
        assert!(
            (first.at[k] - last.at[k]).abs() < 1e-5,
            "it ends where it began"
        );
    }
    // the water 4.1 m up the bowl's side: 20.25 m from its middle
    let rim = 410.0f32.sqrt();
    let round = 2.0 * std::f32::consts::PI * rim;
    assert!(
        s.length > round - 3.0 && s.length < round,
        "{} round",
        s.length
    );
    assert!(s.foamed());
    let mut along = -1.0;
    for &k in &s.points {
        let p = shore.points[k];
        assert!((p.at[2] - 4.1).abs() < 1e-4 && (p.out[2] - 4.1).abs() < 1e-4);
        let r = radius(p.at);
        assert!((r - rim - 0.05).abs() < 0.35, "at the waterline: {r}");
        let reach = r - radius(p.out);
        assert!((10.6..11.6).contains(&reach), "reach {reach}");
        assert!(p.along >= along, "measured along");
        along = p.along;
        assert!((p.across - first.across).abs() < 1e-9 && (0.0..0.8).contains(&p.across));
    }
    assert!(
        (along - s.length).abs() < 1e-4,
        "its last point at its length"
    );
    let v = &shore.vertices;
    assert_eq!(v.len(), 2 * shore.points.len());
    let k = s.points[1];
    assert_eq!(v[2 * k].position, shore.points[k].at);
    assert_eq!(v[2 * k + 1].position, shore.points[k].out);
    let a = shore.points[k];
    assert_eq!(v[2 * k].uv[0], [a.along, a.across]);
    assert_eq!(v[2 * k + 1].uv[0], [a.along, a.across + 1.0]);
    assert_eq!(
        (v[2 * k].uv[1], v[2 * k + 1].uv[1]),
        ([a.along, 0.0], [a.along, 1.0])
    );
    assert_eq!(shore.triangles(false).len(), 3 * (2 * n - 2));
}

/// A closed shore shorter than 20 m (a puddle) carries no foam, though its strip is still drawn
/// into the frame's alpha.
///
/// Behaviour: draw.shore.a-closed-shore-under-20-m-carries-no-foam
#[test]
fn a_closed_shore_under_20_m_carries_no_foam() {
    let shore = pond(0.09);
    assert_eq!(shore.strips.len(), 1);
    let s = &shore.strips[0];
    assert!(s.closed && s.length < FOAM_LEAST_LENGTH, "{}", s.length);
    assert!(!s.foamed());
    assert!(shore.triangles(true).is_empty(), "no foam");
    assert!(!shore.triangles(false).is_empty(), "its alpha");
}

/// A beach running straight across the block makes one open strip along it, which carries foam;
/// the strip's far edge lies out over the water downhill. Its length is measured from each point
/// to the one before as already pulled up the bank.
///
/// Behaviour: draw.shore.a-beach-makes-one-open-strip-along-it
#[test]
fn a_beach_makes_one_open_strip_along_it() {
    let block = ShoreBlock {
        water: Some(0.0),
        own: true,
        triangles: ground(|x, _| (x - 42.0) / 10.0),
    };
    let shore = generate(&[block], &Random::new()).expect("water");
    assert_eq!(shore.strips.len(), 1);
    let s = &shore.strips[0];
    assert!(!s.closed && s.foamed());
    assert_eq!(s.points.len(), 33, "two a square and one more");
    // each step measured from the point before as already pulled up the bank: a little over the
    // 80 m the waterline runs
    let pulled = 0.05f32 * 0.05;
    let steps = 16.0 * ((9.0 + pulled).sqrt() + (4.0 + pulled).sqrt());
    assert!(
        (s.length - steps).abs() < 1e-3,
        "{} against {steps}",
        s.length
    );
    let reach = 12.0 / 1.01f32.sqrt();
    for &k in &s.points {
        let p = shore.points[k];
        assert!((p.at[0] - 42.05).abs() < 1e-4, "{:?}", p.at);
        assert!((p.out[0] - (42.05 - reach)).abs() < 1e-3, "{:?}", p.out);
    }
    assert_eq!(shore.triangles(true).len(), 3 * 64);
}

/// Only the viewer's own block's water and water standing no more than 0.1 m below it make
/// shores: a block whose water stands lower is passed over.
///
/// Behaviour: draw.shore.a-block-whose-water-stands-lower-than-the-viewers-has-no-shore
#[test]
fn a_block_whose_water_stands_lower_than_the_viewers_has_no_shore() {
    let own = ShoreBlock {
        water: Some(1.0),
        own: true,
        triangles: ground(|_, _| 5.0),
    };
    let beach = |water: f32| ShoreBlock {
        water: Some(water),
        own: false,
        triangles: ground(|x, _| (x - 42.0) / 10.0 + water),
    };
    let strips = |water: f32| {
        generate(&[own.clone(), beach(water)], &Random::new())
            .expect("water")
            .strips
            .len()
    };
    assert_eq!(strips(0.85), 0, "0.15 m lower: passed over");
    assert_eq!(strips(0.95), 1, "0.05 m lower: its shore");
    assert_eq!(strips(1.5), 1, "higher: its shore");
    let dry = ShoreBlock {
        water: None,
        ..beach(0.0)
    };
    assert!(
        generate(&[dry], &Random::new()).is_none(),
        "no water at all"
    );
}
