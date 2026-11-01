//! How far the landscape reaches and where frills are drawn, at the camp of block `0xC156`.

use auberean_draw::scene::{SceneBuilder, Spot};
use auberean_primitives::Vec3;

use crate::common::install;

fn spot() -> Spot {
    Spot {
        block: (0xC1, 0x56),
        position: Vec3::new(124.842, 89.758, 79.968),
        heading: 149.65,
        pitch: 0.0,
        time_of_day: 0.5,
        day: None,
    }
}

/// The far landscape reaches as many blocks about the viewer's as its radius (none, 2, 8: the 3 x 3
/// detailed blocks alone, 5 x 5, 17 x 17 where the region has land); the detailed blocks are drawn
/// at every radius. Frills are drawn only in the land cells within their radius of the viewer's
/// (more with two cells than one), none when turned off, and fade out over the last eight meters
/// of their reach.
///
/// Behaviour: draw.world.the-far-landscape-and-the-frills-reach-as-far-as-set
#[test]
fn the_far_landscape_and_the_frills_reach_as_far_as_set() {
    let i = install();
    let region = i.records.region().expect("the region");
    let mut b = SceneBuilder::new(&i.records, &region);
    b.static_shadows = false;
    b.water_reflections = false;
    let land = |r: i32| {
        let mut n = 0;
        for x in 0xC1 - r..=0xC1 + r {
            for y in 0x56 - r..=0x56 + r {
                n += usize::from(i.records.landblock(x, y).is_some());
            }
        }
        n
    };
    let mut blocks = Vec::new();
    for r in [0, 2, 8] {
        b.set_far_radius(r);
        let f = b.frame(&spot(), 60.0);
        blocks.push((f.counts.blocks, land(r.max(1))));
    }
    for (drawn, held) in &blocks {
        assert_eq!(drawn, held, "every block of land in reach: {blocks:?}");
    }
    assert!(blocks[0].0 <= 9 && blocks[1].0 > blocks[0].0 && blocks[2].0 > blocks[1].0);

    b.set_far_radius(0);
    let frills = |b: &mut SceneBuilder<'_>, on: bool, radius: i32, reach: f32| {
        b.set_frills(on, radius, reach);
        let f = b.frame(&spot(), 60.0);
        (f.counts.frills, f.frill_fade)
    };
    let (one, fade_one) = frills(&mut b, true, 1, 1.0);
    let (two, fade_two) = frills(&mut b, true, 2, 1.0);
    let (low, fade_low) = frills(&mut b, true, 1, 0.65);
    let (none, _) = frills(&mut b, false, 1, 1.0);
    assert!(
        one > 0 && two > one,
        "frills: {one} in one cell about, {two} in two"
    );
    assert_eq!(low, one, "the lowest setting draws the same cells");
    assert_eq!(none, 0);
    assert_eq!(fade_one, [12.0, 20.0]);
    assert_eq!(fade_two, [32.0, 40.0]);
    assert!((fade_low[1] - 13.0).abs() < 1e-4 && (fade_low[0] - 5.0).abs() < 1e-4);
}

/// Every frill (the camp's ferns and grass) is drawn with its spot on the ground, where its color
/// is taken from the sun's shadow map about it: each frill drawn names one spot, at its own
/// origin and within the shadow map's reach, and nothing else names one.
///
/// Behaviour: draw.world.a-frill-takes-its-color-from-the-shadow-about-it
#[test]
fn every_frill_is_drawn_with_its_spot_for_the_shadow_about_it() {
    let i = install();
    let region = i.records.region().expect("the region");
    let mut b = SceneBuilder::new(&i.records, &region);
    b.static_shadows = true;
    b.water_reflections = false;
    b.set_far_radius(0);
    b.set_frills(true, 1, 1.0);
    let f = b.frame(&spot(), 60.0);
    let map = f.shadow.as_ref().expect("the sun's shadow map");
    let mut spots: Vec<[u32; 3]> = Vec::new();
    for o in &f.objects {
        let Some(at) = o.shade else { continue };
        let origin = o.transform.point(Vec3::ZERO);
        let gap = auberean_world::math::length(auberean_world::math::sub(origin, at));
        assert!(
            gap < 1e-3,
            "the frill's own origin: {at:?} against {origin:?}"
        );
        let [u, v] = map.uv(at);
        assert!(
            (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v),
            "within the map: {u} {v}"
        );
        let key = [at.x.to_bits(), at.y.to_bits(), at.z.to_bits()];
        if !spots.contains(&key) {
            spots.push(key);
        }
    }
    assert!(f.counts.frills > 0, "frills about the camp");
    assert_eq!(
        spots.len(),
        f.counts.frills,
        "one spot for each frill drawn"
    );
    b.set_frills(false, 1, 1.0);
    let none = b.frame(&spot(), 60.0);
    assert!(
        none.objects.iter().all(|o| o.shade.is_none()),
        "no spot without frills"
    );
}

/// The fog blocks blended for the light are those of the block the viewer stands in, even when it
/// stands past the edge of the block its place is given in: 145 m into block `0xC156` (fogged by its
/// south-west corner's fog kind) gives the same light whether given from that block or as 15 m
/// short of block `0xC256`; the sky's day at the moment, else the light with no sky, under them.
///
/// Behaviour: draw.world.the-light-is-that-of-the-block-the-viewer-stands-in
#[test]
fn the_light_is_that_of_the_block_the_viewer_stands_in() {
    let i = install();
    let region = i.records.region().expect("the region");
    let b = SceneBuilder::new(&i.records, &region);
    let at = |block: (i32, i32), x: f32| {
        let s = Spot {
            block,
            position: Vec3::new(x, 89.758, 79.968),
            ..spot()
        };
        let day = b.day(&s);
        b.light(&s, day.as_ref())
    };
    let inside = at((0xC1, 0x56), 145.0);
    let east = at((0xC2, 0x56), 5.0);
    assert_eq!(at((0xC2, 0x56), -15.0), inside);
    assert_ne!(
        inside.fog_max, east.fog_max,
        "the next block's fog is another"
    );
    let none = b.light(&spot(), None);
    assert_eq!(
        none.light_direction,
        Vec3::new(0.6, 0.0, 0.8),
        "no sky: a light that travels up"
    );
}

/// The sun throws a lens flare: on the camp's day (`0x1A000001`) the sky object whose flare has
/// points is the sun, its flare placed 1000 metres out along the way the sun stands: up in the east
/// in the morning, higher at noon. Seen from below the sun, the points lie on the line through the screen's
/// middle at minus their axis times the sun's place there, square on the display; the flare is
/// strongest with the sun in the middle (its square, never under a fifth of the fade) and draws
/// nothing faded out.
///
/// Behaviour: draw.world.the-sun-throws-a-lens-flare
#[test]
fn the_sun_throws_a_lens_flare() {
    use auberean_draw::flare::{flare_quads, sky_flares, FlareFade, FLARE_DISTANCE};
    let i = install();
    let day = i
        .records
        .day(auberean_primitives::DataId(0x1A00_0001))
        .expect("the day");
    let morning = sky_flares(&day, 0.45);
    let noon = sky_flares(&day, 0.6);
    assert_eq!(morning.len(), 1, "one object throws a flare");
    let sun = &morning[0];
    assert!(
        sun.at.z > 0.0 && sun.at.x > 0.0,
        "up in the east: {:?}",
        sun.at
    );
    let len = auberean_world::math::length(sun.at);
    assert!((len - FLARE_DISTANCE).abs() < 0.1, "1000 metres out: {len}");
    assert!(sun.points.iter().all(|p| p.size > 0.0 && p.texture != 0));
    assert!(
        noon[0].at.z > sun.at.z,
        "higher at noon: {:?} {:?}",
        noon[0].at,
        sun.at
    );
    let middle = flare_quads(&sun.points, (0.0, 0.0), 1.5, 1.0);
    let corner = flare_quads(&sun.points, (0.5, 0.5), 1.5, 1.0);
    assert_eq!(middle.len(), sun.points.len());
    assert!((middle[0].alpha - 1.0).abs() < 1e-6, "full in the middle");
    let near: f32 = 1.0 - (0.5_f32 * 0.5 * 2.0).sqrt() * 0.704_225_36;
    assert!(
        (corner[0].alpha - near * near).abs() < 1e-6,
        "{}",
        corner[0].alpha
    );
    for (p, q) in sun.points.iter().zip(&corner) {
        let c = ((q.rect[0] + q.rect[2]) * 0.5, (q.rect[1] + q.rect[3]) * 0.5);
        assert!((c.0 + 0.5 * p.axis).abs() < 1e-6 && (c.1 + 0.5 * p.axis).abs() < 1e-6);
        assert!((q.rect[2] - q.rect[0] - p.size).abs() < 1e-6);
        assert!(
            (q.rect[3] - q.rect[1] - 1.5 * p.size).abs() < 1e-5,
            "square on the display"
        );
    }
    assert!(flare_quads(&sun.points, (0.0, 0.0), 1.5, 0.0).is_empty());
    let far = flare_quads(&sun.points, (1.0, 1.0), 1.5, 0.5);
    assert!(
        (far[0].alpha - 0.1).abs() < 1e-6,
        "never under a fifth: {}",
        far[0].alpha
    );
    let mut fade = FlareFade::default();
    assert_eq!(
        fade.advance(10.0, sun.fade_speed, true),
        0.0,
        "nothing at once"
    );
    let up = fade.advance(10.1, sun.fade_speed, true);
    assert!((up - (0.1 * sun.fade_speed).min(1.0)).abs() < 1e-5, "{up}");
    let down = fade.advance(10.15, sun.fade_speed, false);
    assert!(down < up, "fades out unseen");
}
