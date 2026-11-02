//! The dynamic objects' volumes of shadow on made-up meshes: how triangles are joined at their
//! edges, the volume a closed mesh casts, its room, the level it is made from, the way the lights
//! throw it and how dark it is.

use std::collections::HashMap;

use auberean_assets::mesh::{DetailLevel, Fragment, Mesh};
use auberean_draw::volume::{
    point_light_shadow, shadow_direction, shadow_volume, volume_level, Connectivity, ShadowLight,
    ShadowLightKind, Volumes, NEAR_OFFSET, VOLUME_LENGTH,
};
use auberean_primitives::Vec3;
use auberean_world::color::Color;

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

/// A unit box standing on the ground, as meshes store one: each face its own four vertices, two
/// triangles a face, wound counter-clockwise seen from outside.
fn unit_box() -> (Vec<Vec3>, Vec<u16>) {
    let faces: [[[f32; 3]; 4]; 6] = [
        [[0., 0., 1.], [1., 0., 1.], [1., 1., 1.], [0., 1., 1.]],
        [[0., 0., 0.], [0., 1., 0.], [1., 1., 0.], [1., 0., 0.]],
        [[0., 0., 0.], [1., 0., 0.], [1., 0., 1.], [0., 0., 1.]],
        [[1., 1., 0.], [0., 1., 0.], [0., 1., 1.], [1., 1., 1.]],
        [[1., 0., 0.], [1., 1., 0.], [1., 1., 1.], [1., 0., 1.]],
        [[0., 1., 0.], [0., 0., 0.], [0., 0., 1.], [0., 1., 1.]],
    ];
    let mut v = Vec::new();
    let mut i = Vec::new();
    for f in faces {
        let base = u16::try_from(v.len()).expect("few");
        v.extend(f.iter().map(|p| Vec3::new(p[0], p[1], p[2])));
        i.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    (v, i)
}

/// A place rounded to the millimeter.
type Point = (i64, i64, i64);

/// Whether the triangles `indices` over `vertices` close up: each edge, by where its ends stand,
/// run as often one way as the other.
fn closed(vertices: &[Vec3], indices: &[u16]) -> bool {
    let key = |p: Vec3| {
        #[allow(clippy::cast_possible_truncation)]
        let q = |x: f32| (x * 1000.0).round() as i64;
        (q(p.x), q(p.y), q(p.z))
    };
    let mut count: HashMap<(Point, Point), i32> = HashMap::new();
    for t in indices.as_chunks::<3>().0 {
        for k in 0..3 {
            let a = key(vertices[usize::from(t[k])]);
            let b = key(vertices[usize::from(t[(k + 1) % 3])]);
            if a == b {
                continue;
            }
            *count.entry((a, b)).or_default() += 1;
            *count.entry((b, a)).or_default() -= 1;
        }
    }
    count.values().all(|&c| c == 0)
}

/// A level's triangles meet at their edges: vertices standing within two ten-thousandths of a
/// meter of each other are one, and an edge a later triangle runs the other way joins the
/// triangle that made it (the later one marking it met); a box's six faces of four vertices each
/// are its eight corners and eighteen edges, every one met.
///
/// Behaviour: draw.volume.triangles-meet-at-edges-run-the-other-way
#[test]
fn triangles_meet_at_edges_run_the_other_way() {
    let (v, i) = unit_box();
    let c = Connectivity::of(&v, [i.as_slice()]).expect("a box");
    assert_eq!(c.local.len(), 8, "eight corners");
    assert_eq!(c.triangles.len(), 12);
    assert_eq!(c.edges.len(), 18);
    assert!(c.edges.iter().all(|e| e.second.is_some()));
    let met: usize = c
        .triangles
        .iter()
        .map(|t| t.shared.iter().filter(|&&m| m).count())
        .sum();
    assert_eq!(met, 18, "each edge met once");
    // two triangles of a square: one edge between them, four of the square's own
    let quad = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(1.0, 1.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.000_1, 0.0, 0.0),
    ];
    let c = Connectivity::of(&quad, [[0u16, 1, 2, 4, 2, 3].as_slice()]).expect("a square");
    assert_eq!(
        c.local,
        vec![0, 1, 2, 3],
        "a vertex a hair from another is that one"
    );
    assert_eq!(c.edges.len(), 5);
    let shared: Vec<_> = c.edges.iter().filter(|e| e.second.is_some()).collect();
    assert_eq!(shared.len(), 1);
    assert_eq!(
        (
            shared[0].from,
            shared[0].to,
            shared[0].first,
            shared[0].second
        ),
        (2, 0, 0, Some(1))
    );
    assert_eq!(c.triangles[1].shared, [true, false, false]);
    assert!(
        Connectivity::of(&quad, [[0u16, 1, 9].as_slice()]).is_none(),
        "no such vertex"
    );
}

/// A closed mesh casts a closed volume away from the light: its triangles facing the light pushed
/// a hair's breadth along it, again sixty meters along it turned to face out, and a side along
/// each edge they share with no triangle facing the light. A box lit from above and a little
/// aside: its top and two sides, six vertices for each of their six triangles, the two caps and
/// the six sides of the hexagon they make. A triangle the light meets edge on faces it too: lit
/// from straight above, the box's four sides with its top, the volume's sides along its bottom.
///
/// Behaviour: draw.volume.a-closed-mesh-casts-a-closed-volume-away-from-the-light
#[test]
fn a_closed_mesh_casts_a_closed_volume_away_from_the_light() {
    let (v, i) = unit_box();
    let c = Connectivity::of(&v, [i.as_slice()]).expect("a box");
    let slant = auberean_world::math::normalize(Vec3::new(-0.3, -0.2, -0.93));
    let vol = shadow_volume(&c, &v, slant, 0.0, NEAR_OFFSET, 40_000, 60_000).expect("a volume");
    assert_eq!(vol.vertices.len(), 36, "six for each of six triangles");
    assert_eq!(vol.indices.len(), 3 * (6 + 6 + 12));
    assert!(closed(&vol.vertices, &vol.indices), "the volume closes up");
    let down = Vec3::new(0.0, 0.0, -1.0);
    let vol = shadow_volume(&c, &v, down, 0.0, NEAR_OFFSET, 40_000, 60_000).expect("a volume");
    assert_eq!(vol.vertices.len(), 60, "the top and the sides met edge on");
    assert_eq!(vol.indices.len(), 3 * (10 + 10 + 8));
    assert!(closed(&vol.vertices, &vol.indices));
    let top: Vec<f32> = vol.vertices[..12].iter().map(|p| p.z).collect();
    assert!(top
        .iter()
        .all(|&z| near(z, 1.0 - NEAR_OFFSET) || near(z, 1.0 - VOLUME_LENGTH)));
    // a long given length is held to sixty meters, a short one kept
    let short = shadow_volume(&c, &v, down, 5.0, NEAR_OFFSET, 40_000, 60_000).expect("a volume");
    assert!(short.vertices.iter().any(|p| near(p.z, -4.0)));
    let long = shadow_volume(&c, &v, down, 500.0, NEAR_OFFSET, 40_000, 60_000).expect("a volume");
    assert!(long.vertices.iter().all(|p| p.z >= -VOLUME_LENGTH - 1e-3));
    // lit from the side, the box's lit face throws a closed volume too
    let side = Vec3::new(-0.6, -0.48, -0.64);
    let vol = shadow_volume(&c, &v, side, 0.0, NEAR_OFFSET, 40_000, 60_000).expect("a volume");
    assert!(closed(&vol.vertices, &vol.indices));
}

/// A volume is made only when it fits the frame's room, whole: none for a mesh of three triangles
/// or fewer, none when its vertices or its indices would pass the room left.
///
/// Behaviour: draw.volume.a-volume-is-made-only-whole-and-in-room
#[test]
fn a_volume_is_made_only_whole_and_in_room() {
    let (v, i) = unit_box();
    let c = Connectivity::of(&v, [i.as_slice()]).expect("a box");
    let slant = auberean_world::math::normalize(Vec3::new(-0.3, -0.2, -0.93));
    let down = Vec3::new(0.0, 0.0, -1.0);
    assert!(shadow_volume(&c, &v, slant, 0.0, NEAR_OFFSET, 36, 72).is_some());
    assert!(shadow_volume(&c, &v, slant, 0.0, NEAR_OFFSET, 35, 72).is_none());
    assert!(shadow_volume(&c, &v, slant, 0.0, NEAR_OFFSET, 36, 71).is_none());
    let tetra = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    ];
    let c = Connectivity::of(&tetra, [[0u16, 2, 1, 0, 1, 3, 1, 2, 3].as_slice()])
        .expect("three triangles");
    assert!(shadow_volume(&c, &tetra, down, 0.0, NEAR_OFFSET, 40_000, 60_000).is_none());
    let c = Connectivity::of(&tetra, [[0u16, 2, 1, 0, 1, 3, 1, 2, 3, 2, 0, 3].as_slice()])
        .expect("four triangles");
    assert!(shadow_volume(&c, &tetra, down, 0.0, NEAR_OFFSET, 40_000, 60_000).is_some());
    // lit from below, the bottom faces the light and its volume reaches up
    let (v, i) = unit_box();
    let c = Connectivity::of(&v, [i.as_slice()]).expect("a box");
    let up = Vec3::new(0.0, 0.0, 1.0);
    let vol = shadow_volume(&c, &v, up, 0.0, NEAR_OFFSET, 40_000, 60_000).expect("a volume");
    assert!(vol.vertices.iter().all(|p| p.z > 0.0));
}

fn mesh_of(details: &[u32]) -> Mesh {
    Mesh {
        levels: details
            .iter()
            .map(|&detail| DetailLevel {
                degrade: 0,
                detail,
                fragments: vec![Fragment::default()],
            })
            .collect(),
        ..Mesh::default()
    }
}

/// A mesh's volume is made from a coarse level: of the levels marked for the model detail (every
/// level when none is), the count less one, or less two at the highest model detail for an object
/// drawn at half detail or more, taken as a level of the mesh itself.
///
/// Behaviour: draw.volume.a-volume-is-made-from-a-coarse-level
#[test]
fn a_volume_is_made_from_a_coarse_level() {
    let m = mesh_of(&[7, 7, 7, 1]);
    assert_eq!(
        volume_level(&m, 1.0, 2),
        Some(1),
        "three marked at the highest detail"
    );
    assert_eq!(volume_level(&m, 0.4, 2), Some(2), "less than half detail");
    assert_eq!(
        volume_level(&m, 1.0, 0),
        Some(3),
        "four marked at the lowest"
    );
    assert_eq!(volume_level(&m, 1.0, 1), Some(2));
    let unmarked = mesh_of(&[0, 0]);
    assert_eq!(
        volume_level(&unmarked, 1.0, 2),
        Some(1),
        "none marked: every level, less one"
    );
    assert_eq!(volume_level(&mesh_of(&[4]), 1.0, 2), Some(0), "one level");
    assert_eq!(volume_level(&mesh_of(&[]), 1.0, 2), None);
}

/// The lights an object keeps throw its shadow, the first deciding how: a directional light first
/// throws it along its direction; a point light first sums the lights on the object's side, each
/// from itself to the object leaning a fifth of the way down, by its weight (a sky light the
/// sun's way), made a unit, straight down when they sum to nothing; with no light, the sun's way.
///
/// Behaviour: draw.volume.the-lights-an-object-keeps-throw-its-shadow
#[test]
fn the_lights_an_object_keeps_throw_its_shadow() {
    let at = Vec3::new(0.0, 0.0, 0.0);
    let up = Vec3::new(0.0, 0.0, 1.0);
    let sun = Vec3::new(0.6, 0.0, -0.8);
    let point = |x: f32, weight: f32, inside: bool| ShadowLight {
        kind: ShadowLightKind::Point,
        origin: Vec3::new(x, 0.0, 0.0),
        inside,
        direction: Vec3::ZERO,
        weight,
    };
    assert_eq!(shadow_direction(&[], at, true, up, sun), sun);
    let dir = ShadowLight {
        kind: ShadowLightKind::Directional,
        direction: Vec3::new(0.0, 1.0, 0.0),
        ..point(0.0, 1.0, true)
    };
    assert_eq!(
        shadow_direction(&[dir, point(5.0, 1.0, true)], at, true, up, sun),
        dir.direction
    );
    // one light to the east: the shadow falls west and a little down
    let d = point_light_shadow(&point(5.0, 1.0, true), at, up, sun);
    let s = 1.0 / 1.04f32.sqrt();
    assert!(
        near(d.x, -s) && near(d.z, -0.2 * s) && near(d.y, 0.0),
        "{d:?}"
    );
    // two lights on either side: the stronger wins; equal ones cancel but for the lean down
    let w = shadow_direction(
        &[point(5.0, 0.2, true), point(-5.0, 0.8, true)],
        at,
        true,
        up,
        sun,
    );
    assert!(w.x > 0.0 && w.z < 0.0, "{w:?}");
    let even = shadow_direction(
        &[point(5.0, 0.5, true), point(-5.0, 0.5, true)],
        at,
        true,
        up,
        sun,
    );
    assert!(near(even.z, -1.0), "{even:?}");
    // a light outside does not throw the shadow of an object inside, nor any weightless one
    let none = shadow_direction(&[point(5.0, 1.0, false)], at, true, up, sun);
    assert!(near(none.z, -1.0) && near(none.x, 0.0), "{none:?}");
    let sky = ShadowLight {
        kind: ShadowLightKind::Sky,
        ..point(5.0, 1.0, true)
    };
    let d = shadow_direction(&[sky], at, true, up, sun);
    assert!(near(d.x, sun.x) && near(d.z, sun.z));
}

/// A volume's shadow takes away the shadow color of the light outdoors, held to 0..1 with alpha
/// one, and a mid gray (0.45) inside.
///
/// Behaviour: draw.volume.a-shadow-takes-away-the-shadow-color-or-gray-inside
#[test]
fn a_shadow_takes_away_the_shadow_color_or_gray_inside() {
    let c = Volumes::strength_of(Color::new(0.3, 1.4, -0.2, 0.5), false);
    assert_eq!((c.r, c.g, c.b, c.a), (0.3, 1.0, 0.0, 1.0));
    let c = Volumes::strength_of(Color::new(0.3, 0.3, 0.3, 1.0), true);
    assert_eq!((c.r, c.g, c.b, c.a), (0.45, 0.45, 0.45, 1.0));
}
