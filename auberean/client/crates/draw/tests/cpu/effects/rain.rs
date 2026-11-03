//! Rain: how many streaks fall, where they land, what they look like, what they throw off, and what
//! rain and water do to a body. Fixture: a flat made-up land (made-up heights), a seeded dice.

use auberean_draw::rain::{
    body_effects, count, Bone, Ground, Rain, RainWorld, Thrown, SEGMENTS, SHOWS_OVER,
};
use auberean_primitives::random::Random;
use auberean_primitives::Vec3;
use auberean_world::math::Mat4;

const CELL: f32 = 20.0;

fn world(now: f64) -> RainWorld {
    RainWorld {
        cell: (8 * 100 + 4, 8 * 100 + 4),
        viewer: Vec3::new(80.0, 80.0, 50.0),
        right: Vec3::new(1.0, 0.0, 0.0),
        push: Vec3::new(0.0, 0.0, -9.8),
        water: None,
        now,
        cells_per_block: 8,
        cell_length: CELL,
    }
}

fn flat(height: f32) -> impl FnMut((i32, i32), Vec3) -> Ground {
    move |_, _| Ground::At(Some(height))
}

/// Streaks fall in proportion to the rain, up to 2,500; they are drawn over 0.4 of it.
///
/// Behaviour: draw.rain.as-many-streaks-fall-as-the-rain-asks
#[test]
fn as_many_streaks_fall_as_the_rain_asks() {
    assert_eq!(count(0.0), 0);
    assert_eq!(count(0.5), 1250);
    assert_eq!(count(1.0), SEGMENTS);
    assert_eq!(count(2.0), SEGMENTS, "held at all of them");
    assert_eq!(count(-1.0), 0);
    assert_eq!(SHOWS_OVER, 0.4);
    let (mut rain, rng) = (Rain::default(), Random::new());
    rain.update(0.1, &world(1.0), &rng, &mut flat(30.0), &mut |_, _| {});
    let (v, i) = rain.drawn();
    assert_eq!(v.len(), 250 * 4);
    assert_eq!(i.len(), 250 * 6);
    assert_eq!(&i[6..12], &[4, 5, 6, 4, 7, 6], "two triangles a streak");
}

/// A streak's quad is a tenth of a meter wide across the camera, its top wholly transparent and its
/// foot pale blue-white at 0.45, 8 m long along its slant, ending on the way to its spot; every spot
/// is within 130 m each way of the viewer and on the ground.
///
/// Behaviour: draw.rain.a-streak-is-a-thin-quad-fading-toward-its-top
#[test]
fn a_streak_is_a_thin_quad_fading_toward_its_top() {
    let (mut rain, rng) = (Rain::default(), Random::new());
    rain.update(1.0, &world(1.0), &rng, &mut flat(30.0), &mut |_, _| {});
    let (v, _) = rain.drawn();
    let mut full = 0;
    for q in v.chunks(4) {
        assert_eq!(q[0].diffuse, 0x00E5_E5FF);
        assert_eq!(q[1].diffuse, 0x72E5_E5FF);
        assert_eq!(q[2].diffuse, 0x72E5_E5FF);
        assert_eq!(q[3].diffuse, 0x00E5_E5FF);
        assert_eq!(q[0].uv[0], [0.0, 0.0]);
        assert_eq!(q[2].uv[0], [1.0, 1.0]);
        let across = q[3].position[0] - q[0].position[0];
        assert!((across - 0.1).abs() < 1e-3, "{across}");
        // a first fall has no slant yet only when its life had not wrapped: straight or slanted, its
        // foot lies no lower than the ground plus nothing
        assert!(q[1].position[2] >= 29.9, "{:?}", q[1].position);
        assert!((q[0].position[0] - 80.0).abs() <= 131.0);
        assert!((q[0].position[1] - 80.0).abs() <= 131.0);
        full += usize::from(q[0].position[2] != q[1].position[2]);
    }
    // after a fall each has its slant, mostly down
    let mut now = 1.0;
    for _ in 0..30 {
        now += 0.05;
        rain.update(1.0, &world(now), &rng, &mut flat(30.0), &mut |_, _| {});
    }
    let (v, _) = rain.drawn();
    let long = v
        .chunks(4)
        .filter(|q| ((q[0].position[2] - q[1].position[2]) - 8.0).abs() < 0.3)
        .count();
    assert!(long > SEGMENTS / 2, "{long} long, {full} before");
}

/// Touching down a streak throws up vapor where it lands; on the water of the viewer's cell the
/// spot stands on the water and it makes a ripple too, within 65 m of the viewer.
///
/// Behaviour: draw.rain.a-streak-touching-down-throws-up-vapor-and-ripples
#[test]
fn a_streak_touching_down_throws_up_vapor_and_ripples() {
    let (mut rain, rng) = (Rain::default(), Random::new());
    let mut w = world(1.0);
    w.water = Some(40.0);
    let mut seen = Vec::new();
    for k in 0..40 {
        w.now = 1.0 + f64::from(k) * 0.02;
        rain.update(1.0, &w, &rng, &mut flat(30.0), &mut |t, at| {
            seen.push((t, at))
        });
    }
    let vapor = seen.iter().filter(|(t, _)| *t == Thrown::Vapor).count();
    let ripples: Vec<&Vec3> = seen
        .iter()
        .filter(|(t, _)| *t == Thrown::Ripple)
        .map(|(_, at)| at)
        .collect();
    assert!(vapor > 1000, "{vapor}");
    assert!(ripples.len() > 100 && ripples.len() < vapor);
    for at in &ripples {
        assert_eq!(at.z, 40.0, "on the water");
        let (dx, dy) = (at.x - 80.0, at.y - 80.0);
        assert!(dx * dx + dy * dy < 4225.0, "{at:?}");
    }
    assert!(seen
        .iter()
        .filter(|(t, _)| *t == Thrown::Vapor)
        .all(|(_, at)| at.z == 40.0));
}

/// Over ground not loaded a spot keeps the viewer's height and makes no ripple, whatever the water.
///
/// Behaviour: draw.rain.over-land-not-loaded-a-spot-keeps-the-viewers-height
#[test]
fn over_land_not_loaded_a_spot_keeps_the_viewers_height() {
    let (mut rain, rng) = (Rain::default(), Random::new());
    let mut w = world(1.0);
    w.water = Some(60.0);
    let mut ripples = 0;
    for k in 0..40 {
        w.now = 1.0 + f64::from(k) * 0.02;
        rain.update(1.0, &w, &rng, &mut |_, _| Ground::Unknown, &mut |t, at| {
            if t == Thrown::Vapor {
                assert_eq!(at.z, 50.0);
            }
            ripples += usize::from(t == Thrown::Ripple);
        });
    }
    assert_eq!(ripples, 0);
}

/// A streak left more than five cells from the viewer picks a new spot near where the viewer now
/// stands, in the viewer's block's frame.
///
/// Behaviour: draw.rain.a-streak-left-behind-picks-a-new-spot
#[test]
fn a_streak_left_behind_picks_a_new_spot() {
    let (mut rain, rng) = (Rain::default(), Random::new());
    let w = world(1.0);
    rain.update(1.0, &w, &rng, &mut flat(30.0), &mut |_, _| {});
    // ten blocks east: every streak is far behind
    let mut far = world(1.05);
    far.cell.0 += 80;
    rain.update(1.0, &far, &rng, &mut flat(30.0), &mut |_, _| {});
    let (v, _) = rain.drawn();
    assert!(v
        .iter()
        .all(|c| (c.position[0] - 80.0).abs() <= 131.0 && (c.position[1] - 80.0).abs() <= 131.0));
}

fn bones() -> Vec<Bone> {
    let spine = |z0: f32, z1: f32| Bone {
        parent: Vec3::new(0.0, 0.0, z0),
        at: Vec3::new(0.0, 0.0, z1),
    };
    let arm = |z: f32| Bone {
        parent: Vec3::new(0.0, 0.0, z),
        at: Vec3::new(0.5, 0.0, z),
    };
    vec![
        spine(0.0, 0.5),
        spine(0.5, 1.0),
        spine(1.0, 1.5),
        arm(1.5),
        arm(1.0),
    ]
}

/// Rain strikes bones lying across it, throwing up vapor on them; a bone along the rain takes none.
/// A body out of the water takes no bubbles or ripples.
///
/// Behaviour: draw.rain.rain-strikes-a-bodys-bones-across-it
#[test]
fn rain_strikes_a_bodys_bones_across_it() {
    let rng = Random::new();
    let down = Vec3::new(0.0, 0.0, -1.0);
    let mut thrown = Vec::new();
    for _ in 0..200 {
        body_effects(
            &bones(),
            &Mat4::IDENTITY,
            Some(down),
            None,
            &rng,
            &mut |t, at| {
                thrown.push((t, at));
            },
        );
    }
    assert!(thrown.len() > 100);
    for (t, at) in &thrown {
        assert_eq!(*t, Thrown::Vapor);
        assert!(at.x > 0.0, "only the arms lie across the rain: {at:?}");
    }
    let mut none = 0;
    body_effects(&bones(), &Mat4::IDENTITY, None, None, &rng, &mut |_, _| {
        none += 1
    });
    assert_eq!(none, 0, "no rain, no water");
}

/// In water, a body's bones deep under give off bubbles and those crossing the surface ripples on
/// it; the vapor stays above the water.
///
/// Behaviour: draw.rain.water-gives-a-body-bubbles-and-ripples
#[test]
fn water_gives_a_body_bubbles_and_ripples() {
    let rng = Random::new();
    let down = Vec3::new(0.0, 0.0, -1.0);
    let (mut bubbles, mut ripples) = (0, 0);
    for _ in 0..200 {
        body_effects(
            &bones(),
            &Mat4::IDENTITY,
            Some(down),
            Some(0.75),
            &rng,
            &mut |t, at| match t {
                Thrown::Bubbles => {
                    bubbles += 1;
                    assert!(at.z < 0.55, "{at:?}");
                }
                Thrown::Ripple => {
                    ripples += 1;
                    assert_eq!(at.z, 0.75);
                    assert_eq!((at.x, at.y), (0.0, 0.0), "the bone that crosses it");
                }
                Thrown::Vapor => assert!(at.z > 0.75, "{at:?}"),
            },
        );
    }
    assert!(bubbles > 50, "{bubbles}");
    assert!(ripples > 10 && ripples < 80, "one time in five: {ripples}");
}

/// A streak starts 80 m over its landing and falls at 150 m a second: no quad stands higher, some
/// nearly that high, and between two moments a streak falls by that speed.
///
/// Behaviour: draw.rain.a-streak-falls-from-80-m-at-150-m-a-second
#[test]
fn a_streak_falls_from_80_m_at_150_m_a_second() {
    let (mut rain, rng) = (Rain::default(), Random::new());
    let mut now = 1.0;
    rain.update(1.0, &world(now), &rng, &mut flat(30.0), &mut |_, _| {});
    for _ in 0..40 {
        now += 0.02;
        rain.update(1.0, &world(now), &rng, &mut flat(30.0), &mut |_, _| {});
    }
    let before = rain.drawn().0;
    now += 0.02;
    rain.update(1.0, &world(now), &rng, &mut flat(30.0), &mut |_, _| {});
    let after = rain.drawn().0;
    let top = before
        .iter()
        .map(|c| c.position[2])
        .fold(f32::MIN, f32::max);
    assert!(top <= 110.5 && top > 100.0, "{top}");
    let mut fell = 0;
    for (a, b) in before.chunks(4).zip(after.chunks(4)) {
        let d = a[0].position[2] - b[0].position[2];
        if d > 0.0 {
            // a fall of 3 m along a slant all but straight down
            assert!((2.8..=3.1).contains(&d), "{d}");
            fell += 1;
        }
    }
    assert!(fell > SEGMENTS * 9 / 10, "{fell}");
}
