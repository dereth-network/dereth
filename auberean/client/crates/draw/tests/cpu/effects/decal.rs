//! Decals on made-up ground: the clip to a plane, a decal's triangles and picture coordinates,
//! its life and fade, a group's room and the groups by material.

use auberean_draw::decal::{
    clip_polygon, construct_decal, landscape_decal, Corner, Decal, DecalDesc, DecalGroup,
    DecalManager, DECAL_LIFT, GROUP_VERTICES,
};
use auberean_primitives::Vec3;
use auberean_world::color::Color;

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

fn up() -> Vec3 {
    Vec3::new(0.0, 0.0, 1.0)
}

/// A flat square of ground, `side` meters from the origin along x and y, as two triangles.
fn flat(side: f32, z: f32) -> [[Corner; 3]; 2] {
    let c = |x: f32, y: f32| (Vec3::new(x, y, z), up());
    [
        [c(0.0, 0.0), c(side, 0.0), c(side, side)],
        [c(0.0, 0.0), c(side, side), c(0.0, side)],
    ]
}

fn area(t: &[auberean_draw::decal::DecalVertex; 3]) -> f32 {
    let (a, b, c) = (t[0].position, t[1].position, t[2].position);
    let (u, v) = (
        Vec3::new(b.x - a.x, b.y - a.y, b.z - a.z),
        Vec3::new(c.x - a.x, c.y - a.y, c.z - a.z),
    );
    let n = auberean_world::math::cross(u, v);
    0.5 * auberean_world::math::length(n)
}

/// A polygon cut by a plane keeps the side where the plane's measure is not negative: a point
/// where an edge crosses put between the kept and the dropped point, its normal taken along the
/// edge too; nothing is left when every point is dropped.
///
/// Behaviour: draw.decal.a-polygon-is-cut-to-a-planes-side
#[test]
fn a_polygon_is_cut_to_a_planes_side() {
    let tri = [
        (Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0)),
        (Vec3::new(2.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0)),
        (Vec3::new(0.0, 2.0, 0.0), Vec3::new(0.0, 0.0, 1.0)),
    ];
    // keep x <= 1: the corner at x = 2 is cut off, leaving a quadrilateral
    let cut = clip_polygon(Vec3::new(-1.0, 0.0, 0.0), 1.0, &tri);
    assert_eq!(cut.len(), 4);
    assert_eq!(cut[0], tri[0]);
    assert!(near(cut[1].0.x, 1.0) && near(cut[1].0.y, 0.0));
    assert!(
        near(cut[1].1.x, 0.5) && near(cut[1].1.z, 0.5),
        "the normal along the edge"
    );
    assert!(near(cut[2].0.x, 1.0) && near(cut[2].0.y, 1.0));
    assert_eq!(cut[3], tri[2]);
    assert_eq!(
        clip_polygon(Vec3::new(1.0, 0.0, 0.0), 5.0, &tri),
        tri.to_vec()
    );
    assert!(clip_polygon(Vec3::new(1.0, 0.0, 0.0), -5.0, &tri).is_empty());
}

/// A decal is the ground cut to its box, fanned into triangles, each corner's picture coordinates
/// its place across the box (its middle at one half) and lifted fifteen millimeters: a two meter
/// by one decal turned a quarter on flat ground covers two square meters, its corners' coordinates
/// within 0..1 and its lowest at the box's corner.
///
/// Behaviour: draw.decal.a-decal-is-the-ground-cut-to-its-box
#[test]
fn a_decal_is_the_ground_cut_to_its_box() {
    let ground = flat(10.0, 3.0);
    let center = Vec3::new(5.0, 5.0, 3.0);
    let north = Vec3::new(0.0, 1.0, 0.0);
    let tris = construct_decal(center, up(), north, Vec3::new(2.0, 1.0, 2.0), &ground);
    let total: f32 = tris.iter().map(area).sum();
    assert!(near(total, 2.0), "{total}");
    for t in &tris {
        for v in t {
            assert!(near(v.position.z, 3.0 + DECAL_LIFT));
            assert!((-1e-4..=1.0001).contains(&v.uv[0]) && (-1e-4..=1.0001).contains(&v.uv[1]));
            // u runs north, v west (up × north)
            assert!(near(v.uv[0], (v.position.y - 5.0) / 2.0 + 0.5));
            assert!(near(v.uv[1], (5.0 - v.position.x) + 0.5));
        }
    }
    // a decal whose box stands above the ground lays nothing
    let high = Vec3::new(5.0, 5.0, 10.0);
    assert!(construct_decal(high, up(), north, Vec3::new(2.0, 1.0, 2.0), &ground).is_empty());
}

/// A decal on the landscape takes the ground of the nine squares about it (its own and those a
/// square's side away along each axis and diagonal), so one reaching past its own square is laid
/// whole.
///
/// Behaviour: draw.decal.a-landscape-decal-takes-the-nine-squares-about-it
#[test]
fn a_landscape_decal_takes_the_nine_squares_about_it() {
    // squares of ten meters, the ground at height 1 everywhere
    let square_at = |p: Vec3| -> Option<[Corner; 6]> {
        let (i, j) = ((p.x / 10.0).floor(), (p.y / 10.0).floor());
        let c = |x: f32, y: f32| (Vec3::new(x, y, 1.0), up());
        let (x0, y0, x1, y1) = (i * 10.0, j * 10.0, i * 10.0 + 10.0, j * 10.0 + 10.0);
        Some([
            c(x0, y0),
            c(x1, y0),
            c(x1, y1),
            c(x0, y0),
            c(x1, y1),
            c(x0, y1),
        ])
    };
    let desc = DecalDesc {
        size: Vec3::new(6.0, 6.0, 2.0),
        ..DecalDesc::default()
    };
    // standing near a square's corner, the decal reaches into four squares
    let tris = landscape_decal(Vec3::new(21.0, 39.0, 1.0), &desc, 10.0, &square_at);
    let total: f32 = tris.iter().map(area).sum();
    assert!(near(total, 36.0), "{total}");
    let none = |_: Vec3| -> Option<[Corner; 6]> { None };
    assert!(landscape_decal(Vec3::new(21.0, 39.0, 1.0), &desc, 10.0, &none).is_empty());
}

/// A decal lives out its lifespan, fading in over its first tenth (a quarter second at most) and
/// out over its last fifth (a second at most); one drawn once shows whole at once and is gone once
/// drawn. Its vertices take its color, its alpha times how much shows.
///
/// Behaviour: draw.decal.a-decal-fades-in-and-out-over-its-life
#[test]
fn a_decal_fades_in_and_out_over_its_life() {
    let desc = DecalDesc {
        lifespan: 10.0,
        color: Color::new(1.0, 0.5, 0.0, 0.8),
        ..DecalDesc::default()
    };
    let tris = construct_decal(
        Vec3::new(5.0, 5.0, 0.0),
        up(),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(1.0, 1.0, 1.0),
        &flat(10.0, 0.0),
    );
    let mut e = Decal::new(&desc, 100.0, tris.clone());
    e.think(100.0);
    assert!(near(e.opacity, 0.0), "fading in");
    e.think(100.05);
    assert!(near(e.opacity, 1.0), "a fifth of the way in, held to one");
    e.think(109.5);
    assert!(near(e.opacity, 0.5), "half of the last second left");
    let mut out = Vec::new();
    e.geometry(&mut out);
    assert_eq!(out.len(), 3 * tris.len());
    assert_eq!(
        out[0].diffuse, 0x66FF_7F00,
        "the color, its alpha by the fade"
    );
    assert!(e.alive);
    e.think(110.0);
    assert!(!e.alive && e.triangles.is_empty(), "its lifespan spent");
    let once = DecalDesc {
        draw_once: true,
        ..desc
    };
    let mut e = Decal::new(&once, 100.0, tris);
    e.think(100.0);
    assert!(near(e.opacity, 1.0), "whole at once");
    let mut out = Vec::new();
    e.geometry(&mut out);
    assert!(!out.is_empty());
    assert!(!e.alive, "gone once drawn");
}

/// A group draws its decals together only while they fit its room (a vertex short of 8,192):
/// past it, it draws none of them and its decals drawn once stay. Decals are kept by material,
/// a group made for each, none for a material not found, none for a decal in no cell.
///
/// Behaviour: draw.decal.decals-are-drawn-by-material-within-a-groups-room
#[test]
fn decals_are_drawn_by_material_within_a_groups_room() {
    let one = construct_decal(
        Vec3::new(7.0, 3.0, 0.0),
        up(),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(1.0, 1.0, 1.0),
        &flat(10.0, 0.0),
    );
    assert_eq!(one.len(), 2);
    let once = DecalDesc {
        draw_once: true,
        material: 7,
        ..DecalDesc::default()
    };
    let fit = (GROUP_VERTICES - 1) / 3 / 2;
    let mut g = DecalGroup {
        material: 7,
        effects: (0..fit)
            .map(|_| Decal::new(&once, 0.0, one.clone()))
            .collect(),
    };
    assert_eq!(g.render().len(), fit * 6);
    g.think(0.0);
    assert!(g.effects.is_empty(), "drawn once and gone");
    let mut g = DecalGroup {
        material: 7,
        effects: (0..=fit)
            .map(|_| Decal::new(&once, 0.0, one.clone()))
            .collect(),
    };
    assert!(g.render().is_empty(), "past its room");
    g.think(0.0);
    assert_eq!(g.effects.len(), fit + 1, "still there");

    let mut m = DecalManager::default();
    assert!(m.create(0x1234_0001, &once, 0.0, one.clone(), true));
    assert!(m.create(
        0x1234_0001,
        &DecalDesc {
            material: 9,
            ..once
        },
        0.0,
        one.clone(),
        true
    ));
    assert!(
        m.create(0x1234_0001, &once, 0.0, one.clone(), false),
        "its group is there"
    );
    assert!(!m.create(
        0x1234_0001,
        &DecalDesc {
            material: 11,
            ..once
        },
        0.0,
        one.clone(),
        false
    ));
    assert!(!m.create(0, &once, 0.0, one.clone(), true), "in no cell");
    assert_eq!(m.groups.len(), 2);
    let drawn = m.render();
    assert_eq!(
        drawn.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
        vec![7, 9]
    );
    assert_eq!(drawn[0].1.len(), 12);
    m.think(0.5);
    assert!(m.render().is_empty(), "every one drawn once");
}

/// A point light standing above the land lays a pool of its light there: a box twice its range on
/// every side about it running north, its color times its intensity, how much of it shows and how
/// near it stands to the ground (one less its height over its range), held to 0..1, drawn once;
/// none when it stands below the ground or its range above it. The pool's material is the
/// night's, the twilight's or the day's by the time of day, and the ground's height under a point
/// is its square's triangle's plane there.
///
/// Behaviour: draw.decal.a-point-light-lays-a-pool-of-its-light-below-it
#[test]
fn a_point_light_lays_a_pool_of_its_light_below_it() {
    use auberean_draw::decal::{ground_height, point_light_pool, pool_material_value};
    let orange = Color::new(1.0, 0.5, 0.0, 1.0);
    let at = Vec3::new(3.0, 4.0, 14.0);
    let pool = point_light_pool(7, at, 10.0, 8.0, 0.8, orange, 1.0).expect("a pool");
    assert_eq!(pool.material, 7);
    assert_eq!(pool.size, Vec3::new(16.0, 16.0, 16.0));
    assert_eq!(pool.direction, Vec3::new(0.0, 1.0, 0.0));
    assert!(pool.draw_once);
    // four meters up of eight: half
    assert!(near(pool.color.r, 0.4) && near(pool.color.g, 0.2) && near(pool.color.b, 0.0));
    assert!(near(pool.color.a, 1.0));
    let bright = point_light_pool(7, at, 10.0, 8.0, 5.0, orange, 1.0).expect("a pool");
    assert!(near(bright.color.r, 1.0), "held to one");
    assert!(
        point_light_pool(7, at, 14.0, 8.0, 1.0, orange, 1.0).is_none(),
        "on the ground"
    );
    assert!(
        point_light_pool(7, at, 5.0, 8.0, 1.0, orange, 1.0).is_none(),
        "out of reach"
    );
    assert_eq!(pool_material_value(0.1), 0x4000_002C);
    assert_eq!(pool_material_value(0.97), 0x4000_002C);
    assert_eq!(pool_material_value(0.25), 0x4000_002B);
    assert_eq!(pool_material_value(0.9), 0x4000_002B);
    assert_eq!(pool_material_value(0.5), 0x4000_000D);
    // a square rising one meter eastward over ten
    let c = |x: f32, y: f32| (Vec3::new(x, y, x / 10.0), up());
    let square = [
        c(0.0, 0.0),
        c(10.0, 0.0),
        c(10.0, 10.0),
        c(0.0, 0.0),
        c(10.0, 10.0),
        c(0.0, 10.0),
    ];
    assert!(near(ground_height(&square, 5.0, 2.0).expect("in it"), 0.5));
    assert!(near(ground_height(&square, 2.0, 8.0).expect("in it"), 0.2));
    assert!(ground_height(&square, 12.0, 8.0).is_none());
}
