//! The landscape's shadows: the sun's shadow map of the near objects and the clouds' shadows.

use auberean_draw::shadow::{light_frame, CloudShadows, ShadowMap};
use auberean_primitives::Vec3;
use auberean_world::color::Color;
use auberean_world::math::{cross, dot};

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

/// The sun's frame looks along its light with the world's up kept as near up as it can be, its
/// right the light crossed with that up; light falling straight down takes north for up.
///
/// Behaviour: draw.shadow.the-sun-looks-along-its-light-with-up-kept-up
#[test]
fn the_sun_looks_along_its_light_with_up_kept_up() {
    let [right, along, up] = light_frame(Vec3::new(-2.0, 0.0, -2.0)).expect("a direction");
    let s = 1.0 / 2.0f32.sqrt();
    assert!(
        near(along.x, -s) && near(along.z, -s),
        "along is the light, unit length"
    );
    assert!(near(dot(up, along), 0.0) && near(dot(up, up), 1.0));
    assert!(up.z > 0.0 && near(up.x, -s), "up leans away from the light");
    let r = cross(along, up);
    assert!(near(right.x, r.x) && near(right.y, r.y) && near(right.z, r.z));
    let [_, _, up] = light_frame(Vec3::new(0.0, 0.0, -1.0)).expect("a direction");
    assert!(near(up.y, 1.0), "straight down: north is up");
    assert!(light_frame(Vec3::new(0.0, 0.0, 0.0)).is_none());
}

/// The map covers the near terrain as the sun sees it: the terrain's corners land on the
/// picture's edges, its first row at the top of the sun's up, and the map's clip space spans the
/// same box; a shadow takes away 0.95 of the day's shadow color.
///
/// Behaviour: draw.shadow.the-map-spans-the-near-terrain-from-the-sun
#[test]
fn the_map_spans_the_near_terrain_from_the_sun() {
    let ground = [
        Vec3::new(0.0, 0.0, 10.0),
        Vec3::new(480.0, 0.0, 10.0),
        Vec3::new(0.0, 480.0, 30.0),
        Vec3::new(480.0, 480.0, 20.0),
    ];
    let shadow = Color::new(0.5, 0.6, 1.2, 1.0);
    let map =
        ShadowMap::new(Vec3::new(-1.0, -0.5, -1.0), ground, shadow, Vec::new()).expect("a map");
    for p in ground {
        let [u, v] = map.uv(p);
        assert!((-1e-4..=1.0001).contains(&u) && (-1e-4..=1.0001).contains(&v));
        let c = map.view_projection().point(p);
        assert!(c.x.abs() <= 1.0001 && c.y.abs() <= 1.0001 && (0.0..=1.0).contains(&c.z));
        assert!(
            near(u * 2.0 - 1.0, c.x) && near(1.0 - v * 2.0, c.y),
            "picture and clip agree"
        );
    }
    let us: Vec<f32> = ground.iter().map(|&p| map.uv(p)[0]).collect();
    assert!(us.iter().any(|&u| near(u, 0.0)) && us.iter().any(|&u| near(u, 1.0)));
    assert!(near(map.strength.r, 0.475) && near(map.strength.g, 0.57));
    assert!(near(map.strength.b, 1.0), "held to 1");
}

/// The clouds' picture lies flat at its scale and slides by its speed times the client's
/// running time, a repeat at a time.
///
/// Behaviour: draw.shadow.cloud-shadows-slide-with-the-running-time
#[test]
fn cloud_shadows_slide_with_the_running_time() {
    let c = CloudShadows {
        picture: 1,
        scale: 0.001,
        scroll: [-0.006, -0.002],
    };
    let [u, v] = c.uv(500.0, 250.0, 0.0);
    assert!(near(u, 0.5) && near(v, 0.25));
    let [u, v] = c.uv(500.0, 250.0, 100.0);
    assert!(
        near(u, 0.5 + 0.4) && near(v, 0.25 + 0.8),
        "slid 0.6 and 0.2 back"
    );
    let late = c.offset(1.0e6 + 100.0);
    assert!(
        near(late[0], 0.4) && near(late[1], 0.8),
        "a long-running clock keeps its fraction"
    );
}

/// Whether a sky object is seen is asked of the one pixel at its place: the marker covers that
/// pixel's middle and no neighbor's, wherever on the picture it falls.
///
/// Behaviour: draw.flare.a-flare-asks-of-one-pixel-whether-its-object-is-seen
#[test]
fn a_flare_asks_of_one_pixel_whether_its_object_is_seen() {
    use auberean_draw::flare::seen_marker;
    let inside = |t: &[(f32, f32); 3], p: (f32, f32)| {
        let side =
            |a: (f32, f32), b: (f32, f32)| (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0);
        let s = [side(t[0], t[1]), side(t[1], t[2]), side(t[2], t[0])];
        s.iter().all(|v| *v > 0.0) || s.iter().all(|v| *v < 0.0)
    };
    let (w, h) = (1280_u32, 720_u32);
    #[allow(clippy::cast_precision_loss)]
    let pixel = |i: i32, j: i32| {
        (
            (i as f32 + 0.5) * 2.0 / w as f32 - 1.0,
            (j as f32 + 0.5) * 2.0 / h as f32 - 1.0,
        )
    };
    for (i, j) in [(640, 360), (3, 700), (1000, 12)] {
        let (x, y) = pixel(i, j);
        // Anywhere within the pixel.
        let t = seen_marker((x + 0.0003, y - 0.0005), w, h);
        let mut covered = Vec::new();
        for di in -3..=3 {
            for dj in -3..=3 {
                if inside(&t, pixel(i + di, j + dj)) {
                    covered.push((di, dj));
                }
            }
        }
        assert_eq!(covered, [(0, 0)], "at pixel {i} {j}");
    }
}
