//! How far the landscape reaches and where frills are drawn, at the camp of block `0xC156`.

use auberean_draw::scene::{fade_objects, far_stride, SceneBuilder, Spot};
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

/// The far landscape reaches as many blocks past the detailed ones as its radius (none, 2, 8: the
/// 3 x 3 detailed blocks alone, 7 x 7, 19 x 19 where the region has land); the detailed blocks are
/// drawn at every radius. Frills are drawn only in the land cells within their radius of the viewer's
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
        blocks.push((f.counts.blocks, land(r + 1)));
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

/// The far landscape thins out with distance: each block's squares lie 1, 2, 2, 4, 4, 8, 8 or 16
/// vertices apart by how many rings past the detailed blocks it stands, and a block next to a
/// coarser one fills the gap along their edge.
///
/// Behaviour: draw.world.the-far-landscape-thins-out-with-distance
#[test]
fn the_far_landscape_thins_out_with_distance() {
    assert_eq!(
        (1..=9).map(|r| far_stride(r, 16)).collect::<Vec<_>>(),
        [1, 2, 2, 4, 4, 8, 8, 16, 16]
    );
    let i = install();
    let region = i.records.region().expect("the region");
    let mut b = SceneBuilder::new(&i.records, &region);
    b.static_shadows = false;
    b.water_reflections = false;
    let (cx, cy) = (0xC1, 0x56);
    let f = b.frame(&spot(), 60.0);
    let far = f.terrain.iter().find(|t| t.far).expect("the far landscape");
    let mut vertices = 0;
    let mut squares = 0;
    for x in cx - 9..=cx + 9 {
        for y in cy - 9..=cy + 9 {
            let ring = i32::max(i32::abs(x - cx), i32::abs(y - cy)) - 1;
            if ring >= 1 && i.records.landblock(x, y).is_some() {
                let side = 16 / far_stride(ring, 16);
                vertices += (side + 1) * (side + 1);
                squares += side * side;
            }
        }
    }
    assert_eq!(far.vertices.len(), vertices);
    assert!(
        far.indices.len() > squares * 6,
        "the gaps' triangles besides the squares': {} of {}",
        far.indices.len(),
        squares * 6
    );
}

/// A far block's water is one polygon, its picture and the ripples on the one set of coordinates,
/// four repeats across the block for a tile width of 10 (a repeat every 40 m, as near the viewer).
///
/// Behaviour: draw.world.a-far-blocks-water-repeats-every-four-tile-widths
#[test]
fn a_far_blocks_water_repeats_every_four_tile_widths() {
    let i = install();
    let region = i.records.region().expect("the region");
    let mut b = SceneBuilder::new(&i.records, &region);
    b.static_shadows = false;
    let mut s = spot();
    s.position.z = 81.6;
    let f = b.frame(&s, 60.0);
    let far: Vec<_> = f.water.iter().filter(|w| w.vertices.len() == 4).collect();
    assert!(!far.is_empty(), "water among the far blocks");
    for w in far {
        assert_eq!(w.indices.len(), 6, "one polygon");
        for v in w.vertices.iter() {
            let [x, y, _] = v.position;
            let across = [x / 40.0, y / 40.0];
            assert_eq!(v.uv[0], across, "the picture every 40 m");
            assert_eq!(v.uv[1], across, "the ripples on the same coordinates");
        }
    }
}

/// A detailed block's water is laid four polygons by four, each a quarter of the block (40 m)
/// wide, with the water's picture and the ripples' coordinates both 0..1 across each polygon,
/// whatever the water's own tile and polygon widths say; the reflection about it ripples only when
/// the reflections are at their highest.
///
/// Behaviour: draw.world.a-blocks-water-lies-in-quarter-block-polygons
#[test]
fn a_detailed_blocks_water_lies_in_quarter_block_polygons() {
    let i = install();
    let region = i.records.region().expect("the region");
    let mut b = SceneBuilder::new(&i.records, &region);
    b.static_shadows = false;
    b.set_far_radius(0);
    let mut s = spot();
    s.position.z = 81.6;
    let f = b.frame(&s, 60.0);
    assert!(!f.water.is_empty(), "the camp's pond");
    let quarter = 160.0 * auberean_draw::scene::NEAR_WATER_SHARE;
    for w in &f.water {
        assert_eq!(w.vertices.len(), 25, "five by five corners");
        assert_eq!(w.indices.len(), 6 * 16, "sixteen polygons");
        for v in w.vertices.iter() {
            let [x, y, _] = v.position;
            let across = [x / quarter, y / quarter];
            assert_eq!(v.uv[0], across, "the picture once a polygon");
            assert_eq!(v.uv[1], across, "the ripples once a polygon");
        }
    }
    assert_eq!(f.reflection.map(|r| r.ripples), Some(true));
    b.water_ripples = false;
    let low = b.frame(&s, 60.0);
    assert_eq!(
        low.reflection.map(|r| r.ripples),
        Some(false),
        "low: no ripples"
    );
}

/// The camp's pond has its shore: strips where the detailed blocks' water meets the ground, at the
/// water's height, reaching at most 12 m out from the waterline, drawn first with the shore's
/// alpha material (black at half alpha, its picture's alpha written alone), then, with the shore
/// effects at their highest, with the foam's (black, blended by its own alpha). The strips are
/// found once for the viewer's block (each strip's offset across the foam rolled then) and kept
/// while the viewer stays in it; with the effects at their middle setting there is no foam, and
/// with them off no shore.
///
/// Behaviour: draw.shore.the-camps-pond-has-a-shore-where-its-water-meets-the-ground
#[test]
fn the_camps_pond_has_a_shore_where_its_water_meets_the_ground() {
    use auberean_draw::material::d3d;
    let i = install();
    let region = i.records.region().expect("the region");
    let mut b = SceneBuilder::new(&i.records, &region);
    b.static_shadows = false;
    b.water_reflections = false;
    b.set_far_radius(0);
    let mut s = spot();
    s.position.z = 81.6;
    let f = b.frame(&s, 60.0);
    let water = f.water.first().expect("the camp's pond").vertices[0].position[2];
    let (shore, foam) = (
        f.shore.first().expect("a shore"),
        f.foam.first().expect("foam"),
    );
    assert_eq!((f.shore.len(), f.foam.len()), (1, 1), "one layer each");
    assert_eq!(
        (shore.layer.source_blend, shore.layer.dest_blend),
        (d3d::BLEND_ONE, d3d::BLEND_ZERO)
    );
    assert_eq!(
        (foam.layer.source_blend, foam.layer.dest_blend),
        (d3d::BLEND_SRC_ALPHA, d3d::BLEND_INV_SRC_ALPHA)
    );
    assert_eq!(shore.tint.a, 0.5);
    assert_eq!(foam.tint.a, 1.0);
    for t in [shore.tint, foam.tint] {
        assert_eq!((t.r, t.g, t.b), (0.0, 0.0, 0.0), "black");
    }
    assert!(
        shore.vertices.len() > 20,
        "{} vertices",
        shore.vertices.len()
    );
    assert!(!shore.indices.is_empty() && foam.indices.len() <= shore.indices.len());
    for pair in shore.vertices.as_chunks::<2>().0 {
        let (at, out) = (pair[0].position, pair[1].position);
        assert!((at[2] - water).abs() < 1e-3 && (out[2] - water).abs() < 1e-3);
        let reach = dereth_primitives::num::math::hypotf(out[0] - at[0], out[1] - at[1]);
        assert!(reach <= 12.0 + 1e-3, "{reach}");
        assert_eq!(pair[1].uv[1][1] - pair[0].uv[1][1], 1.0, "across the strip");
    }
    // kept while the viewer stays in the block
    let again = b.frame(&s, 60.0);
    assert!(std::sync::Arc::ptr_eq(
        &again.shore[0].vertices,
        &shore.vertices
    ));
    b.shore_effects = 1;
    let middle = b.frame(&s, 60.0);
    assert_eq!((middle.shore.len(), middle.foam.len()), (1, 0), "no foam");
    b.shore_effects = 0;
    let off = b.frame(&s, 60.0);
    assert!(off.shore.is_empty() && off.foam.is_empty(), "no shore");
}

/// With the sun low, some of the land stands in the shadow of the land about it: an object placed
/// there is drawn dark, by the share of the four corners of the square under it that the sun
/// reaches (nothing under a tenth, whole over nine tenths); off the blocks kept it is drawn whole.
/// What is placed on a block takes it at where it stands; what is seen through never.
///
/// Behaviour: draw.world.what-stands-on-the-land-takes-the-lands-shadow
#[test]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn what_stands_on_the_land_takes_the_lands_shadow() {
    let r = &crate::common::install().records;
    let region = r.region().expect("the region");
    let mut builder = auberean_draw::scene::SceneBuilder::new(r, &region);
    // The first of the world's blocks, at a low sun, with land in the land's shadow.
    let coordinates = r.landblock_coordinates();
    let found = coordinates.iter().step_by(37).find_map(|&block| {
        let spot = auberean_draw::scene::Spot {
            block,
            position: auberean_primitives::Vec3::new(80.0, 80.0, 200.0),
            heading: 90.0,
            pitch: 5.0,
            time_of_day: 0.27,
            day: None,
        };
        let frame = builder.frame(&spot, 60.0);
        let sun = frame.light.light_direction;
        let mut seen = std::collections::BTreeSet::new();
        for i in 0..107 {
            for j in 0..107 {
                let at = auberean_primitives::Vec3::new(i as f32 * 1.5, j as f32 * 1.5, 0.0);
                let s = builder.land_shadow_at(at, block, sun);
                assert!([0.0, 0.25, 0.5, 0.75, 1.0].contains(&s), "{s}");
                seen.insert((s * 4.0) as u32);
            }
        }
        (seen.len() >= 2 && frame.objects.iter().any(|b| b.block.is_some()))
            .then_some((spot, frame, seen))
    });
    let (spot, frame, seen) = found.expect("a block in the land's shadow");
    println!("shares by quarters at {:x?}: {seen:?}", spot.block);
    let sun = frame.light.light_direction;
    let off = auberean_primitives::Vec3::new(160.0 * 20.0, 0.0, 0.0);
    assert_eq!(
        builder.land_shadow_at(off, spot.block, sun),
        1.0,
        "off the blocks"
    );
    let mut placed = 0;
    for b in &frame.objects {
        if b.block.is_none() {
            continue;
        }
        let at = b
            .anchor
            .unwrap_or_else(|| b.transform.point(auberean_primitives::Vec3::ZERO));
        let s = if b.seen_through {
            1.0
        } else {
            builder.land_shadow_at(at, spot.block, sun)
        };
        assert_eq!((b.tint.r, b.tint.g, b.tint.b), (s, s, s), "{at:?}");
        assert!(b.ground.is_none(), "the map is for what moves");
        placed += 1;
    }
    assert!(placed > 0, "{placed}");
}

/// What a frame draws of each placed object, by the object's mesh and place: the draws it has.
fn placed(f: &auberean_draw::scene::Frame) -> std::collections::HashMap<String, Vec<usize>> {
    let mut objects: std::collections::HashMap<String, Vec<usize>> = Default::default();
    for (k, b) in f.objects.iter().enumerate() {
        if b.lod.is_some() {
            objects
                .entry(format!("{:x} {:?}", b.mesh, b.transform))
                .or_default()
                .push(k);
        }
    }
    objects
}

/// How many indices of the draws `of` pass `which`, read from the frame `f`.
fn indices_of(
    f: &auberean_draw::scene::Frame,
    of: &[usize],
    which: impl Fn(&auberean_draw::scene::ObjectBatch) -> bool,
) -> usize {
    of.iter()
        .map(|&k| &f.objects[k])
        .filter(|b| which(b))
        .map(|b| b.indices.len())
        .sum()
}

/// With the frame built once, the camera moving on within its land cell still finds each placed
/// object's draw for the distance it has come to: some objects step to another level of detail, and
/// none in sight is left with nothing to draw.
///
/// Behaviour: draw.mesh.an-objects-detail-follows-the-camera-every-frame
#[test]
fn an_objects_detail_follows_the_camera_every_frame() {
    let i = install();
    let region = i.records.region().expect("the region");
    let mut b = SceneBuilder::new(&i.records, &region);
    b.static_shadows = false;
    b.camera_moves = true;
    b.set_far_radius(0);
    let s = spot();
    let mut f = b.frame(&s, 60.0);
    let objects = placed(&f);
    assert!(!objects.is_empty(), "the camp's objects");
    let (from, to) = (
        s.position,
        Vec3::new(s.position.x + 20.0, s.position.y + 20.0, s.position.z),
    );
    fade_objects(&mut f.objects, from, &[]);
    let before: Vec<(&String, usize)> = objects
        .iter()
        .map(|(o, of)| (o, indices_of(&f, of, |b| b.shown())))
        .collect();
    fade_objects(&mut f.objects, to, &[]);
    let mut stepped = 0;
    for (o, was) in before {
        let of = &objects[o];
        let first = &f.objects[of[0]];
        let distance = first.distance_from(to);
        let now = indices_of(&f, of, |b| b.shown());
        if first.visible.is_some_and(|v| distance < v * 0.99) {
            assert!(now > 0, "{o} at {distance} has a draw");
        }
        if now != was && now > 0 && was > 0 {
            stepped += 1;
        }
    }
    assert!(stepped > 0, "some object steps to another level");
}

/// The water's reflection takes an object's distances at 40%, its detail falling that much sooner:
/// an object it still shows may stand at a lower level there than in the frame.
///
/// Behaviour: draw.mesh.the-reflection-takes-an-objects-detail-distances-at-40-percent
#[test]
fn the_reflection_takes_an_objects_detail_distances_at_40_percent() {
    let i = install();
    let region = i.records.region().expect("the region");
    let mut b = SceneBuilder::new(&i.records, &region);
    b.static_shadows = false;
    b.camera_moves = true;
    b.set_far_radius(0);
    let s = spot();
    let mut f = b.frame(&s, 60.0);
    let objects = placed(&f);
    fade_objects(&mut f.objects, s.position, &[]);
    let mut lower = 0;
    for of in objects.values() {
        let shown = indices_of(&f, of, |b| b.shown());
        let reflected = indices_of(&f, of, |b| b.reflected(s.position));
        assert!(
            of.iter().all(|&k| {
                let b = &f.objects[k];
                let d = b.distance_from(s.position);
                !b.reflected(s.position) || b.visible.is_none_or(|v| d <= v * 0.4)
            }),
            "none past 40% of its visible distance"
        );
        if reflected > 0 && reflected < shown {
            lower += 1;
        }
    }
    assert!(
        lower > 0,
        "some object shows fewer triangles in the reflection"
    );
}
